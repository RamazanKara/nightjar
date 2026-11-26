//! The relay driver's rules without sockets: an unreachable PC keeps its enrollment, one that
//! refuses the relay for now is not served until a refresh succeeds, one that refuses it for
//! good loses it and stays refused across restarts, and secrets stay out of every note.
use super::*;
use crate::http::native_api::error::ApiError;
use rubylight_traverse::wake_relay::{device_id, hex};

const PC: &str = "PC-UNIQUE";
const T0: u64 = 1_800_000_000;

fn assignment() -> RelayAssignment {
    RelayAssignment {
        target_id: PC.into(),
        target_name: "Desktop".into(),
        epoch: 1,
        secret: hex(&[7; 32]),
        macs: vec!["10:7B:44:00:00:01".into()],
        broadcasts: vec!["192.168.4.255".into()],
        address: Some("192.168.4.20".into()),
        http_port: 47989,
        devices: vec![hex(&device_id("phone")), hex(&device_id("tablet"))],
        refresh_seconds: 300,
    }
}

fn assigned(assignment: RelayAssignment) -> Outcome {
    Outcome::Assigned(Box::new(assignment))
}

fn paired() -> Vec<(String, String)> {
    vec![(PC.into(), "Desktop".into())]
}

/// A driver enrolled with the PC at `T0`.
fn enrolled() -> Driver {
    let mut driver = Driver::new(Saved::default(), false);
    driver.set_hosts(&paired());
    assert_eq!(driver.due(T0), [(PC.to_owned(), Ask::Enroll)]);
    let note = driver.on_result(PC, assigned(assignment()), T0);
    assert_eq!(note, Some(Note::Enrolled { devices: 2 }));
    assert!(driver.take_changed());
    driver
}

fn api(status: u16, code: ErrorCode) -> Error {
    Error::Api {
        status,
        error: Box::new(ApiError::new(
            code,
            "That wake relay is not enrolled on this PC.",
        )),
    }
}

/// Refusals for good: the relay removed on the PC, the pairing ended, Host control taken away.
fn refusals() -> Vec<Error> {
    vec![
        api(404, ErrorCode::NotFound),
        api(404, ErrorCode::Unknown),
        api(403, ErrorCode::HostControlRequired),
        api(403, ErrorCode::PermissionRequired),
        api(401, ErrorCode::NotPaired),
        api(401, ErrorCode::Unknown),
        Error::Status {
            code: 401,
            message: "Unauthorized".into(),
        },
        Error::NotFound("/api/native/v1/wake/relay/enroll".into()),
        Error::NotPaired,
    ]
}

/// Refusals for now (`forbidden`): the PC's setting off, or the relay not on its network.
fn refusals_for_now() -> Vec<Error> {
    vec![
        api(403, ErrorCode::Forbidden),
        api(403, ErrorCode::Unknown),
        Error::Status {
            code: 403,
            message: "Forbidden".into(),
        },
    ]
}

fn unreachable() -> Vec<Error> {
    vec![
        Error::Io(std::io::ErrorKind::TimedOut.into()),
        Error::Io(std::io::ErrorKind::ConnectionRefused.into()),
        Error::Io(std::io::ErrorKind::HostUnreachable.into()),
        Error::Tls("handshake".into()),
        Error::CertificateMismatch,
        Error::Cancelled,
    ]
}

#[test]
fn an_unreachable_pc_keeps_its_enrollment() {
    for error in unreachable() {
        let mut driver = enrolled();
        let outcome = of_error(&error);
        assert!(matches!(outcome, Outcome::Unreachable(_)), "{error:?}");
        let note = driver.on_result(PC, outcome.clone(), T0 + 300);
        assert!(matches!(note, Some(Note::Unreachable(_))));
        assert_eq!(driver.enrollments().len(), 1);
        assert_eq!(driver.saved().hosts[0].assignment, Some(assignment()));
        assert!(!driver.take_changed());
        // Said once while it sleeps, asked again on the PC's interval, never sooner.
        assert!(driver.due(T0 + 599).is_empty());
        assert_eq!(driver.due(T0 + 600), [(PC.to_owned(), Ask::Refresh)]);
        assert_eq!(driver.on_result(PC, outcome, T0 + 600), None);
        assert_eq!(
            driver.on_result(PC, assigned(assignment()), T0 + 900),
            Some(Note::Back)
        );
    }
}

