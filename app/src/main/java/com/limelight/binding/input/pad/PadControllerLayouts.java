package com.limelight.binding.input.pad;

import android.content.SharedPreferences;

import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.TreeMap;
import java.util.function.Predicate;

/**
 * On-screen layouts that belong to one physical controller (Rubylight #154): a pad without trackpads or back buttons
 * brings the touch controls it lacks, and the full layout comes back when it goes. A controller is known by its model,
 * the key its button mapping uses ({@link com.limelight.binding.input.ControllerButtonMap#deviceKey}). Its layout is
 * kept per orientation in the on-screen controller's own preferences, beside the layouts for all games and for one
 * game, under its own prefix so an older build and the classic layouts' migration never read it as a game's.
 *
 * <p>Nothing here exists until the player saves a layout for a controller in the editor: without one the controls
 * show the layout for this game or all games, as before, and a connected pad hides them as before.
 */
public final class PadControllerLayouts {
    /** The prefix of a controller's layout: "padlayout:landscape:usb:045e:028e". */
    static final String ROOT = "padlayout";
    /** The prefix of the name a controller's layout was saved under, the connected controller's at the time. */
    static final String NAME = "padname";

    private PadControllerLayouts() {
    }

    /** The key of a controller model's layout in one orientation. */
    public static String key(boolean landscape, String model) {
        return ROOT + ':' + (landscape ? PadLayoutKeys.LANDSCAPE : PadLayoutKeys.PORTRAIT) + ':' + model;
    }

    /** The key of the name a controller model's layout shows under in the settings. */
    public static String nameKey(String model) {
        return NAME + ':' + model;
    }

    /** Whether a controller model has a layout of its own, in either orientation. */
    public static boolean has(SharedPreferences store, String model) {
        return model != null && (store.contains(key(true, model)) || store.contains(key(false, model)));
    }

    /**
     * The controller whose layout the controls show: the last one to connect that has a layout of its own, so plugging
     * in a second pad with its own layout switches to it and unplugging it goes back. Null when none of them has one,
     * and then the controls show what they showed before this feature.
     *
     * @param connected the external controllers' models, in the order they connected
     */
    public static String choose(List<String> connected, Predicate<String> hasLayout) {
        if (connected == null) return null;
        for (int i = connected.size() - 1; i >= 0; i--) {
            String model = connected.get(i);
            if (model != null && hasLayout.test(model)) return model;
        }
        return null;
    }

    /**
     * The saved layout text for a controller model in this orientation, or its other orientation's while this one has
     * none: a control keeps its place as a share of the screen, so the layout still fits after turning.
     */
    static String read(SharedPreferences store, boolean landscape, String model) {
        String text = store.getString(key(landscape, model), null);
        return text != null ? text : store.getString(key(!landscape, model), null);
    }

    /** Saves a controller model's layout for this orientation under the name the settings list it by. */
    static void save(SharedPreferences.Editor edit, boolean landscape, String model, String name, String text) {
        edit.putString(key(landscape, model), text);
        edit.putString(nameKey(model), name == null || name.isEmpty() ? model : name);
    }

    /** The controller models with a layout of their own and the names they were saved under, sorted by name. */
    static List<String[]> saved(SharedPreferences store) {
        Map<String, String> byName = new TreeMap<>(String.CASE_INSENSITIVE_ORDER);
        String prefix = ROOT + ':';
        for (Map.Entry<String, ?> entry : store.getAll().entrySet()) {
            String key = entry.getKey();
            if (!key.startsWith(prefix)) continue;
            String rest = key.substring(prefix.length());
            int colon = rest.indexOf(':');
            if (colon < 0) continue;
            String model = rest.substring(colon + 1);
            String name = store.getString(nameKey(model), model);
            byName.put(name + '\u0000' + model, model);
        }
        List<String[]> out = new ArrayList<>();
        for (Map.Entry<String, String> entry : byName.entrySet()) {
            String name = entry.getKey().substring(0, entry.getKey().indexOf('\u0000'));
            out.add(new String[] {entry.getValue(), name});
        }
        return out;
    }

    /** Forgets a controller model's layouts, both orientations, and its name, as part of a larger edit. */
    static void remove(SharedPreferences.Editor edit, String model) {
        edit.remove(key(true, model)).remove(key(false, model)).remove(nameKey(model));
    }
}
