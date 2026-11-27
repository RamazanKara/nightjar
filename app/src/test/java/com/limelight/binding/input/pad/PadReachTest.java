package com.limelight.binding.input.pad;

import android.app.Application;
import android.os.SystemClock;
import android.view.MotionEvent;
import android.view.View;

import com.limelight.binding.input.SteamControls;
import com.limelight.nvstream.jni.MoonBridge;

import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.annotation.Config;

import java.util.ArrayList;
import java.util.List;

import static org.junit.Assert.*;

/**
 * The on-screen controls' touch area (Nightjar #172): at the default a control takes touches on its box alone, as
 * before; a larger area lets a finger around the box work it, and the layout keeps the area for older builds too.
 */
@RunWith(RobolectricTestRunner.class)
@Config(sdk = 30, application = Application.class, qualifiers = "w800dp-h360dp-land-xhdpi")
public class PadReachTest extends ModernControllerHarness {
    private ModernController withReach(PadLayout.Control control, float reach) {
        ModernController first = controller(GAME);
        PadLayout draft = first.layout().copy();
        draft.setReach(control, reach);
        first.save(draft, false);
        first.destroy();
        return controller(GAME);
    }

    private static MotionEvent touch(long down, long at, int action, float x, float y) {
        return MotionEvent.obtain(down, at, action, x, y, 0);
    }

    @Test public void theDefaultAreaIsTheBoxAndSavesNothingNew() {
        ModernController controller = controller(GAME);
        for (PadLayout.Control control : PadLayout.Control.values()) {
            PadControlView view = controller.view(control);
            if (view != null) assertEquals(control.name(), View.GONE, view.reachLayer().getVisibility());
        }
        PadLayout draft = controller.layout().copy();
        draft.setScale(PadLayout.Control.FACE, 1.2f);
        assertEquals(PadLayout.MIN_REACH, draft.spec(PadLayout.Control.FACE).reach, 0);
        assertFalse("No ninth field without a touch area", draft.serialize().matches(".*FACE(,[^|,]*){8}.*"));
    }

    @Test public void theAreaRoundTripsAndAnOlderReaderKeepsTheRest() {
        PadLayout layout = new PadLayout();
        layout.setReach(PadLayout.Control.LB, 1.75f);
        layout.setReach(PadLayout.Control.RB, 9f);
        String text = layout.serialize();
        PadLayout back = PadLayout.parse(text);
        assertEquals(1.75f, back.spec(PadLayout.Control.LB).reach, 1e-3);
        assertEquals("kept within the slider's range", PadLayout.MAX_REACH, back.spec(PadLayout.Control.RB).reach, 1e-3);
        assertEquals(1f, back.spec(PadLayout.Control.LB).widthScale, 1e-3);
        // A build before the field reads the first eight and drops the ninth: the box stays as it was.
        PadLayout old = PadLayout.parse(text.replaceAll("(LB(,[^|,]*){7}),[^|,]*", "$1"));
        assertEquals(PadLayout.MIN_REACH, old.spec(PadLayout.Control.LB).reach, 0);
        assertEquals(1f, old.spec(PadLayout.Control.LB).scale(), 1e-3);
    }

    @Test public void aFingerBesideAButtonPressesItWithinItsArea() {
        ModernController controller = withReach(PadLayout.Control.LB, 2f);
        PadControlView lb = controller.view(PadLayout.Control.LB);
        View area = lb.reachLayer();
        assertEquals(View.VISIBLE, area.getVisibility());
        assertTrue("the area is larger than the box", area.getWidth() > lb.getWidth() && area.getHeight() > lb.getHeight());
        long t = SystemClock.uptimeMillis();
        float x = (lb.getLeft() - area.getLeft()) / 2f, y = area.getHeight() / 2f;
        area.dispatchTouchEvent(touch(t, t, MotionEvent.ACTION_DOWN, x, y));
        assertNotEquals("LB holds from beside its box", 0, last()[0]);
        area.dispatchTouchEvent(touch(t, t + 50, MotionEvent.ACTION_UP, x, y));
        assertEquals("and lets go with the finger", 0, last()[0]);
    }

    /** #150 holds in the area too: the touch goes down at zero pressure, and a tap clicks by a click-pressure MOVE. */
    @Test public void aTapInATrackpadsAreaClicksItAsOnThePadItself() {
        prefs.oscSteamControls = true;
        ModernController controller = withReach(PadLayout.Control.LEFT_PAD, 1.5f);
        List<float[]> touches = new ArrayList<>();
        controller.setTouchSink((type, pointer, x, y, pressure) -> touches.add(new float[] {type, x, y, pressure}));
        View area = controller.view(PadLayout.Control.LEFT_PAD).reachLayer();
        assertEquals(View.VISIBLE, area.getVisibility());
        long t = SystemClock.uptimeMillis();
        float x = 2, y = area.getHeight() / 2f;
        area.dispatchTouchEvent(touch(t, t, MotionEvent.ACTION_DOWN, x, y));
        assertFalse(touches.isEmpty());
        assertEquals(0f, touches.get(touches.size() - 1)[3], 0);
        area.dispatchTouchEvent(touch(t, t + 100, MotionEvent.ACTION_UP, x, y));
        float[] click = touches.get(touches.size() - 1);
        assertEquals(MoonBridge.LI_TOUCH_EVENT_MOVE, (int) click[0]);
        assertEquals(SteamControls.PAD_CLICK_PRESSURE, click[3], 1e-3);
        assertEquals(SteamControls.LEFT_PAD_CLICK_FLAG, last()[0] & SteamControls.LEFT_PAD_CLICK_FLAG);
    }

    @Test public void theEditorAndHidingTakeTheAreaAway() {
        ModernController controller = withReach(PadLayout.Control.RB, 1.5f);
        View area = controller.view(PadLayout.Control.RB).reachLayer();
        assertEquals(View.VISIBLE, area.getVisibility());
        controller.startEditing();
        assertEquals(View.GONE, area.getVisibility());
        controller.finishEditing(false);
        assertEquals(View.VISIBLE, area.getVisibility());
        controller.hide();
        assertEquals(View.GONE, area.getVisibility());
    }
}
