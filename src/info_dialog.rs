//! Info-Dialog: zeigt Versionsinformationen als gerendertes Markdown via WebView2.
//! Gleiche Titelzeile wie quick_input, Inhalt aus InfoDialog.md (compile-time).

use std::num::NonZeroIsize;
use std::sync::atomic::{AtomicIsize, Ordering};

use anyhow::Result;
use windows::core::w;
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetSystemMetrics, GetWindow,
    IsWindowVisible, RegisterClassW, SetForegroundWindow, ShowWindow, HMENU,
    GW_CHILD, SM_CXSCREEN, SM_CYSCREEN, SW_HIDE, SW_SHOW, WNDCLASSW, WM_SETFOCUS,
    WS_EX_TOPMOST, WS_POPUP,
};

use wry::dpi::{PhysicalPosition, PhysicalSize};
use wry::http::Request;
use wry::raw_window_handle::{
    HasWindowHandle, HandleError, RawWindowHandle, Win32WindowHandle, WindowHandle,
};
use wry::{Rect, WebContext, WebViewBuilder};

static INFO_HWND_VAL: AtomicIsize = AtomicIsize::new(0);

const HTML_TEMPLATE: &str = include_str!("info_dialog.html");
const INFO_MD: &str = include_str!("InfoDialog.md");

pub struct InfoDialog {
    hwnd: HWND,
    webview: Option<wry::WebView>,
}

impl InfoDialog {
    pub fn create() -> Result<Self> {
        let (hwnd, webview) = unsafe { create_window()? };
        Ok(Self { hwnd, webview: Some(webview) })
    }

    pub fn show(&self) {
        unsafe {
            if IsWindowVisible(self.hwnd).as_bool() {
                let _ = SetForegroundWindow(self.hwnd);
            } else {
                let _ = ShowWindow(self.hwnd, SW_SHOW);
                let _ = SetForegroundWindow(self.hwnd);
            }
        }
    }
}

impl Drop for InfoDialog {
    fn drop(&mut self) {
        self.webview.take();
        unsafe { let _ = DestroyWindow(self.hwnd); }
    }
}

// ── Wrapper für raw-window-handle ────────────────────────────────────────────

struct HwndHandle(HWND);

impl HasWindowHandle for HwndHandle {
    fn window_handle(&self) -> std::result::Result<WindowHandle<'_>, HandleError> {
        let handle = Win32WindowHandle::new(
            NonZeroIsize::new(self.0.0 as isize).expect("null HWND"),
        );
        unsafe { Ok(WindowHandle::borrow_raw(RawWindowHandle::Win32(handle))) }
    }
}

// ── Fenstererstellung ────────────────────────────────────────────────────────

unsafe fn create_window() -> Result<(HWND, wry::WebView)> {
    let hinstance = HINSTANCE(GetModuleHandleW(None)?.0);
    let class_name = w!("WaystoneInfoDialog");

    let wc = WNDCLASSW {
        lpfnWndProc: Some(wndproc),
        hInstance: hinstance,
        lpszClassName: class_name,
        ..Default::default()
    };
    let _ = RegisterClassW(&wc);

    let sw = GetSystemMetrics(SM_CXSCREEN);
    let sh = GetSystemMetrics(SM_CYSCREEN);
    let win_w = 720_i32;
    let win_h = 560_i32;
    let x = (sw - win_w) / 2;
    let y = (sh - win_h) / 2;

    let hwnd = CreateWindowExW(
        WS_EX_TOPMOST,
        class_name,
        w!(""),
        WS_POPUP,
        x, y, win_w, win_h,
        HWND::default(),
        HMENU::default(),
        hinstance,
        None,
    )?;
    INFO_HWND_VAL.store(hwnd.0 as isize, Ordering::Relaxed);

    // Markdown für JS-Template-Literal escapen (`, \, ${).
    let md_escaped = INFO_MD
        .replace("{VERSION}", env!("CARGO_PKG_VERSION"))
        .replace('\\', "\\\\")
        .replace('`', "\\`")
        .replace("${", "\\${");
    let html = HTML_TEMPLATE.replace("{MD_CONTENT}", &md_escaped);

    // Eigenes Datenverzeichnis für diesen WebView2-Kontext.
    let data_dir = std::env::var("LOCALAPPDATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir())
        .join("Waystone-Ridge")
        .join("InfoDialog");
    let _ = std::fs::create_dir_all(&data_dir);
    let mut web_context = WebContext::new(Some(data_dir));

    let hwnd_val = hwnd.0 as isize;
    let webview = WebViewBuilder::new_with_web_context(&mut web_context)
        .with_html(html)
        .with_bounds(Rect {
            position: PhysicalPosition::new(0_i32, 0_i32).into(),
            size: PhysicalSize::<u32>::new(win_w as u32, win_h as u32).into(),
        })
        .with_ipc_handler(move |request: Request<String>| {
            let body = request.body();
            if body == "close" {
                unsafe { let _ = ShowWindow(HWND(hwnd_val as *mut _), SW_HIDE); }
            } else if let Some(url) = body.strip_prefix("url:") {
                open_url(url);
            }
        })
        .build_as_child(&HwndHandle(hwnd))?;

    Ok((hwnd, webview))
}

// ── URL im Standardbrowser öffnen ────────────────────────────────────────────

fn open_url(url: &str) {
    let _ = std::process::Command::new("cmd")
        .args(["/C", "start", "", url])
        .spawn();
}

// ── Fensterprozedur ──────────────────────────────────────────────────────────

unsafe extern "system" fn wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // Win32-Tastaturfokus an das WebView2-Childfenster weiterleiten.
    if msg == WM_SETFOCUS {
        if let Ok(child) = GetWindow(hwnd, GW_CHILD) {
            let _ = SetFocus(child);
        }
        return LRESULT(0);
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}
