//! Wächter für die Backend-Module (`projects/*/server`, Etappe 4b), wie
//! `kein_command_ausserhalb` für den Dienst: eine Ordnung, keine Isolation.
//! Er ist ein Textscanner und schützt vor Versehen, nicht vor Absicht; gegen
//! Absicht schützt die Prüfung jedes Merge-Requests.
//!
//! Je `.rs`-Datei unter `server/` (auch `tests/`, ohne `target/`):
//! - `#![forbid(unsafe_code)]` steht darin, und sonst kein `unsafe`;
//! - kein `std::fs`, `std::net`, `std::env`, `std::process`, `thread::spawn`,
//!   `static mut`, `Command` (auch nicht als `use std::{fs, …}`), kein `#[path]`;
//! - von tokio nichts außer Tests: kein `tokio::fs`, `tokio::net`,
//!   `tokio::process`, `tokio::spawn`, `task::spawn`, `spawn_blocking` (der
//!   Workspace vereinigt die Features des Dienstes; im Endpunkt
//!   `k.im_hintergrund` nutzen); kein `ProjektKontext::neu`, kein
//!   `#[ts(export_to)]`;
//! - `include_str!` und `include_bytes!` nur mit einem schlichten Pfad im
//!   eigenen Ordner (kein `..`, kein Laufwerk, nicht absolut); `include!` nie.
//!
//! `Cargo.toml` des Moduls:
//! - nur Kisten aus [`GEPRUEFTE_KISTEN`], `tokio` nur unter
//!   `[dev-dependencies]`, nie die Kiste `dashboard`;
//! - `dashboard-projekt` nur mit dem Weg zum SDK genau dieses Code-Repos
//!   (vom Ordner `server` aus aufgelöst, [`ist_sdk_pfad`]), ohne Features;
//!   unter `[dev-dependencies]` höchstens `pruefstand`;
//! - kein Bauskript (`build.rs`, `build = …`, `links`), kein proc-macro, kein
//!   `[patch]` oder `[replace]`, kein `path` in `[lib]`, `[[bin]]`,
//!   `[[test]]`, `[[example]]`, `[[bench]]`.
//!
//! Kommentare zählen nicht; Text in Zeichenketten schon (lieber einmal zu streng).

use std::path::{Path, PathBuf};

/// Kisten, die ein Modul nutzen darf (Liste des Betreuers des Dashboards). Eine neue
/// braucht eine Entscheidung der Architektur-Rolle und einen Baunachweis.
pub const GEPRUEFTE_KISTEN: &[&str] = &["dashboard-projekt", "serde", "serde_json", "ts-rs"];

/// Kisten, die nur unter `[dev-dependencies]` stehen dürfen.
pub const NUR_FUER_TESTS: &[&str] = &["tokio"];

/// Der Weg zum SDK von `projects/<slug>/server` im Code-Repo aus (nur die
/// Vorlage liegt dort); in einer Inhaltsquelle steht dort der Weg zum
/// Code-Repo daneben, etwa `../../../../dashboard/crates/dashboard-projekt`.
pub const SDK_PFAD: &str = "../../../crates/dashboard-projekt";

/// Wo ein Modul liegt und welches Code-Repo gilt: Daraus folgt der einzige
/// erlaubte Weg zum SDK (`<code_repo>/crates/dashboard-projekt`).
#[derive(Debug, Clone, Copy)]
pub struct SdkOrt<'a> {
    /// Der Ordner `server` des Moduls (dort steht seine Cargo.toml).
    pub server: &'a Path,
    /// Das Code-Repo, dessen SDK gilt.
    pub code_repo: &'a Path,
}

/// Ein Pfad ohne `.` und `..`, Schritt für Schritt gerechnet, ohne einer
/// Verknüpfung zu folgen; Groß/Klein und Trenner wie unter Windows gleich.
/// `None`, wenn `..` über die Wurzel hinaus führt.
fn lexikalisch(p: &Path) -> Option<String> {
    use std::path::Component;
    let p = std::path::absolute(p).ok()?;
    let mut teile: Vec<String> = Vec::new();
    let mut vorne = String::new();
    for k in p.components() {
        match k {
            Component::Prefix(x) => vorne = x.as_os_str().to_string_lossy().to_lowercase(),
            Component::RootDir => {}
            Component::CurDir => {}
            Component::ParentDir => {
                teile.pop()?;
            }
            Component::Normal(n) => teile.push(n.to_string_lossy().to_lowercase()),
        }
    }
    Some(format!("{vorne}/{}", teile.join("/")))
}

