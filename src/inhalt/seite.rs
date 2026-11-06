//! Projekte lesen: `projects/<slug>/projekt.toml` (Kopf, Typ
//! `vertrag::projekt::Projekt`) und `daten/*.toml` (Typ
//! `vertrag::daten::ProjektDaten`). Ohne `projekt.toml` gibt es das Projekt
//! nicht.
//!
//! Intern trägt jedes Projekt einen `Meta`-Kopf; der Kopf aus `projekt.toml`
//! wird dafür übersetzt (Status und Backup als Wort).

use std::path::{Path, PathBuf};

use anyhow::Result;
use chrono::{DateTime, Local};
use serde::Serialize;
use ts_rs::TS;

use crate::vertrag::abgeleitet::{self, Quellen};
use crate::vertrag::daten::{DatenInhalt, ProjektDaten, ist_datenname};
use crate::vertrag::lesen::{LeseFehler, Mangel, toml_lesen};
pub use crate::vertrag::projekt::{HttpCheck, Link, RepoRef};
use crate::vertrag::projekt::{Projekt, Seite};

/// Der interne Kopf eines Projekts, aus `projekt.toml` übersetzt.
/// `status` und `backup` stehen hier als Wort.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Meta {
    pub title: String,
    pub status: String,
    pub summary: String,
    pub repos: Vec<RepoRef>,
    pub links: Vec<Link>,
    pub open: Vec<String>,
    /// Kleine Lebenszeichen: HTTP-Adressen, die erreichbar sein sollen.
    pub checks: Vec<HttpCheck>,
    /// Programme, die laufen sollen (Dateiname, z. B. `schaufenster.exe`).
    pub processes: Vec<String>,
    /// Reihenfolge auf der Startseite (kleiner = weiter vorn).
    pub order: i32,
    /// Welcher Chat das Projekt verwaltet (Regel: jedes Projekt hat seinen eigenen Chat).
    pub chat: Option<String>,
    /// Stand der Datensicherung dieses Projekts: `rot`, `gelb` oder `grün`.
    pub backup: Option<String>,
    /// Kurze Begründung zum Backup-Stand.
    pub backup_note: String,
    /// Fertiger Entwurf (eine HTML-Datei in `assets/`, alles eingebettet), den das
    /// Dashboard als Unterseite `/projects/<slug>/vorschau` im Rahmen zeigt.
    /// Erlaubt ist nur `assets/<name>.html`.
    pub vorschau: Option<String>,
}

/// Dateiname aus `vorschau = "assets/<name>.html"`, wenn das Feld gültig ist.
pub fn preview_file(value: &str) -> Option<&str> {
    let name = value.strip_prefix("assets/")?;
    let plain =
        !name.is_empty() && name.len() <= 128 && !name.starts_with('.') && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'));
    let html = name.ends_with(".html") || name.ends_with(".htm");
    (plain && html).then_some(name)
}

/// Was an `vorschau = "…"` falsch ist (nur die Form, nicht ob die Datei da ist).
pub fn vorschau_mangel(v: &str) -> Option<Mangel> {
    preview_file(v).is_none().then(|| {
        Mangel::bei(
            "vorschau",
            v,
            format!("Bei `vorschau` steht „{v}“. Erlaubt ist nur assets/<name>.html (einfacher Dateiname, kein Unterordner), etwa `vorschau = \"assets/vorschau.html\"`."),
        )
    })
}

impl From<&Projekt> for Meta {
    /// Der Kopf als interner Kopf: Status und Backup als Wort.
    fn from(p: &Projekt) -> Self {
        Self {
            title: p.title.clone(),
            status: p.status.wort().to_string(),
            summary: p.summary.clone(),
            repos: p.repos.clone(),
            links: p.links.clone(),
            open: p.open.clone(),
            checks: p.checks.clone(),
            processes: p.processes.clone(),
            order: p.order,
            chat: p.chat.clone(),
            backup: p.backup.map(|b| b.wort().to_string()),
            backup_note: p.backup_note.clone(),
            vorschau: p.vorschau.clone(),
        }
    }
}

