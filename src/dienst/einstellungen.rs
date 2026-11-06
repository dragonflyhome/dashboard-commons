//! Die Einstellungsdatei des Dienstes (`dienst.toml`, Etappe 7a). Sie ersetzt
//! im Dienst die Umgebungsvariablen: Repo, Port und alle Pfade stehen hier.
//! Das Einrichtungsskript schreibt sie nach `C:/ProgramData/Dashboard/dienst.toml`;
//! ein Beispiel liegt in `tests/beispiel/dienst.toml` (wird von keinem Test geladen).
//!
//! Seit dem Umzug des Inhalts (Entscheidung `github-trennung`,
//! [docs_oeffentlich]) liegt der private Inhalt außerhalb des Code-Repos. Der
//! freiwillige Abschnitt `[inhalt]` nennt ihn:
//!
//! ```toml
//! [inhalt]
//! projekte = ["C:/Users/<name>/dev/dashboard-inhalt/projects"]   # Liste der Inhaltsquellen
//! daten = "C:/Users/<name>/dev/dashboard-inhalt/data"             # pc.toml, schluessel.toml, …
//! entscheidungen = "C:/Users/<name>/dev/dashboard-inhalt/docs/entscheidungen"
//! ```
//!
//! Ohne ihn gilt alles wie vorher unter `repo` (`projects`, `data`,
//! `docs/entscheidungen`). `DASHBOARD_PROJECTS_DIR` (Liste mit `;`) ersetzt
//! weiter die Liste der Quellen ([`Einstellungen::projekt_quellen`]), für
//! Dienst, Konsole und Bote gleich.
//!
//! Fehler kommen als ein Satz in einfachem Deutsch mit Datei und Zeile.

use std::fmt;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::vertrag::lesen;

/// Fassung der Datei, die dieses Programm kennt.
pub const SCHEMA: u32 = 1;

/// Ein Satz für den Inhaber: was an der Einstellungsdatei nicht stimmt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Satz(pub String);

