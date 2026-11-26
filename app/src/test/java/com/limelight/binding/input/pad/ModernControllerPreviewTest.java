package com.limelight.binding.input.pad;

import android.app.Application;
import android.graphics.Bitmap;
import android.graphics.Canvas;
import android.graphics.Rect;
import android.graphics.drawable.GradientDrawable;
import android.os.Looper;
import android.view.MotionEvent;
import android.view.View;
import android.view.ViewGroup;
import android.widget.FrameLayout;

import com.limelight.binding.input.HeldInputState;
import com.limelight.preferences.PreferenceConfiguration;
import com.limelight.ui.MicButton;
import com.limelight.ui.UiNavigationTest;

import org.junit.Assume;
import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.Robolectric;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.RuntimeEnvironment;
import org.robolectric.Shadows;
import org.robolectric.annotation.Config;
import org.robolectric.annotation.GraphicsMode;

import java.io.File;
import java.io.FileOutputStream;
import java.io.IOException;
import java.util.ArrayList;
import java.util.List;

import static org.junit.Assert.*;

/**
 * Optional previews of the modern controller, its editor and the microphone button over a stand-in
 * stream. Set NIGHTJAR_PREVIEW_OUTPUT to a directory to capture PNGs; every capture also checks that
 * the controls keep apart, stay on the surface and are at least 48 dp to touch.
 */