/// Ein fertig geladenes Projekt.
#[derive(Debug, Clone, Serialize)]
pub struct Project {
    pub slug: String,
    pub meta: Meta,
    /// Unterseiten aus `projekt.toml`, nach `order` und Name.
    pub seiten: Vec<Seite>,
    pub daten: Option<ProjektDaten>,
    pub file_modified: Option<DateTime<Local>>,
}

impl Project {
    /// „Letzte Änderung“: das Dateidatum (`projekt.toml` und `daten/`).
    pub fn last_change(&self) -> String {
        self.file_modified.map(|t| t.format("%Y-%m-%d %H:%M").to_string()).unwrap_or_else(|| "unbekannt".to_string())
    }
}

/// Erlaubt sind nur Kleinbuchstaben, Ziffern und Bindestriche.
pub fn is_valid_slug(slug: &str) -> bool {
    !slug.is_empty() && slug.len() <= 64 && slug.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') && !slug.starts_with('-')
}

/// Änderungszeit einer Datei.
fn geaendert(path: &Path) -> Option<DateTime<Local>> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok().map(DateTime::<Local>::from)
}

/// Ein Ordner, aber keine Verknüpfung auf einen (die meldet [`neue_datei_lesen`]).
pub fn ist_echter_ordner(pfad: &Path) -> bool {
    std::fs::symlink_metadata(pfad).map(|m| m.is_dir()).unwrap_or(false)
}

/// Ist `pfad` eine Verknüpfung (Symlink oder, unter Windows, Junction)?
/// Erst nach dem Eintrag selbst (`symlink_metadata`, folgt nicht); als zweite
/// Absicherung, ob der aufgelöste Pfad woanders liegt als der logische
/// (Ordner davor aufgelöst plus Name). Gibt es den Pfad nicht: `false`.
pub fn ist_verknuepfung(pfad: &Path) -> bool {
    let Ok(m) = std::fs::symlink_metadata(pfad) else { return false };
    if m.file_type().is_symlink() {
        return true;
    }
    let (Some(eltern), Some(name)) = (pfad.parent(), pfad.file_name()) else { return false };
    let eltern = if eltern.as_os_str().is_empty() { Path::new(".") } else { eltern };
    match (std::fs::canonicalize(pfad), std::fs::canonicalize(eltern)) {
        (Ok(echt), Ok(e)) => !gleicher_pfad(&echt, &e.join(name)),
        _ => false,
    }
}

/// Satz für einen Ordner, der eine Verknüpfung ist.
pub fn verknuepfung_satz(ordner: &str) -> String {
    format!("Der Ordner {ordner} ist eine Verknüpfung (Symlink oder Junction); das Dashboard liest nur echte Ordner.")
}

/// Zwei (aufgelöste) Pfade gleich, Bestandteil für Bestandteil. Unter Windows
/// ohne Unterschied zwischen `\\?\C:` und `C:` und ohne Groß/Klein.
pub fn gleicher_pfad(a: &Path, b: &Path) -> bool {
    teile(a) == teile(b)
}

/// Liegt die (aufgelöste) Datei `datei` direkt im (aufgelösten) Ordner `ordner`?
pub fn liegt_direkt_in(datei: &Path, ordner: &Path) -> bool {
    datei.parent().is_some_and(|p| gleicher_pfad(p, ordner))
}

/// Bestandteile eines Pfads zum Vergleichen.
fn teile(p: &Path) -> Vec<String> {
    use std::path::{Component, Prefix};
    p.components()
        .filter(|c| !matches!(c, Component::CurDir))
        .map(|c| {
            let s = match c {
                Component::Prefix(v) => match v.kind() {
                    Prefix::Disk(d) | Prefix::VerbatimDisk(d) => format!("{}:", d as char),
                    Prefix::UNC(s, f) | Prefix::VerbatimUNC(s, f) => {
                        format!("//{}/{}", s.to_string_lossy(), f.to_string_lossy())
                    }
                    _ => v.as_os_str().to_string_lossy().into_owned(),
                },
                andere => andere.as_os_str().to_string_lossy().into_owned(),
            };
            if cfg!(windows) { s.to_lowercase() } else { s }
        })
        .collect()
}

