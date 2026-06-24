//! Overlay-Fenster in zwei Varianten:
//!  – **Subtle**: kleines, dezentes Eck-Overlay für Workspace-Namen
//!  – **Prominent**: großes, zentriertes Overlay für Respite-Meldungen
//!
//! Jede Instanz trägt ihre eigenen Zeichendaten (Text + Stil) über
//! `GWLP_USERDATA`, sodass beide Varianten gleichzeitig existieren können.

use std::sync::Mutex;

use anyhow::Result;
use windows::core::w;
use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateFontW, CreateSolidBrush, DeleteObject, DrawTextW, EndPaint, FillRect,
    InvalidateRect, SelectObject, SetBkMode, SetTextColor, TextOutW,
    BACKGROUND_MODE, DT_CENTER, DT_NOPREFIX, DT_WORDBREAK, HDC, HGDIOBJ, PAINTSTRUCT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetClientRect, GetWindowLongPtrW,
    GetSystemMetrics, RegisterClassW, SetLayeredWindowAttributes, SetWindowLongPtrW, ShowWindow,
    SystemParametersInfoW, GWLP_USERDATA, HMENU, LAYERED_WINDOW_ATTRIBUTES_FLAGS,
    SHOW_WINDOW_CMD, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
    SPI_GETWORKAREA, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, WM_ERASEBKGND,
    WM_PAINT, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
    WS_EX_TRANSPARENT, WS_POPUP,
};

use crate::config::OverlayCorner;

// ── Dezentes Eck-Overlay (Workspace-Namen) ────────────────────────────────────

const SUBTLE_W: i32 = 240;
const SUBTLE_H: i32 = 48;
const SUBTLE_MARGIN: i32 = 20;
/// BGR: dunkles Schiefergrau (R=52, G=58, B=68).
const SUBTLE_BG: u32 = 0x00_44_3A_34;
/// Schrift: weiß.
const SUBTLE_FG: u32 = 0x00_FF_FF_FF;
/// Alpha: leicht transparent.
const SUBTLE_ALPHA: u8 = 210;
const SUBTLE_FONT_H: i32 = 24;

// ── Prominentes Zentrum-Overlay (Respite-Meldungen) ───────────────────────────

// PROM_W / PROM_H entfallen – das prominente Overlay spannt alle Monitore auf.
/// BGR: helles, warmes Cremeweiß (R=250, G=250, B=245).
const PROM_BG: u32 = 0x00_F5_FA_FA;
/// Schrift: fast schwarz.
const PROM_FG: u32 = 0x00_1A_1A_1A;
/// Alpha: nahezu vollständig opak.
const PROM_ALPHA: u8 = 252;
const PROM_FONT_H: i32 = 40;

// ── Interne Fensterdaten ──────────────────────────────────────────────────────

enum OverlayStyle {
    Subtle,
    Prominent,
}

struct WindowData {
    text: Mutex<String>,
    style: OverlayStyle,
}

// ── Öffentliche API ───────────────────────────────────────────────────────────

/// Handle auf ein Overlay-Fenster.
pub struct Overlay {
    hwnd: HWND,
    /// Hält die Fensterdaten am Leben; der raw pointer davon steckt in GWLP_USERDATA.
    _data: Box<WindowData>,
}

impl Overlay {
    /// Kleines, dezentes Overlay in einer Bildschirmecke (Workspace-Anzeige).
    pub fn create_subtle(corner: &OverlayCorner, initial_text: &str) -> Result<Self> {
        let data = Box::new(WindowData {
            text: Mutex::new(initial_text.to_string()),
            style: OverlayStyle::Subtle,
        });
        let hwnd = unsafe { create_subtle_window(corner, &*data as *const WindowData)? };
        Ok(Self { hwnd, _data: data })
    }

    /// Großes, zentriertes Overlay für Respite-Meldungen.
    pub fn create_prominent(initial_text: &str) -> Result<Self> {
        let data = Box::new(WindowData {
            text: Mutex::new(initial_text.to_string()),
            style: OverlayStyle::Prominent,
        });
        let hwnd = unsafe { create_prominent_window(&*data as *const WindowData)? };
        Ok(Self { hwnd, _data: data })
    }

