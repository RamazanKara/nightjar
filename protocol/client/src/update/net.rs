//! HTTPS transport for self-updates and GitHub release checks (`rustls` + `webpki-roots`),
//! manual redirect validation, and URL parsing shared with `nightjar-desktop::update_net`.

use std::io::{ErrorKind, Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use nightjar_policy::http::TransferCancel;
use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection, RootCertStore, StreamOwned};

use crate::http::transport::{Abort, Host, connect, io_tls_error, tls_error};
use crate::http::wire::{Reader, read_final_head};

const MAX_REDIRECTS: usize = 5;
/// Cap on buffered response bodies for [`WebPkiTransport::exchange`] (24 MiB).
pub const MAX_EXCHANGE_BYTES: usize = 24 * 1024 * 1024;
/// Cap on downloaded game info assets via [`WebPkiTransport::download_to_path`] (64 MiB).
const MAX_DOWNLOAD_BYTES: u64 = 64 * 1024 * 1024;

/// Buffered HTTPS response from [`WebPkiTransport::exchange`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Exchanged {
    pub status: u16,
    pub retry_after: Option<u64>,
    pub body: Vec<u8>,
}

/// An http or https address with a plain hostname or IPv4 literal: no userinfo, no IPv6, no
/// whitespace/control chars, so what is judged here is what gets connected to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Url {
    pub secure: bool,
    pub host: String,
    pub port: u16,
    /// Path with optional query.
    pub path: String,
}

impl Url {
    pub fn parse(text: &str) -> Option<Self> {
        let (secure, rest) = match (text.strip_prefix("https://"), text.strip_prefix("http://")) {
            (Some(rest), _) => (true, rest),
            (_, Some(rest)) => (false, rest),
            _ => return None,
        };
        let rest = rest.split_once('#').map_or(rest, |(head, _)| head);
        let (authority, path) = match rest.find(['/', '?']) {
            Some(at) => rest.split_at(at),
            None => (rest, "/"),
        };
        let path = if path.starts_with('?') {
            format!("/{path}")
        } else {
            path.to_owned()
        };
        let (host, port) = match authority.split_once(':') {
            Some((host, port)) => (host, port.parse().ok()?),
            None => (authority, if secure { 443 } else { 80 }),
        };
        let plain = |b: u8| b.is_ascii_alphanumeric() || b == b'.' || b == b'-';
        if host.is_empty()
            || !host.bytes().all(plain)
            || !path.starts_with('/')
            || path.bytes().any(|b| b <= b' ' || b == 0x7F)
        {
            return None;
        }
        Some(Self {
            secure,
            host: host.to_ascii_lowercase(),
            port,
            path,
        })
    }

    /// Resolves a redirect's `Location` relative to `self`.
    pub fn resolve(&self, location: &str) -> Option<Self> {
        let location = location.split_once('#').map_or(location, |(head, _)| head);
        if location.starts_with('/') && !location.starts_with("//") {
            if location.bytes().any(|b| b <= b' ' || b == 0x7F) {
                return None;
            }
            return Some(Self {
                path: location.to_owned(),
                ..self.clone()
            });
        }
        Self::parse(location)
    }
}

pub struct Response {
    pub status: u16,
    pub location: Option<String>,
    pub length: Option<u64>,
    pub body: Box<dyn Read + Send>,
}

/// One HTTP GET that never follows redirects on its own.
pub trait Transport: Send + Sync {
    fn get(&self, url: &Url, api: bool) -> Result<Response, String>;
}

/// The hosts a release download goes through, GitHub's CDN redirect targets included.
pub fn download_host(url: &Url) -> bool {
    url.secure
        && url.port == 443
        && matches!(
            url.host.as_str(),
            "github.com" | "release-assets.githubusercontent.com" | "objects.githubusercontent.com"
        )
}

/// The GitHub REST API host for release metadata queries.
pub fn api_host(url: &Url) -> bool {
    url.secure && url.port == 443 && url.host == "api.github.com"
}

/// Error from opening an HTTP(S) URL with redirect validation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OpenError {
    Address(String),
    Network(String),
}

