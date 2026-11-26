package com.limelight.utils;

import com.limelight.nvstream.http.HostCapabilities;
import com.limelight.utils.WiredLink.LocalSubnet;
import com.limelight.utils.WiredLink.Route;

import org.junit.Test;

import java.net.InetAddress;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collections;
import java.util.List;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertTrue;

public class WiredLinkTest {
    private static Route ethernet(String name, int prefix, boolean active) {
        return new Route(name, true, false, active, prefix);
    }

    private static Route wifi(int prefix, boolean active) {
        return new Route("wlan0", false, false, active, prefix);
    }

    @Test public void speedFileHoldsMegabitsPerSecond() {
        assertEquals(1000, WiredLink.parseMbps("1000\n"));
        assertEquals(2500, WiredLink.parseMbps(" 2500 "));
        assertEquals(10000, WiredLink.parseMbps("10000"));
    }

    @Test public void unknownOrBrokenSpeedsAreZero() {
        assertEquals(0, WiredLink.parseMbps("-1"));
        assertEquals(0, WiredLink.parseMbps("0"));
        assertEquals(0, WiredLink.parseMbps(""));
        assertEquals(0, WiredLink.parseMbps(null));
        assertEquals(0, WiredLink.parseMbps("unknown"));
        assertEquals(0, WiredLink.parseMbps("99999999999"));
        assertEquals(0, WiredLink.parseMbps("2000000"));
    }

    @Test public void ethernetRouteReportsItsInterfaceSpeed() {
        assertEquals(2500, WiredLink.mbpsToward(Collections.singletonList(ethernet("eth0", 24, true)),
                name -> name.equals("eth0") ? 2500 : -7));
    }

    @Test public void wifiCellularAndNoRouteGiveNoHint() {
        WiredLink.SpeedSource fast = name -> 1000;
        assertEquals(0, WiredLink.mbpsToward(Collections.singletonList(wifi(24, true)), fast));
        assertEquals(0, WiredLink.mbpsToward(Collections.<Route>emptyList(), fast));
    }

    @Test public void theMostSpecificRouteDecides() {
        WiredLink.SpeedSource fast = name -> 1000;
        // Ethernet and Wi-Fi both on the host's subnet: the active network wins the tie.
        assertEquals(1000, WiredLink.mbpsToward(Arrays.asList(wifi(24, false), ethernet("eth0", 24, true)), fast));
        assertEquals(0, WiredLink.mbpsToward(Arrays.asList(ethernet("eth0", 24, false), wifi(24, true)), fast));
        // A subnet route on Wi-Fi beats Ethernet's default route, and the other way round.
        assertEquals(0, WiredLink.mbpsToward(Arrays.asList(ethernet("eth0", 0, true), wifi(24, false)), fast));
        assertEquals(1000, WiredLink.mbpsToward(Arrays.asList(wifi(0, true), ethernet("eth0", 24, false)), fast));
    }

    @Test public void vpnOrOddInterfaceNamesGiveNoHint() {
        WiredLink.SpeedSource fast = name -> 1000;
        assertEquals(0, WiredLink.mbpsToward(Collections.singletonList(new Route("eth0", true, true, true, 0)), fast));
        assertEquals(0, WiredLink.mbpsToward(Collections.singletonList(ethernet("../eth0", 24, true)), fast));
        assertEquals(0, WiredLink.mbpsToward(Collections.singletonList(ethernet(null, 24, true)), fast));
        assertEquals(0, WiredLink.mbpsToward(Collections.singletonList(ethernet("eth0", 24, true)), name -> 0));
    }

    private static Route vpnOver(boolean wifi, boolean cellular, int prefix, boolean active) {
        return new Route("tun0", false, true, active, prefix, wifi, cellular);
    }

    @Test public void wifiAndCellularRoutesAreWireless() {
        assertTrue(WiredLink.wirelessToward(Collections.singletonList(
                new Route("wlan0", false, false, true, 24, true, false))));
        assertTrue(WiredLink.wirelessToward(Collections.singletonList(
                new Route("rmnet0", false, false, true, 0, false, true))));
    }

    @Test public void wiredUnknownAndEmptyRoutesAreNotWireless() {
        assertFalse(WiredLink.wirelessToward(Collections.singletonList(ethernet("eth0", 24, true))));
        // A network that shows no Wi-Fi or cellular transport (the 5-argument constructor sets
        // neither) is unknown, not wireless.
        assertFalse(WiredLink.wirelessToward(Collections.singletonList(new Route("tun9", false, false, true, 24))));
        assertFalse(WiredLink.wirelessToward(Collections.<Route>emptyList()));
        // Ethernet among the transports wins over a Wi-Fi flag on the same network.
        assertFalse(WiredLink.wirelessToward(Collections.singletonList(
                new Route("eth0", true, false, true, 24, true, false))));
    }

