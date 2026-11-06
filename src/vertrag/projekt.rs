//! Der Kopf eines Projekts in neuer Form: `projects/<slug>/projekt.toml`
//! (Zielbild, Abschnitt b, „Der Kopf: projekt.toml“). Dazu seine Teile:
//! Links, Repos, Lebenszeichen.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::lesen::Mangel;

/// Fassung des Inhaltsmodells für `projekt.toml`. Fassung 1 war page.md ohne
/// Nummer, Fassung 2 page.md mit Nummer (nie gebaut), Fassung 3 der Ordner.
pub const SCHEMA: u32 = 3;

/// `projekt.toml`. Unbekannte Felder sind ein Fehler.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Projekt {
    /// Muss 3 sein.
    pub schema: u32,
    pub title: String,
    pub status: Status,
    /// Seit wann archiviert (JJJJ-MM-TT), nur mit `status = "archiviert"`
    /// (Entscheidung archiv-status). Fehlt es, gilt das Dateidatum.
    #[serde(default)]
    pub archiviert_seit: Option<String>,
    /// Ein Satz für die Kachel.
    #[serde(default)]
    pub summary: String,
    /// Reihenfolge auf der Startseite (kleiner = weiter vorn).
    #[serde(default = "standard_order")]
    pub order: i32,
    /// Welcher Chat das Projekt verwaltet.
    #[serde(default)]
    pub chat: Option<String>,
    /// Stand der Datensicherung.
    #[serde(default)]
    pub backup: Option<BackupStand>,
    /// Kurze Begründung zum Backup-Stand.
    #[serde(default)]
    pub backup_note: String,
    /// Fertiger Entwurf, nur `assets/<name>.html`.
    #[serde(default)]
    pub vorschau: Option<String>,
    /// Offene Punkte für die Kachel.
    #[serde(default)]
    pub open: Vec<String>,
    /// Unterseiten: Datei `seiten/<name>.svelte`.
    #[serde(default)]
    pub seiten: Vec<Seite>,
    #[serde(default)]
    pub links: Vec<Link>,
    #[serde(default)]
    pub repos: Vec<RepoRef>,
    /// Kleine Lebenszeichen: HTTP-Adressen, die erreichbar sein sollen.
    #[serde(default)]
    pub checks: Vec<HttpCheck>,
    /// Programme, die laufen sollen (Dateiname, z. B. `schaufenster.exe`).
    #[serde(default)]
    pub processes: Vec<String>,
}

fn standard_order() -> i32 {
    50
}

/// Feste Wörter für `status`. `archiviert` (Entscheidung archiv-status)
/// heißt ruhig: keine Farbe, keine Ampel; das Projekt fehlt in Seitenleiste,
/// Schublade, Startseite, Cockpit, Anstehend, Glocke und allen Zählern und steht
/// nur noch in der Archivansicht `/projekte/archiviert` und auf seiner eigenen Seite.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Aktiv,
    Pausiert,
    Geplant,
    Fertig,
    Problem,
    Archiviert,
}

/// Das Wort für archivierte Projekte.
pub const ARCHIVIERT: &str = "archiviert";

/// Ist `status` (das Wort aus dem Kopf) „archiviert“?
/// Groß und klein und Leerzeichen am Rand sind egal.
pub fn ist_archiviert(status: &str) -> bool {
    status.trim().eq_ignore_ascii_case(ARCHIVIERT)
}

impl Status {
    pub fn wort(self) -> &'static str {
        match self {
            Self::Aktiv => "aktiv",
            Self::Pausiert => "pausiert",
            Self::Geplant => "geplant",
            Self::Fertig => "fertig",
            Self::Problem => "problem",
            Self::Archiviert => ARCHIVIERT,
        }
    }
}

/// Feste Wörter für `backup`: `rot`, `gelb`, `grün` (gelesen auch `gruen`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
pub enum BackupStand {
    #[serde(rename = "rot")]
    Rot,
    #[serde(rename = "gelb")]
    Gelb,
    #[serde(rename = "grün")]
    Gruen,
}

/// Von Hand statt `alias`, weil ts-rs `alias` nicht kennt.
impl<'de> Deserialize<'de> for BackupStand {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let wort = String::deserialize(d)?;
        match wort.as_str() {
            "rot" => Ok(Self::Rot),
            "gelb" => Ok(Self::Gelb),
            "grün" | "gruen" => Ok(Self::Gruen),
            _ => Err(serde::de::Error::unknown_variant(&wort, &["rot", "gelb", "grün"])),
        }
    }
}

impl BackupStand {
    pub fn wort(self) -> &'static str {
        match self {
            Self::Rot => "rot",
            Self::Gelb => "gelb",
            Self::Gruen => "grün",
        }
    }
}

/// Eine Unterseite (`[[seiten]]`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Seite {
    /// Dateiname ohne Endung: `seiten/<name>.svelte`; Kleinbuchstaben, Ziffern, Bindestriche.
    pub name: String,
    pub title: String,
    #[serde(default = "standard_order")]
    pub order: i32,
}

