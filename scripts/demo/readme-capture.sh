#!/bin/bash
# Captures the README's Android pictures and clips from Nightjar's offline demo (a debug build's
# DemoLauncherActivity: a sample library and a bundled test scene, no PC and no network).
#
#   readme-capture.sh <captures>/android [stills] [clips] [tv] [phone]
#
# Writes raw/<state>.png and clips/<scene>.mp4 for docs/media/build_readme_media.py at the repository root (or the
# Rubylight checkout this folder is mirrored from). adb picks the device from ANDROID_SERIAL; NIGHTJAR_DEMO_PACKAGE
# names a debug build installed under another application ID. Each part wants its own device, which an emulator
# stands in for well:
#
#   stills, clips  a 1920x1080 landscape handheld (--es ui handheld); the taps of the on-screen controls clip are
#                  in its pixels
#   tv             a 1920x1080 Android TV
#   phone          an upright phone
#
# Android's status bar is emptied for the run (no clock, no icons) and given back at the end, and the full-screen
# hint is marked as seen. Before capturing, turn off anything else the device draws over apps (a handheld's
# floating FPS counter, for one): screencap and screenrecord take those along.
export MSYS_NO_PATHCONV=1
OUT="${1:?captures folder}"; shift
# adb pull wants a Windows path on Windows.
PULL_OUT="$(cygpath -m "$OUT" 2>/dev/null || echo "$OUT")"
PKG="${NIGHTJAR_DEMO_PACKAGE:-com.butterpollo.client.debug}"
DEMO="$PKG/com.limelight.demo.DemoLauncherActivity"
mkdir -p "$OUT/raw" "$OUT/clips"

want() { [ $# -eq 1 ] && [ "$1" != tv ] && [ "$1" != phone ] && return 0; local name="$1"; shift; for arg in "$@"; do [ "$arg" = "$name" ] && return 0; done; return 1; }
start() { adb shell am force-stop "$PKG"; adb shell am start -n "$DEMO" "$@" >/dev/null; }
shot() { sleep "${2:-6}"; adb exec-out screencap -p > "$OUT/raw/$1.png"; }
# Keys come from a gamepad, so Nightjar shows a controller's buttons in its hints.
key() { for name in "$@"; do adb shell input gamepad keyevent "KEYCODE_$name"; sleep 0.6; done; }
record() { adb shell screenrecord --bit-rate 20000000 --time-limit "$2" "/sdcard/readme-$1.mp4" & RECORDER=$!; sleep 1.2; }
finish() { wait "$RECORDER"; sleep 0.5; adb pull "/sdcard/readme-$1.mp4" "$PULL_OUT/clips/$1.mp4" >/dev/null; adb shell rm "/sdcard/readme-$1.mp4"; }

adb shell svc power stayon true
adb shell input keyevent KEYCODE_WAKEUP; sleep 1
adb shell settings put secure immersive_mode_confirmations confirmed
adb shell cmd statusbar send-disable-flag clock system-icons notification-icons

if want stills "$@"; then
  # Right and back left gives the hero the focus, and the top bar the controller's buttons for it.
  start --es state home --es host native --es ui handheld --es look dark --es view default; sleep 7; key DPAD_RIGHT DPAD_LEFT; shot home 2
  start --es state home --es host native --es ui handheld --es look day --es view default; sleep 7; key DPAD_RIGHT DPAD_LEFT; shot home-day 2
  # B opens the Control Center during a stream.
  start --es state stream --es ui handheld; sleep 8; key BUTTON_B; shot control-center 3
  start --es state statistics --es ui handheld; shot stream-stats 8
  start --es state stream --es ui handheld --ez modern true; shot stream-modern 8
  start --es state stream --es ui handheld --ez pckeyboard true; shot stream-keyboard 8
fi

if want clips "$@"; then
  # Home in the TV layout: the focus crosses two rows of covers, then opens a game's page.
  start --es state home --es host native --es ui tv --es look dark --es view default; sleep 8
  record tv-home 19
  key DPAD_RIGHT DPAD_RIGHT DPAD_RIGHT
  key DPAD_DOWN DPAD_RIGHT DPAD_RIGHT DPAD_RIGHT
  key DPAD_UP DPAD_LEFT DPAD_LEFT
  key DPAD_CENTER; sleep 4
  finish tv-home
  # A stream: the Control Center opens, the focus walks down its rail (Picture, Audio, PC) and back, and it closes.
  start --es state stream --es ui handheld; sleep 9
  record control-center 24
  sleep 1.2; key BUTTON_B; sleep 1.2
  key DPAD_LEFT
  for _ in 1 2 3; do key DPAD_DOWN; sleep 1; done
  key DPAD_UP DPAD_UP DPAD_UP; sleep 0.6
  key BUTTON_B; sleep 1.5
  finish control-center
  # The on-screen controls: both sticks, then two face buttons.
  start --es state stream --es ui handheld --ez modern true; sleep 9
  record controls 12
  sleep 1
  adb shell input swipe 276 643 396 573 700; sleep 0.3
  adb shell input swipe 276 643 176 713 700; sleep 0.3
  adb shell input swipe 1248 890 1348 804 700; sleep 0.3
  adb shell input tap 1651 754; sleep 0.4
  adb shell input tap 1762 643; sleep 1.5
  finish controls
fi

if want tv "$@"; then
  start --es state home --es host native --es ui tv --es look dark --es view default; shot tv-home 10
  start --es state detail --es host native --es ui tv --es look dark; shot tv-detail 8
fi

if want phone "$@"; then
  start --es state home --es host native --es look dark --es view default --ez setupcard true; shot phone-home 8
  start --es state detail --es host native --es look dark; shot phone-game 8
fi

adb shell am force-stop "$PKG"
adb shell cmd statusbar send-disable-flag none
adb shell svc power stayon false
ls "$OUT/raw" "$OUT/clips"
