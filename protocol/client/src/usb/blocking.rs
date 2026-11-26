//! Runs a device whose USB stack only has blocking calls (Android's `UsbDeviceConnection`,
//! libusb's synchronous API): one thread per endpoint, transfers in the order the driver
//! submitted them, as the endpoint itself takes them.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};

use rubylight_protocol::usbip as wire;

use super::server::{Completion, Direction, Reply, Transfer, UsbDevice};

/// Maximum concurrent in-flight isochronous transfers per endpoint lane.
const MAX_ISO_IN_FLIGHT: usize = 8;
/// Maximum shared worker threads servicing concurrent isochronous transfers.
const MAX_ISO_WORKERS: usize = 16;

/// A device that runs one transfer at a time per endpoint, blocking until it ends.
pub trait BlockingDevice: Send + Sync + 'static {
    /// See [`UsbDevice::describe`].
    fn describe(&self) -> wire::Device;
    /// See [`UsbDevice::class`].
    fn class(&self) -> u8 {
        self.describe().class
    }
    /// Runs `transfer` to its end. A transfer that waits for the device (an interrupt IN with
    /// nothing to report) waits in short slices, a second or less, and gives up with
    /// [`wire::ECONNRESET`] once `cancelled` is set.
    fn transfer(&self, transfer: &Transfer, cancelled: &AtomicBool) -> Reply;
    /// The attachment ended; no transfer runs any more.
    fn close(&self) {}
}

struct Job {
    transfer: Transfer,
    done: Completion,
    cancelled: Arc<AtomicBool>,
}

#[derive(Default)]
struct IsoLane {
    next_ticket: u64,
    next_complete: u64,
    in_flight: usize,
    ready: HashMap<u64, (Completion, Reply)>,
    completing: bool,
}

#[derive(Default)]
struct Lanes {
    queues: HashMap<(u8, Direction), VecDeque<Job>>,
    iso: HashMap<(u8, Direction), IsoLane>,
    iso_queue: VecDeque<((u8, Direction), u64, Job)>,
    iso_idle: usize,
    iso_threads: Vec<std::thread::JoinHandle<()>>,
    /// The cancel flag of each transfer a lane runs now.
    running: HashMap<u32, Arc<AtomicBool>>,
    threads: Vec<std::thread::JoinHandle<()>>,
    closed: bool,
}

struct Inner<D> {
    device: D,
    lanes: Mutex<Lanes>,
    wake: Condvar,
    iso_wake: Condvar,
    /// A lane finished a transfer; an unlink waits on it.
    stopped: Condvar,
}

impl<D> Inner<D> {
    fn lanes(&self) -> MutexGuard<'_, Lanes> {
        self.lanes.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// A [`UsbDevice`] made of a [`BlockingDevice`].
pub struct Blocking<D: BlockingDevice> {
    inner: Arc<Inner<D>>,
}

impl<D: BlockingDevice> Blocking<D> {
    pub fn new(device: D) -> Self {
        Self {
            inner: Arc::new(Inner {
                device,
                lanes: Mutex::new(Lanes::default()),
                wake: Condvar::new(),
                iso_wake: Condvar::new(),
                stopped: Condvar::new(),
            }),
        }
    }

    pub fn device(&self) -> &D {
        &self.inner.device
    }
}

/// The control endpoint is one lane both ways; every other endpoint is one per direction.
fn lane_of(transfer: &Transfer) -> (u8, Direction) {
    if transfer.is_control() {
        (0, Direction::Out)
    } else {
        (transfer.endpoint, transfer.direction)
    }
}

/// Cleans up `running` if `device.transfer` unwinds so later `unlink` calls cannot hang.
struct SerialGuard<'a, D: BlockingDevice> {
    inner: &'a Inner<D>,
    seqnum: u32,
    armed: bool,
}

impl<D: BlockingDevice> SerialGuard<'_, D> {
    fn finish(&mut self) {
        self.armed = false;
        self.inner.lanes().running.remove(&self.seqnum);
        self.inner.stopped.notify_all();
    }
}

impl<D: BlockingDevice> Drop for SerialGuard<'_, D> {
    fn drop(&mut self) {
        if self.armed {
            self.inner.lanes().running.remove(&self.seqnum);
            self.inner.stopped.notify_all();
        }
    }
}

struct IsoGuard<'a, D: BlockingDevice> {
    inner: &'a Inner<D>,
    lane: (u8, Direction),
    ticket: u64,
    seqnum: u32,
    done: Option<Completion>,
}

impl<D: BlockingDevice> IsoGuard<'_, D> {
    fn finish(mut self, reply: Reply) {
        let done = self.done.take().expect("iso guard finished once");
        complete_iso(self.inner, self.lane, self.ticket, self.seqnum, done, reply);
    }
}

