//! Laden, Erzeugen und Validieren der `config.toml`.
//!
//! Die Datei liegt neben der ausführbaren Datei. Existiert sie nicht, wird eine
//! Standardkonfiguration mit den Workspaces 1–7 erzeugt.

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Konfiguration eines einzelnen Workspace, wie sie in `config.toml` steht.
#[derive(Debug, Clone, Deserialize)]
pub struct WorkspaceConfig {
    pub id: u32,
    pub name: String,
    #[serde(default)]
    pub activate_hotkey: Option<String>,
    #[serde(default)]
    pub move_window_hotkey: Option<String>,
    #[serde(default)]
    pub assigned_monitors: Vec<String>,
}

/// Ecke des Bildschirms, in der das Overlay angezeigt wird.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum OverlayCorner {
    TopLeft,
    TopRight,
    BottomLeft,
    #[default]
    BottomRight,
}

/// Gesamte Konfiguration.
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub workspaces: Vec<WorkspaceConfig>,
    /// Overlay einschalten (Standard: aus).
    #[serde(default)]
    pub show_overlay: bool,
    /// Ecke des Overlay-Fensters (Standard: bottom_right).
    #[serde(default)]
    pub overlay_corner: OverlayCorner,
}

/// Pfad zur `config.toml` neben der ausführbaren Datei.
pub fn config_path() -> Result<PathBuf> {
    let exe = std::env::current_exe().context("aktuellen EXE-Pfad bestimmen")?;
    let dir = exe
        .parent()
        .context("Verzeichnis der ausführbaren Datei bestimmen")?;
    Ok(dir.join("config.toml"))
}

/// Lädt die Konfiguration. Fehlt die Datei, wird die Standardkonfiguration
/// geschrieben und anschließend geladen.
///
/// Mit `override_path` (z. B. aus `--config <pfad>`) wird statt der Standarddatei
/// neben der ausführbaren Datei das angegebene File verwendet.
pub fn load_or_create(override_path: Option<&Path>) -> Result<Config> {
    let path = match override_path {
        Some(p) => p.to_path_buf(),
        None => config_path()?,
    };
    if !path.exists() {
        std::fs::write(&path, DEFAULT_CONFIG)
            .with_context(|| format!("Standardkonfiguration schreiben: {}", path.display()))?;
        tracing::info!("Standard-config.toml erzeugt: {}", path.display());
    }
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("config.toml lesen: {}", path.display()))?;
    let cfg: Config = toml::from_str(&text).context("config.toml parsen")?;
    validate(&cfg)?;
    Ok(cfg)
}

/// Prüft die Konfiguration auf grundlegende Konsistenz.
fn validate(cfg: &Config) -> Result<()> {
    if cfg.workspaces.is_empty() {
        bail!("keine Workspaces in der Konfiguration definiert");
    }
    let mut seen = HashSet::new();
    for w in &cfg.workspaces {
        if !seen.insert(w.id) {
            bail!("doppelte Workspace-ID: {}", w.id);
        }
    }
    Ok(())
}

/// Standardkonfiguration: Workspaces 1–7 mit Win+N / Win+Shift+N.
pub const DEFAULT_CONFIG: &str = r#"# Workspace Manager – Konfiguration
# Diese Datei wurde automatisch erzeugt. Sie kann frei angepasst werden.
#
# Eigenschaften je Workspace:
#   id                 eindeutige Zahl
#   name               Anzeigename
#   activate_hotkey    Hotkey zum Aktivieren (z. B. "Win+1", "Ctrl+Alt+1", "Win+F1")
#   move_window_hotkey Hotkey, um das aktive Fenster in diesen Workspace zu verschieben
#   assigned_monitors  optionale Liste stabiler Monitor-IDs (für spätere Versionen)
#
# Overlay-Optionen (globale Einstellungen, außerhalb der [[workspaces]]-Blöcke):
#   show_overlay   = true/false   – permanentes Overlay-Fenster aktivieren (Standard: false)
#   overlay_corner = "top_left" | "top_right" | "bottom_left" | "bottom_right"
#                                 – Bildschirmecke für das Overlay (Standard: bottom_right)
#
# Beispiel:
#   show_overlay = true
#   overlay_corner = "top_right"

[[workspaces]]
id = 1
name = "Workspace 1"
activate_hotkey = "Win+1"
move_window_hotkey = "Win+Shift+1"

[[workspaces]]
id = 2
name = "Workspace 2"
activate_hotkey = "Win+2"
move_window_hotkey = "Win+Shift+2"

[[workspaces]]
id = 3
name = "Workspace 3"
activate_hotkey = "Win+3"
move_window_hotkey = "Win+Shift+3"

[[workspaces]]
id = 4
name = "Workspace 4"
activate_hotkey = "Win+4"
move_window_hotkey = "Win+Shift+4"

[[workspaces]]
id = 5
name = "Workspace 5"
activate_hotkey = "Win+5"
move_window_hotkey = "Win+Shift+5"

[[workspaces]]
id = 6
name = "Workspace 6"
activate_hotkey = "Win+6"
move_window_hotkey = "Win+Shift+6"

[[workspaces]]
id = 7
name = "Workspace 7"
activate_hotkey = "Win+7"
move_window_hotkey = "Win+Shift+7"
"#;
