// Kein Konsolenfenster im Normalbetrieb. Wird `--debug` übergeben, beschafft sich
// die App zur Laufzeit eine Konsole (siehe `logging::init`).
#![windows_subsystem = "windows"]

//! Workspace Manager für Windows.
//!
//! Hintergrundprozess mit Tray-Icon, der benannte Workspaces verwaltet. Beim
//! Aktivieren eines Workspace werden nur dessen Fenster angezeigt, alle anderen
//! verwalteten Fenster versteckt (`ShowWindow(SW_HIDE)`). Beim Beenden werden
//! alle versteckten Fenster wieder sichtbar gemacht.

mod cli;
mod config;
mod hook;
mod hotkeys;
mod info_dialog;
mod logging;
mod monitors;
mod overlay;
mod quick_input;
mod windows;
mod workspace;

use anyhow::{Context, Result};
use std::collections::HashMap;

use ::windows::core::w;
use ::windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use ::windows::Win32::System::LibraryLoader::GetModuleHandleW;
use ::windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
    GetWindowLongPtrW, PostQuitMessage, RegisterClassW, SetWindowLongPtrW, TranslateMessage,
    CW_USEDEFAULT, GWLP_USERDATA, HHOOK, HMENU, MSG, WINDOW_EX_STYLE, WM_DESTROY, WM_DISPLAYCHANGE,
    WM_HOTKEY, WNDCLASSW, WS_OVERLAPPED,
};

use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

use hotkeys::Action;
use workspace::WorkspaceManager;

/// Aktion eines Menüeintrags im Tray.
#[derive(Debug, Clone)]
enum MenuAction {
    Activate(u32),
    ShowInfo,
    Quit,
}

/// Gesamter Anwendungszustand, erreichbar aus der Fensterprozedur über
/// `GWLP_USERDATA` und aus der Nachrichtenschleife über den Rohzeiger.
struct AppState {
    manager: WorkspaceManager,
    /// Hotkey-ID → Aktion.
    actions: HashMap<i32, Action>,
    /// Vergebene Hotkey-IDs (zum Deregistrieren).
    hotkey_ids: Vec<i32>,
    /// Menü-Eintrag-ID → Aktion.
    menu_actions: HashMap<String, MenuAction>,
    /// Workspace-ID → Menü-Eintrag (zum Setzen des Häkchens am aktiven Workspace).
    ws_check_items: HashMap<u32, CheckMenuItem>,
    /// Tray-Icon (lebt, solange die App läuft).
    tray: TrayIcon,
    /// Eigenes (verstecktes) Fenster für Hotkey- und Display-Nachrichten.
    hwnd: HWND,
    /// Optionaler Low-Level-Keyboard-Hook (Fallback für reservierte Hotkeys).
    keyboard_hook: Option<HHOOK>,
    /// Zuletzt bekannte Monitor-IDs (für Änderungserkennung).
    last_monitor_ids: Vec<String>,
    /// Optionales Desktop-Overlay (nur wenn show_overlay = true in config.toml).
    overlay: Option<overlay::Overlay>,
    /// Optionales randloses Schnelleingabe-Fenster.
    quick_input: Option<quick_input::QuickInput>,
    /// Info-Dialog (WebView2, immer erstellt).
    info_dialog: Option<info_dialog::InfoDialog>,
}

impl AppState {
    /// Aktualisiert Icon, Tooltip, Häkchen und Overlay auf den aktiven Workspace.
    fn refresh_tray(&self) {
        let current = self.manager.current;
        let name = self.manager.name_of(current);

        let tooltip = format!("Workspace Manager – aktiv: {} ({})", current, name);
        let _ = self.tray.set_tooltip(Some(tooltip));

        // Tray-Icon mit aktueller Workspace-Nummer neu zeichnen.
        let _ = self.tray.set_icon(Some(make_numbered_icon(current)));

        // Nur der aktive Workspace erhält ein Häkchen.
        for (id, item) in &self.ws_check_items {
            item.set_checked(*id == current);
        }

        // Overlay aktualisieren, falls aktiv.
        if let Some(ref ov) = self.overlay {
            ov.update(&name);
        }
    }
}

