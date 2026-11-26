//! Button names in hints follow the device in the hand: Xbox by default, PlayStation symbols or
//! Switch labels when such a controller is attached, a keyboard's keys, a TV remote's OK. The
//! controller styles are ported from Moonlight V+ (streaming/input/gamepadglyphs.h and
//! SdlGamepadKeyNavigation::detectUiStyle).
//!
//! Both apps ask here on every input event, so a hint changes with the next press and never
//! animates: Android through `InputSource`, the desktop through `glyphs` and `input_source`. A
//! finger and a mouse get no hints at all.

/// Sony's USB vendor id: its controllers carry the PlayStation symbols.
pub const SONY_VENDOR: u16 = 0x054C;
/// Nintendo's USB vendor id: its controllers carry the Switch labels.
pub const NINTENDO_VENDOR: u16 = 0x057E;

/// The labels a controller carries.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Style {
    #[default]
    Xbox,
    PlayStation,
    Nintendo,
}

/// A face button by its position, as SDL names them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Face {
    South,
    East,
    West,
    North,
}

impl Style {
    /// The style of a controller from its USB vendor: anything but Sony and Nintendo reads as Xbox.
    pub fn of_vendor(vendor_id: u16) -> Style {
        match vendor_id {
            SONY_VENDOR => Style::PlayStation,
            NINTENDO_VENDOR => Style::Nintendo,
            _ => Style::Xbox,
        }
    }

    /// The style of several attached controllers: the first PlayStation or Switch one sets it.
    pub fn of_styles(styles: impl IntoIterator<Item = Style>) -> Style {
        styles.into_iter().find(|style| *style != Style::Xbox).unwrap_or(Style::Xbox)
    }

    /// The label on the face button at `face`: Switch labels follow its layout (south is B).
    pub fn face(self, face: Face) -> &'static str {
        match (self, face) {
            // V+ shows ✕ (U+2715); the bundled font has no such glyph, × (U+00D7) is the same mark.
            (Style::PlayStation, Face::South) => "×",
            (Style::PlayStation, Face::East) => "○",
            (Style::PlayStation, Face::West) => "□",
            (Style::PlayStation, Face::North) => "△",
            (Style::Nintendo, Face::South) => "B",
            (Style::Nintendo, Face::East) => "A",
            (Style::Nintendo, Face::West) => "Y",
            (Style::Nintendo, Face::North) => "X",
            (Style::Xbox, Face::South) => "A",
            (Style::Xbox, Face::East) => "B",
            (Style::Xbox, Face::West) => "X",
            (Style::Xbox, Face::North) => "Y",
        }
    }

    pub fn start(self) -> &'static str {
        match self {
            Style::PlayStation => "Options",
            Style::Nintendo => "+",
            Style::Xbox => "Start",
        }
    }

    pub fn select(self) -> &'static str {
        match self {
            Style::PlayStation => "Share",
            Style::Nintendo => "−",
            Style::Xbox => "Select",
        }
    }

    pub fn left_shoulder(self) -> &'static str {
        match self {
            Style::PlayStation => "L1",
            Style::Nintendo => "L",
            Style::Xbox => "LB",
        }
    }

    pub fn right_shoulder(self) -> &'static str {
        match self {
            Style::PlayStation => "R1",
            Style::Nintendo => "R",
            Style::Xbox => "RB",
        }
    }

    /// Buttons held together: "Start+Select", or "+ + −" where the names are + and −.
    pub fn chord(self, buttons: &[&str]) -> String {
        buttons.join(if self == Style::Nintendo { " + " } else { "+" })
    }

    /// The buttons the controller mouse clicks left and right with: the PC's A and B, which the
    /// Nintendo layout setting moves from south and east to east and south.
    pub fn mouse_clicks(self, swapped: bool) -> (&'static str, &'static str) {
        let (left, right) = if swapped { (Face::East, Face::South) } else { (Face::South, Face::East) };
        (self.face(left), self.face(right))
    }
}

/// What the last input came from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Source {
    /// A finger: no hints.
    #[default]
    Touch,
    /// A mouse or a trackpad's pointer: no hints.
    Mouse,
    Keyboard,
    /// A TV remote: a D-pad with OK and Back, and no face buttons.
    Remote,
    Pad(Style),
}

