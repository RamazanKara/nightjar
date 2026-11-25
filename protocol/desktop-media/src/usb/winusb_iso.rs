//! Concurrent isochronous stream state coordination and port-reset logic for WinUSB.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

#[cfg(windows)]
use windows::core::PCWSTR;
#[cfg(windows)]
use windows::Win32::Foundation::{
    CloseHandle, GetLastError, GENERIC_READ, GENERIC_WRITE, HANDLE,
};
#[cfg(windows)]
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_FLAG_OVERLAPPED, FILE_SHARE_READ, FILE_SHARE_WRITE,
    OPEN_EXISTING,
};
#[cfg(windows)]
use windows::Win32::System::Threading::ResetEvent;

use nightjar_core::usb::{wire, Reply};
#[cfg(windows)]
use super::ffi::{
    win32_error_to_errno, IsochBufferHandle, OwnedEvent, UsbInterfaceDescriptor, WinUsbApi,
    WinUsbHandle,
};

/// Win32 error codes returned by WinUSB when an isochronous pipe's `ContinueStream` frame window
/// has lapsed (`ERROR_NOT_READY = 21`, `ERROR_UNEXP_NET_ERR = 59`). Note `ERROR_INVALID_PARAMETER`
/// (`87`) is never retried here so genuine invalid parameters surface directly (`R1-DM1`).
pub(in crate::usb) const WINUSB_CONTINUE_STREAM_LAPSED_ERRORS: [u32; 2] = [21, 59];

/// Snapshot returned from starting a pipelined isochronous submission (`begin`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::usb) struct IsoTicket {
    pub(in crate::usb) reset_epoch: u64,
    pub(in crate::usb) stream_id: u64,
    pub(in crate::usb) continue_stream: bool,
}

/// Serializes concurrent isochronous submissions per WinUSB endpoint so the first starts with
/// `ContinueStream = FALSE`, pipelined followers queue with `ContinueStream = TRUE`, and a stale
/// completion from an older reset epoch never corrupts a newer stream.
#[derive(Default)]
pub(in crate::usb) struct IsoEndpointState {
    streaming: bool,
    reset_epoch: u64,
    stream_id: u64,
}

impl IsoEndpointState {
    #[cfg(test)]
    pub(in crate::usb) fn is_streaming(&self) -> bool {
        self.streaming
    }

    pub(in crate::usb) fn reset(&mut self) {
        self.streaming = false;
        self.reset_epoch = self.reset_epoch.wrapping_add(1);
        self.stream_id = self.stream_id.wrapping_add(1);
    }

    /// Completes or aborts a submission recorded by `submit_pipelined`: on error in the matching
    /// `(reset_epoch, stream_id)`, resets `streaming` so the next submission restarts cleanly.
    pub(in crate::usb) fn finish(&mut self, ticket: IsoTicket, ok: bool) {
        if !ok && self.reset_epoch == ticket.reset_epoch && self.stream_id == ticket.stream_id {
            self.streaming = false;
            self.stream_id = self.stream_id.wrapping_add(1);
        }
    }
}

/// Runs a non-blocking isochronous kernel submission under `ep`'s submission lock, retrying once
/// with `ContinueStream = FALSE` only when `try_continue` was true and WinUSB reported a lapsed
/// stream (`21` or `59`, never `87`). Releases the lock before the caller waits on overlapped I/O.
pub(in crate::usb) fn submit_pipelined<T>(
    ep: &Mutex<IsoEndpointState>,
    mut try_issue: impl FnMut(bool) -> Result<T, (u32, i32)>,
) -> Result<(T, IsoTicket), i32> {
    let mut guard = ep.lock().unwrap_or_else(PoisonError::into_inner);
    let mut try_continue = guard.streaming;
    let reset_epoch = guard.reset_epoch;
    for attempt in 0..2 {
        match try_issue(try_continue) {
            Ok(val) => {
                guard.streaming = true;
                let ticket = IsoTicket {
                    reset_epoch,
                    stream_id: guard.stream_id,
                    continue_stream: try_continue,
                };
                return Ok((val, ticket));
            }
            Err((win32_err, errno)) => {
                if attempt == 0
                    && try_continue
                    && WINUSB_CONTINUE_STREAM_LAPSED_ERRORS.contains(&win32_err)
                {
                    try_continue = false;
                    continue;
                }
                guard.streaming = false;
                guard.stream_id = guard.stream_id.wrapping_add(1);
                return Err(errno);
            }
        }
    }
    guard.streaming = false;
    guard.stream_id = guard.stream_id.wrapping_add(1);
    Err(wire::EPROTO)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::usb) struct EndpointInfo {
    pub(in crate::usb) interface: u8,
    pub(in crate::usb) max_packet: u16,
    pub(in crate::usb) max_bytes_per_interval: u32,
}

