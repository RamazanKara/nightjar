package com.limelight.ui;

import android.content.Context;
import android.content.DialogInterface;
import android.view.KeyEvent;
import android.view.View;
import android.view.ViewGroup;
import android.widget.Button;
import android.widget.ListView;

import androidx.appcompat.app.AlertDialog;

import com.google.android.material.dialog.MaterialAlertDialogBuilder;
import com.limelight.R;

/**
 * The app's dialogs: the glass panel, one moon action, and the same Back/B/Escape navigation as the action sheets. All of them
 * come from here, so they are one look on every form.
 */
public final class NavigationDialogBuilder extends MaterialAlertDialogBuilder {
    private DialogInterface.OnKeyListener keyListener;
    private boolean startOnCancel;
    private boolean ownSurface;
    private final boolean overStream;

    public NavigationDialogBuilder(Context context) {
        super(context);
        // Over a stream, the stream menu's glass: StreamGlass.show puts a blur under it where the device affords one.
        overStream = StreamGlass.overStream(getContext());
        setBackground(overStream ? GlassPanelDrawable.menu(getContext(), false) : GlassPanelDrawable.dialog(getContext()));
    }

    /** For a confirmation that ends or loses something: a pad's or a remote's first press is Cancel, not the moon. */
    public NavigationDialogBuilder startOnCancel() {
        startOnCancel = true;
        return this;
    }

    /**
     * For a dialog that is a task of its own (a pairing PIN): it stands on the sky instead of over the page, so its panel never
     * covers the page's heading or a card's edge. Over a stream it stays as any dialog there.
     */
    public NavigationDialogBuilder ownSurface() {
        ownSurface = true;
        return this;
    }

    @Override
    public MaterialAlertDialogBuilder setOnKeyListener(DialogInterface.OnKeyListener listener) {
        keyListener = listener;
        return super.setOnKeyListener(listener);
    }

    @Override
    public AlertDialog create() {
        AlertDialog dialog = super.create();
        // The platform's selector is a grey block, which is no focus language of ours.
        ListView list = dialog.getListView();
        if (list != null) list.setSelector(R.drawable.dialog_row_focus);
        boolean onSky = ownSurface && !overStream;
        if (dialog.getWindow() != null) {
            // Over a stream the game stays in view around the glass, as around the stream menu; on its own surface the sky
            // is all there is behind it, as around the PIN lock.
            if (overStream || onSky) dialog.getWindow().clearFlags(android.view.WindowManager.LayoutParams.FLAG_DIM_BEHIND);
            else dialog.getWindow().setDimAmount(GlassPanelDrawable.dialogBackdropDim(getContext()));
        }
        // On a page, the page blurs behind the dialog where this device affords it; never over a stream, and not under a sky
        // that hides it.
        if (onSky) DialogSky.under(dialog);
        else PageBlur.behind(dialog);
        // Builds the views now: the buttons exist, and their bar can be laid out for capsules.
        dialog.create();
        styleButtons(dialog);
        DialogInterface.OnKeyListener custom = keyListener;
        // One filter per dialog: a pad that sends DPAD keys and a hat must not move its focus twice. The
        // dialog is short-lived, so it has no focus-loss reset (the listener has no hook for it).
        DpadEchoFilter dpadEchoes = new DpadEchoFilter();
        dialog.setOnKeyListener((target, key, event) -> {
            if (dpadEchoes.drop(event)) return true;
            if (custom != null && custom.onKey(target, key, event)) return true;
            // A dialog has its own window, out of the page's reach: Y would come back as Space and press its button.
            // This listener runs before the dialog's views, so a focused text field is asked for here.
            if (UiNavigation.swallowsPadY(key, dialog.getCurrentFocus())) return true;
            if (!event.hasNoModifiers() || (key != KeyEvent.KEYCODE_BUTTON_B && key != KeyEvent.KEYCODE_ESCAPE)) return false;
            // Use the normal Back path: cancel callbacks and non-cancelable dialogs retain their rules.
            if (event.getAction() == KeyEvent.ACTION_UP && !event.isCanceled()) dialog.onBackPressed();
            return true;
        });
        return dialog;
    }

