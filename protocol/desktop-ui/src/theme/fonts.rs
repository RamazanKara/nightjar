//! Figtree font definitions and asynchronous per-language system fallback font loading.

use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use egui::{FontData, FontDefinitions, FontFamily, FontId};

use crate::strings::Lang;
use super::{high_contrast, set_look, FIGTREE, FIGTREE_SEMIBOLD};

pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("semibold".into()))
}

pub fn regular(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}


/// Additional script fallback faces needed for game titles or PC names whose script differs
/// from the interface language.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub struct ScriptSet(u8);

impl ScriptSet {
    const SANS: u8 = 1 << 0;
    const JA: u8 = 1 << 1;
    const KO: u8 = 1 << 2;
    const ZH_HANS: u8 = 1 << 3;
    const ZH_HANT: u8 = 1 << 4;

    pub fn merge(&mut self, other: Self) {
        self.0 |= other.0;
    }

    pub fn without_primary(self, lang: Lang) -> Self {
        let covered = match lang {
            Lang::En | Lang::De | Lang::Es | Lang::Fr | Lang::It | Lang::PtBr => 0,
            Lang::Pl | Lang::Ru | Lang::Tr | Lang::Uk | Lang::Vi => Self::SANS,
            Lang::Ja => Self::JA | Self::ZH_HANS | Self::ZH_HANT,
            Lang::Ko => Self::KO,
            Lang::ZhHans => Self::ZH_HANS,
            Lang::ZhHant => Self::ZH_HANT,
        };
        Self(self.0 & !covered)
    }

    fn contains_lang(self, lang: Lang) -> bool {
        let bit = match lang {
            Lang::Pl => Self::SANS,
            Lang::Ja => Self::JA,
            Lang::Ko => Self::KO,
            Lang::ZhHans => Self::ZH_HANS,
            Lang::ZhHant => Self::ZH_HANT,
            _ => return false,
        };
        self.0 & bit != 0
    }

    fn items(self) -> impl Iterator<Item = (u8, Lang)> {
        [
            (Self::JA, Lang::Ja),
            (Self::KO, Lang::Ko),
            (Self::ZH_HANS, Lang::ZhHans),
            (Self::ZH_HANT, Lang::ZhHant),
            (Self::SANS, Lang::Pl),
        ]
        .into_iter()
        .filter(move |(bit, _)| self.0 & *bit != 0)
    }
}

/// Detects non-Latin-1 scripts in `text` (PC names and game titles) so their fallback faces
/// load even when the interface is in another language.
pub fn scripts_for_text(text: &str) -> ScriptSet {
    if text.is_ascii() {
        return ScriptSet::default();
    }
    let mut mask = 0u8;
    let mut has_han = false;
    for c in text.chars() {
        match u32::from(c) {
            0x3040..=0x30FF | 0x31F0..=0x31FF | 0xFF65..=0xFF9F => mask |= ScriptSet::JA,
            0x1100..=0x11FF | 0x3130..=0x318F | 0xAC00..=0xD7AF => mask |= ScriptSet::KO,
            0x3100..=0x312F | 0x31A0..=0x31BF => mask |= ScriptSet::ZH_HANT,
            0x4E00..=0x9FFF | 0x3400..=0x4DBF | 0xF900..=0xFAFF | 0x20000..=0x2FA1F => {
                has_han = true;
            }
            code if code > 0x00FF && c.is_alphabetic() => mask |= ScriptSet::SANS,
            _ => {}
        }
    }
    if has_han && (mask & (ScriptSet::JA | ScriptSet::ZH_HANT | ScriptSet::ZH_HANS)) == 0 {
        mask |= ScriptSet::ZH_HANS;
    }
    ScriptSet(mask)
}

/// The language whose system faces the context's fonts hold ([`fonts_for`]).
pub fn fonts_key() -> egui::Id {
    egui::Id::unique("nightjar-fonts-for")
}

/// The extra script faces active alongside the interface language's own faces.
pub fn extra_scripts_key() -> egui::Id {
    egui::Id::unique("nightjar-extra-scripts")
}

/// Figtree and egui's fonts, and the style. [`fonts_for`] adds the system faces a language needs.
pub fn apply(ctx: &egui::Context) {
    ctx.set_fonts(definitions(SystemFonts::default()));
    ctx.data_mut(|d| {
        d.remove::<Lang>(fonts_key());
        d.remove::<ScriptSet>(extra_scripts_key());
    });
    set_look(ctx, high_contrast(), false);
}

/// Puts the system faces that draw `lang`'s letters beside Figtree. Cheap unless the language
/// changed since the last call, so it runs every frame. The faces load on a thread the first
/// time (a CJK file is 12-36 MB, Linux asks fc-match); until then the fonts in use stay.
pub fn fonts_for(ctx: &egui::Context, lang: Lang) {
    fonts_for_scripts(ctx, lang, ScriptSet::default());
}

