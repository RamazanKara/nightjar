package com.limelight.binding.input.pad;

import android.annotation.SuppressLint;
import android.content.Context;
import android.content.res.Resources;
import android.graphics.Canvas;
import android.graphics.Paint;
import android.graphics.RectF;
import android.view.MotionEvent;
import android.view.View;

import androidx.core.content.ContextCompat;
import androidx.core.graphics.ColorUtils;

import com.limelight.R;
import com.limelight.ui.Haptics;

/**
 * The full-screen editing surface behind {@link PadEditor}'s dock: draws the dimmed stream, the grid, the controls
 * with their outlines, the picked one's corner handles and the guides, and handles tap-to-pick, drag, pinch-resize and
 * corner handle sizing. {@link PadSnapGrid} says where a moved or sized control lands; a line it holds shows as one
 * luminous guide while the finger is down, and taking hold of one is one light tick.
 */
@SuppressLint("ViewConstructor") // Created by PadEditor.
final class PadEditorSurface extends View {
    /** How far outside a control's box its outline and its corner handles are drawn, in dp. */
    static final float OUTLINE_DP = 3;
    /** In dp, and not in the tokens: the outline's corner radius and a corner handle's weight. */
    private static final float OUTLINE_RADIUS_DP = 10, HANDLE_WEIGHT_DP = 3.5f;
    /** Every third grid line is drawn: 8 dp lines would hide the game under them. */
    private static final int GRID_EVERY = 3;
    /** A guide's light around its line: this many times as wide, at this share of its colour. */
    private static final float GUIDE_SPILL = 4, GUIDE_SPILL_ALPHA = .22f;

    private final PadEditor editor;
    private final PadPainter painter;
    private final PadLayout draft;
    private final PadSnapGrid snap;
    private final float density, gridStep, handle;
    private final Paint scrim = new Paint(), grid = new Paint(), guide = new Paint(Paint.ANTI_ALIAS_FLAG),
            guideSpill = new Paint(Paint.ANTI_ALIAS_FLAG), faint = new Paint(Paint.ANTI_ALIAS_FLAG),
            picked = new Paint(Paint.ANTI_ALIAS_FLAG), under = new Paint(Paint.ANTI_ALIAS_FLAG);
    private final RectF rect = new RectF();
    private final float[] box = new float[4], other = new float[4];
    private final PadPainter.State idle = new PadPainter.State(), pointerIdle = new PadPainter.State();
    private float offsetX, offsetY, pinchDistance, pinchWidth, pinchHeight, anchorX, anchorY, downX, downY;
    /** A finger that has moved this far along an axis is dragging along it, not tapping. */
    private final float slop;
    private boolean dragging, pinching, movedX, movedY;
    /** Whether this gesture has changed the layout yet: its first change is what Undo takes back. */
    private boolean changing;
    /** The corner handle being dragged ({@link #corner}'s bits), or -1. */
    private int resizing = -1;

    PadEditorSurface(Context context, PadEditor editor, PadPainter painter, PadLayout draft, float density) {
        super(context);
        this.editor = editor;
        this.painter = painter;
        this.draft = draft;
        this.density = density;
        Resources resources = context.getResources();
        gridStep = resources.getDimension(R.dimen.nj_pad_studio_grid);
        handle = resources.getDimension(R.dimen.nj_pad_studio_handle);
        snap = new PadSnapGrid(gridStep, resources.getDimension(R.dimen.nj_pad_studio_snap_distance));
        slop = android.view.ViewConfiguration.get(context).getScaledTouchSlop();
        scrim.setColor(0x99000000);
        grid.setColor(ContextCompat.getColor(context, R.color.nj_pad_studio_grid_color));
        grid.setStrokeWidth(Math.max(1, density));
        int guideColor = ContextCompat.getColor(context, R.color.nj_pad_studio_guide_color);
        float guideWidth = resources.getDimension(R.dimen.nj_pad_studio_guide_width);
        guide.setColor(guideColor);
        guide.setStrokeWidth(guideWidth);
        guideSpill.setColor(ColorUtils.setAlphaComponent(guideColor,
                Math.round(android.graphics.Color.alpha(guideColor) * GUIDE_SPILL_ALPHA)));
        guideSpill.setStrokeWidth(guideWidth * GUIDE_SPILL);
        // Every control's touch box, faintly; the picked one's in the studio's amber over a dark hairline.
        faint.setStyle(Paint.Style.STROKE);
        faint.setStrokeWidth(Math.max(1, density));
        faint.setColor(ContextCompat.getColor(context, R.color.nj_pad_studio_inspector_rim));
        picked.setStyle(Paint.Style.STROKE);
        picked.setColor(ContextCompat.getColor(context, R.color.nj_pad_studio_selected_rim));
        under.setStyle(Paint.Style.STROKE);
        under.setColor(ContextCompat.getColor(context, R.color.nj_pad_skin_shadow));
        // The sticks show the deadzone they play with.
        idle.deadzone = editor.stickDeadzone();
    }