/// Für Tests: legt `link` als Verknüpfung auf den Ordner `ziel` an, unter
/// Windows als Junction (`mklink /J`, geht ohne Administrator), sonst als
/// Symlink. `false`, wenn das nicht ging (der Test überspringt dann).
#[cfg(test)]
pub fn junction_anlegen(link: &Path, ziel: &Path) -> bool {
    #[cfg(windows)]
    {
        let Ok(mut cmd) = crate::dienst::programme::befehl(crate::dienst::programme::Werkzeug::Cmd) else {
            return false;
        };
        cmd.args(["/c", "mklink", "/J"])
            .arg(link)
            .arg(ziel)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
            && link.exists()
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(ziel, link).is_ok()
    }
}

/// Fehler, wenn der Projektordner oder sein Ordner `daten/` eine Verknüpfung ist.
fn verknuepfung_pruefen(pfad: &Path, datei: String, ordner: &str) -> Result<(), LeseFehler> {
    if ist_verknuepfung(pfad) {
        return Err(LeseFehler { datei, zeile: None, text: verknuepfung_satz(ordner) });
    }
    Ok(())
}

/// Ist der Fehler ein Lesefehler (Datei gerade gesperrt oder ohne Rechte)?
/// Fehlende Dateien (`NotFound`) und unlesbarer Inhalt (`InvalidData`, etwa
/// UTF-16) zählen nicht. Windows: 32 ERROR_SHARING_VIOLATION, 33 ERROR_LOCK_VIOLATION.
pub fn ist_gesperrt(e: &std::io::Error) -> bool {
    e.kind() == std::io::ErrorKind::PermissionDenied || matches!(e.raw_os_error(), Some(32 | 33))
}

thread_local! {
    /// Dateien, deren Lesen in diesem Faden an einer Sperre scheiterte
    /// (Etappe 2d, für den ContentStore). Gemerkt wird der Fehler der
    /// Leseoperation selbst, nicht ein späterer Blick. `None` außerhalb von
    /// [`mit_merkliste`]: Dann merkt sich niemand etwas, damit die Liste in
    /// Fäden ohne Store (API-Wege, Prüfung) nicht ohne Ende wächst.
    static GESPERRT: std::cell::RefCell<Option<Vec<String>>> = const { std::cell::RefCell::new(None) };
}

/// Merkt `datei`, wenn `e` eine Sperre ist und `pfad` keine Verknüpfung
/// (Windows meldet beim Öffnen einer Verknüpfung auf einen Ordner „kein
/// Zugriff“; das ist ein Fehler im Inhalt, keine Sperre). Nur innerhalb von
/// [`mit_merkliste`]; sonst geschieht nichts.
pub fn gesperrt_merken(datei: &str, pfad: &Path, e: &std::io::Error) {
    if ist_gesperrt(e) && !ist_verknuepfung(pfad) {
        GESPERRT.with(|g| {
            if let Some(liste) = g.borrow_mut().as_mut() {
                liste.push(datei.to_string());
            }
        });
    }
}

/// Stellt beim Verlassen (auch bei Panik) die äußere Merkliste wieder her.
struct Merkliste {
    vorher: Option<Vec<String>>,
}

impl Drop for Merkliste {
    fn drop(&mut self) {
        let vorher = self.vorher.take();
        GESPERRT.with(|g| *g.borrow_mut() = vorher);
    }
}

/// Führt `f` mit einer eigenen Merkliste aus und gibt dazu die Dateien, die
/// dabei an einer Sperre scheiterten. Verschachtelt: Die innere Liste geht
/// danach auch in die äußere ein.
pub fn mit_merkliste<T>(f: impl FnOnce() -> T) -> (T, Vec<String>) {
    let vorher = GESPERRT.with(|g| g.borrow_mut().replace(Vec::new()));
    let mut wache = Merkliste { vorher };
    let wert = f();
    let liste = GESPERRT.with(|g| g.borrow_mut().take()).unwrap_or_default();
    if let Some(aussen) = wache.vorher.as_mut() {
        aussen.extend(liste.iter().cloned());
    }
    drop(wache);
    (wert, liste)
}

/// Ist in diesem Faden gerade eine Merkliste offen? (Für Tests.)
#[cfg(test)]
pub fn merkliste_offen() -> bool {
    GESPERRT.with(|g| g.borrow().is_some())
}

/// So groß darf eine Datei der neuen Form (`projekt.toml`, `daten/*`) höchstens sein.
pub const TOML_HOECHSTENS: u64 = 1024 * 1024;

