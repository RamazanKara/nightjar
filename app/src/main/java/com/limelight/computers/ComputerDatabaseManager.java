/**
 * Persistent host registry adapter backed by Rust {@code HostStore} ({@code hosts.json}) via
 * {@link NightjarHostsJni}, with one-time migration from SQLite {@code computers4.db} and
 * {@code LegacyDatabaseReader}/{@code 2}/{@code 3}.
 */
package com.limelight.computers;

import android.content.Context;
import android.database.Cursor;
import android.database.sqlite.SQLiteDatabase;
import android.database.sqlite.SQLiteException;

import androidx.annotation.VisibleForTesting;

import com.limelight.LimeLog;
import com.limelight.nvstream.http.ComputerDetails;
import com.limelight.nvstream.http.NvHTTP;
import com.limelight.nvstream.jni.Json;
import com.limelight.nvstream.jni.NightjarHostsJni;
import com.limelight.utils.RedactedLog;

import org.json.JSONException;
import org.json.JSONObject;

import java.io.ByteArrayInputStream;
import java.io.File;
import java.security.cert.CertificateEncodingException;
import java.security.cert.CertificateException;
import java.security.cert.CertificateFactory;
import java.security.cert.X509Certificate;
import java.util.ArrayList;
import java.util.Base64;
import java.util.LinkedList;
import java.util.List;
import java.util.Set;
import java.util.concurrent.ConcurrentHashMap;

public class ComputerDatabaseManager {
    private static final String COMPUTER_DB_NAME = "computers4.db";
    private static final String COMPUTER_TABLE_NAME = "Computers";

    private interface AddressFields {
        String LOCAL = "local";
        String REMOTE = "remote";
        String MANUAL = "manual";
        String IPv6 = "ipv6";

        String ADDRESS = "address";
        String PORT = "port";
    }

    private static final ConcurrentHashMap<String, Long> HANDLES = new ConcurrentHashMap<>();
    private static final Set<String> MIGRATED_DIRS = ConcurrentHashMap.newKeySet();

    private final long handle;

    public ComputerDatabaseManager(Context c) {
        Context app = c.getApplicationContext();
        this.handle = openAndMigrate(app);
    }

    public void close() {
        // Store handle is shared per application files directory and closed via resetForTests/process exit.
    }

    private static long handleFor(File dir) {
        String key = dir.getAbsolutePath();
        synchronized (HANDLES) {
            return HANDLES.computeIfAbsent(key, NightjarHostsJni::hostStoreOpen);
        }
    }

    private static long openAndMigrate(Context app) {
        File dir = app.getFilesDir();
        long h = handleFor(dir);
        String key = dir.getAbsolutePath();
        if (!MIGRATED_DIRS.contains(key)) {
            synchronized (MIGRATED_DIRS) {
                if (!MIGRATED_DIRS.contains(key)) {
                    if (!NightjarHostsJni.hostStoreHasMigrated(h)) {
                        migrateLegacy(app, h);
                    }
                    if (NightjarHostsJni.hostStoreHasMigrated(h)) {
                        MIGRATED_DIRS.add(key);
                    }
                }
            }
        }
        return h;
    }

    private static void migrateLegacy(Context app, long handle) {
        List<ComputerDetails> combined = new ArrayList<>();
        List<String> deleteAfter = new ArrayList<>();

        File v4Db = app.getDatabasePath(COMPUTER_DB_NAME);
        if (v4Db != null && v4Db.exists()) {
            List<ComputerDetails> v4 = readSqliteV4(v4Db.getPath());
            if (v4 == null) {
                LimeLog.warning("Deferring legacy host migration because computers4.db could not be read");
                return;
            }
            combined.addAll(v4);
        }

        List<ComputerDetails> v1 = LegacyDatabaseReader.migrateAllComputers(app);
        if (!v1.isEmpty()) {
            combined.addAll(v1);
            deleteAfter.add("computers.db");
        }
        List<ComputerDetails> v2 = LegacyDatabaseReader2.migrateAllComputers(app);
        if (!v2.isEmpty()) {
            combined.addAll(v2);
            deleteAfter.add("computers2.db");
        }
        List<ComputerDetails> v3 = LegacyDatabaseReader3.migrateAllComputers(app);
        if (!v3.isEmpty()) {
            combined.addAll(v3);
            deleteAfter.add("computers3.db");
        }

        Json.Arr arr = Json.arr();
        for (ComputerDetails details : combined) {
            if (details != null && details.uuid != null && !details.uuid.isEmpty()) {
                arr.add(toJsonObj(details));
            }
        }
        NightjarHostsJni.hostStoreImportLegacy(handle, arr.toString());
        if (NightjarHostsJni.hostStoreHasMigrated(handle)) {
            for (String db : deleteAfter) {
                app.deleteDatabase(db);
            }
        }
    }

