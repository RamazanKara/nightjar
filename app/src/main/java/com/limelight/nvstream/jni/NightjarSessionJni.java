/**
 * Pure-JDK JNI bridge to Rust connection orchestration ({@code nightjar_core::connect::*})
 * and connection/session policy ({@code nightjar_policy::{connect_text, mic_route, permissions}}),
 * and the stream's launch curtain ({@code nightjar_policy::launch_curtain}, through {@link CurtainBridge}).
 */
package com.limelight.nvstream.jni;

public final class NightjarSessionJni {
    static {
        NightjarJni.ensureLibraryLoaded();
    }

    private NightjarSessionJni() {
    }

    /** Callback boundary for two-step platform video format &amp; HDR negotiation. */
    public interface NegotiationCallback {
        int preparePackedModes(int probeFormats);

        int prepareVideoFormats(int commonFormats, int hostScm);

        long pickHdrFormat(
                int hostHdrFormats, int negotiatedFormats, boolean packed444, boolean frameWarp);

        int videoCapabilities(int negotiatedFormats, int hdrFormat);
    }

    /** Callback boundary for bandwidth &amp; jumbo pace probes returning JSON envelopes. */
    public interface LinkProbe {
        String probeLinkMbps();

        String probePace(int kbps, int size, int fps, int pace);
    }

    /** Callback boundary for host HTTP launch/resume/quit &amp; 410 replace prompts. */
    public interface HostCallback {
        String serverInfo(boolean fast);

        boolean hasEndedSession();

        String launch(String verb, int appId, boolean hdr, boolean confirmReplace);

        String quitApp();

        int confirmReplace(boolean terminateOnly);

        boolean isStopped();
    }

    // --- Pure Policy (`nightjar_policy::{connect_text, mic_route, permissions}`) ---

    public static native String stageTextKey(String stageName);

    public static native String terminationTextKey(int code);

    public static native int classifyStartupFailure(
            boolean hasDecoderFailure,
            String stageName,
            boolean hasConnectionErrorMessage,
            int errorCode);

    public static native int classifyLaunchFailure(
            boolean isLocal, int code, String trimmedReason);

    public static native boolean isPacked444Refused(int errorCode);

    public static native int micRouteSnapshot(
            boolean micMuted, boolean[] allowed, boolean[] ready, boolean[] hostMuted);

    public static native boolean microphoneWanted(
            boolean enableMic, boolean micOnly, boolean remoteMonitor);

    public static native boolean micDisablesEchoCancellation(boolean micOnly);

    public static native int streamPermissions(
            boolean stopRequested,
            boolean isMonkey,
            int roleOrdinal,
            boolean paired,
            long permission,
            boolean supportsRuntimeBitrate);

    public static native boolean canRunServerCommand(
            boolean stopRequested,
            boolean isMonkey,
            int roleOrdinal,
            boolean paired,
            long permission,
            int commandIndex,
            int commandCount,
            boolean commandMatchesName);

    public static native boolean serverCommandThrottleAllows(long lastSentMs, long nowMs);

    // --- Hardware Metrics Poller (`HardwareMetricsPoller`) ---

    public static native long metricsPollerCreate();

    public static native void metricsPollerDestroy(long handle);

    public static native void metricsPollerStop(long handle);

    public static native boolean metricsPollerIsUnavailable(long handle);

    public static native int metricsPollerDue(long handle, long nowMs, boolean wanted);

    public static native long[] metricsPollerFinishStep(
            long handle,
            int generation,
            long nowMs,
            boolean ok,
            long sampledAtUnixMs,
            int intervalMs,
            int httpStatus,
            boolean unsupportedVersion);

    // --- Two-step Video Format & SharpColor Negotiation ---

    public static native int negotiateVideoFormats(int formats, int serverFormats);

    public static native boolean pyroWaveHdrLost(int commonFormats, int negotiatedFormats);

    public static native String sharpColorRequestJson(
            int offered,
            boolean sdr10,
            boolean pyrowave,
            boolean packedEngine,
            int hostScm,
            int hostPacked,
            boolean packedRefusedBefore);

    public static native int formatsAfterSharpColor(
            String pickKind, int formatsOrModes, int offered, boolean hdr);

    public static native String negotiateTwoStep(
            int initialFormats,
            int hostScm,
            int hostHdrFormats,
            boolean frameWarp,
            boolean sharpColor,
            boolean sdr10,
            boolean pyrowave,
            boolean packedEngine,
            int hostPacked,
            boolean packedRefusedBefore,
            NegotiationCallback callback);

    // --- Preflight, First Bitrate & PyroWave Sizing ---

    public static native int firstBitrateKbps(
            int configuredKbps,
            boolean linkProbeEnabled,
            boolean automaticBitrate,
            boolean resumeOnly,
            int controlOrdinal,
            int roleOrdinal,
            boolean smallStreamInputOnly,
            int negotiatedFormats,
            boolean paired,
            boolean hasBandwidthProbe,
            LinkProbe hostCallback);

    public static native int pyroWaveStartKbps(
            int negotiatedFormats,
            int configuredKbps,
            boolean automaticBitrate,
            int width,
            int height,
            int fps,
            int linkMbps,
            int bitrateLimitKbps);

    public static native int[] pyroWaveLinkNotice(
            int negotiatedFormats,
            int width,
            int height,
            int fps,
            int linkMbps,
            int configuredKbps,
            boolean automaticBitrate);

    public static native int[] resolvePacketSizePreflight(
            int locationOrdinal,
            boolean isPrivateAddr,
            boolean vpnOrTunnel,
            int offeredMtu,
            boolean burstProbe,
            int requestedPacketSize,
            boolean jumboFrames,
            boolean autoReconnect,
            int negotiatedPacketSize,
            LinkProbe hostCallback);

    // --- Launch / Resume / Quit / ConfirmReplace ---

    public static native String launchAppConfirming(
            String targetJson, String optsJson, String verb, HostCallback hostCallback);

    public static native String quitAndLaunch(
            String targetJson, String optsJson, HostCallback hostCallback);

    public static native String launchOrResume(
            String serverInfoJson, String targetJson, String optsJson, HostCallback hostCallback);

    // --- Lifecycle, Recovery Gating, MoveWatch & Session Actions ---

    public static native int[] sessionEvaluateTermination(
            int reportedCode,
            boolean exitRequested,
            boolean hostGone,
            boolean endedSession,
            boolean movedToPresent,
            boolean guestEnded,
            boolean videoStarted,
            boolean autoReconnect);

    public static native String sessionStuckStopReport(boolean isAlive, String stackLines);

    public static native boolean serverinfoConfirmsEndedSession(
            boolean stopRequested,
            int roleOrdinal,
            boolean isControlAction,
            boolean paired,
            boolean hasViewLaunchPerm,
            boolean sessionIdle,
            int currentGame,
            String currentGameUuid);

    public static native long moveWatchCreate(int reads);

    public static native int moveWatchStep(
            long handle, boolean showing, boolean movedKnown, boolean moved, boolean owns);

    public static native void moveWatchDestroy(long handle);

    // --- Launch curtain (protocol/android launch_curtain.rs; callers go through CurtainBridge) ---

    public static native void launchCurtainBegin(boolean asked);

    public static native void launchCurtainEnd();

    public static native String launchCurtainState();

    public static native String launchCurtainTick();

    public static native String launchCurtainDismiss();

    public static native void sessionResetForTests();
}
