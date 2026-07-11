//! Windows WH_KEYBOARD_LL implementation + SendInput injection.
//! Follows plan: dedicated thread for hook + message loop, bidirectional KeyCode map,
//! eat original on remap, pass injected events, low latency decision inside callback.

use crate::{
    config::AppConfig, layout::Layout, loader, Event, EventKind, InputMatcher, KeyCode,
    KeyboardLayout, MatchAction, Modifiers, OutputSeq, OutputToken, SpecialKey,
};
use std::collections::HashSet;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::thread::{self, JoinHandle};
use windows::core::PWSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Input::Ime::ImmGetDefaultIMEWnd;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
    KEYEVENTF_UNICODE, VIRTUAL_KEY, VK_BACK, VK_CAPITAL, VK_CONVERT, VK_DOWN, VK_ESCAPE, VK_KANA,
    VK_KANJI, VK_LCONTROL, VK_LEFT, VK_LMENU, VK_LSHIFT, VK_LWIN, VK_NONCONVERT, VK_RCONTROL,
    VK_RETURN, VK_RIGHT, VK_RMENU, VK_RSHIFT, VK_RWIN, VK_SPACE, VK_TAB, VK_UP,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetForegroundWindow, GetMessageW, GetWindowThreadProcessId, SetWindowsHookExW,
    UnhookWindowsHookEx, KBDLLHOOKSTRUCT, LLKHF_INJECTED, MSG, WH_KEYBOARD_LL, WM_KEYDOWN,
    WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
};
use windows::Win32::UI::WindowsAndMessaging::{SendMessageTimeoutW, SMTO_ABORTIFHUNG};

static HOOK_STATE: OnceLock<Mutex<HookState>> = OnceLock::new();

/// When true, the active layout is configured to remap only while the IME is
/// OFF (see `AppConfig::activate_only_when_ime_off`). Read lock-free by the
/// IME poll thread; set on install/reload.
static IME_GATING: AtomicBool = AtomicBool::new(false);

/// Flush-timer dispatch rate, in milliseconds (`AppConfig::dispatch_rate_ms`):
/// how long the chord flush thread sleeps between wakeups. Read lock-free each
/// iteration so a live reload takes effect without restarting the thread. 0 is
/// clamped to 1ms.
static DISPATCH_RATE_MS: AtomicU64 = AtomicU64::new(5);

/// Last IME open state we logged, so transitions are logged once (not every
/// poll). -1 = unknown, 0 = closed/off, 1 = open/on.
static LAST_IME_LOG: std::sync::atomic::AtomicI8 = std::sync::atomic::AtomicI8::new(-1);

/// `AppConfig::enable_ctrl_space_ime_toggle`: when set, Ctrl+Space toggles
/// the IME instead of producing a space. Read lock-free in the hook's hot
/// path; set on install/reload.
static CTRL_SPACE_IME_TOGGLE: AtomicBool = AtomicBool::new(false);

/// Whether either Ctrl key is currently held, tracked from raw VK codes
/// (independent of the matcher) so the hot path can recognise the Ctrl+Space
/// chord without waiting on the matcher's own modifier tracking.
static CTRL_HELD: AtomicBool = AtomicBool::new(false);

/// Whether Space is currently held as part of an already-handled Ctrl+Space
/// toggle, so OS auto-repeat doesn't retoggle the IME on every repeat tick.
static CTRL_SPACE_DOWN: AtomicBool = AtomicBool::new(false);

/// SandS direct-input mode (`AppConfig::direct_input_mode`): what holding
/// `direct_input_key` does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum DirectInputMode {
    /// `direct_input_key` is inert.
    #[default]
    Off,
    /// While held, fully bypass remapping (raw physical-keyboard input).
    Raw,
    /// While held, switch the active layout to `ime_off_layout` (falls back
    /// to `Raw` if `ime_off_layout` is unset).
    ImeOff,
}

impl DirectInputMode {
    fn from_config_str(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "raw" => DirectInputMode::Raw,
            "ime_off" => DirectInputMode::ImeOff,
            _ => DirectInputMode::Off,
        }
    }
}

/// `WM_IME_CONTROL` / `IMC_GETOPENSTATUS`: query whether the IME is open
/// (Japanese conversion on). Not exported by the `windows` crate, so spelled
/// out here.
const WM_IME_CONTROL: u32 = 0x0283;
const IMC_GETOPENSTATUS: usize = 0x0005;

/// Ask one window's default IME whether it is open, via `WM_IME_CONTROL`.
/// Returns `None` if the window has no IME or the query times out.
unsafe fn ime_open_of(hwnd: windows::Win32::Foundation::HWND) -> Option<bool> {
    if hwnd.0 == 0 {
        return None;
    }
    let ime = ImmGetDefaultIMEWnd(hwnd);
    if ime.0 == 0 {
        return None;
    }
    let mut result: usize = 0;
    let ok = SendMessageTimeoutW(
        ime,
        WM_IME_CONTROL,
        WPARAM(IMC_GETOPENSTATUS),
        LPARAM(0),
        SMTO_ABORTIFHUNG,
        100,
        Some(&mut result as *mut usize),
    );
    if ok.0 == 0 {
        None // timed out or failed
    } else {
        Some(result != 0)
    }
}

