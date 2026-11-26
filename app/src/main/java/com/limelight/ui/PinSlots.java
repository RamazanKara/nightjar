package com.limelight.ui;

import android.content.Context;
import android.util.AttributeSet;
import android.view.Gravity;
import android.view.View;
import android.widget.LinearLayout;
import android.widget.TextView;

import com.limelight.R;

/**
 * The digits of a PIN as slots, the next one ringed. A PIN that may run longer than its least gains slots as it is typed.
 * A saved PIN's digits never come here: a masked entry is only a count.
 */
public final class PinSlots extends LinearLayout {
    private static final String DOT = "\u2022";
    private int least = 4;
    private int most = 4;
    private int gap;
    private int widest;
    private CharSequence digits = "";
    private float digitSize;
    private int tall;
    private boolean toRead;

    public PinSlots(Context context) {
        this(context, null);
    }

    public PinSlots(Context context, AttributeSet attrs) {
        super(context, attrs);
        setOrientation(HORIZONTAL);
        setGravity(Gravity.CENTER);
        // Digits read left to right in every language.
        setLayoutDirection(LAYOUT_DIRECTION_LTR);
        gap = getResources().getDimensionPixelSize(R.dimen.space_s);
        widest = getResources().getDimensionPixelSize(R.dimen.pin_slot_width);
        digitSize = getResources().getDimension(R.dimen.pin_key_text);
        tall = getResources().getDimensionPixelSize(R.dimen.pin_slot_height);
    }

    /**
     * Cells for a PIN that is shown to be read and typed elsewhere: this wide and tall at most, with digits of this size, and
     * none of them ringed, since nothing is typed here.
     */
    public void setCells(int width, int height, float textSize) {
        widest = width;
        tall = height;
        digitSize = textSize;
        toRead = true;
        removeAllViews();
    }

    /** A cell's width when {@code slots} of them share {@code room} with a gap between them: never wider than {@code widest}. */
    static int cellWidth(int room, int slots, int widest, int gap) {
        if (slots <= 0) return 0;
        return Math.max(0, Math.min(widest, (room - gap * (slots - 1)) / slots));
    }

    /** How many slots stand empty at the start, and how many there can be. */
    public void setRange(int least, int most) {
        this.least = least;
        this.most = most;
    }

    /** A PIN to read: its digits stand in the slots. */
    public void show(CharSequence entered) {
        digits = entered;
        fill(entered.length(), entered);
    }

    /** A PIN to keep: a dot for each digit. */
    public void showMasked(int count) {
        digits = "";
        fill(count, null);
    }

    /** The digits now in the slots; empty for a masked PIN. */
    public CharSequence shown() {
        return digits;
    }

    private void fill(int count, CharSequence entered) {
        int slots = toRead ? Math.max(least, count) : Math.max(least, Math.min(most, count + 1));
        while (getChildCount() < slots) addView(newSlot());
        while (getChildCount() > slots) removeViewAt(getChildCount() - 1);
        for (int i = 0; i < slots; i++) {
            TextView slot = (TextView) getChildAt(i);
            slot.setText(i >= count ? "" : entered == null ? DOT : String.valueOf(entered.charAt(i)));
            // A bullet is a small glyph at a digit's size.
            slot.setTextSize(android.util.TypedValue.COMPLEX_UNIT_PX, entered == null ? digitSize * 1.6f : digitSize);
            slot.setActivated(!toRead && i == count);
        }
    }

    private TextView newSlot() {
        TextView slot = new TextView(getContext());
        slot.setTextAppearance(getContext(), R.style.TextAppearance_Nightjar_PinKey);
        slot.setGravity(Gravity.CENTER);
        // Every digit takes the same room, so a 1 stands in the middle of its cell as an 8 does.
        slot.setFontFeatureSettings("tnum");
        slot.setIncludeFontPadding(false);
        slot.setBackgroundResource(R.drawable.pin_slot);
        slot.setImportantForAccessibility(View.IMPORTANT_FOR_ACCESSIBILITY_NO);
        LayoutParams params = new LayoutParams(widest, tall);
        slot.setLayoutParams(params);
        return slot;
    }

    @Override
    protected void onMeasure(int widthSpec, int heightSpec) {
        int slots = getChildCount();
        int room = MeasureSpec.getSize(widthSpec);
        int width = widest;
        // Eight slots share the room four had.
        if (slots > 0 && MeasureSpec.getMode(widthSpec) != MeasureSpec.UNSPECIFIED) {
            width = cellWidth(room - getPaddingLeft() - getPaddingRight(), slots, widest, gap);
        }
        for (int i = 0; i < slots; i++) {
            LayoutParams params = (LayoutParams) getChildAt(i).getLayoutParams();
            params.width = width;
            params.setMarginStart(i == 0 ? 0 : gap);
        }
        super.onMeasure(widthSpec, heightSpec);
    }
}
