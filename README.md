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
- Erkennung von Monitoränderungen (`WM_DISPLAYCHANGE`) über **stabile Geräte-IDs**
  (Docking-Szenarien); Zuordnungen bleiben erhalten.
- Tray-Icon-Menü: Workspace wählen, Beenden; der **aktive Workspace ist mit einem
  Häkchen markiert**.
- Filtert ungeeignete Fenster (Tool-/Shell-/cloaked UWP-Fenster).
- Optionale Konsolenausgabe (`--debug`), alternatives Config-File (`--config`) und
  ein auf 1000 Zeilen begrenztes Logfile (`--log`).

## Voraussetzungen

- [Rust](https://www.rust-lang.org/tools/install) (stable) inkl. `cargo`.
- Windows 10/11 zum Ausführen.

## Bauen

### Nativ unter Windows (empfohlen, MSVC)

```powershell
rustup default stable
cargo build --release
```

Das Ergebnis: `target\release\workspace-manager.exe`.

### Cross-Compile von Linux (GNU-Target)

```bash
rustup target add x86_64-pc-windows-gnu
# Linker: gcc-mingw-w64-x86-64
export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=x86_64-w64-mingw32-gcc
cargo build --release --target x86_64-pc-windows-gnu
```

> Hinweis: Diese App nutzt ausschließlich Windows-APIs und läuft nur unter
> Windows. Auf Linux lässt sie sich kompilieren, aber nicht ausführen/testen.

## Konfiguration

Beim ersten Start wird `config.toml` **neben der EXE** erzeugt (Workspaces 1–7).
Beispiel:

```toml
[[workspaces]]
id = 1
name = "Arbeit"
activate_hotkey = "Win+1"
move_window_hotkey = "Win+Shift+1"
assigned_monitors = []   # optionale stabile Monitor-IDs (in v1 informativ)
```

## Bedienung

- `Win+1` … `Win+7`: entsprechenden Workspace aktivieren.
- `Win+Shift+1` … `Win+Shift+7`: aktuelles Vordergrundfenster dorthin verschieben.
  Dabei wird der **Zielworkspace zum aktiven Workspace** – das verschobene Fenster
  bleibt sichtbar, die Fenster des vorherigen Workspace werden ausgeblendet.
- Tray-Icon (Rechts-/Linksklick): Workspace wählen oder **Beenden**. Der gerade
  aktive Workspace trägt im Menü ein Häkchen.

## Kommandozeilenoptionen

| Option            | Wirkung |
|-------------------|---------|
| `--debug`         | Konsolenausgabe aktivieren. Ohne diese Option läuft die App **fensterlos** (kein Konsolenfenster). Beim Start aus einer Konsole hängt sie sich an deren Fenster an, sonst öffnet sie ein neues. |
| `--config <pfad>` | Verwendet die angegebene Konfigurationsdatei statt `config.toml` neben der EXE. Existiert die Datei nicht, wird dort eine Standardkonfiguration angelegt. |
| `--log <pfad>`    | Schreibt ein Logfile an den angegebenen Pfad. Die Datei behält **nie mehr als die letzten 1000 Zeilen**; ältere Zeilen werden gelöscht. |

Ungültige Argumente und fatale Startfehler werden – mangels Konsole im Normalbetrieb –
in einer **MessageBox** angezeigt.

### Reservierte Hotkeys (Win+Ziffer)

`Win+1` … `Win+0` (und `Win+Shift+N`) sind von der Windows-Taskleiste reserviert;
`RegisterHotKey` lehnt sie ab und es gibt **keine** Möglichkeit, diese
Registrierung regulär zu überschreiben. Die App erkennt das automatisch und
weicht für solche Kombinationen auf einen **Low-Level-Keyboard-Hook**
(`WH_KEYBOARD_LL`) aus, der die Tasten vor der Shell abfängt und deren
Standardverhalten (inkl. Startmenü) unterdrückt. Nicht reservierte Hotkeys
(z. B. `Ctrl+Alt+1`, `Win+F1`) nutzen weiterhin den robusten
`RegisterHotKey`-Pfad. Welcher Pfad je Hotkey gewählt wurde, steht im Log.

## Logging

Standardmäßig läuft die App ohne Konsole und ohne Logfile. Mit `--debug` gehen
die Logs nach `stdout` (Konsole), mit `--log <pfad>` zusätzlich in eine Datei, die
auf die letzten 1000 Zeilen begrenzt ist. Beides lässt sich kombinieren.

## Bekannte Grenzen (v1)

- In-Memory-Zuordnung (keine Persistenz von Fensterzuständen).
- UWP-/Admin-/Systemfenster können Sonderverhalten zeigen; manche sind nicht
  steuerbar (Admin-Fenster nur bei erhöhten Rechten).
- Monitorzuordnungen steuern in v1 die Sichtbarkeit nicht (nur HWND→Workspace).

## Module

| Datei          | Aufgabe |
|----------------|---------|
| `main.rs`      | Start, Message-Loop, Tray-Icon, Hotkey-Dispatch, Cleanup |
| `cli.rs`       | Kommandozeilen-Argumente parsen (`--debug`, `--config`, `--log`) |
| `logging.rs`   | Tracing-Init: Konsole (bei `--debug`) und auf 1000 Zeilen begrenztes Logfile |
| `config.rs`    | `config.toml` laden/erzeugen/validieren |
| `hotkeys.rs`   | Hotkey-Strings parsen, globale Hotkeys registrieren |
| `windows.rs`   | Fenster enumerieren/filtern, anzeigen/verstecken |
| `monitors.rs`  | Monitore enumerieren, stabile IDs, Änderungserkennung |
| `workspace.rs` | `WorkspaceManager`: Zuordnung, Wechsel, Verschieben |

## Tests

```powershell
cargo test
```

Enthält u. a. Unit-Tests für das Parsen der Kommandozeilenoptionen (`cli.rs`).

## Lizenz

Siehe LICENSE (MIT Lizenz)
