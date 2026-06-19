//! Randloses, immer-oben Markdown-Memo, das per Hotkey ein-/ausgeblendet wird.
//! Editor links (Textarea), Live-Vorschau rechts. Implementiert mit WebView2 (wry).

use std::num::NonZeroIsize;
use std::sync::atomic::{AtomicIsize, Ordering};

use anyhow::Result;
use windows::core::w;
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetForegroundWindow, GetSystemMetrics,
    GetWindow, IsWindowVisible, RegisterClassW, SetForegroundWindow, ShowWindow, HMENU,
    GW_CHILD, SM_CXSCREEN, SM_CYSCREEN, SW_HIDE, SW_SHOW, WNDCLASSW, WM_ACTIVATE,
    WM_SETFOCUS, WS_EX_TOPMOST, WS_POPUP,
};

use wry::dpi::{PhysicalPosition, PhysicalSize};
use wry::http::Request;
use wry::raw_window_handle::{
    HasWindowHandle, HandleError, RawWindowHandle, Win32WindowHandle, WindowHandle,
};
use wry::{Rect, WebViewBuilder};

static PARENT_HWND_VAL: AtomicIsize = AtomicIsize::new(0);
static PREV_FOREGROUND_VAL: AtomicIsize = AtomicIsize::new(0);

const HTML_TEMPLATE: &str = include_str!("quick_input.html");

pub struct QuickInput {
    hwnd: HWND,
    webview: Option<wry::WebView>,
}

impl QuickInput {
    /// Erstellt das Fenster (anfangs unsichtbar).
    ///
    /// - `width_pct` / `height_pct`: Fenstergröße in Prozent der Bildschirmmaße (5–95).
    /// - `font_size`: Schriftgröße in Punkt; 0 = 14 px Standard.
    pub fn create(width_pct: u32, height_pct: u32, font_size: u32) -> Result<Self> {
        let (hwnd, webview) = unsafe { create_window(width_pct, height_pct, font_size)? };
        Ok(Self { hwnd, webview: Some(webview) })
    }

    /// Ein- oder Ausblenden je nach aktuellem Zustand.
    pub fn toggle(&self) {
        unsafe {
            if IsWindowVisible(self.hwnd).as_bool() {
                hide_with_restore(self.hwnd);
            } else {
                let prev = GetForegroundWindow();
                if prev != self.hwnd {
                    PREV_FOREGROUND_VAL.store(prev.0 as isize, Ordering::Relaxed);
                }
                let _ = ShowWindow(self.hwnd, SW_SHOW);
                let _ = SetForegroundWindow(self.hwnd);
                if let Some(wv) = &self.webview {
                    let _ = wv.evaluate_script("if(window.wysiwyg)window.wysiwyg.show()");
                }
            }
        }
    }
}

impl Drop for QuickInput {
    fn drop(&mut self) {
        // WebView zuerst aufräumen, dann das Elternfenster zerstören.
        self.webview.take();
        unsafe { let _ = DestroyWindow(self.hwnd); }
    }
}

// ── Wrapper für raw-window-handle ────────────────────────────────────────────

struct HwndHandle(HWND);

impl HasWindowHandle for HwndHandle {
    fn window_handle(&self) -> std::result::Result<WindowHandle<'_>, HandleError> {
        let hwnd_isize = self.0.0 as isize;
        let handle = Win32WindowHandle::new(
            NonZeroIsize::new(hwnd_isize).expect("null HWND"),
        );
        unsafe { Ok(WindowHandle::borrow_raw(RawWindowHandle::Win32(handle))) }
    }
}

// ── Hilfsfunktionen ──────────────────────────────────────────────────────────

unsafe fn hide_with_restore(hwnd: HWND) {
    let _ = ShowWindow(hwnd, SW_HIDE);
    let raw = PREV_FOREGROUND_VAL.load(Ordering::Relaxed);
    if raw != 0 {
        let _ = SetForegroundWindow(HWND(raw as *mut _));
    }
}

// ── Fenstererstellung ────────────────────────────────────────────────────────

unsafe fn create_window(
    width_pct: u32,
    height_pct: u32,
    font_size: u32,
) -> Result<(HWND, wry::WebView)> {
    let hinstance = HINSTANCE(GetModuleHandleW(None)?.0);
    let class_name = w!("WaystoneQuickInput");

    let wc = WNDCLASSW {
        lpfnWndProc: Some(parent_wndproc),
        hInstance: hinstance,
        lpszClassName: class_name,
        ..Default::default()
    };
    let _ = RegisterClassW(&wc);

    let sw = GetSystemMetrics(SM_CXSCREEN);
    let sh = GetSystemMetrics(SM_CYSCREEN);
    let win_w = (sw as u32 * width_pct.clamp(5, 95) / 100) as i32;
    let win_h = (sh as u32 * height_pct.clamp(5, 95) / 100) as i32;
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

    // Punkte → logische Pixel bei 96 DPI; 0 → 14 px (entspricht ~10 pt Segoe UI).
    let font_px = if font_size > 0 { font_size * 96 / 72 } else { 14 };
    let html = HTML_TEMPLATE.replace("{FONT_SIZE}", &font_px.to_string());

    let hwnd_val = hwnd.0 as isize;
    let webview = WebViewBuilder::new()
        .with_html(html)
        .with_bounds(Rect {
            position: PhysicalPosition::new(0_i32, 0_i32).into(),
            size: PhysicalSize::<u32>::new(win_w as u32, win_h as u32).into(),
        })
        .with_ipc_handler(move |request: Request<String>| {
            if request.body() == "close" {
                unsafe { hide_with_restore(HWND(hwnd_val as *mut _)); }
            }
        })
        .build_as_child(&HwndHandle(hwnd))?;

    Ok((hwnd, webview))
}

// ── Fensterprozedur ──────────────────────────────────────────────────────────

unsafe extern "system" fn parent_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // WA_INACTIVE (Low-Word = 0): Fenster verliert Aktivierung → ausblenden.
    // Fokus geht durch den auslösenden Klick automatisch zum anderen Fenster.
    if msg == WM_ACTIVATE && (wparam.0 & 0xFFFF) == 0 {
        let _ = ShowWindow(hwnd, SW_HIDE);
        return LRESULT(0);
    }
    // WM_SETFOCUS: Win32-Tastaturfokus an das WebView2-Childfenster weiterleiten.
    // Ohne das erhält das WebView2-Child keinen Fokus, wenn das Elternfenster
    // nach einem Hide/Show per SetForegroundWindow wieder in den Vordergrund kommt.
    if msg == WM_SETFOCUS {
        if let Ok(child) = GetWindow(hwnd, GW_CHILD) {
            let _ = SetFocus(child);
        }
        return LRESULT(0);
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}
