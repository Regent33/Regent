//! The reader pump, driven with a fake `Read` so no shell is involved.

use super::*;
use std::sync::{Arc, Mutex};

/// Collects everything the pump emits, plus whether it reported the exit.
#[derive(Default)]
struct Sink {
    chunks: Mutex<Vec<String>>,
    exited: Mutex<bool>,
}

impl Sink {
    fn hooks(self: &Arc<Self>) -> (Emit, OnExit) {
        let for_emit = Arc::clone(self);
        let for_exit = Arc::clone(self);
        (
            Box::new(move |_id, payload| {
                for_emit.chunks.lock().unwrap().push(payload.to_owned());
            }),
            Box::new(move |_id| {
                *for_exit.exited.lock().unwrap() = true;
            }),
        )
    }

    /// Every emitted batch, base64-decoded and concatenated.
    fn decoded(&self) -> Vec<u8> {
        self.chunks
            .lock()
            .unwrap()
            .iter()
            .flat_map(|c| B64.decode(c).expect("emitted payloads are valid base64"))
            .collect()
    }
}

/// Waits for the reader thread to finish rather than sleeping a fixed guess.
fn wait_for_exit(sink: &Arc<Sink>) {
    for _ in 0..200 {
        if *sink.exited.lock().unwrap() {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("the pump never reported the exit");
}

#[test]
fn output_arrives_intact_and_the_exit_is_reported() {
    let sink = Arc::new(Sink::default());
    let (emit, on_exit) = sink.hooks();
    let data = b"hello from the shell\r\n".to_vec();

    spawn(
        "p1".into(),
        Box::new(std::io::Cursor::new(data.clone())),
        emit,
        on_exit,
    );
    wait_for_exit(&sink);

    assert_eq!(sink.decoded(), data);
}

/// The reason base64 is here at all: a multi-byte character split across two
/// reads must survive. Raw reads placed into JSON strings would corrupt exactly
/// this case, and it is invisible until someone `cat`s a file with an emoji in it.
#[test]
fn a_multibyte_character_split_across_reads_survives() {
    // READ_CHUNK (8192) IS a multiple of 4, so a run of 4-byte characters would
    // land every boundary cleanly and prove nothing. One leading ASCII byte
    // shifts every subsequent boundary into the middle of a character.
    let mut data = b"x".to_vec();
    data.extend("😀".repeat(3000).as_bytes());

    let sink = Arc::new(Sink::default());
    let (emit, on_exit) = sink.hooks();
    spawn(
        "p2".into(),
        Box::new(std::io::Cursor::new(data.clone())),
        emit,
        on_exit,
    );
    wait_for_exit(&sink);

    let decoded = sink.decoded();
    assert_eq!(decoded, data, "bytes must round-trip exactly");
    assert!(
        String::from_utf8(decoded).is_ok(),
        "and reassemble into valid UTF-8"
    );
}

/// A flood must not become one message per read. This is the whole point of the
/// flush tick.
#[test]
fn a_flood_is_batched_rather_than_emitted_per_read() {
    let data = vec![b'a'; 2 * 1024 * 1024]; // 2 MiB = 256 reads at READ_CHUNK
    let sink = Arc::new(Sink::default());
    let (emit, on_exit) = sink.hooks();

    spawn(
        "p3".into(),
        Box::new(std::io::Cursor::new(data.clone())),
        emit,
        on_exit,
    );
    wait_for_exit(&sink);

    assert_eq!(sink.decoded().len(), data.len(), "nothing is dropped");
    let batches = sink.chunks.lock().unwrap().len();
    assert!(
        batches < 256,
        "256 reads must collapse into fewer batches, got {batches}"
    );
}

/// A `Read` that yields `data` once and then BLOCKS forever, the way a real pty
/// does while its shell waits for input.
struct ThenBlocks {
    data: Option<Vec<u8>>,
}

impl std::io::Read for ThenBlocks {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if let Some(data) = self.data.take() {
            let n = data.len().min(buf.len());
            buf[..n].copy_from_slice(&data[..n]);
            return Ok(n);
        }
        // No EOF, no error — exactly what a pty does when the shell is waiting.
        loop {
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

/// The deadlock, pinned.
///
/// The first pump batched inside the read loop, flushing only when the NEXT read
/// returned. PowerShell opens by emitting the 4-byte cursor query `\x1b[6n` and
/// then blocking until a terminal answers — so those bytes sat in the buffer, no
/// further read ever came, and the terminal opened blank forever. Diagnosed
/// 2026-07-30 with a raw portable-pty probe after two wrong guesses (test
/// contention, then the slave-drop race).
///
/// A `Cursor` cannot express this: it reports EOF immediately and the post-loop
/// flush hides the bug. The reader has to go quiet WITHOUT ending.
#[test]
fn output_followed_by_silence_is_flushed_without_another_read() {
    let sink = Arc::new(Sink::default());
    let (emit, on_exit) = sink.hooks();
    let dsr = b"\x1b[6n".to_vec();

    spawn(
        "p5".into(),
        Box::new(ThenBlocks {
            data: Some(dsr.clone()),
        }),
        emit,
        on_exit,
    );

    // Generously longer than FLUSH_INTERVAL, but nowhere near "forever": if this
    // needs seconds, the timer is not driving the flush.
    for _ in 0..40 {
        if !sink.chunks.lock().unwrap().is_empty() {
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }

    assert_eq!(
        sink.decoded(),
        dsr,
        "a shell waiting for a reply must still have its bytes delivered"
    );
    assert!(
        !*sink.exited.lock().unwrap(),
        "and it has NOT exited — it is waiting, which is different"
    );
}

/// A shell that prints nothing must not emit an empty payload — the client would
/// decode it to zero bytes and write nothing, so it is pure noise.
#[test]
fn silence_emits_nothing_at_all() {
    let sink = Arc::new(Sink::default());
    let (emit, on_exit) = sink.hooks();

    spawn(
        "p4".into(),
        Box::new(std::io::Cursor::new(Vec::new())),
        emit,
        on_exit,
    );
    wait_for_exit(&sink);

    assert!(sink.chunks.lock().unwrap().is_empty());
}
