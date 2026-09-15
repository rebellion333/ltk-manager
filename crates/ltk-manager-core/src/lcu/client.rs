//! Read-only HTTP access to one League client.
//!
//! The client presents a self-signed certificate, so verification is off on
//! this one `reqwest` instance, which builds every URL itself from the lockfile
//! it was given and reaches nothing else. Reads answer `Option`, never
//! `Result`: every caller has a fallback, and "the client didn't answer" is
//! not a failure worth showing a user.

use std::time::Duration;

use base64::Engine;
use serde::de::DeserializeOwned;

use super::lockfile::LeagueLockfile;

/// One client, one port, one password.
#[derive(Clone)]
pub struct LcuClient {
    http: reqwest::blocking::Client,
    base_url: String,
    authorization: String,
}

impl std::fmt::Debug for LcuClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LcuClient")
            .field("base_url", &self.base_url)
            .finish_non_exhaustive()
    }
}

impl LcuClient {
    /// A client for the process the lockfile names.
    pub fn new(lockfile: &LeagueLockfile) -> Option<Self> {
        let http = reqwest::blocking::Client::builder()
            .danger_accept_invalid_certs(true)
            .timeout(Duration::from_secs(5))
            .build()
            .map_err(|e| tracing::warn!("Could not build the LCU HTTP client: {e}"))
            .ok()?;
        Some(Self {
            http,
            base_url: lockfile.base_url(),
            authorization: authorization(lockfile),
        })
    }

    /// `GET <path>` decoded as JSON, or `None` for any refusal, timeout or shape
    /// the type does not accept.
    pub fn get_json<T: DeserializeOwned>(&self, path: &str) -> Option<T> {
        let url = format!("{}{}", self.base_url, path);
        let response = self
            .http
            .get(&url)
            .header("Authorization", &self.authorization)
            .send()
            .map_err(|e| tracing::debug!("GET {path} failed: {e}"))
            .ok()?;
        let status = response.status();
        if !status.is_success() {
            tracing::debug!("GET {path} answered HTTP {status}");
            return None;
        }
        response
            .json::<T>()
            .map_err(|e| tracing::debug!("GET {path} did not decode: {e}"))
            .ok()
    }

    /// The base every request starts from, for a log line.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }
}

/// `Basic riot:<password>`, the header both HTTP and the WebSocket send.
pub(super) fn authorization(lockfile: &LeagueLockfile) -> String {
    let token =
        base64::engine::general_purpose::STANDARD.encode(format!("riot:{}", lockfile.password));
    format!("Basic {token}")
}
