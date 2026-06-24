//! Zeitgesteuerte Eingabesperre (Respite).
//!
//! Definiert Zeitfenster, in denen Maus und Tastatur vollständig gesperrt sind.
//! Konfiguration über `[[respite]]`-Blöcke in der config.toml.

use crate::config::RespiteConfig;

/// Ein aufbereiteter Respite-Zeitslot (geparste Form der `RespiteConfig`).
pub struct RespiteSchedule {
    /// Wochentage als Bitmaske: Bit 0 = Montag … Bit 6 = Sonntag.
    days: u8,
    /// Beginn der Sperre in Minuten seit Mitternacht.
    start_mins: u16,
    /// Ende der Sperre in Minuten seit Mitternacht.
    end_mins: u16,
    /// Anzeigename für das Overlay.
    pub label: String,
}

/// Parst eine Liste von `RespiteConfig`s. Ungültige Einträge werden übersprungen.
pub fn parse(configs: &[RespiteConfig]) -> Vec<RespiteSchedule> {
    configs.iter().filter_map(parse_one).collect()
}

fn parse_one(c: &RespiteConfig) -> Option<RespiteSchedule> {
    let start_mins = parse_time(&c.start)?;
    let end_mins = parse_time(&c.end)?;
    if end_mins <= start_mins {
        tracing::warn!(
            "Respite: end '{}' ist nicht nach start '{}', Eintrag ignoriert",
            c.end,
            c.start
        );
        return None;
    }
    let mut days: u8 = 0;
    for d in &c.days {
        match day_bit(d.as_str()) {
            Some(b) => days |= b,
            None => tracing::warn!("Respite: unbekannter Wochentag '{}', ignoriert", d),
        }
    }
    if days == 0 {
        tracing::warn!("Respite: kein gültiger Wochentag konfiguriert, Eintrag ignoriert");
        return None;
    }
    Some(RespiteSchedule {
        days,
        start_mins,
        end_mins,
        label: c.label.clone().unwrap_or_else(|| "Pause".to_string()),
    })
}

fn parse_time(s: &str) -> Option<u16> {
    let mut parts = s.splitn(2, ':');
    let h: u16 = parts.next()?.trim().parse().ok()?;
    let m: u16 = parts.next()?.trim().parse().ok()?;
    if h > 23 || m > 59 {
        tracing::warn!("Respite: Zeit '{}' außerhalb des gültigen Bereichs (00:00–23:59)", s);
        return None;
    }
    Some(h * 60 + m)
}

fn day_bit(s: &str) -> Option<u8> {
    match s.to_lowercase().as_str() {
        "mon" | "mo" | "monday" | "montag" => Some(1 << 0),
        "tue" | "di" | "tuesday" | "dienstag" => Some(1 << 1),
        "wed" | "mi" | "wednesday" | "mittwoch" => Some(1 << 2),
        "thu" | "do" | "thursday" | "donnerstag" => Some(1 << 3),
        "fri" | "fr" | "friday" | "freitag" => Some(1 << 4),
        "sat" | "sa" | "saturday" | "samstag" => Some(1 << 5),
        "sun" | "so" | "sunday" | "sonntag" => Some(1 << 6),
        _ => None,
    }
}

/// Gibt den ersten aktuell aktiven `RespiteSchedule` zurück (oder `None`).
pub fn active_slot(schedules: &[RespiteSchedule]) -> Option<&RespiteSchedule> {
    use windows::Win32::System::SystemInformation::GetLocalTime;

    let st = unsafe { GetLocalTime() };

    // wDayOfWeek: 0 = Sonntag, 1 = Montag … 6 = Samstag.
    let today_bit: u8 = match st.wDayOfWeek {
        1 => 1 << 0,
        2 => 1 << 1,
        3 => 1 << 2,
        4 => 1 << 3,
        5 => 1 << 4,
        6 => 1 << 5,
        0 => 1 << 6,
        _ => 0,
    };

    let now_mins = st.wHour as u16 * 60 + st.wMinute as u16;

    schedules.iter().find(|s| {
        (s.days & today_bit) != 0 && now_mins >= s.start_mins && now_mins < s.end_mins
    })
}

/// Formatiert die Endzeit eines Slots als "HH:MM".
pub fn format_end(slot: &RespiteSchedule) -> String {
    format!("{:02}:{:02}", slot.end_mins / 60, slot.end_mins % 60)
}
