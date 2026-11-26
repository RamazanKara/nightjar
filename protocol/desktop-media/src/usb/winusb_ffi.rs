//! WinUSB FFI bindings (`winusb.dll`), USBD status mapping, and iso packet packing.
//!
//! Dynamically loads Microsoft's inbox `winusb.dll` on Windows (8.1/10/11) and provides pure
//! packet/status helpers unit-tested on every platform.

#![cfg_attr(not(windows), allow(dead_code))]

use nightjar_core::usb::{wire, Direction, IsoPacket, Reply};

use super::super::iso::{pack_iso_reply, CompletedPacket};

/// `USBD_ISO_PACKET_DESCRIPTOR` from Windows `usb.h`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::usb) struct UsbdIsoPacketDescriptor {
    pub(in crate::usb) offset: u32,
    pub(in crate::usb) length: u32,
    pub(in crate::usb) status: u32,
}

/// `WINUSB_SETUP_PACKET` from Windows `winusb.h` (1-byte packed).
#[cfg(windows)]
#[repr(C, packed)]
#[derive(Clone, Copy, Debug)]
pub(super) struct WinUsbSetupPacket {
    pub(super) request_type: u8,
    pub(super) request: u8,
    pub(super) value: u16,
    pub(super) index: u16,
    pub(super) length: u16,
}

/// `USB_INTERFACE_DESCRIPTOR` from `usb100.h` (9 bytes, 1-byte packed).
#[cfg(windows)]
#[repr(C, packed)]
#[derive(Clone, Copy, Default)]
pub(super) struct UsbInterfaceDescriptor {
    pub(super) b_length: u8,
    pub(super) b_descriptor_type: u8,
    pub(super) b_interface_number: u8,
    pub(super) b_alternate_setting: u8,
    pub(super) b_num_endpoints: u8,
    pub(super) b_interface_class: u8,
    pub(super) b_interface_sub_class: u8,
    pub(super) b_interface_protocol: u8,
    pub(super) i_interface: u8,
}

/// `WINUSB_PIPE_INFORMATION_EX` from Windows `winusbio.h`.
#[cfg(windows)]
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct WinUsbPipeInformationEx {
    pub(super) pipe_type: u32,
    pub(super) pipe_id: u8,
    pub(super) maximum_packet_size: u16,
    pub(super) interval: u8,
    pub(super) maximum_bytes_per_interval: u32,
}

#[cfg(windows)]
const _: () = assert!(std::mem::size_of::<WinUsbPipeInformationEx>() == 16);

#[cfg(windows)]
pub(super) const USBD_PIPE_TYPE_ISOCHRONOUS: u32 = 1;

const USBD_STATUS_SUCCESS: u32 = 0x0000_0000;
const USBD_STATUS_STALL_PID: u32 = 0xC000_0004;
const USBD_STATUS_DATA_OVERRUN: u32 = 0xC000_0008;
const USBD_STATUS_BUFFER_OVERRUN: u32 = 0xC000_000C;
const USBD_STATUS_TIMEOUT: u32 = 0xC000_6000;
const USBD_STATUS_DEVICE_GONE: u32 = 0xC000_7000;
const USBD_STATUS_CANCELED: u32 = 0xC001_0000;

/// Maps a Windows `USBD_STATUS` value from `USBD_ISO_PACKET_DESCRIPTOR` to USB/IP's negated errno.
pub(in crate::usb) fn usbd_status_to_errno(status: u32) -> i32 {
    match status {
        USBD_STATUS_SUCCESS => 0,
        USBD_STATUS_STALL_PID => wire::EPIPE,
        USBD_STATUS_DATA_OVERRUN | USBD_STATUS_BUFFER_OVERRUN => wire::EOVERFLOW,
        USBD_STATUS_TIMEOUT => wire::ETIMEDOUT,
        USBD_STATUS_DEVICE_GONE => wire::ENODEV,
        USBD_STATUS_CANCELED => wire::ECONNRESET,
        _ => wire::EPROTO,
    }
}

