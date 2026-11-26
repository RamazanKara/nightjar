//! Unit tests for classic app lists, unpairing, pairing link claims, rechecks and PC actions.

use super::covers::art_store;
use super::*;

#[test]
fn the_hosts_placeholder_box_art_shows_the_title() {
    let placeholder = include_bytes!("../../../../client/tests/fixtures/box-placeholder.png").to_vec();
    assert!(decode_image(&placeholder).is_some(), "the placeholder is a real PNG");
    assert!(art_image(Some(placeholder)).is_none(), "and no cover: the card shows the title");
    let mut cover = Vec::new();
    image::RgbaImage::new(30, 40).write_to(&mut std::io::Cursor::new(&mut cover), image::ImageFormat::Png).unwrap();
    assert!(art_image(Some(cover)).is_some());
    assert!(art_image(None).is_none());
}

#[test]
fn a_pinned_address_wins_until_the_pin_is_removed() {
    let record = HostRecord { other_addresses: vec!["10.0.0.5".into(), "fd00::5".into()], ..paired_record("uid") };
    let mut hosts = Hosts::without_discovery(vec![record], Arc::new(|| {}));
    let id = hosts.list[0].id;
    assert_eq!(poll_addresses(&hosts.list[0].record), ["192.0.2.1", "10.0.0.5", "fd00::5"], "automatic: last that answered first");
    assert!(!hosts.pin_address(id, Some("198.51.100.9")), "only a known address is pinned");
    assert!(hosts.pin_address(id, Some("fd00::5")));
    let pinned = &hosts.list[0].record;
    assert_eq!((pinned.address.as_str(), pinned.pinned_address.as_deref()), ("fd00::5", Some("fd00::5")), "streams go to it");
    assert_eq!(poll_addresses(pinned), ["fd00::5"], "the only address polled");
    // An answer from the old address, still in flight when the user pinned, keeps the pin.
    let seq = hosts.get(id).unwrap().info_seq + 1;
    send(&mut hosts, Event::Info { path: InfoPath::Pinned, id, seq, address: "192.0.2.1".into(), result: Ok(Box::new(native_info())) });
    assert_eq!(hosts.list[0].record.address, "fd00::5");
    assert!(hosts.pin_address(id, None));
    let automatic = &hosts.list[0].record;
    assert_eq!(automatic.pinned_address, None);
    assert_eq!(poll_addresses(automatic), ["fd00::5", "10.0.0.5", "192.0.2.1"], "back to the automatic pick");
}

fn classic_app(id: i32, name: &str, uuid: &str, control: AppControl) -> App {
    App { name: name.into(), id, hdr_supported: false, uuid: uuid.into(), host_index: None, art_version: format!("v{id}"), control, monitor_resume: false }
}

