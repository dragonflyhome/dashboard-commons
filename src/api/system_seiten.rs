//! `/api/entscheidungen`, `/api/zielbild` (Etappe 4d) und `/api/anleitung`
//! (Etappe 6b): die Daten der System-Seiten aus `docs/entscheidungen/*.toml`,
//! `docs/zielbild/*.toml` und `docs/anleitung/*.toml`.
//!
//! Entscheidungen und Zielbild gehören der Architektur-Rolle, die Anleitung
//! dem Betreuer des Dashboards; der Server liest nur und
//! schreibt nie hinein. Gelesen wird je Anfrage (wenige kleine Dateien) im
//! Hintergrund. Duldsam: ein fehlendes Feld fehlt einfach, eine kaputte Datei
//! wird ein Eintrag mit Satz, der Rest der Liste bleibt. Fehlt der Ordner,
//! ist die Liste leer; darf der Server ihn nicht lesen (Hotfix 4d: der Dienst
//! hatte kein Recht auf docs), kommt eine `warnung` statt einer still leeren
//! Liste. Nach außen geht nur
//! der Dateiname, kein Pfad. Seit der Abnahme 4d alle drei geschützt
//! (`sicherheit::geschuetzt`, ohne Anmeldung 401); käme doch eine Anfrage
//! ohne Anmeldung durch, antwortet der Handler selbst 401 (zweite Wand wie
//! `/api/fristen`).

use std::path::{Path, PathBuf};

use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use serde::Serialize;

use super::vertrag::{
    AnleitungAntwort, Entscheidung, EntscheidungsListe, Feld, FeldWert, SystemAbschnitt, SystemBlock, SystemBlockInhalt, SystemBlockTabelle, ZielbildAntwort,
};
use crate::anmeldung::Angemeldet;
use crate::app::AppState;
use crate::fehler::AppError;
use crate::vertrag::lesen;

/// Größer liest der Server eine Datei nicht (eine Entscheidung hat wenige KB).
/// Dieselbe Grenze gilt für `pruefe --system`.
pub const HOECHSTENS: u64 = 512 * 1024;

/// Höchstens so viele Dateien je Ordner; was darüber liegt, bleibt weg
/// (auch bei `pruefe --system`, das es dann meldet).
pub const HOECHSTENS_DATEIEN: usize = 2000;

/// `docs/` im Code-Repo (dort liegt `docs/zielbild`): aus den Einstellungen
/// (`repo`), sonst neben dem Projektordner, wenn es dort `docs/zielbild`
/// gibt (vor dem Umzug; in Tests `tests/beispiel/docs`), sonst neben `web/`.
/// Die Entscheidungen liegen seit dem Umzug im privaten Inhalt
/// ([`entscheidungen_ordner`]).
pub fn docs_ordner(state: &AppState) -> PathBuf {
    docs_ordner_aus(state.einstellungen.as_deref(), &state.projects_dir, state.web.build())
}

/// Dasselbe ohne `AppState`, für den Beobachter beim Zusammenbau
/// (Live-Lücken); `web_build` ist `<repo>/web/build`.
pub fn docs_ordner_aus(einstellungen: Option<&crate::dienst::einstellungen::Einstellungen>, projekte: &Path, web_build: &Path) -> PathBuf {
    if let Some(e) = einstellungen {
        return e.repo.join("docs");
    }
    let neben = projekte.parent().map(Path::to_path_buf).unwrap_or_default().join("docs");
    if neben.join("zielbild").is_dir() {
        return neben;
    }
    match web_build.parent().and_then(Path::parent) {
        Some(repo) if !repo.as_os_str().is_empty() => repo.join("docs"),
        _ => neben,
    }
}

/// Die Entscheidungen (`/api/entscheidungen`): privater Inhalt, aus
/// `[inhalt].entscheidungen` (sonst `<repo>/docs/entscheidungen`) oder ohne
/// Einstellungen neben dem Projektordner (Entscheidung
/// `github-trennung` [docs_oeffentlich]).
pub fn entscheidungen_ordner(state: &AppState) -> PathBuf {
    state.entscheidungen_dir.to_path_buf()
}

/// Eine Datei des Ordners: Name und Inhalt als Tabelle, oder der Satz, warum nicht.
struct Gelesen {
    name: String,
    inhalt: Result<toml::Table, String>,
}

/// Der Befehl, mit dem der Inhaber das Lese-Recht des Dienstes einmal nachzieht
/// (als Administrator, nur wenn Dienst.ps1 -Status es meldet; /anleitung, „Das Dashboard als Dienst“).
pub const BEFEHL_LESERECHT: &str = r"scripts\Dienst-Einrichten.ps1 -NurLeserecht";

/// Wer das fehlende Lese-Recht setzt, nach Ordner (kurzer Name wie in
/// `meldungen::gelesene_ordner`; Auflage 6d): docs\zielbild und docs\anleitung
/// liegen im Code-Repo, dort setzt es der Betreuer des Dashboards als Besitzer; web
/// gibt nur das Einrichten frei; alles andere liegt im privaten Inhalt (derselbe
/// Befehl; er liest die Ordner aus [inhalt] in dienst.toml, -Inhalt braucht es nicht).
pub fn leserecht_hilfe(ordner: &str) -> String {
    match ordner {
        r"docs\zielbild" | r"docs\anleitung" => format!(
            "Das Lese-Recht setzt der Betreuer des Dashboards (der Ordner liegt im Code-Repo). Meldet scripts\\Dienst.ps1 -Status es trotzdem, einmal als Administrator {BEFEHL_LESERECHT} (Anleitung, „Das Dashboard als Dienst“)."
        ),
        "web" => format!("Einmal als Administrator {BEFEHL_LESERECHT} (Anleitung, „Das Dashboard als Dienst“)."),
        _ => format!(
            "Der Ordner liegt im privaten Inhalt; dort hilft kein Chat. Einmal als Administrator {BEFEHL_LESERECHT} (Anleitung, „Das Dashboard als Dienst“)."
        ),
    }
}

/// Welcher Ordner, für die Sätze.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ordner {
    Entscheidungen,
    Zielbild,
    Anleitung,
}

impl Ordner {
    fn mit_artikel(self) -> &'static str {
        match self {
            Ordner::Entscheidungen => "den Ordner mit den Entscheidungen",
            Ordner::Zielbild => "den Ordner mit dem Zielbild",
            Ordner::Anleitung => "den Ordner mit der Anleitung",
        }
    }

    /// Der kurze Name wie in `meldungen::gelesene_ordner`.
    fn name(self) -> &'static str {
        match self {
            Ordner::Entscheidungen => r"docs\entscheidungen",
            Ordner::Zielbild => r"docs\zielbild",
            Ordner::Anleitung => r"docs\anleitung",
        }
    }

    fn nominativ(self) -> &'static str {
        match self {
            Ordner::Entscheidungen => "Der Ordner mit den Entscheidungen",
            Ordner::Zielbild => "Der Ordner mit dem Zielbild",
            Ordner::Anleitung => "Der Ordner mit der Anleitung",
        }
    }
}

/// Die Warnung zu einem Fehler beim Öffnen des Ordners, ohne Pfad. Fehlt der
/// Ordner (`NotFound`), keine: Dann zeigt die Seite wie bisher den
/// Platzhalter. Kein Recht (`PermissionDenied`) ist etwas anderes als
/// „fehlt“ (Live-Befund 4d: der Dienst durfte docs nicht lesen, und die
/// Seite war still leer).
pub fn ordner_warnung(ordner: Ordner, e: &std::io::Error) -> Option<String> {
    match e.kind() {
        std::io::ErrorKind::NotFound => None,
        std::io::ErrorKind::PermissionDenied => {
            Some(format!("Das Dashboard darf {} nicht lesen. {} Danach steht hier wieder alles.", ordner.mit_artikel(), leserecht_hilfe(ordner.name())))
        }
        _ => Some(format!("{} lässt sich gerade nicht lesen. Einzelheiten stehen im Protokoll.", ordner.nominativ())),
    }
}

/// Der Satz zu einer Datei, die sich nicht öffnen lässt (ohne Pfad).
pub fn datei_satz(e: &std::io::Error) -> String {
    match e.kind() {
        std::io::ErrorKind::PermissionDenied => "Das Dashboard darf diese Datei nicht lesen (kein Leserecht). Der Rest der Liste ist vollständig.".to_string(),
        _ => "Diese Datei lässt sich gerade nicht öffnen.".to_string(),
    }
}

