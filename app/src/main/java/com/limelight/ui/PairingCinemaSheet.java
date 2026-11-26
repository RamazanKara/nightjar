package com.limelight.ui;

import android.app.Activity;
import android.content.res.Configuration;
import android.text.TextUtils;
import android.util.DisplayMetrics;
import android.util.TypedValue;
import android.view.Gravity;
import android.view.LayoutInflater;
import android.view.View;
import android.view.ViewGroup;
import android.view.Window;
import android.widget.TextView;

import androidx.appcompat.app.AlertDialog;

import com.google.android.material.color.MaterialColors;
import com.limelight.R;

/**
 * Pairing with a PC as one panel on the stage: the PC's name, the PIN in large cells of tabular digits, one line saying where
 * to type it, the wait, and Cancel. A phone held upright gets the panel at the foot of the window, in a thumb's reach; every
 * other window gets it in the middle. A pairing that failed stands on the same panel in the same place, its reason in the
 * danger colour. It is an {@link AlertDialog} from {@link NavigationDialogBuilder}, so Back, B and Escape cancel it as they
 * cancel every dialog.
 */
public final class PairingCinemaSheet {
    private PairingCinemaSheet() {
    }

    /** Whether the panel stands at the foot of the window: only on a phone held upright. */
    static boolean atFoot(GlassForm form, boolean upright) {
        return form == GlassForm.PHONE && upright;
    }

    /**
     * A digit cell as {width, height, digit size} in pixels, from the pixels in a dp and in an sp. Larger away from a phone,
     * where it is read from further off, and it grows with a large font instead of cutting its digit.
     */
    static int[] cell(GlassForm form, float dp, float sp) {
        boolean phone = form == GlassForm.PHONE;
        float digit = (phone ? 40f : 48f) * sp;
        int width = Math.round(Math.max((phone ? 64f : 72f) * dp, digit * 1.25f));
        int height = Math.round(Math.max((phone ? 80f : 88f) * dp, digit * 1.6f));
        return new int[] {width, height, Math.round(digit)};
    }

    /** The PIN as a screen reader says it: digit by digit, not as one number. */
    static String spoken(CharSequence pin) {
        StringBuilder out = new StringBuilder(pin.length() * 2);
        for (int i = 0; i < pin.length(); i++) {
            if (i > 0) out.append(' ');
            out.append(pin.charAt(i));
        }
        return out.toString();
    }

    /**
     * Shows the panel for a pairing that has started. With no one-time PIN, {@code pin} is shown to be typed at the PC; with
     * one, the pairing needs nothing typed and the panel only waits. Cancel, Back and a tap outside run {@code onCancel}.
     */
    public static AlertDialog show(Activity activity, CharSequence pcName, String oneTimePin, String pin, Runnable onCancel) {
        NavigationDialogBuilder builder = new NavigationDialogBuilder(activity).ownSurface();
        View body = LayoutInflater.from(builder.getContext()).inflate(R.layout.dialog_pairing_cinema, null);
        TextView name = body.findViewById(R.id.pairing_cinema_pc);
        PinSlots cells = body.findViewById(R.id.pairing_cinema_pin);
        if (TextUtils.isEmpty(pcName)) name.setVisibility(View.GONE);
        else name.setText(pcName);
        if (oneTimePin == null) {
            DisplayMetrics metrics = activity.getResources().getDisplayMetrics();
            int[] cell = cell(GlassForm.of(activity), metrics.density, TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_SP, 1f, metrics));
            cells.setCells(cell[0], cell[1], cell[2]);
            cells.setRange(pin.length(), pin.length());
            cells.show(pin);
            cells.setContentDescription(spoken(pin));
        } else {
            cells.setVisibility(View.GONE);
            body.findViewById(R.id.pairing_cinema_where).setVisibility(View.GONE);
        }
        // With motion reduced the word says it alone.
        if (Motion.reduced(activity)) body.findViewById(R.id.pairing_cinema_spinner).setVisibility(View.GONE);
        builder.setTitle(R.string.pair_pairing_title).setView(body)
                .setNegativeButton(android.R.string.cancel, (d, which) -> onCancel.run())
                .setOnCancelListener(d -> onCancel.run());
        AlertDialog dialog = placed(activity, builder);
        dialog.show();
        return dialog;
    }

    /**
     * Shows why a pairing failed, where its PIN stood: the reason in the danger colour, and Try again when {@code retry} can
     * be run.
     */
    public static AlertDialog showFailure(Activity activity, CharSequence title, CharSequence message, Runnable retry) {
        NavigationDialogBuilder builder = new NavigationDialogBuilder(activity).ownSurface();
        builder.setTitle(title).setMessage(message);
        if (retry == null) {
            builder.setPositiveButton(android.R.string.ok, null);
        } else {
            builder.setNegativeButton(android.R.string.cancel, null)
                    .setPositiveButton(R.string.pair_try_again, (d, which) -> retry.run());
        }
        AlertDialog dialog = placed(activity, builder);
        dialog.show();
        TextView reason = dialog.findViewById(android.R.id.message);
        if (reason != null) reason.setTextColor(MaterialColors.getColor(reason, androidx.appcompat.R.attr.colorError));
        return dialog;
    }

    private static AlertDialog placed(Activity activity, NavigationDialogBuilder builder) {
        boolean upright = activity.getResources().getConfiguration().orientation == Configuration.ORIENTATION_PORTRAIT;
        boolean foot = atFoot(GlassForm.of(activity), upright);
        if (foot) {
            // The panel keeps the page's margin to the window's edges, and stands clear of the bar under it.
            int margin = activity.getResources().getDimensionPixelSize(R.dimen.nj_layout_phone_margin);
            builder.setBackgroundInsetStart(margin).setBackgroundInsetEnd(margin)
                    .setBackgroundInsetTop(margin).setBackgroundInsetBottom(margin);
        }
        AlertDialog dialog = builder.create();
        Window window = dialog.getWindow();
        if (foot && window != null) {
            window.setGravity(Gravity.BOTTOM);
            window.setLayout(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT);
        }
        return dialog;
    }
}
