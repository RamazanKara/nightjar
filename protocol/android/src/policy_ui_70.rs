//! JNI bridge for `NightjarUiPolicyJni`: the 7.0 interface's decisions, from `nightjar-policy`,
//! the same code desktop Nightjar asks. The surface tier and its frame probe and crash guard
//! (`glass_probe`), the interface's size (`viewport_scale`), the Resolution picker's steps
//! (`resolution_scale`), the hints' button names (`input_glyphs`), touch feedback
//! (`ui_feedback_contract`) and the Control Center (`control_center`, JSON in and out like the
//! stream menu's call).
//!
//! Each export is a thin wrapper over a pure function here, which the tests below pin. Tiers,
//! form factors, sources, hints and events cross as their stable string keys.

mod control_center;

use jni::JNIEnv;
use jni::objects::{JClass, JString};
use jni::sys::{jboolean, jfloat, jfloatArray, jint, jintArray, jlong, jlongArray, jstring};
use nightjar_policy::glass_probe::{self, Device, GUARD_WARNING, Guard, Probe, Tier};
use nightjar_policy::input_glyphs::{self, Glyph, Hint, Source, Style};
use nightjar_policy::resolution_scale;
use nightjar_policy::ui_feedback_contract::{self, Event, Feedback, Gate, Haptic, Limiter, Sound};
use nightjar_policy::viewport_scale::{self, FormFactor, Viewport};

use crate::native_api;
use crate::policy::{int_array, jbool, non_neg_u32, non_neg_u64};
use crate::policy_recovery::new_jstring;
use crate::slab::MutexSlab;

// --- Surface tier (glass_probe) ---

/// Where a device lands: the tier's key. `stored` is the probe's last verdict, or null.
pub fn tier_land(device: Device, stored: Option<&str>) -> &'static str {
    glass_probe::land(&device, stored.and_then(Tier::parse)).key()
}

/// One tier down, as a failed probe steps; an unknown key reads as baked.
pub fn tier_step_down(tier: &str) -> &'static str {
    Tier::parse(tier).map_or(Tier::Baked, Tier::step_down).key()
}

/// Whether a frame of `work_ns` that landed after `total_ns` (negative: unknown) is slow on a
/// screen refreshing at `refresh_hz`.
pub fn probe_slow(refresh_hz: f32, work_ns: i64, total_ns: i64) -> bool {
    let probe = Probe::new(Tier::Live, refresh_hz).expect("live is probed");
    let total = u64::try_from(total_ns).ok();
    probe.slow(u64::try_from(work_ns).unwrap_or(0), total)
}

/// The probe's constants: frames, settle time in ms, and the floor rate's budget in ns.
pub fn probe_constants() -> [i64; 3] {
    [
        i64::from(glass_probe::PROBE_FRAMES),
        glass_probe::SETTLE_MS as i64,
        glass_probe::budget_ns(glass_probe::MIN_BUDGET_HZ) as i64,
    ]
}

/// The probe's fractions: the p90's share of the budget, how late a slow frame lands, and the
/// rate the budget floors at.
pub fn probe_fractions() -> [f32; 3] {
    [
        glass_probe::P90_BUDGET_FRACTION,
        glass_probe::LATE_FRACTION,
        glass_probe::MIN_BUDGET_HZ,
    ]
}

/// What the guard commits: `arm` before the first live frame, otherwise its clear.
pub fn guard_in_flight(arm: bool, fingerprint: &str) -> bool {
    if arm {
        Guard::arm(fingerprint)
    } else {
        Guard::clear(fingerprint)
    }
    .in_flight
}

/// Whether the stored guard says the last probe on this build never finished.
pub fn guard_tripped(in_flight: bool, stored: &str, fingerprint: &str) -> bool {
    Guard {
        in_flight,
        fingerprint: stored.to_owned(),
    }
    .tripped(fingerprint)
}

// --- Interface size (viewport_scale) ---

/// The form factor of its key (`phone`, `handheld`, `tv`, `desktop`); anything else is a phone.
pub fn form(key: &str) -> FormFactor {
    control_center::form_factor(key).unwrap_or(FormFactor::Phone)
}

