//! A real terminal, end to end (ADR-044).
//!
//! The unit tests cover shell resolution and the reader pump with a fake `Read`.
//! This spawns an actual shell through `PtyRegistry`, types a command into it,
//! and waits for the output — the one test that fails if the ConPTY/forkpty
//! wiring, the slave-drop, the writer, or the reader thread is wrong.

use regent_deacon::PtyRegistry;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Accumulates decoded output from every batch the pump emits.
#[derive(Default)]
struct Output {
    text: Mutex<String>,
    exited: Mutex<bool>,
}

impl Output {
    fn hooks(self: &Arc<Self>) -> (regent_deacon::PtyEmit, regent_deacon::PtyOnExit) {
        use base64::Engine as _;
        let for_data = Arc::clone(self);
        let for_exit = Arc::clone(self);
        (
            Box::new(move |_id, payload| {
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(payload)
                    .expect("the pump emits valid base64");
                for_data
                    .text
                    .lock()
                    .unwrap()
                    .push_str(&String::from_utf8_lossy(&bytes));
            }),
            Box::new(move |_id| {
                *for_exit.exited.lock().unwrap() = true;
            }),
        )
    }

    /// Polls until `needle` has appeared at least `times`, or gives up. A real
    /// shell prints a banner and a prompt on its own schedule, so this waits for
    /// the thing under test rather than sleeping a guess and hoping.
    ///
    /// Counting rather than merely detecting matters here: a shell ECHOES the
    /// command line before running it, so the first occurrence proves only that
    /// the keystrokes landed. Waiting for one and then asserting on two is a race
    /// this test lost on the first run.
    fn wait_for_times(&self, needle: &str, times: usize, within: Duration) -> bool {
        let deadline = Instant::now() + within;
        while Instant::now() < deadline {
            if self.text.lock().unwrap().matches(needle).count() >= times {
                return true;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        false
    }

    fn wait_for(&self, needle: &str, within: Duration) -> bool {
        self.wait_for_times(needle, 1, within)
    }

    fn snapshot(&self) -> String {
        self.text.lock().unwrap().clone()
    }
}

fn type_line(registry: &PtyRegistry, id: &str, line: &str) {
    // `\r`, not `\n`: a pty carries what a keyboard sends, and Enter is carriage
    // return. `\n` leaves cmd/PowerShell waiting for the rest of the line.
    registry
        .write(id, format!("{line}\r").as_bytes())
        .expect("the terminal accepts keystrokes");
}

/// Opens a shell in a temp dir, runs `echo`, and reads the result back.
///
/// The marker is deliberately not a bare word: a shell echoes the command line
/// itself before running it, so asserting on "hello" would pass on the echo alone
/// and prove nothing about execution. `REGENT` + a split literal appears once in
/// the typed command and once in its output, and the test waits for TWO.
#[test]
fn a_real_shell_runs_a_command_and_returns_its_output() {
    let dir = tempfile::tempdir().unwrap();
    let registry = Arc::new(PtyRegistry::default());
    let output = Arc::new(Output::default());
    let (emit, on_exit) = output.hooks();

    registry
        .open("t1".into(), Some(dir.path()), (80, 24), emit, on_exit)
        .expect("a pty opens on this platform");

    // A REAL requirement, discovered by this test failing with `\x1b[6n` as the
    // only output: PowerShell opens by sending DSR (Device Status Report, "where
    // is the cursor?") and BLOCKS until something answers. A terminal emulator
    // replies `\x1b[row;colR` — xterm.js does it automatically, which is why the
    // app works — so this test has to play the terminal too, or the shell never
    // reaches its prompt and nothing else is testable.
    if output.wait_for("\u{1b}[6n", Duration::from_secs(10)) {
        registry
            .write("t1", b"\x1b[1;1R")
            .expect("answering the cursor query");
    }

    // Then wait for the shell to actually be listening. Keystrokes sent before
    // the prompt exists are dropped, and on Windows the ConPTY banner takes a
    // moment.
    std::thread::sleep(Duration::from_millis(600));
    type_line(&registry, "t1", "echo REGENT-PTY-OK");

    // Twice: once as the echoed command line, once as the command's own output.
    // One occurrence would mean the shell received the keystrokes but never ran
    // them, which is exactly the failure mode a bare `contains` would miss.
    let ran = output.wait_for_times("REGENT-PTY-OK", 2, Duration::from_secs(20));
    let text = output.snapshot();
    assert!(ran, "the command never executed within 20s; got: {text:?}");

    registry.close("t1");
}

/// Closing must actually kill the shell, or every opened terminal leaks a
/// process for the life of the deacon.
#[test]
fn closing_ends_the_shell() {
    let registry = Arc::new(PtyRegistry::default());
    let output = Arc::new(Output::default());
    let (emit, on_exit) = output.hooks();

    registry
        .open("t2".into(), None, (80, 24), emit, on_exit)
        .expect("a pty opens");
    std::thread::sleep(Duration::from_millis(400));
    registry.close("t2");

    // The reader thread sees EOF once the child is gone and reports the exit.
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        if *output.exited.lock().unwrap() {
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    assert!(
        *output.exited.lock().unwrap(),
        "the shell survived close() — every terminal would leak a process"
    );

    // And a write to a closed terminal is an error rather than a silent no-op:
    // a client typing into a dead shell has to be told.
    assert!(registry.write("t2", b"x").is_err());
}

/// Resizing an unknown terminal is an error, not a panic — the client may resize
/// on a layout pass that races its own close.
#[test]
fn operations_on_an_unknown_terminal_are_errors() {
    let registry = Arc::new(PtyRegistry::default());
    assert!(registry.resize("nope", 100, 40).is_err());
    assert!(registry.write("nope", b"x").is_err());
    // Close stays idempotent — unmount calls it unconditionally.
    registry.close("nope");
}

/// The cwd handed to the shell must be usable by tools that shell out through
/// CMD.EXE.
///
/// `resolve_workspace_root` canonicalizes, which on Windows yields the
/// extended-length form. PowerShell accepts it as a cwd, so the prompt LOOKED
/// right — but CMD.EXE refuses it ("UNC paths are not supported. Defaulting to
/// Windows directory"), so `flutter`, `npm` and every other .bat/.cmd shim
/// silently ran in C:\Windows and reported the project missing. Reported
/// 2026-07-30.
///
/// Drives the real canonicalized path rather than a hand-built string, so this
/// fails if canonicalize ever changes shape.
///
/// The probe is a file the child has to FIND, not a string it has to echo.
/// `cmd /c echo CWDOK` printed the marker from any directory on earth, so the
/// assertion passed on a wrong cwd and only the UNC warning below carried the
/// real signal — and that warning is Windows-only, which left the Unix side of
/// this test proving nothing at all. It also hard-coded `cmd`, so on Linux it
/// asserted against "Command 'cmd' not found" and had never once been green.
/// Reading a file that exists only in the project directory is the same check
/// on both platforms, and it still reproduces the original bug: CMD.EXE handed
/// an extended-length cwd runs in C:\Windows, where this file is not.
#[test]
fn the_shell_starts_in_a_directory_tools_can_actually_use() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("proj");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(project.join("cwd-probe.txt"), "CWDOK").unwrap();
    let canonical = std::fs::canonicalize(&project).expect("canonicalize");

    let registry = Arc::new(PtyRegistry::default());
    let output = Arc::new(Output::default());
    let (emit, on_exit) = output.hooks();
    registry
        .open("t3".into(), Some(&canonical), (100, 30), emit, on_exit)
        .expect("a pty opens");

    if output.wait_for("\u{1b}[6n", Duration::from_secs(10)) {
        registry
            .write("t3", b"\x1b[1;1R")
            .expect("answer the cursor query");
    }
    std::thread::sleep(Duration::from_millis(600));

    // A CHILD has to read it, not the shell: on Windows `type` is a PowerShell
    // builtin and would inherit the cwd PowerShell already accepted, testing
    // nothing — CMD.EXE is the process that refuses the extended-length form.
    // On Unix `cat` is that child. The typed line carries the file NAME and the
    // output carries the CONTENT, so one occurrence of the marker is
    // unambiguous where the old echo needed two.
    let probe = if cfg!(windows) {
        "cmd /c type cwd-probe.txt"
    } else {
        "cat cwd-probe.txt"
    };
    type_line(&registry, "t3", probe);
    let ran = output.wait_for("CWDOK", Duration::from_secs(20));
    let text = output.snapshot();
    registry.close("t3");

    assert!(
        ran,
        "the child could not read a file in its own cwd, so it did not start there; got: {text:?}"
    );
    assert!(
        !text.contains("UNC paths are not supported"),
        "the cwd reached a child process in extended-length form; got: {text:?}"
    );
}
