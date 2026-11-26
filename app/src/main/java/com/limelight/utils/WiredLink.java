package com.limelight.utils;

import android.content.Context;
import android.net.ConnectivityManager;
import android.net.LinkProperties;
import android.net.Network;
import android.net.NetworkCapabilities;
import android.net.RouteInfo;
import android.os.Build;

import com.limelight.LimeLog;

import java.io.BufferedReader;
import java.io.File;
import java.io.FileReader;
import java.io.IOException;
import java.net.Inet6Address;
import java.net.InetAddress;
import java.net.InterfaceAddress;
import java.net.NetworkInterface;
import java.net.SocketException;
import java.util.ArrayList;
import java.util.Enumeration;
import java.util.List;
import java.util.Locale;
import java.util.regex.Pattern;

/**
 * The speed of this device's own wired port on the route to a host, for the PyroWave pacing hint
 * (x-ss-video[0].pyrowaveLinkMbps). A host on a faster port than its client would otherwise burst
 * frames faster than the client's port takes them. Zero means unknown: Wi-Fi, cellular, a VPN, a
 * route that cannot be told or a port that does not report its speed. A wrong low value would
 * throttle the stream, so Android's own bandwidth estimate for Ethernet (a fixed 100 Mbps on many
 * builds) is never used; only the speed the network interface reports counts.
 */
public final class WiredLink {
    private static final Pattern INTERFACE_NAME = Pattern.compile("[A-Za-z0-9._-]{1,15}");
    private static final int MAX_MBPS = 1_000_000;

    /** A network that can reach the host, with how specific its route to the host is. */
    static final class Route {
        final String interfaceName;
        final boolean ethernet, vpn, active;
        /** Wi-Fi or cellular is among the network's transports; a VPN shows its underlying ones. */
        final boolean wifi, cellular;
        /** Prefix length of the best route to the host on this network; 0 is the default route. */
        final int prefixLength;

        Route(String interfaceName, boolean ethernet, boolean vpn, boolean active, int prefixLength) {
            this(interfaceName, ethernet, vpn, active, prefixLength, false, false);
        }

        Route(String interfaceName, boolean ethernet, boolean vpn, boolean active, int prefixLength,
              boolean wifi, boolean cellular) {
            this.interfaceName = interfaceName;
            this.ethernet = ethernet;
            this.vpn = vpn;
            this.active = active;
            this.prefixLength = prefixLength;
            this.wifi = wifi;
            this.cellular = cellular;
        }
    }

    /** Reads the speed in Mbps an interface reports; zero when it reports none. */
    interface SpeedSource {
        int mbps(String interfaceName);
    }

    /** A local address and prefix configured directly on a network interface. */
    static final class LocalSubnet {
        final String interfaceName;
        final InetAddress address;
        final int prefixLength;

        LocalSubnet(String interfaceName, InetAddress address, int prefixLength) {
            this.interfaceName = interfaceName;
            this.address = address;
            this.prefixLength = prefixLength;
        }
    }

    private WiredLink() {}

    /** The pacing hints for one host, both from the same route lookup. */
    public static final class Hints {
        /** The wired link speed in Mbps, or 0 when unknown. */
        public final int mbps;
        /**
         * The route is Wi-Fi or cellular, for the clientWireless hint (x-rl-video[0].clientWireless)
         * that lets a Rubylight host pace for a wireless link from the first frame. False when it
         * is wired, unknown, a VPN whose underlying transport is not visible, or undeterminable.
         */
        public final boolean wireless;

        public Hints(int mbps, boolean wireless) {
            this.mbps = mbps;
            this.wireless = wireless;
        }
    }

    /** The wired link speed in Mbps toward {@code host}, or 0 when unknown. Blocks on name lookup. */
    public static int mbpsToward(Context context, String host) {
        return hintsToward(context, host).mbps;
    }

