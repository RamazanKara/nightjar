//! One game's lookup as steps the app carries out: [`Job::next`] answers with a request to send,
//! a wait, or the finished [`Record`]. The job keeps no socket and reads no clock; the app passes
//! each reply and its monotonic `now_ms` in.
//!
//! SteamGridDB first, when there is its key and art is wanted: the game by its Steam app id, else
//! by its cleaned name, then the cover, hero and logo wanted. Then IGDB, when there are its client
//! ID and secret and facts are wanted: a token when none is held, the game through its Steam id
//! (`external_games`), else by name, then its time to beat.

use serde::{Deserialize, Serialize};

use super::cache::{Outcome, Record};
use super::names::{self, clean};
use super::pace::{self, Pace};
use super::redact::redact;
use super::services;

/// Who a request goes to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Service {
    SteamGridDb,
    Twitch,
    Igdb,
    /// An image file a record points at, from SteamGridDB's image host; no key, no pace.
    Image,
}

/// One HTTPS request. Its URL and headers can hold keys: the app never logs them.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Request {
    pub service: Service,
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    /// The body of a POST.
    pub body: Option<String>,
}

impl std::fmt::Debug for Request {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // No URL (Twitch's carries the secret) and no headers (they carry the keys).
        write!(f, "Request {{ {:?} {} }}", self.service, self.method)
    }
}

/// What came back: the HTTP status (0 when no answer came: no network, a timeout, a TLS
/// failure), its body as text and its `Retry-After` in seconds when it had one.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reply {
    pub status: u16,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub retry_after: Option<u64>,
}

/// The kinds of art.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Art {
    /// 600 by 900.
    Cover,
    /// Wide, for behind a game's page.
    Hero,
    Logo,
}

/// The player's keys. Empty is none.
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Keys {
    #[serde(default)]
    pub steamgriddb: String,
    #[serde(default)]
    pub igdb_client_id: String,
    #[serde(default)]
    pub igdb_client_secret: String,
}

impl std::fmt::Debug for Keys {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let set = |s: &str| if s.trim().is_empty() { "none" } else { super::redact::MASK };
        write!(f, "Keys {{ steamgriddb: {}, igdb_client_id: {}, igdb_client_secret: {} }}", set(&self.steamgriddb), set(&self.igdb_client_id), set(&self.igdb_client_secret))
    }
}

impl Keys {
    pub fn art(&self) -> bool {
        !self.steamgriddb.trim().is_empty()
    }

    pub fn info(&self) -> bool {
        !self.igdb_client_id.trim().is_empty() && !self.igdb_client_secret.trim().is_empty()
    }

    /// Whether any service can be asked. Without, the apps start no job and show nothing new.
    pub fn any(&self) -> bool {
        self.art() || self.info()
    }

    pub fn available(&self) -> super::cache::Available {
        super::cache::Available { art: self.art(), info: self.info() }
    }

    /// The secrets, for [`redact`].
    pub fn secrets(&self) -> [&str; 3] {
        [self.steamgriddb.trim(), self.igdb_client_id.trim(), self.igdb_client_secret.trim()]
    }

    /// A hash of all three keys, for a session to start over when they change.
    fn fingerprint(&self) -> u64 {
        fingerprint(&[self.steamgriddb.trim(), self.igdb_client_id.trim(), self.igdb_client_secret.trim()])
    }

    /// A hash of SteamGridDB's key, which a record's art part was asked with.
    pub fn art_fingerprint(&self) -> u64 {
        fingerprint(&[self.steamgriddb.trim()])
    }

    /// A hash of IGDB's client ID and secret, which a record's facts part was asked with.
    pub fn info_fingerprint(&self) -> u64 {
        fingerprint(&[self.igdb_client_id.trim(), self.igdb_client_secret.trim()])
    }
}

