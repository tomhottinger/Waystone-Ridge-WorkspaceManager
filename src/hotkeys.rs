//! Hotkey-Parsing und Registrierung globaler Hotkeys.
//!
//! Format: `Mod+Mod+...+Key`, z. B. `Win+1`, `Win+Shift+1`, `Ctrl+Alt+F2`.
//! Unterstützte Modifier: `Win`, `Ctrl`, `Alt`, `Shift`.
//! Unterstützte Tasten: Ziffern `0–9`, Buchstaben `A–Z`, Funktionstasten `F1–F24`.

use anyhow::{anyhow, bail, Result};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT,
    MOD_SHIFT, MOD_WIN,
};

/// Aktion, die ein Hotkey auslöst.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Workspace mit dieser ID aktivieren.
    Activate(u32),
    /// Vordergrundfenster in den Workspace mit dieser ID verschieben.
    MoveWindow(u32),
    /// Erstes Fenster, dessen Titel den Teilstring enthält, auf den aktuellen
    /// Workspace holen (Groß-/Kleinschreibung wird ignoriert). Wird kein Fenster
    /// gefunden und `launch` ist gesetzt, wird dieser Befehl gestartet.
    Summon { title: String, launch: Option<String>, launch_dir: Option<String> },
    /// Randloses Texteingabefeld ein- oder ausblenden.
    ToggleQuickInput,
}

/// Geparster Hotkey: Modifier-Maske und virtueller Tastencode.
#[derive(Debug, Clone, Copy)]
pub struct ParsedHotkey {
    pub modifiers: HOT_KEY_MODIFIERS,
    pub vk: u32,
}

impl ParsedHotkey {
    /// Reine Modifier-Bits (ohne `MOD_NOREPEAT`), kompatibel zum Bit-Schema des
    /// Keyboard-Hooks: `Alt=1, Ctrl=2, Shift=4, Win=8`.
    pub fn mod_bits(&self) -> u32 {
        self.modifiers.0 & 0xF
    }
}

/// Parst einen Hotkey-String wie `"Win+Shift+1"`.
pub fn parse(spec: &str) -> Result<ParsedHotkey> {
    let mut modifiers = MOD_NOREPEAT;
    let mut vk: Option<u32> = None;

    for raw in spec.split('+') {
        let part = raw.trim();
        if part.is_empty() {
            continue;
        }
        match part.to_ascii_lowercase().as_str() {
            "win" | "super" | "meta" | "windows" => modifiers |= MOD_WIN,
            "ctrl" | "control" | "strg" => modifiers |= MOD_CONTROL,
            "alt" => modifiers |= MOD_ALT,
            "shift" | "umschalt" => modifiers |= MOD_SHIFT,
            _ => {
                if vk.is_some() {
                    bail!("mehr als eine Taste im Hotkey '{}'", spec);
                }
                vk = Some(parse_key(part)?);
            }
        }
    }

    let vk = vk.ok_or_else(|| anyhow!("kein Tastencode im Hotkey '{}'", spec))?;
    Ok(ParsedHotkey { modifiers, vk })
}

/// Übersetzt einen Tastennamen in einen virtuellen Tastencode (VK).
fn parse_key(key: &str) -> Result<u32> {
    let lo = key.to_ascii_lowercase();
    let up = key.to_ascii_uppercase();

    // Benannte Sondertasten.
    match lo.as_str() {
        "space" | "leertaste"              => return Ok(0x20), // VK_SPACE
        "escape" | "esc"                   => return Ok(0x1B), // VK_ESCAPE
        "tab"                              => return Ok(0x09), // VK_TAB
        "return" | "enter"                 => return Ok(0x0D), // VK_RETURN
        "backspace" | "bs"                 => return Ok(0x08), // VK_BACK
        "delete" | "del" | "entf"          => return Ok(0x2E), // VK_DELETE
        "insert" | "ins" | "einfg"         => return Ok(0x2D), // VK_INSERT
        "home" | "pos1"                    => return Ok(0x24), // VK_HOME
        "end" | "ende"                     => return Ok(0x23), // VK_END
        "pageup"   | "pgup"   | "bildauf"  => return Ok(0x21), // VK_PRIOR
        "pagedown" | "pgdown" | "pgdn" | "bildab" => return Ok(0x22), // VK_NEXT
        "left"  | "links"                  => return Ok(0x25), // VK_LEFT
        "right" | "rechts"                 => return Ok(0x27), // VK_RIGHT
        "up"    | "oben"                   => return Ok(0x26), // VK_UP
        "down"  | "unten"                  => return Ok(0x28), // VK_DOWN
        _ => {}
    }

    // Einzelne Ziffer oder einzelner Buchstabe: VK == ASCII-Code.
    if up.chars().count() == 1 {
        let c = up.chars().next().unwrap();
        if c.is_ascii_digit() || c.is_ascii_uppercase() {
            return Ok(c as u32);
        }
    }

    // Funktionstasten F1..F24 -> VK_F1 (0x70) .. VK_F24 (0x87).
    if let Some(num) = up.strip_prefix('F') {
        if let Ok(n) = num.parse::<u32>() {
            if (1..=24).contains(&n) {
                return Ok(0x70 + (n - 1));
            }
        }
    }

    bail!("unbekannte Taste '{}'", key)
}

/// Registriert einen globalen Hotkey für das Fenster `hwnd` unter der ID `id`.
pub fn register(hwnd: HWND, id: i32, hk: &ParsedHotkey) -> Result<()> {
    unsafe {
        RegisterHotKey(hwnd, id, hk.modifiers, hk.vk)
            .map_err(|e| anyhow!("RegisterHotKey (id {}) fehlgeschlagen: {}", id, e))?;
    }
    Ok(())
}

/// Hebt die Registrierung eines Hotkeys wieder auf (Fehler werden ignoriert).
pub fn unregister(hwnd: HWND, id: i32) {
    unsafe {
        let _ = UnregisterHotKey(hwnd, id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parst_ziffern() {
        let hk = parse("Win+1").unwrap();
        assert_eq!(hk.vk, '1' as u32);
        assert!(hk.modifiers.contains(MOD_WIN));
    }

    #[test]
    fn parst_shift_kombination() {
        let hk = parse("Win+Shift+2").unwrap();
        assert_eq!(hk.vk, '2' as u32);
        assert!(hk.modifiers.contains(MOD_WIN));
        assert!(hk.modifiers.contains(MOD_SHIFT));
    }

    #[test]
    fn parst_funktionstaste() {
        let hk = parse("Ctrl+Alt+F2").unwrap();
        assert_eq!(hk.vk, 0x71);
        assert!(hk.modifiers.contains(MOD_CONTROL));
        assert!(hk.modifiers.contains(MOD_ALT));
    }

    #[test]
    fn fehler_ohne_taste() {
        assert!(parse("Win+Shift").is_err());
    }
}
