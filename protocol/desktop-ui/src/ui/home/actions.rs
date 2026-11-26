//! Home's first-pair setup card: what a stream from the PC just paired starts with, until dismissed.

use super::*;

/// The setup card's width at most; its left edge is the page's.
const SETUP_CARD_W: f32 = 420.0;
const SETUP_CARD_CLOSE: f32 = 36.0;

/// The line under the setup card's title: what a stream from this PC asks for with `settings`
/// (the PC's resolved settings), as the game page says it, then HDR or SDR as the start requests it,
/// then the codec: Auto is negotiated when the stream starts, a forced one is named.
pub(in crate::ui) fn setup_line(settings: &Settings, display: Display) -> String {
    let (w, h, fps, kbps) = stream_mode(settings, display);
    let mut line = console::stream_summary(w, h, fps, kbps);
    line.push_str(if settings.hdr { " · HDR" } else { " · SDR" });
    line.push_str(" · ");
    line.push_str(match settings.codec {
        CodecPreference::Auto => tr("codec chosen when the stream starts"),
        codec => settings::codec_name(codec),
    });
    line
}

impl Launcher {
    /// A pairing succeeded and `paired_pcs` PCs are paired now: the first one on this install
    /// (exactly one paired, no pairing seen before) gets Home's setup card. Every pairing counts
    /// as seen, so one made beside another PC leaves none for later either. True when the
    /// settings changed and want saving.
    pub fn offer_setup_card(&mut self, host: u64, paired_pcs: usize) -> bool {
        if self.settings.setup_card_seen {
            return false;
        }
        self.settings.setup_card_seen = true;
        if paired_pcs == 1 {
            self.settings.setup_card_host = Some(host);
        }
        true
    }

    /// The app starts with a PC paired. With no pairing seen, the install comes from a version
    /// before the card and made its first pairing long ago: pairing that PC again offers nothing.
    /// True when the settings changed and want saving.
    pub fn paired_before_setup_card(&mut self) -> bool {
        !std::mem::replace(&mut self.settings.setup_card_seen, true)
    }

    /// The setup card's title and line while it shows: its PC is still paired, no device waits to
    /// pair (that banner goes first), and the settings are what "Auto (recommended)" writes in all
    /// three Recommended choices. With Custom settings it says nothing, and stays for later. The
    /// rule is Android's: Recommended's own settings decide, the ones its page shows, and the line
    /// then gives what a stream to this PC starts with, a value of the PC's own included.
    pub(in crate::ui) fn setup_card_words(&self, view: &View<'_>) -> Option<(String, String)> {
        let host = view.hosts.get(self.settings.setup_card_host?)?;
        if !host.paired() || view.hosts.list.iter().any(|h| !h.waiting_to_pair().is_empty()) {
            return None;
        }
        let kind = self.network.kind(&self.profiles.home_wifi);
        let settings = host.record.settings(&self.profiles.settings(&self.settings, kind));
        let vrr_offered = self.tearing_supported || cfg!(not(windows));
        let caps = settings::simple_caps(&self.display, self.hdr_display, vrr_offered);
        let (choices, custom) = settings::simple_state(&self.settings, &self.display, &caps, vrr_offered);
        if custom || choices != nightjar_policy::simple_profile::recommended(&caps) {
            return None;
        }
        Some((trf("Set up for this device and {}", &[&host.display_name()]), setup_line(&settings, self.display)))
    }

    /// Home's card after the first pairing: the page's first content, where the pairing banner
    /// stands, so the hero and the shelves start beneath it and it scrolls away with them. Its
    /// close button is the only control, reached by Tab and the arrow keys as Home's others are.
    /// True when it was dismissed.
    pub(in crate::ui) fn setup_card(&mut self, ui: &mut Ui, view: &View<'_>) -> bool {
        if self.settings.setup_card_host.is_none() {
            return false;
        }
        // HDR Auto depends on the screen: read it as the Recommended section does, once.
        if !std::mem::replace(&mut self.setup_card_probed, true)
            && let Some(probe) = self.hdr_display_probe {
                self.hdr_display = probe();
            }
        let Some((title, line)) = self.setup_card_words(view) else {
            return false;
        };
        let width = SETUP_CARD_W.min(ui.available_width()).max(200.0);
        let place = ui.painter().add(egui::Shape::Noop);
        // The close button ends the card, as on Android: above the hero's buttons, where Up from them lands.
        let margin = Margin { left: 18, right: (SETUP_CARD_CLOSE + 18.0) as i8, top: 14, bottom: 14 };
        let shown = Frame::new().inner_margin(margin).show(ui, |ui| {
            ui.set_width(width - f32::from(margin.left) - f32::from(margin.right));
            ui.spacing_mut().item_spacing.y = space::XS;
            ui.label(RichText::new(&title).font(semibold(text::BODY)).color(ON_SURFACE));
            ui.label(RichText::new(&line).font(regular(text::LABEL)).color(ON_SURFACE_VARIANT));
        });
        let rect = shown.response.rect;
        let mut shapes = Vec::new();
        glass::without_backdrop(&mut shapes, ui.ctx(), rect, t::RADIUS_GROUP, glass::Material::Menu);
        ui.painter().set(place, egui::Shape::Vec(shapes));
        let close = Rect::from_min_size(pos2(rect.right() - 8.0 - SETUP_CARD_CLOSE, rect.top() + 8.0), Vec2::splat(SETUP_CARD_CLOSE));
        let close = round_button(ui, close, Icon::Close, tr("Dismiss"));
        self.focus_item(&close, "setup-card-dismiss", false);
        // The button is small and stands between the top bar and the hero: egui's arrow keys pick
        // what lies straight above or below and would pass it. A move by Up or Down from one side
        // of the card to the other stops on it instead.
        let focused = ui.ctx().memory(|m| m.focused()).and_then(|id| ui.ctx().read_response(id)).map(|r| r.rect);
        if let (Some(from), Some(now)) = (self.setup_card_from.take(), focused) {
            let crossed = (from.top() >= rect.bottom() && now.bottom() <= rect.top())
                || (from.bottom() <= rect.top() && now.top() >= rect.bottom());
            if crossed && matches!(self.dialog, Dialog::None) && !egui::Popup::is_any_open(ui.ctx()) {
                close.request_focus();
            }
        }
        if ui.input(|i| i.key_pressed(egui::Key::ArrowUp) || i.key_pressed(egui::Key::ArrowDown)) {
            self.setup_card_from = focused;
        }
        ui.add_space(space::M);
        if close.clicked() {
            self.settings.setup_card_host = None;
        }
        close.clicked()
    }
}
