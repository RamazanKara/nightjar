//! Direct Windows inbox `winusb.dll` USB/IP transport with full isochronous support.
//!
//! Serves control, bulk, interrupt, and isochronous USB/IP transfers directly through Microsoft's
//! inbox `winusb.dll` (`WinUsb_RegisterIsochBuffer` / `WinUsb_ReadIsochPipeAsap` /
//! `WinUsb_WriteIsochPipeAsap`) so Windows users need zero extra drivers beyond `WinUSB`.

#![cfg_attr(not(windows), allow(dead_code))]

#[path = "winusb_ffi.rs"]
mod ffi;
#[path = "winusb_iso.rs"]
pub(super) mod iso_coord;

#[cfg(test)]
pub(super) use ffi::{
    control_target_endpoint, control_target_interface, overlapped_error_to_errno,
    pack_winusb_iso_in, pack_winusb_iso_out, usbd_status_to_errno,
    validate_winusb_iso_out_table, win32_error_to_errno, UsbdIsoPacketDescriptor,
};

#[cfg(windows)]
mod platform {
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
    use std::time::{Duration, Instant};

    use nightjar_core::usb::{wire, Blocking, BlockingDevice, Direction, Reply, Transfer, UsbDevice};
    use windows::Win32::Foundation::{GetLastError, ERROR_IO_PENDING, WAIT_OBJECT_0, WAIT_TIMEOUT};
    use windows::Win32::System::IO::{CancelIoEx, OVERLAPPED};
    use windows::Win32::System::Threading::WaitForSingleObject;

    use super::ffi::{
        api, control_target_endpoint, control_target_interface, overlapped_error_to_errno,
        pack_winusb_iso_in, pack_winusb_iso_out, validate_winusb_iso_out_table,
        win32_error_to_errno, OwnedEvent, UsbdIsoPacketDescriptor, WinBool, WinUsbApi,
        WinUsbPipeInformationEx, WinUsbSetupPacket, USBD_PIPE_TYPE_ISOCHRONOUS,
    };
    use super::iso_coord::{
        acquire_iso_slot, invalidate_iso_pools, release_iso_slot, submit_pipelined,
        winusb_reset_port, EndpointInfo, Handles, InterfaceEntry, IsoEndpointState, IsoPoolState,
        IsoSlot, PooledEvent, ResettableState, WinUsbPortReset,
    };
    use crate::usb::iso::iso_plan;
    use crate::usb::libusb::{standard, Standard};

    const URB_ZERO_PACKET: u32 = 0x40;
    const CONTROL_TIMEOUT: Duration = Duration::from_secs(5);
    const ISO_TIMEOUT: Duration = Duration::from_secs(5);
    const POLL_MS: u32 = 10;

    struct State {
        configuration: u8,
        active: u8,
        handles: Handles,
    }

    struct WinUsbDevice {
        api: &'static WinUsbApi,
        described: wire::Device,
        class: u8,
        closed: AtomicBool,
        state: Mutex<State>,
        resettable: Arc<Mutex<ResettableState>>,
        iso_streams: Mutex<HashMap<u8, Arc<Mutex<IsoEndpointState>>>>,
        iso_pools: Mutex<IsoPoolState<IsoSlot>>,
        event_pool: Arc<Mutex<Vec<OwnedEvent>>>,
    }

    impl WinUsbDevice {
        fn state(&self) -> MutexGuard<'_, State> {
            self.state.lock().unwrap_or_else(PoisonError::into_inner)
        }

        fn interface_handle(&self, interface: Option<u8>) -> Option<InterfaceEntry> {
            let state = self.state();
            if let Some(num) = interface
                && let Some(&entry) = state.handles.interfaces.get(&num)
            {
                return Some(entry);
            }
            state
                .handles
                .interfaces
                .get(&state.handles.default_interface)
                .copied()
                .or_else(|| state.handles.interfaces.values().next().copied())
        }

