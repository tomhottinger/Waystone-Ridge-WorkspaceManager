# Workspace Manager (Windows, Rust)

Hintergrundprozess mit Tray-Icon, der benannte **Workspaces** verwaltet. Beim
Aktivieren eines Workspace sind nur dessen Fenster sichtbar; Fenster anderer
Workspaces bleiben geöffnet, werden aber per `ShowWindow(SW_HIDE)` ausgeblendet.
Beim Beenden werden alle versteckten Fenster wieder sichtbar gemacht.

## Funktionen

- Beliebig viele Workspaces (Standard: 1–7).
- Globale Hotkeys: `Win+N` aktivieren, `Win+Shift+N` aktives Fenster verschieben.
- Beliebige Hotkey-Kombinationen (`Win`, `Ctrl`, `Alt`, `Shift` + Ziffer/Buchstabe/`F1–F24`).
- Automatische Zuordnung neuer Fenster zum aktiven Workspace.
- **Dynamisches Tray-Icon**: zeigt die Nummer des aktiven Workspace als Pixel-Text.
- **Desktop-Overlay**: always-on-top, halbtransparentes Fenster in einer konfigurierbaren
  Bildschirmecke — zeigt dauerhaft den Namen des aktiven Workspace (opt-in via `config.toml`).
- **Summon-Hotkeys**: ein beliebiges Fenster per Fenstertitel-Suche auf den aktuellen
  Workspace holen — auch wenn es gerade versteckt auf einem anderen Workspace liegt.
  Das Fenster erhält automatisch den Fokus und wird in den Vordergrund gebracht.
  Ist das Fenster bereits aktiv auf dem aktuellen Workspace, wird es stattdessen **minimiert** (Toggle).
  Wird kein passendes Fenster gefunden, kann optional eine **Kommandozeile** gestartet werden.
- Erkennung von Monitoränderungen (`WM_DISPLAYCHANGE`) über **stabile Geräte-IDs**
  (Docking-Szenarien); Zuordnungen bleiben erhalten.
- Tray-Icon-Menü: Workspace wählen, Beenden; der aktive Workspace trägt ein Häkchen.
- Filtert ungeeignete Fenster (Tool-/Shell-/cloaked UWP-Fenster).
- Optionale Konsolenausgabe (`--debug`), alternatives Config-File (`--config`) und
  ein auf 1000 Zeilen begrenztes Logfile (`--log`).

## Voraussetzungen