#[test]
fn a_classic_pcs_app_list_becomes_its_home_library_and_is_saved() {
    let apps = vec![
        classic_app(1, "Desktop", "D-1", AppControl::None),
        classic_app(2, "Hades", "", AppControl::None),
        classic_app(3, "Terminate", "E16CBE1B-295D-4632-9A76-EC4180C857D3", AppControl::Terminate),
    ];
    let mut hosts = Hosts::without_discovery(vec![paired_record("uid")], Arc::new(|| {}));
    let id = hosts.list[0].id;
    info_lands(&mut hosts, id, stock_info());
    assert!(hosts.list[0].library().is_none(), "nothing before the list is read");
    send(&mut hosts, Event::Apps { id, result: Ok(apps.clone()) });
    let pc = &hosts.list[0];
    let games: Vec<_> = pc.library().unwrap().games.iter().map(|g| (g.app_id, g.title.as_str(), g.running)).collect();
    assert_eq!(games, [(1, "Desktop", false), (2, "Hades", false)], "the games, not the control tiles");
    assert_eq!(pc.cover_version(2), "v2", "covers at the list's box art version");
    let view = pc.ui_view();
    let source = crate::console::Source::of(&view);
    assert!(source.classic && source.library.is_some());
    let saves = hosts.take_classic_saves();
    assert_eq!(saves.len(), 1);
    assert_eq!((saves[0].0.as_str(), saves[0].1.as_ref().map(|a| a.apps.len())), ("uid", Some(2)));
    send(&mut hosts, Event::Apps { id, result: Ok(apps.clone()) });
    assert!(hosts.take_classic_saves().is_empty(), "the same list is not saved again");

    // currentgame marks the running app while the PC answers; away, nothing runs.
    let mut playing = stock_info();
    playing.current_game = 2;
    info_lands(&mut hosts, id, playing);
    assert!(hosts.list[0].library().unwrap().games[1].running);
    let seq = hosts.list[0].info_seq + 1;
    send(&mut hosts, Event::Info { path: InfoPath::Pinned, id, seq, address: "192.0.2.1".into(), result: Err("timed out".into()) });
    assert!(hosts.list[0].library().unwrap().games.iter().all(|g| !g.running));
    assert!(hosts.list[0].library().is_some(), "the list stays for Home while the PC is away");

    // Unpaired, the list goes with the pairing.
    send(&mut hosts, Event::Unpaired { id, result: Ok(()) });
    assert!(hosts.list[0].library().is_none() && hosts.list[0].classic_apps.is_none());

    // A PC with the native library keeps no app list for Home, and one kept is forgotten.
    let mut hosts = Hosts::without_discovery(vec![paired_record("uid")], Arc::new(|| {}));
    hosts.list[0].classic_apps = Some(Arc::new(crate::config::ClassicApps::of(&apps)));
    let id = hosts.list[0].id;
    info_lands(&mut hosts, id, native_info());
    send(&mut hosts, Event::Apps { id, result: Ok(apps) });
    assert!(hosts.list[0].classic.is_none() && !crate::console::Source::of(&hosts.list[0].ui_view()).classic);
    let saves = hosts.take_classic_saves();
    assert_eq!((saves.len(), saves[0].1.is_none()), (1, true), "the saved list is forgotten");
}

