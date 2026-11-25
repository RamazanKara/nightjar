//! OS system-font discovery (`skrifa`, `core-text`, Windows Fonts, Linux `fc-match`) and OS
//! reduced-motion preference registered into `nightjar-desktop-ui::theme`.

use egui::FontData;
#[cfg(any(target_os = "linux", test))]
use std::time::Instant;

use nightjar_desktop_config::strings::Lang;
use nightjar_desktop_ui::theme::{self, fallback_ahead, SystemFonts};

/// Registers the real OS font loader, face validator and reduced-motion reader in `theme`.
pub fn register() {
    theme::register_font_loader(system_fonts, is_usable_face);
    theme::register_reduces_motion_hook(os_reduces_motion);
}

#[cfg(windows)]
pub fn os_reduces_motion() -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{
        SystemParametersInfoW, SPI_GETCLIENTAREAANIMATION, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
    };
    let mut animations = 1i32;
    // SAFETY: the pointer is to a live BOOL-sized integer, which this action writes.
    let read = unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            Some((&mut animations as *mut i32).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };
    read.is_ok() && animations == 0
}

#[cfg(target_os = "macos")]
pub fn os_reduces_motion() -> bool {
    // AppKit exposes this display preference without asking for accessibility control.
    unsafe { objc2_app_kit::NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceMotion() }
}

/// Whether `org.freedesktop.appearance` `reduced-motion` (`1` = reduce) asks to reduce motion.
#[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
pub(crate) fn portal_reduces_motion(value: u32) -> bool {
    value == 1
}

#[cfg(target_os = "linux")]
pub fn os_reduces_motion() -> bool {
    if std::env::var_os("GTK_ENABLE_ANIMATIONS").is_some_and(|v| v == "0") {
        return true;
    }
    use std::time::Duration;
    use zbus::blocking::{connection::Builder, Proxy};
    use zbus::zvariant::OwnedValue;
    let read = (|| -> zbus::Result<OwnedValue> {
        let connection = Builder::session()?.method_timeout(Duration::from_millis(200)).build()?;
        let portal = Proxy::new_owned(
            connection,
            "org.freedesktop.portal.Desktop",
            "/org/freedesktop/portal/desktop",
            "org.freedesktop.portal.Settings",
        )?;
        portal.call("ReadOne", &("org.freedesktop.appearance", "reduced-motion"))
    })();
    read.ok().and_then(|v| u32::try_from(v).ok()).is_some_and(portal_reduces_motion)
}

/// Validates a loaded font face with `skrifa` so egui never panics on WOFF2 or broken indices.
pub fn is_usable_face(data: &FontData) -> bool {
    skrifa::FontRef::from_index((*data.font).as_ref(), data.index).is_ok()
}

/// Where a language's letters come from when Figtree has not got them.
struct Fallback {
    ahead: bool,
    regular: &'static [Face],
    semibold: &'static [Face],
    /// What to install, in [`faces::INSTALL`], when none is found.
    install: usize,
}

fn fallback(lang: Lang) -> Option<Fallback> {
    let (ahead, regular, semibold, install) = match lang {
        Lang::En | Lang::De | Lang::Es | Lang::Fr | Lang::It | Lang::PtBr => return None,
        Lang::Pl | Lang::Ru | Lang::Tr | Lang::Uk | Lang::Vi => {
            (fallback_ahead(lang).unwrap_or(true), faces::SANS, faces::SANS_SEMIBOLD, 0)
        }
        Lang::Ja => (false, faces::JA, faces::JA_SEMIBOLD, 1),
        Lang::Ko => (false, faces::KO, faces::KO_SEMIBOLD, 1),
        Lang::ZhHans => (false, faces::ZH_HANS, faces::ZH_HANS_SEMIBOLD, 1),
        Lang::ZhHant => (false, faces::ZH_HANT, faces::ZH_HANT_SEMIBOLD, 1),
    };
    Some(Fallback { ahead, regular, semibold, install })
}

