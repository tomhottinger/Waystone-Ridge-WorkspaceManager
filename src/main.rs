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
mod respite;
mod windows;
mod workspace;

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::PathBuf;

use ::windows::core::w;
use ::windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use ::windows::Win32::System::LibraryLoader::GetModuleHandleW;
use ::windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
    GetWindowLongPtrW, KillTimer, PostMessageW, PostQuitMessage, RegisterClassW, SetTimer,
    SetWindowLongPtrW, TranslateMessage, CW_USEDEFAULT, GWLP_USERDATA, HHOOK, HMENU, MSG,
    WINDOW_EX_STYLE, WM_DESTROY, WM_DISPLAYCHANGE, WM_HOTKEY, WM_TIMER, WNDCLASSW, WS_OVERLAPPED,
};

use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

use hotkeys::Action;
use workspace::WorkspaceManager;

/// Timer-ID für die periodische Respite-Prüfung.
const TIMER_RESPITE: usize = 1;

/// Aktion eines Menüeintrags im Tray.
#[derive(Debug, Clone)]
enum MenuAction {
    Activate(u32),
    ShowInfo,
    OpenConfigFile,
    ReloadConfig,
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
    /// Low-Level-Keyboard-Hook für Hotkey-Fallback (reservierte Tasten).
    keyboard_hook: Option<HHOOK>,
    /// Keyboard-Hook, der ausschließlich für die Respite-Blockierung installiert wurde.
    respite_keyboard_hook: Option<HHOOK>,
    /// Maus-Hook für die Respite-Blockierung.
    mouse_hook: Option<HHOOK>,
    /// Zuletzt bekannte Monitor-IDs (für Änderungserkennung).
    last_monitor_ids: Vec<String>,
    /// Optionales Desktop-Overlay (show_overlay = true in config.toml).
    overlay: Option<overlay::Overlay>,
    /// Temporäres Overlay während einer aktiven Respite (nur wenn kein reguläres Overlay).
    respite_overlay: Option<overlay::Overlay>,
    /// Optionales randloses Schnelleingabe-Fenster.
    quick_input: Option<quick_input::QuickInput>,
    /// Info-Dialog (WebView2, immer erstellt).
    info_dialog: Option<info_dialog::InfoDialog>,
    /// Geparste Respite-Zeitpläne.
    respite_schedules: Vec<respite::RespiteSchedule>,
    /// Ob gerade ein Respite-Zeitfenster aktiv ist.
    respite_active: bool,
    /// Gesetzt, wenn der Nutzer Respite im aktuellen Slot per Breakout beendet hat.
    respite_escaped_this_slot: bool,
    /// Globale Breakout-Konfiguration (Defaults, können pro Slot überschrieben werden).
    breakout: config::BreakoutConfig,
    /// Effektive Mindestwartezeit des aktuell laufenden Slots.
    respite_current_min_wait_secs: u32,
    /// Effektive Sequenzlänge des aktuell laufenden Slots.
    respite_current_escape_len: usize,
    /// Zufällig generierte Zeichensequenz für den aktuellen Respite-Slot.
    respite_escape_sequence: String,
    /// Bisher korrekt eingetippte Zeichen der Escape-Sequenz.
    respite_typed: String,
    /// Zeitpunkt des Respite-Starts (für Mindestwartezeit).
    respite_start_instant: Option<std::time::Instant>,
    /// Ob die Mindestwartezeit abgelaufen und der Breakout entsperrt ist.
    respite_escape_unlocked: bool,
    /// Pfad zur geladenen Konfigurationsdatei.
    config_file_path: PathBuf,
}

impl AppState {
    /// Aktualisiert Icon, Tooltip, Häkchen und Overlay auf den aktiven Workspace.
    fn refresh_tray(&self) {
        let current = self.manager.current;
        let name = self.manager.name_of(current);

        let tooltip = format!("Workspace Manager – aktiv: {} ({})", current, name);
        let _ = self.tray.set_tooltip(Some(tooltip));

        let _ = self.tray.set_icon(Some(make_numbered_icon(current)));

        for (id, item) in &self.ws_check_items {
            item.set_checked(*id == current);
        }

        if !self.respite_active {
            if let Some(ref ov) = self.overlay {
                ov.update(&name);
            }
        }
    }
}

