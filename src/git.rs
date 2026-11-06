//! Liest den Stand lokaler Git-Repos über das `git`-Programm, immer mit vollem
//! Pfad aus der Git-Installation (`dienst::programme`), nie über die PATH-Suche.
//!
//! Jedes Repo bekommt ein eigenes Budget von 3 s für alle drei Abfragen
//! zusammen; hängt es, fehlt nur seine Zeile, nicht die ganze Projektliste.
//! Hängende Prozesse werden beim Abbruch beendet (`kill_on_drop`).
//!
//! Die API fragt git nie selbst, sie liest den Zwischenspeicher
//! (`git_zwischenspeicher`), der im Hintergrund erneuert.

use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Duration;
use ts_rs::TS;

use tokio::process::Command;

use crate::dienst::programme::{self, Werkzeug};

pub use crate::git_zwischenspeicher::Zwischenspeicher;

/// Budget je Repo (Zielbild, Kapitel c: „git 3 s je Repo“).
pub const BUDGET: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct RepoStatus {
    pub path: String,
    pub label: String,
    pub private: bool,
    pub branch: Option<String>,
    pub commit_hash: Option<String>,
    pub commit_date: Option<String>,
    pub commit_subject: Option<String>,
    /// Anzahl geänderter oder neuer Dateien laut `git status --porcelain`.
    pub dirty_files: Option<usize>,
    pub error: Option<String>,
    /// Wann dieser Stand ermittelt wurde (RFC 3339), gesetzt vom
    /// Zwischenspeicher (`git_zwischenspeicher`). `None` nur, solange er das
    /// Repo noch nicht abgefragt hat; die Oberfläche sagt dann „wird ermittelt“.
    pub stand: Option<String>,
}

/// Schalter vor jedem Unterbefehl: nur lesen, nichts aus dem Repo ausführen.
/// Kein Pager; keine optionalen Sperren (status schreibt den Index nicht);
/// kein fsmonitor (würde ein Programm aus der Repo-Konfiguration starten);
/// `core.hooksPath` leer: keine Hooks, auch nicht aus `.git/hooks` (geprüft
/// mit git 2.56 unter Windows: ein leerer Wert führt keinen Hook aus).
pub const SCHALTER: [&str; 6] = ["--no-pager", "--no-optional-locks", "-c", "core.fsmonitor=false", "-c", "core.hooksPath="];

/// Nur für Tests: Der Beispiel-Ordner `haengt` steht für ein Repo, dessen git
/// nicht antwortet; statt git läuft dann ein Schlaf-Befehl (prüft das Budget).
/// Fehlt git an den festen Orten, ein Satz statt eines Befehls.
fn kommando(dir: &Path, args: &[&str]) -> Result<Command, String> {
    #[cfg(test)]
    if dir.file_name().is_some_and(|n| n == "haengt") {
        let mut c = programme::befehl_async(Werkzeug::WindowsPowerShell)?;
        c.args(["-NoProfile", "-NonInteractive", "-Command", "Start-Sleep -Seconds 8"]);
        c.kill_on_drop(true);
        return Ok(c);
    }
    // Fester Pfad und ohne Fenster (dienst::programme).
    let mut c = programme::befehl_async(Werkzeug::Git)?;
    c.args(SCHALTER).arg("-C").arg(dir).args(args).env("GIT_OPTIONAL_LOCKS", "0").env("GIT_PAGER", "cat").env("GIT_TERMINAL_PROMPT", "0");
    c.kill_on_drop(true);
    Ok(c)
}

async fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let out = kommando(dir, args)?.output().await.map_err(|e| format!("git lässt sich nicht starten: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim_end().to_string())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// Fragt Branch, letzten Commit und ungesicherte Änderungen ab, zusammen
/// höchstens `BUDGET` lang.
pub async fn inspect(path: &str, label: Option<&str>, private: bool) -> RepoStatus {
    let mut status = leer(path, label, private);
    let dir = Path::new(path);
    if !dir.is_dir() {
        status.error = Some("Ordner nicht gefunden".to_string());
        return status;
    }
    match tokio::time::timeout(BUDGET, abfragen(dir, private, &mut status)).await {
        Ok(()) => {}
        Err(_) => {
            tracing::warn!("git in {path} antwortet nicht innerhalb von {BUDGET:?}");
            status.error = Some("git antwortet nicht (Zeitlimit)".to_string());
        }
    }
    status
}

async fn abfragen(dir: &Path, private: bool, status: &mut RepoStatus) {
    match git(dir, &["rev-parse", "--abbrev-ref", "HEAD"]).await {
        Ok(b) => status.branch = Some(b),
        Err(e) => {
            status.error = Some(if e.contains("not a git repository") { "kein Git-Repo".to_string() } else { e });
            return;
        }
    }
    if let Ok(line) = git(dir, &["log", "-1", "--format=%h%x1f%cs%x1f%s"]).await {
        let mut parts = line.split('\u{1f}');
        status.commit_hash = parts.next().map(str::to_string);
        status.commit_date = parts.next().map(str::to_string);
        status.commit_subject = if private { None } else { parts.next().map(str::to_string) };
    }
    match git(dir, &["status", "--porcelain", "--untracked-files=normal"]).await {
        Ok(s) => status.dirty_files = Some(s.lines().filter(|l| !l.is_empty()).count()),
        Err(e) => status.error = Some(e),
    }
}

fn display_name(path: &str) -> String {
    Path::new(path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| path.to_string())
}

/// Ein Repo über den Boten (der Dienst hat keine Leserechte auf die Repos des Inhabers
/// und bekäme „dubious ownership“): dort mit Budget 3 s, hier mit drei
/// Sekunden Spielraum für die Pipe und das Warten auf einen freien Platz (bei
/// vielen Repos zugleich stehen einige kurz in der Schlange,
/// `bote::client::PLAETZE`).
pub async fn ueber_boten(bote: &crate::bote::client::Bote, path: &str, label: Option<&str>, private: bool) -> RepoStatus {
    match ueber_boten_oder_weg(bote, path, label, private).await {
        Ok(s) => s,
        Err(weg) => RepoStatus { error: Some(format!("Git-Stand unbekannt: {}", weg.satz)), ..leer(path, label, private) },
    }
}

/// Der Bote hat keinen Stand geliefert.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoteWeg {
    pub satz: String,
    /// Der Bote ist da, aber alle Verbindungen waren belegt oder er war zu
    /// langsam (`Lage::Ueberlastet`): nur eine Verzögerung, kein „Bote weg“.
    pub ausgelastet: bool,
}

/// Wie `ueber_boten`, aber `Err`, wenn der Bote nicht erreichbar,
/// ausgelastet oder zu langsam war: Dann weiß niemand etwas Neues über das
/// Repo, und der Zwischenspeicher behält den letzten Stand. Ein Fehler von
/// git selbst ist dagegen ein Stand und kommt als `Ok` mit `error`.
pub async fn ueber_boten_oder_weg(bote: &crate::bote::client::Bote, path: &str, label: Option<&str>, private: bool) -> Result<RepoStatus, BoteWeg> {
    use crate::vertrag::bote::{Anfrage, Antwort};
    let mit_fehler = |fehler: String| RepoStatus { error: Some(fehler), ..leer(path, label, private) };
    let anfrage = Anfrage::GitStatus { pfad: path.to_string(), privat: private };
    match bote.anfragen_mit_lage(&anfrage, BUDGET + Duration::from_secs(3)).await {
        Ok(Antwort::GitStatus(mut s)) => {
            s.label = label.map(str::to_string).unwrap_or_else(|| display_name(path));
            s.private = private;
            if private {
                s.commit_subject = None;
            }
            Ok(s)
        }
        Ok(Antwort::Fehler { satz }) => Ok(mit_fehler(satz)),
        Ok(anders) => Ok(mit_fehler(format!("Der Bote hat unpassend geantwortet ({}).", anders.art()))),
        Err((satz, lage)) => Err(BoteWeg { satz: satz.0, ausgelastet: lage == crate::bote::client::Lage::Ueberlastet }),
    }
}

/// Eine Zeile ohne jeden Git-Stand (noch nicht abgefragt).
pub fn leer(path: &str, label: Option<&str>, private: bool) -> RepoStatus {
    RepoStatus {
        path: path.to_string(),
        label: label.map(str::to_string).unwrap_or_else(|| display_name(path)),
        private,
        branch: None,
        commit_hash: None,
        commit_date: None,
        commit_subject: None,
        dirty_files: None,
        error: None,
        stand: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn missing_dir_reports_error() {
        let s = inspect("C:/gibt/es/nicht", None, false).await;
        assert_eq!(s.error.as_deref(), Some("Ordner nicht gefunden"));
        assert_eq!(s.label, "nicht");
    }

    #[tokio::test]
    async fn this_repo_has_a_branch() {
        let here = env!("CARGO_MANIFEST_DIR");
        let s = inspect(here, Some("hier"), true).await;
        assert!(s.error.is_none(), "{:?}", s.error);
        assert!(s.branch.is_some());
        assert!(s.commit_subject.is_none(), "private Repos zeigen keine Texte");
    }

    /// Die Schalter stehen vor `-C` und dem Unterbefehl, git nimmt sie an.
    #[test]
    fn schalter_vor_dem_unterbefehl() {
        let c = kommando(Path::new(env!("CARGO_MANIFEST_DIR")), &["status"]).unwrap();
        let args: Vec<String> = c.as_std().get_args().map(|a| a.to_string_lossy().to_string()).collect();
        assert_eq!(&args[..6], SCHALTER);
        assert_eq!(args[6], "-C");
        assert_eq!(args.last().map(String::as_str), Some("status"));
        // Voller Pfad, keine PATH-Suche.
        let programm = Path::new(c.as_std().get_program());
        assert!(programm.is_absolute(), "{}", programm.display());
        assert_eq!(Some(programm.to_path_buf()), programme::pfad_von(Werkzeug::Git));
        let out = programme::befehl(Werkzeug::Git).unwrap().args(SCHALTER).arg("--version").output().unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    }

    /// Ein hängendes git hält höchstens 3 s auf und hinterlässt eine Fehlerzeile.
    #[tokio::test]
    async fn hanging_git_is_cut_off_after_budget() {
        let haengt = format!("{}/tests/beispiel/haengt", env!("CARGO_MANIFEST_DIR"));
        let start = std::time::Instant::now();
        let s = inspect(&haengt, Some("haengt"), false).await;
        let dauer = start.elapsed();
        assert!(dauer >= BUDGET && dauer < BUDGET + Duration::from_secs(2), "{dauer:?}");
        assert_eq!(s.error.as_deref(), Some("git antwortet nicht (Zeitlimit)"));
        assert!(s.branch.is_none());
    }
}
