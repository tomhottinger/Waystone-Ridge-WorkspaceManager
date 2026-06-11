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

### Summon-Hotkeys

Mit `[[summons]]`-Blöcken lassen sich Hotkeys definieren, die ein bestimmtes
Fenster **auf den aktuellen Workspace holen** — auch wenn es gerade versteckt
auf einem anderen Workspace liegt:

```toml
[[summons]]
hotkey = "Win+F1"
title  = "Outlook"

[[summons]]
hotkey = "Win+F2"
title  = "Slack"
```

- `title` ist ein **Teilstring** des Fenstertitels; Groß-/Kleinschreibung wird ignoriert.
- Das gefundene Fenster wird auf den aktuellen Workspace geholt, in den Vordergrund
  gebracht und erhält den Fokus. War es minimiert, wird es dabei wiederhergestellt.
- Wird kein passendes Fenster gefunden, passiert nichts (Meldung im Log bei `--debug`).
- Beliebig viele Blöcke möglich, auch keiner.

## Bedienung

| Aktion | Standard-Hotkey |
|--------|----------------|
| Workspace N aktivieren | `Win+N` |
| Aktives Fenster zu Workspace N verschieben | `Win+Shift+N` |
| Fenster per Titel holen (Summon) | konfigurierbar, z. B. `Win+F1` |

- Beim Verschieben eines Fensters wird der **Zielworkspace zum aktiven Workspace**.
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
