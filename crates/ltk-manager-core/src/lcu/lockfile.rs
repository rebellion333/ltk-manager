//! The League client's `lockfile`: where it listens, and the password to it.
//!
//! Written by `LeagueClient.exe` into the install root as
//! `LeagueClient:<pid>:<port>:<password>:<protocol>`, and left behind when the
//! client dies without cleaning up. A parsed lockfile is therefore a claim, and
//! [`LeagueLockfile::is_live`] is what checks it against the process table.

use std::path::Path;

use fs_err as fs;

/// One parsed `lockfile`.
#[derive(Clone, PartialEq, Eq)]
pub struct LeagueLockfile {
    pub name: String,
    pub pid: u32,
    pub port: u16,
    pub password: String,
    pub protocol: String,
}

/// Debug without the password, so a log line never carries it.
impl std::fmt::Debug for LeagueLockfile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LeagueLockfile")
            .field("name", &self.name)
            .field("pid", &self.pid)
            .field("port", &self.port)
            .field("password", &"<redacted>")
            .field("protocol", &self.protocol)
            .finish()
    }
}

impl LeagueLockfile {
    /// Parse the file's contents.
    ///
    /// The five-part form is the one every current client writes. The four-part
    /// form (`name:port:password:protocol`) is kept for the older clients the
    /// hot-reload path already tolerated. Anything else is `None`.
    pub fn parse(raw: &str) -> Option<Self> {
        let parts: Vec<&str> = raw.trim().split(':').collect();
        let (name, pid, port, password, protocol) = match parts.as_slice() {
            [name, pid, port, password, protocol] => {
                (*name, pid.parse().ok()?, *port, *password, *protocol)
            }
            [name, port, password, protocol] => (*name, 0, *port, *password, *protocol),
            _ => return None,
        };
        Some(Self {
            name: name.to_string(),
            pid,
            port: port.parse().ok()?,
            password: password.to_string(),
            protocol: protocol.to_string(),
        })
    }

    /// The lockfile under a League install root, when one is there and parses.
    pub fn read(league_root: &Path) -> Option<Self> {
        let path = league_root.join("lockfile");
        let raw = match fs::read_to_string(&path) {
            Ok(raw) => raw,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
            Err(e) => {
                tracing::debug!("Could not read {}: {e}", path.display());
                return None;
            }
        };
        let parsed = Self::parse(&raw);
        if parsed.is_none() {
            tracing::warn!("Unrecognised lockfile format at {}", path.display());
        }
        parsed
    }

    /// Whether the process the lockfile names is still running.
    ///
    /// A lockfile from a client that crashed stays on disk, and connecting to
    /// its port would wait on nothing. A four-part lockfile names no pid and is
    /// taken at its word.
    pub fn is_live(&self) -> bool {
        if self.pid == 0 {
            return true;
        }
        process_exists(self.pid)
    }

    /// `https://127.0.0.1:<port>`, the base of every request.
    pub fn base_url(&self) -> String {
        format!("{}://127.0.0.1:{}", self.protocol, self.port)
    }
}

#[cfg(windows)]
fn process_exists(pid: u32) -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, STILL_ACTIVE};
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    // SAFETY: `OpenProcess` takes plain values and returns a handle or null,
    // and the handle is closed on every path below.
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return false;
        }
        let mut code: u32 = 0;
        let queried = GetExitCodeProcess(handle, &mut code);
        CloseHandle(handle);
        queried != 0 && code == STILL_ACTIVE as u32
    }
}

#[cfg(not(windows))]
fn process_exists(pid: u32) -> bool {
    Path::new(&format!("/proc/{pid}")).exists()
}

#[cfg(test)]
mod tests;
