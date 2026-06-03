//! Leichtgewichtiges CLI-Argument-Parsing (ohne externe Abhängigkeit).
//!
//! Unterstützte Argumente:
//!   --debug              Konsolenausgabe aktivieren (sonst läuft die App fensterlos)
//!   --config <pfad>      alternatives Konfigurationsfile statt der Standarddatei
//!   --log <pfad>         Logdatei anlegen (begrenzt auf die letzten 1000 Zeilen)
//!
//! `--config=<pfad>` und `--log=<pfad>` werden ebenfalls akzeptiert.

use anyhow::{anyhow, bail, Result};
use std::path::PathBuf;

/// Geparste Kommandozeilenoptionen.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Cli {
    /// Konsolenausgabe aktivieren (--debug).
    pub debug: bool,
    /// Überschriebener Pfad zur Konfigurationsdatei (--config <pfad>).
    pub config: Option<PathBuf>,
    /// Pfad zur Logdatei (--log <pfad>).
    pub log: Option<PathBuf>,
}

impl Cli {
    /// Parst die Argumente des aktuellen Prozesses (ohne argv[0]).
    pub fn parse() -> Result<Cli> {
        Self::parse_from(std::env::args().skip(1))
    }

    /// Parst eine beliebige Argumentfolge (ohne argv[0]). Testbar.
    pub fn parse_from<I>(args: I) -> Result<Cli>
    where
        I: IntoIterator<Item = String>,
    {
        let mut cli = Cli::default();
        let mut it = args.into_iter();
        while let Some(arg) = it.next() {
            match arg.as_str() {
                "--debug" => cli.debug = true,
                "--config" => {
                    let v = it
                        .next()
                        .ok_or_else(|| anyhow!("--config benötigt einen Pfad"))?;
                    cli.config = Some(PathBuf::from(v));
                }
                "--log" => {
                    let v = it.next().ok_or_else(|| anyhow!("--log benötigt einen Pfad"))?;
                    cli.log = Some(PathBuf::from(v));
                }
                other if other.starts_with("--config=") => {
                    cli.config = Some(PathBuf::from(&other["--config=".len()..]));
                }
                other if other.starts_with("--log=") => {
                    cli.log = Some(PathBuf::from(&other["--log=".len()..]));
                }
                other => bail!("unbekanntes Argument: {other}"),
            }
        }
        Ok(cli)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Cli> {
        Cli::parse_from(args.iter().map(|s| s.to_string()))
    }

    #[test]
    fn empty_is_default() {
        assert_eq!(parse(&[]).unwrap(), Cli::default());
    }

    #[test]
    fn debug_flag() {
        let cli = parse(&["--debug"]).unwrap();
        assert!(cli.debug);
        assert!(cli.config.is_none());
        assert!(cli.log.is_none());
    }

    #[test]
    fn config_and_log_with_space() {
        let cli = parse(&["--config", "C:/c.toml", "--log", "C:/wm.log"]).unwrap();
        assert_eq!(cli.config, Some(PathBuf::from("C:/c.toml")));
        assert_eq!(cli.log, Some(PathBuf::from("C:/wm.log")));
    }

    #[test]
    fn config_and_log_with_equals() {
        let cli = parse(&["--config=C:/c.toml", "--log=C:/wm.log", "--debug"]).unwrap();
        assert_eq!(cli.config, Some(PathBuf::from("C:/c.toml")));
        assert_eq!(cli.log, Some(PathBuf::from("C:/wm.log")));
        assert!(cli.debug);
    }

    #[test]
    fn missing_value_errors() {
        assert!(parse(&["--config"]).is_err());
        assert!(parse(&["--log"]).is_err());
    }

    #[test]
    fn unknown_argument_errors() {
        assert!(parse(&["--nope"]).is_err());
        assert!(parse(&["foo"]).is_err());
    }
}