- [Rust](https://www.rust-lang.org/tools/install) (stable) inkl. `cargo`.
- Windows 10/11 zum Ausführen.

## Bauen

```powershell
rustup default stable
cargo build --release
```

Das Ergebnis: `target\release\Waystone-Ridge.exe`.

## Konfiguration

Beim ersten Start wird `config.toml` **neben der EXE** erzeugt (Workspaces 1–7).
Mit `--config <Pfad>` kann eine andere Datei angegeben werden.

### Workspaces

```toml
[[workspaces]]
id = 1
name = "Entwicklung"
activate_hotkey    = "Win+1"
move_window_hotkey = "Win+Shift+1"
```

### Workspace-Anzeige

Das **Tray-Icon** zeigt immer die Nummer des aktiven Workspace als Pixel-Text —
ohne Hovern oder Öffnen des Menüs.

Das **Desktop-Overlay** ist standardmäßig deaktiviert. Es wird durch zwei globale
Einträge (außerhalb aller `[[…]]`-Blöcke, am Anfang der Datei) eingeschaltet:

```toml
show_overlay   = true
overlay_corner = "top_right"
```

Erlaubte Werte für `overlay_corner`:

| Oben | Mitte-oben | Oben rechts |
|---|---|---|
| `top_left` | `top_center` | `top_right` |
| `bottom_left` | `bottom_center` | `bottom_right` |

Das Overlay passt sich bei jedem Workspace-Wechsel sofort an und ist
click-through (Mausklicks gehen durch).

### Verhalten beim Fenster verschieben

Mit `move_window_follow` (globale Option) lässt sich steuern, ob nach dem
Verschieben eines Fensters automatisch in den Zielworkspace gewechselt wird:

```toml
move_window_follow = true   # Standard: in den Zielworkspace wechseln
move_window_follow = false  # auf dem aktuellen Workspace bleiben
```

| Wert | Verhalten |
|------|-----------|
| `true` (Standard) | Das Fenster wird verschoben **und** der Zielworkspace wird aktiv. |
| `false` | Das Fenster verschwindet aus dem aktuellen Workspace und liegt auf dem Ziel bereit — der aktive Workspace ändert sich nicht. |

### Summon-Hotkeys

Mit `[[summons]]`-Blöcken lassen sich Hotkeys definieren, die ein bestimmtes
Fenster **auf den aktuellen Workspace holen** — auch wenn es gerade versteckt
auf einem anderen Workspace liegt:

```toml
# Einfacher Summon:
[[summons]]
hotkey = "Win+F1"
title  = "Outlook"

# Mit launch-Fallback:
[[summons]]
hotkey = "Win+F2"
title  = "Slack"
launch = "slack.exe"

# Mit launch und Arbeitsverzeichnis:
[[summons]]
hotkey     = "Win+F3"
title      = "Tom's Console"
launch     = "%LOCALAPPDATA%\\Microsoft\\WindowsApps\\wt.exe -p \"Tom's Console\""
launch_dir = "C:\\dev"
```

| Feld | Pflicht | Beschreibung |
|------|---------|--------------|
| `hotkey` | ja | Hotkey-Format wie bei Workspaces |
| `title` | ja | Teilstring des Fenstertitels (Groß-/Kleinschreibung egal) |
| `launch` | nein | Kommandozeile, die gestartet wird, wenn kein Fenster gefunden wird |
| `launch_dir` | nein | Arbeitsverzeichnis für den gestarteten Prozess |

**Verhalten:**

| Situation | Aktion |
|-----------|--------|
| Fenster auf aktuellem Workspace **und** im Vordergrund | Fenster wird **minimiert** (Toggle) |
| Fenster existiert, aber auf anderem Workspace oder nicht im Vordergrund | Fenster wird auf den aktuellen Workspace geholt, erhält Fokus |
| Fenster nicht gefunden, `launch` definiert | Programm wird gestartet |
| Fenster nicht gefunden, kein `launch` | Nichts (Meldung im Log bei `--debug`) |

**Hinweise zu `launch`:**
- `%ENVVAR%`-Variablen werden von `cmd.exe` expandiert.
- In TOML-Basic-Strings (doppelte Anführungszeichen) müssen `\` als `\\` und `"` als `\"` geschrieben werden.
- Enthält der Befehl keine Anführungszeichen, kann auch eine Literal-String verwendet werden: `launch = 'pfad\programm.exe'`
- Beliebig viele `[[summons]]`-Blöcke möglich, auch keiner.

## Bedienung

| Aktion | Standard-Hotkey |
|--------|----------------|
| Workspace N aktivieren | `Win+N` |
| Aktives Fenster zu Workspace N verschieben | `Win+Shift+N` |
| Fenster per Titel holen (Summon) | konfigurierbar, z. B. `Win+F1` |

- Beim Verschieben eines Fensters wird standardmäßig der **Zielworkspace zum aktiven Workspace** (konfigurierbar via `move_window_follow`).
- Tray-Icon (Rechts-/Linksklick): Workspace wählen oder **Beenden**.

## Kommandozeilenoptionen

| Option            | Wirkung |
|-------------------|---------|
| `--debug`         | Konsolenausgabe aktivieren. Ohne diese Option läuft die App **fensterlos**. |
| `--config <pfad>` | Verwendet die angegebene Konfigurationsdatei statt `config.toml` neben der EXE. |
| `--log <pfad>`    | Schreibt ein Logfile; nie mehr als die letzten **1000 Zeilen**. |

Fatale Startfehler werden in einer **MessageBox** angezeigt.

### Reservierte Hotkeys (Win+Ziffer)

`Win+1` … `Win+0` (und `Win+Shift+N`) sind von der Windows-Taskleiste reserviert.
Die App erkennt das automatisch und weicht auf einen **Low-Level-Keyboard-Hook**
(`WH_KEYBOARD_LL`) aus. Nicht reservierte Hotkeys nutzen `RegisterHotKey`.

## Logging

Standard: keine Ausgabe. Mit `--debug` → Konsole, mit `--log <pfad>` → Datei
(auf 1000 Zeilen begrenzt). Beides kombinierbar.

## Bekannte Grenzen (v1)

- In-Memory-Zuordnung (keine Persistenz von Fensterzuständen nach Neustart).
- UWP-/Admin-/Systemfenster können Sonderverhalten zeigen.
- Monitorzuordnungen steuern in v1 die Sichtbarkeit nicht (nur HWND→Workspace).

## Module

| Datei           | Aufgabe |
|-----------------|---------|
| `main.rs`       | Start, Message-Loop, Tray-Icon, Hotkey-Dispatch, Cleanup |
| `cli.rs`        | Kommandozeilen-Argumente parsen |
| `logging.rs`    | Tracing-Init: Konsole und begrenztes Logfile |
| `config.rs`     | `config.toml` laden/erzeugen/validieren |
| `hotkeys.rs`    | Hotkey-Strings parsen, globale Hotkeys registrieren |
| `hook.rs`       | Low-Level-Keyboard-Hook für reservierte Hotkeys |
| `windows.rs`    | Fenster enumerieren/filtern, anzeigen/verstecken, Titelsuche |
| `monitors.rs`   | Monitore enumerieren, stabile IDs, Änderungserkennung |
| `workspace.rs`  | `WorkspaceManager`: Zuordnung, Wechsel, Verschieben, Holen |
| `overlay.rs`    | Desktop-Overlay-Fenster (always-on-top, halbtransparent) |

## Tests

```powershell
cargo test
```

## Lizenz

Siehe LICENSE (MIT Lizenz)
