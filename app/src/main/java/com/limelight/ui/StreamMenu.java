package com.limelight.ui;

import android.app.Activity;
import android.widget.LinearLayout;
import android.widget.TextView;

import com.google.android.material.button.MaterialButton;
import com.limelight.LimeLog;
import com.limelight.R;
import com.limelight.binding.input.KeyboardShortcuts;
import com.limelight.binding.video.PerformanceOverlay;
import com.limelight.console.ConnectionInfo;
import com.limelight.console.NativeMenuController;
import com.limelight.nativeapi.NativeAudio;
import com.limelight.nvstream.http.ComputerDetails;
import com.limelight.nvstream.http.NvApp;

import java.util.ArrayList;
import java.util.EnumMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.function.Supplier;

/**
 * The stream menu, shown as the Control Center over the stream ({@link ControlCenterDock}): the game over the stream's
 * mode in its header, a rail of modules, and the selected module's page of cards and rows as the shared model sorts
 * them ({@link ControlCenterModel}, the same one desktop Nightjar draws from), the user's shortcuts and Add at the end
 * of Quick. Return to stream and the ways out of the session stand under every page. The overlay, the host's page,
 * the shortcuts and their editor open one level down, and Back there returns to the module. Y, X, LB and RB run the
 * cards that show them, on whichever module they are. Items a host or this stream cannot use are left out.
 */
public final class StreamMenu {
    public enum Item {
        KEYBOARD, PC_KEYBOARD, OVERLAY, MICROPHONE, NIGHT_MODE, HEADPHONE_SURROUND,
        TOUCH_MODE, MOUSE_CURSOR, CONTROLLER_MOUSE, ONSCREEN_CONTROLS, CONTROLS_LAYOUT, MIC_BUTTON, MENU_BUTTON, GAME_CONTROLS, KEYBOARD_KEYS,
        VIDEO_SCALING, PICTURE_POSITION, FLIP_SCREEN, ZOOM, ZOOM_RESET, OVERLAY_PAGE, PICTURE_IN_PICTURE, VIDEO_OUTPUT,
        BITRATE, RATE_CHANGE, AUTOMATIC_QUALITY, AUTOMATIC_BITRATE,
        HOST_MENU, SERVER_COMMANDS, CLIPBOARD_SEND, CLIPBOARD_RECEIVE, IMAGE_CLIPBOARD_SEND, IMAGE_CLIPBOARD_RECEIVE,
        USB_DEVICES, GUESTS, FILES, HOST_WEB_UI, HOST_STATUS,
        RECONNECT, DISCONNECT, CONFIRM_DISCONNECT, QUIT_GAME, END_SESSION, END_HOST_SESSION, CLOSE_GAME,
        NATIVE_PAGE, NATIVE_RESUME, SUSPEND_GAME, RESUME_GAME, SHORTCUTS, CLOSE,
        OVERLAY_OFF, OVERLAY_COMPACT, OVERLAY_STATISTICS, OVERLAY_DEBUG,
        OVERLAY_TOP_LEFT, OVERLAY_TOP_RIGHT, OVERLAY_BOTTOM_LEFT, OVERLAY_BOTTOM_RIGHT, OVERLAY_COPY, STREAM_HEALTH, LATENCY_TEST
    }

    public enum Section {
        QUICK(0), INPUT(R.string.stream_section_input), DISPLAY(R.string.stream_section_display),
        QUALITY(R.string.stream_section_quality), HOST(R.string.stream_section_host), SESSION(0),
        OVERLAY_STYLE(0), OVERLAY_POSITION(R.string.overlay_position_section), DIAGNOSTICS(0), NATIVE_SESSION(R.string.native_section_session);

        public final int title;

        Section(int title) {
            this.title = title;
        }
    }

    /** {@code MAIN} is the selected module's own page; the others open one level down from it. */
    public enum Page { MAIN, SHORTCUTS, EDITOR, OVERLAY, NATIVE }

    /**
     * What the Control Center keeps between two openings in one stream: the module it showed and the card or row the
     * focus was on, so it opens where it was left. The stream holds one and hands it to each menu it opens.
     */
    public static final class Session {
        String module;
        Item focused;
    }

