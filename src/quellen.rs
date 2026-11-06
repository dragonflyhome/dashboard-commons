//! Mehrere Inhaltsquellen (Etappe 4b, Entscheidung
//! `inhaltsquellen`): `DASHBOARD_PROJECTS_DIR` darf eine Liste
//! sein, getrennt durch `;`. Jeder Eintrag ist entweder
//!
//! - ein **Ordner mit Projekten** (wie heute `projects\`): jeder Unterordner
//!   mit gültigem Namen ist ein Projekt, oder
//! - **ein einzelnes Projekt**: der Ordner selbst enthält `projekt.toml`; sein
//!   Name ist der Slug.
//!
//! Der Bestand ist die Vereinigung. Kommt ein Slug in zwei Quellen vor, bleibt
//! der aus der früheren sichtbar, und es gibt einen Satz in einfachem Deutsch
//! ([`satz_doppelt`]). Mit nur einem Eintrag (der Fall heute) ändert sich
//! nichts: Alle Wege sehen denselben Ordner wie vorher.
//!
//! Jedes Projekt wird hier als (Elternordner, Slug) geführt, denn die Prüfung
//! und das Lesen arbeiten mit `dir.join(slug)`: für einen Ordner mit Projekten
//! ist das der Ordner selbst, für ein einzelnes Projekt sein Elternordner.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::inhalt::seite;

/// Einträge, die schon einmal ein einzelnes Projekt waren. Fehlt `projekt.toml`
/// kurz (git checkout, pull), gelten sie sonst als Ordner, und `assets`,
/// `daten`, `server` erschienen als Projekte. Die Liste wird an vielen Stellen
/// neu aufgelöst, darum hält der Prozess die Art fest, nicht ein `Quellen`.
static EINZELPROJEKTE: Mutex<BTreeSet<PathBuf>> = Mutex::new(BTreeSet::new());

/// Die Fassung dieses Dashboards, wie sie in der [`VERSIONSDATEI`] steht.
const EIGENE: &str = concat!("v", env!("CARGO_PKG_VERSION"));

/// Trennzeichen der Liste in `DASHBOARD_PROJECTS_DIR`.
pub const TRENNER: char = ';';

/// Was ein Eintrag der Liste ist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Art {
    /// Ein Ordner mit Projekten darin.
    Ordner,
    /// Ein einzelnes Projekt (der Ordner enthält `projekt.toml`).
    Projekt,
}

/// Ein Eintrag der Liste.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Quelle {
    pub pfad: PathBuf,
    pub art: Art,
}

/// Die Liste der Inhaltsquellen, aufgelöst zu Projekten.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Quellen {
    pub eintraege: Vec<Quelle>,
    /// Slug → Elternordner (das Projekt liegt in `<eltern>/<slug>`).
    projekte: BTreeMap<String, PathBuf>,
    /// Je doppeltem Slug ein Satz.
    pub doppelt: Vec<String>,
    /// Je Projekt einer weiteren Quelle mit `server/` (Rechenteil) ein Hinweis:
    /// Slug und Satz ([`SATZ_RECHENTEIL`]).
    pub rechenteile: Vec<(String, String)>,
    /// Nummern der Einträge, deren Ordner sich nicht lesen lässt (Satz: [`satz_unlesbar`]).
    pub unlesbar: Vec<usize>,
    /// Je Quelle mit abweichender oder kaputter [`VERSIONSDATEI`] ein Satz.
    pub version: Vec<String>,
}

/// Im Wurzelordner eines Overlay-Repos (über `projects\`): `tag = "v0.1.0"`,
/// die Fassung des Code-Repos, gegen die der Inhalt gebaut ist.
pub const VERSIONSDATEI: &str = "dashboard-version.toml";

/// Name einer Quelle in Sätzen, ohne Pfad (auch für `meldungen::gelesene_ordner`).
pub fn name(nr: usize) -> String {
    if nr == 0 { "projects".to_string() } else { format!("projects ({})", nr + 1) }
}

