//! Die Prüfung je Projekt (Zielbild, Abschnitt d, „Prüfung je Projekt“):
//! alles, was der Server beim Lesen ablehnt, und dazu, was ihm nur auffällt,
//! als Liste von Befunden statt als Abbruch beim ersten Fehler. Jeder Befund
//! nennt Datei, Zeile (wo sie sich bestimmen lässt), was falsch ist und wie es
//! richtig geht (Abschnitt b, „Fehlermeldungen in einfachem Deutsch“).
//!
//! - **Fehler**: so lässt sich das Projekt nicht (ganz) zeigen; der Bau nimmt es aus.
//! - **Hinweis**: geht, sollte aber anders werden.
//!
//! Benutzt von `dashboard pruefe` (`werkzeug::pruefe`) und `/api/pruefung`.
//! Die Regeln für `assets/` stehen in `assets.rs`.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use serde::Serialize;
use ts_rs::TS;

use super::seite;
use crate::vertrag::abgeleitet::{self, Quellen};
use crate::vertrag::daten::{Frage, ProjektDaten, ist_kennung};
use crate::vertrag::lesen::{LeseFehler, toml_lesen, zeile_mit, zeile_mit_ab, zeile_mit_schluessel, zeile_mit_text};
use crate::vertrag::projekt::{Projekt, ist_seitenname};

/// So lang darf ein offener Punkt (`open`) für die Kachel sein, in Zeichen.
pub const OPEN_HOECHSTENS: usize = 200;

/// Wie schwer ein Befund wiegt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum Schwere {
    /// So lässt sich das Projekt nicht (ganz) zeigen.
    Fehler,
    /// Geht, sollte aber anders werden.
    Hinweis,
}

/// Ein Befund der Prüfung.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct Befund {
    /// Pfad relativ zum Repo, etwa `projects/x/daten/gantt.toml`.
    pub datei: String,
    /// Zeile ab 1; `null`, wenn sie sich nicht bestimmen lässt.
    pub zeile: Option<u32>,
    pub schwere: Schwere,
    /// Was falsch ist und wie es richtig geht, ein bis zwei Sätze.
    pub text: String,
}

impl Befund {
    pub fn fehler(datei: impl Into<String>, zeile: Option<usize>, text: impl Into<String>) -> Self {
        Self::neu(Schwere::Fehler, datei.into(), zeile, text.into())
    }

    pub fn hinweis(datei: impl Into<String>, zeile: Option<usize>, text: impl Into<String>) -> Self {
        Self::neu(Schwere::Hinweis, datei.into(), zeile, text.into())
    }

    /// Jeder Befund verbirgt Profil und Benutzernamen (`datenschutz`): Er geht
    /// über `/api/pruefung` (frei), die Meldung „Prüfung rot“ und die Bauliste
    /// hinaus, und ein Satz zitiert oft, was in der Datei steht (etwa die
    /// Adresse eines Links, `url = 'C:\Users\<name>\…'`).
    fn neu(schwere: Schwere, datei: String, zeile: Option<usize>, text: String) -> Self {
        let (datei, text) = (crate::datenschutz::verbergen(&datei), crate::datenschutz::verbergen(&text));
        Self { datei, zeile: zeile.and_then(|z| u32::try_from(z).ok()), schwere, text }
    }

    /// Ein Lesefehler als Fehler; `versatz` Zeilen stehen davor.
    fn aus_lesefehler(e: LeseFehler, versatz: usize) -> Self {
        Self::fehler(e.datei, e.zeile.map(|z| z + versatz), e.text)
    }

    pub fn ist_fehler(&self) -> bool {
        self.schwere == Schwere::Fehler
    }
}

impl fmt::Display for Befund {
    /// `projects/x/daten/gantt.toml, Zeile 12: …` (ohne Zeile: `projects/x/…: …`).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.zeile {
            Some(z) => write!(f, "{}, Zeile {z}: {}", self.datei, self.text),
            None => write!(f, "{}: {}", self.datei, self.text),
        }
    }
}

/// Zahl der Fehler und Hinweise in `befunde`.
pub fn zaehlen<'a>(befunde: impl IntoIterator<Item = &'a Befund>) -> (usize, usize) {
    befunde.into_iter().fold((0, 0), |(f, h), b| if b.ist_fehler() { (f + 1, h) } else { (f, h + 1) })
}

/// Gibt es das Projekt? Ja, wenn der Slug gültig ist und `projekt.toml` da
/// ist (auch als Ordner oder Verknüpfung: die Prüfung meldet dann, dass es
/// keine Datei ist). Wie der Server (`seite::load_one`).
pub fn ist_projekt(dir: &Path, slug: &str) -> bool {
    seite::is_valid_slug(slug) && std::fs::symlink_metadata(dir.join(slug).join("projekt.toml")).is_ok()
}

/// Wird der Ordner geprüft? Wenn es das Projekt gibt ([`ist_projekt`]) oder
/// wenn im Ordner (mit gültigem Slug) eine Markdown-Datei liegt: Ein Ordner
/// nur mit einer alten `page.md` verschwindet sonst stumm aus der Prüfung.
pub fn wird_geprueft(dir: &Path, slug: &str) -> bool {
    ist_projekt(dir, slug) || (seite::is_valid_slug(slug) && !seite::ist_verknuepfung(&dir.join(slug)) && !markdown_dateien(&dir.join(slug)).is_empty())
}

/// Satz zu einer Markdown-Datei im Projektordner (Etappe 6a): ein Fehler.
pub const SATZ_MARKDOWN: &str = "Markdown-Dateien gibt es im Projekt nicht mehr. Bitte diese Datei löschen; \
     der Kopf gehört in projekt.toml, Daten in daten/*.toml und die Seite in Seite.svelte.";

/// Satz, wenn im Ordner eine Markdown-Datei liegt, aber keine `projekt.toml`.
pub const SATZ_OHNE_KOPF: &str = "projekt.toml fehlt. Ohne sie gibt es das Projekt nicht; bitte projekt.toml anlegen \
     (Vorlage in projects/_vorlage) oder den Ordner löschen.";

/// So tief sucht die Prüfung Markdown-Dateien in Unterordnern.
const MARKDOWN_TIEFE: usize = 8;

/// Alle Dateien mit der Endung `.md` (gleich wie geschrieben) im
/// Projektordner und seinen Unterordnern, als Pfad relativ zum
/// Projektordner mit `/`, sortiert. Verknüpfungen werden nicht verfolgt.
/// `node_modules` bleibt draußen wie beim Scanner und in der tsconfig der
/// Projektprüfung: Projekte bauen mit `web/node_modules`, ein eigenes wäre
/// ein Versehen (`npm install` im Projekt) und brächte je Paket ein README.md.
pub fn markdown_dateien(ordner: &Path) -> Vec<String> {
    fn gehen(ordner: &Path, rel: &str, tiefe: usize, aus: &mut Vec<String>) {
        let Ok(eintraege) = std::fs::read_dir(ordner) else { return };
        for e in eintraege.filter_map(|e| e.ok()) {
            let Ok(name) = e.file_name().into_string() else { continue };
            let Ok(art) = e.file_type() else { continue };
            let pfad = if rel.is_empty() { name.clone() } else { format!("{rel}/{name}") };
            if art.is_dir() && !art.is_symlink() {
                if tiefe < MARKDOWN_TIEFE && name != "node_modules" {
                    gehen(&e.path(), &pfad, tiefe + 1, aus);
                }
            } else if name.to_ascii_lowercase().ends_with(".md") {
                aus.push(pfad);
            }
        }
    }
    let mut aus = Vec::new();
    gehen(ordner, "", 0, &mut aus);
    aus.sort();
    aus
}

/// Prüft ein Projekt. Gibt es das Projekt nicht und liegt auch keine
/// Markdown-Datei im Ordner, ist die Liste leer (vorher mit
/// [`wird_geprueft`] fragen).
pub fn pruefe_projekt(dir: &Path, slug: &str) -> Vec<Befund> {
    let mut befunde = Vec::new();
    if !wird_geprueft(dir, slug) {
        return befunde;
    }
    if seite::ist_verknuepfung(&dir.join(slug)) {
        // Wie der Server: in einen verknüpften Projektordner wird nicht hineingeschaut.
        befunde.push(Befund::fehler(format!("projects/{slug}"), None, seite::verknuepfung_satz(slug)));
        return befunde;
    }
    for md in markdown_dateien(&dir.join(slug)) {
        befunde.push(Befund::fehler(format!("projects/{slug}/{md}"), None, SATZ_MARKDOWN));
    }
    if !ist_projekt(dir, slug) {
        befunde.push(Befund::fehler(format!("projects/{slug}/projekt.toml"), None, SATZ_OHNE_KOPF));
        return befunde;
    }
    neue_form(dir, slug, &mut befunde);
    befunde.extend(super::assets::pruefe_assets(dir, slug));
    // Etappe 4v: Svelte- und TS-Dateien, einmalige Links (werkzeug::scanner).
    befunde.extend(crate::werkzeug::scanner::pruefe_projekt(dir, slug));
    befunde
}

/// Prüft alle Projekte im Ordner (wie `seite::load_all`: nur Ordner mit
/// gültigem Slug und `projekt.toml`, dazu Ordner mit einer Markdown-Datei),
/// auch die ohne Befund.
pub fn pruefe_alle(dir: &Path) -> BTreeMap<String, Vec<Befund>> {
    let Ok(eintraege) = std::fs::read_dir(dir) else {
        return BTreeMap::new();
    };
    eintraege
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|slug| wird_geprueft(dir, slug))
        .map(|slug| {
            let b = pruefe_projekt(dir, &slug);
            (slug, b)
        })
        .collect()
}

/// Wie [`pruefe_alle`] über alle Inhaltsquellen (Etappe 4b, Umzug des
/// Inhalts): je Projekt in der Quelle, in der es zuerst liegt. Mit einer
/// Quelle genau [`pruefe_alle`].
pub fn pruefe_quellen(quellen: &[std::path::PathBuf]) -> BTreeMap<String, Vec<Befund>> {
    match quellen {
        [] => BTreeMap::new(),
        [eine] => pruefe_alle(eine),
        mehrere => crate::quellen::Quellen::aus_liste(mehrere.to_vec())
            .projekte()
            .iter()
            .filter(|(slug, eltern)| wird_geprueft(eltern, slug))
            .map(|(slug, eltern)| (slug.clone(), pruefe_projekt(eltern, slug)))
            .collect(),
    }
}

// ----- Steuerzeichen in Texten (Etappe 3b, Fund des Prüfers) -----

/// Satz zu einem Text, in dem ein Steuerzeichen steckt.
/// Etappe B1: `nan` und `inf` sind in TOML gültig, aber keine Zahl, mit
/// der sich rechnen lässt (im JSON würden sie `null`). Ein Fehler je Zeile.
pub fn nicht_endliche_zahlen(text: &str, datei: &str, befunde: &mut Vec<Befund>) {
    fn hat_nicht_endliche(v: &toml::Value) -> bool {
        match v {
            toml::Value::Float(f) => !f.is_finite(),
            toml::Value::Array(a) => a.iter().any(hat_nicht_endliche),
            toml::Value::Table(t) => t.values().any(hat_nicht_endliche),
            _ => false,
        }
    }
    let quelle = text.strip_prefix('\u{feff}').unwrap_or(text);
    let Ok(tabelle) = quelle.parse::<toml::Table>() else { return };
    if !tabelle.values().any(hat_nicht_endliche) {
        return;
    }
    const WOERTER: [&str; 6] = ["nan", "+nan", "-nan", "inf", "+inf", "-inf"];
    /// Die Zeile ohne Texte in Anführungszeichen (ein „nan“ darin ist keine Zahl).
    fn ohne_texte(zeile: &str) -> String {
        let mut aus = String::new();
        let mut in_text: Option<char> = None;
        let mut escape = false;
        for c in zeile.chars() {
            match in_text {
                Some(q) => {
                    if escape {
                        escape = false;
                    } else if q == '"' && c == '\\' {
                        escape = true;
                    } else if c == q {
                        in_text = None;
                    }
                }
                None if c == '"' || c == '\'' => {
                    in_text = Some(c);
                    aus.push(' ');
                }
                None => aus.push(c),
            }
        }
        aus
    }
    let satz = |w: &str| {
        format!(
            "Hier steht `{w}`. Das ist in TOML erlaubt, aber keine Zahl, mit der der Server rechnen kann. Bitte eine echte Zahl eintragen (etwa `0`) oder das Feld weglassen."
        )
    };
    let vorher = befunde.len();
    for (i, zeile) in quelle.lines().enumerate() {
        let zeile = ohne_texte(zeile);
        let ohne_kommentar = zeile.split('#').next().unwrap_or("");
        // Ohne `=` ist die Zeile die Fortsetzung eines mehrzeiligen Arrays:
        // dann zählt auch das erste Stück (`  nan,`).
        let ab = usize::from(ohne_kommentar.contains('='));
        let gefunden =
            ohne_kommentar.split(['=', ',', '[', '{']).skip(ab).filter_map(|t| t.trim().split([']', '}', ' ', '\t']).next()).find(|t| WOERTER.contains(t));
        if let Some(w) = gefunden {
            befunde.push(Befund::fehler(datei, Some(i + 1), satz(w)));
        }
    }
    // Die Datei hat sicher einen solchen Wert; fand der Blick je Zeile keinen,
    // dann wenigstens ein Fehler ohne Zeile.
    if befunde.len() == vorher {
        befunde.push(Befund::fehler(datei, None, satz("nan` oder `inf")));
    }
}

fn steuerzeichen_satz(was: &str) -> String {
    format!(
        "In einem Text steht {was}. Das wird ein unsichtbares Steuerzeichen, ein Pfad stimmt dann nicht mehr \
         (meist ein einfacher Rückwärtsstrich in doppelten Anführungszeichen, wie in \"D:\\Ablage\\fach\"). \
         Bitte Schrägstrich `/` nehmen (etwa \"D:/Ablage/fach\") oder den Text in einfache Anführungszeichen setzen ('D:\\Ablage\\fach')."
    )
}

/// Steuerzeichen in den Texten einer TOML-Datei: Escapes wie `\t`, `\b`,
/// `\f`, `\r`, `\e` oder `\u0001` in Texten mit doppelten Anführungszeichen,
/// dazu echte Steuerzeichen (etwa ein Tabulator) in jedem Text. Erlaubt ist
/// nur der Zeilenumbruch in mehrzeiligen Texten (`"""…"""`, `'''…'''`). In
/// TOML sind diese Escapes gültig, darum merkt das Lesen nichts; der Pfad
/// `"…\fach"` enthält dann aber einen Seitenvorschub. Ein Hinweis je Zeile;
/// `versatz`: Zeilen vor `text` in der Datei.
pub fn steuerzeichen(text: &str, datei: &str, versatz: usize, befunde: &mut Vec<Befund>) {
    #[derive(Clone, Copy, PartialEq)]
    enum Lage {
        Draussen,
        Kommentar,
        Einfach,
        Woertlich,
        MehrEinfach,
        MehrWoertlich,
    }
    let z: Vec<char> = text.chars().collect();
    let mut lage = Lage::Draussen;
    let mut zeile = 1usize;
    let mut gemeldet = 0usize;
    let mut i = 0;
    let mut melden = |zeile: usize, was: String, befunde: &mut Vec<Befund>| {
        if gemeldet != zeile {
            gemeldet = zeile;
            befunde.push(Befund::hinweis(datei, Some(zeile + versatz), steuerzeichen_satz(&was)));
        }
    };
    let drei = |i: usize, c: char| z.get(i) == Some(&c) && z.get(i + 1) == Some(&c) && z.get(i + 2) == Some(&c);
    while i < z.len() {
        let c = z[i];
        if c == '\n' {
            zeile += 1;
            if matches!(lage, Lage::Kommentar | Lage::Einfach | Lage::Woertlich) {
                lage = Lage::Draussen;
            }
            i += 1;
            continue;
        }
        match lage {
            Lage::Draussen => {
                if c == '#' {
                    lage = Lage::Kommentar;
                } else if drei(i, '"') {
                    lage = Lage::MehrEinfach;
                    i += 3;
                    continue;
                } else if drei(i, '\'') {
                    lage = Lage::MehrWoertlich;
                    i += 3;
                    continue;
                } else if c == '"' {
                    lage = Lage::Einfach;
                } else if c == '\'' {
                    lage = Lage::Woertlich;
                }
            }
            Lage::Kommentar => {}
            Lage::Einfach | Lage::MehrEinfach => {
                let mehr = lage == Lage::MehrEinfach;
                if mehr && drei(i, '"') {
                    lage = Lage::Draussen;
                    i += 3;
                    continue;
                }
                if !mehr && c == '"' {
                    lage = Lage::Draussen;
                } else if c == '\\' {
                    let n = z.get(i + 1).copied().unwrap_or(' ');
                    let hex = |laenge: usize| -> Option<u32> {
                        let s: String = z.get(i + 2..i + 2 + laenge)?.iter().collect();
                        u32::from_str_radix(&s, 16).ok()
                    };
                    let steuer = match n {
                        'b' | 't' | 'f' | 'r' | 'e' => true,
                        'n' => !mehr,
                        'u' => hex(4).is_some_and(|v| v < 0x20 || v == 0x7f),
                        'U' => hex(8).is_some_and(|v| v < 0x20 || v == 0x7f),
                        _ => false,
                    };
                    if steuer {
                        let laenge = match n {
                            'u' => 4,
                            'U' => 8,
                            _ => 0,
                        };
                        let rest: String = z.iter().skip(i + 2).take(laenge).collect();
                        melden(zeile, format!("`\\{n}{rest}`"), befunde);
                    }
                    // Das Zeichen nach dem Rückwärtsstrich gehört zum Escape (auch `\\` und `\"`);
                    // ein Zeilenende darin (Fortsetzung im mehrzeiligen Text) zählt die Schleife.
                    if n != '\n' {
                        i += 2;
                        continue;
                    }
                } else if (c < ' ' && c != '\r') || c == '\u{7f}' {
                    melden(zeile, "ein Tabulator oder anderes Steuerzeichen".to_string(), befunde);
                }
            }
            Lage::Woertlich | Lage::MehrWoertlich => {
                let mehr = lage == Lage::MehrWoertlich;
                if mehr && drei(i, '\'') {
                    lage = Lage::Draussen;
                    i += 3;
                    continue;
                }
                if !mehr && c == '\'' {
                    lage = Lage::Draussen;
                } else if (c < ' ' && c != '\r') || c == '\u{7f}' {
                    melden(zeile, "ein Tabulator oder anderes Steuerzeichen".to_string(), befunde);
                }
            }
        }
        i += 1;
    }
}