    /** What this stream and its host offer right now; the stream activity fills it each time the menu opens. */
    public static final class Options {
        public final NvApp.Role role;
        public final ComputerDetails host;
        /** The streamed app's name for the line under the title; null leaves only the PC's name. */
        public String appName;
        /** The game's cover for the header; null leaves it out. */
        public android.graphics.Bitmap cover;
        /** The stream's bitrate for the slider on the first page, in kbps; 0 leaves the slider out. */
        public int bitrateKbps;
        /**
         * Sends a bitrate the slider settled on and answers the rate the slider is to show: the one sent, or the one still applied
         * when nothing could be sent. With no way to send it the slider is left out.
         */
        public java.util.function.IntUnaryOperator bitrateChange;
        /** The PC has not said yet what it counts the stream's bitrate as: the slider shows it disabled. */
        public boolean bitrateWaiting;
        public boolean canSendInput = true;
        public boolean microphoneOffered, microphoneMuted;
        /** The stream has a microphone button (not on a remote monitor), and whether it shows (its setting is on). */
        public boolean micButtonAvailable, micButtonShown;
        /** The stream has a floating menu button, and whether it shows (its setting is on). */
        public boolean menuButtonAvailable, menuButtonShown;
        /** The stream's size, frame rate, Moonlight video format bits and HDR, for the header's mode line; 0 while unknown. */
        public int streamWidth, streamHeight, streamFps, videoFormat;
        public boolean hdr;
        public boolean overlayShown;
        public String overlayMode = PerformanceOverlay.MODE_COMPACT;
        /** The corner the overlay sits in, as stored under {@code performance_overlay_position}. */
        public String overlayPosition = OverlayPlacement.Corner.TOP_LEFT.key;
        /** False while video plays on an external display and this device is the touchpad. */
        public boolean localOutput = true;
        public boolean mouseConnected, cursorVisible;
        /** A controller is connected, and whether it drives the mouse now. */
        public boolean controllerMouseAvailable, controllerMouseOn;
        public boolean onscreenControls, gameControls, keyboardKeysShown;
        /** Stretching can change live: local MediaCodec video without an upscaler. */
        public boolean scalingAvailable;
        /** The picture can be moved by hand, and a landscape window flipped over: video on this phone's own screen. */
        public boolean picturePositionAvailable, flipAvailable;
        public boolean zoomEnabled;
        public boolean zoomed;
        public boolean pictureInPicture;
        public boolean externalDisplays;
        /** The host can change resolution and frame rate in place, which automatic quality needs. */
        public boolean automaticQuality;
        public boolean automaticBitrateOn;
        /** The host offered 0x553C rate changes to this Rust stream: bitrate and frame rate change in one dialog. */
        public boolean rateChange;
        /** Automatic bitrate on this device can run here, so the automatic bitrate toggle drives it. */
        public boolean rateChangeBitrate;
        /** The host's web UI address is known for this stream. */
        public boolean webUiAddress;
        /** A native API host lets this device close the game (the host control permission); without it Quit app does. */
        public boolean nativeHostControl;
        /** The host's native API offers its own menu in the stream ({@code session.couch_menu}). */
        public boolean hostMenu;
        /** The host offers USB forwarding and it is on here (Rust engine only). */
        public boolean usbForwarding;
        /** The host lets other devices join this game as guests and this device manages them ({@code session.guests}). */
        public boolean sessionGuests;
        /** The host's native API carries clipboard images ({@code clipboard.image}). */
        public boolean imageClipboard;
        /** The host offers night mode and headphone surround for this device's stream ({@code audio.*}). */
        public boolean nightModeOffered, headphoneSurroundOffered;
        /** The host's last Quick Suspend state offers a tile: Resume game while the game is paused, Suspend game otherwise. */
        public boolean quickSuspendOffered, quickSuspendPaused;
        /** What the host last said about them (GET /audio); null until it answered. */
        public NativeAudio hostAudio;
        /** The host offers them but GET /audio failed: the first page says so in a line. */
        public boolean hostAudioUnread;
        /** Host, codec, size, bitrate and link for the native page; null leaves the connection facts out. */
        public ConnectionInfo connection;
        /** What the stream said when it started (the launch notice), in its own words; null or empty for nothing. */
        public String launchNotice;
        /** The PC's video encoder load in percent as the statistics show it ({@code "12.5"}); null while it reports none. */
        public String encoderLoad;
        /** The later PC features this PC advertises, by the model's gate names; a card of theirs shows only when named. */
        public java.util.Set<String> gates = java.util.Collections.emptySet();

