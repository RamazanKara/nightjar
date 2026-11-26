//! JNI bridge for `SharedFolders` (`Java_com_limelight_nvstream_jni_NightjarHttpJni_folder*`).
//!
//! Bridges `nightjar_core::http::folders` to Android SAF `InputStream` / `OutputStream` /
//! `Progress` / `TransferCancel` without requiring detachable file descriptors. Enforces exact
//! upload length (checking for extra trailing bytes before sending the final chunk) and maps
//! local stream I/O errors to `local_file` (`SharedFolders.LocalFileException`).

use std::io::{Read, Write};

use jni::JNIEnv;
use jni::objects::{GlobalRef, JClass, JObject, JString, JValue};
use jni::sys::{jint, jlong, jstring};
use nightjar_core::http::{EntryKind, Error, FileError, FileRefusal, TransferCancel};
use serde_json::{Value, json};

use crate::bridge::{byte_array, jbytes};
use crate::http::{CANCELS, CLIENTS, error_json, new_jstring, ok_json};
use crate::native_api;

fn refusal_name(refusal: FileRefusal) -> &'static str {
    match refusal {
        FileRefusal::Forbidden => "FORBIDDEN",
        FileRefusal::FileTransferDisabled => "FILE_TRANSFER_DISABLED",
        FileRefusal::InvalidName => "INVALID_NAME",
        FileRefusal::IncompleteUpload => "INCOMPLETE_UPLOAD",
        FileRefusal::LengthRequired => "LENGTH_REQUIRED",
        FileRefusal::TooLarge => "TOO_LARGE",
        FileRefusal::InsufficientStorage => "INSUFFICIENT_STORAGE",
        FileRefusal::NoConsoleUser => "NO_CONSOLE_USER",
        FileRefusal::TransferBusy => "TRANSFER_BUSY",
        FileRefusal::NotFound => "NOT_FOUND",
        FileRefusal::FoldersDisabled => "FOLDERS_DISABLED",
        FileRefusal::FolderUnreadable => "FOLDER_UNREADABLE",
    }
}

fn folder_error_json(error: FileError) -> String {
    match error {
        FileError::Local(e) => json!({
            "error": {"kind": "local_file", "message": e.to_string()}
        })
        .to_string(),
        FileError::Refused {
            refusal,
            status,
            message,
        } => json!({
            "error": {
                "kind": "refused",
                "refusal": refusal_name(refusal),
                "code": refusal.code(),
                "http_status": status,
                "message": message,
            }
        })
        .to_string(),
        FileError::Host(Error::Cancelled) => json!({
            "error": {"kind": "cancelled", "message": "Transfer cancelled"}
        })
        .to_string(),
        FileError::Host(e) => error_json(&e),
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarHttpJni_folderList(
    env: JNIEnv,
    _: JClass,
    handle: jlong,
    _caps_json: JString,
) -> jstring {
    let reply = match CLIENTS.get(handle) {
        Some(client) => match client.shared_folders() {
            Ok(folders) => {
                let items: Vec<Value> = folders
                    .into_iter()
                    .map(|f| {
                        let writable = f.writable();
                        json!({
                            "id": f.id,
                            "name": f.name,
                            "writable": writable,
                        })
                    })
                    .collect();
                ok_json(items)
            }
            Err(e) => folder_error_json(e),
        },
        None => error_json(&Error::InvalidArgument("unknown HTTP client handle".into())),
    };
    new_jstring(&env, &reply)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarHttpJni_folderEntries(
    mut env: JNIEnv,
    _: JClass,
    handle: jlong,
    _caps_json: JString,
    folder: JString,
    path: JString,
) -> jstring {
    let folder = native_api::string(&mut env, &folder).unwrap_or_default();
    let path = native_api::string(&mut env, &path).unwrap_or_default();
    let reply = match CLIENTS.get(handle) {
        Some(client) => match client.folder_entries(&folder, &path) {
            Ok(listing) => {
                let entries: Vec<Value> = listing
                    .entries
                    .into_iter()
                    .map(|e| {
                        json!({
                            "name": e.name,
                            "kind": match e.kind {
                                EntryKind::Dir => "DIRECTORY",
                                EntryKind::File => "FILE",
                                EntryKind::Unknown => "UNKNOWN",
                            },
                            "size": e.size,
                            "modified_unix_ms": e.modified_unix_ms,
                        })
                    })
                    .collect();
                ok_json(json!({
                    "path": listing.path,
                    "entries": entries,
                }))
            }
            Err(e) => folder_error_json(e),
        },
        None => error_json(&Error::InvalidArgument("unknown HTTP client handle".into())),
    };
    new_jstring(&env, &reply)
}

fn next_array_capacity(current_capacity: usize, needed: usize) -> Option<usize> {
    if current_capacity >= needed && current_capacity > 0 {
        None
    } else {
        Some(needed.max(1).min(i32::MAX as usize))
    }
}

fn ensure_cached_byte_array<'a>(
    env: &mut JNIEnv,
    slot: &'a mut Option<(GlobalRef, usize)>,
    needed: usize,
) -> Result<&'a GlobalRef, jni::errors::Error> {
    let current = slot.as_ref().map_or(0, |(_, cap)| *cap);
    if let Some(cap) = next_array_capacity(current, needed) {
        let len = i32::try_from(cap).unwrap_or(i32::MAX);
        let arr = env.new_byte_array(len)?;
        *slot = Some((env.new_global_ref(arr)?, len as usize));
    }
    Ok(&slot.as_ref().expect("allocated above").0)
}

struct JniDownloadSink<'local, 'env, 'obj> {
    env: &'env mut JNIEnv<'local>,
    sink: &'obj JObject<'local>,
    buf_array: Option<(GlobalRef, usize)>,
}