        fn endpoint_entry(&self, address: u8) -> Option<(InterfaceEntry, EndpointInfo)> {
            let info = *self
                .resettable
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .endpoints
                .get(&address)?;
            let state = self.state();
            let entry = *state.handles.interfaces.get(&info.interface)?;
            Some((entry, info))
        }

        fn iso_stream(&self, address: u8) -> Arc<Mutex<IsoEndpointState>> {
            self.iso_streams
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .entry(address)
                .or_default()
                .clone()
        }

        fn invalidate_iso_pools(&self, interface: Option<u8>) {
            invalidate_iso_pools(&self.iso_pools, interface);
        }

        /// Finishes an overlapped WinUSB request synchronously and checks its result.
        fn complete_overlapped(
            &self,
            entry: InterfaceEntry,
            overlapped: &mut OVERLAPPED,
        ) -> Result<u32, i32> {
            let mut transferred = 0u32;
            // SAFETY: `entry.winusb` is open and `overlapped` belongs to this in-flight request.
            let ok = unsafe {
                (self.api.get_overlapped_result)(entry.winusb, overlapped, &mut transferred, 1)
            };
            if ok != 0 {
                Ok(transferred)
            } else {
                let err = unsafe { GetLastError().0 };
                Err(win32_error_to_errno(err))
            }
        }

        /// Waits for an overlapped WinUSB call to complete, cancelling via `CancelIoEx` and
        /// draining `WinUsb_GetOverlappedResult(..., TRUE)` before returning so caller buffers
        /// and iso registrations are never touched after return.
        fn wait_overlapped(
            &self,
            entry: InterfaceEntry,
            overlapped: &mut OVERLAPPED,
            timeout: Option<Duration>,
            cancelled: &AtomicBool,
        ) -> Result<u32, i32> {
            let deadline = timeout.map(|d| Instant::now() + d);
            let mut cancel_reason: Option<i32> = None;

            loop {
                // SAFETY: `overlapped.hEvent` is a valid manual-reset event owned by caller.
                let wait = unsafe { WaitForSingleObject(overlapped.hEvent, POLL_MS) };
                if wait == WAIT_OBJECT_0 {
                    break;
                }
                if wait != WAIT_TIMEOUT {
                    cancel_reason = Some(wire::EPROTO);
                    break;
                }
                if cancelled.load(Ordering::SeqCst) || self.closed.load(Ordering::Acquire) {
                    cancel_reason = Some(wire::ECONNRESET);
                    break;
                }
                if let Some(limit) = deadline
                    && Instant::now() >= limit
                {
                    cancel_reason = Some(wire::ETIMEDOUT);
                    break;
                }
            }

            if cancel_reason.is_some() {
                // SAFETY: `entry.file` is open and `overlapped` stays pinned until
                // `WinUsb_GetOverlappedResult(..., TRUE)` returns below.
                unsafe {
                    let _ = CancelIoEx(entry.file, Some(overlapped));
                }
            }

            let mut transferred = 0u32;
            // SAFETY: `bWait = 1` guarantees the kernel has completely finished with `overlapped`
            // and its buffers before this call returns.
            let ok = unsafe {
                (self.api.get_overlapped_result)(entry.winusb, overlapped, &mut transferred, 1)
            };
            if ok != 0 {
                return Ok(transferred);
            }
            let err = unsafe { GetLastError().0 };
            let final_reason = if cancel_reason.is_none()
                && (cancelled.load(Ordering::SeqCst) || self.closed.load(Ordering::Acquire))
            {
                Some(wire::ECONNRESET)
            } else {
                cancel_reason
            };
            Err(overlapped_error_to_errno(err, final_reason))
        }

