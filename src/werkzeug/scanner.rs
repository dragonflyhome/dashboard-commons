//! Der Scanner für die Svelte- und TS-Dateien eines Projekts (Etappe 4v,
//! Teil B). Er geht durch den ganzen Ordner `projects/<slug>/` (ohne
//! `assets/` und `daten/`, die prüft `inhalt::pruefung`; in `server/` nur
//! Verknüpfungen), liest jede `*.svelte` und `*.ts` als Text und meldet
//! Befunde mit Datei, Zeile und Satz:
//!
//! - **Ordner**: jede Verknüpfung (Symlink, Junction) ist ein Fehler, ebenso
//!   Dateien mit Programm oder Stil in anderer Form (`.js`, `.mjs`, `.jsx`,
//!   `.tsx`, `.css`, `.scss`, `.html` …). Der Scanner kann sie nicht lesen,
//!   und vite würde sie mitbauen, sobald etwas sie erreicht.
//! - **Importe**: `$katalog`, `$projekt` und `$vertrag` nur ohne Unterpfad
//!   (dazu genau `$vertrag/projekte/<eigener-slug>`), `svelte` selbst (dazu
//!   `svelte/transition`, `svelte/motion`, `svelte/easing`, `svelte/animate`)
//!   und `./…` im eigenen Projektordner, nur auf `.svelte` und `.ts` (ohne
//!   Endung muss es die `.ts` geben), nicht nach `assets/`, `daten/`,
//!   `server/`, ohne `?` und `#`.
//! - **Verbotene Formen**: `fetch`, `XMLHttpRequest`, `EventSource`,
//!   `WebSocket`, `navigator.sendBeacon`, `{@html}` (und `innerHTML` & Co.),
//!   `eval`, `new Function`, `setTimeout`/`setInterval` mit Text,
//!   `localStorage`, `sessionStorage`, `indexedDB`, `caches`,
//!   `new Worker`/`SharedWorker`, `serviceWorker`, `import.meta` (außer
//!   `import.meta.env`), `document.cookie`, `url(http…)`, `@import`,
//!   `:global`, `<script src=…>`, `<script>` im Markup, `<svelte:head>`,
//!   `<svelte:window>`, `<svelte:body>`, `<svelte:document>`,
//!   `<svelte:element>`, `javascript:`/`data:`-Adressen, Links ohne https://,
//!   Dateien aus dem Netz, eingebettete fremde Seiten.
//! - **Token-Regel**: Farben nur über `var(--…)`, auch nicht als Rückfall.
//! - **Daten-Regel**: eingetippte Termine und Reihen sind
//!   Fehler, jede andere Zahl außer 0, 1 und -1 ein Hinweis. Der `<style>`-Block
//!   zählt dafür nicht; Zahlen in Attributen und im Fließtext sind höchstens
//!   Hinweis; SVG mit Koordinaten (`d=`, `points=`) ist ein Fehler.
//! - **Einmalige Links** (Etappe 5a) in allen `*.toml`.
//!
//! - **Abschalten** (Etappe 3b-2): `@ts-nocheck`, `@ts-ignore`,
//!   `@ts-expect-error`, `svelte-ignore`, `eslint-disable…` und ESLint-Regeln
//!   im Kommentar sind Fehler; Projektseiten schalten keine Prüfung ab.
//! - **Globale Typen** (Zweig B): `declare global`, `declare module`,
//!   `/// <reference …>` (auch im `<script>` von `.svelte`) und `.d.ts`-Dateien
//!   sind Fehler; sonst könnte ein Projekt im gemeinsamen svelte-check die
//!   Typen anderer ändern.
//!
//! Sonst prüft er Kommentare (`//`, `/* */`, `<!-- -->`) nicht.
//!
//! Der Scanner schützt vor Versehen, nicht vor Absicht: Wer es darauf
//! anlegt, kommt an Mustern über Text vorbei (etwa mit
//! `globalThis['fe' + 'tch']`). Die eigentliche Grenze bleibt, dass
//! `/api/schluessel` nie Werte zurückgibt. Er ist reines Rust über Text, ohne
//! Node, und schnell (alle Projekte zusammen wenige Millisekunden).
//!
//! Dazu: [`svelte_check`] (svelte-check über `dienst::programme`, ohne
//! Fenster, mit Zeitlimit) und [`bauliste`]/[`bauliste_schreiben`] für
//! `web/src/lib/projekte.generated.ts`.

use std::collections::{BTreeMap, HashSet};
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use crate::dienst::programme::{self, Werkzeug};
use crate::inhalt::pruefung::{Befund, Schwere};
use crate::inhalt::seite;
use crate::vertrag::projekt::ist_seitenname;

/// Satz der Daten-Regel für eine einzelne Zahl.
pub const SATZ_ZAHL: &str = "Diese Zahl gehört in daten/*.toml.";
/// Satz der Token-Regel.
pub const SATZ_FARBE: &str = "Farben nur über die Design-Token (var(--…)), nicht frei.";
/// Satz für eingebettete Abbildungen mit Koordinaten.
pub const SATZ_ABBILDUNG: &str = "Abbildungen gehören als Datei nach assets/ oder als Daten nach daten/*.toml.";
/// Satz für einmalige Links in TOML.
pub const SATZ_EINMALIG: &str = "Einmalige Links gehören in eine Aufgabe, nicht ins Repo.";

const ERLAUBTE_IMPORTE: &str = "Erlaubt sind nur Dateien im eigenen Projektordner (./…), $katalog, $projekt, $vertrag und svelte.";
const SATZ_DATEN_API: &str = "Daten kommen nur über `ladeDaten` aus '$projekt'.";
const SATZ_HTML: &str = "`{@html …}` ist verboten. Text direkt ins Markup schreiben oder Bausteine aus $katalog nehmen.";
const SATZ_ROH_HTML: &str = "`innerHTML`, `outerHTML`, `insertAdjacentHTML` und `document.write` sind verboten, wie `{@html …}`. Text direkt ins Markup schreiben oder Bausteine aus $katalog nehmen.";
const SATZ_SPEICHER: &str = "Was bleiben soll, gehört in daten/*.toml oder über die API in den Datenspeicher.";
const SATZ_NEW_FUNCTION: &str = "`new Function` ist verboten. Es führt Text als Programm aus.";
const SATZ_COOKIE: &str = "`document.cookie` ist verboten. Die Anmeldung gehört dem Dashboard, nicht der Seite.";
const SATZ_URL_HTTP: &str = "`url(http…)` lädt eine Datei aus dem Netz. Bilder und Schriften gehören nach assets/.";
const SATZ_CSS_IMPORT: &str = "`@import` im CSS ist verboten. Eigenes CSS steht nur im <style> der eigenen Datei.";
const SATZ_SCRIPT_SRC: &str = "`<script src=…>` ist verboten. Programm nur im eigenen <script> der Datei, ohne fremde Dateien.";
const SATZ_SCRIPT_HEAD: &str = "Im `<svelte:head>` ist kein `<script>` erlaubt.";
const SATZ_SCRIPT_MARKUP: &str = "Ein `<script>` im Markup ist verboten. Nur das `<script>` oben in der Datei zählt.";
const SATZ_EINBETTEN: &str = "`<iframe>`, `<object>`, `<embed>` und `<base>` sind verboten. Fremde Seiten werden verlinkt (https://), nicht eingebettet.";
const SATZ_ON_TEXT: &str = "Ereignisse wie `onclick` nur mit {…} und einer Funktion, nicht als Text.";
const SATZ_DATE: &str = "`new Date(…)` mit festen Werten ist ein eingetippter Termin. Termine gehören in daten/*.toml.";
const SATZ_ALIAS: &str = "$katalog und $projekt nur ohne Unterpfad, $vertrag nur ohne Unterpfad oder als $vertrag/projekte/<slug> des eigenen Projekts.";
const SATZ_CSS_DATEI: &str = "CSS-Importe sind verboten. Stil gehört ins <style> der Datei.";
const SATZ_IMPORT_META: &str = "`import.meta` ist verboten (etwa `import.meta.glob` bindet beliebige Dateien ein). Erlaubt ist nur `import.meta.env`.";
const SATZ_GLOBAL: &str =
    "`:global` ist verboten. Stil gilt nur in der eigenen Datei; `:global(nav) { display: none }` bliebe sonst auf allen Seiten des Dashboards aktiv.";
const SATZ_SVELTE_FENSTER: &str =
    "ist verboten. Eine Projektseite hängt nichts an Fenster oder Dokument des Dashboards; Ereignisse gehören an eigene Elemente.";
const SATZ_SVELTE_HEAD: &str = "`<svelte:head>` ist verboten. Titel und Kopf der Seite gehören dem Dashboard.";
const SATZ_SVELTE_ELEMENT: &str = "`<svelte:element>` ist verboten. Elemente stehen fest im Markup, damit der Scanner sie sieht.";
const SATZ_WORKER: &str = "`new Worker`, `SharedWorker` und Service Worker sind verboten. Eine Projektseite startet keine eigenen Hintergrund-Programme.";
const SATZ_LINK_DATEI: &str = "Die Datei ist eine Verknüpfung (Symlink); das Dashboard liest nur echte Dateien.";
/// Zweig B: Typen über das eigene Projekt hinaus. Im gemeinsamen svelte-check
/// könnte ein Projekt sonst die Typen anderer ändern und ihre Fehler verdecken.
pub const SATZ_GLOBALE_TYPEN: &str = "`declare global`, `declare module`, `/// <reference …>` und `.d.ts`-Dateien sind in Projektseiten verboten; sie ändern Typen über das eigene Projekt hinaus. Eigene Typen als `export type` oder `export interface` in eine .ts-Datei schreiben.";

/// Endungen, die im Projektordner (außer `assets/`) ein Fehler sind: Programm
/// oder Stil in einer Form, die der Scanner nicht liest.
const FREMDE_ENDUNGEN: &[&str] = &[
    "js", "mjs", "cjs", "jsx", "tsx", "mts", "cts", "css", "scss", "sass", "less", "styl", "pcss", "postcss", "html", "htm", "xhtml", "vue", "wasm", "svelte",
    "ts",
];
/// Stil-Dateien: ein Import darauf bekommt den Satz [`SATZ_CSS_DATEI`].
const STIL_ENDUNGEN: &[&str] = &["css", "scss", "sass", "less", "styl", "pcss", "postcss"];
/// Diese Ordner oben im Projekt prüft `inhalt::pruefung` (Dateien, Verknüpfungen).
const NICHT_DURCHSUCHT: [&str; 2] = ["assets", "daten"];
/// In diesem Ordner (Etappe 4b) sucht der Scanner nur Verknüpfungen.
const NUR_VERKNUEPFUNGEN: &str = "server";
/// Aus diesen Ordnern oben im Projekt darf keine Seite importieren.
const KEIN_IMPORT_AUS: [&str; 3] = ["assets", "daten", "server"];
/// So tief geht der Scanner in Unterordner.
const TIEFE_HOECHSTENS: usize = 8;

/// Größer liest der Scanner eine Datei nicht (wie bei TOML: 1 MB).
const HOECHSTENS: u64 = seite::TOML_HOECHSTENS;

// ----- Ein Projekt -----

/// Prüft die Svelte-, TS- und TOML-Dateien eines Projekts im
/// Projektordner `dir`. Gibt es das Projekt nicht (oder ist es eine
/// Verknüpfung, das meldet `pruefung`), ist die Liste leer.
pub fn pruefe_projekt(dir: &Path, slug: &str) -> Vec<Befund> {
    let ordner = dir.join(slug);
    if !seite::is_valid_slug(slug) || !seite::ist_echter_ordner(&ordner) || seite::ist_verknuepfung(&ordner) {
        return Vec::new();
    }
    let mut fund = Fund::default();
    durchgehen(&ordner, slug, "", 0, false, &mut fund);
    let mut befunde = fund.befunde;
    let mut dateien = fund.code;
    toml_dateien(&ordner, "", 0, &mut dateien);
    for (rel, pfad) in dateien {
        let datei = format!("projects/{slug}/{rel}");
        match lesen(&pfad, &datei) {
            Ok(text) => befunde.extend(pruefe_datei_in(&datei, &text, Some(&ordner))),
            Err(b) => befunde.push(b),
        }
    }
    befunde
}

/// Hat das Projekt Svelte- oder TS-Dateien (in irgendeinem Unterordner)?
pub fn hat_svelte(dir: &Path, slug: &str) -> bool {
    let mut fund = Fund::default();
    durchgehen(&dir.join(slug), slug, "", 0, false, &mut fund);
    !fund.code.is_empty()
}

