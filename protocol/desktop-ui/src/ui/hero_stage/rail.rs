//! Continue: the short rail of the other running and recent games, beside the hero in a wide window
//! and under it in a narrow one, where its lines stand side by side. A line opens its game's page;
//! its menu is the game's.

use super::*;

/// The rail's heading and one line's height.
pub(super) const RAIL_HEAD: f32 = 38.0;
pub(super) const RAIL_ROW: f32 = 73.0;
/// Under the hero, a line is at least this wide before the next one moves down.
pub(super) const LINE_LEAST: f32 = 260.0;
const THUMB: Vec2 = vec2(38.0, 57.0);
const INSET: f32 = 16.0;

/// How many lines stand side by side in a rail `width` wide: one beside the hero, as many as fit
/// under it.
pub(super) fn columns(width: f32, lines: usize, beside: bool) -> usize {
    if beside {
        1
    } else {
        ((width / LINE_LEAST).floor() as usize).clamp(1, lines.max(1))
    }
}

/// The rail's height for `lines` in `columns`.
pub(super) fn height(lines: usize, columns: usize) -> f32 {
    if lines == 0 {
        return 0.0;
    }
    RAIL_HEAD + RAIL_ROW * lines.div_ceil(columns.max(1)) as f32
}

impl Launcher {
    /// The rail in `rect`, its lines in `columns`. The first line takes the focus when nothing on
    /// the hero could.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn continue_rail(
        &mut self,
        ui: &mut Ui,
        view: &View<'_>,
        rect: Rect,
        lines: &[RailLine],
        columns: usize,
        first: &mut bool,
        commands: &mut Vec<Command>,
    ) {
        let ctx = ui.ctx().clone();
        let mut out = Vec::new();
        glass::group(&mut out, &ctx, rect, t::RADIUS_GROUP, shade::TINT_GROUP_ON_ART_FILL.get());
        ui.painter().extend(out);
        let head = ui.painter().layout_no_wrap(tr("Continue").to_owned(), semibold(t::TYPE_HEADLINE_SIZE), *ON_SURFACE);
        ui.painter().galley(rect.left_top() + vec2(INSET, 12.0), head, *ON_SURFACE);
        let columns = columns.max(1);
        let line_w = rect.width() / columns as f32;
        let separator = Stroke::new(1.0, shade::TINT_SEPARATOR.get());
        for (n, item) in lines.iter().enumerate() {
            let (column, row) = ((n % columns) as f32, (n / columns) as f32);
            let top = rect.top() + RAIL_HEAD + RAIL_ROW * row;
            let line = Rect::from_min_size(pos2(rect.left() + line_w * column, top), vec2(line_w, RAIL_ROW));
            if row > 0.0 && column == 0.0 {
                ui.painter().hline(rect.left() + INSET..=rect.right(), top, separator);
            }
            if column > 0.0 {
                ui.painter().vline(line.left(), line.top() + 12.0..=line.bottom() - 12.0, separator);
            }
            self.rail_line(ui, view, line, item, first, commands);
        }
    }

    fn rail_line(&mut self, ui: &mut Ui, view: &View<'_>, line: Rect, item: &RailLine, first: &mut bool, commands: &mut Vec<Command>) {
        let ctx = ui.ctx().clone();
        let response = ui.interact(line, Id::unique(("continue-line", &item.key)), Sense::click());
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, format!("{}, {}", item.title, item.line)));
        let feel = glass::feel(&ctx, &response, 1.0, 1.0);
        if feel.hovered || feel.pressed {
            let lift = if feel.pressed { shade::PRESS_FILL_LIFT.get() } else { shade::HOVER_FILL_LIFT.get() };
            let mut out = Vec::new();
            glass::solid(&mut out, &ctx, line.shrink2(vec2(4.0, 1.0)), 14.0, lift);
            ui.painter().extend(out);
        }
        let cover = Rect::from_min_size(pos2(line.left() + INSET, line.top() + 8.0), THUMB);
        let art = shown_art(view, item.host, item.app);
        let mut out = Vec::new();
        cover_shapes(ui, &mut out, cover, cover.width(), 8.0, Id::unique(("cover-in", item.host, item.app)), &item.title, art, 1.0);
        ui.painter().extend(out);
        if item.has_cover {
            request_cover(ui, view, item.host, item.app, &response, commands);
        }
        let words = (line.width() - INSET - THUMB.x - 12.0 - 12.0).max(24.0);
        let at = pos2(cover.right() + 12.0, line.center().y);
        let set = |text: &str, font: egui::FontId, ink: Color32| {
            let mut job = egui::text::LayoutJob::simple_singleline(text.to_owned(), font, ink);
            job.wrap = egui::text::TextWrapping::truncate_at_width(words);
            ui.painter().layout_job(job)
        };
        let title = set(&item.title, semibold(t::TYPE_BODY_SIZE), *ON_SURFACE);
        ui.painter().galley(at - vec2(0.0, title.size().y), title, *ON_SURFACE);
        let said = set(&item.line, regular(t::TYPE_SUB_SIZE), *ON_SURFACE_VARIANT);
        ui.painter().galley(at + vec2(0.0, 2.0), said, *ON_SURFACE_VARIANT);
        focus_ring(ui, line.shrink2(vec2(4.0, 4.0)), &response, 14);
        match &item.game {
            Some(game) => self.focus_card(&response, &item.key, game, std::mem::take(first)),
            None => self.focus_item(&response, &item.key, std::mem::take(first)),
        }
        if response.clicked() {
            self.open(Screen::Game { host: item.host, app: item.app });
        }
        if let Some(anchor) = menu_request(ui, &response, None) {
            let game = GameTarget::Library { app: item.app, favourite: false };
            self.open_menu(Dialog::GameMenu { host: item.host, game }, anchor, item.key.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn beside_the_hero_the_rail_is_one_column_and_under_it_as_many_as_fit() {
        assert_eq!(columns(300.0, 3, true), 1);
        assert_eq!(columns(888.0, 3, false), 3);
        assert_eq!(columns(600.0, 3, false), 2);
        assert_eq!(columns(200.0, 3, false), 1, "never none");
        assert_eq!(columns(2000.0, 2, false), 2, "never more than its lines");
        assert_eq!(height(3, 1), RAIL_HEAD + 3.0 * RAIL_ROW);
        assert_eq!(height(3, 2), RAIL_HEAD + 2.0 * RAIL_ROW);
        assert_eq!(height(0, 1), 0.0);
    }
}