fn main() {
    if let Err(e) = run() {
        report_fatal_error(&e);
        std::process::exit(1);
    }
}

/// Eigentlicher Programmablauf. Fehler werden von `main` per MessageBox angezeigt,
/// da im Normalbetrieb keine Konsole existiert.
fn run() -> Result<()> {
    // 0. Kommandozeile parsen und Logging einrichten (Konsole nur bei --debug,
    //    Logdatei nur bei --log).
    let cli = cli::Cli::parse()?;
    logging::init(cli.debug, cli.log.as_deref())?;

    tracing::info!("Workspace Manager startet");

    // 1.–3. Konfiguration laden und validieren.
    let cfg = config::load_or_create(cli.config.as_deref())?;

    // 4. WorkspaceManager erzeugen.
    let mut manager = WorkspaceManager::new(&cfg);
    let start_ws = manager.current;

    // 5. Aktuelle Fenster dem Start-Workspace zuordnen und Sichtbarkeit setzen.
    manager.assign_all_visible(start_ws);
    manager.apply_visibility();

    // Verstecktes Fenster für Nachrichten erzeugen.
    let hwnd = create_message_window().context("Nachrichtenfenster erzeugen")?;

    // Hotkeys registrieren (6.). Reservierte Kombis landen im Keyboard-Hook.
    let (actions, hotkey_ids) = register_hotkeys(hwnd, &cfg);

    // Keyboard-Hook nur installieren, wenn mindestens ein Hotkey ihn benötigt.
    hook::set_hwnd(hwnd);
    let keyboard_hook = if hook::is_empty() {
        None
    } else {
        match hook::install() {
            Ok(h) => {
                tracing::info!("Low-Level-Keyboard-Hook installiert (für reservierte Hotkeys)");
                Some(h)
            }
            Err(e) => {
                tracing::warn!("Keyboard-Hook konnte nicht installiert werden: {}", e);
                None
            }
        }
    };

    // Tray-Icon mit Menü aufbauen.
    let (tray, menu_actions, ws_check_items) =
        build_tray(&cfg, manager.current).context("Tray-Icon erzeugen")?;

    let last_monitor_ids = monitors::current_ids();

    // Desktop-Overlay erzeugen, wenn in der Konfiguration aktiviert.
    let overlay = if cfg.show_overlay {
        let name = manager.name_of(manager.current);
        match overlay::Overlay::create(&cfg.overlay_corner, &name) {
            Ok(ov) => {
                tracing::info!("Desktop-Overlay aktiviert");
                Some(ov)
            }
            Err(e) => {
                tracing::warn!("Overlay konnte nicht erstellt werden: {}", e);
                None
            }
        }
    } else {
        None
    };

    // Schnelleingabe-Fenster erzeugen, wenn ein Hotkey konfiguriert ist.
    let quick_input = if cfg.quick_input_hotkey.is_some() {
        match quick_input::QuickInput::create(cfg.quick_input_width_pct, cfg.quick_input_height_pct, cfg.quick_input_font_size) {
            Ok(qi) => {
                tracing::info!("Quick-Input-Fenster erstellt");
                Some(qi)
            }
            Err(e) => {
                tracing::warn!("Quick-Input-Fenster konnte nicht erstellt werden: {}", e);
                None
            }
        }
    } else {
        None
    };

    // Info-Dialog erstellen (immer, unabhängig von Config).
    let info_dialog = match info_dialog::InfoDialog::create() {
        Ok(d) => {
            tracing::info!("Info-Dialog erstellt");
            Some(d)
        }
        Err(e) => {
            tracing::warn!("Info-Dialog konnte nicht erstellt werden: {}", e);
            None
        }
    };

    // Zustand boxen und Zeiger im Fenster hinterlegen.
    let state = Box::new(AppState {
        manager,
        actions,
        hotkey_ids,
        menu_actions,
        ws_check_items,
        tray,
        hwnd,
        keyboard_hook,
        last_monitor_ids,
        overlay,
        quick_input,
        info_dialog,
    });
    let state_ptr = Box::into_raw(state);
    unsafe {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, state_ptr as isize);
        (*state_ptr).refresh_tray();
    }

    tracing::info!("Bereit. Hotkeys aktiv, Tray-Icon angelegt.");

    // 7. Nachrichtenschleife.
    run_message_loop(state_ptr);

    // Aufräumen: Zustand zurücknehmen, Fenster zerstören.
    let mut state = unsafe { Box::from_raw(state_ptr) };
    cleanup(&mut state);
    unsafe {
        let _ = DestroyWindow(hwnd);
    }

    tracing::info!("Workspace Manager beendet");
    Ok(())
}

