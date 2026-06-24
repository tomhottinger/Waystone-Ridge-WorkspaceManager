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
- **Markdown-Schnellnotiz (WYSIWYG)**: randloses, always-on-top Fenster mit WebView2.
  Typora-ähnlicher Block-Editor: Blöcke werden gerendert angezeigt, der aktive Block
  zeigt seinen Markdown-Quelltext. Per konfigurierbarem Hotkey ein-/ausgeblendet.
  Inhalt bleibt erhalten. Toolbar, Tastaturkürzel, Maus- und Tastaturnavigation.
- Erkennung von Monitoränderungen (`WM_DISPLAYCHANGE`) über **stabile Geräte-IDs**
  (Docking-Szenarien); Zuordnungen bleiben erhalten.
- **Konfigurationsmenü im Tray**: „Konfigurationsfile öffnen" startet den Standard-Texteditor;
  „neu einlesen" lädt `config.toml` sofort neu — kein Neustart nötig.
- **Respite – Zeitgesteuerte Eingabesperre**: Sperrt Maus und Tastatur für konfigurierte
  Zeitfenster (z. B. erzwungene Bildschirmpause). Prominentes zentriertes Overlay mit
  Countdown. Notausgang via `Ctrl+Alt+Shift+Delete`. Beliebig viele `[[respite]]`-Blöcke.
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

Beim ersten Start wird `config.toml` **neben der EXE** erzeugt — mit dem vollständig
kommentierten Inhalt der `config.example.toml` (zur Compile-Zeit eingebettet).
Die Datei kann direkt angepasst werden; kein separater Download nötig.
Mit `--config <Pfad>` kann auch eine andere Datei angegeben werden.

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

### Markdown-Schnellnotiz

Ein randloses, immer-oben Fenster mit **Typora-ähnlichem Block-Editor** (WebView2).
Blöcke werden gerendert dargestellt; der aktive Block zeigt seinen Markdown-Quelltext.
Beim Öffnen erhält der Editor den Fokus direkt am Ende des letzten Blocks.

Das Fenster erscheint **zentriert** auf dem Bildschirm.
WebView2-Daten liegen in `%LOCALAPPDATA%\Waystone-Ridge\WebView2` (nicht neben der EXE).

```toml
quick_input_hotkey     = "Ctrl+Space"  # beliebige Hotkey-Kombination
quick_input_width_pct  = 40            # Breite in % der Bildschirmbreite (Standard: 40)
quick_input_height_pct = 30            # Höhe in % der Bildschirmhöhe   (Standard: 30)
quick_input_font_size  = 0             # Schriftgröße in Punkt (0 = 14 px Standard)
```

| Feld | Pflicht | Beschreibung |
|------|---------|--------------|
| `quick_input_hotkey` | ja (zum Aktivieren) | Hotkey-Format wie bei Workspaces |
| `quick_input_width_pct` | nein | Fensterbreite 5–95 % der Bildschirmbreite (Standard: 40) |
| `quick_input_height_pct` | nein | Fensterhöhe 5–95 % der Bildschirmhöhe (Standard: 30) |
| `quick_input_font_size` | nein | Schriftgröße in Punkt; 0 = 14 px Standard |

**Fensterverhalten:**

| Aktion | Ergebnis |
|--------|----------|
| Hotkey (Fenster unsichtbar) | Fenster erscheint, Editor erhält Fokus |
| Hotkey (Fenster sichtbar) | Fenster verschwindet, Fokus kehrt zurück |
| `ESC` | Fenster verschwindet, Fokus kehrt zurück |
| Klick woanders | Fenster verschwindet automatisch |

**Tastaturkürzel im Editor:**

| Kürzel | Aktion |
|--------|--------|
| `Ctrl+B` | Fett (`**text**`) |
| `Ctrl+I` | Kursiv (`*text*`) |
| `Ctrl+K` | Inline-Code (`` `text` ``) |
| `Tab` | 2 Leerzeichen einfügen |
| `Enter` | Zeilenumbruch innerhalb eines Blocks |
| `Ctrl+Enter` | Neuen Block direkt nach dem aktuellen anlegen |
| `Alt+↓` | Nächsten Block aktivieren (oder neuen am Ende) |
| `Alt+↑` | Vorherigen Block aktivieren |

**Export (`…`-Button in der Toolbar):**

| Eintrag | Aktion |
|---------|--------|
| Markdown kopieren | Rohtext in die Zwischenablage |
| Speichern als … | Nativer Speichern-Dialog, Standard-Filter `*.md` |

**Unterstützte Markdown-Elemente:** Überschriften (H1–H6), Fett/Kursiv/Durchgestrichen,
Inline-Code und Code-Blöcke (mit Sprachkennung), Blockquotes, Listen (geordnet/ungeordnet),
Tabellen, horizontale Linien, Links.

Das Fenster ist deaktiviert, solange `quick_input_hotkey` nicht gesetzt ist.
Voraussetzung: Microsoft Edge / WebView2-Runtime (auf Windows 10/11 vorinstalliert).

### Konfigurationsmenü / Hot-Reload

Das Tray-Icon enthält ein Untermenü **Konfiguration**:

| Eintrag | Aktion |
|---------|--------|
| Konfigurationsfile öffnen | Öffnet `config.toml` im System-Standardeditor (`cmd /C start ""`) |
| neu einlesen | Lädt `config.toml` sofort neu — kein Neustart nötig |

