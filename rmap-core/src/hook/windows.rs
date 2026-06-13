//! Windows WH_KEYBOARD_LL implementation + SendInput injection.
//! Follows plan: dedicated thread for hook + message loop, bidirectional KeyCode map,
//! eat original on remap, pass injected events, low latency decision inside callback.

use crate::{Event, EventKind, KeyCode, Modifiers, OutputSeq, OutputToken, SpecialKey, InputMatcher, MatchAction, layout::Layout, DvorakJLayoutLoader, config::AppConfig, loader::LayoutLoader};
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::thread::{self, JoinHandle};
use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM, HANDLE, CloseHandle};
use windows::core::PWSTR;
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetMessageW, SetWindowsHookExW, UnhookWindowsHookEx, KBDLLHOOKSTRUCT,
    MSG, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP, LLKHF_INJECTED,
    GetForegroundWindow, GetWindowThreadProcessId,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS,
    KEYEVENTF_KEYUP, KEYEVENTF_UNICODE,
    VIRTUAL_KEY,
    VK_LSHIFT, VK_RSHIFT, VK_LCONTROL, VK_RCONTROL, VK_LMENU, VK_RMENU,
    VK_LWIN, VK_RWIN,
    VK_SPACE, VK_RETURN, VK_TAB, VK_BACK, VK_ESCAPE, VK_LEFT, VK_RIGHT, VK_UP, VK_DOWN,
    VK_CAPITAL, VK_CONVERT, VK_NONCONVERT, VK_KANA, VK_KANJI,
};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};

static HOOK_STATE: OnceLock<Mutex<HookState>> = OnceLock::new();

struct HookState {
    matcher: InputMatcher,
    layout: std::sync::Arc<Layout>,
    app_config: AppConfig,
    current_app: String,
}

pub fn install_and_run_windows_hook() -> JoinHandle<()> {
    // Load config (or fallback), resolve initial app + its layout.
    // Supports FR-3 per-app profile (app_map) from day one of prototype.
    let app_config = AppConfig::load(Path::new("data/config.json"))
        .unwrap_or_else(|_| AppConfig::fallback());
    let initial_app = get_foreground_app_id();
    let layout = load_layout_for_app(&initial_app, &app_config);

    let mut matcher = InputMatcher::default();
    // FR-6: apply configured disable keys (held -> full passthrough).
    matcher.set_disable_keys(app_config.disable_keycodes());

    let state = HookState {
        matcher,
        layout: std::sync::Arc::new(layout),
        app_config,
        current_app: initial_app,
    };
    HOOK_STATE.set(Mutex::new(state)).ok();

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

    let code = vk_to_keycode(vk, (flags.0 & 0x01) != 0 /* extended */);
    let mods = current_modifiers_from_vk(vk, is_down); // best effort; real state tracked in matcher too

    let ev = Event {
        kind: if is_down { EventKind::KeyDown } else { EventKind::KeyUp },
        code,
        modifiers: mods,
        timestamp: kbd.time as u64,
        held: false, // will be set by matcher based on its pressed
    };

    // Lock state, let matcher decide (fast path inside hook callback for NFR-1)
    if let Some(state) = HOOK_STATE.get() {
        if let Ok(mut st) = state.lock() {
            // FR-3: per-app profile switch on key event (cheap check + re-query only on change).
            // Avoids heavy work in hot path; swap only when fg app actually changes.
            let fg = get_foreground_app_id();
            if fg != st.current_app {
                st.current_app = fg.clone();
                let new_layout = load_layout_for_app(&fg, &st.app_config);
                st.matcher.clear(); // safe boundary per NFR-4 (no keys held across switch in practice)
                st.layout = std::sync::Arc::new(new_layout);
            }

            // Update held flag using our pressed set before processing
            let held = st.matcher.was_already_pressed(&code);
            let ev = if held { ev.with_held(true) } else { ev };

            // Clone Arc for immutable layout while holding mutable borrow on matcher only
            let layout = std::sync::Arc::clone(&st.layout);
            match st.matcher.process(&ev, &layout) {
                MatchAction::Emit(seq) => {
                    // Remap: synthesize and eat the original.
                    inject_output_seq(&seq);
                    return LRESULT(1);
                }
                MatchAction::Block => {
                    // Consume the original (e.g. a held layer key) with no output.
                    return LRESULT(1);
                }
                MatchAction::PassThrough => {
                    return CallNextHookEx(None, n_code, w_param, l_param);
                }
            }
        }
    }

    CallNextHookEx(None, n_code, w_param, l_param)
}

