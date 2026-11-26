//! The Control Center: the dock a stream opens over its video in place of the 6.x stream menu
//! (client/design/README.md, "Control Center"). This file is its frame: where it sits
//! (`nightjar_policy::control_center::placement`), what it is painted on (`surface`), the rail of
//! modules, the page and the footer pinned under it, and what it keeps between openings ([`State`]:
//! the module shown and the focus). `control_center_cards.rs` draws a page's cards, `stream_menu.rs`
//! fills the pages and the app hosts it.
//!
//! Night-locked: every colour is a `SURFACE_CONTROL_CENTER_*` token, the same in the night, day and
//! OLED themes; high contrast takes solid black and a 2 px rim. Closed it draws nothing: `show` is
//! only called while the stream's menu is up, and [`video_glass`] says what the presenter may hold.

use egui::{pos2, vec2, Align2, Color32, Context, CornerRadius, Id, Key, Modifiers, Order, Rect, Sense, Ui, UiBuilder};
use nightjar_policy::control_center::{self as model, Dock, Module, Placement, Surface};
use nightjar_policy::ui_feedback_contract::Event;
use nightjar_policy::viewport_scale::FormFactor;

use super::control_center_cards::{amber, inks, line};
use super::design_tokens as tokens;
use super::{feedback_hook, glass};
use crate::glyphs::{self, Icon, Piece};
use crate::input_source::InputSource;
use crate::live_glass::LIVE_BACKDROP;
use crate::strings::tr;
use crate::theme::{regular, semibold, text};

pub mod video_glass;

/// The round of the dock's corners that face the video.
pub const RADIUS: f32 = 24.0;
/// Around the page inside the dock.
pub const MARGIN: f32 = 16.0;
/// A window narrower than this takes the dock's compact width. The tokens have the two widths,
/// not where one becomes the other.
pub const COMPACT_BELOW: f32 = 960.0;
/// How much of the window the sheet takes, where `placement` gives the sheet.
const SHEET_PART: f32 = 0.72;
/// The scrim's share of the blurred video under the dock's fill, so the video is as dim under
/// the dock as beside it (the blur is taken of the bare frame).
const BACKDROP_DIM: u8 = 166;
const RAIL_CELL: f32 = 56.0;
const RAIL_GAP: f32 = 4.0;

/// What the Control Center keeps while a stream runs, so opening it again lands where it was left.
#[derive(Clone, Debug)]
pub struct State {
    /// The module whose page shows.
    pub module: Module,
    /// The control that had the focus, on a module's page.
    focus: Option<Id>,
    open: bool,
    /// The focus was on the rail last frame: moving along it there turns the pages.
    on_rail: bool,
    /// A trigger asked for the next (true) or previous module since the last frame.
    step: Option<bool>,
}

impl Default for State {
    fn default() -> Self {
        Self { module: Module::Quick, focus: None, open: false, on_rail: false, step: None }
    }
}

impl State {
    /// The Control Center closed: the next frame it draws is an opening.
    pub fn closed(&mut self) {
        self.open = false;
        self.on_rail = false;
        self.step = None;
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// A trigger steps to the next (`forward`) or previous module; the next frame turns the page.
    pub fn request_step(&mut self, forward: bool) {
        self.step = Some(forward);
    }

    /// Shows `module`: a button ran a card of another module, or a guest asked to join. The page's
    /// first control takes the focus.
    pub fn select(&mut self, ctx: &Context, module: Module) {
        if self.module != module {
            self.module = module;
            surrender(ctx);
        }
    }
}

fn surrender(ctx: &Context) {
    ctx.memory_mut(|m| {
        if let Some(id) = m.focused() {
            m.surrender_focus(id);
        }
    });
}

/// A module's button on the rail.
pub fn rail_id(module: Module) -> Id {
    Id::unique(("control-center-rail", module.key()))
}

/// Where the Control Center's parts are in a window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Geometry {
    pub placement: Placement,
    /// The whole dock (or sheet).
    pub panel: Rect,
    pub rail: Rect,
    /// Header, page and footer, inside the margin.
    pub content: Rect,
}