/// Die `*.svelte`-Dateien eines Projekts, die der Scanner liest (Pfad relativ
/// zum Projektordner, Pfad auf der Platte); für die Stil-Prüfung
/// (`werkzeug::stil`). Ohne Verknüpfungen, `assets/`, `daten/` und `server/`.
pub fn svelte_dateien(dir: &Path, slug: &str) -> Vec<(String, PathBuf)> {
    let ordner = dir.join(slug);
    if !seite::is_valid_slug(slug) || !seite::ist_echter_ordner(&ordner) || seite::ist_verknuepfung(&ordner) {
        return Vec::new();
    }
    let mut fund = Fund::default();
    durchgehen(&ordner, slug, "", 0, false, &mut fund);
    fund.code.into_iter().filter(|(r, _)| r.ends_with(".svelte")).collect()
}

/// Alle `*.svelte` und `*.ts` eines Projekts (ohne `assets/`, `daten/`,
/// `server/` und Verknüpfungen): Pfad relativ zum Projektordner und auf der
/// Platte. Für den Baumeister (Etappe 4b: Fingerabdruck, grüne Kopie, ESLint).
pub fn code_dateien(dir: &Path, slug: &str) -> Vec<(String, PathBuf)> {
    let ordner = dir.join(slug);
    if !seite::is_valid_slug(slug) || !seite::ist_echter_ordner(&ordner) || seite::ist_verknuepfung(&ordner) {
        return Vec::new();
    }
    let mut fund = Fund::default();
    durchgehen(&ordner, slug, "", 0, false, &mut fund);
    fund.code
}

/// Was der Gang durch den Projektordner findet.
#[derive(Default)]
struct Fund {
    /// `*.svelte` und `*.ts` (Pfad relativ zum Projektordner, Pfad auf der Platte).
    code: Vec<(String, PathBuf)>,
    /// Verknüpfungen, fremde Dateiarten, zu tiefe Ordner.
    befunde: Vec<Befund>,
}

/// Geht rekursiv durch `ordner` (relativ zum Projekt: `rel`, mit `/` am
/// Ende oder leer). Oben lässt es `assets/` und `daten/` aus; in `server/`
/// (und darunter, `nur_links`) meldet es nur Verknüpfungen.
fn durchgehen(ordner: &Path, slug: &str, rel: &str, tiefe: usize, nur_links: bool, fund: &mut Fund) {
    let Ok(eintraege) = std::fs::read_dir(ordner) else { return };
    let mut namen: Vec<(String, PathBuf)> = eintraege.filter_map(|e| e.ok()).filter_map(|e| Some((e.file_name().into_string().ok()?, e.path()))).collect();
    namen.sort();
    for (name, pfad) in namen {
        let klein = name.to_ascii_lowercase();
        if tiefe == 0 && NICHT_DURCHSUCHT.contains(&klein.as_str()) {
            continue;
        }
        let r = format!("{rel}{name}");
        let datei = format!("projects/{slug}/{r}");
        let Ok(meta) = std::fs::symlink_metadata(&pfad) else { continue };
        // `symlink_metadata` folgt keiner Verknüpfung. Bei Ordnern dazu
        // `ist_verknuepfung` (löst auf, kostet etwas; nur je Ordner).
        if meta.file_type().is_symlink() || (meta.is_dir() && seite::ist_verknuepfung(&pfad)) {
            // Eine Junction ist nach `symlink_metadata` kein Ordner; `is_dir` folgt ihr.
            let satz = if meta.is_dir() || pfad.is_dir() { seite::verknuepfung_satz(&r) } else { SATZ_LINK_DATEI.to_string() };
            fund.befunde.push(Befund::fehler(datei, None, satz));
            continue;
        }
        if meta.is_dir() {
            if tiefe + 1 >= TIEFE_HOECHSTENS {
                fund.befunde.push(Befund::fehler(
                    datei,
                    None,
                    format!("Der Ordner liegt zu tief (mehr als {TIEFE_HOECHSTENS} Ebenen); der Scanner prüft darin nichts. Bitte flacher ordnen."),
                ));
                continue;
            }
            let links = nur_links || (tiefe == 0 && klein == NUR_VERKNUEPFUNGEN);
            durchgehen(&pfad, slug, &format!("{r}/"), tiefe + 1, links, fund);
            continue;
        }
        if nur_links {
            continue;
        }
        if name.ends_with(".svelte") || name.ends_with(".ts") {
            // TypeScript wertet auch `*.d.<x>.ts` als Deklarationsdatei.
            if klein.ends_with(".ts") && klein.contains(".d.") {
                fund.befunde.push(Befund::fehler(datei, None, SATZ_GLOBALE_TYPEN));
            }
            fund.code.push((r, pfad));
            continue;
        }
        let Some((stamm, endung)) = klein.rsplit_once('.') else { continue };
        if stamm.is_empty() || !FREMDE_ENDUNGEN.contains(&endung) {
            continue;
        }
        let satz = if endung == "svelte" || endung == "ts" {
            "Bitte die Endung klein schreiben (.svelte oder .ts); so liest der Scanner die Datei nicht.".to_string()
        } else {
            format!(
                "Eine .{endung}-Datei gehört nicht in den Projektordner. Programm steht nur in .svelte- und .ts-Dateien, Stil im <style> der Datei; Dateien zum Ansehen gehören nach assets/."
            )
        };
        fund.befunde.push(Befund::fehler(datei, None, satz));
    }
}

/// Alle `*.toml` unter `ordner` (ohne Verknüpfungen, `target`, `node_modules`, `.git`).
fn toml_dateien(ordner: &Path, rel: &str, tiefe: usize, aus: &mut Vec<(String, PathBuf)>) {
    let Ok(eintraege) = std::fs::read_dir(ordner) else { return };
    let mut namen: Vec<(String, PathBuf)> = eintraege.filter_map(|e| e.ok()).filter_map(|e| Some((e.file_name().into_string().ok()?, e.path()))).collect();
    namen.sort();
    for (name, pfad) in namen {
        let r = format!("{rel}{name}");
        if seite::ist_echter_ordner(&pfad) {
            // `ist_echter_ordner` folgt keiner Verknüpfung (Symlink, Junction).
            if tiefe < 8 && !matches!(name.as_str(), "target" | "node_modules" | ".git") {
                toml_dateien(&pfad, &format!("{r}/"), tiefe + 1, aus);
            }
        } else if name.ends_with(".toml") && std::fs::symlink_metadata(&pfad).is_ok_and(|m| m.is_file()) {
            // Verknüpfte Dateien meldet `durchgehen` (in daten/: `inhalt::pruefung`).
            aus.push((r, pfad));
        }
    }
}

/// Text einer Datei ohne BOM, sonst ein Befund.
fn lesen(pfad: &Path, datei: &str) -> Result<String, Befund> {
    // Nur `symlink_metadata` (schnell); `ist_verknuepfung` löst Pfade auf und
    // kostet je Datei Millisekunden.
    if std::fs::symlink_metadata(pfad).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err(Befund::fehler(datei, None, SATZ_LINK_DATEI));
    }
    if std::fs::metadata(pfad).is_ok_and(|m| m.len() > HOECHSTENS) {
        return Err(Befund::fehler(datei, None, "Die Datei ist größer als 1 MB; so viel gehört nicht in eine Seite."));
    }
    let text = std::fs::read_to_string(pfad).map_err(|_| Befund::fehler(datei, None, "Die Datei lässt sich nicht lesen. Sie muss Text in UTF-8 sein."))?;
    Ok(text.strip_prefix('\u{feff}').map(str::to_string).unwrap_or(text))
}

/// Prüft den Text einer Datei; die Art ergibt sich aus der Endung von
/// `datei` (Pfad relativ zum Repo, etwa `projects/x/seiten/plan.svelte`).
pub fn pruefe_datei(datei: &str, text: &str) -> Vec<Befund> {
    pruefe_datei_in(datei, text, None)
}

/// Wie [`pruefe_datei`]; mit `wurzel` (dem Projektordner auf der Platte)
/// prüft es auch, ob es zu einem Import ohne Endung die `.ts` gibt.
fn pruefe_datei_in(datei: &str, text: &str, wurzel: Option<&Path>) -> Vec<Befund> {
    let mut s = Sammler::neu(datei, text);
    s.wurzel = wurzel;
    if datei.ends_with(".svelte") {
        svelte_datei(&mut s);
        abschaltungen_pruefen(&mut s);
    } else if datei.ends_with(".ts") {
        let mut lx = Lexer::neu(text, 0);
        lx.js(text.len(), false);
        js_regeln(&mut s, &lx.toks);
        abschaltungen_pruefen(&mut s);
    } else if datei.ends_with(".md") || datei.ends_with(".toml") {
        links_pruefen(&mut s);
    }
    s.fertig()
}

/// Sammelt die Befunde einer Datei mit Zeilen.
struct Sammler<'a> {
    datei: &'a str,
    src: &'a str,
    /// Byte-Stellen, an denen eine Zeile beginnt.
    zeilen: Vec<usize>,
    /// Der Slug aus `projects/<slug>/…` (sonst leer).
    slug: String,
    /// Die Ordner der Datei im Projekt (leer: oben, `["seiten"]` in `seiten/`).
    ordner: Vec<String>,
    /// Der Projektordner auf der Platte, wenn bekannt.
    wurzel: Option<&'a Path>,
    befunde: Vec<Befund>,
}

/// Slug und Ordner einer Datei `projects/<slug>/a/b/x.svelte`.
fn slug_und_ordner(datei: &str) -> (String, Vec<String>) {
    let teile: Vec<&str> = datei.split('/').collect();
    if teile.len() >= 3 && teile[0] == "projects" {
        (teile[1].to_string(), teile[2..teile.len() - 1].iter().map(|t| t.to_string()).collect())
    } else {
        (String::new(), Vec::new())
    }
}

impl<'a> Sammler<'a> {
    fn neu(datei: &'a str, src: &'a str) -> Self {
        let zeilen = std::iter::once(0).chain(src.match_indices('\n').map(|(i, _)| i + 1)).collect();
        let (slug, ordner) = slug_und_ordner(datei);
        Self { datei, src, zeilen, slug, ordner, wurzel: None, befunde: Vec::new() }
    }

    fn zeile(&self, pos: usize) -> usize {
        self.zeilen.partition_point(|&s| s <= pos)
    }

    fn fehler(&mut self, pos: usize, text: impl Into<String>) {
        let z = self.zeile(pos);
        self.befunde.push(Befund::fehler(self.datei, Some(z), text));
    }

    fn hinweis(&mut self, pos: usize, text: impl Into<String>) {
        let z = self.zeile(pos);
        self.befunde.push(Befund::hinweis(self.datei, Some(z), text));
    }

    /// Ohne Doppelte (gleiche Zeile, gleicher Satz), nach Zeile sortiert.
    fn fertig(self) -> Vec<Befund> {
        let mut gesehen = HashSet::new();
        let mut b: Vec<Befund> = self.befunde.into_iter().filter(|b| gesehen.insert((b.zeile, b.ist_fehler(), b.text.clone()))).collect();
        b.sort_by_key(|b| b.zeile);
        b
    }
}

// ----- Zerlegen von JS und TS -----

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Art {
    Name,
    Zahl,
    /// Text in '…' oder "…" (mit Anführungszeichen).
    Text,
    /// Ein Textstück einer Vorlage `…` (ohne Backticks und ohne `${}`).
    Vorlage,
    Regex,
    Zeichen(u8),
}

#[derive(Clone, Copy, Debug)]
struct Tok {
    art: Art,
    von: usize,
    bis: usize,
}

/// Wörter, nach denen ein `/` einen regulären Ausdruck beginnt.
const VOR_AUSDRUCK: &[&str] = &["return", "typeof", "case", "do", "else", "in", "of", "new", "delete", "void", "throw", "yield", "await", "instanceof"];

/// Ein einfacher Zerleger für JS/TS: Namen, Zahlen, Texte, Zeichen; ohne
/// Kommentare. Genug für die Regeln, kein Parser.
struct Lexer<'a> {
    s: &'a str,
    b: &'a [u8],
    pos: usize,
    toks: Vec<Tok>,
}

fn ist_namenszeichen(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c == b'$' || c >= 0x80
}

impl<'a> Lexer<'a> {
    fn neu(s: &'a str, von: usize) -> Self {
        Self { s, b: s.as_bytes(), pos: von, toks: Vec::new() }
    }

    fn push(&mut self, art: Art, von: usize) {
        self.toks.push(Tok { art, von, bis: self.pos });
    }