/// Whether the focused control's IME is open (Japanese input mode), or `None`
/// if it can't be determined. We target the *focus* window of the foreground
/// thread (not just the top-level window) because that's where the IME context
/// actually lives; we fall back to the top-level foreground window.
/// Runs only on the poll thread (never inside the hook) — the underlying
/// `SendMessage` is blocking, so we use `SendMessageTimeoutW`.
fn ime_open_status() -> Option<bool> {
    use windows::Win32::UI::WindowsAndMessaging::{GetGUIThreadInfo, GUITHREADINFO};
    unsafe {
        let fg = GetForegroundWindow();
        if fg.0 == 0 {
            return None;
        }
        // Resolve the actual focused control of the foreground thread.
        let tid = GetWindowThreadProcessId(fg, None);
        let mut gui = GUITHREADINFO {
            cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };
        let focus = if GetGUIThreadInfo(tid, &mut gui).is_ok() && gui.hwndFocus.0 != 0 {
            gui.hwndFocus
        } else {
            fg
        };
        ime_open_of(focus).or_else(|| ime_open_of(fg))
    }
}

/// `IMC_SETOPENSTATUS`: set whether one window's default IME is open.
const IMC_SETOPENSTATUS: usize = 0x0006;

/// Set one window's default IME open/closed, via `WM_IME_CONTROL`. Mirrors
/// `ime_open_of`'s targeting (default IME window of `hwnd`).
unsafe fn ime_set_open_of(hwnd: windows::Win32::Foundation::HWND, open: bool) -> bool {
    if hwnd.0 == 0 {
        return false;
    }
    let ime = ImmGetDefaultIMEWnd(hwnd);
    if ime.0 == 0 {
        return false;
    }
    let ok = SendMessageTimeoutW(
        ime,
        WM_IME_CONTROL,
        WPARAM(IMC_SETOPENSTATUS),
        LPARAM(open as isize),
        SMTO_ABORTIFHUNG,
        100,
        None,
    );
    ok.0 != 0
}

/// Ctrl+Space handler: flip the focused control's IME open state. Targets
/// the same hwnd `ime_open_status` would read from, so the toggle direction
/// matches what the user currently sees.
fn toggle_ime_open_status() {
    use windows::Win32::UI::WindowsAndMessaging::{GetGUIThreadInfo, GUITHREADINFO};
    unsafe {
        let fg = GetForegroundWindow();
        if fg.0 == 0 {
            return;
        }
        let tid = GetWindowThreadProcessId(fg, None);
        let mut gui = GUITHREADINFO {
            cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };
        let focus = if GetGUIThreadInfo(tid, &mut gui).is_ok() && gui.hwndFocus.0 != 0 {
            gui.hwndFocus
        } else {
            fg
        };
        let target = if ime_open_of(focus).is_some() {
            focus
        } else {
            fg
        };
        if let Some(current) = ime_open_of(target) {
            ime_set_open_of(target, !current);
        }
    }
}

/// Recompute which layout should be active and whether remapping should be
/// bypassed, based on two independent inputs:
/// - `ime_open`: whether the IME is currently ON (irrelevant if gating is
///   disabled, in which case we always treat it as ON).
/// - `direct_input_active`: whether the configured `direct_input_key` is
///   currently held.
///
/// Either "IME is OFF" (when gating is enabled) or "direct-input key held"
/// wants the `ime_off_layout` (if configured) active; if neither wants it,
/// `normal_layout` is active. If `ime_off_layout` is unset, either condition
/// instead falls back to a full bypass (raw passthrough), preserving the
/// original behaviours of both features. No-op if nothing changed.
fn recompute_layout_locked(st: &mut HookState) {
    let gating = IME_GATING.load(Ordering::Relaxed);
    let ime_open = if gating { st.ime_open } else { true };

    // IME-off gating: prefer ime_off_layout while the IME is OFF, else bypass.
    let ime_wants_off_layout = !ime_open && st.ime_off_layout.is_some();
    let ime_wants_bypass = !ime_open && st.ime_off_layout.is_none();

    // SandS direct-input key held: `direct_input_mode` decides what happens.
    // `ImeOff` switches to ime_off_layout (falling back to `Raw`/bypass if
    // unset); `Raw` fully bypasses; `Off` falls back to raw bypass (so the
    // configured direct-input key always produces physical-key output).
    let direct_wants_off_layout = st.direct_input_active
        && st.direct_input_mode == DirectInputMode::ImeOff
        && st.ime_off_layout.is_some();
    let direct_wants_bypass = st.direct_input_active
        && match st.direct_input_mode {
            DirectInputMode::Raw | DirectInputMode::Off => true,
            DirectInputMode::ImeOff => st.ime_off_layout.is_none(),
        };

    let want_off = ime_wants_off_layout || direct_wants_off_layout;
    if want_off != st.using_ime_off_layout {
        st.using_ime_off_layout = want_off;
        st.layout = if want_off {
            st.ime_off_layout.clone().unwrap()
        } else {
            st.normal_layout.clone()
        };
        st.keyboard = st.layout.keyboard;
        st.matcher.clear();
    }
    let bypass = ime_wants_bypass || direct_wants_bypass;
    st.matcher.set_external_bypass(bypass);

    let sands_enabled = if ime_open {
        st.app_config.enable_sands_ime_on
    } else {
        st.app_config.enable_sands_ime_off
    };
    st.matcher.set_sands_enabled(sands_enabled);
}

/// Lock the hook state, recovering from a poisoned mutex instead of giving up.
/// A poisoned lock (a thread panicked while holding it) must never make the
/// hook fall through to `CallNextHookEx` — that would leak the raw key to the
/// OS (the "occasional stray 'a'" symptom). The protected state is plain data,
/// safe to keep using after a panic.
fn lock_state() -> std::sync::MutexGuard<'static, HookState> {
    let m = HOOK_STATE.get().expect("hook state set before use");
    m.lock().unwrap_or_else(|p| p.into_inner())
}

