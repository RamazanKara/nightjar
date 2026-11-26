//! The away wake's JSON in and out: the bundle the PC hands out, the relays tried in order and
//! the outcome each relay's answer gives. No network: the relays are fakes, or the real
//! requester on loopback without DHT routers.

use super::*;
use nightjar_core::http::native_api::wake_relay::WakeRelayInfo;
use rubylight_traverse::wake_relay::{device_id, hex};

const FIXTURE: &str = include_str!("../../../native-api/tests/fixtures/wake_relay.json");

fn loopback() -> SocketAddr {
    "127.0.0.1:0".parse().unwrap()
}

fn bundle(names: &[&str]) -> String {
    json!({
        "target_id": "PC-UNIQUE",
        "epoch": 3,
        "device": hex(&device_id("phone")),
        "relays": names.iter().enumerate().map(|(index, name)| json!({
            "id": format!("relay-{index}"),
            "name": name,
            "secret": hex(&[index as u8 + 1; 32]),
        })).collect::<Vec<_>>(),
    })
    .to_string()
}

fn relays_of(names: &[&str]) -> Vec<Relay> {
    relays(Some(&bundle(names)), loopback(), &[], &[]).unwrap()
}

fn status(status: Status) -> WakeStatus {
    let mut answer = WakeStatus::new(status, [0; 16]);
    if status == Status::RateLimited {
        answer.retry_after = 30;
    }
    answer
}

/// Runs [`wake`] with relays that answer `answers` in turn; gives the outcome and how many were
/// asked.
fn wake_with(answers: Vec<Result<WakeStatus, Failure>>) -> (Value, usize) {
    let names: Vec<String> = (0..answers.len()).map(|i| format!("Relay {i}")).collect();
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    let mut answers = answers.into_iter();
    let mut asked = 0;
    let outcome = wake(
        relays_of(&names),
        &AtomicBool::new(false),
        &mut |_| {},
        &mut |_, _, _| {
            asked += 1;
            answers.next().unwrap()
        },
    );
    (outcome, asked)
}

#[test]
fn the_bundle_gives_one_relay_per_usable_key_in_its_order() {
    let mut raw: Value = serde_json::from_str(&bundle(&["NUC", "Broken", "Server"])).unwrap();
    raw["relays"][1]["secret"] = json!("not hex");
    let relays = relays(Some(&raw.to_string()), loopback(), &[], &[]).unwrap();
    let names: Vec<&str> = relays.iter().map(|relay| relay.name.as_str()).collect();
    assert_eq!(names, ["NUC", "Server"]);
    assert_eq!(relays[0].config.target_id, "PC-UNIQUE");
    assert_eq!(relays[0].config.epoch, 3);
    assert_eq!(relays[0].config.device, device_id("phone"));
    assert_eq!(relays[1].config.secret, [3; 32]);
    assert_eq!(relays[0].config.bind, loopback());
}

#[test]
fn a_missing_empty_or_malformed_bundle_wakes_nothing() {
    let no_relays = |bundle: Option<&str>| relays(bundle, loopback(), &[], &[]).err();
    assert_eq!(no_relays(None), Some(Outcome::Invalid));
    assert_eq!(no_relays(Some("{}")), Some(Outcome::Invalid));
    assert_eq!(no_relays(Some(&bundle(&[]))), Some(Outcome::NoRelay));
    let mut broken: Value = serde_json::from_str(&bundle(&["NUC"])).unwrap();
    broken["device"] = json!("zz");
    assert_eq!(no_relays(Some(&broken.to_string())), Some(Outcome::Invalid));
    assert_eq!(
        wake(
            Vec::new(),
            &AtomicBool::new(false),
            &mut |_| {},
            &mut |_, _, _| { unreachable!("no relay to ask") }
        )["outcome"],
        "no_relay"
    );
}

#[test]
fn the_keys_op_hands_java_the_bundle_as_the_pc_sent_it() {
    let info: WakeRelayInfo = serde_json::from_str(FIXTURE).unwrap();
    let answer: Value = serde_json::from_str(&native_api::ok(Ok(info))).unwrap();
    let bundle = &answer["ok"]["bundle"];
    assert_eq!(bundle["relays"][0]["name"], "Living Room NUC");
    assert_eq!(bundle["epoch"], 1);
    let relays = relays(Some(&bundle.to_string()), loopback(), &[], &[]).unwrap();
    assert_eq!(relays.len(), 1);
    assert_eq!(
        relays[0].config.target_id,
        "4F1C2B7A-90DE-4C1B-8E2A-5D3F6A7B8C9D"
    );
}

#[test]
fn each_answer_of_a_relay_is_its_outcome() {
    let outcome = |answer| wake_with(vec![answer]).0;
    assert_eq!(outcome(Ok(status(Status::HostAwake)))["outcome"], "awake");
    assert_eq!(
        outcome(Ok(status(Status::TimedOut)))["outcome"],
        "timed_out"
    );
    let limited = outcome(Ok(status(Status::RateLimited)));
    assert_eq!(limited["outcome"], "rate_limited");
    assert_eq!(limited["retry_after"], 30);
    assert_eq!(
        outcome(Ok(status(Status::RejectedAuth)))["outcome"],
        "refused"
    );
    assert_eq!(
        outcome(Ok(status(Status::RejectedReplay)))["outcome"],
        "refused"
    );
    assert_eq!(
        outcome(Ok(status(Status::PacketSent)))["outcome"],
        "sent_no_answer"
    );
    assert_eq!(
        outcome(Ok(status(Status::WaitingForBoot)))["outcome"],
        "sent_no_answer"
    );
    assert_eq!(outcome(Err(Failure::Cancelled))["outcome"], "cancelled");
    let network = outcome(Err(Failure::Network("no route".into())));
    assert_eq!(network["outcome"], "network");
    assert_eq!(network["detail"], "no route");
    let awake = outcome(Ok(status(Status::HostAwake)));
    assert_eq!(awake["relays"], json!(["Relay 0"]));
    assert_eq!(awake["retry_after"], 0);
}

