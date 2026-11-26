package com.limelight.ui;

import static org.junit.Assert.assertArrayEquals;
import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertNotEquals;
import static org.junit.Assert.assertTrue;

import android.app.Application;
import android.os.Looper;
import android.preference.PreferenceManager;
import android.view.Gravity;
import android.view.View;
import android.widget.TextView;

import androidx.appcompat.app.AlertDialog;

import com.google.android.material.color.MaterialColors;
import com.limelight.R;

import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.Robolectric;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.RuntimeEnvironment;
import org.robolectric.Shadows;
import org.robolectric.annotation.Config;
import org.robolectric.annotation.GraphicsMode;

/**
 * The pairing panel: the PIN in four large cells of tabular digits that fit a small phone, the PC and where to type it, the
 * wait, Cancel; at the foot of an upright phone and in the middle elsewhere; and a failure in the danger colour.
 */
@RunWith(RobolectricTestRunner.class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = 34, application = Application.class, qualifiers = "w412dp-h915dp-port-night-xhdpi")
public class PairingCinemaSheetTest {
    private static void idle() {
        Shadows.shadowOf(Looper.getMainLooper()).idle();
    }

    private static String text(AlertDialog dialog, int id) {
        return ((TextView) dialog.findViewById(id)).getText().toString();
    }

    @Test
    public void onlyAPhoneHeldUprightGetsThePanelAtItsFoot() {
        assertTrue(PairingCinemaSheet.atFoot(GlassForm.PHONE, true));
        assertFalse("a short window has no room under its middle", PairingCinemaSheet.atFoot(GlassForm.PHONE, false));
        assertFalse(PairingCinemaSheet.atFoot(GlassForm.HANDHELD, false));
        assertFalse(PairingCinemaSheet.atFoot(GlassForm.TELEVISION, false));
        assertFalse(PairingCinemaSheet.atFoot(GlassForm.TELEVISION, true));
    }

    @Test
    public void aCellIsLargeLargerAwayFromAPhoneAndGrowsWithTheFont() {
        assertArrayEquals(new int[] {128, 160, 80}, PairingCinemaSheet.cell(GlassForm.PHONE, 2f, 2f));
        assertArrayEquals(new int[] {144, 176, 96}, PairingCinemaSheet.cell(GlassForm.TELEVISION, 2f, 2f));
        assertArrayEquals(PairingCinemaSheet.cell(GlassForm.TELEVISION, 2f, 2f), PairingCinemaSheet.cell(GlassForm.HANDHELD, 2f, 2f));
        for (float font : new float[] {.85f, 1f, 1.3f, 2f}) {
            int[] cell = PairingCinemaSheet.cell(GlassForm.PHONE, 2f, 2f * font);
            assertTrue("a digit's line fits its cell at font scale " + font, cell[1] >= cell[2] * 1.5f);
            assertTrue("and its width", cell[0] >= cell[2] * 1.2f);
            assertTrue("never under a touch target's size", cell[0] >= 96 && cell[1] >= 96);
        }
    }

    @Test
    public void fourCellsShareTheRoomOfASmallPhone() {
        assertEquals("room to spare: the cell's own width", 64, PinSlots.cellWidth(332, 4, 64, 8));
        assertEquals("a 320 dp phone: narrower cells, the same gaps", 54, PinSlots.cellWidth(240, 4, 64, 8));
        assertEquals(0, PinSlots.cellWidth(10, 4, 64, 8));
        assertEquals(0, PinSlots.cellWidth(100, 0, 64, 8));
        for (int room = 0; room < 600; room += 7) {
            for (int slots = 1; slots <= 8; slots++) {
                int width = PinSlots.cellWidth(room, slots, 64, 8);
                assertTrue(width >= 0 && width <= 64);
                if (width > 0) assertTrue("the row stays in its room", width * slots + 8 * (slots - 1) <= room);
            }
        }
    }