/// Maps a Win32 error code from WinUSB / overlapped I/O to USB/IP's negated errno.
pub(in crate::usb) fn win32_error_to_errno(error: u32) -> i32 {
    match error {
        // ERROR_OPERATION_ABORTED
        995 => wire::ECONNRESET,
        // ERROR_SEM_TIMEOUT | ERROR_TIMEOUT
        121 | 1460 => wire::ETIMEDOUT,
        // ERROR_FILE_NOT_FOUND | ERROR_BAD_COMMAND | ERROR_NO_SUCH_DEVICE | ERROR_DEVICE_NOT_CONNECTED
        2 | 22 | 433 | 1167 => wire::ENODEV,
        // ERROR_BUFFER_OVERFLOW | ERROR_MORE_DATA
        111 | 234 => wire::EOVERFLOW,
        // ERROR_GEN_FAILURE (WinUSB reports endpoint STALL as ERROR_GEN_FAILURE)
        31 => wire::EPIPE,
        // ERROR_INVALID_PARAMETER
        87 => wire::EINVAL,
        _ => wire::EPROTO,
    }
}

/// Maps a Win32 error from overlapped completion to USB/IP errno, preserving cancellation.
pub(in crate::usb) fn overlapped_error_to_errno(error: u32, cancel_reason: Option<i32>) -> i32 {
    if error == 995 {
        cancel_reason.unwrap_or(wire::ECONNRESET)
    } else if cancel_reason == Some(wire::ECONNRESET) {
        wire::ECONNRESET
    } else {
        win32_error_to_errno(error)
    }
}

/// Validates an isochronous OUT packet table before calling `WinUsb_WriteIsochPipeAsap`.
///
/// `WinUsb_WriteIsochPipeAsap` only accepts a flat contiguous byte length and splits it on the wire
/// into `max_bytes_per_interval` chunks with at most one trailing shorter packet. Any zero-length,
/// non-contiguous, oversized, or interior short packet cannot be expressed accurately and is
/// refused with `wire::EINVAL`.
pub(in crate::usb) fn validate_winusb_iso_out_table(
    asked: &[IsoPacket],
    max_bytes_per_interval: u32,
) -> Result<u32, i32> {
    if asked.is_empty() || max_bytes_per_interval == 0 {
        return Err(wire::EINVAL);
    }
    let mut expected_offset = 0u32;
    let last_idx = asked.len() - 1;
    for (idx, packet) in asked.iter().enumerate() {
        if packet.offset != expected_offset
            || packet.length == 0
            || packet.length > max_bytes_per_interval
        {
            return Err(wire::EINVAL);
        }
        if idx < last_idx && packet.length != max_bytes_per_interval {
            return Err(wire::EINVAL);
        }
        expected_offset = expected_offset
            .checked_add(packet.length)
            .ok_or(wire::EINVAL)?;
    }
    Ok(expected_offset)
}

/// Target interface number if a control setup packet has recipient = Interface (`bmRequestType & 0x1f == 1`).
pub(in crate::usb) fn control_target_interface(setup: &[u8; 8]) -> Option<u8> {
    (setup[0] & 0x1f == 1).then_some(setup[4])
}

/// Target endpoint address if a control setup packet has recipient = Endpoint (`bmRequestType & 0x1f == 2`).
pub(in crate::usb) fn control_target_endpoint(setup: &[u8; 8]) -> Option<u8> {
    (setup[0] & 0x1f == 2).then_some(setup[4])
}

/// Packs completed WinUSB IN isochronous descriptors and transfer buffer into a USB/IP [`Reply`].
pub(in crate::usb) fn pack_winusb_iso_in(
    max_total: u32,
    asked: &[IsoPacket],
    descriptors: &[UsbdIsoPacketDescriptor],
    buffer: &[u8],
) -> Reply {
    let completed: Vec<CompletedPacket> = descriptors
        .iter()
        .map(|d| CompletedPacket {
            offset: d.offset as usize,
            actual_length: d.length,
            status: usbd_status_to_errno(d.status),
        })
        .collect();
    pack_iso_reply(Direction::In, max_total, asked, &completed, buffer)
}

/// Packs a completed WinUSB OUT isochronous transfer (`WinUsb_WriteIsochPipeAsap` reports only
/// total transferred bytes, not per-packet descriptors) into a USB/IP [`Reply`].
pub(in crate::usb) fn pack_winusb_iso_out(max_total: u32, asked: &[IsoPacket], transferred: u32) -> Reply {
    let mut remaining = transferred.min(max_total);
    let mut completed = Vec::with_capacity(asked.len());
    for packet in asked {
        let actual = remaining.min(packet.length);
        remaining -= actual;
        completed.push(CompletedPacket {
            offset: 0,
            actual_length: actual,
            status: 0,
        });
    }
    pack_iso_reply(Direction::Out, max_total, asked, &completed, &[])
}

