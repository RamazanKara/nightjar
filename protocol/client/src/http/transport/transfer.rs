//! Streaming HTTP uploads (`Expect: 100-continue`), downloads, and throttled progress.

use super::super::wire::{read_body, read_final_head, read_head, read_response, Reader};
use super::super::Error;
use super::{
    connection, head_with, io_tls_error, request_head, write_timeout, Cancel, Deadline, Request,
    Response, Stream,
};
use std::io::{ErrorKind, Read, Write};
use std::time::Duration;

/// A body streamed from a reader: exactly `length` bytes, announced as the Content-Length.
pub(crate) struct Upload<'a> {
    pub reader: &'a mut dyn Read,
    pub length: u64,
    pub content_type: &'static str,
    /// `POST` for file transfer, `PUT` for a shared folder.
    pub method: &'static str,
}

/// Bytes moved so far and the total: once at 0, then at most every `PROGRESS_BYTES` and
/// `PROGRESS_INTERVAL`, whichever comes later, and once at the total.
type Progress<'a> = dyn FnMut(u64, u64) + 'a;

pub(super) const PROGRESS_BYTES: u64 = 256 * 1024;
const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

/// Spaces the progress calls, so a fast link does not call into the app for every chunk.
pub(super) struct Throttle {
    total: u64,
    reported: u64,
    pub(super) at: std::time::Instant,
}

impl Throttle {
    /// Reports 0 of `total`.
    pub(super) fn start(progress: &mut Progress<'_>, total: u64) -> Self {
        progress(0, total);
        Self { total, reported: 0, at: std::time::Instant::now() }
    }

    pub(super) fn update(&mut self, progress: &mut Progress<'_>, done: u64) {
        if self.due(done, std::time::Instant::now()) {
            progress(done, self.total);
        }
    }

    pub(super) fn due(&mut self, done: u64, now: std::time::Instant) -> bool {
        let due = done > self.reported && (done == self.total
            || (done - self.reported >= PROGRESS_BYTES && now.saturating_duration_since(self.at) >= PROGRESS_INTERVAL));
        if due {
            self.reported = done;
            self.at = now;
        }
        due
    }
}

/// The most an upload reads from its file at a time.
const UPLOAD_CHUNK: usize = 64 * 1024;

/// POSTs `upload` in chunks without holding the whole body, then reads the answer like `send`.
/// `transfer` cancels only this upload, at once, even while a write blocks; the read timeout
/// also bounds every write, and the time without progress over all of them, so a host that
/// stops reading fails the upload once. A reader that ends before `length` fails loudly.
///
/// The body waits for the host's `100 Continue`: a host that refuses the upload by its head
/// (413, 403) answers and closes without reading it, and on Windows the reset of a body sent
/// anyway discards that answer before it is read. A host (or proxy) that says nothing for
/// [`CONTINUE_WAIT`] gets the body anyway, as curl sends it.
pub(crate) fn upload(request: &Request<'_>, cancel: &Cancel, upload: Upload<'_>, progress: &mut Progress<'_>, transfer: &Cancel) -> Result<Response, Error> {
    let mut head = head_with(upload.method, request, Some((upload.content_type, upload.length)));
    head.insert_str(head.len() - 2, "Expect: 100-continue\r\n");
    let continue_by = ContinueBy::default();
    connection(request, &[cancel, transfer], Deadline::idle(request), request.timeouts.read, &head, Some(&continue_by), |stream, _| {
        let answer = await_continue(stream, request.body_limit, &continue_by);
        continue_by.set(None);
        match answer? {
            Some(refusal) => Ok(refusal),
            None => send_upload(stream, upload, request.body_limit, progress, transfer),
        }
    })
}

/// How long an upload waits for `100 Continue` before it sends the body anyway.
pub(super) const CONTINUE_WAIT: Duration = Duration::from_secs(1);

/// When an upload stops waiting for `100 Continue`: armed once its head is out, it cuts the
/// socket's reads short until `upload` disarms it.
pub(super) type ContinueBy = std::rc::Rc<std::cell::Cell<Option<std::time::Instant>>>;

/// What a read waiting for `100 Continue` fails with once [`CONTINUE_WAIT`] is over.
#[derive(Debug)]
struct ContinueElapsed;

impl std::fmt::Display for ContinueElapsed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("no 100 Continue")
    }
}

impl std::error::Error for ContinueElapsed {}

pub(super) fn continue_elapsed() -> std::io::Error {
    std::io::Error::new(ErrorKind::TimedOut, ContinueElapsed)
}

