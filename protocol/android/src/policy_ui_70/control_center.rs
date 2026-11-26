//! The Control Center's JSON call for the app (`NightjarUiPolicyJni.controlCenterCall`): one
//! request in, one answer out, `{"ok": …}` or `{"error": code, "detail": …}`, the shape of
//! `nightjar_policy::stream_menu::json`. It answers from `nightjar_policy::control_center`: the
//! [`dock`], where it sits ([`placement`]) and what it is drawn on ([`surface`]); a dock request
//! also answers a step along the rail and a button's place.

use nightjar_policy::control_center::{Dock, Gates, Live, Module, dock, placement, surface};
use nightjar_policy::glass_probe::Tier;
use nightjar_policy::stream_menu::{Buttons, Hotkey, Offer, UnknownTile};
use nightjar_policy::viewport_scale::FormFactor;
use serde::Deserialize;
use serde_json::{Value, json};

/// [`Gates`] as JSON: a feature the request does not name is off.
#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(default)]
struct GatesJson {
    stream_dvr: bool,
    hdr_sdr_tone_map: bool,
    frame_warp: bool,
    party_audio: bool,
    input_deck: bool,
    quick_suspend: bool,
}

impl From<GatesJson> for Gates {
    fn from(g: GatesJson) -> Self {
        Gates {
            stream_dvr: g.stream_dvr,
            hdr_sdr_tone_map: g.hdr_sdr_tone_map,
            frame_warp: g.frame_warp,
            party_audio: g.party_audio,
            input_deck: g.input_deck,
            quick_suspend: g.quick_suspend,
        }
    }
}