/// Ein lokales Git-Repo, dessen Stand angezeigt werden soll.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[serde(deny_unknown_fields)]
pub struct RepoRef {
    pub path: String,
    #[serde(default)]
    pub label: Option<String>,
    /// Bei privaten Repos zeigt das Dashboard keine Commit-Texte, nur Zahlen.
    #[serde(default)]
    pub private: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[serde(deny_unknown_fields)]
pub struct Link {
    pub name: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[serde(deny_unknown_fields)]
pub struct HttpCheck {
    pub name: String,
    pub url: String,
}

/// Name einer Unterseite: wie ein Slug.
pub fn ist_seitenname(name: &str) -> bool {
    !name.is_empty() && name.len() <= 64 && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') && !name.starts_with('-')
}

impl Projekt {
    /// Was serde nicht prüfen kann. `vorschau` prüft der Aufrufer (seite.rs).
    pub fn pruefen(&self) -> Result<(), Mangel> {
        if self.schema != SCHEMA {
            return Err(Mangel::bei(
                "schema",
                &self.schema.to_string(),
                format!("Bei `schema` steht {}. Für projekt.toml gilt `schema = {SCHEMA}`.", self.schema),
            ));
        }
        if self.title.trim().is_empty() {
            return Err(Mangel::bei("title", "", "`title` ist leer. Beispiel: `title = \"Beispiel A\"`"));
        }
        if let Some(s) = self.archiviert_seit.as_deref()
            && chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").is_err()
        {
            return Err(Mangel::bei(
                "archiviert_seit",
                s,
                format!("Bei `archiviert_seit` steht „{s}“. Erlaubt ist ein Datum wie `archiviert_seit = \"2026-11-01\"`."),
            ));
        }
        let mut namen = std::collections::BTreeSet::new();
        for s in &self.seiten {
            if !ist_seitenname(&s.name) {
                return Err(Mangel::bei(
                    "name",
                    &s.name,
                    format!(
                        "Die Unterseite „{}“ hat keinen einfachen Namen. Erlaubt sind Kleinbuchstaben, Ziffern und Bindestriche, etwa `name = \"beet-a\"`.",
                        s.name
                    ),
                ));
            }
            if !namen.insert(s.name.as_str()) {
                return Err(Mangel::beim_zweiten("name", &s.name, format!("Die Unterseite „{}“ steht doppelt da.", s.name)));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vertrag::lesen::toml_lesen;

    const GUT: &str = r#"schema = 3
title = "Beispiel A"
status = "aktiv"
summary = "Ein Satz für die Kachel."
order = 30
chat = "Beispiel A"
backup = "grün"
backup_note = "nur auf C:"
vorschau = "assets/vorschau.html"
open = ["Offener Punkt"]

[[seiten]]
name = "beet-a"
title = "Beet A: Hochbeete"
order = 10

[[links]]
name = "Ordner"
url = "D:/Ablage/Projekte/beispiel-a"

[[repos]]
path = "D:/Ablage/dev/beispiel-a"
label = "beispiel-a"
"#;

    #[test]
    fn guter_kopf() {
        let p: Projekt = toml_lesen("projects/n/projekt.toml", GUT).unwrap();
        assert_eq!(p.status, Status::Aktiv);
        assert_eq!(p.backup, Some(BackupStand::Gruen));
        assert_eq!(p.seiten[0].name, "beet-a");
        assert_eq!(p.repos[0].label.as_deref(), Some("beispiel-a"));
        assert!(p.pruefen().is_ok());
        let p: Projekt = toml_lesen("x", "schema = 3\ntitle = \"A\"\nstatus = \"fertig\"\nbackup = \"gruen\"\n").unwrap();
        assert_eq!(p.order, 50);
        assert_eq!(p.backup, Some(BackupStand::Gruen));
        assert_eq!(serde_json::to_value(p.backup).unwrap(), "grün");
    }

    #[test]
    fn falscher_status_mit_zeile() {
        let text = "schema = 3\ntitle = \"Beispiel A\"\nstatus = \"aktief\"\n";
        let e = toml_lesen::<Projekt>("projects/beispiel-a/projekt.toml", text).unwrap_err();
        assert_eq!(
            e.to_string(),
            "projects/beispiel-a/projekt.toml, Zeile 3: Bei `status` steht „aktief“. Erlaubt sind: aktiv, pausiert, geplant, fertig, problem, archiviert."
        );
    }

    #[test]
    fn unbekanntes_feld_und_falsches_schema() {
        let e = toml_lesen::<Projekt>("p", "schema = 3\ntitle = \"A\"\nstatus = \"aktiv\"\nupdated = \"2026\"\n").unwrap_err();
        assert_eq!(e.zeile, Some(4));
        assert!(e.text.starts_with("Das Feld `updated` gibt es hier nicht."), "{e}");
        let e = toml_lesen::<Projekt>("p", "schema = 3\ntitle = \"A\"\nstatus = \"aktiv\"\nbackup = \"blau\"\n").unwrap_err();
        assert!(e.text.contains("Erlaubt sind: rot, gelb, grün"), "{e}");
        let p: Projekt = toml_lesen("p", "schema = 2\ntitle = \"A\"\nstatus = \"aktiv\"\n").unwrap();
        let m = p.pruefen().unwrap_err().in_datei("p", "schema = 2\n");
        assert_eq!(m.to_string(), "p, Zeile 1: Bei `schema` steht 2. Für projekt.toml gilt `schema = 3`.");
        let e = toml_lesen::<Projekt>("p", "title = \"A\"\nstatus = \"aktiv\"\n").unwrap_err();
        assert!(e.text.contains("fehlt `schema`. Beispiel: `schema = 3`"), "{e}");
    }
}