#[test]
fn a_refusing_pc_drops_its_enrollment_at_once_and_stays_refused() {
    for error in refusals() {
        let mut driver = enrolled();
        let outcome = of_error(&error);
        assert!(matches!(outcome, Outcome::Refused(_)), "{error:?}");
        let note = driver.on_result(PC, outcome, T0 + 300);
        assert!(matches!(note, Some(Note::Refused(_))));
        assert!(driver.enrollments().is_empty(), "{error:?}");
        assert!(driver.take_changed());
        let saved = driver.saved();
        assert_eq!(saved.hosts.len(), 1);
        assert_eq!(saved.hosts[0].assignment, None);
        assert!(saved.hosts[0].refused.is_some());
        assert!(driver.due(T0 + 10_000).is_empty());

        // Started again: the refusal holds, and nothing is asked.
        let mut again = Driver::new(saved.clone(), false);
        again.set_hosts(&paired());
        assert!(again.enrollments().is_empty());
        assert!(again.due(T0 + 20_000).is_empty());
        assert_eq!(again.hosts()[0].refused, saved.hosts[0].refused);

        // Only `--enroll` enrolls again.
        let mut explicit = Driver::new(saved, true);
        assert!(explicit.take_changed());
        explicit.set_hosts(&paired());
        assert_eq!(explicit.due(T0), [(PC.to_owned(), Ask::Enroll)]);
    }
}

#[test]
fn a_refusal_for_good_before_enrolling_is_saved_too_and_one_for_now_is_not() {
    let mut driver = Driver::new(Saved::default(), false);
    driver.set_hosts(&paired());
    let note = driver.on_result(PC, of_error(&api(404, ErrorCode::NotFound)), T0);
    assert!(matches!(note, Some(Note::Refused(_))));
    assert!(driver.take_changed());
    assert!(driver.saved().hosts[0].refused.is_some());
    assert!(driver.due(T0 + 10_000).is_empty());

    for error in refusals_for_now() {
        let mut driver = Driver::new(Saved::default(), false);
        driver.set_hosts(&paired());
        let note = driver.on_result(PC, of_error(&error), T0);
        assert!(matches!(note, Some(Note::NotOffered(_))), "{error:?}");
        assert!(!driver.take_changed());
        assert!(driver.saved().hosts.is_empty());
        assert_eq!(
            driver.due(T0 + DEFAULT_REFRESH_SECONDS),
            [(PC.to_owned(), Ask::Enroll)]
        );
    }
}

#[test]
fn a_pc_that_refuses_the_relay_for_now_is_not_served_but_refreshed_until_it_takes_it_again() {
    for error in refusals_for_now() {
        let mut driver = enrolled();
        let outcome = of_error(&error);
        assert!(matches!(outcome, Outcome::NotOffered(_)), "{error:?}");
        let note = driver.on_result(PC, outcome.clone(), T0 + 300);
        assert!(matches!(note, Some(Note::NotOffered(_))), "{error:?}");
        // Not served at once, while the assignment is kept and saved with the PC's words.
        assert!(driver.enrollments().is_empty(), "{error:?}");
        assert!(driver.take_changed());
        let saved = driver.saved();
        assert_eq!(saved.hosts[0].assignment, Some(assignment()));
        assert!(saved.hosts[0].refused.is_some());
        assert_eq!(driver.hosts()[0].devices, None);
        assert!(driver.hosts()[0].refused.is_none());
        assert!(driver.hosts()[0].off.is_some());
        // Refreshed on the PC's interval, never enrolled again, and said once.
        assert!(driver.due(T0 + 599).is_empty());
        assert_eq!(driver.due(T0 + 600), [(PC.to_owned(), Ask::Refresh)]);
        assert_eq!(driver.on_result(PC, outcome, T0 + 600), None);
        assert!(!driver.take_changed());
        assert!(driver.enrollments().is_empty());
        assert_eq!(driver.due(T0 + 900), [(PC.to_owned(), Ask::Refresh)]);
        // The PC takes it again: served again without `--enroll`.
        assert_eq!(
            driver.on_result(PC, assigned(assignment()), T0 + 900),
            Some(Note::Enrolled { devices: 2 })
        );
        assert_eq!(driver.enrollments().len(), 1);
        assert!(driver.take_changed());
        assert_eq!(driver.saved().hosts[0].refused, None);
        assert_eq!(driver.hosts()[0].devices, Some(2));
        assert_eq!(driver.due(T0 + 1200), [(PC.to_owned(), Ask::Refresh)]);
    }
}

