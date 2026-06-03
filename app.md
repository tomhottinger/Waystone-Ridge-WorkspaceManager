# Workspace Manager für Windows (Rust)

## Ziel

Entwickle eine Windows-Applikation in Rust, die mehrere benannte Workspaces verwaltet. Die App ist eine reine Ergänzung zu Windows und ersetzt keine Windows-Funktion.

Ein Workspace definiert eine Arbeitsumgebung mit zugeordneten Fenstern und Monitoren. Beim Aktivieren eines Workspaces sollen nur die Fenster sichtbar sein, die diesem Workspace zugeordnet sind.

Fenster anderer Workspaces bleiben geöffnet, werden jedoch ausgeblendet.

---

# Plattform

```text
Betriebssystem: Windows 10/11
Sprache: Rust
App-Typ: Hintergrundprozess mit Tray-Icon
UI: Minimal
Konfiguration: Datei basiert
```

---

# Workspaces

## Anzahl

Die Anzahl der Workspaces ist nicht begrenzt.

Standardkonfiguration:

```text
Workspace 1 bis Workspace 7
```

Der Benutzer kann beliebig viele weitere Workspaces definieren.

Beispiel:

```toml
[[workspaces]]
id = 8
name = "Kunde A"

[[workspaces]]
id = 9
name = "Kunde B"
```

---

## Eigenschaften

```text
id
name
activate_hotkey
move_window_hotkey
assigned_monitors
```

Beispiel:

```toml
[[workspaces]]
id = 1
name = "Arbeit"
activate_hotkey = "Win+1"
move_window_hotkey = "Win+Shift+1"
```

---

# Hotkeys

## Workspace aktivieren

Beispiele:

```text
Win+1
Win+2
Win+3
...
Win+7
```

Darüber hinaus können beliebige Hotkeys definiert werden:

```text
Win+F1
Win+F2
Ctrl+Alt+1
Ctrl+Alt+2
```

## Aktuelles Fenster verschieben

Beispiele:

```text
Win+Shift+1
Win+Shift+2
Win+Shift+3
...
Win+Shift+7
```

---

# Fensterzuordnung

Jedes verwaltete Fenster gehört genau einem Workspace.

Interne Zuordnung:

```text
HWND → WorkspaceId
```

Neue Fenster werden automatisch dem aktuell aktiven Workspace zugeordnet.

Für die erste Version genügt eine In-Memory-Zuordnung.

---

# Workspace aktivieren

Beim Aktivieren eines Workspace:

```text
- Ziel-Workspace wird aktiv
- Fenster des Ziel-Workspace werden angezeigt
- Fenster anderer Workspaces werden versteckt
- Fensterpositionen bleiben erhalten
- Monitorzuordnungen bleiben erhalten
```

---

# Fenster verschieben

Beim Verschieben eines Fensters:

```text
- Vordergrundfenster bestimmen
- Workspace-Zuordnung ändern
- Fenster ggf. ausblenden
- Fenster ggf. anzeigen
```

---

# Sichtbarkeit

Erste Implementierung:

```text
ShowWindow(hwnd, SW_HIDE)
ShowWindow(hwnd, SW_SHOW)
```

Später kann DWM Cloaking evaluiert werden.

---

# Monitor-Modell

Ein Workspace besteht aus einer Menge von Monitoren.

```rust
Workspace {
    id,
    name,
    assigned_monitors[]
}
```

Ein Monitor kann genau einem Workspace zugeordnet sein.

---

# Dynamische Monitoränderungen

Die Anwendung muss Monitoränderungen zur Laufzeit erkennen.

Beispiele:

```text
- Dockingstation angeschlossen
- Dockingstation entfernt
- Laptopdeckel geöffnet
- Laptopdeckel geschlossen
- Monitor ein- oder ausgeschaltet
```

---

# Laptop-Docking-Szenario

Beispiel:

```text
Monitor A = Extern links
Monitor B = Extern rechts
Monitor C = Laptopdisplay
```

Situation:

```text
Laptop offen
→ A + B + C
```

```text
Laptop geschlossen
→ A + B
```

```text
Laptop wieder offen
→ A + B + C
```

Die Anwendung muss diese Änderungen automatisch verarbeiten.

---

# Verhalten bei Monitoränderungen

Wenn ein Monitor erscheint:

```text
- Monitor erkennen
- Zuordnung beibehalten
- Workspace-Fenster anzeigen
```

Wenn ein Monitor verschwindet:

```text
- Workspace bleibt bestehen
- Fensterzuordnungen bleiben bestehen
- Fenster werden nicht gelöscht
```

Bei Rückkehr:

```text
- Zuordnung wiederherstellen
- Fenster erscheinen erneut
```

---

# Monitoridentifikation

Monitore dürfen nicht über Bildschirmpositionen identifiziert werden.

Verwende stabile Gerätekennungen.

