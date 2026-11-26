//! Away wake relay for the app's `com.limelight.computers.AwayWakeFlow`
//! (`docs/designs/15-away-wake-relay.md`): this device's wake keys come from the PC's
//! `GET /wake/relay` (the native API's [`KEYS_OP`]), and a wake from away asks the bundle's
//! relays, in order, until one answers, with `rubylight-traverse`'s requester on the calling
//! thread. Progress and the outcome cross JNI as JSON, so the app words them; the keys never
//! reach a log.

use jni::JNIEnv;
use jni::objects::{JClass, JObject, JString, JValue};
use jni::sys::{jlong, jstring};
use nightjar_core::http::native_api::wake_relay::WakeRelayBundle;
use rubylight_traverse::wake_relay::{Status, WakeStatus};
use rubylight_traverse::wake_relay_node::requester::{self, Failure, Progress, RequestConfig};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

use crate::native_api;

/// The native API op that reads `GET /wake/relay` (`native_api::request`).
pub const KEYS_OP: &str = "wake_relay";

/// Where a request's socket binds on a device: any address, any port.
const BIND: SocketAddr = SocketAddr::new(std::net::IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0);

/// One relay of the bundle: its name, for the app's words, and what this device asks it with.
pub struct Relay {
    pub name: String,
    pub config: RequestConfig,
}

/// The relays of `bundle` (the bundle of `GET /wake/relay`, as JSON) in its order, each asked
/// from `bind`; a relay whose key is malformed is left out. [`Outcome::NoRelay`] for a bundle
/// without relays, [`Outcome::Invalid`] for one that does not parse or has no usable key.
pub fn relays(
    bundle: Option<&str>,
    bind: SocketAddr,
    stun_servers: &[String],
    bootstrap: &[String],
) -> Result<Vec<Relay>, Outcome> {
    let bundle: WakeRelayBundle = bundle
        .and_then(|bundle| serde_json::from_str(bundle).ok())
        .ok_or(Outcome::Invalid)?;
    if bundle.relays.is_empty() {
        return Err(Outcome::NoRelay);
    }
    let relays: Vec<Relay> = bundle
        .relays
        .iter()
        .filter_map(|relay| {
            let config = nightjar_core::wake_relay::request_config(
                &bundle,
                relay,
                bind,
                stun_servers.to_vec(),
                bootstrap.to_vec(),
            )?;
            Some(Relay {
                name: relay.name.clone(),
                config,
            })
        })
        .collect();
    if relays.is_empty() {
        return Err(Outcome::Invalid);
    }
    Ok(relays)
}

/// How a wake from away ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The PC answers on its network.
    Awake,
    /// The magic packets went out, and the relay said nothing more.
    SentNoAnswer,
    /// The PC did not answer within the relay's wait.
    TimedOut,
    /// Too many wakes lately: again after this many seconds.
    RateLimited(u16),
    /// The relay refused this device's keys (unpaired, older keys) or its clock.
    Refused,
    /// The relays that answered are not on the PC's network.
    NotOnPcNetwork,
    /// No relay published a record lately.
    RelayOffline,
    /// No path to a relay opened, or it never answered.
    RelayUnreachable,
    Cancelled,
    /// This device could not open its socket.
    Network(String),
    /// The bundle names no relay.
    NoRelay,
    /// The bundle, or every key in it, is malformed.
    Invalid,
}

impl Outcome {
    pub fn key(&self) -> &'static str {
        match self {
            Outcome::Awake => "awake",
            Outcome::SentNoAnswer => "sent_no_answer",
            Outcome::TimedOut => "timed_out",
            Outcome::RateLimited(_) => "rate_limited",
            Outcome::Refused => "refused",
            Outcome::NotOnPcNetwork => "not_on_pc_network",
            Outcome::RelayOffline => "relay_offline",
            Outcome::RelayUnreachable => "relay_unreachable",
            Outcome::Cancelled => "cancelled",
            Outcome::Network(_) => "network",
            Outcome::NoRelay => "no_relay",
            Outcome::Invalid => "invalid",
        }
    }

    /// What a relay's last status says. `None` for a relay that is not on the PC's network:
    /// the next one may be.
    fn of(status: &WakeStatus) -> Option<Self> {
        Some(match status.status {
            Status::HostAwake => Outcome::Awake,
            Status::TimedOut => Outcome::TimedOut,
            Status::RateLimited => Outcome::RateLimited(status.retry_after),
            Status::RejectedAuth | Status::RejectedReplay => Outcome::Refused,
            Status::PacketSent | Status::WaitingForBoot => Outcome::SentNoAnswer,
            Status::TargetNotOnLan => return None,
        })
    }

    /// `{"outcome", "relays": [the names it concerns], "retry_after"}`, and `detail` for a
    /// network failure (for the log).
    pub fn json(&self, relays: &[String]) -> Value {
        let mut out = json!({
            "outcome": self.key(),
            "relays": relays,
            "retry_after": match self {
                Outcome::RateLimited(seconds) => *seconds,
                _ => 0,
            },
        });
        if let Outcome::Network(detail) = self {
            out["detail"] = Value::from(detail.as_str());
        }
        out
    }
}

