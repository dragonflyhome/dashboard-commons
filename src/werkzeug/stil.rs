//! Stil-Prüfung der Projektseiten mit Stylelint (Etappe 3b, Teil C;
//! Entscheidung `plan-3b`, Antwort 3).
//!
//! Die `<style>`-Blöcke (und `style="…"`) in den `*.svelte` der Projekte
//! bekommen dieselben Regeln wie `web/src` (`web/stylelint.config.js`):
//! Farben, Schatten und Schriften nur als Token, kein `!important`, kein
//! `:global`, nichts aus dem Netz. Rust ruft dafür Node mit
//! `web/scripts/stil-projekte.mjs` auf (die Dateien auf stdin, je Fund eine
//! Zeile JSON zurück); nichts Neues ist installiert, Stylelint liegt ohnehin
//! in `web/node_modules`. Abschalten per Kommentar (`stylelint-disable`)
//! wirkt in Projekten nicht.
//!
//! Läuft in `dashboard pruefe <slug>` (wie svelte-check, `--ohne-stil` lässt
//! es weg), in `pruefe --alle` (ein Lauf für alle Projekte, unter einer
//! Sekunde) und in der Bauliste mit `--svelte`, also im Bau des Boten (neben
//! svelte-check in einem eigenen Faden, kostet darum kaum Zeit). Fehlen Node
//! oder Stylelint, läuft es zu lange oder nicht durch: ein Hinweis, dass die
//! Stil-Prüfung übersprungen ist; die anderen Prüfungen und der Bau laufen
//! weiter. Seit 3b-2 nimmt ein Absturz nicht mehr alle Projekte aus der
//! Prüfung: Das Skript fängt ihn je Datei ab, und läuft der gemeinsame Lauf
//! trotzdem nicht durch, läuft er je Projekt einzeln ([`pruefen_mit`]).
//!
//! Bis [`STIL_ALS_FEHLER`] umgestellt ist, sind alle Funde Hinweise: Sie
//! machen kein Projekt rot und brechen keinen Bau.

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use crate::dienst::programme::{self, Werkzeug};
use crate::inhalt::pruefung::Befund;
use crate::inhalt::seite;
use crate::werkzeug::scanner;

/// Stil-Funde als Fehler statt als Hinweis.
///
/// Heute `false`: Funde sind `HINWEIS` und brechen nichts. Auf `true` geht
/// das, sobald die Projekte ihre Zeilen erledigt haben, in einem kleinen
/// eigenen Commit mit Abnahme. Dann ist jeder Fund ein `FEHLER`,
/// und ein Projekt mit Fund fällt aus der Bauliste (wird nicht gebaut).
pub const STIL_ALS_FEHLER: bool = false;

/// Steht bis zur Umstellung hinter jedem Fund (ohne Datum).
pub const SATZ_BIS_UMSTELLUNG: &str = "Wird ein Fehler, sobald STIL_ALS_FEHLER umgestellt ist.";

/// So lange darf die Stil-Prüfung laufen (alle Projekte zusammen brauchen
/// heute unter einer Sekunde).
pub const STIL_ZEITLIMIT: Duration = Duration::from_secs(60);

/// Das Skript in `web/`.
pub const STIL_SKRIPT: &str = "scripts/stil-projekte.mjs";

/// Stil-Prüfung für ein Projekt (`slug`) oder alle Projekte in `dir`.
pub fn pruefen(dir: &Path, repo: &Path, slug: Option<&str>) -> Vec<Befund> {
    match slug {
        Some(s) => pruefen_fuer(dir, repo, &[s.to_string()], &format!("projects/{s}")),
        None => {
            let Ok(eintraege) = std::fs::read_dir(dir) else { return Vec::new() };
            let mut slugs: Vec<String> =
                eintraege.filter_map(Result::ok).filter_map(|e| e.file_name().into_string().ok()).filter(|s| seite::is_valid_slug(s)).collect();
            slugs.sort();
            pruefen_fuer(dir, repo, &slugs, "projects")
        }
    }
}

