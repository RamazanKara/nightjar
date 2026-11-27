//! 7.1.4 to Windows' default output through WASAPI in shared mode. SDL opens at most 8 channels,
//! so a 12-channel stream plays here and every other stream stays on SDL. It runs as SDL's WASAPI
//! backend does: event driven, one device period written per wake into a buffer of the same size,
//! at the same thread priority, so it adds no latency over SDL. What runs dry between packets is
//! concealed from the playout, as SDL's request callback does.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use rubylight_protocol::audio_ext::{CHANNELS_714, CHANNEL_MASK_714};
use windows::core::{implement, Interface, GUID, PCWSTR};
use windows::Win32::Foundation::{
    CloseHandle, E_UNEXPECTED, HANDLE, PROPERTYKEY, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows::Win32::Media::Audio::{
    eConsole, eRender, EDataFlow, ERole, IAudioClient, IAudioClient3, IAudioRenderClient,
    IMMDeviceEnumerator, IMMNotificationClient, IMMNotificationClient_Impl, MMDeviceEnumerator,
    AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM, AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
    AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY, DEVICE_STATE, WAVEFORMATEX, WAVEFORMATEXTENSIBLE,
    WAVEFORMATEXTENSIBLE_0,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED,
};
use windows::Win32::System::Threading::{
    AvRevertMmThreadCharacteristics, AvSetMmThreadCharacteristicsW, CreateEventW, GetCurrentThread,
    SetEvent, SetThreadPriority, WaitForSingleObject, THREAD_PRIORITY_TIME_CRITICAL,
};

use super::pipeline::lock;
use super::playout::Playout;

/// 7.1.4's channels, in the stream's order FL FR FC LFE BL BR SL SR TFL TFR TBL TBR. Windows
/// interleaves by mask bit, so `CHANNEL_MASK_714` puts them where they belong as they are.
pub const CHANNELS: usize = CHANNELS_714 as usize;
const RATE: u32 = 48_000;
const WAVE_FORMAT_EXTENSIBLE: u16 = 0xFFFE;
const SUBTYPE_IEEE_FLOAT: GUID = GUID::from_u128(0x00000003_0000_0010_8000_00aa00389b71);
/// How long the offer's probe of the output is trusted.
const OFFER_FOR: Duration = Duration::from_secs(2);
const LOW_PERIOD_ENV: &str = "NIGHTJAR_WASAPI_LOW_PERIOD";

fn low_period_enabled_from(value: Option<&str>) -> bool {
    value.is_none_or(|v| v.trim() != "0")
}

fn low_period_enabled() -> bool {
    low_period_enabled_from(std::env::var(LOW_PERIOD_ENV).ok().as_deref())
}

fn select_low_period(min_period: u32, default_period: u32) -> Option<u32> {
    (min_period > 0 && min_period < default_period).then_some(min_period)
}

/// Returns true when `mix` matches our 48 kHz 12-channel 7.1.4 float format directly, so
/// `IAudioClient3::InitializeSharedAudioStream` can open without `AUTOCONVERTPCM`.
///
/// # Safety
/// `mix` must be null or point to a readable `WAVEFORMATEX` (and full `WAVEFORMATEXTENSIBLE`
/// when `wFormatTag == WAVE_FORMAT_EXTENSIBLE` and `cbSize >= 22`).
unsafe fn mix_matches_714(mix: *const WAVEFORMATEX) -> bool {
    if mix.is_null() {
        return false;
    }
    // SAFETY: caller guarantees `mix` points to a readable WAVEFORMATEX.
    let header = unsafe { std::ptr::read_unaligned(mix) };
    if header.wFormatTag != WAVE_FORMAT_EXTENSIBLE
        || header.cbSize < 22
        || header.nChannels != CHANNELS as u16
        || header.nSamplesPerSec != RATE
        || header.wBitsPerSample != 32
        || header.nBlockAlign != (CHANNELS * 4) as u16
    {
        return false;
    }
    // SAFETY: `wFormatTag == WAVE_FORMAT_EXTENSIBLE` and `cbSize >= 22` verified above.
    let ext = unsafe { std::ptr::read_unaligned(mix.cast::<WAVEFORMATEXTENSIBLE>()) };
    // SAFETY: `Samples` union is read as `wValidBitsPerSample` on an extensible PCM/float format.
    let valid_bits = unsafe { ext.Samples.wValidBitsPerSample };
    let mask = ext.dwChannelMask;
    let subformat = ext.SubFormat;
    valid_bits == 32 && mask == CHANNEL_MASK_714 && subformat == SUBTYPE_IEEE_FLOAT
}

/// The stream's format: 12 float channels at 48 kHz in 7.1.4's mask. Windows converts it to the
/// output's own format (`AUTOCONVERTPCM`), as it converts SDL's rate.
pub(super) fn format() -> WAVEFORMATEXTENSIBLE {
    let block = (CHANNELS * 4) as u16;
    WAVEFORMATEXTENSIBLE {
        Format: WAVEFORMATEX {
            wFormatTag: WAVE_FORMAT_EXTENSIBLE,
            nChannels: CHANNELS as u16,
            nSamplesPerSec: RATE,
            nAvgBytesPerSec: RATE * u32::from(block),
            nBlockAlign: block,
            wBitsPerSample: 32,
            cbSize: (size_of::<WAVEFORMATEXTENSIBLE>() - size_of::<WAVEFORMATEX>()) as u16,
        },
        Samples: WAVEFORMATEXTENSIBLE_0 { wValidBitsPerSample: 32 },
        dwChannelMask: CHANNEL_MASK_714,
        SubFormat: SUBTYPE_IEEE_FLOAT,
    }
}

/// COM on this thread for as long as it lives.
struct Com(bool);

impl Com {
    fn init() -> Self {
        // SAFETY: paired with CoUninitialize in drop when it succeeded.
        Self(unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok())
    }
}

impl Drop for Com {
    fn drop(&mut self) {
        if self.0 {
            // SAFETY: initialised in init on this thread.
            unsafe { CoUninitialize() };
        }
    }
}

struct WakeHandle(Option<HANDLE>);

// SAFETY: Only mutated by the render thread (cleared before closing the event); `SetEvent` is thread-safe.
unsafe impl Send for WakeHandle {}
unsafe impl Sync for WakeHandle {}

#[implement(IMMNotificationClient)]
struct DefaultDeviceWatch {
    reopen: Arc<AtomicBool>,
    wake: Arc<Mutex<WakeHandle>>,
}

impl IMMNotificationClient_Impl for DefaultDeviceWatch_Impl {
    fn OnDeviceStateChanged(&self, _: &PCWSTR, _: DEVICE_STATE) -> windows::core::Result<()> {
        Ok(())
    }
    fn OnDeviceAdded(&self, _: &PCWSTR) -> windows::core::Result<()> {
        Ok(())
    }
    fn OnDeviceRemoved(&self, _: &PCWSTR) -> windows::core::Result<()> {
        Ok(())
    }
    fn OnDefaultDeviceChanged(&self, flow: EDataFlow, role: ERole, _: &PCWSTR) -> windows::core::Result<()> {
        if flow == eRender && role == eConsole {
            self.reopen.store(true, Ordering::Release);
            if let Some(handle) = lock(&self.wake).0 {
                // SAFETY: `Device::render` clears `wake` before dropping its `Event`.
                let _ = unsafe { SetEvent(handle) };
            }
        }
        Ok(())
    }
    fn OnPropertyValueChanged(&self, _: &PCWSTR, _: &PROPERTYKEY) -> windows::core::Result<()> {
        Ok(())
    }
}

struct NotificationRegistration {
    enumerator: IMMDeviceEnumerator,
    client: IMMNotificationClient,
}

impl NotificationRegistration {
    fn install(reopen: Arc<AtomicBool>, wake: Arc<Mutex<WakeHandle>>) -> Option<Self> {
        let client: IMMNotificationClient = DefaultDeviceWatch { reopen, wake }.into();
        // SAFETY: COM is initialized on the render thread; paired with unregister on drop.
        unsafe {
            let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
            enumerator.RegisterEndpointNotificationCallback(&client).ok()?;
            Some(Self { enumerator, client })
        }
    }
}

impl Drop for NotificationRegistration {
    fn drop(&mut self) {
        // SAFETY: registered above on the same enumerator.
        let _ = unsafe { self.enumerator.UnregisterEndpointNotificationCallback(&self.client) };
    }
}

fn default_client() -> windows::core::Result<IAudioClient> {
    // SAFETY: COM is initialised on the calling thread.
    unsafe {
        let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        enumerator.GetDefaultAudioEndpoint(eRender, eConsole)?.Activate(CLSCTX_ALL, None)
    }
}

/// The default output's mix format has 12 or more channels. Run on a thread of its own so the
/// caller's COM apartment is never touched.
pub fn plays_714() -> bool {
    let Ok(thread) = std::thread::Builder::new().name("wasapi-714".into()).spawn(|| {
        let _com = Com::init();
        let Ok(client) = default_client() else { return 0 };
        // SAFETY: the mix format is read and freed once.
        unsafe {
            let Ok(mix) = client.GetMixFormat() else { return 0 };
            let channels = (*mix).nChannels;
            CoTaskMemFree(Some(mix.cast_const().cast()));
            channels
        }
    }) else {
        return false;
    };
    thread
        .join()
        .is_ok_and(|channels| usize::from(channels) >= CHANNELS)
}

struct Offer {
    plays: bool,
    checked: Option<Instant>,
    probing: bool,
}

static OFFER: Mutex<Offer> = Mutex::new(Offer { plays: false, checked: None, probing: false });

/// [`plays_714`] for the audio setting, which asks every frame it is shown: the last answer,
/// probed again in the background once it is older than `OFFER_FOR`.
pub fn offered() -> bool {
    let mut offer = lock(&OFFER);
    if !offer.probing && offer.checked.is_none_or(|at| at.elapsed() >= OFFER_FOR) {
        offer.probing = true;
        if let Err(error) = std::thread::Builder::new().name("wasapi-offer".into()).spawn(|| {
            let plays = plays_714();
            let mut offer = lock(&OFFER);
            *offer = Offer { plays, checked: Some(Instant::now()), probing: false };
        }) {
            offer.probing = false;
            offer.checked = Some(Instant::now());
            eprintln!("nightjar: could not spawn 7.1.4 audio offer check: {error}");
        }
    }
    offer.plays
}

/// What the render thread plays, under the lock `play` holds while it feeds it.
#[derive(Default)]
pub struct Queue {
    /// Interleaved 12-channel samples waiting for the device.
    pub pcm: VecDeque<f32>,
    /// What runs dry is concealed from (`Playout::starve`); none before playback starts.
    pub playout: Option<Arc<Mutex<Playout>>>,
}

/// Fills `out` (interleaved, `CHANNELS` per frame) from the queue, concealing what it lacks
/// first, and silence after that.
pub(super) fn fill(queue: &mut Queue, out: &mut [f32]) {
    let needed = out.len() / CHANNELS;
    let queued = queue.pcm.len() / CHANNELS;
    if queued < needed
        && let Some(playout) = queue.playout.clone() {
            queue.pcm.extend(lock(&playout).starve(needed - queued));
        }
    let n = out.len().min(queue.pcm.len());
    if n > 0 {
        let (front, back) = queue.pcm.as_slices();
        let first = n.min(front.len());
        out[..first].copy_from_slice(&front[..first]);
        let rest = n - first;
        if rest > 0 {
            out[first..n].copy_from_slice(&back[..rest]);
        }
        queue.pcm.drain(..n);
    }
    out[n..].fill(0.0);
}

struct Event(HANDLE);

impl Drop for Event {
    fn drop(&mut self) {
        // SAFETY: created in Device::open and closed once.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

/// Frames written per wake from WASAPI's default period (100 ns units) and buffer size.
/// Returns `None` when the device reports a zero-frame buffer.
fn compute_period(default_period_100ns: i64, buffer: u32) -> Option<u32> {
    if buffer == 0 {
        return None;
    }
    let frames = (default_period_100ns.max(0) as f64 / 10_000.0 * f64::from(RATE) / 1000.0).ceil();
    let frames = if frames.is_finite() && frames > 0.0 {
        frames.min(f64::from(buffer)) as u32
    } else {
        1
    };
    Some(frames.clamp(1, buffer))
}

/// Frames to write on a render wake: zero when more than a period is already queued or no buffer
/// space remains, avoiding `GetBuffer(0)` (which may return `S_OK` with a null pointer).
fn render_frames(period: u32, buffer: u32, padding: u32) -> u32 {
    if padding > period {
        return 0;
    }
    period.min(buffer.saturating_sub(padding))
}

struct Device {
    // Field drop order releases COM clients before closing the event handle set on `client`.
    render: IAudioRenderClient,
    client: IAudioClient,
    event: Event,
    /// Frames written per wake: the device period, as SDL's WASAPI backend writes.
    period: u32,
    buffer: u32,
}

impl Device {
    fn open() -> windows::core::Result<Self> {
        // Declared before `client` so any `?` after `SetEventHandle` drops `client` first.
        // SAFETY: creating a plain unnamed auto-reset event owned by `Event`.
        let event = Event(unsafe { CreateEventW(None, false, false, PCWSTR::null()) }?);
        let mut client = default_client()?;
        let format = format();
        let format_ptr = (&raw const format).cast::<WAVEFORMATEX>();
        // SAFETY: COM is initialised on this thread; `format` outlives the calls and `GetMixFormat`
        // allocations are freed once.
        unsafe {
            let mut low_period = None;
            if low_period_enabled() {
                let matches_714 = client.GetMixFormat().ok().is_some_and(|mix| {
                    let ok = mix_matches_714(mix);
                    CoTaskMemFree(Some(mix.cast_const().cast()));
                    ok
                });
                if matches_714 && let Ok(client3) = client.cast::<IAudioClient3>() {
                    let (
                        mut default_period,
                        mut fundamental_period,
                        mut min_period,
                        mut max_period,
                    ) = (0, 0, 0, 0);
                    if client3
                        .GetSharedModeEnginePeriod(
                            format_ptr,
                            &mut default_period,
                            &mut fundamental_period,
                            &mut min_period,
                            &mut max_period,
                        )
                        .is_ok()
                        && let Some(selected) = select_low_period(min_period, default_period)
                    {
                        let init = client3
                            .InitializeSharedAudioStream(
                                AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
                                selected,
                                format_ptr,
                                None,
                            )
                            .and_then(|()| client.SetEventHandle(event.0));
                        if init.is_ok() {
                            low_period = Some(selected);
                        } else {
                            // COM IAudioClient cannot be Initialized twice after a failed Initialize*.
                            drop(client3);
                            client = default_client()?;
                        }
                    }
                }
            }
            let mut default_period = 0i64;
            if low_period.is_none() {
                client.GetDevicePeriod(Some(&raw mut default_period), None)?;
                client.Initialize(
                    AUDCLNT_SHAREMODE_SHARED,
                    AUDCLNT_STREAMFLAGS_EVENTCALLBACK
                        | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM
                        | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
                    0,
                    0,
                    format_ptr,
                    None,
                )?;
                client.SetEventHandle(event.0)?;
            }
            let buffer = client.GetBufferSize()?;
            let period = match low_period {
                Some(frames) => (buffer > 0).then(|| frames.clamp(1, buffer)),
                None => compute_period(default_period, buffer),
            }
            .ok_or_else(|| windows::core::Error::new(E_UNEXPECTED, "WASAPI audio buffer size is zero"))?;
            let render = client.GetService::<IAudioRenderClient>()?;
            Ok(Self { render, client, event, period, buffer })
        }
    }

    /// Plays until `stop`, `reopen` (default audio endpoint changed), or device failure.
    fn render(
        &self,
        queue: &Mutex<Queue>,
        stop: &AtomicBool,
        reopen: &AtomicBool,
        wake: &Mutex<WakeHandle>,
    ) -> windows::core::Result<bool> {
        // SAFETY: the client and render client are this device's; the buffer WASAPI hands out
        // holds `frames` frames of the stream's format until it is released.
        unsafe {
            let _wake_guard = register_wake(wake, self.event.0);
            self.client.Start()?;
            let mut switch = false;
            let played = (|| {
                while !stop.load(Ordering::Relaxed) {
                    if reopen.swap(false, Ordering::AcqRel) {
                        switch = true;
                        break;
                    }
                    let waited = WaitForSingleObject(self.event.0, 200);
                    if reopen.swap(false, Ordering::AcqRel) {
                        switch = true;
                        break;
                    }
                    if waited == WAIT_TIMEOUT {
                        continue;
                    }
                    if waited != WAIT_OBJECT_0 {
                        return Err(windows::core::Error::from_thread());
                    }
                    let padding = self.client.GetCurrentPadding()?;
                    let frames = render_frames(self.period, self.buffer, padding);
                    if frames == 0 {
                        continue;
                    }
                    let data = self.render.GetBuffer(frames)?;
                    if data.is_null() {
                        return Err(windows::core::Error::new(E_UNEXPECTED, "WASAPI GetBuffer returned null"));
                    }
                    let out = std::slice::from_raw_parts_mut(data.cast::<f32>(), frames as usize * CHANNELS);
                    fill(&mut lock(queue), out);
                    self.render.ReleaseBuffer(frames, 0)?;
                }
                Ok(switch)
            })();
            let _ = self.client.Stop();
            played
        }
    }
}

/// Registers `handle` as the active wake event for `DefaultDeviceWatch` and guarantees clearing it
/// on return (including early `?` failures in `IAudioClient::Start` before `Event::drop`).
struct WakeGuard<'a>(&'a Mutex<WakeHandle>);

impl Drop for WakeGuard<'_> {
    fn drop(&mut self) {
        lock(self.0).0 = None;
    }
}

fn register_wake(wake: &Mutex<WakeHandle>, handle: HANDLE) -> WakeGuard<'_> {
    lock(wake).0 = Some(handle);
    WakeGuard(wake)
}

