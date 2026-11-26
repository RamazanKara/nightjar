//! Linux: a touchpad's fingers from its evdev device (multi-touch protocol B), read without
//! grabbing it, so the desktop keeps its pointer and the main loop drops what it makes of them.
//! Reading `/dev/input` needs the `input` group (or a seat ACL) on most distributions.

use std::collections::HashSet;
use std::fs::{File, OpenOptions};
use std::io::{ErrorKind, Read};
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::Duration;

use nightjar_core::input::touchpad::{length_mm, unit};
use sdl3_sys::everything::SDL_Window;

use super::{Event, Frame, RawContact};

const EV_SYN: u16 = 0x00;
const EV_KEY: u16 = 0x01;
const EV_ABS: u16 = 0x03;
const SYN_REPORT: u16 = 0;
const SYN_DROPPED: u16 = 3;
const BTN_LEFT: u16 = 0x110;
const ABS_MT_SLOT: u16 = 0x2f;
const ABS_MT_POSITION_X: u16 = 0x35;
const ABS_MT_POSITION_Y: u16 = 0x36;
const ABS_MT_TOOL_TYPE: u16 = 0x37;
const ABS_MT_TRACKING_ID: u16 = 0x39;
const ABS_MT_PRESSURE: u16 = 0x3a;
const ABS_CNT: usize = 0x40;
const KEY_CNT: usize = 0x300;
const INPUT_PROP_POINTER: usize = 0x00;
const INPUT_PROP_DIRECT: usize = 0x01;
const MT_TOOL_PALM: i32 = 2;
const MAX_SLOTS: usize = 16;
/// New touchpads (a keyboard cover attached) are looked for this often.
const RESCAN: Duration = Duration::from_secs(2);

/// `_IOC(_IOC_READ, 'E', nr, size)` (x86, Arm and RISC-V layout).
fn ioc_read(nr: u32, size: usize) -> libc::Ioctl {
    ((2u32 << 30) | ((size as u32 & 0x3fff) << 16) | ((b'E' as u32) << 8) | nr) as libc::Ioctl
}

/// `EVIOCGBIT(ev, len)`: which codes of an event type (0 for the types) the device has.
fn bits(file: &File, ev: u32, len: usize) -> Option<Vec<u8>> {
    let mut buffer = vec![0u8; len];
    // SAFETY: the kernel writes at most `len` bytes into the buffer.
    let n = unsafe { libc::ioctl(file.as_raw_fd(), ioc_read(0x20 + ev, len), buffer.as_mut_ptr()) };
    (n >= 0).then_some(buffer)
}

/// `EVIOCGPROP(len)`: the device's input properties.
fn properties(file: &File) -> Option<Vec<u8>> {
    let mut buffer = vec![0u8; 4];
    // SAFETY: the kernel writes at most the buffer's length.
    let n = unsafe { libc::ioctl(file.as_raw_fd(), ioc_read(0x09, buffer.len()), buffer.as_mut_ptr()) };
    (n >= 0).then_some(buffer)
}

/// `EVIOCGKEY(len)`: global key/button states currently depressed on the device.
fn key_bits(file: &File) -> Option<Vec<u8>> {
    let mut buffer = vec![0u8; KEY_CNT / 8];
    // SAFETY: the kernel writes at most the buffer's length.
    let n = unsafe { libc::ioctl(file.as_raw_fd(), ioc_read(0x18, buffer.len()), buffer.as_mut_ptr()) };
    (n >= 0).then_some(buffer)
}

/// `EVIOCGABS(abs)`: an axis' range, resolution and current value.
fn absinfo(file: &File, code: u16) -> Option<libc::input_absinfo> {
    // SAFETY: zeroed is a valid input_absinfo (plain integers).
    let mut info: libc::input_absinfo = unsafe { std::mem::zeroed() };
    // SAFETY: the kernel writes one input_absinfo.
    let n = unsafe {
        libc::ioctl(file.as_raw_fd(), ioc_read(0x40 + u32::from(code), size_of::<libc::input_absinfo>()), &mut info as *mut libc::input_absinfo)
    };
    (n >= 0).then_some(info)
}

