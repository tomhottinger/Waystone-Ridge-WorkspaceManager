//! Permanentes Desktop-Overlay: always-on-top, halbtransparent, zeigt den
//! aktuellen Workspace-Namen. Wird nur erzeugt wenn `show_overlay = true` in
//! der config.toml gesetzt ist.

use std::sync::{Mutex, OnceLock};

use anyhow::Result;
use windows::core::w;
use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateFontW, CreateSolidBrush, DeleteObject, EndPaint, FillRect, InvalidateRect,
    SelectObject, SetBkMode, SetTextColor, TextOutW, BACKGROUND_MODE, HGDIOBJ, PAINTSTRUCT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetClientRect, RegisterClassW,
    SetLayeredWindowAttributes, ShowWindow, SystemParametersInfoW, HMENU,
    LAYERED_WINDOW_ATTRIBUTES_FLAGS, SHOW_WINDOW_CMD, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
    WM_ERASEBKGND, WM_PAINT, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};

use crate::config::OverlayCorner;

const WIN_W: i32 = 240;
const WIN_H: i32 = 48;
const MARGIN: i32 = 20;
/// Hintergrundfarbe (BGR): dunkles Schieferblau.
const BG_COLOR: u32 = 0x00_3A_28_1E;
/// Schriftfarbe: weiß.
const FG_COLOR: u32 = 0x00_FF_FF_FF;
/// Globale Alpha-Deckkraft (0=transparent … 255=opak).
const ALPHA: u8 = 210;

/// Gemeinsamer Speicher für den anzuzeigenden Text (wird aus dem Haupt-Thread
/// geschrieben und aus dem Overlay-WndProc auf dem selben Thread gelesen).
static LABEL: OnceLock<Mutex<String>> = OnceLock::new();

fn label_mutex() -> &'static Mutex<String> {
    LABEL.get_or_init(|| Mutex::new(String::new()))
}

/// Handle auf das Overlay-Fenster. Wird beim Beenden zerstört.
pub struct Overlay {
    hwnd: HWND,
}

impl Overlay {
    /// Erzeugt das Overlay-Fenster.
    pub fn create(corner: &OverlayCorner, initial_text: &str) -> Result<Self> {
        *label_mutex().lock().unwrap() = initial_text.to_string();
        let hwnd = unsafe { create_window(corner)? };
        Ok(Self { hwnd })
    }

    /// Aktualisiert den angezeigten Text und löst einen Neuzeichenvorgang aus.
    pub fn update(&self, text: &str) {
        *label_mutex().lock().unwrap() = text.to_string();
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

unsafe fn create_window(corner: &OverlayCorner) -> Result<HWND> {
    let hinstance = HINSTANCE(GetModuleHandleW(None)?.0);
    let class_name = w!("WaystoneOverlay");

    let wc = WNDCLASSW {
        lpfnWndProc: Some(wndproc),
        hInstance: hinstance,
        lpszClassName: class_name,
        ..Default::default()
    };
    // Fehler ignorieren – Klasse ist nach dem ersten Aufruf bereits registriert.
    let _ = RegisterClassW(&wc);

    // Arbeitsbereich (ohne Taskleiste) für Positionierung verwenden.
    let mut work = RECT::default();
    let _ = SystemParametersInfoW(
        windows::Win32::UI::WindowsAndMessaging::SPI_GETWORKAREA,
        0,
        Some(&mut work as *mut RECT as *mut std::ffi::c_void),
        SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
    );

    let (x, y) = corner_pos(corner, &work);

    let ex_style = WS_EX_TOPMOST
        | WS_EX_LAYERED
        | WS_EX_TRANSPARENT
        | WS_EX_NOACTIVATE
        | WS_EX_TOOLWINDOW;

    let hwnd = CreateWindowExW(
        ex_style,
        class_name,
        w!(""),
        WS_POPUP,
        x,
        y,
        WIN_W,
        WIN_H,
        HWND::default(),
        HMENU::default(),
        hinstance,
        None,
    )?;

    // Globale Deckkraft über LWA_ALPHA setzen.
    let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), ALPHA, LAYERED_WINDOW_ATTRIBUTES_FLAGS(2));

    // Fenster anzeigen, ohne es zu aktivieren (SW_SHOWNOACTIVATE = 4).
    let _ = ShowWindow(hwnd, SHOW_WINDOW_CMD(4));

    Ok(hwnd)
}

fn corner_pos(corner: &OverlayCorner, work: &RECT) -> (i32, i32) {
    let center_x = work.left + (work.right - work.left - WIN_W) / 2;
    match corner {
        OverlayCorner::TopLeft => (work.left + MARGIN, work.top + MARGIN),
        OverlayCorner::TopCenter => (center_x, work.top + MARGIN),
        OverlayCorner::TopRight => (work.right - WIN_W - MARGIN, work.top + MARGIN),
        OverlayCorner::BottomLeft => (work.left + MARGIN, work.bottom - WIN_H - MARGIN),
        OverlayCorner::BottomCenter => (center_x, work.bottom - WIN_H - MARGIN),
        OverlayCorner::BottomRight => (work.right - WIN_W - MARGIN, work.bottom - WIN_H - MARGIN),
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);

            let mut rc = RECT::default();
            let _ = GetClientRect(hwnd, &mut rc);

            // Hintergrund füllen.
            let bg = CreateSolidBrush(COLORREF(BG_COLOR));
            FillRect(hdc, &rc, bg);
            let _ = DeleteObject(HGDIOBJ(bg.0));

            // Text lesen und zeichnen.
            let text = label_mutex().lock().unwrap().clone();
            let text_w: Vec<u16> = text.encode_utf16().collect();

            let font_h: i32 = 24;
            let font = CreateFontW(
                font_h,
                0,
                0,
                0,
                700, // FW_BOLD
                0,
                0,
                0,
                1, // DEFAULT_CHARSET
                0,
                0,
                5, // CLEARTYPE_QUALITY
                0,
                w!("Segoe UI"),
            );
            let old = SelectObject(hdc, HGDIOBJ(font.0));
            SetBkMode(hdc, BACKGROUND_MODE(1)); // TRANSPARENT
            SetTextColor(hdc, COLORREF(FG_COLOR));

            let text_y = (WIN_H - font_h) / 2;
            let _ = TextOutW(hdc, 14, text_y, &text_w);

            SelectObject(hdc, old);
            let _ = DeleteObject(HGDIOBJ(font.0));
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