/// Der Satz für eine Quelle, die fehlt oder sich nicht lesen lässt.
pub fn satz_unlesbar(nr: usize) -> String {
    format!(
        "Die Inhaltsquelle {} lässt sich nicht lesen (sie fehlt, oder das Dashboard hat kein Leserecht). Ihre Projekte bleiben mit dem letzten guten Stand; das Dashboard versucht es alle 2 s wieder.",
        name(nr)
    )
}

/// Vergleicht die [`VERSIONSDATEI`] in `wurzel` mit `eigene` (`v<version>`).
/// Keine Datei: `None`; abweichend oder unlesbar: ein Satz, nie ein Abbruch.
fn satz_version(nr: usize, wurzel: &Path, eigene: &str) -> Option<String> {
    let text = match std::fs::read_to_string(wurzel.join(VERSIONSDATEI)) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(_) => None,
        Ok(t) => Some(t),
    };
    let tag = text.and_then(|t| t.parse::<toml::Table>().ok()).and_then(|t| t.get("tag").and_then(|v| v.as_str()).map(str::to_string));
    match tag {
        Some(tag) if tag == eigene => None,
        Some(tag) => {
            Some(format!("Die Inhaltsquelle {} ist gegen das Dashboard {tag} gebaut, hier läuft {eigene}. Bitte prüfen, ob ihre Seiten noch passen.", name(nr)))
        }
        None => Some(format!("Die Datei {VERSIONSDATEI} der Inhaltsquelle {} lässt sich nicht lesen (erwartet: tag = \"v…\").", name(nr))),
    }
}

/// Mit nur einem Ordner mit Projekten: der Satz zur [`VERSIONSDATEI`] über ihm.
pub fn version_ordner(ordner: &Path) -> Vec<String> {
    ordner.parent().and_then(|w| satz_version(0, w, EIGENE)).into_iter().collect()
}

/// Hinweis für ein Projekt aus einer weiteren Inhaltsquelle mit `server/`.
/// Entscheidung `inhaltsquellen` [geteilt_merge_first]: Ein
/// Rechenteil aus einer fremden Quelle kommt erst nach Durchsicht hinein, über
/// einen Prüfweg, den es nach 4b gibt. Bis dahin baut und lädt der Server
/// keinen Rechenteil aus einer fremden Quelle. Der Baumeister
/// sieht `server/` ohnehin nie an (nur `.svelte` und `.ts` außerhalb davon).
pub const SATZ_RECHENTEIL: &str = "Rechenteil aus einer fremden Quelle wird noch nicht gebaut. Der Ordner server/ bleibt liegen; die Seite selbst geht.";

/// Ist `ordner` ein einzelnes Projekt (mit `projekt.toml`)?
pub fn ist_einzelnes_projekt(ordner: &Path) -> bool {
    std::fs::symlink_metadata(ordner.join("projekt.toml")).is_ok_and(|m| m.is_file())
}

/// Hat ein Unterordner von `ordner` eine `projekt.toml`?
fn hat_unterprojekt(ordner: &Path) -> bool {
    std::fs::read_dir(ordner).is_ok_and(|rd| rd.filter_map(Result::ok).any(|d| ist_einzelnes_projekt(&d.path())))
}

/// Der Satz für einen Slug, der in zwei Quellen vorkommt.
pub fn satz_doppelt(slug: &str, erste: &Path, zweite: &Path) -> String {
    format!(
        "Das Projekt „{slug}“ gibt es zweimal, in {} und in {}. Gezeigt wird das erste; das zweite bitte umbenennen oder aus der Liste nehmen.",
        erste.display(),
        zweite.display()
    )
}

impl Quellen {
    /// Aus dem Text von `DASHBOARD_PROJECTS_DIR` (leere Einträge zählen nicht).
    pub fn aus_text(text: &str) -> Quellen {
        Self::aus_liste(text.split(TRENNER).map(str::trim).filter(|t| !t.is_empty()).map(PathBuf::from).collect())
    }