/// [`fonts_for`], plus system fallback faces for any non-Latin-1 scripts present in `scripts`
/// (e.g. CJK or Cyrillic game titles shown in an English interface).
pub fn fonts_for_scripts(ctx: &egui::Context, lang: Lang, scripts: ScriptSet) {
    let wanted = scripts.without_primary(lang);
    let applied_lang = ctx.data(|d| d.get_temp::<Lang>(fonts_key()));
    let applied_scripts = ctx.data(|d| d.get_temp::<ScriptSet>(extra_scripts_key()));
    if applied_lang == Some(lang) && applied_scripts == Some(wanted) {
        return;
    }
    let Some(system) = loaded(ctx, lang) else {
        // The loader requests a repaint too, but the launcher's loop sleeps until an SDL event.
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
        return;
    };
    let mut extras = Vec::new();
    let mut ready = ScriptSet::default();
    let mut pending = false;
    for (bit, extra_lang) in wanted.items() {
        if let Some(sys) = loaded(ctx, extra_lang) {
            extras.push(sys);
            ready.0 |= bit;
        } else {
            pending = true;
        }
    }
    if pending {
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
    if applied_lang != Some(lang) || applied_scripts != Some(ready) {
        ctx.data_mut(|d| {
            d.insert_temp(fonts_key(), lang);
            d.insert_temp(extra_scripts_key(), ready);
        });
        ctx.set_fonts(definitions_with_extras(system, &extras));
    }
    // egui holds the faces in use now; unrelated languages' files (up to 38 MB each) go.
    if !pending {
        LOADED
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .retain(|e| e.0 == lang || wanted.contains_lang(e.0));
    }
}

/// The system faces loaded for a language and not yet replaced by another's.
pub static LOADED: Mutex<Vec<(Lang, SystemFonts)>> = Mutex::new(Vec::new());
/// The languages a thread loads faces for.
pub static LOADING: Mutex<Vec<Lang>> = Mutex::new(Vec::new());

/// Takes a language off `LOADING` when its loader ends, panicking or not. A loader that panicked
/// leaves the language without system faces, so the UI stops waiting instead of retrying.
pub struct Loading(pub Lang);

impl Drop for Loading {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!("nightjar: loading the {} fonts failed; drawing without them", self.0.name());
            LOADED.lock().unwrap_or_else(PoisonError::into_inner).push((self.0, SystemFonts::default()));
        }
        LOADING.lock().unwrap_or_else(PoisonError::into_inner).retain(|l| *l != self.0);
    }
}

/// `lang`'s system faces when they are loaded; else starts loading them and returns None.
/// Returns empty defaults immediately when no OS font loader is registered (for headless UI tests).
pub(super) fn loaded(ctx: &egui::Context, lang: Lang) -> Option<SystemFonts> {
    loaded_with(ctx, lang, FONT_LOADER.get().copied())
}

pub(super) fn loaded_with(
    ctx: &egui::Context,
    lang: Lang,
    loader: Option<(FontLoaderFn, FontValidateFn)>,
) -> Option<SystemFonts> {
    if !needs_fallback(lang) && loader.is_none() {
        return Some(SystemFonts::default());
    }
    if let Some((_, system)) = LOADED.lock().unwrap_or_else(PoisonError::into_inner).iter().find(|e| e.0 == lang) {
        return Some(system.clone());
    }
    let Some((loader_fn, _)) = loader else {
        return loaded_on_thread(ctx, lang, |_| SystemFonts::default());
    };
    loaded_on_thread(ctx, lang, loader_fn)
}

fn loaded_on_thread(ctx: &egui::Context, lang: Lang, loader_fn: FontLoaderFn) -> Option<SystemFonts> {
    let mut loading = LOADING.lock().unwrap_or_else(PoisonError::into_inner);
    if !loading.contains(&lang) {
        loading.push(lang);
        let ctx = ctx.clone();
        let spawned = std::thread::Builder::new().name("nightjar-fonts".into()).spawn(move || {
            let _loading = Loading(lang);
            let system = loader_fn(lang);
            LOADED.lock().unwrap_or_else(PoisonError::into_inner).push((lang, system));
            ctx.request_repaint();
        });
        if let Err(e) = spawned {
            eprintln!("nightjar: could not start the font loader ({e}); loading the {} fonts here", lang.name());
            loading.retain(|l| *l != lang);
            let system = loader_fn(lang);
            LOADED.lock().unwrap_or_else(PoisonError::into_inner).push((lang, system.clone()));
            return Some(system);
        }
    }
    None
}