    /// Liest bis `ende` oder (mit `bis_klammer`) bis zur schließenden `}` auf
    /// oberster Ebene; gibt deren Stelle zurück (sonst `ende`).
    fn js(&mut self, ende: usize, bis_klammer: bool) -> usize {
        let mut tiefe = 0usize;
        while self.pos < ende {
            let c = self.b[self.pos];
            let n = if self.pos + 1 < ende { self.b[self.pos + 1] } else { 0 };
            let von = self.pos;
            match c {
                b' ' | b'\t' | b'\r' | b'\n' | 0x0b | 0x0c => self.pos += 1,
                b'/' if n == b'/' => {
                    while self.pos < ende && self.b[self.pos] != b'\n' {
                        self.pos += 1;
                    }
                }
                b'/' if n == b'*' => self.pos = finde(self.b, self.pos + 2, ende, b"*/").map_or(ende, |p| p + 2),
                b'\'' | b'"' => {
                    self.pos += 1;
                    while self.pos < ende {
                        let ch = self.b[self.pos];
                        if ch == b'\\' {
                            self.pos += 2;
                            continue;
                        }
                        self.pos += 1;
                        if ch == c || ch == b'\n' {
                            break;
                        }
                    }
                    self.pos = self.pos.min(ende);
                    self.push(Art::Text, von);
                }
                b'`' => self.vorlage(ende),
                b'0'..=b'9' => self.zahl(ende),
                b'.' if n.is_ascii_digit() => self.zahl(ende),
                b'/' if self.regex_moeglich() => self.regex(ende),
                b'{' => {
                    tiefe += 1;
                    self.pos += 1;
                    self.push(Art::Zeichen(c), von);
                }
                b'}' => {
                    if bis_klammer && tiefe == 0 {
                        return self.pos;
                    }
                    tiefe = tiefe.saturating_sub(1);
                    self.pos += 1;
                    self.push(Art::Zeichen(c), von);
                }
                _ if ist_namenszeichen(c) => {
                    while self.pos < ende && ist_namenszeichen(self.b[self.pos]) {
                        self.pos += 1;
                    }
                    self.push(Art::Name, von);
                }
                _ => {
                    self.pos += 1;
                    self.push(Art::Zeichen(c), von);
                }
            }
        }
        ende
    }

    fn vorlage(&mut self, ende: usize) {
        self.pos += 1;
        let mut teil = self.pos;
        while self.pos < ende {
            match self.b[self.pos] {
                b'\\' => self.pos += 2,
                b'`' => {
                    self.toks.push(Tok { art: Art::Vorlage, von: teil, bis: self.pos });
                    self.pos += 1;
                    return;
                }
                b'$' if self.pos + 1 < ende && self.b[self.pos + 1] == b'{' => {
                    self.toks.push(Tok { art: Art::Vorlage, von: teil, bis: self.pos });
                    self.pos += 2;
                    let zu = self.js(ende, true);
                    self.pos = (zu + 1).min(ende);
                    teil = self.pos;
                }
                _ => self.pos += 1,
            }
        }
        self.pos = ende;
        self.toks.push(Tok { art: Art::Vorlage, von: teil.min(ende), bis: ende });
    }

    fn zahl(&mut self, ende: usize) {
        let von = self.pos;
        while self.pos < ende {
            let ch = self.b[self.pos];
            let n = if self.pos + 1 < ende { self.b[self.pos + 1] } else { 0 };
            if (ch == b'e' || ch == b'E') && (n == b'+' || n == b'-') && !self.s[von..self.pos].starts_with("0x") {
                self.pos += 2;
            } else if ch.is_ascii_alphanumeric() || ch == b'_' || (ch == b'.' && n.is_ascii_digit()) {
                self.pos += 1;
            } else {
                break;
            }
        }
        self.pos = self.pos.min(ende);
        self.push(Art::Zahl, von);
    }

    fn regex_moeglich(&self) -> bool {
        match self.toks.last() {
            None => true,
            Some(t) => match t.art {
                Art::Zeichen(c) => !matches!(c, b')' | b']' | b'}'),
                Art::Name => VOR_AUSDRUCK.contains(&&self.s[t.von..t.bis]),
                _ => false,
            },
        }
    }

    fn regex(&mut self, ende: usize) {
        let von = self.pos;
        self.pos += 1;
        let mut klasse = false;
        while self.pos < ende {
            match self.b[self.pos] {
                b'\\' => self.pos += 1,
                b'\n' => break,
                b'[' => klasse = true,
                b']' => klasse = false,
                b'/' if !klasse => {
                    self.pos += 1;
                    break;
                }
                _ => {}
            }
            self.pos += 1;
        }
        while self.pos < ende && self.b[self.pos].is_ascii_alphanumeric() {
            self.pos += 1;
        }
        self.pos = self.pos.min(ende);
        self.push(Art::Regex, von);
    }
}

/// Erste Stelle von `muster` in `b[von..bis]`.
fn finde(b: &[u8], von: usize, bis: usize, muster: &[u8]) -> Option<usize> {
    if von >= bis {
        return None;
    }
    b[von..bis].windows(muster.len()).position(|w| w == muster).map(|p| p + von)
}

/// Wie `finde`, ohne Unterschied zwischen Groß und Klein.
fn finde_ci(b: &[u8], von: usize, bis: usize, muster: &[u8]) -> Option<usize> {
    if von >= bis {
        return None;
    }
    b[von..bis].windows(muster.len()).position(|w| w.eq_ignore_ascii_case(muster)).map(|p| p + von)
}

/// Inhalt eines Texts: (Byte-Stelle des Inhalts, Inhalt ohne Anführungszeichen).
fn inhalt(src: &str, k: Tok) -> (usize, &str) {
    if k.art == Art::Vorlage {
        return (k.von, &src[k.von..k.bis]);
    }
    let roh = &src[k.von..k.bis];
    let q = roh.as_bytes()[0];
    let ohne = &roh[1..];
    let ohne = if !ohne.is_empty() && ohne.as_bytes()[ohne.len() - 1] == q { &ohne[..ohne.len() - 1] } else { ohne };
    (k.von + 1, ohne)
}

/// Steht `k` für einen Wert (danach ist `[` ein Zugriff, `/` eine Teilung)?
fn ist_wert(src: &str, k: Tok) -> bool {
    match k.art {
        Art::Zahl | Art::Text | Art::Vorlage | Art::Regex => true,
        Art::Name => !VOR_AUSDRUCK.contains(&&src[k.von..k.bis]),
        Art::Zeichen(c) => matches!(c, b')' | b']' | b'}'),
    }
}

// ----- Regeln für JS und TS -----

/// Eine offene Klammer; `literal`: Liste oder Objekt (nicht Aufruf oder Block).
struct Klammer {
    literal: bool,
    von: usize,
    zahlen: Vec<(usize, String)>,
}

/// Verbotene Namen, je mit eigenem Satz.
fn verbotener_name(w: &str) -> Option<String> {
    Some(match w {
        "fetch" => format!("`fetch` ist in Projektseiten verboten. {SATZ_DATEN_API}"),
        "XMLHttpRequest" | "EventSource" | "WebSocket" => format!("`{w}` ist verboten. {SATZ_DATEN_API}"),
        "sendBeacon" => "`navigator.sendBeacon` ist verboten. Eine Projektseite schickt nichts nach außen.".to_string(),
        "eval" => "`eval` ist verboten. Es führt Text als Programm aus.".to_string(),
        "localStorage" | "sessionStorage" | "indexedDB" => format!("`{w}` ist verboten. {SATZ_SPEICHER}"),
        "serviceWorker" => SATZ_WORKER.to_string(),
        "innerHTML" | "outerHTML" | "insertAdjacentHTML" => SATZ_ROH_HTML.to_string(),
        _ => return None,
    })
}

fn js_regeln(s: &mut Sammler, t: &[Tok]) {
    let src = s.src;
    let wort = |i: usize| t.get(i).map_or("", |k| &src[k.von..k.bis]);
    let art = |i: usize| t.get(i).map(|k| k.art);
    let name = |i: usize, w: &str| art(i) == Some(Art::Name) && wort(i) == w;
    let zeichen = |i: usize, c: u8| art(i) == Some(Art::Zeichen(c));
    // Texte, die Adressen eines Imports sind (für sie gilt nur die Import-Regel).
    let mut adressen: HashSet<usize> = HashSet::new();
    let mut stapel: Vec<Klammer> = Vec::new();
    let mut i = 0;
    while i < t.len() {
        let k = t[i];
        let nach_punkt = i > 0 && zeichen(i - 1, b'.');
        match k.art {
            Art::Name => {
                let w = wort(i);
                if let Some(satz) = verbotener_name(w) {
                    s.fehler(k.von, satz);
                }
                match w {
                    "cookie" if nach_punkt && i >= 2 && name(i - 2, "document") => s.fehler(k.von, SATZ_COOKIE),
                    "write" | "writeln" if nach_punkt && i >= 2 && name(i - 2, "document") => s.fehler(k.von, SATZ_ROH_HTML),
                    "Function" if (i > 0 && name(i - 1, "new")) || zeichen(i + 1, b'(') => s.fehler(k.von, SATZ_NEW_FUNCTION),
                    "Worker" | "SharedWorker" if i > 0 && name(i - 1, "new") => s.fehler(k.von, SATZ_WORKER),
                    "declare" if !nach_punkt && (name(i + 1, "global") || name(i + 1, "module")) => s.fehler(k.von, SATZ_GLOBALE_TYPEN),
                    "caches" if zeichen(i + 1, b'.') && ["open", "match", "matchAll", "keys", "has", "delete"].iter().any(|m| name(i + 2, m)) => {
                        s.fehler(k.von, format!("`caches` ist verboten. {SATZ_SPEICHER}"))
                    }
                    "setTimeout" | "setInterval" if zeichen(i + 1, b'(') && matches!(art(i + 2), Some(Art::Text | Art::Vorlage)) => {
                        s.fehler(k.von, format!("`{w}` mit Text als erstem Wert ist verboten. Es führt Text als Programm aus; bitte eine Funktion übergeben."))
                    }
                    "import" if !nach_punkt && zeichen(i + 1, b'.') => {
                        let env = name(i + 2, "meta") && zeichen(i + 3, b'.') && name(i + 4, "env");
                        if !env {
                            s.fehler(k.von, SATZ_IMPORT_META);
                        }
                    }
                    "import" | "require" if !nach_punkt && zeichen(i + 1, b'(') => {
                        if art(i + 2) == Some(Art::Text) && (zeichen(i + 3, b')') || zeichen(i + 3, b',')) {
                            adressen.insert(i + 2);
                            import_pruefen(s, t[i + 2]);
                        } else {
                            s.fehler(
                                k.von,
                                format!(
                                    "`{w}()` nur mit einer festen Adresse in Anführungszeichen, nicht mit Variablen oder Vorlagen-Text. {ERLAUBTE_IMPORTE}"
                                ),
                            );
                        }
                    }
                    "import" | "from" if !nach_punkt && art(i + 1) == Some(Art::Text) => {
                        adressen.insert(i + 1);
                        import_pruefen(s, t[i + 1]);
                    }
                    "Date" => {
                        let auf = if i > 0 && name(i - 1, "new") && zeichen(i + 1, b'(') {
                            Some(i + 1)
                        } else if zeichen(i + 1, b'.') && name(i + 2, "UTC") && zeichen(i + 3, b'(') {
                            Some(i + 3)
                        } else {
                            None
                        };
                        if let Some(auf) = auf {
                            let zu = passende_klammer(t, auf);
                            if t[auf + 1..zu].iter().any(|k| matches!(k.art, Art::Zahl | Art::Text | Art::Vorlage)) {
                                s.fehler(k.von, SATZ_DATE);
                                i = zu + 1;
                                continue;
                            }
                        }
                    }
                    _ => {}
                }
            }
            Art::Zahl => {
                let negativ = i > 0 && zeichen(i - 1, b'-') && !(i >= 2 && ist_wert(src, t[i - 2]));
                if !js_zahl_ausgenommen(wort(i), negativ) {
                    let (pos, text) = if negativ { (t[i - 1].von, format!("-{}", wort(i))) } else { (k.von, wort(i).to_string()) };
                    match stapel.last_mut() {
                        Some(kl) if kl.literal => kl.zahlen.push((pos, text)),
                        _ => zahl_hinweis(s, pos, &text),
                    }
                }
            }
            Art::Text | Art::Vorlage => {
                if !adressen.contains(&i) {
                    let (off, text) = inhalt(src, k);
                    js_text_pruefen(s, off, text);
                }
            }
            Art::Regex => {}
            Art::Zeichen(c) => match c {
                b'[' => stapel.push(Klammer { literal: !(i > 0 && ist_wert(src, t[i - 1])), von: k.von, zahlen: Vec::new() }),
                b'{' => {
                    let objekt = i == 0
                        || matches!(art(i - 1), Some(Art::Zeichen(b'=' | b'(' | b',' | b':' | b'[' | b'?' | b'!' | b'&' | b'|')))
                        || ["return", "yield", "await", "default"].iter().any(|w| name(i - 1, w));
                    stapel.push(Klammer { literal: objekt, von: k.von, zahlen: Vec::new() });
                }
                b'(' => stapel.push(Klammer { literal: false, von: k.von, zahlen: Vec::new() }),
                b']' | b'}' | b')' => {
                    if let Some(kl) = stapel.pop() {
                        klammer_schliessen(s, &mut stapel, kl);
                    }
                }
                _ => {}
            },
        }
        i += 1;
    }
    while let Some(kl) = stapel.pop() {
        klammer_schliessen(s, &mut stapel, kl);
    }
}

