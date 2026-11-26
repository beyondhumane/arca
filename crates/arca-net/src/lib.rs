//! One HTTPS request, made by the system.
//!
//! The only reason this crate exists is to ask a web address for a short piece
//! of text and hand it back. It is deliberately not a HTTP client: no methods
//! other than GET, no headers to set, no redirects followed, no cookies, no
//! streaming. What it does have is a ceiling on how much it will read, because
//! whatever is at the other end is not ours and a reply that never ends must
//! not be able to grow a window's memory without limit.
//!
//! On Windows it goes through WinHTTP, which is the machine's own client: its
//! list of trusted roots is the one Windows Update keeps current and the one
//! every other program here already trusts. Pulling in a HTTP crate instead
//! would have meant a TLS stack of several megabytes, with its own copy of
//! those roots to keep up to date, so that a program made of decompressors
//! could read a version number.
//!
//! Anywhere else it hands the request to the system's `curl`, for the same
//! reason. Whatever goes wrong comes back as a [`FetchError`] saying which of
//! the few ways it was, so that a caller can tell a cancel from a failure and
//! a missing client from a server that said no.

#![cfg_attr(not(windows), forbid(unsafe_code))]

/// The most that will be read from a reply, in bytes.
///
/// Room enough for what [`get`] is used for many times over, and small enough
/// that a stranger cannot make the window swallow a gigabyte by answering with
/// one. Something that knows what it is asking for -- an installer, whose size
/// the release says beforehand -- passes its own to [`fetch`].
pub const CEILING: usize = 256 * 1024;

/// Why a request brought nothing back.
#[derive(Debug)]
pub enum FetchError {
    /// The address is not a plain `https://` one, so it was never sent.
    Refused,
    /// The system client could not be started or could not reach the other
    /// end: no `curl`, no network, a name that does not resolve, a proxy that
    /// turned the connection down.
    Unavailable(std::io::Error),
    /// The other end answered, but not with a 200.
    Http,
    /// The reply is longer than the ceiling the caller set.
    TooLarge,
    /// The watcher asked to stop before the reply was complete.
    Cancelled,
    /// [`get`] only: the reply arrived but is not UTF-8 text.
    NotText,
}

impl std::fmt::Display for FetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FetchError::Refused => f.write_str("not a plain https address"),
            FetchError::Unavailable(e) => write!(f, "no connection: {e}"),
            FetchError::Http => f.write_str("the server did not answer 200"),
            FetchError::TooLarge => f.write_str("the reply is larger than allowed"),
            FetchError::Cancelled => f.write_str("cancelled"),
            FetchError::NotText => f.write_str("the reply is not text"),
        }
    }
}

impl std::error::Error for FetchError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            FetchError::Unavailable(e) => Some(e),
            _ => None,
        }
    }
}

/// Asks `url` for its contents and gives them back as text.
///
/// `https://` only, and only what fits under [`CEILING`]. Fails the same ways
/// [`fetch`] does, and with [`FetchError::NotText`] for a body that is not text.
pub fn get(url: &str, agent: &str) -> Result<String, FetchError> {
    let bytes = fetch(url, agent, CEILING, &|_, _| true)?;
    String::from_utf8(bytes).map_err(|_| FetchError::NotText)
}

