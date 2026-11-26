use super::cache::*;
use super::job::*;
use super::library::*;
use super::names::*;
use super::pace::*;
use super::redact::*;

// --- Names -------------------------------------------------------------------------------------

#[test]
fn cleaning_takes_off_marks_tags_and_editions() {
    assert_eq!(clean("DOOM Eternal™"), "DOOM Eternal");
    assert_eq!(clean("Tom Clancy's Rainbow Six® Siege"), "Tom Clancy's Rainbow Six Siege");
    assert_eq!(clean("Mafia: Definitive Edition"), "Mafia");
    assert_eq!(clean("The Witcher 3: Wild Hunt - Game of the Year Edition"), "The Witcher 3: Wild Hunt");
    assert_eq!(clean("Halo: The Master Chief Collection (PC)"), "Halo: The Master Chief Collection");
    assert_eq!(clean("Fallout 4 GOTY"), "Fallout 4");
    assert_eq!(clean("Skyrim [Steam]   Special Edition"), "Skyrim");
    assert_eq!(clean("  Hades  "), "Hades");
}

#[test]
fn cleaning_keeps_what_belongs_to_the_name() {
    // A bracket that is not a platform tag, a word that only ends in an edition word, and a name
    // that is nothing but a suffix all stay.
    assert_eq!(clean("Hitman (2016)"), "Hitman (2016)");
    assert_eq!(clean("Divinity: Original Sin 2"), "Divinity: Original Sin 2");
    assert_eq!(clean("Bigoty"), "Bigoty");
    assert_eq!(clean("GOTY"), "GOTY");
    assert_eq!(clean(""), "");
}

#[test]
fn normalizing_makes_spellings_of_one_name_equal() {
    assert_eq!(normalize("The Witcher 3: Wild Hunt"), normalize("the witcher 3 wild hunt"));
    assert_eq!(normalize("Counter-Strike 2"), "counter strike 2");
    assert_eq!(normalize("Final Fantasy VII"), "final fantasy 7");
    assert_eq!(normalize("Ratchet & Clank"), "ratchet and clank");
    assert_eq!(normalize("Assassin’s Creed"), "assassins creed");
}

#[test]
fn matching_takes_the_same_game_and_refuses_another() {
    let results = ["DOOM", "DOOM Eternal", "DOOM 64"];
    assert_eq!(best("DOOM Eternal™", &results), Some(1));
    assert_eq!(best("Doom", &results), Some(0));
    // A sequel or another game of the series is not the game.
    assert_eq!(best("DOOM 3", &results), None);
    assert_eq!(best("Hades", &["Hades II", "Hadean Lands"]), None);
    assert_eq!(best("Spider-Man", &["Spider-Man 2", "Spider-Man II"]), None);
    assert_eq!(best("Left 4 Dead", &["Left 4 Dead 2"]), None);
    assert_eq!(best("Half-Life 2: Episode 1", &["Half-Life 1: Episode 2"]), None);
    assert_eq!(best("Witcher 3: Wild Hunt", &["The Witcher 3: Wild Hunt"]), Some(0));
    assert_eq!(best("Mafia: Definitive Edition", &["Mafia", "Mafia II"]), Some(0));
    // The service's order decides between two equally good.
    assert_eq!(best("Hitman", &["HITMAN", "Hitman"]), Some(0));
    assert_eq!(best("", &["Anything"]), None);
    assert_eq!(best::<&str>("Hades", &[]), None);
}

#[test]
fn a_record_key_prefers_the_steam_id() {
    assert_eq!(key("Hades™", Some(1145360)), "steam:1145360");
    assert_eq!(key("The Witcher 3: Wild Hunt - GOTY Edition", None), "name:the witcher 3 wild hunt");
    assert_eq!(steam_id(Some("steam:1145360")), Some(1145360));
    assert_eq!(steam_id(Some("epic:abc")), None);
    assert_eq!(steam_id(Some("steam:0")), None);
    assert_eq!(steam_id(None), None);
}

#[test]
fn a_search_term_is_safe_in_a_path() {
    assert_eq!(path_segment("Half-Life 2: Episode One"), "Half-Life%202%3A%20Episode%20One");
    assert_eq!(path_segment("a/b?c#d"), "a%2Fb%3Fc%23d");
    assert_eq!(path_segment("Pokémon"), "Pok%C3%A9mon");
}

// --- Cache -------------------------------------------------------------------------------------

fn record(art: Outcome, info: Outcome) -> Record {
    Record { key: "steam:1".into(), fetched_at: 1_000_000, art, info, ..Record::default() }
}

const BOTH: Available = Available { art: true, info: true };