/// The dock at the right edge over the window's whole height, compact in a narrow window; the
/// sheet over the lower part only where the policy's `placement` says so.
pub fn geometry(screen: Rect) -> Geometry {
    let placement = model::placement(FormFactor::Desktop, screen.width(), screen.height());
    let rail_size = tokens::SIZE_CONTROL_CENTER_RAIL;
    match placement {
        Placement::Dock => {
            let width = if screen.width() < COMPACT_BELOW { tokens::SIZE_CONTROL_CENTER_WIDTH_COMPACT } else { tokens::SIZE_CONTROL_CENTER_WIDTH };
            let panel = Rect::from_min_max(pos2(screen.right() - width.min(screen.width()), screen.top()), screen.max);
            let rail = Rect::from_min_max(panel.min, pos2(panel.left() + rail_size, panel.bottom()));
            let content = Rect::from_min_max(pos2(rail.right(), panel.top()), panel.max).shrink(MARGIN);
            Geometry { placement, panel, rail, content }
        }
        Placement::Sheet => {
            let panel = Rect::from_min_max(pos2(screen.left(), screen.bottom() - (screen.height() * SHEET_PART).round()), screen.max);
            let rail = Rect::from_min_max(panel.min, pos2(panel.right(), panel.top() + rail_size));
            let content = Rect::from_min_max(pos2(panel.left(), rail.bottom()), panel.max).shrink(MARGIN);
            Geometry { placement, panel, rail, content }
        }
    }
}

/// What one frame of the Control Center shows around its page.
#[derive(Clone, Copy, Debug)]
pub struct Frame<'a> {
    /// The game, or the page a card opened.
    pub title: &'a str,
    /// The stream's mode, or where the page belongs.
    pub line: &'a str,
    /// What `nightjar_policy::control_center::surface` gave for the open dock.
    pub surface: Surface,
    /// What was used last: the rail hints its triggers or its keys.
    pub input: InputSource,
    /// A page a card opened, or a question, is up: the rail rests until it is left.
    pub sub_page: bool,
}

/// What the user did around the page.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Shown {
    /// A click on the video beside the dock: back to the stream.
    pub scrim_clicked: bool,
}

/// A module's name on the rail, as Android's `control_center_<key>` strings say it.
pub fn module_label(module: Module) -> &'static str {
    match module {
        Module::Quick => tr("Quick"),
        Module::Picture => tr("Picture"),
        Module::Audio => tr("Audio"),
        Module::Clips => tr("Clips"),
        Module::Pc => tr("PC"),
        Module::Controller => tr("Controller"),
        Module::Guests => tr("Guests"),
        Module::Power => tr("Power"),
    }
}

/// The module icons the glyph set has none for, drawn on its 18-point grid in its stroke.
fn module_icon(painter: &egui::Painter, rect: Rect, module: Module, color: Color32) {
    let s = rect.width() / 18.0;
    let at = |x: f32, y: f32| rect.min + vec2(x, y) * s;
    let stroke = egui::Stroke::new(1.6 * s.max(1.0), color);
    match module {
        Module::Picture => glyphs::icon(painter, rect, Icon::Screen, color),
        Module::Audio => glyphs::icon(painter, rect, Icon::Speaker, color),
        Module::Pc => glyphs::icon(painter, rect, Icon::Terminal, color),
        Module::Controller => glyphs::icon(painter, rect, Icon::Controller, color),
        Module::Power => glyphs::icon(painter, rect, Icon::Power, color),
        // A bolt.
        Module::Quick => {
            let bolt = vec![at(10.5, 1.5), at(4.0, 10.0), at(8.6, 10.0), at(7.5, 16.5), at(14.0, 8.0), at(9.4, 8.0)];
            painter.add(egui::Shape::closed_line(bolt, stroke));
        }
        // A frame with the record dot.
        Module::Clips => {
            painter.rect_stroke(Rect::from_min_max(at(2.0, 4.0), at(16.0, 14.0)), 3.0 * s, stroke, egui::StrokeKind::Middle);
            painter.circle_filled(at(9.0, 9.0), 2.2 * s, color);
        }
        // Two players.
        Module::Guests => {
            painter.circle_stroke(at(7.0, 6.0), 2.6 * s, stroke);
            painter.add(egui::Shape::line(vec![at(2.5, 15.5), at(2.5, 14.0), at(4.5, 11.6), at(9.5, 11.6), at(11.5, 14.0), at(11.5, 15.5)], stroke));
            painter.circle_stroke(at(13.2, 7.2), 2.0 * s, stroke);
            painter.add(egui::Shape::line(vec![at(13.6, 11.8), at(15.6, 13.6), at(15.6, 15.5)], stroke));
        }
    }
}

