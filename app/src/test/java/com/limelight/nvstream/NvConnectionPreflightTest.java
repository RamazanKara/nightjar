/**
 * Unit tests for server address resolution in {@link NvConnectionPreflight}.
 */
package com.limelight.nvstream;

import com.limelight.nvstream.http.ComputerDetails;

import org.junit.Test;

import java.net.InetAddress;
import java.util.concurrent.atomic.AtomicInteger;

import static org.junit.Assert.assertEquals;

public class NvConnectionPreflightTest {
    @Test
    public void singleResolvedAddressSkipsTcpProbe() throws Exception {
        ConnectionContext ctx = new ConnectionContext();
        ctx.serverAddress = new ComputerDetails.AddressTuple("host.test", 47989);
        InetAddress only = InetAddress.getByAddress("host.test", new byte[] {127, 0, 0, 1});
        AtomicInteger probes = new AtomicInteger();

        InetAddress picked = NvConnectionPreflight.resolveServerAddress(ctx, new NvConnectionPreflight.AddressLookup() {
            @Override
            public InetAddress[] resolveAll(String host) {
                return new InetAddress[] {only};
            }

            @Override
            public boolean probeReachable(InetAddress addr, int port, int timeoutMs) {
                probes.incrementAndGet();
                return true;
            }
        });

        assertEquals(only, picked);
        assertEquals("Single resolved address must not run a blocking TCP probe", 0, probes.get());
    }

    @Test
    public void multipleResolvedAddressesPickFirstReachableOrFallbackToFirst() throws Exception {
        ConnectionContext ctx = new ConnectionContext();
        ctx.serverAddress = new ComputerDetails.AddressTuple("multi.test", 47989);
        InetAddress unreachable = InetAddress.getByAddress("multi.test", new byte[] {10, 0, 0, 1});
        InetAddress reachable = InetAddress.getByAddress("multi.test", new byte[] {10, 0, 0, 2});
        AtomicInteger probes = new AtomicInteger();

        InetAddress picked = NvConnectionPreflight.resolveServerAddress(ctx, new NvConnectionPreflight.AddressLookup() {
            @Override
            public InetAddress[] resolveAll(String host) {
                return new InetAddress[] {unreachable, reachable};
            }

            @Override
            public boolean probeReachable(InetAddress addr, int port, int timeoutMs) {
                probes.incrementAndGet();
                return addr.equals(reachable);
            }
        });
        assertEquals(reachable, picked);
        assertEquals(2, probes.get());

        InetAddress fallback = NvConnectionPreflight.resolveServerAddress(ctx, new NvConnectionPreflight.AddressLookup() {
            @Override
            public InetAddress[] resolveAll(String host) {
                return new InetAddress[] {unreachable, reachable};
            }

            @Override
            public boolean probeReachable(InetAddress addr, int port, int timeoutMs) {
                return false;
            }
        });
        assertEquals(unreachable, fallback);
    }
}
