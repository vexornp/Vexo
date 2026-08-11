//! `ureq`-based implementation of vexo's `HttpFetch` trait.
//!
//! This is the production HTTP fetcher for desktop platforms. It uses
//! `ureq` (blocking HTTP with rustls TLS) so no async runtime is needed.
//!
//! Mobile platforms will eventually use platform-native HTTP
//! (NSURLSession on iOS, OkHttp on Android) via separate crates
//! implementing the same `HttpFetch` trait.

use std::io::Read;

use url::Url;

use vexo::{FetchError, HttpFetch};

/// Production HTTP fetcher using `ureq` (blocking, rustls TLS).
///
/// Stateless — each `fetch` call creates a new `ureq::get` request.
/// A future optimization would hold an `ureq::Agent` for connection
/// pooling; v1 uses the simple stateless form.
pub struct UreqHttpFetch;

impl UreqHttpFetch {
    pub fn new() -> Self {
        Self
    }
}

impl Default for UreqHttpFetch {
    fn default() -> Self {
        Self::new()
    }
}

/// Maximum response body size (10 MB). Prevents unbounded memory on
/// malicious or accidentally-huge responses.
const MAX_BYTES: u64 = 10 * 1024 * 1024;

impl HttpFetch for UreqHttpFetch {
    fn fetch(&self, url: &Url) -> Result<Vec<u8>, FetchError> {
        let response = ureq::get(url.as_str())
            .call()
            .map_err(|e| FetchError::Network(e.to_string()))?;

        // Fast-fail on Content-Length if the server reports it.
        if let Some(len_str) = response.header("Content-Length") {
            if let Ok(len) = len_str.parse::<u64>() {
                if len > MAX_BYTES {
                    return Err(FetchError::TooLarge(len));
                }
            }
        }

        let mut bytes = Vec::new();
        response
            .into_reader()
            .take(MAX_BYTES)
            .read_to_end(&mut bytes)
            .map_err(|e| FetchError::Io(e.to_string()))?;

        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fetch_invalid_url_returns_network_error() {
        let fetcher = UreqHttpFetch::new();
        let url = Url::parse("http://127.0.0.1:1/nonexistent.png").unwrap();
        let result = fetcher.fetch(&url);
        assert!(result.is_err(), "fetching from a dead port should fail");
        match result.unwrap_err() {
            FetchError::Network(msg) => {
                // Expected — connection refused.
                assert!(msg.len() > 0);
            }
            other => panic!("expected Network error, got {:?}", other),
        }
    }
}
