package com.limelight.ui;

import android.app.Activity;
import android.content.ClipData;
import android.content.ClipboardManager;
import android.content.Context;
import android.widget.LinearLayout;

import com.google.android.material.button.MaterialButton;
import com.limelight.R;
import com.limelight.binding.video.StreamDoctorController;
import com.limelight.binding.video.StreamDoctorVerdict;
import com.limelight.nativeapi.NativeStreamDoctor;
import com.limelight.utils.WifiDiagnostics;

import java.text.NumberFormat;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * Stream Doctor 2.0's card, opened by the Control Center's "Why did my stream stutter?": a title that names the stage,
 * the number it stands on, two plain sentences, one button per remedy with its target and an Undo, advice lines, one
 * line about the upscaler when it does not earn its cost, and the classic measurements behind "Show technical
 * measurements". A sheet over a dimmed stream with no blur under it, so looking costs the stream nothing; its rows take
 * touch, a controller, a TV remote and TalkBack like every sheet's. {@link StreamDoctorController} looks again once a
 * second while it shows; the card is rebuilt only when what it says changed, and the focus stays on the same row.
 */
public final class StreamDoctorSheet implements StreamDoctorController.View {
    private static final String SGSR1 = "SGSR 1", FSR1 = "FSR 1";

    private final Context context;
    private final ActionSheet sheet;
    private final LinearLayout group;
    private final StreamDoctorController controller;
    // The rows by what they do, for keeping the focus across a rebuild.
    private final Map<String, MaterialButton> buttons = new LinkedHashMap<>();
    private String shown;
    private String cause;
    private boolean technical;
    private String technicalText = "";

    public StreamDoctorSheet(Activity activity, StreamDoctorController controller) {
        this.context = activity;
        this.controller = controller;
        sheet = new ActionSheet(activity, activity.getString(R.string.health_title));
        group = sheet.addGroup();
        sheet.setOnClosed(controller::close);
    }

    /** The window, for the stream's dialog handling. */
    public ActionSheet dialog() {
        return sheet;
    }

    /** Starts looking; call once the window shows. */
    public void start() {
        controller.open(this);
    }

    @Override
    public void show(StreamDoctorController.Card card) {
        List<Line> lines = lines(card);
        String title = title(card.verdict);
        String key = card.verdict == null ? null : keyLine(card.verdict.key);
        StringBuilder signature = new StringBuilder(title).append('|').append(key).append('|').append(technical)
                .append('|').append(card.busy);
        for (Line line : lines) signature.append('|').append(line.id).append(':').append(line.text);
        if (technical) signature.append('|').append(card.technical);
        if (signature.toString().equals(shown)) return;
        shown = signature.toString();
        String focused = null;
        for (Map.Entry<String, MaterialButton> entry : buttons.entrySet()) {
            if (entry.getValue().hasFocus()) focused = entry.getKey();
        }
        sheet.setHeading(title);
        sheet.setSubtitle(key);
        String newCause = card.verdict == null ? "" : card.verdict.cause;
        // TalkBack hears a new finding; the sheet's title is read when it opens.
        if (cause != null && !cause.equals(newCause)) group.announceForAccessibility(title);
        cause = newCause;
        build(lines, card);
        MaterialButton again = focused == null ? null : buttons.get(focused);
        if (again == null && focused != null && !buttons.isEmpty()) again = buttons.values().iterator().next();
        if (again != null) again.requestFocus();
    }

    /** What the card says under its title, in order; an id marks a row the player presses. */
    private static final class Line {
        final String id, text;
        final boolean error;
        final int icon;
        final Runnable action;

        Line(String id, String text, boolean error, int icon, Runnable action) {
            this.id = id;
            this.text = text;
            this.error = error;
            this.icon = icon;
            this.action = action;
        }
    }