// NFR-4 last-resort guard: never panic the hook thread / system hook.
unsafe extern "system" fn guarded_low_level_proc(n_code: i32, w_param: WPARAM, l_param: LPARAM) -> LRESULT {
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

fn vk_to_keycode(vk: u32, _extended: bool) -> KeyCode {
    match vk as i32 {
        0x41 => KeyCode::A, 0x42 => KeyCode::B, 0x43 => KeyCode::C, 0x44 => KeyCode::D,
        0x45 => KeyCode::E, 0x46 => KeyCode::F, 0x47 => KeyCode::G, 0x48 => KeyCode::H,
        0x49 => KeyCode::I, 0x4A => KeyCode::J, 0x4B => KeyCode::K, 0x4C => KeyCode::L,
        0x4D => KeyCode::M, 0x4E => KeyCode::N, 0x4F => KeyCode::O, 0x50 => KeyCode::P,
        0x51 => KeyCode::Q, 0x52 => KeyCode::R, 0x53 => KeyCode::S, 0x54 => KeyCode::T,
        0x55 => KeyCode::U, 0x56 => KeyCode::V, 0x57 => KeyCode::W, 0x58 => KeyCode::X,
        0x59 => KeyCode::Y, 0x5A => KeyCode::Z,
        0x30 => KeyCode::Num0, 0x31 => KeyCode::Num1, 0x32 => KeyCode::Num2, 0x33 => KeyCode::Num3,
        0x34 => KeyCode::Num4, 0x35 => KeyCode::Num5, 0x36 => KeyCode::Num6, 0x37 => KeyCode::Num7,
        0x38 => KeyCode::Num8, 0x39 => KeyCode::Num9,
        0xBD => KeyCode::Minus, 0xBB => KeyCode::Equal,
        0xDB => KeyCode::LBracket, 0xDD => KeyCode::RBracket, 0xDC => KeyCode::Backslash,
        0xBA => KeyCode::Semicolon, 0xDE => KeyCode::Quote,
        0xBC => KeyCode::Comma, 0xBE => KeyCode::Dot, 0xBF => KeyCode::Slash,
        0xC0 => KeyCode::Grave,
        0x20 => KeyCode::Space,
        0x0D => KeyCode::Enter,
        0x09 => KeyCode::Tab,
        0x08 => KeyCode::Backspace,
        0x1B => KeyCode::Escape,
        0x14 => KeyCode::CapsLock,
        0x25 => KeyCode::Left, 0x27 => KeyCode::Right, 0x26 => KeyCode::Up, 0x28 => KeyCode::Down,
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

fn inject_output_seq(seq: &OutputSeq) {
    if seq.is_empty() { return; }

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

                let main_vk = keycode_to_vk(*code);
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
        }
    }

    if !inputs.is_empty() {
        unsafe {
            let _ = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
        }
    }
}

fn make_key_input(vk: VIRTUAL_KEY, up: bool) -> INPUT {
    let mut ki = KEYBDINPUT::default();
    ki.wVk = vk;
    if up {
        ki.dwFlags = KEYBD_EVENT_FLAGS(KEYEVENTF_KEYUP.0);
    }
    let mut i = INPUT::default();
    i.r#type = INPUT_KEYBOARD;
    i.Anonymous = INPUT_0 { ki };
    i
}

fn make_unicode_input(ch: char, up: bool) -> INPUT {
    let mut ki = KEYBDINPUT::default();
    ki.wScan = ch as u16;
    ki.dwFlags = KEYBD_EVENT_FLAGS(KEYEVENTF_UNICODE.0 | if up { KEYEVENTF_KEYUP.0 } else { 0 });
    let mut i = INPUT::default();
    i.r#type = INPUT_KEYBOARD;
    i.Anonymous = INPUT_0 { ki };
    i
}

fn keycode_to_vk(k: KeyCode) -> VIRTUAL_KEY {
    match k {
        KeyCode::A => VIRTUAL_KEY(0x41), KeyCode::B => VIRTUAL_KEY(0x42), KeyCode::C => VIRTUAL_KEY(0x43),
        KeyCode::D => VIRTUAL_KEY(0x44), KeyCode::E => VIRTUAL_KEY(0x45), KeyCode::F => VIRTUAL_KEY(0x46),
        KeyCode::G => VIRTUAL_KEY(0x47), KeyCode::H => VIRTUAL_KEY(0x48), KeyCode::I => VIRTUAL_KEY(0x49),
        KeyCode::J => VIRTUAL_KEY(0x4A), KeyCode::K => VIRTUAL_KEY(0x4B), KeyCode::L => VIRTUAL_KEY(0x4C),
        KeyCode::M => VIRTUAL_KEY(0x4D), KeyCode::N => VIRTUAL_KEY(0x4E), KeyCode::O => VIRTUAL_KEY(0x4F),
        KeyCode::P => VIRTUAL_KEY(0x50), KeyCode::Q => VIRTUAL_KEY(0x51), KeyCode::R => VIRTUAL_KEY(0x52),
        KeyCode::S => VIRTUAL_KEY(0x53), KeyCode::T => VIRTUAL_KEY(0x54), KeyCode::U => VIRTUAL_KEY(0x55),
        KeyCode::V => VIRTUAL_KEY(0x56), KeyCode::W => VIRTUAL_KEY(0x57), KeyCode::X => VIRTUAL_KEY(0x58),
        KeyCode::Y => VIRTUAL_KEY(0x59), KeyCode::Z => VIRTUAL_KEY(0x5A),
        KeyCode::Num0 => VIRTUAL_KEY(0x30), KeyCode::Num1 => VIRTUAL_KEY(0x31), KeyCode::Num2 => VIRTUAL_KEY(0x32),
        KeyCode::Num3 => VIRTUAL_KEY(0x33), KeyCode::Num4 => VIRTUAL_KEY(0x34), KeyCode::Num5 => VIRTUAL_KEY(0x35),
        KeyCode::Num6 => VIRTUAL_KEY(0x36), KeyCode::Num7 => VIRTUAL_KEY(0x37), KeyCode::Num8 => VIRTUAL_KEY(0x38),
        KeyCode::Num9 => VIRTUAL_KEY(0x39),
        KeyCode::Space => VK_SPACE,
        KeyCode::Enter => VK_RETURN,
        KeyCode::Tab => VK_TAB,
        KeyCode::Backspace => VK_BACK,
        KeyCode::Escape => VK_ESCAPE,
        KeyCode::Left => VK_LEFT, KeyCode::Right => VK_RIGHT, KeyCode::Up => VK_UP, KeyCode::Down => VK_DOWN,
        KeyCode::ShiftL => VK_LSHIFT, KeyCode::ShiftR => VK_RSHIFT,
        KeyCode::CtrlL => VK_LCONTROL, KeyCode::CtrlR => VK_RCONTROL,
        KeyCode::AltL => VK_LMENU, KeyCode::AltR => VK_RMENU,
        KeyCode::MetaL => VK_LWIN, KeyCode::MetaR => VK_RWIN,
        KeyCode::CapsLock => VK_CAPITAL,
        KeyCode::Henkan => VK_CONVERT,
        KeyCode::Muhenkan => VK_NONCONVERT,
        KeyCode::KanaKatakana => VK_KANA,
        KeyCode::HankakuZenkaku => VK_KANJI,
        KeyCode::Minus => VIRTUAL_KEY(0xBD), KeyCode::Equal => VIRTUAL_KEY(0xBB),
        KeyCode::LBracket => VIRTUAL_KEY(0xDB), KeyCode::RBracket => VIRTUAL_KEY(0xDD),
        KeyCode::Backslash => VIRTUAL_KEY(0xDC), KeyCode::Semicolon => VIRTUAL_KEY(0xBA),
        KeyCode::Quote => VIRTUAL_KEY(0xDE), KeyCode::Comma => VIRTUAL_KEY(0xBC),
        KeyCode::Dot => VIRTUAL_KEY(0xBE), KeyCode::Slash => VIRTUAL_KEY(0xBF),
        KeyCode::Grave => VIRTUAL_KEY(0xC0),
        _ => VIRTUAL_KEY(0),
    }
}

#[allow(dead_code)]
fn keycode_to_char_fallback(k: KeyCode) -> Option<char> {
    match k {
        KeyCode::A => Some('a'), KeyCode::B => Some('b'),
        KeyCode::Q => Some('q'), KeyCode::W => Some('w'),
        _ => None,
    }
}

/// Resolve layout bytes for a concrete app_id using the provided AppConfig (per-app or default).
/// Falls back to sample then embedded (NFR-4: never panic, always produce a usable Layout).
fn load_layout_for_app(app_id: &str, cfg: &AppConfig) -> Layout {
    let loader = DvorakJLayoutLoader::new();
    let layout_path = cfg.layout_path_for_app(app_id);

    if let Ok(bytes) = std::fs::read(&layout_path) {
        if let Ok(l) = loader.load(&bytes, &layout_path) {
            return l;
        }
    }

    // Fallback to bundled sample (toy_simul supports SandS + tap for live verification)
    if let Ok(bytes) = std::fs::read("data/layouts/samples/toy_simul.txt") {
        if let Ok(l) = loader.load(&bytes, "toy-sands") {
            return l;
        }
    }

    // Last resort: tiny embedded SandS toy (guarantees runnable prototype even with no data/ dir)
    loader.load(
        b"simultaneous-press layout\n-option-input[ space | -space ]\n[ q | w | e ]\n{space}[ Q | W | E ]\n{space}\n",
        "embedded",
    ).unwrap_or_else(|_| Layout::default())
}

/// Reload the layout from config (or fallback). Called from tray menu or IPC.
/// Re-loads AppConfig too (supports live edit of per-app map), re-resolves for current fg app,
/// swaps Arc<Layout>, clears matcher (safe boundary: no in-flight combo across reload).
pub fn reload_layout() {
    let new_cfg = AppConfig::load(Path::new("data/config.json"))
        .unwrap_or_else(|_| AppConfig::fallback());
    let app = get_foreground_app_id();
    let new_layout = load_layout_for_app(&app, &new_cfg);
    if let Some(state) = HOOK_STATE.get() {
        if let Ok(mut st) = state.lock() {
            st.matcher.clear();
            // FR-6: re-apply disable keys from the freshly loaded config.
            st.matcher.set_disable_keys(new_cfg.disable_keycodes());
            st.app_config = new_cfg;
            st.current_app = app;
            st.layout = std::sync::Arc::new(new_layout);
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
        let ok = QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len).is_ok();
        let _ = CloseHandle(handle);
        if !ok || len == 0 {
            return String::new();
        }
        let path = String::from_utf16_lossy(&buf[..len as usize]);
        let name = path.rsplit(|c| c == '\\' || c == '/').next().unwrap_or(&path);
        let name = name.to_ascii_lowercase();
        let name = name.strip_suffix(".exe").unwrap_or(&name).to_string();
        if name.is_empty() { String::new() } else { name }
    }
}