/// Stelle der `)` zu `(` an `auf` (oder das letzte Stück).
fn passende_klammer(t: &[Tok], auf: usize) -> usize {
    let mut tiefe = 0usize;
    for (j, k) in t.iter().enumerate().skip(auf) {
        match k.art {
            Art::Zeichen(b'(') => tiefe += 1,
            Art::Zeichen(b')') => {
                tiefe -= 1;
                if tiefe == 0 {
                    return j;
                }
            }
            _ => {}
        }
    }
    t.len() - 1
}

/// Eine Liste oder ein Objekt ist zu: in der äußeren Liste weiterzählen,
/// sonst ab drei Zahlen ein Fehler, darunter je Zahl ein Hinweis.
fn klammer_schliessen(s: &mut Sammler, stapel: &mut [Klammer], kl: Klammer) {
    if !kl.literal {
        return;
    }
    if let Some(eltern) = stapel.last_mut().filter(|e| e.literal) {
        eltern.zahlen.extend(kl.zahlen);
        return;
    }
    if kl.zahlen.len() >= 3 {
        s.fehler(kl.von, format!("Eine Reihe mit {} eingetippten Zahlen. Zahlen und Reihen gehören in daten/*.toml.", kl.zahlen.len()));
    } else {
        for (pos, text) in kl.zahlen {
            zahl_hinweis(s, pos, &text);
        }
    }
}

fn zahl_hinweis(s: &mut Sammler, pos: usize, text: &str) {
    s.hinweis(pos, format!("Eingetippte Zahl {text}. {SATZ_ZAHL}"));
}

/// 0, 1 und -1 sind erlaubt (Rechnen, Schleifen).
fn js_zahl_ausgenommen(w: &str, negativ: bool) -> bool {
    let w = w.replace('_', "");
    let w = w.trim_end_matches('n');
    let wert = if let Some(h) = w.strip_prefix("0x").or_else(|| w.strip_prefix("0X")) {
        u64::from_str_radix(h, 16).ok().map(|v| v as f64)
    } else if let Some(o) = w.strip_prefix("0o").or_else(|| w.strip_prefix("0O")) {
        u64::from_str_radix(o, 8).ok().map(|v| v as f64)
    } else if let Some(b) = w.strip_prefix("0b").or_else(|| w.strip_prefix("0B")) {
        u64::from_str_radix(b, 2).ok().map(|v| v as f64)
    } else {
        w.parse::<f64>().ok()
    };
    match wert {
        Some(v) => {
            let v = if negativ { -v } else { v };
            v == 0.0 || v == 1.0 || v == -1.0
        }
        None => false,
    }
}

/// Texte im Programm: Termine, freie Farben, `javascript:`-Adressen.
fn js_text_pruefen(s: &mut Sammler, off: usize, text: &str) {
    for (o, l) in termine_im_text(text) {
        s.fehler(off + o, format!("Eingetipptes Datum „{}“. Termine gehören in daten/*.toml.", &text[o..o + l]));
    }
    let ganz = text.trim();
    let hex = hex_farbe(text).filter(|(o, h)| ganz == *h || text[..*o].contains(':'));
    if let Some((o, f)) = hex.map(|(o, h)| (o, h.to_string())).or_else(|| farbfunktion(text, false)) {
        s.fehler(off + o, format!("Freie Farbe „{f}“. {SATZ_FARBE}"));
    }
    if gefaehrliche_adresse(text) {
        s.fehler(off, format!("Die Adresse „{}“ ist verboten. Links nur mit https:// oder im Dashboard (/…).", kurz(text)));
    }
}

fn kurz(t: &str) -> String {
    let t = t.trim();
    if t.chars().count() > 40 { format!("{}…", t.chars().take(40).collect::<String>()) } else { t.to_string() }
}

/// `javascript:`, `vbscript:` oder `data:` am Anfang (ohne Leerraum, groß/klein egal).
fn gefaehrliche_adresse(t: &str) -> bool {
    let k: String = t.chars().filter(|c| !c.is_whitespace() && !c.is_control()).take(12).collect::<String>().to_ascii_lowercase();
    k.starts_with("javascript:") || k.starts_with("vbscript:") || k.starts_with("data:")
}

/// Prüft die Adresse eines Imports (nur der Satz, wenn sie nicht erlaubt ist).
fn import_pruefen(s: &mut Sammler, k: Tok) {
    let (_, adresse) = inhalt(s.src, k);
    match import_art(adresse, &s.slug, &s.ordner) {
        Import::Erlaubt => {}
        Import::Verboten(satz) => s.fehler(k.von, satz),
        Import::OhneEndung(teile) => {
            let Some(wurzel) = s.wurzel else { return };
            let mut pfad = wurzel.to_path_buf();
            for t in &teile {
                pfad.push(t);
            }
            let name = format!("{}.ts", teile.last().map_or("", String::as_str));
            pfad.set_file_name(&name);
            if !std::fs::symlink_metadata(&pfad).is_ok_and(|m| m.is_file()) {
                s.fehler(
                    k.von,
                    format!(
                        "Der Import „{adresse}“ hat keine Endung; dann muss es die Datei {name} geben. Bitte die Datei mit Endung nennen (.svelte oder .ts)."
                    ),
                );
            }
        }
    }
}

/// Was eine Import-Adresse ist.
enum Import {
    Erlaubt,
    /// Relativ ohne Endung: erlaubt, wenn es `<teile>.ts` gibt (Teile relativ zum Projektordner).
    OhneEndung(Vec<String>),
    Verboten(String),
}

/// Satz, wenn der Import `adresse` aus der Datei `datei` (relativ zum Repo,
/// etwa `projects/x/seiten/plan.svelte`) nicht erlaubt ist. Ob es zu einem
/// Import ohne Endung die `.ts` gibt, prüft erst [`pruefe_projekt`].
pub fn import_mangel(adresse: &str, datei: &str) -> Option<String> {
    let (slug, ordner) = slug_und_ordner(datei);
    match import_art(adresse, &slug, &ordner) {
        Import::Verboten(satz) => Some(satz),
        Import::Erlaubt | Import::OhneEndung(_) => None,
    }
}

fn import_art(adresse: &str, slug: &str, ordner: &[String]) -> Import {
    const ALIASE: [&str; 3] = ["$katalog", "$projekt", "$vertrag"];
    const SVELTE: [&str; 5] = ["svelte", "svelte/transition", "svelte/motion", "svelte/easing", "svelte/animate"];
    if ALIASE.contains(&adresse) || SVELTE.contains(&adresse) {
        return Import::Erlaubt;
    }
    // Für Etappe 4b: die Typen des eigenen Projekts, genau dieser Pfad.
    if !slug.is_empty() && seite::is_valid_slug(slug) && adresse.strip_prefix("$vertrag/projekte/") == Some(slug) {
        return Import::Erlaubt;
    }
    if ALIASE.iter().any(|a| adresse.strip_prefix(a).is_some_and(|r| r.starts_with('/'))) {
        return Import::Verboten(format!("Der Import „{adresse}“ ist nicht erlaubt. {SATZ_ALIAS}"));
    }
    if !(adresse.starts_with("./") || adresse.starts_with("../")) {
        return Import::Verboten(format!("Der Import „{adresse}“ ist nicht erlaubt. {ERLAUBTE_IMPORTE}"));
    }
    if adresse.contains(['\\', '?', '#']) {
        return Import::Verboten(format!("Der Import „{adresse}“ ist nicht erlaubt: Adressen im Projektordner ohne `?`, `#` und `\\`. {ERLAUBTE_IMPORTE}"));
    }
    let teile: Vec<&str> = adresse.split('/').collect();
    let (letzt, davor) = teile.split_last().unwrap_or((&"", &[]));
    let mut stapel: Vec<String> = ordner.to_vec();
    for t in davor {
        match *t {
            "." | "" => {}
            ".." => {
                if stapel.pop().is_none() {
                    return Import::Verboten(format!(
                        "Der Import „{adresse}“ führt aus dem eigenen Projektordner hinaus (etwa in ein anderes Projekt oder nach web/src). {ERLAUBTE_IMPORTE}"
                    ));
                }
            }
            t => stapel.push(t.to_string()),
        }
    }
    if matches!(*letzt, "" | "." | "..") {
        return Import::Verboten(format!("Der Import „{adresse}“ zeigt auf einen Ordner. Bitte die Datei mit Endung nennen (.svelte oder .ts)."));
    }
    if let Some(erster) = stapel.first().map(|e| e.to_ascii_lowercase()).filter(|e| KEIN_IMPORT_AUS.contains(&e.as_str())) {
        return Import::Verboten(format!(
            "Der Import „{adresse}“ zeigt nach {erster}/. Importe nur aus Seiten-Dateien, nicht aus assets/, daten/ oder server/."
        ));
    }
    match letzt.rsplit_once('.').map(|(_, e)| e) {
        None => {
            stapel.push(letzt.to_string());
            Import::OhneEndung(stapel)
        }
        Some("svelte" | "ts") => Import::Erlaubt,
        Some(e) if STIL_ENDUNGEN.contains(&e.to_ascii_lowercase().as_str()) => Import::Verboten(SATZ_CSS_DATEI.to_string()),
        Some(e) => {
            Import::Verboten(format!("Der Import „{adresse}“ zeigt auf eine .{e}-Datei. Aus dem Projektordner nur .svelte- und .ts-Dateien importieren."))
        }
    }
}

// ----- Farben -----

/// Benannte CSS-Farben (ohne transparent und currentColor).
const BENANNTE_FARBEN: &[&str] = &[
    "aliceblue",
    "antiquewhite",
    "aqua",
    "aquamarine",
    "azure",
    "beige",
    "bisque",
    "black",
    "blanchedalmond",
    "blue",
    "blueviolet",
    "brown",
    "burlywood",
    "cadetblue",
    "chartreuse",
    "chocolate",
    "coral",
    "cornflowerblue",
    "cornsilk",
    "crimson",
    "cyan",
    "darkblue",
    "darkcyan",
    "darkgoldenrod",
    "darkgray",
    "darkgreen",
    "darkgrey",
    "darkkhaki",
    "darkmagenta",
    "darkolivegreen",
    "darkorange",
    "darkorchid",
    "darkred",
    "darksalmon",
    "darkseagreen",
    "darkslateblue",
    "darkslategray",
    "darkslategrey",
    "darkturquoise",
    "darkviolet",
    "deeppink",
    "deepskyblue",
    "dimgray",
    "dimgrey",
    "dodgerblue",
    "firebrick",
    "floralwhite",
    "forestgreen",
    "fuchsia",
    "gainsboro",
    "ghostwhite",
    "gold",
    "goldenrod",
    "gray",
    "green",
    "greenyellow",
    "grey",
    "honeydew",
    "hotpink",
    "indianred",
    "indigo",
    "ivory",
    "khaki",
    "lavender",
    "lavenderblush",
    "lawngreen",
    "lemonchiffon",
    "lightblue",
    "lightcoral",
    "lightcyan",
    "lightgoldenrodyellow",
    "lightgray",
    "lightgreen",
    "lightgrey",
    "lightpink",
    "lightsalmon",
    "lightseagreen",
    "lightskyblue",
    "lightslategray",
    "lightslategrey",
    "lightsteelblue",
    "lightyellow",
    "lime",
    "limegreen",
    "linen",
    "magenta",
    "maroon",
    "mediumaquamarine",
    "mediumblue",
    "mediumorchid",
    "mediumpurple",
    "mediumseagreen",
    "mediumslateblue",
    "mediumspringgreen",
    "mediumturquoise",
    "mediumvioletred",
    "midnightblue",
    "mintcream",
    "mistyrose",
    "moccasin",
    "navajowhite",
    "navy",
    "oldlace",
    "olive",
    "olivedrab",
    "orange",
    "orangered",
    "orchid",
    "palegoldenrod",
    "palegreen",
    "paleturquoise",
    "palevioletred",
    "papayawhip",
    "peachpuff",
    "peru",
    "pink",
    "plum",
    "powderblue",
    "purple",
    "rebeccapurple",
    "red",
    "rosybrown",
    "royalblue",
    "saddlebrown",
    "salmon",
    "sandybrown",
    "seagreen",
    "seashell",
    "sienna",
    "silver",
    "skyblue",
    "slateblue",
    "slategray",
    "slategrey",
    "snow",
    "springgreen",
    "steelblue",
    "tan",
    "teal",
    "thistle",
    "tomato",
    "turquoise",
    "violet",
    "wheat",
    "white",
    "whitesmoke",
    "yellow",
    "yellowgreen",
];