/// Zeigt einen fatalen Startfehler in einer MessageBox an (es gibt im
/// Normalbetrieb keine Konsole) und protokolliert ihn zusätzlich, falls bereits
/// ein Logger aktiv ist.
fn report_fatal_error(err: &anyhow::Error) {
    use ::windows::core::PCWSTR;
    use ::windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};

    tracing::error!("Fataler Fehler: {:#}", err);

    let text: Vec<u16> = format!("{err:#}")
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let caption: Vec<u16> = "Workspace Manager"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        MessageBoxW(
            HWND::default(),
            PCWSTR(text.as_ptr()),
            PCWSTR(caption.as_ptr()),
            MB_OK | MB_ICONERROR,
        );
    }
}

/// Registriert die in der Konfiguration definierten Hotkeys.
fn register_hotkeys(hwnd: HWND, cfg: &config::Config) -> (HashMap<i32, Action>, Vec<i32>) {
    let mut actions: HashMap<i32, Action> = HashMap::new();
    let mut ids: Vec<i32> = Vec::new();
    let mut next_id: i32 = 1;

    for ws in &cfg.workspaces {
        if let Some(spec) = &ws.activate_hotkey {
            register_one(hwnd, spec, Action::Activate(ws.id), &mut next_id, &mut actions, &mut ids);
        }
        if let Some(spec) = &ws.move_window_hotkey {
            register_one(
                hwnd,
                spec,
                Action::MoveWindow(ws.id),
                &mut next_id,
                &mut actions,
                &mut ids,
            );
        }
    }
    for summon in &cfg.summons {
        register_one(
            hwnd,
            &summon.hotkey,
            Action::Summon {
                title: summon.title.clone(),
                launch: summon.launch.clone(),
                launch_dir: summon.launch_dir.clone(),
            },
            &mut next_id,
            &mut actions,
            &mut ids,
        );
    }
    if let Some(spec) = &cfg.quick_input_hotkey {
        register_one(hwnd, spec, Action::ToggleQuickInput, &mut next_id, &mut actions, &mut ids);
    }
    (actions, ids)
}

fn register_one(
    hwnd: HWND,
    spec: &str,
    action: Action,
    next_id: &mut i32,
    actions: &mut HashMap<i32, Action>,
    ids: &mut Vec<i32>,
) {
    let parsed = match hotkeys::parse(spec) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!("Hotkey '{}' nicht verstanden: {}", spec, e);
            return;
        }
    };

    let id = *next_id;
    *next_id += 1;

    match hotkeys::register(hwnd, id, &parsed) {
        Ok(()) => {
            actions.insert(id, action);
            ids.push(id);
            tracing::info!("Hotkey '{}' via RegisterHotKey registriert (id {})", spec, id);
        }
        Err(e) => {
            // Reserviert (z. B. Win+N) -> Fallback auf den Low-Level-Keyboard-Hook.
            hook::add_entry(parsed.mod_bits(), parsed.vk, id);
            actions.insert(id, action);
            tracing::info!(
                "Hotkey '{}' von RegisterHotKey abgelehnt ({}); Fallback auf Keyboard-Hook (id {})",
                spec,
                e,
                id
            );
        }
    }
}

