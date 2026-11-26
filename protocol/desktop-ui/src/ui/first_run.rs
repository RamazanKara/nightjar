//! The first run, on the stage: a scan pulse in its middle while the network is searched, the one
//! line of what is happening and Add PC under it. A PC found rises in as a hardware card with the
//! page's one amber action while the pulse fades, and Add PC steps down.

mod card;
mod pulse;

use super::chrome::{capsule_width, control_capsule, play_button, play_width};
use super::design_tokens as t;
use super::pcs::{action_button, pc_action, pc_line_of};
use super::*;
use super::{glass, spring};
use egui::Shape;
use pulse::Pulse;

/// Past this many PCs found it is a list, whatever their state.
const MOST_FOUND: usize = 3;
/// The widest a line of the first run's words runs.
const TEXT_MEASURE: f32 = 560.0;
/// The pulse's radius in a window 800 points tall. It follows the window's height within bounds,
/// and gives way to the words under it in a short one.
const PULSE_R: f32 = 168.0;
const PULSE_LEAST_R: f32 = 48.0;
const PULSE_TO_WORDS: f32 = 32.0;
/// Of the room a block of the page leaves, the share above it: a little over the middle, where
/// the eye rests.
const ABOVE: f32 = 0.4;
const WORDS_TO_ADD: f32 = 24.0;
const ADD_H: f32 = 52.0;
/// Between Add PC and Search again: room for the focus ring of the one beside the other.
const ACTIONS_GAP: f32 = 16.0;
/// How far a found card rises in.
const RISE: f32 = 28.0;
const RISE_TUNE: spring::Tune = spring::Tune {
    stiffness: t::MOTION_SHEET_STIFFNESS,
    ratio: t::MOTION_SHEET_RATIO,
};
const CARDS_TO_LINE: f32 = 24.0;
const LINE_TO_ADD: f32 = 20.0;
const STEPPED_DOWN_H: f32 = 48.0;
/// How to connect under Add PC.
const LINK_GAP: f32 = 8.0;
const LINK_H: f32 = 32.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum FirstRun {
    Searching,
    Found,
}

/// Which first run this list of PCs is in, if any: none yet, or a few found with none of them paired.
/// A PC that has gone away is a list again, with its way to wake or check it.
pub(super) fn first_run_of(hosts: &Hosts) -> Option<FirstRun> {
    if hosts.list.is_empty() {
        return Some(FirstRun::Searching);
    }
    let found = hosts.list.len() <= MOST_FOUND
        && hosts
            .list
            .iter()
            .all(|h| !h.paired() && h.reach != Reach::Offline);
    found.then_some(FirstRun::Found)
}

/// From 1 to 0 as a card arrives. Reduced motion: it is simply there.
fn arrive(ctx: &egui::Context, id: Id) -> f32 {
    if !spring::motion(ctx) {
        return 0.0;
    }
    let seen = id.with("seen");
    if !ctx.data(|d| d.get_temp::<bool>(seen)).unwrap_or(false) {
        ctx.data_mut(|d| d.insert_temp(seen, true));
        spring::animate(ctx, id, 1.0, RISE_TUNE);
    }
    spring::animate(ctx, id, 0.0, RISE_TUNE)
}

/// Where a block `height` tall starts in this window: [`ABOVE`] of the room the chrome leaves over
/// it, and at that room's top once the window is too short (the page scrolls then).
fn block_top(screen: Rect, height: f32) -> f32 {
    let top = screen.top() + chrome::PAGE_TOP;
    let room = screen.bottom() - space::M - top;
    top + ((room - height) * ABOVE).max(0.0)
}

/// The pulse's centre and radius in this window, over `under` of words and actions.
fn pulse_place(screen: Rect, under: f32) -> (Pos2, f32) {
    let room = screen.bottom() - space::M - screen.top() - chrome::PAGE_TOP;
    let wanted = PULSE_R * (screen.height() / 800.0).clamp(0.75, 1.5);
    let radius = wanted.min((room - under - PULSE_TO_WORDS) / 2.0).max(PULSE_LEAST_R);
    let top = block_top(screen, 2.0 * radius + PULSE_TO_WORDS + under);
    (pos2(screen.center().x, top + radius), radius)
}

fn pulse_id() -> Id {
    Id::unique("first-run-pulse")
}