#[test]
fn relays_are_asked_in_order_until_one_answers() {
    let (outcome, asked) = wake_with(vec![
        Err(Failure::RelayOffline),
        Err(Failure::RelayUnreachable),
        Ok(status(Status::TargetNotOnLan)),
        Ok(status(Status::HostAwake)),
    ]);
    assert_eq!(asked, 4);
    assert_eq!(outcome["outcome"], "awake");
    assert_eq!(outcome["relays"], json!(["Relay 3"]));
    // A refusal or a rate limit is the PC's answer, not one relay's: the next is not asked.
    let (outcome, asked) = wake_with(vec![
        Ok(status(Status::RateLimited)),
        Ok(status(Status::HostAwake)),
    ]);
    assert_eq!(
        (outcome["outcome"].as_str(), asked),
        (Some("rate_limited"), 1)
    );
    let (_, asked) = wake_with(vec![
        Err(Failure::Network("down".into())),
        Err(Failure::RelayOffline),
    ]);
    assert_eq!(asked, 1);
}

#[test]
fn when_no_relay_answers_the_most_telling_miss_is_the_outcome() {
    let (offline, _) = wake_with(vec![Err(Failure::RelayOffline), Err(Failure::RelayOffline)]);
    assert_eq!(offline["outcome"], "relay_offline");
    assert_eq!(offline["relays"], json!(["Relay 0", "Relay 1"]));
    let (unreachable, _) = wake_with(vec![
        Err(Failure::RelayOffline),
        Err(Failure::RelayUnreachable),
    ]);
    assert_eq!(unreachable["outcome"], "relay_unreachable");
    assert_eq!(unreachable["relays"], json!(["Relay 1"]));
    let (elsewhere, _) = wake_with(vec![
        Ok(status(Status::TargetNotOnLan)),
        Err(Failure::RelayUnreachable),
    ]);
    assert_eq!(elsewhere["outcome"], "not_on_pc_network");
    assert_eq!(elsewhere["relays"], json!(["Relay 0"]));
}

#[test]
fn progress_names_the_relay_and_its_status() {
    assert_eq!(
        progress_json("NUC", &Progress::LookingUp),
        json!({"stage": "looking_up", "relay": "NUC"})
    );
    assert_eq!(
        progress_json("NUC", &Progress::Punching)["stage"],
        "punching"
    );
    assert_eq!(
        progress_json("NUC", &Progress::Requested)["stage"],
        "requested"
    );
    let mut sent = status(Status::PacketSent);
    sent.elapsed_ms = 1500;
    assert_eq!(
        progress_json("NUC", &Progress::Status(sent)),
        json!({"stage": "status", "relay": "NUC", "status": "packet_sent", "elapsed_ms": 1500})
    );
    let mut reported = Vec::new();
    wake(
        relays_of(&["A", "B"]),
        &AtomicBool::new(false),
        &mut |step| reported.push(step),
        &mut |_, _, progress| {
            progress(Progress::LookingUp);
            Err(Failure::RelayOffline)
        },
    );
    assert_eq!(
        reported,
        [
            json!({"stage": "looking_up", "relay": "A"}),
            json!({"stage": "looking_up", "relay": "B"})
        ]
    );
}

#[test]
fn the_requester_without_dht_routers_finds_every_relay_offline() {
    // The real requester on loopback: no routers to look the relays up with, so each is offline.
    let mut reported = Vec::new();
    let outcome = wake(
        relays_of(&["A", "B"]),
        &AtomicBool::new(false),
        &mut |step| reported.push(step["stage"].as_str().unwrap().to_owned()),
        &mut requester::request_wake,
    );
    assert_eq!(outcome["outcome"], "relay_offline");
    assert_eq!(outcome["relays"], json!(["A", "B"]));
    assert_eq!(reported, ["looking_up", "looking_up"]);
}

#[test]
fn a_cancelled_request_asks_no_relay_and_its_id_ends_with_it() {
    let id = create();
    cancel(id);
    let answer: Value =
        serde_json::from_str(&request(id, Some(&bundle(&["NUC"])), &mut |_| {})).unwrap();
    assert_eq!(answer["outcome"], "cancelled");
    assert!(!cancels().contains_key(&id));
    // An id that ended, or never was, counts as cancelled.
    let again: Value =
        serde_json::from_str(&request(id, Some(&bundle(&["NUC"])), &mut |_| {})).unwrap();
    assert_eq!(again["outcome"], "cancelled");
    cancel(i64::MAX);
    let invalid: Value = serde_json::from_str(&request(create(), None, &mut |_| {})).unwrap();
    assert_eq!(invalid["outcome"], "invalid");
}