    /// Aus einer Liste von Pfaden; liest die Ordner (blockiert kurz).
    pub fn aus_liste(pfade: Vec<PathBuf>) -> Quellen {
        let mut einzeln = EINZELPROJEKTE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let eintraege: Vec<Quelle> = pfade
            .into_iter()
            .map(|pfad| {
                let art = if ist_einzelnes_projekt(&pfad) {
                    einzeln.insert(pfad.clone());
                    Art::Projekt
                } else if einzeln.contains(&pfad) && !hat_unterprojekt(&pfad) {
                    Art::Projekt
                } else {
                    // Lag projekt.toml nur kurz in einem Ordner mit Projekten, wird er wieder Ordner.
                    einzeln.remove(&pfad);
                    Art::Ordner
                };
                Quelle { pfad, art }
            })
            .collect();
        drop(einzeln);
        let mut q = Quellen { eintraege, ..Default::default() };
        q.aufloesen();
        q
    }

    /// Nur ein Ordner, wie bisher (`projects\`).
    pub fn ein_ordner(ordner: PathBuf) -> Quellen {
        Self::aus_liste(vec![ordner])
    }

    /// Dieselbe Liste noch einmal von der Platte (neue oder entfernte Projekte).
    pub fn neu_lesen(&self) -> Quellen {
        Self::aus_liste(self.eintraege.iter().map(|e| e.pfad.clone()).collect())
    }

    fn aufloesen(&mut self) {
        let mut projekte: BTreeMap<String, PathBuf> = BTreeMap::new();
        let mut doppelt = Vec::new();
        let mut rechenteile = Vec::new();
        let mut unlesbar = Vec::new();
        let mut version = Vec::new();
        for (nr, e) in self.eintraege.iter().enumerate() {
            // Repo-Wurzel: über dem Ordner mit Projekten (bei einem einzelnen Projekt über seinem Elternordner).
            let basis = if e.art == Art::Projekt { e.pfad.parent() } else { Some(e.pfad.as_path()) };
            if let Some(satz) = basis.and_then(Path::parent).and_then(|w| satz_version(nr, w, EIGENE)) {
                version.push(satz);
            }
            let gefunden: Vec<(String, PathBuf)> = match e.art {
                // War schon ein einzelnes Projekt, projekt.toml fehlt gerade: unlesbar, der letzte Stand bleibt.
                Art::Projekt if !ist_einzelnes_projekt(&e.pfad) => {
                    unlesbar.push(nr);
                    Vec::new()
                }
                Art::Projekt => {
                    let slug = e.pfad.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
                    let eltern = e.pfad.parent().map(Path::to_path_buf).unwrap_or_default();
                    if seite::is_valid_slug(&slug) { vec![(slug, eltern)] } else { Vec::new() }
                }
                Art::Ordner => {
                    let mut v: Vec<(String, PathBuf)> = std::fs::read_dir(&e.pfad)
                        .map(|rd| {
                            rd.filter_map(Result::ok)
                                .filter(|d| d.file_type().is_ok_and(|t| t.is_dir() || t.is_symlink()))
                                .filter_map(|d| d.file_name().into_string().ok())
                                .filter(|s| seite::is_valid_slug(s))
                                .map(|s| (s, e.pfad.clone()))
                                .collect()
                        })
                        .unwrap_or_else(|_| {
                            unlesbar.push(nr);
                            Vec::new()
                        });
                    v.sort();
                    v
                }
            };
            for (slug, eltern) in gefunden {
                match projekte.get(&slug) {
                    Some(erste) => doppelt.push(satz_doppelt(&slug, &erste.join(&slug), &eltern.join(&slug))),
                    None => {
                        // Nur der erste Eintrag ist der Projektordner des Code-Repos.
                        if nr > 0 && eltern.join(&slug).join("server").is_dir() {
                            rechenteile.push((slug.clone(), SATZ_RECHENTEIL.to_string()));
                        }
                        projekte.insert(slug, eltern);
                    }
                }
            }
        }
        self.projekte = projekte;
        self.doppelt = doppelt;
        self.rechenteile = rechenteile;
        self.unlesbar = unlesbar;
        self.version = version;
    }

