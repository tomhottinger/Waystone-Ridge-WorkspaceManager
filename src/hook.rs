//! Low-Level-Hooks (`WH_KEYBOARD_LL`, `WH_MOUSE_LL`) für zwei Zwecke:
//!
//! 1. **Hotkey-Fallback**: Tastenkombinationen, die `RegisterHotKey` ablehnt
//!    (z. B. `Win+1..0`, `Win+Shift+N`), werden abgefangen und als
//!    `WM_APP_HOOK_HOTKEY` an das Nachrichtenfenster gemeldet.
//!
//! 2. **Respite-Blockierung**: Während ein Respite-Zeitfenster aktiv ist, werden
//!    alle nicht-injizierten Tastatur- und Mauseingaben konsumiert. Als Notausgang
//!    dient `Ctrl+Alt+Shift+Delete` (meldet `WM_APP_RESPITE_ESCAPE`).
//!
//! Modifier-Bits identisch zu den `MOD_*`-Werten von `RegisterHotKey`:
//! `Alt=1, Ctrl=2, Shift=4, Win=8`.

use std::cell::{Cell, RefCell};

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS,
    KEYEVENTF_KEYUP, VIRTUAL_KEY, VK_CONTROL, VK_LCONTROL, VK_LMENU, VK_LSHIFT,
    VK_LWIN, VK_MENU, VK_RCONTROL, VK_RMENU, VK_RSHIFT, VK_RWIN, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, PostMessageW, SetWindowsHookExW, UnhookWindowsHookEx, HC_ACTION, HHOOK,
    KBDLLHOOKSTRUCT, LLKHF_INJECTED, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_KEYDOWN, WM_SYSKEYDOWN,
};

pub const M_ALT: u32 = 0x1;
pub const M_CTRL: u32 = 0x2;
pub const M_SHIFT: u32 = 0x4;
pub const M_WIN: u32 = 0x8;

/// Eigene Fensternachricht: Hotkey über den Low-Level-Hook ausgelöst.
pub const WM_APP_HOOK_HOTKEY: u32 = 0x8000 + 1;

/// Eigene Fensternachricht: Respite-Notausgang betätigt (Sequenz vollständig eingegeben).
pub const WM_APP_RESPITE_ESCAPE: u32 = 0x8000 + 2;

/// Eigene Fensternachricht: Buchstabe/Ziffer während Respite eingegeben (WPARAM = char als u32).
pub const WM_APP_RESPITE_KEYCHAR: u32 = 0x8000 + 3;

struct HookEntry {
    mods: u32,
    vk: u32,
    id: i32,
}

#[derive(Default)]
struct HookData {
    hwnd: isize,
    entries: Vec<HookEntry>,
    /// Manuell verfolgter Modifier-Status – zuverlässiger als `GetAsyncKeyState`
    /// während der Respite-Blockierung, weil Modifier-Keydowns dann konsumiert werden.
    mods: u32,
}

thread_local! {
    static STATE: RefCell<HookData> = RefCell::new(HookData::default());
    static RESPITE_ACTIVE: Cell<bool> = Cell::new(false);
    /// Wird gesetzt sobald die Mindestwartezeit abgelaufen ist — erst dann werden
    /// Buchstaben/Ziffern an main.rs weitergeleitet.
    static RESPITE_ESCAPE_UNLOCKED: Cell<bool> = Cell::new(false);
}

// ── Öffentliche Steuerfunktionen ──────────────────────────────────────────────

pub fn set_hwnd(hwnd: HWND) {
    STATE.with(|s| s.borrow_mut().hwnd = hwnd.0 as isize);
}

pub fn add_entry(mods: u32, vk: u32, id: i32) {
    STATE.with(|s| s.borrow_mut().entries.push(HookEntry { mods, vk, id }));
}

pub fn is_empty() -> bool {
    STATE.with(|s| s.borrow().entries.is_empty())
}

pub fn clear_entries() {
    STATE.with(|s| s.borrow_mut().entries.clear());
}

pub fn set_respite_active(active: bool) {
    RESPITE_ACTIVE.with(|r| r.set(active));
}

pub fn set_respite_escape_unlocked(v: bool) {
    RESPITE_ESCAPE_UNLOCKED.with(|r| r.set(v));
}

// ── Hook-Installation ─────────────────────────────────────────────────────────

pub fn install() -> windows::core::Result<HHOOK> {
    unsafe {
        let hmod = GetModuleHandleW(None)?;
        SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook_proc), hmod, 0)
    }
}

pub fn uninstall(hook: HHOOK) {
    unsafe {
        let _ = UnhookWindowsHookEx(hook);
    }
}

pub fn install_mouse() -> windows::core::Result<HHOOK> {
    unsafe {
        let hmod = GetModuleHandleW(None)?;
        SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook_proc), hmod, 0)
    }
}

pub fn uninstall_mouse(hook: HHOOK) {
    unsafe {
        let _ = UnhookWindowsHookEx(hook);
    }
}