/// Liest eine Datei der neuen Form als Text. Abgelehnt werden Ordner,
/// Verknüpfungen (Symlinks), Dateien über 1 MB und Text, der kein UTF-8 ist.
/// `datei` ist der Pfad für Meldungen (`projects/<slug>/daten/gantt.toml`).
pub fn neue_datei_lesen(pfad: &Path, datei: &str) -> Result<String, LeseFehler> {
    let name = datei.rsplit('/').next().unwrap_or(datei);
    let fehler = |text: String| LeseFehler { datei: datei.to_string(), zeile: None, text };
    let meta = std::fs::symlink_metadata(pfad).map_err(|e| {
        gesperrt_merken(datei, pfad, &e);
        fehler("Die Datei lässt sich nicht lesen.".to_string())
    })?;
    if meta.file_type().is_symlink() {
        return Err(fehler(format!("{name} ist eine Verknüpfung (Symlink). Hier liegen nur echte Dateien; bitte die Datei selbst hierher legen.")));
    }
    if meta.is_dir() {
        return Err(fehler(format!("{name} ist ein Ordner, keine Datei.")));
    }
    if meta.len() > TOML_HOECHSTENS {
        return Err(fehler(format!("{name} ist größer als 1 MB; so viel gehört nicht in eine Datendatei.")));
    }
    std::fs::read_to_string(pfad).map_err(|e| {
        gesperrt_merken(datei, pfad, &e);
        fehler("Die Datei lässt sich nicht lesen. Sie muss Text in UTF-8 sein.".to_string())
    })
}

/// Liest ein Projekt in neuer Form: `projekt.toml` und alle Dateien in `daten/`.
/// Fehler nennen Datei (`projects/<slug>/…`) und Zeile.
pub fn load_neu(dir: &Path, slug: &str) -> Result<Project, LeseFehler> {
    let (kopf, mut zuletzt) = kopf_lesen(dir, slug)?;

    let mut daten = ProjektDaten::default();
    let mut quellen = Quellen::new();
    let mut kennzahlen_text = None;
    for (name, pfad) in daten_dateien(dir, slug)? {
        let datei = format!("projects/{slug}/daten/{name}");
        if ist_echter_ordner(&pfad) {
            return Err(LeseFehler {
                datei,
                zeile: None,
                text: "In `daten/` sind keine Unterordner erlaubt. Mehrere Teile gehen als `[teil.<name>]` in einer Datei.".to_string(),
            });
        }
        let text = neue_datei_lesen(&pfad, &datei)?;
        daten.datei_einlesen(&name, &datei, &text)?;
        if let Some(stamm) = name.strip_suffix(".toml") {
            quellen.insert(stamm.to_string(), Some(toml_lesen(&datei, &text)?));
            if stamm == "kennzahlen" {
                kennzahlen_text = Some(text);
            }
        }
        zuletzt = zuletzt.max(geaendert(&pfad));
    }
    // Etappe B1: Alter der Stände (Kennzahl, Ampel-Zeile).
    daten.stand_rechnen(abgeleitet::heute());
    // Kennzahlen mit `berechnet` bekommen ihren Wert; ein Fehler darin ist wie jeder andere.
    if let (Some(k), Some(text)) = (daten.kennzahlen.as_mut(), kennzahlen_text) {
        let datei = format!("projects/{slug}/daten/kennzahlen.toml");
        if let Some(e) = abgeleitet::kennzahlen_rechnen(k, &quellen, abgeleitet::heute(), &datei, &text).into_iter().next() {
            return Err(e);
        }
    }

    let mut seiten = kopf.seiten.clone();
    seiten.sort_by(|a, b| a.order.cmp(&b.order).then_with(|| a.name.cmp(&b.name)));
    Ok(Project { slug: slug.to_string(), meta: Meta::from(&kopf), seiten, daten: Some(daten), file_modified: zuletzt })
}