/// The system faces for `lang`'s letters, the first found of each list. Says so in the log when
/// a language that needs one finds none: its text then shows in boxes or in a mix of weights.
pub fn system_fonts(lang: Lang) -> SystemFonts {
    let Some(fallback) = fallback(lang) else {
        return SystemFonts::default();
    };
    let regular = fallback.regular.iter().find_map(load);
    let semibold = fallback.semibold.iter().find_map(load);
    if regular.is_none() {
        eprintln!(
            "nightjar: no system font draws {}; its text will show as boxes or in mixed weights. Install {}",
            lang.name(),
            faces::INSTALL[fallback.install]
        );
    }
    SystemFonts { ahead: fallback.ahead, regular, semibold }
}

/// A face in Windows' font folder: its file, and the family and style in its name table, which
/// pick it out of a collection (.ttc).
#[cfg(windows)]
type Face = (&'static str, &'static str, &'static str);

/// A fontconfig pattern and the family the match must have: fc-match answers every pattern with
/// some font.
#[cfg(target_os = "linux")]
type Face = (&'static str, &'static str);

/// Windows 10 and 11 ship Segoe UI, Yu Gothic, Malgun Gothic, YaHei and JhengHei; Meiryo
/// comes with the optional Japanese Supplemental Fonts, so it is only the second choice. The UI cuts (Yu Gothic UI, YaHei UI, JhengHei UI) have
/// the tighter line height menus want.
#[cfg(windows)]
mod faces {
    use super::Face;
    pub const SANS: &[Face] = &[("segoeui.ttf", "Segoe UI", "Regular")];
    pub const SANS_SEMIBOLD: &[Face] = &[("seguisb.ttf", "Segoe UI Semibold", "Regular")];
    pub const JA: &[Face] = &[
        ("YuGothM.ttc", "Yu Gothic UI", "Regular"),
        ("meiryo.ttc", "Meiryo UI", "Regular"),
        ("meiryo.ttc", "Meiryo", "Regular"),
    ];
    pub const JA_SEMIBOLD: &[Face] = &[
        ("YuGothB.ttc", "Yu Gothic UI Semibold", "Regular"),
        ("meiryob.ttc", "Meiryo UI", "Bold"),
        ("meiryob.ttc", "Meiryo", "Bold"),
    ];
    pub const KO: &[Face] = &[("malgun.ttf", "Malgun Gothic", "Regular")];
    pub const KO_SEMIBOLD: &[Face] = &[("malgunbd.ttf", "Malgun Gothic", "Bold")];
    pub const ZH_HANS: &[Face] = &[("msyh.ttc", "Microsoft YaHei UI", "Regular"), ("msyh.ttc", "Microsoft YaHei", "Regular")];
    pub const ZH_HANS_SEMIBOLD: &[Face] = &[("msyhbd.ttc", "Microsoft YaHei UI", "Bold"), ("msyhbd.ttc", "Microsoft YaHei", "Bold")];
    pub const ZH_HANT: &[Face] = &[("msjh.ttc", "Microsoft JhengHei UI", "Regular"), ("msjh.ttc", "Microsoft JhengHei", "Regular")];
    pub const ZH_HANT_SEMIBOLD: &[Face] = &[("msjhbd.ttc", "Microsoft JhengHei UI", "Bold"), ("msjhbd.ttc", "Microsoft JhengHei", "Bold")];
    pub const INSTALL: [&str; 2] = [
        "Segoe UI (Windows' own font).",
        "the language's supplemental fonts (Settings > Apps > Optional features).",
    ];
}

#[cfg(target_os = "macos")]
type Face = &'static str;

#[cfg(target_os = "macos")]
mod faces {
    use super::Face;
    pub const SANS: &[Face] = &["HelveticaNeue", "ArialMT"];
    pub const SANS_SEMIBOLD: &[Face] = &["HelveticaNeue-Medium", "Arial-BoldMT"];
    pub const JA: &[Face] = &["HiraginoSans-W3"];
    pub const JA_SEMIBOLD: &[Face] = &["HiraginoSans-W6"];
    pub const KO: &[Face] = &["AppleSDGothicNeo-Regular"];
    pub const KO_SEMIBOLD: &[Face] = &["AppleSDGothicNeo-SemiBold"];
    pub const ZH_HANS: &[Face] = &["PingFangSC-Regular", "STHeitiSC-Light"];
    pub const ZH_HANS_SEMIBOLD: &[Face] = &["PingFangSC-Semibold", "STHeitiSC-Medium"];
    pub const ZH_HANT: &[Face] = &["PingFangTC-Regular", "STHeitiTC-Light"];
    pub const ZH_HANT_SEMIBOLD: &[Face] = &["PingFangTC-Semibold", "STHeitiTC-Medium"];
    pub const INSTALL: [&str; 2] = [
        "the system fonts in Font Book.",
        "the language's fonts in Font Book.",
    ];
}

#[cfg(target_os = "macos")]
fn load(&name: &Face) -> Option<FontData> {
    use skrifa::{string::StringId, MetadataProvider};
    let font = core_text::font::new_from_name(name, 14.0).ok()?;
    if font.postscript_name() != name {
        return None;
    }
    let bytes = std::fs::read(font.url()?.to_path()?).ok()?;
    let index = skrifa::raw::FileRef::new(&bytes).ok()?.fonts().position(|face| {
        face.is_ok_and(|face| {
            face.localized_strings(StringId::POSTSCRIPT_NAME)
                .any(|s| s.to_string() == name)
        })
    })?;
    face_data(bytes, index.try_into().ok()?)
}

/// Noto ships in most distributions (fonts-noto-core, fonts-noto-cjk); DejaVu is the older default.
#[cfg(target_os = "linux")]
mod faces {
    use super::Face;
    pub const SANS: &[Face] = &[("Noto Sans:style=Regular", "Noto Sans"), ("DejaVu Sans:style=Book", "DejaVu Sans")];
    pub const SANS_SEMIBOLD: &[Face] = &[("Noto Sans:style=SemiBold", "Noto Sans"), ("DejaVu Sans:style=Bold", "DejaVu Sans")];
    pub const JA: &[Face] = &[("Noto Sans CJK JP:style=Regular", "Noto Sans CJK JP"), ("Source Han Sans JP:style=Regular", "Source Han Sans JP")];
    pub const JA_SEMIBOLD: &[Face] = &[("Noto Sans CJK JP:style=Bold", "Noto Sans CJK JP"), ("Source Han Sans JP:style=Bold", "Source Han Sans JP")];
    pub const KO: &[Face] = &[("Noto Sans CJK KR:style=Regular", "Noto Sans CJK KR"), ("Source Han Sans KR:style=Regular", "Source Han Sans KR")];
    pub const KO_SEMIBOLD: &[Face] = &[("Noto Sans CJK KR:style=Bold", "Noto Sans CJK KR"), ("Source Han Sans KR:style=Bold", "Source Han Sans KR")];
    pub const ZH_HANS: &[Face] = &[("Noto Sans CJK SC:style=Regular", "Noto Sans CJK SC"), ("Source Han Sans SC:style=Regular", "Source Han Sans SC")];
    pub const ZH_HANS_SEMIBOLD: &[Face] = &[("Noto Sans CJK SC:style=Bold", "Noto Sans CJK SC"), ("Source Han Sans SC:style=Bold", "Source Han Sans SC")];
    pub const ZH_HANT: &[Face] = &[("Noto Sans CJK TC:style=Regular", "Noto Sans CJK TC"), ("Source Han Sans TC:style=Regular", "Source Han Sans TC")];
    pub const ZH_HANT_SEMIBOLD: &[Face] = &[("Noto Sans CJK TC:style=Bold", "Noto Sans CJK TC"), ("Source Han Sans TC:style=Bold", "Source Han Sans TC")];
    pub const INSTALL: [&str; 2] = [
        "Noto Sans or DejaVu Sans (fonts-noto-core, fonts-dejavu-core).",
        "Noto Sans CJK (fonts-noto-cjk, noto-fonts-cjk, google-noto-sans-cjk-fonts).",
    ];
}

#[cfg(windows)]
fn load(&(file, family, style): &Face) -> Option<FontData> {
    use skrifa::string::StringId;
    use skrifa::MetadataProvider;
    let windows = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
    let bytes = std::fs::read(std::path::Path::new(&windows).join("Fonts").join(file)).ok()?;
    let name = |font: &skrifa::FontRef, id| font.localized_strings(id).english_or_first().map(|s| s.to_string());
    let index = skrifa::raw::FileRef::new(&bytes)
        .ok()?
        .fonts()
        .position(|font| {
            font.is_ok_and(|font| {
                name(&font, StringId::FAMILY_NAME).as_deref() == Some(family)
                    && name(&font, StringId::SUBFAMILY_NAME).as_deref() == Some(style)
            })
        })?;
    face_data(bytes, u32::try_from(index).ok()?)
}

#[cfg(target_os = "linux")]
fn load(&(pattern, family): &Face) -> Option<FontData> {
    let out = fc_match(pattern)?;
    let mut lines = out.lines();
    let (file, index, families) = (lines.next()?, lines.next()?, lines.next()?);
    if !families.split(',').any(|f| f == family) {
        return None;
    }
    face_data(std::fs::read(file).ok()?, index.parse().ok()?)
}

/// fc-match's answer for `pattern`, or None when it fails or takes over two seconds (a cold
/// font cache on a slow disk): the language then shows without its face rather than waiting.
#[cfg(target_os = "linux")]
fn fc_match(pattern: &str) -> Option<String> {
    use std::io::Read;
    let mut child = nightjar_desktop_media::amd_decode::for_child(&mut std::process::Command::new("fc-match"))
        .args(["--format=%{file}\n%{index}\n%{family}\n", pattern])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    let deadline = Instant::now() + std::time::Duration::from_secs(2);
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => break,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(std::time::Duration::from_millis(10)),
            Ok(None) => {
                eprintln!("nightjar: fc-match did not answer for {pattern:?} within 2 s");
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            _ => return None,
        }
    }
    let mut out = String::new();
    child.stdout.take()?.read_to_string(&mut out).ok()?;
    Some(out)
}

