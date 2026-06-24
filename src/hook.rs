//! Low-Level-Hooks (`WH_KEYBOARD_LL`, `WH_MOUSE_LL`) für zwei Zwecke:
//!
//! 1. **Hotkey-Fallback**: Tastenkombinationen, die `RegisterHotKey` ablehnt
//!    (z. B. `Win+1..0`), werden abgefangen und als `WM_APP_HOOK_HOTKEY` an das
//!    Nachrichtenfenster gemeldet.
//!
//! 2. **Respite-Blockierung**: Während ein Respite-Zeitfenster aktiv ist, werden
//!    alle nicht-injizierten Tastatur- und Mauseingaben konsumiert. Als Notausgang
//!    dient `Ctrl+Alt+Shift+Escape` (meldet `WM_APP_RESPITE_ESCAPE`).
//!
//! Das Bit-Schema der Modifier entspricht den `MOD_*`-Werten von `RegisterHotKey`:
//! `Alt=1, Ctrl=2, Shift=4, Win=8`.

use std::cell::{Cell, RefCell};

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS,
    KEYEVENTF_KEYUP, VIRTUAL_KEY, VK_CONTROL, VK_ESCAPE, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, PostMessageW, SetWindowsHookExW, UnhookWindowsHookEx, HC_ACTION, HHOOK,
    KBDLLHOOKSTRUCT, LLKHF_INJECTED, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_KEYDOWN, WM_SYSKEYDOWN,
};

/// Modifier-Bits (identisch zu den `MOD_*`-Werten von RegisterHotKey).
pub const M_ALT: u32 = 0x1;
pub const M_CTRL: u32 = 0x2;
pub const M_SHIFT: u32 = 0x4;
pub const M_WIN: u32 = 0x8;

/// Eigene Fensternachricht: Hotkey über den Low-Level-Hook ausgelöst.
/// `WPARAM` enthält die Hotkey-ID.
pub const WM_APP_HOOK_HOTKEY: u32 = 0x8000 + 1;

/// Eigene Fensternachricht: Respite-Notausgang (`Ctrl+Alt+Shift+Escape`) betätigt.
pub const WM_APP_RESPITE_ESCAPE: u32 = 0x8000 + 2;

/// Eine vom Hook überwachte Tastenkombination (für den Hotkey-Fallback).
struct HookEntry {
    mods: u32,
    vk: u32,
    id: i32,
}

#[derive(Default)]
struct HookData {
    /// Ziel-Fenster (als roher Zahlenwert, da `HWND` nicht `Send`/`Sync` ist).
    hwnd: isize,
    entries: Vec<HookEntry>,
}

thread_local! {
    static STATE: RefCell<HookData> = RefCell::new(HookData::default());
    /// Respite aktiv: alle nicht-injizierten Eingaben werden blockiert.
    static RESPITE_ACTIVE: Cell<bool> = Cell::new(false);
}

// ── Öffentliche Steuerfunktionen ──────────────────────────────────────────────

/// Hinterlegt das Nachrichtenfenster, an das ausgelöste Hotkeys gemeldet werden.
pub fn set_hwnd(hwnd: HWND) {
    STATE.with(|s| s.borrow_mut().hwnd = hwnd.0 as isize);
}

/// Fügt eine zu überwachende Kombination hinzu.
pub fn add_entry(mods: u32, vk: u32, id: i32) {
    STATE.with(|s| s.borrow_mut().entries.push(HookEntry { mods, vk, id }));
}

/// Gibt an, ob überhaupt Kombinationen über den Keyboard-Hook laufen.
pub fn is_empty() -> bool {
    STATE.with(|s| s.borrow().entries.is_empty())
}

/// Entfernt alle registrierten Hook-Einträge (für Config-Reload).
pub fn clear_entries() {
    STATE.with(|s| s.borrow_mut().entries.clear());
}

/// Schaltet die Respite-Blockierung ein oder aus.
pub fn set_respite_active(active: bool) {
    RESPITE_ACTIVE.with(|r| r.set(active));
}

// ── Hook-Installation ─────────────────────────────────────────────────────────

/// Installiert den globalen Low-Level-Keyboard-Hook.
pub fn install() -> windows::core::Result<HHOOK> {
    unsafe {
        let hmod = GetModuleHandleW(None)?;
        SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook_proc), hmod, 0)
    }
}

/// Entfernt den Keyboard-Hook.
pub fn uninstall(hook: HHOOK) {
    unsafe {
        let _ = UnhookWindowsHookEx(hook);
    }
}

/// Installiert den globalen Low-Level-Maus-Hook (nur für Respite-Blockierung).
pub fn install_mouse() -> windows::core::Result<HHOOK> {
    unsafe {
        let hmod = GetModuleHandleW(None)?;
        SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook_proc), hmod, 0)
    }
}

/// Entfernt den Maus-Hook.
pub fn uninstall_mouse(hook: HHOOK) {
    unsafe {
        let _ = UnhookWindowsHookEx(hook);
    }
}

// ── Hilfsfunktionen ───────────────────────────────────────────────────────────

/// Liest den aktuellen Modifier-Zustand über `GetAsyncKeyState`.
fn current_mods() -> u32 {
    let mut m = 0;
    if is_down(VK_LWIN) || is_down(VK_RWIN) {
        m |= M_WIN;
    }
    if is_down(VK_CONTROL) {
        m |= M_CTRL;
    }
    if is_down(VK_MENU) {
        m |= M_ALT;
    }
    if is_down(VK_SHIFT) {
        m |= M_SHIFT;
    }
    m
}

fn is_down(vk: VIRTUAL_KEY) -> bool {
    (unsafe { GetAsyncKeyState(vk.0 as i32) } as u16 & 0x8000) != 0
}

/// Verhindert das Öffnen des Startmenüs nach einer abgefangenen Win-Kombination.
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

            // ── Respite-Modus: alle Eingaben blockieren ────────────────────────
            if RESPITE_ACTIVE.with(|r| r.get()) {
                // Notausgang: Ctrl+Alt+Shift+Escape bricht die Sperre ab.
                if is_keydown {
                    let mods = current_mods();
                    let vk = kb.vkCode;
                    if mods == (M_CTRL | M_ALT | M_SHIFT) && vk == VK_ESCAPE.0 as u32 {
                        let hwnd_val = STATE.with(|s| s.borrow().hwnd);
                        if hwnd_val != 0 {
                            let _ = PostMessageW(
                                HWND(hwnd_val as _),
                                WM_APP_RESPITE_ESCAPE,
                                WPARAM(0),
                                LPARAM(0),
                            );
                        }
                    }
                }
                return LRESULT(1); // Taste schlucken
            }

            // ── Normalmodus: Hotkey-Matching ───────────────────────────────────
            if is_keydown {
                let mods = current_mods();
                let vk = kb.vkCode;

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
            return LRESULT(1); // Mauseingabe schlucken
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}