fn main() {
    if let Err(e) = run() {
        report_fatal_error(&e);
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = cli::Cli::parse()?;
    logging::init(cli.debug, cli.log.as_deref())?;

    tracing::info!("Workspace Manager startet");

    let config_file_path = match cli.config.as_deref() {
        Some(p) => p.to_path_buf(),
        None => config::config_path()?,
    };

    let cfg = config::load_or_create(Some(&config_file_path))?;

    let mut manager = WorkspaceManager::new(&cfg);
    let start_ws = manager.current;
    manager.assign_all_visible(start_ws);
    manager.apply_visibility();

    let hwnd = create_message_window().context("Nachrichtenfenster erzeugen")?;

    let (actions, hotkey_ids) = register_hotkeys(hwnd, &cfg);

    hook::set_hwnd(hwnd);
    let keyboard_hook = if hook::is_empty() {
        None
    } else {
        match hook::install() {
            Ok(h) => {
                tracing::info!("Low-Level-Keyboard-Hook installiert");
                Some(h)
            }
            Err(e) => {
                tracing::warn!("Keyboard-Hook konnte nicht installiert werden: {}", e);
                None
            }
        }
    };

    let (tray, menu_actions, ws_check_items) =
        build_tray(&cfg, manager.current).context("Tray-Icon erzeugen")?;

    let last_monitor_ids = monitors::current_ids();

    let overlay = if cfg.show_overlay {
        let name = manager.name_of(manager.current);
        match overlay::Overlay::create_subtle(&cfg.overlay_corner, &name) {
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

    let quick_input = if cfg.quick_input_hotkey.is_some() {
        match quick_input::QuickInput::create(
            cfg.quick_input_width_pct,
            cfg.quick_input_height_pct,
            cfg.quick_input_font_size,
        ) {
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

    let respite_schedules = respite::parse(&cfg.respite);

    let state = Box::new(AppState {
        manager,
        actions,
        hotkey_ids,
        menu_actions,
        ws_check_items,
        tray,
        hwnd,
        keyboard_hook,
        respite_keyboard_hook: None,
        mouse_hook: None,
        last_monitor_ids,
        overlay,
        respite_overlay: None,
        quick_input,
        info_dialog,
        respite_schedules,
        respite_active: false,
        respite_escaped_this_slot: false,
        breakout: cfg.respite_breakout.clone(),
        respite_current_min_wait_secs: 0,
        respite_current_escape_len: 0,
        respite_escape_sequence: String::new(),
        respite_typed: String::new(),
        respite_start_instant: None,
        respite_escape_unlocked: false,
        config_file_path,
    });
    let state_ptr = Box::into_raw(state);
    unsafe {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, state_ptr as isize);
        (*state_ptr).refresh_tray();
        // Respite sekündlich prüfen (Countdown im Overlay).
        SetTimer(hwnd, TIMER_RESPITE, 1_000, None);
    }

    tracing::info!("Bereit. Hotkeys aktiv, Tray-Icon angelegt.");

    run_message_loop(state_ptr);

    let mut state = unsafe { Box::from_raw(state_ptr) };
    cleanup(&mut state);
    unsafe {
        let _ = DestroyWindow(hwnd);
    }

    tracing::info!("Workspace Manager beendet");
    Ok(())
}

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

fn show_warning(caption: &str, text: &str) {
    use ::windows::core::PCWSTR;
    use ::windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONWARNING, MB_OK};

    let text_w: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let cap_w: Vec<u16> = caption.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        MessageBoxW(
            HWND::default(),
            PCWSTR(text_w.as_ptr()),
            PCWSTR(cap_w.as_ptr()),
            MB_OK | MB_ICONWARNING,
        );
    }
}

/// Öffnet die Konfigurationsdatei im Standard-Texteditor des Betriebssystems.
fn open_config_file(path: &std::path::Path) {
    use std::os::windows::process::CommandExt;
    let path_str = path.to_string_lossy();
    let mut command = std::process::Command::new("cmd");
    command.raw_arg(format!("/C start \"\" \"{}\"", path_str));
    if let Err(e) = command.spawn() {
        tracing::warn!("Konfigurationsfile öffnen fehlgeschlagen: {}", e);
    }
}

/// Lädt die Konfiguration neu und baut den gesamten App-State neu auf.
/// Fenster-Workspace-Zuordnungen gehen dabei verloren (alle Fenster landen
/// wieder auf dem ersten Workspace).
fn reload_config(state: &mut AppState) {
    tracing::info!("Konfiguration wird neu geladen: {}", state.config_file_path.display());

    let cfg = match config::load_or_create(Some(&state.config_file_path)) {
        Ok(c) => c,
        Err(e) => {
            let msg = format!("Konfiguration konnte nicht geladen werden:\n\n{e:#}");
            tracing::warn!("{}", msg);
            show_warning("Workspace Manager – Fehler", &msg);
            return;
        }
    };

    // Alle Fenster wieder sichtbar machen, bevor der Zustand ersetzt wird.
    state.manager.show_all();

    // Respite vollständig deaktivieren.
    if state.respite_active {
        deactivate_respite(state);
    }
    state.respite_escaped_this_slot = false;
    state.respite_escape_sequence = String::new();

    // Keyboard-Hook und Hotkeys deregistrieren.
    if let Some(h) = state.keyboard_hook.take() {
        hook::uninstall(h);
    }
    hook::clear_entries();
    for id in &state.hotkey_ids {
        hotkeys::unregister(state.hwnd, *id);
    }

    // Neuen WorkspaceManager erzeugen.
    let mut manager = WorkspaceManager::new(&cfg);
    let start_ws = manager.current;
    manager.assign_all_visible(start_ws);
    manager.apply_visibility();
    state.manager = manager;

    // Hotkeys neu registrieren.
    let (actions, hotkey_ids) = register_hotkeys(state.hwnd, &cfg);
    state.actions = actions;
    state.hotkey_ids = hotkey_ids;

    // Keyboard-Hook ggf. neu installieren.
    hook::set_hwnd(state.hwnd);
    state.keyboard_hook = if hook::is_empty() {
        None
    } else {
        match hook::install() {
            Ok(h) => Some(h),
            Err(e) => {
                tracing::warn!("Keyboard-Hook konnte nicht installiert werden: {}", e);
                None
            }
        }
    };

    // Tray-Menü neu aufbauen.
    match build_tray(&cfg, state.manager.current) {
        Ok((tray, menu_actions, ws_check_items)) => {
            state.tray = tray;
            state.menu_actions = menu_actions;
            state.ws_check_items = ws_check_items;
        }
        Err(e) => tracing::warn!("Tray-Menü konnte nicht neu aufgebaut werden: {}", e),
    }

    // Overlay aktualisieren.
    state.overlay = if cfg.show_overlay {
        let name = state.manager.name_of(state.manager.current);
        overlay::Overlay::create_subtle(&cfg.overlay_corner, &name).ok()
    } else {
        None
    };

    // Respite-Konfiguration neu laden.
    state.respite_schedules = respite::parse(&cfg.respite);
    state.breakout = cfg.respite_breakout.clone();

    state.refresh_tray();
    tracing::info!("Konfiguration erfolgreich neu geladen");
}

/// Erzeugt eine zufällige Sequenz aus Kleinbuchstaben und Ziffern.
fn generate_escape_sequence(len: usize) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64)
        .unwrap_or(31337);
    let chars: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    let mut state = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    (0..len)
        .map(|_| {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            chars[(state >> 33) as usize % chars.len()] as char
        })
        .collect()
}