fn wortzeichen(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '-'
}

/// Erste Hex-Farbe (#rgb, #rgba, #rrggbb, #rrggbbaa) als ganzes Stück.
fn hex_farbe(text: &str) -> Option<(usize, &str)> {
    for (i, _) in text.match_indices('#') {
        if text[..i].chars().next_back().is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '&') {
            continue;
        }
        let rest = &text[i + 1..];
        let n = rest.bytes().take_while(u8::is_ascii_hexdigit).count();
        if matches!(n, 3 | 4 | 6 | 8) && !rest[n..].chars().next().is_some_and(wortzeichen) {
            return Some((i, &text[i..i + 1 + n]));
        }
    }
    None
}

/// Erste Farbfunktion wie `rgb(` oder `oklch(`; `color(` nur mit `mit_color`.
fn farbfunktion(text: &str, mit_color: bool) -> Option<(usize, String)> {
    const NAMEN: [&str; 10] = ["rgb", "rgba", "hsl", "hsla", "hwb", "lab", "lch", "oklab", "oklch", "color"];
    let klein = text.to_ascii_lowercase();
    for (p, _) in klein.match_indices('(') {
        let vor = &klein[..p];
        let start = vor.rfind(|c: char| !(c.is_ascii_alphabetic() || c == '-')).map_or(0, |i| i + 1);
        let w = &vor[start..];
        if NAMEN.contains(&w) && (mit_color || w != "color") && !vor[..start].chars().next_back().is_some_and(wortzeichen) {
            return Some((start, format!("{w}(")));
        }
    }
    None
}

/// Erste benannte Farbe als ganzes Wort, außerhalb von Anführungszeichen.
fn benannte_farbe(text: &str) -> Option<(usize, &str)> {
    let mut in_text: Option<char> = None;
    let mut wort_von: Option<usize> = None;
    let pruefen = |von: usize, bis: usize| -> Option<(usize, usize)> {
        let w = &text[von..bis];
        let davor = text[..von].chars().next_back();
        let danach = text[bis..].chars().next();
        let grenze = !davor.is_some_and(|c| wortzeichen(c) || c == '#' || c == '.') && !danach.is_some_and(|c| wortzeichen(c) || c == '(');
        (grenze && BENANNTE_FARBEN.contains(&w.to_ascii_lowercase().as_str())).then_some((von, bis))
    };
    for (i, c) in text.char_indices() {
        if let Some(q) = in_text {
            if c == q {
                in_text = None;
            }
            continue;
        }
        if c.is_ascii_alphabetic() {
            wort_von.get_or_insert(i);
            continue;
        }
        if let Some(v) = wort_von.take()
            && let Some((a, b)) = pruefen(v, i)
        {
            return Some((a, &text[a..b]));
        }
        if c == '"' || c == '\'' {
            in_text = Some(c);
        }
    }
    if let Some(v) = wort_von
        && let Some((a, b)) = pruefen(v, text.len())
    {
        return Some((a, &text[a..b]));
    }
    None
}

/// Ein CSS-Wert (nach `prop:`): Hex, Farbfunktionen, benannte Farben.
fn css_wert_pruefen(s: &mut Sammler, off: usize, wert: &str) {
    let fund =
        hex_farbe(wert).map(|(o, h)| (o, h.to_string())).or_else(|| farbfunktion(wert, true)).or_else(|| benannte_farbe(wert).map(|(o, w)| (o, w.to_string())));
    if let Some((o, f)) = fund {
        s.fehler(off + o, format!("Freie Farbe „{f}“. {SATZ_FARBE}"));
    }
}

/// CSS in `src[von..bis]` (ein `<style>`-Block oder ein `style="…"`):
/// `@import`, `url(http…)`, freie Farben in Werten. Zahlen zählen hier nicht.
fn css_pruefen(s: &mut Sammler, von: usize, bis: usize) {
    // Kommentare durch Leerzeichen ersetzen, Stellen bleiben gleich.
    let mut roh = s.src.as_bytes()[von..bis].to_vec();
    let mut i = 0;
    while i + 1 < roh.len() {
        if roh[i] == b'/' && roh[i + 1] == b'*' {
            let ende = finde(&roh, i + 2, roh.len(), b"*/").map_or(roh.len(), |p| p + 2);
            for b in &mut roh[i..ende] {
                if *b != b'\n' {
                    *b = b' ';
                }
            }
            i = ende;
        } else {
            i += 1;
        }
    }
    let Ok(css) = String::from_utf8(roh) else { return };
    let klein = css.to_ascii_lowercase();
    for (p, _) in klein.match_indices("@import") {
        s.fehler(von + p, SATZ_CSS_IMPORT);
    }
    url_und_global(s, von, &css);
    let mut teil = 0;
    for (p, c) in css.char_indices() {
        if matches!(c, ';' | '{' | '}') {
            if c != '{' {
                css_deklaration(s, von + teil, &css[teil..p]);
            }
            teil = p + 1;
        }
    }
    css_deklaration(s, von + teil, &css[teil..]);
}

/// `url(http…)` und `:global` (auch `-global-` vor Keyframes) in CSS-Text ab `off`.
fn url_und_global(s: &mut Sammler, off: usize, css: &str) {
    let klein = css.to_ascii_lowercase();
    for (p, _) in klein.match_indices("url(") {
        let rest = klein[p + 4..].trim_start_matches(|c: char| c.is_whitespace() || c == '"' || c == '\'');
        if rest.starts_with("http:") || rest.starts_with("https:") || rest.starts_with("//") {
            s.fehler(off + p, SATZ_URL_HTTP);
        }
    }
    for muster in [":global", "-global-"] {
        for (p, _) in klein.match_indices(muster) {
            s.fehler(off + p, SATZ_GLOBAL);
        }
    }
}

fn css_deklaration(s: &mut Sammler, off: usize, dekl: &str) {
    if let Some(dp) = dekl.find(':') {
        css_wert_pruefen(s, off + dp + 1, &dekl[dp + 1..]);
    }
}

// ----- Termine und Zahlen in Text -----

fn zahl_bei(b: &[u8], i: usize, n: usize) -> Option<u32> {
    if i + n > b.len() || !b[i..i + n].iter().all(u8::is_ascii_digit) {
        return None;
    }
    std::str::from_utf8(&b[i..i + n]).ok()?.parse().ok()
}

/// Termine im Text: `2026-11-02`, `2026-11`, `2.11.2026` (Stelle, Länge).
pub(crate) fn termine_im_text(text: &str) -> Vec<(usize, usize)> {
    let b = text.as_bytes();
    let ziffer = |i: usize| b.get(i).is_some_and(u8::is_ascii_digit);
    let mut aus = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if !ziffer(i) || (i > 0 && ziffer(i - 1)) {
            i += 1;
            continue;
        }
        let lauf = (i..b.len()).take_while(|&j| ziffer(j)).count();
        // JJJJ-MM(-TT)
        if lauf == 4 && b.get(i + 4) == Some(&b'-') {
            let jahr = zahl_bei(b, i, 4).unwrap_or(0);
            if let Some(monat) = zahl_bei(b, i + 5, 2).filter(|_| !ziffer(i + 7))
                && (1900..=2199).contains(&jahr)
                && (1..=12).contains(&monat)
            {
                let mut l = 7;
                if b.get(i + 7) == Some(&b'-') && zahl_bei(b, i + 8, 2).is_some_and(|t| (1..=31).contains(&t)) && !ziffer(i + 10) {
                    l = 10;
                }
                aus.push((i, l));
                i += l;
                continue;
            }
        }
        // T.M.JJJJ
        if (lauf == 1 || lauf == 2) && b.get(i + lauf) == Some(&b'.') {
            let tag = zahl_bei(b, i, lauf).unwrap_or(0);
            let m = i + lauf + 1;
            let ml = (m..b.len()).take_while(|&j| ziffer(j)).count();
            if (ml == 1 || ml == 2) && b.get(m + ml) == Some(&b'.') {
                let monat = zahl_bei(b, m, ml).unwrap_or(0);
                let j = m + ml + 1;
                let jahr = zahl_bei(b, j, 4).filter(|_| !ziffer(j + 4));
                if let Some(jahr) = jahr
                    && (1..=31).contains(&tag)
                    && (1..=12).contains(&monat)
                    && (1900..=2199).contains(&jahr)
                {
                    aus.push((i, j + 4 - i));
                    i = j + 4;
                    continue;
                }
            }
        }
        i += lauf;
    }
    aus
}

/// Zahlen im Fließtext, die für sich stehen (nicht in Wörtern wie `4v`,
/// `col-2` oder `&#123;`), außer 0, 1 und -1 (Stelle, Zahl).
fn zahlen_im_text(text: &str) -> Vec<(usize, String)> {
    let b = text.as_bytes();
    let termine = termine_im_text(text);
    let mut aus = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if !b[i].is_ascii_digit() {
            i += 1;
            continue;
        }
        let von = i;
        while i < b.len() && (b[i].is_ascii_digit() || ((b[i] == b'.' || b[i] == b',') && b.get(i + 1).is_some_and(u8::is_ascii_digit))) {
            i += 1;
        }
        if termine.iter().any(|&(o, l)| von >= o && von < o + l) {
            continue;
        }
        let davor = text[..von].chars().next_back();
        let danach = text[i..].chars().next();
        let mut start = von;
        let im_wort = match davor {
            Some('-') => {
                let vorvor = text[..von - 1].chars().next_back();
                if vorvor.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                    true
                } else {
                    start = von - 1;
                    false
                }
            }
            Some(c) => c.is_alphanumeric() || matches!(c, '_' | '#' | '.' | '$'),
            None => false,
        } || danach.is_some_and(|c| c.is_alphabetic() || c == '_');
        if im_wort {
            continue;
        }
        let z = &text[start..i];
        if !matches!(z, "0" | "1" | "-1" | "-0") {
            aus.push((start, z.to_string()));
        }
    }
    aus
}

// ----- Svelte -----

const LEERE_ELEMENTE: &[&str] = &["area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source", "track", "wbr"];
const ADRESS_ATTRIBUTE: &[&str] =
    &["href", "src", "srcset", "action", "formaction", "poster", "data", "background", "xlink:href", "ping", "cite", "manifest", "codebase", "longdesc"];
const FARB_ATTRIBUTE: &[&str] = &["fill", "stroke", "color", "stop-color", "flood-color", "lighting-color", "bgcolor"];
const EINBETTEN: &[&str] = &["iframe", "frame", "frameset", "object", "embed", "applet", "base", "portal"];

/// Name eines Elements ab `von` (Buchstaben, Ziffern, `:`, `-`, `.`, `_`).
fn element_name(src: &str, von: usize) -> &str {
    let b = src.as_bytes();
    let mut i = von;
    while i < b.len() && (b[i].is_ascii_alphanumeric() || matches!(b[i], b':' | b'-' | b'.' | b'_')) {
        i += 1;
    }
    &src[von..i]
}

/// Ein `{…}` ab `von` (nach der `{`) als JS prüfen; Stelle nach der `}`.
fn ausdruck(s: &mut Sammler, von: usize) -> usize {
    let n = s.src.len();
    let mut lx = Lexer::neu(s.src, von);
    let zu = lx.js(n, true);
    js_regeln(s, &lx.toks);
    (zu + 1).min(n)
}