/// The triggers that step through the modules, as the controller in hand names them.
fn triggers(style: glyphs::Style) -> (&'static str, &'static str) {
    if style == glyphs::Style::PlayStation {
        ("L2", "R2")
    } else if style == glyphs::Style::Nintendo {
        ("ZL", "ZR")
    } else {
        ("LT", "RT")
    }
}

fn paint_panel(ui: &Ui, g: &Geometry, surface: Surface) {
    let ctx = ui.ctx();
    let screen = ctx.content_rect();
    if surface == Surface::Blurred && !crate::theme::high_contrast() {
        // The video under the dock, blurred by whoever owns the frame: the stream's presenter (or
        // the test canvas, which blurs what was drawn before this boundary).
        glass::boundary(ui.painter(), vec![g.panel]);
    }
    ui.painter().rect_filled(screen, 0, tokens::SURFACE_CONTROL_CENTER_SCRIM);
    let high = crate::theme::high_contrast();
    let dock = g.placement == Placement::Dock;
    // The side at the window's edge is drawn past it, so only the corners facing the video are round.
    let outer = if dock { g.panel.with_max_x(g.panel.right() + RADIUS) } else { g.panel.with_max_y(g.panel.bottom() + RADIUS) };
    let r = RADIUS as u8;
    let corners = if dock { CornerRadius { nw: r, sw: r, ne: 0, se: 0 } } else { CornerRadius { nw: r, ne: r, sw: 0, se: 0 } };
    let mut shapes = Vec::new();
    let fill = match if high { Surface::Solid } else { surface } {
        Surface::Blurred => {
            let uv = Rect::from_min_max(
                pos2((outer.left() - screen.left()) / screen.width(), (outer.top() - screen.top()) / screen.height()),
                pos2((outer.right() - screen.left()) / screen.width(), (outer.bottom() - screen.top()) / screen.height()),
            );
            glass::image(&mut shapes, ctx, outer, RADIUS, LIVE_BACKDROP, uv, Color32::from_gray(BACKDROP_DIM));
            tokens::SURFACE_CONTROL_CENTER_FILL
        }
        Surface::Baked => tokens::SURFACE_CONTROL_CENTER_BAKED_FILL,
        Surface::Solid => tokens::SURFACE_CONTROL_CENTER_SOLID_FILL,
    };
    ui.painter().extend(std::mem::take(&mut shapes));
    ui.painter().rect_filled(g.panel, corners, fill);
    let (width, top, bottom) = if high {
        (tokens::GLASS_MENU_SOLID_RIM_WIDTH, tokens::GLASS_MENU_SOLID_RIM_TOP, tokens::GLASS_MENU_SOLID_RIM_BOTTOM)
    } else {
        (1.0, tokens::SURFACE_CONTROL_CENTER_RIM_TOP, tokens::SURFACE_CONTROL_CENTER_RIM_BOTTOM)
    };
    glass::rim(&mut shapes, ctx, outer, RADIUS, width, &glass::gradient(outer, 180.0, &[(0.0, top), (1.0, bottom)]));
    ui.painter().extend(shapes);
    if high {
        // No tint on black: a line parts the rail from the page.
        let (from, to) = if dock { (g.rail.right_top(), g.rail.right_bottom()) } else { (g.rail.left_bottom(), g.rail.right_bottom()) };
        ui.painter().line_segment([from, to], egui::Stroke::new(1.0, tokens::GLASS_MENU_SOLID_RIM_TOP));
    } else {
        ui.painter().rect_filled(g.rail, corners, tokens::SURFACE_CONTROL_CENTER_RAIL_FILL);
    }
}

fn header(ui: &mut Ui, rect: Rect, frame: &Frame<'_>) -> f32 {
    let (ink, quiet) = inks();
    let title = line(ui, frame.title, semibold(text::CARD_TITLE), ink, rect.width());
    let mode = line(ui, frame.line, regular(text::CAPTION), quiet, rect.width());
    let top = rect.top() + 2.0;
    let mode_top = top + title.size().y + 2.0;
    let bottom = mode_top + mode.size().y;
    ui.painter().galley(pos2(rect.left(), top), title, ink);
    ui.painter().galley(pos2(rect.left(), mode_top), mode, quiet);
    bottom
}

