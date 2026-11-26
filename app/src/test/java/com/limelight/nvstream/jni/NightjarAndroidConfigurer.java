package com.limelight.nvstream.jni;

import javax.annotation.Priority;

import org.robolectric.config.AndroidConfigurer;
import org.robolectric.internal.bytecode.InstrumentationConfiguration;
import org.robolectric.internal.bytecode.Interceptors;
import org.robolectric.internal.bytecode.ShadowProviders;

/**
 * Keeps {@link NightjarJni} on the parent JVM application classloader across all Robolectric
 * {@code SandboxClassLoader} instances so {@code nightjar_core} loads once per test worker and
 * avoids {@code UnsatisfiedLinkError: already loaded in another classloader}.
 */
@Priority(1)
public final class NightjarAndroidConfigurer extends AndroidConfigurer {
    public NightjarAndroidConfigurer(ShadowProviders shadowProviders) {
        super(shadowProviders);
    }

    @Override
    public void configure(InstrumentationConfiguration.Builder builder, Interceptors interceptors) {
        super.configure(builder, interceptors);
        builder.doNotAcquireClass(NightjarJni.class.getName());
        builder.doNotAcquireClass(NightjarJni.HostLossCallback.class.getName());
        builder.doNotAcquireClass(NightjarJni.RouteWaitCallback.class.getName());
        builder.doNotAcquireClass(NightjarPolicyJni.class.getName());
        builder.doNotAcquireClass(NightjarUiPolicyJni.class.getName());
        builder.doNotAcquireClass(NightjarInputJni.class.getName());
        builder.doNotAcquireClass(NightjarPinJni.class.getName());
        builder.doNotAcquireClass(NightjarPinJni.PinVerifier.class.getName());
        builder.doNotAcquireClass(NightjarHttpJni.class.getName());
        builder.doNotAcquireClass(NightjarHttpJni.ByteSink.class.getName());
        builder.doNotAcquireClass(NightjarHttpJni.ByteSource.class.getName());
        builder.doNotAcquireClass(NightjarHttpJni.PercentProgress.class.getName());
        builder.doNotAcquireClass(NightjarHttpJni.FolderProgress.class.getName());
        builder.doNotAcquireClass(NightjarConsoleJni.class.getName());
        builder.doNotAcquireClass(NightjarConsoleJni.TransportCallback.class.getName());
        builder.doNotAcquireClass(NightjarDiscoveryJni.class.getName());
        builder.doNotAcquireClass(NightjarHostsJni.class.getName());
        builder.doNotAcquireClass(NightjarHostsJni.FlowTransport.class.getName());
        builder.doNotAcquireClass(NightjarHostsJni.FlowSleeper.class.getName());
        builder.doNotAcquireClass(NightjarHostsJni.FlowStepListener.class.getName());
        builder.doNotAcquireClass(NightjarSessionJni.class.getName());
        builder.doNotAcquireClass(NightjarSessionJni.NegotiationCallback.class.getName());
        builder.doNotAcquireClass(NightjarSessionJni.LinkProbe.class.getName());
        builder.doNotAcquireClass(NightjarSessionJni.HostCallback.class.getName());
        builder.doNotAcquireClass(CurtainBridge.class.getName());
    }
}