fn svelte_datei(s: &mut Sammler) {
    let src = s.src;
    let b = src.as_bytes();
    let n = b.len();
    let mut i = 0;
    let mut text_von = 0;
    let mut offen: Vec<String> = Vec::new();
    while i < n {
        if b[i] == b'<' {
            if b[i..].starts_with(b"<!--") {
                markup_text(s, text_von, i);
                i = finde(b, i + 4, n, b"-->").map_or(n, |p| p + 3);
                text_von = i;
                continue;
            }
            let nb = b.get(i + 1).copied().unwrap_or(0);
            if nb == b'/' && b.get(i + 2).is_some_and(u8::is_ascii_alphabetic) {
                markup_text(s, text_von, i);
                let name = element_name(src, i + 2).to_ascii_lowercase();
                if let Some(p) = offen.iter().rposition(|o| *o == name) {
                    offen.truncate(p);
                }
                i = finde(b, i, n, b">").map_or(n, |p| p + 1);
                text_von = i;
                continue;
            }
            if nb.is_ascii_alphabetic() {
                markup_text(s, text_von, i);
                let start = i;
                let tag = start_tag(s, i);
                i = tag.ende;
                match tag.name.as_str() {
                    "script" => {
                        if tag.attribute.iter().any(|a| a == "src") {
                            s.fehler(start, SATZ_SCRIPT_SRC);
                        }
                        if offen.iter().any(|o| o == "svelte:head") {
                            s.fehler(start, SATZ_SCRIPT_HEAD);
                        } else if !offen.is_empty() {
                            s.fehler(start, SATZ_SCRIPT_MARKUP);
                        }
                        if !tag.selbst {
                            let zu = finde_ci(b, i, n, b"</script").unwrap_or(n);
                            let mut lx = Lexer::neu(src, i);
                            lx.js(zu, false);
                            js_regeln(s, &lx.toks);
                            i = finde(b, zu, n, b">").map_or(n, |p| p + 1);
                        }
                    }
                    "style" => {
                        if tag.attribute.iter().any(|a| a == "global") {
                            s.fehler(start, SATZ_GLOBAL);
                        }
                        if !tag.selbst {
                            let zu = finde_ci(b, i, n, b"</style").unwrap_or(n);
                            css_pruefen(s, i, zu);
                            i = finde(b, zu, n, b">").map_or(n, |p| p + 1);
                        }
                    }
                    name => {
                        if EINBETTEN.contains(&name) {
                            s.fehler(start, SATZ_EINBETTEN);
                        }
                        match name {
                            "svelte:window" | "svelte:body" | "svelte:document" => s.fehler(start, format!("`<{name}>` {SATZ_SVELTE_FENSTER}")),
                            "svelte:head" => s.fehler(start, SATZ_SVELTE_HEAD),
                            "svelte:element" => s.fehler(start, SATZ_SVELTE_ELEMENT),
                            _ => {}
                        }
                        if !tag.selbst && !LEERE_ELEMENTE.contains(&name) {
                            offen.push(name.to_string());
                        }
                    }
                }
                text_von = i;
                continue;
            }
        }
        if b[i] == b'{' {
            markup_text(s, text_von, i);
            i = markup_ausdruck(s, i);
            text_von = i;
            continue;
        }
        i += 1;
    }
    markup_text(s, text_von, n);
}

struct Tag {
    name: String,
    ende: usize,
    selbst: bool,
    attribute: Vec<String>,
}

/// Liest ein Start-Tag ab `<` an `i` und prüft seine Attribute.
fn start_tag(s: &mut Sammler, i: usize) -> Tag {
    let src = s.src;
    let b = src.as_bytes();
    let n = b.len();
    let roh_name = element_name(src, i + 1);
    let name = roh_name.to_ascii_lowercase();
    let mut p = i + 1 + roh_name.len();
    let mut attribute = Vec::new();
    loop {
        while p < n && b[p].is_ascii_whitespace() {
            p += 1;
        }
        if p >= n {
            return Tag { name, ende: n, selbst: false, attribute };
        }
        match b[p] {
            b'>' => return Tag { name, ende: p + 1, selbst: false, attribute },
            b'/' if b.get(p + 1) == Some(&b'>') => return Tag { name, ende: p + 2, selbst: true, attribute },
            b'{' => {
                p = ausdruck(s, p + 1);
                continue;
            }
            _ => {}
        }
        let a_von = p;
        while p < n && !b[p].is_ascii_whitespace() && !matches!(b[p], b'=' | b'>' | b'{') && !(b[p] == b'/' && b.get(p + 1) == Some(&b'>')) {
            p += 1;
        }
        if p == a_von {
            p += 1;
            continue;
        }
        let a_name = src[a_von..p].to_ascii_lowercase();
        let mut q = p;
        while q < n && b[q].is_ascii_whitespace() {
            q += 1;
        }
        let mut teile: Vec<(usize, usize)> = Vec::new();
        if q < n && b[q] == b'=' {
            q += 1;
            while q < n && b[q].is_ascii_whitespace() {
                q += 1;
            }
            if q < n && (b[q] == b'"' || b[q] == b'\'') {
                let quote = b[q];
                q += 1;
                let mut teil = q;
                while q < n && b[q] != quote {
                    if b[q] == b'{' {
                        teile.push((teil, q));
                        q = ausdruck(s, q + 1);
                        teil = q;
                    } else {
                        q += 1;
                    }
                }
                teile.push((teil.min(n), q.min(n)));
                p = (q + 1).min(n);
            } else if q < n && b[q] == b'{' {
                p = groesse_ausgenommen(src, &name, &a_name, q + 1).unwrap_or_else(|| ausdruck(s, q + 1));
            } else {
                let v = q;
                while q < n && !b[q].is_ascii_whitespace() && b[q] != b'>' {
                    q += 1;
                }
                teile.push((v, q));
                p = q;
            }
        }
        attribut_pruefen(s, &name, &a_name, &teile, a_von);
        attribute.push(a_name);
    }
}

/// Prüft den Text-Teil (`teile`) eines Attributs.
fn attribut_pruefen(s: &mut Sammler, tag: &str, name: &str, teile: &[(usize, usize)], pos: usize) {
    let src = s.src;
    let texte: Vec<(usize, &str)> = teile.iter().map(|&(a, b)| (a, &src[a..b])).collect();
    if name.starts_with("on") && !name.starts_with("on:") && texte.iter().any(|(_, t)| !t.trim().is_empty()) {
        s.fehler(pos, SATZ_ON_TEXT);
    }
    if ADRESS_ATTRIBUTE.contains(&name) {
        if let Some(&(a, w)) = texte.first() {
            adresse_pruefen(s, tag, name, a, w);
        }
        return;
    }
    if name == "style" {
        for &(a, b) in teile {
            css_pruefen(s, a, b);
        }
    } else if name.starts_with("style:") || FARB_ATTRIBUTE.contains(&name) {
        for &(a, w) in &texte {
            css_wert_pruefen(s, a, w);
            if name.starts_with("style:") {
                url_und_global(s, a, w);
            }
        }
    }
    if name == "d" || name == "points" {
        if texte.iter().any(|(_, t)| t.bytes().any(|c| c.is_ascii_digit())) {
            s.fehler(pos, format!("Eingebettetes SVG mit Koordinaten (`{name}=`). {SATZ_ABBILDUNG}"));
        }
        return;
    }
    // `<Bild datei="…">` ($katalog) nennt eine Datei in assets/; Ziffern und
    // Termine im Dateinamen sind keine Daten der Seite. Den Namen prüft der
    // Baustein selbst (wie `is_safe_asset_name`).
    if tag == "bild" && name == "datei" {
        return;
    }
    let mut zahl = None;
    for &(a, w) in &texte {
        for (o, l) in termine_im_text(w) {
            s.fehler(a + o, format!("Eingetipptes Datum „{}“. Termine gehören in daten/*.toml.", &w[o..o + l]));
        }
        if zahl.is_none() {
            zahl = zahlen_im_text(w).into_iter().next();
        }
    }
    if let Some((_, z)) = zahl {
        if GROESSEN_EIGENSCHAFTEN.contains(&(tag, name)) && texte.iter().all(|(_, t)| reine_zahl(t)) {
            return;
        }
        s.hinweis(pos, format!("Eingetippte Zahl {z} im Attribut `{name}`. {SATZ_ZAHL}"));
    }
}

/// Größen-Eigenschaften von Bausteinen aus $katalog (Baustein, Eigenschaft;
/// beides klein geschrieben): `<Tabelle mindestbreite={28}>` ist eine Angabe
/// zum Aussehen in rem, keine Zahl, die in daten/*.toml gehört. Ausgenommen
/// ist nur eine reine Zahl; ein Ausdruck wird weiter geprüft.
const GROESSEN_EIGENSCHAFTEN: &[(&str, &str)] = &[("tabelle", "mindestbreite"), ("zeitachse", "linienhoehe")];

/// `{28}` an einer Größen-Eigenschaft (`von` steht hinter der `{`): das Ende
/// hinter der `}`, sonst `None` (dann prüft `ausdruck` wie überall).
fn groesse_ausgenommen(src: &str, tag: &str, name: &str, von: usize) -> Option<usize> {
    if !GROESSEN_EIGENSCHAFTEN.contains(&(tag, name)) {
        return None;
    }
    let zu = von + src.get(von..)?.find('}')?;
    reine_zahl(&src[von..zu]).then_some(zu + 1)
}

/// Eine nicht negative Dezimalzahl wie `28` oder `2.5`, sonst nichts.
fn reine_zahl(t: &str) -> bool {
    let t = t.trim();
    let mut teile = t.split('.');
    let ganz = teile.next().unwrap_or("");
    let rest = teile.next();
    teile.next().is_none()
        && !ganz.is_empty()
        && ganz.bytes().all(|c| c.is_ascii_digit())
        && rest.is_none_or(|r| !r.is_empty() && r.bytes().all(|c| c.is_ascii_digit()))
}

/// Adressen in `href`, `src` und ähnlichen Attributen.
fn adresse_pruefen(s: &mut Sammler, tag: &str, name: &str, pos: usize, wert: &str) {
    let k: String = wert.chars().filter(|c| !c.is_whitespace() && !c.is_control()).collect::<String>().to_ascii_lowercase();
    if gefaehrliche_adresse(&k) {
        s.fehler(pos, format!("Die Adresse „{}“ ist verboten. Links nur mit https:// oder im Dashboard (/…).", kurz(wert)));
        return;
    }
    let aussen = k.starts_with("http://") || k.starts_with("https://") || k.starts_with("//");
    if name == "href" && matches!(tag, "a" | "area") {
        if aussen && !k.starts_with("https://") {
            s.fehler(pos, format!("Der Link „{}“ nutzt kein https://. Links nach außen nur mit https://.", kurz(wert)));
        }
    } else if aussen || k.contains(",http:") || k.contains(",https:") || k.contains(" http") {
        s.fehler(pos, format!("„{}“ lädt eine Datei aus dem Netz. Bilder, Schriften und Dateien gehören nach assets/.", kurz(wert)));
    }
}

/// Ein `{…}` im Markup ab `{` an `i`; Stelle danach.
fn markup_ausdruck(s: &mut Sammler, i: usize) -> usize {
    let b = s.src.as_bytes();
    let n = b.len();
    let mut p = i + 1;
    while p < n && b[p].is_ascii_whitespace() {
        p += 1;
    }
    if p < n && matches!(b[p], b'#' | b':' | b'/' | b'@') {
        let kw_von = p;
        p += 1;
        while p < n && b[p].is_ascii_alphabetic() {
            p += 1;
        }
        if &s.src[kw_von..p] == "@html" {
            s.fehler(i, SATZ_HTML);
        }
    }
    ausdruck(s, p)
}

/// Fließtext im Markup: Termine sind Fehler, Zahlen Hinweise.
fn markup_text(s: &mut Sammler, von: usize, bis: usize) {
    if von >= bis {
        return;
    }
    let text = &s.src[von..bis];
    for (o, l) in termine_im_text(text) {
        s.fehler(von + o, format!("Eingetipptes Datum „{}“. Termine gehören in daten/*.toml.", &text[o..o + l]));
    }
    for (o, z) in zahlen_im_text(text) {
        zahl_hinweis(s, von + o, &z);
    }
}

// ----- Einmalige Links -----

/// Namen im Query-Teil, die nach einmaligem Link aussehen.
const EINMALIG: [&str; 5] = ["token", "code", "key", "sig", "password"];

fn links_pruefen(s: &mut Sammler) {
    let src = s.src;
    let klein = src.to_ascii_lowercase();
    for (p, _) in klein.match_indices("http") {
        if !(klein[p..].starts_with("http://") || klein[p..].starts_with("https://")) {
            continue;
        }
        let ende = src[p..].find(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | '<' | '>' | ')' | ']' | '`' | '|')).map_or(src.len(), |e| p + e);
        let adresse = &klein[p..ende];
        let Some(q) = adresse.find('?') else { continue };
        let query = adresse[q + 1..].split('#').next().unwrap_or("");
        for teil in query.split(['&', ';']) {
            let Some((name, _)) = teil.split_once('=') else { continue };
            if EINMALIG.iter().any(|g| name == *g || name.ends_with(&format!("_{g}")) || name.ends_with(&format!("-{g}"))) {
                s.hinweis(p, format!("Die Adresse enthält `{name}=`. {SATZ_EINMALIG}"));
                break;
            }
        }
    }
}

