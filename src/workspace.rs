//! Kernlogik: aktueller Workspace, HWND→Workspace-Zuordnung, Wechsel,
//! Fenster verschieben, automatische Zuordnung neuer Fenster.

use std::collections::HashMap;

use ::windows::Win32::Foundation::HWND;

use crate::config::Config;
use crate::monitors::{self, MonitorInfo};
use crate::windows as win;

/// Ein Workspace mit zugeordneten Monitoren.
///
/// `id` und `monitors` gehören zum in der Spezifikation festgelegten Datenmodell.
/// In v1 steuern die Monitore die Sichtbarkeit nicht; die Felder sind für spätere
/// Versionen vorgesehen.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Workspace {
    pub id: u32,
    pub name: String,
    pub monitors: Vec<String>,
}

/// Verwaltet alle Workspaces und die Zuordnung von Fenstern.
pub struct WorkspaceManager {
    /// Aktiver Workspace.
    pub current: u32,
    /// HWND-Schlüssel → Workspace-ID.
    pub window_ws: HashMap<isize, u32>,
    /// Workspace-ID → Workspace.
    pub workspaces: HashMap<u32, Workspace>,
    /// Reihenfolge der Workspaces (wie in der Konfiguration); für spätere
    /// Versionen (z. B. "nächster Workspace") vorgesehen.
    #[allow(dead_code)]
    pub order: Vec<u32>,
    /// Aktuell bekannte Monitore (stabile ID → Info).
    pub monitors: HashMap<String, MonitorInfo>,
}

impl WorkspaceManager {
    /// Erstellt den Manager aus der Konfiguration.
    pub fn new(cfg: &Config) -> Self {
        let mut workspaces = HashMap::new();
        let mut order = Vec::new();
        for w in &cfg.workspaces {
            workspaces.insert(
                w.id,
                Workspace {
                    id: w.id,
                    name: w.name.clone(),
                    monitors: w.assigned_monitors.clone(),
                },
            );
            order.push(w.id);
        }
        let current = order.first().copied().unwrap_or(1);

        let mut mgr = Self {
            current,
            window_ws: HashMap::new(),
            workspaces,
            order,
            monitors: HashMap::new(),
        };
        mgr.refresh_monitors();
        mgr
    }

    /// Name eines Workspace (oder Fallback).
    pub fn name_of(&self, id: u32) -> String {
        self.workspaces
            .get(&id)
            .map(|w| w.name.clone())
            .unwrap_or_else(|| format!("Workspace {id}"))
    }

    /// Aktualisiert die bekannte Monitorliste.
    pub fn refresh_monitors(&mut self) {
        self.monitors = monitors::enumerate()
            .into_iter()
            .map(|m| (m.id.clone(), m))
            .collect();
    }

    /// Ordnet alle aktuell sichtbaren Fenster dem angegebenen Workspace zu.
    /// Wird beim Start verwendet (Schritt: "Aktuelle Fenster Workspace 1 zuordnen").
    pub fn assign_all_visible(&mut self, ws: u32) {
        for hwnd in win::enumerate_manageable() {
            self.window_ws.insert(win::hwnd_key(hwnd), ws);
        }
    }

    /// Erfasst neue, bisher nicht verwaltete Fenster und ordnet sie dem aktuell
    /// aktiven Workspace zu.
    pub fn capture_new_windows(&mut self) {
        let current = self.current;
        for hwnd in win::enumerate_manageable() {
            self.window_ws.entry(win::hwnd_key(hwnd)).or_insert(current);
        }
    }

    /// Aktiviert einen Workspace: neue Fenster erfassen, aktiv setzen, Sichtbarkeit anwenden.
    pub fn activate(&mut self, target: u32) {
        if !self.workspaces.contains_key(&target) {
            tracing::warn!("Aktivierung unbekannter Workspace-ID {target} ignoriert");
            return;
        }
        self.capture_new_windows();
        self.current = target;
        self.apply_visibility();
        tracing::info!("Workspace {} ({}) aktiviert", target, self.name_of(target));
    }

    /// Wendet die Sichtbarkeit an: Fenster des aktiven Workspace anzeigen, alle
    /// anderen verwalteten Fenster verstecken. Tote Handles werden bereinigt.
    pub fn apply_visibility(&mut self) {
        self.window_ws
            .retain(|key, _| win::is_window(win::hwnd_from_key(*key)));

        for (key, ws) in self.window_ws.iter() {
            let hwnd = win::hwnd_from_key(*key);
            if *ws == self.current {
                win::show(hwnd);
            } else {
                win::hide(hwnd);
            }
        }
    }

    /// Verschiebt das Vordergrundfenster in den Ziel-Workspace.
    pub fn move_foreground(&mut self, target: u32) {
        if !self.workspaces.contains_key(&target) {
            tracing::warn!("Verschieben in unbekannten Workspace {target} ignoriert");
            return;
        }
        let hwnd = win::foreground_window();
        if win::hwnd_key(hwnd) == 0 {
            return;
        }
        // Nur sinnvoll verwaltbare Fenster bewegen.
        let key = win::hwnd_key(hwnd);
        if !win::is_manageable(hwnd) && !self.window_ws.contains_key(&key) {
            tracing::debug!("Vordergrundfenster ist nicht verwaltbar, Verschieben übersprungen");
            return;
        }

        // Bisher sichtbare, noch nicht verwaltete Fenster dem alten Workspace
        // zuordnen, bevor gewechselt wird (analog zu `activate`).
        self.capture_new_windows();
        self.window_ws.insert(key, target);
        // Der Ziel-Workspace wird aktiv; das verschobene Fenster bleibt sichtbar,
        // die Fenster des vorher aktiven Workspace werden versteckt.
        self.current = target;
        self.apply_visibility();
        tracing::info!(
            "Fenster nach Workspace {} ({}) verschoben und aktiviert",
            target,
            self.name_of(target)
        );
    }

    /// Holt ein beliebiges Fenster auf den aktuellen Workspace: Zuordnung
    /// anpassen (ggf. neu aufnehmen) und Sichtbarkeit neu anwenden.
    pub fn pull_to_current(&mut self, hwnd: HWND) {
        let key = win::hwnd_key(hwnd);
        self.window_ws.insert(key, self.current);
        self.apply_visibility();
        win::bring_to_foreground(hwnd);
        tracing::info!(
            "Fenster '{}' auf Workspace {} ({}) geholt",
            win::window_title(hwnd),
            self.current,
            self.name_of(self.current)
        );
    }

    /// Sicherheitsfunktion: macht alle verwalteten Fenster wieder sichtbar.
    /// Wird beim Beenden aufgerufen.
    pub fn show_all(&mut self) {
        for key in self.window_ws.keys() {
            win::show(win::hwnd_from_key(*key));
        }
        tracing::info!("Alle verwalteten Fenster wieder sichtbar gemacht");
    }
}