// ----- Neue Form -----

fn neue_form(dir: &Path, slug: &str, befunde: &mut Vec<Befund>) {
    let ordner = dir.join(slug);
    let kopf_datei = format!("projects/{slug}/projekt.toml");
    match neu_lesen(&ordner.join("projekt.toml"), &kopf_datei) {
        Err(b) => befunde.push(b),
        Ok(text) => match toml_lesen::<Projekt>(&kopf_datei, &text) {
            Err(e) => befunde.push(Befund::aus_lesefehler(e, 0)),
            Ok(kopf) => {
                steuerzeichen(&text, &kopf_datei, 0, befunde);
                kopf_pruefen(&ordner, slug, &kopf, &text, &kopf_datei, befunde);
            }
        },
    }
    daten_pruefen(&ordner.join("daten"), slug, befunde);
}

/// Eine Datei der neuen Form (siehe `seite::neue_datei_lesen`), ohne BOM.
fn neu_lesen(pfad: &Path, datei: &str) -> Result<String, Befund> {
    let text = seite::neue_datei_lesen(pfad, datei).map_err(|e| Befund::aus_lesefehler(e, 0))?;
    Ok(match text.strip_prefix('\u{feff}') {
        Some(rest) => rest.to_string(),
        None => text,
    })
}

fn kopf_pruefen(ordner: &Path, slug: &str, kopf: &Projekt, quelle: &str, datei: &str, befunde: &mut Vec<Befund>) {
    if let Err(m) = kopf.pruefen() {
        befunde.push(Befund::aus_lesefehler(m.in_datei(datei, quelle), 0));
    }
    // Entscheidung archiv-status: seit wann archiviert.
    match (kopf.status == crate::vertrag::projekt::Status::Archiviert, kopf.archiviert_seit.as_deref()) {
        (true, None) => befunde.push(Befund::hinweis(
            datei,
            zeile_mit(quelle, "status", "archiviert"),
            "Das Projekt ist archiviert, aber `archiviert_seit` fehlt. Bitte das Datum dazuschreiben, etwa `archiviert_seit = \"2026-11-01\"`; bis dahin zeigt das Archiv das Datum der Datei.",
        )),
        (false, Some(s)) => befunde.push(Befund::hinweis(
            datei,
            zeile_mit(quelle, "archiviert_seit", s),
            "`archiviert_seit` gilt nur mit `status = \"archiviert\"`; bei einem anderen Status bitte die Zeile entfernen.",
        )),
        _ => {}
    }
    if let Some(v) = kopf.vorschau.as_deref() {
        vorschau_pruefen(ordner, v, datei, zeile_mit(quelle, "vorschau", v), befunde);
    }
    if kopf.summary.trim().is_empty() {
        befunde.push(Befund::hinweis(
            datei,
            zeile_mit_schluessel(quelle, "summary"),
            "`summary` ist leer. Ein Satz für die Kachel hilft dem Inhaber, etwa `summary = \"Worum es geht, in einem Satz.\"`",
        ));
    }
    for o in &kopf.open {
        let n = o.chars().count();
        if n > OPEN_HOECHSTENS {
            let anfang: String = o.chars().take(30).collect();
            befunde.push(Befund::hinweis(
                datei,
                zeile_mit_text(quelle, &anfang),
                format!("Der offene Punkt „{anfang}…“ ist {n} Zeichen lang. Für die Kachel bitte höchstens {OPEN_HOECHSTENS} Zeichen; Einzelheiten gehören auf die Seite."),
            ));
        }
    }
    let mut gesehen = std::collections::BTreeSet::new();
    for s in &kopf.seiten {
        if ist_seitenname(&s.name) && gesehen.insert(s.name.as_str()) && !ordner.join("seiten").join(format!("{}.svelte", s.name)).is_file() {
            befunde.push(Befund::hinweis(
                datei,
                zeile_mit(quelle, "name", &s.name),
                format!(
                    "Zur Unterseite „{}“ gibt es noch keine Datei `projects/{slug}/seiten/{}.svelte`. Die Seiten baut Etappe 4; bis dahin zeigt das Dashboard nur den Titel.",
                    s.name, s.name
                ),
            ));
        }
    }
    for l in &kopf.links {
        if !ist_link_adresse(&l.url) {
            befunde.push(Befund::fehler(
                datei,
                zeile_mit(quelle, "url", &l.url),
                format!(
                    "Der Link „{}“ hat die Adresse „{}“. Erlaubt sind Adressen wie `url = \"https://example.org\"` oder ein Ordner wie `url = \"D:/Ablage/Projekte\"`.",
                    l.name, l.url
                ),
            ));
        }
    }
    for c in &kopf.checks {
        if !ist_http_adresse(&c.url) {
            befunde.push(Befund::fehler(
                datei,
                zeile_mit(quelle, "url", &c.url),
                format!(
                    "Das Lebenszeichen „{}“ hat die Adresse „{}“. Erlaubt ist nur http:// oder https://, etwa `url = \"http://127.0.0.1:8787/healthz\"`.",
                    c.name, c.url
                ),
            ));
        }
    }
}

/// `vorschau` muss die Form `assets/<name>.html` haben und auf eine Datei zeigen.
fn vorschau_pruefen(ordner: &Path, v: &str, datei: &str, zeile: Option<usize>, befunde: &mut Vec<Befund>) {
    if let Some(m) = seite::vorschau_mangel(v) {
        befunde.push(Befund::fehler(datei, zeile, m.text));
        return;
    }
    if !ordner.join(v).is_file() {
        befunde.push(Befund::fehler(
            datei,
            zeile,
            format!("Bei `vorschau` steht „{v}“, aber diese Datei gibt es nicht. Bitte die Datei anlegen oder `vorschau` weglassen."),
        ));
    }
}

/// Jede Datei in `daten/` für sich: ein Fehler in einer hält die anderen nicht auf.
fn daten_pruefen(daten_ordner: &Path, slug: &str, befunde: &mut Vec<Befund>) {
    if seite::ist_verknuepfung(daten_ordner) {
        befunde.push(Befund::fehler(format!("projects/{slug}/daten"), None, seite::verknuepfung_satz("daten")));
        return;
    }
    if !daten_ordner.is_dir() {
        return;
    }
    let Ok(eintraege) = std::fs::read_dir(daten_ordner) else {
        befunde.push(Befund::fehler(format!("projects/{slug}/daten"), None, "Der Ordner `daten/` lässt sich nicht lesen."));
        return;
    };
    let mut namen: Vec<(String, PathBuf)> = eintraege.filter_map(|e| e.ok()).filter_map(|e| Some((e.file_name().into_string().ok()?, e.path()))).collect();
    namen.sort();
    // Für `berechnet` in kennzahlen.toml: alle Dateien als Tabelle, kaputte mit `None`.
    let mut quellen = Quellen::new();
    let mut kennzahlen = None;
    for (name, pfad) in namen {
        let datei = format!("projects/{slug}/daten/{name}");
        let stamm = name.strip_suffix(".toml").map(str::to_string);
        if name.to_ascii_lowercase().ends_with(".md") {
            continue; // schon als Markdown-Datei gemeldet (markdown_dateien)
        }
        if seite::ist_echter_ordner(&pfad) {
            befunde.push(Befund::fehler(datei, None, "In `daten/` sind keine Unterordner erlaubt. Mehrere Teile gehen als `[teil.<name>]` in einer Datei."));
            continue;
        }
        let text = match neu_lesen(&pfad, &datei) {
            Ok(t) => t,
            Err(b) => {
                befunde.push(b);
                quellen.extend(stamm.map(|s| (s, None)));
                continue;
            }
        };
        let mut daten = ProjektDaten::default();
        match daten.datei_einlesen(&name, &datei, &text) {
            Err(e) => {
                befunde.push(Befund::aus_lesefehler(e, 0));
                quellen.extend(stamm.map(|s| (s, None)));
            }
            Ok(()) => {
                steuerzeichen(&text, &datei, 0, befunde);
                nicht_endliche_zahlen(&text, &datei, befunde);
                nicht_angezeigt(&daten, &text, &datei, befunde);
                kanban_hinweis(&daten, &datei, befunde);
                zeitstrahl_hinweise(&daten, &text, &datei, befunde);
                fristen_hinweise(&daten, &text, &datei, abgeleitet::heute(), befunde);
                if let Some(f) = &daten.fragen {
                    let fragen: Vec<Frage> = f.inhalte().into_iter().flat_map(|t| t.frage.clone()).collect();
                    fragen_hinweise(&fragen, &text, &datei, befunde);
                }
                if let Some(o) = &daten.orte {
                    // Entscheidung lokal-oeffnen: Verstöße schon beim Bau melden.
                    let projekte = daten_ordner.parent().and_then(Path::parent).unwrap_or(Path::new("."));
                    orte_befunde(o, &text, &datei, &orte_regeln_beim_bau(projekte), befunde);
                }
                if let Some(s) = stamm {
                    quellen.insert(s, toml_lesen::<toml::Table>(&datei, &text).ok());
                }
                if let Some(k) = daten.kennzahlen.take() {
                    kennzahlen = Some((k, datei, text));
                }
            }
        }
    }
    if let Some((mut k, datei, text)) = kennzahlen {
        for e in abgeleitet::kennzahlen_rechnen(&mut k, &quellen, abgeleitet::heute(), &datei, &text) {
            befunde.push(Befund::aus_lesefehler(e, 0));
        }
    }
}

// ----- Orte (Entscheidung lokal-oeffnen) -----

/// Regeln für die Prüfung beim Bau (`pruefe`, Bauliste, `/api/pruefung` und
/// die Meldung „Prüfung rot“ im Dienst): das Profil des Inhabers
/// ([`crate::oeffnen::profil_fuer_pruefung`], im Dienst über seine SID, nie das
/// Profil des Dienstkontos), die Inhaltsquellen, ProgramData und das Programm,
/// die Tabu-Liste aus dem Daten-Ordner des Inhalts. Fehlt sie, prüft `pruefe`
/// den Rest und sagt es; Server und Bote öffnen ohne sie nichts.
pub fn orte_regeln_beim_bau(projekte: &Path) -> crate::oeffnen::Regeln {
    let daten = crate::app::data_file(projekte).parent().map(Path::to_path_buf).unwrap_or_default();
    let liste = daten.join(crate::dienst::einstellungen::AUSSCHLUSS_DATEI);
    let mut quellen = crate::app::quellen().wurzeln();
    quellen.push(projekte.to_path_buf());
    crate::oeffnen::Regeln {
        tabu_pflicht: false,
        ..crate::oeffnen::Regeln::des_servers(
            crate::oeffnen::profil_fuer_pruefung(),
            quellen,
            crate::oeffnen::ordner_fuer_pruefung(),
            crate::pc::inventory::ausschluss_laden(Some(&liste)).ok(),
        )
    }
}

/// Je Ort, der gegen die Regeln verstößt, ein Fehler (er ginge nie auf). Die
/// Sätze nennen die Kennung, nie den Pfad.
///
/// Geprüft wird nur der Text (`oeffnen::vorpruefen`: Form, Tabu, Lage unter
/// einer erlaubten Wurzel) und die verbotenen Endungen, nie die Platte: Die
/// Prüfung läuft auch im Dienst, dessen Konto das Profil des Inhabers nicht
/// lesen darf. Ob es den Ort gibt und wohin er aufgelöst zeigt, prüft allein
/// der Bote beim Öffnen (Entscheidung lokal-oeffnen; sonst
/// stünde „Probe-Orte fehlen“ als Fehler, nur im Dienst). Ist das
/// Profil des Inhabers unbekannt, entfällt die Lage; ein Hinweis sagt es.
pub fn orte_befunde(
    orte: &crate::vertrag::daten::Datei<crate::vertrag::daten::Orte>,
    quelle: &str,
    datei: &str,
    regeln: &crate::oeffnen::Regeln,
    befunde: &mut Vec<Befund>,
) {
    use crate::oeffnen::{Abweisung, VERBOTENE_ENDUNGEN, vorpruefen};
    let alle: Vec<_> = orte.inhalte().into_iter().flat_map(|t| &t.ort).collect();
    if alle.is_empty() {
        return;
    }
    if regeln.tabu.is_none() {
        befunde.push(Befund::hinweis(
            datei,
            None,
            "Die Liste der Tabu-Ordner fehlt; pruefe prüft die Orte ohne sie. Server und Bote öffnen ohne Liste nichts.",
        ));
    }
    if regeln.profil.is_none() {
        befunde.push(Befund::hinweis(datei, None, SATZ_ORTE_OHNE_PROFIL));
    }
    for o in alle {
        let zeile = zeile_mit(quelle, "kennung", &o.kennung);
        // Eine Endung außerhalb der Positivliste ist ein Fehler, auch wenn es
        // die Datei (noch) nicht gibt (wie im Boten: `ENDUNGEN`, zip als Ordner).
        let endung = ort_endung(&o.pfad);
        let ergebnis = match endung {
            Some(e) if VERBOTENE_ENDUNGEN.contains(&e.as_str()) => Err((Abweisung::Endung(e), "")),
            Some(e) if !regeln.endungen.contains(&e) && !regeln.explorer_endungen.contains(&e) => Err((Abweisung::Endung(e), SATZ_ORDNER_MIT_PUNKT)),
            _ => match vorpruefen(&o.pfad, regeln) {
                // Ohne bekanntes Profil lässt sich die Lage nicht sagen (der Hinweis oben sagt, was dann gilt).
                Err(Abweisung::AusserhalbWurzel) if regeln.profil.is_none() => Ok(()),
                r => r.map_err(|a| (a, "")),
            },
        };
        if let Err((a, dazu)) = ergebnis {
            befunde.push(Befund::fehler(datei, zeile, format!("Der Ort „{}“ geht nie auf: {}{dazu}", o.kennung, a.satz())));
        }
    }
}

/// Hinweis bei Orten, wenn das Profil des Inhabers unbekannt ist: Dann öffnet
/// der Server gar nichts (503 „profil-fehlt“), auch keinen Ort im Profil.
pub const SATZ_ORTE_OHNE_PROFIL: &str = "Das Profil des Inhabers ist hier unbekannt (im Dienst fehlt seine SID in den Einstellungen, oder Windows nennt zu ihr kein Profil). \
     Solange geht kein Ort auf, auch keiner in deinem Profil; pruefe prüft die Orte darum nur auf Form, Endung und Tabu.";

/// Zusatz bei einer unbekannten Endung: Vielleicht ist es ein Ordner mit Punkt im Namen.
pub const SATZ_ORDNER_MIT_PUNKT: &str = " Ist es ein Ordner mit Punkt im Namen, bitte mit `\\` am Ende eintragen.";