/// What the client offers: the input of every dock request.
#[derive(Deserialize)]
struct Offers {
    offers: Vec<Offer>,
    /// The saved shortcuts' count; absent where shortcuts cannot be sent.
    #[serde(default)]
    shortcuts: Option<usize>,
    #[serde(default)]
    buttons: Buttons,
    #[serde(default)]
    gates: GatesJson,
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Request {
    /// The Control Center for these offers ([`dock`]).
    Dock {
        #[serde(flatten)]
        offers: Offers,
    },
    /// The module a trigger or a rail arrow lands on from `from` ([`Dock::step`]).
    Step {
        #[serde(flatten)]
        offers: Offers,
        from: String,
        forward: bool,
    },
    /// Where a controller button's tile is: `[module, card]`, or null ([`Dock::hotkey`]).
    Hotkey {
        #[serde(flatten)]
        offers: Offers,
        key: Hotkey,
    },
    /// `sheet` or `dock` ([`placement`]).
    Placement {
        form: String,
        width_dp: f32,
        height_dp: f32,
    },
    /// `blurred`, `baked`, `solid`, or null while closed ([`surface`]).
    Surface {
        open: bool,
        tier: String,
        #[serde(default)]
        blur_available: bool,
        #[serde(default)]
        power_save: bool,
        #[serde(default)]
        thermal_severe: bool,
        #[serde(default)]
        high_contrast: bool,
    },
}

/// The form factor of its JSON name (`phone`, `handheld`, `tv`, `desktop`).
pub fn form_factor(key: &str) -> Option<FormFactor> {
    Some(match key {
        "phone" => FormFactor::Phone,
        "handheld" => FormFactor::Handheld,
        "tv" => FormFactor::Tv,
        "desktop" => FormFactor::Desktop,
        _ => return None,
    })
}

fn invalid(detail: impl Into<String>) -> (&'static str, String) {
    ("invalid", detail.into())
}

fn built(offers: &Offers) -> Result<Dock, (&'static str, String)> {
    dock(
        &offers.offers,
        offers.shortcuts,
        &offers.buttons,
        &offers.gates.into(),
    )
    .map_err(|UnknownTile(id)| invalid(format!("unknown tile {id}")))
}

fn answer(request: Request) -> Result<Value, (&'static str, String)> {
    Ok(match request {
        Request::Dock { offers } => {
            serde_json::to_value(built(&offers)?).expect("a dock serializes")
        }
        Request::Step {
            offers,
            from,
            forward,
        } => {
            let from =
                Module::parse(&from).ok_or_else(|| invalid(format!("unknown module {from}")))?;
            json!(built(&offers)?.step(from, forward).key())
        }
        Request::Hotkey { offers, key } => match built(&offers)?.hotkey(key) {
            Some((module, card)) => json!([module.key(), card]),
            None => Value::Null,
        },
        Request::Placement {
            form,
            width_dp,
            height_dp,
        } => {
            let form = form_factor(&form).ok_or_else(|| invalid(format!("unknown form {form}")))?;
            serde_json::to_value(placement(form, width_dp, height_dp))
                .expect("a placement serializes")
        }
        Request::Surface {
            open,
            tier,
            blur_available,
            power_save,
            thermal_severe,
            high_contrast,
        } => {
            let tier = Tier::parse(&tier).ok_or_else(|| invalid(format!("unknown tier {tier}")))?;
            let live = Live {
                tier,
                blur_available,
                power_save,
                thermal_severe,
                high_contrast,
            };
            serde_json::to_value(surface(open, &live)).expect("a surface serializes")
        }
    })
}

/// Answers one request; a malformed one is an error answer, never a panic across JNI.
pub fn call(request: &str) -> String {
    let result = serde_json::from_str::<Request>(request)
        .map_err(|e| invalid(e.to_string()))
        .and_then(answer);
    match result {
        Ok(value) => json!({"ok": value}).to_string(),
        Err((code, detail)) => json!({"error": code, "detail": detail}).to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(request: &str) -> Value {
        let answer: Value = serde_json::from_str(&call(request)).unwrap();
        assert!(answer.get("error").is_none(), "{request}: {answer}");
        answer["ok"].clone()
    }

    fn error(request: &str) -> Value {
        let answer: Value = serde_json::from_str(&call(request)).unwrap();
        assert_eq!(answer["error"], "invalid", "{request}: {answer}");
        answer["detail"].clone()
    }

    #[test]
    fn a_dock_answers_its_pages_footer_and_buttons() {
        let dock = ok(
            r#"{"op":"dock","offers":[{"id":"microphone","on":true},{"id":"keyboard"},{"id":"disconnect"}],
            "buttons":{"y":"microphone"}}"#,
        );
        assert_eq!(dock["pages"][0]["module"], "quick");
        assert_eq!(dock["pages"][0]["cards"][0]["tile"]["id"], "keyboard");
        assert_eq!(dock["pages"][1]["module"], "audio");
        assert_eq!(
            dock["pages"][1]["cards"][0]["tile"],
            json!({"id": "microphone", "on": true, "hotkey": "y"})
        );
        assert_eq!(dock["footer"], json!(["close", "disconnect"]));
        assert_eq!(dock["buttons"]["y"], "microphone");
    }

    #[test]
    fn a_gated_card_shows_only_with_its_gate() {
        let closed = ok(r#"{"op":"dock","offers":[{"id":"save_clip"}]}"#);
        assert_eq!(closed["pages"].as_array().unwrap().len(), 1);
        let open = ok(r#"{"op":"dock","offers":[{"id":"save_clip"}],"gates":{"stream_dvr":true}}"#);
        assert_eq!(open["pages"][1]["module"], "clips");
    }

    #[test]
    fn steps_and_hotkeys_follow_the_dock() {
        let offers = r#""offers":[{"id":"keyboard"},{"id":"microphone"},{"id":"guests"}],"buttons":{"x":"microphone"}"#;
        assert_eq!(
            ok(&format!(
                r#"{{"op":"step",{offers},"from":"quick","forward":true}}"#
            )),
            "audio"
        );
        assert_eq!(
            ok(&format!(
                r#"{{"op":"step",{offers},"from":"guests","forward":true}}"#
            )),
            "guests"
        );
        assert_eq!(
            ok(&format!(r#"{{"op":"hotkey",{offers},"key":"x"}}"#)),
            json!(["audio", 0])
        );
        assert_eq!(
            ok(&format!(r#"{{"op":"hotkey",{offers},"key":"lb"}}"#)),
            Value::Null
        );
        assert!(
            error(&format!(
                r#"{{"op":"step",{offers},"from":"more","forward":true}}"#
            ))
            .as_str()
            .unwrap()
            .contains("more")
        );
    }

    #[test]
    fn placement_and_surface_answer_by_name() {
        assert_eq!(
            ok(r#"{"op":"placement","form":"phone","width_dp":393,"height_dp":852}"#),
            "sheet"
        );
        assert_eq!(
            ok(r#"{"op":"placement","form":"tv","width_dp":960,"height_dp":540}"#),
            "dock"
        );
        assert_eq!(
            ok(r#"{"op":"surface","open":true,"tier":"live","blur_available":true}"#),
            "blurred"
        );
        assert_eq!(
            ok(
                r#"{"op":"surface","open":true,"tier":"LENS","blur_available":true,"power_save":true}"#
            ),
            "baked"
        );
        assert_eq!(
            ok(r#"{"op":"surface","open":true,"tier":"baked","high_contrast":true}"#),
            "solid"
        );
        assert_eq!(
            ok(r#"{"op":"surface","open":false,"tier":"live"}"#),
            Value::Null
        );
        for form in ["phone", "handheld", "tv", "desktop"] {
            assert!(form_factor(form).is_some());
        }
    }

    #[test]
    fn a_bad_request_is_an_error_answer() {
        assert!(
            error(r#"{"op":"dock","offers":[{"id":"warp_drive"}]}"#)
                .as_str()
                .unwrap()
                .contains("warp_drive")
        );
        error(r#"{"op":"placement","form":"watch","width_dp":1,"height_dp":1}"#);
        error(r#"{"op":"surface","open":true,"tier":"glass"}"#);
        error(r#"{"op":"teleport"}"#);
        error("not json");
    }
}
