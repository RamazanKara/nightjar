package com.limelight.computers;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertNotNull;
import static org.junit.Assert.assertTrue;

import android.app.Application;
import android.content.Intent;
import android.content.ServiceConnection;

import com.limelight.nvstream.http.ComputerDetails;
import com.limelight.nvstream.http.InviteLink;
import com.limelight.nvstream.http.PairingLink;

import org.bouncycastle.asn1.x500.X500Name;
import org.bouncycastle.cert.jcajce.JcaX509CertificateConverter;
import org.bouncycastle.cert.jcajce.JcaX509v3CertificateBuilder;
import org.bouncycastle.operator.jcajce.JcaContentSignerBuilder;
import org.junit.After;
import org.junit.Test;
import org.robolectric.Robolectric;
import org.robolectric.RuntimeEnvironment;
import org.robolectric.annotation.Config;

import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.math.BigInteger;
import java.net.InetAddress;
import java.net.ServerSocket;
import java.net.Socket;
import java.nio.charset.StandardCharsets;
import java.security.KeyPair;
import java.security.KeyPairGenerator;
import java.security.MessageDigest;
import java.security.cert.X509Certificate;
import java.util.Date;
import java.util.HexFormat;

/** Whatever answers at a link's address may claim any PC's identity: a web page can pick that address. */
@org.junit.runner.RunWith(org.robolectric.RobolectricTestRunner.class)
@Config(sdk = 28, application = Application.class)
public class LinkToKnownPcTest {
    private static final ComputerDetails.AddressTuple KNOWN_ADDRESS = new ComputerDetails.AddressTuple("192.168.4.2", 47989);

    public static class Manager extends ComputerManagerService {
        @Override public boolean bindService(Intent intent, ServiceConnection connection, int flags) {
            return false;
        }
    }

    @After
    public void tearDown() {
        ComputerDatabaseManager.resetForTests();
    }

    private static final class ClaimingServer implements AutoCloseable {
        final ServerSocket listener;
        final int port;
        final Thread thread;

        ClaimingServer() throws IOException {
            listener = new ServerSocket(0, 8, InetAddress.getByName("127.0.0.1"));
            port = listener.getLocalPort();
            thread = new Thread(() -> {
                while (!listener.isClosed()) {
                    try (Socket sock = listener.accept()) {
                        InputStream in = sock.getInputStream();
                        byte[] buf = new byte[2048];
                        in.read(buf);
                        String body = "<root status_code=\"200\"><hostname>Claimed</hostname>"
                                + "<uniqueid>known-pc</uniqueid><state>MJ_SERVER_FREE</state>"
                                + "<PairStatus>0</PairStatus><HttpsPort>47984</HttpsPort></root>";
                        String reply = "HTTP/1.1 200 OK\r\nContent-Length: " + body.length()
                                + "\r\nConnection: close\r\n\r\n" + body;
                        OutputStream out = sock.getOutputStream();
                        out.write(reply.getBytes(StandardCharsets.US_ASCII));
                        out.flush();
                    } catch (IOException ignored) {
                    }
                }
            }, "claiming-server");
            thread.start();
        }

        @Override public void close() throws Exception {
            listener.close();
            thread.join(2000);
        }
    }

    private static X509Certificate certificate(long serial) throws Exception {
        KeyPairGenerator generator = KeyPairGenerator.getInstance("RSA");
        generator.initialize(2048);
        KeyPair keys = generator.generateKeyPair();
        X500Name name = new X500Name("CN=Known PC");
        return new JcaX509CertificateConverter().getCertificate(new JcaX509v3CertificateBuilder(
                name, BigInteger.valueOf(serial), new Date(0), new Date(4102444800000L), name, keys.getPublic())
                .build(new JcaContentSignerBuilder("SHA256withRSA").build(keys.getPrivate())));
    }

    private static PairingLink linkNaming(X509Certificate certificate, int port) throws Exception {
        String fp = HexFormat.of().formatHex(MessageDigest.getInstance("SHA-256").digest(certificate.getEncoded()));
        PairingLink link = PairingLink.parse("rubylight://pair?v=1&host=127.0.0.1&port=" + port + "&otp=4821&fp=" + fp);
        assertNotNull(link);
        return link;
    }

    private static ComputerDetails knownPc(X509Certificate pinned) {
        ComputerDetails known = new ComputerDetails();
        known.uuid = "known-pc";
        known.name = "Desk";
        known.state = ComputerDetails.State.ONLINE;
        known.manualAddress = KNOWN_ADDRESS;
        known.activeAddress = KNOWN_ADDRESS;
        known.serverCert = pinned;
        return known;
    }

    private static ComputerDetails linkDetails(int port) {
        ComputerDetails details = new ComputerDetails();
        details.manualAddress = new ComputerDetails.AddressTuple("127.0.0.1", port);
        return details;
    }

    private static void seedKnownPc(ComputerDetails known) {
        ComputerDatabaseManager db = new ComputerDatabaseManager(RuntimeEnvironment.getApplication());
        try {
            db.updateComputer(known);
        } finally {
            db.close();
        }
    }