#[test]
fn a_saved_app_list_shows_while_the_pc_is_away_unless_a_native_library_is_saved() {
    let (store, dir) = art_store("classic-saved");
    let apps = crate::config::ClassicApps::of(&[classic_app(4, "Celeste", "C-4", AppControl::None)]);
    store.save_classic_apps("uid", &apps).unwrap();
    store.save_classic_apps("native", &apps).unwrap();
    store.save_library("native", &library().into()).unwrap();
    let mut unpaired = paired_record("gone");
    unpaired.server_certificate = None;
    store.save_classic_apps("gone", &apps).unwrap();
    let mut hosts = Hosts::without_discovery(vec![paired_record("uid"), paired_record("native"), unpaired], Arc::new(|| {}));
    hosts.load_cached_libraries(&store);
    let titles = |pc: &Host| pc.library().map(|l| l.games.iter().map(|g| g.title.clone()).collect::<Vec<_>>());
    assert_eq!(titles(&hosts.list[0]), Some(vec!["Celeste".to_owned()]));
    assert!(crate::console::Source::of(&hosts.list[0].ui_view()).classic);
    assert_ne!(titles(&hosts.list[1]), Some(vec!["Celeste".to_owned()]), "the native library wins");
    assert!(!crate::console::Source::of(&hosts.list[1].ui_view()).classic);
    assert!(hosts.list[2].library().is_none(), "nothing for a PC not paired");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_native_read_that_lands_after_unpairing_is_dropped() {
    let mut hosts = Hosts::without_discovery(vec![paired_record("uid")], Arc::new(|| {}));
    let id = hosts.list[0].id;
    // The read started while the PC was paired (epoch 0).
    send(&mut hosts, Event::Unpaired { id, result: Ok(()) });
    send(&mut hosts, Event::Native { id, epoch: 0, seq: 1, result: Ok(full_read(session("suspended"))) });
    assert!(hosts.list[0].native.library.is_none(), "the library of an unpaired PC is not taken in");
    assert!(hosts.take_library_saves().is_empty(), "and nothing is saved for it");

    // Even when it was queued for saving before the unpair, an unpaired PC is not saved.
    hosts.list[0].record.server_certificate = Some("pem".into());
    let epoch = hosts.list[0].native_epoch;
    send(&mut hosts, Event::Native { id, epoch, seq: 2, result: Ok(full_read(session("suspended"))) });
    assert!(hosts.list[0].native.library.is_some());
    hosts.list[0].record.server_certificate = None;
    assert!(hosts.take_library_saves().is_empty());
}

#[test]
fn the_certificate_stays_until_the_pc_has_unpaired_the_device() {
    let mut hosts = Hosts::without_discovery(vec![paired_record("uid")], Arc::new(|| {}));
    let id = hosts.list[0].id;
    // No identity: the request cannot be made, and the user is told.
    assert!(hosts.unpair(id).is_err());
    assert!(hosts.list[0].record.server_certificate.is_some());
    // The request failed or never got through.
    send(&mut hosts, Event::Unpaired { id, result: Err(UNPAIR_FAILED.into()) });
    assert!(hosts.list[0].record.server_certificate.is_some(), "a failed unpair keeps the pairing");
    // The PC no longer lists this device, or never did.
    send(&mut hosts, Event::Unpaired { id, result: Err(UNPAIR_NOT_PAIRED.into()) });
    assert!(hosts.list[0].record.server_certificate.is_none());
    hosts.list[0].record.server_certificate = Some("pem".into());
    send(&mut hosts, Event::Unpaired { id, result: Ok(()) });
    assert!(hosts.list[0].record.server_certificate.is_none());
}

#[test]
fn a_session_read_from_before_an_action_does_not_overwrite_its_answer() {
    let mut hosts = Hosts::without_discovery(vec![paired_record("uid")], Arc::new(|| {}));
    let id = hosts.list[0].id;
    // A full read starts (seq 1), then Close game lands and says the session is gone.
    hosts.list[0].native_seq = 1;
    send(&mut hosts, Event::Session { id, action: ActionId::End, result: Ok(Box::new(session("idle"))) });
    assert_eq!(hosts.list[0].native.session.as_ref().map(|s| s.state), Some(session("idle").state));
    // The older read lands afterwards: its library counts, its session does not.
    send(&mut hosts, Event::Native { id, epoch: 0, seq: 1, result: Ok(full_read(session("suspended"))) });
    assert!(hosts.list[0].native.library.is_some());
    assert_eq!(hosts.list[0].native.session.as_ref().map(|s| s.state), Some(session("idle").state));
    // A read that started after the action is newer and replaces it.
    let seq = hosts.list[0].native_seq + 1;
    send(&mut hosts, Event::Native { id, epoch: 0, seq, result: Ok(full_read(session("suspended"))) });
    assert_eq!(hosts.list[0].native.session.as_ref().map(|s| s.state), Some(session("suspended").state));
}

pub(super) const INFO: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?><root status_code=\"200\">        <hostname>Renamed on the PC</hostname><uniqueid>uid</uniqueid><HttpsPort>47984</HttpsPort>        <ExternalPort>47989</ExternalPort><PairStatus>1</PairStatus><currentgame>0</currentgame>        <state>SUNSHINE_SERVER_FREE</state></root>";

pub(super) const PEM: &str = "-----BEGIN CERTIFICATE-----\nAQID\n-----END CERTIFICATE-----\n";

fn link_naming(cert_sha256: [u8; 32]) -> PairingUri {
    PairingUri {
        host: "203.0.113.9".into(),
        port: 47989,
        otp: "4821".into(),
        passphrase: "hunter2".into(),
        unique_id: String::new(),
        cert_sha256,
        name: "Claimed".into(),
    }
}

fn known_pc_with_link(cert_sha256: [u8; 32]) -> (Hosts, u64) {
    let mut known = paired_record("uid");
    known.server_certificate = Some(PEM.into());
    let mut hosts = Hosts::without_discovery(vec![known], Arc::new(|| {}));
    let id = hosts.add_manual("203.0.113.9", Some(47989));
    hosts.pair_link(id, link_naming(cert_sha256));
    (hosts, id)
}

#[test]
fn a_link_address_claiming_a_known_pc_cannot_move_it() {
    let (mut hosts, id) = known_pc_with_link([9; 32]);
    let info = ServerInfo::parse(INFO, 47989).unwrap();
    assert!(hosts.apply_info(id, "203.0.113.9".into(), InfoPath::Pinned, Ok(Box::new(info))));
    assert_eq!(hosts.list.len(), 1, "the record the link added is gone");
    let known = &hosts.list[0].record;
    assert_eq!(known.unique_id, "uid");
    assert_eq!(known.address, "192.0.2.1", "the known PC stays where it was");
    assert!(!known.other_addresses.contains(&"203.0.113.9".to_owned()));
    assert_eq!(known.name, "PC");
    assert_eq!(known.server_certificate.as_deref(), Some(PEM));
}

#[test]
fn the_known_pcs_own_link_may_still_find_it_at_a_new_address() {
    let (mut hosts, id) = known_pc_with_link(nightjar_core::http::certificate_sha256(PEM).unwrap());
    let info = ServerInfo::parse(INFO, 47989).unwrap();
    assert!(hosts.apply_info(id, "203.0.113.9".into(), InfoPath::Pinned, Ok(Box::new(info))));
    assert_eq!(hosts.list.len(), 1, "one PC, as before");
    assert_eq!(hosts.list[0].record.address, "203.0.113.9");
    assert_eq!(hosts.list[0].record.server_certificate.as_deref(), Some(PEM));
}

/// A small PNG of one colour, as a host's box art arrives.

#[test]
fn a_link_to_a_known_address_with_another_certificate_is_refused_at_once() {
    let mut known = paired_record("uid");
    known.server_certificate = Some(PEM.into());
    known.http_port = 47989;
    let mut hosts = Hosts::without_discovery(vec![known], Arc::new(|| {}));
    let id = hosts.list[0].id;
    hosts.pair_link(id, link_naming([9; 32]));
    assert!(matches!(
        hosts.events.try_recv(),
        Ok(Event::Paired { id: refused, result: Err(why), .. }) if refused == id && why == PAIR_LINK_MISMATCH
    ));
    assert!(hosts.link_claims.is_empty());
    assert_eq!(hosts.list[0].record.server_certificate.as_deref(), Some(PEM));
}

#[test]
fn a_pc_paired_nowhere_takes_any_link_and_a_typed_address_is_as_before() {
    assert!(!pins_another(&HostRecord::default(), &[9; 32]));
    let mut record = paired_record("uid");
    record.server_certificate = Some("not a certificate".into());
    assert!(pins_another(&record, &[9; 32]), "unreadable counts as another");
    let mut known = paired_record("uid");
    known.server_certificate = Some(PEM.into());
    let mut hosts = Hosts::without_discovery(vec![known], Arc::new(|| {}));
    let id = hosts.add_manual("203.0.113.9", Some(47989));
    let info = ServerInfo::parse(INFO, 47989).unwrap();
    hosts.apply_info(id, "203.0.113.9".into(), InfoPath::Pinned, Ok(Box::new(info)));
    assert_eq!(hosts.list[0].record.address, "203.0.113.9", "no link, no claim");
}

#[test]
fn a_name_chosen_here_survives_the_pc_reporting_another() {
    let mut record = paired_record("uid");
    record.set_alias("Living room");
    let mut hosts = Hosts::without_discovery(vec![record], Arc::new(|| {}));
    let id = hosts.list[0].id;
    let info = ServerInfo::parse(INFO, 47989).unwrap();
    assert!(hosts.apply_info(id, "192.0.2.1".into(), InfoPath::Pinned, Ok(Box::new(info))));
    let host = &hosts.list[0];
    assert_eq!(host.record.name, "Renamed on the PC", "the PC's own name keeps following the PC");
    assert_eq!(host.display_name(), "Living room");
    assert_eq!(hosts.records()[0].alias.as_deref(), Some("Living room"), "and the alias is saved");
    hosts.list[0].record.set_alias("");
    assert_eq!(hosts.list[0].display_name(), "Renamed on the PC");
}

#[test]
fn names_a_pc_reports_or_is_given_later_are_kept_out_of_the_log() {
    let mut hosts = Hosts::without_discovery(vec![paired_record("uid")], Arc::new(|| {}));
    let id = hosts.list[0].id;
    let info = INFO.replace("Renamed on the PC", "Secret name from serverinfo").replace("<uniqueid>uid", "<uniqueid>secret-uid-from-serverinfo");
    assert!(hosts.apply_info(id, "192.0.2.1".into(), InfoPath::Pinned, Ok(Box::new(ServerInfo::parse(&info, 47989).unwrap()))));
    assert!(hosts.rename(id, "Secret alias given here"));
    hosts.apply_discovery("Secret name from mDNS".into(), vec!["192.0.2.99".parse().unwrap()], 0);
    let line = nightjar_core::report::redact::scrub("Secret name from serverinfo, secret-uid-from-serverinfo, Secret alias given here, Secret name from mDNS");
    // Other tests save a PC named "PC": it does not mark the marks again.
    assert_eq!(line, "[PC], [PC], [PC], [PC]");
}

#[test]
fn a_pc_reads_as_away_before_its_punched_path_is_tried() {
    // The first failed poll of a PC shown online or not known yet does not wait for the punch.
    assert!(!punches_now(false, || false));
    // Once it reads as away, or while a punched path is up (it answers at once), the punch is tried.
    assert!(punches_now(true, || panic!("not asked")));
    assert!(punches_now(false, || true));
    // A failed poll marks the PC Offline, which the next poll punches from.
    let mut hosts = Hosts::without_discovery(vec![paired_record("uid")], Arc::new(|| {}));
    let id = hosts.list[0].id;
    let seq = hosts.list[0].info_seq + 1;
    send(&mut hosts, Event::Info { path: InfoPath::Pinned, id, seq, address: "192.0.2.1".into(), result: Err("timed out".into()) });
    assert!(hosts.list[0].reach == Reach::Offline);
}

#[test]
fn a_failed_quit_names_the_game_and_the_next_step_not_the_pcs_words() {
    let failed = quit_failure("HTTP 500: internal error", "Portal");
    assert_eq!(failed, "Couldn’t quit Portal. Check that the PC is on and on this network, then try again.");
    assert!(!failed.contains("HTTP 500"), "the PC's words go to the log");
    assert_eq!(quit_failure(QUIT_OTHER_DEVICE, "Portal"), QUIT_OTHER_DEVICE);
}

#[test]
fn pairing_texts_follow_the_android_wording() {
    let one_time = pair_outcome(PairResult::PinWrong, true).unwrap_err();
    let plain = pair_outcome(PairResult::PinWrong, false).unwrap_err();
    assert_eq!(one_time, PAIR_OTP_PIN_WRONG);
    assert_eq!(plain, PAIR_PIN_WRONG);
    assert_eq!(pair_outcome(PairResult::AlreadyInProgress, false).unwrap_err(), PAIR_IN_PROGRESS);
    // A PC that ended the pairing is told apart from one that failed its identity check.
    use nightjar_core::http::{PairFailure, PairStep};
    assert_eq!(pair_outcome(PairResult::Failed(PairFailure::Declined(PairStep::ServerCertificate)), false).unwrap_err(), PAIR_DECLINED);
    assert_eq!(pair_outcome(PairResult::Failed(PairFailure::ServerSignature), false).unwrap_err(), PAIR_UNVERIFIED);
    assert!(!PAIR_DECLINED.contains("old request") && !PAIR_UNVERIFIED.contains("old request"));
    let io =nightjar_core::http::Error::Io(std::io::ErrorKind::TimedOut.into());
    assert_eq!(pair_error(&io), PAIR_NETWORK);
    assert_eq!(unpair_error(&nightjar_core::http::Error::NotPaired), UNPAIR_NOT_PAIRED);
    assert_eq!(unpair_error(&io), UNPAIR_FAILED);

    let mut hosts = Hosts::without_discovery(vec![paired_record("uid")], Arc::new(|| {}));
    assert_eq!(hosts.list[0].pairing_blocked(), None);
    hosts.list[0].reach = Reach::Offline;
    assert_eq!(hosts.list[0].pairing_blocked(), Some(PAIR_OFFLINE));
    hosts.list[0].reach = Reach::Online;
    let busy = INFO.replace("<currentgame>0", "<currentgame>7").replace("SERVER_FREE", "SERVER_BUSY");
    hosts.list[0].info = Some(ServerInfo::parse(&busy, 47989).unwrap());
    assert_eq!(hosts.list[0].pairing_blocked(), Some(PAIR_IN_GAME));
}

#[test]
fn merging_two_records_of_one_pc_keeps_the_name_chosen_here() {
    let mut named = paired_record("uid");
    named.set_alias("Living room");
    named.address = "192.0.2.20".into();
    let mut plain = paired_record("uid");
    plain.server_certificate = None;
    plain.address = "192.0.2.21".into();
    for (first, second) in [(named.clone(), plain.clone()), (plain.clone(), named.clone())] {
        let mut hosts = Hosts::without_discovery(vec![first, second], Arc::new(|| {}));
        let id = hosts.list[0].id;
        let info = ServerInfo::parse(INFO, 47989).unwrap();
        assert!(hosts.apply_info(id, hosts.list[0].record.address.clone(), InfoPath::Pinned, Ok(Box::new(info))));
        assert_eq!(hosts.list.len(), 1, "the two records are one PC");
        assert_eq!(hosts.list[0].display_name(), "Living room");
    }
}

#[test]
fn pairing_completing_after_unique_id_merge_lands_on_surviving_pc() {
    let mut existing = HostRecord {
        name: "Existing".into(),
        address: "192.0.2.10".into(),
        http_port: 47989,
        unique_id: "uid".into(),
        ..Default::default()
    };
    existing.wake_macs.push("00:11:22:33:44:55".into());
    let mut extra = HostRecord {
        name: "Manual".into(),
        address: "192.0.2.11".into(),
        http_port: 47989,
        ..Default::default()
    };
    extra.manual = true;
    let mut hosts = Hosts::without_discovery(vec![existing, extra], Arc::new(|| {}));
    let kept_id = hosts.list[0].id;
    let pairing_id = hosts.list[1].id;
    hosts.pairing_generation = 7;

    let info = ServerInfo::parse(INFO, 47989).unwrap();
    assert!(hosts.apply_info(pairing_id, "192.0.2.11".into(), InfoPath::Pinned, Ok(Box::new(info))));
    assert_eq!(hosts.list.len(), 1);
    assert_eq!(hosts.canonical_id(pairing_id), kept_id);

    hosts.sender.send(Event::Paired { id: pairing_id, request: 7, result: Ok(PEM.into()) }).unwrap();
    let (events, changed) = hosts.drain(&egui::Context::default());
    assert!(changed);
    assert_eq!(hosts.list[0].record.server_certificate.as_deref(), Some(PEM));
    assert!(events.iter().any(|e| matches!(e, Event::Paired { id, request: 7, result: Ok(_) } if *id == kept_id)));
}

#[test]
fn distinct_paired_pcs_with_colliding_unique_id_do_not_merge_and_add_manual_matches_all_slots() {
    let mut first = paired_record("uid");
    first.address = "192.0.2.10".into();
    first.other_addresses.push("10.0.0.10".into());
    first.remote_address = Some("203.0.113.10".into());
    first.pinned_address = Some("198.51.100.10".into());
    first.server_certificate = Some("CERT-A".into());

    let mut second = paired_record("uid");
    second.address = "192.0.2.20".into();
    second.server_certificate = Some("CERT-B".into());

    let mut hosts = Hosts::without_discovery(vec![first, second], Arc::new(|| {}));
    assert_eq!(hosts.add_manual("10.0.0.10", Some(47989)), hosts.list[0].id);
    assert_eq!(hosts.add_manual("203.0.113.10", Some(47989)), hosts.list[0].id);
    assert_eq!(hosts.add_manual("198.51.100.10", Some(47989)), hosts.list[0].id);
    assert_eq!(hosts.list.len(), 2, "known extra address slots must not create duplicate records");

    let second_id = hosts.list[1].id;
    let info = ServerInfo::parse(INFO, 47989).unwrap();
    let _ = hosts.apply_info(second_id, "192.0.2.20".into(), InfoPath::Pinned, Ok(Box::new(info)));
    assert_eq!(hosts.list.len(), 2, "distinct certificates on same unique_id must stay separate (#176)");
    assert_eq!(hosts.list[0].record.server_certificate.as_deref(), Some("CERT-A"));
    assert_eq!(hosts.list[1].record.server_certificate.as_deref(), Some("CERT-B"));
}

fn info_event(id: u64, seq: u64, game: i32) -> Event {
    let text = INFO
        .replace("<currentgame>0", &format!("<currentgame>{game}"))
        .replace("SERVER_FREE", if game == 0 { "SERVER_FREE" } else { "SERVER_BUSY" });
    Event::Info { path: InfoPath::Pinned, id, seq, address: "192.0.2.1".into(), result: Ok(Box::new(ServerInfo::parse(&text, 47989).unwrap())) }
}

#[test]
fn an_older_serverinfo_answer_does_not_bring_back_a_finished_game() {
    let mut hosts = Hosts::without_discovery(vec![paired_record("uid")], Arc::new(|| {}));
    let id = hosts.list[0].id;
    send(&mut hosts, info_event(id, 1, 7));
    assert_eq!(hosts.list[0].running_app(), Some(7));
    // The poll after the quit answers first; the one from before it lands late.
    send(&mut hosts, info_event(id, 3, 0));
    send(&mut hosts, info_event(id, 2, 7));
    assert_eq!(hosts.list[0].running_app(), None, "the app list must not show the quit game as running");
}

#[test]
fn the_failure_reason_comes_only_from_the_poll_after_the_stream() {
    let mut hosts = Hosts::without_discovery(vec![paired_record("uid")], Arc::new(|| {}));
    let id = hosts.list[0].id;
    let failed = |seq| {
        let text = INFO.replace("<state>", "<RustHostLastFailure>HDR is on, but the display is not in HDR</RustHostLastFailure><state>");
        Event::Info { path: InfoPath::Pinned, id, seq, address: "192.0.2.1".into(), result: Ok(Box::new(ServerInfo::parse(&text, 47989).unwrap())) }
    };
    // An answer from before the stream says nothing about it.
    send(&mut hosts, failed(1));
    assert_eq!(hosts.failure_after(id, 2), None, "still waiting for the poll after the stream");
    send(&mut hosts, failed(2));
    assert_eq!(hosts.failure_after(id, 2), Some(Some("HDR is on, but the display is not in HDR".into())));
    // A PC that gives no reason, or could not be asked, ends the wait without one.
    send(&mut hosts, info_event(id, 3, 0));
    assert_eq!(hosts.failure_after(id, 3), Some(None));
    send(&mut hosts, Event::Info { path: InfoPath::Pinned, id, seq: 4, address: "192.0.2.1".into(), result: Err("timed out".into()) });
    assert_eq!(hosts.failure_after(id, 4), Some(None));
}

#[test]
fn a_quit_a_closed_game_and_a_stream_end_ask_the_pc_again_for_a_while() {
    let mut hosts = Hosts::without_discovery(vec![paired_record("uid")], Arc::new(|| {}));
    let id = hosts.list[0].id;
    let now = Instant::now();
    assert_eq!(hosts.list[0].poll_interval(now), POLL_INTERVAL);
    for event in [
        Event::Quit { id, result: Ok(()) },
        Event::Quit { id, result: Err("declined".into()) },
        Event::Session { id, action: ActionId::CloseGame, result: Ok(Box::new(session("idle"))) },
        Event::Action { id, result: Ok(()) },
    ] {
        hosts.list[0].recheck_until = None;
        send(&mut hosts, event);
        assert_eq!(hosts.list[0].poll_interval(Instant::now()), RECHECK_INTERVAL);
    }
    // The same entry point serves the end of a stream.
    hosts.list[0].recheck_until = None;
    hosts.refresh_after_exit(id);
    let until = hosts.list[0].recheck_until.expect("a stream's end arms the recheck");
    assert_eq!(hosts.list[0].poll_interval(until), POLL_INTERVAL, "and it ends");
}

/// While the window is minimized or hidden an idle PC is left alone, but the PC being woken and a
/// PC just quit or left are still asked: a wake would otherwise time out on a PC that is up.
#[test]
fn a_pc_being_woken_or_rechecked_is_polled_while_the_window_is_away() {
    let records = [("a", "192.0.2.1"), ("b", "192.0.2.2"), ("c", "192.0.2.3")]
        .map(|(uid, address)| HostRecord { address: address.into(), ..paired_record(uid) });
    let mut hosts = Hosts::without_discovery(records.to_vec(), Arc::new(|| {}));
    let ids: Vec<u64> = hosts.list.iter().map(|h| h.id).collect();
    let (waking, rechecked, idle) = (ids[0], ids[1], ids[2]);
    let now = Instant::now();
    hosts.get_mut(rechecked).unwrap().recheck_until = Some(now + RECHECK_WINDOW);
    let mut due = hosts.due(now, false, Some(waking));
    due.sort_unstable();
    let mut kept = vec![waking, rechecked];
    kept.sort_unstable();
    assert_eq!(due, kept, "the idle PC waits for the window");
    assert_eq!(hosts.due(now, false, None), vec![rechecked], "no wake, no waking PC");
    assert_eq!(hosts.due(now + RECHECK_WINDOW, false, None), Vec::<u64>::new(), "the recheck window ends");
    assert_eq!(hosts.due(now, true, None).len(), 3, "a shown window polls every PC");
    // Still only when due: a PC asked a moment ago waits for its interval.
    hosts.get_mut(waking).unwrap().last_poll = Some(now);
    assert!(!hosts.due(now, false, Some(waking)).contains(&waking));
    assert!(hosts.due(now + POLL_INTERVAL, false, Some(waking)).contains(&waking));
    assert!(hosts.due(now, true, None).contains(&idle) && !hosts.due(now, true, None).contains(&waking));
}

#[test]
fn the_library_is_read_again_every_few_seconds_while_rechecking_only() {
    let mut hosts = Hosts::without_discovery(vec![paired_record("uid")], Arc::new(|| {}));
    let now = Instant::now();
    let host = &mut hosts.list[0];
    host.native.full_at = Some(now);
    assert!(!host.library_recheck_due(now), "outside the window the 30 s timer rules");
    host.recheck_until = Some(now + RECHECK_WINDOW);
    assert!(!host.library_recheck_due(now + RECHECK_LIBRARY_INTERVAL / 2), "just read");
    assert!(host.library_recheck_due(now + RECHECK_LIBRARY_INTERVAL), "read again after a few seconds");
    host.native.full_at = None;
    assert!(host.library_recheck_due(now), "never read: read now");
    assert!(!host.library_recheck_due(now + RECHECK_WINDOW), "the window ends");
}

#[test]
fn pins_are_four_digits() {
    for _ in 0..20 {
        let pin = random_pin();
        assert_eq!(pin.len(), 4);
        assert!(pin.bytes().all(|b| b.is_ascii_digit()));
    }
}

#[test]
fn action_completion_requires_a_known_message_for_the_selected_action() {
    use nightjar_core::http::Error;
    let terminated = Error::Status { code: 410, message: "Application terminated.".into() };
    assert!(completed_action(AppControl::Terminate, &terminated));
    assert!(!completed_action(AppControl::DisconnectInput, &terminated));
    assert!(!completed_action(AppControl::Terminate, &Error::Status { code: 410, message: "Confirm termination".into() }));
    assert!(!completed_action(AppControl::Terminate, &Error::Status { code: 500, message: "Application terminated".into() }));
}