impl Source {
    /// Whether this source gets hints at all.
    pub fn hinted(self) -> bool {
        !matches!(self, Source::Touch | Source::Mouse)
    }
}

/// What a hint is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Hint {
    /// Does what has focus.
    Go,
    /// Opens the options of what has focus.
    More,
    /// Leaves the top layer.
    Back,
}

/// The key or button a hint names. A remote's and a keyboard's are words each app translates;
/// a controller's are the marks on the controller itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Glyph {
    /// A controller's button, by the label it carries.
    Button(&'static str),
    Enter,
    /// A letter or named key that reads the same in every language.
    Key(&'static str),
    Escape,
    RemoteOk,
    /// A remote has no Y: whatever is on a controller's Y also opens from a held OK.
    RemoteHoldOk,
    RemoteBack,
}

impl Glyph {
    /// The English text; the apps translate the words (`OK`, `Hold OK`, `Enter`, `Back`).
    pub fn en(self) -> &'static str {
        match self {
            Glyph::Button(label) | Glyph::Key(label) => label,
            Glyph::Enter => "Enter",
            Glyph::Escape => "Esc",
            Glyph::RemoteOk => "OK",
            Glyph::RemoteHoldOk => "Hold OK",
            Glyph::RemoteBack => "Back",
        }
    }

    /// A stable name for the JNI and for tests.
    pub fn key(self) -> &'static str {
        match self {
            Glyph::Button(_) => "button",
            Glyph::Key(_) => "key",
            Glyph::Enter => "enter",
            Glyph::Escape => "escape",
            Glyph::RemoteOk => "remote_ok",
            Glyph::RemoteHoldOk => "remote_hold_ok",
            Glyph::RemoteBack => "remote_back",
        }
    }
}

/// The key or button for `hint` on `source`; `None` for a finger or a mouse. With
/// `swap_face_buttons` (the Nintendo layout setting) the east button confirms and south goes
/// back, so the hint names those.
pub fn glyph(source: Source, hint: Hint, swap_face_buttons: bool) -> Option<Glyph> {
    match source {
        Source::Touch | Source::Mouse => None,
        Source::Keyboard => Some(match hint {
            Hint::Go => Glyph::Enter,
            Hint::More => Glyph::Key("F"),
            Hint::Back => Glyph::Escape,
        }),
        Source::Remote => Some(match hint {
            Hint::Go => Glyph::RemoteOk,
            Hint::More => Glyph::RemoteHoldOk,
            Hint::Back => Glyph::RemoteBack,
        }),
        Source::Pad(style) => {
            let (go, back) = if swap_face_buttons { (Face::East, Face::South) } else { (Face::South, Face::East) };
            Some(Glyph::Button(style.face(match hint {
                Hint::Go => go,
                Hint::More => Face::North,
                Hint::Back => back,
            })))
        }
    }
}

/// The languages Figtree's bundled Latin-1 files cover whole. Every other language draws the whole
/// interface in the system's face, so a word never mixes two fonts.
pub const FIGTREE_LANGUAGES: [&str; 6] = ["en", "de", "es", "fr", "it", "pt"];