pub(super) fn is_continue_elapsed(error: &Error) -> bool {
    matches!(error, Error::Io(e) if e.get_ref().is_some_and(|inner| inner.is::<ContinueElapsed>()))
}

/// Reads the host's answer to an `Expect: 100-continue` head: `None` once it says `100 Continue`,
/// or says nothing at all by `continue_by`, else its refusal, read whole. It cannot have
/// accepted a body it did not get.
fn await_continue(stream: &mut dyn Stream, body_limit: usize, continue_by: &ContinueBy) -> Result<Option<Response>, Error> {
    let mut reader = Reader { stream, buffer: Vec::new(), eof: false };
    loop {
        let head = match read_head(&mut reader) {
            Ok(head) => head,
            Err(error) if is_continue_elapsed(&error) => {
                continue_by.set(None);
                if reader.buffer.is_empty() {
                    return Ok(None);
                }
                // Part of an answer arrived: the rest has the read timeout.
                continue;
            }
            Err(error) => return Err(error),
        };
        match head.status {
            100 => break,
            102..=199 => continue,
            200..=299 => return Err(Error::Malformed(format!("the host answered {} before the upload was sent", head.status))),
            _ => {
                let body = read_body(&mut reader, &head, body_limit)?;
                return Ok(Some(head.into_response(body)));
            }
        }
    }
    if !reader.buffer.is_empty() {
        return Err(Error::Malformed("the host sent more than 100 Continue before the upload".into()));
    }
    Ok(None)
}

/// Why an upload stopped sending.
enum Unsent {
    /// The reader failed or ended early, or the transfer was cancelled.
    Local(Error),
    /// The connection failed.
    Write(std::io::Error),
}

pub(super) fn send_upload(stream: &mut dyn Stream, upload: Upload<'_>, body_limit: usize, progress: &mut Progress<'_>, transfer: &Cancel) -> Result<Response, Error> {
    match write_upload(stream, upload, progress, transfer) {
        Ok(()) => read_response(stream, body_limit),
        Err(Unsent::Local(error)) => Err(error),
        // A host that stopped reading has no answer either: waiting for one would only double
        // the timeout.
        Err(Unsent::Write(error)) if matches!(error.kind(), ErrorKind::TimedOut | ErrorKind::WouldBlock) => Err(io_tls_error(error)),
        // The host may refuse (413, 403) before it reads the body and close: its answer says
        // more than the failed write. It never accepts a body it did not get whole.
        Err(Unsent::Write(error)) => match read_response(stream, body_limit) {
            Ok(response) if !(200..=299).contains(&response.status) => Ok(response),
            _ => Err(io_tls_error(error)),
        },
    }
}

fn write_upload<W: Write + ?Sized>(stream: &mut W, upload: Upload<'_>, progress: &mut Progress<'_>, transfer: &Cancel) -> Result<(), Unsent> {
    let Upload { reader, length, .. } = upload;
    let mut chunk = vec![0; UPLOAD_CHUNK];
    let mut sent = 0;
    let mut throttle = Throttle::start(progress, length);
    while sent < length {
        if transfer.is_cancelled() {
            return Err(Unsent::Local(Error::Cancelled));
        }
        let want = usize::try_from(length - sent).map_or(UPLOAD_CHUNK, |left| left.min(UPLOAD_CHUNK));
        let read = match reader.read(&mut chunk[..want]) {
            Ok(0) => return Err(Unsent::Local(Error::InvalidArgument(format!(
                "the upload ended after {sent} of {length} bytes")))),
            Ok(read) => read,
            Err(e) if e.kind() == ErrorKind::Interrupted => continue,
            Err(e) => return Err(Unsent::Local(Error::Io(e))),
        };
        stream.write_all(&chunk[..read]).map_err(Unsent::Write)?;
        sent += read as u64;
        throttle.update(progress, sent);
    }
    stream.flush().map_err(Unsent::Write)
}

/// GETs a body into `sink` as it arrives. A 2xx must carry a Content-Length and deliver all of
/// it; its `Response` has an empty body. Any other answer is read whole, up to `body_limit`, for
/// its error. `transfer` cancels only this download, at once, even while a read blocks. The read
/// timeout bounds the time without progress.
pub(crate) fn download(request: &Request<'_>, cancel: &Cancel, sink: &mut dyn Write, progress: &mut Progress<'_>, transfer: &Cancel) -> Result<Response, Error> {
    let head = request_head(request, None);
    connection(request, &[cancel, transfer], Deadline::idle(request), Some(write_timeout(request)), &head, None, |stream, _| {
        receive(stream, request.body_limit, sink, progress, transfer)
    })
}

