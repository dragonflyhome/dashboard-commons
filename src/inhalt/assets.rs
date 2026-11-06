//! Regeln für `projects/<slug>/assets/` (Zielbild, Abschnitt b, „Assets“):
//! einfache Dateinamen ohne Unterordner, feste Formate, Größen, und für SVG
//! und Vorschau-HTML, was darin stehen darf.
//!
//! Gelesen wird nur bis zur Grenze: Bilder nur ihre Größe, SVG und HTML als
//! Text (höchstens so viel, wie erlaubt ist). Befunde zu SVG und HTML nennen
//! die Zeile.

use std::io::Read;
use std::path::Path;

use super::pruefung::Befund;
use crate::vertrag::lesen::zeile_bei;

/// Größen in Bytes. Dieselben Grenzen gelten in der Prüfung und beim
/// Ausliefern (`api::projekte::asset`), siehe [`groesse_grenze`].
pub const BILD_HOECHSTENS: u64 = 2 * 1024 * 1024;
pub const SVG_HOECHSTENS: u64 = 500 * 1024;
pub const HTML_HOECHSTENS: u64 = 2 * 1024 * 1024;
/// Für alle anderen Dateien (PDF, CSV, Text, JSON …).
pub const SONST_HOECHSTENS: u64 = 2 * 1024 * 1024;
/// Alle Dateien in `assets/` zusammen.
pub const PROJEKT_HOECHSTENS: u64 = 50 * 1024 * 1024;

/// So groß darf eine Datei mit dieser Endung höchstens sein (Endung ohne
/// Punkt, Groß- und Kleinschreibung egal).
pub fn groesse_grenze(endung: &str) -> u64 {
    let e = endung.to_ascii_lowercase();
    match e.as_str() {
        "svg" => SVG_HOECHSTENS,
        "html" | "htm" => HTML_HOECHSTENS,
        _ if BILDER.contains(&e.as_str()) => BILD_HOECHSTENS,
        _ => SONST_HOECHSTENS,
    }
}

/// Eine Grenze als Wort: „2 MB“, „500 KB“.
pub fn grenze_wort(bytes: u64) -> String {
    if bytes >= 1024 * 1024 && bytes.is_multiple_of(1024 * 1024) { format!("{} MB", bytes / (1024 * 1024)) } else { format!("{} KB", bytes.div_ceil(1024)) }
}

/// Namen, die Windows für Geräte freihält: `con`, `nul`, `prn`, `aux`,
/// `com1`–`com9`, `lpt1`–`lpt9`. Sie gelten auch mit Endung (`con.svg`),
/// darum zählt nur der Teil vor dem ersten Punkt; Groß/Klein egal.
pub fn ist_geraetename(stamm: &str) -> bool {
    let s = stamm.split('.').next().unwrap_or("").to_ascii_lowercase();
    match s.as_str() {
        "con" | "nul" | "prn" | "aux" => true,
        _ => {
            let b = s.as_bytes();
            b.len() == 4 && (s.starts_with("com") || s.starts_with("lpt")) && (b'1'..=b'9').contains(&b[3])
        }
    }
}

/// Erlaubte Endungen (klein geschrieben).
pub const FORMATE: [&str; 12] = ["svg", "png", "jpg", "jpeg", "webp", "gif", "pdf", "csv", "txt", "json", "html", "htm"];
const BILDER: [&str; 5] = ["png", "jpg", "jpeg", "webp", "gif"];
const SCHRIFTEN: [&str; 5] = ["woff", "woff2", "ttf", "otf", "eot"];
const PROGRAMME: [&str; 13] = ["exe", "dll", "msi", "bat", "cmd", "com", "ps1", "psm1", "sh", "vbs", "scr", "jar", "lnk"];
const ARCHIVE: [&str; 10] = ["zip", "7z", "rar", "tar", "gz", "tgz", "bz2", "xz", "cab", "iso"];

/// Dateiname in `assets/`: `a-z 0-9 . _ -`, nicht mit Punkt vorn, höchstens
/// 128 Zeichen, kein Windows-Gerätename ([`ist_geraetename`]).
pub fn ist_asset_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && !name.starts_with('.')
        && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-'))
        && !ist_geraetename(name)
}