/// Abstract pipe & alt-setting operations so `winusb_reset_port` is unit-tested cleanly on all OSes.
pub(in crate::usb) trait WinUsbPortReset {
    fn reset_pipe(&self, interface: u8, endpoint: u8) -> Result<(), i32>;
    fn set_alt(&self, interface: u8, setting: u8) -> Result<(), i32>;
}

/// Alternate-setting endpoint descriptors: `(interface, alt_setting) -> [(endpoint, max_packet, max_bytes_per_interval)]`.
pub(in crate::usb) type AltEndpointsMap = HashMap<(u8, u8), Vec<(u8, u16, u32)>>;

/// Shared interface/endpoint state touched by `reset_port` and `set_interface`.
pub(in crate::usb) struct ResettableState {
    pub(in crate::usb) interfaces: Vec<u8>,
    pub(in crate::usb) endpoints: HashMap<u8, EndpointInfo>,
    pub(in crate::usb) alt_endpoints: AltEndpointsMap,
}

/// Applies USB `PORT_RESET` semantics on WinUSB: resets active pipes, returns every interface to
/// alternate setting `0` so the device matches what the USB/IP host assumes after `PORT_RESET`,
/// rebuilds the endpoint map from setting `0`, resets alt-0 pipes, and clears all iso streams.
pub(in crate::usb) fn winusb_reset_port(
    ops: &dyn WinUsbPortReset,
    state: &Arc<Mutex<ResettableState>>,
    iso_streams: &Mutex<HashMap<u8, Arc<Mutex<IsoEndpointState>>>>,
) -> Reply {
    {
        let streams = iso_streams.lock().unwrap_or_else(PoisonError::into_inner);
        for ep in streams.values() {
            ep.lock().unwrap_or_else(PoisonError::into_inner).reset();
        }
    }

    let (interfaces, old_pipes): (Vec<u8>, Vec<(u8, u8)>) = {
        let guard: MutexGuard<'_, ResettableState> = state.lock().unwrap_or_else(PoisonError::into_inner);
        let pipes = guard
            .endpoints
            .iter()
            .map(|(&ep, info)| (info.interface, ep))
            .collect();
        (guard.interfaces.clone(), pipes)
    };

    for (iface, ep) in old_pipes {
        if let Err(errno) = ops.reset_pipe(iface, ep) {
            return Reply::failed(errno);
        }
    }

    for &iface in &interfaces {
        if let Err(errno) = ops.set_alt(iface, 0) {
            return Reply::failed(errno);
        }
    }

    let mut alt0_pipes = Vec::new();
    {
        let mut guard = state.lock().unwrap_or_else(PoisonError::into_inner);
        guard.endpoints.clear();
        for &iface in &interfaces {
            if let Some(updated) = guard.alt_endpoints.get(&(iface, 0)).cloned() {
                for (ep, max_packet, max_bytes_per_interval) in updated {
                    guard.endpoints.insert(
                        ep,
                        EndpointInfo {
                            interface: iface,
                            max_packet,
                            max_bytes_per_interval,
                        },
                    );
                    alt0_pipes.push((iface, ep));
                }
            }
        }
    }

    for (iface, ep) in alt0_pipes {
        if let Err(errno) = ops.reset_pipe(iface, ep) {
            return Reply::failed(errno);
        }
    }

    Reply::sent(0)
}

#[cfg(windows)]
#[derive(Clone, Copy)]
pub(super) struct InterfaceEntry {
    pub(super) winusb: WinUsbHandle,
    pub(super) file: HANDLE,
    pub(super) associated: bool,
}

// SAFETY: WinUSB handles opened with `FILE_FLAG_OVERLAPPED` are safe to submit I/O on across
// threads; handle teardown happens in `close()` after `Blocking::close()` joins all lanes.
#[cfg(windows)]
unsafe impl Send for InterfaceEntry {}
#[cfg(windows)]
unsafe impl Sync for InterfaceEntry {}

#[cfg(windows)]
pub(super) struct Handles {
    pub(super) interfaces: HashMap<u8, InterfaceEntry>,
    pub(super) files: Vec<HANDLE>,
    pub(super) default_interface: u8,
}

// SAFETY: Handles are freed exclusively after all I/O lanes have stopped.
#[cfg(windows)]
unsafe impl Send for Handles {}
#[cfg(windows)]
unsafe impl Sync for Handles {}