/// Alle `*.toml` direkt in `ordner` (keine Unterordner, keine Verknüpfungen),
/// nach Namen sortiert. Der Fehler beim Öffnen des Ordners geht an den
/// Aufrufer (`NotFound` heißt: fehlt, alles andere: Warnung).
fn toml_dateien(ordner: &Path) -> std::io::Result<Vec<Gelesen>> {
    let (namen, _) = toml_namen(ordner)?;
    Ok(namen.into_iter().map(|(name, pfad)| Gelesen { inhalt: lesen_als_tabelle(&name, &pfad), name }).collect())
}

/// Name und Pfad aller `*.toml` direkt in `ordner`, nach Namen sortiert,
/// höchstens [`HOECHSTENS_DATEIEN`]; dazu, wie viele es ohne Grenze wären.
/// Server und `pruefe --system` lesen dieselben Dateien.
pub fn toml_namen(ordner: &Path) -> std::io::Result<(Vec<(String, PathBuf)>, usize)> {
    let rd = std::fs::read_dir(ordner)?;
    let mut namen: Vec<(String, PathBuf)> = rd
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            let gut = name.to_ascii_lowercase().ends_with(".toml") && !name.starts_with('.');
            gut.then(|| (name, e.path()))
        })
        .collect();
    namen.sort();
    let alle = namen.len();
    namen.truncate(HOECHSTENS_DATEIEN);
    Ok((namen, alle))
}

/// Der Text einer Datei (ohne BOM), mit derselben Größengrenze wie der
/// Server; sonst der Satz ohne Pfad.
pub fn datei_text(pfad: &Path) -> Result<String, String> {
    let groesse = std::fs::metadata(pfad).map(|m| m.len()).map_err(|e| datei_satz(&e))?;
    if groesse > HOECHSTENS {
        return Err(format!("Diese Datei ist zu groß (über {} KB) und wird nicht gelesen.", HOECHSTENS / 1024));
    }
    let bytes = std::fs::read(pfad).map_err(|e| datei_satz(&e))?;
    let text = String::from_utf8(bytes).map_err(|_| "Diese Datei ist nicht in UTF-8 gespeichert und lässt sich nicht lesen.".to_string())?;
    Ok(match text.strip_prefix('\u{feff}') {
        Some(rest) => rest.to_string(),
        None => text,
    })
}

/// Ergebnis von `toml_dateien` als Liste und Warnung; ein Fehler wird ins
/// Protokoll geschrieben (nur die Art, kein Pfad nach außen).
fn liste_und_warnung(ordner: Ordner, r: std::io::Result<Vec<Gelesen>>) -> (Vec<Gelesen>, Option<String>) {
    match r {
        Ok(v) => (v, None),
        Err(e) => {
            let warnung = ordner_warnung(ordner, &e);
            if warnung.is_some() {
                tracing::warn!("System-Seiten: {} nicht lesbar: {e}", ordner.mit_artikel());
            }
            (Vec::new(), warnung)
        }
    }
}

/// Liest eine Datei als TOML-Tabelle; Fehler als Satz ohne Pfad.
fn lesen_als_tabelle(name: &str, pfad: &Path) -> Result<toml::Table, String> {
    let text = datei_text(pfad)?;
    toml::from_str::<toml::Table>(&text).map_err(|e| {
        let f = lesen::aus_toml_fehler(name, &text, &e);
        match f.zeile {
            Some(z) => format!("Diese Datei lässt sich nicht lesen: In Zeile {z} ist die Schreibweise falsch. Der Rest der Liste ist vollständig."),
            None => "Diese Datei lässt sich nicht lesen: Die Schreibweise ist falsch. Der Rest der Liste ist vollständig.".to_string(),
        }
    })
}

/// Ein einfacher Wert als Text, sonst `None` (Liste oder Tabelle).
fn einfach(v: &toml::Value) -> Option<String> {
    match v {
        toml::Value::String(s) => Some(s.clone()),
        toml::Value::Integer(i) => Some(i.to_string()),
        toml::Value::Float(f) => Some(f.to_string()),
        toml::Value::Boolean(b) => Some(if *b { "ja" } else { "nein" }.to_string()),
        toml::Value::Datetime(d) => Some(d.to_string()),
        toml::Value::Array(_) | toml::Value::Table(_) => None,
    }
}

/// Jeder Wert als einfacher Text, Liste oder Gruppe.
fn wert(v: &toml::Value) -> FeldWert {
    match v {
        toml::Value::Array(a) => FeldWert::Liste(a.iter().map(wert).collect()),
        toml::Value::Table(t) => FeldWert::Gruppe(felder(t.iter())),
        _ => FeldWert::Text(einfach(v).unwrap_or_default()),
    }
}

fn felder<'a>(paare: impl Iterator<Item = (&'a String, &'a toml::Value)>) -> Vec<Feld> {
    paare.map(|(k, v)| Feld { name: k.clone(), wert: wert(v) }).collect()
}

/// `JJJJ-MM-TT` vorn im Dateinamen, sonst `None`.
fn datum_aus_name(name: &str) -> Option<String> {
    let kopf = name.get(..10)?;
    let b = kopf.as_bytes();
    let form = b.iter().enumerate().all(|(i, c)| if i == 4 || i == 7 { *c == b'-' } else { c.is_ascii_digit() });
    form.then(|| kopf.to_string())
}

/// Eine Entscheidung aus einer gelesenen Datei.
fn entscheidung(g: Gelesen) -> Entscheidung {
    let mut e = Entscheidung {
        datum: None,
        thema: None,
        status: None,
        von: None,
        etappe: None,
        urteil: None,
        was: None,
        weitere: Vec::new(),
        fehler: None,
        datei: g.name,
    };
    match g.inhalt {
        Err(satz) => e.fehler = Some(satz),
        Ok(t) => {
            for (k, v) in &t {
                let ziel = match k.as_str() {
                    "datum" => &mut e.datum,
                    "thema" => &mut e.thema,
                    "status" => &mut e.status,
                    "von" => &mut e.von,
                    "etappe" => &mut e.etappe,
                    "urteil" => &mut e.urteil,
                    "was" => &mut e.was,
                    _ => {
                        e.weitere.push(Feld { name: k.clone(), wert: wert(v) });
                        continue;
                    }
                };
                match einfach(v) {
                    Some(s) => *ziel = Some(s),
                    None => e.weitere.push(Feld { name: k.clone(), wert: wert(v) }),
                }
            }
        }
    }
    if e.datum.is_none() {
        e.datum = datum_aus_name(&e.datei);
    }
    e
}

/// Die Slugs aus dem freiwilligen Feld `projekte = ["slug"]` einer
/// Entscheidung (Etappe B8); ohne Feld oder in anderer Form keine. Nur so
/// erscheint sie in der Zeitleiste eines Projekts (`/api/projekte/<slug>/verlauf`).
pub fn projekte_der(e: &Entscheidung) -> Vec<&str> {
    let mut slugs = Vec::new();
    for f in e.weitere.iter().filter(|f| f.name == "projekte") {
        if let FeldWert::Liste(l) = &f.wert {
            slugs.extend(l.iter().filter_map(|w| if let FeldWert::Text(t) = w { Some(t.as_str()) } else { None }));
        }
    }
    slugs
}

/// Alle Entscheidungen in `ordner`, neueste zuerst.
pub fn entscheidungen_lesen(ordner: &Path) -> EntscheidungsListe {
    entscheidungen_aus(toml_dateien(ordner))
}

/// Die Liste aus dem Ergebnis des Lesens (getrennt, damit Tests einen
/// Fehler wie `PermissionDenied` einsetzen können).
fn entscheidungen_aus(r: std::io::Result<Vec<Gelesen>>) -> EntscheidungsListe {
    let (dateien, warnung) = liste_und_warnung(Ordner::Entscheidungen, r);
    let mut eintraege: Vec<Entscheidung> = dateien.into_iter().map(entscheidung).collect();
    eintraege.sort_by(|a, b| b.datum.cmp(&a.datum).then_with(|| b.datei.cmp(&a.datei)));
    EntscheidungsListe { eintraege, warnung }
}