/// The render thread and what it plays.
pub struct Output {
    pub queue: Arc<Mutex<Queue>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    /// Frames the active device takes at once, at 48 kHz (updated on device reopen).
    period_frames: Arc<AtomicUsize>,
}

impl Output {
    /// Opens the default output and starts playing silence until the queue has some.
    pub fn open() -> Result<Self, String> {
        let queue = Arc::new(Mutex::new(Queue::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let period_frames = Arc::new(AtomicUsize::new(0));
        let (opened, first) = mpsc::channel();
        let thread = {
            let (queue, stop, period_frames) = (queue.clone(), stop.clone(), period_frames.clone());
            std::thread::Builder::new()
                .name("nightjar-wasapi".into())
                .spawn(move || run(&queue, &stop, &period_frames, opened))
                .map_err(|e| e.to_string())?
        };
        match first.recv() {
            Ok(Ok(_period)) => Ok(Self { queue, stop, thread: Some(thread), period_frames }),
            Ok(Err(e)) => {
                let _ = thread.join();
                Err(e)
            }
            Err(_) => Err("the output thread ended".into()),
        }
    }

    pub fn period_frames(&self) -> usize {
        self.period_frames.load(Ordering::Acquire)
    }
}

impl Drop for Output {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// The render thread: opens the default output, and opens it again immediately when the default
/// endpoint changes or mid-stream errors occur, updating `period_frames` and the playout cushion.
fn run(
    queue: &Mutex<Queue>,
    stop: &AtomicBool,
    period_frames: &AtomicUsize,
    opened: mpsc::Sender<Result<u32, String>>,
) {
    let _com = Com::init();
    let reopen = Arc::new(AtomicBool::new(false));
    let wake = Arc::new(Mutex::new(WakeHandle(None)));
    let _watch = NotificationRegistration::install(reopen.clone(), wake.clone());
    // As SDL's audio threads: the Pro Audio task, or time-critical priority without it.
    let mut task_index = 0;
    // SAFETY: the task name is a static string; the handle is reverted below on this thread.
    let task = unsafe { AvSetMmThreadCharacteristicsW(windows::core::w!("Pro Audio"), &mut task_index) };
    if task.is_err() {
        // SAFETY: this thread's own pseudo handle.
        let _ = unsafe { SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_TIME_CRITICAL) };
    }
    let mut opened = Some(opened);
    let mut failing = false;
    while !stop.load(Ordering::Relaxed) {
        let mut switch_now = false;
        reopen.store(false, Ordering::Release);
        match Device::open() {
            Ok(device) => {
                period_frames.store(device.period as usize, Ordering::Release);
                if let Some(playout) = lock(queue).playout.clone() {
                    lock(&playout).set_device_frames(device.period as usize);
                }
                match opened.take() {
                    Some(tx) => {
                        let _ = tx.send(Ok(device.period));
                    }
                    None => eprintln!("nightjar: audio: 7.1.4 output open again ({} frames a period)", device.period),
                }
                failing = false;
                match device.render(queue, stop, &reopen, &wake) {
                    Ok(true) => {
                        eprintln!("nightjar: audio: default 7.1.4 output changed; switching");
                        switch_now = true;
                    }
                    Ok(false) => {}
                    Err(e) => {
                        eprintln!("nightjar: audio: the 7.1.4 output failed ({e}); opening the default output again");
                    }
                }
            }
            Err(e) => {
                if let Some(tx) = opened.take() {
                    let _ = tx.send(Err(e.to_string()));
                    break;
                }
                if !failing {
                    eprintln!("nightjar: audio: no 7.1.4 output ({e}); trying again");
                    failing = true;
                }
            }
        }
        if switch_now {
            continue;
        }
        // Not straight back at a device that just failed.
        for _ in 0..5 {
            if stop.load(Ordering::Relaxed) || reopen.load(Ordering::Acquire) {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    if let Ok(task) = task {
        // SAFETY: set above on this thread.
        let _ = unsafe { AvRevertMmThreadCharacteristics(task) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_format_is_twelve_float_channels_in_7_1_4_order() {
        let format = format();
        let header = format.Format;
        assert_eq!({ header.wFormatTag }, WAVE_FORMAT_EXTENSIBLE);
        assert_eq!(usize::from(header.nChannels), CHANNELS);
        assert_eq!({ header.nBlockAlign }, 48);
        assert_eq!({ header.nAvgBytesPerSec }, 48_000 * 48);
        assert_eq!({ header.cbSize }, 22);
        assert_eq!({ format.SubFormat }, SUBTYPE_IEEE_FLOAT);
        // Windows interleaves by mask bit: FL FR FC LFE BL BR SL SR TFL TFR TBL TBR, the
        // stream's order (KSAUDIO_SPEAKER_7POINT1POINT4).
        let order = [0x1, 0x2, 0x4, 0x8, 0x10, 0x20, 0x200, 0x400, 0x1000, 0x4000, 0x8000, 0x20000];
        let mask = { format.dwChannelMask };
        let bits: Vec<u32> = (0..32).map(|bit| 1u32 << bit).filter(|bit| mask & bit != 0).collect();
        assert_eq!(bits, order);
    }

    #[test]
    fn fill_keeps_the_interleaving_and_pads_with_silence() {
        let mut queue = Queue::default();
        // Two frames, each channel numbered by its place.
        queue.pcm.extend((0..2 * CHANNELS).map(|i| (i % CHANNELS) as f32 + 1.0));
        let mut out = vec![-1.0; 3 * CHANNELS];
        fill(&mut queue, &mut out);
        for (i, sample) in out[..2 * CHANNELS].iter().enumerate() {
            assert_eq!(*sample, (i % CHANNELS) as f32 + 1.0, "sample {i}");
        }
        assert!(out[2 * CHANNELS..].iter().all(|s| *s == 0.0), "nothing to conceal from: silence");
        assert!(queue.pcm.is_empty());
        // What the device does not take yet stays queued, in order.
        queue.pcm.extend((0..3 * CHANNELS).map(|i| i as f32));
        let mut one = vec![0.0; CHANNELS];
        fill(&mut queue, &mut one);
        assert_eq!(one, (0..CHANNELS).map(|i| i as f32).collect::<Vec<_>>());
        assert_eq!(queue.pcm.front(), Some(&(CHANNELS as f32)));
        // Wrapped VecDeque (both front and back slices non-empty) copies in exact order.
        let mut wrapped = Queue { pcm: VecDeque::with_capacity(4 * CHANNELS), playout: None };
        wrapped.pcm.extend((0..3 * CHANNELS).map(|i| i as f32));
        wrapped.pcm.drain(..2 * CHANNELS);
        wrapped.pcm.extend((100..100 + 3 * CHANNELS).map(|i| i as f32));
        let mut buf = vec![-1.0; 4 * CHANNELS];
        fill(&mut wrapped, &mut buf);
        let expected: Vec<f32> = (2 * CHANNELS..3 * CHANNELS).chain(100..100 + 3 * CHANNELS).map(|i| i as f32).collect();
        assert_eq!(buf, expected);
        assert!(wrapped.pcm.is_empty());
    }

    #[test]
    fn a_short_queue_is_concealed_from_the_playout_first() {
        use super::super::pipeline::AudioDecoder;
        let playout = Arc::new(Mutex::new(Playout::new(AudioDecoder::Pcm { channels: CHANNELS }, 5, 480)));
        let packet = vec![0x40u8; 2 * CHANNELS * 240];
        assert!(matches!(lock(&playout).packet(Some(&packet), 0), super::super::playout::Fed::Play { .. }));
        let mut queue = Queue { pcm: VecDeque::new(), playout: Some(playout.clone()) };
        let mut out = vec![1.0; 480 * CHANNELS];
        fill(&mut queue, &mut out);
        // PCM conceals with silence, but as whole packets the playout counts.
        assert_eq!(lock(&playout).take_concealed(), 2);
        assert!(out.iter().all(|s| *s == 0.0));
    }

    #[test]
    fn compute_period_and_render_frames_reject_zero_buffer_and_prevent_zero_frame_writes() {
        // Zero buffer is rejected instead of panicking in clamp(1, 0).
        assert_eq!(compute_period(100_000, 0), None);
        assert_eq!(compute_period(0, 0), None);
        // Normal periods (10 ms = 100_000 * 100ns => 480 frames at 48 kHz) and edge clamping.
        assert_eq!(compute_period(100_000, 960), Some(480));
        assert_eq!(compute_period(101_587, 960), Some(488));
        assert_eq!(compute_period(100_000, 240), Some(240), "clamped to buffer size");
        assert_eq!(compute_period(0, 960), Some(1), "non-positive period clamps to 1");
        assert_eq!(compute_period(-10_000, 960), Some(1));
        assert_eq!(compute_period(i64::MAX, 960), Some(960));
        // Render frame count guards against GetBuffer(0) and padding > buffer underflow.
        assert_eq!(render_frames(480, 960, 0), 480);
        assert_eq!(render_frames(480, 960, 480), 480);
        assert_eq!(render_frames(480, 600, 300), 300);
        assert_eq!(render_frames(480, 960, 481), 0, "padding above period waits");
        assert_eq!(render_frames(480, 480, 480), 0, "padding == buffer == period never calls GetBuffer(0)");
        assert_eq!(render_frames(480, 480, 500), 0, "padding > buffer never underflows");
        assert_eq!(render_frames(500, 480, 490), 0, "padding > buffer even when padding <= period");
    }

    #[test]
    fn wake_registration_clears_handle_even_on_early_error() {
        fn run_with_start_error(wake: &Mutex<WakeHandle>, dummy: HANDLE) -> windows::core::Result<()> {
            let _guard = register_wake(wake, dummy);
            assert!(lock(wake).0.is_some());
            Err(windows::core::Error::new(E_UNEXPECTED, "simulated Start failure"))?;
            Ok(())
        }
        let wake = Mutex::new(WakeHandle(None));
        let dummy = HANDLE(std::ptr::dangling_mut());
        assert!(run_with_start_error(&wake, dummy).is_err());
        assert!(lock(&wake).0.is_none(), "wake handle must not outlive early error return");
    }

    #[test]
    fn low_period_compatibility_and_period_selection() {
        let exact = format();
        assert!(unsafe { mix_matches_714((&raw const exact).cast()) });
        assert!(unsafe { !mix_matches_714(std::ptr::null()) });

        let mut wrong_channels = exact;
        wrong_channels.Format.nChannels = 8;
        assert!(unsafe { !mix_matches_714((&raw const wrong_channels).cast()) });

        let mut wrong_rate = exact;
        wrong_rate.Format.nSamplesPerSec = 44_100;
        assert!(unsafe { !mix_matches_714((&raw const wrong_rate).cast()) });

        let mut wrong_mask = exact;
        wrong_mask.dwChannelMask = 0x3F;
        assert!(unsafe { !mix_matches_714((&raw const wrong_mask).cast()) });

        let mut wrong_subtype = exact;
        wrong_subtype.SubFormat = GUID::from_u128(0x00000001_0000_0010_8000_00aa00389b71);
        assert!(unsafe { !mix_matches_714((&raw const wrong_subtype).cast()) });

        let mut wrong_bits = exact;
        wrong_bits.Samples.wValidBitsPerSample = 24;
        assert!(unsafe { !mix_matches_714((&raw const wrong_bits).cast()) });

        let mut plain_tag = exact;
        plain_tag.Format.wFormatTag = 3;
        plain_tag.Format.cbSize = 0;
        assert!(unsafe { !mix_matches_714((&raw const plain_tag).cast()) });

        assert_eq!(select_low_period(0, 480), None);
        assert_eq!(select_low_period(480, 480), None);
        assert_eq!(select_low_period(960, 480), None);
        assert_eq!(select_low_period(96, 480), Some(96));
        assert_eq!(select_low_period(128, 480), Some(128));

        assert!(low_period_enabled_from(None));
        assert!(low_period_enabled_from(Some("1")));
        assert!(low_period_enabled_from(Some("")));
        assert!(!low_period_enabled_from(Some("0")));
        assert!(!low_period_enabled_from(Some(" 0 ")));
    }

    /// Opens this PC's default output with the 7.1.4 format and plays silence for a moment.
    #[test]
    #[ignore = "needs an audio output"]
    fn the_default_output_takes_the_7_1_4_format() {
        eprintln!("plays 7.1.4: {}", plays_714());
        let output = Output::open().expect("the default output opens");
        assert!(output.period_frames() > 0 && output.period_frames() <= 4800, "{}", output.period_frames());
        std::thread::sleep(Duration::from_millis(300));
        assert!(output.thread.as_ref().is_some_and(|t| !t.is_finished()), "still rendering");
        drop(output);
    }
}
