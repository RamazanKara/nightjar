# Nightjar design: Obsidian Cinema (7.0)

The spec for Nightjar 7.0's interface on phone, handheld, TV and desktop. Numbers live in [tokens.json](tokens.json), the single source (flat dotted keys). `python scripts/gen_design_tokens.py` from `client/` emits Android XML, Rust constants and CSS; `python scripts/test_gen_design_tokens.py` fails if they are stale. Decisions both apps make (which tier, which glyph, how big, what the Control Center holds) live in `nightjar-policy` and are named where they apply. This file says how to build the rest.

The prototype is in [prototype/](prototype/) (`index.html` lists every screen; `?theme=day|oled`, `?tier=lens|live|baked`, `?hc=1`, `?motion=reduce`, `?stress=1`, `?lang=de`, on TV `?input=remote|pad`). To run it, from `client/design/prototype`:

```
npm install
python ambient.py                    # bakes ambient/, night and day
node tokens-css.mjs                  # tokens.css from tokens.json
node check-tokens.mjs                # fails on any contrast or spring miss, writes nothing
node shoot.mjs [name...]             # shots/
python check-contrast.py             # measures the worst pixel in the shots
```

`covers/` and `frames/` are not in the repository, so a fresh checkout shows empty cover slots. To see art, point `coverSrc` in `lib.js` at the fixtures in `app/src/debug/assets/demo/art/`.

`shots/`, `covers/`, `frames/`, `ambient/`, `tokens.css` and `node_modules/` are generated and ignored. Figtree comes from `client/protocol/desktop/assets`.

## Principles

1. **The game's art is the only colour on the page.** Surfaces are obsidian: near-black, neutral, lit from above by one thin rim. No sky, no moon, no ornament behind the content. A page without art is a dark stage with one soft light from the top (`sky.*`).
2. **Amber means "this one".** Cinema amber (`color.moon`) is the Play capsule, focus, and selection, and nothing else. The Play capsule is solid, never glass. Selection is a dark amber container (`tint.selected.fill`, text `color.onSelected`), focus is an amber ring.
3. **Art goes edge to edge, chrome floats over it.** Glass is for chrome only: docks, capsules, sheet, dialog, Control Center. One blurred layer per surface; groups, controls and selections inside glass are flat tints (`tint.*`) and never blur.
4. **Text reads over the worst cover in every tier.** A white cover at night, a black cover by day. 4.5:1 is a property of the recipe, not of the art; `check-tokens.mjs` proves it.
5. **A stream costs nothing extra.** While a game plays with the Control Center closed there is no blur, no extra window, no added surface. Hard rule, not a setting.
6. **Simple first.** No instruction sentences, no toggle for what is always right, one hover and focus language. The word "Experimental" appears nowhere.

Token names keep the words of the system they grew from (`color.moon` is the amber accent, `sky.*` the stage light, `color.night` the canvas): both apps read them by name.

## Palette

| | Night (default) | Day | OLED | High contrast |
|---|---|---|---|---|
| Canvas | `color.canvas` #090A0D, TV `color.canvasTv` #050608 | `day.color.canvas` #F5F7FA | #000000 | #000000 |
| Docks | `glass.chrome.*` #101218 at 72–85%, panels #181B24 at 92% | white at 80–92% with one `hairline` | night's | opaque black, 2 dp `hc.color.rim` |
| Text | `color.label` #F5F7FA, `color.labelSecondary` #C7CDD9 | #111318, #3A404D | night's | white |
| Accent | `color.moon` #F5B041, ink on it `color.onMoon` #1A1000 | fill unchanged; as ink `day.color.moonInk` #8A5000 | night's | `hc.color.moon` #FFC857 |
| Focus | ring `focus.ringColor` #FFB94D, halo | ring #995800 on a white gap, halo | night's | 4 dp ring, no halo |
| Ambient | luminance held to `ambient.textZoneMaxLuma` (0.08) | lifted to `day.ambient.textZoneMinLuma` (0.56) | night's | none |