/// Where the pulse last stood, for the frames it fades on once a PC is found.
fn pulse_stood() -> Id {
    pulse_id().with("stood")
}

/// How much of the pulse shows on its way to `target`: it yields to a found PC on the spring the
/// card rises on, and is back the same way when the PC is gone again.
fn pulse_shown(ctx: &egui::Context, target: f32) -> f32 {
    spring::animate(ctx, pulse_id(), target, RISE_TUNE).clamp(0.0, 1.0)
}

impl Launcher {
    pub(super) fn first_run(
        &mut self,
        ui: &mut Ui,
        view: &View<'_>,
        state: FirstRun,
        commands: &mut Vec<Command>,
    ) {
        // A PC has just been found: its Pair takes the focus from Add PC.
        let found = state == FirstRun::Found;
        if found && !self.found_shown {
            self.focus_screen = true;
        }
        self.found_shown = found;
        let foot = match state {
            FirstRun::Searching => self.first_run_searching(ui, view, commands),
            FirstRun::Found => self.first_run_found(ui, view, commands),
        };
        let page = ui.max_rect();
        ui.advance_cursor_after_rect(Rect::from_min_max(
            ui.cursor().min,
            pos2(page.right(), foot + space::M),
        ));
    }

    /// What is happening, in as few words as say so, and whether the search runs. A search that
    /// could not start is said in the page's own ink: Add PC is the way on, and Search again the fix.
    fn search_words(&self, view: &View<'_>) -> (&str, bool) {
        if let Some(reason) = &self.identity_error {
            // A key that cannot be read never arrives: say why instead of waiting for it.
            (reason, false)
        } else if !self.identity_ready {
            (tr("Creating this device’s pairing key…"), true)
        } else if view.hosts.discovering {
            (tr("Searching for PCs…"), true)
        } else {
            (tr("Can’t search this network for PCs"), false)
        }
    }

    /// How to connect, centred at `top`, as Android's search card has it. Returns where it ends.
    fn first_run_link(&mut self, ui: &mut Ui, top: f32, commands: &mut Vec<Command>) -> f32 {
        let page = ui.max_rect();
        let room = Rect::from_min_max(pos2(page.left(), top), pos2(page.right(), top + LINK_H));
        let help = ui
            .scope_builder(
                UiBuilder::new()
                    .max_rect(room)
                    .layout(Layout::top_down(Align::Center)),
                |ui| how_to_connect(ui, commands),
            )
            .inner;
        self.focus_item(&help, "how-to-connect", false);
        room.bottom()
    }

    /// The quiet Add PC at `rect`: the way on by hand, whatever the search finds.
    fn first_run_add(&mut self, ui: &mut Ui, rect: Rect, id: &str, first: bool) {
        let label = tr("Add PC");
        let add = control_capsule(ui, rect, Id::unique(id), Icon::Plus, label, label, true);
        self.focus_item(&add, "first-run-add", first);
        if add.clicked() {
            self.add_host_dialog();
        }
    }