    private List<Line> lines(StreamDoctorController.Card card) {
        List<Line> lines = new ArrayList<>();
        StreamDoctorVerdict v = card.verdict;
        if (v != null) note(lines, body(v), false);
        boolean gathering = v == null || StreamDoctorVerdict.GATHERING.equals(v.cause);
        if (card.failure != null) note(lines, card.failure, true);
        if (card.resizeProblem != StreamDoctorController.RESIZE_OK && card.resized != null) {
            note(lines, context.getString(card.resizeProblem == StreamDoctorController.RESIZE_KEPT
                    ? R.string.doctor_resize_kept : R.string.doctor_resize_not_sent, card.resized[0], card.resized[1]), true);
        }
        String next = null;
        if (!gathering) {
            for (StreamDoctorVerdict.Remedy remedy : v.remedies) {
                if (StreamDoctorVerdict.BITRATE_STEPDOWN.equals(remedy.kind)) {
                    lines.add(new Line("bitrate", context.getString(R.string.doctor_remedy_bitrate, mbps(remedy.targetKbps)),
                            false, R.drawable.ic_speed, () -> controller.apply(remedy)));
                } else if (StreamDoctorVerdict.RESOLUTION_STEPDOWN.equals(remedy.kind)) {
                    lines.add(new Line("resolution", context.getString(R.string.doctor_remedy_resolution, remedy.width,
                            remedy.height), false, R.drawable.ic_monitor, () -> controller.apply(remedy)));
                    if (upscalerName(remedy.upscaler) != null) next = upscalerName(remedy.upscaler);
                }
            }
            if (card.guest && wantsRemedy(v)) note(lines, context.getString(R.string.doctor_host_only), false);
            if (card.hostNote != null && !card.guest && wantsBitrate(v)) note(lines, card.hostNote, false);
        }
        if (card.undoKbps > 0) {
            lines.add(new Line("undo_bitrate", context.getString(R.string.doctor_undo_bitrate, mbps(card.undoKbps)), false,
                    R.drawable.ic_arrow_back, controller::undoBitrate));
        } else if (card.undoReplaced) {
            note(lines, context.getString(R.string.health_trial_replaced), false);
        }
        if (card.undoSize != null) {
            lines.add(new Line("undo_resolution", context.getString(R.string.doctor_undo_resolution, card.undoSize[0],
                    card.undoSize[1]), false, R.drawable.ic_arrow_back, controller::undoResize));
        }
        if (!gathering) {
            for (StreamDoctorVerdict.Advice advice : v.advice) {
                String text = advice(advice);
                if ("use_upscaler".equals(advice.kind)) next = null;
                if (text != null) note(lines, text, false);
            }
            if (next != null) note(lines, context.getString(R.string.doctor_advice_upscaler, next), false);
            String upscaler = upscalerLine(v);
            if (upscaler != null) note(lines, upscaler, false);
        }
        if (card.hostError != null) note(lines, context.getString(R.string.doctor_host_unread, card.hostError), false);
        lines.add(new Line("technical", context.getString(technical ? R.string.doctor_technical_hide
                : R.string.doctor_technical_show), false, technical ? R.drawable.ic_expand_less : R.drawable.ic_expand_more,
                () -> {
                    technical = !technical;
                    shown = null;
                    show(card);
                }));
        if (technical) {
            String host = hostText(card.host);
            technicalText = host == null ? card.technical : card.technical + "\n\n" + host;
            note(lines, technicalText, false);
            lines.add(new Line("copy", context.getString(R.string.overlay_copy), false, R.drawable.ic_copy, this::copy));
        }
        return lines;
    }

    private static void note(List<Line> lines, String text, boolean error) {
        if (text != null && !text.isEmpty()) lines.add(new Line(null, text, error, 0, null));
    }

