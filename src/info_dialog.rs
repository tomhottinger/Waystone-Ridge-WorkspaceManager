//! Info-Dialog: zeigt Versionsinformationen und Lizenzhinweise.
//!
//! Verwendet `TaskDialogIndirect` (Windows Common Controls) für ein
//! natives, DPI-korrektes Erscheinungsbild mit klickbaren Links.

use std::mem::size_of;
use std::sync::atomic::{AtomicBool, Ordering};

use windows::core::{HRESULT, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::Controls::{
    TaskDialogIndirect, TASKDIALOGCONFIG, TASKDIALOGCONFIG_0, TASKDIALOG_NOTIFICATIONS,
    TDCBF_CLOSE_BUTTON, TDF_ALLOW_DIALOG_CANCELLATION, TDF_ENABLE_HYPERLINKS,
    TDF_SIZE_TO_CONTENT, TDN_HYPERLINK_CLICKED,
};

const GITHUB_URL: &str = "https://github.com/tomhottinger/Waystone-Ridge-WorkspaceManager";

// TD_INFORMATION_ICON = MAKEINTRESOURCEW(-3)
const TD_INFORMATION_ICON: PCWSTR = PCWSTR(0xFFFD as usize as *const u16);

static DIALOG_OPEN: AtomicBool = AtomicBool::new(false);

/// Öffnet den Info-Dialog. Ist er bereits geöffnet, passiert nichts.
pub fn show() {
    if DIALOG_OPEN.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(|| {
        unsafe { run_dialog() };
        DIALOG_OPEN.store(false, Ordering::SeqCst);
    });
}

unsafe fn run_dialog() {
    let title = wide("Waystone Ridge");
    let heading = wide("Waystone Ridge");

    let content_str = format!(
        "Version {ver}  ·  Workspace Manager für Windows\n\
         Copyright © 2026 Thomas C. Hottinger\n\
         \n\
         \n\
         WAYSTONE TOOLS\n\
         \n\
         Die Waystone Tools sind eine Gruppe von kleinen Executables, die den Alltag \
         eines fortgeschrittenen Benutzers erleichtern sollen.\n\
         \n\
         \n\
         WAYSTONE RIDGE\n\
         \n\
         Im Moment gibt es nur Waystone-Ridge, ein Workspace Manager.\n\
         <a href=\"gh\">Projektseite auf GitHub</a>\n\
         \n\
         \n\
         LIZENZ UND DISCLAIMER\n\
         \n\
         MIT Lizenz mit Haftungsausschluss.\n\
         <a href=\"gh\">Lizenztext auf GitHub</a>",
        ver = env!("CARGO_PKG_VERSION"),
    );
    let content = wide(&content_str);

    let config = TASKDIALOGCONFIG {
        cbSize: size_of::<TASKDIALOGCONFIG>() as u32,
        dwFlags: TDF_ENABLE_HYPERLINKS | TDF_ALLOW_DIALOG_CANCELLATION | TDF_SIZE_TO_CONTENT,
        dwCommonButtons: TDCBF_CLOSE_BUTTON,
        pszWindowTitle: PCWSTR(title.as_ptr()),
        Anonymous1: TASKDIALOGCONFIG_0 {
            pszMainIcon: TD_INFORMATION_ICON,
        },
        pszMainInstruction: PCWSTR(heading.as_ptr()),
        pszContent: PCWSTR(content.as_ptr()),
        pfCallback: Some(callback),
        cxWidth: 200, // in Dialog-Einheiten; 0 = automatisch
        ..Default::default()
    };

    let _ = TaskDialogIndirect(&config, None, None, None);
}

unsafe extern "system" fn callback(
    _hwnd: HWND,
    msg: TASKDIALOG_NOTIFICATIONS,
    _wparam: WPARAM,
    _lparam: LPARAM,
    _refdata: isize,
) -> HRESULT {
    if msg == TDN_HYPERLINK_CLICKED {
        open_url();
    }
    HRESULT(0) // S_OK
}

fn open_url() {
    use std::os::windows::process::CommandExt;
    let _ = std::process::Command::new("cmd")
        .raw_arg(format!("/C start {}", GITHUB_URL))
        .spawn();
}

fn wide(s: &str) -> Vec<u16> {
    let mut v: Vec<u16> = s.encode_utf16().collect();
    v.push(0);
    v
}