/// Baut das Tray-Icon mit Menü auf und liefert die Zuordnung Menü-ID → Aktion
/// sowie die Häkchen-Einträge je Workspace-ID. `current` erhält das Häkchen.
fn build_tray(
    cfg: &config::Config,
    current: u32,
) -> Result<(TrayIcon, HashMap<String, MenuAction>, HashMap<u32, CheckMenuItem>)> {
    let menu = Menu::new();
    let mut menu_actions: HashMap<String, MenuAction> = HashMap::new();
    let mut ws_check_items: HashMap<u32, CheckMenuItem> = HashMap::new();

    let version_item = MenuItem::new(
        format!("Waystone Ridge v{}", env!("CARGO_PKG_VERSION")),
        true,
        None,
    );
    menu_actions.insert(version_item.id().0.clone(), MenuAction::ShowInfo);
    menu.append(&version_item)?;
    menu.append(&PredefinedMenuItem::separator())?;

    let header = MenuItem::new("Workspace aktivieren:", false, None);
    menu.append(&header)?;

    for ws in &cfg.workspaces {
        let item = CheckMenuItem::new(
            format!("  {} – {}", ws.id, ws.name),
            true,
            ws.id == current,
            None,
        );
        menu_actions.insert(item.id().0.clone(), MenuAction::Activate(ws.id));
        menu.append(&item)?;
        ws_check_items.insert(ws.id, item);
    }

    menu.append(&PredefinedMenuItem::separator())?;
    let quit = MenuItem::new("Beenden", true, None);
    menu_actions.insert(quit.id().0.clone(), MenuAction::Quit);
    menu.append(&quit)?;

    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("Workspace Manager")
        .with_icon(make_numbered_icon(current))
        .build()
        .context("TrayIconBuilder::build")?;

    Ok((tray, menu_actions, ws_check_items))
}

/// 3×5 Pixelfont für die Ziffern 0–9. Bit 2 = linke Spalte, Bit 0 = rechte Spalte.
static DIGITS: [[u8; 5]; 10] = [
    [0b111, 0b101, 0b101, 0b101, 0b111], // 0
    [0b110, 0b010, 0b010, 0b010, 0b111], // 1
    [0b111, 0b001, 0b111, 0b100, 0b111], // 2
    [0b111, 0b001, 0b111, 0b001, 0b111], // 3
    [0b101, 0b101, 0b111, 0b001, 0b001], // 4
    [0b111, 0b100, 0b111, 0b001, 0b111], // 5
    [0b100, 0b100, 0b111, 0b101, 0b111], // 6
    [0b111, 0b001, 0b001, 0b001, 0b001], // 7
    [0b111, 0b101, 0b111, 0b101, 0b111], // 8
    [0b111, 0b101, 0b111, 0b001, 0b111], // 9
];