// ----- Abschalten von Prüfungen (Etappe 3b-2) -----

/// Kommentare, die eine Prüfung abschalten, und ihr Name im Satz. In
/// Projektseiten ist keiner erlaubt (Anleitung, „erlaubte Abschaltungen“:
/// Projektseiten dürfen gar nichts abschalten); `stylelint-disable` wirkt dort
/// ohnehin nicht (`werkzeug::stil`).
const ABSCHALTUNGEN: &[(&str, &str)] = &[
    ("@ts-nocheck", "`@ts-nocheck`"),
    ("@ts-ignore", "`@ts-ignore`"),
    ("@ts-expect-error", "`@ts-expect-error`"),
    ("svelte-ignore", "`svelte-ignore`"),
    ("eslint-disable", "`eslint-disable`"),
];

/// Satz für eine Abschaltung in einer Projektseite.
pub const SATZ_ABSCHALTEN: &str =
    "Eine Projektseite schaltet keine Prüfung ab. Die Stelle selbst in Ordnung bringen; geht das nicht, dem Betreuer des Dashboards Bescheid geben.";

/// Etappe 3b-2 (Auflage aus der Abnahme 3b): `@ts-nocheck`, `@ts-ignore`,
/// `@ts-expect-error`, `svelte-ignore`, `eslint-disable…` und ESLint-Regeln
/// im Kommentar (`/* eslint regel: off */`) sind in Projekten Fehler. Gesucht
/// wird wie in `web/src/abschaltungen.spec.ts`: nach `//`, `/*` oder `<!--`
/// (nicht direkt hinter einem Anführungszeichen) steht das Stichwort; in einem
/// Block-Kommentar auch am Anfang einer Folgezeile (nach `*`).
fn abschaltungen_pruefen(s: &mut Sammler) {
    let src = s.src;
    let mut funde: Vec<(usize, String)> = Vec::new();
    let mut zeile_start = 0;
    for zeile in src.split_inclusive('\n') {
        let b = zeile.as_bytes();
        let mut kandidaten: Vec<usize> = Vec::new();
        for muster in ["//", "/*", "<!--"] {
            for (i, _) in zeile.match_indices(muster) {
                let davor = if i == 0 { None } else { Some(b[i - 1]) };
                if !matches!(davor, Some(b'\'' | b'"' | b'`' | b'\\')) {
                    kandidaten.push(i + muster.len());
                }
            }
        }
        // Folgezeile eines Block-Kommentars: „ * @ts-nocheck“.
        let ohne = zeile.trim_start();
        if let Some(rest) = ohne.strip_prefix('*')
            && !rest.starts_with('/')
        {
            kandidaten.push(zeile.len() - ohne.len() + 1);
        }
        for k in kandidaten {
            let rest = zeile[k..].trim_start();
            let pos = zeile_start + k;
            if rest.strip_prefix('/').is_some_and(|r| r.trim_start().starts_with("<reference")) {
                // `/// <reference …>` (nach `//` steht noch ein `/`): kein Abschalten, aber ebenso nur als Kommentar zu finden.
                funde.push((pos, SATZ_GLOBALE_TYPEN.to_string()));
            } else if let Some((_, name)) = ABSCHALTUNGEN.iter().find(|(w, _)| rest.starts_with(w)) {
                funde.push((pos, format!("{name} ist in Projektseiten verboten. {SATZ_ABSCHALTEN}")));
            } else if let Some(regel) = rest.strip_prefix("eslint ").filter(|r| r.split_whitespace().next().is_some_and(|w| w.ends_with(':'))) {
                let regel = regel.split(':').next().unwrap_or("").trim();
                funde.push((pos, format!("`eslint {regel}: …` im Kommentar stellt eine Regel um und ist in Projektseiten verboten. {SATZ_ABSCHALTEN}")));
            }
        }
        zeile_start += zeile.len();
    }
    for (pos, satz) in funde {
        s.fehler(pos, satz);
    }
}

// ----- svelte-check -----

/// So lange darf svelte-check laufen.
pub const SVELTE_CHECK_ZEITLIMIT: Duration = Duration::from_secs(60);

/// Aufruf in `web/`: `node node_modules/svelte-check/bin/svelte-check …`.
/// Node kommt über `dienst::programme` (fester Pfad, ohne Fenster); npx
/// bräuchte cmd.exe und die PATH-Suche. Die eigene Konfiguration
/// `tsconfig.projekte.json` nimmt alle Projektdateien auf; die der Oberfläche
/// (`tsconfig.json`, für `npm run check`) nur die Vorlage, damit eine kaputte
/// Seite eines Chats nie die Prüfung des Dashboards selbst rot macht.
pub const SVELTE_CHECK_ARGS: &[&str] = &["--output", "machine", "--workspace", ".", "--tsconfig", "./tsconfig.projekte.json"];

/// svelte-check für das Repo `repo`; Befunde nur für Dateien unter
/// `projects/<slug>/` (ohne `slug`: unter `projects/`). Fehlen Node oder
/// `web/node_modules`, läuft es zu lange oder nicht durch: ein Hinweis.
pub fn svelte_check(repo: &Path, slug: Option<&str>) -> Vec<Befund> {
    svelte_check_mit(repo, slug, SVELTE_CHECK_ZEITLIMIT)
}

/// Wie [`svelte_check`], mit eigenem Zeitlimit.
pub fn svelte_check_mit(repo: &Path, slug: Option<&str>, zeitlimit: Duration) -> Vec<Befund> {
    let ort = slug.map_or_else(|| "projects".to_string(), |s| format!("projects/{s}"));
    let hinweis = |satz: String| vec![Befund::hinweis(ort.clone(), None, satz)];
    match svelte_check_lauf(repo, slug, zeitlimit) {
        // Nicht fertig, aber mit Befunden: die zeigen, was svelte-check sah.
        Ok(Lauf { befunde, .. }) => befunde,
        Err(Fehlt::Werkzeug(satz)) => hinweis(format!("{satz}. Die Svelte-Prüfung ist übersprungen; der Scanner hat geprüft.")),
        Err(Fehlt::Lauf(satz)) => hinweis(satz),
    }
}

/// Ein Lauf von svelte-check: seine Befunde.
struct Lauf {
    befunde: Vec<Befund>,
}

/// Warum svelte-check nicht lief.
enum Fehlt {
    /// Node oder web/node_modules fehlt.
    Werkzeug(String),
    /// Start, Zeitlimit oder Abbruch ohne Befunde.
    Lauf(String),
}

fn svelte_check_lauf(repo: &Path, slug: Option<&str>, zeitlimit: Duration) -> Result<Lauf, Fehlt> {
    // Absolut, weil das Kind in `web/` startet und svelte-check absolute Pfade melden kann.
    let repo = &std::path::absolute(repo).unwrap_or_else(|_| repo.to_path_buf());
    let web = repo.join("web");
    let skript = web.join("node_modules").join("svelte-check").join("bin").join("svelte-check");
    if !skript.is_file() {
        return Err(Fehlt::Werkzeug("svelte-check ist nicht da (web/node_modules fehlt)".into()));
    }
    let mut cmd = programme::befehl(Werkzeug::Node).map_err(Fehlt::Werkzeug)?;
    cmd.current_dir(&web).arg(&skript).args(SVELTE_CHECK_ARGS).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let (aus, fehl, code) = mit_zeitlimit(cmd, zeitlimit, "svelte-check", None).map_err(Fehlt::Lauf)?;
    let befunde = machine_lesen(&aus, repo, slug);
    let fertig = aus.contains(" COMPLETED ");
    if befunde.is_empty() && !fertig {
        let grund = fehl.lines().chain(aus.lines()).map(str::trim).find(|l| !l.is_empty()).unwrap_or("ohne Ausgabe");
        let code = code.map_or_else(|| "ohne Rückgabewert".to_string(), |c| format!("Rückgabe {c}"));
        return Err(Fehlt::Lauf(format!("svelte-check lief nicht durch ({code}): {}", kurz(grund))));
    }
    Ok(Lauf { befunde })
}

/// Startet `cmd`, gibt ihm `eingabe` auf stdin (sonst nichts), liest stdout
/// und stderr und bricht nach `limit` ab. `name` steht in den Sätzen.
pub(crate) fn mit_zeitlimit(
    mut cmd: std::process::Command,
    limit: Duration,
    name: &str,
    eingabe: Option<String>,
) -> Result<(String, String, Option<i32>), String> {
    if eingabe.is_some() {
        cmd.stdin(Stdio::piped());
    }
    let mut kind = cmd.spawn().map_err(|e| format!("{name} ließ sich nicht starten: {e}."))?;
    // Eigener Faden: Eine lange Eingabe darf das Lesen der Ausgabe nicht blockieren.
    let schreiber = match (eingabe, kind.stdin.take()) {
        (Some(text), Some(mut ein)) => Some(std::thread::spawn(move || {
            use std::io::Write;
            let _ = ein.write_all(text.as_bytes());
        })),
        _ => None,
    };
    let lies = |quelle: Option<Box<dyn Read + Send>>| {
        std::thread::spawn(move || {
            let mut v = Vec::new();
            if let Some(mut q) = quelle {
                let _ = q.read_to_end(&mut v);
            }
            String::from_utf8_lossy(&v).into_owned()
        })
    };
    let aus = lies(kind.stdout.take().map(|o| Box::new(o) as Box<dyn Read + Send>));
    let fehl = lies(kind.stderr.take().map(|o| Box::new(o) as Box<dyn Read + Send>));
    let start = Instant::now();
    let code = loop {
        match kind.try_wait() {
            Ok(Some(status)) => break status.code(),
            Ok(None) if start.elapsed() < limit => std::thread::sleep(Duration::from_millis(50)),
            _ => {
                let _ = kind.kill();
                let _ = kind.wait();
                return Err(format!("{name} hat nach {} s nicht geantwortet und wurde beendet.", limit.as_secs()));
            }
        }
    };
    if let Some(s) = schreiber {
        let _ = s.join();
    }
    Ok((aus.join().unwrap_or_default(), fehl.join().unwrap_or_default(), code))
}

/// Liest einen JSON-Text am Anfang von `s`: (Text, Rest).
pub(crate) fn json_text(s: &str) -> Option<(String, &str)> {
    let b = s.as_bytes();
    if b.first() != Some(&b'"') {
        return None;
    }
    let mut i = 1;
    while i < b.len() {
        match b[i] {
            b'\\' => i += 2,
            b'"' => return Some((serde_json::from_str(&s[..=i]).ok()?, &s[i + 1..])),
            _ => i += 1,
        }
    }
    None
}

/// Die Ausgabe von `svelte-check --output machine` (Arbeitsordner `web/`)
/// als Befunde, nur für Dateien unter `projects/<slug>/` (ohne `slug`: unter
/// `projects/`). ERROR wird Fehler, WARNING Hinweis. Zeilen wie
/// `1793137645769 ERROR "../projects/x/Seite.svelte" 3:5 "Meldung"`.
pub fn machine_lesen(ausgabe: &str, repo: &Path, slug: Option<&str>) -> Vec<Befund> {
    let praefix = slug.map_or_else(|| "projects/".to_string(), |s| format!("projects/{s}/"));
    let mut befunde = Vec::new();
    for zeile in ausgabe.lines() {
        let mut teile = zeile.trim().splitn(3, ' ');
        let (Some(zeit), Some(art), Some(rest)) = (teile.next(), teile.next(), teile.next()) else { continue };
        if zeit.is_empty() || !zeit.bytes().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let schwere = match art {
            "ERROR" => Schwere::Fehler,
            "WARNING" => Schwere::Hinweis,
            _ => continue,
        };
        let Some((datei, rest)) = json_text(rest.trim_start()) else { continue };
        let rest = rest.trim_start();
        let Some((stelle, rest)) = rest.split_once(' ') else { continue };
        let nr = stelle.split(':').next().and_then(|z| z.parse::<usize>().ok());
        let Some((meldung, _)) = json_text(rest.trim_start()) else { continue };
        let Some(datei) = repo_pfad(&datei, repo) else { continue };
        if !datei.starts_with(&praefix) {
            continue;
        }
        let text = format!("svelte-check: {meldung}");
        befunde.push(match schwere {
            Schwere::Fehler => Befund::fehler(datei, nr, text),
            Schwere::Hinweis => Befund::hinweis(datei, nr, text),
        });
    }
    befunde
}

