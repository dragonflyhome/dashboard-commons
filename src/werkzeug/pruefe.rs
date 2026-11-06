//! `dashboard.exe pruefe [<slug>|--alle] [--svelte]` und
//! `dashboard.exe pruefe --bauliste [--svelte]`: prüft ein Projekt oder alle
//! (`inhalt::pruefung`, dazu der Scanner für Svelte- und TS-Dateien aus
//! `werkzeug::scanner`) und schreibt je Befund eine Zeile, etwa
//!
//! ```text
//! FEHLER projects/x/daten/gantt.toml, Zeile 12: Dem Eintrag „Phase 1“ fehlt `bis`. …
//! FEHLER projects/x/notiz.md: Markdown-Dateien gibt es im Projekt nicht mehr. …
//! 2 Fehler, 0 Hinweise
//! ```
//!
//! Ohne Argument wie `--alle`; dabei auch `data/schwerpunkte.toml` (Etappe
//! A1: doppelt zugeordnet ist ein Fehler, unbekannte Projekte und Projekte
//! ohne Schwerpunkt sind Hinweise). Der Projektordner ist derselbe wie beim Server
//! (`app::projects_dir`, also auch `DASHBOARD_PROJECTS_DIR`); das Repo ist der
//! Ordner darüber. Keine Farben.
//!
//! Außer bei `--bauliste` dazu je Datei oder Ordner der
//! geprüften Quellen, den das Dienstkonto nicht lesen darf, ein FEHLER mit
//! `icacls … /reset` (`werkzeug::rechte`, nur unter Windows; das Konto aus
//! `konten.dienst_sid` der Einstellungen, ohne sie ein HINWEIS).
//!
//! Für ein einzelnes Projekt mit Svelte-Dateien läuft dazu svelte-check
//! (5 bis 25 s, braucht Node und `web/node_modules`; fehlt eines, ein
//! Hinweis), mit der eigenen Konfiguration `web/tsconfig.projekte.json`;
//! `--ohne-svelte` lässt es weg. Bei `--alle` und `--bauliste` nur mit
//! `--svelte`; sonst prüft nur der Scanner, in Millisekunden.
//!
//! Dazu die Stil-Prüfung mit Stylelint (Etappe 3b, `werkzeug::stil`): für ein
//! Projekt mit Svelte-Dateien und bei `--alle` (ein Lauf für alle, unter einer
//! Sekunde; braucht Node und `web/node_modules`, sonst ein Hinweis);
//! `--ohne-stil` lässt sie weg. Bei `--bauliste` läuft sie mit `--svelte`.
//!
//! Seit Etappe 4b für ein Projekt dazu ESLint für den Projektordner (mit
//! `--cache`, `web/eslint.projekte.config.js`) und Vitest für eigene
//! `*.spec.ts` (`werkzeug::projektpruefung`), wie im Baumeister;
//! `--ohne-svelte` lässt beide weg.
//! Bis `stil::STIL_ALS_FEHLER` umgestellt ist, sind ihre Funde Hinweise.
//!
//! `--bauliste` schreibt `web/src/lib/projekte.generated.ts`. Ohne
//! `--svelte` prüft nur der Scanner (grün aus dem Ordner, rot mit Fehlern;
//! für Tests und Proben-Bauten nach `DASHBOARD_BUILD_ZIEL`, nie zum
//! Einspielen). Mit `--svelte` (so ruft `scripts\Dienst.ps1 -Neustart`)
//! fällt das Urteil dieselbe Funktion wie im Boten
//! (`bote::bau::bauliste_einspielen`): je Projekt `pruefe_voll` (TOML,
//! Scanner, Stil, ESLint, svelte-check, Vitest) nur für Geändertes, dann
//! die Gesamtprüfung. In die Liste kommen nur
//! grüne Kopien (`web/.projekte-gruen`): die eben grün geprüfte, sonst die
//! letzte; ohne Kopie fehlt das Projekt. Übersprungen oder nach der Prüfung
//! gespeichert heißt nicht grün (`NICHT GEPRÜFT`). Kopien und `stand.json`
//! teilt es mit dem Boten; alle Inhaltsquellen zählen. Bleibt die
//! Gesamtprüfung rot (`GESAMTPRÜFUNG ROT`), gilt wie im Boten die letzte gute
//! Fassung: In der Liste steht jedes Projekt mit grüner Kopie, keins ist
//! `GRÜN`. Scheitert die Prüfung selbst (Node oder ein Paket fehlt, die
//! Gesamtprüfung läuft nicht), wird die LEERE Liste geschrieben: gebaut wird
//! dann ohne Projektseiten, nie mit einer alten Liste.
//!
//! `pruefe --system <ordner>` prüft die TOML-Dateien einer System-Seite
//! (etwa `docs\zielbild`): Schreibweise und Blöcke (`[[block]]`), je Satz
//! ein FEHLER mit Datei und Zeile des Blocks, wie die Seite ihn als
//! „Block n: …“ zeigt. Ohne Projekte, Stil und Rechte.
//!
//! Rückgabe: 0 ohne Fehler (Hinweise zählen nicht), 1 mit Fehlern, 2 bei
//! falschem Aufruf oder unbekanntem Projekt. `--bauliste` gibt 0, auch wenn
//! Projekte rot oder nicht geprüft sind; 1, wenn die Prüfung scheiterte
//! (Liste leer), 2, wenn die Datei sich nicht schreiben lässt.

use std::io::Write;
use std::path::{Path, PathBuf};

use crate::bote::bau;
use crate::inhalt::pruefung::{self, Befund, Schwere};
use crate::werkzeug::{projektpruefung, rechte, scanner, stil};

