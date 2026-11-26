//! Home's hero stage (Android's `HeroStageView`): the game Home is about, running, else the last
//! played, else the library's first. Its cover stands at the stage's start at its own 2:3 shape,
//! lit (rim, top sheen, a deep shadow); beside it the title, one line of facts, then the one amber
//! Play or Resume and at most one quiet second action. Continue is a short rail beside the hero, or
//! under it in a narrow window; the shelves follow. The stage is at most `layout.stageMaxWidth`
//! wide and centred, so an ultrawide gets margins instead of a stretched hero. The game's art lies
//! in the stage's trailing share behind a soft round mask, held to the stage light ([`art`]); a
//! game without art has its tile's gradient there, and with no games at all the stage is dark with
//! one line.

mod art;
mod rail;

use egui::layers::ShapeIdx;
use egui::{Rangef, Shape};

use super::chrome::{capsule_width, control_capsule, icon_capsule, play_button, play_width};
use super::design_tokens as t;
use super::glass;
use super::home::Hero;
use super::pcs::{pc_action, PcAction};
use super::*;

/// The cover's height: this share of the window's, within bounds; never more than this share of
/// the stage's width.
const COVER_OF_HEIGHT: f32 = 0.34;
const COVER_LEAST_H: f32 = 220.0;
const COVER_MOST_H: f32 = 360.0;
const COVER_MOST_OF_WIDTH: f32 = 0.3;
/// Between the cover and the words, and between the words and the rail.
const GAP: f32 = 32.0;
const RAIL_W: f32 = 300.0;
/// The words keep at least this much room beside the rail; with less, the rail goes under the hero.
const WORDS_LEAST: f32 = 420.0;
const RAIL_UNDER_GAP: f32 = 24.0;
const TITLE_TO_FACTS: f32 = 4.0;
const FACTS_TO_BUTTONS: f32 = 22.0;
/// From the line that says what stops Play to the buttons.
const PROBLEM_GAP: f32 = 10.0;
/// Between Play and the second action: room for the focus ring of each.
const BUTTON_GAP: f32 = 16.0;
const DOT: f32 = 9.0;
/// How wide the ambient's colour glows round the cover, and the deep shadow under it.
const GLOW_BLUR: f32 = 70.0;
const SHADOW_Y: f32 = 18.0;
const SHADOW_BLUR: f32 = 44.0;
/// Under the hero, before the first shelf.
const FOOT: f32 = 12.0;
/// A stage without games: of the room around its line and its way to games, the share above them.
const EMPTY_ABOVE: f32 = 0.45;

/// One line of the Continue rail.
pub(super) struct RailLine {
    pub key: String,
    pub game: Option<String>,
    pub host: u64,
    pub app: u32,
    pub title: String,
    pub line: String,
    /// Its game has a cover to ask the PC for.
    pub has_cover: bool,
}

/// What the page keeps for the hero's art: a slot painted before anything else on the page, and
/// where the page starts, which is where the art starts.
#[derive(Clone, Copy)]
pub(super) struct Under {
    pub slot: ShapeIdx,
    pub page_top: f32,
}

/// Where the stage's parts stand.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::ui) struct Placed {
    /// The stage across, and the hero's block down: the cover, or the rail beside it if taller.
    pub stage: Rect,
    pub cover: Rect,
    /// The words and buttons, from the cover to the rail beside it or the stage's end.
    pub words: Rangef,
    /// Continue, beside the hero or under it.
    pub rail: Option<Rect>,
    pub beside: bool,
    /// How many of the rail's lines stand side by side.
    pub columns: usize,
}

/// The cover's size in a window `window_h` tall on a stage `stage_w` wide.
pub(in crate::ui) fn cover_size(window_h: f32, stage_w: f32) -> Vec2 {
    let h = (window_h * COVER_OF_HEIGHT).clamp(COVER_LEAST_H, COVER_MOST_H);
    let w = (h / 1.5).min(stage_w * COVER_MOST_OF_WIDTH);
    vec2(w, w * 1.5)
}