impl fmt::Display for Satz {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Satz {}

/// Die gelesenen und geprüften Einstellungen.
#[derive(Debug, Clone)]
pub struct Einstellungen {
    /// Die Datei, aus der sie stammen.
    pub datei: PathBuf,
    /// Arbeitsordner: `projects/`, `data/`, `web/build`.
    pub repo: PathBuf,
    pub port: u16,
    pub pfade: Pfade,
    /// Der private Inhalt (Abschnitt `[inhalt]`, sonst im Repo).
    pub inhalt: Inhalt,
    pub konten: Konten,
    pub bote: Bote,
}

/// Wo der private Inhalt liegt (Umzug des Inhalts).
#[derive(Debug, Clone)]
pub struct Inhalt {
    /// Die Inhaltsquellen in ihrer Reihenfolge (je ein Ordner mit Projekten
    /// oder ein einzelnes Projekt); Standard `[<repo>/projects]` oder
    /// `pfade.projekte`. Nie leer.
    pub projekte: Vec<PathBuf>,
    /// Ordner mit `pc.toml`, `schluessel.toml`, `schwerpunkte.toml` und
    /// `inventar-ausschluss.toml`; Standard `<repo>/data`.
    pub daten: PathBuf,
    /// Die Entscheidungen (`/api/entscheidungen`, privat); Standard
    /// `<repo>/docs/entscheidungen`.
    pub entscheidungen: PathBuf,
    /// Steht in der Datei ein Abschnitt `[inhalt]` mit `projekte`? (Sonst gilt
    /// alles wie vor dem Umzug im Repo.) Für `/api/betrieb/voll` und den
    /// zweiten Durchgang des Umzugs.
    pub abschnitt: bool,
}

/// Name der Ausschlussliste des Inventars im Daten-Ordner des Inhalts.
pub const AUSSCHLUSS_DATEI: &str = "inventar-ausschluss.toml";

#[derive(Debug, Clone)]
pub struct Pfade {
    /// Datei des Datenspeichers.
    pub daten: PathBuf,
    /// Ordner der täglichen Sicherungen.
    pub sicherung: PathBuf,
    /// Ordner der Protokolle (`dienst-JJJJ-MM-TT.log`, `bote-…`).
    pub protokoll: PathBuf,
    /// Fertige Oberfläche; Standard `<repo>/web/build`.
    pub web: PathBuf,
    /// Die erste Inhaltsquelle (`inhalt.projekte[0]`, früher allein
    /// `pfade.projekte`); Standard `<repo>/projects`.
    pub projekte: PathBuf,
}

#[derive(Debug, Clone, Default)]
pub struct Konten {
    /// SID des Inhaber-Kontos; vom Einrichtungsskript eingetragen, leer erlaubt (7a1).
    pub inhaber_sid: String,
    /// SID des Dienstkontos (`NT SERVICE\Dashboard`, Form `S-1-5-80-…`, vom
    /// Einrichtungsskript aus `sc.exe showsid` eingetragen). Der Bote gibt ihr
    /// Lesen und Schreiben an der Pipe (ohne Instanzen anzulegen) und erlaubt
    /// ihr, seinen Prozess und sein Token abzufragen, damit der Dienst ihn
    /// prüfen kann. Leer: nur der Inhaber darf an die Pipe (dann kann der Dienst
    /// nicht verbinden; der Bote warnt im Protokoll).
    pub dienst_sid: String,
}

#[derive(Debug, Clone)]
pub struct Bote {
    /// Name der Pipe ohne `\\.\pipe\`.
    pub pipe: String,
    /// Wo das Programm des Boten liegen muss.
    pub programm: PathBuf,
}

/// Hat `s` die Form einer SID, `^S-1-\d+(-\d+)+$` (Handprüfung, ohne Regex):
/// „S-1-“, dann mindestens zwei Zahlen, getrennt durch Bindestriche.
pub fn ist_sid(s: &str) -> bool {
    let Some(rest) = s.strip_prefix("S-1-") else { return false };
    let teile: Vec<&str> = rest.split('-').collect();
    teile.len() >= 2 && teile.iter().all(|t| !t.is_empty() && t.bytes().all(|b| b.is_ascii_digit()))
}

// ----- Form der Datei -----

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Roh {
    schema: i64,
    repo: String,
    port: i64,
    pfade: RohPfade,
    #[serde(default)]
    inhalt: RohInhalt,
    #[serde(default)]
    konten: RohKonten,
    #[serde(default)]
    bote: RohBote,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct RohInhalt {
    projekte: Option<Vec<String>>,
    daten: Option<String>,
    entscheidungen: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RohPfade {
    daten: String,
    sicherung: String,
    protokoll: String,
    web: Option<String>,
    projekte: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct RohKonten {
    #[serde(default)]
    inhaber_sid: String,
    #[serde(default)]
    dienst_sid: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RohBote {
    #[serde(default = "standard_pipe")]
    pipe: String,
    #[serde(default = "standard_bote_programm")]
    programm: String,
}

impl Default for RohBote {
    fn default() -> Self {
        Self { pipe: standard_pipe(), programm: standard_bote_programm() }
    }
}

fn standard_pipe() -> String {
    "dashboard-bote".to_string()
}

fn standard_bote_programm() -> String {
    "C:/ProgramData/Dashboard/bin/dashboard-bote.exe".to_string()
}

impl Einstellungen {
    /// Liest und prüft die Datei.
    pub fn laden(pfad: &Path) -> Result<Einstellungen, Satz> {
        let anzeige = pfad.display().to_string();
        let text = match std::fs::read_to_string(pfad) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(Satz(format!(
                    "Die Einstellungsdatei {anzeige} fehlt. Das Einrichtungsskript legt sie an; ein Beispiel steht in tests/beispiel/dienst.toml."
                )));
            }
            Err(e) => return Err(Satz(format!("Die Einstellungsdatei {anzeige} lässt sich nicht lesen ({e})."))),
        };
        Self::aus_text(pfad, &text)
    }

    /// Wie `laden`, aber mit dem Text schon in der Hand.
    pub fn aus_text(pfad: &Path, text: &str) -> Result<Einstellungen, Satz> {
        let anzeige = pfad.display().to_string();
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        let roh: Roh = lesen::toml_lesen(&anzeige, text).map_err(|e| Satz(e.to_string()))?;
        let fehler = |schluessel: &str, satz: String| {
            let zeile = lesen::zeile_mit_schluessel(text, schluessel.rsplit('.').next().unwrap_or(schluessel));
            Satz(match zeile {
                Some(z) => format!("{anzeige}, Zeile {z}: {satz}"),
                None => format!("{anzeige}: {satz}"),
            })
        };

        if roh.schema != SCHEMA as i64 {
            return Err(fehler("schema", format!("Bei `schema` steht {}. Dieses Programm kennt nur `schema = {SCHEMA}`.", roh.schema)));
        }
        if !(1..=65535).contains(&roh.port) {
            return Err(fehler("port", format!("Bei `port` steht {}. Erlaubt ist eine Zahl von 1 bis 65535, etwa `port = 8787`.", roh.port)));
        }
        let absolut = |schluessel: &str, wert: &str, beispiel: &str| -> Result<PathBuf, Satz> {
            let p = PathBuf::from(wert);
            if wert.trim().is_empty() || !p.is_absolute() {
                return Err(fehler(
                    schluessel,
                    format!("Bei `{schluessel}` steht „{wert}“. Bitte einen vollständigen Pfad mit Laufwerk angeben, etwa „{beispiel}“."),
                ));
            }
            Ok(p)
        };
        let repo = absolut("repo", &roh.repo, "C:/Users/<name>/dev/dashboard")?;
        if !repo.is_dir() {
            return Err(fehler("repo", format!("Bei `repo` steht „{}“, aber diesen Ordner gibt es nicht.", roh.repo)));
        }
        let daten = absolut("pfade.daten", &roh.pfade.daten, "C:/ProgramData/Dashboard/daten/dashboard.redb")?;
        let sicherung = absolut("pfade.sicherung", &roh.pfade.sicherung, "C:/ProgramData/Dashboard/sicherung")?;
        let protokoll = absolut("pfade.protokoll", &roh.pfade.protokoll, "C:/ProgramData/Dashboard/protokoll")?;
        let web = match &roh.pfade.web {
            Some(w) => absolut("pfade.web", w, "C:/Users/<name>/dev/dashboard/web/build")?,
            None => repo.join("web").join("build"),
        };
        let quellen = match (&roh.pfade.projekte, &roh.inhalt.projekte) {
            (Some(_), Some(_)) => {
                return Err(fehler(
                    "inhalt.projekte",
                    "Die Projekte stehen zweimal da, bei `pfade.projekte` und bei `inhalt.projekte`. Bitte nur `inhalt.projekte` behalten.".to_string(),
                ));
            }
            (Some(p), None) => vec![absolut("pfade.projekte", p, "C:/Users/<name>/dev/dashboard-inhalt/projects")?],
            (None, Some(liste)) if liste.is_empty() => {
                return Err(fehler(
                    "inhalt.projekte",
                    "Bei `projekte` steht eine leere Liste. Bitte mindestens einen Ordner nennen, etwa [\"C:/Users/<name>/dev/dashboard-inhalt/projects\"]."
                        .to_string(),
                ));
            }
            (None, Some(liste)) => {
                liste.iter().map(|p| absolut("inhalt.projekte", p, "C:/Users/<name>/dev/dashboard-inhalt/projects")).collect::<Result<Vec<_>, _>>()?
            }
            (None, None) => vec![repo.join("projects")],
        };
        let projekte = quellen[0].clone();
        let inhalt_daten = match &roh.inhalt.daten {
            Some(d) => absolut("inhalt.daten", d, "C:/Users/<name>/dev/dashboard-inhalt/data")?,
            None => repo.join("data"),
        };
        let entscheidungen = match &roh.inhalt.entscheidungen {
            Some(d) => absolut("inhalt.entscheidungen", d, "C:/Users/<name>/dev/dashboard-inhalt/docs/entscheidungen")?,
            None => repo.join("docs").join("entscheidungen"),
        };
        let sid_lesen = |schluessel: &str, wert: &str| -> Result<String, Satz> {
            let sid = wert.trim().to_string();
            let kurz = schluessel.rsplit('.').next().unwrap_or(schluessel);
            if !sid.is_empty() && !ist_sid(&sid) {
                return Err(fehler(
                    schluessel,
                    format!(
                        "Bei `{kurz}` steht keine SID. Eine SID hat die Form „S-1-5-21-…“: „S-1-“ und danach nur Zahlen, \
                         getrennt durch Bindestriche; leer lassen ist auch erlaubt."
                    ),
                ));
            }
            Ok(sid)
        };
        let sid = sid_lesen("konten.inhaber_sid", &roh.konten.inhaber_sid)?;
        let dienst_sid = sid_lesen("konten.dienst_sid", &roh.konten.dienst_sid)?;
        let pipe = roh.bote.pipe.trim().to_string();
        if pipe.is_empty() || pipe.contains(['\\', '/']) {
            return Err(fehler("bote.pipe", format!("Bei `pipe` steht „{pipe}“. Bitte nur den Namen ohne Schrägstriche, etwa „dashboard-bote“.")));
        }
        let programm = absolut("bote.programm", &roh.bote.programm, "C:/ProgramData/Dashboard/bin/dashboard-bote.exe")?;

        Ok(Einstellungen {
            datei: pfad.to_path_buf(),
            repo,
            port: roh.port as u16,
            pfade: Pfade { daten, sicherung, protokoll, web, projekte },
            inhalt: Inhalt { projekte: quellen, daten: inhalt_daten, entscheidungen, abschnitt: roh.inhalt.projekte.is_some() },
            konten: Konten { inhaber_sid: sid, dienst_sid },
            bote: Bote { pipe, programm },
        })
    }

    /// Eine Zeile fürs Protokoll beim Start. Die SID selbst steht nicht darin.
    pub fn kurz(&self) -> String {
        let sid = if self.konten.inhaber_sid.is_empty() { "fehlt noch" } else { "eingetragen" };
        format!(
            "Port {}, Repo {}, Bote: Pipe {} und Programm {}, Inhaber-SID {sid}",
            self.port,
            self.repo.display(),
            self.bote.pipe,
            self.bote.programm.display()
        )
    }

    /// Zwischenspeicher der Ordnergrößen: `inventar.json` neben dem
    /// Datenspeicher (`pfade.daten`), nicht im Profil des Dienstkontos.
    pub fn inventar_datei(&self) -> PathBuf {
        self.pfade.daten.parent().map(Path::to_path_buf).unwrap_or_default().join("inventar.json")
    }

    /// Schlüssel der Ausweise nach Anmeldung mit Passkey (Etappe 7c):
    /// `geheim/jwt-schluessel` neben dem Ordner des Datenspeichers, im Dienst
    /// also `C:/ProgramData/Dashboard/geheim/jwt-schluessel`. Der Dienst legt
    /// die Datei beim ersten Start an (nur Dienstkonto und Administratoren).
    pub fn jwt_schluessel_datei(&self) -> PathBuf {
        let ordner = self.pfade.daten.parent().unwrap_or(Path::new("."));
        ordner.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(ordner).join("geheim").join("jwt-schluessel")
    }

    /// `pc.toml` im Daten-Ordner des Inhalts (Standard `<repo>/data`).
    pub fn pc_datei(&self) -> PathBuf {
        self.inhalt.daten.join("pc.toml")
    }

    /// `schluessel.toml` im Daten-Ordner des Inhalts.
    pub fn schluessel_datei(&self) -> PathBuf {
        self.inhalt.daten.join("schluessel.toml")
    }

    /// Die Ausschlussliste des Inventars im Daten-Ordner des Inhalts.
    pub fn ausschluss_datei(&self) -> PathBuf {
        self.inhalt.daten.join(AUSSCHLUSS_DATEI)
    }

    /// Die Inhaltsquellen: `DASHBOARD_PROJECTS_DIR` (Liste mit `;`), wenn es
    /// gesetzt und nicht leer ist, sonst `inhalt.projekte`. Dienst, Konsole
    /// und Bote nehmen diese eine Stelle.
    pub fn projekt_quellen(&self) -> Vec<PathBuf> {
        let ersatz: Vec<PathBuf> =
            std::env::var_os("DASHBOARD_PROJECTS_DIR").map(|t| crate::quellen::Quellen::aus_text(&t.to_string_lossy()).wurzeln()).unwrap_or_default();
        if ersatz.is_empty() { self.inhalt.projekte.clone() } else { ersatz }
    }

    /// Alle Ordner des Inhalts, die Dienst und Bote lesen: die Quellen, der
    /// Daten-Ordner und die Entscheidungen (für die Lese-Prüfung beim Start).
    pub fn inhalt_ordner(&self) -> Vec<PathBuf> {
        let mut v = self.projekt_quellen();
        v.push(self.inhalt.daten.clone());
        v.push(self.inhalt.entscheidungen.clone());
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ein eigener Temp-Ordner je Test, mit einem Repo-Ordner darin.
    struct Ort {
        ordner: PathBuf,
    }

    impl Ort {
        fn neu(name: &str) -> Ort {
            let ordner = std::env::temp_dir().join(format!("dashboard-einst-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&ordner);
            std::fs::create_dir_all(ordner.join("repo")).unwrap();
            Ort { ordner }
        }

        fn p(&self, rest: &str) -> String {
            self.ordner.join(rest).display().to_string().replace('\\', "/")
        }

        fn gut(&self) -> String {
            format!(
                "schema = 1\nrepo = \"{}\"\nport = 8787\n\n[pfade]\ndaten = \"{}\"\nsicherung = \"{}\"\nprotokoll = \"{}\"\n\n[konten]\ninhaber_sid = \"\"\n\n[bote]\npipe = \"dashboard-bote\"\nprogramm = \"C:/ProgramData/Dashboard/bin/dashboard-bote.exe\"\n",
                self.p("repo"),
                self.p("daten/dashboard.redb"),
                self.p("sicherung"),
                self.p("protokoll"),
            )
        }

        fn laden(&self, text: &str) -> Result<Einstellungen, Satz> {
            let datei = self.ordner.join("dienst.toml");
            std::fs::write(&datei, text).unwrap();
            Einstellungen::laden(&datei)
        }
    }

    impl Drop for Ort {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.ordner);
        }
    }

    #[test]
    fn gute_datei_mit_standardpfaden() {
        let ort = Ort::neu("gut");
        let e = ort.laden(&ort.gut()).unwrap();
        assert_eq!(e.port, 8787);
        assert_eq!(e.pfade.web, e.repo.join("web").join("build"));
        assert_eq!(e.pfade.projekte, e.repo.join("projects"));
        assert_eq!(e.inhalt.projekte, vec![e.repo.join("projects")]);
        assert_eq!(e.pc_datei(), e.repo.join("data").join("pc.toml"));
        assert_eq!(e.schluessel_datei(), e.repo.join("data").join("schluessel.toml"));
        assert_eq!(e.bote.pipe, "dashboard-bote");
        assert!(e.konten.inhaber_sid.is_empty());
        // Ohne [konten] und [bote] gelten die Standards.
        let ohne = ort.gut().split("\n[konten]").next().unwrap().to_string();
        let e = ort.laden(&ohne).unwrap();
        assert_eq!(e.bote.programm, PathBuf::from("C:/ProgramData/Dashboard/bin/dashboard-bote.exe"));
    }

    #[test]
    fn sids_lesen_und_pruefen() {
        let ort = Ort::neu("sids");
        let text = ort.gut().replace("inhaber_sid = \"\"", "inhaber_sid = \"S-1-5-21-1-2\"\ndienst_sid = \" S-1-5-80-9 \"");
        let e = ort.laden(&text).unwrap();
        assert_eq!(e.konten.inhaber_sid, "S-1-5-21-1-2");
        assert_eq!(e.konten.dienst_sid, "S-1-5-80-9");
        assert!(ort.laden(&ort.gut()).unwrap().konten.dienst_sid.is_empty(), "fehlt: leer");
        let text = ort.gut().replace("inhaber_sid = \"\"", "inhaber_sid = \"\"\ndienst_sid = \"Dashboard\"");
        let e = ort.laden(&text).unwrap_err();
        assert!(e.0.contains("Bei `dienst_sid` steht keine SID."), "{e}");
        let text = ort.gut().replace("inhaber_sid = \"\"", "inhaber_sid = \"S-1-5-21-1-\"");
        let e = ort.laden(&text).unwrap_err();
        assert!(e.0.contains("Bei `inhaber_sid` steht keine SID."), "{e}");
    }

    #[test]
    fn sid_form_streng() {
        for gut in ["S-1-5-18", "S-1-5-21-1-2-3-1001", "S-1-5-80-0-0-0-0-0", "S-1-16-12288"] {
            assert!(ist_sid(gut), "{gut}");
        }
        for schlecht in [
            "",
            "S-1",
            "S-1-",
            "S-1-5",
            "S-1-5-",
            "S-1-5--18",
            "S-1-5-18-",
            "s-1-5-18",
            "S-2-5-18",
            "S-1-5-x",
            "S-1-5-18 ",
            " S-1-5-18",
            "S-1-5-1８",
            "S-1-5-18;D:(A;;GA;;;WD)",
            "Dashboard",
        ] {
            assert!(!ist_sid(schlecht), "{schlecht:?}");
        }
    }

    /// Umzug des Inhalts: `[inhalt]` nennt Quellen, Daten und Entscheidungen
    /// außerhalb des Repos; ohne den Abschnitt gilt alles im Repo.
    #[test]
    fn inhalt_ausserhalb_des_repos() {
        let ort = Ort::neu("inhalt");
        let e = ort.laden(&ort.gut()).unwrap();
        assert_eq!(e.inhalt.projekte, vec![e.repo.join("projects")]);
        assert_eq!(e.inhalt.daten, e.repo.join("data"));
        assert_eq!(e.inhalt.entscheidungen, e.repo.join("docs").join("entscheidungen"));
        assert_eq!(e.ausschluss_datei(), e.repo.join("data").join(AUSSCHLUSS_DATEI));
        assert!(!e.inhalt.abschnitt, "ohne [inhalt]");
        let abschnitt = format!(
            "\n[inhalt]\nprojekte = [\"{}\", \"{}\"]\ndaten = \"{}\"\nentscheidungen = \"{}\"\n",
            ort.p("inhalt/projects"),
            ort.p("geteilt/beispiel"),
            ort.p("inhalt/data"),
            ort.p("inhalt/docs/entscheidungen")
        );
        let text = ort.gut().replace("\n[konten]", &format!("{abschnitt}\n[konten]"));
        let e = ort.laden(&text).unwrap();
        assert_eq!(e.inhalt.projekte, vec![PathBuf::from(ort.p("inhalt/projects")), PathBuf::from(ort.p("geteilt/beispiel"))]);
        assert_eq!(e.pfade.projekte, PathBuf::from(ort.p("inhalt/projects")), "pfade.projekte ist die erste Quelle");
        assert_eq!(e.pc_datei(), PathBuf::from(ort.p("inhalt/data")).join("pc.toml"));
        assert_eq!(e.schluessel_datei(), PathBuf::from(ort.p("inhalt/data")).join("schluessel.toml"));
        assert_eq!(e.inhalt.entscheidungen, PathBuf::from(ort.p("inhalt/docs/entscheidungen")));
        assert!(e.inhalt.abschnitt, "mit [inhalt]");
        if std::env::var_os("DASHBOARD_PROJECTS_DIR").is_none() {
            assert_eq!(e.projekt_quellen(), e.inhalt.projekte);
            assert_eq!(e.inhalt_ordner().len(), 4);
        }
        // Beides zugleich, eine leere Liste und ein relativer Pfad: je ein Satz.
        let doppelt = text.replace("[pfade]\n", &format!("[pfade]\nprojekte = \"{}\"\n", ort.p("x")));
        assert!(ort.laden(&doppelt).unwrap_err().0.contains("stehen zweimal da"));
        let leer = ort.gut().replace("\n[konten]", "\n[inhalt]\nprojekte = []\n\n[konten]");
        assert!(ort.laden(&leer).unwrap_err().0.contains("leere Liste"));
        let relativ = ort.gut().replace("\n[konten]", "\n[inhalt]\ndaten = \"data\"\n\n[konten]");
        assert!(ort.laden(&relativ).unwrap_err().0.contains("Bei `inhalt.daten` steht „data“"));
        let fremd = ort.gut().replace("\n[konten]", "\n[inhalt]\nwas = \"x\"\n\n[konten]");
        assert!(ort.laden(&fremd).unwrap_err().0.contains("Das Feld `was` gibt es hier nicht."));
    }

    #[test]
    fn inventar_neben_dem_datenspeicher() {
        let ort = Ort::neu("inventar");
        let e = ort.laden(&ort.gut()).unwrap();
        assert_eq!(e.inventar_datei(), PathBuf::from(ort.p("daten")).join("inventar.json"));
    }

    /// Etappe 7c: der JWT-Schlüssel liegt in `geheim/` neben dem Ordner des
    /// Datenspeichers (im Dienst `C:/ProgramData/Dashboard/geheim/jwt-schluessel`).
    #[test]
    fn jwt_schluessel_neben_dem_datenordner() {
        let ort = Ort::neu("jwt");
        let e = ort.laden(&ort.gut()).unwrap();
        let ordner = PathBuf::from(ort.p("daten")).parent().unwrap().to_path_buf();
        assert_eq!(e.jwt_schluessel_datei(), ordner.join("geheim").join("jwt-schluessel"));
        let mut dienst = e.clone();
        dienst.pfade.daten = PathBuf::from("C:/ProgramData/Dashboard/daten/dashboard.redb");
        assert_eq!(dienst.jwt_schluessel_datei(), PathBuf::from("C:/ProgramData/Dashboard/geheim/jwt-schluessel"));
    }

    #[test]
    fn datei_fehlt() {
        let ort = Ort::neu("fehlt");
        let e = Einstellungen::laden(&ort.ordner.join("gibt-es-nicht.toml")).unwrap_err();
        assert!(e.0.starts_with("Die Einstellungsdatei ") && e.0.contains("fehlt"), "{e}");
    }

    #[test]
    fn fehlendes_feld_nennt_feld_und_zeile() {
        let ort = Ort::neu("fehlendes");
        let text = ort.gut().replace("port = 8787\n", "");
        let e = ort.laden(&text).unwrap_err();
        assert!(e.0.contains("dienst.toml, Zeile 1:") && e.0.contains("`port`"), "{e}");
    }

    #[test]
    fn unbekanntes_feld_nennt_die_erlaubten() {
        let ort = Ort::neu("unbekannt");
        let text = ort.gut().replace("port = 8787\n", "port = 8787\nfarbe = \"blau\"\n");
        let e = ort.laden(&text).unwrap_err();
        assert!(e.0.contains("Zeile 4:"), "{e}");
        assert!(e.0.contains("Das Feld `farbe` gibt es hier nicht. Erlaubt sind:"), "{e}");
    }

    /// Unter `[konten]` gelten nur `inhaber_sid` und `dienst_sid`; ein anderer
    /// Name für die SID (auch ein älterer) lässt das Laden scheitern, mit Zeile.
    #[test]
    fn fremder_sid_schluessel_in_konten() {
        let ort = Ort::neu("konten-fremd");
        let text = ort.gut().replace("inhaber_sid = \"\"", "nebelfink_sid = \"S-1-5-21-1-2\"");
        let e = ort.laden(&text).unwrap_err();
        let zeile = text.lines().position(|z| z.starts_with("nebelfink_sid")).unwrap() + 1;
        assert!(e.0.contains(&format!("dienst.toml, Zeile {zeile}:")), "{e}");
        assert!(e.0.contains("Das Feld `nebelfink_sid` gibt es hier nicht. Erlaubt sind:"), "{e}");
        assert!(e.0.contains("inhaber_sid") && e.0.contains("dienst_sid"), "{e}");
        // Auch neben dem richtigen Schlüssel.
        let beide = ort.gut().replace("inhaber_sid = \"\"", "inhaber_sid = \"S-1-5-21-1-2\"\nnebelfink_sid = \"S-1-5-21-1-3\"");
        assert!(ort.laden(&beide).unwrap_err().0.contains("`nebelfink_sid`"));
    }

    #[test]
    fn port_null_und_zu_gross() {
        let ort = Ort::neu("port");
        let e = ort.laden(&ort.gut().replace("port = 8787", "port = 0")).unwrap_err();
        assert!(e.0.ends_with("Zeile 3: Bei `port` steht 0. Erlaubt ist eine Zahl von 1 bis 65535, etwa `port = 8787`."), "{e}");
        let e = ort.laden(&ort.gut().replace("port = 8787", "port = 70000")).unwrap_err();
        assert!(e.0.contains("Bei `port` steht 70000."), "{e}");
    }

    #[test]
    fn relativer_pfad_wird_abgewiesen() {
        let ort = Ort::neu("relativ");
        let text = ort.gut().replace(&ort.p("sicherung"), "sicherung");
        let e = ort.laden(&text).unwrap_err();
        assert!(e.0.contains("Zeile 7: Bei `pfade.sicherung` steht „sicherung“. Bitte einen vollständigen Pfad"), "{e}");
        let text = ort.gut().replace(&ort.p("repo"), &ort.p("kein-repo"));
        let e = ort.laden(&text).unwrap_err();
        assert!(e.0.contains("diesen Ordner gibt es nicht"), "{e}");
        let e = ort.laden(&ort.gut().replace("schema = 1", "schema = 2")).unwrap_err();
        assert!(e.0.contains("Zeile 1: Bei `schema` steht 2."), "{e}");
    }
}
