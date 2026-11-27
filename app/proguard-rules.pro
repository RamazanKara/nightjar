# Constructed by EvdevCaptureProviderShim only in the root flavor.
-keep,allowoptimization class com.limelight.binding.input.evdev.EvdevCaptureProvider {
    public <init>(android.app.Activity, com.limelight.binding.input.evdev.EvdevListener);
}

# Looked up by name from pyrowave_renderer.cpp and protocol/android/src/{bridge,renderers}.rs.
-keepclassmembers class com.limelight.nvstream.jni.MoonBridge {
    public static *** bridge*(...);
    public static int drSubmitRawDecodeUnit(long, int, int, int, int, char, long, long, long, long);
    public static void drSubmitChunksRaw(long, int, boolean);
    public static int arPlaySampleDirect(long, int, int);
    public static void arPlaySamplePending(short[], int, int);
}
-keepclassmembers class com.limelight.binding.video.FrameLatencyStats {
    void onDecoderOutput(int, long, long);
    void onPresentQueued(int, long, boolean, boolean);
    void onFrameRendered(long, long);
    void discard(long, java.lang.String);
}
# Looked up by name from protocol/android/src/files.rs.
-keepclassmembers interface com.limelight.nvstream.jni.FileTransferBridge$Progress {
    void onProgress(long, long, long);
}
# Looked up by name from protocol/android/src/wake_relay.rs; no Java code calls it.
-keepclassmembers interface com.limelight.nvstream.jni.NightjarJni$WakeRelayListener {
    void progress(java.lang.String);
}
-keepclassmembers class * implements com.limelight.nvstream.jni.NightjarJni$WakeRelayListener {
    void progress(java.lang.String);
}
# Looked up by name from protocol/android/src/http.rs and http_folders.rs.
-keepclassmembers interface com.limelight.nvstream.jni.NightjarHttpJni$ByteSink {
    void write(byte[], int, int);
}
-keepclassmembers interface com.limelight.nvstream.jni.NightjarHttpJni$ByteSource {
    int read(byte[], int, int);
}
-keepclassmembers interface com.limelight.nvstream.jni.NightjarHttpJni$PercentProgress {
    void accept(int);
}
-keepclassmembers interface com.limelight.nvstream.jni.NightjarHttpJni$FolderProgress {
    void onProgress(long, long);
}
# Looked up by name from protocol/android/src/pin.rs.
-keepclassmembers interface com.limelight.nvstream.jni.NightjarPinJni$PinVerifier {
    boolean matches(java.lang.String, byte[]);
}
# Looked up by name from protocol/android/src/console/jni.rs.
-keepclassmembers interface com.limelight.nvstream.jni.NightjarConsoleJni$TransportCallback {
    java.lang.String call(java.lang.String, java.lang.String, java.lang.String);
}
# Looked up by name from protocol/android/src/hosts_power_flows.rs.
-keepclassmembers interface com.limelight.nvstream.jni.NightjarHostsJni$FlowTransport {
    java.lang.String call(java.lang.String, java.lang.String);
}
-keepclassmembers interface com.limelight.nvstream.jni.NightjarHostsJni$FlowSleeper {
    boolean sleep(long);
}
-keepclassmembers interface com.limelight.nvstream.jni.NightjarHostsJni$FlowStepListener {
    void onStep(java.lang.String);
}
# Looked up by name from protocol/android/src/{connect.rs}.
-keepclassmembers class com.limelight.nvstream.jni.NightjarSessionJni {
    <methods>;
}
-keepclassmembers interface com.limelight.nvstream.jni.NightjarSessionJni$NegotiationCallback {
    int preparePackedModes(int);
    int prepareVideoFormats(int, int);
    long pickHdrFormat(int, int, boolean, boolean);
    int videoCapabilities(int, int);
}
-keepclassmembers interface com.limelight.nvstream.jni.NightjarSessionJni$LinkProbe {
    java.lang.String probeLinkMbps();
    java.lang.String probePace(int, int, int, int);
}
-keepclassmembers interface com.limelight.nvstream.jni.NightjarSessionJni$HostCallback {
    java.lang.String serverInfo(boolean);
    boolean hasEndedSession();
    java.lang.String launch(java.lang.String, int, boolean, boolean);
    java.lang.String quitApp();
    int confirmReplace(boolean);
    boolean isStopped();
}
# Looked up by name from protocol/android/src/policy_roaming.rs.
-keepclassmembers interface com.limelight.nvstream.jni.NightjarJni$HostLossCallback {
    java.lang.String gatewayToward();
    boolean isStopped();
    void log(java.lang.String);
}
-keepclassmembers interface com.limelight.nvstream.jni.NightjarJni$RouteWaitCallback {
    long nowMs();
    java.lang.String probe(java.lang.String, int);
    boolean isCancelled();
    boolean sleep(long);
    void logInfo(java.lang.String);
    void logWarn(java.lang.String);
}

# The default Android rules retain native method names and descriptor classes.
# BouncyCastle discovers mappings and JCA services by name. AndroidCryptoProvider
# uses only RSA keys, SHA256withRSA signatures and X.509 certificates from BC.
-keep,allowoptimization class org.bouncycastle.jcajce.provider.asymmetric.RSA$Mappings {
    public <init>();
}
-keep,allowoptimization class org.bouncycastle.jcajce.provider.asymmetric.X509$Mappings {
    public <init>();
}
-keep,allowoptimization class org.bouncycastle.jcajce.provider.digest.SHA256$Mappings {
    public <init>();
}
-keep,allowoptimization class org.bouncycastle.jcajce.provider.asymmetric.rsa.KeyPairGeneratorSpi {
    public <init>();
}
-keep,allowoptimization class org.bouncycastle.jcajce.provider.asymmetric.rsa.KeyFactorySpi {
    public <init>();
}
-keep,allowoptimization class org.bouncycastle.jcajce.provider.asymmetric.rsa.DigestSignatureSpi$SHA256 {
    public <init>();
}
-keep,allowoptimization class org.bouncycastle.jcajce.provider.asymmetric.x509.CertificateFactory {
    public <init>();
}
-keep,allowoptimization class org.bouncycastle.jcajce.provider.digest.SHA256$Digest {
    public <init>();
}

# BC's unused LDAP certificate stores reference desktop JNDI APIs.
-dontwarn javax.naming.**

# OkHttp/Okio ship consumer rules. jcodec's H.264 parsing and
# ShieldControllerExtensions' Binder calls are statically referenced.