#[test]
fn a_relay_restarted_while_the_pc_refuses_it_for_now_serves_nothing_until_a_refresh_succeeds() {
    let mut driver = enrolled();
    driver.on_result(PC, of_error(&api(403, ErrorCode::Forbidden)), T0 + 300);
    let saved = driver.saved();

    let mut again = Driver::new(saved.clone(), false);
    assert!(!again.take_changed());
    assert!(again.enrollments().is_empty());
    again.set_hosts(&paired());
    assert!(again.enrollments().is_empty());
    assert!(again.hosts()[0].off.is_some());
    assert_eq!(again.due(T0 + 400), [(PC.to_owned(), Ask::Refresh)]);
    // Asleep: still kept, still not served.
    again.on_result(PC, of_error(&unreachable().remove(0)), T0 + 400);
    assert!(again.enrollments().is_empty());
    assert_eq!(again.saved(), saved);
    // Still refused: nothing new to say or save.
    assert_eq!(
        again.on_result(PC, of_error(&api(403, ErrorCode::Forbidden)), T0 + 700),
        None
    );
    assert!(!again.take_changed());
    again.on_result(PC, assigned(assignment()), T0 + 1000);
    assert_eq!(again.enrollments().len(), 1);
    assert!(again.take_changed());

    // `--enroll` enrolls, and serves only once that succeeds.
    let mut explicit = Driver::new(saved.clone(), true);
    explicit.set_hosts(&paired());
    assert!(explicit.enrollments().is_empty());
    assert_eq!(explicit.due(T0), [(PC.to_owned(), Ask::Enroll)]);
    explicit.on_result(PC, assigned(assignment()), T0);
    assert_eq!(explicit.enrollments().len(), 1);

    // Refused for good later: dropped, as from a served enrollment.
    let mut revoked = Driver::new(saved.clone(), false);
    revoked.set_hosts(&paired());
    revoked.on_result(PC, of_error(&api(404, ErrorCode::NotFound)), T0 + 400);
    assert!(revoked.enrollments().is_empty());
    assert_eq!(revoked.saved().hosts[0].assignment, None);
    assert!(revoked.due(T0 + 10_000).is_empty());

    // No longer paired here: forgotten, and nothing is kept.
    let mut unpaired = Driver::new(saved, false);
    let notes = unpaired.set_hosts(&[]);
    assert_eq!(notes, [("Desktop".to_owned(), Note::Forgotten)]);
    assert!(unpaired.saved().hosts.is_empty());
}

#[test]
fn other_failures_keep_the_enrollment() {
    let failures = [
        api(503, ErrorCode::TemporarilyUnavailable),
        api(500, ErrorCode::Internal),
        api(400, ErrorCode::InvalidRequest),
        Error::Status {
            code: 500,
            message: "Internal".into(),
        },
        Error::Malformed("native API wake/relay/enroll: EOF".into()),
    ];
    for error in failures {
        let mut driver = enrolled();
        let outcome = of_error(&error);
        assert!(matches!(outcome, Outcome::Failed(_)), "{error:?}");
        assert!(matches!(
            driver.on_result(PC, outcome.clone(), T0 + 300),
            Some(Note::Failed(_))
        ));
        assert_eq!(driver.on_result(PC, outcome, T0 + 600), None, "shown once");
        assert_eq!(driver.enrollments().len(), 1);
        assert!(!driver.take_changed());
    }
    let mut driver = enrolled();
    let empty = outcome(Ok(RelayEnrolled {
        assignment: None,
        relays: Vec::new(),
    }));
    assert!(matches!(empty, Outcome::Failed(_)));
    driver.on_result(PC, empty, T0 + 300);
    assert_eq!(driver.enrollments().len(), 1);
}