    fn first_run_searching(
        &mut self,
        ui: &mut Ui,
        view: &View<'_>,
        commands: &mut Vec<Command>,
    ) -> f32 {
        let ctx = ui.ctx().clone();
        let page = ui.max_rect();
        let middle = page.center().x;
        let dy = -self.page_scrolled;
        let (words, searching) = self.search_words(view);
        let ink = *ON_SURFACE;
        let mut job = egui::text::LayoutJob::simple(
            words.to_owned(),
            semibold(t::TYPE_TITLE_SIZE),
            ink,
            page.width().min(TEXT_MEASURE),
        );
        job.halign = Align::Center;
        let head = ui.painter().layout_job(job);
        let head_h = head.size().y;
        // The search could not start (no network yet, or its port held): it can be tried again.
        let again = !searching && self.identity_ready;
        let add_w = capsule_width(ui, tr("Add PC"));
        let again_w = capsule_width(ui, tr("Search again"));
        let beside = add_w + ACTIONS_GAP + again_w <= page.width();
        let actions_h = if again && !beside { 2.0 * ADD_H + ACTIONS_GAP } else { ADD_H };
        let under = head_h + WORDS_TO_ADD + actions_h + LINK_GAP + LINK_H;
        let (center, radius) = pulse_place(ctx.content_rect(), under);
        ctx.data_mut(|d| d.insert_temp(pulse_stood(), (center, radius)));
        let mut rings = Vec::new();
        let pulse = if searching { Pulse::Scanning } else { Pulse::Still };
        pulse::paint(&ctx, &mut rings, pos2(middle, center.y + dy), radius, pulse_shown(&ctx, 1.0), pulse);
        ui.painter().extend(rings);
        let mut y = center.y + radius + PULSE_TO_WORDS + dy;
        // A centred galley is placed by its middle line.
        ui.painter().galley(pos2(middle, y), head, ink);
        y += head_h + WORDS_TO_ADD;
        let row_w = if again && beside { add_w + ACTIONS_GAP + again_w } else { add_w };
        let add = Rect::from_min_size(pos2(middle - row_w / 2.0, y), vec2(add_w, ADD_H));
        self.first_run_add(ui, add, "first-run-add", true);
        if again {
            let at = if beside { pos2(add.right() + ACTIONS_GAP, y) } else { pos2(middle - again_w / 2.0, add.bottom() + ACTIONS_GAP) };
            let rect = Rect::from_min_size(at, vec2(again_w, ADD_H));
            let label = tr("Search again");
            let r = control_capsule(ui, rect, Id::unique("first-run-search-again"), Icon::Refresh, label, label, true);
            self.focus_item(&r, "first-run-search-again", false);
            if r.clicked() {
                commands.push(Command::SearchAgain);
            }
        }
        self.first_run_link(ui, y + actions_h + LINK_GAP, commands)
    }

    fn first_run_found(
        &mut self,
        ui: &mut Ui,
        view: &View<'_>,
        commands: &mut Vec<Command>,
    ) -> f32 {
        let ctx = ui.ctx().clone();
        let page = ui.max_rect();
        let middle = page.center().x;
        let dy = -self.page_scrolled;
        // The pulse yields where it stood while the first card lands on it.
        let shown = pulse_shown(&ctx, 0.0);
        if shown > 0.0
            && let Some((center, radius)) = ctx.data(|d| d.get_temp::<(Pos2, f32)>(pulse_stood()))
        {
            let mut rings = Vec::new();
            pulse::paint(&ctx, &mut rings, pos2(middle, center.y + dy), radius, shown, Pulse::Leaving);
            ui.painter().extend(rings);
        }
        let list = &view.hosts.list;
        let card_w = page.width().min(card::WIDTH);
        // One width for every card's action, so they stand in a column.
        let action_w = list
            .iter()
            .map(|host| play_width(ui, pc_action(host).words()))
            .fold(0.0, f32::max);
        let card_h = card::height(card_w, action_w);
        let cards_h = list.len() as f32 * (card_h + card::GAP) - card::GAP;
        // The search goes on, under what it found.
        let (words, _) = self.search_words(view);
        let ink = *ON_SURFACE_VARIANT;
        let font = regular(t::TYPE_SUB_SIZE);
        let one_line = ui.painter().layout_no_wrap(words.to_owned(), font.clone(), ink).size().y;
        let mut job = egui::text::LayoutJob::simple(words.to_owned(), font, ink, page.width().min(TEXT_MEASURE) - 24.0);
        job.halign = Align::Center;
        let line = ui.painter().layout_job(job);
        let line_size = line.size();
        let under = CARDS_TO_LINE + line_size.y + LINE_TO_ADD + STEPPED_DOWN_H + LINK_GAP + LINK_H;
        let mut y = block_top(ctx.content_rect(), cards_h + under) + dy;
        for (n, host) in list.iter().enumerate() {
            let slot = Rect::from_min_size(pos2(middle - card_w / 2.0, y), vec2(card_w, card_h));
            self.found_card(ui, host, slot, action_w, n == 0, commands);
            y += card_h + card::GAP;
        }
        y += CARDS_TO_LINE - card::GAP;
        // Its dot before the words, the two centred as one.
        let dot = pos2(middle + 6.0 - line_size.x / 2.0 - 12.0, y + one_line / 2.0);
        ui.painter().circle_filled(dot, 4.0, shade::COLOR_OFFLINE.get());
        ui.painter().galley(pos2(middle + 6.0, y), line, ink);
        y += line_size.y + LINE_TO_ADD;
        let add_w = capsule_width(ui, tr("Add PC"));
        let add = Rect::from_min_size(pos2(middle - add_w / 2.0, y), vec2(add_w, STEPPED_DOWN_H));
        self.first_run_add(ui, add, "first-run-add-small", false);
        self.first_run_link(ui, add.bottom() + LINK_GAP, commands)
    }

