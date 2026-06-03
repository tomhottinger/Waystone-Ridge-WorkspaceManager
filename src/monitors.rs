//! Monitor-Enumeration über stabile Geräte-IDs und Erkennung von Änderungen.
//!
//! Monitore werden **nicht** über ihre Bildschirmposition identifiziert, sondern
//! über den Device-Interface-Pfad (`DISPLAY_DEVICEW::DeviceID` mit dem Flag
//! `EDD_GET_DEVICE_INTERFACE_NAME`). Diese ID bleibt über An-/Abstecken stabil.

use windows::core::PCWSTR;
use windows::Win32::Graphics::Gdi::{EnumDisplayDevicesW, DISPLAY_DEVICEW};
use windows::Win32::UI::WindowsAndMessaging::EDD_GET_DEVICE_INTERFACE_NAME;

/// Ein aktiver Monitor mit stabiler ID und Anzeigename.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonitorInfo {
    pub id: String,
    pub name: String,
}

const DISPLAY_DEVICE_ACTIVE: u32 = 0x0000_0001;

/// Enumeriert alle aktuell angeschlossenen, aktiven Monitore.
pub fn enumerate() -> Vec<MonitorInfo> {
    let mut monitors = Vec::new();
    unsafe {
        let mut adapter_index = 0u32;
        loop {
            let mut adapter = DISPLAY_DEVICEW {
                cb: std::mem::size_of::<DISPLAY_DEVICEW>() as u32,
                ..Default::default()
            };
            let ok = EnumDisplayDevicesW(
                PCWSTR::null(),
                adapter_index,
                &mut adapter,
                EDD_GET_DEVICE_INTERFACE_NAME,
            )
            .as_bool();
            if !ok {
                break;
            }
            adapter_index += 1;

            if (adapter.StateFlags & DISPLAY_DEVICE_ACTIVE) == 0 {
                continue;
            }

            // Monitore an diesem Adapter (DeviceName ist nullterminiert im Puffer).
            let mut monitor_index = 0u32;
            loop {
                let mut monitor = DISPLAY_DEVICEW {
                    cb: std::mem::size_of::<DISPLAY_DEVICEW>() as u32,
                    ..Default::default()
                };
                let ok = EnumDisplayDevicesW(
                    PCWSTR(adapter.DeviceName.as_ptr()),
                    monitor_index,
                    &mut monitor,
                    EDD_GET_DEVICE_INTERFACE_NAME,
                )
                .as_bool();
                if !ok {
                    break;
                }
                monitor_index += 1;

                if (monitor.StateFlags & DISPLAY_DEVICE_ACTIVE) == 0 {
                    continue;
                }

                let id = wide_to_string(&monitor.DeviceID);
                if id.is_empty() {
                    continue;
                }
                let name = wide_to_string(&monitor.DeviceString);
                monitors.push(MonitorInfo { id, name });
            }
        }
    }
    monitors
}

/// Sortierte Liste der Monitor-IDs – praktisch zum Vergleich vor/nach einer Änderung.
pub fn current_ids() -> Vec<String> {
    let mut ids: Vec<String> = enumerate().into_iter().map(|m| m.id).collect();
    ids.sort();
    ids
}

/// Wandelt einen nullterminierten UTF-16-Puffer in einen `String`.
fn wide_to_string(buf: &[u16]) -> String {
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..len])
}