fn rail(ui: &mut Ui, state: &mut State, dock: &Dock, frame: &Frame<'_>, g: &Geometry) {
    let ctx = ui.ctx().clone();
    let vertical = g.placement == Placement::Dock;
    let (ink, quiet) = inks();
    if !frame.sub_page {
        let focused = ctx.memory(|m| m.focused());
        let on = dock.pages.iter().map(|page| page.module).find(|module| Some(rail_id(*module)) == focused);
        if let Some(module) = on.filter(|module| *module != state.module) {
            if state.on_rail {
                // Along the rail the focus turns the pages, as a console's tabs do.
                state.module = module;
                feedback_hook::report(&ctx, Event::FocusTick);
                ctx.request_repaint();
            } else {
                // Coming over from the page it lands on the module shown, and changes nothing.
                ctx.memory_mut(|m| m.request_focus(rail_id(state.module)));
            }
        }
        state.on_rail = on.is_some();
    }
    // The hints that step the modules: the triggers of the controller in hand, or the keys.
    let (before, after) = match frame.input {
        InputSource::Pad(style) => {
            let (left, right) = triggers(style);
            (Some(vec![Piece::Pill(left)]), Some(vec![Piece::Pill(right)]))
        }
        _ => (None, None),
    };
    let hint = glyphs::HEIGHT + 2.0 * RAIL_GAP;
    let count = dock.pages.len().max(1) as f32;
    let (lead, cell) = if vertical {
        let tall = ((g.rail.height() - 2.0 * hint - 2.0 * RAIL_GAP) / count - RAIL_GAP).clamp(40.0, RAIL_CELL);
        (hint, vec2(g.rail.width() - 2.0 * RAIL_GAP, tall))
    } else {
        let lead = if before.is_some() { 44.0 } else { MARGIN };
        (lead, vec2(((g.rail.width() - 2.0 * lead) / count - RAIL_GAP).clamp(32.0, 72.0), RAIL_CELL))
    };
    let centred = |ui: &Ui, pieces: &[Piece], middle: egui::Pos2| {
        glyphs::paint(ui, pos2(middle.x - glyphs::width(ui, pieces) / 2.0, middle.y), pieces);
    };
    if let (Some(before), Some(after)) = (&before, &after) {
        let (first, last) = if vertical {
            (pos2(g.rail.center().x, g.rail.top() + hint / 2.0 + RAIL_GAP), pos2(g.rail.center().x, g.rail.bottom() - hint / 2.0 - RAIL_GAP))
        } else {
            (pos2(g.rail.left() + lead / 2.0, g.rail.center().y), pos2(g.rail.right() - lead / 2.0, g.rail.center().y))
        };
        if !frame.sub_page {
            centred(ui, before, first);
            centred(ui, after, last);
        }
    } else if vertical && frame.input == InputSource::Keyboard && !frame.sub_page {
        ui.painter().text(pos2(g.rail.center().x, g.rail.bottom() - MARGIN), Align2::CENTER_BOTTOM, "Ctrl+Tab", regular(10.0), quiet);
    }
    for (index, page) in dock.pages.iter().enumerate() {
        let step = index as f32;
        let min = if vertical {
            pos2(g.rail.left() + RAIL_GAP, g.rail.top() + lead + RAIL_GAP + step * (cell.y + RAIL_GAP))
        } else {
            pos2(g.rail.left() + lead + step * (cell.x + RAIL_GAP), g.rail.top() + (g.rail.height() - cell.y) / 2.0)
        };
        let rect = Rect::from_min_size(min, cell);
        let selected = page.module == state.module;
        let label = module_label(page.module);
        let sense = if frame.sub_page { Sense::hover() } else { Sense::click() };
        let response = ui.interact(rect, rail_id(page.module), sense);
        if response.clicked() && !selected {
            state.module = page.module;
            feedback_hook::report(&ctx, Event::Confirm);
        }
        if selected {
            let (fill, rim, width) = super::control_center_cards::card_colors(true);
            ui.painter().rect_filled(rect, 14, fill);
            ui.painter().rect_stroke(rect, 14, egui::Stroke::new(width, rim), egui::StrokeKind::Inside);
        } else if response.hovered() {
            ui.painter().rect_filled(rect, 14, tokens::SURFACE_CONTROL_CENTER_CARD_FILL);
        }
        super::focus_ring(ui, rect, &response, 14);
        let dim = if frame.sub_page && !selected { 0.45 } else { 1.0 };
        let colour = if selected { amber() } else { quiet.gamma_multiply(dim) };
        let icon = Rect::from_center_size(pos2(rect.center().x, rect.top() + (rect.height() * 0.36).round()), vec2(20.0, 20.0));
        module_icon(ui.painter(), icon, page.module, colour);
        let name = line(ui, label, regular(10.5), if selected { ink } else { quiet.gamma_multiply(dim) }, rect.width() - 2.0);
        let at = pos2(rect.center().x - name.size().x / 2.0, rect.bottom() - (rect.height() * 0.1).round() - name.size().y);
        ui.painter().galley(at, name, ink);
        let (enabled, said) = (!frame.sub_page, label.to_owned());
        response.widget_info(|| egui::WidgetInfo::selected(egui::Role::RadioButton, enabled, selected, &said));
    }
    // Nothing on the page took the focus (a page of read-only rows): the module shown takes it, so
    // a controller always has somewhere to start from.
    if !frame.sub_page && ctx.memory(|m| m.focused().is_none()) {
        ctx.memory_mut(|m| m.request_focus(rail_id(state.module)));
        state.on_rail = true;
    }
}

