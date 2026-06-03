//! Vereinheitlichte Initialisierung von `tracing`.
//!
//! `tracing` darf global nur **einmal** initialisiert werden. Diese Funktion baut
//! daher einen einzigen Subscriber mit bis zu zwei optionalen Ausgaben auf:
//!
//! * eine Konsolenausgabe (nur bei `--debug`), wozu im fensterlosen Normalbetrieb
//!   erst eine Konsole beschafft wird;
//! * eine Logdatei (nur bei `--log <pfad>`), die nie mehr als die letzten
//!   [`MAX_LINES`] Zeilen behält.

use std::collections::VecDeque;
use std::io::{self, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use tracing::Level;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::prelude::*;

/// Maximale Anzahl Zeilen, die die Logdatei behält. Ältere Zeilen werden gelöscht.
const MAX_LINES: usize = 1000;

/// Initialisiert das Logging. Bei `debug` wird eine Konsole beschafft und die
/// Ausgabe dorthin geleitet; bei gesetztem `log_path` wird zusätzlich in eine auf
/// [`MAX_LINES`] begrenzte Datei geloggt. Ohne beides bleibt die App stumm.
pub fn init(debug: bool, log_path: Option<&Path>) -> Result<()> {
    if debug {
        attach_console();
    }

    let stdout_layer = debug.then(|| {
        tracing_subscriber::fmt::layer()
            .with_writer(std::io::stdout)
            .with_filter(LevelFilter::from_level(Level::DEBUG))
    });

    let file_layer = match log_path {
        Some(path) => {
            let writer = CappedWriter::open(path)
                .with_context(|| format!("Logdatei öffnen: {}", path.display()))?;
            Some(
                tracing_subscriber::fmt::layer()
                    .with_ansi(false)
                    .with_writer(writer)
                    .with_filter(LevelFilter::from_level(Level::DEBUG)),
            )
        }
        None => None,
    };

    tracing_subscriber::registry()
        .with(stdout_layer)
        .with(file_layer)
        .init();
    Ok(())
}

/// Beschafft im fensterlosen Betrieb eine Konsole für die `--debug`-Ausgabe.
///
/// Bevorzugt wird an die Konsole des aufrufenden Terminals angehängt
/// (`AttachConsole(ATTACH_PARENT_PROCESS)`); andernfalls wird eine neue Konsole
/// erzeugt. Danach wird `CONOUT$` neu geöffnet und als Standard-Aus-/Fehlerkanal
/// gesetzt, damit `std::io::stdout()` zuverlässig auf die Konsole zeigt.
fn attach_console() {
    use ::windows::core::w;
    use ::windows::Win32::Foundation::{GENERIC_READ, GENERIC_WRITE, HANDLE};
    use ::windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use ::windows::Win32::System::Console::{
        AllocConsole, AttachConsole, SetStdHandle, ATTACH_PARENT_PROCESS, STD_ERROR_HANDLE,
        STD_OUTPUT_HANDLE,
    };

    unsafe {
        if AttachConsole(ATTACH_PARENT_PROCESS).is_err() {
            let _ = AllocConsole();
        }

        if let Ok(handle) = CreateFileW(
            w!("CONOUT$"),
            GENERIC_READ.0 | GENERIC_WRITE.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_FLAGS_AND_ATTRIBUTES(0),
            HANDLE::default(),
        ) {
            let _ = SetStdHandle(STD_OUTPUT_HANDLE, handle);
            let _ = SetStdHandle(STD_ERROR_HANDLE, handle);
        }
    }
}

/// Hält die letzten [`MAX_LINES`] Logzeilen im Speicher und spiegelt sie in die
/// Datei. Quelle der Wahrheit ist die `VecDeque`; bei jedem Schreibvorgang wird
/// die Datei mit höchstens [`MAX_LINES`] Zeilen neu geschrieben.
struct CappedFile {
    path: std::path::PathBuf,
    lines: VecDeque<String>,
    /// Unvollständige letzte Zeile zwischen zwei Schreibvorgängen.
    partial: String,
}

impl CappedFile {
    fn open(path: &Path) -> io::Result<Self> {
        let mut lines = VecDeque::new();
        if let Ok(text) = std::fs::read_to_string(path) {
            for line in text.lines() {
                lines.push_back(line.to_string());
                if lines.len() > MAX_LINES {
                    lines.pop_front();
                }
            }
        }
        Ok(Self {
            path: path.to_path_buf(),
            lines,
            partial: String::new(),
        })
    }

    /// Zerlegt eingehende Bytes in Zeilen und begrenzt den Puffer.
    fn ingest(&mut self, buf: &[u8]) {
        self.partial.push_str(&String::from_utf8_lossy(buf));
        while let Some(nl) = self.partial.find('\n') {
            let line: String = self.partial.drain(..=nl).collect();
            self.lines
                .push_back(line.trim_end_matches(|c| c == '\r' || c == '\n').to_string());
            if self.lines.len() > MAX_LINES {
                self.lines.pop_front();
            }
        }
    }

    /// Schreibt die (höchstens [`MAX_LINES`]) gepufferten Zeilen in die Datei.
    fn rewrite(&self) -> io::Result<()> {
        let mut f = std::fs::File::create(&self.path)?;
        for line in &self.lines {
            writeln!(f, "{line}")?;
        }
        f.flush()
    }
}

/// `MakeWriter`-Handle, das sich den begrenzten Dateipuffer teilt.
#[derive(Clone)]
struct CappedWriter(Arc<Mutex<CappedFile>>);

impl CappedWriter {
    fn open(path: &Path) -> io::Result<Self> {
        Ok(Self(Arc::new(Mutex::new(CappedFile::open(path)?))))
    }
}

/// Pro Event erzeugter Schreibgriff.
struct CappedGuard(Arc<Mutex<CappedFile>>);

impl Write for CappedGuard {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut guard = self.0.lock().unwrap();
        guard.ingest(buf);
        guard.rewrite()?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for CappedWriter {
    type Writer = CappedGuard;
    fn make_writer(&'a self) -> Self::Writer {
        CappedGuard(self.0.clone())
    }
}