/// `viewport_scale::scale` as nine floats: ui, density, type scale, margin, hero height, columns,
/// smallest target, stage width and text width (sizes in design dp).
pub fn viewport(
    width_dp: f32,
    height_dp: f32,
    dpi: f32,
    form_key: &str,
    user_pct: u32,
    tv_compact: bool,
) -> [f32; 9] {
    let viewport = Viewport {
        width_dp,
        height_dp,
        dpi,
        form: form(form_key),
        user_scale_pct: user_pct,
        tv_compact,
    };
    let s = viewport_scale::scale(&viewport);
    [
        s.ui,
        s.density,
        s.type_scale,
        s.margin_dp,
        s.hero_height,
        s.columns as f32,
        s.touch_min_dp,
        s.stage_max_width_dp,
        s.text_max_width_dp,
    ]
}

// --- Resolution steps (resolution_scale) ---

/// This screen's steps, flattened: percent, width, height for each, largest first.
pub fn resolution_choices(width: u32, height: u32) -> Vec<i32> {
    resolution_scale::choices(width, height)
        .into_iter()
        .flat_map(|(pct, (w, h))| [pct as i32, w as i32, h as i32])
        .collect()
}

pub fn resolution_scaled(width: u32, height: u32, pct: u32) -> [i32; 2] {
    let (w, h) = resolution_scale::scaled(width, height, pct);
    [w as i32, h as i32]
}

/// The step a saved size is on this screen, or 0.
pub fn resolution_step_of(native_w: u32, native_h: u32, w: u32, h: u32) -> i32 {
    resolution_scale::step_of(native_w, native_h, w, h).map_or(0, |pct| pct as i32)
}

// --- Hints (input_glyphs) ---

pub fn style_key(style: Style) -> &'static str {
    match style {
        Style::Xbox => "xbox",
        Style::PlayStation => "playstation",
        Style::Nintendo => "nintendo",
    }
}

fn source(key: &str, vendor: u16) -> Option<Source> {
    Some(match key {
        "touch" => Source::Touch,
        "mouse" => Source::Mouse,
        "keyboard" => Source::Keyboard,
        "remote" => Source::Remote,
        "pad" => Source::Pad(Style::of_vendor(vendor)),
        _ => return None,
    })
}

fn hint(key: &str) -> Option<Hint> {
    Some(match key {
        "go" => Hint::Go,
        "more" => Hint::More,
        "back" => Hint::Back,
        _ => return None,
    })
}

/// The key a hint names: `[glyph key, English text]`, or `None` for no hint (a finger, a mouse,
/// or a name this version does not know). The app translates the words of a remote and a keyboard.
pub fn glyph(
    source_key: &str,
    vendor: u16,
    hint_key: &str,
    swap: bool,
) -> Option<[&'static str; 2]> {
    let glyph: Glyph = input_glyphs::glyph(source(source_key, vendor)?, hint(hint_key)?, swap)?;
    Some([glyph.key(), glyph.en()])
}

// --- Feedback (ui_feedback_contract) ---

pub fn haptic_key(haptic: Haptic) -> &'static str {
    match haptic {
        Haptic::Tick => "tick",
        Haptic::Click => "click",
        Haptic::Press => "press",
        Haptic::Double => "double",
        Haptic::Rise => "rise",
    }
}

pub fn sound_key(sound: Sound) -> &'static str {
    match sound {
        Sound::Tick => "tick",
        Sound::Select => "select",
        Sound::Back => "back",
        Sound::Toggle => "toggle",
        Sound::Snap => "snap",
        Sound::Reject => "reject",
        Sound::Launch => "launch",
    }
}

fn feedback_keys(feedback: Feedback) -> [Option<&'static str>; 2] {
    [
        feedback.haptic.map(haptic_key),
        feedback.sound.map(sound_key),
    ]
}

/// What `event` answers with, `[haptic, sound]`; nothing for an unknown event.
pub fn feedback(event: &str, gate: Gate) -> [Option<&'static str>; 2] {
    Event::parse(event).map_or([None, None], |event| {
        feedback_keys(ui_feedback_contract::feedback(event, gate))
    })
}

/// [`feedback`] through a rate limiter.
pub fn feedback_limited(
    limiter: &mut Limiter,
    event: &str,
    gate: Gate,
    now_ms: u64,
) -> [Option<&'static str>; 2] {
    Event::parse(event).map_or([None, None], |event| {
        feedback_keys(limiter.feedback(event, gate, now_ms))
    })
}

static LIMITER_SLAB: MutexSlab<Limiter> = MutexSlab::new();

// --- JNI helpers ---

fn float_array(env: &mut JNIEnv, values: &[f32]) -> jfloatArray {
    let Ok(arr) = env.new_float_array(values.len() as i32) else {
        return std::ptr::null_mut();
    };
    if env.set_float_array_region(&arr, 0, values).is_err() {
        return std::ptr::null_mut();
    }
    arr.into_raw()
}