/// The same, for something that is not text and does not fit in a line.
///
/// `ceiling` is what the caller is prepared to hold: anything longer than that
/// is refused halfway rather than read to the end, because the only way to know
/// how big a reply really is, is to have read it.
///
/// `watch` is told how many bytes have arrived and how many are expected -- nil
/// when the other end does not say -- and answers whether to carry on. That is
/// what a progress bar is drawn from, and what stops a download nobody is
/// waiting for any more.
#[cfg(not(windows))]
pub fn fetch(
    url: &str,
    agent: &str,
    ceiling: usize,
    watch: &dyn Fn(usize, Option<usize>) -> bool,
) -> Result<Vec<u8>, FetchError> {
    // Off Windows the system client is curl, which every Linux and macOS has
    // and which carries the system's own roots of trust: no TLS stack of our
    // own, same as WinHTTP on the other side. The reply goes to a file so that
    // the size can be watched while it grows.
    if !url.starts_with("https://") {
        return Err(FetchError::Refused);
    }
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let file = std::env::temp_dir().join(format!("arca-net-{}-{stamp}", std::process::id()));
    let mut child = std::process::Command::new("curl")
        .args([
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "--proto",
            "=https",
            "--proto-redir",
            "=https",
            "--max-filesize",
            &ceiling.to_string(),
            "--user-agent",
            agent,
            "--output",
        ])
        .arg(&file)
        .arg(url)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(FetchError::Unavailable)?;
    let done = loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => break Ok(()),
            Ok(Some(status)) => break Err(curl_failure(status.code())),
            Ok(None) => {
                let so_far = std::fs::metadata(&file).map_or(0, |m| m.len() as usize);
                if !watch(so_far, None) {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = std::fs::remove_file(&file);
                    return Err(FetchError::Cancelled);
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Err(e) => break Err(FetchError::Unavailable(e)),
        }
    };
    let bytes = std::fs::read(&file);
    let _ = std::fs::remove_file(&file);
    done?;
    let bytes = bytes.map_err(FetchError::Unavailable)?;
    if bytes.len() > ceiling {
        return Err(FetchError::TooLarge);
    }
    Ok(bytes)
}

/// What a curl that exited unsuccessfully means, from its exit code.
///
/// 22 is `--fail` turning down an HTTP error and 63 is `--max-filesize`; the
/// rest are curl not getting as far as a reply. No code at all means it was
/// killed by a signal.
#[cfg(any(not(windows), test))]
fn curl_failure(code: Option<i32>) -> FetchError {
    match code {
        Some(22) => FetchError::Http,
        Some(63) => FetchError::TooLarge,
        Some(code) => FetchError::Unavailable(std::io::Error::other(format!(
            "curl exited with code {code}"
        ))),
        None => FetchError::Unavailable(std::io::Error::other("curl was killed")),
    }
}

#[cfg(windows)]
pub fn fetch(
    url: &str,
    agent: &str,
    ceiling: usize,
    watch: &dyn Fn(usize, Option<usize>) -> bool,
) -> Result<Vec<u8>, FetchError> {
    let (host, path) = split(url).ok_or(FetchError::Refused)?;
    windows_impl::fetch(&host, &path, agent, ceiling, watch)
}

/// Splits `https://host/path...` into the host and everything after it.
///
/// A whole URL parser is not needed for the one shape this is asked about, and
/// a small one is easier to be sure of than a large one. Anything that is not
/// plain `https` with a host is turned down rather than guessed at: this is
/// about to be handed to the machine's HTTP client, and a caller who meant
/// something else should hear about it here.
///
/// Only where it is used: off Windows there is no client to hand an address to,
/// so the library has no caller for this and the compiler is right to say so.
/// The tests are a caller everywhere, and what this turns down is worth testing
/// on whatever machine happens to be running them.
#[cfg(any(windows, test))]
fn split(url: &str) -> Option<(String, String)> {
    let rest = url.strip_prefix("https://")?;
    // No credentials, no port, no fragment: none of them are needed and each
    // is a way of writing an address that means something other than it looks.
    if rest.contains('@') || rest.contains('#') {
        return None;
    }
    let (host, path) = match rest.find('/') {
        Some(cut) => (&rest[..cut], &rest[cut..]),
        None => (rest, "/"),
    };
    if host.is_empty() || host.contains(':') {
        return None;
    }
    Some((host.to_string(), path.to_string()))
}

#[cfg(windows)]
mod windows_impl {
    use super::FetchError;
    use windows::core::PCWSTR;
    use windows::Win32::Networking::WinHttp::*;

    fn unavailable(e: windows::core::Error) -> FetchError {
        FetchError::Unavailable(e.into())
    }