/// Die Felder, von denen ein Block genau eines hat.
const BLOCK_INHALTE: [&str; 4] = ["text", "punkte", "tabelle", "beispiel"];

/// Für die Sätze: die vier Inhalte zum Aussuchen.
const BLOCK_ODER: &str = "text, punkte, tabelle oder beispiel";

/// Ein Satz zu einem Block, der fehlt oder nur zum Teil gezeigt wird.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockHinweis {
    /// Nummer des Blocks ab 1; `None`: das Feld `block` als Ganzes.
    pub block: Option<usize>,
    /// Der Satz ohne „Block n:“ vorn.
    pub satz: String,
}

impl BlockHinweis {
    /// „Block n: …“, wie Seite und Prüfung ihn zeigen.
    pub fn als_satz(&self) -> String {
        match self.block {
            Some(n) => format!("Block {n}: {}", self.satz),
            None => self.satz.clone(),
        }
    }
}

/// Eine Liste einfacher Werte als Texte, sonst `None`.
fn texte(v: &toml::Value) -> Option<Vec<String>> {
    match v {
        toml::Value::Array(a) => a.iter().map(einfach).collect(),
        _ => None,
    }
}

/// `[block.tabelle]`: Zeilen mit falscher Länge fallen weg, je eine mit Satz.
fn block_tabelle(v: &toml::Value, saetze: &mut Vec<String>) -> Option<SystemBlockTabelle> {
    let toml::Value::Table(t) = v else {
        saetze.push("tabelle muss eine Tabelle [block.tabelle] mit spalten und zeilen sein; der Block wird nicht gezeigt.".to_string());
        return None;
    };
    for k in t.keys().filter(|k| !matches!(k.as_str(), "spalten" | "zeilen")) {
        saetze.push(format!("Das Feld „{k}“ kennt eine Tabelle nicht (nur spalten und zeilen); es wird nicht gezeigt."));
    }
    let spalten = match t.get("spalten").and_then(texte) {
        Some(s) if !s.is_empty() => s,
        _ => {
            saetze.push("Die Tabelle braucht spalten = [\"…\", \"…\"] mit mindestens einer Spalte; der Block wird nicht gezeigt.".to_string());
            return None;
        }
    };
    let mut zeilen = Vec::new();
    match t.get("zeilen") {
        None => {
            saetze.push("Die Tabelle hat keine zeilen; der Block wird nicht gezeigt. Schreib zeilen = [[\"…\", \"…\"]] unter spalten.".to_string());
            return None;
        }
        Some(toml::Value::Array(liste)) if liste.is_empty() => {
            saetze.push("Die Tabelle hat keine Zeilen (zeilen = []); der Block wird nicht gezeigt.".to_string());
            return None;
        }
        Some(toml::Value::Array(liste)) => {
            for (i, z) in liste.iter().enumerate() {
                match texte(z) {
                    Some(z) if z.len() == spalten.len() => zeilen.push(z),
                    Some(z) => {
                        saetze.push(format!("Zeile {} der Tabelle hat {} Felder, die Tabelle aber {} Spalten; die Zeile fehlt.", i + 1, z.len(), spalten.len()))
                    }
                    None => saetze.push(format!("Zeile {} der Tabelle ist keine Liste von Texten wie [\"…\", \"…\"]; die Zeile fehlt.", i + 1)),
                }
            }
        }
        Some(_) => {
            saetze.push("zeilen muss eine Liste von Zeilen sein, etwa zeilen = [[\"…\", \"…\"]]; der Block wird nicht gezeigt.".to_string());
            return None;
        }
    }
    // Jede Zeile war falsch (je mit Satz): keine leere Tabelle zeigen.
    if zeilen.is_empty() {
        return None;
    }
    Some(SystemBlockTabelle { spalten, zeilen })
}

/// Der eine Inhalt eines Blocks; passt der Wert nicht, `None` und ein Satz.
fn block_inhalt(feld: &str, v: &toml::Value, saetze: &mut Vec<String>) -> Option<SystemBlockInhalt> {
    match (feld, v) {
        ("text", toml::Value::String(s)) => Some(SystemBlockInhalt::Text(s.clone())),
        ("beispiel", toml::Value::String(s)) => Some(SystemBlockInhalt::Beispiel(s.clone())),
        ("text" | "beispiel", _) => {
            saetze.push(format!("{feld} muss Text in Anführungszeichen sein (lang: \"\"\"…\"\"\"); der Block wird nicht gezeigt."));
            None
        }
        ("punkte", _) => match texte(v) {
            Some(p) => Some(SystemBlockInhalt::Punkte(p)),
            None => {
                saetze.push("punkte muss eine Liste von Texten sein, etwa punkte = [\"…\", \"…\"]; der Block wird nicht gezeigt.".to_string());
                None
            }
        },
        _ => block_tabelle(v, saetze).map(SystemBlockInhalt::Tabelle),
    }
}

/// Liest den Wert von `block` (die `[[block]]` einer Datei). Duldsam: ein
/// Block, der sich nicht zeigen lässt, fehlt und bekommt einen Satz; die
/// übrigen bleiben. Dieselben Sätze meldet `pruefe --system` mit Zeile.
pub fn bloecke_lesen(v: &toml::Value) -> (Vec<SystemBlock>, Vec<BlockHinweis>) {
    let mut bloecke = Vec::new();
    let mut hinweise = Vec::new();
    let toml::Value::Array(liste) = v else {
        hinweise.push(BlockHinweis {
            block: None,
            satz: "Die Blöcke lassen sich nicht lesen: Schreib jeden Block als eigene Tabelle [[block]] (mit zwei eckigen Klammern).".to_string(),
        });
        return (bloecke, hinweise);
    };
    for (i, b) in liste.iter().enumerate() {
        let mut saetze = Vec::new();
        match b {
            toml::Value::Table(t) => {
                let mut titel = None;
                let mut nummeriert = false;
                for (k, w) in t {
                    match (k.as_str(), w) {
                        ("titel", toml::Value::String(s)) => titel = Some(s.clone()),
                        ("titel", _) => saetze.push("titel muss Text in Anführungszeichen sein; die Überschrift fehlt.".to_string()),
                        ("nummeriert", toml::Value::Boolean(b)) if t.contains_key("punkte") => nummeriert = *b,
                        ("nummeriert", toml::Value::Boolean(_)) => saetze.push("nummeriert gilt nur für punkte; hier wird es nicht beachtet.".to_string()),
                        ("nummeriert", _) => {
                            saetze.push("nummeriert muss true oder false sein (ohne Anführungszeichen); die Punkte bleiben ohne Nummern.".to_string())
                        }
                        (k, _) if BLOCK_INHALTE.contains(&k) => {}
                        (k, _) => saetze
                            .push(format!("Das Feld „{k}“ kennt ein Block nicht (nur titel, nummeriert und eines von {BLOCK_ODER}); es wird nicht gezeigt.")),
                    }
                }
                let da: Vec<&str> = BLOCK_INHALTE.iter().copied().filter(|f| t.contains_key(*f)).collect();
                match da.as_slice() {
                    [] => saetze.push(format!("Er hat keinen Inhalt und wird nicht gezeigt. Schreib genau eines von {BLOCK_ODER}.")),
                    [feld] => {
                        if let Some(inhalt) = block_inhalt(feld, &t[*feld], &mut saetze) {
                            bloecke.push(SystemBlock { titel, nummeriert, inhalt });
                        }
                    }
                    mehr => saetze.push(format!(
                        "Er hat mehr als einen Inhalt ({}) und wird nicht gezeigt. Schreib genau eines von {BLOCK_ODER}; für mehr nimm einen weiteren [[block]].",
                        mehr.join(" und ")
                    )),
                }
            }
            _ => saetze.push("Ein Block muss eine Tabelle sein: [[block]] und darunter seine Felder.".to_string()),
        }
        hinweise.extend(saetze.into_iter().map(|satz| BlockHinweis { block: Some(i + 1), satz }));
    }
    (bloecke, hinweise)
}

