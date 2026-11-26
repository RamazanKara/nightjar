//! Golden tests: packet sequences go through `nightjar-core`, and a digest of everything it
//! outputs (frames with their buffers, control-stream signals, decode queue flushes, audio
//! samples, statistics) must equal the one in `golden.txt` (originally recorded from Nightjar's
//! patched moonlight-common-c, and updated where `nightjar-core` intentionally fixes classic
//! engine bugs: draining contiguous reordered audio packets, requiring an incompatible-packet
//! streak before disabling audio FEC, omitting stale FEC status on multi-block block-0 loss,
//! and using 32-bit frame order before 16-bit sequence checks).
//! The fixture is keyed by a digest of each run's inputs.
//!
//! The test hosts follow the Rubylight host's packetizers (`rust/core/src/packet.rs`).
#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::OnceLock;

pub mod host;
pub mod net;
pub mod rust;

/// FNV-1a, 64 bit.
pub struct Fnv(u64);

impl Fnv {
    pub fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }

    pub fn bytes(&mut self, data: &[u8]) {
        for &b in data {
            self.0 = (self.0 ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3);
        }
    }

    pub fn u64(&mut self, value: u64) {
        self.bytes(&value.to_le_bytes());
    }

    pub fn finish(&self) -> u64 {
        self.0
    }
}

fn recorded() -> &'static HashMap<(String, u64), u64> {
    static RECORDED: OnceLock<HashMap<(String, u64), u64>> = OnceLock::new();
    RECORDED.get_or_init(|| {
        include_str!("golden.txt")
            .lines()
            .map(|line| {
                let mut parts = line.split(' ');
                let kind = parts.next().unwrap().to_string();
                let key = u64::from_str_radix(parts.next().unwrap(), 16).unwrap();
                let output = u64::from_str_radix(parts.next().unwrap(), 16).unwrap();
                ((kind, key), output)
            })
            .collect()
    })
}

/// Asserts that the run keyed `key` produced what the classic engine recorded for it.
pub fn check(kind: &str, key: u64, output: u64) {
    let expected = recorded()
        .get(&(kind.to_string(), key))
        .unwrap_or_else(|| panic!("no recorded {kind} run for inputs {key:016x}: its test changed"));
    assert_eq!(output, *expected, "{kind} run {key:016x}");
}

/// One input to the receivers.
#[derive(Clone, Debug)]
pub enum Step {
    /// A datagram and its receive time.
    Packet(Vec<u8>, u64),
    /// The decoder completes every queued frame: `true` decoded, `false` needs a key frame.
    Complete(bool),
    /// The host's HDR mode changes.
    Hdr(bool),
    /// The audio socket timed out once.
    Timeout,
}

fn steps_into(key: &mut Fnv, steps: &[Step]) {
    for step in steps {
        match step {
            Step::Packet(data, now) => {
                key.bytes(b"P");
                key.u64(*now);
                key.u64(data.len() as u64);
                key.bytes(data);
            }
            Step::Complete(ok) => key.bytes(&[b'C', u8::from(*ok)]),
            Step::Hdr(on) => key.bytes(&[b'H', u8::from(*on)]),
            Step::Timeout => key.bytes(b"T"),
        }
    }
}

/// Digest of the inputs of a run, which keys its recorded output.
pub fn steps_key(steps: &[Step], queue_bound: usize) -> u64 {
    let mut key = Fnv::new();
    key.u64(queue_bound as u64);
    steps_into(&mut key, steps);
    key.finish()
}

/// As [`steps_key`] for a video run, which the session's parameters change too.
pub fn video_key(config: &nightjar_core::video::VideoConfig, steps: &[Step], queue_bound: usize) -> u64 {
    let mut key = Fnv::new();
    key.u64(queue_bound as u64);
    key.u64(u64::from(config.format));
    key.u64(config.packet_size as u64);
    for part in config.app_version {
        key.u64(part as u64);
    }
    key.bytes(&[
        u8::from(config.rfi),
        u8::from(config.pyrowave_records),
        config.colorspace,
    ]);
    key.bytes(&config.encryption_key.unwrap_or([0; 16]));
    key.bytes(&[u8::from(config.encryption_key.is_some())]);
    steps_into(&mut key, steps);
    key.finish()
}

/// Feeds `steps` to the video receiver, checks the run against the recorded one, and returns
/// the number of frames delivered.
pub fn run_video(
    config: &nightjar_core::video::VideoConfig,
    queue_bound: usize,
    steps: &[Step],
) -> usize {
    let events = run_video_events(config, queue_bound, steps);
    events.iter().filter(|e| e.starts_with("frame ")).count()
}

