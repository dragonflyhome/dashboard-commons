//! Prüfung eines einzelnen Projektordners mit den Werkzeugen aus
//! `web/node_modules` (Etappe 4b, Zielbild „Prüfung je Projekt“): ESLint je
//! Projektordner mit `--cache`, svelte-check nur für dieses Projekt (eigene
//! kleine tsconfig) und Vitest für `*.spec.ts` im Projekt. Dazu die Prüfung
//! aus `inhalt::pruefung` (TOML, Assets, Scanner) und Stylelint
//! (`werkzeug::stil`): zusammen [`pruefe_voll`], so wie der Baumeister nach
//! einer Änderung prüft. Zweig B: Viele Projekte prüft [`pruefe_sammel`]
//! mit einem svelte-check über alle und Stylelint je Elternordner; ESLint
//! und Vitest laufen je Projekt in einer [`warteschlange`] mit Frist.
//!
//! Der Projektordner ist `<eltern>/<slug>`; mit mehreren Inhaltsquellen
//! (`quellen`) liegt er nicht unbedingt unter `projects/` im Repo. Befunde
//! nennen ihn trotzdem als `projects/<slug>/…`, wie überall.
//!
//! Fehlen Node oder die Pakete in `web/node_modules`: ein Hinweis, dass die
//! Prüfung übersprungen ist; der Rest läuft. Alle Kindprozesse ohne Fenster,
//! im Baumeister zusätzlich mit niedriger Priorität (`niedrig`).

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};
use std::process::Stdio;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use crate::dienst::programme::{self, Werkzeug};
use crate::inhalt::pruefung::{self, Befund};
use crate::werkzeug::{scanner, stil};

/// Zeitlimit für ESLint, Vitest und svelte-check je Projekt.
pub const ZEITLIMIT: Duration = Duration::from_secs(120);
/// Konfiguration von ESLint für Projekte, in `web/`.
pub const ESLINT_KONFIG: &str = "eslint.projekte.config.js";
/// Konfiguration von Vitest für Projekte, in `web/`.
pub const VITEST_KONFIG: &str = "vitest.projekte.config.ts";
/// Arbeitsordner des Baumeisters in `web/` (in .gitignore): tsconfig je
/// Prüfung, Ausgaben von Vitest.
pub const ARBEITSORDNER: &str = ".baumeister";
/// Cache von ESLint für die Projekte (`--cache`).
pub const ESLINT_CACHE: &str = "node_modules/.cache/eslint-projekte/";

/// Ein Befehl für Node: ohne Fenster, im Baumeister mit niedriger Priorität.
pub fn node(niedrig: bool) -> Result<std::process::Command, String> {
    if niedrig { programme::befehl_niedrig(Werkzeug::Node) } else { programme::befehl(Werkzeug::Node) }
}