/// FNV-1a over `parts` (each ended by a 0 byte), cut to 53 bits: it travels through Android's
/// JSON, whose numbers are doubles, so a wider one would come back changed.
fn fingerprint(parts: &[&str]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for part in parts {
        for byte in part.bytes().chain(std::iter::once(0)) {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
    }
    hash & ((1 << 53) - 1)
}

/// What a lookup wants.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Wants {
    #[serde(default)]
    pub cover: bool,
    #[serde(default)]
    pub hero: bool,
    #[serde(default)]
    pub logo: bool,
    #[serde(default)]
    pub facts: bool,
}

impl Wants {
    fn art(&self) -> Vec<Art> {
        [(self.cover, Art::Cover), (self.hero, Art::Hero), (self.logo, Art::Logo)].into_iter().filter(|(on, _)| *on).map(|(_, art)| art).collect()
    }
}

/// The game looked up: its name as the host lists it and its Steam app id when the host gave one.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Game {
    pub name: String,
    #[serde(default)]
    pub steam: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Token {
    value: String,
    /// When to get a new one, by the caller's monotonic clock.
    until_ms: u64,
}

/// What lasts from one job to the next while the app runs: the pace, IGDB's token, and a service
/// whose keys were refused. Kept in memory only (the token is a secret too); a change of keys
/// starts it over.
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    #[serde(default)]
    pace: Pace,
    #[serde(default)]
    token: Option<Token>,
    #[serde(default)]
    keys: u64,
    /// SteamGridDB refused the key: not asked again until it changes.
    #[serde(default)]
    art_refused: bool,
    /// Twitch or IGDB refused the client ID or secret.
    #[serde(default)]
    info_refused: bool,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Session {{ token: {}, art_refused: {}, info_refused: {} }}", if self.token.is_some() { "held" } else { "none" }, self.art_refused, self.info_refused)
    }
}

impl Session {
    /// Whether SteamGridDB or IGDB refused the keys this session (the settings may say so).
    pub fn refused(&self) -> (bool, bool) {
        (self.art_refused, self.info_refused)
    }
}

/// What to do next.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Step {
    /// Send this, then call [`Job::next`] with its reply.
    Send(Request),
    /// Wait this many milliseconds, then call [`Job::next`] with no reply.
    Wait(u64),
    /// Finished: keep the record and carry the session into the next job.
    Done(Box<Finished>),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Finished {
    pub record: Record,
    pub session: Session,
    /// What went wrong, for the log, without any key or token.
    pub log: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Stage {
    ArtBySteam(u64),
    ArtSearch,
    ArtImage { game: u64, at: usize },
    InfoBySteam(u64),
    InfoSearch,
    InfoGame(u64),
    InfoTime(u64),
    Done,
}

/// One game's lookup. Serializable, so Android carries it across its JSON calls.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Job {
    game: Game,
    keys: Keys,
    wants: Wants,
    session: Session,
    record: Record,
    stage: Stage,
    /// The request of this stage, once made; sent when `sent`.
    pending: Option<Request>,
    sent: bool,
    /// Tries of the pending request so far that failed or met a 429.
    tries: u32,
    /// No request before this (a failed try's backoff).
    hold_until_ms: u64,
    /// IGDB refused the token once and a new one was asked for.
    renewed: bool,
    /// Number of Twitch OAuth tokens fetched during this job (bounds short-lived token loops).
    #[serde(default)]
    twitch_fetches: u8,
    #[serde(default)]
    log: Vec<String>,
}

impl Job {
    /// A lookup of `game` with `keys` for what it `wants`, carrying the `session` of the last
    /// job (or a new one). `wall_secs` (seconds since the Unix epoch) dates the record.
    pub fn new(game: Game, keys: Keys, wants: Wants, session: Option<Session>, wall_secs: u64) -> Self {
        let mut session = session.unwrap_or_default();
        let fingerprint = keys.fingerprint();
        if session.keys != fingerprint {
            session = Session { pace: session.pace, keys: fingerprint, ..Session::default() };
        }
        let record = Record { key: names::key(&game.name, game.steam), fetched_at: wall_secs, art_keys: Some(keys.art_fingerprint()), info_keys: Some(keys.info_fingerprint()), ..Record::default() };
        let mut job = Job { game, keys, wants, session, record, stage: Stage::Done, pending: None, sent: false, tries: 0, hold_until_ms: 0, renewed: false, twitch_fetches: 0, log: Vec::new() };
        job.stage = job.art_start();
        job
    }