pub const AUFRUF: &str = "Aufruf: dashboard pruefe <slug> [--ohne-svelte] [--ohne-stil]  oder  dashboard pruefe --alle [--svelte] [--ohne-stil]  oder  dashboard pruefe --bauliste [--svelte]  oder  dashboard pruefe --system <ordner>";

/// Ziel der Bauliste, relativ zum Repo.
pub const BAULISTE_DATEI: &str = "web/src/lib/projekte.generated.ts";

/// Führt den Befehl aus, schreibt nach stdout und gibt den Rückgabewert.
pub fn ausfuehren(args: &[String]) -> i32 {
    // System-Seiten: nur der genannte Ordner, ohne Projekte und Rechte.
    if args.iter().any(|a| a == "--system") {
        let (text, code) = system_aufruf(args);
        let mut aus = std::io::stdout().lock();
        let _ = aus.write_all(text.as_bytes());
        let _ = aus.flush();
        return code;
    }
    // Etappe 4b und Umzug des Inhalts: Das Projekt wird in der Quelle geprüft,
    // in der es liegt; die Werkzeuge (web/) kommen aus dem Code-Repo, auch
    // wenn der Inhalt außerhalb liegt (`code_repo`).
    let q = crate::app::quellen();
    let erste = q.erste();
    let repo = code_repo(&erste);
    let anders = |dir: &Path| (dir.parent() != Some(repo.as_path())).then_some(repo.as_path());
    let slug = args.iter().find(|a| !a.starts_with('-'));
    // Darf der Dienst alles im Inhalt lesen? Nur hier
    // (läuft als Inhaber), nicht in den Tests der übrigen Prüfung und nicht bei --bauliste.
    // Dazu data/ und docs/ neben jeder Quelle (rechte::ziele).
    let ziele: Vec<(std::path::PathBuf, std::path::PathBuf)> = match (slug, slug.and_then(|s| q.eltern(s))) {
        _ if args.iter().any(|a| a == "--bauliste") => Vec::new(),
        (Some(s), Some(eltern)) => rechte::ziele(&[eltern.to_path_buf()], Some(s)),
        (Some(_), None) => Vec::new(),
        (None, _) => rechte::ziele(&q.wurzeln(), None),
    };
    let rechte = rechte::pruefen(&ziele);
    let (text, code) = match slug.and_then(|s| q.eltern(s)) {
        Some(eltern) => lauf_in_mit(eltern, anders(eltern), args, rechte, Vec::new()),
        None if slug.is_none() && !q.einzeln() && !args.iter().any(|a| a == "--bauliste") => lauf_quellen_mit(&q, &repo, args, rechte),
        // Mit allen Inhaltsquellen (für --bauliste --svelte, wie der Bote).
        None => lauf_in_mit(&erste, anders(&erste), args, rechte, q.wurzeln()),
    };
    let mut aus = std::io::stdout().lock();
    let _ = aus.write_all(text.as_bytes());
    let _ = aus.flush();
    code
}

