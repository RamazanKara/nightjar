package com.limelight.binding.video;

import com.limelight.nativeapi.NativeApiException;
import com.limelight.nativeapi.NativeStreamDoctor;
import com.limelight.nvstream.jni.Json;
import com.limelight.nvstream.jni.NightjarPolicyJni;
import com.limelight.utils.StreamHealth;

import org.junit.Before;
import org.junit.Test;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;

import static org.junit.Assert.*;

/**
 * Stream Doctor 2.0's controller against a fake stream: the host's doctor is read off the UI thread once a second only
 * while the card shows and the host lists the feature; the bitrate step-down goes to the host's doctor or the classic
 * trial as the verdict says, with the host's undo only while it can; the resolution step-down keeps a session-only Undo;
 * a refusal shows the host's words and changes nothing. The last tests run the verdict through the shared policy.
 */
public class StreamDoctorControllerTest {
    private static final String HOST_BITRATE = "[{\"remedy\":\"bitrate_stepdown\",\"target_kbps\":36000,\"on_host\":true}]";
    private static final String CLASSIC_BITRATE = "[{\"remedy\":\"bitrate_stepdown\",\"target_kbps\":36000,\"on_host\":false}]";
    private static final String RESOLUTION =
            "[{\"remedy\":\"resolution_stepdown\",\"width\":1920,\"height\":1080,\"upscaler\":\"sgsr1\"}]";
    private static final String OPEN_REMEDY =
            "{\"id\":\"bitrate_stepdown\",\"configurable\":true,\"target_kbps\":36000,\"reason\":null,\"message\":null}";
    private static final String BUSY_REMEDY = "{\"id\":\"bitrate_stepdown\",\"configurable\":false,\"target_kbps\":null,"
            + "\"reason\":\"trial_active\",\"message\":\"A lower bitrate is already being tried. Undo it first.\"}";
    private static final String FIXED_REMEDY = "{\"id\":\"bitrate_stepdown\",\"configurable\":false,\"target_kbps\":null,"
            + "\"reason\":\"not_writable\",\"message\":\"This encoder keeps its bitrate.\"}";

    private FakeHost host;
    private StreamDoctorController controller;
    private final List<StreamDoctorController.Card> cards = new ArrayList<>();
    private final StreamDoctorController.View view = cards::add;

    @Before
    public void setUp() {
        host = new FakeHost();
        controller = new StreamDoctorController(host);
    }

    @Test
    public void withoutTheFeatureTheHostIsNeverAsked() {
        controller.open(view);
        assertEquals(1, cards.size());
        assertTrue(host.work.isEmpty());
        assertEquals(StreamDoctorController.POLL_MS, host.pendingDelay);
        host.tick();
        assertEquals(2, cards.size());
        assertEquals(0, host.fetches);
    }

    @Test
    public void readsOffTheUiThreadOnceASecondOnlyWhileOpen() {
        host.offered = true;
        controller.open(view);
        assertEquals("never read on the calling thread", 0, host.fetches);
        assertEquals(1, host.work.size());
        host.tick();
        assertEquals("one read at a time", 1, host.work.size());
        host.runWork();
        assertEquals(1, host.fetches);
        assertNotNull(last().host);
        host.tick();
        host.runWork();
        assertEquals(2, host.fetches);

        Runnable stale = host.pending;
        controller.close();
        assertNull("no look is left scheduled", host.pending);
        assertFalse(controller.isOpen());
        int shown = cards.size();
        stale.run();
        assertTrue(host.work.isEmpty());
        assertEquals(shown, cards.size());
    }

    @Test
    public void anAnswerAfterCloseShowsNothing() {
        host.offered = true;
        controller.open(view);
        controller.close();
        host.runWork();
        assertEquals(1, cards.size());
    }

