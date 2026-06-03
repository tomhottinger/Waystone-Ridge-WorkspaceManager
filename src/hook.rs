//! Low-Level-Keyboard-Hook (`WH_KEYBOARD_LL`) als Fallback für Hotkeys, die
//! `RegisterHotKey` ablehnt, weil die Windows-Shell sie reserviert (typisch:
//! `Win+1..0`, `Win+Shift+N`).
//!
//! Der Hook fängt die Tastenkombination ab, *bevor* die Shell sie sieht,
//! unterdrückt deren Standardverhalten (Rückgabe `1`) und meldet die Aktion per
//! `PostMessageW` an das Nachrichtenfenster der App.
//!
//! Das Bit-Schema der Modifier entspricht den `MOD_*`-Werten von `RegisterHotKey`:
//! `Alt=1, Ctrl=2, Shift=4, Win=8`.

use std::cell::RefCell;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS,
    KEYEVENTF_KEYUP, VIRTUAL_KEY, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, PostMessageW, SetWindowsHookExW, UnhookWindowsHookEx, HC_ACTION, HHOOK,
    KBDLLHOOKSTRUCT, LLKHF_INJECTED, WH_KEYBOARD_LL, WM_KEYDOWN, WM_SYSKEYDOWN,
};

/// Modifier-Bits (identisch zu den `MOD_*`-Werten von RegisterHotKey).
pub const M_ALT: u32 = 0x1;
pub const M_CTRL: u32 = 0x2;
pub const M_SHIFT: u32 = 0x4;
pub const M_WIN: u32 = 0x8;

/// Eigene Fensternachricht, mit der der Hook eine ausgelöste Aktion meldet.
/// `WPARAM` enthält die Hotkey-ID. (`WM_APP` = 0x8000.)
pub const WM_APP_HOOK_HOTKEY: u32 = 0x8000 + 1;

/// Eine vom Hook überwachte Tastenkombination.
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
}

/// Hinterlegt das Nachrichtenfenster, an das ausgelöste Hotkeys gemeldet werden.
pub fn set_hwnd(hwnd: HWND) {
    STATE.with(|s| s.borrow_mut().hwnd = hwnd.0 as isize);
}

/// Fügt eine zu überwachende Kombination hinzu.
pub fn add_entry(mods: u32, vk: u32, id: i32) {
    STATE.with(|s| s.borrow_mut().entries.push(HookEntry { mods, vk, id }));
}

/// Gibt an, ob überhaupt Kombinationen über den Hook laufen müssen.
pub fn is_empty() -> bool {
    STATE.with(|s| s.borrow().entries.is_empty())
}

/// Installiert den globalen Low-Level-Keyboard-Hook.
pub fn install() -> windows::core::Result<HHOOK> {
    unsafe {
        let hmod = GetModuleHandleW(None)?;
        SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), hmod, 0)
    }
}

/// Entfernt den Hook wieder.
pub fn uninstall(hook: HHOOK) {
    unsafe {
        let _ = UnhookWindowsHookEx(hook);
    }
}

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

/// Unterdrückt das Öffnen des Startmenüs, wenn eine Win-Kombination abgefangen
/// wurde: Ohne diesen Trick würde Windows beim Loslassen der Win-Taste das
/// Startmenü öffnen, weil es die abgefangene Taste nie gesehen hat. Ein
/// injizierter, nicht belegter Tastencode "entwertet" die Win-Sequenz.
fn suppress_start_menu() {
    let vk = VIRTUAL_KEY(0xE8); // nicht zugewiesener virtueller Tastencode
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
                dwFlags: if up {
                    KEYEVENTF_KEYUP
                } else {
                    KEYBD_EVENT_FLAGS(0)
                },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let kb = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        let injected = (kb.flags.0 & LLKHF_INJECTED.0) != 0;
        if !injected {
            let event = wparam.0 as u32;
            if event == WM_KEYDOWN || event == WM_SYSKEYDOWN {
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
                    return LRESULT(1); // Taste schlucken
                }
            }
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}