    /** How many alignment guides the surface shows now, for tests. */
    int guides() {
        return snap.guides();
    }

    @Override
    protected void onDraw(Canvas canvas) {
        float width = getWidth(), height = getHeight();
        canvas.drawRect(0, 0, width, height, scrim);
        float step = gridStep * GRID_EVERY;
        for (float x = step; x < width; x += step) canvas.drawLine(x, 0, x, height, grid);
        for (float y = step; y < height; y += step) canvas.drawLine(0, y, width, y, grid);
        float base = editor.baseOpacity();
        PadLayout.Control selected = editor.selected();
        for (PadLayout.Control control : PadLayout.Control.values()) {
            if (!editor.onSurface(control)) continue;
            PadLayout.Spec spec = draft.spec(control);
            draft.bounds(control, width, height, density, box);
            // Show the real opacity, but never so faint that a control cannot be found. The skin fades its own
            // paints, so no control needs a layer.
            painter.setOpacity(Math.max(.35f, base * spec.opacity / 100f));
            painter.draw(canvas, control, box[0], box[1], box[2], box[3], previewState(control));
            painter.setOpacity(1);
            if (control != selected) outline(canvas, false);
        }
        // The picked control's outline and handles over every control, so a neighbour never covers a handle.
        if (selected != null && editor.onSurface(selected)) {
            draft.bounds(selected, width, height, density, box);
            outline(canvas, true);
        }
        float half = guide.getStrokeWidth() / 2;
        if (!Float.isNaN(snap.guideX)) {
            // A guide on the screen's edge stays whole on it.
            float x = Math.max(half, Math.min(width - half, snap.guideX));
            canvas.drawLine(x, 0, x, height, guideSpill);
            canvas.drawLine(x, 0, x, height, guide);
        }
        if (!Float.isNaN(snap.guideY)) {
            float y = Math.max(half, Math.min(height - half, snap.guideY));
            canvas.drawLine(0, y, width, y, guideSpill);
            canvas.drawLine(0, y, width, y, guide);
        }
    }

    /**
     * The outline around the control in {@link #box}: a faint line, or for the picked one the studio's amber with a
     * handle at each corner, which is the outline's own corner drawn heavy for the length of a handle.
     */
    private void outline(Canvas canvas, boolean selected) {
        float pad = OUTLINE_DP * density, radius = OUTLINE_RADIUS_DP * density;
        rect.set(box[0] - pad, box[1] - pad, box[0] + box[2] + pad, box[1] + box[3] + pad);
        if (!selected) {
            canvas.drawRoundRect(rect, radius, radius, faint);
            return;
        }
        float line = guide.getStrokeWidth(), weight = HANDLE_WEIGHT_DP * density, hair = Math.max(1, density);
        under.setStrokeWidth(line + 2 * hair);
        canvas.drawRoundRect(rect, radius, radius, under);
        picked.setStrokeWidth(line);
        canvas.drawRoundRect(rect, radius, radius, picked);
        // Two handles never meet along a side, however small the control.
        float leg = Math.min(handle, Math.min(rect.width(), rect.height()) / 2 - 2 * density);
        float out = weight + hair;
        for (int corner = 0; corner < 4; corner++) {
            boolean right = (corner & PadEditor.RIGHT_CORNER) != 0, bottom = (corner & PadEditor.BOTTOM_CORNER) != 0;
            float x = right ? rect.right : rect.left, y = bottom ? rect.bottom : rect.top;
            int saved = canvas.save();
            canvas.clipRect(right ? x - leg : x - out, bottom ? y - leg : y - out, right ? x + out : x + leg,
                    bottom ? y + out : y + leg);
            under.setStrokeWidth(weight + 2 * hair);
            canvas.drawRoundRect(rect, radius, radius, under);
            picked.setStrokeWidth(weight);
            canvas.drawRoundRect(rect, radius, radius, picked);
            canvas.restoreToCount(saved);
        }
    }

    /** What a control shows at rest here: the pointer touchpad its mode's word, as it does in play. */
    PadPainter.State previewState(PadLayout.Control control) {
        if (control != PadLayout.Control.POINTER_PAD) return idle;
        pointerIdle.label = painter.pointerLabel(draft.pointerMode() == PadLayout.PointerMode.CAMERA);
        return pointerIdle;
    }

