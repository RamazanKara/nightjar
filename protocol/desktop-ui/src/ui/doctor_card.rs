//! The Stream Doctor card in the Control Center (Stream Doctor 2.0, item 11): the stage that cost
//! the stream its smoothness as a title, the number behind it in one line, two plain sentences,
//! one button per remedy for this stream with Undo while one is in force, advice, one line of
//! upscaler guidance, and the technical measurements in a drawer that starts closed, with Copy
//! report under it. It draws what `nightjar_policy::doctor_v2::diagnose` decided and what the
//! stream can undo; it decides nothing and asks the PC for nothing (the app runs the
//! [`Action`] it answers). Drawn only inside the open doctor fold, on the dock's own surface:
//! no blur, and every button takes the keyboard and controller focus like the rest of the dock.

pub mod copy;

use egui::{Id, Response, RichText, Sense, Ui, WidgetInfo, vec2};
use nightjar_policy::doctor_v2::{ActiveUpscaler, Diagnosis, Remedy, RootCause};
use rubylight_native_api::doctor::DoctorSnapshot;

use super::control_center_cards as cards;
use crate::menu_style;
use crate::strings::tr;
use crate::theme::{ON_SURFACE, regular, semibold, space, text};

/// What the card shows.
pub struct View<'a> {
    pub diagnosis: &'a Diagnosis,
    /// The host's snapshot while it offers the doctor: its numbers go in the drawer.
    pub host: Option<&'a DoctorSnapshot>,
    /// This device owns the stream; always so without the host's doctor. A guest's card hides
    /// the remedies, which change the PC.
    pub owner: bool,
    /// The upscaler the settings run, named by the guidance line.
    pub upscaler: ActiveUpscaler,
    /// A remedy or its undo is on its way: the buttons wait for it.
    pub busy: bool,
    /// A bitrate step-down of the doctor's is in force and may be undone.
    pub undo_bitrate: bool,
    /// A resolution step-down of the doctor's is in force and may be undone.
    pub undo_resolution: bool,
    /// The bitrate step-down can no longer be undone: something else changed the bitrate since.
    pub replaced: bool,
    /// This device's own report (`stream::doctor::report_text`), for the drawer.
    pub technical: &'a str,
}

/// What the player asked for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    Apply(Remedy),
    UndoBitrate,
    UndoResolution,
    /// Copy [`report`] to the clipboard.
    Copy,
}

/// Where egui keeps whether the technical drawer is open: closed at first, as the player left it
/// after that.
pub fn technical_id() -> Id {
    Id::unique("stream-doctor-technical")
}

/// The remedies the card offers: none while it gathers, none to a guest.
pub fn offered<'a>(view: &View<'a>) -> &'a [Remedy] {
    if view.owner && view.diagnosis.cause != RootCause::Gathering {
        &view.diagnosis.remedies
    } else {
        &[]
    }
}

