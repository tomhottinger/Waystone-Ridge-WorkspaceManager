//! Info-Dialog: zeigt Versionsinformationen und Lizenzhinweise zur App.

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};

use windows::core::w;
use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateFontW, CreatePen, CreateSolidBrush, DeleteObject, DrawTextW, EndPaint,
    FillRect, LineTo, MoveToEx, SelectObject, SetBkMode, SetTextColor, BACKGROUND_MODE,
    DRAW_TEXT_FORMAT, HDC, HGDIOBJ, PAINTSTRUCT, PS_SOLID,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    AdjustWindowRectEx, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    GetMessageW, GetSystemMetrics, PostQuitMessage, RegisterClassW, ShowWindow, TranslateMessage,
    BS_DEFPUSHBUTTON, HMENU, MSG, SM_CXSCREEN, SM_CYSCREEN, SW_SHOW, WINDOW_EX_STYLE,
    WINDOW_STYLE, WM_CLOSE, WM_COMMAND, WM_DESTROY, WM_LBUTTONDOWN, WM_PAINT, WNDCLASSW,
    WS_CAPTION, WS_CHILD, WS_SYSMENU, WS_VISIBLE,
};

// --- Layout ------------------------------------------------------------------
const CLIENT_W: i32 = 440;
const CLIENT_H: i32 = 520;
const HEADER_H: i32 = 100;
const PAD: i32 = 28;
const BTN_ID: usize = 101;
const GITHUB_URL: &str = "https://github.com/tomhottinger/Waystone-Ridge-WorkspaceManager";

// --- Farben (COLORREF = R | G<<8 | B<<16) ------------------------------------
const C_HDR_BG: u32 = 0x00503219; // RGB(25,  50,  80) – dunkles Stahlblau
const C_HDR_FG: u32 = 0x00FFFFFF; // weiß
const C_HDR_SUB: u32 = 0x00E1C8B4; // RGB(180,200,225) – helles Stahlblau
const C_BODY_BG: u32 = 0x00FAF7F5; // RGB(245,247,250) – fast weiß
const C_SECTION: u32 = 0x00503219; // wie Header
const C_TEXT: u32 = 0x00373232; // RGB( 50, 50, 55) – fast schwarz
const C_COPY: u32 = 0x006E6464; // RGB(100,100,110) – grau
const C_LINK: u32 = 0x00C86400; // RGB(  0,100,200) – blau
const C_SEP: u32 = 0x00E1D2C8; // RGB(200,210,225) – heller Teiler

// DT_* Flags
const DT_SINGLE: DRAW_TEXT_FORMAT = DRAW_TEXT_FORMAT(0x20); // DT_SINGLELINE
const DT_WRAP: DRAW_TEXT_FORMAT = DRAW_TEXT_FORMAT(0x10); // DT_WORDBREAK

static DIALOG_OPEN: AtomicBool = AtomicBool::new(false);

thread_local! {
    static LINK_RECTS: RefCell<[RECT; 2]> = RefCell::new([RECT::default(); 2]);
}

/// Öffnet den Info-Dialog. Ein zweiter Aufruf, solange der Dialog geöffnet ist,
/// wird ignoriert.
pub fn show() {
    if DIALOG_OPEN.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(|| {
        unsafe { run_dialog() };
        DIALOG_OPEN.store(false, Ordering::SeqCst);
    });
}

// -----------------------------------------------------------------------------