/// Das Code-Repo (mit `web/` und `Cargo.toml`): der Arbeitsordner, wenn er
/// eins ist (so ruft Dienst.ps1), sonst der Ordner über `target\<profil>\`
/// des Programms, sonst das Repo, aus dem das Programm gebaut wurde (Aufruf
/// aus dem Inhaltsordner mit der Dienst-Kopie in ProgramData), sonst der
/// Ordner über dem ersten Projektordner (wie vor dem Umzug des Inhalts).
pub fn code_repo(erste: &Path) -> std::path::PathBuf {
    let exe = std::env::current_exe().ok().and_then(|e| e.ancestors().nth(3).map(Path::to_path_buf));
    let kandidaten = [std::env::current_dir().ok(), exe, Some(std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")))];
    erstes_repo(kandidaten.into_iter().flatten()).unwrap_or_else(|| match erste.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => std::path::PathBuf::from("."),
    })
}

fn erstes_repo(kandidaten: impl IntoIterator<Item = std::path::PathBuf>) -> Option<std::path::PathBuf> {
    kandidaten.into_iter().find(|p| p.join("web").is_dir() && p.join("Cargo.toml").is_file())
}

/// `pruefe --alle` (oder ohne Argument) über mehrere Inhaltsquellen: jedes
/// Projekt in seiner Quelle; `data/` liegt neben dem ersten Ordner, Schwerpunkte
/// und Gruppen kennen die Projekte aller Quellen; die Stil-Prüfung nur den ersten.
/// Eine unlesbare Quelle ist hier ein Fehler ([`satz_unlesbar`]). Mit nur einer
/// Quelle, bei `pruefe <slug>` und bei `--bauliste` läuft `lauf_in_mit`, dort
/// bleibt eine unlesbare Quelle stumm.
#[cfg(test)]
fn lauf_quellen(q: &crate::quellen::Quellen, repo: &Path, args: &[String]) -> (String, i32) {
    lauf_quellen_mit(q, repo, args, Vec::new())
}

/// Der Satz der Prüfung für eine unlesbare Inhaltsquelle, mit Pfad. Anders als
/// im Dienst gibt es hier keinen letzten Stand und keinen zweiten Versuch.
fn satz_unlesbar(pfad: &Path) -> String {
    format!("Die Inhaltsquelle {} lässt sich nicht lesen (sie fehlt, oder es fehlt das Leserecht); ihre Projekte fehlen in dieser Prüfung.", pfad.display())
}

/// Wie `lauf_quellen`, mit Befunden von außen (die Rechte des Dienstes, [`rechte`]).
fn lauf_quellen_mit(q: &crate::quellen::Quellen, repo: &Path, args: &[String], zusatz: Vec<Befund>) -> (String, i32) {
    let rest: Vec<&str> = args.iter().map(String::as_str).filter(|a| !matches!(*a, "--svelte" | "--ohne-svelte" | "--ohne-stil")).collect();
    if !matches!(rest.as_slice(), [] | ["--alle"]) {
        return (format!("{AUFRUF}\n"), 2);
    }
    let erste = q.erste();
    let mut befunde: Vec<Befund> = pruefung::pruefe_quellen(&q.wurzeln()).into_values().flatten().collect();
    let projekt_zahlen = pruefung::zaehlen(&befunde);
    let daten = daten_ordner(&erste);
    befunde.extend(pruefung::pruefe_schwerpunkte(&daten.join("schwerpunkte.toml"), &q.wurzeln()));
    befunde.extend(pruefung::pruefe_gruppen(&daten.join("gruppen.toml"), &q.wurzeln()));
    befunde.extend(pruefung::pruefe_pc(&daten.join("pc.toml")));
    if args.iter().any(|a| a == "--svelte") {
        befunde.extend(scanner::svelte_check(repo, None));
    }
    if !args.iter().any(|a| a == "--ohne-stil") {
        befunde.extend(stil::pruefen(&erste, repo, None));
    }
    // Eine unlesbare Quelle fehlt sonst stumm; ihre Projekte hießen dann in data/ „gibt es nicht“.
    for satz in q.doppelt.iter().cloned().chain(q.unlesbare_pfade().iter().map(|p| satz_unlesbar(p))) {
        befunde.push(Befund::fehler("DASHBOARD_PROJECTS_DIR", None, satz));
    }
    befunde.extend(zusatz);
    befunde.dedup();
    let mut text = String::new();
    zeilen(&mut text, &befunde);
    let f = summe(&mut text, &befunde, Some(projekt_zahlen));
    (text, if f > 0 { 1 } else { 0 })
}

/// `data/` (Schwerpunkte, Gruppen, PC, Tabu-Liste) liegt neben dem
/// Projektordner, so wie der Server sie liest (`app::data_file`).
fn daten_ordner(projekte: &Path) -> std::path::PathBuf {
    projekte.parent().map(Path::to_path_buf).unwrap_or_default().join("data")
}

fn zahl_wort(f: usize, h: usize) -> String {
    format!("{f} Fehler, {h} {}", if h == 1 { "Hinweis" } else { "Hinweise" })
}

/// Schreibt die Summe und gibt die Zahl der Fehler. Bei `--alle` steht davor,
/// was allein die Projekte ergeben: Das zählt auch `/api/pruefung` (dieselbe
/// Prüfung je Projekt). `data/`, Stil und svelte-check kommen nur hier dazu,
/// Störungen der Rechenteile nur im Dienst (sonst „andere Zahlen“).
fn summe(text: &mut String, befunde: &[Befund], projekte: Option<(usize, usize)>) -> usize {
    let (f, h) = pruefung::zaehlen(befunde);
    if let Some((pf, ph)) = projekte.filter(|p| *p != (f, h)) {
        text.push_str(&format!(
            "Davon in den Projekten (so zählt auch /api/pruefung): {}. Der Rest kommt aus data/ und der Stil-Prüfung.\n",
            zahl_wort(pf, ph)
        ));
    }
    text.push_str(&format!("{}\n", zahl_wort(f, h)));
    f
}

/// Der Befehl ohne Ein- und Ausgabe: Text und Rückgabewert.
pub fn lauf(dir: &Path, args: &[String]) -> (String, i32) {
    lauf_in(dir, None, args)
}

/// Wie [`lauf`]; `repo` (mit `web/`) wenn der Projektordner nicht im Repo
/// liegt (eine weitere Inhaltsquelle). Dann läuft svelte-check mit eigener
/// tsconfig (`projektpruefung`).
pub fn lauf_in(dir: &Path, repo_anders: Option<&Path>, args: &[String]) -> (String, i32) {
    lauf_in_mit(dir, repo_anders, args, Vec::new(), Vec::new())
}

/// Wie [`lauf_in`], mit Befunden von außen (die Rechte des Dienstes, [`rechte`])
/// und den Inhaltsquellen für `--bauliste --svelte` (leer: nur `dir`).
fn lauf_in_mit(dir: &Path, repo_anders: Option<&Path>, args: &[String], zusatz: Vec<Befund>, quellen: Vec<PathBuf>) -> (String, i32) {
    if !dir.is_dir() {
        return (format!("Den Projektordner {} gibt es nicht.\n", dir.display()), 2);
    }
    let svelte = args.iter().any(|a| a == "--svelte");
    let ohne_svelte = args.iter().any(|a| a == "--ohne-svelte");
    let ohne_stil = args.iter().any(|a| a == "--ohne-stil");
    let rest: Vec<&str> = args.iter().map(String::as_str).filter(|a| !matches!(*a, "--svelte" | "--ohne-svelte" | "--ohne-stil")).collect();
    let repo = match repo_anders.or(dir.parent()) {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    };
    let mut projekt_zahlen = None;
    let mut befunde: Vec<Befund> = match rest.as_slice() {
        [] | ["--alle"] => {
            let mut b = alle(dir);
            projekt_zahlen = Some(pruefung::zaehlen(&b));
            // `data/` liegt neben dem Projektordner (wie beim Server und bei
            // `lauf_quellen`), nicht im Code-Repo: Liegt der Inhalt außerhalb,
            // hieß es sonst fälschlich „schwerpunkte.toml fehlt“.
            let daten = daten_ordner(dir);
            // Etappe A1: data/schwerpunkte.toml gegen die Projekte.
            b.extend(pruefung::pruefe_schwerpunkte(&daten.join("schwerpunkte.toml"), &[dir.to_path_buf()]));
            // Entscheidung projektgruppe: data/gruppen.toml gegen die Projekte.
            b.extend(pruefung::pruefe_gruppen(&daten.join("gruppen.toml"), &[dir.to_path_buf()]));
            // data/pc.toml gegen den Vertrag (etwa `cloud_stand`).
            b.extend(pruefung::pruefe_pc(&daten.join("pc.toml")));
            if svelte {
                b.extend(scanner::svelte_check(repo, None));
            }
            if !ohne_stil {
                b.extend(stil::pruefen(dir, repo, None));
            }
            b
        }
        ["--bauliste"] => return bauliste(dir, repo, svelte, quellen),
        [slug] if !slug.starts_with('-') => {
            if !pruefung::wird_geprueft(dir, slug) {
                return (format!("Ein Projekt „{slug}“ gibt es in {} nicht.\n", dir.display()), 2);
            }
            let mut b = pruefung::pruefe_projekt(dir, slug);
            // Für ein Projekt mit Svelte-Dateien läuft svelte-check von selbst
            // (Etappe 4v: „pruefe <slug> prüft auch Svelte“); --ohne-svelte
            // lässt es weg.
            if !ohne_svelte && scanner::hat_svelte(dir, slug) {
                if repo_anders.is_some() {
                    b.extend(projektpruefung::svelte_check_projekt(&repo.join("web"), dir, slug, false));
                } else {
                    b.extend(scanner::svelte_check(repo, Some(slug)));
                }
            }
            // Etappe 3b: Stylelint für die Stile der Svelte-Dateien.
            if !ohne_stil {
                b.extend(stil::pruefen(dir, repo, Some(slug)));
            }
            // Etappe 4b: ESLint für den Projektordner (mit --cache) und Vitest
            // für eigene *.spec.ts, wie im Baumeister; --ohne-svelte lässt
            // beide weg (dann prüft nur der Scanner, in Millisekunden).
            if !ohne_svelte {
                let web = repo.join("web");
                b.extend(projektpruefung::eslint(&web, dir, slug, false));
                b.extend(projektpruefung::vitest(&web, dir, slug, false));
            }
            b
        }
        _ => return (format!("{AUFRUF}\n"), 2),
    };
    befunde.extend(zusatz);
    befunde.dedup();
    let mut text = String::new();
    zeilen(&mut text, &befunde);
    let f = summe(&mut text, &befunde, projekt_zahlen);
    (text, if f > 0 { 1 } else { 0 })
}

fn zeilen(text: &mut String, befunde: &[Befund]) {
    for b in befunde {
        let wort = match b.schwere {
            Schwere::Fehler => "FEHLER",
            Schwere::Hinweis => "HINWEIS",
        };
        text.push_str(&format!("{wort} {b}\n"));
    }
}

/// `pruefe --bauliste`: Datei schreiben, rote Projekte mit Fehlern nennen.
/// Mit `svelte` (der Einspiel-Weg) fällt das Urteil der Bote ([`einspielen`]).
fn bauliste(dir: &Path, repo: &Path, svelte: bool, quellen: Vec<PathBuf>) -> (String, i32) {
    if !repo.join("web").is_dir() {
        return (format!("Den Ordner web gibt es in {} nicht.\n", repo.display()), 2);
    }
    if svelte {
        return einspielen(&bau::BauOrt { repo: repo.to_path_buf(), projekte: dir.to_path_buf(), quellen });
    }
    let ziel = repo.join(BAULISTE_DATEI);
    let scanner::Geschrieben { gruen, rote, neu, gescheitert } = match scanner::bauliste_schreiben_mit(dir, &ziel) {
        Ok(x) => x,
        Err(e) => return (format!("{BAULISTE_DATEI} lässt sich nicht schreiben: {e}\n"), 2),
    };
    if let Some(grund) = gescheitert {
        let wie = if neu { "neu geschrieben" } else { "unverändert" };
        return (format!("GESCHEITERT {}\n{BAULISTE_DATEI} {wie} (leer).\n", scanner::satz_gescheitert(&grund)), 1);
    }
    let mut text = String::new();
    for (slug, fehler) in &rote {
        text.push_str(&format!("ROT {slug}: {} Fehler, wird nicht gebaut.\n", fehler.len()));
        zeilen(&mut text, fehler);
    }
    let liste = if gruen.is_empty() { "keins".to_string() } else { gruen.join(", ") };
    text.push_str(&format!("GRÜN {liste}\n"));
    text.push_str(&format!("{BAULISTE_DATEI} {} ({} grün, {} rot).\n", if neu { "neu geschrieben" } else { "unverändert" }, gruen.len(), rote.len()));
    (text, 0)
}

/// `pruefe --bauliste --svelte`: das Urteil des Boten ([`bau::bauliste_einspielen`]).
fn einspielen(ort: &bau::BauOrt) -> (String, i32) {
    einspiel_text(ort, bau::bauliste_einspielen(ort))
}

/// Text und Rückgabe zum Urteil `v`. Zeilen: `ROT slug: Satz` (fehlt im Bau),
/// `KOPIE slug: Satz` (rot, gebaut aus der letzten grünen Kopie),
/// `NICHT GEPRÜFT slug: Satz` (ein Werkzeug lief nicht oder nach der Prüfung
/// gespeichert; Kopie oder fehlt), Stil-Hinweise, dann `GRÜN a, b` (nur die
/// grünen), `GEBAUT a, b` (alles in der Bauliste) und, wenn welche fehlen,
/// `FEHLT a, b`. Gescheitert: die leere Liste, Rückgabe 1. Gesamtprüfung rot:
/// zuerst `GESAMTPRÜFUNG ROT Grund`, keins `GRÜN`, gebaut aus allen grünen
/// Kopien, Rückgabe 0 (Dienst.ps1 baut damit).
pub(crate) fn einspiel_text(ort: &bau::BauOrt, v: bau::Vorbereitet) -> (String, i32) {
    let ziel = ort.repo.join(BAULISTE_DATEI);
    if let Some(grund) = v.gescheitert {
        return match scanner::bauliste_leer_schreiben(&ort.projekte, &ziel) {
            Ok(_) => (format!("GESCHEITERT {}\n{BAULISTE_DATEI} geschrieben (leer).\n", scanner::satz_gescheitert(&grund)), 1),
            Err(e) => (format!("{BAULISTE_DATEI} lässt sich nicht schreiben: {e}\n"), 2),
        };
    }
    let gruen = ort.gruen();
    let stand = bau::Stand::laden(&gruen);
    let satz = |s: &str| stand.projekte.get(s).map(|p| p.satz.clone()).unwrap_or_default();
    let mut text = String::new();
    if let Some(grund) = &v.gesamt_rot {
        // Die Sätze im Stand sind hier die alten (er bleibt wie im Boten): keine Zeilen je Projekt.
        text.push_str(&format!("GESAMTPRÜFUNG ROT {grund}\nDie Gesamtprüfung ist rot; gebaut werden die letzten grünen Kopien, ohne Kopie fehlt die Seite.\n"));
    } else {
        for (wort, slugs) in [("ROT", &v.rote), ("KOPIE", &v.alte_fassung), ("NICHT GEPRÜFT", &v.nicht_geprueft)] {
            for s in slugs {
                text.push_str(&format!("{wort} {s}: {}\n", satz(s)));
            }
        }
    }
    zeilen(&mut text, &v.stil);
    let namen = |l: Vec<&str>| if l.is_empty() { "keins".to_string() } else { l.join(", ") };
    let gebaut: Vec<&str> = v.eintraege.iter().map(|(s, _)| s.as_str()).collect();
    let nicht_gruen = |s: &str| v.gesamt_rot.is_some() || v.alte_fassung.iter().chain(&v.nicht_geprueft).any(|x| x == s);
    let fehlt: Vec<&str> = v.rote.iter().chain(&v.nicht_geprueft).map(String::as_str).filter(|s| !gebaut.contains(s)).collect();
    text.push_str(&format!("GRÜN {}\n", namen(gebaut.iter().copied().filter(|s| !nicht_gruen(s)).collect())));
    text.push_str(&format!("GEBAUT {}\n", namen(gebaut.clone())));
    if !fehlt.is_empty() {
        text.push_str(&format!("FEHLT {}\n", fehlt.join(", ")));
    }
    let zahlen = match v.gesamt_rot {
        Some(_) => format!("Gesamtprüfung rot, {} ohne Kopie", v.rote.len()),
        None => format!("{} rot, {} nicht geprüft", v.rote.len() + v.alte_fassung.len(), v.nicht_geprueft.len()),
    };
    text.push_str(&format!("{BAULISTE_DATEI} geschrieben ({} in der Liste, alle aus der grünen Kopie; {zahlen}).\n", gebaut.len()));
    (text, 0)
}

fn alle(dir: &Path) -> Vec<Befund> {
    pruefung::pruefe_alle(dir).into_values().flatten().collect()
}

/// `pruefe --system <ordner>`: genau diese zwei Argumente.
fn system_aufruf(args: &[String]) -> (String, i32) {
    match args {
        [a, ordner] | [ordner, a] if a == "--system" && !ordner.starts_with('-') => system(Path::new(ordner), ordner),
        _ => (format!("{AUFRUF}\n"), 2),
    }
}

/// Prüft die `*.toml` einer System-Seite (etwa `docs\zielbild`): Schreibweise,
/// bekannte Felder mit falschem Typ (`reihenfolge = "2"`, `titel = 3`) und
/// die Blöcke (`[[block]]`) mit denselben Sätzen, die die Seite als
/// „Block n: …“ zeigt, hier mit Datei und Zeile. Jeder Satz ist ein FEHLER:
/// Auf der Seite fehlt dann etwas. Unbekannte Felder eines Abschnitts sind
/// erlaubt (die Seite zeigt sie unter den Feldern). Gelesen wird wie im
/// Server (`api::system_seiten`): dieselben Dateien, dieselbe Größengrenze.
pub fn system(ordner: &Path, anzeige: &str) -> (String, i32) {
    use crate::api::system_seiten as sys;
    let (namen, alle) = match sys::toml_namen(ordner) {
        Ok(x) => x,
        Err(_) => return (format!("Den Ordner {anzeige} gibt es nicht, oder er lässt sich nicht lesen.\n"), 2),
    };
    let vorn = anzeige.replace('\\', "/");
    let vorn = vorn.trim_end_matches('/');
    let mut befunde = Vec::new();
    if alle > namen.len() {
        befunde.push(Befund::fehler(
            vorn,
            None,
            format!("Im Ordner liegen {alle} TOML-Dateien; das Dashboard liest nur die ersten {} (nach Namen).", sys::HOECHSTENS_DATEIEN),
        ));
    }
    for (name, pfad) in &namen {
        let datei = format!("{vorn}/{name}");
        let text = match sys::datei_text(pfad) {
            Ok(t) => t,
            Err(satz) => {
                befunde.push(Befund::fehler(&datei, None, satz));
                continue;
            }
        };
        let tabelle = match toml::from_str::<toml::Table>(&text) {
            Ok(t) => t,
            Err(e) => {
                let f = crate::vertrag::lesen::aus_toml_fehler(&datei, &text, &e);
                befunde.push(Befund::fehler(f.datei, f.zeile, f.text));
                continue;
            }
        };
        for (feld, satz) in sys::abschnitt_feld_hinweise(&tabelle) {
            befunde.push(Befund::fehler(&datei, sys::feld_zeile(&text, &feld), satz));
        }
        let Some(v) = tabelle.get("block") else { continue };
        let (_, hinweise) = sys::bloecke_lesen(v);
        let zeilen = sys::block_zeilen(&text);
        for h in hinweise {
            let zeile = match h.block {
                Some(n) => zeilen.get(n - 1).copied(),
                None => sys::feld_zeile(&text, "block").or_else(|| text.lines().position(|l| l.trim_start().starts_with("[block")).map(|i| i + 1)),
            };
            befunde.push(Befund::fehler(&datei, zeile, h.als_satz()));
        }
    }
    let mut text = String::new();
    zeilen(&mut text, &befunde);
    let f = summe(&mut text, &befunde, None);
    (text, if f > 0 { 1 } else { 0 })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn beispiel() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("beispiel").join("projects")
    }

    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn ein_gutes_projekt() {
        let (text, code) = lauf(&beispiel(), &args(&["neu-form"]));
        assert_eq!(code, 0, "{text}");
        let zeilen: Vec<&str> = text.lines().collect();
        // Zwei Unterseiten ohne Datei, vier Felder, die erst Etappe 3a zeigt,
        // ein Eintrag im Zeitstrahl ohne `kennung` und mit `farbe`. Ohne
        // Svelte-Dateien gibt es keine Stil-Prüfung (Etappe 3b), auch keinen Hinweis.
        // Seit Etappe B2 dazu: kanban.toml läuft aus.
        assert_eq!(zeilen.len(), 10, "{text}");
        assert!(zeilen[0].starts_with("HINWEIS projects/neu-form/projekt.toml, Zeile "), "{text}");
        assert!(text.contains("HINWEIS projects/neu-form/daten/kanban.toml: `kanban.toml` läuft aus"), "{text}");
        assert_eq!(zeilen[9], "0 Fehler, 9 Hinweise");
        assert_eq!(lauf(&beispiel(), &args(&["neu-form", "--ohne-stil"])), (text, 0), "--ohne-stil wird angenommen");
    }

    #[test]
    fn ein_kaputtes_projekt() {
        let (text, code) = lauf(&beispiel(), &args(&["neu-kaputt"]));
        assert_eq!(code, 1, "{text}");
        assert_eq!(
            text,
            "FEHLER projects/neu-kaputt/projekt.toml, Zeile 4: Bei `status` steht „aktief“. Erlaubt sind: aktiv, pausiert, geplant, fertig, problem, archiviert.\n1 Fehler, 0 Hinweise\n"
        );
    }

    #[test]
    fn alle_projekte() {
        let (text, code) = lauf(&beispiel(), &args(&["--alle"]));
        assert_eq!(code, 1, "{text}");
        assert_eq!(text.lines().filter(|l| l.starts_with("FEHLER ")).count(), 2, "{text}");
        assert!(text.lines().last().unwrap().starts_with("2 Fehler, "), "{text}");
        assert_eq!(lauf(&beispiel(), &[]), (text, 1), "ohne Argument wie --alle");
    }

    #[test]
    fn falscher_aufruf_und_unbekanntes_projekt() {
        let (text, code) = lauf(&beispiel(), &args(&["gibt-es-nicht"]));
        assert_eq!(code, 2);
        assert!(text.starts_with("Ein Projekt „gibt-es-nicht“ gibt es in "), "{text}");
        assert_eq!(lauf(&beispiel(), &args(&["--alles"])), (format!("{AUFRUF}\n"), 2));
        assert_eq!(lauf(&beispiel(), &args(&["a", "b"])).1, 2);
        assert_eq!(lauf(&beispiel().join("gibt-es-nicht"), &[]).1, 2);
    }

    /// `pruefe --system <ordner>`: die Block-Hinweise mit Datei und Zeile des
    /// Blocks, kaputtes TOML mit Zeile, alte Dateien ohne Befund.
    #[test]
    fn system_seiten_pruefen() {
        let docs = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("beispiel").join("docs").join("zielbild");
        let (text, code) = system(&docs, r"docs\zielbild\");
        assert_eq!(code, 1, "{text}");
        let zeilen: Vec<&str> = text.lines().collect();
        assert_eq!(zeilen.len(), 5, "{text}");
        assert!(zeilen[0].starts_with("FEHLER docs/zielbild/c-kaputt.toml, Zeile "), "{text}");
        assert!(zeilen[1].starts_with("FEHLER docs/zielbild/d-bloecke.toml, Zeile 19: Block 3: Zeile 2 der Tabelle hat 2 Felder"), "{text}");
        assert!(zeilen[2].starts_with("FEHLER docs/zielbild/d-bloecke.toml, Zeile 36: Block 5: Er hat keinen Inhalt"), "{text}");
        assert!(zeilen[3].starts_with("FEHLER docs/zielbild/d-bloecke.toml, Zeile 39: Block 6: Er hat mehr als einen Inhalt"), "{text}");
        assert_eq!(zeilen[4], "4 Fehler, 0 Hinweise");

        // Ein Ordner nur mit guten Dateien: grün.
        let ordner = std::env::temp_dir().join(format!("dashboard-pruefe-system-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&ordner);
        std::fs::create_dir_all(&ordner).unwrap();
        std::fs::write(ordner.join("a.toml"), "titel = \"A\"\nweiteres = 1\n[[block]]\nbeispiel = \"\"\"\n[[block]]\"\"\"\n").unwrap();
        std::fs::write(ordner.join("notiz.md"), "kein TOML").unwrap();
        assert_eq!(system(&ordner, "x"), ("0 Fehler, 0 Hinweise\n".to_string(), 0));

        // Falsche Typen oben in der Datei und eine zu große Datei (Grenze des Servers).
        std::fs::write(ordner.join("b.toml"), "titel = 3\n\nreihenfolge = \"2\"\n").unwrap();
        let gross = format!("titel = \"G\"\ntext = \"{}\"\n", "x".repeat(crate::api::system_seiten::HOECHSTENS as usize));
        std::fs::write(ordner.join("c.toml"), gross).unwrap();
        let (text, code) = system(&ordner, "x/");
        assert_eq!(code, 1, "{text}");
        let z: Vec<&str> = text.lines().collect();
        assert!(z[0].starts_with("FEHLER x/b.toml, Zeile 1: titel muss Text"), "{text}");
        assert!(z[1].starts_with("FEHLER x/b.toml, Zeile 3: reihenfolge muss eine ganze Zahl"), "{text}");
        assert_eq!(z[2], "FEHLER x/c.toml: Diese Datei ist zu groß (über 512 KB) und wird nicht gelesen.");
        assert_eq!(z[3], "3 Fehler, 0 Hinweise");
        let _ = std::fs::remove_dir_all(&ordner);

        // Aufruf: genau ein Ordner; einen, den es nicht gibt: 2.
        assert_eq!(system_aufruf(&args(&["--system"])), (format!("{AUFRUF}\n"), 2));
        assert_eq!(system_aufruf(&args(&["--system", "a", "b"])).1, 2);
        let (t, c) = system_aufruf(&args(&["--system", "gibt-es-nicht-hier"]));
        assert_eq!((t.as_str(), c), ("Den Ordner gibt-es-nicht-hier gibt es nicht, oder er lässt sich nicht lesen.\n", 2));
    }

    /// Entscheidung archiv-status: `status = "archiviert"` ist ein
    /// erlaubtes Wort; das Projekt wird weiter geprüft (ohne Fehler). Fehlt
    /// `archiviert_seit`, gibt es einen Hinweis; ein falsches Datum ist ein Fehler,
    /// `archiviert_seit` ohne Archiv ein Hinweis.
    #[test]
    fn archiviert_ist_erlaubt() {
        let wurzel = std::env::temp_dir().join(format!("dashboard-pruefe-archiv-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&wurzel);
        std::fs::create_dir_all(wurzel.join("ruhig")).unwrap();
        let mit = |kopf: &str| {
            std::fs::write(wurzel.join("ruhig/projekt.toml"), format!("schema = 3\ntitle = \"Ruhig\"\n{kopf}summary = \"S\"\n")).unwrap();
            lauf(&wurzel, &args(&["ruhig", "--ohne-stil"]))
        };
        let gut = mit("status = \"archiviert\"\narchiviert_seit = \"2026-11-01\"\n");
        let ohne_datum = mit("status = \"archiviert\"\n");
        let falsches_datum = mit("status = \"archiviert\"\narchiviert_seit = \"1.11.2026\"\n");
        let ohne_archiv = mit("status = \"aktiv\"\narchiviert_seit = \"2026-11-01\"\n");
        let _ = std::fs::remove_dir_all(&wurzel);
        assert_eq!(gut, ("0 Fehler, 0 Hinweise\n".to_string(), 0));
        assert_eq!(ohne_datum.1, 0, "{}", ohne_datum.0);
        assert!(
            ohne_datum.0.starts_with("HINWEIS projects/ruhig/projekt.toml, Zeile 3: Das Projekt ist archiviert, aber `archiviert_seit` fehlt.")
                && ohne_datum.0.ends_with("0 Fehler, 1 Hinweis\n"),
            "{}",
            ohne_datum.0
        );
        assert_eq!(falsches_datum.1, 1, "{}", falsches_datum.0);
        assert!(falsches_datum.0.contains("Zeile 4: Bei `archiviert_seit` steht „1.11.2026“."), "{}", falsches_datum.0);
        assert!(ohne_archiv.0.contains("`archiviert_seit` gilt nur mit `status = \"archiviert\"`"), "{}", ohne_archiv.0);
    }

    /// Liegt der Inhalt außerhalb des Code-Repos, liest
    /// `pruefe --alle` `data/` neben dem Projektordner (wie der Server), nicht
    /// im Code-Repo; sonst hieß es fälschlich „schwerpunkte.toml fehlt“. Vor
    /// der Summe steht, was die Projekte allein ergeben (so zählt `/api/pruefung`).
    #[test]
    fn alle_liest_data_neben_dem_inhalt_und_nennt_die_zahl_des_dienstes() {
        let inhalt = std::env::temp_dir().join(format!("dashboard-pruefe-data-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&inhalt);
        let projekte = inhalt.join("projects");
        std::fs::create_dir_all(projekte.join("ruhig")).unwrap();
        std::fs::create_dir_all(inhalt.join("data")).unwrap();
        std::fs::write(projekte.join("ruhig/projekt.toml"), "schema = 3\ntitle = \"Ruhig\"\nstatus = \"aktiv\"\nsummary = \"S\"\n").unwrap();
        std::fs::write(inhalt.join("data/schwerpunkte.toml"), "[[schwerpunkt]]\nname = \"A\"\nreihenfolge = 1\nprojekte = [\"ruhig\"]\n").unwrap();
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let (text, code) = lauf_in(&projekte, Some(&repo), &args(&["--alle", "--ohne-stil"]));
        assert_eq!(code, 0, "{text}");
        assert!(!text.contains("schwerpunkte.toml fehlt"), "{text}");
        // Ohne Schwerpunkt-Datei: der Hinweis, und die Zahl der Projekte allein steht davor.
        std::fs::remove_file(inhalt.join("data/schwerpunkte.toml")).unwrap();
        let (ohne, _) = lauf_in(&projekte, Some(&repo), &args(&["--alle", "--ohne-stil"]));
        let _ = std::fs::remove_dir_all(&inhalt);
        assert!(ohne.contains("HINWEIS data/schwerpunkte.toml: data/schwerpunkte.toml fehlt"), "{ohne}");
        let zeilen: Vec<&str> = ohne.lines().collect();
        assert_eq!(
            zeilen[zeilen.len() - 2],
            "Davon in den Projekten (so zählt auch /api/pruefung): 0 Fehler, 0 Hinweise. Der Rest kommt aus data/ und der Stil-Prüfung.",
            "{ohne}"
        );
        assert_eq!(zeilen[zeilen.len() - 1], "0 Fehler, 1 Hinweis", "{ohne}");
    }

    /// Umzug des Inhalts: `pruefe --alle` über zwei Quellen sieht die
    /// Projekte beider; das Code-Repo bleibt das Repo mit web/.
    #[test]
    fn alle_ueber_mehrere_quellen() {
        let extra = std::env::temp_dir().join(format!("dashboard-pruefe-quellen-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&extra);
        std::fs::create_dir_all(extra.join("beispiel-c")).unwrap();
        std::fs::write(extra.join("beispiel-c/projekt.toml"), "schema = 3\ntitle = \"C\"\nstatus = \"aktief\"\n").unwrap();
        let q = crate::quellen::Quellen::aus_liste(vec![beispiel(), extra.clone()]);
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let (text, code) = lauf_quellen(&q, &repo, &args(&["--alle", "--ohne-stil"]));
        assert_eq!(code, 1, "{text}");
        assert!(text.contains("FEHLER projects/beispiel-c/projekt.toml"), "die zweite Quelle zählt: {text}");
        assert!(text.lines().last().unwrap().starts_with("3 Fehler, "), "{text}");
        assert_eq!(lauf_quellen(&q, &repo, &args(&["--alles"])).1, 2);
        assert_eq!(code_repo(&beispiel()), repo, "das Repo mit web/ und Cargo.toml");
        let _ = std::fs::remove_dir_all(&extra);
    }

    /// Nach einem Umzug: data/ (neben der ersten Quelle) nennt ein Projekt der
    /// zweiten Quelle, das kennt die Prüfung. Eine fehlende Quelle ist ein
    /// Fehler mit eigenem Satz; ohne sie (Gegenprobe) steht der Satz nicht da.
    #[test]
    fn data_ueber_mehrere_quellen_und_unlesbare_quelle() {
        let ort = std::env::temp_dir().join(format!("dashboard-pruefe-data-quellen-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&ort);
        let eins = ort.join("eins").join("projects");
        let zwei = ort.join("zwei").join("projects");
        std::fs::create_dir_all(eins.parent().unwrap().join("data")).unwrap();
        std::fs::create_dir_all(zwei.join("umgezogen")).unwrap();
        std::fs::create_dir_all(&eins).unwrap();
        std::fs::write(zwei.join("umgezogen/projekt.toml"), "schema = 3\ntitle = \"U\"\nstatus = \"aktiv\"\nsummary = \"S\"\n").unwrap();
        let daten = eins.parent().unwrap().join("data");
        std::fs::write(daten.join("schwerpunkte.toml"), "[[schwerpunkt]]\nname = \"A\"\nreihenfolge = 1\nprojekte = [\"umgezogen\"]\n").unwrap();
        std::fs::write(daten.join("gruppen.toml"), "[[gruppe]]\nslug = \"g\"\ntitel = \"G\"\nteile = [\"umgezogen\"]\n").unwrap();
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let a = args(&["--alle", "--ohne-stil"]);
        // Wie in der Ausgabe: Befund zeigt den Pfad ohne Benutzerordner.
        let unlesbar = Befund::fehler("DASHBOARD_PROJECTS_DIR", None, satz_unlesbar(&ort.join("weg"))).to_string();
        let (text, code) = lauf_quellen(&crate::quellen::Quellen::aus_liste(vec![eins.clone(), zwei.clone(), ort.join("weg")]), &repo, &a);
        let (ohne, _) = lauf_quellen(&crate::quellen::Quellen::aus_liste(vec![eins.clone(), zwei.clone()]), &repo, &a);
        let _ = std::fs::remove_dir_all(&ort);
        assert!(!text.contains("„umgezogen“ steht hier, aber") && !text.contains("„umgezogen“ in der Gruppe"), "{text}");
        assert!(text.contains(&format!("FEHLER {unlesbar}")) && unlesbar.contains("weg lässt sich nicht lesen"), "{text}");
        assert_eq!(code, 1, "{text}");
        assert!(!ohne.contains(&unlesbar), "{ohne}");
        assert!(!text.contains("alle 2 s") && !text.contains("letzten guten Stand"), "nur der Dienst versucht es wieder: {text}");
    }

    /// Aufruf aus dem Inhaltsordner: Arbeitsordner und Programmordner sind
    /// kein Code-Repo, dann zählt das Repo, aus dem gebaut wurde.
    #[test]
    fn code_repo_auch_aus_dem_inhaltsordner() {
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let inhalt = repo.join("tests").join("beispiel");
        assert_eq!(erstes_repo([inhalt.clone(), repo.join("web"), repo.clone()]), Some(repo));
        assert_eq!(erstes_repo([inhalt]), None);
    }
}