    /// The next step, given the reply to the last [`Step::Send`] (None after a wait or at the start).
    pub fn next(&mut self, reply: Option<Reply>, now_ms: u64) -> Step {
        if let Some(reply) = reply
            && std::mem::take(&mut self.sent)
        {
            self.answer(reply, now_ms);
        }
        if !self.sent && self.session.token.as_ref().is_some_and(|t| t.until_ms <= now_ms) {
            // Out of date: a new one first, and the request made with the old one made again.
            self.session.token = None;
            if self.pending.as_ref().is_some_and(|r| r.service == Service::Igdb) {
                self.pending = None;
            }
        }
        loop {
            if self.stage == Stage::Done {
                return Step::Done(Box::new(Finished { record: self.record.clone(), session: self.session.clone(), log: std::mem::take(&mut self.log) }));
            }
            if self.pending.is_none() {
                match self.request() {
                    Some(request) => self.pending = Some(request),
                    None => continue,
                }
            }
            let Some(request) = self.pending.clone() else { continue };
            let wait = self.session.pace.wait_ms(request.service, now_ms).max(self.hold_until_ms.saturating_sub(now_ms));
            if wait > 0 {
                return Step::Wait(wait);
            }
            self.session.pace.sent(request.service, now_ms);
            self.sent = true;
            return Step::Send(request);
        }
    }

    fn art_start(&mut self) -> Stage {
        let art = !self.wants.art().is_empty() && self.keys.art();
        if !art {
            return self.info_start();
        }
        if self.session.art_refused {
            self.record.art = Outcome::Failed;
            return self.info_start();
        }
        match self.game.steam {
            Some(app) => Stage::ArtBySteam(app),
            None => Stage::ArtSearch,
        }
    }

    fn info_start(&mut self) -> Stage {
        if !self.wants.facts || !self.keys.info() {
            return Stage::Done;
        }
        if self.session.info_refused {
            self.record.info = Outcome::Failed;
            return Stage::Done;
        }
        match self.game.steam {
            Some(app) => Stage::InfoBySteam(app),
            None => Stage::InfoSearch,
        }
    }

    /// The request of the stage, or None when the stage moved on without one.
    fn request(&mut self) -> Option<Request> {
        let key = self.keys.steamgriddb.trim().to_owned();
        let term = clean(&self.game.name);
        match self.stage.clone() {
            Stage::ArtBySteam(app) => Some(services::sgdb_by_steam(&key, app)),
            Stage::ArtSearch => {
                if term.is_empty() {
                    self.record.art = Outcome::NotFound;
                    self.stage = self.info_start();
                    return None;
                }
                Some(services::sgdb_search(&key, &term))
            }
            Stage::ArtImage { game, at } => {
                let kinds = self.wants.art();
                let Some(kind) = kinds.get(at) else {
                    let any = self.record.cover.is_some() || self.record.hero.is_some() || self.record.logo.is_some();
                    self.record.art = if any { Outcome::Found } else { Outcome::NotFound };
                    self.stage = self.info_start();
                    return None;
                };
                Some(match kind {
                    Art::Cover => services::sgdb_covers(&key, game),
                    Art::Hero => services::sgdb_heroes(&key, game),
                    Art::Logo => services::sgdb_logos(&key, game),
                })
            }
            Stage::Done => None,
            info => {
                let id = self.keys.igdb_client_id.trim().to_owned();
                let Some(token) = self.session.token.clone() else {
                    return Some(services::twitch_token(&id, self.keys.igdb_client_secret.trim()));
                };
                Some(match info {
                    Stage::InfoBySteam(app) => services::igdb_external(&id, &token.value, app),
                    Stage::InfoSearch => {
                        if term.is_empty() {
                            self.record.info = Outcome::NotFound;
                            self.stage = Stage::Done;
                            return None;
                        }
                        services::igdb_search(&id, &token.value, &term)
                    }
                    Stage::InfoGame(game) => services::igdb_game(&id, &token.value, game),
                    Stage::InfoTime(game) => services::igdb_time_to_beat(&id, &token.value, game),
                    _ => return None,
                })
            }
        }
    }

