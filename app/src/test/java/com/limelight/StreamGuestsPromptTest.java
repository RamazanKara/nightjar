package com.limelight;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertNotNull;
import static org.junit.Assert.assertNull;
import static org.junit.Assert.assertTrue;

import android.content.DialogInterface;
import android.widget.Button;

import androidx.appcompat.app.AlertDialog;

import com.limelight.nativeapi.NativeApiClient;
import com.limelight.nativeapi.NativeGuests;

import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.Robolectric;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.shadows.ShadowLooper;
import org.robolectric.util.ReflectionHelpers;

import java.util.List;
import java.util.concurrent.CopyOnWriteArrayList;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicLong;

/** Focus and accidental-input protection on guest join prompts. */
@RunWith(RobolectricTestRunner.class)
public class StreamGuestsPromptTest {
    @Test
    public void promptFocusesDenyFirstAndIgnoresClicksDuringInitialSecond() throws Exception {
        Game game = Robolectric.buildActivity(Game.class).get();
        game.setTheme(androidx.appcompat.R.style.Theme_AppCompat);
        List<String> requests = new CopyOnWriteArrayList<>();
        CountDownLatch sent = new CountDownLatch(1);
        NativeApiClient api = new NativeApiClient((host, op, arg) -> {
            requests.add(op + ":" + arg);
            sent.countDown();
            return "{\"ok\":{\"manage\":true,\"self_id\":1,\"self_name\":\"Host\","
                    + "\"permission\":1,\"max_permission\":4,\"keyboard_mouse\":-1,"
                    + "\"guests\":[],\"waiting\":[]}}";
        }, null);
        StreamGuests guests = new StreamGuests(game, api);
        AtomicLong nowMs = new AtomicLong(5_000L);
        guests.clock = nowMs::get;

        guests.askAbout(new NativeGuests.Request(7, "GuestPad", 42));
        ShadowLooper.idleMainLooper();

        AlertDialog dialog = ReflectionHelpers.getField(guests, "prompt");
        assertNotNull(dialog);
        assertTrue(dialog.isShowing());

        Button approve = dialog.getButton(DialogInterface.BUTTON_POSITIVE);
        Button deny = dialog.getButton(DialogInterface.BUTTON_NEGATIVE);
        assertNotNull(approve);
        assertNotNull(deny);
        assertTrue("Deny must hold initial focus", deny.hasFocus());
        assertFalse(approve.hasFocus());

        // Accidental button presses within the first second must keep the dialog open.
        approve.performClick();
        ShadowLooper.idleMainLooper();
        assertTrue(dialog.isShowing());
        assertEquals(Integer.valueOf(7), ReflectionHelpers.getField(guests, "promptFor"));
        assertTrue(requests.isEmpty());

        nowMs.set(5_999L);
        deny.performClick();
        ShadowLooper.idleMainLooper();
        assertTrue(dialog.isShowing());
        assertEquals(Integer.valueOf(7), ReflectionHelpers.getField(guests, "promptFor"));
        assertTrue(requests.isEmpty());

        // Once the guard window has elapsed, Deny dismisses and sends the deny action.
        nowMs.set(6_000L);
        deny.performClick();
        ShadowLooper.idleMainLooper();
        assertFalse(dialog.isShowing());
        assertNull(ReflectionHelpers.getField(guests, "prompt"));
        assertTrue(sent.await(3, TimeUnit.SECONDS));
        assertEquals(1, requests.size());
        assertTrue(requests.get(0).contains("deny"));
    }
}
