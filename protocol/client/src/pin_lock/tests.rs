//! Unit tests for `nightjar-core::pin_lock` (PBKDF2 hasher, file store, gate, legacy import).

use super::hasher;
use super::store::file_lock;
use super::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};

struct FakeClock {
    now: AtomicI64,
    boot: AtomicI64,
}

impl Clock for FakeClock {
    fn elapsed(&self) -> i64 {
        self.now.load(Ordering::SeqCst)
    }
    fn boot(&self) -> i64 {
        self.boot.load(Ordering::SeqCst)
    }
}

struct FakeCrypto {
    seq: AtomicU64,
}

impl PinCrypto for FakeCrypto {
    fn create(&self, pin: &[u8], iterations: u32) -> Result<String, String> {
        let text = std::str::from_utf8(pin).map_err(|_| "bad utf-8".to_owned())?;
        Ok(format!("v1${iterations}$fake${text}"))
    }
    fn verify(&self, stored: &str, pin: &[u8]) -> bool {
        let Ok(text) = std::str::from_utf8(pin) else {
            return false;
        };
        stored.split('$').nth(3) == Some(text)
    }
    fn well_formed(&self, stored: &str) -> bool {
        let parts: Vec<&str> = stored.split('$').collect();
        parts.len() == 4 && parts[0] == "v1" && !parts[2].is_empty() && !parts[3].is_empty()
    }
    fn calibrate(&self) -> u32 {
        1_000
    }
    fn random_token(&self) -> Result<String, String> {
        let n = self.seq.fetch_add(1, Ordering::SeqCst) + 1;
        Ok(format!("{n:062x}ab"))
    }
    fn token_hash(&self, token: &str) -> String {
        let mut h = 0xcbf2_9ce4_8422_2325_u64;
        for b in token.bytes() {
            h = (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
        }
        format!("fnv1a:{h:016x}")
    }
}

fn store(tag: &str) -> (PinStore, Arc<FakeClock>, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("nightjar-core-pin-{tag}-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).unwrap();
    let clock = Arc::new(FakeClock {
        now: AtomicI64::new(1_000_000),
        boot: AtomicI64::new(7),
    });
    let crypto = Arc::new(FakeCrypto {
        seq: AtomicU64::new(0),
    });
    (PinStore::new(&dir, clock.clone(), crypto), clock, dir)
}

#[test]
fn a_hash_verifies_its_pin_only_and_keeps_androids_format() {
    let record = hasher::create(b"2580", 1_000).unwrap();
    let parts: Vec<&str> = record.split('$').collect();
    assert_eq!(parts.len(), 4, "{record}");
    assert_eq!((parts[0], parts[1]), ("v1", "1000"));
    assert_eq!(
        crate::http::der::base64_decode(parts[2]).unwrap().len(),
        hasher::SALT_BYTES
    );
    assert_eq!(
        crate::http::der::base64_decode(parts[3]).unwrap().len(),
        hasher::KEY_BYTES
    );
    assert!(!record.contains("2580"), "the PIN is never stored");
    assert!(hasher::verify(&record, b"2580"));
    for wrong in [&b"2581"[..], b"258", b"25800", b""] {
        assert!(!hasher::verify(&record, wrong), "{wrong:?}");
    }
    assert_ne!(
        record,
        hasher::create(b"2580", 1_000).unwrap(),
        "every PIN gets a fresh salt"
    );
}

#[test]
fn a_record_androids_pin_hasher_wrote_verifies() {
    let out = hasher::pbkdf2(b"1234", &[0; 16], 1000, 32).unwrap();
    let record = format!(
        "v1$1000${}${}",
        crate::http::der::base64_encode(&[0u8; 16]),
        crate::http::der::base64_encode(&out)
    );
    assert!(hasher::verify(&record, b"1234"));
    let vector = hasher::pbkdf2(b"passwd", b"salt", 1, 64).unwrap();
    assert_eq!(
        &vector[..8],
        &[0x55, 0xac, 0x04, 0x6e, 0x56, 0xe3, 0x08, 0x9f]
    );
}

#[test]
fn records_that_were_not_written_here_never_verify() {
    let good = hasher::create(b"1234", 1_000).unwrap();
    let (_, rest) = good.split_once('$').unwrap();
    for bad in [
        String::new(),
        format!("v2${rest}"),
        good.replacen("1000", "0", 1),
        good.replacen("1000", "99999999", 1),
        good.replacen("1000", "x", 1),
        "v1$1000$$".into(),
        "v1$1000$!!$!!".into(),
        "v1$1000$AAAA$AA A=".into(),
        format!("{good}$extra"),
    ] {
        assert!(!hasher::verify(&bad, b"1234"), "{bad}");
        assert!(!hasher::well_formed(&bad), "{bad}");
    }
}

#[test]
fn calibration_aims_at_the_target_and_stays_in_range() {
    assert_eq!(
        hasher::scale(15_000_000),
        100_000,
        "1.5 us an iteration: 150 ms is 100k"
    );
    assert_eq!(hasher::scale(1), hasher::MAX_ITERATIONS);
    assert_eq!(hasher::scale(u64::MAX), hasher::MIN_ITERATIONS);
    let measured = hasher::calibrate();
    assert!((hasher::MIN_ITERATIONS..=hasher::MAX_ITERATIONS).contains(&measured));
    assert!(hasher::constant_time_equals(b"1234", b"1234"));
    assert!(!hasher::constant_time_equals(b"1234", b"1235"));
    assert!(!hasher::constant_time_equals(b"1234", b"12345"));
}

#[test]
fn the_store_locks_out_after_wrong_tries_and_a_right_one_clears_it() {
    let (store, clock, dir) = store("lockout");
    assert!(!store.has_pin() && store.mode() == Mode::Off);
    store.set_pin_with(b"4321", 1_000).unwrap();
    store.set_mode(Mode::App);
    assert_eq!((store.pin_length(), store.mode()), (4, Mode::App));
    store.flush();
    for _ in 0..4 {
        assert_eq!(store.try_pin(b"0000"), Outcome::Wrong { locked_for_ms: 0 });
    }
    assert_eq!(
        store.try_pin(b"0000"),
        Outcome::Wrong {
            locked_for_ms: 30_000
        }
    );
    assert_eq!(
        store.try_pin(b"4321"),
        Outcome::Locked {
            locked_for_ms: 30_000
        },
        "even the right PIN waits"
    );
    clock.now.fetch_add(10_000, Ordering::SeqCst);
    assert_eq!(store.locked_for_ms(), 20_000);
    clock.boot.store(8, Ordering::SeqCst);
    assert_eq!(store.locked_for_ms(), 30_000);
    clock.now.fetch_add(30_000, Ordering::SeqCst);
    assert_eq!(
        store.try_pin(b"0000"),
        Outcome::Wrong {
            locked_for_ms: 60_000
        },
        "the sixth wrong try doubles"
    );
    clock.now.fetch_add(60_000, Ordering::SeqCst);
    assert_eq!(store.try_pin(b"4321"), Outcome::Ok);
    assert_eq!(store.locked_for_ms(), 0);
    for _ in 0..4 {
        assert_eq!(
            store.try_pin(b"1111"),
            Outcome::Wrong { locked_for_ms: 0 },
            "the count started over"
        );
    }
    store.set_pin_with(b"98765", 1_000).unwrap();
    assert_eq!(store.try_pin(b"1111"), Outcome::Wrong { locked_for_ms: 0 });
    drop(store);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn a_try_is_counted_before_the_check_runs() {
    let (store, _, dir) = store("counted");
    store.set_pin_with(b"1234", 1_000).unwrap();
    let on_disk = || {
        serde_json::from_slice::<Saved>(&std::fs::read(dir.join(FILE)).unwrap())
            .unwrap()
            .fails
    };
    let fails_during_check = std::cell::Cell::new(0);
    let crypto = FakeCrypto {
        seq: AtomicU64::new(0),
    };
    let outcome = store.try_pin_with(b"1234", |hash, pin| {
        fails_during_check.set(on_disk());
        crypto.verify(hash, pin)
    });
    assert_eq!(outcome, Outcome::Ok);
    assert_eq!(
        fails_during_check.get(),
        1,
        "a kill during the check cannot give a free guess"
    );
    assert_eq!(on_disk(), 0, "and a match clears it");
    drop(store);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn a_fallible_verifier_error_leaves_the_failure_counted() {
    let (store, _, dir) = store("fallible");
    store.set_pin_with(b"1234", 1_000).unwrap();
    for _ in 0..5 {
        let err = store
            .try_pin_fallible(b"1234", |_, _| Err::<bool, _>("killed"))
            .unwrap_err();
        assert_eq!(err, "killed");
    }
    let saved: Saved = serde_json::from_slice(&std::fs::read(dir.join(FILE)).unwrap()).unwrap();
    assert_eq!(saved.fails, 5);
    assert_eq!(store.locked_for_ms(), 30_000);
    drop(store);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn a_pin_needs_four_to_eight_digits_and_removing_it_turns_the_lock_off() {
    let (store, _, dir) = store("rules");
    for bad in [&b"123"[..], b"123456789", b"12a4", b""] {
        assert!(store.set_pin_with(bad, 1_000).is_err(), "{bad:?}");
    }
    assert!(!store.has_pin());
    store.set_pin_with(b"12345678", 1_000).unwrap();
    store.set_mode(Mode::Streams);
    store.set_grace_minutes(30);
    store.set_pcs(vec!["UID".into()]);
    store.remove_pin();
    assert!(!store.has_pin());
    assert_eq!(
        (store.mode(), store.grace_minutes(), store.view().pcs.len()),
        (Mode::Off, DEFAULT_GRACE_MINUTES, 0),
        "everything that depended on it goes"
    );
    drop(store);
    std::fs::remove_dir_all(dir).ok();
}

fn retry_now(store: &PinStore) {
    store.memory().retry_at = Instant::now();
}

#[test]
fn a_damaged_file_keeps_the_app_locked_counts_no_tries_and_is_never_overwritten() {
    let crypto = Arc::new(FakeCrypto {
        seq: AtomicU64::new(0),
    });
    for (n, damaged) in ["{ not json", r#"{"hash":"not a hash","mode":"off"}"#]
        .into_iter()
        .enumerate()
    {
        let (_, clock, dir) = store(&format!("damaged{n}"));
        std::fs::write(dir.join(FILE), damaged).unwrap();
        let store = PinStore::new(&dir, clock.clone(), crypto.clone());
        assert!(
            matches!(store.problem(), Some(Problem::Damaged(_))),
            "{damaged}"
        );
        assert!(store.has_pin() && store.mode() == Mode::App);
        let gate = PinGate::new(&store);
        assert!(gate.needs_pin_for_app(&store));
        for _ in 0..8 {
            assert_eq!(
                store.try_pin(b"1234"),
                Outcome::Unavailable,
                "no PIN opens it, and no try counts"
            );
        }
        assert_eq!(store.locked_for_ms(), 0);
        store.set_mode(Mode::Off);
        store.set_grace_minutes(60);
        store.flush();
        assert_eq!(
            std::fs::read_to_string(dir.join(FILE)).unwrap(),
            damaged,
            "the closed state is never saved over it"
        );
        std::fs::remove_file(dir.join(FILE)).unwrap();
        retry_now(&store);
        store.problem();
        store.flush();
        assert_eq!(store.problem(), None);
        assert!(!store.has_pin() && !gate.needs_pin_for_app(&store));
        drop(store);
        std::fs::remove_dir_all(dir).ok();
    }
}

#[test]
fn a_file_that_cannot_be_read_now_locks_until_it_can_and_keeps_the_pin() {
    let (store, clock, dir) = store("unreadable");
    let crypto = store.crypto().clone();
    store.set_pin_with(b"1234", 1_000).unwrap();
    store.set_mode(Mode::Streams);
    store.flush();
    let good = std::fs::read(dir.join(FILE)).unwrap();
    drop(store);
    std::fs::remove_file(dir.join(FILE)).unwrap();
    std::fs::create_dir(dir.join(FILE)).unwrap();
    let store = PinStore::new(&dir, clock.clone(), crypto);
    assert!(matches!(store.problem(), Some(Problem::Unreadable(_))));
    assert!(
        store.has_pin() && store.mode() == Mode::App,
        "locked meanwhile"
    );
    store.set_mode(Mode::Off);
    store.flush();
    assert!(dir.join(FILE).is_dir(), "nothing was written in its place");
    std::fs::remove_dir(dir.join(FILE)).unwrap();
    std::fs::write(dir.join(FILE), &good).unwrap();
    assert_eq!(
        store.try_pin(b"1234"),
        Outcome::Ok,
        "a try reads it again first"
    );
    assert_eq!((store.problem(), store.mode()), (None, Mode::Streams));
    drop(store);
    std::fs::remove_dir_all(dir).ok();
}

#[cfg(windows)]
#[test]
fn a_file_another_program_holds_is_read_again_not_replaced() {
    use std::os::windows::fs::OpenOptionsExt;
    let (store, clock, dir) = store("shared");
    let crypto = store.crypto().clone();
    store.set_pin_with(b"1234", 1_000).unwrap();
    drop(store);
    let held = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(dir.join(FILE))
        .unwrap();
    let store = PinStore::new(&dir, clock.clone(), crypto);
    assert!(
        matches!(store.problem(), Some(Problem::Unreadable(_))),
        "a sharing violation is not damage"
    );
    drop(held);
    retry_now(&store);
    store.problem();
    store.flush();
    assert_eq!(store.problem(), None);
    assert_eq!(store.pin_length(), 4);
    drop(store);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn the_ui_never_waits_for_the_file() {
    let (store, _, dir) = store("nowait");
    store.set_pin_with(b"1234", 1_000).unwrap();
    let held = file_lock();
    let (done, wait) = channel();
    let ui = store.clone();
    std::thread::spawn(move || {
        ui.set_mode(Mode::App);
        ui.set_grace_minutes(30);
        let answer = (
            ui.locked_for_ms(),
            ui.has_pin(),
            ui.mode(),
            ui.view().grace_minutes,
            ui.problem(),
        );
        let _ = done.send(answer);
    });
    let answer = wait
        .recv_timeout(Duration::from_secs(5))
        .expect("answered without the file lock");
    assert_eq!(answer, (0, true, Mode::App, 30, None));
    drop(held);
    store.flush();
    let saved: Saved = serde_json::from_slice(&std::fs::read(dir.join(FILE)).unwrap()).unwrap();
    assert_eq!(
        (saved.mode, saved.grace_min),
        (Mode::App, 30),
        "the file thread wrote it after"
    );
    drop(store);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn continue_heres_press_leaves_the_grant_to_the_stream_it_starts() {
    let (store, _clock, dir) = store("press");
    store.set_pin_with(b"1234", 1_000).unwrap();
    store.set_mode(Mode::Streams);
    let mut gate = PinGate::new(&store);
    assert!(
        gate.locks_stream(&store, "pc"),
        "the press waits for the PIN"
    );
    gate.grant(&store, "pc");
    assert!(
        !gate.locks_stream(&store, "pc"),
        "the PIN passed: the press goes on"
    );
    assert!(
        !gate.locks_stream(&store, "pc"),
        "and asking did not use it up"
    );
    assert!(
        !gate.needs_pin_for_stream(&store, "pc"),
        "the stream after the press does not ask again"
    );
    assert!(gate.locks_stream(&store, "pc"), "the next press asks again");
    drop(store);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_press_that_failed_leaves_no_grant() {
    let (store, _clock, dir) = store("grant-forgotten");
    store.set_pin_with(b"1234", 1_000).unwrap();
    store.set_mode(Mode::Streams);
    let mut gate = PinGate::new(&store);
    gate.grant(&store, "pc");
    assert!(!gate.locks_stream(&store, "pc"));
    gate.forget_grant();
    assert!(gate.locks_stream(&store, "pc"), "the next start asks");
    assert!(gate.needs_pin_for_stream(&store, "pc"));
    drop(store);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_grant_is_used_up_by_one_start_and_expires() {
    let (store, clock, dir) = store("grant");
    store.set_pin_with(b"1234", 1_000).unwrap();
    store.set_mode(Mode::Streams);
    let mut gate = PinGate::new(&store);
    assert!(gate.needs_pin_for_stream(&store, "pc"));
    gate.grant(&store, "PC");
    assert!(!gate.needs_pin_for_stream(&store, "pc"), "granted");
    assert!(gate.needs_pin_for_stream(&store, "pc"), "used up");
    gate.grant(&store, "pc");
    assert!(
        gate.needs_pin_for_stream(&store, "other"),
        "for that PC only"
    );
    clock.now.fetch_add(GRANT_MS, Ordering::SeqCst);
    assert!(gate.needs_pin_for_stream(&store, "pc"), "expired");
    let (no_id, other) = (stream_key("", 5), stream_key("", 6));
    assert_eq!(stream_key("UID", 5), "UID");
    assert!(gate.needs_pin_for_stream(&store, &no_id));
    gate.grant(&store, &no_id);
    assert!(
        gate.needs_pin_for_stream(&store, &other),
        "not for another PC without one"
    );
    gate.grant(&store, &no_id);
    assert!(
        !gate.needs_pin_for_stream(&store, &no_id),
        "the right PIN lets it through"
    );
    drop(store);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn leaving_the_window_for_longer_than_the_grace_time_locks_the_app() {
    let (store, clock, dir) = store("grace");
    store.set_pin_with(b"1234", 1_000).unwrap();
    store.set_mode(Mode::App);
    let mut gate = PinGate::new(&store);
    assert!(gate.needs_pin_for_app(&store), "locked at start");
    gate.unlock_app();
    assert!(!gate.needs_pin_for_app(&store));
    gate.left(&store);
    clock.now.fetch_add(9 * 60_000, Ordering::SeqCst);
    gate.returned(&store);
    assert!(!gate.needs_pin_for_app(&store), "back within ten minutes");
    gate.left(&store);
    assert!(!PinGate::new(&store).needs_pin_for_app(&store));
    clock.now.fetch_add(10 * 60_000, Ordering::SeqCst);
    assert!(
        PinGate::new(&store).needs_pin_for_app(&store),
        "but not after it"
    );
    gate.returned(&store);
    assert!(gate.needs_pin_for_app(&store), "away ten minutes: locked");
    store.set_grace_minutes(0);
    gate.unlock_app();
    gate.left(&store);
    gate.returned(&store);
    assert!(gate.needs_pin_for_app(&store));
    drop(store);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn a_hand_over_opens_the_second_screens_lock_once_written_and_only_briefly() {
    let (first, clock, dir) = store("hand-over");
    let crypto = first.crypto().clone();
    assert_eq!(first.hand_over(), Ok(None), "no PIN, nothing to hand over");
    first.set_pin_with(b"1234", 1_000).unwrap();
    first.set_mode(Mode::Streams);
    first.flush();
    let token = first.hand_over().unwrap().expect("a token");
    assert_eq!(token.len(), 64);
    let file = std::fs::read_to_string(dir.join(FILE)).unwrap();
    assert!(
        !file.contains(&token),
        "the file keeps the token's hash, never the token"
    );
    assert_ne!(
        first.hand_over().unwrap(),
        Some(token.clone()),
        "every hand-over has a fresh token"
    );
    let token = first.hand_over().unwrap().unwrap();

    let second = PinStore::new(&dir, clock.clone(), crypto.clone());
    for wrong in ["", "00", &token[1..], &token.to_uppercase()] {
        assert!(!second.handed_over(wrong), "{wrong:?}");
    }
    assert!(second.handed_over(&token));
    let mut gate = PinGate::new(&second);
    assert!(
        gate.needs_pin_for_stream(&second, "UID"),
        "without it the stream asks"
    );
    gate.hand_over();
    second.set_mode(Mode::App);
    assert!(
        !gate.needs_pin_for_app(&second)
            && !gate.needs_pin_for_stream(&second, "UID")
            && !gate.needs_pin_for_stream(&second, "UID")
    );
    second.flush();
    let before = std::fs::read(dir.join(FILE)).unwrap();
    gate.left(&second);
    second.flush();
    assert_eq!(
        std::fs::read(dir.join(FILE)).unwrap(),
        before,
        "leaving it writes nothing: the first keeps the file"
    );
    clock.now.fetch_add(60 * 60_000, Ordering::SeqCst);
    gate.returned(&second);
    assert!(
        !gate.needs_pin_for_app(&second),
        "it ends with the first's stream, never locks over it"
    );
    clock.now.fetch_sub(60 * 60_000, Ordering::SeqCst);

    clock.now.fetch_add(HAND_OVER_MS, Ordering::SeqCst);
    assert!(
        !PinStore::new(&dir, clock.clone(), crypto.clone()).handed_over(&token),
        "too late"
    );
    clock.now.fetch_sub(HAND_OVER_MS, Ordering::SeqCst);
    clock.boot.store(8, Ordering::SeqCst);
    assert!(
        !PinStore::new(&dir, clock.clone(), crypto.clone()).handed_over(&token),
        "another boot"
    );
    clock.boot.store(7, Ordering::SeqCst);
    assert!(PinStore::new(&dir, clock.clone(), crypto.clone()).handed_over(&token));

    first.end_hand_over();
    first.flush();
    assert!(
        !PinStore::new(&dir, clock.clone(), crypto.clone()).handed_over(&token),
        "the second screen is gone"
    );
    let token = first.hand_over().unwrap().unwrap();
    first.remove_pin();
    first.flush();
    assert!(
        !PinStore::new(&dir, clock, crypto).handed_over(&token),
        "no PIN, no hand-over"
    );
    drop(first);
    drop(second);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn legacy_android_prefs_import_once_and_never_overwrite() {
    let dir = std::env::temp_dir().join(format!("nightjar-core-pin-import-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).unwrap();
    let hash = hasher::create(b"2468", 1_000).unwrap();
    let legacy = serde_json::json!({
        "pin_hash": hash,
        "pin_length": 4,
        "pin_mode": "selected",
        "pin_pcs": ["pc-a", "pc-b"],
        "pin_grace_min": 30,
        "pin_fails": 5,
        "pin_lock_until": 42_000,
        "pin_lock_boot": 9,
        "pin_active_at": 10_000,
        "pin_active_boot": 9,
    })
    .to_string();
    assert_eq!(import_legacy_json(&dir, &legacy, &RingPinCrypto), Ok(true));
    let clock = Arc::new(FakeClock {
        now: AtomicI64::new(12_000),
        boot: AtomicI64::new(9),
    });
    let store = PinStore::new(&dir, clock, Arc::new(RingPinCrypto));
    assert!(store.has_pin());
    assert_eq!(store.pin_length(), 4);
    assert_eq!(store.mode(), Mode::Selected);
    assert_eq!(store.view().pcs, vec!["pc-a", "pc-b"]);
    assert_eq!(store.grace_minutes(), 30);
    assert_eq!(store.locked_for_ms(), 30_000);
    assert_eq!(store.active_at(), 10_000);
    assert_eq!(store.active_boot(), 9);
    // Once pin_lock.json exists, re-importing never overwrites it.
    assert_eq!(
        import_legacy_json(&dir, r#"{"pin_hash":null}"#, &RingPinCrypto),
        Ok(false)
    );
    assert!(store.has_pin());
    drop(store);
    std::fs::remove_dir_all(&dir).unwrap();
    std::fs::create_dir_all(&dir).unwrap();
    assert!(import_legacy_json(&dir, r#"{"pin_hash":"bad"}"#, &RingPinCrypto).is_err());
    assert!(!dir.join(FILE).exists());
    assert_eq!(
        import_legacy_json(&dir, r#"{"pin_hash":null}"#, &RingPinCrypto),
        Ok(true)
    );
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn failed_set_pin_and_hand_over_keep_prior_memory_and_lockout_intact() {
    let (store, _, dir) = store("write-fail-rollback");
    store.set_pin_with(b"1234", 1_000).unwrap();
    store.set_mode(Mode::App);
    store.flush();
    for _ in 0..5 {
        let _ = store.try_pin(b"0000");
    }
    assert_eq!(store.locked_for_ms(), 30_000);

    // Replace `pin_lock.json` with a non-empty directory so `write_file` (rename) fails on all OSes.
    std::fs::remove_file(dir.join(FILE)).unwrap();
    std::fs::create_dir(dir.join(FILE)).unwrap();
    std::fs::write(dir.join(FILE).join("blocker"), b"x").unwrap();

    assert!(store.set_pin_with(b"98765", 1_000).is_err());
    assert!(store.hand_over().is_err());

    // Neither the PIN length nor the active lockout counter was clobbered in memory.
    assert_eq!(store.pin_length(), 4);
    assert_eq!(store.locked_for_ms(), 30_000);
    assert!(!store.handed_over("0000"));
    assert_eq!(
        store.try_pin(b"98765"),
        Outcome::Locked {
            locked_for_ms: 30_000
        }
    );

    drop(store);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn failed_throttle_write_returns_unavailable() {
    let (store, _, dir) = store("throttle-write-fail");
    store.set_pin_with(b"1234", 1_000).unwrap();
    store.set_mode(Mode::App);
    store.flush();

    std::fs::remove_file(dir.join(FILE)).unwrap();
    std::fs::create_dir(dir.join(FILE)).unwrap();
    std::fs::write(dir.join(FILE).join("blocker"), b"x").unwrap();

    assert_eq!(store.try_pin(b"0000"), Outcome::Unavailable);
    drop(store);
    std::fs::remove_dir_all(dir).ok();
}