/// Liest und prüft `projekt.toml` (auch `vorschau`); dazu ihre Änderungszeit.
/// Der Projektordner darf keine Verknüpfung sein.
pub fn kopf_lesen(dir: &Path, slug: &str) -> Result<(Projekt, Option<DateTime<Local>>), LeseFehler> {
    verknuepfung_pruefen(&dir.join(slug), format!("projects/{slug}"), slug)?;
    let kopf_datei = format!("projects/{slug}/projekt.toml");
    let pfad = dir.join(slug).join("projekt.toml");
    let text = neue_datei_lesen(&pfad, &kopf_datei)?;
    let kopf: Projekt = toml_lesen(&kopf_datei, &text)?;
    let quelle = text.strip_prefix('\u{feff}').unwrap_or(&text);
    kopf.pruefen().map_err(|m| m.in_datei(&kopf_datei, quelle))?;
    if let Some(m) = kopf.vorschau.as_deref().and_then(vorschau_mangel) {
        return Err(m.in_datei(&kopf_datei, quelle));
    }
    Ok((kopf, geaendert(&pfad)))
}

/// Alle Einträge in `daten/` (Dateiname, Pfad), nach Namen. Kein Ordner
/// `daten/`: leer. Ist `daten/` oder der Projektordner eine Verknüpfung: Fehler.
pub fn daten_dateien(dir: &Path, slug: &str) -> Result<Vec<(String, PathBuf)>, LeseFehler> {
    verknuepfung_pruefen(&dir.join(slug), format!("projects/{slug}"), slug)?;
    let daten_ordner = dir.join(slug).join("daten");
    verknuepfung_pruefen(&daten_ordner, format!("projects/{slug}/daten"), "daten")?;
    if !daten_ordner.is_dir() {
        return Ok(Vec::new());
    }
    let mut namen: Vec<(String, PathBuf)> = std::fs::read_dir(&daten_ordner)
        .inspect_err(|e| {
            gesperrt_merken(&format!("projects/{slug}/daten"), &daten_ordner, e);
        })
        .map_err(|_| LeseFehler { datei: format!("projects/{slug}/daten"), zeile: None, text: "Der Ordner `daten/` lässt sich nicht lesen.".to_string() })?
        .filter_map(|e| e.ok())
        .filter_map(|e| Some((e.file_name().into_string().ok()?, e.path())))
        .collect();
    namen.sort();
    Ok(namen)
}

/// Die Namen der Daten-Dateien (ohne `.toml`, `eigen-<name>` wie in der
/// Datei), die der Vertrag kennt und die echte Dateien sind. Anderes in
/// `daten/` meldet die Prüfung.
pub fn daten_namen(dir: &Path, slug: &str) -> Result<Vec<String>, LeseFehler> {
    Ok(daten_dateien(dir, slug)?
        .into_iter()
        .filter(|(_, pfad)| std::fs::symlink_metadata(pfad).is_ok_and(|m| m.is_file()))
        .filter_map(|(name, _)| name.strip_suffix(".toml").filter(|s| ist_datenname(s)).map(str::to_string))
        .collect())
}

/// Eine Datei `daten/<name>.toml` eines Projekts, geprüft mit dem Vertrag:
/// der Inhalt (bei Kennzahlen mit `berechnet` fertig gerechnet, mit
/// `heute`) und die Tabelle für das Abgeleitete. `Ok(None)`: Name unbekannt
/// oder Datei fehlt. Nur echte Dateien und Ordner, keine Verknüpfungen.
pub fn daten_lesen(dir: &Path, slug: &str, name: &str, heute: chrono::NaiveDate) -> Result<Option<(DatenInhalt, toml::Table)>, LeseFehler> {
    if !is_valid_slug(slug) || !ist_datenname(name) {
        return Ok(None);
    }
    let Some(text) = eine_datei(dir, slug, name)? else { return Ok(None) };
    let datei = format!("projects/{slug}/daten/{name}.toml");
    let mut daten = ProjektDaten::default();
    daten.datei_einlesen(&format!("{name}.toml"), &datei, &text)?;
    let tabelle: toml::Table = toml_lesen(&datei, &text)?;
    if let Some(k) = daten.kennzahlen.as_mut() {
        let staemme: Vec<String> = k
            .inhalte()
            .iter()
            .flat_map(|t| &t.kennzahl)
            .filter_map(|z| z.berechnet.as_ref())
            .map(|b| b.quelle.split('.').next().unwrap_or("").to_string())
            .collect();
        let mut quellen = Quellen::new();
        for stamm in staemme {
            if quellen.contains_key(&stamm) || !ist_datenname(&stamm) {
                continue;
            }
            // Gibt es die Quelle nicht, fehlt sie in `quellen`; lässt sie sich nicht
            // lesen oder bricht sie den Vertrag, steht sie mit `None` darin.
            let quelltext = match eine_datei(dir, slug, &stamm) {
                Ok(None) => continue,
                Ok(Some(t)) => Some(t),
                Err(_) => None,
            };
            let d = format!("projects/{slug}/daten/{stamm}.toml");
            let tabelle = quelltext.and_then(|t| {
                ProjektDaten::default().datei_einlesen(&format!("{stamm}.toml"), &d, &t).ok()?;
                toml_lesen::<toml::Table>(&d, &t).ok()
            });
            quellen.insert(stamm, tabelle);
        }
        if let Some(e) = abgeleitet::kennzahlen_rechnen(k, &quellen, heute, &datei, &text).into_iter().next() {
            return Err(e);
        }
    }
    // Etappe B1: Alter der Stände (Kennzahl, Ampel-Zeile) am Tag `heute`.
    daten.stand_rechnen(heute);
    Ok(daten.nimm(name).map(|inhalt| (inhalt, tabelle)))
}