struct HookState {
    matcher: InputMatcher,
    /// Layout currently in effect (mirrors either `normal_layout` or
    /// `ime_off_layout`, whichever IME state we're in).
    layout: std::sync::Arc<Layout>,
    /// Layout resolved for the current foreground app + profile, used while
    /// the IME is ON (or always, if `ime_off_layout` is unset).
    normal_layout: std::sync::Arc<Layout>,
    /// Optional alternate layout used while the IME is OFF
    /// (`AppConfig::ime_off_layout`), loaded once at install/reload.
    ime_off_layout: Option<std::sync::Arc<Layout>>,
    /// Whether `layout` currently points at `ime_off_layout` (vs `normal_layout`).
    using_ime_off_layout: bool,
    /// Last-known IME open state, as reported by the poll thread. Only
    /// meaningful while `IME_GATING` is set; otherwise treated as `true`.
    ime_open: bool,
    /// Configured "direct input" key(s) (`AppConfig::direct_input_key`):
    /// while any of these is held and `direct_input_mode != Off`, prefer
    /// `ime_off_layout` (or bypass, if unset / mode is `Raw`) even while the
    /// IME is ON.
    direct_input_keys: HashSet<KeyCode>,
    /// Whether a configured direct-input key is currently held.
    direct_input_active: bool,
    /// SandS direct-input mode (`AppConfig::direct_input_mode`).
    direct_input_mode: DirectInputMode,
    app_config: AppConfig,
    current_app: String,
    /// JIS vs US/ANSI, taken from the loaded layout's filename suffix
    /// (`.jp.txt` = JIS, `.en.txt` = US; see `Layout::keyboard`).
    keyboard: KeyboardLayout,
}

/// Load an alternate layout file from a config-supplied path (FR: IME-off
/// layout). Empty path or load failure -> `None` (NFR-4: never panic, just
/// keep the old behaviour for that path).
fn load_optional_layout(path: &str) -> Option<std::sync::Arc<Layout>> {
    let path = path.trim();
    if path.is_empty() {
        return None;
    }
    let resolved = crate::config::resolve_layout_path(path);
    std::fs::read(&resolved)
        .ok()
        .and_then(|bytes| loader::default_loader().load(&bytes, &resolved).ok())
        .map(std::sync::Arc::new)
}

pub fn install_and_run_windows_hook() -> JoinHandle<()> {
    // Load config (or fallback), resolve initial app + its layout.
    // Supports FR-3 per-app profile (app_map) from day one of prototype.
    let app_config =
        AppConfig::load(Path::new("data/config.json")).unwrap_or_else(|_| AppConfig::fallback());
    IME_GATING.store(app_config.activate_only_when_ime_on, Ordering::Relaxed);
    DISPATCH_RATE_MS.store(app_config.dispatch_rate_ms.max(1), Ordering::Relaxed);
    CTRL_SPACE_IME_TOGGLE.store(app_config.enable_ctrl_space_ime_toggle, Ordering::Relaxed);
    let initial_app = get_foreground_app_id();
    let normal_layout = std::sync::Arc::new(load_layout_for_app(&initial_app, &app_config));
    let ime_off_layout = load_optional_layout(&app_config.ime_off_layout);

    let mut matcher = InputMatcher::default();
    // FR-6: apply configured disable keys (held -> full passthrough).
    matcher.set_disable_keys(app_config.disable_keycodes());
    if app_config.combo_window_ms > 0 {
        matcher.set_combo_window_ms(app_config.combo_window_ms);
    }
    if app_config.prefix_window_ms > 0 {
        matcher.set_prefix_window_ms(app_config.prefix_window_ms);
    }
    matcher.set_hold_mode(app_config.hold_mode);
    let direct_input_keys: HashSet<KeyCode> =
        crate::config::keycodes_from_config_name(&app_config.direct_input_key)
            .into_iter()
            .collect();
    let direct_input_mode = DirectInputMode::from_config_str(&app_config.direct_input_mode);

    let layout = normal_layout.clone();
    let keyboard = layout.keyboard;
    let state = HookState {
        matcher,
        layout,
        normal_layout,
        ime_off_layout,
        using_ime_off_layout: false,
        ime_open: true,
        direct_input_keys,
        direct_input_active: false,
        direct_input_mode,
        app_config,
        current_app: initial_app,
        keyboard,
    };
    HOOK_STATE.set(Mutex::new(state)).ok();
    recompute_layout_locked(&mut lock_state());

    // 同時打鍵 flush timer + IME gate poller. The flush part runs every
    // `DISPATCH_RATE_MS` (default 5ms, user-tunable) so a pending solo key is
    // emitted promptly once its combo window elapses. The IME gate is checked
    // on a fixed ~60ms wall-clock cadence (independent of the dispatch rate)
    // because `SendMessageW` is comparatively expensive. Both extract what they
    // need under the lock and release it before any blocking call (inject / IME
    // query).
    thread::spawn(|| {
        const IME_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(60);
        let mut last_ime_poll = std::time::Instant::now();
        loop {
            thread::sleep(std::time::Duration::from_millis(
                DISPATCH_RATE_MS.load(Ordering::Relaxed).max(1),
            ));

            // Flush a due chord: take the output under the lock, inject after.
            let flush: Option<(OutputSeq, KeyboardLayout)> = {
                let mut st = lock_state();
                if st.matcher.has_pending() {
                    let layout = std::sync::Arc::clone(&st.layout);
                    let keyboard = st.keyboard;
                    st.matcher.flush_due(&layout).map(|seq| (seq, keyboard))
                } else {
                    None
                }
            };
            if let Some((seq, keyboard)) = flush {
                inject_output_seq(&seq, keyboard);
            }

            // IME gate (~60ms): if the layout is configured to be active only
            // while the IME is ON, bypass remapping whenever the IME is OFF.
            if last_ime_poll.elapsed() >= IME_POLL_INTERVAL {
                last_ime_poll = std::time::Instant::now();
                if IME_GATING.load(Ordering::Relaxed) {
                    match ime_open_status() {
                        Some(open) => {
                            // Log on transitions so the user can verify gating.
                            let cur = open as i8;
                            if LAST_IME_LOG.swap(cur, Ordering::Relaxed) != cur {
                                crate::log::log(if open {
                                    "IME on -> remap active"
                                } else {
                                    "IME off -> remap bypassed (passthrough)"
                                });
                            }
                            // active only while IME ON -> swap layout (or bypass
                            // if no IME-off layout is configured) while IME OFF.
                            let mut st = lock_state();
                            st.ime_open = open;
                            recompute_layout_locked(&mut st);
                        }
                        None => {
                            // Can't read IME state: log once, leave bypass as-is.
                            if LAST_IME_LOG.swap(-2, Ordering::Relaxed) != -2 {
                                crate::log::log("IME state unknown (no IME window / query failed)");
                            }
                        }
                    }
                } else {
                    // Gating disabled: never bypass for IME reasons, always the
                    // normal (per-app) layout.
                    LAST_IME_LOG.store(-1, Ordering::Relaxed);
                    let mut st = lock_state();
                    st.ime_open = true;
                    recompute_layout_locked(&mut st);
                }
            }
        }
    });

    thread::spawn(|| unsafe {
        // Use the guarded version for NFR-4 (catch_unwind around the real proc so a panic in user layout logic cannot crash the hook thread / system).
        let hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(guarded_low_level_proc), None, 0);
        if hook.is_err() {
            eprintln!("Failed to install WH_KEYBOARD_LL hook. Run as admin or grant perms?");
            return;
        }
        let hhook = hook.unwrap();

        // Message loop to keep the hook thread alive and deliver LL events
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            // No Translate/Dispatch needed for pure LL hook, but loop keeps the thread alive and pumping messages.
        }

        let _ = UnhookWindowsHookEx(hhook);
    })
}