/// Die Endung des letzten Teils (klein), wie der Bote sie liest: keine bei
/// `\` oder `/` am Ende (ein Ordner), bei einem Namen wie `.ssh` (Punkt vorn)
/// und ohne Punkt. Als Endung zählt nur, was wie eine aussieht: bis 16
/// Zeichen aus Buchstaben, Ziffern und `-`, mit einem Buchstaben (so ist
/// `Fotos_etc._` ein Ordnername, keine Datei mit der Endung `_`; beim
/// echten Inhalt ein Ordner, der aufgeht).
pub fn ort_endung(pfad: &str) -> Option<String> {
    let name = pfad.rsplit(['\\', '/']).next().unwrap_or("");
    let wie_endung = |e: &str| e.len() <= 16 && e.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') && e.chars().any(|c| c.is_ascii_alphabetic());
    name.rsplit_once('.').filter(|(stamm, e)| !stamm.is_empty() && wie_endung(e)).map(|(_, e)| e.to_lowercase())
}

/// Felder, die der Vertrag kennt, die die Oberfläche aber nicht zeigt
/// (siehe Kopf von `web/src/lib/daten.ts`): je gesetztem Feld ein Hinweis.
/// Bis 3b hieß es „bis Etappe 3a“; 3a hat sie nicht eingebaut, Farben wählt
/// die Oberfläche selbst (Grenze), Text an Fluss-Pfeilen zeichnet sie nicht.
fn nicht_angezeigt(daten: &ProjektDaten, quelle: &str, datei: &str, befunde: &mut Vec<Befund>) {
    let mut suche = Suche { quelle, datei, befunde, ab: 0 };
    for k in daten.kennzahlen.iter().flat_map(|d| d.inhalte()) {
        for z in &k.kennzahl {
            suche.eintrag(("titel", &z.titel), "farbe", z.farbe.is_some(), "Kennzahlen");
        }
    }
    suche.ab = 0;
    for k in daten.kanban.iter().flat_map(|d| d.inhalte()) {
        for s in &k.spalte {
            suche.eintrag(("titel", &s.titel), "", false, "");
            for c in &s.karte {
                suche.eintrag(("titel", &c.titel), "farbe", c.farbe.is_some(), "Kanban-Karten");
            }
        }
    }
    suche.ab = 0;
    for f in daten.fluss.iter().flat_map(|d| d.inhalte()) {
        for k in &f.kasten {
            suche.eintrag(("id", &k.id), "farbe", k.farbe.is_some(), "Fluss-Kästen");
        }
    }
    suche.ab = 0;
    for f in daten.fluss.iter().flat_map(|d| d.inhalte()) {
        for p in &f.pfeil {
            suche.eintrag(("nach", &p.nach), "text", p.text.is_some(), "Fluss-Pfeilen");
        }
    }
}

/// Geht die Einträge einer Datei in Reihenfolge durch und findet ihre Zeilen.
struct Suche<'a> {
    quelle: &'a str,
    datei: &'a str,
    befunde: &'a mut Vec<Befund>,
    /// So viele Zeilen sind schon vergeben.
    ab: usize,
}

impl Suche<'_> {
    /// Der nächste Eintrag mit `anker` (etwa `titel = "Budget"`); ist `gesetzt`,
    /// ein Hinweis auf das Feld `feld` in der Zeile, in der es im Eintrag steht.
    fn eintrag(&mut self, anker: (&str, &str), feld: &str, gesetzt: bool, wo: &str) {
        let zeile = zeile_mit_ab(self.quelle, anker.0, anker.1, self.ab);
        if let Some(z) = zeile {
            self.ab = z;
        }
        if gesetzt {
            let zeile = zeile.map(|z| feld_im_eintrag(self.quelle, z, feld).unwrap_or(z));
            let grund = if feld == "farbe" { "Farben wählt die Oberfläche selbst" } else { "die Oberfläche zeichnet keinen Text an Pfeilen" };
            self.befunde.push(Befund::hinweis(
                self.datei,
                zeile,
                format!("`{feld}` bei {wo} wird nicht angezeigt ({grund}); die Seite sieht ohne das Feld gleich aus."),
            ));
        }
    }
}

/// Zeile (ab 1) von `feld = …` im selben Eintrag wie Zeile `zeile`: zwischen
/// der Tabellenzeile davor (`[[…]]`) und der danach.
fn feld_im_eintrag(quelle: &str, zeile: usize, feld: &str) -> Option<usize> {
    let zeilen: Vec<&str> = quelle.lines().collect();
    let tabelle = |l: &&str| l.trim_start().starts_with('[');
    let von = zeilen[..zeile.min(zeilen.len())].iter().rposition(&tabelle).map(|i| i + 1).unwrap_or(0);
    let bis = zeilen[zeile.min(zeilen.len())..].iter().position(tabelle).map(|i| i + zeile).unwrap_or(zeilen.len());
    zeilen[von..bis].iter().position(|l| l.split_once('=').is_some_and(|(k, _)| k.trim() == feld)).map(|i| von + i + 1)
}

/// `http://…` oder `https://…` mit einem Rechnernamen, ohne Leerzeichen.
pub fn ist_http_adresse(u: &str) -> bool {
    let Some(rest) = u.strip_prefix("https://").or_else(|| u.strip_prefix("http://")) else {
        return false;
    };
    let rechner = rest.split(['/', '?', '#']).next().unwrap_or("");
    !rechner.is_empty()
        && !u.chars().any(|c| c.is_whitespace() || c.is_control())
        && rechner.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | ':' | '[' | ']' | '@' | '%'))
}

/// Was in `[[links]]` als `url` stehen darf: eine Web-Adresse, `mailto:`,
/// `file:///…`, ein Laufwerkspfad (`C:/…`) oder ein Weg im Dashboard (`/…`).
pub fn ist_link_adresse(u: &str) -> bool {
    if u.trim().is_empty() || u.trim() != u || u.chars().any(char::is_control) {
        return false;
    }
    if ist_http_adresse(u) {
        return true;
    }
    if let Some(r) = u.strip_prefix("mailto:") {
        return r.contains('@') && !r.contains(char::is_whitespace);
    }
    if u.starts_with("file:///") {
        return true;
    }
    let b = u.as_bytes();
    if b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'/' || b[2] == b'\\') {
        return true;
    }
    u.starts_with('/') && !u.starts_with("//") && !u.contains(char::is_whitespace)
}

// ----- Schwerpunkte (Etappe A1) -----

/// Prüft `data/schwerpunkte.toml` gegen die Projekte aller Inhaltsquellen
/// (`quellen` wie bei [`pruefe_quellen`]):
/// fehlt die Datei, ein Hinweis; bricht sie den Vertrag, ein Fehler. Ein
/// Projekt in zwei Schwerpunkten (oder zweimal in einem) und ein Name, der
/// doppelt vorkommt, sind Fehler; ein Projekt, das es nicht gibt, und ein
/// Projekt ohne Zuordnung (es steht dann unter „Sonstiges“) sind Hinweise.
pub fn pruefe_schwerpunkte(datei_pfad: &Path, quellen: &[std::path::PathBuf]) -> Vec<Befund> {
    use crate::vertrag::schwerpunkte::{DATEI, SONSTIGES, Schwerpunkte};
    let mut befunde = Vec::new();
    if std::fs::symlink_metadata(datei_pfad).is_err() {
        befunde.push(Befund::hinweis(DATEI, None, format!("{DATEI} fehlt; in der Übersicht „Anstehend“ stehen alle Projekte unter „{SONSTIGES}“.")));
        return befunde;
    }
    let text = match neu_lesen(datei_pfad, DATEI) {
        Ok(t) => t,
        Err(b) => return vec![b],
    };
    let s = match Schwerpunkte::lesen(&text) {
        Ok(s) => s,
        Err(e) => return vec![Befund::aus_lesefehler(e, 0)],
    };
    let mut namen = std::collections::BTreeSet::new();
    for sp in &s.schwerpunkt {
        if sp.name.trim().is_empty() {
            befunde.push(Befund::fehler(
                DATEI,
                zeile_mit(&text, "name", &sp.name),
                "Ein Schwerpunkt hat keinen Namen. Beispiel: `name = \"Bienen und Honig\"`",
            ));
        } else if !namen.insert(sp.name.as_str()) {
            let zeile = zeile_mit_ab(&text, "name", &sp.name, zeile_mit(&text, "name", &sp.name).unwrap_or(0));
            befunde.push(Befund::fehler(DATEI, zeile, format!("Den Schwerpunkt „{}“ gibt es zweimal. Jeder Name nur einmal.", sp.name)));
        }
    }
    // Alle Quellen, wie `pruefe_quellen` (sortiert, da BTreeMap).
    let da: Vec<String> = crate::quellen::Quellen::aus_liste(quellen.to_vec())
        .projekte()
        .iter()
        .filter(|(slug, eltern)| ist_projekt(eltern, slug))
        .map(|(slug, _)| slug.clone())
        .collect();
    // Wo steht ein Projekt? Je Vorkommen der Name des Schwerpunkts.
    let mut wo: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for sp in &s.schwerpunkt {
        for p in &sp.projekte {
            wo.entry(p.as_str()).or_default().push(sp.name.as_str());
        }
    }
    for (p, schwerpunkte) in &wo {
        if schwerpunkte.len() > 1 {
            let zeile = nte_zeile_mit(&text, &format!("\"{p}\""), 2);
            let liste = schwerpunkte.iter().map(|n| format!("„{n}“")).collect::<Vec<_>>().join(" und ");
            befunde.push(Befund::fehler(DATEI, zeile, format!("Das Projekt „{p}“ steht mehrfach ({liste}). Jedes Projekt gehört zu genau einem Schwerpunkt.")));
        }
        if !da.iter().any(|d| d == p) {
            befunde.push(Befund::hinweis(
                DATEI,
                nte_zeile_mit(&text, &format!("\"{p}\""), 1),
                format!("Das Projekt „{p}“ steht hier, aber es gibt keinen Ordner projects/{p} (vertippt, oder noch nicht angelegt?)."),
            ));
        }
    }
    for p in da.iter().filter(|p| !wo.contains_key(p.as_str())) {
        befunde.push(Befund::hinweis(
            DATEI,
            None,
            format!("Das Projekt „{p}“ hat keinen Schwerpunkt und steht unter „{SONSTIGES}“. Bitte in {DATEI} bei `projekte` eintragen."),
        ));
    }
    befunde
}

// ----- Projektgruppen (Entscheidung projektgruppe) -----

/// Prüft `data/gruppen.toml` gegen die Projekte aller Inhaltsquellen
/// (`quellen` wie bei [`pruefe_quellen`]). Fehlt
/// die Datei, nichts (dann gibt es keine Gruppen). Fehler sind: ein Bruch des
/// Vertrags, ein leerer oder ungültiger Slug, ein Slug zweimal, ein Slug, den
/// es als Projekt schon gibt (eigener Raum unter `/gruppen/`), ein leerer
/// Titel, ein Teil, den es nicht gibt, und ein Teil, der doppelt steht (in
/// einer Gruppe oder in zwei; ein Projekt gehört höchstens einer Gruppe an).
/// Eine Gruppe ohne Teile ist ein Hinweis (sie erscheint nicht).
pub fn pruefe_gruppen(datei_pfad: &Path, quellen: &[std::path::PathBuf]) -> Vec<Befund> {
    use crate::vertrag::gruppen::{DATEI, Gruppen};
    if std::fs::symlink_metadata(datei_pfad).is_err() {
        return Vec::new();
    }
    let q = crate::quellen::Quellen::aus_liste(quellen.to_vec());
    let ist_da = |slug: &str| q.eltern(slug).is_some_and(|e| ist_projekt(e, slug));
    let text = match neu_lesen(datei_pfad, DATEI) {
        Ok(t) => t,
        Err(b) => return vec![b],
    };
    let g = match Gruppen::lesen(&text) {
        Ok(g) => g,
        Err(e) => return vec![Befund::aus_lesefehler(e, 0)],
    };
    // Die Zeile eines Teils: die erste Zeile ab `ab` (Zeile der Gruppe), in der er in Anführungszeichen steht.
    let zeile_teil = |ab: usize, teil: &str| -> Option<usize> {
        let stueck = format!("\"{teil}\"");
        text.lines()
            .enumerate()
            .skip(ab.saturating_sub(1))
            .find(|(_, l)| {
                let k = l.split_once('=').map(|(k, _)| k.trim()).unwrap_or("");
                k != "slug" && k != "titel" && l.contains(&stueck)
            })
            .map(|(i, _)| i + 1)
    };
    let mut befunde = Vec::new();
    let mut slugs = std::collections::BTreeSet::new();
    // Wo steht ein Teil schon? Teil → Titel der Gruppe.
    let mut vergeben: BTreeMap<&str, &str> = BTreeMap::new();
    let mut ab = 0;
    for gr in &g.gruppe {
        let zeile = zeile_mit_ab(&text, "slug", &gr.slug, ab).or_else(|| zeile_mit(&text, "slug", &gr.slug));
        ab = zeile.unwrap_or(ab);
        let name = if gr.titel.trim().is_empty() { gr.slug.as_str() } else { gr.titel.as_str() };
        if !seite::is_valid_slug(&gr.slug) {
            befunde.push(Befund::fehler(
                DATEI,
                zeile,
                format!("Der Slug „{}“ geht nicht: nur Kleinbuchstaben a-z, Ziffern und Bindestrich, nicht mit Bindestrich vorn. Beispiel: `slug = \"alpha-und-beta\"`", gr.slug),
            ));
        } else if !slugs.insert(gr.slug.as_str()) {
            befunde.push(Befund::fehler(DATEI, zeile, format!("Die Gruppe „{}“ gibt es zweimal. Jeder Slug nur einmal.", gr.slug)));
        } else if ist_da(&gr.slug) {
            befunde.push(Befund::fehler(
                DATEI,
                zeile,
                format!("Den Slug „{}“ gibt es schon als Projekt. Eine Gruppe braucht einen eigenen Slug, etwa „{}-gruppe“.", gr.slug, gr.slug),
            ));
        }
        if gr.titel.trim().is_empty() {
            befunde.push(Befund::fehler(DATEI, zeile, format!("Die Gruppe „{}“ hat keinen Titel. Beispiel: `titel = \"Alpha und Beta\"`", gr.slug)));
        }
        if gr.teile.is_empty() {
            befunde.push(Befund::hinweis(DATEI, zeile, format!("Die Gruppe „{name}“ hat keine Teile und erscheint nicht.")));
        }
        for t in &gr.teile {
            let zt = zeile_teil(ab, t);
            if let Some(vorher) = vergeben.get(t.as_str()) {
                let wo = if *vorher == name { "in dieser Gruppe schon einmal".to_string() } else { format!("schon in der Gruppe „{vorher}“") };
                befunde.push(Befund::fehler(DATEI, zt, format!("Das Projekt „{t}“ steht {wo}. Ein Projekt gehört höchstens einer Gruppe an, und nur einmal.")));
                continue;
            }
            vergeben.insert(t.as_str(), name);
            if !ist_da(t) {
                befunde.push(Befund::fehler(
                    DATEI,
                    zt,
                    format!("Das Projekt „{t}“ in der Gruppe „{name}“ gibt es nicht (kein Ordner projects/{t} mit projekt.toml; vertippt?)."),
                ));
            }
        }
    }
    befunde
}

// ----- PC-Eckdaten -----

/// Prüft `data/pc.toml` gegen den Vertrag (`pc::pcdata`): ein Fehler mit
/// Zeile, etwa bei einem falschen Wort in `cloud_stand`; ein Hinweis, wenn
/// `cloud` etwas sagt, aber `cloud_stand` fehlt (dann gilt `teilweise`, auch
/// bei Texten wie „keine“, die der Server nicht deutet). Dazu die `kennung`
/// in `[[history]]` (Etappe 2e, [`historie_kennungen`]). Fehlt die Datei,
/// nichts (der Server nimmt dann die im Repo).
pub fn pruefe_pc(datei_pfad: &Path) -> Vec<Befund> {
    use crate::pc::pcdata::{self, DATEI};
    if !datei_pfad.is_file() {
        return Vec::new();
    }
    let text = match std::fs::read_to_string(datei_pfad) {
        Ok(t) => t,
        Err(e) => return vec![Befund::fehler(DATEI, None, format!("{DATEI} lässt sich nicht lesen: {e}"))],
    };
    match pcdata::lesen(&text) {
        Ok(pc) => {
            let mut befunde = Vec::new();
            if pc.backup.cloud_stand.is_none() && !pc.backup.cloud.trim().is_empty() {
                let zeile = text.lines().position(|z| z.trim_start().starts_with("cloud")).map(|i| i + 1);
                befunde.push(Befund::hinweis(DATEI, zeile, "Bitte cloud_stand setzen (vorhanden, teilweise oder fehlt)."));
            }
            historie_kennungen(&pc.history, &text, &mut befunde);
            befunde
        }
        Err(e) => vec![Befund::aus_lesefehler(e, 0)],
    }
}

