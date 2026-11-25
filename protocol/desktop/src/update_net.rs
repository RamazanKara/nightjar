//! The HTTP under the update check and its download: one GET that never follows a redirect on its
//! own, so that self_update.rs can judge every address before anything connects to it. WinHTTP on
//! Windows (the system's proxy settings and certificate store, nothing bundled), ureq elsewhere.

use std::io::Read;
use std::time::Duration;

pub use nightjar_core::update::net::{Response, Transport, Url};

pub struct Native {
    pub agent: String,
    /// How long a body may stay silent before the transfer fails.
    pub read_timeout: Duration,
}

impl Native {
    pub fn new(agent: String) -> Self {
        Self { agent, read_timeout: Duration::from_secs(15) }
    }
}

#[cfg(windows)]
impl Transport for Native {
    fn get(&self, url: &Url, api: bool) -> Result<Response, String> {
        use windows::core::{HSTRING, PCWSTR};
        use windows::Win32::Networking::WinHttp::*;

        fn open(handle: *mut core::ffi::c_void, what: &str) -> Result<Handle, String> {
            if handle.is_null() { Err(format!("{what}: {}", windows::core::Error::from_thread())) } else { Ok(Handle(handle)) }
        }

        // SAFETY: WinHTTP calls on handles owned here; strings and buffers outlive the calls.
        unsafe {
            let session = open(WinHttpOpen(&HSTRING::from(self.agent.as_str()), WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, PCWSTR::null(), PCWSTR::null(), 0), "WinHttpOpen")?;
            WinHttpSetTimeouts(session.0, 10_000, 10_000, 15_000, self.read_timeout.as_millis() as i32).map_err(|e| format!("timeouts: {e}"))?;
            let connection = open(WinHttpConnect(session.0, &HSTRING::from(url.host.as_str()), url.port, 0), "connect")?;
            let flags = if url.secure { WINHTTP_FLAG_SECURE } else { WINHTTP_OPEN_REQUEST_FLAGS(0) };
            let request = open(WinHttpOpenRequest(connection.0, &HSTRING::from("GET"), &HSTRING::from(url.path.as_str()), PCWSTR::null(), PCWSTR::null(), std::ptr::null(), flags), "request")?;
            let never = WINHTTP_OPTION_REDIRECT_POLICY_NEVER.to_ne_bytes();
            WinHttpSetOption(Some(request.0), WINHTTP_OPTION_REDIRECT_POLICY, Some(&never)).map_err(|e| format!("redirect policy: {e}"))?;
            let headers: Vec<u16> = if api { "Accept: application/vnd.github+json\r\nX-GitHub-Api-Version: 2022-11-28\r\n" } else { "Accept: application/octet-stream\r\n" }.encode_utf16().collect();
            WinHttpSendRequest(request.0, Some(&headers), None, 0, 0, 0).map_err(|e| format!("send: {e}"))?;
            WinHttpReceiveResponse(request.0, std::ptr::null_mut()).map_err(|e| format!("response: {e}"))?;
            let number = |query: u32| -> Option<u32> {
                let mut value = 0u32;
                let mut size = std::mem::size_of::<u32>() as u32;
                WinHttpQueryHeaders(request.0, query | WINHTTP_QUERY_FLAG_NUMBER, PCWSTR::null(), Some((&mut value as *mut u32).cast()), &mut size, std::ptr::null_mut()).ok()?;
                Some(value)
            };
            let status = number(WINHTTP_QUERY_STATUS_CODE).ok_or("no status")?;
            let length = number(WINHTTP_QUERY_CONTENT_LENGTH).map(u64::from);
            // Release storage addresses are long, signed URLs.
            let mut text = vec![0u16; 8 * 1024];
            let mut size = (text.len() * 2) as u32;
            let location = WinHttpQueryHeaders(request.0, WINHTTP_QUERY_LOCATION, PCWSTR::null(), Some(text.as_mut_ptr().cast()), &mut size, std::ptr::null_mut())
                .ok()
                .map(|()| String::from_utf16_lossy(&text[..size as usize / 2]));
            Ok(Response { status: status as u16, location, length, body: Box::new(Body { request, _connection: connection, _session: session }) })
        }
    }
}

/// What [`Native::exchange`] got back: the status, `Retry-After` in seconds when the answer had a
/// number there, and the body.
pub struct Exchanged {
    pub status: u16,
    pub retry_after: Option<u64>,
    pub body: Vec<u8>,
}