/// Die Zeilen (ab 1) der `[[block]]`-Köpfe in `text`, in ihrer Reihenfolge:
/// die Zeile zu Block n für `pruefe --system`. Köpfe in mehrzeiligen Texten
/// (`"""…"""`, `'''…'''`, etwa in einem `beispiel`) zählen nicht.
pub fn block_zeilen(text: &str) -> Vec<usize> {
    freie_zeilen(text)
        .into_iter()
        .filter(|(_, zeile)| {
            zeile
                .trim()
                .strip_prefix("[[")
                .and_then(|r| r.split_once("]]"))
                .is_some_and(|(innen, rest)| innen.trim() == "block" && (rest.trim().is_empty() || rest.trim_start().starts_with('#')))
        })
        .map(|(n, _)| n)
        .collect()
}

/// Die Zeilen (ab 1, mit Text), die nicht mitten in einem mehrzeiligen Text
/// beginnen. Ein kleiner Leser für TOML: einzeilige Texte (`"…"` mit `\`,
/// `'…'`) und Kommentare werden übersprungen, damit drei Anführungszeichen
/// darin keinen mehrzeiligen Text öffnen.
fn freie_zeilen(text: &str) -> Vec<(usize, &str)> {
    const BASIS: &[u8] = b"\"\"\"";
    const WOERTLICH: &[u8] = b"'''";
    let mut aus = Vec::new();
    let mut offen: Option<&[u8]> = None;
    for (i, zeile) in text.lines().enumerate() {
        if offen.is_none() {
            aus.push((i + 1, zeile));
        }
        let b = zeile.as_bytes();
        let mut p = 0;
        while p < b.len() {
            match offen {
                Some(z) => {
                    if z == BASIS && b[p] == b'\\' {
                        p += 2;
                    } else if b[p..].starts_with(z) {
                        // Bis zu zwei Anführungszeichen mehr gehören noch zum Text.
                        while p < b.len() && b[p] == z[0] {
                            p += 1;
                        }
                        offen = None;
                    } else {
                        p += 1;
                    }
                }
                None if b[p..].starts_with(BASIS) => {
                    offen = Some(BASIS);
                    p += 3;
                }
                None if b[p..].starts_with(WOERTLICH) => {
                    offen = Some(WOERTLICH);
                    p += 3;
                }
                None => match b[p] {
                    b'"' => {
                        p += 1;
                        while p < b.len() && b[p] != b'"' {
                            p += if b[p] == b'\\' { 2 } else { 1 };
                        }
                        p += 1;
                    }
                    b'\'' => {
                        p += 1;
                        while p < b.len() && b[p] != b'\'' {
                            p += 1;
                        }
                        p += 1;
                    }
                    b'#' => break,
                    _ => p += 1,
                },
            }
        }
    }
    aus
}

/// Zeile (ab 1) von `schluessel = …` oben in der Datei (vor der ersten
/// Tabelle `[…]`), für `pruefe --system`.
pub fn feld_zeile(text: &str, schluessel: &str) -> Option<usize> {
    for (n, zeile) in freie_zeilen(text) {
        let t = zeile.trim_start();
        if t.starts_with('[') {
            return None;
        }
        if t.strip_prefix(schluessel).is_some_and(|r| r.trim_start().starts_with('=')) {
            return Some(n);
        }
    }
    None
}

/// Bekannte Felder eines Abschnitts mit falschem Typ: (Feld, Satz). Die Seite
/// zeigt sie unter den übrigen Feldern; `pruefe --system` meldet sie, damit
/// etwa `reihenfolge = "2"` nicht still ans Ende sortiert.
pub fn abschnitt_feld_hinweise(t: &toml::Table) -> Vec<(String, String)> {
    let mut aus = Vec::new();
    for (k, v) in t {
        let satz = match (k.as_str(), v) {
            ("titel" | "text", toml::Value::String(_)) => None,
            ("titel" | "text", _) => Some(format!("{k} muss Text in Anführungszeichen sein; so steht es nur unter den übrigen Feldern.")),
            ("reihenfolge", toml::Value::Integer(_)) => None,
            ("reihenfolge", _) => {
                Some("reihenfolge muss eine ganze Zahl ohne Anführungszeichen sein (etwa reihenfolge = 2); so steht der Abschnitt am Ende.".to_string())
            }
            ("punkte", _) if texte(v).is_some() => None,
            ("punkte", _) => {
                Some("punkte muss eine Liste von Texten sein, etwa punkte = [\"…\", \"…\"]; so steht sie nur unter den übrigen Feldern.".to_string())
            }
            ("nummeriert", toml::Value::Boolean(_)) => None,
            ("nummeriert", _) => Some("nummeriert muss true oder false sein (ohne Anführungszeichen).".to_string()),
            _ => None,
        };
        if let Some(s) = satz {
            aus.push((k.clone(), s));
        }
    }
    aus
}

/// Ein Abschnitt einer System-Seite aus einer gelesenen Datei.
fn abschnitt(g: Gelesen) -> SystemAbschnitt {
    let mut a = SystemAbschnitt {
        titel: None,
        reihenfolge: None,
        text: None,
        punkte: Vec::new(),
        nummeriert: false,
        bloecke: Vec::new(),
        hinweise: Vec::new(),
        weitere: Vec::new(),
        fehler: None,
        datei: g.name,
    };
    match g.inhalt {
        Err(satz) => a.fehler = Some(satz),
        Ok(t) => {
            for (k, v) in &t {
                let genommen = match (k.as_str(), v) {
                    ("titel", toml::Value::String(s)) => {
                        a.titel = Some(s.clone());
                        true
                    }
                    ("text", toml::Value::String(s)) => {
                        a.text = Some(s.clone());
                        true
                    }
                    ("reihenfolge", toml::Value::Integer(i)) => {
                        a.reihenfolge = Some(*i);
                        true
                    }
                    ("nummeriert", toml::Value::Boolean(b)) => {
                        a.nummeriert = *b;
                        true
                    }
                    ("punkte", toml::Value::Array(liste)) => match liste.iter().map(einfach).collect::<Option<Vec<_>>>() {
                        Some(p) => {
                            a.punkte = p;
                            true
                        }
                        None => false,
                    },
                    ("block", _) => {
                        let (bloecke, hinweise) = bloecke_lesen(v);
                        a.bloecke = bloecke;
                        a.hinweise = hinweise.iter().map(BlockHinweis::als_satz).collect();
                        true
                    }
                    _ => false,
                };
                if !genommen {
                    a.weitere.push(Feld { name: k.clone(), wert: wert(v) });
                }
            }
        }
    }
    a
}

/// Alle Abschnitte des Zielbilds in `ordner`: nach `reihenfolge` (ohne ans
/// Ende), dann nach Dateiname.
pub fn zielbild_lesen(ordner: &Path) -> ZielbildAntwort {
    zielbild_aus(toml_dateien(ordner))
}

/// Das Zielbild aus dem Ergebnis des Lesens (für Tests getrennt).
fn zielbild_aus(r: std::io::Result<Vec<Gelesen>>) -> ZielbildAntwort {
    let (abschnitte, warnung) = abschnitte_aus(Ordner::Zielbild, r);
    ZielbildAntwort { abschnitte, warnung }
}

/// Alle Abschnitte der Anleitung in `ordner` (Etappe 6b), wie beim Zielbild
/// nach `reihenfolge`, dann nach Dateiname.
pub fn anleitung_lesen(ordner: &Path) -> AnleitungAntwort {
    anleitung_aus(toml_dateien(ordner))
}

/// Die Anleitung aus dem Ergebnis des Lesens (für Tests getrennt).
fn anleitung_aus(r: std::io::Result<Vec<Gelesen>>) -> AnleitungAntwort {
    let (abschnitte, warnung) = abschnitte_aus(Ordner::Anleitung, r);
    AnleitungAntwort { abschnitte, warnung }
}

/// Die Abschnitte einer System-Seite, sortiert, und die Warnung zum Ordner.
fn abschnitte_aus(ordner: Ordner, r: std::io::Result<Vec<Gelesen>>) -> (Vec<SystemAbschnitt>, Option<String>) {
    let (dateien, warnung) = liste_und_warnung(ordner, r);
    let mut abschnitte: Vec<SystemAbschnitt> = dateien.into_iter().map(abschnitt).collect();
    abschnitte.sort_by(|a, b| {
        let r = |x: &SystemAbschnitt| x.reihenfolge.map_or((1, 0), |n| (0, n));
        r(a).cmp(&r(b)).then_with(|| a.datei.cmp(&b.datei))
    });
    (abschnitte, warnung)
}