/// The quiet lines under the buttons: why they wait or are hidden.
fn notes(view: &View<'_>) -> Vec<&'static str> {
    let mut notes = Vec::new();
    if view.busy {
        notes.push(tr("Waiting for the PC…"));
    }
    if !view.owner
        && view.diagnosis.cause != RootCause::Gathering
        && !view.diagnosis.remedies.is_empty()
    {
        notes.push(tr("Host device only"));
    }
    if view.replaced {
        notes.push(tr(
            "The PC’s bitrate has changed since the trial. Undo was cancelled to preserve that change.",
        ));
    }
    notes
}

/// The advice and, last, the upscaler guidance.
fn advice_lines(view: &View<'_>) -> Vec<String> {
    let d = view.diagnosis;
    let mut lines: Vec<String> = d.advice.iter().map(copy::advice).collect();
    lines.extend(copy::upscaler(&d.upscaler, view.upscaler));
    lines
}

/// The card as plain text, for Copy report: its words, then the measurements.
pub fn report(view: &View<'_>) -> String {
    let d = view.diagnosis;
    let mut text = copy::title(d).to_owned();
    if let Some(key) = copy::key_line(&d.key) {
        text.push('\n');
        text.push_str(&key);
    }
    text.push('\n');
    text.push_str(copy::explanation(d));
    for line in advice_lines(view) {
        text.push_str("\n• ");
        text.push_str(&line);
    }
    text.push_str("\n\n");
    text.push_str(view.technical);
    if let Some(host) = view.host.and_then(copy::host_lines) {
        text.push_str("\n\n");
        text.push_str(&host);
    }
    text
}

/// A filled button like `menu_style::button` whose label wraps instead of ending in an ellipsis:
/// a remedy names its target, and the dock is narrow.
fn button(ui: &mut Ui, label: &str) -> Response {
    let enabled = ui.is_enabled();
    let ink = if enabled {
        *ON_SURFACE
    } else {
        ON_SURFACE.gamma_multiply(0.4)
    };
    let width = ui.available_width();
    let galley = ui.painter().layout(
        label.to_owned(),
        semibold(text::LABEL),
        ink,
        (width - 2.0 * space::L).max(space::L),
    );
    let size = vec2(
        (galley.size().x + 2.0 * space::L).min(width),
        (galley.size().y + 2.0 * space::S).max(36.0),
    );
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let r = 18.0_f32.min(rect.height() / 2.0);
    let mut shapes = Vec::new();
    super::glass::control(&mut shapes, ui.ctx(), rect, r);
    super::lift_shapes(
        &mut shapes,
        ui.ctx(),
        rect,
        r,
        enabled && response.hovered(),
        enabled && response.is_pointer_button_down_on(),
    );
    ui.painter().extend(shapes);
    ui.painter()
        .galley(rect.center() - galley.size() / 2.0, galley, ink);
    menu_style::ring(ui, rect, &response, r as u8);
    response.widget_info(|| WidgetInfo::labeled(egui::Role::Button, enabled, label));
    response
}

/// Lines in the card's quieter ink.
fn quiet_line(ui: &mut Ui, line: &str) {
    ui.label(
        RichText::new(line)
            .font(regular(text::LABEL))
            .color(cards::inks().1),
    );
}

/// One frame of the card; answers what the player asked for.
pub fn show(ui: &mut Ui, view: &View<'_>) -> Option<Action> {
    let d = view.diagnosis;
    let ink = cards::inks().0;
    let mut action = None;
    ui.label(
        RichText::new(copy::title(d))
            .font(semibold(text::BODY))
            .color(ink),
    );
    if let Some(key) = copy::key_line(&d.key) {
        ui.label(
            RichText::new(key)
                .font(semibold(text::LABEL))
                .color(cards::amber()),
        );
    }
    ui.add_space(space::XS);
    ui.label(
        RichText::new(copy::explanation(d))
            .font(regular(text::LABEL))
            .color(ink),
    );
    let remedies = offered(view);
    if !remedies.is_empty() || view.undo_bitrate || view.undo_resolution {
        ui.add_space(space::S);
        ui.add_enabled_ui(!view.busy, |ui| {
            for remedy in remedies {
                if button(ui, &copy::remedy(remedy)).clicked() {
                    action = Some(Action::Apply(*remedy));
                }
                ui.add_space(space::XS);
            }
            if view.undo_bitrate && button(ui, tr("Undo bitrate trial")).clicked() {
                action = Some(Action::UndoBitrate);
            }
            if view.undo_resolution && button(ui, tr("Undo resolution change")).clicked() {
                action = Some(Action::UndoResolution);
            }
        });
    }
    for note in notes(view) {
        ui.label(
            RichText::new(note)
                .font(regular(text::CAPTION))
                .color(cards::inks().1),
        );
    }
    let lines = advice_lines(view);
    if !lines.is_empty() {
        ui.add_space(space::S);
        for line in &lines {
            quiet_line(ui, &format!("• {line}"));
        }
    }
    ui.add_space(space::S);
    let mut drawer = egui::collapsing_header::CollapsingState::load_with_default_open(
        ui.ctx(),
        technical_id(),
        false,
    );
    let label = if drawer.is_open() {
        tr("Hide technical measurements")
    } else {
        tr("Show technical measurements")
    };
    if button(ui, label).clicked() {
        drawer.toggle(ui);
    }
    drawer.show_body_unindented(ui, |ui| {
        ui.add_space(space::XS);
        ui.label(
            RichText::new(view.technical)
                .font(regular(text::CAPTION))
                .color(cards::inks().1),
        );
        if let Some(host) = view.host.and_then(copy::host_lines) {
            ui.add_space(space::XS);
            ui.label(
                RichText::new(host)
                    .font(regular(text::CAPTION))
                    .color(cards::inks().1),
            );
        }
    });
    ui.add_space(space::XS);
    if button(ui, tr("Copy report")).clicked() {
        action = Some(Action::Copy);
    }
    action
}

#[cfg(test)]
mod tests;
