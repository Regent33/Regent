//! The human is superior: a host-side watcher that (1) notices when a real
//! person touches the keyboard or mouse while the agent is acting, and (2)
//! offers a global emergency stop, **Ctrl+Alt+Shift+Esc**.
//!
//! Windows: one thread installs `WH_KEYBOARD_LL` + `WH_MOUSE_LL` hooks and a
//! `RegisterHotKey` chord, then runs a message loop for life. Everything the
//! agent injects (`SendInput` / `keybd_event` / `mouse_event`, which is what
//! `SendKeys` and the PowerShell backend use) carries the `*_INJECTED` flag,
//! so an unflagged key-down / button / wheel event is a human. Mouse *moves*
//! are ignored: brushing a touchpad is not a takeover. Neither hook nor hotkey
//! fires while an elevated window or the secure desktop is up (UIPI) — known,
//! accepted.
//!
//! Other platforms: every function is a no-op that reports "no human input"
//! and "not halted", so callers need no `cfg`.
//!
//! State is process-global on purpose: there is one keyboard, and the e-stop
//! must outrank every session.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Epoch millis of the last non-injected human input event (0 = never).
static LAST_HUMAN_MS: AtomicU64 = AtomicU64::new(0);
/// Latched by the e-stop; cleared only by [`rearm`] (a new user turn).
static HALTED: AtomicBool = AtomicBool::new(false);
static ESTOP: OnceLock<tokio::sync::Notify> = OnceLock::new();

fn estop_notify() -> &'static tokio::sync::Notify {
    ESTOP.get_or_init(tokio::sync::Notify::new)
}

/// Epoch millis now.
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// When the last real human input happened (epoch millis; 0 = never seen).
pub fn last_human_input_ms() -> u64 {
    LAST_HUMAN_MS.load(Ordering::Relaxed)
}

/// Record a human input event. The hook thread calls this; tests call it to
/// simulate a person reaching for the keyboard.
pub fn note_human_input() {
    LAST_HUMAN_MS.store(now_ms(), Ordering::Relaxed);
}