/// Performs a GET, manually following up to 5 redirects while checking `allow(&url)` before each hop.
pub fn open(
    net: &dyn Transport,
    address: &str,
    allow: fn(&Url) -> bool,
    api: bool,
) -> Result<Response, OpenError> {
    let mut url = Url::parse(address).ok_or_else(|| OpenError::Address(address.to_owned()))?;
    for _ in 0..=MAX_REDIRECTS {
        if !allow(&url) {
            return Err(OpenError::Address(url.host));
        }
        let response = net.get(&url, api).map_err(OpenError::Network)?;
        if !matches!(response.status, 301 | 302 | 303 | 307 | 308) {
            return Ok(response);
        }
        let location = response
            .location
            .as_deref()
            .ok_or_else(|| OpenError::Network("a redirect to nowhere".into()))?;
        url = url
            .resolve(location)
            .ok_or_else(|| OpenError::Address(location.to_owned()))?;
    }
    Err(OpenError::Network("too many redirects".into()))
}

/// Pure-Rust TLS/HTTP transport using `rustls` + Mozilla WebPKI root CAs (`webpki-roots`).
pub struct WebPkiTransport {
    agent: String,
    read_timeout: Duration,
    tls_config: Arc<ClientConfig>,
    cancel: Option<Arc<TransferCancel>>,
}

type RawResponse = (u16, Option<String>, Option<u64>, Box<dyn Read>);

impl WebPkiTransport {
    pub fn new(agent: String) -> Self {
        let roots = RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        Self::with_roots(agent, roots)
    }

    #[cfg(test)]
    pub(crate) fn with_roots_for_test(agent: String, roots: RootCertStore) -> Self {
        Self::with_roots(agent, roots)
    }

    fn with_roots(agent: String, roots: RootCertStore) -> Self {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let tls_config = ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .expect("valid TLS versions")
            .with_root_certificates(roots)
            .with_no_client_auth();
        Self {
            agent,
            read_timeout: Duration::from_secs(15),
            tls_config: Arc::new(tls_config),
            cancel: None,
        }
    }

    pub fn with_read_timeout(mut self, timeout: Duration) -> Self {
        self.read_timeout = timeout;
        self
    }

    pub fn with_cancel(mut self, cancel: Arc<TransferCancel>) -> Self {
        self.cancel = Some(cancel);
        self
    }
}

/// Socket wrapper enforcing an idle timeout and immediate [`TransferCancel`] abort.
struct CancelSocket {
    socket: TcpStream,
    abort: Arc<Abort>,
    read_timeout: Duration,
    cancel: Option<Arc<TransferCancel>>,
    hook_id: Option<u64>,
}

impl Drop for CancelSocket {
    fn drop(&mut self) {
        if let (Some(cancel), Some(id)) = (&self.cancel, self.hook_id.take()) {
            cancel.unregister_hook(id);
        }
    }
}

impl Read for CancelSocket {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.cancel.as_ref().is_some_and(|c| c.is_cancelled()) {
            return Err(std::io::Error::new(
                ErrorKind::ConnectionAborted,
                "cancelled",
            ));
        }
        let _ = self.socket.set_read_timeout(Some(self.read_timeout));
        self.abort.guard(|| self.socket.read(buf))
    }
}

impl Write for CancelSocket {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if self.cancel.as_ref().is_some_and(|c| c.is_cancelled()) {
            return Err(std::io::Error::new(
                ErrorKind::ConnectionAborted,
                "cancelled",
            ));
        }
        let _ = self.socket.set_write_timeout(Some(Duration::from_secs(15)));
        self.abort.guard(|| self.socket.write(buf))
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.socket.flush()
    }
}

trait ReadWrite: Read + Write + Send {}
impl<T: Read + Write + Send> ReadWrite for T {}

/// Formats the HTTP/1.1 `Host:` header, omitting default ports `:443` and `:80` so signed CDN
/// URLs (`X-Amz-SignedHeaders=host`) validate cleanly.
pub(crate) fn format_host_header(host: &Host, port: u16, secure: bool) -> String {
    let default_port = if secure { 443 } else { 80 };
    if port == default_port {
        host.url_host()
    } else {
        format!("{}:{port}", host.url_host())
    }
}