/// Etappe 2e: `kennung` je Eintrag in `[[history]]`. Keine einfache oder eine
/// doppelte ist ein Fehler (Zeile der zweiten), eine fehlende ein Hinweis
/// (bis Schritt 6 erlaubt; der Verlauf bildet sie dann aus Datum und Titel).
fn historie_kennungen(history: &[crate::pc::pcdata::HistoryEntry], text: &str, befunde: &mut Vec<Befund>) {
    use crate::pc::pcdata::DATEI;
    let mut gesehen = std::collections::BTreeMap::new();
    for (i, h) in history.iter().enumerate() {
        let eintrag = nte_zeile_mit(text, "[[history]]", i + 1);
        match h.kennung.as_deref() {
            None => befunde.push(Befund::hinweis(
                DATEI,
                eintrag,
                format!(
                    "Der Eintrag „{}“ in [[history]] hat keine `kennung`. Bitte eine eindeutige setzen, etwa `kennung = \"{}\"` (bis Schritt 6 erlaubt; bis dahin bildet der Server sie aus Datum und Titel).",
                    h.title,
                    vorschlag_kennung(&h.title)
                ),
            )),
            Some(k) if !ist_kennung(k) => befunde.push(Befund::fehler(
                DATEI,
                eintrag.and_then(|z| zeile_mit_ab(text, "kennung", k, z - 1)).or(eintrag),
                format!("Der Eintrag „{}“ hat keine einfache `kennung`. Erlaubt sind Kleinbuchstaben, Ziffern und Bindestriche, etwa `kennung = \"netzteil-getauscht\"`.", h.title),
            )),
            Some(k) => {
                if gesehen.insert(k, eintrag).is_some() {
                    befunde.push(Befund::fehler(
                        DATEI,
                        eintrag.and_then(|z| zeile_mit_ab(text, "kennung", k, z - 1)).or(eintrag),
                        format!("Die Kennung „{k}“ gibt es in [[history]] zweimal. Jede `kennung` nur einmal."),
                    ));
                }
            }
        }
    }
}

/// Vorschlag für eine Kennung aus einem Titel (für Hinweise).
fn vorschlag_kennung(titel: &str) -> String {
    let k = crate::vertrag::verlauf::kennung_aus_text(titel);
    if k.is_empty() { "eintrag-1".to_string() } else { k }
}

/// Etappe B3: Hinweis zu `daten/fristen.toml`: ein Ereignis mit einem Datum in
/// der Zukunft. Ein Ereignis bekommt sein Datum, wenn es eingetreten ist; ein
/// Datum in der Zukunft ist meist ein Versehen (oder ein Termin, der eine feste
/// Frist wäre). Der Server rechnet trotzdem damit. Alles andere (unbekanntes
/// Ereignis, doppelte Kennung, Abstand, Frist ohne Datum und Ereignis) ist
/// schon beim Lesen ein Fehler (`vertrag::fristen`).
fn fristen_hinweise(daten: &ProjektDaten, quelle: &str, datei: &str, heute: chrono::NaiveDate, befunde: &mut Vec<Befund>) {
    for e in daten.fristen.iter().flat_map(|d| d.inhalte()).flat_map(|t| &t.ereignis) {
        let Some(d) = e.datum.as_deref() else { continue };
        let Ok(tag) = chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d") else { continue };
        if tag > heute {
            befunde.push(Befund::hinweis(
                datei,
                zeile_mit(quelle, "kennung", &e.kennung).map(|z| feld_im_eintrag(quelle, z, "datum").unwrap_or(z)),
                format!(
                    "Das Ereignis „{}“ hat das Datum {d}, das liegt in der Zukunft. Ein Ereignis bekommt sein `datum`, wenn es eingetreten ist; die Fristen daran rechnet der Server schon jetzt mit diesem Tag. Für einen festen Termin besser eine Frist mit `datum`.",
                    e.kennung
                ),
            ));
        }
    }
}

/// Etappe 2e: Hinweise zu `daten/zeitstrahl.toml`: ein Eintrag ohne `kennung`
/// und `farbe` statt `art` (beides bis Schritt 6 erlaubt). Keine einfache oder
/// eine doppelte Kennung ist schon beim Lesen ein Fehler.
fn zeitstrahl_hinweise(daten: &ProjektDaten, quelle: &str, datei: &str, befunde: &mut Vec<Befund>) {
    use crate::vertrag::verlauf::{Hand, art_wort};
    let heute = abgeleitet::heute();
    let mut ab = 0;
    for z in daten.zeitstrahl.iter().flat_map(|d| d.inhalte()) {
        for e in &z.eintrag {
            let zeile = zeile_mit_ab(quelle, "titel", &e.titel, ab);
            if let Some(z) = zeile {
                ab = z;
            }
            if e.kennung.is_none() {
                befunde.push(Befund::hinweis(
                    datei,
                    zeile,
                    format!(
                        "Der Eintrag „{}“ hat keine `kennung`. Bitte eine eindeutige setzen, etwa `kennung = \"{}\"` (bis Schritt 6 erlaubt; bis dahin bildet der Server sie aus Datum und Titel).",
                        e.titel,
                        vorschlag_kennung(&e.titel)
                    ),
                ));
            }
            if e.farbe.is_some() && e.art.is_none() {
                let hand = Hand { kennung: None, datum: &e.datum, titel: &e.titel, text: None, art: None, farbe: e.farbe };
                let art = hand.ereignis(heute).map(|v| art_wort(v.art)).unwrap_or("erledigt");
                befunde.push(Befund::hinweis(
                    datei,
                    zeile.map(|z| feld_im_eintrag(quelle, z, "farbe").unwrap_or(z)),
                    format!(
                        "`farbe` im Zeitstrahl wird `art` (bis Schritt 6 erlaubt): hier `art = \"{art}\"`. Erlaubt sind erledigt, begonnen, geplant, abgebrochen, routine, problem."
                    ),
                ));
            }
        }
    }
}

/// Die Zeile (ab 1) des `n`-ten Vorkommens von `stueck` (gezählt je Vorkommen, nicht je Zeile).
fn nte_zeile_mit(text: &str, stueck: &str, n: usize) -> Option<usize> {
    let mut gesehen = 0;
    for (i, zeile) in text.lines().enumerate() {
        gesehen += zeile.matches(stueck).count();
        if gesehen >= n {
            return Some(i + 1);
        }
    }
    None
}

/// Etappe B2: `daten/kanban.toml` läuft aus. Die Spalten ergeben sich jetzt aus
/// Stand und Status im Datenspeicher (Backlog oben auf jeder Projektseite).
/// Lesen und zeigen geht vorerst weiter; darum ein Hinweis, kein Fehler.
pub const SATZ_KANBAN_LAEUFT_AUS: &str = "`kanban.toml` läuft aus (Etappe B2): Die Spalten zeigt das Backlog oben auf jeder Projektseite aus Aufgaben, Arbeiten und Fragen im Datenspeicher. Bitte Karten als Arbeiten melden (`POST /api/arbeiten`) und die Datei danach herausnehmen; bis dahin wird sie weiter gezeigt.";

/// Der Hinweis zu `kanban.toml` (einmal je Datei, ohne Zeile).
fn kanban_hinweis(daten: &ProjektDaten, datei: &str, befunde: &mut Vec<Befund>) {
    if daten.kanban.is_some() {
        befunde.push(Befund::hinweis(datei, None, SATZ_KANBAN_LAEUFT_AUS));
    }
}