        fn set_interface(&self, interface: u8, setting: u8) -> Reply {
            let entry = {
                let state = self.state();
                let Some(&entry) = state.handles.interfaces.get(&interface) else {
                    return Reply::failed(wire::EINVAL);
                };
                entry
            };
            let removed_eps: Vec<u8> = {
                let resettable = self.resettable.lock().unwrap_or_else(PoisonError::into_inner);
                resettable
                    .endpoints
                    .iter()
                    .filter_map(|(&ep, info)| (info.interface == interface).then_some(ep))
                    .collect()
            };
            self.invalidate_iso_pools(Some(interface));
            // SAFETY: `entry.winusb` is an open WinUSB handle for `interface`.
            let ok = unsafe { (self.api.set_alt)(entry.winusb, setting) };
            if ok == 0 {
                let err = unsafe { GetLastError().0 };
                self.invalidate_iso_pools(Some(interface));
                return Reply::failed(win32_error_to_errno(err));
            }
            let updated_eps = {
                let mut resettable = self.resettable.lock().unwrap_or_else(PoisonError::into_inner);
                for ep in &removed_eps {
                    resettable.endpoints.remove(ep);
                }
                let updated = resettable.alt_endpoints.get(&(interface, setting)).cloned();
                let mut touched = removed_eps;
                if let Some(updated) = updated {
                    for (ep, max_packet, max_bytes_per_interval) in updated {
                        resettable.endpoints.insert(
                            ep,
                            EndpointInfo {
                                interface,
                                max_packet,
                                max_bytes_per_interval,
                            },
                        );
                        touched.push(ep);
                    }
                }
                touched
            };
            {
                let streams = self.iso_streams.lock().unwrap_or_else(PoisonError::into_inner);
                for ep in updated_eps {
                    if let Some(stream) = streams.get(&ep) {
                        stream.lock().unwrap_or_else(PoisonError::into_inner).reset();
                    }
                }
            }
            // Bump and drain again after alt-setting handles/endpoints settle so any concurrent
            // ISO acquire/release overlapping `set_alt` cannot return pre-switch slots to the pool.
            self.invalidate_iso_pools(Some(interface));
            Reply::sent(0)
        }

        fn clear_halt(&self, endpoint: u8) -> Reply {
            let Some((entry, _)) = self.endpoint_entry(endpoint) else {
                return Reply::failed(wire::EPIPE);
            };
            // SAFETY: `entry.winusb` owns `endpoint`.
            let ok = unsafe { (self.api.reset_pipe)(entry.winusb, endpoint) };
            self.iso_stream(endpoint)
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .reset();
            if ok == 0 {
                let err = unsafe { GetLastError().0 };
                Reply::failed(win32_error_to_errno(err))
            } else {
                Reply::sent(0)
            }
        }

        fn reset_port(&self) -> Reply {
            struct DevicePortReset<'a>(&'a WinUsbDevice);
            impl WinUsbPortReset for DevicePortReset<'_> {
                fn reset_pipe(&self, interface: u8, endpoint: u8) -> Result<(), i32> {
                    let Some(entry) = self.0.interface_handle(Some(interface)) else {
                        return Err(wire::ENODEV);
                    };
                    unsafe {
                        let _ = (self.0.api.abort_pipe)(entry.winusb, endpoint);
                        if endpoint & 0x80 != 0 {
                            let _ = (self.0.api.flush_pipe)(entry.winusb, endpoint);
                        }
                        if (self.0.api.reset_pipe)(entry.winusb, endpoint) == 0 {
                            let err = GetLastError().0;
                            return Err(win32_error_to_errno(err));
                        }
                    }
                    Ok(())
                }