pub(crate) fn build_request_head(url: &Url, host: &Host, agent: &str, api: bool) -> String {
    let host_header = format_host_header(host, url.port, url.secure);
    let accept = if api {
        "Accept: application/vnd.github+json\r\nX-GitHub-Api-Version: 2022-11-28\r\n"
    } else {
        "Accept: application/octet-stream\r\n"
    };
    format!(
        "GET {} HTTP/1.1\r\nHost: {host_header}\r\nConnection: close\r\nUser-Agent: {agent}\r\n{accept}\r\n",
        url.path
    )
}

impl Transport for WebPkiTransport {
    fn get(&self, url: &Url, api: bool) -> Result<Response, String> {
        if !url.secure {
            return Err("HTTPS required".into());
        }
        if self.cancel.as_ref().is_some_and(|c| c.is_cancelled()) {
            return Err("cancelled".into());
        }
        let host = Host::parse(&url.host).map_err(|e| e.to_string())?;
        let tcp =
            connect(&host, url.port, Duration::from_secs(10), None).map_err(|e| e.to_string())?;
        let _ = tcp.set_nodelay(true);
        let abort = Abort::new(&tcp).map_err(|e| e.to_string())?;
        let hook_id = match &self.cancel {
            Some(cancel) => {
                let abort_ref = abort.clone();
                Some(
                    cancel
                        .register_hook(Arc::new(move || abort_ref.abort()))
                        .map_err(|e| e.to_string())?,
                )
            }
            None => None,
        };
        let sock = CancelSocket {
            socket: tcp,
            abort,
            read_timeout: self.read_timeout,
            cancel: self.cancel.clone(),
            hook_id,
        };
        let name = ServerName::try_from(url.host.clone())
            .map_err(|_| format!("invalid host name: {}", url.host))?;
        let conn = ClientConnection::new(self.tls_config.clone(), name)
            .map_err(|e| tls_error(e).to_string())?;
        let mut tls = StreamOwned::new(conn, sock);
        while tls.conn.is_handshaking() {
            tls.conn
                .complete_io(&mut tls.sock)
                .map_err(|e| io_tls_error(e).to_string())?;
        }
        let req = build_request_head(url, &host, &self.agent, api);
        tls.write_all(req.as_bytes())
            .and_then(|()| tls.flush())
            .map_err(|e| io_tls_error(e).to_string())?;

        let mut stream: Box<dyn ReadWrite> = Box::new(tls);
        let mut buffer = Vec::new();
        let mut eof = false;
        let head = {
            let mut reader = Reader {
                stream: &mut *stream,
                buffer: std::mem::take(&mut buffer),
                eof,
            };
            let head = read_final_head(&mut reader).map_err(|e| e.to_string())?;
            buffer = reader.buffer;
            eof = reader.eof;
            head
        };
        let status = head.status;
        let location = head.header("Location").map(str::to_owned);
        let chunked = head
            .header("Transfer-Encoding")
            .is_some_and(|t| t.eq_ignore_ascii_case("chunked"));
        let length = if chunked {
            None
        } else {
            head.content_length().map_err(|e| e.to_string())?
        };
        let framing = if chunked {
            Framing::Chunked {
                left: 0,
                first: true,
                done: false,
            }
        } else if let Some(len) = length {
            Framing::Sized(len)
        } else {
            Framing::Close
        };
        Ok(Response {
            status,
            location,
            length,
            body: Box::new(HttpBody {
                io: StreamBuf {
                    stream,
                    buffer,
                    eof,
                },
                framing,
            }),
        })
    }
}

enum Framing {
    Chunked {
        left: usize,
        first: bool,
        done: bool,
    },
    Sized(u64),
    Close,
}

struct StreamBuf {
    stream: Box<dyn ReadWrite>,
    buffer: Vec<u8>,
    eof: bool,
}

impl StreamBuf {
    fn fill(&mut self) -> std::io::Result<bool> {
        let mut reader = Reader {
            stream: &mut *self.stream,
            buffer: std::mem::take(&mut self.buffer),
            eof: self.eof,
        };
        let res = reader.fill();
        self.buffer = reader.buffer;
        self.eof = reader.eof;
        res.map_err(std::io::Error::other)
    }