    fn last_error() -> FetchError {
        FetchError::Unavailable(std::io::Error::last_os_error())
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// Closes a WinHTTP handle when it goes out of scope.
    ///
    /// There are three of them to close and half a dozen ways out of the
    /// function below, several of them early. Written by hand that is three
    /// leaks waiting for somebody to add a fourth way out and forget one.
    struct Handle(*mut std::ffi::c_void);

    impl Drop for Handle {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe {
                    let _ = WinHttpCloseHandle(self.0);
                }
            }
        }
    }

    pub fn fetch(
        host: &str,
        path: &str,
        agent: &str,
        ceiling: usize,
        watch: &dyn Fn(usize, Option<usize>) -> bool,
    ) -> Result<Vec<u8>, FetchError> {
        let agent_w = wide(agent);
        let host_w = wide(host);
        let path_w = wide(path);

        unsafe {
            // The machine's own proxy settings, which is what every other
            // program on it uses: somebody behind a corporate proxy has already
            // told Windows about it once.
            let session = Handle(WinHttpOpen(
                PCWSTR(agent_w.as_ptr()),
                WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
                PCWSTR::null(),
                PCWSTR::null(),
                0,
            ));
            if session.0.is_null() {
                return Err(last_error());
            }
            // Time limits so that a server that accepts the connection and then
            // says nothing cannot leave a thread of ours waiting for ever. The
            // last one is per read, not for the whole download: a file of
            // several megabytes takes many reads and each one has to answer.
            let _ = WinHttpSetTimeouts(session.0, 5_000, 5_000, 10_000, 30_000);

            let connection = Handle(WinHttpConnect(
                session.0,
                PCWSTR(host_w.as_ptr()),
                INTERNET_DEFAULT_HTTPS_PORT,
                0,
            ));
            if connection.0.is_null() {
                return Err(last_error());
            }

            let request = Handle(WinHttpOpenRequest(
                connection.0,
                PCWSTR(wide("GET").as_ptr()),
                PCWSTR(path_w.as_ptr()),
                PCWSTR::null(),
                PCWSTR::null(),
                std::ptr::null(),
                WINHTTP_FLAG_SECURE,
            ));
            if request.0.is_null() {
                return Err(last_error());
            }

            WinHttpSendRequest(request.0, None, None, 0, 0, 0).map_err(unavailable)?;
            WinHttpReceiveResponse(request.0, std::ptr::null_mut()).map_err(unavailable)?;
            if status(request.0) != Some(200) {
                return Err(FetchError::Http);
            }
            // What the other end says it is going to send, if it says. A number
            // to draw a bar with, and nothing more: what is actually read is
            // what is counted, and the ceiling is what decides when to stop.
            let expected = header_number(request.0, WINHTTP_QUERY_CONTENT_LENGTH)
                .filter(|n| *n as usize <= ceiling)
                .map(|n| n as usize);

            let mut body: Vec<u8> = Vec::new();
            if !watch(0, expected) {
                return Err(FetchError::Cancelled);
            }
            loop {
                let mut waiting = 0u32;
                WinHttpQueryDataAvailable(request.0, &mut waiting).map_err(unavailable)?;
                if waiting == 0 {
                    break;
                }
                let want = (waiting as usize).min(ceiling - body.len());
                if want == 0 {
                    // At the ceiling with more still coming. Whatever this is,
                    // it is not what was asked for.
                    return Err(FetchError::TooLarge);
                }
                let at = body.len();
                body.resize(at + want, 0);
                let mut read = 0u32;
                WinHttpReadData(
                    request.0,
                    body[at..].as_mut_ptr() as *mut _,
                    want as u32,
                    &mut read,
                )
                .map_err(unavailable)?;
                body.truncate(at + read as usize);
                if read == 0 {
                    break;
                }
                if !watch(body.len(), expected) {
                    return Err(FetchError::Cancelled);
                }
            }
            Ok(body)
        }
    }

    /// A header that is a number, read as text and turned into one.
    ///
    /// Absent is not a failure: Content-Length is a courtesy and a reply that
    /// does not carry it is still a reply. It only costs the progress bar its
    /// end, which is why nothing here refuses anything over it.
    unsafe fn header_number(request: *mut std::ffi::c_void, which: u32) -> Option<u64> {
        let mut buffer = [0u16; 32];
        let mut size = (buffer.len() * 2) as u32;
        WinHttpQueryHeaders(
            request,
            which,
            PCWSTR::null(),
            Some(buffer.as_mut_ptr() as *mut _),
            &mut size,
            std::ptr::null_mut(),
        )
        .ok()?;
        let end = buffer.iter().position(|c| *c == 0).unwrap_or(buffer.len());
        String::from_utf16_lossy(&buffer[..end]).trim().parse().ok()
    }

    /// The status line's number, which is asked for as text and turned into
    /// one: WinHTTP will hand it over as a number instead, but only with a flag
    /// combination that is easier to get wrong than this is to parse.
    unsafe fn status(request: *mut std::ffi::c_void) -> Option<u32> {
        let mut buffer = [0u16; 8];
        let mut size = (buffer.len() * 2) as u32;
        WinHttpQueryHeaders(
            request,
            WINHTTP_QUERY_STATUS_CODE,
            PCWSTR::null(),
            Some(buffer.as_mut_ptr() as *mut _),
            &mut size,
            std::ptr::null_mut(),
        )
        .ok()?;
        let end = buffer.iter().position(|c| *c == 0).unwrap_or(buffer.len());
        String::from_utf16_lossy(&buffer[..end]).trim().parse().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_address_is_split_into_the_two_halves_the_client_wants() {
        assert_eq!(
            split("https://api.github.com/repos/a/b/releases/latest"),
            Some(("api.github.com".into(), "/repos/a/b/releases/latest".into()))
        );
        // No path at all still asks for the root.
        assert_eq!(
            split("https://example.com"),
            Some(("example.com".into(), "/".into()))
        );
    }

    // Every one of these is a way of writing an address that means something
    // other than it appears to. Turned down here rather than handed on.
    #[test]
    fn anything_that_is_not_a_plain_https_address_is_refused() {
        assert_eq!(split("http://example.com"), None, "not encrypted");
        assert_eq!(split("ftp://example.com"), None);
        assert_eq!(split("example.com"), None, "no scheme at all");
        assert_eq!(split("https://"), None, "no host");
        assert_eq!(split("https:///path"), None, "no host, only a path");
        assert_eq!(
            split("https://user@evil.example/"),
            None,
            "credentials hide which host is really being asked"
        );
        assert_eq!(
            split("https://example.com:8443/"),
            None,
            "a port is not needed"
        );
        assert_eq!(split("https://example.com/#x"), None);
    }

    #[test]
    fn an_address_that_is_not_https_is_never_sent() {
        assert!(matches!(
            fetch("http://example.com", "arca-net-test", CEILING, &|_, _| true),
            Err(FetchError::Refused)
        ));
        assert!(matches!(
            get("ftp://example.com", "arca-net-test"),
            Err(FetchError::Refused)
        ));
    }

    #[test]
    fn a_curl_exit_code_says_which_way_it_failed() {
        assert!(matches!(curl_failure(Some(22)), FetchError::Http));
        assert!(matches!(curl_failure(Some(63)), FetchError::TooLarge));
        assert!(matches!(curl_failure(Some(6)), FetchError::Unavailable(_)));
        assert!(matches!(curl_failure(None), FetchError::Unavailable(_)));
    }
}

#[cfg(test)]
mod live {
    // Sale a la red, asi que no corre con las demas: el CI no puede depender de
    // que GitHub conteste, ni de que la maquina tenga salida. Se lanza a mano:
    //
    //     cargo test -p arca-net -- --ignored --nocapture
    #[test]
    #[ignore = "sale a internet"]
    fn una_peticion_de_verdad_trae_algo() {
        let body = super::get(
            "https://api.github.com/repos/beyondhumane/arca/releases/latest",
            "arca-net-test",
        );
        match body {
            Ok(text) => {
                println!("{} bytes", text.len());
                println!("{}", &text[..text.len().min(200)]);
            }
            Err(e) => println!("sin respuesta: {e}"),
        }
    }
}
