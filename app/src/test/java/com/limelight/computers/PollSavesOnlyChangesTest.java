package com.limelight.computers;

import android.app.Application;
import android.content.Context;
import android.database.sqlite.SQLiteDatabase;

import com.limelight.nativeapi.NativeApiFixtures;
import com.limelight.nvstream.http.ComputerDetails;

import org.json.JSONArray;
import org.json.JSONObject;
import org.junit.After;
import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.Robolectric;
import org.robolectric.RuntimeEnvironment;
import org.robolectric.android.controller.ServiceController;
import org.robolectric.annotation.Config;

import java.io.File;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.net.InetAddress;
import java.net.ServerSocket;
import java.net.Socket;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collections;
import java.util.List;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.function.Supplier;

import static org.junit.Assert.*;

/**
 * Verifies change-detecting {@code hosts.json} persistence across {@link ComputerManagerService},
 * Rust {@code HostManager}, and {@link ComputerDatabaseManager}.
 */
@RunWith(org.robolectric.RobolectricTestRunner.class)
@Config(sdk = 28, application = Application.class)
public class PollSavesOnlyChangesTest {
    private static final String UUID = "polled-host";
    private static final ComputerDetails.AddressTuple REMOTE = new ComputerDetails.AddressTuple("203.0.113.9", 47989);

    @After
    public void tearDown() {
        ComputerDatabaseManager.resetForTests();
    }

    private static final class LoopbackServer implements AutoCloseable {
        final ServerSocket listener;
        final int port;
        final Thread thread;
        final AtomicInteger hits = new AtomicInteger();
        volatile Supplier<String> body = () -> "<root status_code=\"200\"><hostname>PC</hostname><uniqueid>" + UUID
                + "</uniqueid><state>MJ_SERVER_FREE</state><PairStatus>0</PairStatus><mac>AA:BB:CC:DD:EE:01</mac></root>";

        LoopbackServer() throws IOException {
            listener = new ServerSocket(0, 8, InetAddress.getByName("127.0.0.1"));
            port = listener.getLocalPort();
            thread = new Thread(() -> {
                while (!listener.isClosed()) {
                    try (Socket sock = listener.accept()) {
                        InputStream in = sock.getInputStream();
                        byte[] buf = new byte[2048];
                        in.read(buf);
                        String xml = body.get();
                        String reply = "HTTP/1.1 200 OK\r\nContent-Length: " + xml.length()
                                + "\r\nConnection: close\r\n\r\n" + xml;
                        OutputStream out = sock.getOutputStream();
                        out.write(reply.getBytes(StandardCharsets.US_ASCII));
                        out.flush();
                        hits.incrementAndGet();
                    } catch (IOException ignored) {
                    }
                }
            }, "poll-saves-server");
            thread.start();
        }

        @Override public void close() throws Exception {
            listener.close();
            thread.join(2000);
        }
    }

    @Test
    public void identicalPollsNeverRewriteHostsJsonAndChangesPersistOnce() throws Exception {
        Context context = RuntimeEnvironment.getApplication();
        try (LoopbackServer server = new LoopbackServer()) {
            ComputerDetails stored = new ComputerDetails();
            stored.uuid = UUID;
            stored.name = "PC";
            stored.localAddress = new ComputerDetails.AddressTuple("127.0.0.1", server.port);
            stored.remoteAddress = REMOTE;
            stored.macAddress = "AA:BB:CC:DD:EE:01";
            ComputerDatabaseManager saved = new ComputerDatabaseManager(context);
            saved.updateComputer(stored);
            saved.close();

            ServiceController<ComputerManagerLifecycleTest.Manager> controller =
                    Robolectric.buildService(ComputerManagerLifecycleTest.Manager.class).create();
            ComputerManagerService service = controller.get();
            ComputerManagerService.ComputerManagerBinder binder =
                    (ComputerManagerService.ComputerManagerBinder) service.onBind(null);
            try {
                File hostsFile = new File(context.getFilesDir(), "hosts.json");
                assertTrue(hostsFile.isFile());

                // First poll brings host ONLINE and saves active/state in memory; second poll is identical.
                CountDownLatch online = new CountDownLatch(1);
                binder.startPolling(details -> {
                    if (UUID.equals(details.uuid) && details.state == ComputerDetails.State.ONLINE) {
                        online.countDown();
                    }
                });
                assertTrue("host must come ONLINE", online.await(10, TimeUnit.SECONDS));
                Thread.sleep(200);
                assertTrue(hostsFile.setLastModified(1_000_000_000L));
                long unchangedStamp = hostsFile.lastModified();

                // Let subsequent periodic polls run against identical XML; hosts.json stays untouched.
                int before = server.hits.get();
                long deadline = System.currentTimeMillis() + 5000;
                while (server.hits.get() < before + 2 && System.currentTimeMillis() < deadline) {
                    Thread.sleep(50);
                }
                assertTrue(server.hits.get() >= before + 1);
                assertEquals("identical polls must not rewrite hosts.json", unchangedStamp, hostsFile.lastModified());

                // Rename on host triggers exactly one update in hosts.json.
                CountDownLatch renamed = new CountDownLatch(1);
                binder.startPolling(details -> {
                    if (UUID.equals(details.uuid) && "DEN-PC".equals(details.name)) {
                        renamed.countDown();
                    }
                });
                server.body = () -> "<root status_code=\"200\"><hostname>DEN-PC</hostname><uniqueid>" + UUID
                        + "</uniqueid><state>MJ_SERVER_FREE</state><PairStatus>0</PairStatus><mac>AA:BB:CC:DD:EE:02</mac></root>";
                assertTrue("renamed host must be emitted", renamed.await(10, TimeUnit.SECONDS));
                ComputerDetails after = binder.getComputer(UUID);
                assertEquals("DEN-PC", after.name);
                assertEquals("AA:BB:CC:DD:EE:02", after.macAddress);
            } finally {
                controller.destroy();
            }
        }
    }