    fn read_line(&mut self) -> std::io::Result<String> {
        let mut reader = Reader {
            stream: &mut *self.stream,
            buffer: std::mem::take(&mut self.buffer),
            eof: self.eof,
        };
        let res = reader.line();
        self.buffer = reader.buffer;
        self.eof = reader.eof;
        res.map_err(std::io::Error::other)
    }
}

struct HttpBody {
    io: StreamBuf,
    framing: Framing,
}

impl Read for HttpBody {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        if out.is_empty() {
            return Ok(0);
        }
        let io = &mut self.io;
        match &mut self.framing {
            Framing::Chunked { done: true, .. } | Framing::Sized(0) => Ok(0),
            Framing::Chunked { left, first, done } => {
                while *left == 0 {
                    if !*first && !io.read_line()?.is_empty() {
                        return Err(std::io::Error::new(
                            ErrorKind::InvalidData,
                            "invalid chunk terminator",
                        ));
                    }
                    let line = io.read_line()?;
                    let size_text = line.split(';').next().unwrap_or_default().trim();
                    let size = size_text
                        .bytes()
                        .all(|b| b.is_ascii_hexdigit())
                        .then(|| usize::from_str_radix(size_text, 16).ok())
                        .flatten()
                        .ok_or_else(|| {
                            std::io::Error::new(ErrorKind::InvalidData, "invalid chunk size")
                        })?;
                    if size == 0 {
                        *done = true;
                        return Ok(0);
                    }
                    *left = size;
                    *first = false;
                }
                if io.buffer.is_empty() && !io.fill()? {
                    return Err(std::io::Error::new(
                        ErrorKind::UnexpectedEof,
                        "chunked body ended early",
                    ));
                }
                let take = out.len().min(*left).min(io.buffer.len());
                out[..take].copy_from_slice(&io.buffer[..take]);
                io.buffer.drain(..take);
                *left -= take;
                Ok(take)
            }
            Framing::Sized(remaining) => {
                let want = usize::try_from(*remaining)
                    .unwrap_or(usize::MAX)
                    .min(out.len());
                if io.buffer.is_empty() && !io.fill()? {
                    return Err(std::io::Error::new(
                        ErrorKind::UnexpectedEof,
                        "content-length body ended early",
                    ));
                }
                let take = want.min(io.buffer.len());
                out[..take].copy_from_slice(&io.buffer[..take]);
                io.buffer.drain(..take);
                *remaining -= take as u64;
                Ok(take)
            }
            Framing::Close => {
                if !io.buffer.is_empty() {
                    let take = out.len().min(io.buffer.len());
                    out[..take].copy_from_slice(&io.buffer[..take]);
                    io.buffer.drain(..take);
                    return Ok(take);
                }
                io.stream.read(out)
            }
        }
    }
}