/// Figtree has Latin-1 only: no Cyrillic, no Polish, Turkish or Vietnamese letters, no CJK.
/// egui's own Ubuntu-Light draws some of them in a lighter weight, and none of CJK.
#[derive(Clone, Default)]
pub struct SystemFonts {
    /// Ahead of Figtree, so each word comes from one face: the language's words mix Figtree's
    /// letters with ones it lacks (Polish, Turkish, Vietnamese) or are Cyrillic, whose digits
    /// and punctuation would otherwise be Figtree's.
    pub ahead: bool,
    pub regular: Option<FontData>,
    pub semibold: Option<FontData>,
    pub emoji: Option<FontData>,
}

pub type FontLoaderFn = fn(Lang) -> SystemFonts;
pub type FontValidateFn = fn(&FontData) -> bool;

static FONT_LOADER: OnceLock<(FontLoaderFn, FontValidateFn)> = OnceLock::new();

/// Registers the OS/skrifa font loader and validator implemented in `nightjar-desktop::font_loader`.
pub fn register_font_loader(loader: FontLoaderFn, validate: FontValidateFn) {
    let _ = FONT_LOADER.set((loader, validate));
}

/// Whether `lang` needs a system fallback face (ahead of Figtree = true, behind = false, none = None).
pub fn fallback_ahead(lang: Lang) -> Option<bool> {
    match lang {
        Lang::En | Lang::De | Lang::Es | Lang::Fr | Lang::It | Lang::PtBr => None,
        Lang::Pl | Lang::Ru | Lang::Tr | Lang::Uk | Lang::Vi => Some(true),
        Lang::Ja | Lang::Ko | Lang::ZhHans | Lang::ZhHant => Some(false),
    }
}

pub fn needs_fallback(lang: Lang) -> bool {
    fallback_ahead(lang).is_some()
}

pub fn definitions(system: SystemFonts) -> FontDefinitions {
    definitions_with_extras(system, &[])
}

pub fn definitions_with_extras(system: SystemFonts, extras: &[SystemFonts]) -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert("figtree".into(), Arc::new(FontData::from_static(FIGTREE)));
    fonts.font_data.insert("figtree-semibold".into(), Arc::new(FontData::from_static(FIGTREE_SEMIBOLD)));
    let egui_fonts = fonts
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    // Drop unparseable font faces if a validator is registered (`font_loader` checks via `skrifa`).
    let usable = |data: Option<FontData>| {
        data.filter(|data| {
            let ok = FONT_LOADER.get().is_none_or(|(_, validate)| validate(data));
            if !ok {
                eprintln!("nightjar: a system font (face {}) cannot be read; drawing without it", data.index);
            }
            ok
        })
    };
    let (regular, semibold) = (usable(system.regular), usable(system.semibold));
    let emoji = usable(system.emoji)
        .or_else(|| extras.iter().find_map(|extra| usable(extra.emoji.clone())))
        .map(|data| {
            fonts.font_data.insert("system-emoji".into(), Arc::new(data));
            "system-emoji".to_owned()
        });
    let semibold = semibold.or_else(|| regular.clone());
    let regular = regular.map(|data| {
        fonts.font_data.insert("system".into(), Arc::new(data));
        "system".to_owned()
    });
    let semibold = semibold.map(|data| {
        fonts.font_data.insert("system-semibold".into(), Arc::new(data));
        "system-semibold".to_owned()
    });
    let mut extra_regular = Vec::new();
    let mut extra_semibold = Vec::new();
    for (idx, extra) in extras.iter().enumerate() {
        let reg = usable(extra.regular.clone());
        let semi = usable(extra.semibold.clone()).or_else(|| reg.clone());
        if let Some(data) = reg {
            let name = format!("system-extra-{idx}");
            fonts.font_data.insert(name.clone(), Arc::new(data));
            extra_regular.push(name);
        }
        if let Some(data) = semi {
            let name = format!("system-extra-{idx}-semibold");
            fonts.font_data.insert(name.clone(), Arc::new(data));
            extra_semibold.push(name);
        }
    }
    let family = |figtree: &str, system_face: Option<&String>, extra_faces: &[String]| {
        let mut list: Vec<String> = Vec::new();
        if system.ahead {
            list.extend(system_face.cloned());
        }
        list.push(figtree.to_owned());
        if !system.ahead {
            list.extend(system_face.cloned());
        }
        list.extend(extra_faces.iter().cloned());
        list.extend(emoji.iter().cloned());
        list.extend(egui_fonts.iter().cloned());
        list.push("Hack".into());
        list
    };
    let proportional = family("figtree", regular.as_ref(), &extra_regular);
    let bold = family("figtree-semibold", semibold.as_ref(), &extra_semibold);
    fonts.families.insert(FontFamily::Proportional, proportional);
    fonts.families.insert(FontFamily::Name("semibold".into()), bold);
    fonts
}