/// Größe in einfachem Deutsch: „2,4 MB“, „612 KB“.
fn groesse(bytes: u64) -> String {
    if bytes >= 1024 * 1024 { format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0)).replace('.', ",") } else { format!("{} KB", bytes.div_ceil(1024)) }
}

/// Prüft alle Dateien in `assets/` eines Projekts. Kein Ordner: keine Befunde.
pub fn pruefe_assets(dir: &Path, slug: &str) -> Vec<Befund> {
    let ordner = dir.join(slug).join("assets");
    let mut befunde = Vec::new();
    let ort = format!("projects/{slug}/assets");
    // Vor `is_dir`: das folgt einer Verknüpfung (Symlink, Junction) einfach.
    if super::seite::ist_verknuepfung(&ordner) {
        befunde.push(Befund::fehler(&ort, None, super::seite::verknuepfung_satz("assets")));
        return befunde;
    }
    if !ordner.is_dir() {
        return befunde;
    }
    let Ok(eintraege) = std::fs::read_dir(&ordner) else {
        befunde.push(Befund::fehler(&ort, None, "Der Ordner `assets/` lässt sich nicht lesen."));
        return befunde;
    };
    let mut namen: Vec<(String, std::fs::DirEntry)> = eintraege.filter_map(|e| e.ok()).map(|e| (e.file_name().to_string_lossy().into_owned(), e)).collect();
    namen.sort_by(|a, b| a.0.cmp(&b.0));
    let mut summe: u64 = 0;
    for (name, eintrag) in namen {
        let datei = format!("{ort}/{name}");
        // `file_type` folgt Verknüpfungen nicht: ein Symlink bleibt ein Symlink.
        let Ok(art) = eintrag.file_type() else { continue };
        if art.is_symlink() {
            befunde.push(Befund::fehler(
                &datei,
                None,
                format!("{name} ist eine Verknüpfung (Symlink). In `assets/` liegen nur echte Dateien; bitte die Datei selbst hierher legen."),
            ));
            continue;
        }
        if art.is_dir() {
            befunde.push(Befund::fehler(
                &datei,
                None,
                "In `assets/` sind keine Unterordner erlaubt. Bitte die Dateien direkt in `assets/` legen, mit einfachen Namen wie `plan-2.svg`.",
            ));
            continue;
        }
        let laenge = eintrag.metadata().map(|m| m.len()).unwrap_or(0);
        summe = summe.saturating_add(laenge);
        if ist_geraetename(&name) {
            befunde.push(Befund::fehler(
                &datei,
                None,
                format!("„{name}“ ist ein reservierter Windows-Name (con, nul, prn, aux, com1 bis com9, lpt1 bis lpt9, auch mit Endung). Bitte anders nennen, etwa `plan-2.svg`."),
            ));
        } else if !ist_asset_name(&name) {
            befunde.push(Befund::fehler(
                &datei,
                None,
                format!("„{name}“ hat keinen einfachen Namen. Erlaubt sind Kleinbuchstaben, Ziffern, Punkt, Unterstrich und Bindestrich, etwa `plan-2.svg`."),
            ));
        }
        let endung = match name.rsplit_once('.') {
            Some((stamm, e)) if !stamm.is_empty() => e.to_ascii_lowercase(),
            _ => String::new(),
        };
        let e = endung.as_str();
        if let Some(satz) = verboten(e) {
            befunde.push(Befund::fehler(&datei, None, satz));
            continue;
        }
        if !FORMATE.contains(&e) {
            let satz = if e.is_empty() {
                format!("Die Datei hat keine Endung. Erlaubt sind: {}.", FORMATE.join(", "))
            } else {
                format!("Das Format `.{e}` ist in `assets/` nicht erlaubt. Erlaubt sind: {}.", FORMATE.join(", "))
            };
            befunde.push(Befund::fehler(&datei, None, satz));
            continue;
        }
        let grenze = groesse_grenze(e);
        if laenge > grenze {
            let wie = format!("{} groß; erlaubt sind höchstens {}", groesse(laenge), grenze_wort(grenze));
            let satz = if BILDER.contains(&e) {
                format!("Das Bild ist {wie}. Bitte verkleinern, etwa als WebP.")
            } else if e == "svg" {
                format!("Das SVG ist {wie}. Bitte vereinfachen oder als PNG oder WebP ablegen.")
            } else if e == "html" || e == "htm" {
                format!("Die Vorschau ist {wie}. Bitte Bilder verkleinern oder als eigene Datei in `assets/` legen.")
            } else {
                format!("Die Datei ist {wie}. Das Dashboard liefert sie so nicht aus.")
            };
            befunde.push(Befund::fehler(&datei, None, satz));
            continue;
        }
        if e == "svg"
            && let Some(text) = text_bis(&eintrag.path(), grenze)
        {
            befunde.extend(svg_pruefen(&datei, &text));
        }
        if (e == "html" || e == "htm")
            && let Some(text) = text_bis(&eintrag.path(), grenze)
        {
            befunde.extend(html_pruefen(&datei, &text));
        }
    }
    if summe > PROJEKT_HOECHSTENS {
        befunde.push(Befund::fehler(
            &ort,
            None,
            format!("Alle Dateien in `assets/` zusammen sind {} groß; erlaubt sind höchstens 50 MB je Projekt.", groesse(summe)),
        ));
    }
    befunde
}