Beispiele:

```text
DISPLAY_DEVICE
Monitor Device Path
Monitor Interface Name
```

---

# Konfigurationsdatei

Datei:

```text
config.toml
```

Beispiel:

```toml
[[workspaces]]
id = 1
name = "Arbeit"
activate_hotkey = "Win+1"
move_window_hotkey = "Win+Shift+1"

[[workspaces]]
id = 2
name = "Entwicklung"
activate_hotkey = "Win+2"
move_window_hotkey = "Win+Shift+2"

[[workspaces]]
id = 3
name = "Kommunikation"
activate_hotkey = "Win+3"
move_window_hotkey = "Win+Shift+3"
```

---

# Empfohlener Rust Stack

```toml
[dependencies]
windows = { version = "0.58", features = [
  "Win32_Foundation",
  "Win32_UI_WindowsAndMessaging",
  "Win32_Graphics_Dwm",
  "Win32_System_Threading"
] }

serde = { version = "1", features = ["derive"] }
toml = "0.8"
tracing = "0.1"
tracing-subscriber = "0.3"
anyhow = "1"
```

Optional:

```toml
tray-icon = "0.14"
tao = "0.28"
```

---

# Modulstruktur

```text
main.rs
config.rs
hotkeys.rs
windows.rs
workspace.rs
monitors.rs
```

---

# config.rs

Aufgaben:

```text
- config.toml laden
- Workspaces deserialisieren
- Hotkeys parsen
```

---

# hotkeys.rs

Aufgaben:

```text
- globale Hotkeys registrieren
- Hotkey IDs verwalten
- WM_HOTKEY verarbeiten
```

---

# windows.rs

Aufgaben:

```text
- Fenster enumerieren
- Vordergrundfenster bestimmen
- Fenster anzeigen
- Fenster verstecken
- ungeeignete Fenster filtern
```

---

# monitors.rs

Aufgaben:

```text
- Monitore enumerieren
- stabile Monitor-ID bestimmen
- Monitoränderungen erkennen
- Workspace-Monitorzuordnung verwalten
```

Windows APIs:

```text
EnumDisplayMonitors
GetMonitorInfo
EnumDisplayDevices
WM_DISPLAYCHANGE
```

---

# workspace.rs

Aufgaben:

```text
- aktueller Workspace
- HWND → Workspace Mapping
- Workspace wechseln
- Fenster verschieben
- neue Fenster automatisch zuordnen
```

---

# Datenmodell

```rust
struct Workspace {
    id: u32,
    name: String,
    monitors: Vec<MonitorId>,
}
```

```rust
struct Monitor {
    id: String,
    name: String,
}
```

```rust
struct WorkspaceManager {
    current_workspace: u32,
    window_workspace: HashMap<HWND, u32>,
    workspaces: HashMap<u32, Workspace>,
    monitors: HashMap<String, Monitor>,
}
```

---

# Startablauf

```text
1. Logging initialisieren
2. config.toml laden
3. Workspaces validieren
4. WorkspaceManager erzeugen
5. Aktuelle Fenster Workspace 1 zuordnen
6. Hotkeys registrieren
7. Message Loop starten
```

---

# Workspace-Wechsel

```text
1. Fenster enumerieren
2. neue Fenster erfassen
3. Workspace setzen
4. Fenster anzeigen oder verstecken
```

---

# Hotkey Parsing

Mindestens:

```text
Win+1
Win+2
Win+3
Win+Shift+1
Win+Shift+2
Win+Shift+3
```

Später beliebige Kombinationen.

---

# Nicht-Ziele der ersten Version

```text
- Tiling Window Manager
- eigene Fensteranordnung
- virtuelle Monitore
- Persistenz von Fensterzuständen
- Shell-Erweiterungen
- Synchronisation mit Windows Virtual Desktops
- komplexe GUI
```

---

# Einschränkungen

```text
- UWP Apps können Sonderverhalten zeigen
- Admin-Fenster sind eventuell nicht steuerbar
- manche Systemfenster müssen gefiltert werden
- Abstürze können versteckte Fenster hinterlassen
```

---

# Sicherheitsanforderung

Beim Beenden müssen alle verwalteten Fenster wieder sichtbar gemacht werden.

```text
Alle versteckten Fenster wieder anzeigen.
```

---

# Akzeptanzkriterien

```text
- beliebig viele Workspaces
- Standardkonfiguration 1 bis 7
- beliebige Hotkeys möglich
- Workspacewechsel funktioniert
- Fenster können zwischen Workspaces verschoben werden
- Monitoränderungen werden erkannt
- Docking-Szenarien funktionieren
- Fenster anderer Workspaces sind unsichtbar
- Fenster bleiben geöffnet
- Fensterpositionen bleiben erhalten
- Hintergrundprozess läuft stabil
- Beim Beenden werden alle Fenster sichtbar
```