/// Ist `pfad` (aus `server/Cargo.toml`) genau der Weg zum SDK des Code-Repos
/// `ort.code_repo`? Relativ (kein Laufwerk, nichts Absolutes), und vom Ordner
/// `server` aus genau `<code_repo>/crates/dashboard-projekt`: einmal Schritt
/// für Schritt gerechnet (ohne einer Verknüpfung zu folgen) und einmal auf der
/// Platte aufgelöst (kanonisch). Ein gleichnamiger Ordner anderswo, etwa im
/// Inhalt, zählt nicht, auch nicht über eine Verknüpfung.
pub fn ist_sdk_pfad(pfad: &str, ort: SdkOrt) -> bool {
    let p = pfad.replace('\\', "/");
    if p.trim().is_empty() || p.starts_with('/') || p.contains(':') {
        return false;
    }
    let ist = ort.server.join(&p);
    let soll = ort.code_repo.join("crates").join("dashboard-projekt");
    let gleich_gerechnet = matches!((lexikalisch(&ist), lexikalisch(&soll)), (Some(a), Some(b)) if a == b);
    let gleich_aufgeloest = matches!((std::fs::canonicalize(&ist), std::fs::canonicalize(&soll)), (Ok(a), Ok(b)) if a == b);
    gleich_gerechnet && gleich_aufgeloest
}

const VERBOTEN: [(&str, &str); 15] = [
    ("std::fs", "Kein std::fs: Dateien nur über den ProjektKontext (eigene TOML) lesen."),
    ("std::net", "Kein std::net: Module greifen nicht aufs Netz zu."),
    ("std::env", "Kein std::env: Einstellungen kommen aus den eigenen TOML-Dateien."),
    ("std::process", "Kein std::process: Kindprozesse startet nur der Dienst (dienst::programme)."),
    ("thread::spawn", "Kein thread::spawn: der Dienst ruft Hooks in eigenen Aufgaben auf; im Endpunkt k.im_hintergrund nutzen."),
    ("static mut", "Kein static mut: Zustand gehört in die eigene Tabelle des Datenspeichers."),
    ("Command", "Kein Command: Kindprozesse startet nur der Dienst (dienst::programme)."),
    ("tokio::fs", "Kein tokio::fs: Dateien nur über den ProjektKontext (eigene TOML) lesen."),
    ("tokio::net", "Kein tokio::net: Module greifen nicht aufs Netz zu."),
    ("tokio::process", "Kein tokio::process: Kindprozesse startet nur der Dienst (dienst::programme)."),
    ("tokio::spawn", "Kein tokio::spawn: Hintergrundarbeit nur über die Hooks oder k.im_hintergrund."),
    ("task::spawn", "Kein task::spawn: Hintergrundarbeit nur über die Hooks oder k.im_hintergrund."),
    ("spawn_blocking", "Kein spawn_blocking: im Endpunkt k.im_hintergrund nutzen."),
    ("ProjektKontext::neu", "Kein ProjektKontext::neu: Kontexte baut nur der Dienst (in Tests der Prüfstand)."),
    ("export_to", "Kein #[ts(export_to)]: die Typen gehen fest nach web/src/lib/vertrag/projekte/<slug>/."),
];

/// Ein Fund: Datei (relativ), Zeile (ab 1, 0 = ganze Datei) und Satz.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fund {
    pub datei: String,
    pub zeile: usize,
    pub satz: String,
}

impl std::fmt::Display for Fund {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.zeile == 0 { write!(f, "{}: {}", self.datei, self.satz) } else { write!(f, "{}, Zeile {}: {}", self.datei, self.zeile, self.satz) }
    }
}