/// `EVIOCGMTSLOTS`: an axis' value in each slot.
fn slot_values(file: &File, code: u16, slots: usize) -> Option<Vec<i32>> {
    let mut buffer = vec![0i32; slots + 1];
    buffer[0] = i32::from(code);
    // SAFETY: the kernel reads the code and writes at most `slots` values after it.
    let n = unsafe { libc::ioctl(file.as_raw_fd(), ioc_read(0x0a, buffer.len() * 4), buffer.as_mut_ptr()) };
    (n >= 0).then(|| buffer[1..].to_vec())
}

fn has(bits: &[u8], index: usize) -> bool {
    bits.get(index / 8).is_some_and(|byte| byte & (1 << (index % 8)) != 0)
}

/// Whether the device's bits are a touchpad's: multi-touch protocol B positions on an indirect
/// pointer (a touchscreen is direct).
pub fn is_touchpad(abs: &[u8], props: &[u8]) -> bool {
    has(abs, usize::from(ABS_MT_SLOT))
        && has(abs, usize::from(ABS_MT_POSITION_X))
        && has(abs, usize::from(ABS_MT_POSITION_Y))
        && has(props, INPUT_PROP_POINTER)
        && !has(props, INPUT_PROP_DIRECT)
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Slot {
    /// -1 when no finger is in the slot.
    tracking: i32,
    x: i32,
    y: i32,
    pressure: i32,
    palm: bool,
}

/// One touchpad's multi-touch state machine, fed evdev events.
#[derive(Debug)]
pub struct State {
    device: usize,
    slots: Vec<Slot>,
    slot: usize,
    button: bool,
    x: (i32, i32),
    y: (i32, i32),
    pressure: Option<(i32, i32)>,
    size_mm: (u16, u16),
    changed: bool,
    dropped: bool,
}

impl State {
    fn new(device: usize, slots: usize, x: (i32, i32), y: (i32, i32), pressure: Option<(i32, i32)>, size_mm: (u16, u16)) -> State {
        State {
            device,
            slots: vec![Slot { tracking: -1, ..Slot::default() }; slots.clamp(1, MAX_SLOTS)],
            slot: 0,
            button: false,
            x,
            y,
            pressure,
            size_mm,
            changed: false,
            dropped: false,
        }
    }

    /// One event; a frame at each `SYN_REPORT` that changed something. After `SYN_DROPPED`
    /// events up to the next report are lost: `resync` then reads the slots again.
    pub fn event(&mut self, kind: u16, code: u16, value: i32) -> Option<Frame> {
        if self.dropped {
            if kind == EV_SYN && code == SYN_REPORT {
                self.dropped = false;
            }
            return None;
        }
        match (kind, code) {
            (EV_SYN, SYN_REPORT) if self.changed => {
                self.changed = false;
                return Some(self.frame());
            }
            (EV_SYN, SYN_DROPPED) => self.dropped = true,
            (EV_KEY, BTN_LEFT) => {
                self.button = value != 0;
                self.changed = true;
            }
            (EV_ABS, ABS_MT_SLOT) => self.slot = usize::try_from(value).unwrap_or(usize::MAX),
            (EV_ABS, _) => {
                let slot = self.slots.get_mut(self.slot)?;
                match code {
                    ABS_MT_TRACKING_ID => {
                        slot.tracking = value;
                        slot.palm = false;
                    }
                    ABS_MT_POSITION_X => slot.x = value,
                    ABS_MT_POSITION_Y => slot.y = value,
                    ABS_MT_PRESSURE => slot.pressure = value,
                    ABS_MT_TOOL_TYPE => slot.palm = value == MT_TOOL_PALM,
                    _ => return None,
                }
                self.changed = true;
            }
            _ => {}
        }
        None
    }

    fn frame(&self) -> Frame {
        let contacts = self.slots.iter().filter(|s| s.tracking >= 0).map(|s| RawContact {
            pointer_id: s.tracking as u32,
            x: unit(s.x as f32, self.x.0 as f32, self.x.1 as f32),
            y: unit(s.y as f32, self.y.0 as f32, self.y.1 as f32),
            pressure: self.pressure.map_or(0.0, |(min, max)| unit(s.pressure as f32, min as f32, max as f32)),
            touching: !s.palm,
        }).collect();
        Frame { device: self.device, contacts: Some(contacts), button: self.button, width_mm: self.size_mm.0, height_mm: self.size_mm.1 }
    }

    /// Sets the active MT slot from `EVIOCGABS(ABS_MT_SLOT).value`, clamped to the valid slot range.
    pub fn set_slot(&mut self, value: i32) {
        self.slot = usize::try_from(value).map_or(0, |s| s.min(self.slots.len().saturating_sub(1)));
    }

    /// Updates `BTN_LEFT` after a `SYN_DROPPED` resync (`EVIOCGKEY`) so a missed release never sticks.
    pub fn set_button(&mut self, down: bool) {
        if self.button != down {
            self.button = down;
            self.changed = true;
        }
    }

    /// Puts the slots read from the device in place, as after `SYN_DROPPED` or at the start.
    fn set_slots(&mut self, tracking: &[i32], x: &[i32], y: &[i32]) {
        for (i, slot) in self.slots.iter_mut().enumerate() {
            slot.tracking = tracking.get(i).copied().unwrap_or(-1);
            slot.x = x.get(i).copied().unwrap_or(slot.x);
            slot.y = y.get(i).copied().unwrap_or(slot.y);
            slot.palm = false;
        }
        self.changed = true;
    }
}

struct Pad {
    file: File,
    path: PathBuf,
    state: State,
}

impl Pad {
    fn open(path: &Path, device: usize) -> Result<Option<Pad>, std::io::Error> {
        let file = OpenOptions::new().read(true).custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC).open(path)?;
        let (Some(abs), Some(props)) = (bits(&file, u32::from(EV_ABS), ABS_CNT / 8), properties(&file)) else { return Ok(None) };
        if !is_touchpad(&abs, &props) {
            return Ok(None);
        }
        let (Some(x), Some(y), Some(slot)) = (absinfo(&file, ABS_MT_POSITION_X), absinfo(&file, ABS_MT_POSITION_Y), absinfo(&file, ABS_MT_SLOT)) else {
            return Ok(None);
        };
        let pressure = has(&abs, usize::from(ABS_MT_PRESSURE)).then(|| absinfo(&file, ABS_MT_PRESSURE)).flatten().map(|p| (p.minimum, p.maximum));
        let size_mm = (length_mm(x.minimum, x.maximum, x.resolution as f32), length_mm(y.minimum, y.maximum, y.resolution as f32));
        let slots = usize::try_from(slot.maximum).map_or(1, |max| max + 1);
        let mut state = State::new(device, slots, (x.minimum, x.maximum), (y.minimum, y.maximum), pressure, size_mm);
        state.set_slot(slot.value);
        let mut pad = Pad { file, path: path.to_owned(), state };
        pad.resync();
        Ok(Some(pad))
    }

    /// Reads which fingers are down now: at the start, or after the kernel dropped events.
    fn resync(&mut self) {
        if let Some(slot) = absinfo(&self.file, ABS_MT_SLOT) {
            self.state.set_slot(slot.value);
        }
        let n = self.state.slots.len();
        if let (Some(tracking), Some(x), Some(y)) = (
            slot_values(&self.file, ABS_MT_TRACKING_ID, n),
            slot_values(&self.file, ABS_MT_POSITION_X, n),
            slot_values(&self.file, ABS_MT_POSITION_Y, n),
        ) {
            self.state.set_slots(&tracking, &x, &y);
        }
        if let Some(keys) = key_bits(&self.file) {
            self.state.set_button(has(&keys, usize::from(BTN_LEFT)));
        }
    }

    /// The frames since the last call; Err when the touchpad is gone.
    fn read(&mut self, out: &mut Vec<Event>) -> Result<(), ()> {
        const SIZE: usize = size_of::<libc::input_event>();
        let mut buffer = [0u8; SIZE * 64];
        loop {
            match self.file.read(&mut buffer) {
                Ok(0) => return Err(()),
                Ok(n) => {
                    for chunk in buffer[..n].as_chunks::<SIZE>().0 {
                        // SAFETY: a whole input_event the kernel wrote; read unaligned.
                        let event: libc::input_event = unsafe { std::ptr::read_unaligned(chunk.as_ptr().cast()) };
                        let was_dropped = self.state.dropped;
                        if let Some(frame) = self.state.event(event.type_, event.code, event.value) {
                            out.push(Event::Frame(frame));
                        }
                        if was_dropped && !self.state.dropped {
                            self.resync();
                        }
                    }
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => return Ok(()),
                Err(e) if e.kind() == ErrorKind::Interrupted => {}
                Err(_) => return Err(()),
            }
        }
    }
}

#[derive(Default)]
struct ScanShared {
    open_paths: HashSet<PathBuf>,
    pending: Vec<Pad>,
    stop: bool,
}

/// Scans `/dev/input/event*` for new touchpads outside `known`, returning newly opened pads and
/// whether any candidate failed with `PermissionDenied`.
fn scan_dir(known: &HashSet<PathBuf>) -> (Vec<Pad>, bool) {
    let mut found = Vec::new();
    let mut denied = false;
    let Ok(entries) = std::fs::read_dir("/dev/input") else {
        return (found, false);
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let is_event = path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with("event"));
        if !is_event || known.contains(&path) {
            continue;
        }
        match Pad::open(&path, 0) {
            Ok(Some(pad)) => found.push(pad),
            Ok(None) => {}
            Err(e) if e.kind() == ErrorKind::PermissionDenied => denied = true,
            Err(_) => {}
        }
    }
    (found, denied)
}