/// Satz zu einer verbotenen Endung, sonst `None`.
fn verboten(e: &str) -> Option<String> {
    if SCHRIFTEN.contains(&e) {
        return Some(format!("Schriften (`.{e}`) gehören nicht in ein Projekt. Die Seite nimmt die Schriften des Dashboards."));
    }
    if PROGRAMME.contains(&e) {
        return Some(format!("Ausführbare Dateien (`.{e}`) sind in `assets/` nicht erlaubt."));
    }
    if ARCHIVE.contains(&e) {
        return Some(format!("Archive (`.{e}`) sind in `assets/` nicht erlaubt. Bitte die Dateien einzeln ablegen."));
    }
    None
}

/// Höchstens `grenze` Bytes einer Datei als Text (ungültiges UTF-8 wird ersetzt).
fn text_bis(pfad: &Path, grenze: u64) -> Option<String> {
    let mut bytes = Vec::new();
    std::fs::File::open(pfad).ok()?.take(grenze).read_to_end(&mut bytes).ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

// ----- Text zerlegen -----

/// Ein Attribut in einem Element: Zeile (ab 1), Element und Name klein geschrieben, Wert.
struct Attribut<'a> {
    zeile: usize,
    element: String,
    name: String,
    wert: &'a str,
}

/// Ein Element-Anfang (`<form …>`), mit Zeile und Namen klein geschrieben.
struct Element {
    zeile: usize,
    name: String,
}

/// Zerlegt SVG oder HTML grob in Elemente und Attribute. Kommentare, `<!…>`,
/// `<?…?>`, schließende Elemente und der Inhalt von `<script>` und `<style>`
/// werden übersprungen. Kein vollständiger Parser, aber genug für die Regeln.
fn zerlegen(text: &str) -> (Vec<Element>, Vec<Attribut<'_>>) {
    let b = text.as_bytes();
    let n = b.len();
    let klein = text.to_ascii_lowercase();
    let anfaenge: Vec<usize> = std::iter::once(0).chain(b.iter().enumerate().filter(|(_, c)| **c == b'\n').map(|(i, _)| i + 1)).collect();
    let zeile_von = |pos: usize| anfaenge.partition_point(|&s| s <= pos);
    let bis_nach = |ab: usize, ende: &str| text[ab..].find(ende).map(|k| ab + k + ende.len()).unwrap_or(n);
    let leer = |c: u8| c.is_ascii_whitespace();

    let mut elemente = Vec::new();
    let mut attribute = Vec::new();
    let mut i = 0;
    while let Some(rel) = text[i..].find('<') {
        let start = i + rel;
        let rest = &text[start..];
        if rest.starts_with("<!--") {
            i = bis_nach(start + 4, "-->");
            continue;
        }
        if rest.starts_with("<![CDATA[") {
            i = bis_nach(start + 9, "]]>");
            continue;
        }
        if rest.starts_with("<!") || rest.starts_with("<?") || rest.starts_with("</") {
            i = bis_nach(start + 1, ">");
            continue;
        }
        let mut j = start + 1;
        while j < n && (b[j].is_ascii_alphanumeric() || matches!(b[j], b'-' | b'_' | b':' | b'.')) {
            j += 1;
        }
        if j == start + 1 {
            i = start + 1;
            continue;
        }
        let name = klein[start + 1..j].to_string();
        loop {
            while j < n && (leer(b[j]) || b[j] == b'/') {
                j += 1;
            }
            if j >= n {
                break;
            }
            if b[j] == b'>' {
                j += 1;
                break;
            }
            let an = j;
            while j < n && !leer(b[j]) && !matches!(b[j], b'=' | b'>' | b'/') {
                j += 1;
            }
            if j == an {
                j += 1;
                continue;
            }
            let attr = klein[an..j].to_string();
            while j < n && leer(b[j]) {
                j += 1;
            }
            let mut wert = "";
            if j < n && b[j] == b'=' {
                j += 1;
                while j < n && leer(b[j]) {
                    j += 1;
                }
                if j < n && (b[j] == b'"' || b[j] == b'\'') {
                    let q = b[j] as char;
                    let von = j + 1;
                    let bis = text[von..].find(q).map(|k| von + k).unwrap_or(n);
                    wert = &text[von..bis];
                    j = (bis + 1).min(n);
                } else {
                    let von = j;
                    while j < n && !leer(b[j]) && b[j] != b'>' {
                        j += 1;
                    }
                    wert = &text[von..j];
                }
            }
            attribute.push(Attribut { zeile: zeile_von(an), element: name.clone(), name: attr, wert });
        }
        elemente.push(Element { zeile: zeile_von(start), name: name.clone() });
        i = j.min(n);
        if matches!(lokal(&name), "script" | "style") {
            let ende = format!("</{name}");
            i = klein[i..].find(&ende).map(|k| i + k).unwrap_or(n);
        }
    }
    (elemente, attribute)
}