/// Pfad aus svelte-check (relativ zu `web/` oder absolut) relativ zum Repo,
/// mit `/`; `None`, wenn er nicht im Repo liegt.
fn repo_pfad(datei: &str, repo: &Path) -> Option<String> {
    let datei = datei.replace('\\', "/");
    let ist_absolut = datei.starts_with('/') || datei.as_bytes().get(1) == Some(&b':');
    let roh = if ist_absolut {
        let wurzel = repo.to_string_lossy().replace('\\', "/");
        let wurzel = wurzel.trim_end_matches('/');
        let (kopf, rest) = (datei.get(..wurzel.len())?, datei.get(wurzel.len()..)?);
        if !kopf.eq_ignore_ascii_case(wurzel) || !rest.starts_with('/') {
            return None;
        }
        rest[1..].to_string()
    } else {
        format!("web/{datei}")
    };
    let mut teile: Vec<&str> = Vec::new();
    for t in roh.split('/') {
        match t {
            "" | "." => {}
            ".." => {
                teile.pop()?;
            }
            _ => teile.push(t),
        }
    }
    Some(teile.join("/"))
}

// ----- Bauliste -----

/// Grüne Projekte (dürfen gebaut werden) und rote mit ihren Fehlern.
pub type Bauliste = (Vec<String>, BTreeMap<String, Vec<Befund>>);

/// Die Kopfzeile von projekte.generated.ts.
pub const BAULISTE_KOPF: &str = "// Erzeugt von dashboard pruefe --bauliste. Nicht von Hand ändern.";

/// Für alle Projekte in `repo/projects` mit `Seite.svelte`: grün, wenn der
/// Scanner keinen Fehler findet; sonst rot mit den Fehlern.
pub fn bauliste(repo: &Path) -> Bauliste {
    bauliste_pruefen(&repo.join("projects")).unwrap_or_default()
}

/// Wie [`bauliste`], aus dem Projektordner `dir`: nur der Scanner, in
/// Millisekunden (`pruefe --bauliste` ohne `--svelte`). Das Urteil für den
/// Bau (dazu Stil, ESLint, svelte-check, Vitest; übersprungen heißt nicht
/// grün) fällt allein `bote::bau`, beim Einspielen über
/// `pruefe --bauliste --svelte`. `Err` mit dem Grund, wenn der
/// Projektordner sich nicht lesen lässt.
pub fn bauliste_pruefen(dir: &Path) -> Result<Bauliste, String> {
    let eintraege = std::fs::read_dir(dir).map_err(|e| format!("Der Projektordner {} lässt sich nicht lesen: {e}", dir.display()))?;
    let mut slugs: Vec<String> = eintraege
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|s| seite::is_valid_slug(s))
        .filter(|s| std::fs::symlink_metadata(dir.join(s).join("Seite.svelte")).is_ok_and(|m| !m.is_dir()))
        .collect();
    slugs.sort();
    let mut gruen = Vec::new();
    let mut rote = BTreeMap::new();
    for slug in slugs {
        let fehler: Vec<Befund> = if seite::ist_verknuepfung(&dir.join(&slug)) {
            vec![Befund::fehler(format!("projects/{slug}"), None, seite::verknuepfung_satz(&slug))]
        } else {
            pruefe_projekt(dir, &slug).into_iter().filter(Befund::ist_fehler).collect()
        };
        if fehler.is_empty() {
            gruen.push(slug);
        } else {
            rote.insert(slug, fehler);
        }
    }
    Ok((gruen, rote))
}

/// Schreibt die Bauliste nach `ziel` (gedacht:
/// `web/src/lib/projekte.generated.ts`), nur wenn sich der Inhalt ändert.
/// Rückgabe: die Liste und ob die Datei neu geschrieben wurde.
pub fn bauliste_schreiben(repo: &Path, ziel: &Path) -> std::io::Result<(Bauliste, bool)> {
    let g = bauliste_schreiben_mit(&repo.join("projects"), ziel)?;
    Ok(((g.gruen, g.rote), g.neu))
}

/// Was [`bauliste_schreiben_mit`] getan hat.
#[derive(Debug)]
pub struct Geschrieben {
    pub gruen: Vec<String>,
    pub rote: BTreeMap<String, Vec<Befund>>,
    /// Die Datei wurde neu geschrieben (sonst stand derselbe Inhalt schon da).
    pub neu: bool,
    /// Die Prüfung ist gescheitert (Grund); die Datei ist dann die leere Liste.
    pub gescheitert: Option<String>,
}

/// Wie [`bauliste_schreiben`], aus dem Projektordner `dir` (nur der
/// Scanner). Scheitert die Prüfung, wird die LEERE Liste geschrieben (nie
/// bleibt eine alte Liste stehen) und `gescheitert` nennt den Grund.
pub fn bauliste_schreiben_mit(dir: &Path, ziel: &Path) -> std::io::Result<Geschrieben> {
    let (liste, gescheitert) = match bauliste_pruefen(dir) {
        Ok(l) => (l, None),
        Err(grund) => (Bauliste::default(), Some(grund)),
    };
    let neu = liste_schreiben(dir, ziel, &liste.0)?;
    Ok(Geschrieben { gruen: liste.0, rote: liste.1, neu, gescheitert })
}

/// Schreibt die leere Bauliste nach `ziel` (gebaut wird dann ohne
/// Projektseiten). Rückgabe: ob die Datei neu geschrieben wurde.
pub fn bauliste_leer_schreiben(dir: &Path, ziel: &Path) -> std::io::Result<bool> {
    liste_schreiben(dir, ziel, &[])
}

fn liste_schreiben(dir: &Path, ziel: &Path, gruen: &[String]) -> std::io::Result<bool> {
    let eintraege: Vec<(String, PathBuf)> = gruen.iter().map(|s| (s.clone(), dir.join(s))).collect();
    liste_schreiben_aus(ziel, &eintraege)
}

/// Schreibt die Bauliste für `eintraege` (Slug, Ordner des Projekts) nach
/// `ziel`, nur wenn sich der Inhalt ändert. Rückgabe: neu geschrieben?
/// Etappe 4b: Der Ordner kann eine Inhaltsquelle außerhalb des Repos sein
/// oder die letzte grüne Kopie unter `web/.projekte-gruen/<slug>`.
pub fn liste_schreiben_aus(ziel: &Path, eintraege: &[(String, PathBuf)]) -> std::io::Result<bool> {
    let text = bauliste_text(eintraege);
    if std::fs::read_to_string(ziel).is_ok_and(|alt| alt == text) {
        return Ok(false);
    }
    if let Some(eltern) = ziel.parent() {
        std::fs::create_dir_all(eltern)?;
    }
    std::fs::write(ziel, text)?;
    Ok(true)
}
/// Der Satz, wenn die Prüfung für die Bauliste scheitert.
pub fn satz_gescheitert(grund: &str) -> String {
    format!(
        "Die Prüfung für die Bauliste ist gescheitert: {}. Die Bauliste ist deshalb leer: gebaut wird ohne Projektseiten.",
        grund.trim().trim_end_matches('.')
    )
}

/// Abnahme 4b: Der Pfad steht roh in `import('…')` der Bauliste. Er darf
/// darum nur Zeichen haben, die dort nichts anrichten (Buchstaben, Ziffern,
/// Leerzeichen und `-_./:()+,`), und nicht auf einem Netzlaufwerk liegen
/// (`\\server\…` wird als Pfad nicht richtig). Sonst ein Grund in einem Satz.
pub fn import_pfad_pruefen(p: &Path) -> Result<String, String> {
    let abs = std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf());
    if let Some(Component::Prefix(v)) = abs.components().next() {
        use std::path::Prefix;
        if matches!(v.kind(), Prefix::UNC(..) | Prefix::VerbatimUNC(..) | Prefix::DeviceNS(..)) {
            return Err("Der Ordner liegt auf einem Netzlaufwerk; die Bauliste kann nur Ordner auf einem Laufwerk dieses PCs einbinden.".into());
        }
    }
    let weg = import_pfad(p);
    match weg.chars().find(|c| !(c.is_alphanumeric() || " -_./:()+,".contains(*c))) {
        Some(c) => Err(format!(
            "Im Pfad des Ordners steht das Zeichen „{}“; die Bauliste kann ihn so nicht einbinden. Bitte den Ordner ohne dieses Zeichen benennen.",
            if c.is_control() { c.escape_default().to_string() } else { c.to_string() }
        )),
        None => Ok(weg),
    }
}

/// Absoluter Pfad mit `/` für `import(…)` (Etappe 4b: die Bauliste nennt
/// absolute Pfade, damit Projekte aus jeder Inhaltsquelle gehen), ohne `.`
/// und `..`, ohne `\\?\` vorn. Vor dem Schreiben in die Bauliste immer
/// [`import_pfad_pruefen`].
pub fn import_pfad(p: &Path) -> String {
    let p = std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf());
    let mut kopf = String::new();
    let mut teile: Vec<String> = Vec::new();
    for c in p.components() {
        match c {
            Component::Prefix(v) => kopf = v.as_os_str().to_string_lossy().trim_start_matches(r"\\?\").replace('\\', "/"),
            Component::RootDir | Component::CurDir => {}
            Component::ParentDir => {
                teile.pop();
            }
            Component::Normal(n) => teile.push(n.to_string_lossy().into_owned()),
        }
    }
    format!("{kopf}/{}", teile.join("/"))
}
/// Schlüssel im TS-Objekt: ohne Anführungszeichen, wo das geht (wie prettier).
fn ts_schluessel(k: &str) -> String {
    let frei = k.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_' || c == '$')
        && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$');
    if frei { k.to_string() } else { format!("'{k}'") }
}

/// Ein Objekt `name` mit den Einträgen, so formatiert, wie prettier es
/// schreibt (Tabs, keine Komma am Ende, Umbruch über 100 Zeichen).
fn ts_objekt(name: &str, eintraege: &[(String, String)]) -> String {
    if eintraege.is_empty() {
        return format!("export const {name}: Record<string, Laden> = {{}};\n");
    }
    let mut t = format!("export const {name}: Record<string, Laden> = {{\n");
    for (i, (k, weg)) in eintraege.iter().enumerate() {
        let komma = if i + 1 < eintraege.len() { "," } else { "" };
        let zeile = format!("{}: () => import('{weg}'){komma}", ts_schluessel(k));
        // Ein Tab zählt bei prettier zwei Zeichen.
        if zeile.chars().count() + 2 > 100 {
            t.push_str(&format!("\t{}: () =>\n\t\timport('{weg}'){komma}\n", ts_schluessel(k)));
        } else {
            t.push_str(&format!("\t{zeile}\n"));
        }
    }
    t.push_str("};\n");
    t
}

/// Inhalt von projekte.generated.ts für `eintraege` (Slug, Ordner des
/// Projekts): je Projekt `Seite.svelte` und die Unterseiten aus `seiten/`,
/// mit absoluten Pfaden und nur explizitem `import()` (kein `import.meta.glob`).
pub fn bauliste_text(eintraege: &[(String, PathBuf)]) -> String {
    // Was sich nicht sicher einbinden lässt, fehlt in der Liste (der Bau
    // meldet es vorher als rot; hier nur die letzte Sperre).
    let eintraege: Vec<(String, PathBuf, String)> =
        eintraege.iter().filter_map(|(s, o)| import_pfad_pruefen(o).ok().map(|w| (s.clone(), o.clone(), w))).collect();
    let seiten: Vec<(String, String)> = eintraege.iter().map(|(s, _, w)| (s.clone(), format!("{w}/Seite.svelte"))).collect();
    let mut unter: Vec<(String, String)> = Vec::new();
    for (slug, ordner, basis) in &eintraege {
        let Ok(e) = std::fs::read_dir(ordner.join("seiten")) else { continue };
        let mut namen: Vec<String> = e
            .filter_map(|e| e.ok())
            .filter(|e| !e.path().is_dir())
            .filter_map(|e| e.file_name().into_string().ok())
            .filter_map(|n| n.strip_suffix(".svelte").map(str::to_string))
            .filter(|n| ist_seitenname(n))
            .collect();
        namen.sort();
        unter.extend(namen.into_iter().map(|n| (format!("{slug}/{n}"), format!("{basis}/seiten/{n}.svelte"))));
    }
    format!(
        "{BAULISTE_KOPF}\n\ntype Laden = () => Promise<{{ default: import('svelte').Component }}>;\n\n{}\n{}",
        ts_objekt("projektSeiten", &seiten),
        ts_objekt("projektUnterseiten", &unter)
    )
}