    private PadLayout.Control hit(float x, float y) {
        PadLayout.Control[] controls = PadLayout.Control.values();
        float slop = 8 * density;
        // The last drawn is on top, so it wins.
        for (int i = controls.length - 1; i >= 0; i--) {
            if (!editor.onSurface(controls[i])) continue;
            draft.bounds(controls[i], getWidth(), getHeight(), density, box);
            if (x >= box[0] - slop && x <= box[0] + box[2] + slop && y >= box[1] - slop && y <= box[1] + box[3] + slop) {
                return controls[i];
            }
        }
        return null;
    }

    /**
     * The picked control's corner handle under ({@code x}, {@code y}), as {@link PadEditor#RIGHT_CORNER} and
     * {@link PadEditor#BOTTOM_CORNER} bits, or -1. Each handle takes a 48 dp touch around the corner it is drawn on;
     * inside the control only a corner's quarter counts, so a small control's middle still moves it.
     */
    private int corner(float x, float y) {
        PadLayout.Control selected = editor.selected();
        if (selected == null || !editor.onSurface(selected)) return -1;
        draft.bounds(selected, getWidth(), getHeight(), density, box);
        float reach = PadLayout.MIN_TOUCH_DP / 2 * density, outline = OUTLINE_DP * density;
        float insideX = Math.min(reach, box[2] / 4), insideY = Math.min(reach, box[3] / 4);
        boolean inside = x > box[0] && x < box[0] + box[2] && y > box[1] && y < box[1] + box[3];
        int best = -1;
        float bestDistance = Float.MAX_VALUE;
        for (int corner = 0; corner < 4; corner++) {
            boolean right = (corner & PadEditor.RIGHT_CORNER) != 0, bottom = (corner & PadEditor.BOTTOM_CORNER) != 0;
            float cornerX = right ? box[0] + box[2] : box[0], cornerY = bottom ? box[1] + box[3] : box[1];
            float handleX = cornerX + (right ? outline : -outline), handleY = cornerY + (bottom ? outline : -outline);
            if (Math.abs(x - handleX) > reach || Math.abs(y - handleY) > reach) continue;
            if (inside && (Math.abs(x - cornerX) > insideX || Math.abs(y - cornerY) > insideY)) continue;
            float distance = (float) Math.hypot(x - handleX, y - handleY);
            if (distance < bestDistance) {
                best = corner;
                bestDistance = distance;
            }
        }
        return best;
    }

    @SuppressLint("ClickableViewAccessibility") // Editing gestures; the dock's buttons are the accessible path.
    @Override
    public boolean onTouchEvent(MotionEvent event) {
        PadLayout.Control selected = editor.selected();
        switch (event.getActionMasked()) {
            case MotionEvent.ACTION_DOWN: {
                pinching = false;
                changing = false;
                movedX = movedY = false;
                downX = event.getX();
                downY = event.getY();
                snap.release();
                // A corner handle of the picked control wins over whatever lies under it.
                resizing = corner(event.getX(), event.getY());
                if (resizing >= 0) {
                    dragging = false;
                    draft.bounds(selected, getWidth(), getHeight(), density, box);
                    boolean right = (resizing & PadEditor.RIGHT_CORNER) != 0, bottom = (resizing & PadEditor.BOTTOM_CORNER) != 0;
                    // The dragged corner follows the finger from where it took hold; the opposite one stays.
                    offsetX = (right ? box[0] + box[2] : box[0]) - event.getX();
                    offsetY = (bottom ? box[1] + box[3] : box[1]) - event.getY();
                    anchorX = right ? box[0] : box[0] + box[2];
                    anchorY = bottom ? box[1] : box[1] + box[3];
                    break;
                }
                PadLayout.Control control = hit(event.getX(), event.getY());
                editor.select(control);
                dragging = control != null;
                if (dragging) {
                    draft.bounds(control, getWidth(), getHeight(), density, box);
                    offsetX = box[0] + box[2] / 2 - event.getX();
                    offsetY = box[1] + box[3] / 2 - event.getY();
                }
                break;
            }
            case MotionEvent.ACTION_POINTER_DOWN:
                if (selected != null && event.getPointerCount() == 2) {
                    pinching = true;
                    dragging = false;
                    resizing = -1;
                    snap.release();
                    pinchDistance = distance(event);
                    pinchWidth = draft.spec(selected).widthScale;
                    pinchHeight = draft.spec(selected).heightScale;
                }
                break;
            case MotionEvent.ACTION_MOVE:
                if (pinching && event.getPointerCount() >= 2 && pinchDistance > 0) {
                    change(selected);
                    draft.scaleFrom(selected, pinchWidth, pinchHeight, distance(event) / pinchDistance, getWidth(),
                            getHeight(), density);
                    editor.carryTwin(selected);
                    editor.changed();
                } else if ((resizing >= 0 || dragging) && selected != null) {
                    // A tap that trembles moves nothing, and a drag straight along one axis leaves the other alone:
                    // each axis follows once the finger has really set off along it.
                    movedX |= Math.abs(event.getX() - downX) >= slop;
                    movedY |= Math.abs(event.getY() - downY) >= slop;
                    if (!movedX && !movedY) break;
                    if (resizing >= 0) {
                        resize(selected, event.getX() + offsetX, event.getY() + offsetY);
                    } else {
                        drag(selected, event.getX() + offsetX, event.getY() + offsetY);
                    }
                }
                break;
            case MotionEvent.ACTION_POINTER_UP:
                // The pinch ends; the remaining finger does not jump the control.
                pinching = false;
                break;
            case MotionEvent.ACTION_UP:
            case MotionEvent.ACTION_CANCEL:
                dragging = false;
                pinching = false;
                resizing = -1;
                snap.release();
                invalidate();
                editor.stepBack(false);
                editor.keepPanelClear();
                editor.updatePanel();
                break;
            default:
                break;
        }
        return true;
    }