/// Beendet die aktive Respite-Sperre und räumt alle zugehörigen Ressourcen auf.
fn deactivate_respite(state: &mut AppState) {
    state.respite_active = false;
    state.respite_escape_unlocked = false;
    state.respite_start_instant = None;
    state.respite_typed = String::new();
    hook::set_respite_active(false);
    hook::set_respite_escape_unlocked(false);
    if let Some(h) = state.respite_keyboard_hook.take() {
        hook::uninstall(h);
    }
    if let Some(h) = state.mouse_hook.take() {
        hook::uninstall_mouse(h);
    }
    state.respite_overlay = None;
    if let Some(ref ov) = state.overlay {
        ov.update(&state.manager.name_of(state.manager.current));
    }
}

/// Aktualisiert den Overlay-Text und entsperrt ggf. den Notausgang.
fn update_respite_overlay(state: &mut AppState) {
    let slot = match respite::active_slot(&state.respite_schedules) {
        Some(s) => s,
        None => return,
    };
    let secs_remaining = respite::remaining_secs(slot);
    let countdown = respite::format_remaining(secs_remaining);
    let elapsed = state.respite_start_instant
        .map(|i| i.elapsed().as_secs())
        .unwrap_or(0);

    let text = if elapsed < state.respite_current_min_wait_secs as u64 {
        let wait_left = state.respite_current_min_wait_secs as u64 - elapsed;
        format!(
            "{}\nnoch {}\n\nBreakout in {} verfügbar",
            slot.label,
            countdown,
            respite::format_remaining(wait_left as u32)
        )
    } else {
        // Mindestwartezeit abgelaufen – Breakout entsperren.
        if !state.respite_escape_unlocked {
            state.respite_escape_unlocked = true;
            hook::set_respite_escape_unlocked(true);
        }
        let seq = &state.respite_escape_sequence;
        let typed = &state.respite_typed;
        if typed.is_empty() {
            format!("{}\nnoch {}\n\nTippe zum Beenden:\n{}", slot.label, countdown, seq)
        } else {
            let remaining_underscores = "_".repeat(seq.len() - typed.len());
            format!(
                "{}\nnoch {}\n\nTippe zum Beenden:\n{}\n{}{}",
                slot.label, countdown, seq, typed, remaining_underscores
            )
        }
    };

    if let Some(ref ov) = state.respite_overlay {
        ov.update(&text);
    }
}