/// The most of a body [`Native::exchange`] reads: SteamGridDB's largest covers are a few MB.
pub(crate) const EXCHANGE_LIMIT: u64 = 24 << 20;

impl Native {
    /// One request with its own method, headers and body, read whole (up to 24 MB), for
    /// SteamGridDB and IGDB (`crate::game_info`). Redirects are followed only when `follow`
    /// (image files); requests carrying keys never go elsewhere. Errors name no address.
    pub fn exchange(&self, method: &str, url: &Url, headers: &[(String, String)], body: Option<&[u8]>, follow: bool) -> Result<Exchanged, String> {
        self.exchange_native(method, url, headers, body, follow)
    }

    #[cfg(windows)]
    fn exchange_native(&self, method: &str, url: &Url, headers: &[(String, String)], body: Option<&[u8]>, follow: bool) -> Result<Exchanged, String> {
        use windows::core::{HSTRING, PCWSTR};
        use windows::Win32::Networking::WinHttp::*;

        fn open(handle: *mut core::ffi::c_void, what: &str) -> Result<Handle, String> {
            if handle.is_null() { Err(format!("{what}: {}", windows::core::Error::from_thread())) } else { Ok(Handle(handle)) }
        }

        // SAFETY: WinHTTP calls on handles owned here; strings and buffers outlive the calls.
        unsafe {
            let session = open(WinHttpOpen(&HSTRING::from(self.agent.as_str()), WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, PCWSTR::null(), PCWSTR::null(), 0), "WinHttpOpen")?;
            WinHttpSetTimeouts(session.0, 10_000, 10_000, 15_000, self.read_timeout.as_millis() as i32).map_err(|e| format!("timeouts: {e}"))?;
            let connection = open(WinHttpConnect(session.0, &HSTRING::from(url.host.as_str()), url.port, 0), "connect")?;
            let flags = if url.secure { WINHTTP_FLAG_SECURE } else { WINHTTP_OPEN_REQUEST_FLAGS(0) };
            let request = open(WinHttpOpenRequest(connection.0, &HSTRING::from(method), &HSTRING::from(url.path.as_str()), PCWSTR::null(), PCWSTR::null(), std::ptr::null(), flags), "request")?;
            let policy = if follow { WINHTTP_OPTION_REDIRECT_POLICY_DISALLOW_HTTPS_TO_HTTP } else { WINHTTP_OPTION_REDIRECT_POLICY_NEVER }.to_ne_bytes();
            WinHttpSetOption(Some(request.0), WINHTTP_OPTION_REDIRECT_POLICY, Some(&policy)).map_err(|e| format!("redirect policy: {e}"))?;
            let mut lines = String::new();
            for (name, value) in headers {
                lines.push_str(&format!("{name}: {value}\r\n"));
            }
            if body.is_some() && !headers.iter().any(|(name, _)| name.eq_ignore_ascii_case("content-type")) {
                lines.push_str("Content-Type: text/plain\r\n");
            }
            let lines: Vec<u16> = lines.encode_utf16().collect();
            let data = body.unwrap_or_default();
            let sent = WinHttpSendRequest(
                request.0,
                if lines.is_empty() { None } else { Some(&lines) },
                if data.is_empty() { None } else { Some(data.as_ptr().cast()) },
                data.len() as u32,
                data.len() as u32,
                0,
            );
            // The error text, not the request: its headers hold keys.
            sent.map_err(|e| format!("send: {}", e.code()))?;
            WinHttpReceiveResponse(request.0, std::ptr::null_mut()).map_err(|e| format!("response: {}", e.code()))?;
            let number = |query: u32| -> Option<u32> {
                let mut value = 0u32;
                let mut size = std::mem::size_of::<u32>() as u32;
                WinHttpQueryHeaders(request.0, query | WINHTTP_QUERY_FLAG_NUMBER, PCWSTR::null(), Some((&mut value as *mut u32).cast()), &mut size, std::ptr::null_mut()).ok()?;
                Some(value)
            };
            let status = number(WINHTTP_QUERY_STATUS_CODE).ok_or("no status")? as u16;
            let retry_after = number(WINHTTP_QUERY_RETRY_AFTER).map(u64::from);
            let mut body = Vec::new();
            Body { request, _connection: connection, _session: session }.take(EXCHANGE_LIMIT).read_to_end(&mut body).map_err(|e| format!("read: {}", e.kind()))?;
            Ok(Exchanged { status, retry_after, body })
        }
    }