#[test]
fn a_record_holds_by_how_each_part_ended() {
    let found = record(Outcome::Found, Outcome::Found);
    assert!(found.fresh(1_000_000 + FOUND_FOR - 1, BOTH));
    assert!(!found.fresh(1_000_000 + FOUND_FOR, BOTH));
    let missing = record(Outcome::Found, Outcome::NotFound);
    assert!(missing.fresh(1_000_000 + NOT_FOUND_FOR - 1, BOTH));
    assert!(!missing.fresh(1_000_000 + NOT_FOUND_FOR, BOTH));
    let failed = record(Outcome::Failed, Outcome::Found);
    assert!(failed.fresh(1_000_000 + FAILED_FOR - 1, BOTH));
    assert!(!failed.fresh(1_000_000 + FAILED_FOR, BOTH));
}

#[test]
fn a_key_added_since_makes_a_record_stale_and_none_keeps_it() {
    let art_only = record(Outcome::Found, Outcome::Skipped);
    assert!(art_only.fresh(1_000_001, Available { art: true, info: false }));
    assert!(!art_only.fresh(1_000_001, BOTH), "IGDB keys added: ask for facts now");
    let nothing = record(Outcome::Skipped, Outcome::Skipped);
    assert!(nothing.fresh(u64::MAX, Available::default()));
}

#[test]
fn a_record_from_the_future_is_taken_as_new() {
    assert!(record(Outcome::Found, Outcome::Found).fresh(0, BOTH));
}

#[test]
fn a_new_record_keeps_the_parts_it_did_not_ask_with_their_times() {
    let older = Record { cover: Some("https://c/a.png".into()), art: Outcome::Found, info: Outcome::Skipped, ..record(Outcome::Found, Outcome::Skipped) };
    let facts_only = Record { key: "steam:1".into(), fetched_at: 2_000_000, info: Outcome::Found, facts: Some(Facts { genres: vec!["RPG".into()], ..Facts::default() }), ..Record::default() };
    let merged = facts_only.merged(Some(&older));
    assert_eq!(merged.cover.as_deref(), Some("https://c/a.png"));
    assert_eq!((merged.art, merged.info), (Outcome::Found, Outcome::Found));
    assert_eq!(merged.art_at, Some(1_000_000), "the art keeps its own age");
    // So the art runs out on its own time, not the facts'.
    assert!(!merged.fresh(1_000_000 + FOUND_FOR, BOTH));
    assert!(merged.fresh(1_000_000 + FOUND_FOR - 1, BOTH));
    // A failed retry keeps showing what was found before; another game's record is not taken.
    let failed = Record { key: "steam:1".into(), fetched_at: 3_000_000, art: Outcome::Failed, info: Outcome::Failed, ..Record::default() };
    let kept = failed.clone().merged(Some(&merged));
    assert_eq!(kept.cover.as_deref(), Some("https://c/a.png"));
    assert_eq!(kept.facts.unwrap().genres, ["RPG"]);
    assert_eq!(kept.art, Outcome::Failed);
    let other = Record { key: "steam:2".into(), ..failed.clone() };
    assert_eq!(other.clone().merged(Some(&merged)), other);
    assert_eq!(failed.clone().merged(None), failed);
}

#[test]
fn file_names_are_stable_and_safe() {
    assert_eq!(record_file("steam:1"), record_file("steam:1"));
    assert_ne!(record_file("steam:1"), record_file("steam:2"));
    assert!(record_file("name:a/../b").chars().all(|c| c.is_ascii_hexdigit() || c == '.' || c.is_ascii_lowercase()));
    assert!(image_file("https://cdn2.steamgriddb.com/grid/abc.PNG").ends_with(".png"));
    assert!(image_file("https://cdn2.steamgriddb.com/grid/abc.webp").ends_with(".img"));
    assert_eq!(image_file("https://x/a.jpg").len(), 16 + 4);
}

// --- Redaction ---------------------------------------------------------------------------------

#[test]
fn redaction_masks_known_and_unknown_secrets() {
    let text = "POST https://id.twitch.tv/oauth2/token?client_id=abcdefg12345&client_secret=hijklmn67890&grant_type=client_credentials";
    let out = redact(text, &[]);
    assert!(!out.contains("abcdefg12345") && !out.contains("hijklmn67890"), "{out}");
    assert!(out.contains("grant_type=client_credentials"));
    let out = redact("Authorization: Bearer sgdbkey123 failed", &[]);
    assert!(!out.contains("sgdbkey123"), "{out}");
    let out = redact("{\"access_token\":\"tok98765\",\"expires_in\":5}", &[]);
    assert!(!out.contains("tok98765"), "{out}");
    let out = redact("key mysecretkey seen", &["mysecretkey", "", "ab"]);
    assert_eq!(out, format!("key {MASK} seen"));
}