    /**
     * Whether to send the wireless-route hint: the setting is on, the route is Wi-Fi or cellular
     * and the host says it is Rubylight, the only host that reads it.
     */
    public static boolean tellsHostWireless(boolean setting,
                                            com.limelight.nvstream.http.HostCapabilities.Family family,
                                            boolean wireless) {
        return setting && wireless && family == com.limelight.nvstream.http.HostCapabilities.Family.RUBYLIGHT;
    }

    /** Both hints toward {@code host} from one name lookup and one route list. */
    public static Hints hintsToward(Context context, String host) {
        try {
            InetAddress address = InetAddress.getByName(host);
            List<Route> routes = routesTo(context, address);
            return new Hints(mbpsToward(routes, WiredLink::sysfsMbps), wirelessToward(routes));
        } catch (IOException | RuntimeException e) {
            // Unresolvable name, no permission or a connectivity service that is gone: no hints.
            return new Hints(0, false);
        }
    }

    static boolean wirelessToward(List<Route> routes) {
        Route best = bestRoute(routes);
        // A VPN counts by the transport underneath it when Android shows one; a network with
        // Ethernet among its transports is never wireless, whatever else it carries.
        return best != null && !best.ethernet && (best.wifi || best.cellular);
    }

    private static Route bestRoute(List<Route> routes) {
        Route best = null;
        for (Route route : routes) {
            if (best == null || route.prefixLength > best.prefixLength ||
                    (route.prefixLength == best.prefixLength && route.active && !best.active)) {
                best = route;
            }
        }
        return best;
    }

    static int mbpsToward(List<Route> routes, SpeedSource speeds) {
        Route best = bestRoute(routes);
        // Only the network the traffic takes counts: a wired port that loses to Wi-Fi or a VPN
        // on the route to this host says nothing about the stream.
        if (best == null || !best.ethernet || best.vpn || best.interfaceName == null ||
                !INTERFACE_NAME.matcher(best.interfaceName).matches()) {
            return 0;
        }
        return speeds.mbps(best.interfaceName);
    }

    @SuppressWarnings("deprecation")
    private static List<Route> routesTo(Context context, InetAddress host) {
        List<Route> routes = new ArrayList<>();
        ConnectivityManager connectivity = (ConnectivityManager) context.getSystemService(Context.CONNECTIVITY_SERVICE);
        if (connectivity != null) {
            if (Build.VERSION.SDK_INT < Build.VERSION_CODES.Q) {
                // RouteInfo cannot be asked about a destination before Android 10, so the routes are
                // unknown. The active network is the best guess for the medium, never for the speed.
                routes.addAll(activeNetworkRoute(connectivity));
            } else {
                Network active = connectivity.getActiveNetwork();
                for (Network network : connectivity.getAllNetworks()) {
                    NetworkCapabilities capabilities = connectivity.getNetworkCapabilities(network);
                    LinkProperties properties = connectivity.getLinkProperties(network);
                    if (capabilities == null || properties == null) {
                        continue;
                    }
                    int prefix = -1;
                    for (RouteInfo route : properties.getRoutes()) {
                        if (route.matches(host)) {
                            prefix = Math.max(prefix, route.getDestination().getPrefixLength());
                        }
                    }
                    if (prefix >= 0) {
                        routes.add(new Route(properties.getInterfaceName(),
                                capabilities.hasTransport(NetworkCapabilities.TRANSPORT_ETHERNET),
                                capabilities.hasTransport(NetworkCapabilities.TRANSPORT_VPN) ||
                                        !capabilities.hasCapability(NetworkCapabilities.NET_CAPABILITY_NOT_VPN),
                                network.equals(active), prefix,
                                capabilities.hasTransport(NetworkCapabilities.TRANSPORT_WIFI),
                                capabilities.hasTransport(NetworkCapabilities.TRANSPORT_CELLULAR)));
                    }
                }
            }
        }
        // Android's ConnectivityManager lists upstream networks and omits local tethering
        // interfaces (SoftAP hotspot and wired USB/Ethernet tethering), so add any matching
        // local subnet route too.
        boolean keepActiveEthernet = Build.VERSION.SDK_INT < Build.VERSION_CODES.Q && hasEthernetRoute(routes);
        addLocalRoutes(routes, host, localSubnets(), keepActiveEthernet);
        return routes;
    }