    @Test
    public void everyColumnAndAddressOfTheTableIsCompared() throws Exception {
        Context context = RuntimeEnvironment.getApplication();
        ComputerDetails stored = new ComputerDetails();
        stored.uuid = UUID;
        stored.name = "PC";
        stored.localAddress = new ComputerDetails.AddressTuple("192.168.1.9", 47989);
        stored.remoteAddress = REMOTE;
        stored.manualAddress = new ComputerDetails.AddressTuple("den.example", 47989);
        stored.ipv6Address = new ComputerDetails.AddressTuple("2001:db8::9", 47989);
        stored.macAddress = "AA:BB:CC:DD:EE:01";
        stored.serverCert = NativeApiFixtures.certificate();
        ComputerDatabaseManager db = new ComputerDatabaseManager(context);
        try {
            assertTrue(db.updateComputer(stored));
            assertFalse(db.updateComputer(stored));

            File hostsFile = new File(context.getFilesDir(), "hosts.json");
            assertTrue(hostsFile.isFile());
            String json = new String(Files.readAllBytes(hostsFile.toPath()), StandardCharsets.UTF_8);
            JSONArray entries = new JSONArray(json);
            assertEquals(1, entries.length());
            JSONObject saved = entries.getJSONObject(0);
            List<String> fields = new ArrayList<>();
            for (java.util.Iterator<String> keys = saved.keys(); keys.hasNext(); ) {
                String k = keys.next();
                if (!saved.isNull(k)) fields.add(k);
            }
            Collections.sort(fields);
            String hint = "ComputerDatabaseManager.columns() has to compare what is new here";
            assertTrue(hint + ": " + fields, fields.containsAll(Arrays.asList(
                    "ipv6_endpoint", "local_endpoint", "mac", "manual_endpoint",
                    "name", "remote_endpoint", "server_certificate", "unique_id")));
            assertEquals(hint, 8, ComputerDatabaseManager.columns(stored).length);
        } finally {
            db.close();
        }
    }

    @Test
    public void legacySqliteMigratesOnceAndDeletedHostDoesNotResurrect() throws Exception {
        Context context = RuntimeEnvironment.getApplication();
        File dbFile = context.getDatabasePath("computers4.db");
        dbFile.getParentFile().mkdirs();
        try (SQLiteDatabase db = SQLiteDatabase.openOrCreateDatabase(dbFile, null)) {
            db.execSQL("CREATE TABLE Computers (UUID TEXT PRIMARY KEY, ComputerName TEXT NOT NULL, "
                    + "Addresses TEXT NOT NULL, MacAddress TEXT, ServerCert BLOB)");
            db.execSQL("INSERT INTO Computers (UUID, ComputerName, Addresses, MacAddress, ServerCert) VALUES (?, ?, ?, ?, ?)",
                    new Object[] {
                            "legacy-pc",
                            "Legacy PC",
                            "{\"local\":{\"address\":\"192.168.1.50\",\"port\":47989}}",
                            "11:22:33:44:55:66",
                            NativeApiFixtures.certificate().getEncoded()
                    });
        }

        ComputerDatabaseManager first = new ComputerDatabaseManager(context);
        ComputerDetails migrated = first.getComputerByUUID("legacy-pc");
        assertNotNull(migrated);
        assertEquals("Legacy PC", migrated.name);
        assertEquals(new ComputerDetails.AddressTuple("192.168.1.50", 47989), migrated.localAddress);
        assertEquals("11:22:33:44:55:66", migrated.macAddress);
        assertEquals(NativeApiFixtures.certificate(), migrated.serverCert);
        assertTrue(new File(context.getFilesDir(), "hosts.json").isFile());

        first.deleteComputer(migrated);
        assertNull(first.getComputerByUUID("legacy-pc"));
        first.close();
        ComputerDatabaseManager.resetForTests();

        assertTrue("legacy SQLite remains intact until future cleanup", dbFile.exists());
        ComputerDatabaseManager second = new ComputerDatabaseManager(context);
        assertNull("deleted host must never resurrect from legacy computers4.db", second.getComputerByUUID("legacy-pc"));
        second.close();
    }