/// Resolves once a human input event is recorded after `since_ms`. Polled,
/// not signalled: the recorder is a plain atomic written from a raw hook
/// callback, where anything heavier is a liability.
/// ponytail: 50ms poll — a chunk of typing is longer than that anyway.
pub async fn wait_for_human_input(since_ms: u64) {
    loop {
        if last_human_input_ms() > since_ms {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// `true` while the emergency stop is latched.
pub fn halted() -> bool {
    HALTED.load(Ordering::Relaxed)
}

/// Latch the emergency stop and wake whoever is waiting to cancel turns.
pub fn estop() {
    HALTED.store(true, Ordering::Relaxed);
    estop_notify().notify_one();
}

/// Clear the latch. Called when the user sends a new message — a deliberate
/// act, unlike a second (panicked) press of the chord, which must not re-arm.
pub fn rearm() {
    HALTED.store(false, Ordering::Relaxed);
}

/// Resolves each time the e-stop fires (a permit is kept if nobody waits).
pub async fn estop_notified() {
    estop_notify().notified().await;
}

/// The chord, for messages to the user and the model.
pub const ESTOP_CHORD: &str = "Ctrl+Alt+Shift+Esc";

/// Start the watcher thread once. Idempotent; a no-op off Windows.
pub fn start() {
    static STARTED: OnceLock<()> = OnceLock::new();
    STARTED.get_or_init(|| {
        #[cfg(windows)]
        windows::spawn();
    });
}

#[cfg(windows)]
mod windows {
    use super::{estop, note_human_input};
    use std::ffi::c_void;

    #[repr(C)]
    struct Point {
        x: i32,
        y: i32,
    }
    #[repr(C)]
    struct Msg {
        hwnd: *mut c_void,
        message: u32,
        wparam: usize,
        lparam: isize,
        time: u32,
        pt: Point,
    }
    /// KBDLLHOOKSTRUCT: flags at offset 8.
    #[repr(C)]
    struct KbdLl {
        _vk: u32,
        _scan: u32,
        flags: u32,
        _time: u32,
        _extra: usize,
    }
    /// MSLLHOOKSTRUCT: `mouseData` precedes `flags`, so flags sit at offset 12.
    #[repr(C)]
    struct MsLl {
        _pt: Point,
        _mouse_data: u32,
        flags: u32,
        _time: u32,
        _extra: usize,
    }
    type HookProc = unsafe extern "system" fn(i32, usize, isize) -> isize;

    // Hand-declared rather than a crate: four functions with scalar/pointer
    // signatures, and no feature flags to guess at.
    #[link(name = "user32")]
    unsafe extern "system" {
        fn SetWindowsHookExW(id: i32, f: HookProc, hmod: *mut c_void, tid: u32) -> *mut c_void;
        fn CallNextHookEx(h: *mut c_void, code: i32, w: usize, l: isize) -> isize;
        fn GetMessageW(msg: *mut Msg, hwnd: *mut c_void, min: u32, max: u32) -> i32;
        fn RegisterHotKey(hwnd: *mut c_void, id: i32, mods: u32, vk: u32) -> i32;
    }

    const WH_KEYBOARD_LL: i32 = 13;
    const WH_MOUSE_LL: i32 = 14;
    const LLKHF_INJECTED: u32 = 0x10;
    const LLMHF_INJECTED: u32 = 0x01;
    const WM_KEYDOWN: u32 = 0x0100;
    const WM_SYSKEYDOWN: u32 = 0x0104;
    const WM_LBUTTONDOWN: u32 = 0x0201;
    const WM_RBUTTONDOWN: u32 = 0x0204;
    const WM_MBUTTONDOWN: u32 = 0x0207;
    const WM_MOUSEWHEEL: u32 = 0x020A;
    const WM_XBUTTONDOWN: u32 = 0x020B;
    const WM_MOUSEHWHEEL: u32 = 0x020E;
    const WM_HOTKEY: u32 = 0x0312;
    const MOD_ALT: u32 = 0x1;
    const MOD_CONTROL: u32 = 0x2;
    const MOD_SHIFT: u32 = 0x4;
    const MOD_NOREPEAT: u32 = 0x4000;
    const VK_ESCAPE: u32 = 0x1B;
    const ESTOP_ID: i32 = 0x5245; // 'RE'

    /// SAFETY: called by the system on the hook thread with `lparam` pointing
    /// at a live KBDLLHOOKSTRUCT for `code >= 0`; we only read its flags and
    /// always chain to the next hook.
    unsafe extern "system" fn keyboard(code: i32, wparam: usize, lparam: isize) -> isize {
        if code >= 0 && (wparam as u32 == WM_KEYDOWN || wparam as u32 == WM_SYSKEYDOWN) {
            let flags = unsafe { (*(lparam as *const KbdLl)).flags };
            if flags & LLKHF_INJECTED == 0 {
                note_human_input();
            }
        }
        unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) }
    }

    /// SAFETY: as `keyboard`, with an MSLLHOOKSTRUCT.
    unsafe extern "system" fn mouse(code: i32, wparam: usize, lparam: isize) -> isize {
        let is_press = matches!(
            wparam as u32,
            WM_LBUTTONDOWN
                | WM_RBUTTONDOWN
                | WM_MBUTTONDOWN
                | WM_XBUTTONDOWN
                | WM_MOUSEWHEEL
                | WM_MOUSEHWHEEL
        );
        if code >= 0 && is_press {
            let flags = unsafe { (*(lparam as *const MsLl)).flags };
            if flags & LLMHF_INJECTED == 0 {
                note_human_input();
            }
        }
        unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) }
    }

    pub(super) fn spawn() {
        std::thread::Builder::new()
            .name("regent-human-watch".into())
            .spawn(|| {
                // SAFETY: plain Win32 calls with null module/window handles,
                // which is the documented form for global LL hooks and a
                // thread-owned hotkey. The message loop keeps the hooks alive.
                unsafe {
                    let kb = SetWindowsHookExW(WH_KEYBOARD_LL, keyboard, std::ptr::null_mut(), 0);
                    let ms = SetWindowsHookExW(WH_MOUSE_LL, mouse, std::ptr::null_mut(), 0);
                    if kb.is_null() || ms.is_null() {
                        tracing::warn!("human-input hooks failed to install; user takeover will not pause the agent");
                    }
                    let mods = MOD_CONTROL | MOD_ALT | MOD_SHIFT | MOD_NOREPEAT;
                    if RegisterHotKey(std::ptr::null_mut(), ESTOP_ID, mods, VK_ESCAPE) == 0 {
                        tracing::warn!(chord = super::ESTOP_CHORD, "emergency-stop hotkey is taken by another app; only Stop in the app cancels the agent");
                    } else {
                        tracing::info!(chord = super::ESTOP_CHORD, "emergency stop armed");
                    }
                    let mut msg = Msg {
                        hwnd: std::ptr::null_mut(),
                        message: 0,
                        wparam: 0,
                        lparam: 0,
                        time: 0,
                        pt: Point { x: 0, y: 0 },
                    };
                    while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                        if msg.message == WM_HOTKEY && msg.wparam as i32 == ESTOP_ID {
                            tracing::warn!("EMERGENCY STOP pressed — cancelling every turn");
                            estop();
                        }
                    }
                }
            })
            .map(drop)
            .unwrap_or_else(|e| tracing::warn!(%e, "human-input watcher thread failed to start"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn human_input_after_the_mark_resolves_the_wait_and_estop_latches_until_rearm() {
        let mark = now_ms();
        tokio::time::sleep(Duration::from_millis(5)).await;
        // Nothing recorded after the mark: the wait must still be pending.
        let pending =
            tokio::time::timeout(Duration::from_millis(120), wait_for_human_input(mark)).await;
        assert!(pending.is_err(), "no human input yet, must keep waiting");
        note_human_input();
        tokio::time::timeout(Duration::from_millis(500), wait_for_human_input(mark))
            .await
            .expect("human input resolves the wait");

        assert!(!halted());
        estop();
        assert!(halted());
        tokio::time::timeout(Duration::from_millis(200), estop_notified())
            .await
            .expect("e-stop wakes the canceller even if it subscribed late");
        estop(); // a second press keeps it latched
        assert!(halted());
        rearm();
        assert!(!halted());
    }
}