    private static List<ComputerDetails> readSqliteV4(String path) {
        List<ComputerDetails> out = new ArrayList<>();
        try (SQLiteDatabase db = SQLiteDatabase.openDatabase(path, null, SQLiteDatabase.OPEN_READONLY);
             Cursor c = db.rawQuery("SELECT * FROM " + COMPUTER_TABLE_NAME, null)) {
            while (c.moveToNext()) {
                ComputerDetails details = new ComputerDetails();
                details.uuid = c.getString(0);
                details.name = c.getString(1);
                try {
                    JSONObject addresses = new JSONObject(c.getString(2));
                    details.localAddress = tupleFromJson(addresses, AddressFields.LOCAL);
                    details.remoteAddress = tupleFromJson(addresses, AddressFields.REMOTE);
                    details.manualAddress = tupleFromJson(addresses, AddressFields.MANUAL);
                    details.ipv6Address = tupleFromJson(addresses, AddressFields.IPv6);
                } catch (JSONException e) {
                    LimeLog.warning("Skipping corrupt addresses in legacy DB: " + e.getMessage());
                }
                details.externalPort = details.remoteAddress != null
                        ? details.remoteAddress.port : NvHTTP.DEFAULT_HTTP_PORT;
                details.macAddress = c.getString(3);
                byte[] der = c.getBlob(4);
                if (der != null) {
                    try {
                        details.serverCert = (X509Certificate) CertificateFactory.getInstance("X.509")
                                .generateCertificate(new ByteArrayInputStream(der));
                    } catch (CertificateException e) {
                        LimeLog.warning("Skipping corrupt cert in legacy DB: " + e.getMessage());
                    }
                }
                details.state = ComputerDetails.State.UNKNOWN;
                out.add(details);
            }
        } catch (SQLiteException e) {
            LimeLog.warning("Failed to read legacy computers4.db rows: " + e.getMessage());
            return null;
        }
        return out;
    }

    @VisibleForTesting
    public static void closeForTests(Context context) {
        if (context == null) return;
        File dir = context.getApplicationContext().getFilesDir();
        if (dir == null) return;
        String key = dir.getAbsolutePath();
        synchronized (HANDLES) {
            Long h = HANDLES.remove(key);
            MIGRATED_DIRS.remove(key);
            if (h != null && h != 0L) {
                NightjarHostsJni.hostStoreDestroy(h);
            }
        }
    }

    @VisibleForTesting
    public static void resetForTests() {
        synchronized (HANDLES) {
            for (Long h : new ArrayList<>(HANDLES.values())) {
                if (h != null && h != 0L) {
                    NightjarHostsJni.hostStoreDestroy(h);
                }
            }
            HANDLES.clear();
            MIGRATED_DIRS.clear();
        }
    }

    public void deleteComputer(ComputerDetails details) {
        if (details != null && details.uuid != null) {
            NightjarHostsJni.hostStoreDelete(handle, details.uuid);
        }
    }

    public static JSONObject tupleToJson(ComputerDetails.AddressTuple tuple) throws JSONException {
        if (tuple == null) {
            return null;
        }
        JSONObject json = new JSONObject();
        json.put(AddressFields.ADDRESS, tuple.address);
        json.put(AddressFields.PORT, tuple.port);
        return json;
    }

    public static ComputerDetails.AddressTuple tupleFromJson(JSONObject json, String name) throws JSONException {
        if (!json.has(name)) {
            return null;
        }
        JSONObject address = json.getJSONObject(name);
        return new ComputerDetails.AddressTuple(
                address.getString(AddressFields.ADDRESS), address.getInt(AddressFields.PORT));
    }