impl Write for JniDownloadSink<'_, '_, '_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let Self {
            env,
            sink,
            buf_array,
        } = self;
        let len = i32::try_from(buf.len()).unwrap_or(i32::MAX);
        let write_slice = &buf[..len as usize];
        let res = env.with_local_frame(4, |env| {
            let global = ensure_cached_byte_array(env, buf_array, write_slice.len())?;
            env.set_byte_array_region(byte_array(global), 0, jbytes(write_slice))?;
            env.call_method(
                *sink,
                "write",
                "([BII)V",
                &[
                    JValue::Object(global.as_obj()),
                    JValue::Int(0),
                    JValue::Int(len),
                ],
            )?;
            Ok::<(), jni::errors::Error>(())
        });
        if res.is_err() {
            if self.env.exception_check().unwrap_or(false) {
                let _ = self.env.exception_clear();
            }
            return Err(std::io::Error::other("Destination write failed"));
        }
        Ok(write_slice.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

struct JniUploadSource<'local, 'env, 'obj> {
    env: &'env mut JNIEnv<'local>,
    source: &'obj JObject<'local>,
    remaining: u64,
    checked_eof: bool,
    buf_array: Option<(GlobalRef, usize)>,
}

fn validate_java_read_len(max_len: usize, n: jint) -> Result<Option<usize>, jni::errors::Error> {
    if n <= 0 {
        return Ok(None);
    }
    let len =
        usize::try_from(n)
            .ok()
            .filter(|&n| n <= max_len)
            .ok_or(jni::errors::Error::JniCall(
                jni::errors::JniError::InvalidArguments,
            ))?;
    Ok(Some(len))
}

impl JniUploadSource<'_, '_, '_> {
    fn read_java_into(&mut self, dst: &mut [u8]) -> std::io::Result<Option<usize>> {
        if dst.is_empty() {
            return Ok(None);
        }
        let Self {
            env,
            source,
            buf_array,
            ..
        } = self;
        let ask = i32::try_from(dst.len()).unwrap_or(i32::MAX);
        let dst = &mut dst[..ask as usize];
        let res = env.with_local_frame(4, |env| {
            let global = ensure_cached_byte_array(env, buf_array, dst.len())?;
            let raw_n = env
                .call_method(
                    *source,
                    "read",
                    "([BII)I",
                    &[
                        JValue::Object(global.as_obj()),
                        JValue::Int(0),
                        JValue::Int(ask),
                    ],
                )?
                .i()?;
            let Some(n) = validate_java_read_len(dst.len(), raw_n)? else {
                return Ok::<Option<usize>, jni::errors::Error>(None);
            };
            // SAFETY: `i8` and `u8` have identical layout and alignment, and `&mut dst[..n]`
            // is uniquely borrowed for the duration of `get_byte_array_region`.
            let out_i8 =
                unsafe { std::slice::from_raw_parts_mut(dst[..n].as_mut_ptr().cast::<i8>(), n) };
            env.get_byte_array_region(byte_array(global), 0, out_i8)?;
            Ok(Some(n))
        });
        match res {
            Ok(opt) => Ok(opt),
            Err(_) => {
                if self.env.exception_check().unwrap_or(false) {
                    let _ = self.env.exception_clear();
                }
                Err(std::io::Error::other("Source read failed"))
            }
        }
    }
}

