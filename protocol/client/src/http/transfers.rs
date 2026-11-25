//! File transfer local download safety (`download_into`) and `HostClient::download_to_path`.
//! Pure queue/state logic lives in `nightjar_policy::http::transfers`.

pub use nightjar_policy::http::transfers::{
    partial_path, upload_name, Direction, Problem, Start, State, Transfer, Transfers,
    MAX_NAME_UNITS, MAX_RUNNING_DOWNLOADS, MAX_RUNNING_UPLOADS,
};
use super::files::{FileError, TransferCancel};
use super::HostClient;
use std::io::Write;
use std::path::Path;

/// Runs `fetch` into a new file beside `dest` and moves it to `dest` only when `fetch` succeeded
/// and the file is on disk. A `<dest>.part` that is already there is not touched, and a file
/// that took `dest` during the download is not replaced: both fail the download. On any other
/// failure, a cancel included, the partial file is deleted (one that can't be is named in the
/// error) and `dest` is left as it was: a short file never carries the name.
pub fn download_into(
    dest: &Path,
    fetch: impl FnOnce(&mut dyn Write) -> Result<u64, FileError>,
) -> Result<u64, FileError> {
    let part = partial_path(dest);
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&part)
        .map_err(|e| {
            FileError::Local(std::io::Error::new(
                e.kind(),
                format!("{}: {e}", part.display()),
            ))
        })?;
    let result = fetch(&mut file).and_then(|size| {
        file.sync_all().map_err(FileError::Local)?;
        Ok(size)
    });
    drop(file);
    result
        .and_then(|size| {
            move_partial(&part, dest).map_err(FileError::Local)?;
            Ok(size)
        })
        .map_err(|error| discard_partial(&part, error))
}

/// Moves the finished download `part` to `dest`, which must still be free: a file that took
/// the name during the download is never replaced, and the move fails naming it.
pub(super) fn move_partial(part: &Path, dest: &Path) -> std::io::Result<()> {
    rename_new(part, dest).map_err(|e| match e.kind() {
        std::io::ErrorKind::AlreadyExists => std::io::Error::new(
            e.kind(),
            format!(
                "{} appeared during the download and was not replaced",
                dest.display()
            ),
        ),
        _ => e,
    })
}

/// Deletes the partial file of a failed download and returns `error`, or, when the file stays,
/// an error that names it beside `error`.
pub(super) fn discard_partial(part: &Path, error: FileError) -> FileError {
    match std::fs::remove_file(part) {
        Ok(()) => error,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => error,
        Err(e) => FileError::Local(std::io::Error::new(
            e.kind(),
            format!(
                "{error}; the partial file {} is left, it could not be deleted: {e}",
                part.display()
            ),
        )),
    }
}

/// `std::fs::rename`, except that an existing `to` fails it with `AlreadyExists` instead of
/// being replaced.
#[cfg(windows)]
fn rename_new(from: &Path, to: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    unsafe extern "system" {
        fn MoveFileExW(existing: *const u16, new: *const u16, flags: u32) -> i32;
    }
    let wide = |path: &Path| -> Vec<u16> { path.as_os_str().encode_wide().chain([0]).collect() };
    let (from, to) = (wide(from), wide(to));
    // SAFETY: both are NUL-terminated and outlive the call. Without MOVEFILE_REPLACE_EXISTING
    // an existing target fails the move (ERROR_ALREADY_EXISTS).
    if unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), 0) } == 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

/// `std::fs::rename`, except that an existing `to` fails it with `AlreadyExists` instead of
/// being replaced: `renameat2(RENAME_NOREPLACE)`, or a hard link and unlink where the file
/// system does not have it.
#[cfg(target_os = "linux")]
fn rename_new(from: &Path, to: &Path) -> std::io::Result<()> {
    use std::os::unix::ffi::OsStrExt;
    let c = |path: &Path| {
        std::ffi::CString::new(path.as_os_str().as_bytes())
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))
    };
    let (from_c, to_c) = (c(from)?, c(to)?);
    // SAFETY: both are NUL-terminated and outlive the call.
    let moved = unsafe {
        libc::renameat2(
            libc::AT_FDCWD,
            from_c.as_ptr(),
            libc::AT_FDCWD,
            to_c.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    if moved == 0 {
        return Ok(());
    }
    let error = std::io::Error::last_os_error();
    match error.raw_os_error() {
        Some(libc::EINVAL | libc::ENOSYS) => link_and_unlink(from, to),
        _ => Err(error),
    }
}

#[cfg(all(unix, not(target_os = "linux")))]
fn rename_new(from: &Path, to: &Path) -> std::io::Result<()> {
    link_and_unlink(from, to)
}

/// A link fails on an existing target, so nothing is replaced. Where the filesystem or sandbox
/// forbids hard links, falls back to creating `to` exclusively if absent (or failing `AlreadyExists`).
#[cfg(unix)]
fn link_and_unlink(from: &Path, to: &Path) -> std::io::Result<()> {
    match std::fs::hard_link(from, to) {
        Ok(()) => std::fs::remove_file(from),
        Err(e)
            if matches!(
                e.kind(),
                std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::Unsupported
            ) || matches!(
                e.raw_os_error(),
                Some(libc::EPERM | libc::EXDEV | libc::EOPNOTSUPP | libc::ENOSYS | libc::EINVAL)
            ) =>
        {
            copy_new_and_unlink(from, to)
        }
        Err(e) => Err(e),
    }
}

/// Opens `from` before exclusively creating `to` so a missing source never leaves an empty target.
#[cfg(any(unix, test))]
fn copy_new_and_unlink(from: &Path, to: &Path) -> std::io::Result<()> {
    let mut src = std::fs::File::open(from)?;
    let mut created = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(to)?;
    if let Err(copy_err) = std::io::copy(&mut src, &mut created).and_then(|_| created.sync_all()) {
        drop(created);
        let _ = std::fs::remove_file(to);
        return Err(copy_err);
    }
    drop(created);
    let _ = std::fs::remove_file(from);
    Ok(())
}

impl HostClient {
    /// Downloads the host's `name` to the file `dest` through [`download_into`], with
    /// `download_file`'s size check, progress and cancel.
    pub fn download_to_path(
        &self,
        name: &str,
        listed: u64,
        dest: &Path,
        progress: &mut dyn FnMut(u64, u64),
        stop: &TransferCancel,
    ) -> Result<u64, FileError> {
        download_into(dest, |sink| {
            self.download_file(name, listed, sink, progress, stop)
        })
    }
}

#[cfg(test)]
mod tests;
