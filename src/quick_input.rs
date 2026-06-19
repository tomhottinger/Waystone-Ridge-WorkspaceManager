//! Randloses, immer-oben Textfeld (Memo), das per Hotkey ein-/ausgeblendet wird.
//!
//! Mehrere Zeilen möglich. Der Text bleibt zwischen den Sitzungen erhalten.
//! ESC oder Fokusverlust blendet das Feld aus und gibt den Fokus zurück.

use std::sync::atomic::{AtomicIsize, Ordering};

use anyhow::Result;
use windows::core::w;
use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateFontW, CreateSolidBrush, DeleteObject, GetStockObject, HDC, HGDIOBJ, DEFAULT_GUI_FONT,
    SetBkColor, SetTextColor,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{SetFocus, VK_ESCAPE};
use windows::Win32::UI::WindowsAndMessaging::{
    CallWindowProcW, CreateWindowExW, DefWindowProcW, DestroyWindow, GetForegroundWindow,
    GetSystemMetrics, IsWindowVisible, RegisterClassW, SendMessageW, SetForegroundWindow,
    SetWindowLongPtrW, ShowWindow, GWLP_WNDPROC, HMENU, SM_CXSCREEN, SM_CYSCREEN, SW_HIDE,
    SW_SHOW, WINDOW_EX_STYLE, WINDOW_STYLE, WNDCLASSW, WM_ACTIVATE, WM_KEYDOWN, WM_SETFONT,
    WS_CHILD, WS_EX_TOPMOST, WS_POPUP, WS_VISIBLE,
};

const PAD_H: i32 = 10;
const PAD_V: i32 = 8;

/// BGR-Farben (Win32-Konvention): weißer Hintergrund, schwarzer Text.
const BG_COLOR: u32 = 0x00_FF_FF_FF;
const FG_COLOR: u32 = 0x00_00_00_00;
const WM_CTLCOLOREDIT: u32 = 0x0133;

static PARENT_HWND_VAL: AtomicIsize = AtomicIsize::new(0);
static EDIT_HWND_VAL: AtomicIsize = AtomicIsize::new(0);
static PREV_FOREGROUND_VAL: AtomicIsize = AtomicIsize::new(0);
static BG_BRUSH_VAL: AtomicIsize = AtomicIsize::new(0);
static FONT_VAL: AtomicIsize = AtomicIsize::new(0);
static ORIG_EDIT_PROC_VAL: AtomicIsize = AtomicIsize::new(0);

/// Handle auf das Quick-Input-Fenster.
pub struct QuickInput {
    hwnd: HWND,
}

impl QuickInput {
    /// Erstellt das Fenster (anfangs unsichtbar).
    ///
    /// - `width_pct` / `height_pct`: Fenstergröße in Prozent der Bildschirmmaße (5–95).
    /// - `font_size`: Schriftgröße in Punkt; 0 = Windows-Standardschrift.
    pub fn create(width_pct: u32, height_pct: u32, font_size: u32) -> Result<Self> {
        let hwnd = unsafe { create_window(width_pct, height_pct, font_size)? };
        Ok(Self { hwnd })
    }

    /// Ein- oder Ausblenden je nach aktuellem Zustand.
    pub fn toggle(&self) {
        unsafe {
            if IsWindowVisible(self.hwnd).as_bool() {
                do_hide(self.hwnd);
            } else {
                do_show(self.hwnd);
            }
        }
    }
}

impl Drop for QuickInput {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.hwnd);
            let font_raw = FONT_VAL.swap(0, Ordering::Relaxed);
            if font_raw != 0 {
                let _ = DeleteObject(HGDIOBJ(font_raw as *mut _));
            }
            let brush_raw = BG_BRUSH_VAL.swap(0, Ordering::Relaxed);
            if brush_raw != 0 {
                let _ = DeleteObject(HGDIOBJ(brush_raw as *mut _));
            }
        }
    }
}

unsafe fn do_show(hwnd: HWND) {
    let prev = GetForegroundWindow();
    if prev != hwnd {
        PREV_FOREGROUND_VAL.store(prev.0 as isize, Ordering::Relaxed);
    }
    // Text absichtlich NICHT leeren — Inhalt bleibt erhalten.
    let _ = ShowWindow(hwnd, SW_SHOW);
    let _ = SetForegroundWindow(hwnd);
    let _ = SetFocus(HWND(EDIT_HWND_VAL.load(Ordering::Relaxed) as *mut _));
}

unsafe fn do_hide(hwnd: HWND) {
    let _ = ShowWindow(hwnd, SW_HIDE);
    let raw = PREV_FOREGROUND_VAL.load(Ordering::Relaxed);
    if raw != 0 {
        let _ = SetForegroundWindow(HWND(raw as *mut _));
    }
}