        public Options(NvApp.Role role, ComputerDetails host) {
            this.role = role;
            this.host = host;
        }

        boolean mediaStream() {
            return role != NvApp.Role.INPUT_ONLY;
        }
    }

    public static final class Entry {
        public final Section section;
        public final Item item;
        public final int label;
        public final int icon;
        /** For toggles and choices; null for plain actions. */
        public final Boolean checked;
        /** In the error colour: it ends the PC's game or session. */
        public final boolean destructive;

        Entry(Section section, Item item, int label, int icon, Boolean checked, boolean destructive) {
            this.section = section;
            this.item = item;
            this.label = label;
            this.icon = icon;
            this.checked = checked;
            this.destructive = destructive;
        }

        /** Quick toggles change in place; everything else closes the menu or opens a page. */
        public boolean keepsMenuOpen() {
            return item == Item.OVERLAY || item == Item.MICROPHONE || item == Item.NIGHT_MODE || item == Item.HEADPHONE_SURROUND;
        }

        public boolean opensPage() {
            return item == Item.OVERLAY_PAGE || item == Item.NATIVE_PAGE || item == Item.SHORTCUTS;
        }

        @Override
        public String toString() {
            return section + ":" + item;
        }
    }

    public static int title(NvApp.Role role) {
        return StreamMenuCatalog.title(role);
    }

    public static List<Entry> mainPage(Options o) {
        return StreamMenuCatalog.mainPage(o);
    }

    /**
     * The row D-pad focus starts on: the first that doesn't end the stream, so OK pressed right after opening the
     * menu never disconnects or quits.
     */
    public static Item firstFocus(List<Entry> entries) {
        return StreamMenuCatalog.firstFocus(entries);
    }

    /**
     * The native page's actions: Resume closes the menu, Disconnect leaves the session suspended, End session
     * ends it, and the game is closed with Close game when this device has host control, else with Quit app.
     * The connection facts and the Radeon panel follow them.
     */
    public static List<Entry> nativePage(Options o) {
        return StreamMenuCatalog.nativePage(o);
    }

    public static List<Entry> overlayPage(Options o) {
        return StreamMenuCatalog.overlayPage(o);
    }

    /** Runs a chosen item. Items that close the menu run once it is gone. */
    public interface Listener {
        void onStreamMenuItem(Item item);
    }

    /** The user's own shortcuts: kept by the app (KeyboardShortcuts), shown and edited in the menu. */
    public interface Shortcuts {
        List<KeyboardShortcuts.Shortcut> saved();

        void save(List<KeyboardShortcuts.Shortcut> shortcuts);

        /** Closes the menu and presses {@code keys} on the PC once the stream has focus again. */
        void send(int[] keys);
    }

    static final List<String> FALLBACK_FOOTER = StreamMenuCatalog.FALLBACK_FOOTER;

    /** Set by a test for a controller that is there (true) or not (false); null asks Android. */
    static Boolean gamepadForTest;

    private final Activity activity;
    private final Supplier<Options> options;
    private final Listener listener;
    private final Shortcuts shortcuts;
    private final Session session;
    private final ControlCenterDock dock;
    private final ControlCenterPage view;
    private final StreamMenuModulePage modulePage;
    private final StreamMenuShortcutsPage shortcutsPage;
    private final Map<Item, MaterialButton> rows = new EnumMap<>(Item.class);
    private Page page = Page.MAIN;
    private String module = ControlCenterModel.QUICK;
    private List<String> modules = new ArrayList<>();
    private List<Entry> entries = new ArrayList<>();
    private final NativeMenuController radeon;
    private RadeonPanelView radeonView;
    private ControlCenterModel.Dock model;
    // What the page was built from, so a refresh can tell a changed state from items that came or went.
    private List<Item> offeredItems = new ArrayList<>();
    private TextView failure;

    public StreamMenu(Activity activity, Supplier<Options> options, Listener listener) {
        this(activity, options, listener, null, null, null);
    }

    public StreamMenu(Activity activity, Supplier<Options> options, Listener listener, NativeMenuController radeon) {
        this(activity, options, listener, radeon, null, null);
    }