    @Test public void vpnCountsByItsVisibleUnderlyingTransport() {
        assertTrue(WiredLink.wirelessToward(Collections.singletonList(vpnOver(true, false, 0, true))));
        assertTrue(WiredLink.wirelessToward(Collections.singletonList(vpnOver(false, true, 0, true))));
        assertFalse(WiredLink.wirelessToward(Collections.singletonList(vpnOver(false, false, 0, true))));
    }

    // Android 9 and older cannot ask for the route to a host, so the active network's transports
    // stand in for it: they can say wireless, never a link speed.
    @Test public void activeNetworkAloneDecidesBeforeAndroid10() {
        // ethernet, wifi, cellular, vpn
        assertTrue(WiredLink.wirelessToward(WiredLink.activeNetworkRoute(false, true, false, false)));
        assertTrue(WiredLink.wirelessToward(WiredLink.activeNetworkRoute(false, false, true, false)));
        assertTrue(WiredLink.wirelessToward(WiredLink.activeNetworkRoute(false, true, false, true)));
        assertFalse(WiredLink.wirelessToward(WiredLink.activeNetworkRoute(true, false, false, false)));
        assertFalse(WiredLink.wirelessToward(WiredLink.activeNetworkRoute(true, true, false, false)));
        // A VPN that shows no underlying transport, or no active network at all, is unknown.
        assertTrue(WiredLink.activeNetworkRoute(false, false, false, true).isEmpty());
        assertFalse(WiredLink.wirelessToward(WiredLink.activeNetworkRoute(false, false, false, false)));
        // Never a speed: even an Ethernet guess has no interface to read one from.
        assertEquals(0, WiredLink.mbpsToward(WiredLink.activeNetworkRoute(true, false, false, false), name -> 1000));
    }

    @Test public void theRouteTheTrafficTakesDecidesWhetherItIsWireless() {
        Route wlan = new Route("wlan0", false, false, true, 0, true, false);
        // Wi-Fi is active, but the host's subnet is on Ethernet: the stream is wired.
        assertFalse(WiredLink.wirelessToward(Arrays.asList(wlan, ethernet("eth0", 24, false))));
        // Ethernet is the default, but the host's subnet is on Wi-Fi.
        assertTrue(WiredLink.wirelessToward(Arrays.asList(ethernet("eth0", 0, true),
                new Route("wlan0", false, false, false, 24, true, false))));
        // Equal prefix: the active network wins the tie.
        assertTrue(WiredLink.wirelessToward(Arrays.asList(
                new Route("wlan0", false, false, true, 24, true, false), ethernet("eth0", 24, false))));
        assertFalse(WiredLink.wirelessToward(Arrays.asList(
                new Route("wlan0", false, false, false, 24, true, false), ethernet("eth0", 24, true))));
    }

    @Test public void onlyARubylightHostOnAWirelessRouteWithTheSettingOnIsTold() {
        for (HostCapabilities.Family family : HostCapabilities.Family.values()) {
            boolean rubylight = family == HostCapabilities.Family.RUBYLIGHT;
            assertEquals(family.name(), rubylight, WiredLink.tellsHostWireless(true, family, true));
            assertFalse(WiredLink.tellsHostWireless(false, family, true));
            assertFalse(WiredLink.tellsHostWireless(true, family, false));
        }
    }

    @Test public void subnetMatchesPrefixLengthsAndFamilies() throws Exception {
        InetAddress ap = InetAddress.getByName("172.31.225.1");
        InetAddress peer = InetAddress.getByName("172.31.225.121");
        InetAddress otherSubnet = InetAddress.getByName("172.31.224.121");
        assertTrue(WiredLink.subnetMatches(ap, 24, peer));
        assertTrue(WiredLink.subnetMatches(ap, 16, otherSubnet));
        assertFalse(WiredLink.subnetMatches(ap, 24, otherSubnet));
        assertTrue(WiredLink.subnetMatches(peer, 32, peer));
        assertFalse(WiredLink.subnetMatches(ap, 32, peer));
        assertFalse(WiredLink.subnetMatches(ap, 0, peer));
        assertFalse(WiredLink.subnetMatches(ap, 33, peer));

        InetAddress v6Net = InetAddress.getByName("fd00:1234:5678::1");
        InetAddress v6Peer = InetAddress.getByName("fd00:1234:5678::42");
        InetAddress v6Other = InetAddress.getByName("fd00:1234:5679::42");
        assertTrue(WiredLink.subnetMatches(v6Net, 64, v6Peer));
        assertFalse(WiredLink.subnetMatches(v6Net, 64, v6Other));
        assertFalse(WiredLink.subnetMatches(ap, 24, v6Peer));
        assertFalse(WiredLink.subnetMatches(null, 24, peer));
        assertFalse(WiredLink.subnetMatches(ap, 24, null));
    }

