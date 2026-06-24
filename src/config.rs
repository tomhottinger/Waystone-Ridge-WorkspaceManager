//! Laden, Erzeugen und Validieren der `config.toml`.
//!
//! Die Datei liegt neben der ausführbaren Datei. Existiert sie nicht, wird eine
//! Standardkonfiguration mit den Workspaces 1–7 erzeugt.

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Konfiguration eines Summon-Hotkeys: holt ein Fenster per Titelsuche.
#[derive(Debug, Clone, Deserialize)]
pub struct SummonConfig {
    /// Hotkey, der die Suche auslöst.
    pub hotkey: String,
    /// Teilstring, nach dem im Fenstertitel gesucht wird (Groß-/Kleinschreibung egal).
    pub title: String,
    /// Kommandozeile, die gestartet wird, wenn kein passendes Fenster gefunden wird.
    #[serde(default)]
    pub launch: Option<String>,
    /// Arbeitsverzeichnis für den gestarteten Prozess (optional).
    #[serde(default)]
    pub launch_dir: Option<String>,
}

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

/// Position des Overlay-Fensters am Bildschirmrand.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum OverlayCorner {
    TopLeft,
    TopCenter,
    TopRight,
    BottomLeft,
    BottomCenter,
    #[default]
    BottomRight,
}

fn default_true() -> bool {
    true
}

fn default_quick_input_width_pct() -> u32 {
    40
}

fn default_quick_input_height_pct() -> u32 {
    30
}

fn default_quick_input_font_size() -> u32 {
    0
}

fn default_breakout_min_wait_secs() -> u32 {
    360
}

fn default_breakout_escape_len() -> usize {
    12
}

/// Globale Breakout-Einstellungen für den Respite-Notausgang.
#[derive(Debug, Clone, Deserialize)]
pub struct BreakoutConfig {
    /// Sekunden, die mindestens gewartet werden muss, bevor der Breakout aktivierbar ist.
    #[serde(default = "default_breakout_min_wait_secs")]
    pub min_wait_secs: u32,
    /// Länge der abzutippenden Zeichensequenz.
    #[serde(default = "default_breakout_escape_len")]
    pub escape_len: usize,
}

impl Default for BreakoutConfig {
    fn default() -> Self {
        Self {
            min_wait_secs: default_breakout_min_wait_secs(),
            escape_len: default_breakout_escape_len(),
        }
    }
}

/// Konfiguration eines einzelnen Respite-Zeitfensters.
#[derive(Debug, Clone, Deserialize)]
pub struct RespiteConfig {
    /// Anzeigename im Overlay (Standard: "Pause").
    #[serde(default)]
    pub label: Option<String>,
    /// Wochentage: "Mon"–"Sun" oder Deutsch "Montag"–"Sonntag", Kürzel "Mo"–"So".
    #[serde(default)]
    pub days: Vec<String>,
    /// Beginn der Sperre im Format "HH:MM".
    pub start: String,
    /// Ende der Sperre im Format "HH:MM".
    pub end: String,
    /// Überschreibt [respite_breakout].min_wait_secs für diesen Block.
    #[serde(default)]
    pub min_wait_secs: Option<u32>,
    /// Überschreibt [respite_breakout].escape_len für diesen Block.
    #[serde(default)]
    pub escape_len: Option<usize>,
}

/// Gesamte Konfiguration.
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub workspaces: Vec<WorkspaceConfig>,
    /// Summon-Hotkeys: Fenster per Titelsuche auf den aktuellen Workspace holen.
    #[serde(default)]
    pub summons: Vec<SummonConfig>,
    /// Overlay einschalten (Standard: aus).
    #[serde(default)]
    pub show_overlay: bool,
    /// Ecke des Overlay-Fensters (Standard: bottom_right).
    #[serde(default)]
    pub overlay_corner: OverlayCorner,
    /// Nach dem Verschieben eines Fensters in den Zielworkspace wechseln (Standard: true).
    #[serde(default = "default_true")]
    pub move_window_follow: bool,
    /// Hotkey für das randlose Schnelleingabe-Textfeld (Standard: nicht belegt).
    #[serde(default)]
    pub quick_input_hotkey: Option<String>,
    /// Breite des Schnelleingabe-Felds in Prozent der Bildschirmbreite (Standard: 40).
    #[serde(default = "default_quick_input_width_pct")]
    pub quick_input_width_pct: u32,
    /// Höhe des Schnelleingabe-Felds in Prozent der Bildschirmhöhe (Standard: 30).
    #[serde(default = "default_quick_input_height_pct")]
    pub quick_input_height_pct: u32,
    /// Schriftgröße des Schnelleingabe-Felds in Punkt (Standard: 0 = Windows-Standardschrift).
    #[serde(default = "default_quick_input_font_size")]
    pub quick_input_font_size: u32,
    /// Zeitgesteuerte Eingabesperren.
    #[serde(default)]
    pub respite: Vec<RespiteConfig>,
    /// Globale Breakout-Einstellungen für alle Respite-Sperren.
    #[serde(default)]
    pub respite_breakout: BreakoutConfig,
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

/// Standardkonfiguration: vollständig kommentierte config.example.toml, zur Compile-Zeit eingebettet.
const DEFAULT_CONFIG: &str = include_str!("../config.example.toml");