    public StreamMenu(Activity activity, Supplier<Options> options, Listener listener, NativeMenuController radeon, Shortcuts shortcuts) {
        this(activity, options, listener, radeon, shortcuts, null);
    }

    /**
     * {@code radeon} reads and writes the host's Radeon settings for the native page; null leaves the panel out.
     * {@code shortcuts} puts the user's shortcuts on Quick; null leaves them and the Shortcuts card out. {@code session}
     * opens the Control Center on the module and card it was left on; null opens it on Quick.
     */
    public StreamMenu(Activity activity, Supplier<Options> options, Listener listener, NativeMenuController radeon,
                      Shortcuts shortcuts, Session session) {
        this.activity = activity;
        this.options = options;
        this.listener = listener;
        this.radeon = radeon;
        this.shortcuts = shortcuts;
        this.session = session;
        Options current = options.get();
        dock = new ControlCenterDock(activity, activity.getString(title(current.role)));
        view = dock.page();
        dock.setOnClosing(this::remember);
        dock.setOnClosed(this::stopRadeon);
        dock.setKeyHandler(this::hotkey);
        dock.setNavigation(this::select, this::step);
        modulePage = new StreamMenuModulePage(activity, dock.getContext(), view, new StreamMenuModulePage.Host() {
            @Override public void choose(Entry entry) {
                StreamMenu.this.choose(entry);
            }

            @Override public void put(Entry entry, MaterialButton row) {
                rows.put(entry.item, row);
                entries.add(entry);
            }

            @Override public void send(KeyboardShortcuts.Shortcut shortcut) {
                shortcuts.send(shortcut.keys());
            }

            @Override public void addShortcut() {
                shortcutsPage.edit(null);
            }
        });
        shortcutsPage = new StreamMenuShortcutsPage(activity, view, shortcuts, this::back,
                () -> render(Page.SHORTCUTS, options.get(), null, false),
                () -> render(Page.EDITOR, options.get(), null, false));
        if (session != null && session.module != null) module = session.module;
        render(Page.MAIN, current, null, false);
        if (session != null && session.focused != null) dock.setStartFocus(rows.get(session.focused));
    }

    // The panel polls the host only while its page is on screen.
    private void stopRadeon() {
        if (radeonView != null) {
            radeonView.detach();
            radeonView = null;
        }
        if (radeon != null) radeon.stop();
    }

    // The module and the card the focus is on, for the next opening in this stream. It never opens on a way out.
    private void remember() {
        if (session == null) return;
        session.module = module;
        Item focused = page == Page.MAIN ? currentFocusedItem() : null;
        session.focused = null;
        for (Entry entry : entries) {
            if (entry.item == focused && entry.section != Section.SESSION && !entry.destructive) session.focused = focused;
        }
    }

    /** The Control Center's window. */
    public ControlCenterDock sheet() {
        return dock;
    }

    public Page page() {
        return page;
    }

    public List<Entry> entries() {
        return entries;
    }

    /** The card or row for an item on the page that shows, or under it; null when it is elsewhere or left out. */
    public MaterialButton row(Item item) {
        return rows.get(item);
    }

    /**
     * The card or row for an item wherever the Control Center has it: the module that holds it shows, as after choosing
     * it on the rail. Null for an item this stream leaves out, or one on a page a level down that is not open.
     */
    public MaterialButton reveal(Item item) {
        if (rows.get(item) != null) return rows.get(item);
        String holder = model == null ? null : model.moduleOf(StreamMenuCatalog.id(item));
        if (holder == null || !modules.contains(holder)) return null;
        if (page != Page.MAIN || !holder.equals(module)) {
            module = holder;
            render(Page.MAIN, options.get(), null, false);
        }
        return rows.get(item);
    }

    /** The Control Center as the shared model sorted it; null when the model failed. */
    public ControlCenterModel.Dock dock() {
        return model;
    }

    /** The module whose page shows, by the model's key. */
    public String module() {
        return module;
    }

    /** The rail: the modules that have something for this stream, in order. */
    public List<String> modules() {
        return new ArrayList<>(modules);
    }

    /** The cards of the module's page, in order; null on a page a level down. */
    public TileGrid grid() {
        return page == Page.MAIN ? modulePage.grid() : null;
    }

    /** The line saying why the menu could not be laid out, or null when it could. */
    public TextView failure() {
        return failure;
    }

