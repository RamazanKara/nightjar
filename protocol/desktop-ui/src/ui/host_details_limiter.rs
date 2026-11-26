//! The PC details page's read-only rows: [`value_row`], which the page's "PC" section and its
//! remote check's measurements stand in too, and the frame limiter's, for a PC that reports its
//! frame limiter in serverinfo (`Feature::FrameLimiterStatus`); other PCs show nothing of it. The
//! rows stand in one glass group as the page's other rows do: what each is at its leading edge, its
//! value at the trailing one.

use super::*;
use nightjar_policy::http::{Feature, HostCapabilities, ServerInfo};

/// The rows to show, as (English label, value), or `None` for a PC without the status.
pub(super) fn limiter_rows(info: &ServerInfo, caps: &HostCapabilities) -> Option<Vec<(&'static str, String)>> {
    if !caps.has(Feature::FrameLimiterStatus) {
        return None;
    }
    let on_off = |on: bool| if on { tr("On") } else { tr("Off") }.to_owned();
    let limit = match info.frame_limiter_fps_limit_millihz {
        0 => tr("None").to_owned(),
        millihz => trf("{} fps", &[&fps(millihz)]),
    };
    Some(vec![
        (en("Frame limiter"), on_off(info.frame_limiter_enabled)),
        (en("Frame limiter on the virtual display"), on_off(info.virtual_display_frame_limiter_enabled)),
        (en("Frame limit"), limit),
    ])
}

/// 119880 mHz as "119.88", 60000 as "60".
fn fps(millihz: u32) -> String {
    let text = format!("{:.3}", f64::from(millihz) / 1000.0);
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if crate::strings::current().decimal_comma() { text.replace('.', ",") } else { text.to_owned() }
}

impl Launcher {
    /// The frame limiter section of a PC's details page.
    pub(super) fn frame_limiter_status(&mut self, ui: &mut Ui, info: &ServerInfo, caps: &HostCapabilities) {
        let Some(rows) = limiter_rows(info, caps) else {
            return;
        };
        section(ui, "Frame limiter");
        for (label, value) in rows {
            value_row(ui, tr(label), &value);
        }
    }
}

/// Between a label and the value that went under it, as between a row's label and its description.
const UNDER_GAP: f32 = 2.0;

/// A read-only row of a group: `label` as a settings row's, and `value` at the trailing edge in the
/// quieter ink. Neither is cut: a long label wraps beside its value, and a value that fits neither
/// beside its label nor in half the row (a long address in a narrow window) goes under the label,
/// as a row's description does. A screen reader hears both as one line.
pub(super) fn value_row(ui: &mut Ui, label: &str, value: &str) {
    group_row(ui, |ui| {
        let width = ui.available_width();
        let (label_font, value_font) = (semibold(text::BODY), regular(text::BODY));
        let one_line = ui.painter().layout_no_wrap(value.to_owned(), value_font.clone(), *ON_SURFACE_VARIANT);
        let label_w = ui.painter().layout_no_wrap(label.to_owned(), label_font.clone(), *ON_SURFACE).size().x;
        let value_w = one_line.size().x;
        let beside = label_w + space::M + value_w <= width || value_w <= (width - space::M) / 2.0;
        let (label_room, value_galley) = if beside {
            ((width - value_w - space::M).max(0.0), one_line)
        } else {
            (width, ui.painter().layout(value.to_owned(), value_font, *ON_SURFACE_VARIANT, width))
        };
        let label_galley = ui.painter().layout(label.to_owned(), label_font, *ON_SURFACE, label_room);
        let (label_size, value_size) = (label_galley.size(), value_galley.size());
        let words = if beside { label_size.y.max(value_size.y) } else { label_size.y + UNDER_GAP + value_size.y };
        let height = words.max(design_tokens::SIZE_ROW_HEIGHT - 2.0 * 8.0);
        let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, format!("{label}: {value}")));
        let middle = rect.center().y;
        let (label_at, value_at) = if beside {
            (pos2(rect.left(), middle - label_size.y / 2.0), pos2(rect.right() - value_size.x, middle - value_size.y / 2.0))
        } else {
            let top = middle - words / 2.0;
            (pos2(rect.left(), top), pos2(rect.left(), top + label_size.y + UNDER_GAP))
        };
        ui.painter().galley(label_at, label_galley, *ON_SURFACE);
        ui.painter().galley(value_at, value_galley, *ON_SURFACE_VARIANT);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_need_the_capability_and_read_the_status() {
        let parse = |limiter: &str| {
            let xml = format!(
                "<root status_code=\"200\"><hostname>PC</hostname><uniqueid>uid</uniqueid><PairStatus>1</PairStatus>\
                 <state>SUNSHINE_SERVER_FREE</state>{limiter}<FrameLimiterEnabled>1</FrameLimiterEnabled>\
                 <VirtualDisplayFrameLimiterEnabled>0</VirtualDisplayFrameLimiterEnabled>\
                 <FrameLimiterFpsLimitMilliHz>119880</FrameLimiterFpsLimitMilliHz></root>"
            );
            ServerInfo::parse(&xml, 47989).expect("the answer parses")
        };
        let mut info = parse("<FrameLimiterSupported>1</FrameLimiterSupported>");
        assert_eq!(limiter_rows(&info, &HostCapabilities::UNKNOWN), None);
        let other = parse("");
        assert_eq!(limiter_rows(&other, &other.capabilities), None, "an older PC shows nothing");
        let caps = info.capabilities.clone();
        let rows = limiter_rows(&info, &caps).unwrap();
        assert_eq!(rows, vec![("Frame limiter", "On".to_owned()), ("Frame limiter on the virtual display", "Off".to_owned()), ("Frame limit", "119.88 fps".to_owned())]);
        info.frame_limiter_fps_limit_millihz = 0;
        assert_eq!(limiter_rows(&info, &caps).unwrap()[2].1, "None");
        assert_eq!(fps(60_000), "60");
        assert_eq!(fps(59_940), "59.94");
    }
}