fn long_array(env: &mut JNIEnv, values: &[i64]) -> jlongArray {
    let Ok(arr) = env.new_long_array(values.len() as i32) else {
        return std::ptr::null_mut();
    };
    if env.set_long_array_region(&arr, 0, values).is_err() {
        return std::ptr::null_mut();
    }
    arr.into_raw()
}

/// `[haptic, sound]` as the app reads it: `haptic|sound`, an empty side for none.
pub fn pair(answer: [Option<&str>; 2]) -> String {
    format!(
        "{}|{}",
        answer[0].unwrap_or_default(),
        answer[1].unwrap_or_default()
    )
}

fn text(env: &mut JNIEnv, value: &JString) -> Option<String> {
    if value.is_null() {
        return None;
    }
    native_api::string(env, value)
}

fn gate(stream_active: jboolean, haptics: jboolean, sounds: jboolean) -> Gate {
    Gate {
        stream_active: stream_active != 0,
        haptics: haptics != 0,
        sounds: sounds != 0,
    }
}

// --- Exports ---

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarUiPolicyJni_tierLand(
    mut env: JNIEnv,
    _: JClass,
    lens: jboolean,
    live: jboolean,
    low_ram: jboolean,
    power_save: jboolean,
    thermal_severe: jboolean,
    high_contrast: jboolean,
    crash_guard_tripped: jboolean,
    stored: JString,
) -> jstring {
    let device = Device {
        lens: lens != 0,
        live: live != 0,
        low_ram: low_ram != 0,
        power_save: power_save != 0,
        thermal_severe: thermal_severe != 0,
        high_contrast: high_contrast != 0,
        crash_guard_tripped: crash_guard_tripped != 0,
    };
    let stored = text(&mut env, &stored);
    new_jstring(&env, tier_land(device, stored.as_deref()))
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarUiPolicyJni_tierStepDown(
    mut env: JNIEnv,
    _: JClass,
    tier: JString,
) -> jstring {
    let tier = text(&mut env, &tier).unwrap_or_default();
    new_jstring(&env, tier_step_down(&tier))
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarUiPolicyJni_probeBudgetNs(
    _: JNIEnv,
    _: JClass,
    refresh_hz: jfloat,
) -> jlong {
    glass_probe::budget_ns(refresh_hz) as jlong
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarUiPolicyJni_probeSlow(
    _: JNIEnv,
    _: JClass,
    refresh_hz: jfloat,
    work_ns: jlong,
    total_ns: jlong,
) -> jboolean {
    jbool(probe_slow(refresh_hz, work_ns, total_ns))
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarUiPolicyJni_probeSlowFramesToFail(
    _: JNIEnv,
    _: JClass,
    frames: jint,
) -> jint {
    glass_probe::slow_frames_to_fail(non_neg_u32(frames)) as jint
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarUiPolicyJni_probeSettled(
    _: JNIEnv,
    _: JClass,
    elapsed_ms: jlong,
) -> jboolean {
    jbool(elapsed_ms >= 0 && Probe::settled(non_neg_u64(elapsed_ms)))
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarUiPolicyJni_probeConstants(
    mut env: JNIEnv,
    _: JClass,
) -> jlongArray {
    long_array(&mut env, &probe_constants())
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarUiPolicyJni_probeFractions(
    mut env: JNIEnv,
    _: JClass,
) -> jfloatArray {
    float_array(&mut env, &probe_fractions())
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarUiPolicyJni_guardInFlight(
    mut env: JNIEnv,
    _: JClass,
    arm: jboolean,
    fingerprint: JString,
) -> jboolean {
    let fingerprint = text(&mut env, &fingerprint).unwrap_or_default();
    jbool(guard_in_flight(arm != 0, &fingerprint))
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarUiPolicyJni_guardTripped(
    mut env: JNIEnv,
    _: JClass,
    in_flight: jboolean,
    stored: JString,
    fingerprint: JString,
) -> jboolean {
    let stored = text(&mut env, &stored).unwrap_or_default();
    let fingerprint = text(&mut env, &fingerprint).unwrap_or_default();
    jbool(guard_tripped(in_flight != 0, &stored, &fingerprint))
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarUiPolicyJni_guardWarning(
    env: JNIEnv,
    _: JClass,
) -> jstring {
    new_jstring(&env, GUARD_WARNING)
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarUiPolicyJni_viewportScale(
    mut env: JNIEnv,
    _: JClass,
    width_dp: jfloat,
    height_dp: jfloat,
    dpi: jfloat,
    form_key: JString,
    user_pct: jint,
    tv_compact: jboolean,
) -> jfloatArray {
    let form_key = text(&mut env, &form_key).unwrap_or_default();
    let scale = viewport(
        width_dp,
        height_dp,
        dpi,
        &form_key,
        non_neg_u32(user_pct),
        tv_compact != 0,
    );
    float_array(&mut env, &scale)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarUiPolicyJni_resolutionChoices(
    mut env: JNIEnv,
    _: JClass,
    width: jint,
    height: jint,
) -> jintArray {
    int_array(
        &mut env,
        &resolution_choices(non_neg_u32(width), non_neg_u32(height)),
    )
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarUiPolicyJni_resolutionScaled(
    mut env: JNIEnv,
    _: JClass,
    width: jint,
    height: jint,
    pct: jint,
) -> jintArray {
    int_array(
        &mut env,
        &resolution_scaled(non_neg_u32(width), non_neg_u32(height), non_neg_u32(pct)),
    )
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarUiPolicyJni_resolutionStepOf(
    _: JNIEnv,
    _: JClass,
    native_width: jint,
    native_height: jint,
    width: jint,
    height: jint,
) -> jint {
    resolution_step_of(
        non_neg_u32(native_width),
        non_neg_u32(native_height),
        non_neg_u32(width),
        non_neg_u32(height),
    )
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarUiPolicyJni_inputGlyph(
    mut env: JNIEnv,
    _: JClass,
    source_key: JString,
    vendor_id: jint,
    hint_key: JString,
    swap: jboolean,
) -> jstring {
    let source_key = text(&mut env, &source_key).unwrap_or_default();
    let hint_key = text(&mut env, &hint_key).unwrap_or_default();
    let vendor = u16::try_from(vendor_id).unwrap_or(0);
    match glyph(&source_key, vendor, &hint_key, swap != 0) {
        Some([key, en]) => new_jstring(&env, &format!("{key}:{en}")),
        None => std::ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarUiPolicyJni_inputStyleOfVendor(
    env: JNIEnv,
    _: JClass,
    vendor_id: jint,
) -> jstring {
    new_jstring(
        &env,
        style_key(Style::of_vendor(u16::try_from(vendor_id).unwrap_or(0))),
    )
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarUiPolicyJni_figtreeCovers(
    mut env: JNIEnv,
    _: JClass,
    locale: JString,
) -> jboolean {
    let locale = text(&mut env, &locale).unwrap_or_default();
    jbool(input_glyphs::figtree_covers(&locale))
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarUiPolicyJni_feedback(
    mut env: JNIEnv,
    _: JClass,
    event: JString,
    stream_active: jboolean,
    haptics: jboolean,
    sounds: jboolean,
) -> jstring {
    let event = text(&mut env, &event).unwrap_or_default();
    new_jstring(
        &env,
        &pair(feedback(&event, gate(stream_active, haptics, sounds))),
    )
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarUiPolicyJni_feedbackLimiterCreate(
    _: JNIEnv,
    _: JClass,
) -> jlong {
    LIMITER_SLAB.insert(Limiter::default())
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarUiPolicyJni_feedbackLimiterDestroy(
    _: JNIEnv,
    _: JClass,
    handle: jlong,
) {
    LIMITER_SLAB.remove(handle);
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarUiPolicyJni_feedbackLimited(
    mut env: JNIEnv,
    _: JClass,
    handle: jlong,
    event: JString,
    stream_active: jboolean,
    haptics: jboolean,
    sounds: jboolean,
    now_ms: jlong,
) -> jstring {
    let event = text(&mut env, &event).unwrap_or_default();
    let gate = gate(stream_active, haptics, sounds);
    let answer = LIMITER_SLAB
        .with(handle, |limiter| {
            feedback_limited(limiter, &event, gate, non_neg_u64(now_ms))
        })
        // A destroyed limiter limits nothing; the gate still holds.
        .unwrap_or_else(|| feedback(&event, gate));
    new_jstring(&env, &pair(answer))
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarUiPolicyJni_controlCenterCall(
    mut env: JNIEnv,
    _: JClass,
    request: JString,
) -> jstring {
    let answer = match text(&mut env, &request) {
        Some(request) => control_center::call(&request),
        None => serde_json::json!({"error": "invalid", "detail": "the request is not a string"})
            .to_string(),
    };
    new_jstring(&env, &answer)
}

#[cfg(test)]
mod tests;