    @Test
    public void hostStepDownAndUndoGoThroughTheHostsDoctor() {
        host.offered = true;
        host.snapshot = snapshot(true, "[]", OPEN_REMEDY);
        host.diagnosis = verdict("network_jitter", HOST_BITRATE);
        controller.open(view);
        host.runWork();
        assertTrue(abilities().optBoolean("bitrate"));

        host.answer = snapshot(true, trial(true), BUSY_REMEDY);
        controller.apply(last().verdict.remedy(StreamDoctorVerdict.BITRATE_STEPDOWN));
        assertTrue(last().busy);
        assertTrue("sent off the UI thread", host.applies.isEmpty());
        host.runWork();
        assertEquals(Arrays.asList("apply null"), host.applies);
        assertEquals(Arrays.asList(36000), host.applied);
        assertTrue("never the classic trial too", host.classic.isEmpty());
        assertFalse(last().busy);
        assertEquals(45000, last().undoKbps);
        assertFalse("no second step-down while one is tried", abilities().optBoolean("bitrate"));
        assertNull(last().hostNote);

        controller.undoBitrate();
        host.runWork();
        assertEquals(Arrays.asList("apply null", "undo t-3"), host.applies);
        assertEquals(Arrays.asList(36000, 45000), host.applied);
    }

    @Test
    public void theHostsTrialIsUndoneOnlyWhileItCanBe() {
        host.offered = true;
        host.snapshot = snapshot(true, trial(false), BUSY_REMEDY);
        controller.open(view);
        host.runWork();
        assertEquals(0, last().undoKbps);
        assertTrue(last().undoReplaced);
        controller.undoBitrate();
        assertTrue(host.work.isEmpty());
        assertTrue(host.applies.isEmpty());
        assertTrue(host.classic.isEmpty());
    }

    @Test
    public void aRefusalShowsTheHostsWordsAndChangesNothing() {
        host.offered = true;
        host.snapshot = snapshot(true, "[]", OPEN_REMEDY);
        host.diagnosis = verdict("network_jitter", HOST_BITRATE);
        host.refusal = new NativeApiException(NativeApiException.Kind.API, "The encoder is restarting. Try again.");
        controller.open(view);
        host.runWork();
        controller.apply(last().verdict.remedy(StreamDoctorVerdict.BITRATE_STEPDOWN));
        host.runWork();
        assertEquals("The encoder is restarting. Try again.", last().failure);
        assertTrue(host.applied.isEmpty());
        assertFalse(last().busy);
        assertEquals(0, last().undoKbps);
    }

    @Test
    public void withoutTheHostsStepDownTheClassicTrialRuns() {
        host.facts.classicBitrate = true;
        host.diagnosis = verdict("network_jitter", CLASSIC_BITRATE);
        controller.open(view);
        assertTrue(abilities().optBoolean("bitrate"));
        controller.apply(last().verdict.remedy(StreamDoctorVerdict.BITRATE_STEPDOWN));
        assertEquals(Arrays.asList(false), host.classic);
        assertTrue(host.work.isEmpty());

        host.facts.classicUndoKbps = 45000;
        host.tick();
        assertEquals(45000, last().undoKbps);
        assertFalse(abilities().optBoolean("bitrate"));
        controller.undoBitrate();
        assertEquals(Arrays.asList(false, true), host.classic);
    }

    @Test
    public void aHostThatListsItsStepDownNeverGetsTheClassicOne() {
        host.offered = true;
        host.facts.classicBitrate = true;
        host.snapshot = snapshot(true, "[]", FIXED_REMEDY);
        controller.open(view);
        host.runWork();
        assertFalse(abilities().optBoolean("bitrate"));
        assertEquals("This encoder keeps its bitrate.", last().hostNote);
    }

    @Test
    public void aGuestGetsNoRemedies() {
        host.offered = true;
        host.facts.resize = true;
        host.snapshot = snapshot(false, trial(true), BUSY_REMEDY);
        controller.open(view);
        host.runWork();
        assertTrue(last().guest);
        assertFalse(abilities().optBoolean("bitrate"));
        assertFalse(abilities().optBoolean("resize"));
        assertEquals(0, last().undoKbps);
    }