/// Erzeugt ein 32×32 Tray-Icon mit der Workspace-Nummer als Pixel-Text.
fn make_numbered_icon(ws_id: u32) -> Icon {
    const SIZE: u32 = 32;
    let text = format!("{}", ws_id);
    let n = text.len() as u32;

    let scale: u32 = if n == 1 { 5 } else if n == 2 { 4 } else { 2 };
    let digit_w = 3 * scale;
    let digit_h = 5 * scale;
    let gap = scale;
    let total_w = digit_w * n + gap * n.saturating_sub(1);
    let start_x = (SIZE - total_w) / 2;
    let start_y = (SIZE - digit_h) / 2;

    let mut rgba = vec![0u8; (SIZE * SIZE * 4) as usize];

    // Hintergrund: blau.
    for chunk in rgba.chunks_mut(4) {
        chunk[0] = 0x2D;
        chunk[1] = 0x7D;
        chunk[2] = 0xD2;
        chunk[3] = 0xFF;
    }

    // Rand: dunkelgrau.
    for y in 0..SIZE {
        for x in 0..SIZE {
            if x < 2 || y < 2 || x >= SIZE - 2 || y >= SIZE - 2 {
                let i = ((y * SIZE + x) * 4) as usize;
                rgba[i] = 0x20;
                rgba[i + 1] = 0x20;
                rgba[i + 2] = 0x20;
                rgba[i + 3] = 0xFF;
            }
        }
    }

    // Ziffern rendern.
    for (ci, ch) in text.chars().enumerate() {
        let d = (ch as u8 - b'0') as usize;
        let cx = start_x + ci as u32 * (digit_w + gap);
        for (row, &bits) in DIGITS[d].iter().enumerate() {
            for col in 0..3u32 {
                if bits & (1 << (2 - col)) != 0 {
                    for sy in 0..scale {
                        for sx in 0..scale {
                            let px = cx + col * scale + sx;
                            let py = start_y + row as u32 * scale + sy;
                            if px < SIZE && py < SIZE {
                                let i = ((py * SIZE + px) * 4) as usize;
                                rgba[i] = 0xFF;
                                rgba[i + 1] = 0xFF;
                                rgba[i + 2] = 0xFF;
                                rgba[i + 3] = 0xFF;
                            }
                        }
                    }
                }
            }
        }
    }

    Icon::from_rgba(rgba, SIZE, SIZE).expect("gültiges Icon")
}

/// Erzeugt ein verstecktes Top-Level-Fenster, das `WM_HOTKEY` und
/// `WM_DISPLAYCHANGE` empfängt. (Message-only-Fenster erhalten kein
/// `WM_DISPLAYCHANGE`, daher ein normales, nie angezeigtes Fenster.)
fn create_message_window() -> Result<HWND> {
    unsafe {
        let hinstance = HINSTANCE(GetModuleHandleW(None)?.0);
        let class_name = w!("WorkspaceManagerMsgWindow");

        let wc = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: hinstance,
            lpszClassName: class_name,
            ..Default::default()
        };
        let atom = RegisterClassW(&wc);
        if atom == 0 {
            anyhow::bail!("RegisterClassW fehlgeschlagen");
        }

        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            class_name,
            w!("Workspace Manager"),
            WS_OVERLAPPED,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            0,
            0,
            HWND::default(),
            HMENU::default(),
            hinstance,
            None,
        )
        .context("CreateWindowExW")?;

        Ok(hwnd)
    }
}

/// Standard-Nachrichtenschleife. Verarbeitet Fensternachrichten und pollt
/// danach die Menü-Ereignisse des Tray-Icons.
fn run_message_loop(state_ptr: *mut AppState) {
    let mut msg = MSG::default();
    loop {
        let ret = unsafe { GetMessageW(&mut msg, None, 0, 0) };
        // 0 = WM_QUIT, -1 = Fehler.
        if ret.0 <= 0 {
            break;
        }
        unsafe {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        // Menü-Ereignisse des Tray-Icons abholen.
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            let state = unsafe { &mut *state_ptr };
            match state.menu_actions.get(&event.id.0).cloned() {
                Some(MenuAction::Activate(ws)) => {
                    state.manager.activate(ws);
                    state.refresh_tray();
                }
                Some(MenuAction::ShowInfo) => {
                    if let Some(ref d) = state.info_dialog {
                        d.show();
                    }
                }
                Some(MenuAction::Quit) => {
                    tracing::info!("Beenden über Tray-Menü angefordert");
                    return;
                }
                None => {}
            }
        }
    }
}