unsafe extern "system" fn low_level_proc(n_code: i32, w_param: WPARAM, l_param: LPARAM) -> LRESULT {
    if n_code < 0 {
        return CallNextHookEx(None, n_code, w_param, l_param);
    }

    let kbd = &*(l_param.0 as *const KBDLLHOOKSTRUCT);
    let vk = kbd.vkCode;
    let flags = kbd.flags;

    // Pass through our own injected events (prevents re-remapping our output)
    if (flags & LLKHF_INJECTED).0 != 0 {
        return CallNextHookEx(None, n_code, w_param, l_param);
    }

    let is_down = matches!(w_param.0 as u32, WM_KEYDOWN | WM_SYSKEYDOWN);
    let is_up = matches!(w_param.0 as u32, WM_KEYUP | WM_SYSKEYUP);

    if !is_down && !is_up {
        return CallNextHookEx(None, n_code, w_param, l_param);
    }

    // Ctrl+Space IME toggle (AppConfig::enable_ctrl_space_ime_toggle): tracked
    // ahead of the matcher so the chord is recognised even though the matcher
    // has no notion of "OS modifier + key". Ctrl itself always passes through
    // unchanged; Space is only eaten while Ctrl is down and the feature is on.
    const VK_LCONTROL_U32: u32 = 0xA2;
    const VK_RCONTROL_U32: u32 = 0xA3;
    const VK_CONTROL_U32: u32 = 0x11;
    if matches!(vk, VK_LCONTROL_U32 | VK_RCONTROL_U32 | VK_CONTROL_U32) {
        CTRL_HELD.store(is_down, Ordering::Relaxed);
    } else if vk == VK_SPACE.0 as u32 {
        if is_down
            && CTRL_SPACE_IME_TOGGLE.load(Ordering::Relaxed)
            && CTRL_HELD.load(Ordering::Relaxed)
        {
            if !CTRL_SPACE_DOWN.swap(true, Ordering::Relaxed) {
                toggle_ime_open_status();
            }
            return LRESULT(1);
        } else if is_up {
            CTRL_SPACE_DOWN.store(false, Ordering::Relaxed);
        }
    }

    let mods = current_modifiers_from_vk(vk, is_down); // best effort; real state tracked in matcher too

    // Decide under the lock, but release it *before* injecting: SendInput and
    // CallNextHookEx must not run while the global mutex is held, or the flush /
    // IME-poll threads block the hot path and the callback can exceed
    // LowLevelHooksTimeout — at which point Windows drops the hook and the raw
    // key leaks to the OS (the "occasional stray 'a'"). We compute the action,
    // drop the guard, then act.
    let (action, keyboard) = {
        let mut st = lock_state();

        // FR-3: per-app profile switch on key event (cheap check + re-query only on change).
        let fg = get_foreground_app_id();
        if fg != st.current_app {
            st.current_app = fg.clone();
            let new_layout = std::sync::Arc::new(load_layout_for_app(&fg, &st.app_config));
            st.matcher.clear(); // safe boundary per NFR-4 (no keys held across switch in practice)
            st.normal_layout = new_layout;
            if !st.using_ime_off_layout {
                st.layout = st.normal_layout.clone();
                st.keyboard = st.layout.keyboard;
            }
        }

        let code = vk_to_keycode(vk, (flags.0 & 0x01) != 0 /* extended */, st.keyboard);
        let held = st.matcher.was_already_pressed(&code);

        // Direct-input key (AppConfig::direct_input_key): while held, prefer
        // `ime_off_layout` (or bypass, if unset) even though the IME is ON.
        // Active whenever the key is configured (non-empty), regardless of
        // `direct_input_mode` — even "off" gives raw bypass so that e.g.
        // Shift+letter always produces the physical key.
        // Toggle only on the real down/up edge (ignore OS auto-repeat).
        if !st.direct_input_keys.is_empty() && st.direct_input_keys.contains(&code) {
            if is_down && !held {
                st.direct_input_active = true;
                recompute_layout_locked(&mut st);
            } else if is_up {
                st.direct_input_active = false;
                recompute_layout_locked(&mut st);
            }
        }

        let ev = Event {
            kind: if is_down {
                EventKind::KeyDown
            } else {
                EventKind::KeyUp
            },
            code,
            modifiers: mods,
            timestamp: kbd.time as u64,
            held: false, // will be set by matcher based on its pressed
        };
        let ev = if held { ev.with_held(true) } else { ev };

        let layout = std::sync::Arc::clone(&st.layout);
        let keyboard = st.keyboard;
        let action = st.matcher.process(&ev, &layout);
        (action, keyboard)
        // guard dropped here
    };

    match action {
        MatchAction::Emit(seq) => {
            // Remap: synthesize and eat the original.
            inject_output_seq(&seq, keyboard);
            LRESULT(1)
        }
        MatchAction::EmitThenPass(seq) => {
            // Flush a pending chord, then let the original key through unchanged.
            inject_output_seq(&seq, keyboard);
            CallNextHookEx(None, n_code, w_param, l_param)
        }
        MatchAction::Block => LRESULT(1),
        MatchAction::PassThrough => CallNextHookEx(None, n_code, w_param, l_param),
    }
}