unsafe fn run_dialog() {
    let hinstance = match GetModuleHandleW(None) {
        Ok(h) => HINSTANCE(h.0),
        Err(_) => return,
    };

    let class_name = w!("WaystoneInfoDlg");
    let wc = WNDCLASSW {
        lpfnWndProc: Some(wndproc),
        hInstance: hinstance,
        lpszClassName: class_name,
        ..Default::default()
    };
    let _ = RegisterClassW(&wc);

    let style = WS_CAPTION | WS_SYSMENU;
    let mut rc = RECT { left: 0, top: 0, right: CLIENT_W, bottom: CLIENT_H };
    let _ = AdjustWindowRectEx(&mut rc, style, false, WINDOW_EX_STYLE(0));
    let win_w = rc.right - rc.left;
    let win_h = rc.bottom - rc.top;

    let sx = GetSystemMetrics(SM_CXSCREEN);
    let sy = GetSystemMetrics(SM_CYSCREEN);

    let hwnd = match CreateWindowExW(
        WINDOW_EX_STYLE(0),
        class_name,
        w!("Über Waystone Ridge"),
        style,
        (sx - win_w) / 2,
        (sy - win_h) / 2,
        win_w,
        win_h,
        HWND::default(),
        HMENU::default(),
        hinstance,
        None,
    ) {
        Ok(h) => h,
        Err(_) => return,
    };

    let _ = CreateWindowExW(
        WINDOW_EX_STYLE(0),
        w!("BUTTON"),
        w!("Schließen"),
        WINDOW_STYLE(WS_CHILD.0 | WS_VISIBLE.0 | BS_DEFPUSHBUTTON as u32),
        CLIENT_W - PAD - 110,
        CLIENT_H - 52,
        110,
        32,
        hwnd,
        HMENU(BTN_ID as *mut _),
        hinstance,
        None,
    );

    let _ = ShowWindow(hwnd, SW_SHOW);

    let mut msg = MSG::default();
    while GetMessageW(&mut msg, None, 0, 0).as_bool() {
        let _ = TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }
}