    /** Back: from the editor to the shortcuts, from any other page a level down to its module; a module's page closes. */
    public void back() {
        switch (page) {
            case MAIN: dock.dismiss(); return;
            case EDITOR: render(Page.SHORTCUTS, options.get(), null, false); return;
            case NATIVE: render(Page.MAIN, options.get(), Item.NATIVE_PAGE, false); return;
            case OVERLAY: render(Page.MAIN, options.get(), Item.OVERLAY_PAGE, false); return;
            default: render(Page.MAIN, options.get(), Item.SHORTCUTS, false);
        }
    }

    /** Shows a module's page, as its entry on the rail does. A module the rail does not show changes nothing. */
    public void select(String target) {
        if (!modules.contains(target) || (target.equals(module) && page == Page.MAIN)) return;
        boolean onRail = dock.railFocused();
        module = target;
        render(Page.MAIN, options.get(), null, false);
        // Stepping along the rail keeps the focus on the rail.
        if (onRail) dock.focusRail();
    }

    /** A trigger, a rail arrow or a swipe: the next module (forward) or the one before, as the model steps; it stops at both ends. */
    public void step(boolean forward) {
        if (model == null) return;
        try {
            String next = model.step(module, forward);
            // The model steps along its own pages: one this client shows nothing for is passed over.
            for (int guard = ControlCenterModel.MODULES.size(); !modules.contains(next) && guard > 0; guard--) {
                String further = model.step(next, forward);
                if (further.equals(next)) return;
                next = further;
            }
            if (!next.equals(module)) select(next);
        } catch (StreamMenuModel.Refused e) {
            LimeLog.warning("Stream menu: the step failed: " + e.getMessage());
        }
    }

    private void render(Page target, Options o, Item focus, boolean focusSlider) {
        page = target;
        stopRadeon();
        view.clear();
        rows.clear();
        entries = new ArrayList<>();
        modulePage.clear();
        shortcutsPage.clearViews();
        failure = null;
        // A remote has no face buttons: the caps show only while a controller could press them.
        boolean caps = capsShown();
        dock.setCapsShown(caps);
        List<Entry> offered = offered(o);
        offeredItems = StreamMenuCatalog.items(offered);
        List<KeyboardShortcuts.Shortcut> saved = shortcuts != null && o.canSendInput ? shortcuts.saved() : null;
        Map<String, String> labels = null;
        String refused = null;
        try {
            model = ControlCenterModel.dock(StreamMenuCatalog.offers(offered), saved == null ? null : saved.size(),
                    buttonChoices(activity), o.gates);
            labels = StreamMenuModel.labels();
        } catch (StreamMenuModel.Refused e) {
            LimeLog.warning("Stream menu: the model failed: " + e.getMessage());
            model = null;
            labels = null;
            refused = String.valueOf(e.getMessage());
        }
        modules = new ArrayList<>();
        if (model != null) {
            for (ControlCenterModel.Page shown : model.pages) {
                if (StreamMenuModulePage.shows(shown, offered, o)) modules.add(shown.module);
            }
        }
        if (modules.isEmpty()) modules.add(ControlCenterModel.QUICK);
        if (!modules.contains(module)) module = modules.get(0);
        dock.setRail(modules, module);
        String[] header = target == Page.MAIN ? header(o) : null;
        dock.setHeading(target == Page.EDITOR ? activity.getString(shortcutsPage.editorHeading())
                : header != null ? header[0] : activity.getString(StreamMenuCatalog.heading(target, o)));
        dock.setSubtitle(header == null ? null : header[1]);
        // The game's cover heads a module's page only; a page below says where it is, its back button in the cover's place.
        dock.setCover(target == Page.MAIN && o.role == NvApp.Role.STREAM ? o.cover : null);
        dock.setBackHandler(target == Page.MAIN ? null : this::back);
        ControlCenterModel.Page shown = model == null ? null : model.page(module);
        if (target == Page.MAIN && shown != null) modulePage.render(shown, o, offered, labels, saved, caps);
        else if (target == Page.MAIN) renderWithoutModel(offered, refused);
        else if (target == Page.SHORTCUTS) failure = shortcutsPage.renderShortcuts();
        else if (target == Page.EDITOR) failure = shortcutsPage.renderEditor();
        else renderList(target == Page.NATIVE ? nativePage(o) : overlayPage(o));
        if (target == Page.NATIVE) addNativeFacts(o);
        addFooter(offered, model == null ? FALLBACK_FOOTER : model.footer, labels);
        dock.focusFirstAction();
        StepSlider slider = modulePage.bitrateSlider();
        if (focusSlider && slider != null && !slider.isInTouchMode()) {
            slider.requestFocus();
        } else {
            MaterialButton focused = focus == null ? null : rows.get(focus);
            if (focused != null && !focused.isInTouchMode()) focused.requestFocus();
        }
    }