#[cfg(windows)]
mod loader {
    use std::ffi::c_void;
    use std::sync::OnceLock;

    use nightjar_core::usb::wire;
    use windows::core::{s, w, PCWSTR};
    use windows::Win32::Foundation::{CloseHandle, FreeLibrary, HANDLE};
    use windows::Win32::System::IO::OVERLAPPED;
    use windows::Win32::System::LibraryLoader::{
        GetProcAddress, LoadLibraryExW, LOAD_LIBRARY_SEARCH_SYSTEM32,
    };
    use windows::Win32::System::Threading::CreateEventW;

    use super::{
        UsbInterfaceDescriptor, UsbdIsoPacketDescriptor, WinUsbPipeInformationEx, WinUsbSetupPacket,
    };

    pub(in crate::usb::winusb) type WinUsbHandle = *mut c_void;
    pub(in crate::usb::winusb) type IsochBufferHandle = *mut c_void;
    pub(in crate::usb::winusb) type WinBool = i32;

    type FnInitialize = unsafe extern "system" fn(HANDLE, *mut WinUsbHandle) -> WinBool;
    type FnFree = unsafe extern "system" fn(WinUsbHandle) -> WinBool;
    type FnGetAssociated = unsafe extern "system" fn(WinUsbHandle, u8, *mut WinUsbHandle) -> WinBool;
    type FnQuerySettings =
        unsafe extern "system" fn(WinUsbHandle, u8, *mut UsbInterfaceDescriptor) -> WinBool;
    type FnQueryPipeEx = unsafe extern "system" fn(
        WinUsbHandle,
        u8,
        u8,
        *mut WinUsbPipeInformationEx,
    ) -> WinBool;
    type FnGetAlt = unsafe extern "system" fn(WinUsbHandle, *mut u8) -> WinBool;
    type FnSetAlt = unsafe extern "system" fn(WinUsbHandle, u8) -> WinBool;
    type FnControl = unsafe extern "system" fn(
        WinUsbHandle,
        WinUsbSetupPacket,
        *mut u8,
        u32,
        *mut u32,
        *mut OVERLAPPED,
    ) -> WinBool;
    type FnPipeIo = unsafe extern "system" fn(
        WinUsbHandle,
        u8,
        *mut u8,
        u32,
        *mut u32,
        *mut OVERLAPPED,
    ) -> WinBool;
    type FnPipeOp = unsafe extern "system" fn(WinUsbHandle, u8) -> WinBool;
    type FnGetOverlapped =
        unsafe extern "system" fn(WinUsbHandle, *mut OVERLAPPED, *mut u32, WinBool) -> WinBool;
    type FnRegisterIsoch = unsafe extern "system" fn(
        WinUsbHandle,
        u8,
        *mut u8,
        u32,
        *mut IsochBufferHandle,
    ) -> WinBool;
    type FnUnregisterIsoch = unsafe extern "system" fn(IsochBufferHandle) -> WinBool;
    type FnReadIsochAsap = unsafe extern "system" fn(
        IsochBufferHandle,
        u32,
        u32,
        WinBool,
        u32,
        *mut UsbdIsoPacketDescriptor,
        *mut OVERLAPPED,
    ) -> WinBool;
    type FnWriteIsochAsap = unsafe extern "system" fn(
        IsochBufferHandle,
        u32,
        u32,
        WinBool,
        *mut OVERLAPPED,
    ) -> WinBool;

    pub(in crate::usb::winusb) struct WinUsbApi {
        pub(in crate::usb::winusb) initialize: FnInitialize,
        pub(in crate::usb::winusb) free: FnFree,
        pub(in crate::usb::winusb) get_associated: FnGetAssociated,
        pub(in crate::usb::winusb) query_settings: FnQuerySettings,
        pub(in crate::usb::winusb) query_pipe_ex: FnQueryPipeEx,
        pub(in crate::usb::winusb) get_alt: FnGetAlt,
        pub(in crate::usb::winusb) set_alt: FnSetAlt,
        pub(in crate::usb::winusb) control_transfer: FnControl,
        pub(in crate::usb::winusb) read_pipe: FnPipeIo,
        pub(in crate::usb::winusb) write_pipe: FnPipeIo,
        pub(in crate::usb::winusb) reset_pipe: FnPipeOp,
        pub(in crate::usb::winusb) abort_pipe: FnPipeOp,
        pub(in crate::usb::winusb) flush_pipe: FnPipeOp,
        pub(in crate::usb::winusb) get_overlapped_result: FnGetOverlapped,
        pub(in crate::usb::winusb) register_isoch_buffer: FnRegisterIsoch,
        pub(in crate::usb::winusb) unregister_isoch_buffer: FnUnregisterIsoch,
        pub(in crate::usb::winusb) read_isoch_pipe_asap: FnReadIsochAsap,
        pub(in crate::usb::winusb) write_isoch_pipe_asap: FnWriteIsochAsap,
    }