Status colours (`color.ok`, `color.warn`, `color.error`) are words plus an icon, never a filled shape, so warn never reads as the amber action. Quit game is the one red control (`color.dangerFill`).

## Glass recipe

Layers, back to front: backdrop → blur → saturate → **brightness** → fill → lens band → sheen (vertical gradient `sheenTop` to clear at `sheenEnd`) → rim (`rimTop`, `rimMid`, `rimBottom`) → shadow behind all of it. Brightness before fill is what keeps white backdrops legible: worst case is `white × brightness × (1 − fillAlpha) + fill`. The rim is 1 px and lit from the top only: bright at the top edge, almost nothing down the sides, a faint return at the bottom.

Materials: `chrome` (docks, capsules), `panel` (sheet, dropdown, dialog), `menu` (Control Center), `chip` (labels on art). Keys are `glass.<material>.<tier>.<property>`.

| Tier | What it is | Android | Desktop (egui over SDL) | Web |
|---|---|---|---|---|
| `lens` | Live blur plus a refracted edge band | API 33+: `RenderNode` + `RenderEffect` blur chained with a colour matrix, `RuntimeShader` for the band | Ring of quads with displaced UVs over the live chain, no shader | `backdrop-filter` plus a masked second layer |
| `live` | Live blur, no band | API 31-32 or a lens probe failure | Linear down/upsample chain through SDL render targets, sampled with screen-space UVs | `backdrop-filter` |
| `baked` | Samples the static ambient in screen space, × brightness, under a denser fill. Live content behind does not show | API 25-30, low RAM, failed probe, power saver | CPU-blurred backdrop texture, same UV meshes | Not needed |
| `solid` | High contrast: opaque black, 2 dp light rim, no sheen or shadow | Same | Same | `@media (prefers-contrast: more)`, `prefers-reduced-transparency` |

**Landing on a tier** is `nightjar_policy::glass_probe`, the same on both apps, by capability and never by device name. High contrast is `solid`. Otherwise the best tier the system has, capped at `baked` on low RAM, in power saver and under severe heat; a probed tier stored for this build of this device wins over it and is never raised. Then the probe: once the screen has settled for `tier.probe.settleMs` (a first launch compiles its shaders before that), `tier.probe.frames` frames are timed. A frame is slow when its work goes over `tier.probe.p90BudgetFraction` of the frame budget **and** it lands more than `tier.probe.lateFraction` of the budget late: a 120 Hz handheld at low clocks goes over its share on frames that all land on time. The budget never shrinks below a `tier.probe.minBudgetHz` (120 Hz) frame, so a 144 or 240 Hz screen keeps live glass while the chain fits a 120 Hz frame. As soon as enough slow frames have decided the p90 (7 of 60), the tier steps down once and that result is stored.

**The probe cannot crash twice.** Before the first probed frame the app stores `in_flight` with the build's fingerprint (`glass_probe::Guard`) and clears it when the probe ends either way. A start that finds it still set lands on `baked`, stores that, and logs "Previous live-surface probe did not finish; using baked surfaces". A driver that dies inside the blur chain costs one launch, not every launch.

A cheap TV on `baked` is a full design, not a degraded one: check `variant-tv-home-baked.png`.

**What keeps Android's Home cheap on a TV.** Same look, less fill: dimmed covers and the fading hero draw with no offscreen layer (their children do not overlap; a cover's placeholder is painted only where art does not hide it); a focus move touches two covers, not all of them; the ambient's fade to the canvas is baked into its bitmap, so the rest of the screen blends over one opaque draw (with a dim it stays painted). Cover shadows stop at 2.25 sigmas instead of 3: what lies beyond is under 3 levels in 255.

**Android live blur.** Draw the content into a `RenderNode` at 1/4 scale, apply `RenderEffect.createBlurEffect` chained with `createColorFilterEffect` (saturation and brightness in one matrix), draw it clipped to the glass shape, then fill, sheen and rim. One shared backdrop node per window; each glass surface samples it with its own clip. The band shader only brightens and shifts UVs in the outer `lensWidth` dp.