impl Read for JniUploadSource<'_, '_, '_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if buf.is_empty() || self.checked_eof {
            return Ok(0);
        }
        if self.remaining == 0 {
            self.checked_eof = true;
            let mut probe = [0u8; 1];
            if self.read_java_into(&mut probe)?.is_some() {
                return Err(std::io::Error::other("File size changed during the upload"));
            }
            return Ok(0);
        }
        let ask = usize::try_from(self.remaining)
            .unwrap_or(usize::MAX)
            .min(buf.len());
        let Some(n) = self.read_java_into(&mut buf[..ask])? else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "Source stream ended before declared length",
            ));
        };
        self.remaining -= n as u64;
        if self.remaining == 0 {
            // Check for unexpected extra bytes BEFORE handing back the final chunk, so the last
            // bytes never cross the socket if the file grew.
            self.checked_eof = true;
            let mut probe = [0u8; 1];
            if self.read_java_into(&mut probe)?.is_some() {
                return Err(std::io::Error::other("File size changed during the upload"));
            }
        }
        Ok(n)
    }
}

pub(crate) fn call_folder_progress(env: &mut JNIEnv, progress: &JObject, done: u64, total: Option<u64>) {
    if progress.is_null() {
        return;
    }
    let total_jlong = total
        .map(|t| i64::try_from(t).unwrap_or(i64::MAX))
        .unwrap_or(-1);
    let called = env.with_local_frame(4, |env| {
        env.call_method(
            progress,
            "onProgress",
            "(JJ)V",
            &[
                JValue::Long(i64::try_from(done).unwrap_or(i64::MAX)),
                JValue::Long(total_jlong),
            ],
        )
        .map(|_| ())
    });
    if called.is_err() && env.exception_check().unwrap_or(false) {
        let _ = env.exception_clear();
    }
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarHttpJni_folderDownload(
    env: JNIEnv,
    _: JClass,
    handle: jlong,
    caps_json: JString,
    folder: JString,
    path: JString,
    listed: jlong,
    sink_obj: JObject,
    progress_obj: JObject,
    cancel_handle: jlong,
) -> jstring {
    folder_download_impl(
        env,
        handle,
        caps_json,
        folder,
        path,
        listed,
        sink_obj,
        progress_obj,
        cancel_handle,
    )
}

#[allow(clippy::too_many_arguments)]
fn folder_download_impl<'local>(
    mut env: JNIEnv<'local>,
    handle: jlong,
    _caps_json: JString<'local>,
    folder: JString<'local>,
    path: JString<'local>,
    listed: jlong,
    sink_obj: JObject<'local>,
    progress_obj: JObject<'local>,
    cancel_handle: jlong,
) -> jstring {
    let folder = native_api::string(&mut env, &folder).unwrap_or_default();
    let path = native_api::string(&mut env, &path).unwrap_or_default();
    let listed_opt = u64::try_from(listed).ok();
    let cancel = CANCELS
        .get(cancel_handle)
        .unwrap_or_else(|| std::sync::Arc::new(TransferCancel::new()));
    let reply = match CLIENTS.get(handle) {
        Some(client) => {
            let vm = env.get_java_vm().ok();
            let mut sink = JniDownloadSink {
                env: &mut env,
                sink: &sink_obj,
                buf_array: None,
            };
            let mut on_progress = |done: u64, total: Option<u64>| {
                if let Some(ref vm) = vm
                    && let Ok(mut guard) = vm.get_env()
                {
                    call_folder_progress(&mut guard, &progress_obj, done, total);
                }
            };
            match client.download_folder_file_stream(
                &folder,
                &path,
                listed_opt,
                &mut sink,
                &mut on_progress,
                &cancel,
            ) {
                Ok(written) => ok_json(written),
                Err(e) => folder_error_json(e),
            }
        }
        None => error_json(&Error::InvalidArgument("unknown HTTP client handle".into())),
    };
    new_jstring(&env, &reply)
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarHttpJni_folderUpload(
    env: JNIEnv,
    _: JClass,
    handle: jlong,
    caps_json: JString,
    folder: JString,
    path: JString,
    source_obj: JObject,
    length: jlong,
    progress_obj: JObject,
    cancel_handle: jlong,
) -> jstring {
    folder_upload_impl(
        env,
        handle,
        caps_json,
        folder,
        path,
        source_obj,
        length,
        progress_obj,
        cancel_handle,
    )
}

#[allow(clippy::too_many_arguments)]
fn folder_upload_impl<'local>(
    mut env: JNIEnv<'local>,
    handle: jlong,
    _caps_json: JString<'local>,
    folder: JString<'local>,
    path: JString<'local>,
    source_obj: JObject<'local>,
    length: jlong,
    progress_obj: JObject<'local>,
    cancel_handle: jlong,
) -> jstring {
    let folder = native_api::string(&mut env, &folder).unwrap_or_default();
    let path = native_api::string(&mut env, &path).unwrap_or_default();
    let Ok(len) = u64::try_from(length) else {
        return new_jstring(
            &env,
            &folder_error_json(FileError::Local(std::io::Error::other(
                "Negative upload length",
            ))),
        );
    };
    let cancel = CANCELS
        .get(cancel_handle)
        .unwrap_or_else(|| std::sync::Arc::new(TransferCancel::new()));
    let reply = match CLIENTS.get(handle) {
        Some(client) => {
            let vm = env.get_java_vm().ok();
            let mut source = JniUploadSource {
                env: &mut env,
                source: &source_obj,
                remaining: len,
                checked_eof: false,
                buf_array: None,
            };
            if len == 0 {
                let mut probe = [0u8; 1];
                match source.read_java_into(&mut probe) {
                    Ok(Some(_)) => {
                        return new_jstring(
                            &env,
                            &folder_error_json(FileError::Local(std::io::Error::other(
                                "File size changed during the upload",
                            ))),
                        );
                    }
                    Ok(None) => {}
                    Err(e) => {
                        return new_jstring(&env, &folder_error_json(FileError::Local(e)));
                    }
                }
            }
            let mut on_progress = |done: u64, total: u64| {
                if let Some(ref vm) = vm
                    && let Ok(mut guard) = vm.get_env()
                {
                    call_folder_progress(&mut guard, &progress_obj, done, Some(total));
                }
            };
            match client.upload_folder_file(
                &folder,
                &path,
                &mut source,
                len,
                &mut on_progress,
                &cancel,
            ) {
                Ok(uploaded) => ok_json(uploaded.name),
                Err(e) => folder_error_json(e),
            }
        }
        None => error_json(&Error::InvalidArgument("unknown HTTP client handle".into())),
    };
    new_jstring(&env, &reply)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_java_read_len_guards_bounds() {
        assert_eq!(validate_java_read_len(3, 0).unwrap(), None);
        assert_eq!(validate_java_read_len(3, -1).unwrap(), None);
        assert_eq!(validate_java_read_len(4, 3).unwrap(), Some(3));
        assert_eq!(validate_java_read_len(4, 4).unwrap(), Some(4));
        assert!(validate_java_read_len(4, 5).is_err());
        assert!(validate_java_read_len(2, i32::MAX).is_err());
    }

    #[test]
    fn next_array_capacity_reuses_until_larger_needed() {
        assert_eq!(next_array_capacity(0, 0), Some(1));
        assert_eq!(next_array_capacity(0, 4096), Some(4096));
        assert_eq!(next_array_capacity(4096, 1024), None);
        assert_eq!(next_array_capacity(4096, 4096), None);
        assert_eq!(next_array_capacity(4096, 8192), Some(8192));
    }
}