    /// Je unlesbarer Quelle ein Satz ohne Pfad.
    pub fn unlesbar_saetze(&self) -> Vec<String> {
        self.unlesbar.iter().map(|&nr| satz_unlesbar(nr)).collect()
    }

    /// Die Pfade der unlesbaren Einträge.
    pub fn unlesbare_pfade(&self) -> Vec<PathBuf> {
        self.unlesbar.iter().map(|&nr| self.eintraege[nr].pfad.clone()).collect()
    }

    /// Nur ein Eintrag (heute der Normalfall): Dann gilt alles wie vor 4b.
    pub fn einzeln(&self) -> bool {
        self.eintraege.len() <= 1
    }

    /// Der erste Ordner: für einen Ordner mit Projekten er selbst, für ein
    /// einzelnes Projekt sein Elternordner. Das ist der Projektordner, den
    /// alle Wege ohne Slug nutzen (Repo, `data/` daneben).
    pub fn erste(&self) -> PathBuf {
        match self.eintraege.first() {
            Some(Quelle { pfad, art: Art::Ordner }) => pfad.clone(),
            Some(Quelle { pfad, art: Art::Projekt }) => pfad.parent().map(Path::to_path_buf).unwrap_or_default(),
            None => PathBuf::from("projects"),
        }
    }

    /// Alle Projekte: Slug → Elternordner.
    pub fn projekte(&self) -> &BTreeMap<String, PathBuf> {
        &self.projekte
    }

    /// Der Elternordner eines Projekts (`None`: gibt es in keiner Quelle).
    pub fn eltern(&self, slug: &str) -> Option<&Path> {
        self.projekte.get(slug).map(PathBuf::as_path)
    }

    /// Der Ordner eines Projekts.
    pub fn ordner(&self, slug: &str) -> Option<PathBuf> {
        self.eltern(slug).map(|e| e.join(slug))
    }

    /// Die Wurzeln für `server.fs.allow` von vite: je Eintrag der Pfad selbst.
    pub fn wurzeln(&self) -> Vec<PathBuf> {
        self.eintraege.iter().map(|e| e.pfad.clone()).collect()
    }

    /// Alle Projektseiten aller Quellen, je Slug aus der ersten Quelle (wie
    /// `seite::load_all` je Ordner; kaputte fehlen). Liest die Platte.
    pub fn seiten(&self) -> Vec<seite::Project> {
        let mut v: Vec<seite::Project> = self.projekte.iter().filter_map(|(slug, eltern)| seite::load_one(eltern, slug).ok().flatten()).collect();
        v.sort_by(|a, b| a.slug.cmp(&b.slug));
        v
    }