/// As [`run_video`], returning the whole event log in readable form.
pub fn run_video_events(
    config: &nightjar_core::video::VideoConfig,
    queue_bound: usize,
    steps: &[Step],
) -> Vec<String> {
    let mut r = rust::RustVideo::new(config.clone(), queue_bound);
    let mut all = Vec::new();
    let mut digest = Fnv::new();
    for (index, step) in steps.iter().enumerate() {
        match step {
            Step::Packet(data, now) => r.receive(data, *now),
            Step::Complete(ok) => r.complete_all(*ok),
            Step::Hdr(active) => r.set_hdr(*active),
            Step::Timeout => {}
        }
        let log = r.take_log();
        digest.u64(index as u64);
        digest.bytes(&log);
        digest.u64(u64::from(r.current_frame()));
        all.extend(log);
    }
    digest.u64(u64::from(r.stats().0));
    digest.u64(u64::from(r.stats().1));
    check("video", video_key(config, steps, queue_bound), digest.finish());
    describe_log(&all)
}

/// As [`run_video`] for audio; returns the number of samples played (including concealed).
pub fn run_audio(config: &nightjar_core::audio::AudioConfig, steps: &[Step]) -> usize {
    let mut r = rust::RustAudio::new(config.clone());
    let mut played = 0;
    let mut digest = Fnv::new();
    for (index, step) in steps.iter().enumerate() {
        match step {
            Step::Packet(data, now) => r.receive(data, *now),
            Step::Timeout => r.timeout(),
            _ => {}
        }
        let log = r.take_log();
        digest.u64(index as u64);
        digest.bytes(&log);
        for stat in r.stats() {
            digest.u64(u64::from(stat));
        }
        played += describe_log(&log).len();
    }
    check("audio", steps_key(steps, 0), digest.finish());
    played
}

/// Readable form of an event log, for failure messages.
pub fn describe_log(mut log: &[u8]) -> Vec<String> {
    fn take<'a>(log: &mut &'a [u8], n: usize) -> &'a [u8] {
        let n = n.min(log.len());
        let (head, tail) = log.split_at(n);
        *log = tail;
        head
    }
    fn u32le(log: &mut &[u8]) -> u32 {
        let b = take(log, 4);
        let mut a = [0; 4];
        a[..b.len()].copy_from_slice(b);
        u32::from_le_bytes(a)
    }
    fn u64le(log: &mut &[u8]) -> u64 {
        let b = take(log, 8);
        let mut a = [0; 8];
        a[..b.len()].copy_from_slice(b);
        u64::from_le_bytes(a)
    }
    let mut out = Vec::new();
    while !log.is_empty() {
        let tag = take(&mut log, 1)[0];
        out.push(match tag {
            b'F' => {
                let number = u32le(&mut log);
                let kind = take(&mut log, 1)[0];
                let latency = u16::from_le_bytes(take(&mut log, 2).try_into().unwrap_or([0; 2]));
                let receive = u64le(&mut log);
                let enqueue = u64le(&mut log);
                let pts = u64le(&mut log);
                let rtp = u32le(&mut log);
                let flags = take(&mut log, 2).to_vec();
                let count = u32le(&mut log);
                let mut buffers = Vec::new();
                let mut sum = 0u32;
                for _ in 0..count {
                    let kind = take(&mut log, 1)[0];
                    let len = u32le(&mut log);
                    let bytes = take(&mut log, len as usize);
                    sum = bytes.iter().fold(sum, |s, &b| s.wrapping_mul(31).wrapping_add(b.into()));
                    buffers.push(format!("{kind}:{len}"));
                }
                format!(
                    "frame {number} type {kind} lat {latency} recv {receive} enq {enqueue} pts {pts} \
                     rtp {rtp} hdr/cs {flags:?} buffers [{}] sum {sum:08x}",
                    buffers.join(" ")
                )
            }
            b'I' => "request IDR".into(),
            b'R' => format!("invalidate {}..={}", u32le(&mut log), u32le(&mut log)),
            b'S' => format!("fec status {:02x?}", take(&mut log, 21)),
            b'C' => format!("received {} ltr {}", u32le(&mut log), take(&mut log, 1)[0]),
            b'W' => format!("saw {}", u32le(&mut log)),
            b'B' => format!(
                "fec block {}.{}/{} {}+{} arrived {} missing {} unrecovered {}",
                u32le(&mut log),
                take(&mut log, 1)[0],
                take(&mut log, 1)[0],
                u32le(&mut log),
                u32le(&mut log),
                u32le(&mut log),
                u32le(&mut log),
                take(&mut log, 1)[0]
            ),
            b'X' => "flush".into(),
            b'A' => {
                let len = u32le(&mut log);
                format!("audio {len} {:02x?}", take(&mut log, len as usize))
            }
            other => format!("unknown tag {other}"),
        });
    }
    out
}