    private static boolean hasEthernetRoute(List<Route> routes) {
        for (Route route : routes) {
            if (route.ethernet && route.active) {
                return true;
            }
        }
        return false;
    }

    static boolean subnetMatches(InetAddress network, int prefixLength, InetAddress host) {
        if (network == null || host == null) {
            return false;
        }
        if (network.isLinkLocalAddress() || host.isLinkLocalAddress()) {
            if (!(network instanceof Inet6Address) || !(host instanceof Inet6Address)) {
                return false;
            }
            int netScope = ((Inet6Address) network).getScopeId();
            int hostScope = ((Inet6Address) host).getScopeId();
            if (netScope == 0 || netScope != hostScope) {
                return false;
            }
        }
        byte[] netBytes = network.getAddress();
        byte[] hostBytes = host.getAddress();
        if (netBytes.length != hostBytes.length || prefixLength <= 0 || prefixLength > netBytes.length * 8) {
            return false;
        }
        int fullBytes = prefixLength / 8;
        for (int i = 0; i < fullBytes; i++) {
            if (netBytes[i] != hostBytes[i]) {
                return false;
            }
        }
        int rem = prefixLength % 8;
        if (rem > 0) {
            int mask = (0xFF << (8 - rem)) & 0xFF;
            if ((netBytes[fullBytes] & mask) != (hostBytes[fullBytes] & mask)) {
                return false;
            }
        }
        return true;
    }

    static boolean isWirelessHotspotInterface(String interfaceName) {
        if (interfaceName == null || interfaceName.isEmpty()) {
            return false;
        }
        String name = interfaceName.toLowerCase(Locale.ROOT);
        if (name.startsWith("wlan") || name.startsWith("swlan") || name.startsWith("softap")
                || name.startsWith("p2p") || name.startsWith("wifi")) {
            return true;
        }
        if (name.startsWith("ap")) {
            if (name.length() == 2) {
                return true;
            }
            char third = name.charAt(2);
            return Character.isDigit(third) || third == '_';
        }
        if (name.startsWith("sap") && name.length() > 3) {
            char fourth = name.charAt(3);
            return Character.isDigit(fourth) || fourth == '_';
        }
        return false;
    }

    static boolean isWiredLocalInterface(String interfaceName) {
        if (interfaceName == null || interfaceName.isEmpty()) {
            return false;
        }
        String name = interfaceName.toLowerCase(Locale.ROOT);
        if (name.startsWith("rndis") || name.startsWith("ncm")
                || name.startsWith("usb") || name.startsWith("eth")) {
            return true;
        }
        return name.startsWith("en") && name.length() > 2 && Character.isDigit(name.charAt(2));
    }

    static void addLocalRoutes(List<Route> routes, InetAddress host, List<LocalSubnet> localSubnets,
                               boolean keepActiveEthernet) {
        for (LocalSubnet subnet : localSubnets) {
            if (!subnetMatches(subnet.address, subnet.prefixLength, host)) {
                continue;
            }
            boolean wirelessHotspot = isWirelessHotspotInterface(subnet.interfaceName);
            boolean wiredLocal = !wirelessHotspot && isWiredLocalInterface(subnet.interfaceName);
            if (!wirelessHotspot && !wiredLocal) {
                continue;
            }
            if (keepActiveEthernet && subnet.interfaceName != null
                    && subnet.interfaceName.toLowerCase(Locale.ROOT).startsWith("wlan")) {
                // On Android < Q, an active Ethernet fallback route has prefixLength 0 and no
                // interface name; do not let an ordinary associated wlan* LAN override it.
                continue;
            }
            boolean covered = false;
            for (Route existing : routes) {
                if (subnet.interfaceName.equals(existing.interfaceName)
                        && existing.prefixLength >= subnet.prefixLength) {
                    covered = true;
                    break;
                }
            }
            if (!covered) {
                routes.add(new Route(subnet.interfaceName, false, false, false, subnet.prefixLength,
                        wirelessHotspot, false));
            }
        }
    }