// NFR-4 last-resort guard: never panic the hook thread / system hook.
unsafe extern "system" fn guarded_low_level_proc(
    n_code: i32,
    w_param: WPARAM,
    l_param: LPARAM,
) -> LRESULT {
    let res = std::panic::catch_unwind(|| unsafe { low_level_proc(n_code, w_param, l_param) });
    res.unwrap_or_else(|_| {
        // In production log the panic; for prototype just pass the original event through.
        CallNextHookEx(None, n_code, w_param, l_param)
    })
}

fn current_modifiers_from_vk(_vk: u32, _is_down: bool) -> Modifiers {
    // For prototype, we let the layout/matcher handle layers; here we just report active OS mods if needed.
    // Real: we can track from events, but matcher already has pressed for layers.
    // For output tokens that carry explicit SHIFT etc, we synthesize in inject.
    Modifiers::empty()
}

fn vk_to_keycode(vk: u32, _extended: bool, keyboard: KeyboardLayout) -> KeyCode {
    // JIS-only physical keys: the driver reports these OEM VKs differently
    // than on a US/ANSI keyboard (@, ^, ¥ and the extra \(ろ) key).
    if keyboard == KeyboardLayout::Jis {
        match vk as i32 {
            0xC0 => return KeyCode::AtSign,    // JIS "@" key (US: Grave)
            0xDE => return KeyCode::Colon,     // JIS ":" key (US: Quote)
            0xDC => return KeyCode::Yen,       // JIS "¥" key (US: Backslash)
            0xBB => return KeyCode::Caret,     // JIS "^" key (US: Equal)
            0xE2 => return KeyCode::Backslash, // JIS "\ろ" key (no US equivalent)
            _ => {}
        }
    }
    match vk as i32 {
        0x41 => KeyCode::A,
        0x42 => KeyCode::B,
        0x43 => KeyCode::C,
        0x44 => KeyCode::D,
        0x45 => KeyCode::E,
        0x46 => KeyCode::F,
        0x47 => KeyCode::G,
        0x48 => KeyCode::H,
        0x49 => KeyCode::I,
        0x4A => KeyCode::J,
        0x4B => KeyCode::K,
        0x4C => KeyCode::L,
        0x4D => KeyCode::M,
        0x4E => KeyCode::N,
        0x4F => KeyCode::O,
        0x50 => KeyCode::P,
        0x51 => KeyCode::Q,
        0x52 => KeyCode::R,
        0x53 => KeyCode::S,
        0x54 => KeyCode::T,
        0x55 => KeyCode::U,
        0x56 => KeyCode::V,
        0x57 => KeyCode::W,
        0x58 => KeyCode::X,
        0x59 => KeyCode::Y,
        0x5A => KeyCode::Z,
        0x30 => KeyCode::Num0,
        0x31 => KeyCode::Num1,
        0x32 => KeyCode::Num2,
        0x33 => KeyCode::Num3,
        0x34 => KeyCode::Num4,
        0x35 => KeyCode::Num5,
        0x36 => KeyCode::Num6,
        0x37 => KeyCode::Num7,
        0x38 => KeyCode::Num8,
        0x39 => KeyCode::Num9,
        0xBD => KeyCode::Minus,
        0xBB => KeyCode::Equal,
        0xDB => KeyCode::LBracket,
        0xDD => KeyCode::RBracket,
        0xDC => KeyCode::Backslash,
        0xBA => KeyCode::Semicolon,
        0xDE => KeyCode::Quote,
        0xBC => KeyCode::Comma,
        0xBE => KeyCode::Dot,
        0xBF => KeyCode::Slash,
        0xC0 => KeyCode::Grave,
        0x20 => KeyCode::Space,
        0x0D => KeyCode::Enter,
        0x09 => KeyCode::Tab,
        0x08 => KeyCode::Backspace,
        0x1B => KeyCode::Escape,
        0x14 => KeyCode::CapsLock,
        0x25 => KeyCode::Left,
        0x27 => KeyCode::Right,
        0x26 => KeyCode::Up,
        0x28 => KeyCode::Down,
        0x10 => KeyCode::ShiftL,
        0xA0 => KeyCode::ShiftL,
        0xA1 => KeyCode::ShiftR,
        0x11 => KeyCode::CtrlL,
        0xA2 => KeyCode::CtrlL,
        0xA3 => KeyCode::CtrlR,
        0x12 => KeyCode::AltL,
        0xA4 => KeyCode::AltL,
        0xA5 => KeyCode::AltR,
        0x5B => KeyCode::MetaL,
        0x5C => KeyCode::MetaR,
        // Japan
        0x1C => KeyCode::Henkan,
        0x1D => KeyCode::Muhenkan,
        0x15 => KeyCode::KanaKatakana,
        0x19 => KeyCode::HankakuZenkaku,
        0x7D => KeyCode::Yen,
        _ => KeyCode::Unknown(vk),
    }
}

