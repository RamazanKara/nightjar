//! Asynchronous libusb isochronous transfers and shared USB/IP iso reply packing.
//!
//! Serves isochronous USB/IP transfers through libusb's asynchronous transfer API on macOS,
//! Linux (when `usbip-host` is unavailable), and Windows devices already bound to `libusbK`.
//! Windows devices on Microsoft's inbox `WinUSB` driver use [`super::winusb`] instead.

use std::ffi::{c_int, c_uint, c_void};
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
// Windows (non-unix): POLL_SLICE uses Duration.
#[cfg(not(unix))]
use std::time::Duration;

use nightjar_core::usb::{wire, Direction, IsoPacket, Reply, Transfer};
use rusb::constants::{
    LIBUSB_ERROR_NOT_SUPPORTED, LIBUSB_TRANSFER_CANCELLED, LIBUSB_TRANSFER_COMPLETED,
    LIBUSB_TRANSFER_NO_DEVICE, LIBUSB_TRANSFER_OVERFLOW, LIBUSB_TRANSFER_STALL,
    LIBUSB_TRANSFER_TIMED_OUT, LIBUSB_TRANSFER_TYPE_ISOCHRONOUS,
};
use rusb::UsbContext;

/// Upper bound on packets per isochronous URB (matches USB/IP and Linux usbfs caps).
pub(super) const MAX_ISO_PACKETS: usize = 1024;
/// Timeout for a single isochronous transfer in milliseconds.
const ISO_TIMEOUT_MS: c_uint = 5_000;
/// Event-loop poll interval so cancellation and close wake promptly.
// Linux and macOS (unix): libusb_handle_events_timeout_completed takes a libc::timeval slice.
#[cfg(unix)]
const POLL_SLICE_US: i64 = 2_000;
// Windows (non-unix): rusb::Context::handle_events takes a Duration.
#[cfg(not(unix))]
const POLL_SLICE: Duration = Duration::from_millis(2);

const _: () = assert!(std::mem::size_of::<AtomicI32>() == std::mem::size_of::<c_int>());

/// Outcome of one completed isochronous packet inside a contiguous transfer buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct CompletedPacket {
    /// Byte offset of this packet's payload inside the completed transfer buffer.
    pub(super) offset: usize,
    /// Bytes transferred for this packet before clamping.
    pub(super) actual_length: u32,
    /// USB/IP negated errno (`0` on success).
    pub(super) status: i32,
}

/// Validates an isochronous [`Transfer`]'s packet table and returns its total buffer byte length.
pub(super) fn iso_plan(transfer: &Transfer) -> Result<usize, i32> {
    if transfer.iso.is_empty() || transfer.iso.len() > MAX_ISO_PACKETS {
        return Err(wire::EINVAL);
    }
    let mut total = 0usize;
    for packet in &transfer.iso {
        total = total.checked_add(packet.length as usize).ok_or(wire::EINVAL)?;
    }
    if total == 0 || total > transfer.length as usize || c_int::try_from(total).is_err() {
        return Err(wire::EINVAL);
    }
    if transfer.direction == Direction::Out && transfer.data.len() < total {
        return Err(wire::EINVAL);
    }
    Ok(total)
}

/// Maps a `libusb_transfer_status` code to USB/IP's negated errno.
pub(super) fn libusb_transfer_status(status: c_int) -> i32 {
    match status {
        LIBUSB_TRANSFER_COMPLETED => 0,
        LIBUSB_TRANSFER_STALL => wire::EPIPE,
        LIBUSB_TRANSFER_NO_DEVICE => wire::ENODEV,
        LIBUSB_TRANSFER_OVERFLOW => wire::EOVERFLOW,
        LIBUSB_TRANSFER_TIMED_OUT => wire::ETIMEDOUT,
        LIBUSB_TRANSFER_CANCELLED => wire::ECONNRESET,
        _ => wire::EPROTO,
    }
}

/// Packs per-packet isochronous outcomes into a USB/IP [`Reply`] that satisfies
/// [`wire::iso_reply_fits`].
pub(super) fn pack_iso_reply(
    direction: Direction,
    max_total: u32,
    asked: &[IsoPacket],
    completed: &[CompletedPacket],
    buffer: &[u8],
) -> Reply {
    let mut reply = Reply {
        iso: Vec::with_capacity(asked.len()),
        ..Reply::default()
    };
    let mut remaining = max_total;
    let mut total_actual = 0u32;
    let mut errors = 0u32;

    for (idx, packet) in asked.iter().enumerate() {
        let done = completed.get(idx).copied().unwrap_or(CompletedPacket {
            offset: 0,
            actual_length: 0,
            status: wire::EPROTO,
        });
        let mut actual = done.actual_length.min(packet.length).min(remaining);
        if direction == Direction::In {
            let start = done.offset.min(buffer.len());
            let avail = buffer.len() - start;
            actual = actual.min(u32::try_from(avail).unwrap_or(u32::MAX));
            if actual > 0 {
                reply.data.extend_from_slice(&buffer[start..start + actual as usize]);
            }
        }
        remaining -= actual;
        total_actual += actual;
        if done.status != 0 {
            errors += 1;
        }
        reply.iso.push(IsoPacket {
            actual_length: actual,
            status: done.status,
            ..*packet
        });
    }

    if direction == Direction::Out {
        reply.sent = total_actual;
    }
    reply.error_count = errors;
    reply
}

extern "system" fn on_iso_complete(transfer: *mut rusb::ffi::libusb_transfer) {
    // SAFETY: `user_data` points to the stack-pinned `AtomicI32` inside `libusb_iso_transfer`,
    // which waits until this flag becomes non-zero before freeing `transfer` or returning.
    unsafe {
        let done = (*transfer).user_data as *const AtomicI32;
        if !done.is_null() {
            (*done).store(1, Ordering::Release);
        }
    }
}