impl<D: BlockingDevice> Drop for IsoGuard<'_, D> {
    fn drop(&mut self) {
        if let Some(done) = self.done.take() {
            complete_iso(
                self.inner,
                self.lane,
                self.ticket,
                self.seqnum,
                done,
                Reply::failed(wire::EPROTO),
            );
        }
    }
}

fn complete_iso<D: BlockingDevice>(
    inner: &Inner<D>,
    lane: (u8, Direction),
    ticket: u64,
    seqnum: u32,
    done: Completion,
    reply: Reply,
) {
    let mut lanes = inner.lanes();
    lanes.running.remove(&seqnum);
    if let Some(iso) = lanes.iso.get_mut(&lane) {
        iso.in_flight = iso.in_flight.saturating_sub(1);
        iso.ready.insert(ticket, (done, reply));
    } else {
        drop(lanes);
        inner.stopped.notify_all();
        inner.wake.notify_all();
        done.complete(reply);
        return;
    }
    inner.stopped.notify_all();
    inner.wake.notify_all();

    let can_drain = lanes
        .iso
        .get_mut(&lane)
        .filter(|iso| !iso.completing && iso.ready.contains_key(&iso.next_complete))
        .map(|iso| iso.completing = true)
        .is_some();
    if !can_drain {
        return;
    }

    loop {
        let next = lanes.iso.get_mut(&lane).and_then(|iso| {
            let entry = iso.ready.remove(&iso.next_complete)?;
            iso.next_complete += 1;
            Some(entry)
        });
        let Some((done, reply)) = next else {
            if let Some(iso) = lanes.iso.get_mut(&lane) {
                iso.completing = false;
            }
            inner.wake.notify_all();
            break;
        };
        drop(lanes);
        done.complete(reply);
        lanes = inner.lanes();
    }
}

fn run_iso_worker<D: BlockingDevice>(inner: Arc<Inner<D>>) {
    loop {
        let work = {
            let mut lanes = inner.lanes();
            loop {
                if lanes.closed {
                    return;
                }
                if let Some(work) = lanes.iso_queue.pop_front() {
                    break work;
                }
                lanes.iso_idle += 1;
                lanes = inner
                    .iso_wake
                    .wait(lanes)
                    .unwrap_or_else(PoisonError::into_inner);
                lanes.iso_idle -= 1;
            }
        };
        let (lane, ticket, job) = work;
        let guard = IsoGuard {
            inner: &inner,
            lane,
            ticket,
            seqnum: job.transfer.seqnum,
            done: Some(job.done),
        };
        let reply = inner.device.transfer(&job.transfer, &job.cancelled);
        guard.finish(reply);
    }
}

fn run_lane<D: BlockingDevice>(inner: Arc<Inner<D>>, lane: (u8, Direction)) {
    loop {
        enum Next {
            Serial(Job),
            IsoDispatched,
        }
        let next = {
            let mut lanes = inner.lanes();
            loop {
                if lanes.closed {
                    return;
                }
                let is_iso = match lanes.queues.get(&lane).and_then(VecDeque::front) {
                    Some(job) => job.transfer.is_isochronous(),
                    None => {
                        lanes = inner
                            .wake
                            .wait(lanes)
                            .unwrap_or_else(PoisonError::into_inner);
                        continue;
                    }
                };
                if is_iso {
                    let busy = lanes
                        .iso
                        .get(&lane)
                        .is_some_and(|iso| iso.in_flight >= MAX_ISO_IN_FLIGHT);
                    if busy {
                        lanes = inner
                            .wake
                            .wait(lanes)
                            .unwrap_or_else(PoisonError::into_inner);
                        continue;
                    }
                    let job = lanes
                        .queues
                        .get_mut(&lane)
                        .and_then(VecDeque::pop_front)
                        .expect("front existed");
                    lanes
                        .running
                        .insert(job.transfer.seqnum, job.cancelled.clone());
                    let iso = lanes.iso.entry(lane).or_default();
                    let ticket = iso.next_ticket;
                    iso.next_ticket += 1;
                    iso.in_flight += 1;
                    lanes.iso_queue.push_back((lane, ticket, job));
                    let need_worker =
                        lanes.iso_idle == 0 && lanes.iso_threads.len() < MAX_ISO_WORKERS;
                    if need_worker {
                        let worker_inner = inner.clone();
                        match std::thread::Builder::new()
                            .name(format!("usb-iso-{}", lanes.iso_threads.len()))
                            .spawn(move || run_iso_worker(worker_inner))
                        {
                            Ok(thread) => lanes.iso_threads.push(thread),
                            Err(_) if lanes.iso_threads.is_empty() => {
                                let (lane, ticket, job) =
                                    lanes.iso_queue.pop_back().expect("just pushed");
                                let seqnum = job.transfer.seqnum;
                                drop(lanes);
                                complete_iso(
                                    &inner,
                                    lane,
                                    ticket,
                                    seqnum,
                                    job.done,
                                    Reply::failed(wire::EPROTO),
                                );
                                break Next::IsoDispatched;
                            }
                            Err(_) => {}
                        }
                    }
                    break Next::IsoDispatched;
                }
                let iso_draining = lanes.iso.get(&lane).is_some_and(|iso| {
                    iso.in_flight > 0 || iso.completing || !iso.ready.is_empty()
                });
                if iso_draining {
                    lanes = inner
                        .wake
                        .wait(lanes)
                        .unwrap_or_else(PoisonError::into_inner);
                    continue;
                }
                let job = lanes
                    .queues
                    .get_mut(&lane)
                    .and_then(VecDeque::pop_front)
                    .expect("front existed");
                lanes
                    .running
                    .insert(job.transfer.seqnum, job.cancelled.clone());
                break Next::Serial(job);
            }
        };
        match next {
            Next::IsoDispatched => inner.iso_wake.notify_one(),
            Next::Serial(job) => {
                let mut guard = SerialGuard {
                    inner: &inner,
                    seqnum: job.transfer.seqnum,
                    armed: true,
                };
                let reply = inner.device.transfer(&job.transfer, &job.cancelled);
                guard.finish();
                job.done.complete(reply);
            }
        }
    }
}