    // What the stream offers: the entries it can use, and the shortcuts where they can be sent.
    private List<Entry> offered(Options o) {
        List<Entry> list = StreamMenuCatalog.pageEntries(activity, Page.MAIN, o);
        if (shortcuts != null && o.canSendInput) {
            list.add(StreamMenuCatalog.action(Section.QUICK, Item.SHORTCUTS, R.string.stream_menu_shortcuts, R.drawable.ic_keyboard));
        }
        return list;
    }

    // Without the model the stream still runs: the page says why, and every entry stays reachable as a plain list, in
    // this client's order and words, above the ways out. Those stay the model's two; Reconnect and End session join the list.
    private void renderWithoutModel(List<Entry> offered, String why) {
        failure = view.addNote(view.addGroup(), activity.getString(R.string.stream_menu_model_failed, why), true);
        List<Entry> plain = new ArrayList<>();
        for (Entry entry : offered) if (!FALLBACK_FOOTER.contains(StreamMenuCatalog.id(entry.item))) plain.add(entry);
        addRows(plain);
        entries.addAll(plain);
        if (!plain.isEmpty()) view.setFirstAction(rows.get(firstFocus(plain)));
    }

    /** The view of the model's card at this index on the module's page. */
    android.view.View cell(int index) {
        return modulePage.cell(index);
    }

    /**
     * The stream's bitrate while the menu is open: the slider and the bitrate row show it, without sending anything,
     * and the slider is usable or disabled (while the PC has not said what it counts the bitrate as).
     */
    public void showBitrate(int kbps, boolean usable) {
        modulePage.showBitrate(kbps, usable);
    }

    /** The bitrate slider of the page that shows; null when it has none. */
    StepSlider bitrateSlider() {
        return modulePage.bitrateSlider();
    }

    /** The string for a label key of the shared model ({@code stream_menu::label}), or 0 for a key this client lacks. */
    public static int tileLabel(String key) {
        return StreamMenuCatalog.tileLabel(key);
    }

    /**
     * A module page's title and the line under it, as desktop Nightjar's: the game over the stream's mode
     * ({@code 1440p · 144 FPS · HEVC}), or over the PC while the mode is unknown; without a game, the menu's title
     * over the app and PC.
     */
    String[] header(Options o) {
        return StreamMenuCatalog.header(activity, o);
    }

    /**
     * Under every page: Return to stream, then the ways out in the model's order and in its words ({@link #FALLBACK_FOOTER},
     * in this client's words, when it failed); the last that ends something is the one red control. A way out only
     * Android offers, a remote monitor's, keeps its own words.
     */
    private void addFooter(List<Entry> offered, List<String> footer, Map<String, String> labels) {
        String back = labels != null ? StreamMenuCatalog.label(activity, labels, StreamMenuModel.CLOSE)
                : activity.getString(R.string.stream_menu_return);
        rows.put(Item.CLOSE, dock.setPrimary(back));
        Entry red = null;
        for (Entry entry : offered) {
            if (entry.section == Section.SESSION && entry.destructive && footer.contains(StreamMenuCatalog.id(entry.item))) red = entry;
        }
        for (Entry entry : offered) {
            if (entry.section != Section.SESSION || !footer.contains(StreamMenuCatalog.id(entry.item))) continue;
            String name = labels != null && labels.containsKey(StreamMenuCatalog.id(entry.item))
                    ? StreamMenuCatalog.label(activity, labels, StreamMenuCatalog.id(entry.item)) : activity.getString(entry.label);
            int icon = entry == red ? R.drawable.ic_glass_power : entry.item == Item.DISCONNECT ? R.drawable.ic_stream_leave : entry.icon;
            MaterialButton button = dock.addExit(icon, name, entry == red,
                    StreamMenuCatalog.footerHotkey(model == null ? null : model.buttons, entry.item));
            // A page a level down that lists the same way out keeps its own row under this name.
            rows.putIfAbsent(entry.item, button);
            entries.add(entry);
            button.setOnClickListener(pressed -> choose(entry));
        }
    }