/// Pfad zum Vergleichen: absolut, `..` aufgelöst, `/`, klein (Windows).
pub fn vergleichbar(p: &Path) -> String {
    let p = std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf());
    let mut teile: Vec<String> = Vec::new();
    for c in p.components() {
        match c {
            Component::ParentDir => {
                teile.pop();
            }
            Component::CurDir => {}
            Component::RootDir => {}
            Component::Prefix(v) => teile.push(v.as_os_str().to_string_lossy().trim_start_matches(r"\\?\").to_string()),
            Component::Normal(n) => teile.push(n.to_string_lossy().into_owned()),
        }
    }
    teile.join("/").to_lowercase()
}

/// `datei` relativ zu `ordner` mit `/`, wenn sie darin liegt.
pub fn relativ(datei: &Path, ordner: &Path) -> Option<String> {
    let (d, o) = (vergleichbar(datei), vergleichbar(ordner));
    let rest = d.strip_prefix(&o)?.strip_prefix('/')?;
    // Groß/Klein der Datei wie auf der Platte: aus dem nicht verkleinerten Pfad.
    let roh = std::path::absolute(datei).unwrap_or_else(|_| datei.to_path_buf()).to_string_lossy().replace('\\', "/");
    Some(roh.get(roh.len().saturating_sub(rest.len())..).unwrap_or(rest).to_string())
}

/// Das Ende jedes Satzes „… ist übersprungen“ ([`uebersprungen`]); daran
/// erkennt der Bau ein übersprungenes Werkzeug ([`ist_uebersprungen`]).
const UEBERSPRUNGEN_ENDE: &str = ". Die anderen Prüfungen liefen.";

/// Ein Hinweis bei `ort` (etwa `projects/<slug>`), dass ein Werkzeug
/// übersprungen ist (fehlt, startet nicht, bricht ab, Zeitlimit). Jedes
/// Werkzeug meldet Überspringen nur hierüber, auch Stylelint und Vitest.
pub fn uebersprungen(ort: &str, was: &str, grund: &str) -> Vec<Befund> {
    vec![Befund::hinweis(ort.to_string(), None, format!("{was} ist übersprungen: {}{UEBERSPRUNGEN_ENDE}", grund.trim().trim_end_matches('.')))]
}

/// Ist `b` der Hinweis, dass ein Werkzeug übersprungen ist? Dann gilt das
/// Projekt im Bau als nicht geprüft, nicht als grün (Zweig A, `bote::bau`).
// shortcut: erkennt Überspringen am Satz, neue Wege müssen uebersprungen() nutzen; aufrüsten zu eigenem Befund-Typ, wenn ein Werkzeug dazukommt.
pub fn ist_uebersprungen(b: &Befund) -> bool {
    !b.ist_fehler() && b.text.contains(" ist übersprungen: ") && b.text.ends_with(UEBERSPRUNGEN_ENDE)
}

/// Ist `b` übersprungen, weil die Frist des Baus um war (nicht wegen des
/// Projekts selbst)? Dann zählt es im Bau nicht als Prüfversuch.
pub fn frist_verpasst(b: &Befund) -> bool {
    ist_uebersprungen(b) && (b.text.contains(SATZ_FRIST) || b.text.contains(NACH_FRIST))
}

/// Das Ergebnis eines Werkzeug-Fadens: Brach er ab (Panik, etwa beim Lesen
/// der Ausgabe), ist das Werkzeug übersprungen, nie eine leere Liste (grün).
fn faden_ergebnis(r: std::thread::Result<Vec<Befund>>, slug: &str, was: &str) -> Vec<Befund> {
    r.unwrap_or_else(|_| uebersprungen(&format!("projects/{slug}"), was, "der Prüf-Faden brach ab"))
}

// ----- ESLint -----

/// ESLint für den Projektordner `<eltern>/<slug>` mit `--cache`. Ohne
/// `.svelte`- und `.ts`-Dateien läuft es nicht.
pub fn eslint(web: &Path, eltern: &Path, slug: &str, niedrig: bool) -> Vec<Befund> {
    if scanner::code_dateien(eltern, slug).is_empty() {
        return Vec::new();
    }
    let web = std::path::absolute(web).unwrap_or_else(|_| web.to_path_buf());
    let eltern = std::path::absolute(eltern).unwrap_or_else(|_| eltern.to_path_buf());
    let skript = web.join("node_modules").join("eslint").join("bin").join("eslint.js");
    if !skript.is_file() || !web.join(ESLINT_KONFIG).is_file() {
        return uebersprungen(&format!("projects/{slug}"), "ESLint", "ESLint ist nicht da (web/node_modules oder die Konfiguration fehlt)");
    }
    let mut cmd = match node(niedrig) {
        Ok(c) => c,
        Err(satz) => return uebersprungen(&format!("projects/{slug}"), "ESLint", &satz),
    };
    // Arbeitsordner ist der Ordner über dem Projekt: ESLint prüft nur Dateien
    // darunter, und die Konfiguration kommt mit --config aus web/.
    cmd.current_dir(&eltern)
        .arg(&skript)
        .arg("--config")
        .arg(web.join(ESLINT_KONFIG))
        .arg("--cache")
        .arg("--cache-location")
        .arg(web.join(ESLINT_CACHE))
        .args(["--format", "json", "--no-warn-ignored"])
        .arg(slug)
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    match scanner::mit_zeitlimit(cmd, ZEITLIMIT, "ESLint", None) {
        Err(satz) => uebersprungen(&format!("projects/{slug}"), "ESLint", &satz),
        Ok((aus, fehl, code)) => match eslint_lesen(&aus, &eltern.join(slug), slug) {
            Some(b) => b,
            None => {
                let grund = fehl.lines().chain(aus.lines()).map(str::trim).find(|l| !l.is_empty()).unwrap_or("ohne Ausgabe");
                let code = code.map_or_else(|| "ohne Rückgabewert".to_string(), |c| format!("Rückgabe {c}"));
                uebersprungen(
                    &format!("projects/{slug}"),
                    "ESLint",
                    &format!("ESLint lief nicht durch ({code}): {}", grund.chars().take(200).collect::<String>()),
                )
            }
        },
    }
}

/// Die JSON-Ausgabe von ESLint als Befunde (`None`: kein JSON). Fehler
/// (severity 2) werden Fehler, Warnungen Hinweise.
pub fn eslint_lesen(aus: &str, projekt: &Path, slug: &str) -> Option<Vec<Befund>> {
    let start = aus.find('[')?;
    let v: serde_json::Value = serde_json::from_str(aus[start..].trim()).ok()?;
    let mut befunde = Vec::new();
    for datei in v.as_array()? {
        let pfad = datei["filePath"].as_str().unwrap_or("");
        let rel =
            relativ(Path::new(pfad), projekt).unwrap_or_else(|| Path::new(pfad).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
        for m in datei["messages"].as_array().into_iter().flatten() {
            let zeile = m["line"].as_u64().map(|z| z as usize);
            let regel = m["ruleId"].as_str().map(|r| format!(" ({r})")).unwrap_or_default();
            let text = format!("ESLint: {}{regel}", m["message"].as_str().unwrap_or("").trim());
            let ort = format!("projects/{slug}/{rel}");
            befunde.push(if m["severity"].as_u64() == Some(2) || m["fatal"].as_bool() == Some(true) {
                Befund::fehler(ort, zeile, text)
            } else {
                Befund::hinweis(ort, zeile, text)
            });
        }
    }
    Some(befunde)
}

// ----- svelte-check für einen Teil -----

/// Schreibt `web/.baumeister/<name>.json`: erbt `web/tsconfig.json`, nimmt
/// die Projektordner `ordner` auf (absolut) und mit `mit_src` die Oberfläche
/// selbst (wie `tsconfig.projekte.json`). Rückgabe: der Pfad. Gibt es `web`
/// nicht (falsches Code-Repo, etwa der Inhalt), wird nichts angelegt.
pub fn tsconfig_schreiben(web: &Path, name: &str, ordner: &[PathBuf], mit_src: bool) -> std::io::Result<PathBuf> {
    let web = std::path::absolute(web)?;
    if !web.is_dir() {
        return Err(std::io::Error::new(std::io::ErrorKind::NotFound, format!("den Ordner {} gibt es nicht", web.display())));
    }
    let ziel_ordner = web.join(ARBEITSORDNER);
    std::fs::create_dir_all(&ziel_ordner)?;
    let mut include: Vec<String> = Vec::new();
    if mit_src {
        for s in ["src", "vite.config.ts", "vitest-setup.ts"] {
            include.push(web.join(s).to_string_lossy().replace('\\', "/"));
        }
    }
    for o in ordner {
        let o = std::path::absolute(o).unwrap_or_else(|_| o.clone()).to_string_lossy().replace('\\', "/");
        include.push(format!("{o}/**/*.svelte"));
        include.push(format!("{o}/**/*.ts"));
    }
    let wert = serde_json::json!({
        "extends": web.join("tsconfig.json").to_string_lossy().replace('\\', "/"),
        "include": include,
        // Rechenteile (Rust) und Abhängigkeiten nie.
        "exclude": ["**/node_modules/**", "**/server/**", "**/target/**"],
    });
    let ziel = ziel_ordner.join(format!("{name}.json"));
    let text = serde_json::to_string_pretty(&wert).unwrap_or_default();
    if std::fs::read_to_string(&ziel).ok().as_deref() != Some(text.as_str()) {
        std::fs::write(&ziel, text)?;
    }
    Ok(ziel)
}

/// Ein Fund von svelte-check, einem Projekt zugeordnet.
#[derive(Debug, Clone, PartialEq)]
pub struct CheckFund {
    pub slug: String,
    pub befund: Befund,
}

/// svelte-check mit der tsconfig `tsconfig` (Arbeitsordner `web/`).
/// `zuordnung` nennt je Projekt seinen Ordner; Befunde in anderen Dateien
/// (die Oberfläche selbst) zählen nicht. `Err`: lief nicht (bis zum Ende).
pub fn svelte_check_teil(web: &Path, tsconfig: &Path, zuordnung: &[(String, PathBuf)], niedrig: bool) -> Result<Vec<CheckFund>, String> {
    svelte_check_lauf(web, tsconfig, zuordnung, niedrig, ZEITLIMIT)?
}

/// Wie [`svelte_check_teil`], aber außen `Err`: svelte-check oder Node ist
/// nicht da; innen `Err`: lief nicht durch (startet nicht, Zeitlimit, kein
/// COMPLETED). Das Zeitlimit zählt wie ein Absturz: Im Sammellauf hält
/// sonst ein schweres Projekt alle anderen ungeprüft ([`pruefe_sammel`]).
fn svelte_check_lauf(
    web: &Path,
    tsconfig: &Path,
    zuordnung: &[(String, PathBuf)],
    niedrig: bool,
    limit: Duration,
) -> Result<Result<Vec<CheckFund>, String>, String> {
    let web = std::path::absolute(web).unwrap_or_else(|_| web.to_path_buf());
    let skript = web.join("node_modules").join("svelte-check").join("bin").join("svelte-check");
    if !skript.is_file() {
        return Err("svelte-check ist nicht da (web/node_modules fehlt)".into());
    }
    let mut cmd = node(niedrig)?;
    cmd.current_dir(&web)
        .arg(&skript)
        .args(["--output", "machine", "--workspace", ".", "--tsconfig"])
        .arg(tsconfig)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let (aus, fehl, code) = match scanner::mit_zeitlimit(cmd, limit, "svelte-check", None) {
        Ok(x) => x,
        Err(satz) => return Ok(Err(satz)),
    };
    if !aus.contains(" COMPLETED ") {
        let grund = fehl.lines().chain(aus.lines()).map(str::trim).find(|l| !l.is_empty()).unwrap_or("ohne Ausgabe");
        let code = code.map_or_else(|| "ohne Rückgabewert".to_string(), |c| format!("Rückgabe {c}"));
        return Ok(Err(format!("svelte-check lief nicht durch ({code}): {}", grund.chars().take(200).collect::<String>())));
    }
    Ok(Ok(check_lesen(&aus, &web, zuordnung)))
}

/// Die Ausgabe von `svelte-check --output machine` (Pfade relativ zu `web`
/// oder absolut), in der Reihenfolge der Ausgabe, nur Dateien der Projekte.
pub fn check_lesen(aus: &str, web: &Path, zuordnung: &[(String, PathBuf)]) -> Vec<CheckFund> {
    let mut funde = Vec::new();
    for zeile in aus.lines() {
        let mut teile = zeile.trim().splitn(3, ' ');
        let (Some(zeit), Some(art), Some(rest)) = (teile.next(), teile.next(), teile.next()) else { continue };
        if zeit.is_empty() || !zeit.bytes().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let fehler = match art {
            "ERROR" => true,
            "WARNING" => false,
            _ => continue,
        };
        let Some((datei, rest)) = scanner::json_text(rest.trim_start()) else { continue };
        let Some((stelle, rest)) = rest.trim_start().split_once(' ') else { continue };
        let nr = stelle.split(':').next().and_then(|z| z.parse::<usize>().ok());
        let Some((meldung, _)) = scanner::json_text(rest.trim_start()) else { continue };
        let pfad = web.join(&datei);
        let Some((slug, rel)) = zuordnung.iter().find_map(|(s, o)| relativ(&pfad, o).map(|r| (s.clone(), r))) else { continue };
        let ort = format!("projects/{slug}/{rel}");
        let text = format!("svelte-check: {meldung}");
        let befund = if fehler { Befund::fehler(ort, nr, text) } else { Befund::hinweis(ort, nr, text) };
        funde.push(CheckFund { slug, befund });
    }
    funde
}

/// svelte-check nur für ein Projekt (eigene tsconfig `projekt-<slug>`).
pub fn svelte_check_projekt(web: &Path, eltern: &Path, slug: &str, niedrig: bool) -> Vec<Befund> {
    if !scanner::hat_svelte(eltern, slug) && scanner::code_dateien(eltern, slug).is_empty() {
        return Vec::new();
    }
    let ordner = eltern.join(slug);
    let tsconfig = match tsconfig_schreiben(web, &format!("projekt-{slug}"), std::slice::from_ref(&ordner), false) {
        Ok(t) => t,
        Err(e) => return uebersprungen(&format!("projects/{slug}"), "svelte-check", &format!("die tsconfig lässt sich nicht schreiben ({e})")),
    };
    match svelte_check_teil(web, &tsconfig, &[(slug.to_string(), ordner)], niedrig) {
        Ok(f) => f.into_iter().map(|f| f.befund).collect(),
        Err(satz) => uebersprungen(&format!("projects/{slug}"), "svelte-check", &satz),
    }
}

// ----- Vitest -----

/// Vitest für `*.spec.ts` im Projekt (nur wenn es welche gibt).
pub fn vitest(web: &Path, eltern: &Path, slug: &str, niedrig: bool) -> Vec<Befund> {
    if !hat_specs(eltern, slug) {
        return Vec::new();
    }
    let web = std::path::absolute(web).unwrap_or_else(|_| web.to_path_buf());
    let projekt = std::path::absolute(eltern.join(slug)).unwrap_or_else(|_| eltern.join(slug));
    let skript = web.join("node_modules").join("vitest").join("vitest.mjs");
    if !skript.is_file() || !web.join(VITEST_KONFIG).is_file() {
        return uebersprungen(&format!("projects/{slug}"), "Vitest", "Vitest ist nicht da (web/node_modules oder die Konfiguration fehlt)");
    }
    let bericht = web.join(ARBEITSORDNER).join(format!("vitest-{slug}.json"));
    let _ = std::fs::create_dir_all(web.join(ARBEITSORDNER));
    let _ = std::fs::remove_file(&bericht);
    let mut cmd = match node(niedrig) {
        Ok(c) => c,
        Err(satz) => return uebersprungen(&format!("projects/{slug}"), "Vitest", &satz),
    };
    cmd.current_dir(&web)
        .arg(&skript)
        .args(["run", "--config", VITEST_KONFIG, "--reporter=json"])
        .arg(format!("--outputFile={}", bericht.display()))
        .env("DASHBOARD_VITEST_PROJEKT", &projekt)
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    match scanner::mit_zeitlimit(cmd, ZEITLIMIT, "Vitest", None) {
        Err(satz) => uebersprungen(&format!("projects/{slug}"), "Vitest", &satz),
        Ok((aus, fehl, code)) => match std::fs::read_to_string(&bericht).ok().and_then(|t| vitest_lesen(&t, &projekt, slug)) {
            // Rückgabe ≠ 0 ohne roten Test (etwa „Unhandled Errors“) ist nicht grün.
            Some(b) if code == Some(0) || b.iter().any(Befund::ist_fehler) => b,
            _ => {
                let grund = fehl.lines().chain(aus.lines()).map(str::trim).find(|l| !l.is_empty()).unwrap_or("ohne Ausgabe");
                let code = code.map_or_else(|| "ohne Rückgabewert".to_string(), |c| format!("Rückgabe {c}"));
                // Ohne lesbaren Bericht (Absturz, Abbruch) ist das Projekt nicht geprüft, nicht rot; echte Testfehler kommen über `vitest_lesen`.
                uebersprungen(
                    &format!("projects/{slug}"),
                    "Vitest",
                    &format!("Vitest lief nicht durch ({code}): {}", grund.chars().take(200).collect::<String>()),
                )
            }
        },
    }
}

/// Der JSON-Bericht von Vitest: je gescheitertem Test ein Fehler. `None`
/// auch ohne Testdateien im Bericht (gerufen wird nur mit `*.spec.ts`).
pub fn vitest_lesen(text: &str, projekt: &Path, slug: &str) -> Option<Vec<Befund>> {
    let v: serde_json::Value = serde_json::from_str(text).ok()?;
    let mut befunde = Vec::new();
    for datei in v["testResults"].as_array().filter(|a| !a.is_empty())? {
        let pfad = datei["name"].as_str().unwrap_or("");
        let rel = relativ(Path::new(pfad), projekt).unwrap_or_else(|| pfad.to_string());
        let ort = format!("projects/{slug}/{rel}");
        let mut einzeln = false;
        for t in datei["assertionResults"].as_array().into_iter().flatten() {
            if t["status"].as_str() == Some("failed") {
                einzeln = true;
                let warum = t["failureMessages"][0].as_str().and_then(|m| m.lines().next()).unwrap_or("").trim().to_string();
                befunde.push(Befund::fehler(ort.clone(), None, format!("Vitest: „{}“ ist rot. {warum}", t["fullName"].as_str().unwrap_or("?"))));
            }
        }
        if !einzeln && datei["status"].as_str() == Some("failed") {
            let warum = datei["message"].as_str().and_then(|m| m.lines().next()).unwrap_or("").trim().to_string();
            befunde.push(Befund::fehler(ort, None, format!("Vitest: Die Datei lief nicht. {warum}")));
        }
    }
    Some(befunde)
}

// ----- Alles für ein Projekt -----

/// Die ganze Prüfung eines Projekts, wie der Baumeister sie nach einer
/// Änderung macht: TOML, Assets, Scanner, Stil, ESLint, svelte-check, Vitest.
/// Vitest startet nur, wenn es vor `frist` fertig würde, sonst ist es
/// übersprungen (das Projekt nicht geprüft, nie grün).
pub fn pruefe_voll(web: &Path, eltern: &Path, slug: &str, niedrig: bool, frist: Instant) -> Vec<Befund> {
    let mut b = pruefung::pruefe_projekt(eltern, slug);
    let repo = web.parent().unwrap_or(web);
    // ESLint und Stylelint neben svelte-check: verschiedene Prozesse, je ein Faden.
    let (stil_b, eslint_b, check_b) = std::thread::scope(|s| {
        let stil_f = s.spawn(|| stil::pruefen_fuer(eltern, repo, &[slug.to_string()], &format!("projects/{slug}")));
        let eslint_f = s.spawn(|| eslint(web, eltern, slug, niedrig));
        let check = svelte_check_projekt(web, eltern, slug, niedrig);
        (faden_ergebnis(stil_f.join(), slug, "Die Stil-Prüfung (Stylelint)"), faden_ergebnis(eslint_f.join(), slug, "ESLint"), check)
    });
    b.extend(stil_b);
    b.extend(eslint_b);
    b.extend(check_b);
    b.extend(vitest_bis_frist(web, eltern, slug, niedrig, frist));
    b.dedup();
    b
}

/// [`vitest`], aber nur, wenn es vor `frist` fertig würde; sonst übersprungen
/// (nicht geprüft, nie grün). Ohne `*.spec.ts` nichts, auch nach der Frist.
fn vitest_bis_frist(web: &Path, eltern: &Path, slug: &str, niedrig: bool, frist: Instant) -> Vec<Befund> {
    if Instant::now() + ZEITLIMIT > frist && hat_specs(eltern, slug) {
        return uebersprungen(&format!("projects/{slug}"), "Vitest", SATZ_FRIST);
    }
    vitest(web, eltern, slug, niedrig)
}

/// Hat das Projekt `*.spec.ts`? Nur dann läuft Vitest.
fn hat_specs(eltern: &Path, slug: &str) -> bool {
    scanner::code_dateien(eltern, slug).iter().any(|(r, _)| r.ends_with(".spec.ts"))
}

// ----- Viele Projekte (Zweig B) -----

/// So viele Prüfungen je Projekt laufen zugleich ([`warteschlange`]).
pub const ZUGLEICH: usize = 3;
/// Ab so vielen Projekten prüft der Bau gesammelt ([`pruefe_sammel`]).
pub const SAMMEL_AB: usize = 4;
/// Grund, wenn ein Lauf nicht mehr vor der Frist fertig würde.
pub(crate) const SATZ_FRIST: &str = "die Frist für die Prüfungen dieses Baus war um; geprüft wird erst im nächsten Bau (nach einer Änderung oder einem Anstoß)";
/// Ende des Grunds, wenn nach dem gemeinsamen Lauf die Nachprüfung die Frist verpasste ([`frist_mit_grund`]).
const NACH_FRIST: &str = "je Projekt kam es nicht mehr vor der Frist dran";

/// Prüft `projekte` (Slug, Elternordner) mit `pruefe` in einer
/// Warteschlange: höchstens `zugleich` zugleich, wer fertig ist, nimmt das
/// nächste. Was nicht vor `frist` fertig würde (Start später als `frist`
/// minus [`ZEITLIMIT`]) oder dessen Faden abbricht, ist übersprungen (`was`).
/// Befunde je Projekt in derselben Reihenfolge.
pub fn warteschlange(
    projekte: &[(String, PathBuf)],
    zugleich: usize,
    frist: Instant,
    was: &str,
    pruefe: &(dyn Fn(&str, &Path) -> Vec<Befund> + Sync),
) -> Vec<Vec<Befund>> {
    let naechstes = AtomicUsize::new(0);
    let fertig: Mutex<Vec<Option<Vec<Befund>>>> = Mutex::new(vec![None; projekte.len()]);
    std::thread::scope(|sc| {
        for _ in 0..zugleich.clamp(1, projekte.len().max(1)) {
            sc.spawn(|| {
                loop {
                    let i = naechstes.fetch_add(1, Ordering::SeqCst);
                    let Some((slug, eltern)) = projekte.get(i) else { break };
                    // shortcut: rechnet mit ZEITLIMIT, pruefe_voll braucht ein paar Sekunden mehr (Scanner, tsconfig); aufrüsten, wenn Bauten die Frist messbar überziehen.
                    let b = if Instant::now() + ZEITLIMIT > frist {
                        uebersprungen(&format!("projects/{slug}"), was, SATZ_FRIST)
                    } else {
                        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| pruefe(slug, eltern)))
                            .unwrap_or_else(|_| uebersprungen(&format!("projects/{slug}"), was, "der Prüf-Faden brach ab"))
                    };
                    fertig.lock().unwrap_or_else(|e| e.into_inner())[i] = Some(b);
                }
            });
        }
    });
    ohne_ergebnis_uebersprungen(fertig.into_inner().unwrap_or_else(|e| e.into_inner()), projekte, was)
}