impl<D: BlockingDevice> UsbDevice for Blocking<D> {
    fn describe(&self) -> wire::Device {
        self.inner.device.describe()
    }

    fn class(&self) -> u8 {
        self.inner.device.class()
    }

    fn submit(&self, transfer: Transfer, done: Completion) {
        let lane = lane_of(&transfer);
        let mut lanes = self.inner.lanes();
        if lanes.closed {
            drop(lanes);
            return done.complete(Reply::failed(wire::ESHUTDOWN));
        }
        if !lanes.queues.contains_key(&lane) {
            let inner = self.inner.clone();
            let spawned = std::thread::Builder::new()
                .name(format!("usb-ep{}-{:?}", lane.0, lane.1))
                .spawn(move || run_lane(inner, lane));
            match spawned {
                Ok(thread) => lanes.threads.push(thread),
                Err(_) => {
                    drop(lanes);
                    return done.complete(Reply::failed(wire::EPROTO));
                }
            }
        }
        lanes.queues.entry(lane).or_default().push_back(Job {
            transfer,
            done,
            cancelled: Arc::new(AtomicBool::new(false)),
        });
        drop(lanes);
        self.inner.wake.notify_all();
    }

    fn unlink(&self, seqnum: u32) {
        let mut lanes = self.inner.lanes();
        if let Some(cancelled) = lanes.running.get(&seqnum) {
            cancelled.store(true, Ordering::SeqCst);
            // The device sees the flag only between its slices, and the slice that runs may
            // still take data (a key-up) the driver now expects the next transfer to get. The
            // server answers the unlink once this returns, so wait for the lane to let go.
            while lanes.running.contains_key(&seqnum) {
                lanes = self
                    .inner
                    .stopped
                    .wait(lanes)
                    .unwrap_or_else(PoisonError::into_inner);
            }
            return;
        }
        // Not started: it never runs. The server already dropped it, so its completion
        // writes nothing.
        for queue in lanes.queues.values_mut() {
            queue.retain(|job| job.transfer.seqnum != seqnum);
        }
    }

    fn close(&self) {
        let (was_closed, threads, iso_threads, queued) = {
            let mut lanes = self.inner.lanes();
            let was_closed = std::mem::replace(&mut lanes.closed, true);
            for cancelled in lanes.running.values() {
                cancelled.store(true, Ordering::SeqCst);
            }
            let mut queued: Vec<Job> = lanes
                .queues
                .values_mut()
                .flat_map(|q| q.drain(..))
                .collect();
            let iso_queue = std::mem::take(&mut lanes.iso_queue);
            for (lane, _ticket, job) in iso_queue {
                lanes.running.remove(&job.transfer.seqnum);
                if let Some(iso) = lanes.iso.get_mut(&lane) {
                    iso.in_flight = iso.in_flight.saturating_sub(1);
                }
                queued.push(job);
            }
            (
                was_closed,
                std::mem::take(&mut lanes.threads),
                std::mem::take(&mut lanes.iso_threads),
                queued,
            )
        };
        self.inner.wake.notify_all();
        self.inner.iso_wake.notify_all();
        self.inner.stopped.notify_all();
        drop(queued);
        let current = std::thread::current().id();
        for thread in threads.into_iter().chain(iso_threads) {
            // A lane or worker that closes its own device can't wait for itself.
            if thread.thread().id() != current {
                let _ = thread.join();
            }
        }
        if !was_closed {
            self.inner.device.close();
        }
    }
}

impl<D: BlockingDevice> Drop for Blocking<D> {
    fn drop(&mut self) {
        UsbDevice::close(self);
    }
}