**Desktop.** `gui.rs` walks `Primitive::Mesh` and skips the rest: a `Shape::Callback` marker is the boundary between content and chrome. Paint content, snapshot the regions under chrome, run the chain, then draw chrome meshes textured with screen-space UVs × vertex colour (brightness is a multiply; saturation exists only at bake time, so `live` on desktop keeps the bake's colour lift). Rim is a ring mesh with a vertex-colour gradient and a feather ring for antialiasing. Shadows and the focus halo are baked nine-slices. Corners are circular.

### Ambient backdrop

Baked once per cover, never per frame. Reference: `prototype/ambient.py`. Squash the **whole** cover to 96×54 (or 54×96 in portrait; hosts give 2:3 box art, no landscape art is assumed anywhere), Gaussian blur, saturate × `ambient.saturate`, hue-shift yellows (35-80°) toward 22° so they never dim to olive, then clamp each pixel's linear luminance to `ambient.textZoneMaxLuma`. A white cover comes out as a safe dark grey. Add ±1 LSB noise at bake: 96×54 stretched over a large dark panel can band.

**Day ambient.** Same squash and blur, then saturate × `day.ambient.saturate` and mix each pixel toward white until its luminance reaches `day.ambient.textZoneMinLuma`. Black bakes to a soft grey, never to dark. `ambient.py` writes both.

## Hero stage

Home opens on the game to continue, as a stage (`HeroStageView` on Android, `ui/hero_stage.rs` on desktop):

- The focused game's ambient fills the screen behind everything and fades to the canvas (`tint.fade`) under the shelves. It follows focus once focus rests, cross-fading over `motion.ambient.ms`.
- The cover stands at the left at its own 2:3 shape, lit: rim, top sheen, a deep shadow. Beside it the title (`type.hero`), one line of facts (PC, last played, running), then the amber Play or Resume capsule and at most one quiet second action. Never more than one amber control on a screen.
- Continue is a short rail beside or under the hero; shelves follow. The stage is at most `layout.stageMaxWidth` wide and centred, so an ultrawide monitor gets margins instead of a stretched hero.
- With no art the stage shows the no-art tile's gradient where the ambient would be. With no games at all it is the dark stage and one line.
- On a phone the middle cover takes 290 of 412 dp where the window is high enough, and otherwise as much as leaves the top row, the name, Play, the first shelf's heading and the upper half of its covers above the tab bar (`HomeHero.coverWidth`). The design's 412 × 915 dp phone gets 285: its top row and name lay out 16 dp taller than the mockup's.

## Control Center

What opens over a stream. It replaces the stream menu's grid and More list with **eight modules on a rail**, each a page of cards and rows (`nightjar_policy::control_center`; `ControlCenterDock` and `ControlCenterCards` on Android, `ui/control_center.rs` and `ui/control_center_cards.rs` on desktop):

| Module | Holds |
|---|---|
| Quick | Keyboards, the controller mouse, the overlay, touch mode, zoom, the clipboard, the user's shortcuts and Add |
| Picture | Quality, video output, diagnostics, picture and scaling, stream size and health, night mode |
| Audio | Microphone, its button, headphone surround, controller audio |
| Clips | Saving a clip and recording, when the PC offers them |
| PC | Session and graphics, PC commands, files, the PC's menu, web UI and status |
| Controller | On-screen controls and their layout, the menu button, game controls, USB devices |
| Guests | Who else plays |
| Power | Reconnect, ending the PC's session |

Every tile the stream menu had sits in exactly one module, so nothing is lost; a module with nothing offered does not show, and a card a later PC feature adds shows only when the PC advertises that feature (`control_center::Gates`, from `HostCapabilities`, never from a PC's brand). **Return to stream, Disconnect and Quit game sit under every page.** Quit game is the one red control.

- **Where it sits** (`control_center::placement`): a sheet from the bottom with the rail across its top on an upright phone; a dock at the right edge with the rail down its left side on everything wide (a phone on its side, a handheld, a TV, a desktop window). Width `size.controlCenterWidth`, `size.controlCenterWidthCompact` on a handheld, `size.controlCenterWidthTv` on a TV; rail `size.controlCenterRail`, `size.controlCenterRailTv` on a TV, whose 10-foot type needs the room for Controller and Keyboard whole; cards `size.controlCenterCard` tall. A card's controller cap stands at its end; where the name's longest word would not fit beside it (a handheld's narrow cards) the cap goes up into the top end corner and the name starts under it.
- **Moving through it.** D-pad or stick inside a page, left onto the rail. The triggers step through the modules from anywhere. Y, X, LB and RB stay the user's own menu buttons (Settings → Controls), shown as caps on their cards. B, Esc and Back close it and give the stream its input back.
- **A card** is a tile: icon, short name, state. A switch that is on wears the amber container (`surface.controlCenter.card.fillOn`). **A row** says more in the client's own words and leads one level down.
- **What it is drawn on** (`control_center::surface`): closed, nothing. Open, the game blurred behind it only where the interface's own tier blurs and the system allows it at that moment: one `MenuBlurWindow` sized to the dock on Android API 31+ (`glass.menu.window.*`: cross-window blur has no colour filter), one pass over the video texture on desktop. Battery saver, severe heat, or the system switching blur off while it is open drops it to the baked panel (`surface.controlCenter.baked.fill`) at once, for that moment only; the stored tier never changes because of it. High contrast is black with the rim.
- **Night over video, always.** The Control Center and everything that stays up over a game are night in every theme: a stream is usually dark and a white panel over it would glare. There is no `day.glass.menu`.
- **Closing removes, never hides.** The window and the blur targets are gone on the frame after closing, and that frame costs what the frame before opening did.

**What stays up while the game plays** (the stats capsule, the menu hint, the stream's notices, both on-screen keyboards) stands on `surface.hud.*` / `glass.menu.hud.*`: see-through fill, lit rim, blur 0 and no shadow, drawn in the view's own pass. Keyboard keys are opaque caps on it, so a label reads over any game.

## Settings

Four sections, Recommended first: **Recommended**, **Controls**, **Audio**, **App**, then the **More settings** fold (Presets, Profiles, Video, Display & image, Streaming, Advanced, Debug). The fold is one row until opened; whether it was open is kept as UI state (`settings_more_open`), never as a setting.

**Preview cards** illustrate the settings whose words nobody can picture: a compact three-card visual preview strip (`Stream smoothness`, `HDR`, `Upscaling`) sits at the top of `Recommended`, and detailed preview cards accompany Frame pacing, HDR, Upscaling and Video size in `Video` and `Display & image`. Each card draws a small live vector diagram of what the setting does (a frame timeline, a tone curve, a pixel edge, a screen with bars), its name and active choice; Drawings are runtime vector shapes on both apps, never screenshots. On Desktop, `Controls` and `Controller` open with a **Visual Controller Studio** card showing an adaptive vector controller diagram (`Xbox`, `PlayStation` DualSense with touchpad, `Nintendo`, `Steam Deck`) alongside a live stick deadzone and response curve.

**Presets** are four outcomes, each one tap: Native (recommended), Smooth TV, Wi-Fi / Away, Battery saver. Native is not labelled "lowest latency": no measurement of ours backs that claim. They set existing settings only and read Custom as soon as one of those is changed by hand.

A setting's help opens from a small pill that names the real button: the `(i)` cap, or Y, △, X, or "Hold OK" (`input_glyphs`).

## First run and pairing

- **First run**: no PCs. The dark stage, and in its middle a **scan pulse**: three thin rings (`sky.ring`) leaving a point, slowly, while discovery runs; still under reduced motion. "Searching for PCs…" (`searching_pc`) and one quiet action, Add PC. No paragraph.
- **A found PC** rises in as a hardware card: the PC glyph, its name, its ping, and Pair as the one amber action. The pulse fades when the first card lands.
- **Pairing** is one sheet (phone) or centred panel: the PIN in four large tabular digits, the PC's name, and one line saying where to type it. On a phone a second tab scans the PC's QR code inside a viewfinder with lit corners. Success is the PC card settling into the list with a green dot; failure is the app's real message and Try again.

## Form factors and scale

One design, four hands. `nightjar_policy::viewport_scale` turns the window, its density and the user's Interface size into the numbers a layout uses, the same on both apps.

| | Top level | Game page | Control Center | Scale |
|---|---|---|---|---|
| Phone | Floating bottom dock (Home, PCs, Settings) plus a round search pill; PC capsule on top | Sheet with detents `sheet.detentMedium` / `sheet.detentLarge`, cover morphs from its card, flick to close | Bottom sheet | The user's size only; 3 covers a row |
| Handheld | Compact top pill with the same three tabs and the PC capsule; hero with Continue beside it | Full page | Compact right dock | Paddings tighten on a 720p screen, targets stay `layout.touchMin` |
| TV | Top capsule, left/right along shelves, up to chrome; ambient follows focus | Full page, no nav bar; Back returns to the same focused cover | Right dock | One 960×540 dp canvas whatever density the TV reports; a target is at least 72 lines of 1080 |
| Desktop | Top capsule; keyboard, mouse and controller walk everything | Full page | Right dock; Esc or Guide | The user's size; at the default, a screen taller than 1600 points fits to its height |

One rule everywhere: Back closes the top layer and restores focus to what opened it. Nothing is reachable by touch or mouse only, and nothing by remote only. Below `layout.settingsMaxWidth` everything is one column; text never runs wider than that.

**Stream resolution** is the screen's own or a step of it, the same five steps on both apps: 100, 85, 75, 67 and 50% (`nightjar_policy::resolution_scale`; 67% is exactly two thirds, so 1080p gives 720p).

## Components

All have default, hover, focus, pressed and disabled, on `solid` too. Touch targets are at least `size.touchMin` (48 dp).

- **Dock (tab bar)**: `chrome`, height `size.tabBarHeight`. The selected pill springs (`motion.tabPill`) and travels as one layer so it never redraws the tabs' text. Under it, one gradient to `tint.fade` (no blur, no extra surface) so names and art never sit half hidden at rest.
- **Top capsule**: `chrome`, `size.topCapsuleHeight`. Status dot, PC name, chevron.
- **Cover card**: 2:3 fixed (`cover.aspect`), `radius.cover`, rim and top sheen, shadow. Status dot on a dark disc at the bottom right for the running game. Label below on phone, only on focus on TV.
- **No-art tile**: see below. Same slot, rim, sheen and shadow as a cover.
- **PC card**: a small screen on a stand, then name, status and one action. Flat `tint.groupOnSky`, never glass.
- **Button hint**: a key cap plus the word, following the input device. See "Button hints".
- **Play button**: solid `play.*` gradient, lit rim, soft glow, dark text. One per screen. Says Play, or Resume when running.
- **Sheet**: `panel`, `radius.sheet`, grabber, drag between detents, flick to close. Scroll hands over to drag at the top edge.
- **Grouped row**: `tint.group` (`tint.groupOnArt` on art), `size.rowHeight`, leading icon optional, value and chevron trailing. Long German labels wrap to two lines, values never truncate first.
- **Switch, slider, segmented, chips**: tints only. Amber for on and filled, white thumb. The slider shows its value at all times and steps with D-pad left and right.
- **Dialog**: `panel`, centred, one primary action in amber; destructive uses `color.dangerFill`. On a TV at night the page behind goes to `dialog.tvBackdropBrightness`.
- **Toast**: `chip`, top on phone and above the shelves on TV, one line.
- **Stats overlay**: `surface.hud`, tabular numbers (`type.numbers`), label colour only. Never blurred.
- **PIN pad**: `panel` sheet on phone, dialog elsewhere. Four digit slots, a 3×4 grid of `tint.control` capsules. The remote's digits and D-pad both work.

**One word.** Ending a game is "Quit game" everywhere: sheet, TV page, desktop, Control Center, PC list (`applist_menu_quit`, German "Spiel beenden"). Never "Close game".

### No-art tile

What an app with no cover art shows, on Android (`Canvas`) and in egui (a gradient mesh, a circle, text). No browser, no procedural art, no decode, nothing cached. It must look like a decision, not a gap. Same in every theme.

1. `h` = FNV-1a 32-bit over the title's lowercase UTF-8 bytes (offset `0x811C9DC5`, prime `0x01000193`, unsigned).
2. Background: linear gradient at 165° from `tile.pair<N>.top` to `tile.pair<N>.bottom`, `N = h % 12`. All 12 pairs pass 4.5:1 with white text at their lightest point; `check-tokens.mjs` enforces it.
3. Disc: one filled circle, white at `tile.discAlpha`, diameter `tile.discScale` × width, centre from `(h >>> 8) % 4` into `[.74,.62] [.26,.7] [.8,.42] [.5,.78]` (x × width, y × height).
4. Title: white, Figtree 600, top left, inset `tile.insetScale` × width, size `tile.titleScale` × width, line height 1.08, tracking -0.01 em, up to 3 lines, then an ellipsis. If the longest word does not fit, drop once to `tile.titleMinScale` × width.
5. Then the cover's own sheen, rim, radius and shadow.

## Touch controller studio

The on-screen controller and its editor (Android only).

- **Skin** (`PadSkin70`, `pad.skin.*`): every control is a dark glass disc with a light rim and a white glyph, readable over any game without a blur. Pressed, it fills amber. The user's opacity runs from 100% down to 0%, where the controls are invisible and still work.
- **Editing** (`PadSnapGrid`, `pad.studio.*`): a dragged control snaps to an 8 dp grid and to guides through the centres and edges of the others and of the screen; a guide shows as one amber line while it holds. Mirror places the same control on the other side. Each snap is one light tick (`SnapGrid`).
- **Inspector** (`PadStudioInspector`): a small dock that can be dragged out of the way, on the baked panel with no blur. For the selected control: size, shape, opacity, Split or Join, Hide; for the layout: add a control, a starting layout (Standard, Compact, Shooter, Racing, Touchpad first), Undo, Reset, and whether it applies to this game or all.

## Focus and hover

One language. **Focus**: `focus.ringColor` ring, dark gap, halo, small lift (`focus.scaleCover`, 1.10 on TV), neighbours recede on TV (`focus.neighbourDim`). On the amber Play button the ring is white (`focus.ringColorOnMoon`). **Hover** is the same step quieter: rim brightens, fill lifts, no ring. It has to read from across a room on a `baked` TV.

### Button hints

Hints show what the device in the hand really has (`nightjar_policy::input_glyphs`). Take the source from the last input event and its device; never assume from form factor. Change on the next input, with no animation.

| Last input | Go | More | Back |
|---|---|---|---|
| Remote (D-pad, no gamepad buttons) | OK | Hold OK | Back |
| Xbox-style gamepad | A | Y | B |
| PlayStation | × | △ | ○ |
| Switch | B | X | A |
| Keyboard | Enter | F | Esc |
| Touch, mouse | No hints | No hints | No hints |

With the Nintendo layout setting on, Go and Back trade places. A remote has no Y, so anything on Y must also open from Hold OK.

### Feedback

`nightjar_policy::ui_feedback_contract` names what the hand may feel: a focus move, a confirm, back, a switch, a snap in the editor, a press that can do nothing, a launch. Each platform maps them to its motor. **While a stream is on screen all of it is off**: the motors belong to the game. A held D-pad ticks no faster than a motor can.

## States

- **Loading**: covers keep their 2:3 slot with `cover.placeholder`, art fades in over `motion.coverIn.ms`, no layout jump, no spinner on Home.
- **PC states**: the PC glyph, the word and the action all say the same thing.

  | State | Screen face | Line | Action |
  |---|---|---|---|
  | Running | The game's art | Green dot, game name | Resume, More |
  | Online | Hash gradient plus disc (the tile's recipe, on the PC name) | Green dot, "Online, 12 ms" | Open, or tap the row |
  | Offline | Dark (`color.offline`) | Grey dot, "Offline" | Wake (`pcview_menu_send_wol`), when the PC has a saved MAC; its covers stay visible |
  | Unpaired | Hash gradient with a lock | Lock, "Not paired" | Pair (`pcview_menu_pair_pc`), in amber text, no second amber fill |

- **App grid**: 3 columns on phone, up to 8 on TV and desktop. Fixed slots, covers and no-art tiles side by side. A PC with no apps shows one line and a Refresh action.
- **Connecting and launching**: one screen. The cover (or tile) sits in the middle of its ambient with one thin ring turning around it and one word: "Connecting" (`conn_establishing_title`), then "Starting" (`conn_starting`). The word changes when the protocol stage does; there is no fake progress bar. The ring stops on the first decoded frame, with nothing held back for it. Cancel, or Back, stops it.
- **Error**: loud, in words with an icon, in the place the action happened. Never a silent fallback. A failed connection is a sheet over the page it came from: "Connection Error" (`conn_error_title`), the app's real message unshortened, an amber Try again, and Test the network as the second action.

## Motion

Springs (mass 1), `motion.<name>.stiffness` and `.ratio`, all settle under 250 ms: press 103, focus 180, nav 184, switch 202, tabPill 206, sheet 222. Android: `SpringAnimation`, retarget on every new input. Desktop: a small damped integrator per value, stepping on real frame time.

- Everything is interruptible: a new input retargets from the current position and velocity.
- Only animate transform and alpha; backdrops update on scroll, never on a timer.
- Launching a game has no exit animation that delays it: the stream starts on the press, the page fades underneath.

**Reduced motion** (`reduced.*`, `Motion.reduced()`): springs become short fades, no scale, no overshoot, ambient cuts, the scan pulse stands still. **High contrast**: `solid` tier, `hc.color.*`, a 4 dp ring, no halo, covers keep their art with a light rim.

## Speed budgets

| What | Budget |
|---|---|
| Feedback for any input | Next frame (`motion.firstFeedbackFrames`) |
| Navigation | Interruptible, ≤ `motion.navMaxMs` (250 ms); input during it is honoured at once |
| Main thread | Nothing blocks input: ambient bake and cover decode off-thread, no blur work on first draw |
| Covers | Appear with no layout jump |
| Launch | No animation delays it |
| Stream with the Control Center closed | Zero added layers or GPU work |
| Control Center closes | The next frame costs what the frame before opening did |
| Scroll on `live` and `lens` | Holds the display rate on the probed device, or the tier steps down. The budget floors at 120 Hz |

## Type

Figtree 400 and 600 only, `type.*`. On Android bundle `Figtree-400.ttf` and `Figtree-600.ttf` from `client/protocol/desktop/assets` in `res/font`.

These files have Latin-1 only (`input_glyphs::figtree_covers`): en, de, es, fr, it and pt-BR draw in Figtree. pl, tr, vi, ru, uk, ja, ko, zh-CN and zh-TW miss letters, so those languages use the system face for the whole UI, and a word never mixes two fonts. Keep measuring the longest language per component.

## Public images

`prototype/banner.html` is the README banner. All store and README shots come from fixtures. Public text never claims first, only, fastest or most beautiful.

## Not exact, on purpose

- True refraction exists only on API 33+ (band shader); desktop uses displaced UVs, everywhere else there is no band.
- Desktop has no live saturation and no superellipse corners.
- Cross-window blur has no colour filter, and devices or battery saver can refuse it.
- Day has only been judged on screenshots so far, not on a real phone in daylight or a real TV panel.
- Both the shipping apps (Android and Desktop) and `prototype/` support all four 7.0 Obsidian Cinema looks (`night`, `day`, `oled`, and `hc`) with the calm overhead stage light (`#090A0D` Night, `#F5F7FA` Day, `#000000` OLED/HC, `#F5B041` warm amber) and zero 6.x starry skies or moon-orbit rings.