/// Befunde je Projekt; ein Platz ohne Ergebnis (`None`, sollte nicht
/// vorkommen) heißt übersprungen, nie still grün.
fn ohne_ergebnis_uebersprungen(je: Vec<Option<Vec<Befund>>>, projekte: &[(String, PathBuf)], was: &str) -> Vec<Vec<Befund>> {
    je.into_iter().zip(projekte).map(|(b, (s, _))| b.unwrap_or_else(|| uebersprungen(&format!("projects/{s}"), was, "der Prüf-Faden brach ab"))).collect()
}

/// Verteilt die Befunde eines gemeinsamen Stylelint-Laufs über `slugs` auf
/// die Projekte (nach `projects/<slug>`). „Übersprungen“ bei `projects` (der
/// gemeinsame Lauf lief nicht) bekommt jedes bei seinem Ordner. Ein Befund,
/// der keinem Projekt gehört, macht keins rot: Dann ist der Stil für alle
/// übersprungen (nicht geprüft).
fn verteilen(befunde: Vec<Befund>, slugs: &[&str]) -> Vec<Vec<Befund>> {
    let mut je = vec![Vec::new(); slugs.len()];
    for b in befunde {
        match slugs.iter().position(|s| b.datei == format!("projects/{s}") || b.datei.starts_with(&format!("projects/{s}/"))) {
            Some(i) => je[i].push(b),
            None if b.datei == "projects" => {
                for (i, s) in slugs.iter().enumerate() {
                    je[i].push(Befund { datei: format!("projects/{s}"), ..b.clone() });
                }
            }
            None => {
                for (i, s) in slugs.iter().enumerate() {
                    je[i].extend(uebersprungen(
                        &format!("projects/{s}"),
                        "Die Stil-Prüfung (Stylelint)",
                        &format!("ein Befund gehört keinem Projekt ({})", b.datei),
                    ));
                }
            }
        }
    }
    je
}

/// Stylelint für die Projekte `slugs` eines Elternordners, Befunde je
/// Projekt. Lief der gemeinsame Lauf gar nicht (Zeitlimit, startet nicht,
/// fehlt), läuft es je Projekt, nacheinander: Sonst hielte ein schweres
/// Projekt alle anderen dauerhaft ungeprüft. Fehlt Stylelint oder ist die
/// Frist um, endet das je Projekt ohne Node-Start (`node_lauf`, [`bis_frist`]).
fn stil_gruppe(eltern: &Path, slugs: &[&str], lauf: &(dyn Fn(String) -> stil::Lauf + Sync)) -> Vec<Vec<Befund>> {
    let erster_lief_nicht = std::cell::Cell::new(None);
    let gemerkt = |eingabe| {
        let r = lauf(eingabe);
        if erster_lief_nicht.get().is_none() {
            erster_lief_nicht.set(Some(r.is_err()));
        }
        r
    };
    let s: Vec<String> = slugs.iter().map(|s| s.to_string()).collect();
    let b = stil::pruefen_mit(eltern, &s, "projects", &gemerkt);
    if slugs.len() > 1 && erster_lief_nicht.get() == Some(true) {
        return s.iter().map(|x| stil::pruefen_mit(eltern, std::slice::from_ref(x), &format!("projects/{x}"), lauf)).collect();
    }
    verteilen(b, slugs)
}

/// Das Ergebnis eines Fadens für alle `n` Projekte (Zweig B): bricht er ab,
/// ist das Werkzeug für alle übersprungen.
fn faden_alle(r: std::thread::Result<Vec<Vec<Befund>>>, projekte: &[(String, PathBuf)], was: &str) -> Vec<Vec<Befund>> {
    r.unwrap_or_else(|_| projekte.iter().map(|(s, _)| uebersprungen(&format!("projects/{s}"), was, "der Prüf-Faden brach ab")).collect())
}

/// Ein Lauf eines Werkzeugs über die Projekte (`slugs`) eines Elternordners, Befunde je Projekt.
type GruppenLauf<'a> = &'a (dyn Fn(&Path, &[&str]) -> Vec<Vec<Befund>> + Sync);

/// `lauf`, aber nur, solange er (höchstens `dauer`) vor `frist` fertig würde.
fn bis_frist<'a>(frist: Instant, dauer: Duration, lauf: &'a (dyn Fn(String) -> stil::Lauf + Sync)) -> impl Fn(String) -> stil::Lauf + Sync + 'a {
    move |eingabe| if Instant::now() + dauer > frist { Err(SATZ_FRIST.to_string()) } else { lauf(eingabe) }
}

/// Je Elternordner ein `lauf`, die Befunde zurück an ihre Projekte. Würde ein
/// Lauf nicht vor `frist` fertig, ist `was` für die Projekte dieses Ordners übersprungen.
fn je_gruppe(projekte: &[(String, PathBuf)], frist: Instant, was: &str, lauf: GruppenLauf) -> Vec<Vec<Befund>> {
    let mut gruppen: BTreeMap<&Path, Vec<usize>> = BTreeMap::new();
    for (i, (_, e)) in projekte.iter().enumerate() {
        gruppen.entry(e.as_path()).or_default().push(i);
    }
    let mut je: Vec<Option<Vec<Befund>>> = vec![None; projekte.len()];
    for (eltern, ix) in &gruppen {
        if Instant::now() + ZEITLIMIT > frist {
            for &i in ix {
                je[i] = Some(uebersprungen(&format!("projects/{}", projekte[i].0), was, SATZ_FRIST));
            }
            continue;
        }
        let slugs: Vec<&str> = ix.iter().map(|&i| projekte[i].0.as_str()).collect();
        for (&i, b) in ix.iter().zip(lauf(eltern, &slugs)) {
            je[i] = Some(b);
        }
    }
    ohne_ergebnis_uebersprungen(je, projekte, was)
}