pub(crate) fn download_optional(
    request: &Request<'_>,
    cancel: &Cancel,
    sink: &mut dyn Write,
    progress: &mut dyn FnMut(u64, Option<u64>),
    transfer: &Cancel,
) -> Result<Response, Error> {
    let head = request_head(request, None);
    connection(request, &[cancel, transfer], Deadline::idle(request), Some(write_timeout(request)), &head, None, |stream, _| {
        let mut reader = Reader { stream, buffer: Vec::new(), eof: false };
        let head = read_final_head(&mut reader)?;
        if !(200..=299).contains(&head.status) {
            let body = read_body(&mut reader, &head, request.body_limit)?;
            return Ok(head.into_response(body));
        }
        if head.header("Transfer-Encoding").is_some_and(|t| t.eq_ignore_ascii_case("chunked")) {
            let mut done = 0u64;
            progress(0, None);
            loop {
                let line = reader.line()?;
                let size_text = line.split(';').next().unwrap_or_default().trim();
                let mut left = size_text.bytes().all(|b| b.is_ascii_hexdigit())
                    .then(|| usize::from_str_radix(size_text, 16).ok()).flatten()
                    .ok_or_else(|| Error::Malformed("invalid chunk size".into()))?;
                if left == 0 { break; }
                while left > 0 {
                    if transfer.is_cancelled() { return Err(Error::Cancelled); }
                    if reader.buffer.is_empty() && !reader.fill()? {
                        return Err(Error::Malformed("the download ended early".into()));
                    }
                    let take = left.min(reader.buffer.len());
                    sink.write_all(&reader.buffer[..take])?;
                    reader.buffer.drain(..take);
                    done += take as u64;
                    left -= take;
                    progress(done, None);
                }
                if !reader.line()?.is_empty() { return Err(Error::Malformed("invalid chunk terminator".into())); }
            }
            sink.flush()?;
            return Ok(head.into_response(Vec::new()));
        }
        let length = head.content_length()?.ok_or_else(|| Error::Malformed("a download without a Content-Length".into()))?;
        let mut done = 0u64;
        let mut throttle = Throttle::start(&mut |d, t| progress(d, Some(t)), length);
        while done < length {
            if transfer.is_cancelled() { return Err(Error::Cancelled); }
            if reader.buffer.is_empty() && !reader.fill()? {
                return Err(Error::Malformed(format!("the download ended after {done} of {length} bytes")));
            }
            let n = usize::try_from(length - done).map_or(reader.buffer.len(), |left| left.min(reader.buffer.len()));
            sink.write_all(&reader.buffer[..n])?;
            reader.buffer.drain(..n);
            done += n as u64;
            throttle.update(&mut |d, t| progress(d, Some(t)), done);
        }
        if !reader.buffer.is_empty() {
            return Err(Error::Malformed(format!("the download is longer than its {length} bytes")));
        }
        sink.flush()?;
        Ok(head.into_response(Vec::new()))
    })
}

fn receive(stream: &mut dyn Stream, body_limit: usize, sink: &mut dyn Write, progress: &mut Progress<'_>, transfer: &Cancel) -> Result<Response, Error> {
    let mut reader = Reader { stream, buffer: Vec::new(), eof: false };
    let head = read_final_head(&mut reader)?;
    if !(200..=299).contains(&head.status) {
        let body = read_body(&mut reader, &head, body_limit)?;
        return Ok(head.into_response(body));
    }
    if head.header("Transfer-Encoding").is_some() {
        return Err(Error::Malformed("a download must not be chunked".into()));
    }
    let length = head
        .content_length()?
        .ok_or_else(|| Error::Malformed("a download without a Content-Length".into()))?;
    let mut done = 0;
    let mut throttle = Throttle::start(progress, length);
    while done < length {
        if transfer.is_cancelled() {
            return Err(Error::Cancelled);
        }
        if reader.buffer.is_empty() && !reader.fill()? {
            return Err(Error::Malformed(format!("the download ended after {done} of {length} bytes")));
        }
        let n = usize::try_from(length - done).map_or(reader.buffer.len(), |left| left.min(reader.buffer.len()));
        sink.write_all(&reader.buffer[..n])?;
        reader.buffer.drain(..n);
        done += n as u64;
        throttle.update(progress, done);
    }
    // Trailing bytes already received past Content-Length mean the host and client disagree.
    if !reader.buffer.is_empty() {
        return Err(Error::Malformed(format!("the download is longer than its {length} bytes")));
    }
    sink.flush()?;
    Ok(head.into_response(Vec::new()))
}