unsafe extern "system" fn wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            paint(hdc);
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_COMMAND => {
            if wparam.0 & 0xFFFF == BTN_ID {
                let _ = DestroyWindow(hwnd);
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let x = (lparam.0 & 0xFFFF) as i16 as i32;
            let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
            LINK_RECTS.with(|rects| {
                for rc in rects.borrow().iter() {
                    if x >= rc.left && x < rc.right && y >= rc.top && y < rc.bottom {
                        open_url();
                        break;
                    }
                }
            });
            LRESULT(0)
        }
        WM_CLOSE => {
            let _ = DestroyWindow(hwnd);
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn open_url() {
    use std::os::windows::process::CommandExt;
    let _ = std::process::Command::new("cmd")
        .raw_arg(format!("/C start {}", GITHUB_URL))
        .spawn();
}

// --- Zeichnen ----------------------------------------------------------------

unsafe fn paint(hdc: HDC) {
    // Fonts
    let f_title = make_font(-28, 700, 0, 0);
    let f_sub = make_font(-13, 400, 0, 0);
    let f_sec = make_font(-14, 700, 0, 0);
    let f_body = make_font(-13, 400, 0, 0);
    let f_copy = make_font(-13, 400, 1, 0);
    let f_link = make_font(-13, 400, 0, 1);

    SetBkMode(hdc, BACKGROUND_MODE(1)); // TRANSPARENT

    // ── Header ───────────────────────────────────────────────────────────────
    let hdr_brush = CreateSolidBrush(COLORREF(C_HDR_BG));
    FillRect(hdc, &RECT { left: 0, top: 0, right: CLIENT_W, bottom: HEADER_H }, hdr_brush);
    let _ = DeleteObject(HGDIOBJ(hdr_brush.0));

    let orig = SelectObject(hdc, HGDIOBJ(f_title.0));
    SetTextColor(hdc, COLORREF(C_HDR_FG));
    dt(hdc, "Waystone Ridge", 14, 14, CLIENT_W - PAD, 52, DT_SINGLE);

    SelectObject(hdc, HGDIOBJ(f_sub.0));
    SetTextColor(hdc, COLORREF(C_HDR_SUB));
    let ver = format!("v{}", env!("CARGO_PKG_VERSION"));
    dt(hdc, &ver, PAD, 54, CLIENT_W - PAD, 72, DT_SINGLE);
    dt(hdc, "Workspace Manager für Windows", PAD, 72, CLIENT_W - PAD, 96, DT_SINGLE);

    // ── Body-Hintergrund ─────────────────────────────────────────────────────
    let body_brush = CreateSolidBrush(COLORREF(C_BODY_BG));
    FillRect(
        hdc,
        &RECT { left: 0, top: HEADER_H, right: CLIENT_W, bottom: CLIENT_H },
        body_brush,
    );
    let _ = DeleteObject(HGDIOBJ(body_brush.0));

    // Copyright
    SelectObject(hdc, HGDIOBJ(f_copy.0));
    SetTextColor(hdc, COLORREF(C_COPY));
    dt(hdc, "Copyright © 2026 Thomas C. Hottinger", PAD, 114, CLIENT_W - PAD, 132, DT_SINGLE);

    sep(hdc, PAD, 142, CLIENT_W - PAD);

    // ── Waystone Tools ───────────────────────────────────────────────────────
    SelectObject(hdc, HGDIOBJ(f_sec.0));
    SetTextColor(hdc, COLORREF(C_SECTION));
    dt(hdc, "Waystone Tools", PAD, 152, CLIENT_W - PAD, 172, DT_SINGLE);

    SelectObject(hdc, HGDIOBJ(f_body.0));
    SetTextColor(hdc, COLORREF(C_TEXT));
    dt(
        hdc,
        "Die Waystone Tools sind eine Gruppe von kleinen Executables, die den Alltag eines fortgeschrittenen Benutzers erleichtern sollen.",
        PAD, 174, CLIENT_W - PAD, 230,
        DT_WRAP,
    );

    sep(hdc, PAD, 238, CLIENT_W - PAD);

    // ── Waystone Ridge ───────────────────────────────────────────────────────
    SelectObject(hdc, HGDIOBJ(f_sec.0));
    SetTextColor(hdc, COLORREF(C_SECTION));
    dt(hdc, "Waystone Ridge", PAD, 248, CLIENT_W - PAD, 268, DT_SINGLE);

    SelectObject(hdc, HGDIOBJ(f_body.0));
    SetTextColor(hdc, COLORREF(C_TEXT));
    dt(
        hdc,
        "Im Moment gibt es nur Waystone-Ridge, ein Workspace Manager.",
        PAD, 270, CLIENT_W - PAD, 290,
        DT_WRAP,
    );

    SelectObject(hdc, HGDIOBJ(f_link.0));
    SetTextColor(hdc, COLORREF(C_LINK));
    let mut link1 = RECT { left: PAD, top: 298, right: PAD + 230, bottom: 318 };
    DrawTextW(hdc, &mut to_wide("Projektseite auf GitHub →"), &mut link1, DT_SINGLE);

    sep(hdc, PAD, 330, CLIENT_W - PAD);

    // ── Lizenz ───────────────────────────────────────────────────────────────
    SelectObject(hdc, HGDIOBJ(f_sec.0));
    SetTextColor(hdc, COLORREF(C_SECTION));
    dt(hdc, "Lizenz und Disclaimer", PAD, 340, CLIENT_W - PAD, 360, DT_SINGLE);

    SelectObject(hdc, HGDIOBJ(f_body.0));
    SetTextColor(hdc, COLORREF(C_TEXT));
    dt(
        hdc,
        "MIT Lizenz mit Haftungsausschluss.",
        PAD, 362, CLIENT_W - PAD, 382,
        DT_WRAP,
    );

    SelectObject(hdc, HGDIOBJ(f_link.0));
    SetTextColor(hdc, COLORREF(C_LINK));
    let mut link2 = RECT { left: PAD, top: 390, right: PAD + 200, bottom: 410 };
    DrawTextW(hdc, &mut to_wide("Lizenztext auf GitHub →"), &mut link2, DT_SINGLE);

    // Originalfont wiederherstellen, dann alle Fonts löschen
    SelectObject(hdc, orig);
    for h in [f_title, f_sub, f_sec, f_body, f_copy, f_link] {
        let _ = DeleteObject(HGDIOBJ(h.0));
    }

    // Link-Rects für Klick-Erkennung speichern
    LINK_RECTS.with(|r| {
        let mut r = r.borrow_mut();
        r[0] = link1;
        r[1] = link2;
    });
}

fn make_font(height: i32, weight: i32, italic: u32, underline: u32) -> windows::Win32::Graphics::Gdi::HFONT {
    unsafe {
        CreateFontW(height, 0, 0, 0, weight, italic, underline, 0, 1, 0, 0, 5, 0, w!("Segoe UI"))
    }
}

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

fn dt(hdc: HDC, text: &str, left: i32, top: i32, right: i32, bottom: i32, flags: DRAW_TEXT_FORMAT) {
    let mut buf = to_wide(text);
    let mut rc = RECT { left, top, right, bottom };
    unsafe { DrawTextW(hdc, &mut buf, &mut rc, flags) };
}

fn sep(hdc: HDC, x1: i32, y: i32, x2: i32) {
    unsafe {
        let pen = CreatePen(PS_SOLID, 1, COLORREF(C_SEP));
        let old = SelectObject(hdc, HGDIOBJ(pen.0));
        let _ = MoveToEx(hdc, x1, y, None);
        let _ = LineTo(hdc, x2, y);
        SelectObject(hdc, old);
        let _ = DeleteObject(HGDIOBJ(pen.0));
    }
}
