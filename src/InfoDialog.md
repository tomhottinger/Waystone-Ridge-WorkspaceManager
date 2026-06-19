# Waystone Ridge
**Workspace Manager für Windows** · Version {VERSION} · Copyright © 2026 Thomas C. Hottinger

---

## Was macht Waystone Ridge?

Waystone Ridge teilt deinen Bildschirm in benannte **Workspaces** auf. Jedes Programmfenster gehört zu genau einem Workspace. Wenn du einen Workspace aktivierst, werden alle Fenster der anderen Workspaces ausgeblendet — sie laufen weiter, sind aber unsichtbar. So hast du immer nur das auf dem Schirm, was du gerade brauchst.

---

## Workspaces

Jeder Workspace hat einen Namen und zwei Hotkeys — einen zum **Aktivieren** und einen zum **Verschieben** des aktiven Fensters:

| Aktion | Standard-Hotkey |
|--------|----------------|
| Workspace 1 aktivieren | `Win+1` |
| Aktives Fenster zu Workspace 1 verschieben | `Win+Shift+1` |

Die Nummern 1–7 sind voreingestellt. Du kannst beliebige Hotkeys in `config.toml` festlegen — auch Buchstaben oder F-Tasten, z.B. `Ctrl+Alt+E` für „Entwicklung".

Das **Tray-Icon** zeigt immer die Nummer des aktiven Workspace. Ein Rechtsklick auf das Tray-Icon öffnet das Menü.

---

## Summon-Hotkeys

Ein Summon-Hotkey holt ein bestimmtes Fenster auf den **aktuellen Workspace** — egal auf welchem Workspace es gerade liegt oder ob es ausgeblendet ist. Das Fenster bekommt automatisch den Fokus.

Ist das Fenster bereits vorne auf dem aktuellen Workspace, wird es stattdessen **minimiert** (Toggle-Verhalten).

Wird kein passendes Fenster gefunden, kann optional ein Programm gestartet werden.

Summons werden in `config.toml` konfiguriert:

```toml
[[summons]]
hotkey = "Win+F1"
title  = "Outlook"
launch = "outlook.exe"
```

---

## Markdown-Schnellnotiz

Ein randloses, always-on-top Fenster mit einem **Markdown-Block-Editor**. Per Hotkey aufrufen, Notiz tippen, wieder wegklicken — der Inhalt bleibt erhalten.

**Öffnen/Schliessen:** konfigurierbarer Hotkey (z.B. `Ctrl+s`), `ESC` oder Klick ins Leere.

**Tastaturkürzel im Editor:**

| Kürzel | Aktion |
|--------|--------|
| `Ctrl+B` | **Fett** |
| `Ctrl+I` | *Kursiv* |
| `Ctrl+K` | `Inline-Code` |
| `Tab` | 2 Leerzeichen |
| `Ctrl+Enter` | Neuen Block anlegen |
| `Alt+↓` | Nächsten Block aktivieren |
| `Alt+↑` | Vorherigen Block aktivieren |

Diese Hotkeys solltest Du für die Workspaces und Summons nicht verwenden, wenn Du Schnellnotizen verwendest. 

Der **`…`-Button** in der Toolbar öffnet das Export-Menü: Markdown in die Zwischenablage kopieren oder als `.md`-Datei speichern.

---

## Desktop-Overlay

Optional kann in einer Bildschirmecke dauerhaft der Name des aktiven Workspace angezeigt werden. Das Overlay ist click-through und stört die Arbeit nicht.

```toml
show_overlay   = true
overlay_corner = "top_right"
```

---

## Konfiguration

Beim ersten Start wird `config.toml` **neben der EXE** erzeugt — vollständig kommentiert, mit allen Optionen und Erklärungen. Kein separater Download nötig: einfach starten, die erzeugte Datei öffnen und anpassen.

Mit `--config <Pfad>` kann auch eine andere Konfigurationsdatei angegeben werden.

---

## Lizenz und Quellcode

Waystone Ridge ist Open Source und in Rust geschrieben.

[Quellcode und Dokumentation auf GitHub](https://github.com/tomhottinger/Waystone-Ridge-WorkspaceManager)

[Lizenztext (MIT)](https://raw.githubusercontent.com/tomhottinger/Waystone-Ridge-WorkspaceManager/refs/heads/main/LICENSE)

Mit dem Verwenden dieser Software stimmst du den Lizenzbedingungen zu.