/// `bytes` as egui's font, or None when egui could not parse face `index` (it panics on one).
/// fontconfig gives a variable font's named instance in the upper 16 bits of the index,
/// `(instance + 1) << 16 | face`: the instance becomes the face's axis coordinates.
fn face_data(bytes: Vec<u8>, index: u32) -> Option<FontData> {
    use skrifa::MetadataProvider;
    let (face, instance) = (index & 0xFFFF, index >> 16);
    let font = skrifa::FontRef::from_index(&bytes, face).ok()?;
    let coords = instance
        .checked_sub(1)
        .and_then(|i| font.named_instances().get(usize::try_from(i).ok()?))
        .map(|named| {
            egui::epaint::text::VariationCoords::new(
                font.axes().iter().map(|axis| axis.tag()).zip(named.user_coords()),
            )
        });
    let mut data = FontData::from_owned(bytes);
    data.index = face;
    if let Some(coords) = coords {
        data.tweak.coords = coords;
    }
    Some(data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nightjar_desktop_ui::theme::{
        apply, definitions, fonts_for, fonts_key, regular, semibold, text, FIGTREE,
        FIGTREE_SEMIBOLD, LOADED, LOADING,
    };

    /// Whether the face at `index` in `bytes` maps `c` to a glyph.
    fn covers(bytes: &[u8], index: u32, c: char) -> bool {
        use skrifa::MetadataProvider;
        skrifa::FontRef::from_index(bytes, index).is_ok_and(|font| font.charmap().map(c).is_some())
    }

    fn figtree_covers(c: char) -> bool {
        covers(FIGTREE, 0, c) && covers(FIGTREE_SEMIBOLD, 0, c)
    }

    /// Scripts written with spaces between words, where one word may not mix two faces.
    fn alphabetic_script(c: char) -> bool {
        matches!(u32::from(c), 0x0041..=0x024F | 0x0370..=0x052F | 0x1E00..=0x1EFF)
    }

    #[test]
    fn a_language_drawn_with_figtree_first_has_no_letter_figtree_lacks() {
        for lang in Lang::all() {
            let ahead = fallback(lang).is_some_and(|f| f.ahead);
            let lacking: std::collections::BTreeSet<char> = lang
                .texts()
                .flat_map(str::chars)
                .filter(|c| alphabetic_script(*c) && !figtree_covers(*c))
                .collect();
            if ahead {
                assert!(
                    !lacking.is_empty(),
                    "{lang:?} puts a system face ahead of Figtree without needing one"
                );
            } else {
                assert!(
                    lacking.is_empty(),
                    "{lang:?} needs a system face ahead of Figtree for {lacking:?}"
                );
            }
        }
    }

    #[test]
    fn every_text_of_every_language_has_its_glyphs() {
        register();
        let mut checked = 0;
        for lang in Lang::all() {
            let system = system_fonts(lang);
            if fallback(lang).is_some() && system.regular.is_none() {
                eprintln!("skipped {lang:?}: its system font is not installed here");
                continue;
            }
            if let Some(face) = system.regular.as_ref().filter(|_| system.ahead) {
                let bytes = (*face.font).as_ref();
                let lacking: std::collections::BTreeSet<char> = lang
                    .texts()
                    .flat_map(str::chars)
                    .filter(|c| c.is_alphabetic() && !covers(bytes, face.index, *c))
                    .collect();
                assert!(lacking.is_empty(), "{lang:?}'s system face lacks {lacking:?}");
            }
            let ctx = egui::Context::default();
            ctx.set_fonts(definitions(system));
            ctx.run_ui(egui::RawInput::default(), |_| {}).textures_delta.clear();
            for font in [regular(text::BODY), semibold(text::BODY)] {
                let missing: std::collections::BTreeSet<char> = lang
                    .texts()
                    .chain([lang.name()])
                    .flat_map(str::chars)
                    .filter(|c| !c.is_control() && !ctx.fonts_mut(|f| f.has_glyph(&font, *c)))
                    .collect();
                assert!(missing.is_empty(), "{lang:?} draws {missing:?} as boxes in {font:?}");
            }
            checked += 1;
        }
        assert!(checked >= Lang::all().filter(|l| fallback(*l).is_none()).count());
    }

    #[test]
    fn a_face_egui_cannot_read_is_dropped_instead_of_panicking() {
        register();
        let mut bad_index = FontData::from_static(FIGTREE);
        bad_index.index = 7;
        let system = SystemFonts {
            ahead: true,
            regular: Some(FontData::from_owned(b"not a font".to_vec())),
            semibold: Some(bad_index),
        };
        let ctx = egui::Context::default();
        ctx.set_fonts(definitions(system));
        ctx.run_ui(egui::RawInput::default(), |_| {}).textures_delta.clear();
        assert!(ctx.fonts_mut(|f| {
            f.has_glyph(&regular(text::BODY), 'A') && f.has_glyph(&semibold(text::BODY), 'A')
        }));
    }

    #[test]
    fn fontconfig_s_named_instance_index_picks_the_face_and_its_axes() {
        assert_eq!(face_data(FIGTREE.to_vec(), 0).map(|d| d.index), Some(0));
        assert_eq!(face_data(FIGTREE.to_vec(), 1 << 16).map(|d| d.index), Some(0));
        assert!(face_data(FIGTREE.to_vec(), 3).is_none());
        assert!(face_data(b"wOF2 not really".to_vec(), 0).is_none());
    }

    #[test]
    fn a_language_s_faces_load_off_the_ui_thread() {
        register();
        let ctx = egui::Context::default();
        apply(&ctx);
        let started = Instant::now();
        fonts_for(&ctx, Lang::Ja);
        while ctx.data(|d| d.get_temp::<Lang>(fonts_key())) != Some(Lang::Ja) {
            assert!(started.elapsed().as_secs() < 30, "the Japanese fonts never loaded");
            std::thread::sleep(std::time::Duration::from_millis(10));
            fonts_for(&ctx, Lang::Ja);
        }
        fonts_for(&ctx, Lang::En);
        assert!(!LOADED.lock().unwrap().iter().any(|e| e.0 == Lang::Ja));
        assert!(!LOADING.lock().unwrap().contains(&Lang::Ja));
    }

    #[test]
    fn linux_portal_reduced_motion_value_maps_one_only() {
        assert!(!portal_reduces_motion(0));
        assert!(portal_reduces_motion(1));
        assert!(!portal_reduces_motion(2));
    }
}