// ── Hilfsfunktionen ───────────────────────────────────────────────────────────

/// Liest Modifier-Status via `GetAsyncKeyState` (für den Normalmodus).
fn current_mods() -> u32 {
    let mut m = 0;
    if is_down(VK_LWIN) || is_down(VK_RWIN) { m |= M_WIN; }
    if is_down(VK_CONTROL) { m |= M_CTRL; }
    if is_down(VK_MENU) { m |= M_ALT; }
    if is_down(VK_SHIFT) { m |= M_SHIFT; }
    m
}

fn is_down(vk: VIRTUAL_KEY) -> bool {
    (unsafe { GetAsyncKeyState(vk.0 as i32) } as u16 & 0x8000) != 0
}

/// Gibt das Modifier-Bit eines VK-Codes zurück (0 wenn kein Modifier).
fn vk_to_mod(vk: u32) -> u32 {
    match vk {
        v if v == VK_LWIN.0 as u32 || v == VK_RWIN.0 as u32 => M_WIN,
        v if v == VK_CONTROL.0 as u32
            || v == VK_LCONTROL.0 as u32
            || v == VK_RCONTROL.0 as u32 => M_CTRL,
        v if v == VK_MENU.0 as u32
            || v == VK_LMENU.0 as u32
            || v == VK_RMENU.0 as u32 => M_ALT,
        v if v == VK_SHIFT.0 as u32
            || v == VK_LSHIFT.0 as u32
            || v == VK_RSHIFT.0 as u32 => M_SHIFT,
        _ => 0,
    }
}

fn suppress_start_menu() {
    let vk = VIRTUAL_KEY(0xE8);
    let inputs = [make_key(vk, false), make_key(vk, true)];
    unsafe {
        SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }
}

fn make_key(vk: VIRTUAL_KEY, up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

// ── Hook-Callbacks ────────────────────────────────────────────────────────────

unsafe extern "system" fn keyboard_hook_proc(
    code: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if code == HC_ACTION as i32 {
        let kb = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        let injected = (kb.flags.0 & LLKHF_INJECTED.0) != 0;

        if !injected {
            let event = wparam.0 as u32;
            let is_keydown = event == WM_KEYDOWN || event == WM_SYSKEYDOWN;
            let vk = kb.vkCode;

            // Modifier-Status manuell verfolgen (läuft auch bei Respite-Blockierung,
            // damit der Notausgang zuverlässig erkannt wird).
            let mod_bit = vk_to_mod(vk);
            if mod_bit != 0 {
                STATE.with(|s| {
                    let mut d = s.borrow_mut();
                    if is_keydown { d.mods |= mod_bit; } else { d.mods &= !mod_bit; }
                });
            }

            // ── Respite-Modus: alle Eingaben blockieren ────────────────────────
            if RESPITE_ACTIVE.with(|r| r.get()) {
                if is_keydown && RESPITE_ESCAPE_UNLOCKED.with(|r| r.get()) {
                    // Nach Ablauf der Mindestwartezeit: Buchstaben (a–z) und Ziffern (0–9)
                    // für die Escape-Sequenz an main.rs weiterleiten.
                    let ch: Option<char> = match vk {
                        v if (0x41..=0x5A).contains(&v) => {
                            Some((b'a' + (v as u8 - 0x41)) as char)
                        }
                        v if (0x30..=0x39).contains(&v) => {
                            Some((b'0' + (v as u8 - 0x30)) as char)
                        }
                        _ => None,
                    };
                    if let Some(ch) = ch {
                        let hwnd_val = STATE.with(|s| s.borrow().hwnd);
                        if hwnd_val != 0 {
                            let _ = PostMessageW(
                                HWND(hwnd_val as _),
                                WM_APP_RESPITE_KEYCHAR,
                                WPARAM(ch as usize),
                                LPARAM(0),
                            );
                        }
                    }
                }
                return LRESULT(1);
            }

            // ── Normalmodus: Hotkey-Matching ───────────────────────────────────
            if is_keydown {
                let mods = current_mods();
                let mut hit: Option<i32> = None;
                let mut hwnd_val: isize = 0;
                STATE.with(|s| {
                    let data = s.borrow();
                    hwnd_val = data.hwnd;
                    for e in &data.entries {
                        if e.vk == vk && e.mods == mods {
                            hit = Some(e.id);
                            break;
                        }
                    }
                });

                if let Some(id) = hit {
                    if hwnd_val != 0 {
                        let hwnd = HWND(hwnd_val as _);
                        let _ = PostMessageW(
                            hwnd,
                            WM_APP_HOOK_HOTKEY,
                            WPARAM(id as usize),
                            LPARAM(0),
                        );
                    }
                    if (mods & M_WIN) != 0 {
                        suppress_start_menu();
                    }
                    return LRESULT(1);
                }
            }
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}

unsafe extern "system" fn mouse_hook_proc(
    code: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if code == HC_ACTION as i32 {
        if RESPITE_ACTIVE.with(|r| r.get()) {
            return LRESULT(1);
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}