    /// A PC found: its hardware card, and its one action. The first card's is the page's amber
    /// one; the next cards' stand on a tint, so amber stays the one thing to press.
    fn found_card(
        &mut self,
        ui: &mut Ui,
        host: &Host,
        slot: Rect,
        action_w: f32,
        first: bool,
        commands: &mut Vec<Command>,
    ) {
        let ctx = ui.ctx().clone();
        let id = Id::unique(("found-card", host.id));
        let card = ui.interact(slot, id, Sense::CLICK);
        let name = host.display_name();
        let line = pc_line_of(host);
        let said = format!("{name}, {}", line.words());
        card.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, said.clone()));
        let rising = arrive(&ctx, id.with("arrive"));
        let rect = slot.translate(vec2(0.0, RISE * rising));
        let parts = card::parts(rect, action_w);
        let action = pc_action(host);
        let words = action.words();
        let mut pair = None;
        let mut cut = false;
        ui.scope(|ui| {
            ui.multiply_opacity((1.0 - rising).clamp(0.0, 1.0));
            let mut out = Vec::new();
            let lift = (card.hovered(), card.is_pointer_button_down_on());
            cut = card::paint(ui, &mut out, rect, &parts, host, &line, lift);
            ui.painter().extend(out);
            let id = id.with("pair");
            pair = Some(if first {
                play_button(ui, parts.action, id, action.icon().unwrap_or(Icon::Lock), words, true, false)
            } else {
                action_button(ui, parts.action, id, action, &format!("{words}, {name}"))
            });
        });
        let pair = pair.expect("the scope ran");
        self.focus_item(&pair, &format!("found:{}", host.id), first);
        if pair.clicked() {
            commands.push(action.command(host.id));
        } else if card.clicked() {
            host_clicked(host, self.settings.quick_resume, commands);
        }
        // Pair holds the focus for its card: its keys, its word, and the options by the card.
        let keys = menu_request_going(ui, &pair, None, words).map(|_| Anchor::Beside(card.rect));
        let asked = menu_request(ui, &card, None).or(keys);
        if let Some(anchor) = asked {
            self.open_menu(
                Dialog::HostOptions { host: host.id },
                anchor,
                format!("found:{}", host.id),
            );
        }
        if cut {
            // A name cut to fit is whole under the pointer.
            let _ = card.on_hover_text(name);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(w: f32, h: f32) -> Rect {
        Rect::from_min_size(Pos2::ZERO, vec2(w, h))
    }

    #[test]
    fn the_pulse_stands_in_the_middle_over_its_words_in_every_window() {
        let under = 150.0;
        for (w, h) in [(1280.0, 800.0), (720.0, 480.0), (2560.0, 1080.0), (853.0, 320.0)] {
            let screen = window(w, h);
            let (center, radius) = pulse_place(screen, under);
            assert_eq!(center.x, w / 2.0, "{w}x{h}");
            assert!((PULSE_LEAST_R..=PULSE_R * 1.5).contains(&radius), "{w}x{h}: {radius}");
            assert!(center.y - radius >= chrome::PAGE_TOP - 0.5, "{w}x{h}: under the chrome, {center:?} {radius}");
            let foot = center.y + radius + PULSE_TO_WORDS + under;
            // A window with the room for it shows it whole; a shorter one scrolls.
            if h >= 480.0 {
                assert!(foot <= h - space::M + 0.5, "{w}x{h}: its words end at {foot}");
                assert!(center.y < h / 2.0, "{w}x{h}: above the middle, {center:?}");
            }
        }
        let (_, tall) = pulse_place(window(1280.0, 800.0), under);
        let (_, short) = pulse_place(window(720.0, 480.0), under);
        assert_eq!(tall, PULSE_R);
        assert!(short < tall, "a short window has a smaller pulse: {short}");
    }

    #[test]
    fn a_block_too_tall_for_the_window_starts_under_the_chrome() {
        let screen = window(1280.0, 480.0);
        assert_eq!(block_top(screen, 900.0), chrome::PAGE_TOP);
        let top = block_top(screen, 200.0);
        assert!(top > chrome::PAGE_TOP && top + 200.0 < 480.0 - space::M, "{top}");
    }
}