    /**
     * What {@link #updateComputer} saves of a PC, to tell whether a poll found anything to save.
     * Every persistent field is here: a PC whose columns equal those of the row read is saved as it is.
     */
    static Object[] columns(ComputerDetails details) {
        return new Object[] {
                details.uuid, details.name,
                details.localAddress, details.remoteAddress, details.manualAddress, details.ipv6Address,
                details.macAddress, details.serverCert,
        };
    }

    public boolean updateComputer(ComputerDetails details) {
        if (details == null) {
            return false;
        }
        RedactedLog.addComputerSecrets(details);
        return NightjarHostsJni.hostStoreUpsert(handle, toJsonObj(details).toString());
    }

    public List<ComputerDetails> getAllComputers() {
        Json.Arr arr = Json.parseArray(NightjarHostsJni.hostStoreAll(handle));
        LinkedList<ComputerDetails> computerList = new LinkedList<>();
        for (int i = 0; i < arr.length(); i++) {
            Json.Obj obj = arr.optObject(i);
            if (obj != null) {
                computerList.add(fromJsonObj(obj));
            }
        }
        return computerList;
    }

    public ComputerDetails getComputerByName(String name) {
        if (name == null) {
            return null;
        }
        String raw = NightjarHostsJni.hostStoreGetByName(handle, name);
        return raw == null ? null : fromJsonObj(Json.parseObject(raw));
    }

    public ComputerDetails getComputerByUUID(String uuid) {
        if (uuid == null) {
            return null;
        }
        String raw = NightjarHostsJni.hostStoreGetByUuid(handle, uuid);
        return raw == null ? null : fromJsonObj(Json.parseObject(raw));
    }

    private static Json.Obj toJsonObj(ComputerDetails details) {
        Json.Obj obj = Json.obj()
                .put("uuid", details.uuid == null ? "" : details.uuid)
                .put("name", details.name == null ? "" : details.name)
                .put("local", slotObj(details.localAddress))
                .put("remote", slotObj(details.remoteAddress))
                .put("manual", slotObj(details.manualAddress))
                .put("ipv6", slotObj(details.ipv6Address))
                .put("mac", details.macAddress);
        if (details.serverCert != null) {
            try {
                obj.put("server_cert_b64", Base64.getEncoder().encodeToString(details.serverCert.getEncoded()));
            } catch (CertificateEncodingException e) {
                obj.put("server_cert_b64", null);
            }
        } else {
            obj.put("server_cert_b64", null);
        }
        return obj;
    }

    private static Json.Obj slotObj(ComputerDetails.AddressTuple tuple) {
        if (tuple == null) {
            return null;
        }
        return Json.obj().put("address", tuple.address).put("port", tuple.port);
    }

    private static ComputerDetails fromJsonObj(Json.Obj obj) {
        ComputerDetails details = new ComputerDetails();
        details.uuid = obj.optString("uuid", "");
        details.name = obj.optString("name", "");
        details.localAddress = slotFromJson(obj.optObject("local"));
        details.remoteAddress = slotFromJson(obj.optObject("remote"));
        details.manualAddress = slotFromJson(obj.optObject("manual"));
        details.ipv6Address = slotFromJson(obj.optObject("ipv6"));
        details.externalPort = details.remoteAddress != null
                ? details.remoteAddress.port : NvHTTP.DEFAULT_HTTP_PORT;
        details.macAddress = obj.isNull("mac") ? null : obj.optString("mac", null);
        String certB64 = obj.isNull("server_cert_b64") ? null : obj.optString("server_cert_b64", null);
        if (certB64 != null && !certB64.isEmpty()) {
            try {
                byte[] der = Base64.getDecoder().decode(certB64);
                details.serverCert = (X509Certificate) CertificateFactory.getInstance("X.509")
                        .generateCertificate(new ByteArrayInputStream(der));
            } catch (IllegalArgumentException | CertificateException e) {
                LimeLog.warning("Corrupt certificate in hosts.json: " + e.getMessage());
            }
        }
        details.state = ComputerDetails.State.UNKNOWN;
        RedactedLog.addComputerSecrets(details);
        return details;
    }

    private static ComputerDetails.AddressTuple slotFromJson(Json.Obj obj) {
        if (obj == null) {
            return null;
        }
        String address = obj.optString("address", null);
        int port = obj.optInt("port", 0);
        if (address == null || port <= 0 || port > 65535) {
            return null;
        }
        return new ComputerDetails.AddressTuple(address, port);
    }
}