/// Prüft, ob ein Respite-Zeitfenster beginnt oder endet, und reagiert entsprechend.
fn check_respite(state: &mut AppState) {
    let slot = respite::active_slot(&state.respite_schedules);
    let slot_active = slot.is_some();

    // Wenn das Zeitfenster vorbei ist, Escape-Flag zurücksetzen.
    if !slot_active {
        state.respite_escaped_this_slot = false;
    }

    let should_be_active = slot_active && !state.respite_escaped_this_slot;

    if should_be_active == state.respite_active {
        return;
    }

    state.respite_active = should_be_active;
    hook::set_respite_active(should_be_active);

    if should_be_active {
        let slot = slot.unwrap();
        tracing::info!("Respite beginnt: {} (bis {})", slot.label, respite::format_end(slot));

        // Effektive Breakout-Werte: Slot-Override hat Vorrang vor globalem Default.
        state.respite_current_min_wait_secs = slot
            .min_wait_secs
            .unwrap_or(state.breakout.min_wait_secs);
        state.respite_current_escape_len = slot
            .escape_len
            .unwrap_or(state.breakout.escape_len)
            .max(1);

        // Escape-Sequenz für diesen Slot frisch erzeugen.
        state.respite_escape_sequence = generate_escape_sequence(state.respite_current_escape_len);
        state.respite_typed = String::new();
        state.respite_start_instant = Some(std::time::Instant::now());
        state.respite_escape_unlocked = false;
        hook::set_respite_escape_unlocked(false);

        // Keyboard-Hook installieren, falls noch keiner läuft.
        if state.keyboard_hook.is_none() && state.respite_keyboard_hook.is_none() {
            match hook::install() {
                Ok(h) => state.respite_keyboard_hook = Some(h),
                Err(e) => tracing::warn!("Respite-Keyboard-Hook nicht installierbar: {}", e),
            }
        }

        // Maus-Hook installieren.
        match hook::install_mouse() {
            Ok(h) => state.mouse_hook = Some(h),
            Err(e) => tracing::warn!("Respite-Maus-Hook nicht installierbar: {}", e),
        }

        // Prominentes Overlay mit initialem Text anlegen.
        let initial = format!(
            "{}\nnoch {}\n\nBreakout in {} verfügbar",
            slot.label,
            respite::format_remaining(respite::remaining_secs(slot)),
            respite::format_remaining(state.respite_current_min_wait_secs)
        );
        match overlay::Overlay::create_prominent(&initial) {
            Ok(ov) => state.respite_overlay = Some(ov),
            Err(e) => tracing::warn!("Respite-Overlay konnte nicht erstellt werden: {}", e),
        }
    } else {
        tracing::info!("Respite endet");
        state.respite_start_instant = None;
        state.respite_typed = String::new();
        state.respite_escape_unlocked = false;
        hook::set_respite_escape_unlocked(false);

        if let Some(h) = state.respite_keyboard_hook.take() {
            hook::uninstall(h);
        }
        if let Some(h) = state.mouse_hook.take() {
            hook::uninstall_mouse(h);
        }
        state.respite_overlay = None;
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

/// Baut das Tray-Icon mit Menü auf. Gibt Tray-Icon, Menü-Aktions-Map und
/// Workspace-Häkchen-Map zurück.
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

    // Konfigurationsmenü als Submenu.
    let config_sub = Submenu::new("Konfiguration", true);
    let open_cfg = MenuItem::new("Konfigurationsfile öffnen", true, None);
    let reload_cfg = MenuItem::new("neu einlesen", true, None);
    menu_actions.insert(open_cfg.id().0.clone(), MenuAction::OpenConfigFile);
    menu_actions.insert(reload_cfg.id().0.clone(), MenuAction::ReloadConfig);
    config_sub.append(&open_cfg)?;
    config_sub.append(&reload_cfg)?;
    menu.append(&config_sub)?;

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

/// 3×5 Pixelfont für die Ziffern 0–9.
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

    for chunk in rgba.chunks_mut(4) {
        chunk[0] = 0x2D;
        chunk[1] = 0x7D;
        chunk[2] = 0xD2;
        chunk[3] = 0xFF;
    }

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

fn run_message_loop(state_ptr: *mut AppState) {
    let mut msg = MSG::default();
    loop {
        let ret = unsafe { GetMessageW(&mut msg, None, 0, 0) };
        if ret.0 <= 0 {
            break;
        }
        unsafe {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        // Tray-Menü-Ereignisse abholen.
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
                Some(MenuAction::OpenConfigFile) => {
                    open_config_file(&state.config_file_path.clone());
                }
                Some(MenuAction::ReloadConfig) => {
                    reload_config(state);
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
                    let on_current =
                        state.manager.window_ws.get(&key) == Some(&state.manager.current);
                    let is_foreground = hwnd == windows::foreground_window();
                    if on_current && is_foreground {
                        windows::minimize(hwnd);
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

unsafe extern "system" fn wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_HOTKEY | hook::WM_APP_HOOK_HOTKEY => {
            let state_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut AppState;
            if !state_ptr.is_null() {
                dispatch_hotkey(&mut *state_ptr, wparam.0 as i32);
            }
            LRESULT(0)
        }
        WM_TIMER => {
            if wparam.0 == TIMER_RESPITE {
                let state_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut AppState;
                if !state_ptr.is_null() {
                    let state = &mut *state_ptr;
                    check_respite(state);
                    if state.respite_active {
                        update_respite_overlay(state);
                    }
                }
            }
            LRESULT(0)
        }
        hook::WM_APP_RESPITE_ESCAPE => {
            let state_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut AppState;
            if !state_ptr.is_null() {
                let state = &mut *state_ptr;
                if state.respite_active {
                    tracing::info!("Respite-Notausgang: Sequenz korrekt eingegeben");
                    state.respite_escaped_this_slot = true;
                    deactivate_respite(state);
                }
            }
            LRESULT(0)
        }
        hook::WM_APP_RESPITE_KEYCHAR => {
            let state_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut AppState;
            if !state_ptr.is_null() {
                let state = &mut *state_ptr;
                if state.respite_active && state.respite_escape_unlocked {
                    let ch = char::from_u32(wparam.0 as u32).unwrap_or('\0');
                    let expected = state.respite_escape_sequence
                        .chars()
                        .nth(state.respite_typed.len());
                    if Some(ch) == expected {
                        state.respite_typed.push(ch);
                        if state.respite_typed == state.respite_escape_sequence {
                            // Sequenz vollständig → Escape auslösen.
                            let _ = PostMessageW(
                                hwnd,
                                hook::WM_APP_RESPITE_ESCAPE,
                                WPARAM(0),
                                LPARAM(0),
                            );
                        } else {
                            update_respite_overlay(state);
                        }
                    } else if !state.respite_typed.is_empty() {
                        // Falsches Zeichen → von vorne.
                        state.respite_typed = String::new();
                        update_respite_overlay(state);
                    }
                }
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

fn handle_display_change(state: &mut AppState) {
    let new_ids = monitors::current_ids();
    if new_ids == state.last_monitor_ids {
        return;
    }

    let added: Vec<&String> =
        new_ids.iter().filter(|id| !state.last_monitor_ids.contains(id)).collect();
    let removed: Vec<&String> =
        state.last_monitor_ids.iter().filter(|id| !new_ids.contains(id)).collect();

    if !added.is_empty() {
        tracing::info!("Monitor(e) hinzugekommen: {:?}", added);
    }
    if !removed.is_empty() {
        tracing::info!("Monitor(e) entfernt: {:?}", removed);
    }

    state.manager.refresh_monitors();
    state.last_monitor_ids = new_ids;
    state.manager.apply_visibility();
}

fn cleanup(state: &mut AppState) {
    unsafe {
        let _ = KillTimer(state.hwnd, TIMER_RESPITE);
    }
    hook::set_respite_active(false);
    if let Some(hook) = state.keyboard_hook.take() {
        hook::uninstall(hook);
    }
    if let Some(hook) = state.respite_keyboard_hook.take() {
        hook::uninstall(hook);
    }
    if let Some(hook) = state.mouse_hook.take() {
        hook::uninstall_mouse(hook);
    }
    for id in &state.hotkey_ids {
        hotkeys::unregister(state.hwnd, *id);
    }
    state.manager.show_all();
}