#[test]
fn keys_never_show_in_debug_output() {
    let keys = Keys { steamgriddb: "sgdb-secret-1".into(), igdb_client_id: "client-2".into(), igdb_client_secret: "secret-3".into() };
    let shown = format!("{keys:?}");
    for secret in keys.secrets() {
        assert!(!shown.contains(secret), "{shown}");
    }
    let request = super::services::twitch_token("client-2", "secret-3");
    let shown = format!("{request:?}");
    assert!(!shown.contains("secret-3") && !shown.contains("client-2"), "{shown}");
}

// --- Pace --------------------------------------------------------------------------------------

#[test]
fn requests_to_one_service_are_spaced_to_four_a_second() {
    let mut pace = Pace::default();
    assert_eq!(pace.wait_ms(Service::Igdb, 1_000), 0);
    pace.sent(Service::Igdb, 1_000);
    assert_eq!(pace.wait_ms(Service::Igdb, 1_100), 150);
    assert_eq!(pace.wait_ms(Service::Twitch, 1_100), 150, "the token shares IGDB's pace");
    assert_eq!(pace.wait_ms(Service::SteamGridDb, 1_100), 0, "another service is not held");
    assert_eq!(pace.wait_ms(Service::Igdb, 1_250), 0);
    assert_eq!(pace.wait_ms(Service::Image, 1_100), 0);
}

#[test]
fn a_429_pauses_by_retry_after_or_doubling() {
    assert_eq!(backoff_ms(1), 1_000);
    assert_eq!(backoff_ms(2), 2_000);
    assert_eq!(backoff_ms(3), 4_000);
    assert_eq!(backoff_ms(40), MAX_BACKOFF_MS);
    let mut pace = Pace::default();
    assert_eq!(pace.too_many(Service::SteamGridDb, 0, None, 2), 2_000);
    assert_eq!(pace.wait_ms(Service::SteamGridDb, 500), 1_500);
    assert_eq!(pace.too_many(Service::Igdb, 0, Some(7), 1), 7_000);
    assert_eq!(pace.wait_ms(Service::Igdb, 0), 7_000);
    assert_eq!(pace.too_many(Service::Igdb, 0, Some(100_000), 1), MAX_BACKOFF_MS, "a huge Retry-After is capped");
}

// --- Library -----------------------------------------------------------------------------------

fn facts(genres: &[&str], normally: Option<u64>) -> Facts {
    Facts { name: "x".into(), genres: genres.iter().map(|g| (*g).to_owned()).collect(), normally, ..Facts::default() }
}

const H: u64 = 3600;

#[test]
fn lengths_fall_into_buckets_by_the_usual_time() {
    assert_eq!(Length::of_seconds(4 * H + 3599), Length::Short);
    assert_eq!(Length::of_seconds(5 * H), Length::Medium);
    assert_eq!(Length::of_seconds(15 * H), Length::Long);
    assert_eq!(Length::of_seconds(40 * H), Length::VeryLong);
    let only_complete = Facts { completely: Some(50 * H), ..Facts::default() };
    assert_eq!(Length::of(&only_complete), Some(Length::VeryLong));
    let hasty_and_normal = Facts { hastily: Some(2 * H), normally: Some(6 * H), ..Facts::default() };
    assert_eq!(Length::of(&hasty_and_normal), Some(Length::Medium));
    assert_eq!(Length::of(&Facts::default()), None);
    for length in Length::ALL {
        assert_eq!(Length::from_key(length.key()), Some(length));
    }
}

#[test]
fn hours_read_as_people_say_them() {
    assert_eq!(hours(None), None);
    assert_eq!(hours(Some(600)), Some("0.5".into()));
    assert_eq!(hours(Some(H + H / 2)), Some("1.5".into()));
    assert_eq!(hours(Some(2 * H + 60)), Some("2".into()));
    assert_eq!(hours(Some(12 * H + 20 * 60)), Some("12".into()));
    assert_eq!(hours(Some(45 * H + 40 * 60)), Some("46".into()));
}