    private void build(List<Line> lines, StreamDoctorController.Card card) {
        group.removeAllViews();
        buttons.clear();
        for (Line line : lines) {
            if (line.id == null) {
                sheet.addNote(group, line.text, line.error);
                continue;
            }
            MaterialButton button = sheet.addAction(group, line.icon, line.text);
            // Undo, the technical measurements and Copy stay usable while a request runs; a remedy waits for it.
            boolean waits = card.busy && !"technical".equals(line.id) && !"copy".equals(line.id);
            button.setEnabled(!waits);
            button.setOnClickListener(pressed -> line.action.run());
            buttons.put(line.id, button);
        }
        if (!buttons.isEmpty()) sheet.setFirstAction(buttons.values().iterator().next());
    }

    private void copy() {
        ClipboardManager clipboard = (ClipboardManager) context.getSystemService(Context.CLIPBOARD_SERVICE);
        if (clipboard != null) {
            clipboard.setPrimaryClip(ClipData.newPlainText(context.getString(R.string.health_title), technicalText));
        }
    }

    private static boolean wantsBitrate(StreamDoctorVerdict v) {
        return StreamDoctorVerdict.NETWORK_JITTER.equals(v.cause) || StreamDoctorVerdict.RECEIVE_BUFFER_SHORT.equals(v.cause);
    }

    private static boolean wantsRemedy(StreamDoctorVerdict v) {
        return wantsBitrate(v) || StreamDoctorVerdict.HOST_OVERLOAD.equals(v.cause)
                || StreamDoctorVerdict.DECODE_PRESSURE.equals(v.cause);
    }

    private String title(StreamDoctorVerdict v) {
        if (v == null) return context.getString(R.string.health_title);
        switch (v.cause) {
            case StreamDoctorVerdict.GATHERING: return context.getString(R.string.doctor_title_gathering);
            case StreamDoctorVerdict.HEALTHY: return context.getString(R.string.doctor_title_healthy);
            case StreamDoctorVerdict.RECEIVE_BUFFER_SHORT: return context.getString(R.string.doctor_title_receive_buffer);
            case StreamDoctorVerdict.HOST_OVERLOAD: return context.getString(R.string.doctor_title_host);
            case StreamDoctorVerdict.DECODE_PRESSURE: return context.getString(R.string.doctor_title_decode);
            case StreamDoctorVerdict.DISPLAY_PACING: return context.getString(R.string.doctor_title_display);
            case StreamDoctorVerdict.NETWORK_JITTER:
                return context.getString("host_send".equals(v.stage) ? R.string.doctor_title_host_send
                        : R.string.doctor_title_network);
            default: return context.getString(R.string.health_title);
        }
    }

    private String body(StreamDoctorVerdict v) {
        switch (v.cause) {
            case StreamDoctorVerdict.GATHERING: return context.getString(R.string.doctor_gathering);
            case StreamDoctorVerdict.HEALTHY: return context.getString(R.string.doctor_body_healthy);
            case StreamDoctorVerdict.RECEIVE_BUFFER_SHORT: return context.getString(R.string.doctor_body_receive_buffer);
            case StreamDoctorVerdict.HOST_OVERLOAD: return context.getString(R.string.doctor_body_host);
            case StreamDoctorVerdict.DECODE_PRESSURE: return context.getString(R.string.doctor_body_decode);
            case StreamDoctorVerdict.DISPLAY_PACING: return context.getString(R.string.doctor_body_display);
            case StreamDoctorVerdict.NETWORK_JITTER:
                return context.getString("host_send".equals(v.stage) ? R.string.doctor_body_host_send
                        : R.string.doctor_body_network);
            default: return null;
        }
    }

    /** The number a finding stands on, or null for none or one the stream cannot state. */
    private String keyLine(StreamDoctorVerdict.Key k) {
        switch (k.kind) {
            case "loss_percent": return finite(k.percent) ? context.getString(R.string.doctor_key_loss, k.percent) : null;
            case "late_seconds": return context.getString(R.string.doctor_key_late, (int) k.late, (int) k.of);
            case "host_lost":
                return finite(k.lostMs) ? context.getString(R.string.doctor_key_host_lost, (int) k.late, (int) k.pictures,
                        stageName(k.hostStage), k.lostMs) : null;
            case "host_p95": return ms(R.string.doctor_key_host_p95, k);
            case "decode_ms": return ms(R.string.doctor_key_decode, k);
            case "display_p95": return ms(R.string.doctor_key_display, k);
            case "receive_buffer":
                return context.getString(R.string.doctor_key_receive_buffer, (int) k.grantedKib, (int) k.askedKib);
            default: return null;
        }
    }