    @Test
    public void updateComputerReturnsTrueOnlyWhenChangedAndDoesNotFabricateLocalAddress() {
        Context context = RuntimeEnvironment.getApplication();
        ComputerDatabaseManager db = new ComputerDatabaseManager(context);

        ComputerDetails manualOnly = new ComputerDetails();
        manualOnly.uuid = "manual-only";
        manualOnly.name = "ManualPC";
        manualOnly.manualAddress = new ComputerDetails.AddressTuple("my-pc.example", 47989);
        assertTrue(db.updateComputer(manualOnly));
        assertFalse("identical upsert returns false", db.updateComputer(manualOnly));
        ComputerDetails manualRead = db.getComputerByUUID("manual-only");
        assertNotNull(manualRead);
        assertNull("manual-only host must not fabricate localAddress", manualRead.localAddress);
        assertEquals(manualOnly.manualAddress, manualRead.manualAddress);

        ComputerDetails remoteOnly = new ComputerDetails();
        remoteOnly.uuid = "remote-only";
        remoteOnly.name = "RemotePC";
        remoteOnly.remoteAddress = new ComputerDetails.AddressTuple("203.0.113.55", 48989);
        assertTrue(db.updateComputer(remoteOnly));
        assertFalse(db.updateComputer(remoteOnly));
        ComputerDetails remoteRead = db.getComputerByUUID("remote-only");
        assertNotNull(remoteRead);
        assertNull("remote-only host must not fabricate localAddress", remoteRead.localAddress);
        assertEquals(remoteOnly.remoteAddress, remoteRead.remoteAddress);

        ComputerDetails ipv6Only = new ComputerDetails();
        ipv6Only.uuid = "ipv6-only";
        ipv6Only.name = "V6PC";
        ipv6Only.ipv6Address = new ComputerDetails.AddressTuple("2001:db8::1", 47989);
        assertTrue(db.updateComputer(ipv6Only));
        ComputerDetails ipv6Read = db.getComputerByUUID("ipv6-only");
        assertNotNull(ipv6Read);
        assertNull("ipv6-only host must not fabricate localAddress", ipv6Read.localAddress);
        assertEquals(ipv6Only.ipv6Address, ipv6Read.ipv6Address);

        ComputerDetails cleared = new ComputerDetails();
        cleared.uuid = "local-cleared";
        cleared.name = "ClearedPC";
        cleared.localAddress = new ComputerDetails.AddressTuple("192.168.1.10", 47989);
        cleared.remoteAddress = new ComputerDetails.AddressTuple("203.0.113.99", 47989);
        assertTrue(db.updateComputer(cleared));
        cleared.localAddress = null;
        assertTrue(db.updateComputer(cleared));
        assertFalse(db.updateComputer(cleared));
        ComputerDetails clearedRead = db.getComputerByUUID("local-cleared");
        assertNotNull(clearedRead);
        assertNull("cleared localAddress must stay null", clearedRead.localAddress);
        assertEquals(cleared.remoteAddress, clearedRead.remoteAddress);

        db.close();
    }

    @Test
    public void legacyComputersDbSurvivesWhenHostsJsonIsUnreadableDirectory() {
        Context context = RuntimeEnvironment.getApplication();
        File v1Db = context.getDatabasePath("computers.db");
        v1Db.getParentFile().mkdirs();
        try (SQLiteDatabase db = SQLiteDatabase.openOrCreateDatabase(v1Db, null)) {
            db.execSQL("CREATE TABLE Computers (ComputerName TEXT, UUID TEXT, LocalAddress TEXT, RemoteAddress TEXT, MacAddress TEXT)");
            db.execSQL("INSERT INTO Computers (ComputerName, UUID, LocalAddress, RemoteAddress, MacAddress) VALUES (?, ?, ?, ?, ?)",
                    new Object[] {"LegacyV1", "v1-uuid", "ADDRESS_PREFIX__192.168.1.40", null, "00:11:22:33:44:55"});
        }

        File hostsDir = new File(context.getFilesDir(), "hosts.json");
        assertTrue(hostsDir.mkdirs());

        ComputerDatabaseManager mgr = new ComputerDatabaseManager(context);
        mgr.close();

        assertTrue("computers.db must survive when hosts.json cannot be written", v1Db.exists());
        assertTrue(hostsDir.isDirectory());
        assertFalse(new File(context.getFilesDir(), ".hosts_migrated").exists());
    }

    @Test
    public void corruptComputers4DbDefersMigrationWithoutMarkingDone() throws Exception {
        Context context = RuntimeEnvironment.getApplication();
        File v4Db = context.getDatabasePath("computers4.db");
        v4Db.getParentFile().mkdirs();
        // Write non-SQLite bytes so opening computers4.db raises SQLiteException.
        java.nio.file.Files.write(v4Db.toPath(), "not-a-sqlite-database".getBytes(java.nio.charset.StandardCharsets.UTF_8));

        ComputerDatabaseManager mgr = new ComputerDatabaseManager(context);
        mgr.close();

        assertFalse("migration must not be marked complete when computers4.db fails to read",
                new File(context.getFilesDir(), ".hosts_migrated").exists());
    }
}