    @Test
    public void resolutionStepDownKeepsAnUndoForThisStream() {
        host.facts.resize = true;
        host.diagnosis = verdict("decode_pressure", RESOLUTION);
        controller.open(view);
        assertTrue(abilities().optBoolean("resize"));
        controller.apply(last().verdict.remedy(StreamDoctorVerdict.RESOLUTION_STEPDOWN));
        assertArrayEquals(new int[] {1920, 1080}, host.resizes.get(0));
        assertArrayEquals(new int[] {2560, 1440}, last().undoSize);
        assertFalse("one mode change at a time", abilities().optBoolean("resize"));

        host.facts.width = 1920;
        host.facts.height = 1080;
        host.tick();
        assertArrayEquals(new int[] {2560, 1440}, last().undoSize);
        controller.undoResize();
        assertArrayEquals(new int[] {2560, 1440}, host.resizes.get(1));
        assertNull(last().undoSize);
    }

    @Test
    public void theResolutionUndoEndsWithTheStreamOrAnotherChange() {
        host.facts.resize = true;
        host.diagnosis = verdict("decode_pressure", RESOLUTION);
        controller.open(view);
        controller.apply(last().verdict.remedy(StreamDoctorVerdict.RESOLUTION_STEPDOWN));
        host.stream = new Object();
        host.tick();
        assertNull("a reconnect drops it", last().undoSize);

        controller.apply(last().verdict.remedy(StreamDoctorVerdict.RESOLUTION_STEPDOWN));
        host.facts.width = 1920;
        host.facts.height = 1080;
        host.tick();
        host.facts.width = 1280;
        host.facts.height = 720;
        host.tick();
        assertNull("another change replaced it", last().undoSize);
    }

    @Test
    public void aModeChangeThatDidNotHappenIsSaid() {
        host.facts.resize = true;
        host.diagnosis = verdict("decode_pressure", RESOLUTION);
        host.resizeSends = false;
        controller.open(view);
        controller.apply(last().verdict.remedy(StreamDoctorVerdict.RESOLUTION_STEPDOWN));
        assertEquals(StreamDoctorController.RESIZE_NOT_SENT, last().resizeProblem);
        assertArrayEquals(new int[] {1920, 1080}, last().resized);
        assertNull(last().undoSize);

        host.resizeSends = true;
        controller.apply(last().verdict.remedy(StreamDoctorVerdict.RESOLUTION_STEPDOWN));
        assertEquals(StreamDoctorController.RESIZE_OK, last().resizeProblem);
        host.now += StreamDoctorController.RESIZE_CONFIRM_MS + 1;
        host.tick();
        assertEquals(StreamDoctorController.RESIZE_KEPT, last().resizeProblem);
        assertNull(last().undoSize);
    }

    @Test
    public void theSharedPolicyGathersFirst() {
        host.realPolicy = true;
        controller.open(view);
        assertEquals(StreamDoctorVerdict.GATHERING, last().verdict.cause);
        assertTrue(last().verdict.remedies.isEmpty());
    }

    @Test
    public void theSharedPolicyReadsTheRequestAndTheHostsSnapshot() {
        host.realPolicy = true;
        StreamHealth health = new StreamHealth();
        for (int i = 1; i <= 20; i++) {
            health.add(new StreamHealth.Sample(i * 1000L, true, 60, 0f, 1f, 4f, 5f, 3f, 60f, 60f, 60f, 0, true));
        }
        host.facts.report = health.report(20_000);
        controller.open(view);
        assertEquals(StreamDoctorVerdict.HEALTHY, last().verdict.cause);
        assertFalse(last().verdict.hostMeasured);

        host.offered = true;
        host.snapshot = snapshot(true, "[]", OPEN_REMEDY);
        host.tick();
        host.runWork();
        assertNotNull("the snapshot is a request the policy takes", last().verdict);
        assertTrue(last().verdict.hostMeasured);
    }

    private StreamDoctorController.Card last() {
        return cards.get(cards.size() - 1);
    }

    private Json.Obj abilities() {
        return Json.parseObject(host.lastRequest).optObject("abilities");
    }

    private static String verdict(String cause, String remedies) {
        return "{\"ok\":{\"cause\":\"" + cause + "\",\"stage\":\"network\",\"key\":{\"kind\":\"none\"},\"remedies\":"
                + remedies + ",\"advice\":[],\"upscaler\":{\"verdict\":\"fine\"},\"host_measured\":false}}";
    }