    /**
     * The gesture is about to change {@code control} for the first time: Undo's step, the twin a lock carries, and
     * the dock steps back until the finger lifts, so the control and its guides show through it.
     */
    private void change(PadLayout.Control control) {
        if (changing) return;
        changing = true;
        editor.beginChange(control);
        editor.stepBack(true);
    }

    /**
     * Tells {@link #snap} the lines {@code selected} can share: every other control on the surface and its twin's
     * mirror. A twin the mirror lock carries moves with it, so it offers no line.
     */
    private void lines(PadLayout.Control selected) {
        float width = getWidth(), height = getHeight();
        snap.begin(width, height);
        PadLayout.Control carried = editor.carried(), twin = PadSnapGrid.twin(selected);
        for (PadLayout.Control control : PadLayout.Control.values()) {
            if (control == selected || control == carried || !editor.onSurface(control)) continue;
            draft.bounds(control, width, height, density, other);
            snap.neighbour(other[0], other[1], other[2], other[3]);
            if (control == twin) snap.twin(other[0], other[1], other[2], other[3]);
        }
    }

    /** One light tick as an axis takes hold of a line it did not hold a step ago. */
    private void tick(float heldX, float heldY) {
        boolean taken = (!Float.isNaN(snap.guideX) && snap.guideX != heldX)
                || (!Float.isNaN(snap.guideY) && snap.guideY != heldY);
        if (taken) Haptics.feedback(this, Haptics.Event.SNAP_GRID);
    }

    /** Moves the dragged control's centre to where {@link PadSnapGrid#move} lands ({@code x}, {@code y}). */
    private void drag(PadLayout.Control selected, float x, float y) {
        float width = getWidth(), height = getHeight(), heldX = snap.guideX, heldY = snap.guideY;
        change(selected);
        draft.bounds(selected, width, height, density, box);
        lines(selected);
        snap.move(x, y, box[2], box[3]);
        if (!movedX) snap.keepX(box[0] + box[2] / 2);
        if (!movedY) snap.keepY(box[1] + box[3] / 2);
        draft.moveTo(selected, snap.x, snap.y, width, height, density);
        editor.carryTwin(selected);
        tick(heldX, heldY);
        invalidate();
    }

    /**
     * Puts the dragged corner where {@link PadSnapGrid#size} lands ({@code x}, {@code y}) and sizes the picked
     * control's width and height between it and the opposite corner, which stays: never smaller than a full touch,
     * never past the surface.
     */
    private void resize(PadLayout.Control selected, float x, float y) {
        float width = getWidth(), height = getHeight(), heldX = snap.guideX, heldY = snap.guideY;
        float u = PadLayout.unit(width, height, density);
        float leastWide = PadLayout.width(selected, u, density, PadLayout.MIN_SCALE);
        float leastHigh = PadLayout.height(selected, u, density, PadLayout.MIN_SCALE);
        boolean right = (resizing & PadEditor.RIGHT_CORNER) != 0, bottom = (resizing & PadEditor.BOTTOM_CORNER) != 0;
        change(selected);
        draft.bounds(selected, width, height, density, box);
        lines(selected);
        snap.size(x, y, anchorX, anchorY, right, bottom, leastWide, leastHigh);
        if (!movedX) snap.keepX(right ? box[0] + box[2] : box[0]);
        if (!movedY) snap.keepY(bottom ? box[1] + box[3] : box[1]);
        float left = right ? anchorX : snap.x, top = bottom ? anchorY : snap.y;
        draft.setBounds(selected, left, top, Math.abs(snap.x - anchorX), Math.abs(snap.y - anchorY), width, height,
                density);
        editor.carryTwin(selected);
        tick(heldX, heldY);
        editor.changed();
    }

    private float distance(MotionEvent event) {
        return (float) Math.hypot(event.getX(0) - event.getX(1), event.getY(0) - event.getY(1));
    }
}