impl WebPkiTransport {
    /// Performs an HTTPS request using strict `rustls` + WebPKI CA verification.
    /// Follows up to `MAX_REDIRECTS` HTTPS redirects when `follow_redirects` is true (never
    /// downgrading HTTPS to plain HTTP). Caps buffered body at [`MAX_EXCHANGE_BYTES`] (24 MiB).
    pub fn exchange(
        &self,
        method: &str,
        url: &Url,
        headers: &[(String, String)],
        body: Option<&[u8]>,
        follow_redirects: bool,
    ) -> Result<Exchanged, String> {
        let valid_token = |s: &str| {
            !s.is_empty()
                && !s.contains('\r')
                && !s.contains('\n')
                && s.bytes().all(|b| b.is_ascii_graphic())
        };
        if !valid_token(method) {
            return Err("invalid HTTP method".into());
        }
        for (k, v) in headers {
            if !valid_token(k) || v.contains('\r') || v.contains('\n') {
                return Err(format!("invalid header: {k}"));
            }
        }
        let mut current = url.clone();
        let mut current_method = method.to_owned();
        let mut current_body = body;
        let mut current_headers: Vec<(String, String)> = headers.to_vec();
        for _ in 0..=MAX_REDIRECTS {
            let (status, location, retry_after, mut resp_body) =
                self.send_request(&current_method, &current, &current_headers, current_body)?;
            if follow_redirects && matches!(status, 301 | 302 | 303 | 307 | 308) {
                let Some(loc) = location else {
                    return Err(format!("redirect {status} without Location"));
                };
                let next = current
                    .resolve(&loc)
                    .ok_or_else(|| format!("invalid redirect URL: {loc}"))?;
                if !next.secure {
                    return Err("redirect downgraded off HTTPS".into());
                }
                if next.host != current.host || next.port != current.port {
                    // Drop credential and unlisted headers cross-origin; only keep safe content
                    // negotiation headers, and refuse cross-origin 307/308 bodies/mutations.
                    current_headers.retain(|(k, _)| {
                        k.eq_ignore_ascii_case("accept")
                            || k.eq_ignore_ascii_case("accept-language")
                    });
                    if matches!(status, 307 | 308)
                        && (current_body.is_some()
                            || (!current_method.eq_ignore_ascii_case("GET")
                                && !current_method.eq_ignore_ascii_case("HEAD")))
                    {
                        return Err(format!(
                            "refusing cross-origin {status} redirect for {current_method}"
                        ));
                    }
                }
                if matches!(status, 301..=303) && current_method.eq_ignore_ascii_case("POST") {
                    current_method = "GET".to_owned();
                    current_body = None;
                }
                current = next;
                continue;
            }
            let mut buf = Vec::new();
            let mut chunk = [0u8; 8192];
            loop {
                let n = resp_body.read(&mut chunk).map_err(|e| e.to_string())?;
                if n == 0 {
                    break;
                }
                if buf.len() + n > MAX_EXCHANGE_BYTES {
                    return Err("response exceeded 24 MiB limit".into());
                }
                buf.extend_from_slice(&chunk[..n]);
            }
            return Ok(Exchanged {
                status,
                retry_after,
                body: buf,
            });
        }
        Err(format!("too many redirects ({MAX_REDIRECTS})"))
    }

    /// Streams an HTTPS `GET` directly into `target` (writing a temporary sibling `.part` file
    /// and renaming atomically), returning HTTP status. Fails cleanly on non-2xx or IO errors.
    pub fn download_to_path(
        &self,
        url: &Url,
        target: &Path,
        follow_redirects: bool,
    ) -> Result<u16, String> {
        let mut current = url.clone();
        for _ in 0..=MAX_REDIRECTS {
            let (status, location, _, mut resp_body) = self.send_request("GET", &current, &[], None)?;
            if follow_redirects && matches!(status, 301 | 302 | 303 | 307 | 308) {
                let Some(loc) = location else {
                    return Err(format!("redirect {status} without Location"));
                };
                let next = current
                    .resolve(&loc)
                    .ok_or_else(|| format!("invalid redirect URL: {loc}"))?;
                if !next.secure {
                    return Err("redirect downgraded off HTTPS".into());
                }
                current = next;
                continue;
            }
            if !(200..=299).contains(&status) {
                return Ok(status);
            }
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let mut part_name = target
                .file_name()
                .ok_or_else(|| "missing target file name".to_owned())?
                .to_os_string();
            part_name.push(".part");
            let part = target.with_file_name(part_name);
            let write_res = (|| -> Result<(), String> {
                let mut file = std::fs::File::create(&part).map_err(|e| e.to_string())?;
                let mut total = 0u64;
                let mut chunk = [0u8; 8192];
                loop {
                    let n = resp_body.read(&mut chunk).map_err(|e| e.to_string())?;
                    if n == 0 {
                        break;
                    }
                    total += n as u64;
                    if total > MAX_DOWNLOAD_BYTES {
                        return Err("download exceeded 64 MiB limit".into());
                    }
                    file.write_all(&chunk[..n]).map_err(|e| e.to_string())?;
                }
                file.sync_all().map_err(|e| e.to_string())?;
                Ok(())
            })();
            if let Err(err) = write_res {
                let _ = std::fs::remove_file(&part);
                return Err(err);
            }
            std::fs::rename(&part, target).map_err(|e| {
                let _ = std::fs::remove_file(&part);
                e.to_string()
            })?;
            return Ok(status);
        }
        Err(format!("too many redirects ({MAX_REDIRECTS})"))
    }