                fn set_alt(&self, interface: u8, setting: u8) -> Result<(), i32> {
                    let Some(entry) = self.0.interface_handle(Some(interface)) else {
                        return Err(wire::ENODEV);
                    };
                    if unsafe { (self.0.api.set_alt)(entry.winusb, setting) } == 0 {
                        let err = unsafe { GetLastError().0 };
                        return Err(win32_error_to_errno(err));
                    }
                    Ok(())
                }
            }

            self.invalidate_iso_pools(None);
            let reply = winusb_reset_port(&DevicePortReset(self), &self.resettable, &self.iso_streams);
            // Bump global pool epoch again after updating alt-0 endpoints so slots woken mid-reset
            // by `abort_pipe` never return to the post-reset pool.
            self.invalidate_iso_pools(None);
            reply
        }

        fn control(&self, transfer: &Transfer, cancelled: &AtomicBool) -> Reply {
            let setup = &transfer.setup;
            match standard(setup) {
                Some(Standard::SetConfiguration(value)) => {
                    let mut state = self.state();
                    if value == state.configuration {
                        return Reply::sent(0);
                    }
                    if value == 0 {
                        state.configuration = 0;
                        return Reply::sent(0);
                    }
                    if value == state.active {
                        state.configuration = value;
                        return Reply::sent(0);
                    }
                    return Reply::failed(wire::EINVAL);
                }
                Some(Standard::SetInterface { interface, setting }) => {
                    return self.set_interface(interface, setting);
                }
                Some(Standard::ClearHalt(endpoint)) => return self.clear_halt(endpoint),
                Some(Standard::PortReset) => return self.reset_port(),
                None => {}
            }

            let target_iface = control_target_interface(setup).or_else(|| {
                control_target_endpoint(setup).and_then(|ep| {
                    self.resettable
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .endpoints
                        .get(&ep)
                        .map(|i| i.interface)
                })
            });
            let Some(entry) = self.interface_handle(target_iface) else {
                return Reply::failed(wire::ENODEV);
            };
            let packet = WinUsbSetupPacket {
                request_type: setup[0],
                request: setup[1],
                value: u16::from_le_bytes([setup[2], setup[3]]),
                index: u16::from_le_bytes([setup[4], setup[5]]),
                length: u16::from_le_bytes([setup[6], setup[7]]),
            };
            let is_in = setup[0] & 0x80 != 0;
            let len = if is_in {
                usize::from(packet.length).min(transfer.length as usize)
            } else {
                transfer.data.len()
            };
            let mut buffer = if is_in { vec![0u8; len] } else { transfer.data.clone() };
            let event = match PooledEvent::acquire(&self.event_pool) {
                Ok(e) => e,
                Err(status) => return Reply::failed(status),
            };
            let mut overlapped = OVERLAPPED { hEvent: event.handle(), ..Default::default() };
            let buf_ptr = if buffer.is_empty() { std::ptr::null_mut() } else { buffer.as_mut_ptr() };
            // SAFETY: `buffer` and `overlapped` remain pinned until overlapped completion returns.
            // Per WinUSB docs, `LengthTransferred` must be NULL when `Overlapped` is non-NULL.
            let ok = unsafe {
                (self.api.control_transfer)(
                    entry.winusb,
                    packet,
                    buf_ptr,
                    buffer.len() as u32,
                    std::ptr::null_mut(),
                    &mut overlapped,
                )
            };
            let moved = if ok != 0 {
                self.complete_overlapped(entry, &mut overlapped)
            } else {
                let err = unsafe { GetLastError().0 };
                if err == ERROR_IO_PENDING.0 {
                    self.wait_overlapped(entry, &mut overlapped, Some(CONTROL_TIMEOUT), cancelled)
                } else {
                    Err(win32_error_to_errno(err))
                }
            };
            match moved {
                Ok(n) if is_in => {
                    buffer.truncate((n as usize).min(buffer.len()));
                    Reply::read(buffer)
                }
                Ok(n) => Reply::sent(n),
                Err(status) => Reply::failed(status),
            }
        }

        fn pipe_transfer(
            &self,
            entry: InterfaceEntry,
            address: u8,
            max_packet: u16,
            transfer: &Transfer,
            cancelled: &AtomicBool,
        ) -> Reply {
            let submit = |buf: *mut u8, len: u32, is_in: bool| -> Result<u32, i32> {
                let event = PooledEvent::acquire(&self.event_pool)?;
                let mut overlapped = OVERLAPPED { hEvent: event.handle(), ..Default::default() };
                let call = if is_in { self.api.read_pipe } else { self.api.write_pipe };
                // SAFETY: caller keeps `buf` alive until overlapped completion returns.
                // Per WinUSB docs, `LengthTransferred` must be NULL when `Overlapped` is non-NULL.
                let ok = unsafe {
                    call(entry.winusb, address, buf, len, std::ptr::null_mut(), &mut overlapped)
                };
                if ok != 0 {
                    self.complete_overlapped(entry, &mut overlapped)
                } else {
                    let err = unsafe { GetLastError().0 };
                    if err == ERROR_IO_PENDING.0 {
                        self.wait_overlapped(entry, &mut overlapped, None, cancelled)
                    } else {
                        Err(win32_error_to_errno(err))
                    }
                }
            };

            match transfer.direction {
                Direction::In => {
                    let mut buffer = vec![0u8; transfer.length as usize];
                    let ptr = if buffer.is_empty() { std::ptr::null_mut() } else { buffer.as_mut_ptr() };
                    match submit(ptr, buffer.len() as u32, true) {
                        Ok(n) => {
                            buffer.truncate((n as usize).min(buffer.len()));
                            Reply::read(buffer)
                        }
                        Err(status) => Reply::failed(status),
                    }
                }
                Direction::Out => {
                    let data = &transfer.data;
                    let ptr = if data.is_empty() { std::ptr::null_mut() } else { data.as_ptr().cast_mut() };
                    let sent = match submit(ptr, data.len() as u32, false) {
                        Ok(n) => n,
                        Err(status) => return Reply::failed(status),
                    };
                    let zero = transfer.flags & URB_ZERO_PACKET != 0
                        && !data.is_empty()
                        && max_packet > 0
                        && data.len().is_multiple_of(usize::from(max_packet));
                    if zero
                        && let Err(status) = submit(std::ptr::null_mut(), 0, false)
                    {
                        return Reply::failed(status);
                    }
                    Reply::sent(sent)
                }
            }
        }

        fn isochronous(&self, address: u8, transfer: &Transfer, cancelled: &AtomicBool) -> Reply {
            let total = match iso_plan(transfer) {
                Ok(total) => total,
                Err(status) => return Reply::failed(status),
            };
            let Some((entry, ep_info)) = self.endpoint_entry(address) else {
                return Reply::failed(wire::EPIPE);
            };
            if transfer.direction == Direction::Out
                && let Err(status) = validate_winusb_iso_out_table(&transfer.iso, ep_info.max_bytes_per_interval)
            {
                return Reply::failed(status);
            }
            let mut slot = match acquire_iso_slot(
                self.api,
                entry.winusb,
                &self.iso_pools,
                ep_info.interface,
                address,
                total,
            ) {
                Ok(slot) => slot,
                Err(status) => return Reply::failed(status),
            };
            if transfer.direction == Direction::Out {
                slot.buffer[..total].copy_from_slice(&transfer.data[..total]);
            }
            let ep_stream = self.iso_stream(address);

            let reply = match transfer.direction {
                Direction::In => {
                    let mut descs = vec![UsbdIsoPacketDescriptor::default(); transfer.iso.len()];
                    let result = self.submit_iso(entry, &ep_stream, cancelled, |try_cont, ov| unsafe {
                        (self.api.read_isoch_pipe_asap)(
                            slot.handle,
                            0,
                            total as u32,
                            WinBool::from(try_cont),
                            descs.len() as u32,
                            descs.as_mut_ptr(),
                            ov,
                        )
                    });
                    match result {
                        Ok(_) => pack_winusb_iso_in(transfer.length, &transfer.iso, &descs, &slot.buffer[..total]),
                        Err(status) => Reply::failed(status),
                    }
                }
                Direction::Out => {
                    let result = self.submit_iso(entry, &ep_stream, cancelled, |try_cont, ov| unsafe {
                        (self.api.write_isoch_pipe_asap)(
                            slot.handle,
                            0,
                            total as u32,
                            WinBool::from(try_cont),
                            ov,
                        )
                    });
                    match result {
                        Ok(transferred) => pack_winusb_iso_out(transfer.length, &transfer.iso, transferred),
                        Err(status) => Reply::failed(status),
                    }
                }
            };
            release_iso_slot(&self.closed, &self.resettable, &self.iso_pools, address, slot);
            reply
        }

        fn submit_iso(
            &self,
            entry: InterfaceEntry,
            ep_stream: &Arc<Mutex<IsoEndpointState>>,
            cancelled: &AtomicBool,
            mut issue: impl FnMut(bool, *mut OVERLAPPED) -> WinBool,
        ) -> Result<u32, i32> {
            let event = PooledEvent::acquire(&self.event_pool)?;
            let mut overlapped = OVERLAPPED { hEvent: event.handle(), ..Default::default() };
            let (immediate, ticket) = submit_pipelined(ep_stream, |try_continue| {
                // SAFETY: `overlapped` stays pinned across submission and wait below.
                if issue(try_continue, &mut overlapped) != 0 {
                    Ok(true)
                } else {
                    let err = unsafe { GetLastError().0 };
                    if err == ERROR_IO_PENDING.0 {
                        Ok(false)
                    } else {
                        Err((err, win32_error_to_errno(err)))
                    }
                }
            })?;
            let result = if immediate {
                self.complete_overlapped(entry, &mut overlapped)
            } else {
                self.wait_overlapped(entry, &mut overlapped, Some(ISO_TIMEOUT), cancelled)
            };
            ep_stream
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .finish(ticket, result.is_ok());
            result
        }
    }

    impl Drop for WinUsbDevice {
        fn drop(&mut self) {
            self.closed.store(true, Ordering::Release);
            self.invalidate_iso_pools(None);
            self.state().handles.close_all(self.api);
        }
    }

    impl BlockingDevice for WinUsbDevice {
        fn describe(&self) -> wire::Device {
            wire::Device {
                configuration: self.state().configuration,
                ..self.described.clone()
            }
        }

        fn class(&self) -> u8 {
            self.class
        }

        fn transfer(&self, transfer: &Transfer, cancelled: &AtomicBool) -> Reply {
            if self.closed.load(Ordering::Acquire) {
                return Reply::failed(wire::ESHUTDOWN);
            }
            if transfer.is_control() {
                return self.control(transfer, cancelled);
            }
            let address = transfer.endpoint | if transfer.direction == Direction::In { 0x80 } else { 0 };
            if transfer.is_isochronous() {
                return self.isochronous(address, transfer, cancelled);
            }
            let Some((entry, info)) = self.endpoint_entry(address) else {
                return Reply::failed(wire::EPIPE);
            };
            self.pipe_transfer(entry, address, info.max_packet, transfer, cancelled)
        }

        fn close(&self) {
            self.closed.store(true, Ordering::Release);
            self.invalidate_iso_pools(None);
            self.state().handles.close_all(self.api);
        }
    }

    fn speed(speed: rusb::Speed) -> u32 {
        match speed {
            rusb::Speed::Low => 1,
            rusb::Speed::Full => 2,
            rusb::Speed::High => 3,
            rusb::Speed::Super => 5,
            rusb::Speed::SuperPlus => 6,
            _ => 0,
        }
    }

    /// Opens a Windows `WinUSB` device (and all its associated/composite interfaces) directly via
    /// inbox `winusb.dll`, without opening a conflicting libusb handle.
    pub(in crate::usb) fn open_device(
        device: &rusb::Device<rusb::Context>,
        busid: &str,
        vid: u16,
        pid: u16,
        paths: &[String],
    ) -> Result<Arc<dyn UsbDevice>, String> {
        let api = api()?;
        if paths.is_empty() {
            return Err("Windows listed no WinUSB interface path for this device.".to_owned());
        }
        let descriptor = device
            .device_descriptor()
            .map_err(|e| format!("The device's USB descriptor could not be read: {e}"))?;
        let config = device
            .active_config_descriptor()
            .map_err(|e| format!("The device's active USB configuration could not be read: {e}"))?;

        let handles = Handles::open_paths(api, paths)?;

        let mut active_alts = HashMap::new();
        for (&num, &entry) in &handles.interfaces {
            let mut current_alt = 0u8;
            if unsafe { (api.get_alt)(entry.winusb, &mut current_alt) } == 0 {
                current_alt = 0;
            }
            active_alts.insert(num, current_alt);
        }

        let mut interfaces = Vec::new();
        let mut endpoints = HashMap::new();
        let mut alt_endpoints: super::iso_coord::AltEndpointsMap = HashMap::new();
        for interface in config.interfaces() {
            let num = interface.number();
            interfaces.push(num);
            let entry = handles.interfaces.get(&num).copied();
            let active_alt = active_alts.get(&num).copied().unwrap_or(0);
            for setting in interface.descriptors() {
                let alt = setting.setting_number();
                let mut pipe_ex_map = HashMap::new();
                if let Some(entry) = entry {
                    for pipe_idx in 0..setting.num_endpoints() {
                        let mut pipe_info = WinUsbPipeInformationEx::default();
                        if unsafe { (api.query_pipe_ex)(entry.winusb, alt, pipe_idx, &mut pipe_info) } != 0 {
                            pipe_ex_map.insert(pipe_info.pipe_id, pipe_info);
                        }
                    }
                }
                let mut list = Vec::new();
                for ep in setting.endpoint_descriptors() {
                    let addr = ep.address();
                    let max_packet = ep.max_packet_size() & 0x7ff;
                    let max_bytes_per_interval = if let Some(info) = pipe_ex_map.get(&addr) {
                        if info.pipe_type == USBD_PIPE_TYPE_ISOCHRONOUS {
                            info.maximum_bytes_per_interval
                        } else {
                            u32::from(max_packet)
                        }
                    } else if ep.transfer_type() == rusb::TransferType::Isochronous {
                        0
                    } else {
                        u32::from(max_packet)
                    };
                    list.push((addr, max_packet, max_bytes_per_interval));
                    if alt == active_alt {
                        endpoints.insert(addr, EndpointInfo { interface: num, max_packet, max_bytes_per_interval });
                    }
                }
                alt_endpoints.insert((num, alt), list);
            }
        }

        let class = match descriptor.class_code() {
            0 => config
                .interfaces()
                .next()
                .and_then(|i| i.descriptors().next())
                .map_or(0, |s| s.class_code()),
            class => class,
        };
        let version = descriptor.device_version();
        let bcd_device = (u16::from(version.0 / 10) << 12)
            | (u16::from(version.0 % 10) << 8)
            | (u16::from(version.1) << 4)
            | u16::from(version.2);
        let configuration = config.number();
        let described = wire::Device {
            path: format!("/winusb/{busid}"),
            busid: busid.to_owned(),
            busnum: u32::from(device.bus_number()),
            devnum: u32::from(device.address()),
            speed: speed(device.speed()),
            vendor: vid,
            product: pid,
            bcd_device,
            class: descriptor.class_code(),
            subclass: descriptor.sub_class_code(),
            protocol: descriptor.protocol_code(),
            configuration,
            configurations: descriptor.num_configurations(),
            interfaces: config.num_interfaces(),
        };

        let device = WinUsbDevice {
            api,
            described,
            class,
            closed: AtomicBool::new(false),
            state: Mutex::new(State {
                configuration,
                active: configuration,
                handles,
            }),
            resettable: Arc::new(Mutex::new(ResettableState {
                interfaces,
                endpoints,
                alt_endpoints,
            })),
            iso_streams: Mutex::new(HashMap::new()),
            iso_pools: Mutex::new(IsoPoolState::default()),
            event_pool: Arc::new(Mutex::new(Vec::new())),
        };
        Ok(Arc::new(Blocking::new(device)))
    }
}

#[cfg(windows)]
pub(super) use platform::open_device;