fn inject_output_seq(seq: &OutputSeq, keyboard: KeyboardLayout) {
    if seq.is_empty() {
        return;
    }

    let mut inputs: Vec<INPUT> = Vec::with_capacity(seq.len() * 4);

    for token in seq {
        match token {
            OutputToken::Key { code, mods } => {
                // Press required modifiers first (for this token)
                let mut pressed_mods: Vec<VIRTUAL_KEY> = vec![];
                if mods.contains(Modifiers::SHIFT) || mods.contains(Modifiers::SHIFT_L) {
                    pressed_mods.push(VK_LSHIFT);
                }
                if mods.contains(Modifiers::SHIFT_R) {
                    pressed_mods.push(VK_RSHIFT);
                }
                if mods.contains(Modifiers::CTRL) || mods.contains(Modifiers::CTRL_L) {
                    pressed_mods.push(VK_LCONTROL);
                }
                if mods.contains(Modifiers::CTRL_R) {
                    pressed_mods.push(VK_RCONTROL);
                }
                if mods.contains(Modifiers::ALT) || mods.contains(Modifiers::ALT_L) {
                    pressed_mods.push(VK_LMENU);
                }
                if mods.contains(Modifiers::ALT_R) {
                    pressed_mods.push(VK_RMENU);
                }
                if mods.contains(Modifiers::META) || mods.contains(Modifiers::META_L) {
                    pressed_mods.push(VK_LWIN);
                }
                if mods.contains(Modifiers::META_R) {
                    pressed_mods.push(VK_RWIN);
                }

                for &mvk in &pressed_mods {
                    inputs.push(make_key_input(mvk, false));
                }

                let main_vk = keycode_to_vk(*code, keyboard);
                if main_vk.0 != 0 {
                    inputs.push(make_key_input(main_vk, false));
                    inputs.push(make_key_input(main_vk, true));
                } else {
                    // fallback unicode if we have a printable
                    if let Some(ch) = keycode_to_char_fallback(*code) {
                        inputs.push(make_unicode_input(ch, false));
                        inputs.push(make_unicode_input(ch, true));
                    }
                }

                // Release mods in reverse
                for &mvk in pressed_mods.iter().rev() {
                    inputs.push(make_key_input(mvk, true));
                }
            }
            OutputToken::Text(s) => {
                for ch in s.chars() {
                    inputs.push(make_unicode_input(ch, false));
                    inputs.push(make_unicode_input(ch, true));
                }
            }
            OutputToken::Named(sk) => {
                let vk = match sk {
                    SpecialKey::Backspace => VK_BACK,
                    SpecialKey::Enter => VK_RETURN,
                    SpecialKey::Tab => VK_TAB,
                    SpecialKey::Escape => VK_ESCAPE,
                    SpecialKey::Left => VK_LEFT,
                    SpecialKey::Right => VK_RIGHT,
                    SpecialKey::Up => VK_UP,
                    SpecialKey::Down => VK_DOWN,
                };
                inputs.push(make_key_input(vk, false));
                inputs.push(make_key_input(vk, true));
            }
            OutputToken::ModDown(code) => {
                let vk = keycode_to_vk(*code, keyboard);
                if vk.0 != 0 {
                    inputs.push(make_key_input(vk, false));
                }
            }
            OutputToken::ModUp(code) => {
                let vk = keycode_to_vk(*code, keyboard);
                if vk.0 != 0 {
                    inputs.push(make_key_input(vk, true));
                }
            }
        }
    }

    if !inputs.is_empty() {
        unsafe {
            let _ = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
        }
    }
}

fn make_key_input(vk: VIRTUAL_KEY, up: bool) -> INPUT {
    let ki = KEYBDINPUT {
        wVk: vk,
        dwFlags: if up {
            KEYBD_EVENT_FLAGS(KEYEVENTF_KEYUP.0)
        } else {
            KEYBD_EVENT_FLAGS(0)
        },
        ..Default::default()
    };
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 { ki },
    }
}

fn make_unicode_input(ch: char, up: bool) -> INPUT {
    let ki = KEYBDINPUT {
        wScan: ch as u16,
        dwFlags: KEYBD_EVENT_FLAGS(KEYEVENTF_UNICODE.0 | if up { KEYEVENTF_KEYUP.0 } else { 0 }),
        ..Default::default()
    };
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 { ki },
    }
}