    fn note(&mut self, what: String) {
        let secrets = self.keys.secrets();
        let mut all: Vec<&str> = secrets.to_vec();
        if let Some(token) = &self.session.token {
            all.push(&token.value);
        }
        self.log.push(redact(&what, &all));
    }

    /// Ends the part of the stage in flight as failed and moves to the next part.
    fn fail_part(&mut self) {
        self.pending = None;
        self.tries = 0;
        // A failed try's backoff is no reason to hold the next part; the service's pace still is.
        self.hold_until_ms = 0;
        if self.is_art() {
            self.record.art = Outcome::Failed;
            self.stage = self.info_start();
        } else {
            self.record.info = Outcome::Failed;
            // Facts found before the time to beat failed are kept, and tried again with it.
            self.stage = Stage::Done;
        }
    }

    fn is_art(&self) -> bool {
        matches!(self.stage, Stage::ArtBySteam(_) | Stage::ArtSearch | Stage::ArtImage { .. })
    }

    fn moved_on(&mut self, stage: Stage) {
        self.stage = stage;
        self.pending = None;
        self.tries = 0;
        self.hold_until_ms = 0;
    }

    fn answer(&mut self, reply: Reply, now_ms: u64) {
        let Some(request) = self.pending.clone() else { return };
        let service = request.service;
        let name = match service {
            Service::SteamGridDb => "SteamGridDB",
            Service::Twitch => "Twitch token",
            Service::Igdb => "IGDB",
            Service::Image => "image",
        };
        // Too many requests, a server error or no answer: the same request again, later.
        if reply.status == 429 || reply.status == 0 || reply.status >= 500 {
            self.tries += 1;
            let pause = if reply.status == 429 { self.session.pace.too_many(service, now_ms, reply.retry_after, self.tries) } else { pace::backoff_ms(self.tries) };
            self.hold_until_ms = now_ms.saturating_add(pause);
            if self.tries >= pace::TRIES {
                let what = if reply.status == 0 { "no answer".to_owned() } else { format!("HTTP {}", reply.status) };
                self.note(format!("{name}: {what} after {} tries for {:?}", self.tries, self.game.name));
                self.pending = None;
                self.tries = 0;
                self.fail_part();
            }
            return;
        }
        if service == Service::Twitch {
            match (reply.status, services::twitch_token_answer(&reply.body)) {
                (200, Some((value, expires_in))) => {
                    // Reject already-expired tokens and bound repeated short-lived token fetches
                    // so a misconfigured OAuth endpoint cannot spin the lookup loop forever.
                    let usable_secs = if expires_in > 60 {
                        expires_in - 60
                    } else if expires_in > 0 {
                        (expires_in / 2).max(1)
                    } else {
                        0
                    };
                    let until_ms = now_ms.saturating_add(usable_secs.saturating_mul(1_000));
                    if usable_secs == 0
                        || until_ms <= now_ms
                        || u32::from(self.twitch_fetches) >= pace::TRIES
                    {
                        self.note(format!("{name}: token expired immediately ({expires_in}s)"));
                        self.pending = None;
                        self.fail_part();
                        return;
                    }
                    self.twitch_fetches = self.twitch_fetches.saturating_add(1);
                    self.session.token = Some(Token { value, until_ms });
                    self.pending = None;
                    self.tries = 0;
                }
                (200, _) => {
                    self.note(format!("{name}: unreadable Twitch token reply"));
                    self.pending = None;
                    self.fail_part();
                }
                (status, _) => {
                    // 400 and 403 are a wrong client ID or secret.
                    if matches!(status, 400 | 401 | 403) {
                        self.session.info_refused = true;
                        self.note(format!("{name}: HTTP {status}, the IGDB client ID or secret was not accepted"));
                    } else {
                        self.note(format!("{name}: HTTP {status}"));
                    }
                    self.pending = None;
                    self.fail_part();
                }
            }
            return;
        }
        if matches!(reply.status, 401 | 403) {
            if service == Service::Igdb && !self.renewed {
                // The token ran out early or was revoked: one new token, then the same request.
                self.renewed = true;
                self.session.token = None;
                self.pending = None;
                return;
            }
            if service == Service::SteamGridDb {
                self.session.art_refused = true;
            } else {
                self.session.info_refused = true;
            }
            self.note(format!("{name}: HTTP {}, the key was not accepted", reply.status));
            self.pending = None;
            self.fail_part();
            return;
        }
        let ok = (200..300).contains(&reply.status);
        let not_found = reply.status == 404;
        if !ok && !not_found {
            self.note(format!("{name}: HTTP {} for {:?}", reply.status, self.game.name));
            self.pending = None;
            self.fail_part();
            return;
        }
        let body = if ok { reply.body.as_str() } else { "" };
        let unreadable = |job: &mut Job| {
            job.note(format!("{name}: an answer that could not be read for {:?}", job.game.name));
            job.pending = None;
            job.fail_part();
        };
        match self.stage.clone() {
            Stage::ArtBySteam(_) => match services::sgdb_game_id(body) {
                Some(game) => self.moved_on(Stage::ArtImage { game, at: 0 }),
                // Not under that Steam id: by name.
                None => self.moved_on(Stage::ArtSearch),
            },
            Stage::ArtSearch => match if not_found { Some(Vec::new()) } else { services::sgdb_games(body) } {
                Some(games) => {
                    let names: Vec<&str> = games.iter().map(|(_, name)| name.as_str()).collect();
                    match names::best(&self.game.name, &names) {
                        Some(at) => self.moved_on(Stage::ArtImage { game: games[at].0, at: 0 }),
                        None => {
                            self.record.art = Outcome::NotFound;
                            let next = self.info_start();
                            self.moved_on(next);
                        }
                    }
                }
                None => unreadable(self),
            },
            Stage::ArtImage { game, at } => {
                let image = if not_found { Some(None) } else { services::sgdb_best_image(body) };
                match image {
                    Some(url) => {
                        match self.wants.art().get(at) {
                            Some(Art::Cover) => self.record.cover = url,
                            Some(Art::Hero) => self.record.hero = url,
                            Some(Art::Logo) => self.record.logo = url,
                            None => {}
                        }
                        self.moved_on(Stage::ArtImage { game, at: at + 1 });
                    }
                    None => unreadable(self),
                }
            }
            Stage::InfoBySteam(_) => match if not_found { Some(None) } else { services::igdb_external_game(body) } {
                Some(Some(game)) => self.moved_on(Stage::InfoGame(game)),
                Some(None) => self.moved_on(Stage::InfoSearch),
                None => unreadable(self),
            },
            Stage::InfoSearch | Stage::InfoGame(_) => match if not_found { Some(Vec::new()) } else { services::igdb_games(body) } {
                Some(games) => {
                    let by_id = matches!(self.stage, Stage::InfoGame(_));
                    let pick = if by_id {
                        (!games.is_empty()).then_some(0)
                    } else {
                        let names: Vec<&str> = games.iter().map(|(_, facts)| facts.name.as_str()).collect();
                        names::best(&self.game.name, &names)
                    };
                    match pick {
                        Some(at) => {
                            let (id, facts) = games[at].clone();
                            self.record.facts = Some(facts);
                            self.moved_on(Stage::InfoTime(id));
                        }
                        None => {
                            self.record.info = Outcome::NotFound;
                            self.moved_on(Stage::Done);
                        }
                    }
                }
                None => unreadable(self),
            },
            Stage::InfoTime(_) => {
                let mut facts = self.record.facts.clone().unwrap_or_default();
                if not_found || services::igdb_time_to_beat_answer(body, &mut facts).is_some() {
                    self.record.facts = Some(facts);
                    self.record.info = Outcome::Found;
                    self.moved_on(Stage::Done);
                } else {
                    unreadable(self);
                }
            }
            Stage::Done => {}
        }
    }
}