/// Text von `daten/<stamm>.toml`; `Ok(None)`, wenn es sie nicht gibt.
fn eine_datei(dir: &Path, slug: &str, stamm: &str) -> Result<Option<String>, LeseFehler> {
    verknuepfung_pruefen(&dir.join(slug), format!("projects/{slug}"), slug)?;
    let daten_ordner = dir.join(slug).join("daten");
    verknuepfung_pruefen(&daten_ordner, format!("projects/{slug}/daten"), "daten")?;
    let pfad = daten_ordner.join(format!("{stamm}.toml"));
    if std::fs::symlink_metadata(&pfad).is_err() {
        return Ok(None);
    }
    neue_datei_lesen(&pfad, &format!("projects/{slug}/daten/{stamm}.toml")).map(Some)
}

/// Lädt ein einzelnes Projekt aus `projekt.toml` und `daten/`. `Ok(None)`
/// heißt: gibt es nicht (ungültiger Slug oder keine `projekt.toml`). Ist
/// `projekt.toml` keine richtige Datei (Ordner, Verknüpfung), ist das ein
/// Fehler.
pub fn load_one(dir: &Path, slug: &str) -> Result<Option<Project>> {
    if !is_valid_slug(slug) {
        return Ok(None);
    }
    // Ein Projektordner als Verknüpfung könnte überallhin zeigen: nicht lesen.
    verknuepfung_pruefen(&dir.join(slug), format!("projects/{slug}"), slug)?;
    if std::fs::symlink_metadata(dir.join(slug).join("projekt.toml")).is_err() {
        return Ok(None);
    }
    Ok(Some(load_neu(dir, slug)?))
}

/// Eine Seite, die sich nicht laden ließ. Wird trotzdem angezeigt,
/// damit der Fehler auffällt.
#[derive(Debug, Clone, Serialize, TS)]
pub struct Broken {
    pub slug: String,
    pub error: String,
}

/// Lädt alle Seiten im Ordner, sortiert nach `order` und Titel.
pub fn load_all(dir: &Path) -> (Vec<Project>, Vec<Broken>) {
    let mut projects = Vec::new();
    let mut broken = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return (projects, broken);
    };
    let mut slugs: Vec<String> =
        entries.filter_map(|e| e.ok()).filter(|e| e.path().is_dir()).filter_map(|e| e.file_name().into_string().ok()).filter(|s| is_valid_slug(s)).collect();
    slugs.sort();
    for slug in slugs {
        match load_one(dir, &slug) {
            Ok(Some(p)) => projects.push(p),
            Ok(None) => {}
            Err(e) => broken.push(Broken { slug, error: format!("{e:#}") }),
        }
    }
    projects.sort_by(|a, b| a.meta.order.cmp(&b.meta.order).then_with(|| a.meta.title.cmp(&b.meta.title)));
    (projects, broken)
}

