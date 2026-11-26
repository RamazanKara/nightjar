//! The PC details page's read-only frame limiter rows, for a PC that reports its frame limiter
//! in serverinfo (`Feature::FrameLimiterStatus`); other PCs show nothing. The rows stand in one
//! glass group as the page's other rows do: what each is at its leading edge, its value at the
//! trailing one.

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

/// A read-only row of a group: `label` as a settings row's, and `value` at the trailing edge in the
/// quieter ink. The value is never cut: a long label wraps beside it. A screen reader hears both
/// as one line.
fn value_row(ui: &mut Ui, label: &str, value: &str) {
    group_row(ui, |ui| {
        let width = ui.available_width();
        let value_galley = ui.painter().layout_no_wrap(value.to_owned(), regular(text::BODY), *ON_SURFACE_VARIANT);
        let room = (width - value_galley.size().x - space::M).max(0.0);
        let label_galley = ui.painter().layout(label.to_owned(), semibold(text::BODY), *ON_SURFACE, room);
        let height = label_galley.size().y.max(value_galley.size().y).max(design_tokens::SIZE_ROW_HEIGHT - 2.0 * 8.0);
        let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, format!("{label}: {value}")));
        let middle = rect.center().y;
        ui.painter().galley(pos2(rect.left(), middle - label_galley.size().y / 2.0), label_galley, *ON_SURFACE);
        ui.painter().galley(pos2(rect.right() - value_galley.size().x, middle - value_galley.size().y / 2.0), value_galley, *ON_SURFACE_VARIANT);
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