/// A progress report of the relay `relay`: `{"stage", "relay"}`, with `status` (its key) and
/// `elapsed_ms` once the relay answered.
pub fn progress_json(relay: &str, progress: &Progress) -> Value {
    match progress {
        Progress::LookingUp => json!({"stage": "looking_up", "relay": relay}),
        Progress::Punching => json!({"stage": "punching", "relay": relay}),
        Progress::Requested => json!({"stage": "requested", "relay": relay}),
        Progress::Status(status) => json!({
            "stage": "status",
            "relay": relay,
            "status": status.status.key(),
            "elapsed_ms": status.elapsed_ms,
        }),
    }
}

/// Asks one relay; [`requester::request_wake`] on a device.
pub type Ask<'a> = dyn FnMut(RequestConfig, &AtomicBool, &mut dyn FnMut(Progress)) -> Result<WakeStatus, Failure>
    + 'a;

/// Asks `relays` in order until one answers (a relay that is offline, unreachable or not on the
/// PC's network hands on to the next), reporting [`progress_json`], and gives the outcome's
/// [`Outcome::json`]. Blocks.
pub fn wake(
    relays: Vec<Relay>,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(Value),
    ask: &mut Ask<'_>,
) -> Value {
    if relays.is_empty() {
        return Outcome::NoRelay.json(&[]);
    }
    let (mut offline, mut unreachable, mut elsewhere) = (Vec::new(), Vec::new(), Vec::new());
    for relay in relays {
        if cancel.load(Ordering::Acquire) {
            return Outcome::Cancelled.json(&[]);
        }
        let name = relay.name;
        let answer = ask(relay.config, cancel, &mut |step| {
            progress(progress_json(&name, &step))
        });
        match answer {
            Ok(status) => match Outcome::of(&status) {
                Some(outcome) => return outcome.json(&[name]),
                None => elsewhere.push(name),
            },
            Err(Failure::RelayOffline) => offline.push(name),
            Err(Failure::RelayUnreachable) => unreachable.push(name),
            Err(Failure::Cancelled) => return Outcome::Cancelled.json(&[]),
            Err(Failure::Network(detail)) => return Outcome::Network(detail).json(&[name]),
        }
    }
    // The most telling miss: a relay that answered, then one that was there, then none.
    if !elsewhere.is_empty() {
        Outcome::NotOnPcNetwork.json(&elsewhere)
    } else if !unreachable.is_empty() {
        Outcome::RelayUnreachable.json(&unreachable)
    } else {
        Outcome::RelayOffline.json(&offline)
    }
}

static NEXT: AtomicI64 = AtomicI64::new(1);
static CANCELS: LazyLock<Mutex<HashMap<i64, Arc<AtomicBool>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn cancels() -> std::sync::MutexGuard<'static, HashMap<i64, Arc<AtomicBool>>> {
    CANCELS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// A new request's id, for [`cancel`]; the [`request`] with it ends it.
pub fn create() -> i64 {
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    cancels().insert(id, Arc::new(AtomicBool::new(false)));
    id
}

/// Cancels the request `id`; one that ended, or never was, is left alone.
pub fn cancel(id: i64) {
    if let Some(flag) = cancels().get(&id) {
        flag.store(true, Ordering::Release);
    }
}

/// Runs the request `id` (from [`create`]) for the bundle's JSON on this thread, from any
/// address, with the default STUN servers and DHT routers. An id that is not known counts as
/// cancelled.
pub fn request(id: i64, bundle: Option<&str>, progress: &mut dyn FnMut(Value)) -> String {
    let flag = cancels()
        .get(&id)
        .cloned()
        .unwrap_or_else(|| Arc::new(AtomicBool::new(true)));
    let defaults = |list: &[&str]| list.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
    let stun = defaults(rubylight_traverse::STUN_SERVERS);
    let bootstrap = defaults(rubylight_traverse::DHT_BOOTSTRAP);
    let answer = match relays(bundle, BIND, &stun, &bootstrap) {
        Ok(relays) => wake(relays, &flag, progress, &mut requester::request_wake),
        Err(outcome) => outcome.json(&[]),
    };
    cancels().remove(&id);
    answer.to_string()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarJni_nativeWakeRelayCreate(
    _: JNIEnv,
    _: JClass,
) -> jlong {
    create()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarJni_nativeWakeRelayCancel(
    _: JNIEnv,
    _: JClass,
    id: jlong,
) {
    cancel(id);
}

/// Blocks until the wake ends (a minute or more): call it off the main thread. `listener`'s
/// `progress(String)` gets each [`progress_json`] on this thread.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarJni_nativeWakeRelayRequest(
    mut env: JNIEnv,
    _: JClass,
    id: jlong,
    bundle: JString,
    listener: JObject,
) -> jstring {
    let bundle = native_api::string(&mut env, &bundle);
    let answer = {
        let mut report = |step: Value| {
            if listener.is_null() {
                return;
            }
            let reported = env.with_local_frame(4, |env| {
                let text = env.new_string(step.to_string())?;
                env.call_method(
                    &listener,
                    "progress",
                    "(Ljava/lang/String;)V",
                    &[JValue::Object(&text)],
                )?;
                Ok::<(), jni::errors::Error>(())
            });
            if reported.is_err() && env.exception_check().unwrap_or(false) {
                let _ = env.exception_clear();
            }
        };
        request(id, bundle.as_deref(), &mut report)
    };
    match env.new_string(answer) {
        Ok(answer) => answer.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

#[cfg(test)]
mod tests;