Beim Neu-Einlesen werden alle Fenster kurz wieder sichtbar gemacht, der WorkspaceManager
neu aufgebaut, Hotkeys neu registriert und aktive Respite-Sperren beendet.

### Respite – Zeitgesteuerte Eingabesperre

`[[respite]]`-Blöcke definieren Zeitfenster, in denen Maus und Tastatur vollständig
blockiert werden. Während der Sperre erscheint ein großes, zentriertes Overlay mit
dem Pausennamen und einem Countdown bis zum Ende.

```toml
[[respite]]
label = "Mittagspause"
days  = ["Mon", "Tue", "Wed", "Thu", "Fri"]
start = "12:00"
end   = "12:15"

[[respite]]
label = "Nachmittagspause"
days  = ["Mon", "Tue", "Wed", "Thu"]
start = "15:30"
end   = "15:45"
```

| Feld | Pflicht | Beschreibung |
|------|---------|--------------|
| `label` | nein | Anzeigename im Overlay (Standard: „Pause") |
| `days` | ja | Wochentage als Liste: `"Mon"`–`"Sun"` oder Deutsch `"Montag"`–`"Sonntag"` |
| `start` | ja | Beginn der Sperre im Format `"HH:MM"` |
| `end` | ja | Ende der Sperre im Format `"HH:MM"` (muss nach `start` liegen) |

**Notausgang:** `Ctrl+Alt+Shift+Delete` bricht die aktive Sperre sofort ab und verhindert
die Reaktivierung für den aktuellen Zeitslot. Beim nächsten Slot greift die Sperre wieder.

Technisch: `WH_KEYBOARD_LL` + `WH_MOUSE_LL` blockieren alle nicht-injizierten Eingaben.
Der Modifier-Status wird manuell verfolgt (zuverlässiger als `GetAsyncKeyState` während
der Blockierung). Ein `respite_escaped_this_slot`-Flag verhindert sofortige Reaktivierung
durch den Sekunden-Timer.

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

### Hotkey-Referenz

#### Modifier (beliebig kombinierbar)

| Bezeichnung in config.toml | Bedeutung |
|----------------------------|-----------|
| `Win` | Windows-Taste (auch: `Super`, `Meta`, `Windows`) |
| `Ctrl` | Steuerungstaste (auch: `Control`, `Strg`) |
| `Alt` | Alt-Taste |
| `Shift` | Umschalttaste (auch: `Umschalt`) |

#### Tasten

| Kategorie | Schlüsselwörter |
|-----------|----------------|
| Buchstaben | `A` – `Z` (Groß-/Kleinschreibung egal) |
| Ziffern | `0` – `9` |
| Funktionstasten | `F1` – `F24` |
| Leertaste | `Space`, `Leertaste` |
| Escape | `Escape`, `Esc` |
| Tabulator | `Tab` |
| Enter | `Return`, `Enter` |
| Rücktaste | `Backspace`, `BS` |
| Entfernen | `Delete`, `Del`, `Entf` |
| Einfügen | `Insert`, `Ins`, `Einfg` |
| Pos1 | `Home`, `Pos1` |
| Ende | `End`, `Ende` |
| Bild auf | `PageUp`, `PgUp`, `Bildauf` |
| Bild ab | `PageDown`, `PgDown`, `PgDn`, `Bildab` |
| Pfeiltasten | `Left`, `Right`, `Up`, `Down` (auch: `Links`, `Rechts`, `Oben`, `Unten`) |

Beispiele: `Ctrl+Space`, `Win+F1`, `Ctrl+Alt+Delete`, `Alt+Left`

#### Von Windows reservierte Kombinationen

Einige Kombinationen sind systemweit belegt und können nicht überschrieben werden:

| Kombination | Windows-Funktion |
|-------------|-----------------|
| `Win+Space` | Eingabesprache wechseln (von Windows auf Kernel-Ebene abgefangen) |
| `Win+L` | Bildschirm sperren |
| `Win+D` | Desktop anzeigen |
| `Win+Tab` | Task View |

`Win+1` … `Win+0` und `Win+Shift+N` sind von der Windows-Taskleiste reserviert;
die App erkennt das automatisch und weicht auf einen **Low-Level-Keyboard-Hook**
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
| `hook.rs`       | Low-Level-Keyboard-/Maus-Hook: Hotkey-Fallback + Respite-Blockierung |
| `respite.rs`    | Respite-Zeitpläne parsen, aktiven Slot bestimmen, Countdown formatieren |
| `windows.rs`    | Fenster enumerieren/filtern, anzeigen/verstecken, Titelsuche |
| `monitors.rs`   | Monitore enumerieren, stabile IDs, Änderungserkennung |
| `workspace.rs`  | `WorkspaceManager`: Zuordnung, Wechsel, Verschieben, Holen |
| `overlay.rs`    | Desktop-Overlay-Fenster (always-on-top, halbtransparent) |
| `quick_input.rs` | Markdown-Schnellnotiz: WYSIWYG-Block-Editor via WebView2 |
| `quick_input.html` | HTML/JS für Editor (Markdown-Parser, Toolbar, Block-Editing) |
| `info_dialog.rs` | Info-/Hilfe-Dialog via WebView2 |
| `info_dialog.html` | HTML/JS für Info-Dialog (Markdown-Renderer, Titelzeile) |
| `InfoDialog.md` | Hilfetext (Markdown, wird zur Compile-Zeit eingebettet) |

## Tests

```powershell
cargo test
```

## Lizenz

Siehe LICENSE (MIT Lizenz)