#[cfg(windows)]
impl Handles {
    pub(super) fn open_paths(api: &WinUsbApi, paths: &[String]) -> Result<Self, String> {
        let mut handles = Handles {
            interfaces: HashMap::new(),
            files: Vec::new(),
            default_interface: 0,
        };
        let open_all = (|| -> Result<(), String> {
            for path in paths {
                let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
                // SAFETY: `wide` is NUL-terminated; file handle is stored in `handles.files` immediately.
                let file = unsafe {
                    CreateFileW(
                        PCWSTR(wide.as_ptr()),
                        GENERIC_READ.0 | GENERIC_WRITE.0,
                        FILE_SHARE_READ | FILE_SHARE_WRITE,
                        None,
                        OPEN_EXISTING,
                        FILE_ATTRIBUTE_NORMAL | FILE_FLAG_OVERLAPPED,
                        None,
                    )
                }
                .map_err(|e| format!("Windows refused to open the WinUSB device; another program may be using it ({e})."))?;
                handles.files.push(file);

                let mut base: WinUsbHandle = std::ptr::null_mut();
                // SAFETY: `file` is an open WinUSB device handle.
                if unsafe { (api.initialize)(file, &mut base) } == 0 || base.is_null() {
                    let err = unsafe { GetLastError().0 };
                    return Err(format!("WinUsb_Initialize failed with Win32 error {err}."));
                }
                let mut desc = UsbInterfaceDescriptor::default();
                let base_num = if unsafe { (api.query_settings)(base, 0, &mut desc) } != 0 {
                    desc.b_interface_number
                } else {
                    0
                };
                if handles.interfaces.is_empty() {
                    handles.default_interface = base_num;
                }
                if let std::collections::hash_map::Entry::Vacant(slot) = handles.interfaces.entry(base_num) {
                    slot.insert(InterfaceEntry { winusb: base, file, associated: false });
                } else {
                    // SAFETY: duplicate interface handle not stored in `handles.interfaces`.
                    unsafe {
                        let _ = (api.free)(base);
                    }
                    continue;
                }

                // Claim every associated interface exposed under the same WinUSB device handle.
                for idx in 0u8..=254 {
                    let mut assoc: WinUsbHandle = std::ptr::null_mut();
                    if unsafe { (api.get_associated)(base, idx, &mut assoc) } == 0 || assoc.is_null() {
                        break;
                    }
                    let mut assoc_desc = UsbInterfaceDescriptor::default();
                    let num = if unsafe { (api.query_settings)(assoc, 0, &mut assoc_desc) } != 0 {
                        assoc_desc.b_interface_number
                    } else {
                        base_num.saturating_add(idx + 1)
                    };
                    if let std::collections::hash_map::Entry::Vacant(slot) = handles.interfaces.entry(num) {
                        slot.insert(InterfaceEntry { winusb: assoc, file, associated: true });
                    } else {
                        // SAFETY: duplicate associated handle not stored in `handles.interfaces`.
                        unsafe {
                            let _ = (api.free)(assoc);
                        }
                    }
                }
            }
            Ok(())
        })();
        if let Err(err) = open_all {
            handles.close_all(api);
            return Err(err);
        }
        Ok(handles)
    }

    pub(super) fn close_all(&mut self, api: &WinUsbApi) {
        // WinUSB requires freeing associated interface handles before their base handle.
        for entry in self.interfaces.values().filter(|e| e.associated) {
            unsafe {
                let _ = (api.free)(entry.winusb);
            }
        }
        for entry in self.interfaces.values().filter(|e| !e.associated) {
            unsafe {
                let _ = (api.free)(entry.winusb);
            }
        }
        self.interfaces.clear();
        for file in self.files.drain(..) {
            unsafe {
                let _ = CloseHandle(file);
            }
        }
    }
}

#[cfg(windows)]
const MAX_EVENT_POOL: usize = 16;
#[cfg(windows)]
const MAX_ISO_POOL: usize = 8;

/// Reusable manual-reset Win32 event wrapper returned to a device pool on drop.
#[cfg(windows)]
pub(super) struct PooledEvent {
    event: Option<OwnedEvent>,
    pool: Arc<Mutex<Vec<OwnedEvent>>>,
}

#[cfg(windows)]
impl PooledEvent {
    pub(super) fn acquire(pool: &Arc<Mutex<Vec<OwnedEvent>>>) -> Result<Self, i32> {
        let recycled = pool.lock().unwrap_or_else(PoisonError::into_inner).pop();
        let event = match recycled {
            Some(ev) => {
                // SAFETY: `ev.0` is an open manual-reset event not attached to any active I/O.
                if unsafe { ResetEvent(ev.0) }.is_err() {
                    OwnedEvent::new()?
                } else {
                    ev
                }
            }
            None => OwnedEvent::new()?,
        };
        Ok(Self { event: Some(event), pool: pool.clone() })
    }

