//! Fensterverwaltung: Enumeration, Vordergrundfenster, Sichtbarkeit, Filter.

use core::ffi::c_void;
use windows::Win32::Foundation::{BOOL, HWND, LPARAM};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetAncestor, GetClassNameW, GetForegroundWindow, GetWindowLongW,
    GetWindowTextLengthW, IsWindow, IsWindowVisible, ShowWindow, GA_ROOTOWNER, GWL_EXSTYLE,
    GWL_STYLE, SW_HIDE, SW_SHOW, WS_CHILD, WS_EX_APPWINDOW, WS_EX_TOOLWINDOW,
};

/// Stabiler, hashbarer Schlüssel für ein Fensterhandle.
///
/// `HWND` ist je nach `windows`-Version pointer- oder integerbasiert; wir
/// arbeiten intern mit dem rohen Zahlenwert, damit der Typ als HashMap-Key dient.
pub fn hwnd_key(hwnd: HWND) -> isize {
    hwnd.0 as isize
}

/// Erzeugt aus einem Schlüssel wieder ein `HWND`.
pub fn hwnd_from_key(key: isize) -> HWND {
    HWND(key as _)
}

/// Liefert das aktuelle Vordergrundfenster.
pub fn foreground_window() -> HWND {
    unsafe { GetForegroundWindow() }
}

/// Zeigt ein Fenster an.
pub fn show(hwnd: HWND) {
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOW);
    }
}

/// Versteckt ein Fenster.
pub fn hide(hwnd: HWND) {
    unsafe {
        let _ = ShowWindow(hwnd, SW_HIDE);
    }
}

/// Prüft, ob das Handle noch ein gültiges Fenster bezeichnet.
pub fn is_window(hwnd: HWND) -> bool {
    unsafe { IsWindow(hwnd).as_bool() }
}

/// Enumeriert alle aktuell sichtbaren, verwaltbaren Top-Level-Fenster.
pub fn enumerate_manageable() -> Vec<HWND> {
    let mut windows: Vec<HWND> = Vec::new();
    unsafe {
        let _ = EnumWindows(
            Some(enum_proc),
            LPARAM(&mut windows as *mut Vec<HWND> as isize),
        );
    }
    windows
}

unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let windows = &mut *(lparam.0 as *mut Vec<HWND>);
    if is_manageable(hwnd) {
        windows.push(hwnd);
    }
    BOOL(1) // weiter enumerieren
}

/// Entscheidet, ob ein Fenster vom Workspace-Manager verwaltet werden soll.
///
/// Heuristik in Anlehnung an die "Alt-Tab"-Fensterauswahl von Windows: sichtbar,
/// Top-Level (eigener Root-Owner), mit Titel, kein reines Tool-Fenster, nicht
/// per DWM "cloaked" (filtert UWP-Hintergrundfenster) und keine Shell-Klasse.
pub fn is_manageable(hwnd: HWND) -> bool {
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() {
            return false;
        }
        if GetAncestor(hwnd, GA_ROOTOWNER) != hwnd {
            return false;
        }
        if GetWindowTextLengthW(hwnd) == 0 {
            return false;
        }

        let style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
        if (style & WS_CHILD.0) != 0 {
            return false;
        }

        let exstyle = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
        let is_tool = (exstyle & WS_EX_TOOLWINDOW.0) != 0;
        let is_app = (exstyle & WS_EX_APPWINDOW.0) != 0;
        if is_tool && !is_app {
            return false;
        }

        if is_cloaked(hwnd) {
            return false;
        }

        const BLOCKED: &[&str] = &[
            "Progman",
            "WorkerW",
            "Shell_TrayWnd",
            "Shell_SecondaryTrayWnd",
            "Windows.UI.Core.CoreWindow",
            "DV2ControlHost",
            "Button",
        ];
        if BLOCKED.contains(&class_name(hwnd).as_str()) {
            return false;
        }

        true
    }
}

/// Liefert den Klassennamen eines Fensters.
fn class_name(hwnd: HWND) -> String {
    let mut buf = [0u16; 256];
    let len = unsafe { GetClassNameW(hwnd, &mut buf) };
    if len <= 0 {
        return String::new();
    }
    String::from_utf16_lossy(&buf[..len as usize])
}

/// Prüft via DWM, ob ein Fenster "cloaked" (unsichtbar gemacht) ist.
fn is_cloaked(hwnd: HWND) -> bool {
    let mut cloaked: u32 = 0;
    let result = unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            &mut cloaked as *mut u32 as *mut c_void,
            std::mem::size_of::<u32>() as u32,
        )
    };
    result.is_ok() && cloaked != 0
}