/// Name ohne Vorsilbe: `svg:script` → `script`.
fn lokal(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}

/// Ein Wert, der als `javascript:` (oder `vbscript:`) ausgeführt würde; Browser
/// übergehen Leerzeichen und Steuerzeichen darin, also hier auch.
fn ist_skript_adresse(wert: &str) -> bool {
    let w: String = wert.chars().filter(|c| !c.is_whitespace() && !c.is_control()).collect::<String>().to_ascii_lowercase();
    w.starts_with("javascript:") || w.starts_with("vbscript:")
}

/// Attribute, deren Wert eine Adresse ist.
const ADRESS_ATTRIBUTE: [&str; 5] = ["href", "xlink:href", "src", "action", "formaction"];

/// Steht in der Zeile ein `url(…)` nach außen, auch mit Leerzeichen (`url( "https://…")`)?
fn url_nach_aussen(zeile: &str) -> bool {
    let z = zeile.to_ascii_lowercase();
    z.match_indices("url(").any(|(i, _)| {
        let rest = z[i + 4..].trim_start();
        let rest = rest.strip_prefix(['"', '\'']).unwrap_or(rest).trim_start();
        nach_aussen(rest)
    })
}

/// Zeilen (ab 1) mit einem `url(…)` nach außen.
fn zeilen_mit_url(text: &str) -> impl Iterator<Item = usize> + '_ {
    text.lines().enumerate().filter(|(_, l)| url_nach_aussen(l)).map(|(i, _)| i + 1)
}

/// Regeln für SVG und HTML gleich: keine `javascript:`-Adressen, keine
/// Zeichencodes (`&#…`) in Adressen, keine Animation von `on…` oder `href`.
fn gemeinsam_pruefen(datei: &str, attribute: &[Attribut<'_>], befunde: &mut Vec<Befund>) {
    for a in attribute {
        if ist_skript_adresse(a.wert) {
            befunde.push(Befund::fehler(
                datei,
                Some(a.zeile),
                format!("Bei `{}=` steht eine `javascript:`-Adresse. Skripte in Adressen sind nicht erlaubt.", a.name),
            ));
        } else if ADRESS_ATTRIBUTE.contains(&a.name.as_str()) && a.wert.contains("&#") {
            befunde.push(Befund::fehler(
                datei,
                Some(a.zeile),
                format!("Bei `{}=` steht ein Zeichencode (`&#…`). In Adressen bitte nur einfache Zeichen schreiben.", a.name),
            ));
        }
        if a.name == "attributename" {
            let ziel = a.wert.trim().to_ascii_lowercase();
            if ziel.starts_with("on") || lokal(&ziel) == "href" {
                befunde.push(Befund::fehler(
                    datei,
                    Some(a.zeile),
                    format!("Eine Animation ändert `{}`. Animationen dürfen keine `on…`-Attribute und kein `href` ändern.", a.wert.trim()),
                ));
            }
        }
    }
}

/// Zeigt ein Wert nach außen (`http:`, `https:`, `//…`)?
fn nach_aussen(wert: &str) -> bool {
    let w = wert.trim().to_ascii_lowercase();
    w.starts_with("http:") || w.starts_with("https:") || w.starts_with("//") || w.starts_with("ftp:")
}

/// Kürzt einen Wert für den Satz.
fn kurz(wert: &str) -> String {
    let w = wert.trim();
    if w.chars().count() > 60 { format!("{}…", w.chars().take(60).collect::<String>()) } else { w.to_string() }
}

/// Erste Stelle je Zeile, an der eines der `muster` vorkommt (Text klein geschrieben).
fn zeilen_mit<'a>(text: &'a str, muster: &[&str]) -> impl Iterator<Item = usize> + 'a {
    let muster: Vec<String> = muster.iter().map(|m| m.to_string()).collect();
    text.lines()
        .enumerate()
        .filter(move |(_, l)| {
            let l = l.to_ascii_lowercase();
            muster.iter().any(|m| l.contains(m.as_str()))
        })
        .map(|(i, _)| i + 1)
}