    @Test
    public void thePinStandsInFourEqualCellsOfTabularDigitsInsideItsRow() {
        try (var controller = Robolectric.buildActivity(MotionTest.Screen.class).setup()) {
            PinSlots cells = new PinSlots(controller.get());
            int[] cell = PairingCinemaSheet.cell(GlassForm.PHONE, 2f, 2f);
            cells.setCells(cell[0], cell[1], cell[2]);
            cells.setRange(4, 4);
            cells.show("0417");
            // A 320 dp phone leaves the row 240 dp.
            cells.measure(View.MeasureSpec.makeMeasureSpec(480, View.MeasureSpec.EXACTLY), View.MeasureSpec.makeMeasureSpec(0, View.MeasureSpec.UNSPECIFIED));
            cells.layout(0, 0, 480, cells.getMeasuredHeight());
            assertEquals(4, cells.getChildCount());
            int gap = controller.get().getResources().getDimensionPixelSize(R.dimen.space_s);
            int width = PinSlots.cellWidth(480, 4, cell[0], gap);
            int before = -gap;
            for (int i = 0; i < 4; i++) {
                TextView digit = (TextView) cells.getChildAt(i);
                assertEquals("equal cells", width, digit.getWidth());
                assertEquals(cell[1], digit.getHeight());
                assertEquals("a gap between two cells", before + gap, digit.getLeft());
                assertTrue("inside the row", digit.getLeft() >= 0 && digit.getRight() <= 480);
                assertEquals("tabular figures", "tnum", digit.getFontFeatureSettings());
                assertEquals((float) cell[2], digit.getTextSize(), .5f);
                assertEquals(String.valueOf("0417".charAt(i)), digit.getText().toString());
                assertFalse("nothing is typed here: no cell is ringed", digit.isActivated());
                assertTrue("the digit fits its cell", digit.getPaint().measureText("8") < digit.getWidth());
                before = digit.getRight();
            }
            assertEquals("0417", cells.shown().toString());
        }
    }

    @Test
    public void thePinIsSpokenDigitByDigit() {
        assertEquals("0 4 1 7", PairingCinemaSheet.spoken("0417"));
        assertEquals("", PairingCinemaSheet.spoken(""));
    }

    @Test
    public void onAnUprightPhoneThePanelStandsAtTheFootWithThePcItsPinAndWhereToTypeIt() {
        try (var controller = Robolectric.buildActivity(MotionTest.Screen.class).setup().visible()) {
            int[] cancelled = {0};
            AlertDialog dialog = PairingCinemaSheet.show(controller.get(), "Tower", null, "0417", () -> cancelled[0]++);
            idle();
            assertTrue(dialog.isShowing());
            assertEquals(Gravity.BOTTOM, dialog.getWindow().getAttributes().gravity & Gravity.VERTICAL_GRAVITY_MASK);
            assertEquals("Tower", text(dialog, R.id.pairing_cinema_pc));
            PinSlots cells = dialog.findViewById(R.id.pairing_cinema_pin);
            assertEquals("0417", cells.shown().toString());
            assertEquals(4, cells.getChildCount());
            assertEquals("0 4 1 7", cells.getContentDescription().toString());
            assertEquals(80f, ((TextView) cells.getChildAt(0)).getTextSize(), .5f);
            assertEquals(controller.get().getString(R.string.pair_pairing_help), text(dialog, R.id.pairing_cinema_where));
            assertEquals(controller.get().getString(R.string.pairing), text(dialog, R.id.pairing_cinema_state));
            assertEquals(View.VISIBLE, dialog.findViewById(R.id.pairing_cinema_spinner).getVisibility());
            assertEquals(View.VISIBLE, dialog.getButton(AlertDialog.BUTTON_NEGATIVE).getVisibility());
            assertNotEquals("Cancel is the only way on", View.VISIBLE, dialog.getButton(AlertDialog.BUTTON_POSITIVE).getVisibility());

            dialog.getButton(AlertDialog.BUTTON_NEGATIVE).performClick();
            idle();
            assertEquals(1, cancelled[0]);
            assertFalse(dialog.isShowing());
        }
    }

    @Test
    public void backCancelsThePairing() {
        try (var controller = Robolectric.buildActivity(MotionTest.Screen.class).setup().visible()) {
            int[] cancelled = {0};
            AlertDialog dialog = PairingCinemaSheet.show(controller.get(), "Tower", null, "0417", () -> cancelled[0]++);
            idle();
            dialog.onBackPressed();
            idle();
            assertEquals(1, cancelled[0]);
            assertFalse(dialog.isShowing());
        }
    }