@RunWith(RobolectricTestRunner.class)
@Config(sdk = 34, application = Application.class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
public class ModernControllerPreviewTest {
    private static final HeldInputState.Sink SINK = new HeldInputState.Sink() {
        @Override public void key(int key, boolean down) { }
        @Override public void mouse(int button, boolean down) { }
        @Override public void gamepad(int buttons, short lx, short ly, short rx, short ry, byte lt, byte rt) { }
    };

    @Test @Config(qualifiers = "w800dp-h360dp-land-notnight-xhdpi")
    public void phoneLandscapeLight() throws IOException { capture("phone-land-light"); }

    @Test @Config(qualifiers = "w800dp-h360dp-land-night-xhdpi")
    public void phoneLandscapeDark() throws IOException { capture("phone-land-dark"); }

    @Test @Config(qualifiers = "w360dp-h800dp-port-notnight-xhdpi")
    public void phonePortraitLight() throws IOException { capture("phone-port-light"); }

    @Test @Config(qualifiers = "w360dp-h800dp-port-night-xhdpi")
    public void phonePortraitDark() throws IOException { capture("phone-port-dark"); }

    @Test @Config(qualifiers = "w1280dp-h800dp-land-notnight-mdpi")
    public void tabletLight() throws IOException { capture("tablet-light"); }

    @Test @Config(qualifiers = "w1280dp-h800dp-land-night-mdpi")
    public void tabletDark() throws IOException { capture("tablet-dark"); }

    private static final class FakeMic implements MicButton.Host {
        boolean muted;
        float rowBottom = Float.NaN;
        @Override public boolean offered() { return true; }
        @Override public boolean muted() { return muted; }
        @Override public void setMuted(boolean muted) { this.muted = muted; }
        @Override public float topRowBottom() { return rowBottom; }
    }

    private void capture(String name) throws IOException {
        File directory = outputDirectory();
        // Each capture shows the editor as it settles, not partway through a change.
        PadEditor.transitions = false;
        try {
            capture(name, directory);
        } finally {
            PadEditor.transitions = true;
        }
    }

    private void capture(String name, File directory) throws IOException {
        RuntimeEnvironment.getApplication().getSharedPreferences("mic_button", 0).edit().clear().commit();
        ModernController.clearLayouts(RuntimeEnvironment.getApplication());
        UiNavigationTest.Screen screen = Robolectric.buildActivity(UiNavigationTest.Screen.class).setup().visible().get();
        android.util.DisplayMetrics metrics = screen.getResources().getDisplayMetrics();
        int width = Math.round(screen.getResources().getConfiguration().screenWidthDp * metrics.density);
        int height = Math.round(screen.getResources().getConfiguration().screenHeightDp * metrics.density);
        FrameLayout frame = new FrameLayout(screen);
        // A stand-in stream: a dark scene that brightens to a pale sky, so the glass is seen on both.
        frame.setBackground(new GradientDrawable(GradientDrawable.Orientation.BR_TL,
                new int[] {0xFF10161F, 0xFF3B4F63, 0xFFB9C9D6}));
        screen.setContentView(frame, new FrameLayout.LayoutParams(width, height));
        PreferenceConfiguration prefs = new PreferenceConfiguration();
        prefs.oscOpacity = 90;
        prefs.showGuideButton = true;
        prefs.showTouchpadButton = true;
        ModernController controller = new ModernController(screen, frame, SINK, prefs, "pc/app");
        controller.refreshLayout();
        FakeMic mic = new FakeMic();
        MicButton button = new MicButton(screen, mic);
        frame.addView(button, button.layoutParams());
        button.setSuppressed(false);
        layout(frame, width, height);
        mic.rowBottom = controller.topRowBottom();
        button.place();
        layout(frame, width, height);
        List<String> problems = new ArrayList<>();
        sane(controller, button, frame, metrics.density, problems);
        write(frame, width, height, new File(directory, name + "-1-idle.png"));

        // Held: A on the face cluster, the left stick pushed up and right, the right one left, RT pulled, the microphone muted.
        PadControlView face = controller.view(PadLayout.Control.FACE);
        float faceSize = Math.min(face.getWidth(), face.getHeight());
        face.down(0, .5f * faceSize, .78f * faceSize, 100);
        PadControlView stick = controller.view(PadLayout.Control.LEFT_STICK);
        float stickSize = Math.min(stick.getWidth(), stick.getHeight());
        stick.down(1, stickSize * .45f, stickSize * .55f, 100);
        stick.move(1, stickSize * .45f + PadPainter.travel(stickSize) * .7f, stickSize * .55f - PadPainter.travel(stickSize) * .7f);
        // The right stick pushed fully left: its knob's centre on the rim, half the knob past the base.
        PadControlView right = controller.view(PadLayout.Control.RIGHT_STICK);
        right.down(3, right.getWidth() / 2f, right.getHeight() / 2f, 100);
        right.move(3, right.getWidth() / 2f - 2 * PadPainter.travel(stickSize), right.getHeight() / 2f);
        controller.view(PadLayout.Control.RT).down(2, 5, 5, 100);
        button.performClick();
        assertTrue(mic.muted);
        write(frame, width, height, new File(directory, name + "-2-pressed-muted.png"));
        controller.releaseInput();

        controller.startEditing();
        layout(frame, width, height);
        Shadows.shadowOf(Looper.getMainLooper()).idle();
        layout(frame, width, height);
        PadEditor editor = (PadEditor) frame.getChildAt(frame.getChildCount() - 1);
        // The dock is its bar alone until there is something to show.
        touchTargets(editor, metrics.density, problems);
        write(frame, width, height, new File(directory, name + "-3-editor-all.png"));
        panelInside(editor, "all", problems);
        // More: every control's size and opacity, the glyphs and where Done saves.
        editor.findViewById(com.limelight.R.id.pad_studio_more).performClick();
        layout(frame, width, height);
        touchTargets(editor, metrics.density, problems);
        panelInside(editor, "more", problems);
        write(frame, width, height, new File(directory, name + "-9-editor-more.png"));
        // A control picked, with the mirror lock on: its tool is lit.
        editor.select(PadLayout.Control.FACE);
        editor.toggleHidden();
        editor.select(PadLayout.Control.DPAD);
        editor.findViewById(com.limelight.R.id.pad_studio_mirror).performClick();
        layout(frame, width, height);
        touchTargets(editor, metrics.density, problems);
        write(frame, width, height, new File(directory, name + "-4-editor-picked.png"));
        panelInside(editor, "picked", problems);
        controller.finishEditing(false);

        // Stealth: at 0 % the pad draws nothing, and the microphone button is all that stands on the video, muted
        // and live.
        prefs.oscOpacity = 0;
        controller.refreshLayout();
        layout(frame, width, height);
        write(frame, width, height, new File(directory, name + "-0-stealth-muted.png"));
        button.performClick();
        assertFalse(mic.muted);
        write(frame, width, height, new File(directory, name + "-0-stealth-live.png"));
        button.performClick();
        assertTrue(mic.muted);
        prefs.oscOpacity = 90;

        // What the look adds beyond the default layout: PlayStation's glyphs on a split face cluster and the
        // pointer touchpad in Camera mode, pressed: the circle button, the D-pad up and right, a shoulder, each with its
        // halo, and the pad with its trail. The split members and the pad at its default overlap nothing, the
        // microphone button included.
        PadLayout extras = controller.layout().copy();
        extras.setGlyph(PadLayout.Glyph.PLAYSTATION);
        extras.spec(PadLayout.Control.POINTER_PAD).hidden = false;
        extras.setPointerMode(PadLayout.PointerMode.CAMERA);
        extras.split(PadLayout.Control.FACE, width, height, metrics.density);
        controller.save(extras, false);
        controller.refreshLayout();
        layout(frame, width, height);
        mic.rowBottom = controller.topRowBottom();
        button.place();
        layout(frame, width, height);
        sane(controller, button, frame, metrics.density, problems);
        PadControlView circle = controller.view(PadLayout.Control.B);
        circle.down(0, circle.getWidth() / 2f, circle.getHeight() / 2f, 100);
        PadControlView dpad = controller.view(PadLayout.Control.DPAD);
        dpad.down(1, dpad.getWidth() * .8f, dpad.getHeight() * .2f, 100);
        PadControlView lb = controller.view(PadLayout.Control.LB);
        lb.down(2, lb.getWidth() / 2f, lb.getHeight() / 2f, 100);
        PadControlView pad = controller.view(PadLayout.Control.POINTER_PAD);
        float padWidth = pad.getWidth(), padHeight = pad.getHeight();
        pad.down(3, padWidth * .2f, padHeight * .75f, 100);
        for (int i = 1; i <= 10; i++) {
            float t = i / 10f;
            pad.move(3, padWidth * (.2f + .55f * t), padHeight * (.75f - .5f * t * (2 - t)), 100_000_000L + i * 16_000_000L);
        }
        write(frame, width, height, new File(directory, name + "-5-play-extras.png"));
        controller.releaseInput();

        // The editor with controls left out: the Add tiles, grouped.
        controller.startEditing();
        layout(frame, width, height);
        Shadows.shadowOf(Looper.getMainLooper()).idle();
        layout(frame, width, height);
        editor = (PadEditor) frame.getChildAt(frame.getChildCount() - 1);
        editor.select(PadLayout.Control.L3);
        editor.toggleHidden();
        editor.select(PadLayout.Control.HOME);
        editor.toggleHidden();
        editor.select(null);
        editor.findViewById(com.limelight.R.id.pad_studio_add).performClick();
        layout(frame, width, height);
        touchTargets(editor, metrics.density, problems);
        tiles(editor, metrics.density, problems);
        panelInside(editor, "add", problems);
        write(frame, width, height, new File(directory, name + "-6-editor-add.png"));

        // The pointer touchpad picked: its Mouse and Camera cards.
        editor.select(PadLayout.Control.POINTER_PAD);
        layout(frame, width, height);
        touchTargets(editor, metrics.density, problems);
        tiles(editor, metrics.density, problems);
        panelInside(editor, "touchpad", problems);
        write(frame, width, height, new File(directory, name + "-7-editor-touchpad.png"));

        // The right stick dragged until its centre lines up with the left one's: a guide shows the line.
        View surface = editor.getChildAt(0);
        float[] leftBox = editor.draft().bounds(PadLayout.Control.LEFT_STICK, surface.getWidth(), surface.getHeight(),
                metrics.density, new float[4]);
        float[] rightBox = editor.draft().bounds(PadLayout.Control.RIGHT_STICK, surface.getWidth(), surface.getHeight(),
                metrics.density, new float[4]);
        float fromX = rightBox[0] + rightBox[2] / 2, fromY = rightBox[1] + rightBox[3] / 2;
        float toX = fromX - rightBox[2] / 3, toY = leftBox[1] + leftBox[3] / 2 + 2 * metrics.density;
        touch(surface, MotionEvent.ACTION_DOWN, fromX, fromY);
        for (int i = 1; i <= 4; i++) {
            touch(surface, MotionEvent.ACTION_MOVE, fromX + (toX - fromX) * i / 4, fromY + (toY - fromY) * i / 4);
        }
        if (editor.guides() == 0) problems.add("no guide while lined up");
        write(frame, width, height, new File(directory, name + "-8-editor-guide.png"));
        touch(surface, MotionEvent.ACTION_UP, toX, toY);
        controller.finishEditing(false);
        controller.destroy();
        assertTrue(name + ": " + problems, problems.isEmpty());
    }

    private static void touch(View view, int action, float x, float y) {
        long now = android.os.SystemClock.uptimeMillis();
        MotionEvent event = MotionEvent.obtain(now, now, action, x, y, 0);
        view.dispatchTouchEvent(event);
        event.recycle();
    }

    /** The editor's panel lies whole on the surface, and its Done button whole in the panel. */
    private static void panelInside(PadEditor editor, String state, List<String> problems) {
        View panel = editor.panel();
        float left = panel.getX(), top = panel.getY();
        if (left < -.5f || top < -.5f || left + panel.getWidth() > editor.getWidth() + .5f
                || top + panel.getHeight() > editor.getHeight() + .5f) {
            problems.add(state + ": the panel leaves the surface");
        }
        View done = editor.findViewById(com.limelight.R.id.pad_editor_done);
        float y = done.getY();
        for (View parent = (View) done.getParent(); parent != panel; parent = (View) parent.getParent()) y += parent.getY();
        if (done.getHeight() == 0 || y < 0 || y + done.getHeight() > panel.getHeight() + .5f) {
            problems.add(state + ": Done is not whole in the panel");
        }
    }

    /** The Add tiles and the touchpad's mode cards take a 48 dp touch. */
    private static void tiles(View view, float density, List<String> problems) {
        if (view.getVisibility() != View.VISIBLE) return;
        boolean card = view.getId() == com.limelight.R.id.pad_editor_mode_mouse || view.getId() == com.limelight.R.id.pad_editor_mode_camera;
        if (card || view instanceof PadEditor.Tile) {
            if (view.getHeight() < Math.floor(48 * density) || view.getWidth() < Math.floor(48 * density)) {
                problems.add(view.getContentDescription() + " is " + view.getWidth() / density + "x" + view.getHeight() / density + " dp");
            }
        }
        if (view instanceof ViewGroup) {
            ViewGroup group = (ViewGroup) view;
            for (int i = 0; i < group.getChildCount(); i++) tiles(group.getChildAt(i), density, problems);
        }
    }

    /** Every shown control on the surface, at least 48 dp, and apart from the others and the microphone button. */
    private static void sane(ModernController controller, MicButton button, FrameLayout frame, float density, List<String> problems) {
        List<Rect> boxes = new ArrayList<>();
        List<String> names = new ArrayList<>();
        for (PadLayout.Control control : PadLayout.Control.values()) {
            PadControlView view = controller.view(control);
            if (view == null || view.getVisibility() != View.VISIBLE) continue;
            Rect box = new Rect(view.getLeft(), view.getTop(), view.getRight(), view.getBottom());
            if (box.left < 0 || box.top < 0 || box.right > frame.getWidth() || box.bottom > frame.getHeight()) {
                problems.add(control + " off the surface");
            }
            if (Math.min(box.width(), box.height()) < Math.floor(48 * density)) {
                problems.add(control + " is " + Math.min(box.width(), box.height()) / density + " dp");
            }
            boxes.add(box);
            names.add(control.name());
        }
        Rect mic = new Rect(Math.round(button.getTranslationX()), Math.round(button.getTranslationY()),
                Math.round(button.getTranslationX()) + button.getWidth(), Math.round(button.getTranslationY()) + button.getHeight());
        boxes.add(mic);
        names.add("MIC");
        for (int i = 0; i < boxes.size(); i++) {
            for (int j = i + 1; j < boxes.size(); j++) {
                if (Rect.intersects(boxes.get(i), boxes.get(j))) problems.add(names.get(i) + " overlaps " + names.get(j));
            }
        }
    }

    /** The editor panel's buttons and switch take a 48 dp touch. */
    private static void touchTargets(View view, float density, List<String> problems) {
        if (view.getVisibility() != View.VISIBLE) return;
        if (view.isClickable() && view instanceof android.widget.Button) {
            if (view.getHeight() < Math.floor(48 * density) || view.getWidth() < Math.floor(48 * density)) {
                problems.add(((android.widget.Button) view).getText() + " is " + view.getWidth() / density + "x"
                        + view.getHeight() / density + " dp");
            }
        }
        if (view instanceof ViewGroup) {
            ViewGroup group = (ViewGroup) view;
            for (int i = 0; i < group.getChildCount(); i++) touchTargets(group.getChildAt(i), density, problems);
        }
    }

    private static File outputDirectory() {
        String output = System.getenv("NIGHTJAR_PREVIEW_OUTPUT");
        Assume.assumeTrue("Native previews are an explicitly requested local capture", output != null && !output.isBlank());
        File directory = new File(output);
        assertTrue(directory.isDirectory() || directory.mkdirs());
        return directory;
    }

    private static void layout(View view, int width, int height) {
        Shadows.shadowOf(Looper.getMainLooper()).idle();
        view.measure(View.MeasureSpec.makeMeasureSpec(width, View.MeasureSpec.EXACTLY),
                View.MeasureSpec.makeMeasureSpec(height, View.MeasureSpec.EXACTLY));
        view.layout(0, 0, width, height);
    }

    private static void write(View view, int width, int height, File destination) throws IOException {
        layout(view, width, height);
        Bitmap bitmap = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888);
        try (FileOutputStream output = new FileOutputStream(destination)) {
            view.draw(new Canvas(bitmap));
            assertTrue(bitmap.compress(Bitmap.CompressFormat.PNG, 100, output));
        } finally {
            bitmap.recycle();
        }
        assertTrue("A nonempty PNG was written", destination.length() > 1000);
    }
}