    private void styleButtons(AlertDialog dialog) {
        Button positive = dialog.getButton(DialogInterface.BUTTON_POSITIVE);
        if (positive == null || !(positive.getParent() instanceof ViewGroup)) return;
        // Capsules want air the text buttons did not: the bar stands on the panel's own margin.
        android.content.res.Resources res = getContext().getResources();
        int edge = res.getDimensionPixelSize(R.dimen.space_l);
        ViewGroup bar = (ViewGroup) positive.getParent();
        bar.setPaddingRelative(edge, res.getDimensionPixelSize(R.dimen.space_m), edge, edge);
        // Focused buttons scale to 1.025x with a 3dp focus ring; disabling clipping along the bar and its
        // ScrollView keeps the right-hand button's focus border intact on TV (nightjar#7).
        bar.setClipChildren(false);
        bar.setClipToPadding(false);
        // The bar's scroller took the window's first focus and showed the platform's grey highlight over the whole bar.
        if (bar.getParent() instanceof ViewGroup) {
            ViewGroup parent = (ViewGroup) bar.getParent();
            parent.setClipChildren(false);
            parent.setClipToPadding(false);
            if (parent instanceof android.widget.ScrollView) {
                parent.setFocusable(false);
                if (android.os.Build.VERSION.SDK_INT >= 26) parent.setDefaultFocusHighlightEnabled(false);
            }
            if (parent.getParent() instanceof ViewGroup) {
                ((ViewGroup) parent.getParent()).setClipChildren(false);
            }
        }
        // A pad or a remote starts on the moon, the dialog's one way on. A list keeps its first focus on its rows.
        Button negative = dialog.getButton(DialogInterface.BUTTON_NEGATIVE);
        boolean cancelFirst = startOnCancel && negative != null && negative.getVisibility() == View.VISIBLE;
        if (android.os.Build.VERSION.SDK_INT >= 26 && dialog.getListView() == null) (cancelFirst ? negative : positive).setFocusedByDefault(true);
        // Labels too long to share a row stack: as full-width capsules, not a staircase along the right edge. A row the
        // dialog's button scroller cannot scroll sideways is stacked too, so its last capsule is not cut at the panel.
        final int[] stackedAtWidth = {0};
        final java.util.Map<View, int[]> originalParams = new java.util.WeakHashMap<>();
        bar.addOnLayoutChangeListener((view, l, t, r, b, ol, ot, or, ob) -> {
            if (!(bar instanceof android.widget.LinearLayout)) return;
            android.widget.LinearLayout linear = (android.widget.LinearLayout) bar;
            int width = r - l;
            if (linear.getOrientation() == android.widget.LinearLayout.VERTICAL) {
                if (stackedAtWidth[0] > 0 && width > stackedAtWidth[0]) {
                    bar.post(() -> {
                        restoreHorizontalBar(linear, originalParams);
                        stackedAtWidth[0] = 0;
                    });
                } else {
                    bar.post(() -> stretchStacked(linear, originalParams));
                }
            } else if (rowWiderThanBar(bar)) {
                stackedAtWidth[0] = width;
                linear.setOrientation(android.widget.LinearLayout.VERTICAL);
                bar.post(() -> stretchStacked(linear, originalParams));
            }
        });
    }

    /**
     * Whether the bar's visible buttons leave no room in its row, which would run the row past the panel: a dialog's
     * button scroller scrolls vertically only, so the last capsule would be cut on the right. A capsule the row
     * squeezed is clamped to the space left without a too-small state of its own (a TextView clamps itself that way),
     * so a row the buttons fill exactly counts too. The spacer and the other children take no width of their own.
     */
    static boolean rowWiderThanBar(ViewGroup bar) {
        int available = bar.getWidth() - bar.getPaddingLeft() - bar.getPaddingRight();
        if (available <= 0) return false;
        int needed = 0;
        for (int i = 0; i < bar.getChildCount(); i++) {
            View child = bar.getChildAt(i);
            if (!(child instanceof Button) || child.getVisibility() != View.VISIBLE) continue;
            if ((child.getMeasuredWidthAndState() & View.MEASURED_STATE_TOO_SMALL) != 0) return true;
            needed += child.getMeasuredWidth();
            ViewGroup.LayoutParams params = child.getLayoutParams();
            if (params instanceof ViewGroup.MarginLayoutParams) {
                ViewGroup.MarginLayoutParams margins = (ViewGroup.MarginLayoutParams) params;
                needed += margins.getMarginStart() + margins.getMarginEnd();
            }
        }
        return needed >= available;
    }

    private void stretchStacked(ViewGroup bar, java.util.Map<View, int[]> originalParams) {
        int gap = getContext().getResources().getDimensionPixelSize(R.dimen.space_s);
        boolean first = true;
        for (int i = 0; i < bar.getChildCount(); i++) {
            View child = bar.getChildAt(i);
            if (!(child instanceof Button) || child.getVisibility() != View.VISIBLE) continue;
            ViewGroup.MarginLayoutParams params = (ViewGroup.MarginLayoutParams) child.getLayoutParams();
            if (!originalParams.containsKey(child)) {
                originalParams.put(child, new int[] {params.width, params.getMarginStart(), params.topMargin});
            }
            int top = first ? 0 : gap;
            first = false;
            if (params.width == ViewGroup.LayoutParams.MATCH_PARENT && params.topMargin == top && params.getMarginStart() == 0) continue;
            params.width = ViewGroup.LayoutParams.MATCH_PARENT;
            params.topMargin = top;
            params.setMarginStart(0);
            child.setLayoutParams(params);
        }
    }

    private static void restoreHorizontalBar(android.widget.LinearLayout bar, java.util.Map<View, int[]> originalParams) {
        bar.setOrientation(android.widget.LinearLayout.HORIZONTAL);
        for (int i = 0; i < bar.getChildCount(); i++) {
            View child = bar.getChildAt(i);
            int[] orig = originalParams.get(child);
            if (orig == null || !(child.getLayoutParams() instanceof ViewGroup.MarginLayoutParams)) continue;
            ViewGroup.MarginLayoutParams params = (ViewGroup.MarginLayoutParams) child.getLayoutParams();
            params.width = orig[0];
            params.setMarginStart(orig[1]);
            params.topMargin = orig[2];
            child.setLayoutParams(params);
        }
    }
}
