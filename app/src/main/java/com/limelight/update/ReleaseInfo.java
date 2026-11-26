package com.limelight.update;

import com.limelight.nvstream.jni.Json;
import com.limelight.nvstream.jni.NightjarPolicyJni;

import org.json.JSONException;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;

/** The parts of a GitHub release that the updater uses. */
public final class ReleaseInfo {
    public static final class Asset {
        public final String name;
        public final String url;
        public final long size;

        public Asset(String name, String url, long size) {
            this.name = name;
            this.url = url;
            this.size = size;
        }

        private static Asset fromJsonObj(Json.Obj obj) {
            return new Asset(
                    obj.getString("name"),
                    obj.getString("url"),
                    obj.optLong("size", -1));
        }

        private Json.Obj toJsonObj() {
            return Json.obj()
                    .put("name", name)
                    .put("url", url)
                    .put("size", size);
        }
    }

    public final String tag;
    public final String body;
    public final boolean draft;
    public final boolean prerelease;
    public final List<Asset> assets;
    private transient String cachedJson;

    public ReleaseInfo(String tag, String body, boolean draft, boolean prerelease, List<Asset> assets) {
        this.tag = tag;
        this.body = body == null ? "" : body;
        this.draft = draft;
        this.prerelease = prerelease;
        this.assets = Collections.unmodifiableList(new ArrayList<>(assets));
    }

    public static ReleaseInfo parse(String json) throws JSONException {
        if (json == null) {
            throw new JSONException("Null release JSON");
        }
        String parsed = NightjarPolicyJni.releaseInfoParse(json);
        if (parsed == null) {
            throw new JSONException("Invalid release JSON");
        }
        return fromJsonObj(Json.parseObject(parsed));
    }

    static ReleaseInfo fromJsonObj(Json.Obj obj) {
        List<Asset> list = new ArrayList<>();
        if (obj.has("assets") && !obj.isNull("assets")) {
            Json.Arr arr = obj.getArray("assets");
            for (int i = 0; i < arr.length(); i++) {
                list.add(Asset.fromJsonObj(arr.getObject(i)));
            }
        }
        return new ReleaseInfo(
                obj.getString("tag"),
                obj.optString("body", ""),
                obj.optBoolean("draft", false),
                obj.optBoolean("prerelease", false),
                list);
    }

    String toJson() {
        String cached = cachedJson;
        if (cached != null) return cached;
        Json.Arr arr = Json.arr();
        for (Asset asset : assets) {
            arr.add(asset.toJsonObj());
        }
        cached = Json.obj()
                .put("tag", tag)
                .put("body", body)
                .put("html_url", "")
                .put("draft", draft)
                .put("prerelease", prerelease)
                .put("assets", arr)
                .toString();
        cachedJson = cached;
        return cached;
    }

    /** The version without a leading v. */
    public String version() {
        return SemVer.normalize(tag);
    }

    /** Whether this release is a final one newer than the running version. */
    public boolean isUpdateFor(String currentVersion) {
        return currentVersion != null && NightjarPolicyJni.releaseInfoIsUpdateFor(toJson(), currentVersion);
    }

    /** The APK for this build: nightjar-X.Y.Z.apk, or nightjar-X.Y.Z-root.apk for the root flavor. */
    public Asset apkAsset(boolean root) {
        String raw = NightjarPolicyJni.releaseInfoApkAsset(toJson(), root);
        return raw == null ? null : Asset.fromJsonObj(Json.parseObject(raw));
    }

    /** SHA256SUMS-X.Y.Z.txt, or null when the release has none. */
    public Asset checksumAsset() {
        String raw = NightjarPolicyJni.releaseInfoChecksumAsset(toJson());
        return raw == null ? null : Asset.fromJsonObj(Json.parseObject(raw));
    }
}