    private static List<LocalSubnet> localSubnets() {
        List<LocalSubnet> subnets = new ArrayList<>();
        try {
            Enumeration<NetworkInterface> interfaces = NetworkInterface.getNetworkInterfaces();
            if (interfaces == null) {
                return subnets;
            }
            while (interfaces.hasMoreElements()) {
                NetworkInterface iface = interfaces.nextElement();
                try {
                    if (!iface.isUp() || iface.isLoopback()) {
                        continue;
                    }
                    String name = iface.getName();
                    for (InterfaceAddress addr : iface.getInterfaceAddresses()) {
                        InetAddress ip = addr.getAddress();
                        int prefix = addr.getNetworkPrefixLength();
                        if (ip != null && prefix > 0
                                && !ip.isLoopbackAddress()
                                && !ip.isAnyLocalAddress()
                                && !ip.isLinkLocalAddress()) {
                            subnets.add(new LocalSubnet(name, ip, prefix));
                        }
                    }
                } catch (SocketException | RuntimeException ignored) {
                    // Skip unreadable interfaces; keep inspecting the rest.
                }
            }
        } catch (SocketException | RuntimeException ignored) {
            // No interface list available: leave routes unchanged.
        }
        return subnets;
    }

    /** The active network as the only route, for Android 9 and older. */
    private static List<Route> activeNetworkRoute(ConnectivityManager connectivity) {
        Network active = connectivity.getActiveNetwork();
        NetworkCapabilities capabilities = active == null ? null : connectivity.getNetworkCapabilities(active);
        if (capabilities == null) {
            return new ArrayList<>();
        }
        return activeNetworkRoute(capabilities.hasTransport(NetworkCapabilities.TRANSPORT_ETHERNET),
                capabilities.hasTransport(NetworkCapabilities.TRANSPORT_WIFI),
                capabilities.hasTransport(NetworkCapabilities.TRANSPORT_CELLULAR),
                capabilities.hasTransport(NetworkCapabilities.TRANSPORT_VPN));
    }

    /**
     * The route to guess from the transports of the active network alone: no interface name and
     * the default route, so it can say wireless but never a link speed.
     */
    static List<Route> activeNetworkRoute(boolean ethernet, boolean wifi, boolean cellular, boolean vpn) {
        List<Route> routes = new ArrayList<>();
        if (ethernet || wifi || cellular) {
            routes.add(new Route(null, ethernet, vpn, true, 0, wifi, cellular));
        }
        return routes;
    }

    private static int sysfsMbps(String interfaceName) {
        File file = new File("/sys/class/net/" + interfaceName + "/speed");
        try (BufferedReader reader = new BufferedReader(new FileReader(file))) {
            int mbps = parseMbps(reader.readLine());
            if (mbps > 0) {
                LimeLog.info("Wired link toward the host: " + mbps + " Mbps");
            }
            return mbps;
        } catch (IOException | SecurityException e) {
            // Unreadable (SELinux), or the port is down: it reports no speed.
            return 0;
        }
    }

    /** The speed in Mbps from the interface's speed file; "-1" (unknown) and nonsense give 0. */
    static int parseMbps(String text) {
        if (text == null) {
            return 0;
        }
        try {
            int mbps = Integer.parseInt(text.trim());
            return mbps > 0 && mbps <= MAX_MBPS ? mbps : 0;
        } catch (NumberFormatException e) {
            return 0;
        }
    }
}
