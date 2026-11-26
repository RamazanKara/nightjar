//! Property tests for the Launch Curtain state (0x5548).

use super::*;
use crate::launch_curtain::{
    CurtainState, MAX_LAUNCHER_BYTES, MAX_REASON_BYTES, MAX_TITLE_BYTES, Phase, PromptKind,
};

fn text(max: usize) -> impl Strategy<Value = String> {
    prop_oneof![
        Just(String::new()),
        "[a-z]{1,12}",
        ".{0,40}",
        Just("ä".repeat(max / 2)),
    ]
    .prop_map(move |text: String| crate::launch_curtain::clip(&text, max).into())
}

fn seeds() -> impl Strategy<Value = Vec<u8>> {
    (
        (any::<u8>(), any::<u8>(), any::<u8>()),
        (any::<u32>(), any::<u32>()),
        (
            text(MAX_LAUNCHER_BYTES),
            text(MAX_TITLE_BYTES),
            text(MAX_REASON_BYTES),
        ),
    )
        .prop_map(
            |((phase, flags, prompt), (sequence, elapsed_ms), (launcher, title, reason))| {
                CurtainState {
                    phase: Phase::from_u8(phase),
                    flags,
                    prompt: PromptKind::from_u8(prompt),
                    sequence,
                    elapsed_ms,
                    launcher,
                    title,
                    reason,
                }
                .encode()
                .unwrap()
            },
        )
}

round_trip!(launch_curtain, seeds(), |b| CurtainState::decode(b), |v| v
    .encode());