/// Führt `f` im Hintergrund aus; ein Absturz darin wird zu 500 mit Satz.
async fn im_hintergrund<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Result<T, AppError> {
    tokio::task::spawn_blocking(f).await.map_err(|e| {
        tracing::error!("System-Seiten: {e}");
        AppError::intern("Die Liste ließ sich nicht lesen; Einzelheiten stehen im Protokoll.")
    })
}

/// Zweite Wand hinter der Schleuse (`sicherheit::geschuetzt`): ohne Anmeldung
/// 401 mit `WWW-Authenticate: Negotiate`, sonst `lesen` im Hintergrund als JSON.
async fn nur_angemeldet<T: Serialize + Send + 'static>(angemeldet: Option<Extension<Angemeldet>>, lesen: impl FnOnce() -> T + Send + 'static) -> Response {
    if angemeldet.is_none() {
        return super::anmeldung::bitte_anmelden(None);
    }
    match im_hintergrund(lesen).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => e.into_response(),
    }
}

/// `GET /api/entscheidungen` (geschützt, privater Inhalt).
pub async fn entscheidungen(State(state): State<AppState>, angemeldet: Option<Extension<Angemeldet>>) -> Response {
    let ordner = entscheidungen_ordner(&state);
    nur_angemeldet::<EntscheidungsListe>(angemeldet, move || entscheidungen_lesen(&ordner)).await
}

/// `GET /api/zielbild` (geschützt).
pub async fn zielbild(State(state): State<AppState>, angemeldet: Option<Extension<Angemeldet>>) -> Response {
    let ordner = docs_ordner(&state).join("zielbild");
    nur_angemeldet::<ZielbildAntwort>(angemeldet, move || zielbild_lesen(&ordner)).await
}

