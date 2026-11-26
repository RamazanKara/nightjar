package com.limelight.nvstream.wol;

import android.content.Context;
import android.content.SharedPreferences;

import com.limelight.nvstream.jni.Json;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;

/**
 * This device's keys for waking each PC from away through its wake relays (GET /wake/relay,
 * docs/designs/15-away-wake-relay.md), kept per PC in app-private preferences, which backups leave out, for as long
 * as the PC offers {@code HostCapabilities.Feature.AWAY_WAKE_RELAY}. {@link WakeTargetsPoll} reads them with the PC's
 * other wake targets. They are secrets: nothing here logs them.
 */
public final class WakeRelayKeys {
    static final String PREFERENCES = "WakeRelayKeys";
    /** Kept for a PC that offers the feature but hands this device no keys. */
    static final String NONE = "";

    /** What is kept for one PC. */
    public static final class Kept {
        /** The bundle's JSON for {@code NightjarJni.wakeRelayRequest}, or null without keys. */
        public final String bundle;
        /** The names of the relays the bundle has keys for, in the order they are asked. */
        public final List<String> relays;

        Kept(String bundle, List<String> relays) {
            this.bundle = bundle;
            this.relays = Collections.unmodifiableList(relays);
        }

        public boolean hasRelay() {
            return bundle != null && !relays.isEmpty();
        }

        /** From the kept text: a bundle without relays, or one that does not parse, has none. */
        public static Kept parse(String kept) {
            List<String> names = new ArrayList<>();
            if (kept == null || kept.isEmpty()) {
                return new Kept(null, names);
            }
            try {
                Json.Arr relays = Json.parseObject(kept).optArray("relays");
                for (int i = 0; relays != null && i < relays.length(); i++) {
                    Json.Obj relay = relays.optObject(i);
                    if (relay != null) {
                        names.add(relay.optString("name", ""));
                    }
                }
            } catch (Json.SyntaxException e) {
                names.clear();
            }
            return new Kept(names.isEmpty() ? null : kept, names);
        }
    }

    private final SharedPreferences preferences;

    public WakeRelayKeys(Context context) {
        preferences = context.getApplicationContext().getSharedPreferences(PREFERENCES, Context.MODE_PRIVATE);
    }

    /** What is kept for the PC, or null while it does not offer waking from away. */
    public Kept load(String hostUuid) {
        String kept = hostUuid == null ? null : preferences.getString(hostUuid, null);
        return kept == null ? null : Kept.parse(kept);
    }

    /** The PC offers waking from away: keeps its bundle (null: none for this device) in place of the kept one. */
    public void store(String hostUuid, String bundle) {
        if (hostUuid == null) {
            return;
        }
        String kept = bundle == null ? NONE : bundle;
        if (!kept.equals(preferences.getString(hostUuid, null))) {
            preferences.edit().putString(hostUuid, kept).apply();
        }
    }

    /** The PC no longer offers waking from away: its keys go. */
    public void forget(String hostUuid) {
        if (hostUuid != null && preferences.contains(hostUuid)) {
            preferences.edit().remove(hostUuid).apply();
        }
    }

    /** The PC was removed or forgot this device: its keys go. */
    public static void forget(Context context, String hostUuid) {
        if (context != null) {
            new WakeRelayKeys(context).forget(hostUuid);
        }
    }
}