    // SAFETY: WinUSB DLL entry points are immutable function pointers for the process lifetime.
    unsafe impl Send for WinUsbApi {}
    unsafe impl Sync for WinUsbApi {}

    static API: OnceLock<Result<WinUsbApi, String>> = OnceLock::new();

    pub(in crate::usb::winusb) fn api() -> Result<&'static WinUsbApi, String> {
        API.get_or_init(|| unsafe {
            let module = LoadLibraryExW(w!("winusb.dll"), None, LOAD_LIBRARY_SEARCH_SYSTEM32)
                .map_err(|e| format!("Windows' inbox winusb.dll could not be loaded: {e}"))?;
            macro_rules! load {
                ($name:literal, $ty:ty) => {{
                    let Some(ptr) = GetProcAddress(module, s!($name)) else {
                        let _ = FreeLibrary(module);
                        return Err(format!(
                            "winusb.dll is missing {}; Windows 8.1 or newer is required",
                            $name
                        ));
                    };
                    std::mem::transmute::<unsafe extern "system" fn() -> isize, $ty>(ptr)
                }};
            }
            Ok(WinUsbApi {
                initialize: load!("WinUsb_Initialize", FnInitialize),
                free: load!("WinUsb_Free", FnFree),
                get_associated: load!("WinUsb_GetAssociatedInterface", FnGetAssociated),
                query_settings: load!("WinUsb_QueryInterfaceSettings", FnQuerySettings),
                query_pipe_ex: load!("WinUsb_QueryPipeEx", FnQueryPipeEx),
                get_alt: load!("WinUsb_GetCurrentAlternateSetting", FnGetAlt),
                set_alt: load!("WinUsb_SetCurrentAlternateSetting", FnSetAlt),
                control_transfer: load!("WinUsb_ControlTransfer", FnControl),
                read_pipe: load!("WinUsb_ReadPipe", FnPipeIo),
                write_pipe: load!("WinUsb_WritePipe", FnPipeIo),
                reset_pipe: load!("WinUsb_ResetPipe", FnPipeOp),
                abort_pipe: load!("WinUsb_AbortPipe", FnPipeOp),
                flush_pipe: load!("WinUsb_FlushPipe", FnPipeOp),
                get_overlapped_result: load!("WinUsb_GetOverlappedResult", FnGetOverlapped),
                register_isoch_buffer: load!("WinUsb_RegisterIsochBuffer", FnRegisterIsoch),
                unregister_isoch_buffer: load!("WinUsb_UnregisterIsochBuffer", FnUnregisterIsoch),
                read_isoch_pipe_asap: load!("WinUsb_ReadIsochPipeAsap", FnReadIsochAsap),
                write_isoch_pipe_asap: load!("WinUsb_WriteIsochPipeAsap", FnWriteIsochAsap),
            })
        })
        .as_ref()
        .map_err(Clone::clone)
    }

    pub(in crate::usb::winusb) struct OwnedEvent(pub(in crate::usb::winusb) HANDLE);

    // SAFETY: Win32 event handles created via `CreateEventW` are kernel objects valid across threads.
    unsafe impl Send for OwnedEvent {}
    // SAFETY: Reading the handle value concurrently to wait/reset/close under WinUsbDevice locks is thread-safe.
    unsafe impl Sync for OwnedEvent {}

    impl OwnedEvent {
        pub(in crate::usb::winusb) fn new() -> Result<Self, i32> {
            // SAFETY: manual-reset, initially unsignaled unnamed event owned by this guard.
            let handle = unsafe { CreateEventW(None, true, false, PCWSTR::null()) }
                .map_err(|_| wire::EPROTO)?;
            Ok(Self(handle))
        }
    }

    impl Drop for OwnedEvent {
        fn drop(&mut self) {
            // SAFETY: handle came from `CreateEventW` and is closed once on drop.
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }
}

#[cfg(windows)]
pub(super) use loader::{api, IsochBufferHandle, OwnedEvent, WinBool, WinUsbApi, WinUsbHandle};
