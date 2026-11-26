//! SDL3 IME composition (`SDL_EVENT_TEXT_EDITING`), commit (`SDL_EVENT_TEXT_INPUT`), and
//! candidate-window positioning (`SDL_SetTextInputArea`) for egui text fields.

use egui::{Event, ImeEvent};
use sdl3_sys::everything::*;

/// Reads an SDL text-editing preedit event into egui [`ImeEvent::Preedit`].
pub(super) unsafe fn handle_text_editing(
    composing: &mut bool,
    events: &mut Vec<Event>,
    edit: &SDL_TextEditingEvent,
) {
    let text = if edit.text.is_null() {
        String::new()
    } else {
        // SAFETY: SDL guarantees `edit.text` is NUL-terminated when non-null.
        unsafe { std::ffi::CStr::from_ptr(edit.text) }
            .to_string_lossy()
            .into_owned()
    };
    let chars = text.chars().count();
    let active_range_chars = (edit.start >= 0 && edit.length > 0).then(|| {
        let start = (edit.start as usize).min(chars);
        let end = start.saturating_add(edit.length as usize).min(chars);
        start..end
    });
    if text.is_empty() {
        if *composing {
            *composing = false;
            events.push(Event::Ime(ImeEvent::Preedit {
                text,
                active_range_chars: None,
            }));
        }
    } else {
        *composing = true;
        events.push(Event::Ime(ImeEvent::Preedit {
            text,
            active_range_chars,
        }));
    }
}

/// Reads an SDL committed-text event into egui [`ImeEvent::Commit`] (during IME composition) or
/// [`Event::Text`]. Returns `false` if SDL delivered a null text pointer.
pub(super) unsafe fn handle_text_input(
    composing: &mut bool,
    events: &mut Vec<Event>,
    input: &SDL_TextInputEvent,
) -> bool {
    if input.text.is_null() {
        return false;
    }
    // SAFETY: SDL guarantees `input.text` is NUL-terminated when non-null.
    let text = unsafe { std::ffi::CStr::from_ptr(input.text) }
        .to_string_lossy()
        .into_owned();
    let was_composing = std::mem::take(composing);
    if !text.chars().any(char::is_control) {
        events.push(if was_composing {
            Event::Ime(ImeEvent::Commit(text))
        } else {
            Event::Text(text)
        });
    } else if was_composing {
        events.push(Event::Ime(ImeEvent::Preedit {
            text: String::new(),
            active_range_chars: None,
        }));
    }
    true
}

/// Clears any open IME composition (for example when focus leaves the window).
pub(super) fn clear_composition(composing: &mut bool, events: &mut Vec<Event>) {
    if *composing {
        *composing = false;
        events.push(Event::Ime(ImeEvent::Preedit {
            text: String::new(),
            active_range_chars: None,
        }));
    }
}

/// Updates SDL3's text-input candidate rect and starts/stops text input on `window`.
pub(super) unsafe fn sync_platform_text_input(
    window: *mut SDL_Window,
    ime_area: &mut Option<([i32; 4], i32)>,
    ime: Option<egui::output::IMEOutput>,
    zoom: f32,
) {
    let area = ime.map(|ime| {
        let rect = SDL_Rect {
            x: (ime.rect.min.x * zoom).round() as i32,
            y: (ime.rect.min.y * zoom).round() as i32,
            w: (ime.rect.width() * zoom).round().max(1.0) as i32,
            h: (ime.rect.height() * zoom).round().max(1.0) as i32,
        };
        let cursor = ((ime.cursor_rect.min.x - ime.rect.min.x) * zoom).round().max(0.0) as i32;
        (rect, cursor, ([rect.x, rect.y, rect.w, rect.h], cursor))
    });
    let wants_text = area.is_some();
    // SAFETY: `window` is a live SDL_Window supplied by the caller.
    unsafe {
        if let Some((rect, cursor, key)) = area {
            if *ime_area != Some(key) {
                SDL_SetTextInputArea(window, &rect, cursor);
                *ime_area = Some(key);
            }
        } else {
            *ime_area = None;
        }
        if wants_text != SDL_TextInputActive(window) {
            if wants_text {
                SDL_StartTextInput(window);
            } else {
                SDL_StopTextInput(window);
            }
        }
    }
}
