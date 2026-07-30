//! Interactive terminals for the workspace panel (ADR-044).
//!
//! A real PTY, not the agent's `terminal` tool. That tool is request/response
//! with a timeout and an output cap — correct for "run this and tell me what
//! happened", and structurally unable to host a REPL, a progress bar, Ctrl+C, or
//! anything that asks a question. Those need a pseudo-terminal, which is what
//! `portable-pty` gives us (ConPTY on Windows, `forkpty` on Unix).
//!
//! **This is a UI surface, not a model capability.** No `pty` tool is
//! registered, so the agent cannot reach any of this; agent tools stay jailed
//! and unchanged. Containment is structural rather than a check somebody has to
//! remember: `pty.*` lives on the stdio dispatcher, the HTTP ingress exposes
//! only `/health` and `/v1/chat`, and the gateway is a separate binary that does
//! not route the dispatcher. See the ADR for the standing constraint on any
//! future transport.
//!
//! The terminal STARTS at the session's workspace root but is not confined to
//! it — `cd ..` works. The path jail exists to contain prompt injection; a human
//! typing is the user, not injected input, and a shell that cannot leave its
//! folder is not a terminal (owner decision, 2026-07-30).

pub mod pump;
pub mod shell;

use crate::domain::errors::DeaconError;
use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use regent_kernel::RegentError;
use std::collections::HashMap;
use std::io::Write;
use std::path::Path;
use std::sync::{Arc, Mutex};

/// One live terminal.
struct Pty {
    /// Writer half — keystrokes go here. `Box<dyn Write>` is what portable-pty
    /// hands back, and it is NOT `Sync`, hence the mutex rather than an RwLock.
    writer: Mutex<Box<dyn Write + Send>>,
    /// Kept so `resize` can reach the pty after open, and so dropping the entry
    /// closes the master and lets the child see EOF.
    ///
    /// Behind a mutex purely for `Sync`: `MasterPty` is `Send` but not `Sync`, and
    /// without this the whole `Dispatcher` stops being `Sync` — which surfaces as
    /// "future cannot be sent between threads safely" in unrelated files that
    /// merely hold a `&Dispatcher` across an await.
    master: Mutex<Box<dyn portable_pty::MasterPty + Send>>,
    child: Mutex<Box<dyn portable_pty::Child + Send + Sync>>,
}

fn tool_err(message: String) -> DeaconError {
    DeaconError::Core(RegentError::Tool {
        tool: "pty".into(),
        message,
    })
}

/// Every open terminal, keyed by the id handed to the client.
///
/// A plain `Mutex<HashMap>` rather than anything cleverer: opens and closes are
/// human-paced (a person clicking a tab), and the only hot path — reading
/// output — happens on the per-pty reader task without touching this map.
#[derive(Default)]
pub struct PtyRegistry {
    open: Mutex<HashMap<String, Arc<Pty>>>,
}

impl PtyRegistry {
    /// Spawns a shell and starts pumping its output through `emit`.
    ///
    /// `emit` is called with `(pty_id, base64_chunk)` for output and the reader
    /// task calls `on_exit` once when the child is gone. Both are injected so
    /// this type never learns about JSON-RPC.
    pub fn open(
        self: &Arc<Self>,
        id: String,
        cwd: Option<&Path>,
        size: (u16, u16),
        emit: pump::Emit,
        on_exit: pump::OnExit,
    ) -> Result<(), DeaconError> {
        let (cols, rows) = size;
        let pair = NativePtySystem::default()
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| tool_err(format!("cannot open a terminal: {e}")))?;

        let mut command = CommandBuilder::new(shell::resolve());
        // Only when the directory really exists: a stale stored workspace would
        // otherwise fail the spawn outright, and a terminal in the wrong folder
        // beats no terminal at all.
        if let Some(dir) = cwd.filter(|p| p.is_dir()) {
            command.cwd(dir);
        }
        // Tells everything downstream (git, ls, cargo, npm) that colour is
        // welcome. xterm.js is a real xterm; claiming "dumb" would strip it.
        command.env("TERM", "xterm-256color");

        let child = pair
            .slave
            .spawn_command(command)
            .map_err(|e| tool_err(format!("cannot start the shell: {e}")))?;

        // Dropped immediately, on every platform: while the deacon holds the
        // slave open, the child's exit never reaches the reader as EOF and the
        // terminal sits there looking alive after `exit`.
        drop(pair.slave);

        let reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| tool_err(format!("cannot read the terminal: {e}")))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|e| tool_err(format!("cannot write to the terminal: {e}")))?;

        let entry = Arc::new(Pty {
            writer: Mutex::new(writer),
            master: Mutex::new(pair.master),
            child: Mutex::new(child),
        });
        self.open.lock().unwrap().insert(id.clone(), entry);
        pump::spawn(id, reader, emit, on_exit);
        Ok(())
    }

    /// Feeds keystrokes to the shell. Unknown ids are an error, not a silent
    /// drop — a client typing into a dead terminal needs to be told.
    pub fn write(&self, id: &str, bytes: &[u8]) -> Result<(), DeaconError> {
        let pty = self.get(id)?;
        let mut writer = pty.writer.lock().unwrap();
        writer
            .write_all(bytes)
            .and_then(|()| writer.flush())
            .map_err(|e| tool_err(format!("terminal write failed: {e}")))
    }

    /// Tells the shell its window changed, so line editing and full-screen
    /// programs reflow instead of wrapping at the old width.
    pub fn resize(&self, id: &str, cols: u16, rows: u16) -> Result<(), DeaconError> {
        self.get(id)?
            .master
            .lock()
            .unwrap()
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| tool_err(format!("terminal resize failed: {e}")))
    }

    /// Kills the shell and forgets it. Idempotent: closing an already-closed
    /// terminal is what a client does on unmount, and it is not an error.
    pub fn close(&self, id: &str) {
        let Some(pty) = self.open.lock().unwrap().remove(id) else {
            return;
        };
        let _ = pty.child.lock().unwrap().kill();
    }

    /// Kills every terminal — deacon shutdown, so no shell outlives the process
    /// that owns it.
    pub fn close_all(&self) {
        let entries: Vec<Arc<Pty>> = self.open.lock().unwrap().drain().map(|(_, v)| v).collect();
        for pty in entries {
            let _ = pty.child.lock().unwrap().kill();
        }
    }

    fn get(&self, id: &str) -> Result<Arc<Pty>, DeaconError> {
        self.open
            .lock()
            .unwrap()
            .get(id)
            .map(Arc::clone)
            .ok_or_else(|| tool_err(format!("unknown terminal {id}")))
    }
}