/// `GET /api/anleitung` (Etappe 6b): `docs/anleitung` im Code-Repo, neben
/// `docs/zielbild`; geschützt wie `/api/zielbild`, mit zweiter Wand.
pub async fn anleitung(State(state): State<AppState>, angemeldet: Option<Extension<Angemeldet>>) -> Response {
    let ordner = docs_ordner(&state).join("anleitung");
    nur_angemeldet::<AnleitungAntwort>(angemeldet, move || anleitung_lesen(&ordner)).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    use crate::app::{AppState, beispiel_fuer_tests, router, router_angemeldet};

    /// Seit der Abnahme 4d geschützt: angemeldet als Inhaber holen.
    async fn hole(pfad: &str) -> (StatusCode, serde_json::Value) {
        let app = router_angemeldet(AppState::fuer_tests_ohne_speicher(&beispiel_fuer_tests(), ""));
        let req = Request::get(pfad).header("host", "127.0.0.1:8787").body(Body::empty()).unwrap();
        let res = app.oneshot(req).await.unwrap();
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        (status, serde_json::from_slice(&bytes).unwrap())
    }

    /// Ohne Windows-Anmeldung: 401 mit `WWW-Authenticate: Negotiate`, kein Inhalt.
    #[tokio::test]
    async fn ohne_anmeldung_401_mit_negotiate() {
        for pfad in ["/api/entscheidungen", "/api/zielbild", "/api/anleitung"] {
            let app = router(AppState::fuer_tests_ohne_speicher(&beispiel_fuer_tests(), ""));
            let req = Request::get(pfad).header("host", "127.0.0.1:8787").body(Body::empty()).unwrap();
            let res = app.oneshot(req).await.unwrap();
            assert_eq!(res.status(), StatusCode::UNAUTHORIZED, "{pfad}");
            assert_eq!(res.headers().get("www-authenticate").and_then(|v| v.to_str().ok()), Some("Negotiate"), "{pfad}");
            let bytes = res.into_body().collect().await.unwrap().to_bytes();
            let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(v["art"], "anmeldung", "{pfad}: {v}");
            assert!(v.get("eintraege").is_none() && v.get("abschnitte").is_none(), "{pfad}: kein Inhalt");
        }
    }

    /// Zweite Wand: auch am Handler selbst, an der Schleuse vorbei, ohne
    /// Anmeldung 401 und kein Inhalt.
    #[tokio::test]
    async fn zweite_wand_im_handler() {
        let state = || AppState::fuer_tests_ohne_speicher(&beispiel_fuer_tests(), "");
        let antworten = [
            ("entscheidungen", entscheidungen(State(state()), None).await),
            ("zielbild", zielbild(State(state()), None).await),
            ("anleitung", anleitung(State(state()), None).await),
        ];
        for (name, res) in antworten {
            assert_eq!(res.status(), StatusCode::UNAUTHORIZED, "{name}");
            assert_eq!(res.headers().get("www-authenticate").and_then(|v| v.to_str().ok()), Some("Negotiate"), "{name}");
            let bytes = res.into_body().collect().await.unwrap().to_bytes();
            let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert!(v.get("eintraege").is_none() && v.get("abschnitte").is_none(), "{name}: kein Inhalt");
        }
    }

    fn eintrag<'a>(v: &'a serde_json::Value, datei: &str) -> &'a serde_json::Value {
        v["eintraege"].as_array().unwrap().iter().find(|e| e["datei"] == datei).unwrap_or_else(|| panic!("{datei} fehlt: {v}"))
    }

    #[tokio::test]
    async fn entscheidungen_neueste_zuerst_mit_kaputter_datei() {
        let (status, v) = hole("/api/entscheidungen").await;
        assert_eq!(status, StatusCode::OK, "{v}");
        assert!(v["warnung"].is_null(), "lesbarer Ordner: keine Warnung: {v}");
        let dateien: Vec<&str> = v["eintraege"].as_array().unwrap().iter().map(|e| e["datei"].as_str().unwrap()).collect();
        // Nach Datum absteigend, bei gleichem Datum nach Name absteigend;
        // ohne `datum` gilt das Datum im Namen; LIESMICH.txt ist kein TOML.
        assert_eq!(
            dateien,
            [
                "2026-10-31-zwei.toml",
                "2026-10-31-eins.toml",
                "2026-10-30-kaputt.toml",
                "2026-10-29-ohne-datum.toml",
                "2026-10-25-alt.toml",
                "ohne-datum-im-namen.toml"
            ]
        );
        // Kaputt: ein Satz mit Zeile, sonst nichts; kein Pfad.
        let k = eintrag(&v, "2026-10-30-kaputt.toml");
        let satz = k["fehler"].as_str().unwrap();
        assert!(satz.contains("Zeile 3"), "{satz}");
        assert!(!satz.contains('\\') && !satz.contains("docs/") && !satz.contains("tests"), "{satz}");
        assert!(k["thema"].is_null() && k["weitere"].as_array().unwrap().is_empty(), "{k}");
        assert_eq!(k["datum"], "2026-10-30");
    }

    #[tokio::test]
    async fn entscheidungen_bekannte_und_weitere_felder() {
        let (_, v) = hole("/api/entscheidungen").await;
        let e = eintrag(&v, "2026-10-31-eins.toml");
        assert_eq!((e["thema"].as_str(), e["status"].as_str(), e["von"].as_str()), (Some("Erste Entscheidung"), Some("entschieden"), Some("Inhaber")));
        // Zahl als Text, fehlende Felder fehlen.
        assert_eq!(e["etappe"], "4");
        assert!(e["urteil"].is_null() && e["was"].is_null() && e["fehler"].is_null(), "{e}");
        let weitere = e["weitere"].as_array().unwrap();
        let namen: Vec<&str> = weitere.iter().map(|f| f["name"].as_str().unwrap()).collect();
        assert_eq!(namen, ["warum", "auflagen", "nachbesserung_1"], "Reihenfolge der Datei");
        assert_eq!(weitere[0]["wert"], serde_json::json!({"art": "text", "inhalt": "Weil es einfacher ist."}));
        assert_eq!(weitere[1]["wert"]["art"], "liste");
        assert_eq!(weitere[1]["wert"]["inhalt"][1], serde_json::json!({"art": "text", "inhalt": "Zweite Auflage"}));
        assert_eq!(weitere[2]["wert"]["art"], "gruppe");
        assert_eq!(weitere[2]["wert"]["inhalt"][0]["name"], "datum");
        assert_eq!(weitere[2]["wert"]["inhalt"][1]["wert"]["inhalt"], "ja", "Wahrheitswert als Text");

        // Ein bekanntes Feld, das eine Liste ist, kommt unter `weitere`.
        let z = eintrag(&v, "2026-10-31-zwei.toml");
        assert!(z["was"].is_null());
        assert_eq!(z["weitere"][0]["name"], "was");
        assert_eq!(z["urteil"], "Ja");

        // Ohne `datum`: aus dem Namen; ohne Datum im Namen: keins, ans Ende.
        assert_eq!(eintrag(&v, "2026-10-29-ohne-datum.toml")["datum"], "2026-10-29");
        assert!(eintrag(&v, "ohne-datum-im-namen.toml")["datum"].is_null());
    }

    #[test]
    fn fehlender_ordner_ist_eine_leere_liste() {
        let nirgends = beispiel_fuer_tests().join("gibt-es-nicht");
        let e = entscheidungen_lesen(&nirgends);
        assert!(e.eintraege.is_empty() && e.warnung.is_none(), "{e:?}");
        let z = zielbild_lesen(&nirgends);
        assert!(z.abschnitte.is_empty() && z.warnung.is_none(), "{z:?}");
        let a = anleitung_lesen(&nirgends);
        assert!(a.abschnitte.is_empty() && a.warnung.is_none(), "{a:?}");
    }

    /// Hotfix 4d: „kein Zugriff“ ist nicht „Ordner fehlt“. Die Liste bleibt
    /// leer, aber mit einem Satz ohne Pfad, der den Befehl nennt.
    #[test]
    fn kein_zugriff_auf_den_ordner_ist_eine_warnung() {
        use std::io::{Error, ErrorKind};
        let e = entscheidungen_aus(Err(Error::new(ErrorKind::PermissionDenied, r"Zugriff verweigert: C:\Users\jemand\dev\dashboard\docs")));
        assert!(e.eintraege.is_empty());
        let satz = e.warnung.expect("Warnung");
        assert!(satz.starts_with("Das Dashboard darf den Ordner mit den Entscheidungen nicht lesen."), "{satz}");
        // Auflage 6d: die Entscheidungen liegen im privaten Inhalt, dort hilft kein Chat.
        assert!(satz.contains("dort hilft kein Chat") && satz.contains(BEFEHL_LESERECHT) && !satz.contains("-Inhalt"), "{satz}");
        assert!(!satz.contains("Users") && !satz.contains("jemand") && !satz.contains("docs"), "kein Pfad: {satz}");

        let z = zielbild_aus(Err(Error::from(ErrorKind::PermissionDenied)));
        assert!(z.abschnitte.is_empty());
        let satz = z.warnung.unwrap();
        assert!(satz.starts_with("Das Dashboard darf den Ordner mit dem Zielbild nicht lesen."), "{satz}");
        assert!(satz.contains("setzt der Betreuer des Dashboards") && !satz.contains("-Inhalt"), "{satz}");
        let a = anleitung_aus(Err(Error::from(ErrorKind::PermissionDenied)));
        assert!(a.abschnitte.is_empty());
        assert!(a.warnung.unwrap().starts_with("Das Dashboard darf den Ordner mit der Anleitung nicht lesen."));
        assert_eq!(
            anleitung_aus(Err(Error::other("kaputt"))).warnung.unwrap(),
            "Der Ordner mit der Anleitung lässt sich gerade nicht lesen. Einzelheiten stehen im Protokoll."
        );

        // Ein anderer Lesefehler: auch eine Warnung, aber ohne Befehl.
        let s = entscheidungen_aus(Err(Error::other("kaputt"))).warnung.unwrap();
        assert_eq!(s, "Der Ordner mit den Entscheidungen lässt sich gerade nicht lesen. Einzelheiten stehen im Protokoll.");
        // Fehlt nur: keine Warnung.
        assert!(entscheidungen_aus(Err(Error::from(ErrorKind::NotFound))).warnung.is_none());
    }

    /// Eine einzelne Datei ohne Leserecht ist ein Eintrag mit Satz, der Rest bleibt.
    #[test]
    fn datei_ohne_leserecht_ist_ein_eintrag_mit_satz() {
        let satz = datei_satz(&std::io::Error::from(std::io::ErrorKind::PermissionDenied));
        assert!(satz.starts_with("Das Dashboard darf diese Datei nicht lesen"), "{satz}");
        assert_eq!(datei_satz(&std::io::Error::other("x")), "Diese Datei lässt sich gerade nicht öffnen.");
        let r = Ok(vec![
            Gelesen { name: "2026-10-31-gesperrt.toml".into(), inhalt: Err(satz.clone()) },
            Gelesen { name: "2026-10-30-gut.toml".into(), inhalt: Ok(toml::from_str("thema = \"Gut\"").unwrap()) },
        ]);
        let l = entscheidungen_aus(r);
        assert!(l.warnung.is_none());
        assert_eq!(l.eintraege.len(), 2);
        assert_eq!(l.eintraege[0].fehler.as_deref(), Some(satz.as_str()));
        assert_eq!(l.eintraege[1].thema.as_deref(), Some("Gut"));
    }

    #[tokio::test]
    async fn zielbild_nach_reihenfolge() {
        let (status, v) = hole("/api/zielbild").await;
        assert_eq!(status, StatusCode::OK, "{v}");
        let a = v["abschnitte"].as_array().unwrap();
        let dateien: Vec<&str> = a.iter().map(|e| e["datei"].as_str().unwrap()).collect();
        assert_eq!(dateien, ["b-grundsatz.toml", "d-bloecke.toml", "a-ohne-reihenfolge.toml", "c-kaputt.toml"]);
        assert_eq!(a[0]["titel"], "Grundsatz");
        assert_eq!(a[0]["punkte"], serde_json::json!(["Erstens", "Zweitens"]));
        assert!(a[0]["text"].as_str().unwrap().contains("\n\n"));
        // Alte Dateien ohne `block`: wie bisher, ohne Blöcke und Hinweise.
        for i in [0, 2] {
            assert_eq!(a[i]["bloecke"], serde_json::json!([]), "{}", a[i]);
            assert_eq!(a[i]["hinweise"], serde_json::json!([]), "{}", a[i]);
            assert_eq!(a[i]["weitere"], serde_json::json!([]), "{}", a[i]);
        }
        assert!(a[3]["fehler"].as_str().unwrap().contains("Zeile"), "{}", a[3]);
    }

    /// Die Blöcke im JSON: alle vier Arten in Reihenfolge, die kaputten als
    /// „Block n: …“ im Abschnitt; `block` steht nicht unter `weitere`.
    #[tokio::test]
    async fn zielbild_mit_bloecken() {
        let (_, v) = hole("/api/zielbild").await;
        let d = v["abschnitte"].as_array().unwrap().iter().find(|a| a["datei"] == "d-bloecke.toml").unwrap();
        assert_eq!(d["text"], "Text vor den Blöcken.");
        assert_eq!(d["punkte"], serde_json::json!(["Punkt vor den Blöcken"]));
        assert_eq!(d["weitere"], serde_json::json!([]), "block ist kein weiteres Feld");
        let b = d["bloecke"].as_array().unwrap();
        let arten: Vec<&str> = b.iter().map(|x| x["inhalt"]["art"].as_str().unwrap()).collect();
        assert_eq!(arten, ["text", "punkte", "tabelle", "beispiel"]);
        assert_eq!(b[0]["titel"], "Absätze");
        assert!(b[1]["titel"].is_null());
        assert_eq!(b[1]["inhalt"]["wert"], serde_json::json!(["Eins", "Zwei"]));
        assert_eq!((b[0]["nummeriert"].as_bool(), b[1]["nummeriert"].as_bool()), (Some(false), Some(true)));
        assert_eq!(d["nummeriert"], false);
        assert_eq!(b[2]["inhalt"]["wert"]["spalten"], serde_json::json!(["Etappe", "Was", "Warum"]));
        assert_eq!(b[2]["inhalt"]["wert"]["zeilen"], serde_json::json!([["1a", "Lesen", "Weil"], ["1c", "Zeigen", "Darum"]]));
        assert!(b[3]["inhalt"]["wert"].as_str().unwrap().contains("[[block]]\n<b>kein HTML</b>"));
        let h: Vec<&str> = d["hinweise"].as_array().unwrap().iter().map(|x| x.as_str().unwrap()).collect();
        assert_eq!(h.len(), 3, "{h:?}");
        assert!(h[0].starts_with("Block 3: Zeile 2 der Tabelle hat 2 Felder, die Tabelle aber 3 Spalten"), "{h:?}");
        assert!(h[1].starts_with("Block 5: Er hat keinen Inhalt"), "{h:?}");
        assert!(h[2].starts_with("Block 6: Er hat mehr als einen Inhalt (text und beispiel)"), "{h:?}");
    }

    /// Etappe 6b: `/api/anleitung` liest `docs/anleitung` neben `docs/zielbild`
    /// (hier im Beispiel), in derselben Form und Reihenfolge wie das Zielbild.
    #[tokio::test]
    async fn anleitung_nach_reihenfolge_mit_bloecken() {
        let (status, v) = hole("/api/anleitung").await;
        assert_eq!(status, StatusCode::OK, "{v}");
        assert!(v["warnung"].is_null(), "{v}");
        let a = v["abschnitte"].as_array().unwrap();
        let dateien: Vec<&str> = a.iter().map(|e| e["datei"].as_str().unwrap()).collect();
        assert_eq!(dateien, ["a-start.toml", "b-schritte.toml", "c-ohne-reihenfolge.toml"]);
        assert_eq!(a[0]["text"], "Erster Abschnitt der Anleitung.");
        let b = a[1]["bloecke"].as_array().unwrap();
        let arten: Vec<&str> = b.iter().map(|x| x["inhalt"]["art"].as_str().unwrap()).collect();
        assert_eq!(arten, ["punkte", "tabelle", "beispiel"]);
        assert_eq!((b[0]["titel"].as_str(), b[0]["nummeriert"].as_bool()), (Some("So geht es"), Some(true)));
        assert_eq!(b[1]["inhalt"]["wert"]["zeilen"], serde_json::json!([["pruefe", "prüft ein Projekt"]]));
        assert_eq!(b[2]["inhalt"]["wert"], "dashboard.exe pruefe --system docs/anleitung");
        assert!(a.iter().all(|x| x["hinweise"] == serde_json::json!([]) && x["fehler"].is_null()), "{v}");
        // Nicht das Zielbild: kein Abschnitt von dort.
        assert!(!v.to_string().contains("b-grundsatz.toml"), "{v}");
    }

    /// Etappe 6b: Die echte Anleitung im Repo (`docs/anleitung`) liest sich
    /// ganz: jede Datei mit Titel, Blöcken und eigener Reihenfolge, ohne
    /// Fehler, ohne Hinweis zu einem Block, ohne übrige Felder und ohne
    /// Markdown-Zeichen (`**`, Backticks).
    #[test]
    fn echte_anleitung_ohne_fehler_und_hinweise() {
        let ordner = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docs").join("anleitung");
        let a = anleitung_lesen(&ordner);
        assert!(a.warnung.is_none(), "{:?}", a.warnung);
        assert!(a.abschnitte.len() >= 10, "{} Abschnitte", a.abschnitte.len());
        let mut reihen = std::collections::HashSet::new();
        for x in &a.abschnitte {
            assert!(x.fehler.is_none() && x.hinweise.is_empty() && x.weitere.is_empty(), "{}: {:?} {:?} {:?}", x.datei, x.fehler, x.hinweise, x.weitere);
            assert!(x.titel.is_some() && !x.bloecke.is_empty(), "{}", x.datei);
            assert!(reihen.insert(x.reihenfolge.expect("reihenfolge")), "{}: reihenfolge doppelt", x.datei);
        }
        let json = serde_json::to_string(&a).unwrap();
        assert!(!json.contains("**") && !json.contains('`'), "kein Markdown in der Anleitung");
        // Nacharbeit 6b: zuerst die acht Schritte, die früher in der Seite standen.
        let erste = &a.abschnitte[0];
        assert_eq!(erste.titel.as_deref(), Some("Die wichtigsten Schritte für einen Chat"));
        let schritte: Vec<&str> = erste.bloecke.iter().filter_map(|b| b.titel.as_deref()).collect();
        assert_eq!(schritte.len(), 8, "{schritte:?}");
        assert!(schritte.iter().enumerate().all(|(i, t)| t.starts_with(&format!("{}. ", i + 1))), "{schritte:?}");
        // Seit 6a nur der neue Stand: keine alte Form mehr.
        for alt in ["page.md", "pulldown-cmark", "segments", "npm run umzug", "dashboard.exe umziehen"] {
            assert!(!json.contains(alt), "die Anleitung nennt noch {alt}");
        }
        // Verweise gehen auf Abschnitte der Anleitung; die .md-Dateien gibt es seit 6d nicht mehr,
        // die Anleitung nennt sie gar nicht (auch nicht „im README“, „web/README“ oder „Contributing“).
        for datei in ["README", "CONTRIBUTING", "Contributing"] {
            assert!(!json.contains(datei), "die Anleitung nennt noch {datei}");
        }
        let klein = json.to_lowercase();
        for datei in ["readme.md", "contributing.md"] {
            assert!(!klein.contains(datei), "die Anleitung verweist noch auf {datei}");
        }
        // Der Teil für den Inhaber steht direkt nach den Schritten und Regeln für Chats.
        assert_eq!(a.abschnitte[2].titel.as_deref(), Some("Dashboard"));
    }

    fn bloecke(toml_text: &str) -> (Vec<SystemBlock>, Vec<String>) {
        let t: toml::Table = toml::from_str(toml_text).unwrap();
        let (b, h) = bloecke_lesen(&t["block"]);
        (b, h.iter().map(BlockHinweis::als_satz).collect())
    }

    #[test]
    fn bloecke_alle_vier_arten() {
        let (b, h) = bloecke(
            "[[block]]\ntext = \"A\"\n[[block]]\ntitel = \"P\"\nnummeriert = true\npunkte = [\"x\", 2, true]\n[[block]]\n[block.tabelle]\nspalten = [\"S\"]\nzeilen = [[\"z\"], [3]]\n[[block]]\nbeispiel = '''\n  eingerückt\n\tTab'''\n",
        );
        assert!(h.is_empty(), "{h:?}");
        assert_eq!(
            b,
            vec![
                SystemBlock { titel: None, nummeriert: false, inhalt: SystemBlockInhalt::Text("A".into()) },
                SystemBlock { titel: Some("P".into()), nummeriert: true, inhalt: SystemBlockInhalt::Punkte(vec!["x".into(), "2".into(), "ja".into()]) },
                SystemBlock {
                    titel: None,
                    nummeriert: false,
                    inhalt: SystemBlockInhalt::Tabelle(SystemBlockTabelle { spalten: vec!["S".into()], zeilen: vec![vec!["z".into()], vec!["3".into()]] })
                },
                SystemBlock { titel: None, nummeriert: false, inhalt: SystemBlockInhalt::Beispiel("  eingerückt\n\tTab".into()) },
            ]
        );
        // Ohne zeilen oder mit zeilen = []: ein Hinweis, keine leere Tabelle.
        let (b, h) = bloecke("[[block]]\ntabelle = { spalten = [\"A\", \"B\"] }\n[[block]]\ntabelle = { spalten = [\"A\"], zeilen = [] }\n");
        assert!(b.is_empty(), "{b:?}");
        assert_eq!(
            h,
            [
                "Block 1: Die Tabelle hat keine zeilen; der Block wird nicht gezeigt. Schreib zeilen = [[\"…\", \"…\"]] unter spalten.",
                "Block 2: Die Tabelle hat keine Zeilen (zeilen = []); der Block wird nicht gezeigt."
            ]
        );
        // nummeriert: nur bei punkte, nur true oder false.
        let (b, h) = bloecke("[[block]]\ntext = \"t\"\nnummeriert = true\n[[block]]\npunkte = [\"a\"]\nnummeriert = \"ja\"\n");
        assert_eq!(b.len(), 2);
        assert!(!b[0].nummeriert && !b[1].nummeriert);
        assert!(h[0].starts_with("Block 1: nummeriert gilt nur für punkte"), "{h:?}");
        assert!(h[1].starts_with("Block 2: nummeriert muss true oder false sein"), "{h:?}");
    }

    /// Abschnitt mit `nummeriert = true`: die Punkte als nummerierte Liste.
    #[test]
    fn abschnitt_nummeriert() {
        let t: toml::Table = toml::from_str("titel = \"A\"\npunkte = [\"x\"]\nnummeriert = true\n").unwrap();
        let a = abschnitt(Gelesen { name: "a.toml".into(), inhalt: Ok(t) });
        assert!(a.nummeriert && a.weitere.is_empty(), "{a:?}");
        let t: toml::Table = toml::from_str("punkte = [\"x\"]\n").unwrap();
        assert!(!abschnitt(Gelesen { name: "b.toml".into(), inhalt: Ok(t) }).nummeriert);
    }

    /// Felder mit falschem Typ: Satz und Zeile für `pruefe --system`.
    #[test]
    fn abschnitt_felder_mit_falschem_typ() {
        let text = "titel = 3\nreihenfolge = \"2\"\ntext = \"gut\"\npunkte = [[\"x\"]]\nnummeriert = \"ja\"\nanderes = 1\n[[block]]\ntext = \"t\"\n";
        let t: toml::Table = toml::from_str(text).unwrap();
        let h = abschnitt_feld_hinweise(&t);
        let felder: Vec<&str> = h.iter().map(|(f, _)| f.as_str()).collect();
        assert_eq!(felder, ["titel", "reihenfolge", "punkte", "nummeriert"]);
        assert!(h[1].1.starts_with("reihenfolge muss eine ganze Zahl"), "{h:?}");
        let zeilen: Vec<Option<usize>> = felder.iter().map(|f| feld_zeile(text, f)).collect();
        assert_eq!(zeilen, [Some(1), Some(2), Some(4), Some(5)]);
        assert_eq!(feld_zeile(text, "gibt-es-nicht"), None);
        // Ein Feld text unter [[block]] ist nicht das des Abschnitts.
        assert_eq!(feld_zeile("[[block]]\ntext = 3\n", "text"), None);
    }

    #[test]
    fn bloecke_hinweise_und_der_rest_bleibt() {
        let (b, h) = bloecke(
            "[[block]]\ntitel = \"leer\"\n[[block]]\ntext = \"a\"\npunkte = [\"b\"]\n[[block]]\ntext = \"bleibt\"\n[[block]]\n[block.tabelle]\nspalten = [\"A\", \"B\"]\nzeilen = [[\"1\"], [\"1\", \"2\"], [\"1\", \"2\", \"3\"]]\n",
        );
        assert_eq!(b.len(), 2, "{b:?}");
        assert_eq!(b[0].inhalt, SystemBlockInhalt::Text("bleibt".into()));
        assert_eq!(
            b[1].inhalt,
            SystemBlockInhalt::Tabelle(SystemBlockTabelle { spalten: vec!["A".into(), "B".into()], zeilen: vec![vec!["1".into(), "2".into()]] })
        );
        assert_eq!(h.len(), 4, "{h:?}");
        assert_eq!(h[0], "Block 1: Er hat keinen Inhalt und wird nicht gezeigt. Schreib genau eines von text, punkte, tabelle oder beispiel.");
        assert!(h[1].starts_with("Block 2: Er hat mehr als einen Inhalt (text und punkte)"), "{h:?}");
        assert_eq!(h[2], "Block 4: Zeile 1 der Tabelle hat 1 Felder, die Tabelle aber 2 Spalten; die Zeile fehlt.");
        assert!(h[3].starts_with("Block 4: Zeile 3 der Tabelle hat 3 Felder"), "{h:?}");

        // Falsche Typen, unbekannte Felder, Tabelle ohne Spalten, block ohne [[ ]].
        let (b, h) = bloecke(
            "[[block]]\ntext = 3\n[[block]]\npunkte = [[\"x\"]]\n[[block]]\nbeispiel = \"b\"\nfarbe = \"rot\"\ntitel = 4\n[[block]]\ntabelle = { zeilen = [] }\n",
        );
        assert_eq!(b, vec![SystemBlock { titel: None, nummeriert: false, inhalt: SystemBlockInhalt::Beispiel("b".into()) }]);
        assert!(h[0].starts_with("Block 1: text muss Text"), "{h:?}");
        assert!(h[1].starts_with("Block 2: punkte muss eine Liste"), "{h:?}");
        assert!(h.iter().any(|s| s.starts_with("Block 3: Das Feld „farbe“ kennt ein Block nicht")), "{h:?}");
        assert!(h.iter().any(|s| s.starts_with("Block 3: titel muss Text")), "{h:?}");
        assert!(h.last().unwrap().starts_with("Block 4: Die Tabelle braucht spalten"), "{h:?}");
        let (b, h) = bloecke("[block]\ntext = \"x\"\n");
        assert!(b.is_empty());
        assert_eq!(h.len(), 1);
        assert!(h[0].starts_with("Die Blöcke lassen sich nicht lesen"), "{h:?}");
    }

    /// Die Zeile zu Block n; ein `[[block]]` in einem mehrzeiligen Text zählt nicht.
    #[test]
    fn block_zeilen_ohne_mehrzeilige_texte() {
        let text = "titel = \"x\"\n\n[[block]]\nbeispiel = \"\"\"\n[[block]]\n\"\"\"\n  [[ block ]]  # zweiter\ntext = '''a'''\n[[block]]\n";
        assert_eq!(block_zeilen(text), vec![3, 7, 9]);
        assert_eq!(block_zeilen("# \"\"\" nur Kommentar\n[[block]]\n[[blocks]]\n[block.tabelle]\n"), vec![2]);
    }

    /// Drei Anführungszeichen in einem einzeiligen Text öffnen keinen
    /// mehrzeiligen Text; die Zeilen danach verrutschen nicht.
    #[test]
    fn block_zeilen_drei_anfuehrungszeichen_in_einzeiligem_text() {
        let text = "titel = 'Er sagte \"\"\" und ging'\ntext = \"Zitat: ''' Ende\"\n[[block]]\ntext = \"a \\\" \\\"\\\"\\\" b\"\n[[block]]\nbeispiel = \"\"\"\nmit \\\"\"\" darin\n[[block]]\n\"\"\"\n[[block]]\n";
        // Prüft zuerst, dass es gültiges TOML mit drei Blöcken ist.
        let t: toml::Table = toml::from_str(text).unwrap();
        assert_eq!(t["block"].as_array().unwrap().len(), 3);
        assert_eq!(block_zeilen(text), vec![3, 5, 10]);
        assert_eq!(feld_zeile(text, "text"), Some(2));
    }

    #[test]
    fn docs_neben_dem_projektordner() {
        let s = AppState::fuer_tests_ohne_speicher(&beispiel_fuer_tests(), "");
        assert_eq!(docs_ordner(&s), beispiel_fuer_tests().join("docs"));
        assert_eq!(entscheidungen_ordner(&s), beispiel_fuer_tests().join("docs").join("entscheidungen"));
    }

    /// Umzug des Inhalts: Die Entscheidungen kommen aus dem privaten Ordner,
    /// das Zielbild weiter aus dem Code-Repo (hier: dem Beispiel).
    #[tokio::test]
    async fn entscheidungen_aus_dem_privaten_ordner() {
        let ordner = std::env::temp_dir().join(format!("dashboard-entscheidungen-privat-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&ordner);
        std::fs::create_dir_all(&ordner).unwrap();
        std::fs::write(ordner.join("2026-11-01-privat.toml"), "datum = \"2026-11-01\"\nthema = \"Nur im Inhalt\"\n").unwrap();
        let mut s = AppState::fuer_tests(&beispiel_fuer_tests());
        s.entscheidungen_dir = std::sync::Arc::new(ordner.clone());
        let app = crate::app::router_angemeldet(s);
        let res = app.clone().oneshot(Request::get("/api/entscheidungen").header("host", "127.0.0.1:8787").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let v: serde_json::Value = serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
        let text = v.to_string();
        assert!(text.contains("2026-11-01-privat.toml") && !text.contains("2026-10-31-eins.toml"), "{text}");
        let res = app.oneshot(Request::get("/api/zielbild").header("host", "127.0.0.1:8787").body(Body::empty()).unwrap()).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
        assert!(v.to_string().contains("b-grundsatz.toml"), "{v}");
        let _ = std::fs::remove_dir_all(&ordner);
    }
}