fn keycode_to_vk(k: KeyCode, keyboard: KeyboardLayout) -> VIRTUAL_KEY {
    // JIS-only symbol keys: only meaningful when a JIS layout is active
    // (these KeyCodes won't be produced by US-physical-row grids).
    if keyboard == KeyboardLayout::Jis {
        match k {
            KeyCode::AtSign => return VIRTUAL_KEY(0xC0),
            KeyCode::Colon => return VIRTUAL_KEY(0xDE),
            KeyCode::Yen => return VIRTUAL_KEY(0xDC),
            KeyCode::Caret => return VIRTUAL_KEY(0xBB),
            _ => {}
        }
    }
    match k {
        KeyCode::A => VIRTUAL_KEY(0x41),
        KeyCode::B => VIRTUAL_KEY(0x42),
        KeyCode::C => VIRTUAL_KEY(0x43),
        KeyCode::D => VIRTUAL_KEY(0x44),
        KeyCode::E => VIRTUAL_KEY(0x45),
        KeyCode::F => VIRTUAL_KEY(0x46),
        KeyCode::G => VIRTUAL_KEY(0x47),
        KeyCode::H => VIRTUAL_KEY(0x48),
        KeyCode::I => VIRTUAL_KEY(0x49),
        KeyCode::J => VIRTUAL_KEY(0x4A),
        KeyCode::K => VIRTUAL_KEY(0x4B),
        KeyCode::L => VIRTUAL_KEY(0x4C),
        KeyCode::M => VIRTUAL_KEY(0x4D),
        KeyCode::N => VIRTUAL_KEY(0x4E),
        KeyCode::O => VIRTUAL_KEY(0x4F),
        KeyCode::P => VIRTUAL_KEY(0x50),
        KeyCode::Q => VIRTUAL_KEY(0x51),
        KeyCode::R => VIRTUAL_KEY(0x52),
        KeyCode::S => VIRTUAL_KEY(0x53),
        KeyCode::T => VIRTUAL_KEY(0x54),
        KeyCode::U => VIRTUAL_KEY(0x55),
        KeyCode::V => VIRTUAL_KEY(0x56),
        KeyCode::W => VIRTUAL_KEY(0x57),
        KeyCode::X => VIRTUAL_KEY(0x58),
        KeyCode::Y => VIRTUAL_KEY(0x59),
        KeyCode::Z => VIRTUAL_KEY(0x5A),
        KeyCode::Num0 => VIRTUAL_KEY(0x30),
        KeyCode::Num1 => VIRTUAL_KEY(0x31),
        KeyCode::Num2 => VIRTUAL_KEY(0x32),
        KeyCode::Num3 => VIRTUAL_KEY(0x33),
        KeyCode::Num4 => VIRTUAL_KEY(0x34),
        KeyCode::Num5 => VIRTUAL_KEY(0x35),
        KeyCode::Num6 => VIRTUAL_KEY(0x36),
        KeyCode::Num7 => VIRTUAL_KEY(0x37),
        KeyCode::Num8 => VIRTUAL_KEY(0x38),
        KeyCode::Num9 => VIRTUAL_KEY(0x39),
        KeyCode::Space => VK_SPACE,
        KeyCode::Enter => VK_RETURN,
        KeyCode::Tab => VK_TAB,
        KeyCode::Backspace => VK_BACK,
        KeyCode::Escape => VK_ESCAPE,
        KeyCode::Left => VK_LEFT,
        KeyCode::Right => VK_RIGHT,
        KeyCode::Up => VK_UP,
        KeyCode::Down => VK_DOWN,
        KeyCode::ShiftL => VK_LSHIFT,
        KeyCode::ShiftR => VK_RSHIFT,
        KeyCode::CtrlL => VK_LCONTROL,
        KeyCode::CtrlR => VK_RCONTROL,
        KeyCode::AltL => VK_LMENU,
        KeyCode::AltR => VK_RMENU,
        KeyCode::MetaL => VK_LWIN,
        KeyCode::MetaR => VK_RWIN,
        KeyCode::CapsLock => VK_CAPITAL,
        KeyCode::Henkan => VK_CONVERT,
        KeyCode::Muhenkan => VK_NONCONVERT,
        KeyCode::KanaKatakana => VK_KANA,
        KeyCode::HankakuZenkaku => VK_KANJI,
        KeyCode::Minus => VIRTUAL_KEY(0xBD),
        KeyCode::Equal => VIRTUAL_KEY(0xBB),
        KeyCode::LBracket => VIRTUAL_KEY(0xDB),
        KeyCode::RBracket => VIRTUAL_KEY(0xDD),
        KeyCode::Backslash => VIRTUAL_KEY(0xDC),
        KeyCode::Semicolon => VIRTUAL_KEY(0xBA),
        KeyCode::Quote => VIRTUAL_KEY(0xDE),
        KeyCode::Comma => VIRTUAL_KEY(0xBC),
        KeyCode::Dot => VIRTUAL_KEY(0xBE),
        KeyCode::Slash => VIRTUAL_KEY(0xBF),
        KeyCode::Grave => VIRTUAL_KEY(0xC0),
        _ => VIRTUAL_KEY(0),
    }
}

#[allow(dead_code)]
fn keycode_to_char_fallback(k: KeyCode) -> Option<char> {
    match k {
        KeyCode::A => Some('a'),
        KeyCode::B => Some('b'),
        KeyCode::Q => Some('q'),
        KeyCode::W => Some('w'),
        _ => None,
    }
}

