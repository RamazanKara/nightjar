//! Android's launch curtain: the stream's [`Curtain`] (`nightjar_policy::launch_curtain`, the
//! rules desktop Nightjar follows too) lives here from `CurtainBridge.begin` until `end`. The
//! listener hands it the DESCRIBE's offer and the host's states (0x5548), the video renderer the
//! first picture; each change calls `MoonBridge.bridgeClLaunchCurtain`, and the app reads the
//! state ([`state_json`]) and calls `CurtainBridge.tick` when its `due_ms` comes, so the watchdog
//! lifts the curtain even if the host never speaks again.

use std::sync::{Mutex, MutexGuard};
use std::time::Instant;

use jni::JNIEnv;
use jni::objects::JClass;
use jni::sys::{JNI_FALSE, jboolean, jstring};
use nightjar_policy::launch_curtain::{Curtain, Lift};
use rubylight_protocol::launch_curtain::{CurtainState, LAUNCH_CURTAIN_STATE_MESSAGE_TYPE};
use serde_json::{Value, json};

use crate::bridge::Bridge;

/// The curtain of the stream starting or running, None without one.
static CURTAIN: Mutex<Option<Curtain>> = Mutex::new(None);

fn curtain() -> MutexGuard<'static, Option<Curtain>> {
    CURTAIN.lock().unwrap_or_else(|p| p.into_inner())
}

/// `CurtainBridge.begin`: a stream starts; `asked` when its launch asked for the curtain.
pub fn begin(asked: bool) {
    *curtain() = asked.then(|| Curtain::new(Instant::now()));
}

/// `CurtainBridge.end`: the stream ended, and its curtain with it.
pub fn end() {
    *curtain() = None;
}

/// The host's announced control messages: without 0x5548 the curtain lifts at once. Calls Java
/// when it did.
pub fn offered(bridge: &Bridge, messages: &[u16]) {
    if offered_in(&mut curtain(), messages) {
        changed(bridge);
    }
}

/// A state from the host (0x5548). Calls Java while the curtain is down: the words, the watchdog
/// or the curtain itself may have changed.
pub fn heard(bridge: &Bridge, state: CurtainState) {
    if heard_in(&mut curtain(), Instant::now(), state) {
        changed(bridge);
    }
}

/// The stream's first picture: Show PC screen is offered, and the quiet watch starts.
pub fn picture(bridge: &Bridge) {
    if picture_in(&mut curtain(), Instant::now()) {
        changed(bridge);
    }
}

fn changed(bridge: &Bridge) {
    bridge.post(bridge.m.launch_curtain, &[]);
}

fn offered_in(slot: &mut Option<Curtain>, messages: &[u16]) -> bool {
    slot.as_mut().is_some_and(|curtain| {
        curtain
            .offered(messages.contains(&LAUNCH_CURTAIN_STATE_MESSAGE_TYPE))
            .is_some()
    })
}

fn heard_in(slot: &mut Option<Curtain>, now: Instant, state: CurtainState) -> bool {
    let Some(curtain) = slot.as_mut().filter(|curtain| curtain.down()) else {
        return false;
    };
    curtain.heard(now, state);
    true
}

fn picture_in(slot: &mut Option<Curtain>, now: Instant) -> bool {
    let Some(curtain) = slot.as_mut().filter(|curtain| curtain.down()) else {
        return false;
    };
    let first = !curtain.view().dismissible;
    curtain.picture(now);
    first
}

/// The curtain for Java, or None without one: `down`; while it is down the stage word's `stage`
/// key, the `attention` card (`key`, `launcher` to fill in, `title`), `dismissible` (Show PC
/// screen offered), `host_dismissible` (the host takes the dismissal) and `due_ms` (when to call
/// `tick`, rounded up); once it lifted `lift` (`how`, the `notice` key, the host's `reason` for a
/// failed start).
fn state_json(slot: &Option<Curtain>, now: Instant) -> Option<Value> {
    let curtain = slot.as_ref()?;
    let view = curtain.view();
    let due_ms = curtain.due().map(|due| {
        let left = due.saturating_duration_since(now).as_micros();
        u64::try_from(left.div_ceil(1000)).unwrap_or(u64::MAX)
    });
    Some(json!({
        "down": curtain.down(),
        "stage": view.stage.map(|words| words.key),
        "attention": view.attention.map(|attention| json!({
            "key": attention.words.key,
            "launcher": attention.launcher,
            "title": attention.title,
        })),
        "dismissible": view.dismissible,
        "host_dismissible": curtain.host_dismissible(),
        "due_ms": due_ms,
        "lift": curtain.lifted().map(lift_json),
    }))
}

fn lift_json(lift: &Lift) -> Value {
    let how = match lift {
        Lift::Ready => "ready",
        Lift::Dismissed => "dismissed",
        Lift::TimedOut(_) => "timed_out",
        Lift::Failed(_) => "failed",
        Lift::NotOffered => "not_offered",
        Lift::Quiet => "quiet",
    };
    // The host's reason follows a failure's notice; a timeout's says no more than its own.
    let reason = matches!(lift, Lift::Failed(_))
        .then(|| lift.reason())
        .flatten();
    json!({
        "how": how,
        "notice": lift.notice().map(|words| words.key),
        "reason": reason,
    })
}

/// Runs `f` on the curtain and returns its state for Java, or null without one.
fn answer(env: &JNIEnv, f: impl FnOnce(&mut Curtain, Instant)) -> jstring {
    let now = Instant::now();
    let mut slot = curtain();
    if let Some(curtain) = slot.as_mut() {
        f(curtain, now);
    }
    match state_json(&slot, now) {
        Some(state) => crate::http::new_jstring(env, &state.to_string()),
        None => std::ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_CurtainBridge_nativeBegin(
    _: JNIEnv,
    _: JClass,
    asked: jboolean,
) {
    begin(asked != JNI_FALSE);
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_CurtainBridge_nativeEnd(
    _: JNIEnv,
    _: JClass,
) {
    end();
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_CurtainBridge_nativeState(
    env: JNIEnv,
    _: JClass,
) -> jstring {
    answer(&env, |_, _| {})
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_CurtainBridge_nativeTick(
    env: JNIEnv,
    _: JClass,
) -> jstring {
    answer(&env, |curtain, now| {
        curtain.tick(now);
    })
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_CurtainBridge_nativeDismiss(
    env: JNIEnv,
    _: JClass,
) -> jstring {
    answer(&env, |curtain, _| {
        curtain.dismiss();
    })
}

#[cfg(test)]
mod tests;