unsafe fn create_window(width_pct: u32, height_pct: u32, font_size: u32) -> Result<HWND> {
    let hinstance = HINSTANCE(GetModuleHandleW(None)?.0);
    let class_name = w!("WaystoneQuickInput");

    let brush = CreateSolidBrush(COLORREF(BG_COLOR));
    BG_BRUSH_VAL.store(brush.0 as isize, Ordering::Relaxed);

    let wc = WNDCLASSW {
        lpfnWndProc: Some(parent_wndproc),
        hInstance: hinstance,
        lpszClassName: class_name,
        hbrBackground: brush,
        ..Default::default()
    };
    let _ = RegisterClassW(&wc);

    let sw = GetSystemMetrics(SM_CXSCREEN);
    let sh = GetSystemMetrics(SM_CYSCREEN);
    let win_w = (sw as u32 * width_pct.clamp(5, 95) / 100) as i32;
    let win_h = (sh as u32 * height_pct.clamp(5, 95) / 100) as i32;
    // Bildschirmmitte.
    let x = (sw - win_w) / 2;
    let y = (sh - win_h) / 2;

    let hwnd = CreateWindowExW(
        WS_EX_TOPMOST,
        class_name,
        w!(""),
        WS_POPUP,
        x,
        y,
        win_w,
        win_h,
        HWND::default(),
        HMENU::default(),
        hinstance,
        None,
    )?;
    PARENT_HWND_VAL.store(hwnd.0 as isize, Ordering::Relaxed);

    // Mehrzeiliges Edit-Steuerelement:
    //   ES_MULTILINE   0x0004  mehrere Zeilen, Zeilenumbruch mit Enter
    //   ES_AUTOVSCROLL 0x0040  automatisch vertikal scrollen
    //   ES_WANTRETURN  0x1000  Enter = Zeilenumbruch (kein Dialog-Abschluss)
    //   WS_VSCROLL     0x00200000  vertikale Scrollleiste
    let edit_style = WINDOW_STYLE(
        WS_CHILD.0 | WS_VISIBLE.0 | 0x00200000 | 0x0004 | 0x0040 | 0x1000,
    );
    let edit = CreateWindowExW(
        WINDOW_EX_STYLE(0),
        w!("EDIT"),
        w!(""),
        edit_style,
        PAD_H,
        PAD_V,
        win_w - 2 * PAD_H,
        win_h - 2 * PAD_V,
        hwnd,
        HMENU::default(),
        hinstance,
        None,
    )?;
    EDIT_HWND_VAL.store(edit.0 as isize, Ordering::Relaxed);

    // Schriftart: konfigurierte Punktgröße oder Windows-Standard.
    let font_handle = if font_size > 0 {
        // Punkte → logische Einheiten bei 96 DPI: px = pt * 96 / 72.
        let height = -(font_size as i32 * 96 / 72);
        let f = CreateFontW(height, 0, 0, 0, 400, 0, 0, 0, 1, 0, 0, 5, 0, w!("Segoe UI"));
        FONT_VAL.store(f.0 as isize, Ordering::Relaxed);
        f.0 as usize
    } else {
        let f = GetStockObject(DEFAULT_GUI_FONT);
        f.0 as usize
    };
    let _ = SendMessageW(edit, WM_SETFONT, WPARAM(font_handle), LPARAM(1));

    // Edit subclassen, um VK_ESCAPE abzufangen.
    let orig = SetWindowLongPtrW(edit, GWLP_WNDPROC, edit_subclass_proc as *const () as isize);
    ORIG_EDIT_PROC_VAL.store(orig, Ordering::Relaxed);

    Ok(hwnd)
}

unsafe extern "system" fn parent_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        // WA_INACTIVE (0) im Low-Word: Aktivierung verloren → ausblenden.
        WM_ACTIVATE if (wparam.0 & 0xFFFF) == 0 => {
            let _ = ShowWindow(hwnd, SW_HIDE);
            LRESULT(0)
        }
        // Hintergrund- und Textfarbe des Edit-Controls steuern.
        WM_CTLCOLOREDIT => {
            let hdc = HDC(wparam.0 as *mut _);
            let _ = SetTextColor(hdc, COLORREF(FG_COLOR));
            let _ = SetBkColor(hdc, COLORREF(BG_COLOR));
            LRESULT(BG_BRUSH_VAL.load(Ordering::Relaxed))
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe extern "system" fn edit_subclass_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_KEYDOWN && wparam.0 == VK_ESCAPE.0 as usize {
        do_hide(HWND(PARENT_HWND_VAL.load(Ordering::Relaxed) as *mut _));
        return LRESULT(0);
    }
    let orig = ORIG_EDIT_PROC_VAL.load(Ordering::Relaxed);
    if orig != 0 {
        return CallWindowProcW(std::mem::transmute(orig), hwnd, msg, wparam, lparam);
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}