/// Resolve layout bytes for a concrete app_id using the provided AppConfig (per-app or default).
/// Falls back to sample then embedded (NFR-4: never panic, always produce a usable Layout).
/// The physical keyboard layout (JIS vs US) is determined entirely by the
/// loaded file's name suffix (`.jp.txt` / `.en.txt`), not the OS locale.
fn load_layout_for_app(app_id: &str, cfg: &AppConfig) -> Layout {
    let ldr = loader::default_loader();
    let layout_path = cfg.layout_path_for_app(app_id);

    if let Ok(bytes) = std::fs::read(&layout_path) {
        if let Ok(l) = ldr.load(&bytes, &layout_path) {
            return l;
        }
    }

    // Fallback to bundled sample (toy_simul supports SandS + tap for live verification)
    if let Ok(bytes) = std::fs::read("data/layouts/samples/toy_simul.txt") {
        if let Ok(l) = ldr.load(&bytes, "toy-sands") {
            return l;
        }
    }

    // Last resort: tiny embedded SandS toy (guarantees runnable prototype even with no data/ dir)
    ldr.load(
        b"simultaneous-press layout\n-option-input[ space | -space ]\n[ q | w | e ]\n{space}[ Q | W | E ]\n{space}\n",
        "embedded",
    ).unwrap_or_else(|_| Layout::default())
}

/// Reload the layout from config (or fallback). Called from tray menu or IPC.
/// Re-loads AppConfig too (supports live edit of per-app map), re-resolves for current fg app,
/// swaps Arc<Layout>, clears matcher (safe boundary: no in-flight combo across reload).
pub fn reload_layout() {
    let new_cfg =
        AppConfig::load(Path::new("data/config.json")).unwrap_or_else(|_| AppConfig::fallback());
    IME_GATING.store(new_cfg.activate_only_when_ime_on, Ordering::Relaxed);
    DISPATCH_RATE_MS.store(new_cfg.dispatch_rate_ms.max(1), Ordering::Relaxed);
    CTRL_SPACE_IME_TOGGLE.store(new_cfg.enable_ctrl_space_ime_toggle, Ordering::Relaxed);
    let app = get_foreground_app_id();
    let new_layout = std::sync::Arc::new(load_layout_for_app(&app, &new_cfg));
    let new_ime_off_layout = load_optional_layout(&new_cfg.ime_off_layout);
    if let Some(state) = HOOK_STATE.get() {
        if let Ok(mut st) = state.lock() {
            st.matcher.clear();
            // FR-6: re-apply disable keys from the freshly loaded config.
            st.matcher.set_disable_keys(new_cfg.disable_keycodes());
            st.direct_input_keys =
                crate::config::keycodes_from_config_name(&new_cfg.direct_input_key)
                    .into_iter()
                    .collect();
            st.direct_input_mode = DirectInputMode::from_config_str(&new_cfg.direct_input_mode);
            if new_cfg.combo_window_ms > 0 {
                st.matcher.set_combo_window_ms(new_cfg.combo_window_ms);
            }
            if new_cfg.prefix_window_ms > 0 {
                st.matcher.set_prefix_window_ms(new_cfg.prefix_window_ms);
            }
            st.matcher.set_hold_mode(new_cfg.hold_mode);
            st.app_config = new_cfg;
            st.current_app = app;
            st.normal_layout = new_layout;
            st.ime_off_layout = new_ime_off_layout;
            st.using_ime_off_layout = false;
            st.direct_input_active = false;
            st.layout = st.normal_layout.clone();
            st.keyboard = st.layout.keyboard;
            recompute_layout_locked(&mut st);
            // In real app we would log "layout reloaded"
        }
    }
}

/// FR-8: persistent stop/resume control for the live matcher. Driven by tray,
/// IPC, or (later) a global hotkey. `set_suspend(true)` = 停止, `false` = 再開.
pub fn set_suspend(suspended: bool) {
    if let Some(state) = HOOK_STATE.get() {
        if let Ok(mut st) = state.lock() {
            st.matcher.set_suspended(suspended);
        }
    }
}

/// FR-8: flip stop/resume; returns the new suspended state (true = stopped).
pub fn toggle_suspend() -> bool {
    if let Some(state) = HOOK_STATE.get() {
        if let Ok(mut st) = state.lock() {
            return st.matcher.toggle_suspended();
        }
    }
    false
}

/// FR-8: query current suspended state (for IPC Status).
pub fn is_suspended() -> bool {
    if let Some(state) = HOOK_STATE.get() {
        if let Ok(st) = state.lock() {
            return st.matcher.is_suspended();
        }
    }
    false
}

/// Get a stable app identifier from the current foreground window (exe basename, lower, no .exe).
/// Returns empty string on failure -> caller uses default_profile.
/// Used for FR-3 per-app profile switching. Called only on key events (and reload); cheap in practice.
fn get_foreground_app_id() -> String {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0 == 0 {
            return String::new();
        }
        let mut pid: u32 = 0;
        let _ = GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 {
            return String::new();
        }
        let handle: HANDLE = match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
            Ok(h) => h,
            Err(_) => return String::new(),
        };
        let mut buf: [u16; 260] = [0; 260];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR(buf.as_mut_ptr()),
            &mut len,
        )
        .is_ok();
        let _ = CloseHandle(handle);
        if !ok || len == 0 {
            return String::new();
        }
        let path = String::from_utf16_lossy(&buf[..len as usize]);
        let name = path.rsplit(['\\', '/']).next().unwrap_or(&path);
        let name = name.to_ascii_lowercase();
        let name = name.strip_suffix(".exe").unwrap_or(&name).to_string();
        if name.is_empty() {
            String::new()
        } else {
            name
        }
    }
}