/// Entfernt Kommentare (`//…` und `/* … */`), Zeilen bleiben erhalten.
/// Zeichenketten werden dabei grob erkannt, damit `"//"` darin bleibt.
fn ohne_kommentare(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let zeichen: Vec<char> = text.chars().collect();
    let (mut i, mut block, mut kette) = (0, 0usize, false);
    while i < zeichen.len() {
        let c = zeichen[i];
        let n = zeichen.get(i + 1).copied();
        if block > 0 {
            if c == '*' && n == Some('/') {
                block -= 1;
                i += 2;
                continue;
            }
            if c == '/' && n == Some('*') {
                block += 1;
                i += 2;
                continue;
            }
            if c == '\n' {
                out.push('\n');
            }
            i += 1;
            continue;
        }
        if kette {
            out.push(c);
            if c == '\\' {
                if let Some(n) = n {
                    out.push(n);
                    i += 2;
                    continue;
                }
            } else if c == '"' {
                kette = false;
            }
            i += 1;
            continue;
        }
        match (c, n) {
            ('/', Some('/')) => {
                while i < zeichen.len() && zeichen[i] != '\n' {
                    i += 1;
                }
            }
            ('/', Some('*')) => {
                block = 1;
                i += 2;
            }
            ('"', _) => {
                kette = true;
                out.push(c);
                i += 1;
            }
            _ => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
}

fn zeile_von(text: &str, stelle: usize) -> usize {
    text[..stelle].matches('\n').count() + 1
}

fn ist_wortzeichen(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Alle Stellen, an denen `wort` als ganzes Wort (bzw. Pfad) steht.
fn stellen<'a>(text: &'a str, wort: &'a str) -> impl Iterator<Item = usize> + 'a {
    text.match_indices(wort).map(|(i, _)| i).filter(move |&i| {
        let davor = text[..i].chars().next_back();
        let danach = text[i + wort.len()..].chars().next();
        !davor.is_some_and(ist_wortzeichen) && !danach.is_some_and(ist_wortzeichen)
    })
}

/// Prüft eine `.rs`-Datei; `datei` ist der Pfad für die Sätze.
pub fn pruefe_rs(datei: &str, text: &str) -> Vec<Fund> {
    let mut funde = Vec::new();
    let mut fund = |zeile: usize, satz: &str| funde.push(Fund { datei: datei.to_string(), zeile, satz: satz.to_string() });
    let rein = ohne_kommentare(text);
    let forbid = "#![forbid(unsafe_code)]";
    if !rein.lines().any(|z| z.trim() == forbid) {
        fund(0, "Es fehlt #![forbid(unsafe_code)] am Anfang der Datei.");
    }
    let ohne_forbid = rein.replace(forbid, &" ".repeat(forbid.len()));
    for i in stellen(&ohne_forbid, "unsafe") {
        fund(zeile_von(&ohne_forbid, i), "Kein unsafe in Backend-Modulen.");
    }
    for (muster, satz) in VERBOTEN {
        for i in stellen(&rein, muster) {
            fund(zeile_von(&rein, i), satz);
        }
    }
    // `use std::{fs, …}` und `use ::std::{…}`.
    for (i, _) in rein.match_indices("std::{") {
        let gruppe = rein[i..].split('}').next().unwrap_or("");
        for (teil, (_, satz)) in ["fs", "net", "env", "process"].iter().zip(VERBOTEN.iter()) {
            if stellen(gruppe, teil).next().is_some() {
                fund(zeile_von(&rein, i), satz);
            }
        }
        if stellen(gruppe, "thread").next().is_some() && rein.contains("spawn") {
            fund(zeile_von(&rein, i), VERBOTEN[4].1);
        }
    }
    // `use tokio::{fs, …}` und Ähnliches.
    for (i, _) in rein.match_indices("tokio::{") {
        let gruppe = rein[i..].split('}').next().unwrap_or("");
        for teil in ["fs", "net", "process", "spawn", "task"] {
            if stellen(gruppe, teil).next().is_some() {
                fund(zeile_von(&rein, i), &format!("Kein tokio::{teil} in Backend-Modulen; tokio nur für Tests."));
            }
        }
    }
    for i in stellen(&rein, "path").filter(|&i| rein[..i].trim_end().ends_with("#[")) {
        fund(zeile_von(&rein, i), "Kein #[path = …]: Moduldateien liegen in server/src.");
    }
    for (i, _) in rein.match_indices("include!") {
        if !rein[..i].chars().next_back().is_some_and(ist_wortzeichen) {
            fund(zeile_von(&rein, i), "Kein include!: Rust-Code eines Moduls steht nur in seinen eigenen Dateien.");
        }
    }
    for makro in ["include_str!", "include_bytes!"] {
        for (i, _) in rein.match_indices(makro) {
            if rein[..i].chars().next_back().is_some_and(ist_wortzeichen) {
                continue;
            }
            let rest = rein[i + makro.len()..].trim_start();
            let arg = rest.strip_prefix('(').map(str::trim_start).unwrap_or("");
            let pfad = arg.strip_prefix('"').and_then(|a| a.split('"').next());
            let gut = pfad
                .is_some_and(|p| !p.is_empty() && !p.contains("..") && !p.contains(':') && !p.starts_with('/') && !p.starts_with('\\') && !p.contains("\\\\"));
            if !gut {
                fund(zeile_von(&rein, i), &format!("{makro} nur mit einem schlichten Pfad im eigenen Ordner (kein .., kein Laufwerk, kein concat!/env!)."));
            }
        }
    }
    funde
}

/// Prüft `Cargo.toml` eines Moduls; `datei` ist der Pfad für die Sätze, `ort`
/// sagt, wo das Modul liegt und welches Code-Repo gilt (Weg zum SDK).
pub fn pruefe_cargo(datei: &str, text: &str, ort: SdkOrt) -> Vec<Fund> {
    let mut funde = Vec::new();
    let mut fund = |satz: String| funde.push(Fund { datei: datei.to_string(), zeile: 0, satz });
    let wert: toml::Table = match toml::from_str(text) {
        Ok(w) => w,
        Err(e) => {
            fund(format!("Cargo.toml ist kein gültiges TOML: {}", e.message()));
            return funde;
        }
    };
    if wert.get("package").and_then(|p| p.get("build")).is_some() {
        fund("Kein Bauskript (build = …) in Backend-Modulen.".into());
    }
    if wert.get("package").and_then(|p| p.get("links")).is_some() {
        fund("Kein links = … in Backend-Modulen (das gehört zu einem Bauskript).".into());
    }
    // Ein proc-macro liefe beim Bauen (wie ein Bauskript), vor jedem Wächter.
    let lib = wert.get("lib");
    if ["proc-macro", "proc_macro"].iter().any(|k| lib.and_then(|l| l.get(*k)).and_then(toml::Value::as_bool) == Some(true)) {
        fund("Kein proc-macro: Ein Modul ist eine gewöhnliche Bibliothek; Makros liefen beim Bauen.".into());
    }
    for k in ["patch", "replace"] {
        if wert.get(k).is_some() {
            fund(format!("Kein [{k}] in Backend-Modulen: Abhängigkeiten nur aus crates.io und das SDK."));
        }
    }
    let mut tabellen: Vec<(String, &toml::Table)> = Vec::new();
    for art in ["dependencies", "dev-dependencies", "build-dependencies"] {
        if let Some(t) = wert.get(art).and_then(toml::Value::as_table) {
            tabellen.push((art.to_string(), t));
        }
    }
    if let Some(ziele) = wert.get("target").and_then(toml::Value::as_table) {
        for (ziel, inhalt) in ziele {
            for art in ["dependencies", "dev-dependencies", "build-dependencies"] {
                if let Some(t) = inhalt.get(art).and_then(toml::Value::as_table) {
                    tabellen.push((format!("target.{ziel}.{art}"), t));
                }
            }
        }
    }
    for (art, t) in tabellen {
        let test_abh = art.ends_with("dev-dependencies");
        for (name, angabe) in t {
            let kiste = angabe.get("package").and_then(toml::Value::as_str).unwrap_or(name);
            if kiste == "dashboard" || name == "dashboard" {
                fund(format!("[{art}] {name}: Ein Modul hängt nie von der Kiste dashboard ab, nur vom SDK dashboard-projekt."));
                continue;
            }
            if NUR_FUER_TESTS.contains(&kiste) {
                if !test_abh {
                    fund(format!("[{art}] {name}: {kiste} nur unter [dev-dependencies] (für Tests); im Modul kommt die Laufzeit über das SDK."));
                }
            } else if !GEPRUEFTE_KISTEN.contains(&kiste) {
                fund(format!(
                    "[{art}] {name}: Diese Kiste steht nicht in der Liste geprüfter Kisten ({}; tokio nur für Tests). Eine neue braucht eine Entscheidung der Architektur-Rolle.",
                    GEPRUEFTE_KISTEN.join(", ")
                ));
            }
            for schluessel in ["git", "registry"] {
                if angabe.get(schluessel).is_some() {
                    fund(format!("[{art}] {name}: Abhängigkeiten nur aus crates.io (wie Cargo.lock), kein {schluessel}."));
                }
            }
            let pfad = angabe.get("path").and_then(toml::Value::as_str).map(|p| p.replace('\\', "/"));
            if kiste == "dashboard-projekt" {
                if !pfad.as_deref().is_some_and(|p| ist_sdk_pfad(p, ort)) {
                    fund(format!(
                        "[{art}] {name}: dashboard-projekt nur mit path zum SDK genau dieses Code-Repos (im Code-Repo \"{SDK_PFAD}\", in einer Inhaltsquelle der Weg zum Code-Repo daneben, etwa \"../../../../dashboard/crates/dashboard-projekt\"); ein anderer Ordner, auch ein gleichnamiger, zählt nicht."
                    ));
                }
                let features: Vec<&str> =
                    angabe.get("features").and_then(toml::Value::as_array).map(|a| a.iter().filter_map(toml::Value::as_str).collect()).unwrap_or_default();
                let erlaubt: &[&str] = if test_abh { &["pruefstand"] } else { &[] };
                if let Some(f) = features.iter().find(|f| !erlaubt.contains(f)) {
                    fund(format!("[{art}] {name}: Feature „{f}“ ist nicht erlaubt (nur „pruefstand“ unter [dev-dependencies])."));
                }
            } else if pfad.is_some() {
                fund(format!("[{art}] {name}: Ein Pfad ist nur zum SDK erlaubt ({SDK_PFAD})."));
            }
        }
    }
    for ziel in ["lib", "bin", "test", "example", "bench"] {
        let eintraege: Vec<&toml::Value> = match wert.get(ziel) {
            Some(toml::Value::Array(a)) => a.iter().collect(),
            Some(v) => vec![v],
            None => Vec::new(),
        };
        if eintraege.iter().any(|e| e.get("path").is_some()) {
            fund(format!("[{ziel}] mit path ist nicht erlaubt: Dateien des Moduls liegen an den üblichen Orten unter server/."));
        }
    }
    funde
}

/// Alle `.rs`-Dateien unter `ordner` (ohne `target/`), sortiert.
fn rs_dateien(ordner: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut offen = vec![ordner.to_path_buf()];
    while let Some(o) = offen.pop() {
        let Ok(rd) = std::fs::read_dir(&o) else { continue };
        for e in rd.filter_map(Result::ok) {
            let p = e.path();
            if p.is_dir() {
                if e.file_name() != "target" {
                    offen.push(p);
                }
            } else if p.extension().is_some_and(|x| x == "rs") {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

/// Prüft ein ganzes Modul (`projects/<slug>/server`); `anzeige` ist der Pfad
/// für die Sätze (etwa `projects/beispiel/server`), `code_repo` das
/// Code-Repo, dessen SDK gilt.
pub fn pruefe_modul(server: &Path, anzeige: &str, code_repo: &Path) -> Vec<Fund> {
    let mut funde = Vec::new();
    match std::fs::read_to_string(server.join("Cargo.toml")) {
        Ok(t) => funde.extend(pruefe_cargo(&format!("{anzeige}/Cargo.toml"), &t, SdkOrt { server, code_repo })),
        Err(_) => funde.push(Fund { datei: format!("{anzeige}/Cargo.toml"), zeile: 0, satz: "Cargo.toml fehlt.".into() }),
    }
    if server.join("build.rs").exists() {
        funde.push(Fund { datei: format!("{anzeige}/build.rs"), zeile: 0, satz: "Kein Bauskript (build.rs) in Backend-Modulen.".into() });
    }

    let dateien = rs_dateien(server);
    if dateien.is_empty() {
        funde.push(Fund { datei: format!("{anzeige}/src"), zeile: 0, satz: "Es gibt keine .rs-Datei unter server/.".into() });
    }
    for p in dateien {
        let rel = p.strip_prefix(server).unwrap_or(&p).to_string_lossy().replace('\\', "/");
        match std::fs::read_to_string(&p) {
            Ok(t) => funde.extend(pruefe_rs(&format!("{anzeige}/{rel}"), &t)),
            Err(_) => funde.push(Fund { datei: format!("{anzeige}/{rel}"), zeile: 0, satz: "Die Datei lässt sich nicht als UTF-8 lesen.".into() }),
        }
    }
    funde
}

/// Alle Module unter `projects/*/server` eines Projektordners: (Anzeige, Funde).
pub fn pruefe_alle(projekte: &Path, code_repo: &Path) -> Vec<(String, Vec<Fund>)> {
    let Ok(rd) = std::fs::read_dir(projekte) else { return Vec::new() };
    let mut out: Vec<(String, Vec<Fund>)> = rd
        .filter_map(Result::ok)
        .filter(|e| e.path().join("server").is_dir())
        .map(|e| {
            let anzeige = format!("projects/{}/server", e.file_name().to_string_lossy());
            let funde = pruefe_modul(&e.path().join("server"), &anzeige, code_repo);
            (anzeige, funde)
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const GUT_RS: &str = "#![forbid(unsafe_code)]\n//! Kommentar mit std::fs und Command, zählt nicht.\nuse serde::Serialize;\n/* auch std::net hier nicht */\npub const T: &str = include_str!(\"text.txt\");\npub fn x() -> &'static str { \"a // b\" }\n";
    const GUT_CARGO: &str = "[package]\nname = \"projekt-probe\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\ndashboard-projekt = { path = \"../../../crates/dashboard-projekt\" }\nserde = { version = \"1\", features = [\"derive\"] }\n\n[dev-dependencies]\ntokio = { version = \"1\" }\n";

    /// Legt ein Testmodul im Temp-Ordner an (nie in echten Projekten), mit
    /// einem kleinen Code-Repo darum: `<basis>/crates/dashboard-projekt` und
    /// das Modul unter `<basis>/projects/probe/server` (wie die Vorlage).
    fn modul(name: &str, cargo: &str, dateien: &[(&str, &str)]) -> PathBuf {
        let basis = std::env::temp_dir().join(format!("dashboard-waechter-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&basis);
        std::fs::create_dir_all(basis.join("crates").join("dashboard-projekt")).unwrap();
        let ordner = basis.join("projects").join("probe").join("server");
        std::fs::create_dir_all(ordner.join("src")).unwrap();
        std::fs::write(ordner.join("Cargo.toml"), cargo).unwrap();
        for (pfad, text) in dateien {
            let p = ordner.join(pfad);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, text).unwrap();
        }
        ordner
    }

    /// Das kleine Code-Repo um ein Testmodul.
    fn basis(server: &Path) -> &Path {
        server.parent().and_then(Path::parent).and_then(Path::parent).unwrap()
    }

    fn aufraeumen(ordner: &Path) {
        let _ = std::fs::remove_dir_all(basis(ordner));
    }

    fn saetze(funde: &[Fund]) -> String {
        funde.iter().map(ToString::to_string).collect::<Vec<_>>().join("\n")
    }

    /// Prüft eine Cargo.toml so, als läge sie in der Vorlage dieses Code-Repos.
    fn cargo_saetze(text: &str) -> String {
        let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
        let server = repo.join("projects").join("_vorlage").join("server");
        saetze(&pruefe_cargo("Cargo.toml", text, SdkOrt { server: &server, code_repo: repo }))
    }

    /// Die Module im Code-Repo (nur die Vorlage; Rechenteile der Projekte
    /// liegen in der Inhaltsquelle) halten alle Regeln ein.
    #[test]
    fn echte_module_sind_gruen() {
        let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
        let alle = pruefe_alle(&repo.join("projects"), repo);
        let namen: Vec<&str> = alle.iter().map(|(n, _)| n.as_str()).collect();
        assert!(namen.contains(&"projects/_vorlage/server"), "{namen:?}");
        for (name, funde) in &alle {
            assert!(funde.is_empty(), "{name}:\n{}", saetze(funde));
        }
    }

    /// Nachbesserung Umzug 3: Der Weg zum SDK ist genau der zum Code-Repo,
    /// aufgelöst; ein gleichnamiger Ordner im Inhalt (drei Schritte hinauf
    /// statt vier), ein absoluter Weg und eine Verknüpfung gelten nicht.
    #[test]
    fn sdk_pfad_nur_zum_code_repo() {
        let basis = std::env::temp_dir().join(format!("dashboard-waechter-{}-sdk", std::process::id()));
        let _ = std::fs::remove_dir_all(&basis);
        let code = basis.join("code");
        let inhalt = basis.join("inhalt");
        let server = inhalt.join("projects").join("p").join("server");
        for o in [
            code.join("crates/dashboard-projekt"),
            inhalt.join("code/crates/dashboard-projekt"),
            inhalt.join("projects/code/crates/dashboard-projekt"),
            server.clone(),
        ] {
            std::fs::create_dir_all(o).unwrap();
        }
        let ort = SdkOrt { server: &server, code_repo: &code };
        for gut in
            ["../../../../code/crates/dashboard-projekt", "..\\..\\..\\..\\code\\crates\\dashboard-projekt\\", "../../../../code/crates/x/../dashboard-projekt"]
        {
            assert!(ist_sdk_pfad(gut, ort), "{gut}");
        }
        let absolut = code.join("crates").join("dashboard-projekt").to_string_lossy().replace('\\', "/");
        for schlecht in [
            // drei Schritte: ein gleichnamiger Ordner im Inhalt (das Loch von hoch.len() >= 3)
            "../../../code/crates/dashboard-projekt",
            // zwei Schritte: unter projects
            "../../code/crates/dashboard-projekt",
            "../../../../../code/crates/dashboard-projekt",
            "../../../../code/crates/anderes",
            "../../../../code/crates/dashboard-projekt/..",
            "../../../../gibt-es-nicht/crates/dashboard-projekt",
            absolut.as_str(),
            "/code/crates/dashboard-projekt",
            "",
        ] {
            assert!(!ist_sdk_pfad(schlecht, ort), "{schlecht}");
        }
        // Eine Verknüpfung im Inhalt auf das echte Code-Repo zählt nicht.
        if crate::inhalt::seite::junction_anlegen(&inhalt.join("verweis"), &code) {
            assert!(!ist_sdk_pfad("../../../verweis/crates/dashboard-projekt", ort));
        }
        // In der Cargo.toml: genau der Weg ist grün, drei Schritte sind rot.
        let gut = GUT_CARGO.replace("../../../crates/dashboard-projekt", "../../../../code/crates/dashboard-projekt");
        assert!(saetze(&pruefe_cargo("Cargo.toml", &gut, ort)).is_empty());
        let drei = GUT_CARGO.replace("../../../crates/dashboard-projekt", "../../../code/crates/dashboard-projekt");
        assert!(saetze(&pruefe_cargo("Cargo.toml", &drei, ort)).contains("genau dieses Code-Repos"));
        let _ = std::fs::remove_dir_all(&basis);
        // Im Code-Repo selbst: der Weg der Vorlage.
        assert!(cargo_saetze(GUT_CARGO).is_empty());
    }

    /// Nachbesserung Umzug 1: proc-macro, links, [patch] und [replace] sind rot.
    #[test]
    fn proc_macro_und_patch_sind_rot() {
        for (zusatz, satz) in [
            ("[lib]\nproc-macro = true\n", "Kein proc-macro"),
            ("[lib]\nproc_macro = true\n", "Kein proc-macro"),
            ("[patch.crates-io]\nserde = { path = \"x\" }\n", "Kein [patch]"),
            ("[replace]\n\"serde:1.0.0\" = { path = \"x\" }\n", "Kein [replace]"),
        ] {
            let s = cargo_saetze(&format!("{GUT_CARGO}\n{zusatz}"));
            assert!(s.contains(satz), "{zusatz}: {s}");
        }
        let s = cargo_saetze(&GUT_CARGO.replace("edition = \"2024\"", "edition = \"2024\"\nlinks = \"x\""));
        assert!(s.contains("Kein links"), "{s}");
    }

    #[test]
    fn gutes_testmodul_ist_gruen() {
        let m = modul("gut", GUT_CARGO, &[("src/lib.rs", GUT_RS), ("src/teil/mod.rs", "#![forbid(unsafe_code)]\npub fn y() {}\n")]);
        let funde = pruefe_modul(&m, "projects/probe/server", basis(&m));
        aufraeumen(&m);
        assert!(funde.is_empty(), "{}", saetze(&funde));
    }

    /// Abnahme: ein Testmodul mit unsafe, std::fs oder Zugriff auf die Kiste dashboard ist rot.
    #[test]
    fn unsafe_ist_rot() {
        let m = modul("unsafe", GUT_CARGO, &[("src/lib.rs", "pub fn x() { unsafe { std::hint::unreachable_unchecked() } }\n")]);
        let funde = pruefe_modul(&m, "projects/probe/server", basis(&m));
        aufraeumen(&m);
        let s = saetze(&funde);
        assert!(s.contains("Es fehlt #![forbid(unsafe_code)]"), "{s}");
        assert!(s.contains("projects/probe/server/src/lib.rs, Zeile 1: Kein unsafe"), "{s}");
    }

    #[test]
    fn std_fs_ist_rot() {
        let m = modul(
            "fs",
            GUT_CARGO,
            &[
                ("src/lib.rs", "#![forbid(unsafe_code)]\npub fn x() { let _ = std::fs::read(\"a\"); }\n"),
                ("src/b.rs", "#![forbid(unsafe_code)]\nuse std::{collections::BTreeMap, fs};\n"),
            ],
        );
        let funde = pruefe_modul(&m, "projects/probe/server", basis(&m));
        aufraeumen(&m);
        let s = saetze(&funde);
        assert!(s.contains("src/lib.rs, Zeile 2: Kein std::fs"), "{s}");
        assert!(s.contains("src/b.rs, Zeile 2: Kein std::fs"), "{s}");
    }

    #[test]
    fn abhaengigkeit_von_dashboard_ist_rot() {
        let cargo = format!("{GUT_CARGO}\n[dependencies.dashboard]\npath = \"../../..\"\n");
        let m = modul("dashboard", &cargo, &[("src/lib.rs", GUT_RS)]);
        let funde = pruefe_modul(&m, "projects/probe/server", basis(&m));
        aufraeumen(&m);
        assert!(saetze(&funde).contains("nie von der Kiste dashboard"), "{}", saetze(&funde));
        let umbenannt = GUT_CARGO.replace("serde = {", "kern = { package = \"dashboard\", path = \"../../..\" }\nserde = {");
        let s = cargo_saetze(&umbenannt);
        assert!(s.contains("kern: Ein Modul hängt nie von der Kiste dashboard ab"), "{s}");
    }

    #[test]
    fn weitere_verbote_sind_rot() {
        let faelle = [
            ("use std::net::TcpStream;", "Kein std::net"),
            ("let v = std::env::var(\"X\");", "Kein std::env"),
            ("let _ = std::process::exit(1);", "Kein std::process"),
            ("std::thread::spawn(|| {});", "Kein thread::spawn"),
            ("static mut ZAHL: u32 = 0;", "Kein static mut"),
            ("let c = Command { };", "Kein Command"),
            ("const T: &str = include_str!(\"../../../data/pc.toml\");", "include_str! nur"),
            ("const T: &[u8] = include_bytes!(concat!(env!(\"X\"), \"/a\"));", "include_bytes! nur"),
            ("#[path = \"../fremd.rs\"]\nmod fremd;", "Kein #[path"),
        ];
        for (zeile, erwartet) in faelle {
            let text = format!("#![forbid(unsafe_code)]\n{zeile}\n");
            let s = saetze(&pruefe_rs("lib.rs", &text));
            assert!(s.contains(erwartet), "{zeile}: {s}");
        }
        // Kein Treffer in Wörtern, die nur so ähnlich heißen.
        let s = saetze(&pruefe_rs("lib.rs", "#![forbid(unsafe_code)]\nfn commands() {}\nlet unsafe_wort = 1;\n"));
        assert!(s.is_empty(), "{s}");
        let cargo = GUT_CARGO.replace("[dev-dependencies]", "reqwest = \"0.12\"\n\n[dev-dependencies]");
        let s = cargo_saetze(&cargo);
        assert!(s.contains("reqwest: Diese Kiste steht nicht in der Liste"), "{s}");
        let s = cargo_saetze(&format!("{GUT_CARGO}\n[package.metadata]\n").replace("edition = \"2024\"", "edition = \"2024\"\nbuild = \"build.rs\""));
        assert!(s.contains("Kein Bauskript"), "{s}");
    }

    /// Nachbesserung 1: tokio nur für Tests; im Modulcode kein tokio::fs,
    /// tokio::net, tokio::process, tokio::spawn, spawn_blocking.
    #[test]
    fn tokio_im_modul_ist_rot() {
        let faelle = [
            ("let _ = tokio::fs::read(\"a\");", "Kein tokio::fs"),
            ("let _ = tokio::net::TcpStream::connect(\"x\");", "Kein tokio::net"),
            ("let _ = tokio::process::Command { };", "Kein tokio::process"),
            ("tokio::spawn(async {});", "Kein tokio::spawn"),
            ("tokio::task::spawn(async {});", "Kein task::spawn"),
            ("tokio::task::spawn_blocking(|| ());", "Kein spawn_blocking"),
            ("use tokio::task::spawn_blocking;", "Kein spawn_blocking"),
            ("use tokio::{fs, io};", "Kein tokio::fs"),
            ("use tokio::{net::TcpListener};", "Kein tokio::net"),
            ("let k = ProjektKontext::neu(\"x\", vec![], a);", "Kein ProjektKontext::neu"),
            ("include!(\"anderes.rs\");", "Kein include!"),
            ("#[ts(export_to = \"../fremd/\")]", "Kein #[ts(export_to)]"),
        ];
        for (zeile, erwartet) in faelle {
            let text = format!("#![forbid(unsafe_code)]\n{zeile}\n");
            let s = saetze(&pruefe_rs("lib.rs", &text));
            assert!(s.contains(erwartet), "{zeile}: {s}");
        }
        // #[tokio::test] in Tests bleibt erlaubt.
        let s = saetze(&pruefe_rs("tests/a.rs", "#![forbid(unsafe_code)]\n#[tokio::test]\nasync fn t() {}\n"));
        assert!(s.is_empty(), "{s}");
        let cargo = GUT_CARGO.replace("serde = {", "tokio = { version = \"1\", features = [\"fs\"] }\nserde = {");
        let s = cargo_saetze(&cargo);
        assert!(s.contains("[dependencies] tokio: tokio nur unter [dev-dependencies]"), "{s}");
    }

    /// Nachbesserung 3: Ziele mit path, SDK ohne path oder mit Feature, alle .rs unter server/.
    #[test]
    fn cargo_ziele_und_sdk_ohne_pfad_sind_rot() {
        for ziel in [
            "[lib]\npath = \"../../fremd/lib.rs\"\n",
            "[[bin]]\nname = \"x\"\npath = \"src/x.rs\"\n",
            "[[test]]\nname = \"t\"\npath = \"../t.rs\"\n",
            "[[example]]\nname = \"e\"\npath = \"e.rs\"\n",
            "[[bench]]\nname = \"b\"\npath = \"b.rs\"\n",
        ] {
            let s = cargo_saetze(&format!("{GUT_CARGO}\n{ziel}"));
            assert!(s.contains("mit path ist nicht erlaubt"), "{ziel}: {s}");
        }
        let ohne_pfad = GUT_CARGO.replace("dashboard-projekt = { path = \"../../../crates/dashboard-projekt\" }", "dashboard-projekt = \"0.1\"");
        assert!(cargo_saetze(&ohne_pfad).contains("dashboard-projekt nur mit path"));
        let feature = GUT_CARGO.replace("crates/dashboard-projekt\" }", "crates/dashboard-projekt\", features = [\"dienst\"] }");
        assert!(cargo_saetze(&feature).contains("Feature „dienst“ ist nicht erlaubt"));
        let test_feature = GUT_CARGO.replace(
            "[dev-dependencies]\n",
            "[dev-dependencies]\ndashboard-projekt = { path = \"../../../crates/dashboard-projekt\", features = [\"pruefstand\"] }\n",
        );
        assert!(cargo_saetze(&test_feature).is_empty());
        // Auch tests/ und Dateien neben src/ werden geprüft.
        let m = modul(
            "tests",
            GUT_CARGO,
            &[("src/lib.rs", GUT_RS), ("tests/a.rs", "#![forbid(unsafe_code)]\nfn f() { let _ = std::fs::read(\"x\"); }\n"), ("anderswo.rs", "fn g() {}\n")],
        );
        let s = saetze(&pruefe_modul(&m, "projects/probe/server", basis(&m)));
        aufraeumen(&m);
        assert!(s.contains("projects/probe/server/tests/a.rs, Zeile 2: Kein std::fs"), "{s}");
        assert!(s.contains("projects/probe/server/anderswo.rs: Es fehlt #![forbid(unsafe_code)]"), "{s}");
    }
}