    fn send_request(
        &self,
        method: &str,
        url: &Url,
        headers: &[(String, String)],
        body: Option<&[u8]>,
    ) -> Result<RawResponse, String> {
        if !url.secure {
            return Err("HTTPS required".into());
        }
        if self.cancel.as_ref().is_some_and(|c| c.is_cancelled()) {
            return Err("cancelled".into());
        }
        let host = Host::parse(&url.host).map_err(|e| e.to_string())?;
        let tcp =
            connect(&host, url.port, Duration::from_secs(10), None).map_err(|e| e.to_string())?;
        let _ = tcp.set_nodelay(true);
        let abort = Abort::new(&tcp).map_err(|e| e.to_string())?;
        let hook_id = match &self.cancel {
            Some(cancel) => {
                let abort_ref = abort.clone();
                Some(
                    cancel
                        .register_hook(Arc::new(move || abort_ref.abort()))
                        .map_err(|e| e.to_string())?,
                )
            }
            None => None,
        };
        let sock = CancelSocket {
            socket: tcp,
            abort,
            read_timeout: self.read_timeout,
            cancel: self.cancel.clone(),
            hook_id,
        };
        let name = ServerName::try_from(url.host.clone())
            .map_err(|_| format!("invalid host name: {}", url.host))?;
        let conn = ClientConnection::new(self.tls_config.clone(), name)
            .map_err(|e| tls_error(e).to_string())?;
        let mut tls = StreamOwned::new(conn, sock);
        while tls.conn.is_handshaking() {
            tls.conn
                .complete_io(&mut tls.sock)
                .map_err(|e| io_tls_error(e).to_string())?;
        }
        let mut req = format!(
            "{} {} HTTP/1.1\r\nHost: {}\r\nUser-Agent: {}\r\nConnection: close\r\n",
            method,
            url.path,
            format_host_header(&host, url.port, url.secure),
            self.agent
        );
        for (k, v) in headers {
            if k.eq_ignore_ascii_case("host")
                || k.eq_ignore_ascii_case("connection")
                || k.eq_ignore_ascii_case("content-length")
            {
                continue;
            }
            req.push_str(k);
            req.push_str(": ");
            req.push_str(v);
            req.push_str("\r\n");
        }
        if let Some(bytes) = body {
            req.push_str(&format!("Content-Length: {}\r\n", bytes.len()));
        } else if method.eq_ignore_ascii_case("POST") || method.eq_ignore_ascii_case("PUT") {
            req.push_str("Content-Length: 0\r\n");
        }
        req.push_str("\r\n");
        tls.write_all(req.as_bytes())
            .and_then(|()| {
                if let Some(bytes) = body {
                    tls.write_all(bytes)?;
                }
                tls.flush()
            })
            .map_err(|e| io_tls_error(e).to_string())?;

        let mut stream: Box<dyn ReadWrite> = Box::new(tls);
        let mut buffer = Vec::new();
        let mut eof = false;
        let head = {
            let mut reader = Reader {
                stream: &mut *stream,
                buffer: std::mem::take(&mut buffer),
                eof,
            };
            let head = read_final_head(&mut reader).map_err(|e| e.to_string())?;
            buffer = reader.buffer;
            eof = reader.eof;
            head
        };
        let status = head.status;
        let location = head.header("Location").map(str::to_owned);
        let retry_after = head
            .header("Retry-After")
            .and_then(|v| v.trim().parse::<u64>().ok());
        let chunked = head
            .header("Transfer-Encoding")
            .is_some_and(|t| t.eq_ignore_ascii_case("chunked"));
        let length = if chunked {
            None
        } else {
            head.content_length().map_err(|e| e.to_string())?
        };
        let framing = if chunked {
            Framing::Chunked {
                left: 0,
                first: true,
                done: false,
            }
        } else if let Some(len) = length {
            Framing::Sized(len)
        } else {
            Framing::Close
        };
        Ok((
            status,
            location,
            retry_after,
            Box::new(HttpBody {
                io: StreamBuf {
                    stream,
                    buffer,
                    eof,
                },
                framing,
            }),
        ))
    }
}