/// Stil-Prüfung für die Projekte `slugs`; `ort` steht bei einem Hinweis, dass
/// sie übersprungen ist.
pub fn pruefen_fuer(dir: &Path, repo: &Path, slugs: &[String], ort: &str) -> Vec<Befund> {
    let repo = std::path::absolute(repo).unwrap_or_else(|_| repo.to_path_buf());
    let web = repo.join("web");
    pruefen_mit(dir, slugs, ort, &|eingabe| node_lauf(&web, eingabe))
}

/// Ausgabe, Fehlerausgabe und Rückgabewert eines Laufs von Stylelint; `Err`
/// mit dem Satz, warum es gar nicht lief (fehlt, startet nicht, zu lange).
pub type Lauf = Result<(String, String, Option<i32>), String>;

/// Ein Lauf von `web/scripts/stil-projekte.mjs` mit Node; `eingabe` ist je
/// Zeile ein Pfad.
pub fn node_lauf(web: &Path, eingabe: String) -> Lauf {
    if !web.join("node_modules").join("stylelint").is_dir() || !web.join("node_modules").join("postcss-html").is_dir() {
        return Err("Stylelint ist nicht da (web/node_modules fehlt)".into());
    }
    if !web.join(STIL_SKRIPT).is_file() {
        return Err(format!("web/{STIL_SKRIPT} fehlt"));
    }
    // Niedrige Priorität wie die anderen Werkzeuge: Der Bau stört die Arbeit nicht.
    let mut cmd = programme::befehl_niedrig(Werkzeug::Node)?;
    cmd.current_dir(web).arg(STIL_SKRIPT).stdout(Stdio::piped()).stderr(Stdio::piped());
    scanner::mit_zeitlimit(cmd, STIL_ZEITLIMIT, "Die Stil-Prüfung", Some(eingabe))
}

/// Wie [`pruefen_fuer`], mit `lauf` statt Node (für Tests).
///
/// Etappe 3b-2 (Auflage aus der Abnahme 3b): Läuft Stylelint für alle
/// Projekte zusammen nicht durch (Absturz ohne Zeile `FERTIG`), läuft es
/// danach je Projekt einzeln. So verliert nur das Projekt seine Stil-Befunde,
/// an dem es liegt; die anderen bekommen ihre wie sonst. Fehlen Node oder
/// Stylelint oder dauert es zu lange, hilft ein Lauf je Projekt nicht: dann
/// bleibt es bei einem Hinweis (der Sammellauf des Baus prüft nach einem
/// Zeitlimit trotzdem je Projekt, `projektpruefung::stil_gruppe`).Einen Absturz bei einer einzelnen Datei fängt
/// schon das Skript ab (Regel `absturz`, ebenso `lesen`: Hinweis bei dieser Datei, ihr
/// Projekt gilt als übersprungen, [`ausgabe_lesen`]).
pub fn pruefen_mit(dir: &Path, slugs: &[String], ort: &str, lauf: &dyn Fn(String) -> Lauf) -> Vec<Befund> {
    // (Pfad relativ zum Repo, Pfad auf der Platte), in der Reihenfolge der Eingabe.
    let dateien: Vec<(String, std::path::PathBuf)> = slugs
        .iter()
        .flat_map(|slug| {
            scanner::svelte_dateien(dir, slug)
                .into_iter()
                .map(move |(rel, pfad)| (format!("projects/{slug}/{rel}"), std::path::absolute(&pfad).unwrap_or(pfad)))
        })
        .collect();
    if dateien.is_empty() {
        return Vec::new();
    }
    // Zweig A: derselbe Satz wie bei den anderen Werkzeugen, damit der Bau
    // das Überspringen erkennt (`projektpruefung::ist_uebersprungen`).
    let uebersprungen = |grund: &str| crate::werkzeug::projektpruefung::uebersprungen(ort, "Die Stil-Prüfung (Stylelint)", grund);
    let eingabe: String = dateien.iter().map(|(_, p)| format!("{}\n", p.display())).collect();
    let (aus, fehl, code) = match lauf(eingabe) {
        Ok(x) => x,
        Err(satz) => return uebersprungen(&satz),
    };
    if let Some(befunde) = ausgabe_lesen(&aus, &dateien) {
        return befunde;
    }
    // Abgestürzt: je Projekt einzeln (nur Projekte mit Svelte-Dateien).
    let mit_dateien: Vec<&String> = slugs.iter().filter(|s| dateien.iter().any(|(rel, _)| rel.starts_with(&format!("projects/{s}/")))).collect();
    if mit_dateien.len() > 1 {
        return mit_dateien.into_iter().flat_map(|s| pruefen_mit(dir, std::slice::from_ref(s), &format!("projects/{s}"), lauf)).collect();
    }
    let grund = fehl.lines().chain(aus.lines()).map(str::trim).find(|l| !l.is_empty()).unwrap_or("ohne Ausgabe");
    let code = code.map_or_else(|| "ohne Rückgabewert".to_string(), |c| format!("Rückgabe {c}"));
    let grund: String = grund.chars().take(200).collect();
    uebersprungen(&format!("Stylelint lief nicht durch ({code}): {grund}"))
}