/// Hinweise zu `daten/fragen.toml` (Etappe A2): eine Frage ohne `prioritaet`
/// (sie gilt dann als „bald“). Doppelte Kennungen sind schon beim Lesen ein
/// Fehler. Seit Etappe B2 gibt es keinen Hinweis mehr bei vielen Fragen: Der
/// Chat legt mehrere ab, der Inhaber sieht je Projekt eine (Warteschlange).
fn fragen_hinweise(fragen: &[Frage], text: &str, datei: &str, befunde: &mut Vec<Befund>) {
    for f in fragen.iter().filter(|f| f.prioritaet.is_none()) {
        befunde.push(Befund::hinweis(
            datei,
            zeile_mit(text, "id", &f.id),
            format!(
                "Die Frage „{}“ hat keine `prioritaet`; sie gilt darum als „bald“. Bitte angeben, etwa `prioritaet = \"bald\"` (jetzt, bald oder irgendwann).",
                f.id
            ),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn beispiel() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("beispiel").join("projects")
    }

    fn kaputt_ordner() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("beispiel-kaputt").join("projects")
    }

    fn fehler_von(slug: &str) -> Vec<Befund> {
        pruefe_projekt(&kaputt_ordner(), slug).into_iter().filter(Befund::ist_fehler).collect()
    }

    /// Genau ein Fehler im Beispiel `slug`, in `datei`, mit Zeile (falls `zeile`)
    /// und dem Satz `satz` darin.
    fn ein_fehler(slug: &str, datei: &str, zeile: Option<u32>, satz: &str) -> Befund {
        let f = fehler_von(slug);
        assert_eq!(f.len(), 1, "{slug}: {f:#?}");
        let b = f.into_iter().next().unwrap();
        assert_eq!(b.datei, format!("projects/{slug}/{datei}"), "{b}");
        assert_eq!(b.zeile, zeile, "{b}");
        assert!(b.text.contains(satz), "{slug}: {b}");
        b
    }

    #[test]
    fn beispiel_repo_meldet_nur_die_bekannten_kaputten() {
        let alle = pruefe_alle(&beispiel());
        let mit_fehler: Vec<&str> = alle.iter().filter(|(_, b)| b.iter().any(Befund::ist_fehler)).map(|(s, _)| s.as_str()).collect();
        assert_eq!(mit_fehler, ["kaputt", "neu-kaputt"], "{alle:#?}");
        assert!(alle.contains_key("neu-form") && alle.contains_key("dashboard") && alle.contains_key("schaufenster"));
        // neu-form: keine Fehler, nur die beiden Unterseiten ohne Datei,
        // vier Felder, die die Oberfläche nicht zeigt, und ein Eintrag im
        // Zeitstrahl in alter Form (ohne `kennung`, mit `farbe`); seit Etappe B2
        // dazu der Hinweis, dass kanban.toml ausläuft (ohne Zeile, die ganze Datei).
        let neu = &alle["neu-form"];
        assert_eq!(zaehlen(neu), (0, 9), "{neu:#?}");
        assert_eq!(neu.iter().filter(|b| b.text == SATZ_KANBAN_LAEUFT_AUS && b.datei == "projects/neu-form/daten/kanban.toml").count(), 1, "{neu:#?}");
        assert!(neu.iter().filter(|b| b.text != SATZ_KANBAN_LAEUFT_AUS).all(|b| b.zeile.is_some()), "{neu:#?}");
        assert_eq!(neu.iter().filter(|b| b.text.contains("Etappe 4")).count(), 2, "{neu:#?}");
        assert!(neu.iter().all(|b| !b.text.contains("Etappe 3a")), "veraltete Etappe im Satz: {neu:#?}");
        let ungezeigt: Vec<String> = neu.iter().filter(|b| b.text.contains("wird nicht angezeigt")).map(|b| b.to_string()).collect();
        assert_eq!(
            ungezeigt,
            [
                "projects/neu-form/daten/fluss.toml, Zeile 8: `farbe` bei Fluss-Kästen wird nicht angezeigt (Farben wählt die Oberfläche selbst); die Seite sieht ohne das Feld gleich aus.",
                "projects/neu-form/daten/fluss.toml, Zeile 13: `text` bei Fluss-Pfeilen wird nicht angezeigt (die Oberfläche zeichnet keinen Text an Pfeilen); die Seite sieht ohne das Feld gleich aus.",
                "projects/neu-form/daten/kanban.toml, Zeile 14: `farbe` bei Kanban-Karten wird nicht angezeigt (Farben wählt die Oberfläche selbst); die Seite sieht ohne das Feld gleich aus.",
                "projects/neu-form/daten/kennzahlen.toml, Zeile 6: `farbe` bei Kennzahlen wird nicht angezeigt (Farben wählt die Oberfläche selbst); die Seite sieht ohne das Feld gleich aus.",
            ]
        );
        let zeitstrahl: Vec<String> = neu.iter().filter(|b| b.datei.ends_with("zeitstrahl.toml")).map(|b| b.to_string()).collect();
        assert_eq!(zeitstrahl.len(), 2, "{zeitstrahl:#?}");
        assert_eq!(
            zeitstrahl[0],
            "projects/neu-form/daten/zeitstrahl.toml, Zeile 15: Der Eintrag „Pilot“ hat keine `kennung`. Bitte eine eindeutige setzen, etwa `kennung = \"pilot\"` (bis Schritt 6 erlaubt; bis dahin bildet der Server sie aus Datum und Titel)."
        );
        assert!(zeitstrahl[1].starts_with("projects/neu-form/daten/zeitstrahl.toml, Zeile 16: `farbe` im Zeitstrahl wird `art`"), "{zeitstrahl:#?}");
        // Die Beispiele in neuer Form ohne Seite.svelte: kein Befund.
        for slug in ["dashboard", "schaufenster", "geflecht"] {
            assert!(alle[slug].is_empty(), "{slug}: {:#?}", alle[slug]);
        }
        // Die Fehler mit Datei und Zeile.
        let k = &alle["kaputt"];
        let f: Vec<&Befund> = k.iter().filter(|b| b.ist_fehler()).collect();
        assert_eq!(f.len(), 1, "{k:#?}");
        assert_eq!((f[0].datei.as_str(), f[0].zeile), ("projects/kaputt/projekt.toml", Some(5)), "{}", f[0]);
        assert!(f[0].text.contains("doppelt"), "{}", f[0]);
        let f = &alle["neu-kaputt"].iter().find(|b| b.ist_fehler()).unwrap();
        assert_eq!(
            f.to_string(),
            "projects/neu-kaputt/projekt.toml, Zeile 4: Bei `status` steht „aktief“. Erlaubt sind: aktiv, pausiert, geplant, fertig, problem, archiviert."
        );
    }

    #[test]
    fn unbekanntes_projekt_gibt_es_nicht() {
        assert!(!ist_projekt(&beispiel(), "gibt-es-nicht"));
        assert!(!ist_projekt(&beispiel(), "../beispiel"));
        assert!(ist_projekt(&beispiel(), "neu-form"));
        assert!(ist_projekt(&beispiel(), "schaufenster"));
        assert!(pruefe_projekt(&beispiel(), "gibt-es-nicht").is_empty());
    }

    #[test]
    fn mindestens_zehn_fehlerbeispiele_mit_datei_und_zeile() {
        let alle = pruefe_alle(&kaputt_ordner());
        let fehler: Vec<&Befund> = alle.values().flatten().filter(|b| b.ist_fehler()).collect();
        assert!(fehler.len() >= 10, "nur {} Fehler: {fehler:#?}", fehler.len());
        for (slug, befunde) in &alle {
            assert!(slug.starts_with("kaputt-"), "{slug}");
            assert!(befunde.iter().any(Befund::ist_fehler), "{slug} ohne Fehler: {befunde:#?}");
            for b in befunde {
                assert!(b.datei.starts_with(&format!("projects/{slug}/")), "{b}");
                assert!(!b.text.is_empty() && !b.datei.contains('\\'), "{b}");
            }
        }
        // Wo eine Zeile sinnvoll ist (TOML-Text, SVG, HTML), steht sie da.
        let mit_zeile = fehler.iter().filter(|b| b.zeile.is_some()).count();
        assert!(mit_zeile >= 10, "{mit_zeile} mit Zeile: {fehler:#?}");
    }

    #[test]
    fn kaputt_unbekanntes_feld() {
        ein_fehler("kaputt-feld", "projekt.toml", Some(5), "Das Feld `updated` gibt es hier nicht. Erlaubt sind: schema, title");
    }

    #[test]
    fn kaputt_schema() {
        ein_fehler("kaputt-schema", "projekt.toml", Some(2), "Bei `schema` steht 2. Für projekt.toml gilt `schema = 3`.");
    }

    #[test]
    fn kaputt_status() {
        ein_fehler("kaputt-status", "projekt.toml", Some(4), "Bei `status` steht „fertich“. Erlaubt sind: aktiv, pausiert");
    }

    #[test]
    fn kaputt_gantt_ohne_bis() {
        ein_fehler("kaputt-gantt", "daten/gantt.toml", Some(9), "Dem Eintrag „Phase 1“ fehlt `bis`. Beispiel: `bis = \"2027-01\"`");
    }

    #[test]
    fn kaputt_rueckwaertsstrich() {
        ein_fehler("kaputt-strich", "projekt.toml", Some(9), "Bitte Schrägstrich `/` nehmen");
    }

    #[test]
    fn kaputt_datum() {
        ein_fehler("kaputt-datum", "daten/zeitstrahl.toml", Some(2), "Bei `datum` steht „17.03.2025“. Erlaubt ist JJJJ-MM-TT oder JJJJ-MM");
    }

    #[test]
    fn kaputt_pfeil_auf_unbekannten_kasten() {
        ein_fehler("kaputt-pfeil", "daten/fluss.toml", Some(11), "Ein Pfeil zeigt bei `nach` auf „ertne“, aber diesen Kasten gibt es nicht.");
    }

    #[test]
    fn kaputt_wahl_mit_einer_option() {
        ein_fehler("kaputt-wahl", "daten/fragen.toml", Some(2), "ist eine Wahl und braucht mindestens zwei `optionen`");
    }

    /// Pruefe meldet eine Option „Etwas anderes“
    /// in fragen.toml; die Oberfläche setzt den Knopf selbst (sie stünde doppelt da).
    #[test]
    fn kaputt_option_etwas_anderes() {
        ein_fehler(
            "kaputt-anderes",
            "daten/fragen.toml",
            Some(2),
            "hat die Option „Etwas anderes …“. „Etwas anderes …“ setzt das Dashboard bei jeder Klick-Antwort selbst dazu",
        );
    }

    #[test]
    fn kaputt_unbekannte_datei() {
        ein_fehler("kaputt-datei", "daten/zeitleiste.toml", None, "Die Datei `zeitleiste.toml` kennt der Server nicht.");
    }

    #[test]
    fn kaputt_fortschritt_alte_namen() {
        // Die alten Namen `von` und `bis` (vor Etappe 2c) heißen jetzt `erreicht` und `gesamt`.
        // serde meldet das erste unbekannte Feld in Reihenfolge der Datei
        // (toml mit `preserve_order`): `von` (Zeile 3).
        ein_fehler(
            "kaputt-fortschritt",
            "daten/fortschritt.toml",
            Some(3),
            "Das Feld `von` gibt es hier nicht. Erlaubt sind: titel, prozent, erreicht, gesamt, einheit, hinweis.",
        );
    }

    #[test]
    fn kaputt_vorschau_fehlt() {
        ein_fehler("kaputt-vorschau", "projekt.toml", Some(5), "Bei `vorschau` steht „assets/entwurf.html“, aber diese Datei gibt es nicht.");
    }

    #[test]
    fn kaputt_unterseite_doppelt() {
        // Die Zeile zeigt auf das zweite, überzählige Vorkommen.
        ein_fehler("kaputt-seite", "projekt.toml", Some(11), "Die Unterseite „plan“ steht doppelt da.");
    }

    #[test]
    fn kaputt_gantt_reihenfolge() {
        ein_fehler("kaputt-gantt-reihenfolge", "daten/gantt.toml", Some(11), "Beim Balken „Phase 1“ liegt `bis` vor `von`.");
    }

    /// Etappe 3b (Fund des Prüfers in einer eigen-ablage.toml):
    /// `\b`, `\t`, `\f` in einem Text mit doppelten Anführungszeichen sind
    /// gültiges TOML, werden aber Steuerzeichen; `pruefe` gibt je Zeile einen
    /// Hinweis. Einfache Anführungszeichen, `\\`, `\"` und Zeilenumbrüche in
    /// mehrzeiligen Texten bleiben still.
    #[test]
    fn steuerzeichen_in_texten_sind_ein_hinweis() {
        let (dir, ordner) = temp_projekt("steuerzeichen");
        std::fs::write(
            ordner.join("daten").join("eigen-ablage.toml"),
            concat!(
                "# Kommentar mit \\t bleibt still\n",
                "[[eintrag]]\n",
                "ordner = \"Projekte\\buero\\fach\"\n",
                "gut = 'Projekte\\buero\\fach'\n",
                "auch_gut = \"C:\\\\Ablage \\\"x\\\" C:/Ablage\"\n",
                "text = \"\"\"\nZeile eins\\nZeile zwei\n\"\"\"\n",
                "tab = \"a\\tb\"\n",
                "unicode = \"a\\u0007b\"\n",
                "zeile = \"D:\\neu\"\n",
            ),
        )
        .unwrap();
        let b = pruefe_projekt(&dir, "p");
        let s: Vec<String> = b.iter().filter(|b| b.text.contains("Steuerzeichen")).map(|b| b.to_string()).collect();
        let zeilen: Vec<Option<u32>> = b.iter().filter(|b| b.text.contains("Steuerzeichen")).map(|b| b.zeile).collect();
        assert_eq!(zeilen, [Some(3), Some(9), Some(10), Some(11)], "{s:#?}");
        assert!(b.iter().filter(|b| b.text.contains("Steuerzeichen")).all(|b| !b.ist_fehler()), "{s:?}");
        assert!(s[0].starts_with("projects/p/daten/eigen-ablage.toml, Zeile 3: In einem Text steht `\\b`"), "{s:?}");
        assert!(s[2].contains("`\\u0007`"), "{s:?}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Ein Projekt in neuer Form in einem frischen Ordner unter temp; `name` unterscheidet die Tests.
    fn temp_projekt(name: &str) -> (PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!("dashboard-pruefung-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let ordner = dir.join("p");
        std::fs::create_dir_all(ordner.join("daten")).unwrap();
        std::fs::write(ordner.join("projekt.toml"), "schema = 3\ntitle = \"P\"\nstatus = \"aktiv\"\nsummary = \"S\"\n").unwrap();
        (dir, ordner)
    }

    /// Etappe 4v: projects/_vorlage hat eine echte projekt.toml, ist aber kein
    /// Projekt. Ordner mit _ am Anfang (kein gültiger Slug) lädt und prüft niemand.
    #[test]
    fn ordner_mit_unterstrich_sind_keine_projekte() {
        let (dir, _) = temp_projekt("unterstrich");
        let vorlage = dir.join("_vorlage");
        std::fs::create_dir_all(vorlage.join("daten")).unwrap();
        std::fs::write(vorlage.join("projekt.toml"), "schema = 3\ntitle = \"Vorlage\"\nstatus = \"geplant\"\nsummary = \"S\"\n").unwrap();
        std::fs::write(vorlage.join("page.md"), "+++\ntitle = \"Alt\"\n+++\nHallo\n").unwrap();
        assert!(!seite::is_valid_slug("_vorlage"));
        assert!(seite::load_one(&dir, "_vorlage").unwrap().is_none());
        assert!(!ist_projekt(&dir, "_vorlage"));
        let (projekte, kaputt) = seite::load_all(&dir);
        let slugs: Vec<&str> = projekte.iter().map(|p| p.slug.as_str()).collect();
        assert_eq!(slugs, ["p"]);
        assert!(kaputt.is_empty(), "{kaputt:?}");
        let alle = pruefe_alle(&dir);
        assert_eq!(alle.keys().map(String::as_str).collect::<Vec<_>>(), ["p"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn projekt_toml_als_ordner_und_zu_gross() {
        let (dir, ordner) = temp_projekt("ordner");
        std::fs::remove_file(ordner.join("projekt.toml")).unwrap();
        std::fs::create_dir_all(ordner.join("projekt.toml")).unwrap();
        // Nur projekt.toml (als Ordner): der Server meldet den Fehler, die Prüfung auch.
        let e = seite::load_one(&dir, "p").unwrap_err();
        assert_eq!(format!("{e:#}"), "projects/p/projekt.toml: projekt.toml ist ein Ordner, keine Datei.");
        assert!(ist_projekt(&dir, "p"));
        // Mit einer alten page.md daneben: beide Fehler, die Datei bleibt ungelesen.
        std::fs::write(ordner.join("page.md"), "+++\ntitle = \"Alt\"\n+++\nHallo\n").unwrap();
        assert!(seite::load_one(&dir, "p").is_err());
        let b = pruefe_projekt(&dir, "p");
        let saetze: Vec<String> = b.iter().map(|b| b.to_string()).collect();
        assert!(saetze.contains(&"projects/p/projekt.toml: projekt.toml ist ein Ordner, keine Datei.".to_string()), "{saetze:#?}");
        assert!(saetze.contains(&format!("projects/p/page.md: {SATZ_MARKDOWN}")), "{saetze:#?}");
        std::fs::remove_file(ordner.join("page.md")).unwrap();

        let mut gross = String::from("[[kennzahl]]\ntitel = \"A\"\nwert = \"1\"\n");
        while gross.len() <= seite::TOML_HOECHSTENS as usize {
            gross.push_str("# Füllzeile, damit die Datei groß wird.\n");
        }
        std::fs::write(ordner.join("daten").join("kennzahlen.toml"), &gross).unwrap();
        let b = pruefe_projekt(&dir, "p");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            b.iter().any(|b| b.ist_fehler()
                && b.to_string() == "projects/p/daten/kennzahlen.toml: kennzahlen.toml ist größer als 1 MB; so viel gehört nicht in eine Datendatei."),
            "{b:#?}"
        );
    }

    #[test]
    fn nicht_angezeigte_felder_als_hinweis() {
        let (dir, ordner) = temp_projekt("felder");
        let d = ordner.join("daten");
        std::fs::write(d.join("kennzahlen.toml"), "[[kennzahl]]\ntitel = \"A\"\nwert = \"1\"\n\n[[kennzahl]]\nfarbe = \"rot\"\ntitel = \"B\"\nwert = \"2\"\n")
            .unwrap();
        std::fs::write(
            d.join("fluss.toml"),
            "[[kasten]]\nid = \"a\"\ntext = \"A\"\n[[kasten]]\nid = \"b\"\ntext = \"B\"\n[[pfeil]]\nvon = \"a\"\nnach = \"b\"\n",
        )
        .unwrap();
        let b = pruefe_projekt(&dir, "p");
        let _ = std::fs::remove_dir_all(&dir);
        let saetze: Vec<String> = b.iter().map(|b| b.to_string()).collect();
        assert_eq!(zaehlen(&b), (0, 1), "{saetze:#?}");
        assert_eq!(
            saetze[0],
            "projects/p/daten/kennzahlen.toml, Zeile 6: `farbe` bei Kennzahlen wird nicht angezeigt (Farben wählt die Oberfläche selbst); die Seite sieht ohne das Feld gleich aus."
        );
    }

    #[test]
    fn verknuepfungen_in_daten_und_assets() {
        let (dir, ordner) = temp_projekt("symlink");
        let ziel = dir.join("ziel.toml");
        std::fs::write(&ziel, "[[kennzahl]]\ntitel = \"A\"\nwert = \"1\"\n").unwrap();
        std::fs::create_dir_all(ordner.join("assets")).unwrap();
        #[cfg(windows)]
        let angelegt = std::os::windows::fs::symlink_file(&ziel, ordner.join("daten").join("kennzahlen.toml"))
            .and_then(|_| std::os::windows::fs::symlink_file(&ziel, ordner.join("assets").join("bild.png")));
        #[cfg(unix)]
        let angelegt = std::os::unix::fs::symlink(&ziel, ordner.join("daten").join("kennzahlen.toml"))
            .and_then(|_| std::os::unix::fs::symlink(&ziel, ordner.join("assets").join("bild.png")));
        if angelegt.is_err() {
            // Ohne Rechte für Symlinks (Windows ohne Entwicklermodus) lässt sich das nicht prüfen.
            let _ = std::fs::remove_dir_all(&dir);
            eprintln!("verknuepfungen_in_daten_und_assets übersprungen: Symlink ließ sich nicht anlegen");
            return;
        }
        let b = pruefe_projekt(&dir, "p");
        let e = seite::load_one(&dir, "p").unwrap_err();
        let _ = std::fs::remove_dir_all(&dir);
        let saetze: Vec<String> = b.iter().filter(|b| b.ist_fehler()).map(|b| b.to_string()).collect();
        assert_eq!(saetze.len(), 2, "{saetze:#?}");
        assert!(saetze[0].starts_with("projects/p/daten/kennzahlen.toml: kennzahlen.toml ist eine Verknüpfung (Symlink)."), "{saetze:#?}");
        assert!(saetze[1].starts_with("projects/p/assets/bild.png: ") && saetze[1].contains("Verknüpfung"), "{saetze:#?}");
        assert!(format!("{e:#}").contains("ist eine Verknüpfung"), "{e:#}");
    }

    /// Junctions (`mklink /J`) auf `daten/`, `assets/` und den Projektordner:
    /// Fehler in der Prüfung, und der Server liest das Projekt nicht.
    #[test]
    fn junctions_auf_ordner() {
        let (dir, ordner) = temp_projekt("junction");
        let fremd = dir.join("fremd");
        std::fs::create_dir_all(&fremd).unwrap();
        std::fs::write(fremd.join("kennzahlen.toml"), "[[kennzahl]]\ntitel = \"A\"\nwert = \"1\"\n").unwrap();
        std::fs::write(fremd.join("bild.png"), b"x").unwrap();
        std::fs::remove_dir(ordner.join("daten")).unwrap();
        let aufraeumen = |dir: &Path| {
            for link in [dir.join("p").join("daten"), dir.join("p").join("assets"), dir.join("q")] {
                if std::fs::symlink_metadata(&link).is_ok_and(|m| m.file_type().is_symlink()) {
                    let _ = std::fs::remove_dir(&link).or_else(|_| std::fs::remove_file(&link));
                }
            }
            let _ = std::fs::remove_dir_all(dir);
        };
        if !seite::junction_anlegen(&ordner.join("daten"), &fremd) || !seite::junction_anlegen(&ordner.join("assets"), &fremd) {
            aufraeumen(&dir);
            eprintln!("junctions_auf_ordner übersprungen: Junction ließ sich nicht anlegen");
            return;
        }
        assert!(seite::ist_verknuepfung(&ordner.join("daten")));
        assert!(!seite::ist_verknuepfung(&ordner));
        assert!(!seite::ist_echter_ordner(&ordner.join("daten")));
        let b = pruefe_projekt(&dir, "p");
        let e = seite::load_one(&dir, "p").unwrap_err();
        // Der Projektordner selbst als Junction (auf p).
        let projekt_link = seite::junction_anlegen(&dir.join("q"), &ordner);
        let bq = pruefe_projekt(&dir, "q");
        let eq = seite::load_one(&dir, "q").map(|_| ()).unwrap_err();
        let (projekte, kaputt) = seite::load_all(&dir);
        aufraeumen(&dir);
        let saetze: Vec<String> = b.iter().filter(|b| b.ist_fehler()).map(|b| b.to_string()).collect();
        assert_eq!(
            saetze,
            [
                "projects/p/daten: Der Ordner daten ist eine Verknüpfung (Symlink oder Junction); das Dashboard liest nur echte Ordner.",
                "projects/p/assets: Der Ordner assets ist eine Verknüpfung (Symlink oder Junction); das Dashboard liest nur echte Ordner.",
            ]
        );
        assert_eq!(format!("{e:#}"), "projects/p/daten: Der Ordner daten ist eine Verknüpfung (Symlink oder Junction); das Dashboard liest nur echte Ordner.");
        assert!(projekt_link, "Junction auf den Projektordner ließ sich nicht anlegen");
        let saetze: Vec<String> = bq.iter().map(|b| b.to_string()).collect();
        assert_eq!(saetze, ["projects/q: Der Ordner q ist eine Verknüpfung (Symlink oder Junction); das Dashboard liest nur echte Ordner."]);
        assert!(format!("{eq:#}").contains("Der Ordner q ist eine Verknüpfung"), "{eq:#}");
        assert!(projekte.is_empty());
        assert_eq!(kaputt.iter().map(|k| k.slug.as_str()).collect::<Vec<_>>(), ["p", "q"]);
    }

    /// Etappe 6a: Jede Markdown-Datei im Projektordner ist ein Fehler, auch in
    /// Unterordnern; ein Ordner nur mit einer alten page.md wird trotzdem
    /// geprüft (projekt.toml fehlt) und verschwindet nicht stumm.
    #[test]
    fn markdown_im_projekt_ist_ein_fehler() {
        // Die Markdown-Dateien entstehen erst hier (im Repo liegt keine *.md).
        let (dir, ordner) = temp_projekt("markdown-neu");
        std::fs::create_dir_all(ordner.join("entwurf")).unwrap();
        std::fs::write(ordner.join("notiz.md"), "Eine Notiz.\n").unwrap();
        std::fs::write(ordner.join("entwurf").join("plan.md"), "# Alte Seite\n").unwrap();
        let f: Vec<Befund> = pruefe_projekt(&dir, "p").into_iter().filter(Befund::ist_fehler).collect();
        // Die Bauliste nimmt das Projekt trotzdem: Der Scanner meldet Markdown nicht.
        let scanner = crate::werkzeug::scanner::pruefe_projekt(&dir, "p");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(scanner.iter().all(|b| !b.text.contains("Markdown")), "{scanner:#?}");
        let saetze: Vec<String> = f.iter().map(|b| b.to_string()).collect();
        assert_eq!(saetze, [format!("projects/p/entwurf/plan.md: {SATZ_MARKDOWN}"), format!("projects/p/notiz.md: {SATZ_MARKDOWN}")], "{saetze:#?}");
        // Nur eine alte page.md, keine projekt.toml: zwei Fehler, das Projekt gibt es nicht.
        let (dir, ordner) = temp_projekt("markdown");
        std::fs::remove_file(ordner.join("projekt.toml")).unwrap();
        std::fs::write(ordner.join("PAGE.MD"), "+++\ntitle = \"Alt\"\n+++\nHallo\n").unwrap();
        assert!(!ist_projekt(&dir, "p"));
        assert!(wird_geprueft(&dir, "p"));
        assert!(seite::load_one(&dir, "p").unwrap().is_none());
        let alle = pruefe_alle(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        let saetze: Vec<String> = alle["p"].iter().map(|b| b.to_string()).collect();
        assert_eq!(saetze, [format!("projects/p/PAGE.MD: {SATZ_MARKDOWN}"), format!("projects/p/projekt.toml: {SATZ_OHNE_KOPF}")], "{saetze:#?}");
        // Ohne Markdown und ohne projekt.toml: kein Projekt, keine Prüfung.
        assert!(!wird_geprueft(&beispiel(), "gibt-es-nicht"));
    }

    #[test]
    fn beispiel_mit_berechneten_kennzahlen_ist_ohne_fehler() {
        let b = pruefe_projekt(&beispiel(), "neu-daten");
        assert_eq!(zaehlen(&b), (0, 0), "{b:#?}");
    }

    /// Fehler bei `berechnet`, die erst mit den anderen Dateien auffallen:
    /// alle auf einmal, je mit Datei, Zeile und Satz.
    #[test]
    fn berechnet_quelle_liste_und_feld() {
        let (dir, ordner) = temp_projekt("berechnet");
        let d = ordner.join("daten");
        std::fs::write(d.join("eigen-s.toml"), "[[zeile]]\ndatum = \"2026-11-01\"\npreis = 3\n").unwrap();
        std::fs::write(
            d.join("kennzahlen.toml"),
            "[[kennzahl]]\ntitel = \"A\"\nberechnet = { quelle = \"ampel\", art = \"anzahl\" }\n\n\
             [[kennzahl]]\ntitel = \"B\"\nberechnet = { quelle = \"eigen-s\", liste = \"zeilen\", art = \"anzahl\" }\n\n\
             [[kennzahl]]\ntitel = \"C\"\nberechnet = { quelle = \"eigen-s\", art = \"summe\", feld = \"datum\" }\n\n\
             [[kennzahl]]\ntitel = \"D\"\n[kennzahl.berechnet]\nquelle = \"eigen-s\"\nart = \"tage_bis\"\nfeld = \"preis\"\n\n\
             [[kennzahl]]\ntitel = \"E\"\nwert = \"5\"\n\n\
             [[kennzahl]]\ntitel = \"F\"\nberechnet = { quelle = \"eigen-s\", art = \"summe\", feld = \"preis\" }\n",
        )
        .unwrap();
        let b = pruefe_projekt(&dir, "p");
        let _ = std::fs::remove_dir_all(&dir);
        let saetze: Vec<String> = b.iter().filter(|b| b.ist_fehler()).map(|b| b.to_string()).collect();
        let k = "projects/p/daten/kennzahlen.toml";
        assert_eq!(
            saetze,
            [
                format!("{k}, Zeile 3: Kennzahl „A“: Die Quelle „ampel“ gibt es nicht: in `daten/` liegt keine Datei `ampel.toml`."),
                format!("{k}, Zeile 7: Kennzahl „B“: In „eigen-s“ gibt es die Liste „zeilen“ nicht. Vorhanden: zeile."),
                format!(
                    "{k}, Zeile 11: Kennzahl „C“: Die Spalte „datum“ in der Liste „zeile“ ist keine Zahlenspalte. Für `summe` stehen dort Zahlen, etwa `datum = 120`."
                ),
                format!(
                    "{k}, Zeile 18: Kennzahl „D“: Die Spalte „preis“ in der Liste „zeile“ ist keine Datumsspalte. Für `tage_bis` stehen dort Daten, etwa `preis = \"2026-11-02\"`."
                ),
            ]
        );
    }

    /// Fehler bei `berechnet`, die schon beim Lesen der Datei auffallen.
    #[test]
    fn berechnet_beides_keines_und_ohne_feld() {
        let fall = |kennzahl: &str| {
            let (dir, ordner) = temp_projekt("berechnet-form");
            std::fs::write(ordner.join("daten").join("kennzahlen.toml"), format!("[[kennzahl]]\ntitel = \"A\"\n{kennzahl}")).unwrap();
            let b = pruefe_projekt(&dir, "p");
            let _ = std::fs::remove_dir_all(&dir);
            let f: Vec<String> = b.iter().filter(|b| b.ist_fehler()).map(|b| b.to_string()).collect();
            assert_eq!(f.len(), 1, "{f:#?}");
            f.into_iter().next().unwrap()
        };
        assert_eq!(
            fall("wert = \"3\"\nberechnet = { quelle = \"gantt\", art = \"anzahl\" }\n"),
            "projects/p/daten/kennzahlen.toml, Zeile 2: Bei der Kennzahl „A“ stehen `wert` und `berechnet`. Bitte genau eines: `wert` für einen eingetippten Wert oder `berechnet`, wenn der Server rechnen soll."
        );
        assert!(fall("einheit = \"€\"\n").starts_with("projects/p/daten/kennzahlen.toml, Zeile 2: Der Kennzahl „A“ fehlt `wert` oder `berechnet`."));
        assert!(fall("berechnet = { quelle = \"gantt\", art = \"summe\" }\n").contains("Zeile 2: Bei der Kennzahl „A“ braucht `art = \"summe\"` auch `feld`"));
        assert!(
            fall("berechnet = { quelle = \"gantt\", art = \"mittel\" }\n")
                .contains("Zeile 3: Bei `berechnet` steht „mittel“. Erlaubt sind: anzahl, anteil, summe, tage_seit, tage_bis.")
        );
        // Etappe B1: `wert` nur mit `feld` und nur bei anzahl/anteil; `feld` bei anteil nur mit `wert`.
        assert!(
            fall("berechnet = { quelle = \"gantt\", art = \"anteil\", wert = \"ja\" }\n")
                .contains("Zeile 2: Bei der Kennzahl „A“ steht `wert = \"ja\"` in `berechnet`, aber kein `feld`.")
        );
        assert!(
            fall("berechnet = { quelle = \"gantt\", art = \"summe\", feld = \"x\", wert = 3 }\n")
                .contains("steht `wert = 3` in `berechnet`. Einen Wert zum Zählen gibt es nur bei")
        );
        assert!(fall("berechnet = { quelle = \"gantt\", art = \"anteil\", feld = \"freigabe\" }\n").contains("steht `feld = \"freigabe\"`, aber kein `wert`."));
        // Etappe B1: `stand` ist ein Tag, `alt_ab` nur mit `stand` und ab 1.
        assert!(fall("wert = \"3\"\nstand = \"2026-10\"\n").contains("Zeile 4: Bei „A“ steht `stand = \"2026-10\"`. Erlaubt ist ein Tag JJJJ-MM-TT"));
        assert!(fall("wert = \"3\"\nalt_ab = 14\n").contains("Zeile 2: Bei „A“ steht `alt_ab`, aber kein `stand`."));
        assert!(fall("wert = \"3\"\nstand = \"2026-10-30\"\nalt_ab = 0\n").contains("Bei „A“ steht `alt_ab = 0`."));
        assert!(fall("wert = \"3\"\nstand_alter = 3\n").contains("`stand_alter` gibt es hier nicht"), "rechnet nur der Server");
    }

    /// Abnahme B1: `nan` und `inf` (in TOML gültig) meldet `pruefe` mit Zeile,
    /// auch in Arrays und Inline-Tabellen; ein Wort „nan“ in einem Text nicht.
    #[test]
    fn b1_nan_und_inf_mit_zeile() {
        let (dir, ordner) = temp_projekt("b1-nan");
        std::fs::write(
            ordner.join("daten").join("eigen-kosten.toml"),
            "titel = \"x = nan im Text\"\n[[posten]]\nname = \"A\"\nbetrag = nan\n[[posten]]\nname = \"B\"\nbetrag = -inf # zu viel\nwerte = [1, +inf]\nteil = { x = inf }\n",
        )
        .unwrap();
        std::fs::write(ordner.join("daten").join("eigen-gut.toml"), "titel = \"nan = inf\"\nzahl = 1.5\n").unwrap();
        // Mehrzeiliges Array: `nan` steht allein in seiner Zeile.
        std::fs::write(ordner.join("daten").join("eigen-reihe.toml"), "werte = [\n  1,\n  nan,\n]\n").unwrap();
        let b = pruefe_projekt(&dir, "p");
        let _ = std::fs::remove_dir_all(&dir);
        let f: Vec<String> = b.iter().filter(|b| b.ist_fehler()).map(|b| b.to_string()).collect();
        let d = "projects/p/daten/eigen-kosten.toml";
        let kosten: Vec<&String> = f.iter().filter(|s| s.starts_with(d)).collect();
        assert_eq!(kosten.len(), 4, "{f:#?}");
        for (i, (zeile, wort)) in [(4, "nan"), (7, "-inf"), (8, "+inf"), (9, "inf")].into_iter().enumerate() {
            assert!(kosten[i].starts_with(&format!("{d}, Zeile {zeile}: Hier steht `{wort}`. Das ist in TOML erlaubt, aber keine Zahl")), "{f:#?}");
        }
        let reihe = "projects/p/daten/eigen-reihe.toml, Zeile 3: Hier steht `nan`.";
        assert!(f.iter().any(|s| s.starts_with(reihe)), "{f:#?}");
        assert_eq!(f.len(), 5, "{f:#?}");
    }

    /// Etappe B1: Kennzahlen über leere Listen ergeben 0, keinen Prüffehler;
    /// `anzahl`/`anteil` mit eigenem Treffer; `stand` an Kennzahl und Ampel.
    #[test]
    fn b1_leere_listen_treffer_und_stand_ohne_fehler() {
        let (dir, ordner) = temp_projekt("b1");
        let d = ordner.join("daten");
        std::fs::write(d.join("fragen.toml"), "# noch keine Fragen\n").unwrap();
        std::fs::write(d.join("eigen-leer.toml"), "posten = []\n").unwrap();
        std::fs::write(
            d.join("eigen-seiten.toml"),
            "[[seite]]\nname = \"Start\"\nfreigabe = \"freigegeben\"\n[[seite]]\nname = \"Preise\"\nfreigabe = \"Entwurf\"\n",
        )
        .unwrap();
        std::fs::write(d.join("ampel.toml"), "[[zeile]]\ntitel = \"Postfach\"\nfarbe = \"gelb\"\nstand = \"2026-10-01\"\nalt_ab = 14\n").unwrap();
        std::fs::write(d.join("fortschritt.toml"), "[[zeile]]\ntitel = \"Ablage\"\nerreicht = 0.2\ngesamt = 1.5\neinheit = \"GB\"\n").unwrap();
        std::fs::write(
            d.join("kennzahlen.toml"),
            "[[kennzahl]]\ntitel = \"Offene Fragen\"\nberechnet = { quelle = \"fragen\", liste = \"frage\", art = \"anzahl\" }\n\n\
             [[kennzahl]]\ntitel = \"Fragen ohne liste\"\nberechnet = { quelle = \"fragen\", art = \"anzahl\" }\n\n\
             [[kennzahl]]\ntitel = \"Kosten\"\nberechnet = { quelle = \"eigen-leer\", liste = \"posten\", art = \"summe\", feld = \"betrag\" }\n\n\
             [[kennzahl]]\ntitel = \"Freigegeben\"\nberechnet = { quelle = \"eigen-seiten\", art = \"anteil\", feld = \"freigabe\", wert = \"freigegeben\" }\n\n\
             [[kennzahl]]\ntitel = \"Ungelesen\"\nwert = \"12\"\nstand = \"2026-10-30\"\n",
        )
        .unwrap();
        let b = pruefe_projekt(&dir, "p");
        let _ = std::fs::remove_dir_all(&dir);
        let f: Vec<String> = b.iter().filter(|b| b.ist_fehler()).map(|b| b.to_string()).collect();
        assert!(f.is_empty(), "{f:#?}");
    }

    #[test]
    fn kaputt_link_ohne_adresse() {
        ein_fehler("kaputt-link", "projekt.toml", Some(8), "Der Link „Hersteller“ hat die Adresse „www.example.org“.");
    }

    /// Auftrag der Architektur: Der Befund zitiert die Adresse des Links, aber
    /// ohne Profil und Benutzernamen (er geht über das freie `/api/pruefung`).
    #[test]
    fn befund_zitiert_link_ohne_profil() {
        let (dir, ordner) = temp_projekt("link-profil");
        std::fs::write(
            ordner.join("projekt.toml"),
            "schema = 3\ntitle = \"P\"\nstatus = \"aktiv\"\nsummary = \"S\"\n[[links]]\nname = \"Ablage\"\nurl = 'siehe C:\\Users\\beispiel\\Ablage'\n",
        )
        .unwrap();
        let b = pruefe_projekt(&dir, "p");
        let _ = std::fs::remove_dir_all(&dir);
        let f: Vec<&Befund> = b.iter().filter(|b| b.ist_fehler()).collect();
        assert_eq!(f.len(), 1, "{f:#?}");
        assert!(f[0].text.contains(r"„siehe %USERPROFILE%\Ablage“"), "{}", f[0]);
        assert!(!f[0].text.contains("beispiel"), "{}", f[0]);
    }

    #[test]
    fn kaputt_svg_mit_skript() {
        let f = fehler_von("kaputt-svg");
        assert_eq!(f.len(), 3, "{f:#?}");
        assert!(f.iter().all(|b| b.datei == "projects/kaputt-svg/assets/plan.svg"), "{f:#?}");
        let zeilen: Vec<Option<u32>> = f.iter().map(|b| b.zeile).collect();
        assert_eq!(zeilen, [Some(2), Some(3), Some(4)], "{f:#?}");
        assert!(f[0].text.contains("`<script>`"), "{}", f[0]);
        assert!(f[1].text.contains("`onclick=`"), "{}", f[1]);
        assert!(f[2].text.contains("nach außen"), "{}", f[2]);
    }

    #[test]
    fn kaputt_svg_mit_tricks() {
        let f = fehler_von("kaputt-svg-tricks");
        let saetze: Vec<String> = f.iter().map(|b| b.to_string()).collect();
        assert!(f.iter().all(|b| b.datei == "projects/kaputt-svg-tricks/assets/tricks.svg"), "{saetze:#?}");
        let zeilen: Vec<u32> = f.iter().filter_map(|b| b.zeile).collect();
        assert_eq!(zeilen, [2, 4, 5, 6, 7, 8, 9, 9, 10, 10], "{saetze:#?}");
        let hat = |z: u32, stueck: &str| f.iter().any(|b| b.zeile == Some(z) && b.text.contains(stueck));
        assert!(hat(2, "eigene Dokumenttyp-Angabe"), "{saetze:#?}");
        assert!(hat(4, "`<svg:script>`"), "{saetze:#?}");
        assert!(hat(5, "`<foreignObject>`"), "{saetze:#?}");
        assert!(hat(6, "Zeichencode (`&#…`)"), "{saetze:#?}");
        assert!(hat(7, "`javascript:`-Adresse"), "{saetze:#?}");
        assert!(hat(8, "Animation ändert `onclick`"), "{saetze:#?}");
        assert!(hat(9, "Animation ändert `href`") && hat(9, "`javascript:`-Adresse"), "{saetze:#?}");
        assert!(hat(10, "`@import`") && hat(10, "`url(http…)`"), "{saetze:#?}");
    }

    #[test]
    fn kaputt_html_mit_netz_und_formular() {
        let f = fehler_von("kaputt-html");
        let saetze: Vec<String> = f.iter().map(|b| b.to_string()).collect();
        assert_eq!(f.len(), 3, "{saetze:#?}");
        assert!(saetze[0].starts_with("projects/kaputt-html/assets/vorschau.html, Zeile 4: "), "{saetze:#?}");
        assert!(saetze[0].contains("aus dem Netz"), "{saetze:#?}");
        assert!(saetze[1].contains("Zeile 5") && saetze[1].contains("@import"), "{saetze:#?}");
        assert!(saetze[2].contains("Zeile 8") && saetze[2].contains("Formular"), "{saetze:#?}");
        // Ein Link nach außen (`<a href="https://…">`) ist erlaubt (Zeile 7).
        assert!(!saetze.iter().any(|s| s.contains("Zeile 7")), "{saetze:#?}");
    }

    #[test]
    fn kaputt_asset_namen_und_schriften() {
        let f = fehler_von("kaputt-asset");
        let saetze: Vec<String> = f.iter().map(|b| b.to_string()).collect();
        assert_eq!(f.len(), 3, "{saetze:#?}");
        assert!(saetze.iter().any(|s| s.contains("assets/Gross.png") && s.contains("einfachen Namen")), "{saetze:#?}");
        assert!(saetze.iter().any(|s| s.contains("assets/schrift.woff") && s.contains("Schriften")), "{saetze:#?}");
        assert!(saetze.iter().any(|s| s.contains("assets/unter") && s.contains("Unterordner")), "{saetze:#?}");
    }

    #[test]
    fn hinweise_summary_open_und_unterseiten() {
        let b = pruefe_projekt(&kaputt_ordner(), "kaputt-hinweise");
        let saetze: Vec<String> = b.iter().map(|b| b.to_string()).collect();
        // Ein Fehler (Lebenszeichen ohne http) und drei Hinweise.
        assert_eq!(zaehlen(&b), (1, 3), "{saetze:#?}");
        assert!(saetze.iter().any(|s| s.contains("Zeile 4: `summary` ist leer")), "{saetze:#?}");
        assert!(saetze.iter().any(|s| s.contains("Zeile 5: Der offene Punkt") && s.contains("höchstens 200 Zeichen")), "{saetze:#?}");
        assert!(saetze.iter().any(|s| s.contains("Zeile 8: Zur Unterseite „plan“")), "{saetze:#?}");
        assert!(saetze.iter().any(|s| s.contains("Zeile 13: Das Lebenszeichen „Website“")), "{saetze:#?}");
    }

    #[test]
    fn adressen() {
        for gut in ["https://example.org", "http://127.0.0.1:8787/", "D:/Ablage/Meine Projekte", "mailto:a@b.invalid", "/projects/x", "file:///C:/x"] {
            assert!(ist_link_adresse(gut), "{gut}");
        }
        for falsch in ["www.example.org", "", " https://a", "https://", "javascript:alert(1)", "//example.org", "https://a b"] {
            assert!(!ist_link_adresse(falsch), "{falsch}");
        }
        assert!(ist_http_adresse("http://127.0.0.1:9/"));
        assert!(!ist_http_adresse("127.0.0.1:9"));
        assert!(!ist_http_adresse("C:/x"));
    }

    /// Etappe A1: schwerpunkte.toml der Beispiele: keine Fehler; ein
    /// unbekanntes Projekt und die Projekte ohne Schwerpunkt als Hinweise.
    #[test]
    fn schwerpunkte_der_beispiele() {
        let beispiel = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("beispiel");
        let b = pruefe_schwerpunkte(&beispiel.join("data").join("schwerpunkte.toml"), &[beispiel.join("projects")]);
        let saetze: Vec<String> = b.iter().map(|b| b.to_string()).collect();
        assert!(b.iter().all(|b| !b.ist_fehler() && b.datei == "data/schwerpunkte.toml"), "{saetze:#?}");
        assert!(saetze.iter().any(|s| s.starts_with("data/schwerpunkte.toml, Zeile 13: Das Projekt „gibt-es-nicht“ steht hier")), "{saetze:#?}");
        assert!(saetze.iter().any(|s| s.contains("„neu-daten“ hat keinen Schwerpunkt und steht unter „Sonstiges“")), "{saetze:#?}");
        assert!(!saetze.iter().any(|s| s.contains("„dashboard“")), "dashboard ist zugeordnet: {saetze:#?}");
        // Ohne Datei: ein Hinweis.
        let b = pruefe_schwerpunkte(&beispiel.join("data").join("gibt-es-nicht.toml"), &[beispiel.join("projects")]);
        assert_eq!((b.len(), b[0].ist_fehler()), (1, false));
    }

    /// Mehrere Inhaltsquellen: Ein Projekt nur in der zweiten Quelle (Ordner
    /// mit Projekten oder einzelnes Projekt) gibt es; eins in keiner Quelle
    /// nicht. Ohne Schwerpunkt meldet sich auch ein Projekt der zweiten Quelle.
    #[test]
    fn schwerpunkte_ueber_alle_quellen() {
        let ordner = std::env::temp_dir().join(format!("dashboard-test-{}-schwerpunkte-quellen", std::process::id()));
        let _ = std::fs::remove_dir_all(&ordner);
        let eins = ordner.join("eins").join("projects");
        let zwei = ordner.join("zwei").join("projects");
        let einzeln = ordner.join("drei").join("projects").join("solo");
        for p in [eins.join("a"), zwei.join("umgezogen"), zwei.join("ohne"), einzeln.clone()] {
            std::fs::create_dir_all(&p).unwrap();
            std::fs::write(p.join("projekt.toml"), "schema = 3\ntitle = \"X\"\nstatus = \"aktiv\"\n").unwrap();
        }
        let datei = ordner.join("schwerpunkte.toml");
        std::fs::write(&datei, "[[schwerpunkt]]\nname = \"Eins\"\nreihenfolge = 1\nprojekte = [\"a\", \"umgezogen\", \"solo\", \"nirgends\"]\n").unwrap();
        let saetze = |quellen: &[PathBuf]| -> Vec<String> { pruefe_schwerpunkte(&datei, quellen).iter().map(|b| b.to_string()).collect() };
        let fehlt = |s: &[String], p: &str| s.iter().any(|s| s.contains(&format!("„{p}“ steht hier, aber es gibt keinen Ordner")));
        let ohne = |s: &[String], p: &str| s.iter().any(|s| s.contains(&format!("„{p}“ hat keinen Schwerpunkt")));
        let alle = saetze(&[eins.clone(), zwei.clone(), einzeln.clone()]);
        assert!(!fehlt(&alle, "umgezogen") && !fehlt(&alle, "solo") && !fehlt(&alle, "a"), "{alle:#?}");
        assert!(fehlt(&alle, "nirgends"), "{alle:#?}");
        assert!(ohne(&alle, "ohne"), "{alle:#?}");
        assert_eq!(alle.len(), 2, "{alle:#?}");
        // Gegenprobe: nur die erste Quelle (der alte Weg) kennt die anderen nicht.
        let erste = saetze(std::slice::from_ref(&eins));
        assert!(fehlt(&erste, "umgezogen") && fehlt(&erste, "solo") && fehlt(&erste, "nirgends"), "{erste:#?}");
        assert!(!ohne(&erste, "ohne"), "{erste:#?}");
        std::fs::remove_dir_all(&ordner).unwrap();
    }

    /// Doppelt zugeordnet und doppelter Name sind Fehler (Zeile beim zweiten
    /// Vorkommen); ein kaputtes Feld ist ein Fehler mit Zeile.
    #[test]
    fn schwerpunkte_doppelt_ist_ein_fehler() {
        let ordner = std::env::temp_dir().join(format!("dashboard-test-{}-schwerpunkte", std::process::id()));
        let _ = std::fs::remove_dir_all(&ordner);
        std::fs::create_dir_all(ordner.join("projects").join("a")).unwrap();
        std::fs::write(ordner.join("projects").join("a").join("projekt.toml"), "schema = 3\ntitle = \"A\"\nstatus = \"aktiv\"\n").unwrap();
        let datei = ordner.join("schwerpunkte.toml");
        std::fs::write(
            &datei,
            "[[schwerpunkt]]\nname = \"Eins\"\nreihenfolge = 1\nprojekte = [\"a\"]\n\n[[schwerpunkt]]\nname = \"Eins\"\nreihenfolge = 2\nprojekte = [\"a\"]\n",
        )
        .unwrap();
        let b = pruefe_schwerpunkte(&datei, &[ordner.join("projects")]);
        let saetze: Vec<String> = b.iter().map(|b| b.to_string()).collect();
        assert_eq!(zaehlen(&b), (2, 0), "{saetze:#?}");
        assert!(saetze.contains(&"data/schwerpunkte.toml, Zeile 7: Den Schwerpunkt „Eins“ gibt es zweimal. Jeder Name nur einmal.".to_string()), "{saetze:#?}");
        assert!(
            saetze.contains(
                &"data/schwerpunkte.toml, Zeile 9: Das Projekt „a“ steht mehrfach („Eins“ und „Eins“). Jedes Projekt gehört zu genau einem Schwerpunkt."
                    .to_string()
            ),
            "{saetze:#?}"
        );
        std::fs::write(&datei, "[[schwerpunkt]]\nname = \"Eins\"\nreihenfolge = \"erst\"\n").unwrap();
        let b = pruefe_schwerpunkte(&datei, &[ordner.join("projects")]);
        assert_eq!((b.len(), b[0].ist_fehler(), b[0].zeile), (1, true, Some(3)), "{b:?}");
        std::fs::remove_dir_all(&ordner).unwrap();
    }

    /// Entscheidung projektgruppe, [anzeige].tests 1: gruppen.toml
    /// gültig (keine Befunde), fehlt (nichts), unbekannter Teil, doppelter Teil
    /// (in einer Gruppe und über zwei), Slug-Kollision mit einem Projekt.
    #[test]
    fn gruppen_gueltig_unbekannt_doppelt_kollision() {
        let ordner = std::env::temp_dir().join(format!("dashboard-test-{}-gruppen", std::process::id()));
        let _ = std::fs::remove_dir_all(&ordner);
        for p in ["alpha", "beta", "gamma"] {
            std::fs::create_dir_all(ordner.join("projects").join(p)).unwrap();
            std::fs::write(ordner.join("projects").join(p).join("projekt.toml"), format!("schema = 3\ntitle = \"{p}\"\nstatus = \"aktiv\"\n")).unwrap();
        }
        let projekte = [ordner.join("projects")];
        let datei = ordner.join("gruppen.toml");
        // Fehlt die Datei: keine Gruppen, kein Befund.
        assert_eq!(pruefe_gruppen(&datei, &projekte), []);
        // Gültig.
        std::fs::write(&datei, "[[gruppe]]\nslug = \"alpha-und-beta\"\ntitel = \"Alpha und Beta\"\nteile = [\"alpha\", \"beta\"]\n").unwrap();
        assert_eq!(pruefe_gruppen(&datei, &projekte), []);
        // Unbekannter Teil.
        std::fs::write(&datei, "[[gruppe]]\nslug = \"alpha-und-beta\"\ntitel = \"Alpha und Beta\"\nteile = [\"alpha\", \"bete\"]\n").unwrap();
        let saetze: Vec<String> = pruefe_gruppen(&datei, &projekte).iter().map(|b| b.to_string()).collect();
        assert_eq!(
            saetze,
            [
                "data/gruppen.toml, Zeile 4: Das Projekt „bete“ in der Gruppe „Alpha und Beta“ gibt es nicht (kein Ordner projects/bete mit projekt.toml; vertippt?)."
            ]
        );
        // Doppelter Teil: über zwei Gruppen und in einer.
        std::fs::write(
            &datei,
            "[[gruppe]]\nslug = \"alpha-und-beta\"\ntitel = \"Alpha und Beta\"\nteile = [\"alpha\", \"beta\"]\n\n\
             [[gruppe]]\nslug = \"zweite\"\ntitel = \"Zweite\"\nteile = [\"gamma\", \"beta\", \"gamma\"]\n",
        )
        .unwrap();
        let b = pruefe_gruppen(&datei, &projekte);
        let saetze: Vec<String> = b.iter().map(|b| b.to_string()).collect();
        assert_eq!(zaehlen(&b), (2, 0), "{saetze:#?}");
        assert!(saetze.contains(&"data/gruppen.toml, Zeile 9: Das Projekt „beta“ steht schon in der Gruppe „Alpha und Beta“. Ein Projekt gehört höchstens einer Gruppe an, und nur einmal.".to_string()), "{saetze:#?}");
        assert!(saetze.contains(&"data/gruppen.toml, Zeile 9: Das Projekt „gamma“ steht in dieser Gruppe schon einmal. Ein Projekt gehört höchstens einer Gruppe an, und nur einmal.".to_string()), "{saetze:#?}");
        // Slug-Kollision mit einem Projekt, doppelter Slug, ungültiger Slug, ohne Titel, ohne Teile.
        std::fs::write(
            &datei,
            "[[gruppe]]\nslug = \"gamma\"\ntitel = \"Gamma\"\nteile = [\"alpha\"]\n\n\
             [[gruppe]]\nslug = \"gamma\"\ntitel = \"\"\nteile = []\n\n\
             [[gruppe]]\nslug = \"Gross\"\ntitel = \"G\"\nteile = [\"beta\"]\n",
        )
        .unwrap();
        let b = pruefe_gruppen(&datei, &projekte);
        let saetze: Vec<String> = b.iter().map(|b| b.to_string()).collect();
        assert_eq!(zaehlen(&b), (4, 1), "{saetze:#?}");
        assert_eq!(
            saetze[0],
            "data/gruppen.toml, Zeile 2: Den Slug „gamma“ gibt es schon als Projekt. Eine Gruppe braucht einen eigenen Slug, etwa „gamma-gruppe“."
        );
        assert!(saetze.contains(&"data/gruppen.toml, Zeile 7: Die Gruppe „gamma“ gibt es zweimal. Jeder Slug nur einmal.".to_string()), "{saetze:#?}");
        assert!(saetze.iter().any(|s| s.starts_with("data/gruppen.toml, Zeile 12: Der Slug „Gross“ geht nicht")), "{saetze:#?}");
        assert!(
            saetze.contains(&"data/gruppen.toml, Zeile 7: Die Gruppe „gamma“ hat keinen Titel. Beispiel: `titel = \"Alpha und Beta\"`".to_string()),
            "{saetze:#?}"
        );
        // Ein kaputtes Feld: ein Fehler mit Zeile.
        std::fs::write(&datei, "[[gruppe]]\nslug = \"x\"\ntitel = \"X\"\nfarbe = \"rot\"\n").unwrap();
        let b = pruefe_gruppen(&datei, &projekte);
        assert_eq!((b.len(), b[0].ist_fehler(), b[0].zeile), (1, true, Some(4)), "{b:?}");
        std::fs::remove_dir_all(&ordner).unwrap();
    }

    /// Mehrere Inhaltsquellen: Ein Teil nur in der zweiten Quelle gibt es, und
    /// ein Gruppen-Slug, den es dort als Projekt gibt, kollidiert. Gegenprobe
    /// mit nur der ersten Quelle (der alte Weg): umgekehrt.
    #[test]
    fn gruppen_ueber_alle_quellen() {
        let ordner = std::env::temp_dir().join(format!("dashboard-test-{}-gruppen-quellen", std::process::id()));
        let _ = std::fs::remove_dir_all(&ordner);
        let eins = ordner.join("eins").join("projects");
        let zwei = ordner.join("zwei").join("projects");
        for p in [eins.join("alpha"), zwei.join("umgezogen"), zwei.join("beides")] {
            std::fs::create_dir_all(&p).unwrap();
            std::fs::write(p.join("projekt.toml"), "schema = 3\ntitle = \"X\"\nstatus = \"aktiv\"\n").unwrap();
        }
        let datei = ordner.join("gruppen.toml");
        std::fs::write(&datei, "[[gruppe]]\nslug = \"beides\"\ntitel = \"Beides\"\nteile = [\"alpha\", \"umgezogen\"]\n").unwrap();
        let saetze = |quellen: &[PathBuf]| -> Vec<String> { pruefe_gruppen(&datei, quellen).iter().map(|b| b.to_string()).collect() };
        let fehlt = "Das Projekt „umgezogen“ in der Gruppe „Beides“ gibt es nicht";
        let kollision = "Den Slug „beides“ gibt es schon als Projekt";
        let alle = saetze(&[eins.clone(), zwei.clone()]);
        assert!(!alle.iter().any(|s| s.contains(fehlt)), "{alle:#?}");
        assert!(alle.iter().any(|s| s.contains(kollision)), "{alle:#?}");
        assert_eq!(alle.len(), 1, "{alle:#?}");
        let erste = saetze(std::slice::from_ref(&eins));
        assert!(erste.iter().any(|s| s.contains(fehlt)), "{erste:#?}");
        assert!(!erste.iter().any(|s| s.contains(kollision)), "{erste:#?}");
        std::fs::remove_dir_all(&ordner).unwrap();
    }

    /// data/pc.toml: die Beispiel-Datei ist gut, ein falsches `cloud_stand`
    /// ist ein Fehler mit Zeile, eine fehlende Datei nichts.
    #[test]
    fn pc_toml_cloud_stand() {
        let beispiel = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("beispiel");
        assert_eq!(pruefe_pc(&beispiel.join("data").join("pc.toml")), []);
        assert_eq!(pruefe_pc(&beispiel.join("data").join("gibt-es-nicht.toml")), []);
        let ordner = std::env::temp_dir().join(format!("dashboard-test-{}-pc-toml", std::process::id()));
        let _ = std::fs::remove_dir_all(&ordner);
        std::fs::create_dir_all(&ordner).unwrap();
        let datei = ordner.join("pc.toml");
        std::fs::write(&datei, "[backup]\nexternal_disk = false\ncloud_stand = \"vollstaendig\"\n").unwrap();
        let b = pruefe_pc(&datei);
        let saetze: Vec<String> = b.iter().map(|b| b.to_string()).collect();
        assert_eq!(saetze, ["data/pc.toml, Zeile 3: Bei `cloud_stand` steht „vollstaendig“. Erlaubt sind: vorhanden, teilweise, fehlt."]);
        assert!(b[0].ist_fehler());
        // cloud sagt etwas, cloud_stand fehlt: nur ein Hinweis, auch bei „keine“.
        std::fs::write(&datei, "[backup]\nexternal_disk = false\ncloud = \"keine\"\n").unwrap();
        let b = pruefe_pc(&datei);
        let saetze: Vec<String> = b.iter().map(|b| b.to_string()).collect();
        assert_eq!(saetze, ["data/pc.toml, Zeile 3: Bitte cloud_stand setzen (vorhanden, teilweise oder fehlt)."]);
        assert!(!b[0].ist_fehler());
        // Mit cloud_stand: nichts.
        std::fs::write(&datei, "[backup]\ncloud = \"Cloud\"\ncloud_stand = \"teilweise\"\n").unwrap();
        assert_eq!(pruefe_pc(&datei), []);
        std::fs::remove_dir_all(&ordner).unwrap();
    }

    /// Etappe 2e: `kennung` in `[[history]]` von data/pc.toml. Doppelt oder
    /// nicht einfach ist ein Fehler (Zeile der zweiten), fehlend ein Hinweis.
    #[test]
    fn pc_toml_kennungen() {
        let ordner = std::env::temp_dir().join(format!("dashboard-test-{}-pc-kennung", std::process::id()));
        let _ = std::fs::remove_dir_all(&ordner);
        std::fs::create_dir_all(&ordner).unwrap();
        let datei = ordner.join("pc.toml");
        let text = "[[history]]\nkennung = \"netzteil\"\ndate = \"2026-10-12\"\ntitle = \"Netzteil getauscht\"\n\n\
                    [[history]]\ndate = \"2026-10-23\"\ntitle = \"Größe geprüft\"\n\n\
                    [[history]]\nkennung = \"netzteil\"\ndate = \"2026-10-24\"\ntitle = \"Noch eine\"\n\n\
                    [[history]]\nkennung = \"Mit Leerzeichen\"\ndate = \"2026-10-25\"\ntitle = \"Kaputt\"\n";
        std::fs::write(&datei, text).unwrap();
        let saetze: Vec<String> = pruefe_pc(&datei).iter().map(|b| format!("{} {b}", if b.ist_fehler() { "F" } else { "H" })).collect();
        assert_eq!(
            saetze,
            [
                "H data/pc.toml, Zeile 6: Der Eintrag „Größe geprüft“ in [[history]] hat keine `kennung`. Bitte eine eindeutige setzen, etwa `kennung = \"groesse-geprueft\"` (bis Schritt 6 erlaubt; bis dahin bildet der Server sie aus Datum und Titel).",
                "F data/pc.toml, Zeile 11: Die Kennung „netzteil“ gibt es in [[history]] zweimal. Jede `kennung` nur einmal.",
                "F data/pc.toml, Zeile 16: Der Eintrag „Kaputt“ hat keine einfache `kennung`. Erlaubt sind Kleinbuchstaben, Ziffern und Bindestriche, etwa `kennung = \"netzteil-getauscht\"`.",
            ]
        );
        std::fs::remove_dir_all(&ordner).unwrap();
    }

    /// Etappe B3: `daten/fristen.toml`. Fehler: unbekanntes Ereignis, doppelte
    /// Kennung, Abstand im falschen Format, Frist ohne Datum und ohne
    /// Ereignis; Hinweis: ein Ereignisdatum in der Zukunft.
    #[test]
    fn fristen_pruefen() {
        let (dir, ordner) = temp_projekt("fristen");
        let datei = ordner.join("daten").join("fristen.toml");
        let ereignis = "[[ereignis]]\nkennung = \"zusage-da\"\ntitel = \"Zusage ist da\"\n\n";
        let fehler = |toml: &str| -> String {
            std::fs::write(&datei, toml).unwrap();
            pruefe_projekt(&dir, "p").into_iter().find(|b| b.ist_fehler()).map(|b| b.to_string()).unwrap_or_else(|| panic!("kein Fehler bei {toml}"))
        };
        assert_eq!(
            fehler(&format!("{ereignis}[[frist]]\nkennung = \"standanmeldung\"\ntitel = \"Stand\"\nereignis = \"zusage\"\nabstand = \"+1 Monat\"\n")),
            "projects/p/daten/fristen.toml, Zeile 8: Die Frist „standanmeldung“ hängt an „zusage“, aber dieses Ereignis gibt es nicht. Vorhanden: zusage-da."
        );
        assert_eq!(
            fehler(&format!("{ereignis}[[frist]]\nkennung = \"zusage-da\"\ntitel = \"Doppelt\"\ndatum = \"2026-11-20\"\n")),
            "projects/p/daten/fristen.toml, Zeile 6: Die Kennung „zusage-da“ gibt es zweimal. Jede `kennung` nur einmal, Ereignisse und Fristen zusammen, auch über Teile hinweg."
        );
        assert_eq!(
            fehler(&format!("{ereignis}[[frist]]\nkennung = \"standanmeldung\"\ntitel = \"Stand\"\nereignis = \"zusage-da\"\nabstand = \"+10 Werktage\"\n")),
            "projects/p/daten/fristen.toml, Zeile 9: Bei der Frist „standanmeldung“ steht `abstand = \"+10 Werktage\"`. Erlaubt ist: Vorzeichen, Zahl, Leerzeichen, Einheit, etwa `+14 Tage`, `+2 Wochen`, `+1 Monat`, `+1 Jahr` oder `-3 Tage`. Werktage gibt es nicht."
        );
        assert_eq!(
            fehler("[[frist]]\nkennung = \"irgendwann\"\ntitel = \"Irgendwann\"\n"),
            "projects/p/daten/fristen.toml, Zeile 2: Die Frist „irgendwann“ hat weder `datum` noch `ereignis`. Fest: `datum = \"2026-11-20\"`; beweglich: `ereignis = \"<kennung>\"` und `abstand = \"+14 Tage\"`."
        );
        assert!(fehler(&format!("{ereignis}[[frist]]\nkennung = \"x\"\ntitel = \"X\"\nereignis = \"zusage-da\"\n")).contains("fehlt `abstand`"));
        assert!(
            fehler("[[frist]]\nkennung = \"x\"\ntitel = \"X\"\ndatum = \"2026-11-20\"\nereignis = \"y\"\nabstand = \"+1 Tag\"\n")
                .contains("hat `datum` und `ereignis`")
        );
        assert!(fehler("[[ereignis]]\nkennung = \"a\"\ntitel = \"A\"\ndatum = \"4.11.2026\"\n").contains("Erlaubt ist JJJJ-MM-TT"));
        // Teile: die Kennung zählt über Teile, das Ereignis darf in einem anderen Teil stehen.
        assert!(
            fehler("[teil.a]\n[[teil.a.ereignis]]\nkennung = \"e\"\ntitel = \"E\"\n[teil.b]\n[[teil.b.ereignis]]\nkennung = \"e\"\ntitel = \"F\"\n")
                .contains("gibt es zweimal")
        );
        std::fs::write(&datei, "[teil.a]\n[[teil.a.ereignis]]\nkennung = \"e\"\ntitel = \"E\"\n[teil.b]\n[[teil.b.frist]]\nkennung = \"f\"\ntitel = \"F\"\nereignis = \"e\"\nabstand = \"+2 Wochen\"\n").unwrap();
        assert_eq!(zaehlen(&pruefe_projekt(&dir, "p")), (0, 0));
        // Ein Ereignisdatum in der Zukunft: nur ein Hinweis.
        let morgen = (abgeleitet::heute() + chrono::Duration::days(1)).format("%Y-%m-%d").to_string();
        std::fs::write(&datei, format!("[[ereignis]]\nkennung = \"termin\"\ntitel = \"Termin\"\ndatum = \"{morgen}\"\n")).unwrap();
        let b = pruefe_projekt(&dir, "p");
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(zaehlen(&b), (0, 1), "{b:#?}");
        assert!(
            b[0].to_string()
                .starts_with(&format!("projects/p/daten/fristen.toml, Zeile 4: Das Ereignis „termin“ hat das Datum {morgen}, das liegt in der Zukunft.")),
            "{}",
            b[0]
        );
    }

    /// Etappe 2e: `kennung` im Zeitstrahl. Doppelt (auch über Teile) oder
    /// nicht einfach ist ein Fehler mit Zeile; fehlend und `farbe` statt
    /// `art` sind Hinweise.
    #[test]
    fn zeitstrahl_kennungen() {
        let (dir, ordner) = temp_projekt("zeitstrahl-kennung");
        let datei = ordner.join("daten").join("zeitstrahl.toml");
        std::fs::write(
            &datei,
            "[teil.a]\n[[teil.a.eintrag]]\nkennung = \"start\"\ndatum = \"2026-10-24\"\ntitel = \"A\"\n\n\
             [teil.b]\n[[teil.b.eintrag]]\nkennung = \"start\"\ndatum = \"2026-10-25\"\ntitel = \"B\"\n",
        )
        .unwrap();
        let f = pruefe_projekt(&dir, "p").into_iter().find(|b| b.ist_fehler()).expect("doppelt ist ein Fehler");
        assert_eq!(
            f.to_string(),
            "projects/p/daten/zeitstrahl.toml, Zeile 9: Die Kennung „start“ gibt es zweimal. Jede `kennung` nur einmal, auch über Teile hinweg."
        );
        std::fs::write(&datei, "[[eintrag]]\nkennung = \"Groß\"\ndatum = \"2026-10-24\"\ntitel = \"A\"\n").unwrap();
        let f = pruefe_projekt(&dir, "p").into_iter().find(|b| b.ist_fehler()).expect("nicht einfach ist ein Fehler");
        assert_eq!(f.zeile, Some(2), "{f}");
        assert!(f.text.starts_with("Der Eintrag „A“ hat keine einfache `kennung`."), "{f}");
        std::fs::write(&datei, "[[eintrag]]\ndatum = \"2020-01\"\ntitel = \"Alt\"\nfarbe = \"rot\"\n\n[[eintrag]]\nkennung = \"neu\"\ndatum = \"2020-02\"\ntitel = \"Neu\"\nart = \"abgebrochen\"\n").unwrap();
        let b: Vec<String> = pruefe_projekt(&dir, "p").iter().filter(|b| b.datei.ends_with("zeitstrahl.toml")).map(|b| b.to_string()).collect();
        assert_eq!(b.len(), 2, "{b:#?}");
        assert!(b[0].starts_with("projects/p/daten/zeitstrahl.toml, Zeile 3: Der Eintrag „Alt“ hat keine `kennung`."), "{b:#?}");
        assert!(
            b[1].starts_with("projects/p/daten/zeitstrahl.toml, Zeile 4: `farbe` im Zeitstrahl wird `art` (bis Schritt 6 erlaubt): hier `art = \"problem\"`."),
            "{b:#?}"
        );
        std::fs::write(&datei, "[[eintrag]]\ndatum = \"2020-01\"\ntitel = \"X\"\nart = \"vielleicht\"\n").unwrap();
        let f = pruefe_projekt(&dir, "p").into_iter().find(|b| b.ist_fehler()).expect("falsche art ist ein Fehler");
        assert!(f.text.contains("vielleicht") && f.text.contains("erledigt, begonnen, geplant, abgebrochen, routine, problem"), "{f}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Etappe B2: `kanban.toml` läuft aus. Lesen geht weiter; `pruefe` gibt
    /// einen Hinweis (keinen Fehler), auch für eine leere Datei.
    #[test]
    fn kanban_laeuft_aus_als_hinweis() {
        let ordner = std::env::temp_dir().join(format!("dashboard-test-{}-kanban-b2", std::process::id()));
        let _ = std::fs::remove_dir_all(&ordner);
        std::fs::create_dir_all(ordner.join("p").join("daten")).unwrap();
        std::fs::write(ordner.join("p").join("projekt.toml"), "schema = 3\ntitle = \"P\"\nstatus = \"aktiv\"\nsummary = \"S\"\n").unwrap();
        for text in ["[[spalte]]\ntitel = \"offen\"\n\n[[spalte.karte]]\ntitel = \"Karte\"\n", ""] {
            std::fs::write(ordner.join("p").join("daten").join("kanban.toml"), text).unwrap();
            let b = pruefe_projekt(&ordner, "p");
            let kanban: Vec<&Befund> = b.iter().filter(|b| b.datei.ends_with("kanban.toml")).collect();
            assert_eq!(kanban.len(), 1, "{b:?}");
            assert!(!kanban[0].ist_fehler());
            assert_eq!(kanban[0].text, SATZ_KANBAN_LAEUFT_AUS);
        }
        std::fs::remove_file(ordner.join("p").join("daten").join("kanban.toml")).unwrap();
        assert!(pruefe_projekt(&ordner, "p").iter().all(|b| !b.text.contains("kanban.toml")));
        std::fs::remove_dir_all(&ordner).unwrap();
    }

    /// Etappe A2: Prüfbefehl zu fragen.toml: doppelte
    /// Kennung ist ein Fehler, eine Frage ohne `prioritaet` ein Hinweis. Seit
    /// Etappe B2 sind viele Fragen kein Hinweis mehr (Warteschlange).
    #[test]
    fn fragen_ohne_prioritaet_und_viele() {
        let ordner = std::env::temp_dir().join(format!("dashboard-test-{}-fragen-a2", std::process::id()));
        let _ = std::fs::remove_dir_all(&ordner);
        let frage = |id: &str, prio: Option<&str>| {
            let p = prio.map(|p| format!("prioritaet = \"{p}\"\n")).unwrap_or_default();
            format!("[[frage]]\nid = \"{id}\"\ntext = \"?\"\nart = \"text\"\ngestellt = \"2026-10-25\"\n{p}\n")
        };
        let slug = "neu";
        std::fs::create_dir_all(ordner.join(slug).join("daten")).unwrap();
        std::fs::write(
            ordner.join(slug).join("projekt.toml"),
            "schema = 3
title = \"P\"
status = \"aktiv\"
summary = \"S\"
",
        )
        .unwrap();
        let text = [frage("a", Some("jetzt")), frage("b", None), frage("c", Some("bald")), frage("d", Some("irgendwann"))].concat();
        std::fs::write(ordner.join(slug).join("daten").join("fragen.toml"), text).unwrap();
        let b = pruefe_projekt(&ordner, slug);
        let fragen: Vec<String> = b.iter().filter(|b| b.datei.ends_with("fragen.toml")).map(|b| b.to_string()).collect();
        assert!(b.iter().filter(|b| b.datei.ends_with("fragen.toml")).all(|b| !b.ist_fehler()), "{b:?}");
        assert_eq!(fragen.len(), 1, "nur die fehlende prioritaet, nichts zur Zahl: {slug}: {fragen:?}");
        assert!(fragen[0].starts_with(&format!("projects/{slug}/daten/fragen.toml, Zeile 9: Die Frage „b“ hat keine `prioritaet`")), "{fragen:?}");
        // Sechs Fragen mit Stufe: kein Hinweis.
        let viele = [frage("e", Some("bald")), frage("f", Some("bald")), frage("g", Some("irgendwann"))].concat();
        std::fs::write(
            ordner.join(slug).join("daten").join("fragen.toml"),
            format!("{}{viele}", [frage("a", Some("jetzt")), frage("c", Some("bald")), frage("d", Some("bald"))].concat()),
        )
        .unwrap();
        assert!(pruefe_projekt(&ordner, slug).iter().all(|b| !b.datei.ends_with("fragen.toml")), "{slug}");
        let drei = [frage("a", Some("jetzt")), frage("c", Some("bald")), frage("d", Some("bald"))].concat();
        std::fs::write(ordner.join(slug).join("daten").join("fragen.toml"), &drei).unwrap();
        assert!(pruefe_projekt(&ordner, slug).iter().all(|b| !b.datei.ends_with("fragen.toml")), "{slug}");
        // Doppelte Kennung: Fehler.
        std::fs::write(ordner.join(slug).join("daten").join("fragen.toml"), format!("{drei}{}", frage("a", Some("bald")))).unwrap();
        let b = pruefe_projekt(&ordner, slug);
        assert!(b.iter().any(|b| b.ist_fehler() && b.text.contains("Die Frage „a“ gibt es zweimal")), "{slug}: {b:?}");
        std::fs::remove_dir_all(&ordner).unwrap();
    }
}