    pub(super) fn handle(&self) -> HANDLE {
        self.event.as_ref().expect("event active until drop").0
    }
}

#[cfg(windows)]
impl Drop for PooledEvent {
    fn drop(&mut self) {
        if let Some(ev) = self.event.take() {
            let mut pool = self.pool.lock().unwrap_or_else(PoisonError::into_inner);
            if pool.len() < MAX_EVENT_POOL {
                pool.push(ev);
            }
        }
    }
}

/// Pre-registered contiguous buffer slice pinned with `WinUsb_RegisterIsochBuffer` so steady-state
/// URBs reuse kernel MDLs instead of registering/unregistering on every transfer.
#[cfg(windows)]
pub(super) struct IsoSlot {
    pub(super) interface: u8,
    pub(super) buffer: Vec<u8>,
    pub(super) handle: IsochBufferHandle,
    pub(super) api: &'static WinUsbApi,
}

// SAFETY: Each `IsoSlot` is exclusively owned while in flight and unregistered before handles free.
#[cfg(windows)]
unsafe impl Send for IsoSlot {}

#[cfg(windows)]
impl Drop for IsoSlot {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            // SAFETY: caller drained/completed overlapped I/O before dropping `IsoSlot`.
            unsafe {
                let _ = (self.api.unregister_isoch_buffer)(self.handle);
            }
        }
    }
}

#[cfg(windows)]
pub(super) fn drain_iso_pools(
    iso_pools: &Mutex<HashMap<u8, Vec<IsoSlot>>>,
    interface: Option<u8>,
) {
    let drained: Vec<IsoSlot> = {
        let mut pools = iso_pools.lock().unwrap_or_else(PoisonError::into_inner);
        let mut out = Vec::new();
        for list in pools.values_mut() {
            let mut idx = 0;
            while idx < list.len() {
                if interface.is_none_or(|iface| list[idx].interface == iface) {
                    out.push(list.swap_remove(idx));
                } else {
                    idx += 1;
                }
            }
        }
        if interface.is_none() {
            pools.clear();
        }
        out
    };
    drop(drained);
}

#[cfg(windows)]
pub(super) fn acquire_iso_slot(
    api: &'static WinUsbApi,
    winusb: super::ffi::WinUsbHandle,
    iso_pools: &Mutex<HashMap<u8, Vec<IsoSlot>>>,
    interface: u8,
    address: u8,
    min_bytes: usize,
) -> Result<IsoSlot, i32> {
    let recycled = {
        let mut pools = iso_pools.lock().unwrap_or_else(PoisonError::into_inner);
        pools.get_mut(&address).and_then(|list| {
            list.iter()
                .position(|s| s.interface == interface && s.buffer.len() >= min_bytes)
                .or_else(|| (!list.is_empty()).then_some(0))
                .map(|idx| list.swap_remove(idx))
        })
    };
    if let Some(slot) = recycled
        && slot.interface == interface
        && slot.buffer.len() >= min_bytes
    {
        return Ok(slot);
    }
    let capacity = min_bytes.max(4096);
    let mut buffer = vec![0u8; capacity];
    let mut iso_handle: IsochBufferHandle = std::ptr::null_mut();
    // SAFETY: `buffer` stays alive inside `IsoSlot` until unregistered in `Drop`.
    let reg = unsafe {
        (api.register_isoch_buffer)(
            winusb,
            address,
            buffer.as_mut_ptr(),
            capacity as u32,
            &mut iso_handle,
        )
    };
    if reg == 0 || iso_handle.is_null() {
        let err = unsafe { GetLastError().0 };
        return Err(win32_error_to_errno(err));
    }
    Ok(IsoSlot { interface, buffer, handle: iso_handle, api })
}

#[cfg(windows)]
pub(super) fn release_iso_slot(
    closed: &std::sync::atomic::AtomicBool,
    resettable: &Mutex<ResettableState>,
    iso_pools: &Mutex<HashMap<u8, Vec<IsoSlot>>>,
    address: u8,
    slot: IsoSlot,
) {
    if closed.load(std::sync::atomic::Ordering::Acquire) {
        return;
    }
    let still_matches = resettable
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .endpoints
        .get(&address)
        .is_some_and(|info| info.interface == slot.interface);
    if !still_matches {
        return;
    }
    let mut pools = iso_pools.lock().unwrap_or_else(PoisonError::into_inner);
    let list = pools.entry(address).or_default();
    if list.len() < MAX_ISO_POOL {
        list.push(slot);
    }
}