fn scan_loop(shared: &(Mutex<ScanShared>, Condvar)) {
    let (lock, wake) = shared;
    let mut guard = lock.lock().unwrap_or_else(PoisonError::into_inner);
    while !guard.stop {
        let (next, _) = wake.wait_timeout(guard, RESCAN).unwrap_or_else(PoisonError::into_inner);
        if next.stop {
            return;
        }
        let known = next.open_paths.clone();
        drop(next);
        let (found, _) = scan_dir(&known);
        guard = lock.lock().unwrap_or_else(PoisonError::into_inner);
        if guard.stop {
            return;
        }
        for pad in found {
            if guard.open_paths.insert(pad.path.clone()) {
                guard.pending.push(pad);
            }
        }
    }
}

/// The touchpads' evdev devices. Hotplug rescans run on a background thread so drawing never
/// stalls on `/dev/input` enumeration or `open()`.
pub struct Evdev {
    pads: Vec<Pad>,
    next_device: usize,
    shared: Arc<(Mutex<ScanShared>, Condvar)>,
    scanner: Option<JoinHandle<()>>,
}

impl Evdev {
    pub fn install(_window: *mut SDL_Window) -> Result<Evdev, String> {
        let (initial, denied) = scan_dir(&HashSet::new());
        if initial.is_empty() {
            return Err(if denied {
                "Nightjar may not read the touchpad. Add your user to the input group, then sign in again.".into()
            } else {
                "no touchpad found".into()
            });
        }
        let shared = Arc::new((Mutex::new(ScanShared::default()), Condvar::new()));
        let mut evdev = Evdev {
            pads: Vec::new(),
            next_device: 1,
            shared: shared.clone(),
            scanner: None,
        };
        {
            let mut guard = shared.0.lock().unwrap_or_else(PoisonError::into_inner);
            for mut pad in initial {
                guard.open_paths.insert(pad.path.clone());
                pad.state.device = evdev.next_device;
                evdev.next_device += 1;
                eprintln!(
                    "nightjar: touchpad {} ({} x {} mm)",
                    pad.path.display(),
                    pad.state.size_mm.0,
                    pad.state.size_mm.1
                );
                evdev.pads.push(pad);
            }
        }
        if let Ok(handle) = std::thread::Builder::new()
            .name("touchpad-scan".into())
            .spawn(move || scan_loop(&shared))
        {
            evdev.scanner = Some(handle);
        }
        Ok(evdev)
    }