// ----- SVG -----

/// SVG: kein `<script>`, keine `on…`-Attribute, kein `<foreignObject>` (auch
/// nicht mit Vorsilbe wie `<svg:script>`), keine eigene Dokumenttyp-Angabe,
/// keine Verweise nach außen, keine eingebetteten Schriften.
pub fn svg_pruefen(datei: &str, text: &str) -> Vec<Befund> {
    let mut befunde = Vec::new();
    let (elemente, attribute) = zerlegen(text);
    let mut schrift_zeilen = std::collections::BTreeSet::new();
    for e in &elemente {
        match lokal(&e.name) {
            "script" => befunde.push(Befund::fehler(
                datei,
                Some(e.zeile),
                format!("Das SVG enthält `<{}>`. Skripte sind in SVG nicht erlaubt; Verhalten bitte in Svelte bauen.", e.name),
            )),
            "foreignobject" => {
                befunde.push(Befund::fehler(datei, Some(e.zeile), "Das SVG enthält `<foreignObject>`. Fremde Inhalte (HTML) im SVG sind nicht erlaubt."))
            }
            "font" | "font-face" => {
                schrift_zeilen.insert(e.zeile);
            }
            _ => {}
        }
    }
    schrift_zeilen.extend(zeilen_mit(text, &["@font-face"]));
    for z in schrift_zeilen {
        befunde.push(Befund::fehler(
            datei,
            Some(z),
            "Das SVG bettet eine Schrift ein (`@font-face` oder `<font>`). Schriften sind nicht erlaubt; Text nimmt die Schrift der Seite.",
        ));
    }
    let klein = text.to_ascii_lowercase();
    let doctype_mit_klammer = klein.match_indices("<!doctype").find(|(i, _)| {
        let ende = klein[*i..].find('>').map(|k| i + k).unwrap_or(klein.len());
        klein[*i..ende].contains('[')
    });
    if let Some((i, _)) = doctype_mit_klammer {
        befunde.push(Befund::fehler(
            datei,
            Some(zeile_bei(text, i)),
            "Das SVG hat eine eigene Dokumenttyp-Angabe (`<!DOCTYPE … [`). Das ist nicht erlaubt; bitte `<!DOCTYPE …>` weglassen.",
        ));
    } else {
        for z in zeilen_mit(text, &["<!entity"]) {
            befunde.push(Befund::fehler(
                datei,
                Some(z),
                "Das SVG legt mit `<!ENTITY` eigene Kürzel an. Das ist nicht erlaubt; bitte den Text direkt schreiben.",
            ));
        }
    }
    for z in zeilen_mit(text, &["@import"]) {
        befunde.push(Befund::fehler(datei, Some(z), "Das SVG holt mit `@import` weitere Stile. Bitte alle Stile direkt ins SVG schreiben."));
    }
    for z in zeilen_mit_url(text) {
        befunde.push(Befund::fehler(datei, Some(z), "Das SVG verweist mit `url(http…)` nach außen. Alles muss im SVG selbst stehen."));
    }
    gemeinsam_pruefen(datei, &attribute, &mut befunde);
    for a in attribute {
        let ereignis = a.name.len() > 2 && a.name.starts_with("on") && a.name.chars().all(|c| c.is_ascii_alphabetic());
        if ereignis {
            befunde.push(Befund::fehler(
                datei,
                Some(a.zeile),
                format!("Das SVG enthält `{}=`. Ereignis-Attribute (`on…=`) sind in SVG nicht erlaubt.", a.name),
            ));
        } else if (a.name == "href" || a.name == "xlink:href") && nach_aussen(a.wert) {
            befunde.push(Befund::fehler(
                datei,
                Some(a.zeile),
                format!(
                    "Das SVG verweist nach außen (`{}=\"{}\"`). Alles muss im SVG selbst stehen; Bilder bitte als eigene Datei in `assets/`.",
                    a.name,
                    kurz(a.wert)
                ),
            ));
        }
    }
    befunde.sort_by_key(|b| b.zeile);
    befunde
}

