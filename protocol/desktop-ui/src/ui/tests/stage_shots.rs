//! Screenshots of the stage and what stands on it without art, in each look and window:
//! `NIGHTJAR_SHOTS=<dir> cargo test -p nightjar-desktop-ui --lib stage_screenshots -- --ignored`.
//! `NIGHTJAR_SHOTS_ONLY=<part of a name>[,<another>]` writes only the files whose name has one.

use super::*;
use crate::config::Theme;

/// The windows: the one the design is drawn in, the smallest SDL allows, and an ultrawide.
const WINDOWS: [(&str, [f32; 2], f32); 3] = [("1280x800", [1280.0, 800.0], 1.5), ("small", [720.0, 480.0], 1.5), ("wide", [2560.0, 1080.0], 1.0)];
const LOOKS: [(&str, Theme, bool); 4] = [("night", Theme::Dark, false), ("day", Theme::Light, false), ("oled", Theme::Oled, false), ("high-contrast", Theme::Dark, true)];

/// A launcher in `look` and `window` over `records`, searching the network as a fresh install does.
fn stage_frames(look: (Theme, bool), window: ([f32; 2], f32), console_home: bool, records: Vec<HostRecord>) -> Frames {
    crate::theme::own_palette();
    let ctx = egui::Context::default();
    crate::theme::apply(&ctx);
    let mut l = launcher(console_home, false);
    l.identity_ready = true;
    l.settings.theme = look.0;
    l.settings.high_contrast = look.1;
    if !console_home {
        l.go(Screen::Hosts);
    }
    let mut hosts = Hosts::without_discovery(records, Arc::new(|| {}));
    hosts.discovering = true;
    let mut f = Frames::over(ctx, l, hosts);
    f.size = window.0.into();
    f.native_ppp = window.1;
    f.shots = true;
    f
}

/// A page of PCs in every state a card has: no art anywhere, so the stage is all there is behind.
fn pcs(look: (Theme, bool), window: ([f32; 2], f32)) -> Frames {
    let records = vec![
        HostRecord { address: "192.0.2.10".into(), ..record("Living Room PC", true, true) },
        HostRecord { address: "192.0.2.11".into(), ..record("Office PC", true, true) },
        HostRecord { address: "192.0.2.12".into(), ..record("Studio", false, false) },
        HostRecord { address: "192.0.2.13".into(), ..record("Laptop", true, false) },
    ];
    let mut f = stage_frames(look, window, false, records);
    f.hosts.list[0].reach = Reach::Online;
    f.hosts.list[0].info = Some(answer(true, 0));
    f.hosts.list[1].reach = Reach::Offline;
    f.hosts.list[2].reach = Reach::Online;
    f.hosts.list[2].info = Some(answer(false, 0));
    f.hosts.list[3].reach = Reach::Unknown;
    f
}

/// The first run with these PCs found on the network, none of them paired yet.
fn found(look: (Theme, bool), window: ([f32; 2], f32), names: &[&str]) -> Frames {
    let records = names.iter().enumerate().map(|(n, name)| HostRecord { address: format!("192.0.2.{}", 12 + n), ..record(name, false, false) }).collect();
    let mut f = stage_frames(look, window, true, records);
    for host in &mut f.hosts.list {
        host.reach = Reach::Online;
        host.info = Some(answer(false, 0));
    }
    f
}

/// A paired PC's details page, the PC reporting its frame limiter.
fn details_page(look: (Theme, bool), window: ([f32; 2], f32)) -> Frames {
    let limiter = "<FrameLimiterSupported>1</FrameLimiterSupported><FrameLimiterEnabled>1</FrameLimiterEnabled>\
        <VirtualDisplayFrameLimiterEnabled>0</VirtualDisplayFrameLimiterEnabled><FrameLimiterFpsLimitMilliHz>119880</FrameLimiterFpsLimitMilliHz>";
    let mut f = stage_frames(look, window, false, vec![HostRecord { address: "192.0.2.10".into(), ..record("Living Room PC", true, true) }]);
    f.hosts.list[0].reach = Reach::Online;
    f.hosts.list[0].info = Some(host_answer("SUNSHINE_SERVER_FREE", &format!("{RUBYLIGHT}{limiter}")));
    let id = f.hosts.list[0].id;
    f.l.open(Screen::HostDetails(id));
    f
}

#[test]
#[ignore = "writes PNGs; set NIGHTJAR_SHOTS"]
fn stage_screenshots() {
    let Some(dir) = std::env::var_os("NIGHTJAR_SHOTS").map(std::path::PathBuf::from) else { return };
    std::fs::create_dir_all(&dir).expect("the folder for the screenshots");
    let only = std::env::var("NIGHTJAR_SHOTS_ONLY").unwrap_or_default();
    let shoot = |f: &mut Frames, name: String| {
        if !only.split(',').any(|part| name.contains(part)) {
            return;
        }
        // Past every fade and spring: what the page looks like at rest.
        for _ in 0..6 {
            f.settle();
        }
        f.shoot(&dir, &name);
    };
    for (look_name, theme, high) in LOOKS {
        for (window_name, size, ppp) in WINDOWS {
            let (look, window) = ((theme, high), (size, ppp));
            let named = |page: &str| format!("stage-{page}-{look_name}-{window_name}");
            shoot(&mut pcs(look, window), named("pcs"));
            shoot(&mut details_page(look, window), named("pc-details"));
            shoot(&mut stage_frames(look, window, true, Vec::new()), named("first-run-scanning"));
            let mut stopped = stage_frames(look, window, true, Vec::new());
            stopped.hosts.discovering = false;
            shoot(&mut stopped, named("first-run-no-search"));
            shoot(&mut found(look, window, &["Living Room PC"]), named("first-run-found"));
            shoot(&mut found(look, window, &["Living Room PC", "Office PC", "Studio"]), named("first-run-found-three"));
            // Pairing with the PC the first run found: its PIN, then the PC turning it down.
            let mut pairing = found(look, window, &["Living Room PC"]);
            let id = pairing.hosts.list[0].id;
            pairing.l.dialog = Dialog::Pairing { host: id, pin: "4071".into() };
            pairing.l.focus_dialog = true;
            shoot(&mut pairing, named("pairing-pin"));
            let mut failed = found(look, window, &["Living Room PC"]);
            failed.l.pair_failed(id, "PIN rejected. Generate a new one-time PIN in the PC’s web UI and check the passphrase.", false);
            shoot(&mut failed, named("pairing-failed"));
        }
    }
}