    @Test
    public void aPairingThatNeedsNothingTypedOnlyWaits() {
        try (var controller = Robolectric.buildActivity(MotionTest.Screen.class).setup().visible()) {
            AlertDialog dialog = PairingCinemaSheet.show(controller.get(), "Tower", "9041", "9041", () -> { });
            idle();
            assertEquals(View.GONE, dialog.findViewById(R.id.pairing_cinema_pin).getVisibility());
            assertEquals(View.GONE, dialog.findViewById(R.id.pairing_cinema_where).getVisibility());
            assertEquals(controller.get().getString(R.string.pairing), text(dialog, R.id.pairing_cinema_state));
            assertEquals(View.VISIBLE, dialog.findViewById(R.id.pairing_cinema_wait).getVisibility());
            dialog.dismiss();
        }
    }

    @Test
    @Config(qualifiers = "w960dp-h540dp-land-night-xhdpi")
    public void heldSidewaysThePanelStandsInTheMiddleWithLargerCells() {
        try (var controller = Robolectric.buildActivity(MotionTest.Screen.class).setup().visible()) {
            assertEquals(GlassForm.HANDHELD, GlassForm.of(controller.get()));
            AlertDialog dialog = PairingCinemaSheet.show(controller.get(), null, null, "0417", () -> { });
            idle();
            assertNotEquals(Gravity.BOTTOM, dialog.getWindow().getAttributes().gravity & Gravity.VERTICAL_GRAVITY_MASK);
            PinSlots cells = dialog.findViewById(R.id.pairing_cinema_pin);
            assertEquals(96f, ((TextView) cells.getChildAt(0)).getTextSize(), .5f);
            assertEquals("a PC with no name leaves no empty line", View.GONE, dialog.findViewById(R.id.pairing_cinema_pc).getVisibility());
            dialog.dismiss();
        }
    }

    @Test
    public void withReducedMotionTheWordSaysItAlone() {
        PreferenceManager.getDefaultSharedPreferences(RuntimeEnvironment.getApplication())
                .edit().putString(Motion.REDUCE_MOTION, Motion.REDUCE).commit();
        try (var controller = Robolectric.buildActivity(MotionTest.Screen.class).setup().visible()) {
            AlertDialog dialog = PairingCinemaSheet.show(controller.get(), "Tower", null, "0417", () -> { });
            idle();
            assertEquals(View.GONE, dialog.findViewById(R.id.pairing_cinema_spinner).getVisibility());
            assertEquals(View.VISIBLE, dialog.findViewById(R.id.pairing_cinema_state).getVisibility());
            dialog.dismiss();
        }
    }

    @Test
    public void aFailureStandsWhereThePinStoodInTheDangerColourWithTryAgain() {
        try (var controller = Robolectric.buildActivity(MotionTest.Screen.class).setup().visible()) {
            int[] retried = {0};
            AlertDialog dialog = PairingCinemaSheet.showFailure(controller.get(), "Pair with PC", "The PIN was wrong.", () -> retried[0]++);
            idle();
            assertTrue(dialog.isShowing());
            assertEquals(Gravity.BOTTOM, dialog.getWindow().getAttributes().gravity & Gravity.VERTICAL_GRAVITY_MASK);
            TextView reason = dialog.findViewById(android.R.id.message);
            assertEquals("The PIN was wrong.", reason.getText().toString());
            assertEquals(MaterialColors.getColor(reason, androidx.appcompat.R.attr.colorError), reason.getCurrentTextColor());
            assertNotEquals(MaterialColors.getColor(reason, com.google.android.material.R.attr.colorOnSurface), reason.getCurrentTextColor());
            assertEquals(controller.get().getString(R.string.pair_try_again), dialog.getButton(AlertDialog.BUTTON_POSITIVE).getText().toString());
            assertEquals(View.VISIBLE, dialog.getButton(AlertDialog.BUTTON_NEGATIVE).getVisibility());
            dialog.getButton(AlertDialog.BUTTON_POSITIVE).performClick();
            idle();
            assertEquals(1, retried[0]);
        }
    }

    @Test
    public void aFailureWithNothingToTryAgainOnlyCloses() {
        try (var controller = Robolectric.buildActivity(MotionTest.Screen.class).setup().visible()) {
            AlertDialog dialog = PairingCinemaSheet.showFailure(controller.get(), "Pair with PC", "The PC is offline.", null);
            idle();
            assertEquals(controller.get().getString(android.R.string.ok), dialog.getButton(AlertDialog.BUTTON_POSITIVE).getText().toString());
            assertNotEquals(View.VISIBLE, dialog.getButton(AlertDialog.BUTTON_NEGATIVE).getVisibility());
            dialog.getButton(AlertDialog.BUTTON_POSITIVE).performClick();
            idle();
            assertFalse(dialog.isShowing());
        }
    }
}