    /** A remote has no face buttons: the caps show only while a controller could press them. */
    private static boolean capsShown() {
        return gamepadForTest != null ? gamepadForTest : StreamMenuHint.hasGamepad();
    }

    /** What the user chose in Settings for the controller's buttons here: by button, the buttons left on Default out. */
    static Map<String, String> buttonChoices(android.content.Context context) {
        return StreamMenuCatalog.buttonChoices(context);
    }

    /**
     * A saved choice as this version reads it: Close game and Quit game became one choice, Quit game; a button that
     * ran End session is back on Default.
     */
    static String buttonChoice(String saved) {
        return StreamMenuCatalog.buttonChoice(saved);
    }

    /**
     * Saves an older install's close_game choice as quit_game, so the setting's list shows Quit game selected. Runs
     * before the settings screen reads the values; the menu reads either the same way.
     */
    public static void upgradeButtonChoices(android.content.SharedPreferences prefs) {
        StreamMenuCatalog.upgradeButtonChoices(prefs);
    }

    /**
     * A controller button on a module's page runs its card, on whichever module the model has it, or a way out under
     * the pages: Y, X, LB or RB.
     */
    public boolean hotkey(int keyCode) {
        if (page != Page.MAIN || model == null) return false;
        String key;
        switch (keyCode) {
            case android.view.KeyEvent.KEYCODE_BUTTON_Y: key = "y"; break;
            case android.view.KeyEvent.KEYCODE_BUTTON_X: key = "x"; break;
            case android.view.KeyEvent.KEYCODE_BUTTON_L1: key = "lb"; break;
            case android.view.KeyEvent.KEYCODE_BUTTON_R1: key = "rb"; break;
            default: return false;
        }
        try {
            ControlCenterModel.Place place = model.hotkey(key);
            if (place != null) {
                // The card's module shows first: what a button ran is never out of sight.
                select(place.module);
                android.view.View card = place.module.equals(module) ? modulePage.cell(place.card) : null;
                if (card == null) return false;
                card.performClick();
                return true;
            }
        } catch (StreamMenuModel.Refused e) {
            LimeLog.warning("Stream menu: the button's card failed: " + e.getMessage());
            return false;
        }
        String id = model.buttons.get(key);
        for (Entry entry : entries) {
            MaterialButton row = rows.get(entry.item);
            if (row != null && StreamMenuCatalog.id(entry.item).equals(id)) {
                // From a button Disconnect asks first, as Quit, Close and End always do: a stray press must not end the stream.
                if (entry.item == Item.DISCONNECT) listener.onStreamMenuItem(Item.CONFIRM_DISCONNECT);
                else row.performClick();
                return true;
            }
        }
        return false;
    }

    /** A built-in's name in this language. */
    public static int builtInLabel(String id) {
        return StreamMenuCatalog.builtInLabel(id);
    }

    /** The editor's Save: the model checks the chord, then the list is saved with it added or replaced. */
    void saveDraft() {
        shortcutsPage.saveDraft();
    }

    // A page of rows a level down: switches side by side at the top, then the rest under their sections.
    private void renderList(List<Entry> list) {
        entries = new ArrayList<>(list);
        addRows(list);
        view.setFirstAction(rows.get(firstFocus(list)));
    }

    private void addRows(List<Entry> list) {
        LinearLayout quickRow = null;
        Section section = null;
        for (Entry entry : list) {
            MaterialButton button;
            if (entry.section == Section.QUICK) {
                if (quickRow == null) quickRow = view.addToggleRow();
                button = view.addToggle(quickRow, entry.icon, activity.getString(entry.label), Boolean.TRUE.equals(entry.checked));
                if (entry.checked == null) button.setCheckable(false);
            } else {
                if (entry.section != section) {
                    if (entry.section.title != 0) view.addSection(entry.section.title);
                    else if (section != null) view.addDivider();
                }
                if (entry.destructive) button = view.addDestructiveAction(entry.icon, activity.getString(entry.label));
                else if (entry.opensPage()) button = view.addPageAction(entry.icon, activity.getString(entry.label));
                else button = view.addAction(entry.icon, activity.getString(entry.label));
                if (entry.checked != null) {
                    if (entry.icon == R.drawable.ic_check) StreamMenuCatalog.markChoice(button, entry.checked);
                    else StreamMenuCatalog.markToggleRow(button, entry.checked);
                }
            }
            section = entry.section;
            rows.put(entry.item, button);
            button.setOnClickListener(pressed -> choose(entry));
        }
    }