    @Test
    public void aServerClaimingAPinnedPcIsRefusedBeforeItsAddressChanges() throws Exception {
        X509Certificate pinned = certificate(1);
        seedKnownPc(knownPc(pinned));
        try (ClaimingServer server = new ClaimingServer()) {
            var controller = Robolectric.buildService(Manager.class).create();
            Manager service = controller.get();
            ComputerManagerService.ComputerManagerBinder binder =
                    (ComputerManagerService.ComputerManagerBinder) service.onBind(null);

            assertEquals(ComputerManagerService.LinkAdd.CERTIFICATE_MISMATCH,
                    service.addComputerForLink(linkDetails(server.port), linkNaming(certificate(2), server.port)));

            ComputerDetails stored = binder.getComputer("known-pc");
            assertEquals(KNOWN_ADDRESS, stored.manualAddress);
            assertEquals("Desk", stored.name);
            controller.destroy();
        }
    }

    private static InviteLink inviteNaming(String id, X509Certificate certificate) throws Exception {
        String fp = HexFormat.of().formatHex(MessageDigest.getInstance("SHA-256").digest(certificate.getEncoded()));
        InviteLink invite = InviteLink.parse("rubylight://invite#v=1&k=" + "ab".repeat(32) + "&fp=" + fp + "&id=" + id
                + "&perm=controller&exp=4000000000&ports=47989,47984,48010,47998,48000,47999,48002");
        assertNotNull(invite);
        return invite;
    }

    private static ComputerDetails invited(X509Certificate certificate) {
        ComputerDetails details = new ComputerDetails();
        details.uuid = "known-pc";
        details.name = "Claimed";
        details.state = ComputerDetails.State.UNKNOWN;
        details.manualAddress = new ComputerDetails.AddressTuple("203.0.113.9", 47989);
        details.activeAddress = details.manualAddress;
        details.httpsPort = 47984;
        details.serverCert = certificate;
        return details;
    }

    @Test
    public void anInviteNeverReplacesTheCertificateOfAPcPairedWithAnotherOne() throws Exception {
        X509Certificate pinned = certificate(1);
        X509Certificate other = certificate(2);
        seedKnownPc(knownPc(pinned));
        var controller = Robolectric.buildService(Manager.class).create();
        Manager service = controller.get();
        ComputerManagerService.ComputerManagerBinder binder =
                (ComputerManagerService.ComputerManagerBinder) service.onBind(null);
        InternetPlay.save(service, "known-pc", "its own");

        assertTrue(service.inviteClaimsKnownPc(inviteNaming("known-pc", other)));
        assertFalse(service.inviteClaimsKnownPc(inviteNaming("known-pc", pinned)));
        assertFalse(service.inviteClaimsKnownPc(inviteNaming("another-pc", other)));

        assertFalse(service.addInvitedComputer(invited(other), "the invite's"));
        ComputerDetails stored = binder.getComputer("known-pc");
        assertEquals(pinned, stored.serverCert);
        assertEquals(KNOWN_ADDRESS, stored.manualAddress);
        assertEquals("Desk", stored.name);
        assertEquals("its own", InternetPlay.reply(service, "known-pc"));

        assertTrue(service.addInvitedComputer(invited(pinned), "the invite's"));
        assertEquals("the invite's", InternetPlay.reply(service, "known-pc"));
        controller.destroy();
    }

    @Test
    public void anInviteNeverReplacesASavedPcsCertificateEither() throws Exception {
        X509Certificate pinned = certificate(1);
        seedKnownPc(knownPc(pinned));
        var controller = Robolectric.buildService(Manager.class).create();
        Manager service = controller.get();
        ComputerDatabaseManager database = new ComputerDatabaseManager(service);
        try {
            assertFalse(service.addInvitedComputer(invited(certificate(2)), "the invite's"));
            assertEquals(pinned, database.getComputerByUUID("known-pc").serverCert);
            assertEquals(KNOWN_ADDRESS, database.getComputerByUUID("known-pc").manualAddress);
        } finally {
            database.close();
        }
        controller.destroy();
    }

    @Test
    public void theRightCertificateStillGoesThrough() throws Exception {
        com.limelight.nvstream.http.TestHttpServer.ensureIdentities();
        X509Certificate pinned = com.limelight.nvstream.http.TestHttpServer.hostCert();
        seedKnownPc(knownPc(pinned));
        final int[] httpsPort = {0};
        try (com.limelight.nvstream.http.TestHttpServer server = new com.limelight.nvstream.http.TestHttpServer(req ->
                com.limelight.nvstream.http.TestHttpServer.Response.xml(200,
                        "<root status_code=\"200\"><hostname>Claimed</hostname>"
                                + "<uniqueid>known-pc</uniqueid><state>MJ_SERVER_FREE</state>"
                                + "<PairStatus>0</PairStatus><HttpsPort>" + httpsPort[0] + "</HttpsPort></root>"))) {
            httpsPort[0] = server.httpsPort();
            var controller = Robolectric.buildService(Manager.class).create();
            Manager service = controller.get();
            assertEquals(ComputerManagerService.LinkAdd.ADDED,
                    service.addComputerForLink(linkDetails(server.httpPort()), linkNaming(pinned, server.httpPort())));
            controller.destroy();
        }
    }

    @Test
    public void aPcWithoutACertificateStillGoesThrough() throws Exception {
        seedKnownPc(knownPc(null));
        try (ClaimingServer server = new ClaimingServer()) {
            var controller = Robolectric.buildService(Manager.class).create();
            Manager service = controller.get();
            assertEquals(ComputerManagerService.LinkAdd.ADDED,
                    service.addComputerForLink(linkDetails(server.port), linkNaming(certificate(2), server.port)));
            controller.destroy();
        }
    }
}