/// Die Ausgabe des Skripts als Befunde; `None`, wenn die Zeile `FERTIG` fehlt.
pub fn ausgabe_lesen(aus: &str, dateien: &[(String, std::path::PathBuf)]) -> Option<Vec<Befund>> {
    let mut befunde = Vec::new();
    let mut fertig = false;
    for zeile in aus.lines().map(str::trim) {
        if zeile.starts_with("FERTIG ") {
            fertig = true;
            continue;
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(zeile) else { continue };
        let Some((datei, _)) = v["nr"].as_u64().and_then(|n| dateien.get(n as usize)) else { continue };
        let nr = v["zeile"].as_u64().map(|z| z as usize);
        let regel = v["regel"].as_str().unwrap_or("");
        let satz = satz(regel, v["text"].as_str().unwrap_or(""));
        let grund = match regel {
            "absturz" => format!("Stylelint ist bei {datei} abgestürzt"),
            "lesen" => format!("{datei} ließ sich nicht lesen"),
            _ => {
                befunde.push(befund(datei.clone(), nr, satz));
                continue;
            }
        };
        // Ein Absturz oder eine unlesbare Datei ist kein Stil-Fund: ein Hinweis ohne
        // Frist, und das Projekt gilt als nicht geprüft (sonst käme es ungeprüft grün in den Bau).
        befunde.push(Befund::hinweis(datei.clone(), nr, satz));
        let ort = datei.splitn(3, '/').take(2).collect::<Vec<_>>().join("/");
        if !befunde.iter().any(|b| b.datei == ort && crate::werkzeug::projektpruefung::ist_uebersprungen(b)) {
            befunde.extend(crate::werkzeug::projektpruefung::uebersprungen(&ort, "Die Stil-Prüfung (Stylelint)", &grund));
        }
    }
    fertig.then_some(befunde)
}

/// Ein Fund als Hinweis oder (nach der Umstellung) als Fehler.
fn befund(datei: String, zeile: Option<usize>, satz: String) -> Befund {
    if STIL_ALS_FEHLER { Befund::fehler(datei, zeile, satz) } else { Befund::hinweis(datei, zeile, format!("{satz} {SATZ_BIS_UMSTELLUNG}")) }
}

/// Der erste Text in „"…"“ in einer Meldung von Stylelint.
fn zitat(text: &str, nr: usize) -> Option<&str> {
    text.split('"').nth(2 * nr + 1)
}

/// Satz in einfachem Deutsch zu einer Regel von Stylelint (dessen Meldungen
/// sind englisch); die Regel steht am Ende in Klammern.
pub fn satz(regel: &str, text: &str) -> String {
    let token = "Farben, Schatten und Schriften nur als Token (var(--…)); erlaubt sind daneben transparent, inherit und currentColor.";
    let kern = match regel {
        "scale-unlimited/declaration-strict-value" => {
            let wert = zitat(text, 0).unwrap_or("?");
            let eigenschaft = zitat(text, 1).unwrap_or("?");
            if eigenschaft == "font-size" || eigenschaft == "font" {
                format!(
                    "Stil: Die Schriftgröße „{wert}“ ist frei. Schriftgrößen nur als Token: var(--text-sm) und die anderen --text-… (rem), relativ zum umgebenden Text var(--text-em-sm) (0.85em), var(--text-em-md) (0.9em) oder var(--text-em-lg) (1.05em)."
                )
            } else {
                format!("Stil: Bei `{eigenschaft}` steht der freie Wert „{wert}“. {token}")
            }
        }
        "color-no-hex" | "color-named" | "function-disallowed-list" => {
            format!("Stil: Freie Farbe „{}“. {token}", zitat(text, 0).unwrap_or("?"))
        }
        "declaration-no-important" | "keyframe-declaration-no-important" => {
            "Stil: `!important` ist verboten. Der Stil gilt nur in der eigenen Datei und muss nichts überstimmen.".to_string()
        }
        "selector-pseudo-class-disallowed-list" => "Stil: `:global` ist verboten. Der Stil gilt nur in der eigenen Datei.".to_string(),
        "at-rule-disallowed-list" => "Stil: `@import` ist verboten. Eigenes CSS steht nur im <style> der eigenen Datei.".to_string(),
        "function-url-scheme-disallowed-list" | "function-url-no-scheme-relative" => {
            "Stil: `url(…)` aus dem Netz ist verboten. Bilder und Schriften gehören nach assets/.".to_string()
        }
        "CssSyntaxError" => format!("Stil: Stylelint kann den Stil nicht lesen ({}). Klammern und Semikolons an der Stelle prüfen.", ohne_regel(text)),
        "lesen" => format!("Stil: Die Datei lässt sich für die Stil-Prüfung nicht lesen ({}).", text.trim()),
        "absturz" => format!(
            "Stil: Stylelint ist bei dieser Datei abgestürzt ({}), ihr Stil ist darum nicht geprüft. Die anderen Dateien und Projekte sind geprüft. Meist hilft es, den <style>-Block auf Tippfehler zu prüfen; sonst dem Betreuer des Dashboards Bescheid geben.",
            text.trim().chars().take(200).collect::<String>()
        ),
        _ => format!("Stil: Stylelint meldet: {}.", ohne_regel(text)),
    };
    if regel.is_empty() || regel == "lesen" || regel == "absturz" { kern } else { format!("{kern} (Regel {regel})") }
}

/// Die Meldung ohne die Regel in Klammern am Ende.
fn ohne_regel(text: &str) -> &str {
    let t = text.trim();
    match t.rfind(" (") {
        Some(i) if t.ends_with(')') => &t[..i],
        _ => t,
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::inhalt::pruefung::Schwere;

    fn dateien() -> Vec<(String, PathBuf)> {
        vec![("projects/a/Seite.svelte".into(), PathBuf::from("C:/x/a/Seite.svelte")), ("projects/b/Seite.svelte".into(), PathBuf::from("C:/x/b/Seite.svelte"))]
    }

    #[test]
    fn ausgabe_wird_zu_befunden() {
        let aus = concat!(
            r#"{"nr":1,"zeile":118,"regel":"scale-unlimited/declaration-strict-value","text":"Expected variable, function or keyword for \"1.05em\" of \"font-size\" (scale-unlimited/declaration-strict-value)"}"#,
            "\n",
            r##"{"nr":0,"zeile":4,"regel":"color-no-hex","text":"Disallowed hex color \"#fff\" (color-no-hex)"}"##,
            "\nkein JSON\nFERTIG 2 2\n"
        );
        let b = ausgabe_lesen(aus, &dateien()).unwrap();
        assert_eq!(b.len(), 2, "{b:?}");
        assert_eq!((b[0].datei.as_str(), b[0].zeile), ("projects/b/Seite.svelte", Some(118)));
        assert!(b[0].text.starts_with("Stil: Die Schriftgröße „1.05em“ ist frei."), "{}", b[0].text);
        assert!(b[0].text.contains("var(--text-em-lg)") && b[0].text.contains("(Regel scale-unlimited/declaration-strict-value)"), "{}", b[0].text);
        assert!(b[1].text.starts_with("Stil: Freie Farbe „#fff“."), "{}", b[1].text);
        // Heute nur Hinweise, mit dem Satz zur Umstellung.
        let schwere = if STIL_ALS_FEHLER { Schwere::Fehler } else { Schwere::Hinweis };
        assert!(b.iter().all(|x| x.schwere == schwere));
        if !STIL_ALS_FEHLER {
            assert!(b[0].text.ends_with(" Wird ein Fehler, sobald STIL_ALS_FEHLER umgestellt ist."), "{}", b[0].text);
        }
    }

    #[test]
    fn ohne_fertig_keine_befunde() {
        assert!(ausgabe_lesen("", &dateien()).is_none());
        assert_eq!(ausgabe_lesen("FERTIG 2 0\n", &dateien()), Some(Vec::new()));
    }

    #[test]
    fn saetze_fuer_die_regeln() {
        assert!(satz("CssSyntaxError", "Unclosed block (CssSyntaxError)").contains("(Unclosed block)"));
        assert_eq!(
            satz("scale-unlimited/declaration-strict-value", r#"Expected variable, function or keyword for "red" of "color" (x)"#),
            "Stil: Bei `color` steht der freie Wert „red“. Farben, Schatten und Schriften nur als Token (var(--…)); erlaubt sind daneben transparent, inherit und currentColor. (Regel scale-unlimited/declaration-strict-value)"
        );
        assert!(satz("lesen", "ENOENT").ends_with("(ENOENT)."));
        assert!(satz("neue-regel", "Something (neue-regel)").starts_with("Stil: Stylelint meldet: Something."));
    }

    /// Echter Lauf mit Node und Stylelint aus `web/node_modules` dieses Repos,
    /// auf Projekten in einem Temp-Ordner. Abschalten per Kommentar wirkt
    /// nicht; ein Projekt ohne Fund bleibt ohne Befund.
    #[test]
    fn echter_lauf_mit_stylelint() {
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        if programme::pfad_von(Werkzeug::Node).is_none() || !repo.join("web/node_modules/stylelint").is_dir() {
            crate::ueberspringen("Node oder web/node_modules/stylelint fehlt.");
            return;
        }
        let dir = std::env::temp_dir().join(format!("dashboard-stil-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("bunt/seiten")).unwrap();
        std::fs::create_dir_all(dir.join("ruhig")).unwrap();
        std::fs::write(
            dir.join("bunt/Seite.svelte"),
            "<p>x</p>\n\n<style>\n\t/* stylelint-disable-next-line color-no-hex -- wirkt in Projekten nicht */\n\tp { color: #fff; }\n</style>\n",
        )
        .unwrap();
        std::fs::write(dir.join("bunt/seiten/mehr.svelte"), "<h3>x</h3>\n<style>\n\th3 { font-size: 1.05em; }\n</style>\n").unwrap();
        std::fs::write(dir.join("ruhig/Seite.svelte"), "<h3>x</h3>\n<style>\n\th3 { font-size: var(--text-em-lg); color: var(--text); }\n</style>\n").unwrap();
        let start = std::time::Instant::now();
        let b = pruefen(&dir, &repo, None);
        eprintln!("Stil-Prüfung, zwei Projekte: {:?}", start.elapsed());
        let _ = std::fs::remove_dir_all(&dir);
        let stellen: Vec<(&str, Option<u32>)> = b.iter().map(|x| (x.datei.as_str(), x.zeile)).collect();
        assert_eq!(
            stellen,
            [("projects/bunt/Seite.svelte", Some(5)), ("projects/bunt/Seite.svelte", Some(5)), ("projects/bunt/seiten/mehr.svelte", Some(3))],
            "{b:?}"
        );
        assert!(b[2].text.contains("„1.05em“"), "{}", b[2].text);
    }

    /// Etappe 3b-2: Ein Absturz von Stylelint bei einer Datei ist ein Hinweis
    /// nur für diese Datei, ohne Frist; die Befunde der anderen bleiben.
    /// Nachbesserung Zweig A: Dazu ist das Projekt der Datei „übersprungen“
    /// (einmal, auch bei zwei Abstürzen), damit der Bau es nicht grün nimmt.
    #[test]
    fn absturz_bei_einer_datei_nur_dort() {
        let aus = concat!(
            r#"{"nr":0,"zeile":null,"regel":"absturz","text":"Cannot read properties of undefined"}"#,
            "\n",
            r#"{"nr":0,"zeile":null,"regel":"absturz","text":"noch einmal"}"#,
            "\n",
            r##"{"nr":1,"zeile":4,"regel":"color-no-hex","text":"Disallowed hex color \"#fff\" (color-no-hex)"}"##,
            "\nFERTIG 2 2\n"
        );
        let b = ausgabe_lesen(aus, &dateien()).unwrap();
        assert_eq!(b.len(), 4, "{b:?}");
        assert_eq!((b[0].datei.as_str(), b[0].schwere, b[0].zeile), ("projects/a/Seite.svelte", Schwere::Hinweis, None));
        assert!(b[0].text.starts_with("Stil: Stylelint ist bei dieser Datei abgestürzt (Cannot read"), "{}", b[0].text);
        assert!(!b[0].text.contains(SATZ_BIS_UMSTELLUNG) && !b[0].text.contains("(Regel"), "{}", b[0].text);
        assert!(!crate::werkzeug::projektpruefung::ist_uebersprungen(&b[0]));
        assert_eq!(b[1].datei, "projects/a");
        assert!(crate::werkzeug::projektpruefung::ist_uebersprungen(&b[1]) && b[1].text.contains("projects/a/Seite.svelte abgestürzt"), "{}", b[1].text);
        assert_eq!(b.iter().filter(|x| crate::werkzeug::projektpruefung::ist_uebersprungen(x)).count(), 1, "{b:?}");
        assert_eq!(b[3].datei, "projects/b/Seite.svelte");
        // Ebenso eine Datei, die sich nicht lesen lässt.
        let b = ausgabe_lesen(concat!(r#"{"nr":1,"zeile":null,"regel":"lesen","text":"EBUSY"}"#, "\nFERTIG 2 2\n"), &dateien()).unwrap();
        assert_eq!((b.len(), b[0].schwere, b[1].datei.as_str()), (2, Schwere::Hinweis, "projects/b"), "{b:?}");
        assert!(
            crate::werkzeug::projektpruefung::ist_uebersprungen(&b[1]) && b[1].text.contains("projects/b/Seite.svelte ließ sich nicht lesen"),
            "{}",
            b[1].text
        );
    }

    /// Etappe 3b-2 (Auflage aus der Abnahme 3b): Stürzt Stylelint im Lauf für
    /// alle Projekte ab, läuft es je Projekt einzeln; nur das Projekt, an dem
    /// es liegt, bekommt den Hinweis „übersprungen“, die anderen ihre Befunde.
    /// Fehlt Stylelint oder dauert es zu lange, gibt es keinen Lauf je Projekt.
    #[test]
    fn absturz_nimmt_nicht_alle_projekte_aus_der_pruefung() {
        let dir = std::env::temp_dir().join(format!("dashboard-stil-absturz-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for slug in ["alpha", "kaputt", "zeta"] {
            std::fs::create_dir_all(dir.join(slug)).unwrap();
            std::fs::write(dir.join(slug).join("Seite.svelte"), "<p>x</p>\n").unwrap();
        }
        let slugs: Vec<String> = ["alpha", "kaputt", "zeta"].map(String::from).to_vec();
        let laeufe = std::cell::Cell::new(0);
        // Wie Node mit einem Absturz, sobald die Datei von „kaputt“ dabei ist;
        // sonst je Datei ein Fund mit freier Farbe.
        let lauf = |eingabe: String| -> Lauf {
            laeufe.set(laeufe.get() + 1);
            if eingabe.contains("kaputt") {
                return Ok((String::new(), "TypeError: Absturz in Stylelint\n    at x".into(), Some(1)));
            }
            let n = eingabe.lines().count();
            let mut aus: String = (0..n)
                .map(|nr| format!(r##"{{"nr":{nr},"zeile":3,"regel":"color-no-hex","text":"Disallowed hex color \"#fff\" (color-no-hex)"}}"##) + "\n")
                .collect();
            aus.push_str(&format!("FERTIG {n} {n}\n"));
            Ok((aus, String::new(), Some(0)))
        };
        let b = pruefen_mit(&dir, &slugs, "projects", &lauf);
        let stellen: Vec<(&str, Option<u32>)> = b.iter().map(|x| (x.datei.as_str(), x.zeile)).collect();
        assert_eq!(stellen, [("projects/alpha/Seite.svelte", Some(3)), ("projects/kaputt", None), ("projects/zeta/Seite.svelte", Some(3))], "{b:?}");
        assert!(b[1].text.contains("übersprungen") && b[1].text.contains("TypeError: Absturz in Stylelint"), "{}", b[1].text);
        assert!(crate::werkzeug::projektpruefung::ist_uebersprungen(&b[1]), "{}", b[1].text);
        assert!(
            !b.iter().filter(|x| x.datei != "projects/kaputt").any(crate::werkzeug::projektpruefung::ist_uebersprungen),
            "Stil-Funde sind kein Überspringen"
        );
        // Ein Lauf für alle, dann einer je Projekt.
        assert_eq!(laeufe.get(), 4);

        // Zu lange (oder Node fehlt): ein Hinweis, kein Lauf je Projekt.
        laeufe.set(0);
        let zu_lange = |_: String| -> Lauf {
            laeufe.set(laeufe.get() + 1);
            Err("Die Stil-Prüfung dauerte länger als 60 s".into())
        };
        let b = pruefen_mit(&dir, &slugs, "projects", &zu_lange);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(b.len(), 1, "{b:?}");
        assert_eq!(b[0].datei, "projects");
        assert_eq!(laeufe.get(), 1);
        assert!(crate::werkzeug::projektpruefung::ist_uebersprungen(&b[0]), "Zweig A: der Bau erkennt das Überspringen: {}", b[0].text);
    }

    /// Ohne web/node_modules wird die Prüfung übersprungen, mit einem Hinweis;
    /// ohne Svelte-Dateien läuft sie gar nicht.
    #[test]
    fn ohne_stylelint_ein_hinweis() {
        let repo = std::env::temp_dir().join(format!("dashboard-stil-ohne-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&repo);
        std::fs::create_dir_all(repo.join("projects/leer")).unwrap();
        std::fs::create_dir_all(repo.join("projects/x")).unwrap();
        std::fs::write(
            repo.join("projects/x/Seite.svelte"),
            "<p>x</p>
",
        )
        .unwrap();
        let leer = pruefen(&repo.join("projects"), &repo, Some("leer"));
        let b = pruefen(&repo.join("projects"), &repo, Some("x"));
        let _ = std::fs::remove_dir_all(&repo);
        assert_eq!(leer, Vec::new());
        assert_eq!(b.len(), 1, "{b:?}");
        assert_eq!(b[0].datei, "projects/x");
        assert_eq!(b[0].schwere, Schwere::Hinweis);
        assert!(b[0].text.starts_with("Die Stil-Prüfung (Stylelint) ist übersprungen: Stylelint ist nicht da"), "{}", b[0].text);
    }
}