    /// Aktualisiert den angezeigten Text.
    pub fn update(&self, text: &str) {
        if let Ok(mut g) = self._data.text.lock() {
            *g = text.to_string();
        }
        unsafe {
            let _ = InvalidateRect(self.hwnd, None, true);
        }
    }
}

impl Drop for Overlay {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.hwnd);
        }
    }
}

// ── Fenstererzeugung ──────────────────────────────────────────────────────────

/// Registriert die Fensterklasse (einmalig; Doppelregistrierung wird ignoriert).
unsafe fn ensure_class(hinstance: HINSTANCE) {
    let wc = WNDCLASSW {
        lpfnWndProc: Some(wndproc),
        hInstance: hinstance,
        lpszClassName: w!("WaystoneOverlay"),
        ..Default::default()
    };
    let _ = RegisterClassW(&wc);
}

/// Gemeinsame Hilfsfunktion: erstellt ein Overlay-Fenster und setzt GWLP_USERDATA.
unsafe fn make_window(
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    alpha: u8,
    data_ptr: *const WindowData,
) -> Result<HWND> {
    let hinstance = HINSTANCE(GetModuleHandleW(None)?.0);
    ensure_class(hinstance);

    let ex_style =
        WS_EX_TOPMOST | WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW;

    let hwnd = CreateWindowExW(
        ex_style,
        w!("WaystoneOverlay"),
        w!(""),
        WS_POPUP,
        x,
        y,
        w,
        h,
        HWND::default(),
        HMENU::default(),
        hinstance,
        None,
    )?;

    SetWindowLongPtrW(hwnd, GWLP_USERDATA, data_ptr as isize);
    let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), alpha, LAYERED_WINDOW_ATTRIBUTES_FLAGS(2));
    // SW_SHOWNOACTIVATE = 4
    let _ = ShowWindow(hwnd, SHOW_WINDOW_CMD(4));

    Ok(hwnd)
}

unsafe fn work_area() -> RECT {
    let mut work = RECT::default();
    let _ = SystemParametersInfoW(
        SPI_GETWORKAREA,
        0,
        Some(&mut work as *mut RECT as *mut std::ffi::c_void),
        SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
    );
    work
}

unsafe fn create_subtle_window(corner: &OverlayCorner, data_ptr: *const WindowData) -> Result<HWND> {
    let work = work_area();
    let (x, y) = subtle_corner_pos(corner, &work);
    make_window(x, y, SUBTLE_W, SUBTLE_H, SUBTLE_ALPHA, data_ptr)
}

unsafe fn create_prominent_window(data_ptr: *const WindowData) -> Result<HWND> {
    // Alle Monitore abdecken: virtuellen Bildschirm abfragen.
    let x = GetSystemMetrics(SM_XVIRTUALSCREEN);
    let y = GetSystemMetrics(SM_YVIRTUALSCREEN);
    let w = GetSystemMetrics(SM_CXVIRTUALSCREEN);
    let h = GetSystemMetrics(SM_CYVIRTUALSCREEN);
    make_window(x, y, w, h, PROM_ALPHA, data_ptr)
}

fn subtle_corner_pos(corner: &OverlayCorner, work: &RECT) -> (i32, i32) {
    let cx = work.left + (work.right - work.left - SUBTLE_W) / 2;
    match corner {
        OverlayCorner::TopLeft => (work.left + SUBTLE_MARGIN, work.top + SUBTLE_MARGIN),
        OverlayCorner::TopCenter => (cx, work.top + SUBTLE_MARGIN),
        OverlayCorner::TopRight => (work.right - SUBTLE_W - SUBTLE_MARGIN, work.top + SUBTLE_MARGIN),
        OverlayCorner::BottomLeft => (work.left + SUBTLE_MARGIN, work.bottom - SUBTLE_H - SUBTLE_MARGIN),
        OverlayCorner::BottomCenter => (cx, work.bottom - SUBTLE_H - SUBTLE_MARGIN),
        OverlayCorner::BottomRight => {
            (work.right - SUBTLE_W - SUBTLE_MARGIN, work.bottom - SUBTLE_H - SUBTLE_MARGIN)
        }
    }
}