// ----- HTML (Vorschau) -----

/// Vorschau-HTML: alles eingebettet, nichts aus dem Netz, keine Formulare.
/// Links nach außen (`<a href="https://…">`) sind erlaubt.
pub fn html_pruefen(datei: &str, text: &str) -> Vec<Befund> {
    let mut befunde = Vec::new();
    for z in zeilen_mit(text, &["@import"]) {
        befunde.push(Befund::fehler(
            datei,
            Some(z),
            "Die Vorschau holt mit `@import` weitere Stile. Bitte alle Stile direkt in `<style>` in die Datei schreiben.",
        ));
    }
    for z in zeilen_mit_url(text) {
        befunde.push(Befund::fehler(
            datei,
            Some(z),
            "Die Vorschau lädt mit `url(http…)` etwas aus dem Netz. Bilder bitte als `data:`-Adresse einbetten oder aus `assets/` nehmen.",
        ));
    }
    let (elemente, attribute) = zerlegen(text);
    gemeinsam_pruefen(datei, &attribute, &mut befunde);
    for e in elemente.iter().filter(|e| e.name == "form") {
        befunde.push(Befund::fehler(datei, Some(e.zeile), "Die Vorschau enthält ein Formular (`<form>`). Formulare sind in der Vorschau nicht erlaubt."));
    }
    for a in attribute {
        let laedt = match a.name.as_str() {
            "src" | "poster" | "data" => nach_aussen(a.wert),
            "srcset" => a.wert.split(',').any(nach_aussen),
            "href" => a.element != "a" && a.element != "area" && nach_aussen(a.wert),
            "xlink:href" => nach_aussen(a.wert),
            _ => false,
        };
        if laedt {
            befunde.push(Befund::fehler(
                datei,
                Some(a.zeile),
                format!(
                    "Die Vorschau lädt `<{} {}=\"{}\">` aus dem Netz. Alles muss in der Datei selbst stehen; der Server lässt nichts aus dem Netz zu.",
                    a.element,
                    a.name,
                    kurz(a.wert)
                ),
            ));
        }
    }
    befunde.sort_by_key(|b| b.zeile);
    befunde
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn namen() {
        assert!(ist_asset_name("plan-2.svg"));
        assert!(ist_asset_name("a_b.c.png"));
        assert!(!ist_asset_name("Plan.svg"));
        assert!(!ist_asset_name(".versteckt"));
        assert!(!ist_asset_name("mit leer.png"));
        assert!(!ist_asset_name("ä.png"));
    }

    #[test]
    fn windows_geraetenamen() {
        for name in ["con", "nul", "prn", "aux", "com1", "com9", "lpt1", "lpt9", "CON", "Nul", "con.svg", "nul.txt", "lpt3.tar.gz", "COM7.PNG"] {
            assert!(ist_geraetename(name), "{name}");
        }
        for name in ["console.svg", "com0", "com10", "lpt", "comx", "null.txt", "icon.svg", "a-con.svg", "prnt.png", ""] {
            assert!(!ist_geraetename(name), "{name}");
        }
        assert!(!ist_asset_name("con.svg"));
        assert!(!ist_asset_name("nul"));
        assert!(!ist_asset_name("com1.png"));
        assert!(ist_asset_name("console.svg"));
    }

    #[test]
    fn grenzen_je_endung() {
        assert_eq!(groesse_grenze("svg"), 500 * 1024);
        assert_eq!(groesse_grenze("SVG"), 500 * 1024);
        assert_eq!(groesse_grenze("png"), 2 * 1024 * 1024);
        assert_eq!(groesse_grenze("html"), 2 * 1024 * 1024);
        assert_eq!(groesse_grenze("htm"), 2 * 1024 * 1024);
        assert_eq!(groesse_grenze("pdf"), 2 * 1024 * 1024);
        assert_eq!(groesse_grenze(""), 2 * 1024 * 1024);
        assert_eq!(grenze_wort(SVG_HOECHSTENS), "500 KB");
        assert_eq!(grenze_wort(HTML_HOECHSTENS), "2 MB");
    }

    #[test]
    fn zu_grosse_vorschau_und_pdf() {
        let dir = std::env::temp_dir().join(format!("dashboard-assets-html-{}", std::process::id()));
        let assets = dir.join("gross").join("assets");
        std::fs::create_dir_all(&assets).unwrap();
        let mut html = String::from("<!doctype html>\n<p>x</p>\n");
        while html.len() <= HTML_HOECHSTENS as usize {
            html.push_str("<p>Füllzeile, damit die Datei groß wird.</p>\n");
        }
        std::fs::write(assets.join("vorschau.html"), &html).unwrap();
        std::fs::write(assets.join("plan.pdf"), vec![0u8; SONST_HOECHSTENS as usize + 1]).unwrap();
        std::fs::write(assets.join("klein.pdf"), b"%PDF-1.4\n").unwrap();
        let b = pruefe_assets(&dir, "gross");
        std::fs::remove_dir_all(&dir).unwrap();
        let saetze: Vec<String> = b.iter().map(|b| b.to_string()).collect();
        assert_eq!(saetze.len(), 2, "{saetze:#?}");
        assert!(saetze[0].starts_with("projects/gross/assets/plan.pdf: Die Datei ist 2,0 MB groß; erlaubt sind höchstens 2 MB."), "{saetze:#?}");
        assert!(saetze[1].starts_with("projects/gross/assets/vorschau.html: Die Vorschau ist 2,0 MB groß; erlaubt sind höchstens 2 MB."), "{saetze:#?}");
    }

    #[test]
    fn svg_regeln() {
        let gut =
            "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\">\n<use href=\"#a\"/><text>online = gut</text>\n</svg>";
        assert!(svg_pruefen("x.svg", gut).is_empty(), "{:#?}", svg_pruefen("x.svg", gut));
        let b = svg_pruefen(
            "x.svg",
            "<svg>\n<style>@font-face { src: url(https://f.example/x.woff) }</style>\n<foreignObject/>\n<a xlink:href='//example.org' onMouseOver=\"x()\"/>\n</svg>",
        );
        let zeilen: Vec<Option<u32>> = b.iter().map(|b| b.zeile).collect();
        assert_eq!(zeilen, [Some(2), Some(2), Some(3), Some(4), Some(4)], "{b:#?}");
        assert!(b.iter().any(|b| b.text.contains("`onmouseover=`")), "{b:#?}");
    }

    #[test]
    fn html_regeln() {
        let gut = "<!doctype html>\n<p onclick=\"a()\"><a href=\"https://example.org\">x</a><img src=\"data:image/png;base64,AA\"><img src=\"bild.png\"></p>\n<script>if (a<b) { x = '<form>'; src = \"https://x\" }</script>";
        assert!(html_pruefen("v.html", gut).is_empty(), "{:#?}", html_pruefen("v.html", gut));
        let b = html_pruefen(
            "v.html",
            "<link rel=\"stylesheet\" href=\"https://x/a.css\">\n<img srcset=\"a.png 1x, https://x/b.png 2x\">\n<div style=\"background: url('http://x/y.png')\"></div>",
        );
        let zeilen: Vec<Option<u32>> = b.iter().map(|b| b.zeile).collect();
        assert_eq!(zeilen, [Some(1), Some(2), Some(3)], "{b:#?}");
    }

    #[test]
    fn versteckte_skripte_und_verweise() {
        // Vorsilben, Dokumenttyp, Entities.
        let b = svg_pruefen("x.svg", "<svg>\n<svg:script>a()</svg:script>\n<X:foreignObject/>\n</svg>");
        assert_eq!(b.iter().map(|b| b.zeile).collect::<Vec<_>>(), [Some(2), Some(3)], "{b:#?}");
        assert!(svg_pruefen("x.svg", "<!DOCTYPE svg PUBLIC \"-//W3C//DTD SVG 1.1//EN\" \"x.dtd\">\n<svg/>").is_empty());
        let b = svg_pruefen("x.svg", "<!DOCTYPE svg [\n<!ENTITY a \"b\">\n]>\n<svg/>");
        assert_eq!(b.len(), 1, "{b:#?}");
        assert_eq!(b[0].zeile, Some(1));
        let b = svg_pruefen("x.svg", "<svg>\n<!ENTITY a \"b\">\n</svg>");
        assert_eq!((b.len(), b[0].zeile), (1, Some(2)), "{b:#?}");
        // `url(` mit Leerzeichen und `@import` auch im SVG.
        assert!(url_nach_aussen("fill: url(  'https://x/y')"));
        assert!(url_nach_aussen("fill: URL( //x/y)"));
        assert!(!url_nach_aussen("fill: url( #verlauf)"));
        let b = svg_pruefen("x.svg", "<svg><style>@import 'a.css';</style></svg>");
        assert!(b.len() == 1 && b[0].text.contains("@import"), "{b:#?}");
        // javascript: auch mit Großbuchstaben, Tab und Leerzeichen; Zeichencodes in Adressen.
        assert!(ist_skript_adresse(" JaVa\tScript:alert(1)"));
        assert!(!ist_skript_adresse("#javascript"));
        let b = svg_pruefen("x.svg", "<svg>\n<a href=\"java&#115;cript:x\"/>\n<image src=\"&#x6A;s\"/>\n<use href=\"#a\"/>\n</svg>");
        assert_eq!(b.iter().map(|b| b.zeile).collect::<Vec<_>>(), [Some(2), Some(3)], "{b:#?}");
        // Animationen dürfen `on…` und `href` nicht ändern, andere Attribute schon.
        let b = svg_pruefen(
            "x.svg",
            "<svg>\n<set attributeName=\"onload\"/>\n<animate attributeName=\"xlink:href\"/>\n<animate attributeName=\"x\" from=\"0\" to=\"5\"/>\n</svg>",
        );
        assert_eq!(b.iter().map(|b| b.zeile).collect::<Vec<_>>(), [Some(2), Some(3)], "{b:#?}");
        // Dieselben Regeln in der Vorschau.
        let b = html_pruefen(
            "v.html",
            "<a href=\"javascript:alert(1)\">x</a>\n<a href=\"&#104;ttps://x\">y</a>\n<div style=\"background: url( 'http://x/y.png')\"></div>",
        );
        assert_eq!(b.iter().map(|b| b.zeile).collect::<Vec<_>>(), [Some(1), Some(2), Some(3)], "{b:#?}");
    }

    #[test]
    fn zu_grosses_svg_und_bild() {
        let dir = std::env::temp_dir().join(format!("dashboard-assets-test-{}", std::process::id()));
        let assets = dir.join("gross").join("assets");
        std::fs::create_dir_all(&assets).unwrap();
        let mut svg = String::from("<svg xmlns=\"http://www.w3.org/2000/svg\">\n");
        while svg.len() <= SVG_HOECHSTENS as usize {
            svg.push_str("<rect width=\"1\" height=\"1\"/>\n");
        }
        svg.push_str("<script>spaet()</script></svg>\n");
        std::fs::write(assets.join("riesig.svg"), &svg).unwrap();
        std::fs::write(assets.join("foto.png"), vec![0u8; BILD_HOECHSTENS as usize + 1]).unwrap();
        let b = pruefe_assets(&dir, "gross");
        std::fs::remove_dir_all(&dir).unwrap();
        let saetze: Vec<String> = b.iter().map(|b| b.to_string()).collect();
        assert_eq!(saetze.len(), 2, "{saetze:#?}");
        assert_eq!(saetze[0], "projects/gross/assets/foto.png: Das Bild ist 2,0 MB groß; erlaubt sind höchstens 2 MB. Bitte verkleinern, etwa als WebP.");
        assert!(saetze[1].starts_with("projects/gross/assets/riesig.svg: Das SVG ist 50"), "{saetze:#?}");
        assert!(saetze[1].contains("höchstens 500 KB"), "{saetze:#?}");
    }
}