    /**
     * Shows what changed while the menu is open, such as the host's answer about its audio or the stream's new size:
     * the switches and the header in place, or the page again when items or its notices came or went.
     */
    public void refresh() {
        if (page != Page.MAIN || !dock.isShowing()) return;
        Options o = options.get();
        StepSlider slider = modulePage.bitrateSlider();
        boolean sliding = slider != null && (slider.isTouching() || slider.isPressed());
        if (StreamMenuCatalog.items(offered(o)).equals(offeredItems) && modulePage.notesMatch(module, o)) {
            if (!sliding) refreshToggles(o);
            showHeader(o);
            modulePage.showEncoderLoad(o.encoderLoad);
        } else if (!sliding) {
            boolean focusSlider = slider != null && slider.hasFocus();
            render(page, o, focusSlider ? null : currentFocusedItem(), focusSlider);
        }
    }

    private Item currentFocusedItem() {
        for (Map.Entry<Item, MaterialButton> entry : rows.entrySet()) {
            MaterialButton row = entry.getValue();
            if (row != null && row.hasFocus()) return entry.getKey();
        }
        return null;
    }

    // The title and mode line as render sets them; the title is set again only when it changed.
    private void showHeader(Options o) {
        String[] header = header(o);
        if (!android.text.TextUtils.equals(dock.heading(), header[0])) dock.setHeading(header[0]);
        dock.setSubtitle(header[1]);
    }

    /** The line saying the host's audio settings could not be read, or null when the page has none. */
    public TextView audioNote() {
        return modulePage.audioNote();
    }

    /** Quick's line with what the stream said when it started; null without one or on another page. */
    public TextView launchNotice() {
        return modulePage.launchNotice();
    }

    /** Picture's line with the PC's video encoder load; null while the PC reports none or on another page. */
    public TextView encoderLoad() {
        return modulePage.encoderLoad();
    }

    // The connection facts and the Radeon panel under the native page's actions.
    private void addNativeFacts(Options o) {
        if (o.connection != null) {
            view.addSection(R.string.native_section_connection);
            for (ConnectionInfo.Line line : o.connection.lines(activity)) {
                view.addInfo(activity.getString(line.label), line.value);
            }
        }
        if (radeon != null) {
            // The panel builds its rows with a sheet's own calls: this sheet only makes them, into the page, and never shows.
            radeonView = new RadeonPanelView(new ActionSheet(dock.getContext(), "", true), view.addGroup(), radeon::write);
            radeon.start(radeonView::show);
        }
    }

    private void choose(Entry entry) {
        Haptics.feedback(dock.panel(), entry.checked != null && !entry.opensPage() ? Haptics.Event.TOGGLE : Haptics.Event.CONFIRM);
        if (entry.opensPage()) {
            render(entry.item == Item.NATIVE_PAGE ? Page.NATIVE : entry.item == Item.SHORTCUTS ? Page.SHORTCUTS : Page.OVERLAY,
                    options.get(), null, false);
            return;
        }
        listener.onStreamMenuItem(entry.item);
        if (entry.keepsMenuOpen()) refreshToggles(options.get());
    }

    // A switch changes in place: show its new state without rebuilding the page or moving focus.
    private void refreshToggles(Options o) {
        for (Entry entry : mainPage(o)) {
            MaterialButton button = rows.get(entry.item);
            if (entry.checked == null || button == null || modulePage.refresh(entry, button)) continue;
            boolean checked = Boolean.TRUE.equals(entry.checked);
            if (button.isCheckable()) {
                button.setChecked(checked);
                button.setIconResource(entry.icon);
            } else if (entry.section != Section.QUICK && entry.icon != R.drawable.ic_check) {
                StreamMenuCatalog.markToggleRow(button, checked);
                button.setIconResource(entry.icon);
                button.setText(entry.label);
            }
        }
    }

    public void show() {
        dock.show();
    }
}