#[test]
fn a_relay_restarted_while_the_pc_sleeps_serves_its_saved_enrollment() {
    let saved = enrolled().saved();
    let mut driver = Driver::new(saved, false);
    assert!(!driver.take_changed());
    assert_eq!(driver.enrollments().len(), 1);
    driver.set_hosts(&paired());
    assert_eq!(driver.enrollments().len(), 1);
    assert_eq!(driver.due(T0), [(PC.to_owned(), Ask::Refresh)]);
    driver.on_result(PC, of_error(&unreachable().remove(0)), T0);
    assert_eq!(driver.enrollments()[0].target_id, PC);
}

#[test]
fn the_first_ask_enrolls_and_later_ones_refresh_on_the_pcs_interval() {
    let mut driver = enrolled();
    assert!(driver.due(T0 + 299).is_empty());
    assert_eq!(driver.due(T0 + 300), [(PC.to_owned(), Ask::Refresh)]);
    // The same answer changes nothing worth saving or saying.
    assert_eq!(driver.on_result(PC, assigned(assignment()), T0 + 300), None);
    assert!(!driver.take_changed());
    // A device unpaired on the PC leaves the relay with the next refresh.
    let mut fewer = assignment();
    fewer.devices.pop();
    let note = driver.on_result(PC, assigned(fewer), T0 + 600);
    assert_eq!(
        note,
        Some(Note::Changed {
            devices: 1,
            epoch: 1
        })
    );
    assert!(driver.take_changed());
    assert_eq!(driver.enrollments()[0].devices.len(), 1);
    // The PC's interval, kept between a minute and an hour.
    for (asked, next) in [(0, 60), (5, 60), (900, 900), (u32::MAX, 3600)] {
        let mut interval = assignment();
        interval.refresh_seconds = asked;
        driver.on_result(PC, assigned(interval), T0);
        assert!(driver.due(T0 + next - 1).is_empty(), "{asked}");
        assert_eq!(driver.due(T0 + next).len(), 1, "{asked}");
    }
}

#[test]
fn enroll_again_enrolls_with_every_pc_once() {
    let mut driver = Driver::new(enrolled().saved(), true);
    driver.set_hosts(&paired());
    assert_eq!(driver.due(T0), [(PC.to_owned(), Ask::Enroll)]);
    driver.on_result(PC, assigned(assignment()), T0);
    assert_eq!(driver.due(T0 + 300), [(PC.to_owned(), Ask::Refresh)]);
}

#[test]
fn an_assignment_for_another_pc_or_a_malformed_one_is_not_taken() {
    let mut other = assignment();
    other.target_id = "ANOTHER-PC".into();
    let mut malformed = assignment();
    malformed.secret = "abcd".into();
    for wrong in [other, malformed] {
        let mut driver = enrolled();
        let note = driver.on_result(PC, assigned(wrong.clone()), T0 + 300);
        assert!(matches!(note, Some(Note::Failed(_))));
        assert_eq!(driver.saved().hosts[0].assignment, Some(assignment()));
        assert!(!driver.take_changed());
        let mut fresh = Driver::new(Saved::default(), false);
        fresh.set_hosts(&paired());
        fresh.on_result(PC, assigned(wrong), T0);
        assert!(fresh.enrollments().is_empty());
        assert!(fresh.saved().hosts.is_empty());
    }
    // A saved one that does not parse is dropped on start.
    let mut saved = enrolled().saved();
    saved.hosts[0].assignment.as_mut().unwrap().secret = "zz".into();
    let mut driver = Driver::new(saved, false);
    assert!(driver.take_changed());
    assert!(driver.enrollments().is_empty());
    assert!(driver.saved().hosts.is_empty());
}

#[test]
fn a_pc_no_longer_paired_here_loses_its_enrollment_and_keeps_its_refusal() {
    let mut driver = enrolled();
    driver.set_hosts(&[
        (PC.into(), "Desktop".into()),
        ("LAPTOP".into(), "Laptop".into()),
    ]);
    driver.on_result("LAPTOP", of_error(&api(404, ErrorCode::NotFound)), T0);
    driver.take_changed();
    let notes = driver.set_hosts(&[]);
    assert_eq!(notes, [("Desktop".to_owned(), Note::Forgotten)]);
    assert!(driver.enrollments().is_empty());
    assert!(driver.take_changed());
    let saved = driver.saved();
    assert_eq!(saved.hosts.len(), 1);
    assert_eq!(saved.hosts[0].host, "LAPTOP");
    // Paired again, the refused PC is still not asked.
    driver.set_hosts(&[("LAPTOP".into(), "Laptop".into())]);
    assert!(driver.due(T0 + 10_000).is_empty());
}

