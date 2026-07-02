//! Filesystem handshake between `wcl wdoc review` (the agent's blocking wait)
//! and a running `wcl wdoc serve --comment` dev server.
//!
//! The two are separate processes, so they coordinate through marker files in a
//! per-document state dir under the OS temp dir — no network client, no port
//! discovery. The dir is keyed by the *canonicalized* root path so a relative
//! and an absolute invocation of serve / review resolve to the same place.
//!
//! Markers (each holds a "round" token — nanosecond stamp — as plain text):
//!   `serve`  — present while a dev server is up (presence = a live server).
//!   `agent`  — present while `review` is blocked (presence = "agent waiting"),
//!              its content the current wait's round.
//!   `ready`  — written by serve when the reviewer clicks "Send to agent",
//!              its content the released round; `review` consumes it and returns.
//!
//! Rounds increase each time `review` is invoked, so the UI can tell a fresh
//! wait (the "agent finished its changes" notification) from the current one.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// The handshake directory for one document, plus the marker operations both
/// sides use.
#[derive(Clone)]
pub struct Handshake {
    dir: PathBuf,
}

impl Handshake {
    /// Resolve the handshake dir for `root_file`:
    /// `<tmp>/wcl-wdoc-review/<hash-of-canonical-root>/`.
    pub fn new(root_file: &Path) -> Self {
        let canon = fs::canonicalize(root_file).unwrap_or_else(|_| root_file.to_path_buf());
        let hash = hash_path(canon.as_os_str().to_string_lossy().as_bytes());
        let dir = std::env::temp_dir().join("wcl-wdoc-review").join(hash);
        Self { dir }
    }

    fn serve_path(&self) -> PathBuf {
        self.dir.join("serve")
    }
    fn agent_path(&self) -> PathBuf {
        self.dir.join("agent")
    }
    fn ready_path(&self) -> PathBuf {
        self.dir.join("ready")
    }

    /// Create the state dir if needed.
    fn ensure_dir(&self) -> std::io::Result<()> {
        fs::create_dir_all(&self.dir)
    }

    // -- serve side --------------------------------------------------------

    /// Mark a live dev server and clear any stale `agent` / `ready` from a
    /// previous run, so the UI doesn't show a phantom "agent waiting".
    pub fn serve_started(&self) -> std::io::Result<()> {
        self.ensure_dir()?;
        let _ = fs::remove_file(self.agent_path());
        let _ = fs::remove_file(self.ready_path());
        fs::write(self.serve_path(), std::process::id().to_string())
    }

    /// Best-effort teardown when the dev server exits.
    pub fn serve_stopped(&self) {
        let _ = fs::remove_file(self.serve_path());
        let _ = fs::remove_file(self.ready_path());
    }

    /// Release the blocked `review` (the toolbar "Send to agent" button): write
    /// `ready` carrying the round the `agent` marker is currently waiting on (or
    /// `0` if none, which a later wait simply ignores).
    pub fn signal_ready(&self) -> std::io::Result<()> {
        self.ensure_dir()?;
        let round = self.agent_waiting().unwrap_or(0);
        fs::write(self.ready_path(), round.to_string())
    }

    /// The round the agent is currently waiting on, if any (presence of the
    /// `agent` marker). Used by the dev server's review-status endpoint.
    pub fn agent_waiting(&self) -> Option<u64> {
        read_round(&self.agent_path())
    }

    // -- review (agent) side ----------------------------------------------

    /// True if a dev server marker is present.
    pub fn server_alive(&self) -> bool {
        self.serve_path().exists()
    }

    /// Begin a wait: allocate a fresh round, drop any stale `ready`, and write
    /// the `agent` marker. Returns the round to poll on.
    pub fn begin_wait(&self) -> std::io::Result<u64> {
        self.ensure_dir()?;
        let round = now_nanos();
        let _ = fs::remove_file(self.ready_path());
        fs::write(self.agent_path(), round.to_string())?;
        Ok(round)
    }

    /// True once the reviewer has sent the current `round` (a `ready` marker
    /// whose round matches, or `0` for an untargeted release).
    ///
    /// `0` is the recovery path: if the dev server restarts mid-wait it
    /// clears the `agent` marker, so a subsequent "Send to agent" click
    /// can't know the round and writes `0`. Accepting it can't release a
    /// *stale* wait — [`Self::begin_wait`] deletes any leftover `ready`
    /// before the wait starts, so any `0` seen here was written after.
    pub fn released(&self, round: u64) -> bool {
        match read_round(&self.ready_path()) {
            Some(r) => r == round || r == 0,
            None => false,
        }
    }

    /// Clear this wait's markers once released (or on abort).
    pub fn end_wait(&self) {
        let _ = fs::remove_file(self.agent_path());
        let _ = fs::remove_file(self.ready_path());
    }
}

/// Read a round token (plain-text integer) from a marker file, if present.
fn read_round(path: &Path) -> Option<u64> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
}

fn now_nanos() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
        .max(1)
}

/// FNV-1a 64-bit over `bytes`, rendered as lowercase hex — a dependency-free,
/// stable directory name for the canonical root path.
fn hash_path(bytes: &[u8]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{h:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_root_same_dir() {
        let a = Handshake::new(Path::new("/some/root.wcl"));
        let b = Handshake::new(Path::new("/some/root.wcl"));
        assert_eq!(a.dir, b.dir);
    }

    #[test]
    fn ready_release_roundtrips() {
        let tmp = std::env::temp_dir().join(format!("wcl-review-test-{}", std::process::id()));
        let hs = Handshake { dir: tmp.clone() };
        hs.serve_started().unwrap();
        assert!(hs.server_alive());
        let round = hs.begin_wait().unwrap();
        assert_eq!(hs.agent_waiting(), Some(round));
        assert!(!hs.released(round));
        hs.signal_ready().unwrap();
        assert!(hs.released(round));
        hs.end_wait();
        assert_eq!(hs.agent_waiting(), None);
        hs.serve_stopped();
        let _ = fs::remove_dir_all(&tmp);
    }
}