    private static String trial(boolean canUndo) {
        return "[{\"trial_id\":\"t-3\",\"remedy\":\"bitrate_stepdown\",\"applied_value\":36000,\"previous_value\":45000,"
                + "\"can_undo\":" + canUndo + "}]";
    }

    // GET /stream/doctor as the host sends it (native-api/tests/fixtures/stream_doctor.json's shape).
    private static String snapshot(boolean owner, String trials, String remedy) {
        return "{\"schema_version\":1,\"session_active\":true,\"caller_is_owner\":" + owner + ",\"window_seconds\":30,"
                + "\"encoder\":{\"backend\":\"amf\",\"codec\":\"hevc\",\"hdr\":false,\"width\":2560,\"height\":1440,"
                + "\"fps\":120,\"live_bitrate_kbps\":45000,\"fec_percent\":20},"
                + "\"host_stages\":{\"pictures\":3560,\"late_pictures\":94,\"dominant_stage\":\"encode\","
                + "\"lost_us\":{\"capture\":18200,\"convert\":11400,\"encode\":642500,\"packetize\":4100,\"send\":22000},"
                + "\"pipeline_rates\":{\"source_mhz\":118400,\"capture_mhz\":118400,\"submit_mhz\":102100,"
                + "\"output_mhz\":101800,\"repeat_mhz\":1200}},"
                + "\"active_trials\":" + trials + ",\"eligible_remedies\":[" + remedy + "]}";
    }

    private static final class FakeHost implements StreamDoctorController.Host {
        boolean offered, realPolicy, resizeSends = true;
        Object stream = new Object();
        final StreamDoctorController.Facts facts = new StreamDoctorController.Facts();
        String diagnosis = verdict("healthy", "[]"), snapshot = snapshot(true, "[]", OPEN_REMEDY), answer, lastRequest;
        NativeApiException refusal;
        int fetches;
        long now, pendingDelay;
        Runnable pending;
        final List<Runnable> work = new ArrayList<>();
        final List<String> applies = new ArrayList<>();
        final List<Boolean> classic = new ArrayList<>();
        final List<int[]> resizes = new ArrayList<>();
        final List<Integer> applied = new ArrayList<>();

        FakeHost() {
            facts.report = new StreamHealth().report(0);
            facts.width = 2560;
            facts.height = 1440;
            facts.fps = 120;
            facts.screenWidth = 2400;
            facts.screenHeight = 1080;
            facts.bitrateManual = true;
            facts.currentKbps = 45000;
        }

        void runWork() {
            while (!work.isEmpty()) work.remove(0).run();
        }

        void tick() {
            Runnable next = pending;
            pending = null;
            next.run();
        }

        @Override public boolean offered() { return offered; }
        @Override public NativeStreamDoctor fetch() {
            fetches++;
            return NativeStreamDoctor.parse(Json.parseObject(snapshot));
        }
        @Override public NativeStreamDoctor apply(boolean undo, String trialId) throws NativeApiException {
            applies.add((undo ? "undo " : "apply ") + trialId);
            if (refusal != null) throw refusal;
            return NativeStreamDoctor.parse(Json.parseObject(answer));
        }
        @Override public boolean background(Runnable task) { return work.add(task); }
        @Override public void ui(Runnable task) { task.run(); }
        @Override public void later(Runnable task, long delayMs) {
            pending = task;
            pendingDelay = delayMs;
        }
        @Override public void cancel(Runnable task) {
            if (pending == task) pending = null;
        }
        @Override public long nowMs() { return now; }
        @Override public StreamDoctorController.Facts facts() { return facts; }
        @Override public String diagnose(String request) {
            lastRequest = request;
            return realPolicy ? NightjarPolicyJni.doctorDiagnose(request) : diagnosis;
        }
        @Override public Object stream() { return stream; }
        @Override public void classicTrial(boolean undo) { classic.add(undo); }
        @Override public boolean resize(int width, int height) {
            resizes.add(new int[] {width, height});
            return resizeSends;
        }
        @Override public void hostBitrateApplied(int kbps) { applied.add(kbps); }
        @Override public String describe(NativeApiException error) { return error.getMessage(); }
    }
}