#[test]
fn the_genre_and_length_filters_show_only_matching_games() {
    let a = facts(&["Shooter", "Adventure"], Some(10 * H));
    let b = facts(&["shooter"], Some(50 * H));
    let c = facts(&["Puzzle"], None);
    let games = [Some(&a), Some(&b), Some(&c), None];
    let genres = genres(&games);
    assert_eq!(genres.iter().map(|c| (c.value.as_str(), c.count)).collect::<Vec<_>>(), [("Shooter", 2), ("Adventure", 1), ("Puzzle", 1)]);
    let lengths = lengths(&games);
    assert_eq!(lengths.iter().map(|c| (c.value, c.count)).collect::<Vec<_>>(), [(Length::Medium, 1), (Length::VeryLong, 1)]);
    let shown = |genre: Option<&str>, length: Option<Length>| games.iter().enumerate().filter(|(_, f)| shows(**f, genre, length)).map(|(at, _)| at).collect::<Vec<_>>();
    assert_eq!(shown(None, None), [0, 1, 2, 3], "no filter: every game, facts or not");
    assert_eq!(shown(Some("SHOOTER"), None), [0, 1]);
    assert_eq!(shown(Some("Shooter"), Some(Length::VeryLong)), [1]);
    assert_eq!(shown(None, Some(Length::Short)), Vec::<usize>::new());
    assert_eq!(shown(Some("Puzzle"), None), [2]);
}

// --- The job -----------------------------------------------------------------------------------

fn keys(art: bool, info: bool) -> Keys {
    Keys {
        steamgriddb: if art { "sgdb-key-1234".into() } else { String::new() },
        igdb_client_id: if info { "igdb-client-5678".into() } else { String::new() },
        igdb_client_secret: if info { "igdb-secret-9012".into() } else { String::new() },
    }
}

const ALL: Wants = Wants { cover: true, hero: true, logo: true, facts: true };

fn ok(body: &str) -> Option<Reply> {
    Some(Reply { status: 200, body: body.into(), retry_after: None })
}

fn status(code: u16) -> Option<Reply> {
    Some(Reply { status: code, body: String::new(), retry_after: None })
}

/// Runs `job` against `serve`, which answers each request by its service and URL/body. Waits pass
/// the clock forward; returns what finished and the requests sent.
fn run(mut job: Job, mut serve: impl FnMut(&Request) -> Option<Reply>) -> (Finished, Vec<Request>) {
    let mut now = 10_000u64;
    let mut reply = None;
    let mut sent = Vec::new();
    for _ in 0..200 {
        match job.next(reply.take(), now) {
            Step::Send(request) => {
                reply = serve(&request);
                sent.push(request);
                now += 10;
            }
            Step::Wait(ms) => {
                assert!(ms > 0);
                now += ms;
            }
            Step::Done(done) => return (*done, sent),
        }
    }
    panic!("the job did not finish");
}

fn about(request: &Request) -> String {
    format!("{} {}", request.url, request.body.clone().unwrap_or_default())
}

const TOKEN: &str = r#"{"access_token":"tok-abcdef","expires_in":5587808,"token_type":"bearer"}"#;
const IGDB_GAME: &str = r#"[{"id":7,"name":"Hades","genres":[{"id":1,"name":"Role-playing (RPG)"},{"id":2,"name":"Indie"}],"themes":[{"id":3,"name":"Action"}]}]"#;
const TTB: &str = r#"[{"id":1,"game_id":7,"hastily":72000,"normally":86400,"completely":324000}]"#;

#[test]
fn without_keys_nothing_is_asked() {
    let job = Job::new(Game { name: "Hades".into(), steam: Some(1) }, keys(false, false), ALL, None, 5);
    let (done, sent) = run(job, |_| panic!("asked"));
    assert!(sent.is_empty());
    assert_eq!((done.record.art, done.record.info), (Outcome::Skipped, Outcome::Skipped));
}

