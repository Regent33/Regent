//! The reader side of a PTY: bytes out of the shell, batched, base64-framed, and
//! handed to a sink that knows nothing about terminals.
//!
//! Why base64. PTY output is arbitrary bytes — escape sequences, and any encoding
//! the program being run feels like emitting. A JSON string requires valid UTF-8,
//! and a multi-byte character split across a read boundary is not valid UTF-8, so
//! putting raw reads into JSON corrupts exactly the text that straddles a chunk.
//! ~33% size for exactness is the right trade for a terminal.
//!
//! Why batching. `yes` produces megabytes a second. One notification per read
//! would drown the stdio channel and the webview both. Reads accumulate into a
//! buffer and flush on a short tick, so throughput costs bytes rather than
//! messages.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use std::io::Read;
use std::time::{Duration, Instant};

/// Called with `(pty_id, base64_payload)` for every flushed batch.
pub type Emit = Box<dyn Fn(&str, &str) + Send + Sync>;
/// Called once, with `(pty_id)`, when the shell is gone.
pub type OnExit = Box<dyn FnOnce(&str) + Send>;

/// How long output may sit in the buffer before it is sent.
///
/// ponytail: a fixed tick, not an adaptive scheduler. 16ms is one frame — below
/// what anyone perceives as lag while typing, and enough to collapse a flood into
/// ~60 messages a second instead of thousands. **Ceiling:** a program emitting
/// faster than the channel drains still grows `pending` between flushes; the
/// upgrade path is a byte cap with a "…output trimmed" marker, which is only
/// worth building once someone actually hits it.
const FLUSH_INTERVAL: Duration = Duration::from_millis(16);

/// Per read. Small enough to stay responsive on a single keystroke echo, large
/// enough that a flood needs few syscalls.
const READ_CHUNK: usize = 8 * 1024;

/// Starts the reader for one PTY: a blocking read thread feeding a batching
/// thread.
///
/// **Two threads, and the split is load-bearing.** The first version batched
/// inside the read loop — flush only if `FLUSH_INTERVAL` had elapsed *when the
/// next read returned*. That deadlocks against any shell that prints and then
/// waits, which is every shell: PowerShell opens by emitting the 4-byte cursor
/// query `\x1b[6n` and blocking until a terminal answers. Those 4 bytes arrive
/// ~1ms in, under the tick, so they sat unflushed — and no further read ever came,
/// because the shell was waiting for the reply to the bytes still in the buffer.
/// The terminal opened blank forever. A `Cursor`-based unit test cannot catch it:
/// EOF arrives immediately and the post-loop flush covers everything.
///
/// So the timer must be able to fire with no read in sight. `recv_timeout` gives
/// exactly that, and the elapsed check alongside it keeps a flood from starving
/// the timeout (under continuous data `recv_timeout` always returns `Ok` and
/// would never fire on its own).
///
/// Threads rather than tokio tasks: `portable_pty`'s reader is a blocking `Read`
/// with no async form, so a task would occupy a runtime worker for the life of
/// the terminal. Two per open terminal, and terminals are opened by a human
/// clicking a tab.
pub fn spawn(id: String, mut reader: Box<dyn Read + Send>, emit: Emit, on_exit: OnExit) {
    let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();

    std::thread::spawn(move || {
        let mut buf = vec![0u8; READ_CHUNK];
        loop {
            match reader.read(&mut buf) {
                // EOF: the shell exited and the master saw the slave close.
                Ok(0) => break,
                // A send error means the batcher is gone; nothing left to do.
                Ok(n) => {
                    if tx.send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
                // Any read error ends the terminal. Retrying a broken pty is how
                // you get a thread spinning forever on the same failure.
                Err(_) => break,
            }
        }
        // `tx` drops here, which is what tells the batcher the shell is gone.
    });

    std::thread::spawn(move || {
        let mut pending: Vec<u8> = Vec::new();
        let mut last_flush = Instant::now();
        loop {
            match rx.recv_timeout(FLUSH_INTERVAL) {
                Ok(chunk) => {
                    pending.extend_from_slice(&chunk);
                    // Under a flood the timeout never fires, so the elapsed check
                    // is what keeps batches moving.
                    if last_flush.elapsed() >= FLUSH_INTERVAL {
                        flush(&id, &mut pending, &emit);
                        last_flush = Instant::now();
                    }
                }
                // The quiet case — and the one the deadlock lived in. Whatever is
                // pending goes out now, with no further read required.
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    flush(&id, &mut pending, &emit);
                    last_flush = Instant::now();
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
        // Whatever the shell printed on its way out — often the only clue about
        // why it died — must not be dropped with the loop.
        flush(&id, &mut pending, &emit);
        on_exit(&id);
    });
}

fn flush(id: &str, pending: &mut Vec<u8>, emit: &Emit) {
    if pending.is_empty() {
        return;
    }
    emit(id, &B64.encode(&pending[..]));
    pending.clear();
}

#[cfg(test)]
#[path = "tests/pump.rs"]
mod tests;