    /// Wieder als Text für die Umgebung (`;` getrennt).
    pub fn als_text(&self) -> String {
        self.wurzeln().iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(";")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Ort(PathBuf);
    impl Drop for Ort {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn ort(name: &str) -> Ort {
        let p = std::env::temp_dir().join(format!("dashboard-quellen-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        Ort(p)
    }

    fn projekt(ordner: &Path) {
        std::fs::create_dir_all(ordner).unwrap();
        std::fs::write(ordner.join("projekt.toml"), "schema = 3\n").unwrap();
    }

    /// Ein Ordner mit Projekten: wie bisher, auch ohne projekt.toml in den
    /// Unterordnern; ungültige Namen zählen nicht.
    #[test]
    fn ein_ordner_wie_bisher() {
        let o = ort("ordner");
        projekt(&o.0.join("projects/alpha"));
        std::fs::create_dir_all(o.0.join("projects/beta")).unwrap();
        std::fs::create_dir_all(o.0.join("projects/_vorlage")).unwrap();
        std::fs::create_dir_all(o.0.join("projects/Gross")).unwrap();
        let q = Quellen::aus_text(&o.0.join("projects").display().to_string());
        assert!(q.einzeln());
        assert_eq!(q.eintraege[0].art, Art::Ordner);
        assert_eq!(q.projekte().keys().collect::<Vec<_>>(), ["alpha", "beta"]);
        assert_eq!(q.erste(), o.0.join("projects"));
        assert_eq!(q.ordner("alpha"), Some(o.0.join("projects").join("alpha")));
        assert!(q.doppelt.is_empty());
    }

    /// Ein einzelnes Projekt (erkannt an projekt.toml): Slug ist der Ordnername.
    #[test]
    fn einzelnes_projekt() {
        let o = ort("einzeln");
        projekt(&o.0.join("teilen/beispiel-b"));
        let q = Quellen::aus_text(&format!(" {} ", o.0.join("teilen/beispiel-b").display()));
        assert_eq!(q.eintraege[0].art, Art::Projekt);
        assert_eq!(q.projekte().keys().collect::<Vec<_>>(), ["beispiel-b"]);
        assert_eq!(q.eltern("beispiel-b"), Some(o.0.join("teilen").as_path()));
        assert_eq!(q.erste(), o.0.join("teilen"));
    }

    /// Ordner und einzelnes Projekt zusammen; ein doppelter Slug ergibt einen
    /// Satz, und der aus der ersten Quelle bleibt.
    #[test]
    fn doppelt_die_erste_bleibt() {
        let o = ort("doppelt");
        projekt(&o.0.join("inhalt/projects/beispiel-b"));
        projekt(&o.0.join("inhalt/projects/schaufenster"));
        projekt(&o.0.join("geteilt/beispiel-b"));
        projekt(&o.0.join("extra/neu"));
        let text = format!("{};{};;{}", o.0.join("inhalt/projects").display(), o.0.join("geteilt/beispiel-b").display(), o.0.join("extra/neu").display());
        let q = Quellen::aus_text(&text);
        assert!(!q.einzeln());
        assert_eq!(q.eintraege.len(), 3, "leere Einträge zählen nicht");
        assert_eq!(q.projekte().keys().collect::<Vec<_>>(), ["beispiel-b", "neu", "schaufenster"]);
        assert_eq!(q.eltern("beispiel-b"), Some(o.0.join("inhalt/projects").as_path()), "die erste Quelle bleibt sichtbar");
        assert_eq!(q.eltern("neu"), Some(o.0.join("extra").as_path()));
        assert_eq!(q.doppelt.len(), 1);
        assert!(q.doppelt[0].starts_with("Das Projekt „beispiel-b“ gibt es zweimal, in "), "{}", q.doppelt[0]);
        assert!(q.doppelt[0].ends_with("Gezeigt wird das erste; das zweite bitte umbenennen oder aus der Liste nehmen."));
        assert_eq!(q.wurzeln().len(), 3);
        assert_eq!(Quellen::aus_text(&q.als_text()), q);
    }

    /// Rechenteil (server/) in einer weiteren Quelle: kein Bau, ein Hinweis;
    /// im ersten Ordner (Code-Repo) wie bisher keiner.
    #[test]
    fn rechenteil_aus_fremder_quelle_nur_hinweis() {
        let o = ort("rechenteil");
        projekt(&o.0.join("repo/projects/beispiel-a"));
        std::fs::create_dir_all(o.0.join("repo/projects/beispiel-a/server/src")).unwrap();
        projekt(&o.0.join("fremd/beispiel-b"));
        std::fs::create_dir_all(o.0.join("fremd/beispiel-b/server/src")).unwrap();
        std::fs::write(o.0.join("fremd/beispiel-b/server/Cargo.toml"), "[package]\n").unwrap();
        projekt(&o.0.join("fremd2/ohne"));
        let q = Quellen::aus_liste(vec![o.0.join("repo/projects"), o.0.join("fremd/beispiel-b"), o.0.join("fremd2/ohne")]);
        assert_eq!(q.rechenteile, vec![("beispiel-b".to_string(), SATZ_RECHENTEIL.to_string())]);
        assert!(q.projekte().contains_key("beispiel-b"), "die Seite selbst bleibt");
        // Der Baumeister sieht server/ nie an: nur .svelte und .ts außerhalb davon.
        std::fs::write(o.0.join("fremd/beispiel-b/server/src/x.ts"), "export const x = 1;\n").unwrap();
        assert!(crate::werkzeug::scanner::code_dateien(&o.0.join("fremd"), "beispiel-b").is_empty());
        assert!(Quellen::ein_ordner(o.0.join("repo/projects")).rechenteile.is_empty());
    }

    /// Ein Ordner, den es nicht gibt, ist eine leere Quelle (kein Absturz).
    #[test]
    fn fehlender_ordner_ist_leer() {
        let q = Quellen::aus_text("C:/gibt/es/nicht/projects");
        assert!(q.projekte().is_empty());
        assert_eq!(q.erste(), PathBuf::from("C:/gibt/es/nicht/projects"));
        assert_eq!(q.unlesbar, [0]);
    }

    /// Eine zweite Quelle, die fehlt (auch ein einzelnes Projekt), gibt je
    /// einen Satz ohne Pfad; lesbare keinen.
    #[test]
    fn unlesbare_quelle_ein_satz_ohne_pfad() {
        let o = ort("unlesbar");
        projekt(&o.0.join("repo/projects/alpha"));
        projekt(&o.0.join("fremd/solo"));
        let pfade = vec![o.0.join("repo/projects"), o.0.join("weg/projects"), o.0.join("fremd/solo")];
        assert_eq!(Quellen::aus_liste(pfade.clone()).unlesbar, [1]);
        std::fs::rename(o.0.join("fremd/solo"), o.0.join("fremd/solo-weg")).unwrap();
        let q = Quellen::aus_liste(pfade);
        assert_eq!(q.unlesbar, [1, 2]);
        assert_eq!(q.unlesbare_pfade(), [o.0.join("weg/projects"), o.0.join("fremd/solo")]);
        let saetze = q.unlesbar_saetze();
        assert!(saetze[1].starts_with("Die Inhaltsquelle projects (3) lässt sich nicht lesen"), "{}", saetze[1]);
        assert!(!saetze.iter().any(|s| s.contains(&o.0.display().to_string())));
    }

    /// Abnahme quellen_haerten (Sollte 1): Fehlt einem einzelnen Projekt kurz
    /// projekt.toml, bleibt es ein Projekt (unlesbar, mit Satz); seine
    /// Unterordner werden nie zu Projekten. Danach geht es wie vorher.
    #[test]
    fn einzelprojekt_ohne_projekt_toml_bleibt_projekt() {
        let o = ort("art-fest");
        projekt(&o.0.join("repo/projects/alpha"));
        let solo = o.0.join("fremd/solo");
        projekt(&solo);
        for u in ["assets", "daten", "server"] {
            std::fs::create_dir_all(solo.join(u)).unwrap();
        }
        let pfade = vec![o.0.join("repo/projects"), solo.clone()];
        assert_eq!(Quellen::aus_liste(pfade.clone()).projekte().keys().collect::<Vec<_>>(), ["alpha", "solo"]);
        std::fs::remove_file(solo.join("projekt.toml")).unwrap();
        let q = Quellen::aus_liste(pfade.clone());
        assert_eq!(q.eintraege[1].art, Art::Projekt);
        assert_eq!(q.projekte().keys().collect::<Vec<_>>(), ["alpha"]);
        assert_eq!(q.unlesbar, [1]);
        assert_eq!(q.unlesbare_pfade(), std::slice::from_ref(&solo), "der Store behält so den letzten Stand");
        projekt(&solo);
        let q = Quellen::aus_liste(pfade);
        assert_eq!(q.projekte().keys().collect::<Vec<_>>(), ["alpha", "solo"]);
        assert!(q.unlesbar.is_empty());
    }

    /// Gegenprüfung quellen_haerten: Liegt projekt.toml nur kurz in einem
    /// Ordner mit Projekten, sind seine Projekte danach wieder da.
    #[test]
    fn ordner_mit_kurzer_projekt_toml_wird_wieder_ordner() {
        let o = ort("art-zurueck");
        let ordner = o.0.join("repo/projects");
        projekt(&ordner.join("a"));
        projekt(&ordner.join("b"));
        let pfade = vec![ordner.clone()];
        assert_eq!(Quellen::aus_liste(pfade.clone()).projekte().keys().collect::<Vec<_>>(), ["a", "b"]);
        std::fs::write(ordner.join("projekt.toml"), "schema = 3\n").unwrap();
        assert_eq!(Quellen::aus_liste(pfade.clone()).projekte().keys().collect::<Vec<_>>(), ["projects"]);
        std::fs::remove_file(ordner.join("projekt.toml")).unwrap();
        let q = Quellen::aus_liste(pfade);
        assert_eq!(q.projekte().keys().collect::<Vec<_>>(), ["a", "b"]);
        assert!(q.unlesbar.is_empty());
    }

    /// dashboard-version.toml über projects/: gleich, anders, kaputt, fehlt.
    #[test]
    fn versionsdatei_gleich_anders_kaputt_fehlt() {
        let o = ort("version");
        let eigene = concat!("v", env!("CARGO_PKG_VERSION"));
        for (repo, inhalt) in [
            ("gleich", Some(format!("tag = \"{eigene}\"\n"))),
            ("anders", Some("tag = \"v0.0.0-alt\"\n".to_string())),
            ("kaputt", Some("tag = [\n".to_string())),
            ("fehlt", None),
        ] {
            projekt(&o.0.join(repo).join("projects").join(repo));
            if let Some(t) = inhalt {
                std::fs::write(o.0.join(repo).join(VERSIONSDATEI), t).unwrap();
            }
        }
        let satz = |repo: &str| Quellen::aus_liste(vec![o.0.join(repo).join("projects")]).version;
        assert!(satz("gleich").is_empty());
        assert!(satz("fehlt").is_empty());
        assert_eq!(
            satz("anders"),
            vec![format!(
                "Die Inhaltsquelle projects ist gegen das Dashboard v0.0.0-alt gebaut, hier läuft {eigene}. Bitte prüfen, ob ihre Seiten noch passen."
            )]
        );
        assert_eq!(
            satz("kaputt"),
            vec!["Die Datei dashboard-version.toml der Inhaltsquelle projects lässt sich nicht lesen (erwartet: tag = \"v…\").".to_string()]
        );
        // Gültiges TOML ohne `tag`, und eine Datei, die sich nicht lesen lässt (hier ein Ordner): derselbe Satz.
        projekt(&o.0.join("ohne-tag/projects/x"));
        std::fs::write(o.0.join("ohne-tag").join(VERSIONSDATEI), "x = 1\n").unwrap();
        assert_eq!(satz("ohne-tag"), satz("kaputt"));
        projekt(&o.0.join("ordner/projects/x"));
        std::fs::create_dir_all(o.0.join("ordner").join(VERSIONSDATEI)).unwrap();
        assert_eq!(satz("ordner"), satz("kaputt"));
        // Ein einzelnes Projekt als zweite Quelle: die Datei liegt zwei Ebenen höher.
        let q = Quellen::aus_liste(vec![o.0.join("gleich/projects"), o.0.join("anders/projects/anders")]);
        assert_eq!(q.version.len(), 1);
        assert!(q.version[0].starts_with("Die Inhaltsquelle projects (2) ist gegen das Dashboard v0.0.0-alt"), "{:?}", q.version);
    }
}