    @Test public void onlyWirelessAndHotspotInterfacesCountAsHotspots() {
        for (String name : Arrays.asList("ap0", "ap1", "ap_br_wlan0", "ap_br_wlan2",
                "wlan1", "swlan0", "softap0", "p2p-wlan0-0", "wifi0", "sap0")) {
            assertTrue(name, WiredLink.isWirelessHotspotInterface(name));
        }
        for (String name : Arrays.asList("eth0", "en0", "rndis0", "usb0", "ncm0",
                "tun0", "lo", "rmnet0", "ccmni0", "v4-rmnet0", "bridge0", "apple0", "", null)) {
            assertFalse(String.valueOf(name), WiredLink.isWirelessHotspotInterface(name));
        }
    }

    @Test public void localHotspotRouteMarksOfflineAndDefaultRoutedSoftApStreamsWireless() throws Exception {
        InetAddress host = InetAddress.getByName("172.31.225.121");
        InetAddress apIp = InetAddress.getByName("172.31.225.1");

        // Local hotspot without mobile data: ConnectivityManager returned nothing.
        List<Route> offlineHotspot = new ArrayList<>();
        WiredLink.addLocalRoutes(offlineHotspot, host,
                Collections.singletonList(new LocalSubnet("ap0", apIp, 24)), false);
        assertTrue(WiredLink.wirelessToward(offlineHotspot));
        assertEquals(0, WiredLink.mbpsToward(offlineHotspot, name -> 1000));

        // Hotspot with an unknown default route (e.g. opaque VPN): /24 hotspot route wins.
        List<Route> unknownDefault = new ArrayList<>();
        unknownDefault.add(new Route("tun0", false, true, true, 0));
        WiredLink.addLocalRoutes(unknownDefault, host,
                Collections.singletonList(new LocalSubnet("ap_br_wlan2", apIp, 24)), false);
        assertTrue(WiredLink.wirelessToward(unknownDefault));

        // USB tethering (rndis0 / ncm0 / usb0) with cellular or Wi-Fi default route stays wired.
        List<Route> usbTether = new ArrayList<>();
        usbTether.add(new Route("rmnet0", false, false, true, 0, false, true));
        WiredLink.addLocalRoutes(usbTether, host,
                Collections.singletonList(new LocalSubnet("rndis0", apIp, 24)), false);
        assertFalse(WiredLink.wirelessToward(usbTether));
        assertEquals(0, WiredLink.mbpsToward(usbTether, name -> 1000));

        // Pre-Q active Ethernet fallback is not overridden by an associated wlan0 LAN subnet.
        List<Route> preQEthernet = new ArrayList<>(WiredLink.activeNetworkRoute(true, false, false, false));
        WiredLink.addLocalRoutes(preQEthernet, host,
                Collections.singletonList(new LocalSubnet("wlan0", apIp, 24)), true);
        assertFalse(WiredLink.wirelessToward(preQEthernet));

        // Link-local IPv6 subnets without matching scope must not match wireless interfaces.
        InetAddress v6LinkLocalHost = InetAddress.getByName("fe80::1234");
        InetAddress v6LinkLocalWlan = InetAddress.getByName("fe80::1");
        assertFalse(WiredLink.subnetMatches(v6LinkLocalWlan, 64, v6LinkLocalHost));
        List<Route> usbWithLinkLocalWlan = new ArrayList<>();
        WiredLink.addLocalRoutes(usbWithLinkLocalWlan, v6LinkLocalHost,
                Collections.singletonList(new LocalSubnet("wlan0", v6LinkLocalWlan, 64)), false);
        assertFalse(WiredLink.wirelessToward(usbWithLinkLocalWlan));

        // Existing ConnectivityManager route on the same interface is not duplicated.
        List<Route> alreadyPresent = new ArrayList<>();
        alreadyPresent.add(new Route("wlan1", false, false, true, 24, true, false));
        WiredLink.addLocalRoutes(alreadyPresent, host,
                Collections.singletonList(new LocalSubnet("wlan1", apIp, 24)), false);
        assertEquals(1, alreadyPresent.size());
    }
}