/// Whether the interface draws in Figtree for `locale` (`de`, `pt-BR`, `pt_BR`, `zh-Hans`).
pub fn figtree_covers(locale: &str) -> bool {
    let language = locale.split(['-', '_']).next().unwrap_or_default();
    FIGTREE_LANGUAGES.iter().any(|covered| covered.eq_ignore_ascii_case(language))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(source: Source, hint: Hint) -> Option<&'static str> {
        glyph(source, hint, false).map(Glyph::en)
    }

    #[test]
    fn each_device_names_its_own_keys() {
        let xbox = Source::Pad(Style::of_vendor(0x045E));
        assert_eq!((text(xbox, Hint::Go), text(xbox, Hint::More)), (Some("A"), Some("Y")));
        let sony = Source::Pad(Style::of_vendor(SONY_VENDOR));
        assert_eq!((text(sony, Hint::Go), text(sony, Hint::More)), (Some("×"), Some("△")));
        let nintendo = Source::Pad(Style::of_vendor(NINTENDO_VENDOR));
        assert_eq!((text(nintendo, Hint::Go), text(nintendo, Hint::More)), (Some("B"), Some("X")));
        assert_eq!((text(Source::Keyboard, Hint::Go), text(Source::Keyboard, Hint::More), text(Source::Keyboard, Hint::Back)), (Some("Enter"), Some("F"), Some("Esc")));
        assert_eq!((text(Source::Remote, Hint::Go), text(Source::Remote, Hint::More)), (Some("OK"), Some("Hold OK")));
    }

    #[test]
    fn a_finger_and_a_mouse_get_no_hints() {
        for source in [Source::Touch, Source::Mouse] {
            assert!(!source.hinted());
            for hint in [Hint::Go, Hint::More, Hint::Back] {
                assert_eq!(glyph(source, hint, false), None);
                assert_eq!(glyph(source, hint, true), None);
            }
        }
        assert!(Source::Remote.hinted() && Source::Keyboard.hinted() && Source::Pad(Style::Xbox).hinted());
        assert_eq!(Source::default(), Source::Touch, "before any input there is nothing to name");
    }

    #[test]
    fn the_swap_setting_moves_go_and_back_and_leaves_more() {
        let nintendo = Source::Pad(Style::Nintendo);
        assert_eq!(glyph(nintendo, Hint::Go, true).map(Glyph::en), Some("A"));
        assert_eq!(glyph(nintendo, Hint::Back, true).map(Glyph::en), Some("B"));
        assert_eq!(glyph(nintendo, Hint::More, true).map(Glyph::en), Some("X"));
        let xbox = Source::Pad(Style::Xbox);
        assert_eq!(glyph(xbox, Hint::Go, true).map(Glyph::en), Some("B"));
        assert_eq!(glyph(xbox, Hint::Back, false).map(Glyph::en), Some("B"));
        // A keyboard and a remote have no such setting.
        assert_eq!(glyph(Source::Keyboard, Hint::Go, true), Some(Glyph::Enter));
        assert_eq!(glyph(Source::Remote, Hint::Back, true), Some(Glyph::RemoteBack));
    }

    #[test]
    fn the_first_playstation_or_switch_controller_sets_the_style() {
        assert_eq!(Style::of_styles([Style::Xbox, Style::Nintendo, Style::PlayStation]), Style::Nintendo);
        assert_eq!(Style::of_styles([Style::Xbox, Style::Xbox]), Style::Xbox);
        assert_eq!(Style::of_styles([]), Style::Xbox);
        assert_eq!(Style::of_vendor(0), Style::Xbox);
    }

    #[test]
    fn a_controllers_other_buttons_keep_their_names() {
        assert_eq!((Style::Xbox.start(), Style::PlayStation.start(), Style::Nintendo.start()), ("Start", "Options", "+"));
        assert_eq!((Style::Xbox.select(), Style::PlayStation.select(), Style::Nintendo.select()), ("Select", "Share", "−"));
        assert_eq!((Style::Xbox.left_shoulder(), Style::PlayStation.right_shoulder(), Style::Nintendo.left_shoulder()), ("LB", "R1", "L"));
        assert_eq!(Style::Xbox.chord(&["Start", "Select"]), "Start+Select");
        assert_eq!(Style::Nintendo.chord(&["+", "−"]), "+ + −");
        assert_eq!(Style::Nintendo.mouse_clicks(false), ("B", "A"));
        assert_eq!(Style::Nintendo.mouse_clicks(true), ("A", "B"));
        assert_eq!(Style::PlayStation.mouse_clicks(false), ("×", "○"));
    }

    #[test]
    fn every_glyph_has_a_name_and_english_text() {
        for glyph in [Glyph::Button("A"), Glyph::Key("F"), Glyph::Enter, Glyph::Escape, Glyph::RemoteOk, Glyph::RemoteHoldOk, Glyph::RemoteBack] {
            assert!(!glyph.en().is_empty() && !glyph.key().is_empty());
        }
    }

    #[test]
    fn figtree_draws_the_six_latin_1_languages_only() {
        for locale in ["en", "de", "es", "fr", "it", "pt-BR", "pt_BR", "EN-us"] {
            assert!(figtree_covers(locale), "{locale}");
        }
        for locale in ["pl", "tr", "vi", "ru", "uk", "ja", "ko", "zh-Hans", "zh_Hant", ""] {
            assert!(!figtree_covers(locale), "{locale}");
        }
    }
}