/// The stage on a page whose content spans `page` (its top is where the hero starts) in `window`,
/// with `lines` in Continue.
pub(in crate::ui) fn place(page: Rect, window: Rect, lines: usize) -> Placed {
    let stage_w = page.width().min(t::LAYOUT_STAGE_MAX_WIDTH);
    let left = page.center().x - stage_w / 2.0;
    let size = cover_size(window.height(), stage_w);
    let beside = lines > 0 && stage_w - size.x - 2.0 * GAP - RAIL_W >= WORDS_LEAST;
    let columns = rail::columns(stage_w, lines, beside);
    let rail_h = rail::height(lines, columns);
    let height = if beside { size.y.max(rail_h) } else { size.y };
    let stage = Rect::from_min_size(pos2(left, page.top()), vec2(stage_w, height));
    let cover = Rect::from_min_size(stage.left_top(), size);
    let words_end = if beside { stage.right() - RAIL_W - GAP } else { stage.right() };
    let rail = (lines > 0).then(|| {
        if beside {
            // Its foot on the cover's, where Play is, unless it is the taller.
            Rect::from_min_size(pos2(stage.right() - RAIL_W, stage.top() + (size.y - rail_h).max(0.0)), vec2(RAIL_W, rail_h))
        } else {
            Rect::from_min_size(pos2(stage.left(), stage.bottom() + RAIL_UNDER_GAP), vec2(stage_w, rail_h))
        }
    });
    Placed { stage, cover, words: Rangef::new(cover.right() + GAP, words_end.max(cover.right() + GAP)), rail, beside, columns }
}

/// The one quiet action beside Play: Try again while something stops Play, else Quit game while the
/// game runs, else the star. The game's menu (F2, More, a right click) holds the rest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum Second {
    Retry,
    End,
    Star,
    None,
}

pub(in crate::ui) fn second_action(away: bool, problem: bool, ends: bool, star: bool) -> Second {
    if !away && problem {
        Second::Retry
    } else if ends {
        Second::End
    } else if star {
        Second::Star
    } else {
        Second::None
    }
}

/// The ink of the facts line. By night the fainter label colour keeps 4.5:1 over the brightest the
/// ambient and the art may be; by day the ambient is only washed toward the page, so the line takes
/// the full label colour.
pub(in crate::ui) fn facts_ink() -> Color32 {
    if look() == Look::Day { *ON_SURFACE } else { *ON_SURFACE_VARIANT }
}