// ── Fensterprozedur ───────────────────────────────────────────────────────────

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => {
            let data_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const WindowData;
            if data_ptr.is_null() {
                return DefWindowProcW(hwnd, msg, wparam, lparam);
            }
            let data = &*data_ptr;

            let text = data.text.lock().map(|g| g.clone()).unwrap_or_default();
            let text_w: Vec<u16> = text.encode_utf16().collect();

            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);

            let mut rc = RECT::default();
            let _ = GetClientRect(hwnd, &mut rc);

            match data.style {
                OverlayStyle::Subtle => paint_subtle(hdc, &rc, &text_w),
                OverlayStyle::Prominent => paint_prominent(hdc, &rc, &text_w),
            }

            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn paint_subtle(hdc: HDC, rc: &RECT, text_w: &[u16]) {
    let bg = CreateSolidBrush(COLORREF(SUBTLE_BG));
    FillRect(hdc, rc, bg);
    let _ = DeleteObject(HGDIOBJ(bg.0));

    let font = CreateFontW(
        SUBTLE_FONT_H, 0, 0, 0,
        700, 0, 0, 0, 1, 0, 0, 5, 0,
        w!("Segoe UI"),
    );
    let old = SelectObject(hdc, HGDIOBJ(font.0));
    SetBkMode(hdc, BACKGROUND_MODE(1));
    SetTextColor(hdc, COLORREF(SUBTLE_FG));

    let text_y = (SUBTLE_H - SUBTLE_FONT_H) / 2;
    let _ = TextOutW(hdc, 14, text_y, text_w);

    SelectObject(hdc, old);
    let _ = DeleteObject(HGDIOBJ(font.0));
}

unsafe fn paint_prominent(hdc: HDC, rc: &RECT, text_w: &[u16]) {
    // Hintergrund über alle Monitore füllen.
    let bg = CreateSolidBrush(COLORREF(PROM_BG));
    FillRect(hdc, rc, bg);
    let _ = DeleteObject(HGDIOBJ(bg.0));

    let font = CreateFontW(
        PROM_FONT_H, 0, 0, 0,
        700, 0, 0, 0, 1, 0, 0, 5, 0,
        w!("Segoe UI"),
    );
    let old = SelectObject(hdc, HGDIOBJ(font.0));
    SetBkMode(hdc, BACKGROUND_MODE(1));
    SetTextColor(hdc, COLORREF(PROM_FG));

    // Text auf dem Tray-Monitor zentrieren (primärer Monitor = work_area).
    // Das Fenster liegt bei (SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN); work_area()
    // liefert Bildschirmkoordinaten → in Clientkoordinaten umrechnen.
    let vx = GetSystemMetrics(SM_XVIRTUALSCREEN);
    let vy = GetSystemMetrics(SM_YVIRTUALSCREEN);
    let wa = work_area();
    const PAD: i32 = 40;
    let mut draw_rc = RECT {
        left:   wa.left   - vx + PAD,
        top:    wa.top    - vy + PAD,
        right:  wa.right  - vx - PAD,
        bottom: wa.bottom - vy - PAD,
    };

    // Erster Pass: Texthöhe berechnen.
    let mut calc_rc = draw_rc;
    let mut tmp = text_w.to_vec();
    DrawTextW(hdc, &mut tmp, &mut calc_rc,
        DT_CENTER | DT_WORDBREAK | DT_NOPREFIX | windows::Win32::Graphics::Gdi::DT_CALCRECT);
    // Zweiter Pass: vertikal zentrieren und zeichnen.
    let text_h = calc_rc.bottom - calc_rc.top;
    let avail_h = draw_rc.bottom - draw_rc.top;
    if text_h < avail_h {
        draw_rc.top += (avail_h - text_h) / 2;
    }
    let mut text_w_draw = text_w.to_vec();
    DrawTextW(hdc, &mut text_w_draw, &mut draw_rc, DT_CENTER | DT_WORDBREAK | DT_NOPREFIX);

    SelectObject(hdc, old);
    let _ = DeleteObject(HGDIOBJ(font.0));
}