/// Führt die zu einer Hotkey-ID gehörende Aktion aus und aktualisiert den Tooltip.
fn dispatch_hotkey(state: &mut AppState, id: i32) {
    if let Some(action) = state.actions.get(&id).cloned() {
        match action {
            Action::Activate(ws) => {
                state.manager.activate(ws);
                state.refresh_tray();
            }
            Action::MoveWindow(ws) => {
                state.manager.move_foreground(ws);
                state.refresh_tray();
            }
            Action::Summon { title, launch, launch_dir } => {
                if let Some(hwnd) = windows::find_by_title_substr(&title) {
                    let key = windows::hwnd_key(hwnd);
                    let on_current = state.manager.window_ws.get(&key) == Some(&state.manager.current);
                    let is_foreground = hwnd == windows::foreground_window();
                    if on_current && is_foreground {
                        windows::minimize(hwnd);
                        tracing::debug!("Summon: Fenster '{}' minimiert (war aktiv auf aktuellem Workspace)", title);
                    } else {
                        state.manager.pull_to_current(hwnd);
                    }
                } else if let Some(cmd) = launch {
                    tracing::info!("Summon: kein Fenster '{}' gefunden, starte '{}'", title, cmd);
                    use std::os::windows::process::CommandExt;
                    let mut command = std::process::Command::new("cmd");
                    command.raw_arg(format!("/C {}", cmd));
                    if let Some(dir) = launch_dir {
                        command.current_dir(&dir);
                    }
                    if let Err(e) = command.spawn() {
                        tracing::warn!("Summon: Starten von '{}' fehlgeschlagen: {}", cmd, e);
                    }
                } else {
                    tracing::debug!("Summon: kein Fenster mit Titel-Substring '{}' gefunden", title);
                }
            }
            Action::ToggleQuickInput => {
                if let Some(ref qi) = state.quick_input {
                    qi.toggle();
                }
            }
        }
    }
}

/// Fensterprozedur: behandelt Hotkeys und Monitoränderungen.
unsafe extern "system" fn wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        // Reguläre RegisterHotKey-Hotkeys und Hook-Fallback teilen sich den Dispatch.
        WM_HOTKEY | hook::WM_APP_HOOK_HOTKEY => {
            let state_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut AppState;
            if !state_ptr.is_null() {
                dispatch_hotkey(&mut *state_ptr, wparam.0 as i32);
            }
            LRESULT(0)
        }
        WM_DISPLAYCHANGE => {
            let state_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut AppState;
            if !state_ptr.is_null() {
                handle_display_change(&mut *state_ptr);
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

/// Reagiert auf Monitoränderungen: neu enumerieren, Differenz protokollieren,
/// Zuordnungen bleiben über stabile IDs erhalten, Sichtbarkeit neu anwenden.
fn handle_display_change(state: &mut AppState) {
    let new_ids = monitors::current_ids();
    if new_ids == state.last_monitor_ids {
        return;
    }

    let added: Vec<&String> = new_ids
        .iter()
        .filter(|id| !state.last_monitor_ids.contains(id))
        .collect();
    let removed: Vec<&String> = state
        .last_monitor_ids
        .iter()
        .filter(|id| !new_ids.contains(id))
        .collect();

    if !added.is_empty() {
        tracing::info!("Monitor(e) hinzugekommen: {:?}", added);
    }
    if !removed.is_empty() {
        tracing::info!("Monitor(e) entfernt: {:?}", removed);
    }

    state.manager.refresh_monitors();
    state.last_monitor_ids = new_ids;

    // Fensterzuordnungen bleiben bestehen; aktiven Workspace sicher anzeigen.
    state.manager.apply_visibility();
}

/// Beendet sauber: Hotkeys deregistrieren und alle Fenster wieder sichtbar machen.
fn cleanup(state: &mut AppState) {
    if let Some(hook) = state.keyboard_hook.take() {
        hook::uninstall(hook);
    }
    for id in &state.hotkey_ids {
        hotkeys::unregister(state.hwnd, *id);
    }
    state.manager.show_all();
}