    private String ms(int id, StreamDoctorVerdict.Key k) {
        return finite(k.ms) && k.ms >= 0 && finite(k.budgetMs) ? context.getString(id, k.ms, k.budgetMs) : null;
    }

    private static boolean finite(double value) {
        return !Double.isNaN(value) && !Double.isInfinite(value);
    }

    private String stageName(String stage) {
        switch (stage) {
            case "capture": return context.getString(R.string.doctor_stage_capture);
            case "convert": return context.getString(R.string.doctor_stage_convert);
            case "encode": return context.getString(R.string.doctor_stage_encode);
            case "packetize": return context.getString(R.string.doctor_stage_packetize);
            default: return context.getString(R.string.doctor_stage_send);
        }
    }

    private String advice(StreamDoctorVerdict.Advice a) {
        switch (a.kind) {
            case "wired_or_quieter_wifi": return context.getString(R.string.doctor_advice_wifi);
            case "periodic_scans":
                return context.getString(R.string.doctor_advice_scans, WifiDiagnostics.period(context, a.periodMs));
            case "raise_receive_buffer": return context.getString(R.string.doctor_advice_receive_buffer);
            case "ease_the_pc": return context.getString(R.string.doctor_advice_ease_pc);
            case "lower_frame_rate": return context.getString(R.string.doctor_advice_frame_rate);
            case "cool_down": return context.getString(R.string.doctor_advice_cool);
            case "frame_pacing": return context.getString(R.string.doctor_advice_pacing);
            case "use_upscaler":
                String name = upscalerName(a.upscaler);
                return name == null ? null : context.getString(R.string.doctor_advice_upscaler, name);
            default: return null;
        }
    }

    /** The one upscaler line, when the verdict is not fine. */
    private String upscalerLine(StreamDoctorVerdict v) {
        switch (v.upscaler) {
            case "not_needed": return context.getString(R.string.doctor_upscaler_not_needed);
            case "too_costly":
                String lighter = upscalerName(v.lighter);
                return lighter == null ? context.getString(R.string.doctor_upscaler_off)
                        : context.getString(R.string.doctor_upscaler_lighter, lighter);
            case "blocked":
                switch (v.blocker) {
                    case "hdr":
                    case "ten_bit": return context.getString(R.string.upscaling_hdr);
                    case "pyro_wave": return context.getString(R.string.upscaling_pyrowave);
                    case "no_upscale": return context.getString(R.string.upscaling_native);
                    default: return null;
                }
            default: return null;
        }
    }

    private static String upscalerName(String upscaler) {
        return "sgsr1".equals(upscaler) ? SGSR1 : "fsr1".equals(upscaler) ? FSR1 : null;
    }

    private String mbps(int kbps) {
        NumberFormat format = NumberFormat.getNumberInstance();
        format.setMaximumFractionDigits(1);
        return format.format(kbps / 1000.0);
    }

    /** The host's stage report and encoder, for the technical measurements; null without one. */
    private String hostText(NativeStreamDoctor host) {
        if (host == null || !host.sessionActive) return null;
        String[] lost = new String[host.lostUs.length];
        for (int i = 0; i < lost.length; i++) lost[i] = String.format(java.util.Locale.getDefault(), "%.1f", host.lostUs[i] / 1000.0);
        return context.getString(R.string.doctor_technical_host, (int) host.latePictures, (int) host.pictures, lost[0], lost[1],
                lost[2], lost[3], lost[4], host.backend + " " + host.codec);
    }
}