#[test]
fn a_steam_game_gets_art_and_facts_by_its_id() {
    let job = Job::new(Game { name: "Hades™".into(), steam: Some(1145360) }, keys(true, true), ALL, None, 5);
    let (done, sent) = run(job, |r| {
        let a = about(r);
        match r.service {
            Service::SteamGridDb if a.contains("/games/steam/1145360") => ok(r#"{"success":true,"data":{"id":42,"name":"Hades"}}"#),
            Service::SteamGridDb if a.contains("/grids/game/42") => {
                ok(r#"{"success":true,"data":[{"id":1,"score":1,"url":"https://cdn2.steamgriddb.com/grid/low.png"},{"id":2,"score":5,"url":"https://cdn2.steamgriddb.com/grid/top.png"}]}"#)
            }
            Service::SteamGridDb if a.contains("/heroes/game/42") => ok(r#"{"success":true,"data":[{"id":3,"score":0,"url":"https://cdn2.steamgriddb.com/hero/h.jpg"}]}"#),
            Service::SteamGridDb if a.contains("/logos/game/42") => ok(r#"{"success":true,"data":[]}"#),
            Service::Twitch => ok(TOKEN),
            Service::Igdb if a.contains("external_games") && a.contains("uid = \"1145360\"") => {
                ok(r#"[{"id":9,"game":99,"uid":"1145360","external_game_source":11},{"id":10,"game":7,"uid":"1145360","external_game_source":1}]"#)
            }
            Service::Igdb if a.contains("/games") && a.contains("where id = 7;") => ok(IGDB_GAME),
            Service::Igdb if a.contains("game_time_to_beats") && a.contains("game_id = 7") => ok(TTB),
            _ => panic!("unexpected {}", a),
        }
    });
    let record = done.record;
    assert_eq!(record.key, "steam:1145360");
    assert_eq!(record.fetched_at, 5);
    assert_eq!(record.cover.as_deref(), Some("https://cdn2.steamgriddb.com/grid/top.png"), "the highest score");
    assert_eq!(record.hero.as_deref(), Some("https://cdn2.steamgriddb.com/hero/h.jpg"));
    assert_eq!(record.logo, None);
    assert_eq!(record.art, Outcome::Found);
    assert_eq!(record.info, Outcome::Found);
    let facts = record.facts.unwrap();
    assert_eq!(facts.genres, ["Role-playing (RPG)", "Indie"]);
    assert_eq!(facts.themes, ["Action"]);
    assert_eq!((facts.hastily, facts.normally, facts.completely), (Some(72000), Some(86400), Some(324000)));
    // Each request carries its own service's credentials, and the IGDB ones the token.
    for request in &sent {
        let headers: Vec<String> = request.headers.iter().map(|(k, v)| format!("{k}: {v}")).collect();
        match request.service {
            Service::SteamGridDb => assert_eq!(headers, ["Authorization: Bearer sgdb-key-1234"]),
            Service::Igdb => {
                assert!(headers.contains(&"Client-ID: igdb-client-5678".to_owned()));
                assert!(headers.contains(&"Authorization: Bearer tok-abcdef".to_owned()));
                assert_eq!(request.method, "POST");
            }
            Service::Twitch => assert!(request.url.starts_with("https://id.twitch.tv/oauth2/token?client_id=igdb-client-5678&client_secret=igdb-secret-9012&grant_type=client_credentials")),
            Service::Image => unreachable!(),
        }
    }
    assert!(done.log.is_empty(), "{:?}", done.log);
}

#[test]
fn a_game_without_an_id_is_found_by_its_cleaned_name() {
    let job =
        Job::new(Game { name: "The Witcher 3: Wild Hunt - Game of the Year Edition".into(), steam: None }, keys(true, true), Wants { cover: true, hero: false, logo: false, facts: true }, None, 5);
    let (done, sent) = run(job, |r| {
        let a = about(r);
        match r.service {
            Service::SteamGridDb if a.contains("/search/autocomplete/The%20Witcher%203%3A%20Wild%20Hunt") => {
                ok(r#"{"success":true,"data":[{"id":5,"name":"The Witcher 2"},{"id":6,"name":"The Witcher 3: Wild Hunt"}]}"#)
            }
            Service::SteamGridDb if a.contains("/grids/game/6") => ok(r#"{"success":true,"data":[{"id":1,"score":0,"url":"https://cdn2.steamgriddb.com/grid/w3.png"}]}"#),
            Service::Twitch => ok(TOKEN),
            Service::Igdb if a.contains("search \"The Witcher 3: Wild Hunt\"") && a.contains("version_parent = null") => {
                ok(r#"[{"id":1,"name":"The Witcher 3: Wild Hunt - Blood and Wine"},{"id":7,"name":"The Witcher 3: Wild Hunt","genres":[{"name":"RPG"}]}]"#)
            }
            Service::Igdb if a.contains("game_time_to_beats") && a.contains("game_id = 7") => ok("[]"),
            _ => panic!("unexpected {}", a),
        }
    });
    assert_eq!(sent.iter().filter(|r| r.url.contains("/heroes/") || r.url.contains("/logos/")).count(), 0, "only what is wanted");
    assert_eq!(done.record.key, "name:the witcher 3 wild hunt");
    assert_eq!(done.record.cover.as_deref(), Some("https://cdn2.steamgriddb.com/grid/w3.png"));
    let facts = done.record.facts.unwrap();
    assert_eq!(facts.genres, ["RPG"]);
    assert_eq!(facts.normally, None, "no time to beat is no time");
    assert_eq!(done.record.info, Outcome::Found);
}

#[test]
fn a_steam_id_the_services_do_not_know_falls_back_to_the_name() {
    let job = Job::new(Game { name: "Hades".into(), steam: Some(3) }, keys(true, true), Wants { cover: true, hero: false, logo: false, facts: true }, None, 5);
    let (done, _) = run(job, |r| {
        let a = about(r);
        match r.service {
            Service::SteamGridDb if a.contains("/games/steam/3") => Some(Reply { status: 404, body: r#"{"success":false,"errors":["Game not found"]}"#.into(), retry_after: None }),
            Service::SteamGridDb if a.contains("/search/autocomplete/Hades") => ok(r#"{"success":true,"data":[{"id":8,"name":"Hades"}]}"#),
            Service::SteamGridDb if a.contains("/grids/game/8") => ok(r#"{"success":true,"data":[]}"#),
            Service::Twitch => ok(TOKEN),
            // Another service's id 3, and no Steam entry.
            Service::Igdb if a.contains("external_games") => ok(r#"[{"id":1,"game":55,"uid":"3","external_game_source":5}]"#),
            Service::Igdb if a.contains("search \"Hades\"") => ok(IGDB_GAME),
            Service::Igdb if a.contains("game_time_to_beats") => ok(TTB),
            _ => panic!("unexpected {}", a),
        }
    });
    assert_eq!(done.record.art, Outcome::NotFound, "the game has no cover there");
    assert_eq!(done.record.info, Outcome::Found);
}

#[test]
fn no_good_match_is_not_found_and_nothing_wrong_is_taken() {
    let job = Job::new(Game { name: "My Homebrew Tool".into(), steam: None }, keys(true, true), Wants { cover: true, hero: false, logo: false, facts: true }, None, 5);
    let (done, sent) = run(job, |r| match r.service {
        Service::SteamGridDb => ok(r#"{"success":true,"data":[{"id":1,"name":"My Time at Portia"}]}"#),
        Service::Twitch => ok(TOKEN),
        Service::Igdb => ok(r#"[{"id":2,"name":"Homebrew Tycoon"}]"#),
        Service::Image => unreachable!(),
    });
    assert_eq!((done.record.art, done.record.info), (Outcome::NotFound, Outcome::NotFound));
    assert_eq!(done.record.cover, None);
    assert_eq!(done.record.facts, None);
    assert_eq!(sent.len(), 3, "search, token, search");
}

#[test]
fn a_429_waits_and_asks_again_and_gives_up_after_its_tries() {
    let mut calls = 0;
    let job = Job::new(Game { name: "Hades".into(), steam: Some(1) }, keys(true, false), Wants { cover: true, hero: false, logo: false, facts: false }, None, 5);
    let mut job_copy = job.clone();
    let (done, _) = run(job, |r| {
        calls += 1;
        if calls == 1 {
            return Some(Reply { status: 429, body: String::new(), retry_after: Some(3) });
        }
        match r.url.as_str() {
            u if u.contains("/games/steam/1") => ok(r#"{"success":true,"data":{"id":42}}"#),
            _ => ok(r#"{"success":true,"data":[{"score":0,"url":"https://cdn2.steamgriddb.com/grid/x.png"}]}"#),
        }
    });
    assert_eq!(done.record.art, Outcome::Found);
    assert_eq!(calls, 3);
    // The wait after the 429 is its Retry-After.
    assert!(matches!(job_copy.next(None, 0), Step::Send(_)));
    assert_eq!(job_copy.next(Some(Reply { status: 429, body: String::new(), retry_after: Some(3) }), 100), Step::Wait(3_000));
    // Every try refused: the art part fails (asked again in an hour), with a log line.
    let job = Job::new(Game { name: "Hades".into(), steam: Some(1) }, keys(true, false), Wants { cover: true, hero: false, logo: false, facts: false }, None, 5);
    let (done, sent) = run(job, |_| status(429));
    assert_eq!(sent.len(), TRIES as usize);
    assert_eq!(done.record.art, Outcome::Failed);
    assert_eq!(done.log.len(), 1);
}

#[test]
fn no_network_fails_quietly_and_is_tried_again_later() {
    let job = Job::new(Game { name: "Hades".into(), steam: None }, keys(true, true), Wants { cover: true, hero: false, logo: false, facts: true }, None, 5);
    let (done, _) = run(job, |_| status(0));
    assert_eq!((done.record.art, done.record.info), (Outcome::Failed, Outcome::Failed));
    assert!(!done.record.fresh(5 + FAILED_FOR, BOTH));
    for line in &done.log {
        assert!(!line.contains("sgdb-key-1234") && !line.contains("igdb-secret-9012") && !line.contains("igdb-client-5678"), "{line}");
    }
}

#[test]
fn a_refused_igdb_token_is_renewed_once_then_the_keys_are_given_up() {
    let mut tokens = 0;
    let job = Job::new(Game { name: "Hades".into(), steam: None }, keys(false, true), Wants { cover: false, hero: false, logo: false, facts: true }, None, 5);
    let (done, _) = run(job, |r| match r.service {
        Service::Twitch => {
            tokens += 1;
            ok(TOKEN)
        }
        _ if tokens == 1 => status(401),
        _ if r.url.contains("game_time_to_beats") => ok(TTB),
        _ => ok(IGDB_GAME),
    });
    assert_eq!(tokens, 2, "a 401 asks for one new token");
    assert_eq!(done.record.info, Outcome::Found);
    // A wrong secret: the token is refused, and the next job in this session asks nothing.
    let job = Job::new(Game { name: "Hades".into(), steam: None }, keys(false, true), Wants { cover: false, hero: false, logo: false, facts: true }, None, 5);
    let (done, _) = run(job, |_| Some(Reply { status: 400, body: r#"{"status":400,"message":"invalid client secret"}"#.into(), retry_after: None }));
    assert_eq!(done.record.info, Outcome::Failed);
    assert_eq!(done.session.refused(), (false, true));
    let next = Job::new(Game { name: "Celeste".into(), steam: None }, keys(false, true), Wants { cover: false, hero: false, logo: false, facts: true }, Some(done.session.clone()), 6);
    let (again, sent) = run(next, |_| panic!("asked with refused keys"));
    assert!(sent.is_empty());
    assert_eq!(again.record.info, Outcome::Failed);
    // New keys start the session over.
    let mut other = keys(false, true);
    other.igdb_client_secret = "another-secret".into();
    let fresh = Job::new(Game { name: "Celeste".into(), steam: None }, other, Wants { cover: false, hero: false, logo: false, facts: true }, Some(done.session), 6);
    let (_, sent) = run(fresh, |r| if r.service == Service::Twitch { ok(TOKEN) } else { ok("[]") });
    assert!(!sent.is_empty());
}

#[test]
fn a_held_token_is_reused_until_it_runs_out() {
    let wants = Wants { cover: false, hero: false, logo: false, facts: true };
    let serve = |r: &Request| match r.service {
        Service::Twitch => ok(r#"{"access_token":"t1","expires_in":120}"#),
        _ if r.url.contains("game_time_to_beats") => ok(TTB),
        _ => ok(IGDB_GAME),
    };
    let (first, sent) = run(Job::new(Game { name: "Hades".into(), steam: None }, keys(false, true), wants, None, 5), serve);
    assert_eq!(sent.iter().filter(|r| r.service == Service::Twitch).count(), 1);
    let mut second = Job::new(Game { name: "Hades".into(), steam: None }, keys(false, true), wants, Some(first.session.clone()), 6);
    let Step::Send(request) = second.next(None, 20_000) else { panic!() };
    assert_eq!(request.service, Service::Igdb, "the held token");
    // 120 s less a minute: by 70 s from its start (10 s) it has run out.
    let mut third = Job::new(Game { name: "Hades".into(), steam: None }, keys(false, true), wants, Some(first.session), 6);
    let Step::Send(request) = third.next(None, 80_000) else { panic!() };
    assert_eq!(request.service, Service::Twitch, "a new token first");
}

#[test]
fn a_session_carries_the_pace_from_one_job_to_the_next() {
    let wants = Wants { cover: true, hero: false, logo: false, facts: false };
    let mut first = Job::new(Game { name: "A".into(), steam: Some(1) }, keys(true, false), wants, None, 5);
    assert!(matches!(first.next(None, 1_000), Step::Send(_)));
    let Step::Wait(_) = first.next(ok(r#"{"success":true,"data":{"id":4}}"#), 1_010) else { panic!("the next request waits its turn") };
}

#[test]
fn an_igdb_search_term_cannot_break_out_of_its_quotes() {
    let request = super::services::igdb_search("id", "tok", "Evil\"; fields *; \\");
    let body = request.body.unwrap();
    assert!(body.starts_with("search \"Evil; fields *; \";"), "{body}");
}

// --- The JSON calls ----------------------------------------------------------------------------

#[test]
fn a_job_runs_over_the_json_calls() {
    let start =
        serde_json::json!({"op": "start", "game": {"name": "Hades", "steam": null}, "keys": {"steamgriddb": "k-123456"}, "wants": {"cover": true}, "session": null, "wall_secs": 9, "now_ms": 1000});
    let answer: serde_json::Value = serde_json::from_str(&super::json::call(&start.to_string())).unwrap();
    let send = &answer["ok"]["step"]["send"];
    assert_eq!(send["service"], "steam_grid_db");
    assert_eq!(send["url"], "https://www.steamgriddb.com/api/v2/search/autocomplete/Hades");
    assert_eq!(send["headers"][0], serde_json::json!(["Authorization", "Bearer k-123456"]));
    let next = serde_json::json!({"op": "next", "job": answer["ok"]["job"], "reply": {"status": 404}, "now_ms": 2000});
    let answer: serde_json::Value = serde_json::from_str(&super::json::call(&next.to_string())).unwrap();
    let done = &answer["ok"]["step"]["done"];
    assert_eq!(done["record"]["art"], "not_found");
    assert_eq!(done["record"]["key"], "name:hades");
    assert_eq!(done["record"]["fetched_at"], 9);
}

/// Every number a job or record carries fits a Java long, as Android's `org.json` keeps it: a wider
/// one comes back as a double, which serde refuses. The key hashes are cut to 53 bits for that.
#[test]
fn a_job_crosses_androids_json_with_any_keys() {
    fn longs_only(value: &serde_json::Value) -> bool {
        match value {
            serde_json::Value::Number(n) => n.as_i64().is_some_and(|n| n.unsigned_abs() < (1 << 53)),
            serde_json::Value::Array(items) => items.iter().all(longs_only),
            serde_json::Value::Object(fields) => fields.values().all(longs_only),
            _ => true,
        }
    }
    for n in 0..500 {
        let keys = Keys { steamgriddb: format!("sgdb-{n}"), igdb_client_id: format!("id-{n}"), igdb_client_secret: format!("secret-{n}") };
        assert!(keys.art_fingerprint() < 1 << 53 && keys.info_fingerprint() < 1 << 53);
        let mut job = Job::new(Game { name: "Hades".into(), steam: Some(1145360) }, keys, ALL, None, 1_789_000_000);
        let _ = job.next(None, 10_000);
        let value = serde_json::to_value(&job).unwrap();
        assert!(longs_only(&value), "{n}: a number past a long's exact range");
        // Through text and back, as the app hands it over.
        let again: Job = serde_json::from_str(&value.to_string()).unwrap();
        assert_eq!(again, job);
    }
}

#[test]
fn a_failure_with_other_keys_is_asked_again_with_the_corrected_ones() {
    let wrong = keys(true, true);
    let job = Job::new(Game { name: "Hades".into(), steam: None }, wrong.clone(), ALL, None, 1_000);
    let (done, _) = run(job, |_| status(401));
    let record = done.record;
    assert_eq!((record.art, record.info), (Outcome::Failed, Outcome::Failed));
    assert!(record.fresh_with(1_001, BOTH, &wrong), "the same keys wait their hour");
    let mut corrected = wrong.clone();
    corrected.igdb_client_secret = "right-secret".into();
    assert!(!record.fresh_with(1_001, BOTH, &corrected), "a corrected secret asks now");
    // A part not wanted is not asked for its keys.
    assert!(record.fresh_with(1_001, Available { art: true, info: false }, &corrected));
    // A record found with the old keys holds with new ones.
    let found = Record { art: Outcome::Found, info: Outcome::Found, ..record };
    assert!(found.fresh_with(1_001, BOTH, &corrected));
}

#[test]
fn a_steam_store_entry_names_the_app_id_too() {
    assert_eq!(steam_of(None, Some(("steam", "20"))), Some(20));
    assert_eq!(steam_of(Some("steam:10"), Some(("steam", "20"))), Some(10), "the game id first");
    assert_eq!(steam_of(Some("epic:x"), Some(("epic", "x"))), None);
    assert_eq!(steam_of(None, Some(("steam", "0"))), None);
}

#[test]
fn short_lived_twitch_tokens_do_not_spin_forever() {
    for expires_in in [0u64, 30, 60] {
        let job = Job::new(
            Game { name: "Hades".into(), steam: Some(1145360) },
            keys(false, true),
            Wants { cover: false, hero: false, logo: false, facts: true },
            None,
            1_000,
        );
        let mut steps = 0usize;
        let (done, _) = run(job, |req| {
            steps += 1;
            assert!(steps < 20, "infinite OAuth loop for expires_in={expires_in}");
            match req.service {
                Service::Twitch => Some(Reply {
                    status: 200,
                    body: format!(r#"{{"access_token":"tok","expires_in":{expires_in}}}"#),
                    retry_after: None,
                }),
                Service::Igdb => Some(Reply { status: 401, body: String::new(), retry_after: None }),
                _ => status(404),
            }
        });
        assert_eq!(done.record.info, Outcome::Failed);
    }
}