/// Pumps libusb's event loop once for up to one poll slice.
fn pump_events(handle: &rusb::DeviceHandle<rusb::Context>, done: &AtomicI32) {
    // Linux and macOS (unix): pass the completion pointer so libusb returns immediately if
    // another thread already invoked our callback.
    #[cfg(unix)]
    unsafe {
        let tv = libc::timeval {
            tv_sec: 0,
            tv_usec: POLL_SLICE_US as _,
        };
        let completed = (done as *const AtomicI32).cast_mut().cast::<c_int>();
        let _ = rusb::ffi::libusb_handle_events_timeout_completed(
            handle.context().as_raw(),
            &tv,
            completed,
        );
    }
    // Windows (non-unix): rusb's handle_events wraps libusb_handle_events_timeout_completed
    // without a direct libc dependency; POLL_SLICE bounds wake latency across lanes.
    #[cfg(not(unix))]
    {
        let _ = done;
        let _ = handle.context().handle_events(Some(POLL_SLICE));
    }
}

/// Executes one isochronous USB/IP transfer through libusb's asynchronous API.
pub(super) fn libusb_iso_transfer(
    handle: &rusb::DeviceHandle<rusb::Context>,
    busid: &str,
    address: u8,
    transfer: &Transfer,
    cancelled: &AtomicBool,
    closed: &AtomicBool,
    warned_unsupported: &AtomicBool,
) -> Reply {
    let total = match iso_plan(transfer) {
        Ok(total) => total,
        Err(status) => return Reply::failed(status),
    };
    let mut buffer = match transfer.direction {
        Direction::In => vec![0u8; total],
        Direction::Out => transfer.data[..total].to_vec(),
    };
    let count = transfer.iso.len();
    // SAFETY: `count` is in 1..=MAX_ISO_PACKETS; `raw` is freed before `buffer` and `done` drop.
    let raw = unsafe { rusb::ffi::libusb_alloc_transfer(count as c_int) };
    if raw.is_null() {
        return Reply::failed(wire::EPROTO);
    }
    let done = Box::new(AtomicI32::new(0));
    let done_ptr = (&*done as *const AtomicI32).cast_mut().cast::<c_void>();

    // SAFETY: `raw` was freshly allocated with `count` iso packet descriptors, `buffer` stays
    // alive and unmoved until after `libusb_free_transfer(raw)`, and `done` is heap-pinned.
    unsafe {
        (*raw).dev_handle = handle.as_raw();
        (*raw).flags = 0;
        (*raw).endpoint = address;
        (*raw).transfer_type = LIBUSB_TRANSFER_TYPE_ISOCHRONOUS;
        (*raw).timeout = ISO_TIMEOUT_MS;
        (*raw).length = total as c_int;
        (*raw).buffer = buffer.as_mut_ptr();
        (*raw).num_iso_packets = count as c_int;
        (*raw).callback = on_iso_complete;
        (*raw).user_data = done_ptr;
        let descs = (*raw).iso_packet_desc.as_mut_ptr();
        for (i, packet) in transfer.iso.iter().enumerate() {
            let desc = descs.add(i);
            (*desc).length = packet.length;
            (*desc).actual_length = 0;
            (*desc).status = 0;
        }
    }

    // SAFETY: `raw` is fully initialized and owned exclusively by libusb until callback fires.
    let submit = unsafe { rusb::ffi::libusb_submit_transfer(raw) };
    if submit < 0 {
        // SAFETY: submission failed synchronously; libusb does not hold `raw`.
        unsafe { rusb::ffi::libusb_free_transfer(raw) };
        if submit == LIBUSB_ERROR_NOT_SUPPORTED && !warned_unsupported.swap(true, Ordering::SeqCst) {
            eprintln!("nightjar: USB {busid}: libusb reported isochronous transfers unsupported on this driver");
        }
        return Reply::failed(super::libusb::errno(submit));
    }

    let mut cancel_sent = false;
    while done.load(Ordering::Acquire) == 0 {
        if !cancel_sent && (cancelled.load(Ordering::SeqCst) || closed.load(Ordering::SeqCst)) {
            cancel_sent = true;
            // SAFETY: `raw` is in flight; libusb ignores cancel if completion raced ahead.
            let _ = unsafe { rusb::ffi::libusb_cancel_transfer(raw) };
        }
        pump_events(handle, &done);
    }

    // SAFETY: `on_iso_complete` has executed (Acquire pairs with callback's Release), so libusb
    // will not touch `raw` or `buffer` again.
    let (overall, completed) = unsafe {
        let overall = (*raw).status;
        let descs = (*raw).iso_packet_desc.as_ptr();
        let mut completed = Vec::with_capacity(count);
        let mut offset = 0usize;
        for (i, packet) in transfer.iso.iter().enumerate() {
            let desc = &*descs.add(i);
            completed.push(CompletedPacket {
                offset,
                actual_length: desc.actual_length,
                status: libusb_transfer_status(desc.status),
            });
            offset += packet.length as usize;
        }
        rusb::ffi::libusb_free_transfer(raw);
        (overall, completed)
    };

    if cancel_sent || overall == LIBUSB_TRANSFER_CANCELLED {
        return Reply::failed(wire::ECONNRESET);
    }
    if overall != LIBUSB_TRANSFER_COMPLETED {
        return Reply::failed(libusb_transfer_status(overall));
    }
    pack_iso_reply(transfer.direction, transfer.length, &transfer.iso, &completed, &buffer)
}