    #[cfg(not(windows))]
    fn exchange_native(&self, method: &str, url: &Url, headers: &[(String, String)], body: Option<&[u8]>, follow: bool) -> Result<Exchanged, String> {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .max_redirects(if follow { 5 } else { 0 })
            .max_redirects_will_error(false)
            .http_status_as_error(false)
            .user_agent(self.agent.as_str())
            .timeout_connect(Some(Duration::from_secs(10)))
            .timeout_recv_response(Some(Duration::from_secs(15)))
            .timeout_per_read(Some(self.read_timeout))
            .build()
            .into();
        let address = format!("{}://{}:{}{}", if url.secure { "https" } else { "http" }, url.host, url.port, url.path);
        let mut builder = ureq::http::Request::builder().method(method).uri(address.as_str());
        for (name, value) in headers {
            builder = builder.header(name.as_str(), value.as_str());
        }
        if body.is_some() && !headers.iter().any(|(name, _)| name.eq_ignore_ascii_case("content-type")) {
            builder = builder.header("Content-Type", "text/plain");
        }
        // ureq's errors can quote the address, whose query may hold a secret: only their kind.
        let request = builder.body(body.unwrap_or_default().to_vec()).map_err(|_| "request".to_owned())?;
        let response = agent.run(request).map_err(|e| match e {
            ureq::Error::Timeout(_) => "timeout".to_owned(),
            ureq::Error::Io(e) => format!("io: {}", e.kind()),
            _ => "transport".to_owned(),
        })?;
        let status = response.status().as_u16();
        let retry_after = response.headers().get("retry-after").and_then(|v| v.to_str().ok()).and_then(|v| v.trim().parse().ok());
        let mut body = Vec::new();
        response.into_body().into_reader().take(EXCHANGE_LIMIT).read_to_end(&mut body).map_err(|e| format!("read: {}", e.kind()))?;
        Ok(Exchanged { status, retry_after, body })
    }
}

#[cfg(windows)]
struct Handle(*mut core::ffi::c_void);

// SAFETY: WinHTTP handles may be used from any thread; each of these is used from one at a time.
#[cfg(windows)]
unsafe impl Send for Handle {}

#[cfg(windows)]
impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: a handle WinHTTP returned, closed once.
        unsafe { let _ = windows::Win32::Networking::WinHttp::WinHttpCloseHandle(self.0); }
    }
}

/// An open response. The fields close in this order: request, connection, session.
#[cfg(windows)]
struct Body {
    request: Handle,
    _connection: Handle,
    _session: Handle,
}

#[cfg(windows)]
impl Read for Body {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let mut read = 0u32;
        let room = buffer.len().min(u32::MAX as usize) as u32;
        // SAFETY: the request is open and the buffer holds `room` bytes.
        unsafe { windows::Win32::Networking::WinHttp::WinHttpReadData(self.request.0, buffer.as_mut_ptr().cast(), room, &mut read) }.map_err(std::io::Error::other)?;
        Ok(read as usize)
    }
}

#[cfg(not(windows))]
impl Transport for Native {
    fn get(&self, url: &Url, api: bool) -> Result<Response, String> {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .max_redirects(0)
            .max_redirects_will_error(false)
            .http_status_as_error(false)
            .user_agent(self.agent.as_str())
            .timeout_connect(Some(Duration::from_secs(10)))
            .timeout_recv_response(Some(Duration::from_secs(15)))
            .timeout_per_read(Some(self.read_timeout))
            .build()
            .into();
        let address = format!("{}://{}:{}{}", if url.secure { "https" } else { "http" }, url.host, url.port, url.path);
        let mut request = agent.get(&address);
        if api {
            request = request.header("Accept", "application/vnd.github+json").header("X-GitHub-Api-Version", "2022-11-28");
        } else {
            request = request.header("Accept", "application/octet-stream");
        }
        let response = request.call().map_err(|e| e.to_string())?;
        let header = |name: &str| response.headers().get(name).and_then(|v| v.to_str().ok()).map(str::to_owned);
        let (status, location, length) = (response.status().as_u16(), header("location"), header("content-length").and_then(|l| l.parse().ok()));
        Ok(Response { status, location, length, body: Box::new(response.into_body().into_reader()) })
    }
}