// Die Farben für `status` und `backup` (früher `status_class` und
// `backup_class`) stehen jetzt als Bedeutung in `vertrag::bedeutung`.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vertrag::bedeutung::Bedeutung;

    #[test]
    fn preview_field_only_accepts_plain_html_in_assets() {
        assert_eq!(preview_file("assets/vorschau.html"), Some("vorschau.html"));
        assert_eq!(preview_file("assets/Entwurf-2.htm"), Some("Entwurf-2.htm"));
        assert!(preview_file("vorschau.html").is_none());
        assert!(preview_file("assets/unter/vorschau.html").is_none());
        assert!(preview_file("assets/../page.md").is_none());
        assert!(preview_file("assets/bild.png").is_none());
        assert!(vorschau_mangel("v.html").is_some());
        assert!(vorschau_mangel("assets/v.html").is_none());
    }

    #[test]
    fn slugs() {
        assert!(is_valid_slug("beispiel-a"));
        assert!(is_valid_slug("netz-werk2"));
        assert!(!is_valid_slug("../etc"));
        assert!(!is_valid_slug("Gross"));
        assert!(!is_valid_slug(""));
        assert!(!is_valid_slug("-a"));
    }

    #[test]
    fn status_bedeutungen() {
        let b = |w: &str| Bedeutung::aus_wort_oder_neutral(w);
        assert_eq!(b("aktiv"), Bedeutung::Laeuft);
        assert_eq!(b("Pausiert"), Bedeutung::Wartet);
        assert_eq!(b("fertig"), Bedeutung::Ok);
        assert_eq!(b("irgendwas"), Bedeutung::Neutral);
    }

    fn beispiel() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("beispiel").join("projects")
    }

    #[test]
    fn pfade_vergleichen() {
        let echt = std::fs::canonicalize(beispiel().join("schaufenster").join("assets").join("bild.svg")).unwrap();
        let ordner = std::fs::canonicalize(beispiel()).unwrap().join("schaufenster").join("assets");
        assert!(liegt_direkt_in(&echt, &ordner));
        assert!(!liegt_direkt_in(&echt, &ordner.join("..")));
        assert!(!liegt_direkt_in(&echt, ordner.parent().unwrap()));
        assert!(!ist_verknuepfung(&beispiel().join("schaufenster").join("assets")));
        assert!(!ist_verknuepfung(&beispiel().join("gibt-es-nicht")));
        #[cfg(windows)]
        {
            // Mit und ohne `\\?\`, Groß/Klein egal; Bestandteile, kein Text-Anfang.
            assert!(gleicher_pfad(Path::new(r"\\?\C:\a\Assets"), Path::new(r"c:\A\assets")));
            assert!(!gleicher_pfad(Path::new(r"C:\a\assets2"), Path::new(r"C:\a\assets")));
            assert!(!liegt_direkt_in(Path::new(r"C:\a\assets-x\b.svg"), Path::new(r"C:\a\assets")));
            assert!(!liegt_direkt_in(Path::new(r"C:\a\assets\u\b.svg"), Path::new(r"C:\a\assets")));
        }
    }

    #[test]
    fn neue_form_wird_gelesen() {
        let p = load_one(&beispiel(), "neu-form").unwrap().unwrap();
        assert_eq!(p.meta.status, "aktiv");
        assert_eq!(p.meta.backup.as_deref(), Some("grün"));
        assert_eq!(p.seiten.len(), 2);
        assert!(p.seiten[0].order <= p.seiten[1].order);
        assert!(p.file_modified.is_some());
        let d = p.daten.as_ref().unwrap();
        assert!(d.kennzahlen.is_some() && d.ampel.is_some() && d.fortschritt.is_some());
        assert!(d.zeitstrahl.is_some() && d.gantt.is_some() && d.kanban.is_some());
        assert!(d.fluss.is_some() && d.fragen.is_some());
        assert!(d.eigen.contains_key("notizen"));
    }

    #[test]
    fn kaputte_neue_form_nennt_datei_und_zeile() {
        let e = load_neu(&beispiel(), "neu-kaputt").unwrap_err();
        assert_eq!(e.datei, "projects/neu-kaputt/projekt.toml");
        assert!(e.zeile.is_some());
        assert!(e.text.contains("Bei `status` steht „aktief“. Erlaubt sind: aktiv, pausiert, geplant, fertig, problem, archiviert."), "{e}");
        let e = load_one(&beispiel(), "neu-kaputt").unwrap_err();
        assert!(format!("{e:#}").starts_with("projects/neu-kaputt/projekt.toml, Zeile "), "{e:#}");
    }

    #[test]
    fn alle_projekte() {
        let (projekte, kaputt) = load_all(&beispiel());
        assert!(projekte.iter().any(|p| p.slug == "neu-form") && projekte.iter().any(|p| p.slug == "dashboard"));
        let kaputt: Vec<&str> = kaputt.iter().map(|b| b.slug.as_str()).collect();
        assert_eq!(kaputt, ["kaputt", "neu-kaputt"]);
    }

    #[test]
    fn berechnete_kennzahlen_haben_ihren_wert() {
        let p = load_one(&beispiel(), "neu-daten").unwrap().unwrap();
        let d = p.daten.unwrap();
        let k = d.kennzahlen.unwrap();
        let werte: Vec<Option<&str>> = k.inhalte()[0].kennzahl.iter().map(|k| k.wert.as_deref()).collect();
        assert!(werte.iter().all(Option::is_some), "{werte:?}");
        assert_eq!(&werte[..4], [Some("45 kg"), Some("2 von 4"), Some("50 %"), Some("1.200,5")]);
    }

    #[test]
    fn daten_namen_und_eine_datei() {
        assert_eq!(daten_namen(&beispiel(), "neu-daten").unwrap(), ["eigen-schritte", "fortschritt", "gantt", "kennzahlen"]);
        assert_eq!(daten_namen(&beispiel(), "dashboard").unwrap(), ["gantt", "zeitstrahl"]);
        let heute = chrono::NaiveDate::from_ymd_opt(2026, 10, 27).unwrap();
        let (inhalt, tabelle) = daten_lesen(&beispiel(), "neu-daten", "gantt", heute).unwrap().unwrap();
        assert!(matches!(inhalt, DatenInhalt::Gantt(_)));
        assert_eq!(tabelle["balken"].as_array().unwrap().len(), 4);
        let (inhalt, _) = daten_lesen(&beispiel(), "neu-form", "eigen-notizen", heute).unwrap().unwrap();
        assert!(matches!(inhalt, DatenInhalt::Eigen(_)));
        // Unbekannte Namen, fehlende Dateien, Pfade: nichts.
        for name in ["ampel", "zeitleiste", "../projekt", "eigen-", "Gantt", "gantt.toml"] {
            assert!(daten_lesen(&beispiel(), "neu-daten", name, heute).unwrap().is_none(), "{name}");
        }
        assert!(daten_lesen(&beispiel(), "..", "gantt", heute).unwrap().is_none());
    }

    /// Die Merkliste der Sperren gibt es nur in `mit_merkliste`: Außerhalb
    /// merkt sich niemand etwas (sonst wüchse sie in Fäden ohne Store ohne
    /// Ende), innerhalb schon, verschachtelt auch außen; ein Panik-Abbruch
    /// schließt die Liste wieder.
    #[test]
    fn merkliste_nur_im_rahmen() {
        let pfad = beispiel().join("neu-form").join("projekt.toml");
        let sperre = std::io::Error::from(std::io::ErrorKind::PermissionDenied);
        let fehlt = std::io::Error::from(std::io::ErrorKind::NotFound);
        assert!(!merkliste_offen());
        for _ in 0..1000 {
            gesperrt_merken("draussen.toml", &pfad, &sperre);
        }
        assert!(!merkliste_offen(), "außerhalb bleibt keine Liste");
        let ((), leer) = mit_merkliste(|| ());
        assert!(leer.is_empty(), "nichts von draußen übrig: {leer:?}");

        let ((), aussen) = mit_merkliste(|| {
            gesperrt_merken("a.toml", &pfad, &sperre);
            gesperrt_merken("fehlt.toml", &pfad, &fehlt);
            let ((), innen) = mit_merkliste(|| gesperrt_merken("b.toml", &pfad, &sperre));
            assert_eq!(innen, ["b.toml"]);
            assert!(merkliste_offen());
        });
        assert_eq!(aussen, ["a.toml", "b.toml"]);
        assert!(!merkliste_offen());

        let r = std::panic::catch_unwind(|| mit_merkliste(|| panic!("Abbruch")));
        assert!(r.is_err());
        assert!(!merkliste_offen(), "nach einer Panik ist die Liste wieder zu");
    }
}