    pub fn take(&mut self) -> Vec<Event> {
        let arrived = {
            let mut guard = self.shared.0.lock().unwrap_or_else(PoisonError::into_inner);
            std::mem::take(&mut guard.pending)
        };
        for mut pad in arrived {
            pad.state.device = self.next_device;
            self.next_device += 1;
            eprintln!(
                "nightjar: touchpad {} ({} x {} mm)",
                pad.path.display(),
                pad.state.size_mm.0,
                pad.state.size_mm.1
            );
            self.pads.push(pad);
        }
        let mut out = Vec::new();
        let mut removed = Vec::new();
        self.pads.retain_mut(|pad| match pad.read(&mut out) {
            Ok(()) => true,
            Err(()) => {
                out.push(Event::Removed(pad.state.device));
                removed.push(pad.path.clone());
                false
            }
        });
        if !removed.is_empty() {
            let mut guard = self.shared.0.lock().unwrap_or_else(PoisonError::into_inner);
            for path in removed {
                guard.open_paths.remove(&path);
            }
        }
        out
    }
}

impl Drop for Evdev {
    fn drop(&mut self) {
        let (lock, wake) = &*self.shared;
        lock.lock().unwrap_or_else(PoisonError::into_inner).stop = true;
        wake.notify_all();
        if let Some(thread) = self.scanner.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> State {
        State::new(1, 5, (0, 1000), (0, 500), Some((0, 255)), (100, 50))
    }

    fn ids(frame: &Frame) -> Vec<(u32, f32, f32, bool)> {
        frame.contacts.as_ref().unwrap().iter().map(|c| (c.pointer_id, c.x, c.y, c.touching)).collect()
    }

    #[test]
    fn protocol_b_slots_become_frames_at_each_report() {
        let mut s = state();
        assert_eq!(s.event(EV_ABS, ABS_MT_SLOT, 0), None);
        s.event(EV_ABS, ABS_MT_TRACKING_ID, 40);
        s.event(EV_ABS, ABS_MT_POSITION_X, 500);
        s.event(EV_ABS, ABS_MT_POSITION_Y, 250);
        let f = s.event(EV_SYN, SYN_REPORT, 0).unwrap();
        assert_eq!(ids(&f), [(40, 0.5, 0.5, true)]);
        assert_eq!((f.device, f.width_mm, f.height_mm, f.button), (1, 100, 50, false));
        // A second finger in slot 1 and the click.
        s.event(EV_ABS, ABS_MT_SLOT, 1);
        s.event(EV_ABS, ABS_MT_TRACKING_ID, 41);
        s.event(EV_ABS, ABS_MT_POSITION_X, 1000);
        s.event(EV_KEY, BTN_LEFT, 1);
        let f = s.event(EV_SYN, SYN_REPORT, 0).unwrap();
        assert_eq!(ids(&f), [(40, 0.5, 0.5, true), (41, 1.0, 0.0, true)]);
        assert!(f.button);
        // A report without changes is no frame.
        assert_eq!(s.event(EV_SYN, SYN_REPORT, 0), None);
        // The first finger lifts; the second turns out to be a palm.
        s.event(EV_ABS, ABS_MT_SLOT, 0);
        s.event(EV_ABS, ABS_MT_TRACKING_ID, -1);
        s.event(EV_ABS, ABS_MT_SLOT, 1);
        s.event(EV_ABS, ABS_MT_TOOL_TYPE, MT_TOOL_PALM);
        let f = s.event(EV_SYN, SYN_REPORT, 0).unwrap();
        assert_eq!(ids(&f), [(41, 1.0, 0.0, false)]);
    }

    #[test]
    fn dropped_events_wait_for_the_report_and_the_slots_read_again() {
        let mut s = state();
        s.event(EV_ABS, ABS_MT_TRACKING_ID, 7);
        s.event(EV_SYN, SYN_REPORT, 0).unwrap();
        s.event(EV_SYN, SYN_DROPPED, 0);
        assert_eq!(s.event(EV_ABS, ABS_MT_TRACKING_ID, -1), None);
        assert_eq!(s.event(EV_SYN, SYN_REPORT, 0), None);
        assert!(!s.dropped);
        s.set_slots(&[-1, 9], &[0, 250], &[0, 125]);
        let f = s.event(EV_SYN, SYN_REPORT, 0).unwrap();
        assert_eq!(ids(&f), [(9, 0.25, 0.25, true)]);
        // A slot past the device's is ignored.
        s.event(EV_ABS, ABS_MT_SLOT, 99);
        assert_eq!(s.event(EV_ABS, ABS_MT_POSITION_X, 5), None);
    }

    #[test]
    fn resync_updates_active_slot_and_clamps_out_of_range_values() {
        let mut s = state();
        // Finger 7 starts in slot 0.
        s.event(EV_ABS, ABS_MT_SLOT, 0);
        s.event(EV_ABS, ABS_MT_TRACKING_ID, 7);
        s.event(EV_ABS, ABS_MT_POSITION_X, 100);
        s.event(EV_ABS, ABS_MT_POSITION_Y, 100);
        s.event(EV_SYN, SYN_REPORT, 0).unwrap();

        // Kernel drops events (including a switch to slot 1) and recovers at SYN_REPORT.
        s.event(EV_SYN, SYN_DROPPED, 0);
        assert_eq!(s.event(EV_SYN, SYN_REPORT, 0), None);
        assert!(!s.dropped);

        // Resync queries ABS_MT_SLOT (now 1) and the slot arrays.
        s.set_slot(1);
        s.set_slots(&[7, 9], &[100, 250], &[100, 125]);
        // Subsequent coordinate updates without an ABS_MT_SLOT prefix must target slot 1.
        s.event(EV_ABS, ABS_MT_POSITION_X, 500);
        let f = s.event(EV_SYN, SYN_REPORT, 0).unwrap();
        assert_eq!(ids(&f), [(7, 0.1, 0.2, true), (9, 0.5, 0.25, true)]);

        // Out-of-range kernel slot values are clamped to 0..slots.len().
        s.set_slot(-3);
        assert_eq!(s.slot, 0);
        s.set_slot(99);
        assert_eq!(s.slot, 4);

        // EVIOCGKEY resync releases BTN_LEFT if its release event was dropped.
        assert_eq!(s.event(EV_KEY, BTN_LEFT, 1), None);
        let f = s.event(EV_SYN, SYN_REPORT, 0).unwrap();
        assert!(f.button);
        s.event(EV_SYN, SYN_DROPPED, 0);
        assert_eq!(s.event(EV_SYN, SYN_REPORT, 0), None);
        s.set_button(false);
        let f = s.event(EV_SYN, SYN_REPORT, 0).unwrap();
        assert!(!f.button, "resync clears BTN_LEFT when EVIOCGKEY reports it released");
    }

    #[test]
    fn only_an_indirect_multi_touch_pointer_is_a_touchpad() {
        let mut abs = vec![0u8; ABS_CNT / 8];
        for code in [ABS_MT_SLOT, ABS_MT_POSITION_X, ABS_MT_POSITION_Y] {
            abs[usize::from(code) / 8] |= 1 << (code % 8);
        }
        assert!(is_touchpad(&abs, &[1 << INPUT_PROP_POINTER]));
        assert!(!is_touchpad(&abs, &[(1 << INPUT_PROP_POINTER) | (1 << INPUT_PROP_DIRECT)]), "a touchscreen");
        assert!(!is_touchpad(&abs, &[0]));
        assert!(!is_touchpad(&[0; ABS_CNT / 8], &[1 << INPUT_PROP_POINTER]));
        // EVIOCGABS(ABS_MT_POSITION_X) as the kernel headers spell it.
        assert_eq!(ioc_read(0x40 + 0x35, 24) as u32, 0x8018_4575);
    }
}