/// The store and when the game was last played, as its List line says it.
fn played_line(entry: Option<&console::Entry>, sources: &[Source<'_>], recents: &[crate::config::Recent]) -> String {
    let Some(entry) = entry else { return String::new() };
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    console::list_line(entry, console::last_played(entry, sources, recents), now)
}

/// The title on one line in `type.hero`, cut with "…" when it does not fit.
fn hero_title(ui: &Ui, left: f32, top: f32, width: f32, title: &str) {
    let format = egui::TextFormat {
        font_id: semibold(t::TYPE_HERO_SIZE),
        color: *ON_SURFACE,
        extra_letter_spacing: t::TYPE_HERO_TRACK,
        ..Default::default()
    };
    let mut job = egui::text::LayoutJob::single_section(title.to_owned(), format);
    job.wrap = egui::text::TextWrapping::truncate_at_width(width.max(40.0));
    let galley = ui.painter().layout_job(job);
    ui.painter().galley(pos2(left, top + (t::TYPE_HERO_LINE - galley.size().y) / 2.0), galley, *ON_SURFACE);
}

/// The one line of facts under the title: while the game runs or waits, a dot and its state, then
/// its PC, then the store and when it was last played. Cut with "…" when it does not fit.
fn facts_line(ui: &Ui, left: f32, top: f32, width: f32, state: GameState, rest: &[&str]) {
    let ink = facts_ink();
    let middle = top + t::TYPE_BODY_LINE / 2.0;
    let (dot, word) = match state {
        GameState::Running => (Some(*OK), Some(tr("Running"))),
        GameState::Suspended => (Some(*TERTIARY), Some(tr("Suspended"))),
        GameState::Idle => (None, None),
    };
    let mut x = left;
    if let Some(color) = dot {
        ui.painter().circle_filled(pos2(x + DOT / 2.0, middle), DOT / 2.0, color);
        x += DOT + 8.0;
    }
    let plain = |font: egui::FontId, color: Color32| egui::TextFormat { font_id: font, color, ..Default::default() };
    let mut job = egui::text::LayoutJob::default();
    let rest: Vec<&str> = rest.iter().copied().filter(|part| !part.is_empty()).collect();
    if let Some(word) = word {
        job.append(word, 0.0, plain(semibold(t::TYPE_BODY_SIZE), *ON_SURFACE));
        if !rest.is_empty() {
            job.append(" · ", 0.0, plain(regular(t::TYPE_BODY_SIZE), ink));
        }
    }
    job.append(&rest.join(" · "), 0.0, plain(regular(t::TYPE_BODY_SIZE), ink));
    job.wrap = egui::text::TextWrapping::truncate_at_width((left + width - x).max(24.0));
    let galley = ui.painter().layout_job(job);
    ui.painter().galley(pos2(x, middle - galley.size().y / 2.0), galley, ink);
}

impl Launcher {
    /// The hero stage for `hero` with `lines` in Continue. `first` goes to Play when it can start,
    /// else to the second action, else to the rail.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn hero_stage(
        &mut self,
        ui: &mut Ui,
        view: &View<'_>,
        sources: &[Source<'_>],
        hero: &Hero,
        entry: Option<&console::Entry>,
        lines: &[RailLine],
        under: Under,
        first: &mut bool,
        commands: &mut Vec<Command>,
    ) {
        let window = ui.ctx().content_rect();
        let page = ui.available_rect_before_wrap();
        let at = place(page, window, lines.len());
        let foot = at.rail.filter(|_| !at.beside).map_or(at.stage.bottom(), |rail| rail.bottom());
        let _ = ui.allocate_exact_size(vec2(page.width(), foot - page.top() + FOOT), Sense::hover());
        // The art lies from the page's top to the hero's foot, under everything on the page.
        let art_stage = Rect::from_x_y_ranges(at.stage.x_range(), under.page_top..=at.stage.bottom() + FOOT);
        let art = self.hero_art(ui.ctx(), view, hero, art_stage, window);
        ui.painter().set(under.slot, Shape::Vec(art));
        // The cover's glow spreads under the words: its slot comes first, its focus after Play's.
        let cover_slot = ui.painter().add(Shape::Noop);
        let source = sources.iter().find(|s| s.id == hero.host);
        let game = source.and_then(|s| s.library).and_then(|l| l.games.iter().find(|g| g.app_id == hero.app));
        let played = played_line(entry, sources, &self.recents);
        match (view.hosts.get(hero.host), source, game) {
            (Some(host), Some(source), Some(game)) => self.hero_words(ui, host, source, game, &played, &at, first, commands),
            // A game the library no longer lists: its title and PC, and its page says the rest.
            (host, ..) => {
                let pc = host.map_or("", |h| h.display_name());
                let width = at.words.max - at.words.min;
                let facts_y = at.cover.bottom() - t::TYPE_BODY_LINE;
                facts_line(ui, at.words.min, facts_y, width, GameState::Idle, &[pc, played.as_str()]);
                hero_title(ui, at.words.min, facts_y - TITLE_TO_FACTS - t::TYPE_HERO_LINE, width, &hero.title);
            }
        }
        self.hero_cover(ui, view, hero, at.cover, cover_slot, game.is_some(), commands);
        if let Some(rail) = at.rail {
            self.continue_rail(ui, view, rail, lines, at.columns, first, commands);
        }
        #[cfg(test)]
        ui.ctx().data_mut(|d| d.insert_temp(placed_id(), at));
    }

    /// The art behind the hero: its cover held to the stage light, its tile's gradient when it has
    /// none, nothing while its cover is on its way. A cover that arrives while Home shows fades in.
    fn hero_art(&self, ctx: &egui::Context, view: &View<'_>, hero: &Hero, stage: Rect, window: Rect) -> Vec<Shape> {
        let look = look();
        let source = match shown_art(view, hero.host, hero.app) {
            Some(Art::Ready(texture)) => Some(art::Source::Cover(texture.id(), texture.size_vec2())),
            Some(Art::Missing) | None => {
                let pair = nightjar_policy::tile::tile_pair(nightjar_policy::tile::title_hash(&hero.title));
                let (top, bottom) = super::cards::tile_colors(pair);
                Some(art::Source::Tile(top, bottom))
            }
            Some(Art::Loading | Art::Dropped) => None,
        };
        let seconds = if spring::motion(ctx) { t::MOTION_AMBIENT_MS as f32 / 1000.0 } else { 0.0 };
        let shown = ctx.animate_bool_with_time(Id::unique(("hero-art", hero.host, hero.app)), source.is_some(), seconds);
        #[cfg(test)]
        ctx.data_mut(|d| d.insert_temp(art_id(), source.as_ref().map(|s| matches!(s, art::Source::Tile(..)))));
        match source {
            Some(source) => art::shapes(&art::place(stage, window), &source, look, shown),
            None => Vec::new(),
        }
    }

    /// The title, the facts line, what stops Play, Play and the second action, on the cover's foot.
    #[allow(clippy::too_many_arguments)]
    fn hero_words(
        &mut self,
        ui: &mut Ui,
        host: &Host,
        source: &Source<'_>,
        game: &rubylight_native_api::library::Game,
        played: &str,
        at: &Placed,
        first: &mut bool,
        commands: &mut Vec<Command>,
    ) {
        let (left, width) = (at.words.min, at.words.max - at.words.min);
        let state = self.play_state(host, source, game);
        let buttons_y = at.cover.bottom() - t::SIZE_PLAY_HEIGHT;
        // What stops Play stands above the buttons, as on the game's page; the words above it rise.
        let problem = state.list_problem(host);
        let mut above = buttons_y;
        if let Some(why) = &problem {
            let line = ui.painter().layout(why.clone(), regular(t::TYPE_SUB_SIZE), *ERROR, width);
            above -= PROBLEM_GAP + line.size().y;
            ui.painter().galley(pos2(left, above), line, *ERROR);
        }
        let facts_y = above - FACTS_TO_BUTTONS - t::TYPE_BODY_LINE;
        facts_line(ui, left, facts_y, width, state.detail.state, &[host.display_name(), played]);
        hero_title(ui, left, facts_y - TITLE_TO_FACTS - t::TYPE_HERO_LINE, width, &game.title);
        // Play starts from the PC's app list: read it while the player is here, not on the press.
        if state.detail.can_start && matches!(host.apps, Apps::NotLoaded) {
            commands.push(Command::LoadApps(host.id));
        }
        let away = host.reach == Reach::Offline || (host.reach == Reach::Online && !host.paired());
        let (label, icon, enabled, working, away_action) = if away {
            let action = pc_action(host);
            let icon = match action {
                PcAction::Pair => Icon::Lock,
                other => other.icon().unwrap_or(Icon::Refresh),
            };
            (action.words(), icon, true, false, Some(action))
        } else {
            let label = if state.resumes { tr("Resume") } else { tr("Play") };
            (label, Icon::Play, state.could_play(host), self.playing_soon(host.id, game.app_id), None)
        };
        let play_rect = Rect::from_min_size(pos2(left, buttons_y), vec2(play_width(ui, label), t::SIZE_PLAY_HEIGHT));
        // A game that can start takes the focus even while its app list is on the way, so the first
        // press is never spent on the second action. A list that came without it is no way to start it.
        let play = play_button(ui, play_rect, Id::unique("hero-play"), icon, label, enabled, working);
        let play_first = *first && enabled;
        if play_first {
            *first = false;
        }
        self.focus_item(&play, "hero-play", play_first);
        if play.clicked() {
            match away_action {
                Some(action) => commands.push(action.command(host.id)),
                None => self.press_play(host, game, &state, commands),
            }
        }
        let target = GameTarget::Library { app: game.app_id, favourite: false };
        if away_action.is_none() {
            let go = if state.resumes { en("Resume") } else { en("Play") };
            if let Some(anchor) = menu_request_going(ui, &play, None, go) {
                self.open_menu(Dialog::GameMenu { host: host.id, game: target }, anchor, "hero-play".into());
            }
        }
        let star = console::favourite_of(source, game);
        let next = pos2(play_rect.right() + BUTTON_GAP, buttons_y);
        match second_action(away, problem.is_some(), state.detail.end != EndAction::None, star.is_some()) {
            Second::Retry => {
                let rect = Rect::from_min_size(next, vec2(capsule_width(ui, tr("Try again")), t::SIZE_PLAY_HEIGHT));
                self.list_retry(ui, rect, "hero-retry", std::mem::take(first), host.id, commands);
            }
            Second::End => {
                let closing = host.native.acting;
                let label = tr("Quit game");
                let rect = Rect::from_min_size(next, vec2(capsule_width(ui, label), t::SIZE_PLAY_HEIGHT));
                let end = control_capsule(ui, rect, Id::unique("hero-end"), Icon::Power, label, label, !closing);
                let end_first = !closing && std::mem::take(first);
                self.focus_item(&end, "hero-end", end_first);
                if let Some(anchor) = menu_request_going(ui, &end, None, en("Quit game")) {
                    self.open_menu(Dialog::GameMenu { host: host.id, game: target }, anchor, "hero-end".into());
                }
                if end.clicked() {
                    self.press_end(host, game, &state);
                }
            }
            Second::Star => {
                let starred = console::is_starred(source, game, &self.favourites);
                // The star alone, as on Android's Home: its words, the game menu's, are its tooltip,
                // its hint and what a screen reader says.
                let (said, going) = if starred {
                    (tr("Remove from favourites"), en("Remove from favourites"))
                } else {
                    (tr("Add to favourites"), en("Add to favourites"))
                };
                let glyph = if starred { Icon::StarOn } else { Icon::Star };
                let rect = Rect::from_min_size(next, Vec2::splat(t::SIZE_PLAY_HEIGHT));
                let r = icon_capsule(ui, rect, Id::unique("hero-star"), glyph, said, true);
                let star_first = std::mem::take(first);
                self.focus_item(&r, "hero-star", star_first);
                if let Some(anchor) = menu_request_going(ui, &r, None, going) {
                    self.open_menu(Dialog::GameMenu { host: host.id, game: target }, anchor, "hero-star".into());
                }
                if r.clicked()
                    && let Some(star) = star
                {
                    commands.push(Command::ToggleFavourite(star));
                }
            }
            Second::None => {}
        }
    }

    /// The cover, lit: the ambient's glow round it and a deep shadow, the art (or its tile), the
    /// sheen and the rim. It opens the game's page; its menu is the game's.
    #[allow(clippy::too_many_arguments)]
    fn hero_cover(&mut self, ui: &mut Ui, view: &View<'_>, hero: &Hero, rect: Rect, slot: ShapeIdx, listed: bool, commands: &mut Vec<Command>) {
        let ctx = ui.ctx().clone();
        let cover = ui.interact(rect, Id::unique("hero-cover"), Sense::click());
        cover.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, hero.title.clone()));
        let feel = glass::feel(&ctx, &cover, t::FOCUS_SCALE_CONTROL, t::HOVER_SCALE_COVER);
        let shown = glass::scaled_rect(rect, feel.scale);
        let mut out = Vec::new();
        glass::cover_glow(&mut out, &ctx, shown, t::RADIUS_COVER, self.ambient.glow(), GLOW_BLUR, SHADOW_Y, SHADOW_BLUR);
        let boost = if feel.hovered || feel.focused { t::HOVER_RIM_BOOST } else { 1.0 };
        let art = shown_art(view, hero.host, hero.app);
        cover_shapes(ui, &mut out, shown, rect.width(), t::RADIUS_COVER, Id::unique(("cover-in", hero.host, hero.app)), &hero.title, art, boost);
        if feel.focused {
            glass::focus(&mut out, &ctx, shown, t::RADIUS_COVER, false);
        }
        ui.painter().set(slot, Shape::Vec(out));
        self.focus_item(&cover, "hero-cover", false);
        if cover.clicked() {
            self.open(Screen::Game { host: hero.host, app: hero.app });
        }
        if let Some(anchor) = menu_request(ui, &cover, None) {
            let game = GameTarget::Library { app: hero.app, favourite: false };
            self.open_menu(Dialog::GameMenu { host: hero.host, game }, anchor, "hero-cover".into());
        }
        if listed {
            request_cover(ui, view, hero.host, hero.app, &cover, commands);
        }
    }

    /// No games at all: the dark stage and one line, with the way to games under it.
    pub(super) fn empty_stage(&mut self, ui: &mut Ui, view: &View<'_>, commands: &mut Vec<Command>) {
        let window = ui.ctx().content_rect();
        let page = ui.available_rect_before_wrap();
        let stage_w = page.width().min(t::LAYOUT_STAGE_MAX_WIDTH);
        let height = cover_size(window.height(), stage_w).y;
        let (block, _) = ui.allocate_exact_size(vec2(page.width(), height + FOOT), Sense::hover());
        let ink = *ON_SURFACE_VARIANT;
        let words = tr("No games to show yet. They appear here once a PC that shares its game library is online.");
        let mut job = egui::text::LayoutJob::simple(words.to_owned(), regular(t::TYPE_HEADLINE_SIZE), ink, stage_w.min(t::LAYOUT_SETTINGS_MAX_WIDTH));
        job.halign = Align::Center;
        let line = ui.painter().layout_job(job);
        let line_h = line.size().y;
        let top = block.top() + ((height - line_h - space::S - CONTROL_HEIGHT) * EMPTY_ABOVE).max(0.0);
        ui.painter().galley(pos2(block.center().x, top), line, ink);
        let way = Rect::from_min_max(pos2(block.left(), top + line_h), block.right_bottom());
        ui.scope_builder(UiBuilder::new().max_rect(way).layout(Layout::top_down(Align::Center)), |ui| self.empty_library_way(ui, view, commands));
    }
}

#[cfg(test)]
fn placed_id() -> Id {
    Id::unique("hero-stage-placed")
}

#[cfg(test)]
fn art_id() -> Id {
    Id::unique("hero-stage-art")
}

/// Where the last frame placed the stage, for a test.
#[cfg(test)]
pub(in crate::ui) fn placed(ctx: &egui::Context) -> Option<Placed> {
    ctx.data(|d| d.get_temp(placed_id()))
}

/// What the last frame laid behind the hero, for a test: the tile's gradient (true), the cover
/// (false), or nothing.
#[cfg(test)]
pub(in crate::ui) fn art_drawn(ctx: &egui::Context) -> Option<bool> {
    ctx.data(|d| d.get_temp::<Option<bool>>(art_id())).flatten()
}

#[cfg(test)]
pub(in crate::ui) use art::{day_wash, held, luminance, night_hold};