/// Ein gemeinsamer Lauf von svelte-check über (Slug, Projektordner) mit
/// Zeitlimit, wie [`svelte_check_lauf`].
type CheckLauf<'a> = &'a (dyn Fn(&[(String, PathBuf)], Duration) -> Result<Result<Vec<CheckFund>, String>, String> + Sync);

/// Spielraum zwischen dem Abbruch des gemeinsamen Laufs und dem Start der
/// Nachprüfung (Prozess beenden, Ausgabe lesen, tsconfig löschen).
const NACH_ABBRUCH: Duration = Duration::from_secs(10);

/// Das Zeitlimit des gemeinsamen svelte-check: höchstens [`ZEITLIMIT`] und
/// so knapp, dass nach einem Abbruch noch eine Runde Nachprüfung je Projekt
/// vor `frist` startet. `None`: dafür reicht die Zeit nicht (unter der
/// Hälfte von `ZEITLIMIT`), dann gleich je Projekt.
fn sammel_limit(jetzt: Instant, frist: Instant) -> Option<Duration> {
    let limit = ZEITLIMIT.min(frist.saturating_duration_since(jetzt).saturating_sub(ZEITLIMIT + NACH_ABBRUCH));
    (limit >= ZEITLIMIT / 2).then_some(limit)
}

/// Kam die Nachprüfung eines Projekts nicht mehr vor der Frist dran, nennt
/// der Hinweis auch, warum der gemeinsame Lauf nicht reichte (`satz`).
fn frist_mit_grund(b: Vec<Befund>, slug: &str, satz: &str) -> Vec<Befund> {
    let ort = format!("projects/{slug}");
    if b != uebersprungen(&ort, "svelte-check", SATZ_FRIST) {
        return b;
    }
    let grund = format!("der gemeinsame Lauf lief nicht durch ({}), und {NACH_FRIST}", satz.trim().trim_end_matches('.'));
    uebersprungen(&ort, "svelte-check", &grund)
}

/// svelte-check im Sammellauf: `gemeinsam` einmal über alle außer `allein`,
/// mit [`sammel_limit`]. Läuft er nicht durch (Absturz oder Zeitlimit), reicht
/// die Zeit nicht für ihn, und für `allein`, prüft `einzeln` je Projekt in der
/// [`warteschlange`], in der Reihenfolge von `projekte` (im Bau die am längsten
/// ungeprüften zuerst, `bau::pruefen_mit`) und nur, solange es vor `frist`
/// fertig würde; was dann nicht mehr drankommt, ist nicht geprüft. Nach einem
/// Zeitlimit reicht die Frist praktisch für eine Runde mit [`ZUGLEICH`]
/// Projekten; die übrigen kommen in späteren Bauten dran, weil die geprüften
/// dann jünger sind (`versucht_um`). Fehlt das Werkzeug (außen `Err`), ist es für alle im
/// gemeinsamen Lauf übersprungen.
fn check_sammel(
    projekte: &[(String, PathBuf)],
    allein: &[usize],
    frist: Instant,
    gemeinsam: CheckLauf,
    einzeln: &(dyn Fn(&str, &Path) -> Vec<Befund> + Sync),
) -> Vec<Vec<Befund>> {
    let mut je: Vec<Option<Vec<Befund>>> = vec![None; projekte.len()];
    let teil: Vec<usize> = (0..projekte.len()).filter(|i| !allein.contains(i)).collect();
    let mut nachpruefen = allein.to_vec();
    let mut grund: Option<String> = None;
    match sammel_limit(Instant::now(), frist) {
        _ if teil.is_empty() => {}
        None => nachpruefen.extend(&teil),
        Some(limit) => {
            let zuordnung: Vec<(String, PathBuf)> = teil.iter().map(|&i| (projekte[i].0.clone(), projekte[i].1.join(&projekte[i].0))).collect();
            match gemeinsam(&zuordnung, limit) {
                Ok(Ok(funde)) => {
                    for &i in &teil {
                        je[i] = Some(Vec::new());
                    }
                    for f in funde {
                        if let Some(&i) = teil.iter().find(|&&i| projekte[i].0 == f.slug) {
                            je[i].get_or_insert_with(Vec::new).push(f.befund);
                        }
                    }
                }
                Ok(Err(satz)) => {
                    grund = Some(satz);
                    nachpruefen.extend(&teil);
                }
                Err(satz) => {
                    for &i in &teil {
                        je[i] = Some(uebersprungen(&format!("projects/{}", projekte[i].0), "svelte-check", &satz));
                    }
                }
            }
        }
    }
    nachpruefen.sort_unstable();
    let paare: Vec<(String, PathBuf)> = nachpruefen.iter().map(|&i| projekte[i].clone()).collect();
    for (&i, b) in nachpruefen.iter().zip(warteschlange(&paare, ZUGLEICH, frist, "svelte-check", einzeln)) {
        je[i] = Some(match &grund {
            Some(satz) if teil.contains(&i) => frist_mit_grund(b, &projekte[i].0, satz),
            _ => b,
        });
    }
    ohne_ergebnis_uebersprungen(je, projekte, "svelte-check")
}

/// Zweig B: die ganze Prüfung wie [`pruefe_voll`] für viele Projekte
/// (Slug, Elternordner) mit wenigen Node-Starts: svelte-check einmal über
/// alle (tsconfig `sammel`, [`check_sammel`]), Stylelint einmal je
/// Elternordner, daneben ESLint und Vitest (nur mit `*.spec.ts`, nur vor
/// der Frist) je Projekt in der [`warteschlange`] ([`sammel_faeden`]).
/// Befunde je Projekt in derselben Reihenfolge. Läuft der gemeinsame svelte-check nicht durch
/// (Absturz oder Zeitlimit), prüft er je Projekt nach, solange die Frist
/// reicht; sein Zeitlimit lässt dafür Platz ([`sammel_limit`]). Fehlt er,
/// ist er für alle übersprungen. Stylelint fällt nach einem Absturz (`stil`)
/// oder Zeitlimit ([`stil_gruppe`]) je Projekt zurück, nacheinander bis
/// `frist`. Projekte mit
/// Fehlern aus dem Rust-Teil (etwa ein verbotenes `declare global`) oder
/// ohne `projekt.toml` (dann läuft der Scanner nicht) prüft svelte-check je
/// Projekt, damit sie die Typen der anderen nicht ändern.
pub fn pruefe_sammel(web: &Path, projekte: &[(String, PathBuf)], frist: Instant, niedrig: bool) -> Vec<Vec<Befund>> {
    let mut alle: Vec<Vec<Befund>> = projekte.iter().map(|(s, e)| pruefung::pruefe_projekt(e, s)).collect();
    let web_abs = std::path::absolute(web).unwrap_or_else(|_| web.to_path_buf());
    // Den Rückfall je Projekt macht `stil` selbst, nacheinander; die Frist hält ihn auf.
    let node_lauf = |eingabe| stil::node_lauf(&web_abs, eingabe);
    let stil_node = bis_frist(frist, stil::STIL_ZEITLIMIT, &node_lauf);
    let stil_lauf = |eltern: &Path, slugs: &[&str]| stil_gruppe(eltern, slugs, &stil_node);
    // Ohne projekt.toml läuft der Scanner nicht; auch so ein Projekt nie in den gemeinsamen Lauf.
    let allein: Vec<usize> =
        (0..projekte.len()).filter(|&i| alle[i].iter().any(Befund::ist_fehler) || !pruefung::ist_projekt(&projekte[i].1, &projekte[i].0)).collect();
    let gemeinsam = |zuordnung: &[(String, PathBuf)], limit: Duration| {
        let ordner: Vec<PathBuf> = zuordnung.iter().map(|(_, o)| o.clone()).collect();
        // Je Prozess ein Name: Bote und `dashboard pruefe` zugleich überschrieben sich sonst die Liste.
        tsconfig_schreiben(web, &format!("sammel-{}", std::process::id()), &ordner, false)
            .map_err(|e| format!("die tsconfig lässt sich nicht schreiben ({e})"))
            .and_then(|ts| {
                let r = svelte_check_lauf(web, &ts, zuordnung, niedrig, limit);
                let _ = std::fs::remove_file(&ts);
                r
            })
    };
    let je_projekt = |s: &str, e: &Path| {
        let mut b = eslint(web, e, s, niedrig);
        b.extend(vitest_bis_frist(web, e, s, niedrig, frist));
        b
    };
    let check = || check_sammel(projekte, &allein, frist, &gemeinsam, &|s, e| svelte_check_projekt(web, e, s, niedrig));
    let [stil_b, je_b, check_b] = sammel_faeden(projekte, frist, &stil_lauf, &je_projekt, &check);
    for (i, b) in alle.iter_mut().enumerate() {
        let fehlt = |was: &str| uebersprungen(&format!("projects/{}", projekte[i].0), was, "der Prüf-Faden brach ab");
        // Vitest wie in `pruefe_voll` nach svelte-check: Der erste Fund gibt den Satz im Stand.
        let (vitest_b, eslint_b): (Vec<Befund>, Vec<Befund>) =
            je_b.get(i).cloned().unwrap_or_else(|| fehlt("ESLint")).into_iter().partition(|x| x.text.starts_with("Vitest"));
        b.extend(stil_b.get(i).cloned().unwrap_or_else(|| fehlt("Die Stil-Prüfung (Stylelint)")));
        b.extend(eslint_b);
        b.extend(check_b.get(i).cloned().unwrap_or_else(|| fehlt("svelte-check")));
        b.extend(vitest_b);
        b.dedup();
    }
    alle
}