#[test]
fn a_new_name_is_saved() {
    let mut driver = enrolled();
    driver.set_hosts(&[(PC.into(), "Gaming PC".into())]);
    assert!(driver.take_changed());
    assert_eq!(driver.saved().hosts[0].name, "Gaming PC");
}

fn capabilities(features: &str, host_control: bool) -> Capabilities {
    serde_json::from_str(&format!(
        r#"{{"api_version":1,"host":{{"name":"PC","version":"7.0.0","unique_id":"{PC}"}},"features":[{features}],"permissions":{{"list":true,"view":true,"launch":true,"host_control":{host_control}}}}}"#
    ))
    .unwrap()
}

#[test]
fn a_relay_enrolls_only_where_the_pc_offers_it_with_host_control() {
    assert_eq!(offered(Ok(capabilities(r#""wake.relay""#, true))), Ok(()));
    let off = offered(Ok(capabilities(r#""power.restart""#, true)));
    assert!(matches!(off, Err(Outcome::NotOffered(_))));
    let no_control = offered(Ok(capabilities(r#""wake.relay""#, false)));
    assert!(matches!(no_control, Err(Outcome::NotOffered(_))));
    let no_api = offered(Err(Error::NotFound("/api/native/v1/capabilities".into())));
    assert!(matches!(no_api, Err(Outcome::NotOffered(_))));
    let asleep = offered(Err(Error::Io(std::io::ErrorKind::TimedOut.into())));
    assert!(matches!(asleep, Err(Outcome::Unreachable(_))));

    // Not offered before enrolling: nothing saved, asked again later.
    let mut driver = Driver::new(Saved::default(), false);
    driver.set_hosts(&paired());
    let note = driver.on_result(PC, off.clone().unwrap_err(), T0);
    assert!(matches!(note, Some(Note::NotOffered(_))));
    assert!(driver.saved().hosts.is_empty());
    assert_eq!(
        driver.due(T0 + DEFAULT_REFRESH_SECONDS),
        [(PC.to_owned(), Ask::Enroll)]
    );
    // Not offered any more to an enrolled relay: not served, its assignment kept.
    let mut driver = enrolled();
    driver.on_result(PC, off.unwrap_err(), T0 + 300);
    assert!(driver.enrollments().is_empty());
    assert_eq!(driver.saved().hosts[0].assignment, Some(assignment()));
    assert!(driver.hosts()[0].refused.is_none());
    assert_eq!(driver.due(T0 + 600), [(PC.to_owned(), Ask::Refresh)]);
}

#[test]
fn the_request_names_the_relay_only_when_enrolling() {
    assert_eq!(
        Ask::Enroll.request(Some("NUC")),
        RelayEnroll::Enroll {
            name: Some("NUC".into())
        }
    );
    assert_eq!(
        Ask::Enroll.request(None),
        RelayEnroll::Enroll { name: None }
    );
    assert_eq!(Ask::Refresh.request(Some("NUC")), RelayEnroll::Refresh);
}

#[test]
fn no_secret_or_device_id_reaches_a_note_or_debug_output() {
    let secret = hex(&[7; 32]);
    let device = hex(&device_id("phone"));
    let mut driver = Driver::new(Saved::default(), false);
    driver.set_hosts(&paired());
    let outcome = assigned(assignment());
    let text = format!(
        "{outcome:?} {:?} {:?} {:?}",
        driver.on_result(PC, outcome.clone(), T0),
        driver.hosts(),
        driver.saved()
    );
    assert!(!text.contains(&secret), "{text}");
    assert!(!text.contains(&device), "{text}");
}

#[test]
fn a_pcs_words_come_on_one_short_line() {
    let long = format!("line one\nline two\u{1b}[31m{}", "x".repeat(500));
    let outcome = of_error(&Error::Api {
        status: 403,
        error: Box::new(ApiError::new(ErrorCode::Forbidden, long)),
    });
    let why = match outcome {
        Outcome::NotOffered(why) => why,
        other => panic!("{other:?}"),
    };
    assert!(!why.chars().any(char::is_control), "{why}");
    assert!(why.chars().count() <= MAX_REASON_CHARS + 1);
    assert!(why.starts_with("line one line two"));
}