/// One frame of the open Control Center: the scrim over the video, the dock, the rail, and inside
/// it `page` for the module shown (given a `Ui` that scrolls) and `footer` pinned under it.
pub fn show(ctx: &Context, state: &mut State, dock: &Dock, frame: &Frame<'_>, page: impl FnOnce(&mut Ui, Module), footer: impl FnOnce(&mut Ui)) -> Shown {
    let screen = ctx.content_rect();
    let g = geometry(screen);
    if !state.open {
        state.open = true;
        // Where the focus was the last time: gone with its control, the page's first one takes it.
        if let Some(id) = state.focus {
            ctx.memory_mut(|m| m.request_focus(id));
        }
    }
    if dock.page(state.module).is_none() {
        state.module = dock.pages.first().map_or(Module::Quick, |page| page.module);
    }
    let mut step = state.step.take();
    if !frame.sub_page {
        ctx.input_mut(|i| {
            if i.consume_key(Modifiers::CTRL | Modifiers::SHIFT, Key::Tab) {
                step = Some(false);
            } else if i.consume_key(Modifiers::CTRL, Key::Tab) {
                step = Some(true);
            }
        });
        if let Some(forward) = step {
            let next = dock.step(state.module, forward);
            if next != state.module {
                state.module = next;
                surrender(ctx);
                feedback_hook::report(ctx, Event::FocusTick);
            }
        }
    }
    let module = state.module;
    let mut shown = Shown::default();
    egui::Area::new(Id::unique("control-center")).order(Order::Middle).fixed_pos(screen.min).constrain(false).fade_in(false).show(ctx, |ui| {
        ui.set_clip_rect(screen);
        let beside = if g.placement == Placement::Dock { screen.with_max_x(g.panel.left()) } else { screen.with_max_y(g.panel.top()) };
        // A click only: focus never rests on the video.
        shown.scrim_clicked = ui.interact(beside, Id::unique("control-center-scrim"), Sense::CLICK).clicked();
        ui.interact(g.panel, Id::unique("control-center-panel"), Sense::CLICK);
        paint_panel(ui, &g, frame.surface);
        let header_bottom = header(ui, g.content, frame);
        // The footer is pinned: its height is last frame's, the page scrolls in what is left.
        let footer_id = Id::unique("control-center-footer-height");
        let footer_height: f32 = ctx.data(|d| d.get_temp(footer_id)).unwrap_or(112.0);
        let footer_rect = Rect::from_min_max(pos2(g.content.left(), g.content.bottom() - footer_height), g.content.max);
        // Wider than the content by the focus ring's reach, so a ring at the edge is not cut.
        let ring = 8.0;
        let page_rect = Rect::from_min_max(pos2(g.content.left() - ring, header_bottom + MARGIN - ring), pos2(g.content.right() + ring, footer_rect.top() - MARGIN + ring));
        if page_rect.height() > 8.0 {
            ui.scope_builder(UiBuilder::new().max_rect(page_rect), |ui| {
                egui::ScrollArea::vertical().id_salt(("control-center-page", module.key(), frame.sub_page)).auto_shrink(false).show(ui, |ui| {
                    egui::Frame::new().inner_margin(egui::Margin::same(ring as i8)).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        page(ui, module);
                    });
                });
            });
        }
        let used = ui.scope_builder(UiBuilder::new().max_rect(footer_rect), footer).response.rect.height();
        if (used - footer_height).abs() > 0.5 {
            ctx.data_mut(|d| d.insert_temp(footer_id, used));
            ctx.request_repaint();
        }
        rail(ui, state, dock, frame, &g);
    });
    if !frame.sub_page && let Some(id) = ctx.memory(|m| m.focused()) {
        state.focus = Some(id);
    }
    shown
}

#[cfg(test)]
mod tests;