/// Die Fäden des Sammellaufs nebeneinander, Befunde je Projekt: Stylelint je
/// Elternordner (`stil`), svelte-check (`check`, [`check_sammel`]) und ESLint
/// mit Vitest (`je_projekt`) je Projekt in der [`warteschlange`]. Vitest hängt
/// so nicht an der Nachprüfung von svelte-check (sonst nach einem Zeitlimit
/// in jedem Bau wegen der Frist übersprungen). ESLint und Vitest teilen sich
/// die Plätze der Warteschlange: höchstens [`ZUGLEICH`] Node-Prozesse dafür,
/// wie zuvor ESLint allein. Mit svelte-check (1, nach Absturz oder Zeitlimit
/// 3) und Stylelint (1) höchstens 7 zugleich; Vitest läuft jetzt neben svelte-check.
fn sammel_faeden(
    projekte: &[(String, PathBuf)],
    frist: Instant,
    stil: GruppenLauf,
    je_projekt: &(dyn Fn(&str, &Path) -> Vec<Befund> + Sync),
    check: &(dyn Fn() -> Vec<Vec<Befund>> + Sync),
) -> [Vec<Vec<Befund>>; 3] {
    std::thread::scope(|sc| {
        let stil_f = sc.spawn(|| je_gruppe(projekte, frist, "Die Stil-Prüfung (Stylelint)", stil));
        let je_f = sc.spawn(|| warteschlange(projekte, ZUGLEICH, frist, "ESLint", je_projekt));
        let check_f = sc.spawn(check);
        [
            faden_alle(stil_f.join(), projekte, "Die Stil-Prüfung (Stylelint)"),
            faden_alle(je_f.join(), projekte, "ESLint"),
            faden_alle(check_f.join(), projekte, "svelte-check"),
        ]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pfade_vergleichen_und_relativ() {
        assert_eq!(vergleichbar(Path::new(r"C:\A\b\..\c\D.svelte")), "c:/a/c/d.svelte");
        assert_eq!(relativ(Path::new(r"C:\Repo\projects\x\seiten\A.svelte"), Path::new("c:/repo/projects/x")).as_deref(), Some("seiten/A.svelte"));
        assert_eq!(relativ(Path::new(r"C:\Repo\projects\xy\a.svelte"), Path::new("C:/Repo/projects/x")), None);
        assert_eq!(relativ(Path::new(r"C:\Repo\web\..\projects\x\a.ts"), Path::new("C:/Repo/projects/x")).as_deref(), Some("a.ts"));
    }

    #[test]
    fn tsconfig_legt_kein_fremdes_web_an() {
        let wurzel = std::env::temp_dir().join(format!("dashboard-tsconfig-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&wurzel);
        std::fs::create_dir_all(&wurzel).unwrap();
        let web = wurzel.join("web");
        assert!(tsconfig_schreiben(&web, "projekt-x", &[wurzel.join("projects/x")], false).is_err());
        assert!(!web.exists(), "ohne web/ entsteht kein web/.baumeister");
        std::fs::create_dir_all(&web).unwrap();
        let ziel = tsconfig_schreiben(&web, "projekt-x", &[wurzel.join("projects/x")], false).unwrap();
        assert!(ziel.is_file());
        let _ = std::fs::remove_dir_all(&wurzel);
    }

    /// Zweig A: Jedes Überspringen ist erkennbar, ein Fund oder Hinweis nicht.
    #[test]
    fn uebersprungen_ist_erkennbar() {
        for was in ["ESLint", "svelte-check", "Vitest", "Die Stil-Prüfung (Stylelint)"] {
            let b = uebersprungen("projects/x", was, "lief länger als 120 s.");
            assert_eq!(b.len(), 1);
            assert!(ist_uebersprungen(&b[0]), "{}", b[0].text);
            assert_eq!(b[0].text, format!("{was} ist übersprungen: lief länger als 120 s. Die anderen Prüfungen liefen."));
        }
        assert!(!ist_uebersprungen(&Befund::hinweis("projects/x/a.ts", Some(1), "ESLint: x is unused (no-unused-vars)")));
        assert!(
            !ist_uebersprungen(&Befund::fehler("projects/x", None, "ESLint ist übersprungen: x. Die anderen Prüfungen liefen.")),
            "ein Fehler ist rot, kein Überspringen"
        );
        // Fehlt ein Werkzeug ganz, meldet es Überspringen (ohne Node-Lauf).
        let leer = std::env::temp_dir().join(format!("dashboard-uebersprungen-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&leer);
        std::fs::create_dir_all(leer.join("projects/x")).unwrap();
        std::fs::create_dir_all(leer.join("web")).unwrap();
        std::fs::write(leer.join("projects/x/Seite.svelte"), "<p>x</p>\n").unwrap();
        std::fs::write(leer.join("projects/x/a.spec.ts"), "export {};\n").unwrap();
        let (e, v) = (eslint(&leer.join("web"), &leer.join("projects"), "x", false), vitest(&leer.join("web"), &leer.join("projects"), "x", false));
        assert!(e.len() == 1 && ist_uebersprungen(&e[0]), "{e:?}");
        assert!(v.len() == 1 && ist_uebersprungen(&v[0]), "{v:?}");
        // Vitest läuft, schreibt aber keinen Bericht (Absturz): übersprungen, nicht rot.
        if programme::pfad_von(Werkzeug::Node).is_some() {
            let web = leer.join("web");
            std::fs::create_dir_all(web.join("node_modules/vitest")).unwrap();
            std::fs::write(web.join("node_modules/vitest/vitest.mjs"), "console.error('Worker abgestürzt'); process.exit(1);\n").unwrap();
            std::fs::write(web.join(VITEST_KONFIG), "export default {};\n").unwrap();
            let v = vitest(&web, &leer.join("projects"), "x", false);
            assert!(v.len() == 1 && ist_uebersprungen(&v[0]) && v[0].text.contains("Worker abgestürzt"), "{v:?}");
        }
        let _ = std::fs::remove_dir_all(&leer);
    }

    /// Zweig A: Gerät ein innerer Werkzeug-Faden in Panik, ist das Werkzeug
    /// übersprungen (nicht still grün durch eine leere Liste).
    #[test]
    fn abgebrochener_faden_ist_uebersprungen() {
        let r = std::thread::scope(|s| s.spawn(|| -> Vec<Befund> { panic!("Absichtlicher Abbruch im Test") }).join());
        let b = faden_ergebnis(r, "x", "ESLint");
        assert!(b.len() == 1 && ist_uebersprungen(&b[0]) && b[0].text.starts_with("ESLint ist übersprungen: der Prüf-Faden brach ab"), "{b:?}");
        let gut = vec![Befund::fehler("projects/x/a.ts", Some(1), "x")];
        assert_eq!(faden_ergebnis(Ok(gut.clone()), "x", "ESLint"), gut);
    }

    #[test]
    fn eslint_ausgabe_lesen() {
        let aus = r#"[{"filePath":"C:\\R\\projects\\x\\Seite.svelte","messages":[{"ruleId":"no-restricted-globals","severity":2,"message":"Daten kommen nur über ladeDaten.","line":4},{"ruleId":null,"fatal":true,"severity":2,"message":"Parsing error: Unexpected token","line":9},{"ruleId":"no-unused-vars","severity":1,"message":"x is unused","line":2}]}]"#;
        let b = eslint_lesen(aus, Path::new(r"C:\R\projects\x"), "x").unwrap();
        assert_eq!(b.len(), 3);
        assert_eq!(b[0].to_string(), "projects/x/Seite.svelte, Zeile 4: ESLint: Daten kommen nur über ladeDaten. (no-restricted-globals)");
        assert!(b[0].ist_fehler() && b[1].ist_fehler() && !b[2].ist_fehler());
        assert!(eslint_lesen("Oops: kein JSON", Path::new("C:/R"), "x").is_none());
    }

    /// Zweig B: Die Warteschlange hält die Reihenfolge, läuft höchstens
    /// `zugleich` zugleich, startet nach der Frist nichts mehr und macht aus
    /// einem abgebrochenen Faden „übersprungen“, nie grün.
    #[test]
    fn warteschlange_reihenfolge_frist_und_abbruch() {
        let projekte: Vec<(String, PathBuf)> = ["a", "b", "c", "d", "e"].iter().map(|s| (s.to_string(), PathBuf::from("C:/q"))).collect();
        let (laeuft, hoechstens) = (AtomicUsize::new(0), AtomicUsize::new(0));
        let pruefe = |s: &str, _: &Path| {
            let n = laeuft.fetch_add(1, Ordering::SeqCst) + 1;
            hoechstens.fetch_max(n, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(30));
            laeuft.fetch_sub(1, Ordering::SeqCst);
            if s == "c" {
                panic!("Absichtlicher Abbruch im Test");
            }
            vec![Befund::fehler(format!("projects/{s}/Seite.svelte"), Some(1), s)]
        };
        let b = warteschlange(&projekte, 2, Instant::now() + Duration::from_secs(600), "ESLint", &pruefe);
        assert_eq!(b.len(), 5);
        for (i, s) in ["a", "b", "d", "e"].iter().enumerate() {
            let j = if i < 2 { i } else { i + 1 };
            assert_eq!(b[j], vec![Befund::fehler(format!("projects/{s}/Seite.svelte"), Some(1), *s)]);
        }
        assert!(b[2].len() == 1 && ist_uebersprungen(&b[2][0]) && b[2][0].text.contains("Faden brach ab"), "{:?}", b[2]);
        assert!(hoechstens.load(Ordering::SeqCst) <= 2, "höchstens zwei zugleich");
        // Frist schon um: nichts startet, alles übersprungen, nichts grün.
        let gerufen = AtomicUsize::new(0);
        let b = warteschlange(&projekte, 3, Instant::now(), "Die Prüfung je Projekt", &|_, _| {
            gerufen.fetch_add(1, Ordering::SeqCst);
            Vec::new()
        });
        assert_eq!(gerufen.load(Ordering::SeqCst), 0);
        assert!(b.iter().all(|b| b.len() == 1 && ist_uebersprungen(&b[0]) && b[0].text.contains("Frist")), "{b:?}");
        assert_eq!(b[4][0].datei, "projects/e");
    }

    #[test]
    fn verteilen_an_die_projekte() {
        let b = vec![
            Befund::hinweis("projects/a/Seite.svelte", Some(3), "Stil: x"),
            Befund::hinweis("projects/ab", None, "Die Stil-Prüfung (Stylelint) ist übersprungen: y. Die anderen Prüfungen liefen."),
            Befund::hinweis("projects", None, "Die Stil-Prüfung (Stylelint) ist übersprungen: z. Die anderen Prüfungen liefen."),
        ];
        let je = verteilen(b, &["a", "ab"]);
        assert_eq!(je[0].iter().map(|b| b.datei.as_str()).collect::<Vec<_>>(), ["projects/a/Seite.svelte", "projects/a"]);
        assert_eq!(je[1].iter().map(|b| b.datei.as_str()).collect::<Vec<_>>(), ["projects/ab", "projects/ab"]);
        assert!(ist_uebersprungen(&je[0][1]) && ist_uebersprungen(&je[1][1]));
        // Ein Fehler, der keinem Projekt gehört, macht keins rot: Stil für alle nicht geprüft.
        let je = verteilen(vec![Befund::fehler("anderswo/x.svelte", Some(1), "Stil: y")], &["a", "ab"]);
        assert!(je.iter().zip(["a", "ab"]).all(|(b, s)| b.len() == 1 && ist_uebersprungen(&b[0]) && b[0].datei == format!("projects/{s}")), "{je:?}");
        // Gegenprobe: bei seinem Projekt bleibt der Fehler ein Fehler.
        let je = verteilen(vec![Befund::fehler("projects/ab/x.svelte", Some(1), "Stil: y")], &["a", "ab"]);
        assert!(je[0].is_empty() && je[1].len() == 1 && je[1][0].ist_fehler(), "{je:?}");
    }

    /// Sollte der Abnahme Zweig B: Reißt der gemeinsame svelte-check das
    /// Zeitlimit, zählt das wie ein Absturz: je Projekt nachprüfen, solange die
    /// Frist reicht, sonst nicht geprüft. Gegenprobe: Fehlt das Werkzeug, hilft
    /// kein Lauf je Projekt; es ist für alle übersprungen.
    #[test]
    fn sammel_check_zeitlimit_wie_absturz() {
        let projekte: Vec<(String, PathBuf)> = ["a", "b", "c", "d"].iter().map(|s| (s.to_string(), PathBuf::from("C:/q"))).collect();
        let gemeinsam_gerufen = Mutex::new(Vec::new());
        let einzeln_gerufen = Mutex::new(Vec::new());
        let einzeln = |s: &str, _: &Path| {
            einzeln_gerufen.lock().unwrap().push(s.to_string());
            vec![Befund::fehler(format!("projects/{s}/Seite.svelte"), Some(1), "svelte-check: je Projekt")]
        };
        let limits = Mutex::new(Vec::new());
        let zeitlimit = |z: &[(String, PathBuf)], limit: Duration| -> Result<Result<Vec<CheckFund>, String>, String> {
            gemeinsam_gerufen.lock().unwrap().push(z.iter().map(|(s, _)| s.clone()).collect::<Vec<_>>());
            limits.lock().unwrap().push(limit);
            Ok(Err("svelte-check hat nach 120 s nicht geantwortet und wurde beendet.".into()))
        };
        // „d“ läuft allein (etwa declare global), nie im gemeinsamen Lauf. Die
        // Frist wie im Bau: Nach einem Zeitlimit, das der gemeinsame Lauf ganz
        // ausschöpft, muss noch eine Nachprüfung starten (Sperre der Warteschlange).
        let start = Instant::now();
        let frist = start + crate::bote::bau::PRUEFUNG_HOECHSTENS;
        let b = check_sammel(&projekte, &[3], frist, &zeitlimit, &einzeln);
        assert_eq!(*gemeinsam_gerufen.lock().unwrap(), [["a", "b", "c"]]);
        let limit = limits.lock().unwrap()[0];
        assert!(limit >= ZEITLIMIT / 2, "der gemeinsame Lauf bekommt genug Zeit: {limit:?}");
        assert!(start + limit + NACH_ABBRUCH + ZEITLIMIT <= frist, "nach dem vollen Zeitlimit startet noch eine Nachprüfung: {limit:?}");
        let mut gerufen = einzeln_gerufen.lock().unwrap().clone();
        gerufen.sort();
        assert_eq!(gerufen, ["a", "b", "c", "d"], "nach dem Zeitlimit je Projekt nachgeprüft");
        assert!(
            b.iter().zip(["a", "b", "c", "d"]).all(|(b, s)| b.len() == 1 && b[0].ist_fehler() && b[0].datei == format!("projects/{s}/Seite.svelte")),
            "{b:?}"
        );

        // Zu wenig Zeit für den gemeinsamen Lauf plus Nachprüfung: gleich je Projekt.
        einzeln_gerufen.lock().unwrap().clear();
        let b = check_sammel(&projekte, &[], Instant::now() + ZEITLIMIT + Duration::from_secs(30), &zeitlimit, &einzeln);
        assert_eq!(gemeinsam_gerufen.lock().unwrap().len(), 1, "kein gemeinsamer Lauf mehr");
        assert_eq!(einzeln_gerufen.lock().unwrap().len(), 4);
        assert!(b.iter().all(|b| b.len() == 1 && b[0].ist_fehler()), "{b:?}");

        // Frist um: nichts startet, nicht geprüft, nie grün.
        einzeln_gerufen.lock().unwrap().clear();
        let b = check_sammel(&projekte, &[], Instant::now() + Duration::from_secs(1), &zeitlimit, &einzeln);
        assert!(einzeln_gerufen.lock().unwrap().is_empty());
        assert_eq!(gemeinsam_gerufen.lock().unwrap().len(), 1, "auch kein gemeinsamer Lauf");
        assert!(b.iter().all(|b| b.len() == 1 && ist_uebersprungen(&b[0]) && b[0].text.contains("Frist")), "{b:?}");

        // Kommt die Nachprüfung nicht mehr dran, nennt der Hinweis den gemeinsamen Lauf.
        let satz = "svelte-check hat nach 110 s nicht geantwortet und wurde beendet.";
        let b = frist_mit_grund(uebersprungen("projects/a", "svelte-check", SATZ_FRIST), "a", satz);
        assert!(b.len() == 1 && ist_uebersprungen(&b[0]) && b[0].text.contains("nach 110 s") && b[0].text.contains("Frist"), "{b:?}");
        // Beide Frist-Sätze zählen nicht als Prüfversuch (`bau::pruefen_mit`), ein eigener Abbruch schon.
        assert!(frist_verpasst(&b[0]) && frist_verpasst(&uebersprungen("projects/a", "ESLint", SATZ_FRIST)[0]));
        assert!(!frist_verpasst(&uebersprungen("projects/a", "svelte-check", satz)[0]), "Gegenprobe: eigenes Zeitlimit");
        // Gegenprobe: ein echtes Ergebnis je Projekt bleibt, wie es ist.
        let echt = vec![Befund::fehler("projects/a/Seite.svelte", Some(1), "svelte-check: x")];
        assert_eq!(frist_mit_grund(echt.clone(), "a", satz), echt);

        // Gegenprobe: Werkzeug fehlt (außen Err): kein Lauf je Projekt, für alle im gemeinsamen Lauf übersprungen.
        let fehlt = |_: &[(String, PathBuf)], _: Duration| -> Result<Result<Vec<CheckFund>, String>, String> { Err("svelte-check ist nicht da".into()) };
        let b = check_sammel(&projekte, &[3], Instant::now() + Duration::from_secs(600), &fehlt, &einzeln);
        assert_eq!(*einzeln_gerufen.lock().unwrap(), ["d"], "nur das Projekt, das ohnehin allein läuft");
        assert!(b[..3].iter().all(|b| b.len() == 1 && ist_uebersprungen(&b[0]) && b[0].text.contains("nicht da")), "{b:?}");

        // Gegenprobe: läuft der gemeinsame Lauf durch, ist ein Projekt ohne Fund grün, keins je Projekt nachgeprüft.
        einzeln_gerufen.lock().unwrap().clear();
        let durch = |_: &[(String, PathBuf)], _: Duration| -> Result<Result<Vec<CheckFund>, String>, String> {
            Ok(Ok(vec![CheckFund { slug: "b".into(), befund: Befund::fehler("projects/b/Seite.svelte", Some(2), "svelte-check: x") }]))
        };
        let b = check_sammel(&projekte, &[], Instant::now() + Duration::from_secs(600), &durch, &einzeln);
        assert!(einzeln_gerufen.lock().unwrap().is_empty());
        assert!(b[0].is_empty() && b[1].len() == 1 && b[1][0].ist_fehler() && b[2].is_empty() && b[3].is_empty(), "{b:?}");

        // Der echte Lauf: Ein svelte-check, der das Zeitlimit reißt, ist innen
        // `Err` (wie ein Absturz); fehlt er, außen `Err`. Attrappe, die schläft.
        if programme::pfad_von(Werkzeug::Node).is_none() {
            crate::ueberspringen("Node fehlt.");
            return;
        }
        let web = std::env::temp_dir().join(format!("dashboard-sammel-zeitlimit-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&web);
        std::fs::create_dir_all(web.join("node_modules/svelte-check/bin")).unwrap();
        std::fs::write(web.join("node_modules/svelte-check/bin/svelte-check"), "setTimeout(() => {}, 30000);\n").unwrap();
        let ts = web.join("tsconfig.json");
        let schlaeft = svelte_check_lauf(&web, &ts, &[], false, Duration::from_secs(1));
        std::fs::remove_file(web.join("node_modules/svelte-check/bin/svelte-check")).unwrap();
        let fehlt = svelte_check_lauf(&web, &ts, &[], false, Duration::from_secs(1));
        let _ = std::fs::remove_dir_all(&web);
        assert!(matches!(&schlaeft, Ok(Err(s)) if s.contains("nicht geantwortet")), "{schlaeft:?}");
        assert!(matches!(&fehlt, Err(s) if s.contains("nicht da")), "{fehlt:?}");
    }

    /// Hinweis zum Nachtrag Tempo B: Rand von `sammel_limit`. Ab 190 s Restzeit
    /// (ZEITLIMIT / 2 + ZEITLIMIT + NACH_ABBRUCH) läuft der gemeinsame Lauf,
    /// knapp darunter gleich je Projekt; ab 250 s bekommt er das volle ZEITLIMIT.
    #[test]
    fn sammel_limit_rand() {
        let jetzt = Instant::now();
        let rest = |ms: u64| sammel_limit(jetzt, jetzt + Duration::from_millis(ms));
        assert_eq!(ZEITLIMIT + NACH_ABBRUCH + ZEITLIMIT / 2, Duration::from_secs(190), "Grenze wie im Kommentar");
        assert_eq!(rest(189_999), None, "knapp unter der Grenze: gleich je Projekt");
        assert_eq!(rest(190_000), Some(ZEITLIMIT / 2));
        assert_eq!(rest(190_001), Some(ZEITLIMIT / 2 + Duration::from_millis(1)));
        assert_eq!(rest(249_999), Some(ZEITLIMIT - Duration::from_millis(1)));
        assert_eq!(rest(250_000), Some(ZEITLIMIT));
        assert_eq!(rest(250_001), Some(ZEITLIMIT), "nie mehr als ZEITLIMIT");
        assert_eq!(sammel_limit(jetzt + Duration::from_secs(1), jetzt), None, "Frist vorbei");
    }

    /// Muss des Nachtrags Tempo B: Reicht die Zeit nicht mehr für den
    /// gemeinsamen svelte-check (`sammel_limit` None, `zeitlimit` wird nicht
    /// gerufen) und die Nachprüfung je Projekt dauert bis an die Frist, ist
    /// Vitest trotzdem nicht wegen der Frist übersprungen: Es läuft im
    /// Sammellauf neben svelte-check, nicht danach. Gegenprobe: Erst nach der
    /// Nachprüfung gestartet (der alte Ablauf), wäre es wegen der Frist übersprungen.
    #[test]
    fn sammel_vitest_nicht_hinter_der_nachpruefung() {
        let wurzel = std::env::temp_dir().join(format!("dashboard-sammel-vitest-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&wurzel);
        let (web, eltern) = (wurzel.join("web"), wurzel.join("projects"));
        std::fs::create_dir_all(&web).unwrap();
        for s in ["a", "b", "c", "mit"] {
            std::fs::create_dir_all(eltern.join(s)).unwrap();
            std::fs::write(eltern.join(s).join("Seite.svelte"), "<p>x</p>\n").unwrap();
        }
        std::fs::write(eltern.join("mit/a.spec.ts"), "export {};\n").unwrap();
        let projekte: Vec<(String, PathBuf)> = ["a", "b", "c", "mit"].iter().map(|s| (s.to_string(), eltern.clone())).collect();
        // Die Frist lässt Vitest nur in den ersten `spiel` starten; die Nachprüfung dauert länger.
        let spiel = crate::zeitgrenze(Duration::from_millis(500), Duration::from_millis(2000));
        let frist = Instant::now() + ZEITLIMIT + spiel;
        let zeitlimit = |_: &[(String, PathBuf)], _: Duration| -> Result<Result<Vec<CheckFund>, String>, String> {
            Ok(Err("svelte-check hat nach 120 s nicht geantwortet und wurde beendet.".into()))
        };
        let einzeln = |_: &str, _: &Path| {
            std::thread::sleep(spiel * 2);
            Vec::new()
        };
        let check = || check_sammel(&projekte, &[], frist, &zeitlimit, &einzeln);
        let stil = |_: &Path, slugs: &[&str]| vec![Vec::new(); slugs.len()];
        let je_projekt = |s: &str, e: &Path| vitest_bis_frist(&web, e, s, false, frist);
        let [_, je, check_b] = sammel_faeden(&projekte, frist, &stil, &je_projekt, &check);
        let danach = vitest_bis_frist(&web, &eltern, "mit", false, frist);
        let _ = std::fs::remove_dir_all(&wurzel);
        assert!(Instant::now() + ZEITLIMIT > frist, "die Nachprüfung lief bis hinter den letzten Vitest-Start");
        assert!(check_b[..3].iter().all(Vec::is_empty), "nachgeprüft: {check_b:?}");
        assert!(je[..3].iter().all(Vec::is_empty), "ohne *.spec.ts kein Vitest: {je:?}");
        // Vitest lief (hier ohne node_modules: übersprungen, aber nicht wegen der Frist).
        assert!(je[3].len() == 1 && je[3][0].text.starts_with("Vitest") && je[3][0].text.contains("nicht da"), "{je:?}");
        assert!(danach.len() == 1 && ist_uebersprungen(&danach[0]) && danach[0].text.contains("Frist"), "Gegenprobe: {danach:?}");
    }

    /// Umfang der Abnahme Zweig B: Vitest läuft in `pruefe_voll` wieder nach
    /// den anderen Werkzeugen. Würde es nicht mehr vor der Frist fertig, ist
    /// es übersprungen (nicht geprüft, nie grün); ohne `*.spec.ts` nicht.
    #[test]
    fn pruefe_voll_vitest_nur_vor_der_frist() {
        let wurzel = std::env::temp_dir().join(format!("dashboard-voll-frist-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&wurzel);
        let (web, eltern) = (wurzel.join("web"), wurzel.join("projects"));
        std::fs::create_dir_all(&web).unwrap();
        for s in ["mit", "ohne"] {
            std::fs::create_dir_all(eltern.join(s)).unwrap();
            std::fs::write(eltern.join(s).join("Seite.svelte"), "<p>x</p>\n").unwrap();
        }
        std::fs::write(eltern.join("mit/a.spec.ts"), "export {};\n").unwrap();
        let vitest_von = |s: &str, frist: Instant| -> Vec<Befund> {
            pruefe_voll(&web, &eltern, s, false, frist).into_iter().filter(|b| b.text.starts_with("Vitest")).collect()
        };
        let knapp = vitest_von("mit", Instant::now());
        let weit = vitest_von("mit", Instant::now() + Duration::from_secs(600));
        let ohne = vitest_von("ohne", Instant::now());
        let _ = std::fs::remove_dir_all(&wurzel);
        assert!(knapp.len() == 1 && ist_uebersprungen(&knapp[0]) && knapp[0].text.contains("Frist"), "{knapp:?}");
        // Gegenprobe: mit Zeit läuft Vitest (hier ohne node_modules: übersprungen, aber nicht wegen der Frist).
        assert!(weit.len() == 1 && ist_uebersprungen(&weit[0]) && weit[0].text.contains("nicht da"), "{weit:?}");
        assert!(ohne.is_empty(), "ohne *.spec.ts kein Vitest, auch nach der Frist: {ohne:?}");
    }

    /// Zweig B: Auch je Elternordner und im Stylelint-Rückfall startet nach
    /// der Frist kein Lauf mehr; die Projekte sind übersprungen, nie grün.
    #[test]
    fn sammel_haelt_die_frist() {
        let projekte: Vec<(String, PathBuf)> =
            [("a", "C:/q1"), ("b", "C:/q2"), ("c", "C:/q2")].iter().map(|(s, e)| (s.to_string(), PathBuf::from(e))).collect();
        let gerufen = AtomicUsize::new(0);
        let lauf = |_: &Path, slugs: &[&str]| {
            gerufen.fetch_add(1, Ordering::SeqCst);
            vec![Vec::new(); slugs.len()]
        };
        let b = je_gruppe(&projekte, Instant::now() + Duration::from_secs(600), "Die Stil-Prüfung (Stylelint)", &lauf);
        assert!(gerufen.load(Ordering::SeqCst) == 2 && b.iter().all(Vec::is_empty), "ein Lauf je Elternordner: {b:?}");
        let b = je_gruppe(&projekte, Instant::now(), "Die Stil-Prüfung (Stylelint)", &lauf);
        assert_eq!(gerufen.load(Ordering::SeqCst), 2, "nach der Frist kein Lauf mehr");
        assert!(b.iter().zip(&projekte).all(|(b, (s, _))| b.len() == 1 && ist_uebersprungen(&b[0]) && b[0].datei == format!("projects/{s}")), "{b:?}");

        // Stylelint stürzt gemeinsam ab; der Rückfall je Projekt endet an der Frist.
        let wurzel = std::env::temp_dir().join(format!("dashboard-sammel-frist-{}", std::process::id()));
        for s in ["a", "b", "c"] {
            std::fs::create_dir_all(wurzel.join(s)).unwrap();
            std::fs::write(wurzel.join(s).join("Seite.svelte"), "<p>x</p>\n").unwrap();
        }
        // Der erste Lauf startet vor der Frist, endet nach ihr (Spielraum unter Last).
        let spiel = crate::zeitgrenze(Duration::from_millis(300), Duration::from_millis(1500));
        let laeufe = AtomicUsize::new(0);
        let absturz = |_: String| -> stil::Lauf {
            laeufe.fetch_add(1, Ordering::SeqCst);
            std::thread::sleep(spiel);
            Ok((String::new(), "Absichtlicher Absturz".into(), Some(1)))
        };
        let dauer = Duration::from_secs(1);
        let lauf = bis_frist(Instant::now() + dauer + spiel, dauer, &absturz);
        let slugs = ["a".to_string(), "b".to_string(), "c".to_string()];
        let b = stil::pruefen_mit(&wurzel, &slugs, "projects", &lauf);
        let _ = std::fs::remove_dir_all(&wurzel);
        assert_eq!(laeufe.load(Ordering::SeqCst), 1, "nur der gemeinsame Lauf, kein Rückfall nach der Frist");
        let je = verteilen(b, &["a", "b", "c"]);
        assert!(je.iter().all(|b| b.len() == 1 && ist_uebersprungen(&b[0]) && b[0].text.contains("Frist")), "{je:?}");
    }

    /// Nachbesserung Zweig B: Reißt Stylelint je Elternordner das Zeitlimit,
    /// läuft es je Projekt (nicht alle dauerhaft ungeprüft). Gegenproben: Läuft
    /// der gemeinsame Lauf, kein Lauf je Projekt; fehlt Stylelint, je Projekt übersprungen.
    #[test]
    fn stil_gruppe_zeitlimit_je_projekt() {
        let wurzel = std::env::temp_dir().join(format!("dashboard-stil-gruppe-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&wurzel);
        for s in ["a", "b"] {
            std::fs::create_dir_all(wurzel.join(s)).unwrap();
            std::fs::write(wurzel.join(s).join("Seite.svelte"), "<p>x</p>\n").unwrap();
        }
        std::fs::create_dir_all(wurzel.join("ohne")).unwrap();
        let laeufe = AtomicUsize::new(0);
        let fund = |eingabe: &str| -> stil::Lauf {
            let n = eingabe.lines().count();
            let mut aus: String = (0..n).map(|nr| format!(r#"{{"nr":{nr},"zeile":1,"regel":"color-named","text":"x"}}"#) + "\n").collect();
            aus.push_str(&format!("FERTIG {n} {n}\n"));
            Ok((aus, String::new(), Some(0)))
        };
        let haengt_gemeinsam = |eingabe: String| -> stil::Lauf {
            laeufe.fetch_add(1, Ordering::SeqCst);
            if eingabe.lines().count() > 1 { Err("Die Stil-Prüfung hat nach 60 s nicht geantwortet und wurde beendet.".into()) } else { fund(&eingabe) }
        };
        let b = stil_gruppe(&wurzel, &["a", "b", "ohne"], &haengt_gemeinsam);
        let n_zeitlimit = laeufe.swap(0, Ordering::SeqCst);
        let geht = |eingabe: String| -> stil::Lauf {
            laeufe.fetch_add(1, Ordering::SeqCst);
            fund(&eingabe)
        };
        let gemeinsam = stil_gruppe(&wurzel, &["a", "b", "ohne"], &geht);
        let n_gemeinsam = laeufe.swap(0, Ordering::SeqCst);
        let fehlt = |_: String| -> stil::Lauf { Err("Stylelint ist nicht da (web/node_modules fehlt)".into()) };
        let weg = stil_gruppe(&wurzel, &["a", "b", "ohne"], &fehlt);
        let _ = std::fs::remove_dir_all(&wurzel);
        assert_eq!(n_zeitlimit, 3, "ein gemeinsamer Lauf, dann je Projekt mit Svelte-Dateien");
        for (b, s) in [(&b, "zeitlimit"), (&gemeinsam, "gemeinsam")] {
            assert!(b[0].len() == 1 && b[0][0].datei == "projects/a/Seite.svelte" && !ist_uebersprungen(&b[0][0]), "{s}: {b:?}");
            assert!(b[1].len() == 1 && b[1][0].datei == "projects/b/Seite.svelte" && !ist_uebersprungen(&b[1][0]), "{s}: {b:?}");
            assert!(b[2].is_empty(), "{s}: {b:?}");
        }
        assert_eq!(n_gemeinsam, 1, "läuft der gemeinsame Lauf, keiner je Projekt");
        assert!(weg[..2].iter().zip(["a", "b"]).all(|(b, s)| b.len() == 1 && ist_uebersprungen(&b[0]) && b[0].datei == format!("projects/{s}")), "{weg:?}");
    }

    /// Zweig B, Rückfall: Stürzt der gemeinsame svelte-check ab (kein
    /// COMPLETED), läuft er je Projekt; fehlt er, ist er für alle übersprungen
    /// (ohne Lauf je Projekt). Ein Projekt mit Fehler aus dem Rust-Teil läuft
    /// nie im gemeinsamen Lauf. Attrappe statt svelte-check.
    #[test]
    fn sammel_faellt_nur_bei_absturz_je_projekt_zurueck() {
        if programme::pfad_von(Werkzeug::Node).is_none() {
            crate::ueberspringen("Node fehlt.");
            return;
        }
        let wurzel = std::env::temp_dir().join(format!("dashboard-sammel-rueckfall-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&wurzel);
        let (web, eltern) = (wurzel.join("web"), wurzel.join("projects"));
        std::fs::create_dir_all(web.join("node_modules/svelte-check/bin")).unwrap();
        std::fs::write(web.join("tsconfig.json"), "{}\n").unwrap();
        let slugs = ["a", "b", "c", "d", "global"];
        for s in slugs {
            std::fs::create_dir_all(eltern.join(s)).unwrap();
            std::fs::write(eltern.join(s).join("Seite.svelte"), "<p>x</p>\n").unwrap();
            // Mit Kopf, sonst läuft der Rust-Teil (Scanner) gar nicht.
            std::fs::write(eltern.join(s).join("projekt.toml"), "schema = 3\ntitle = \"Probe\"\nstatus = \"aktiv\"\nsummary = \"Probe.\"\n").unwrap();
        }
        std::fs::write(eltern.join("global/typen.ts"), "export {};\ndeclare global {\n\tinterface Window {\n\t\tx: string;\n\t}\n}\n").unwrap();
        // Je tsconfig: „sammel“ stürzt ab und schreibt, wen sie enthielt; sonst ein Fund je Projekt.
        let attrappe = r#"const fs = require('fs');
const ts = process.argv[process.argv.indexOf('--tsconfig') + 1];
const cfg = JSON.parse(fs.readFileSync(ts, 'utf8'));
if (/sammel-\d+\.json$/.test(ts)) { fs.writeFileSync(ts + '.inhalt', cfg.include.join('\n')); console.error('Absichtlicher Absturz'); process.exit(1); }
const ordner = cfg.include[0].replace('/**/*.svelte', '');
console.log('1 START "x"');
console.log('2 ERROR ' + JSON.stringify(ordner + '/Seite.svelte') + ' 1:1 "Fund je Projekt"');
console.log('3 COMPLETED 1 FILES 1 ERRORS 0 WARNINGS');
"#;
        std::fs::write(web.join("node_modules/svelte-check/bin/svelte-check"), attrappe).unwrap();
        let projekte: Vec<(String, PathBuf)> = slugs.iter().map(|s| (s.to_string(), eltern.clone())).collect();
        let b = pruefe_sammel(&web, &projekte, Instant::now() + Duration::from_secs(600), false);
        let sammel = web.join(ARBEITSORDNER).join(format!("sammel-{}.json", std::process::id()));
        let inhalt = std::fs::read_to_string(sammel.with_extension("json.inhalt")).unwrap_or_default();
        let sammel_bleibt = sammel.exists();
        // Fehlt svelte-check: für alle übersprungen, kein Lauf je Projekt.
        std::fs::remove_file(web.join("node_modules/svelte-check/bin/svelte-check")).unwrap();
        let weg = pruefe_sammel(&web, &projekte, Instant::now() + Duration::from_secs(600), false);
        let _ = std::fs::remove_dir_all(&wurzel);
        assert!(!sammel_bleibt, "die tsconfig des Sammellaufs ist danach weg");
        assert!(!inhalt.is_empty() && !inhalt.contains("/global/"), "das Projekt mit declare global nie im gemeinsamen Lauf: {inhalt}");
        for (s, b) in slugs.iter().zip(&b) {
            let check: Vec<String> = b.iter().filter(|b| b.text.starts_with("svelte-check")).map(|b| b.to_string()).collect();
            assert_eq!(check, [format!("projects/{s}/Seite.svelte, Zeile 1: svelte-check: Fund je Projekt")], "{s}: {b:?}");
            // ESLint und Stylelint fehlen hier: übersprungen, je Projekt bei seinem Ordner.
            assert!(b.iter().any(|x| ist_uebersprungen(x) && x.datei == format!("projects/{s}") && x.text.starts_with("Die Stil-Prüfung")), "{s}: {b:?}");
            assert!(b.iter().any(|x| ist_uebersprungen(x) && x.text.starts_with("ESLint")), "{s}: {b:?}");
        }
        assert!(b[4].iter().any(|x| x.ist_fehler() && x.datei == "projects/global/typen.ts"), "{:?}", b[4]);
        for (s, b) in slugs.iter().zip(&weg) {
            let check: Vec<&Befund> = b.iter().filter(|b| b.text.starts_with("svelte-check")).collect();
            assert!(check.len() == 1 && ist_uebersprungen(check[0]) && check[0].text.contains("nicht da"), "{s}: {b:?}");
        }
    }

    #[test]
    fn check_ausgabe_den_projekten_zuordnen() {
        let web = Path::new(r"C:\R\web");
        let zu = [
            ("a".to_string(), PathBuf::from(r"C:\R\projects\a")),
            ("b".to_string(), PathBuf::from(r"C:\R\web\.projekte-gruen\b")),
            ("c".to_string(), PathBuf::from(r"D:\Inhalt\c")),
        ];
        let aus = "1 START \"C:/R/web\"\n\
                   2 ERROR \"../projects/a/Seite.svelte\" 3:5 \"Typ falsch\"\n\
                   3 ERROR \"src/lib/x.ts\" 1:1 \"gehört der Oberfläche\"\n\
                   4 WARNING \".projekte-gruen/b/seiten/t.svelte\" 7:1 \"unbenutzt\"\n\
                   5 ERROR \"D:/Inhalt/c/Seite.svelte\" 2:1 \"anderswo\"\n\
                   6 COMPLETED 9 FILES 3 ERRORS 1 WARNINGS";
        let f = check_lesen(aus, web, &zu);
        let kurz: Vec<(String, String, bool)> = f.iter().map(|f| (f.slug.clone(), f.befund.datei.clone(), f.befund.ist_fehler())).collect();
        assert_eq!(
            kurz,
            [
                ("a".to_string(), "projects/a/Seite.svelte".to_string(), true),
                ("b".to_string(), "projects/b/seiten/t.svelte".to_string(), false),
                ("c".to_string(), "projects/c/Seite.svelte".to_string(), true)
            ]
        );
        assert_eq!(f[0].befund.zeile, Some(3));
    }

    #[test]
    fn vitest_bericht_lesen() {
        let t = r#"{"testResults":[{"name":"C:/R/projects/x/rechnen.spec.ts","status":"failed","assertionResults":[{"fullName":"rechnet richtig","status":"failed","failureMessages":["AssertionError: expected 3 to be 4\n  at …"]},{"fullName":"geht","status":"passed"}]},{"name":"C:/R/projects/x/kaputt.spec.ts","status":"failed","message":"SyntaxError: x\nmehr","assertionResults":[]}]}"#;
        let b = vitest_lesen(t, Path::new("C:/R/projects/x"), "x").unwrap();
        assert_eq!(b.len(), 2);
        assert_eq!(b[0].to_string(), "projects/x/rechnen.spec.ts: Vitest: „rechnet richtig“ ist rot. AssertionError: expected 3 to be 4");
        assert_eq!(b[1].to_string(), "projects/x/kaputt.spec.ts: Vitest: Die Datei lief nicht. SyntaxError: x");
        assert_eq!(vitest_lesen(r#"{"testResults":[]}"#, Path::new("C:/R/projects/x"), "x"), None, "ohne Testdateien nicht grün");
    }

    /// Nachbesserung Zweig A: Vitest mit bestandenem Bericht, aber Rückgabe 1
    /// (etwa „Unhandled Errors“), ist übersprungen, nicht grün; mit Rückgabe 0
    /// grün, ein leerer Bericht übersprungen. Attrappe statt Vitest.
    #[test]
    fn vitest_rueckgabe_ohne_roten_test_ist_uebersprungen() {
        if programme::pfad_von(Werkzeug::Node).is_none() {
            crate::ueberspringen("Node fehlt.");
            return;
        }
        let wurzel = std::env::temp_dir().join(format!("dashboard-vitest-rueckgabe-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&wurzel);
        let (web, eltern) = (wurzel.join("web"), wurzel.join("projects"));
        std::fs::create_dir_all(web.join("node_modules/vitest")).unwrap();
        std::fs::create_dir_all(eltern.join("x")).unwrap();
        std::fs::write(eltern.join("x/a.spec.ts"), "export {};\n").unwrap();
        std::fs::write(web.join(VITEST_KONFIG), "export default {};\n").unwrap();
        let bestanden = r#"{"testResults":[{"name":"a.spec.ts","status":"passed","assertionResults":[{"fullName":"geht","status":"passed"}]}]}"#;
        let attrappe = |bericht: &str, code: i32| {
            let js = format!(
                "import fs from 'node:fs';\nconst f = process.argv.find(a => a.startsWith('--outputFile=')).slice(13);\nfs.writeFileSync(f, {bericht:?});\nconsole.error('Unhandled Errors');\nprocess.exit({code});\n"
            );
            std::fs::write(web.join("node_modules/vitest/vitest.mjs"), js).unwrap();
            vitest(&web, &eltern, "x", false)
        };
        let eins = attrappe(bestanden, 1);
        let null = attrappe(bestanden, 0);
        let leer = attrappe(r#"{"testResults":[]}"#, 0);
        let _ = std::fs::remove_dir_all(&wurzel);
        assert!(eins.len() == 1 && ist_uebersprungen(&eins[0]) && eins[0].text.contains("Rückgabe 1"), "{eins:?}");
        assert_eq!(null, Vec::new(), "bestanden mit Rückgabe 0 ist grün");
        assert!(leer.len() == 1 && ist_uebersprungen(&leer[0]), "{leer:?}");
    }
}
