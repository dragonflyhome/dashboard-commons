//! Wächter der Zusammenbau-Kiste `dashboard-lokal` ([zusammenbau],
//! Nachbesserung zum Umzug des Inhalts, Punkte 1, 2, 4 und 5).
//!
//! Er gehört zum Code-Repo und läuft als `dashboard.exe pruefe-zusammenbau
//! <ordner> --code <code-repo>`, bevor irgendein cargo-Aufruf den Ordner
//! anfasst (Dienst.ps1 -Neustart, Inhalt-Umziehen.ps1). Er liest nur Dateien
//! und fragt git; er baut nie und ruft cargo nie auf. So laufen weder
//! Bauskripte noch proc-macros, bevor er grün ist. Erst danach baut cargo mit
//! `--locked`.
//!
//! Er verlangt:
//! - `kisten.toml` streng: `[code].pfad` relativ und genau das Code-Repo
//!   (Schritt für Schritt gerechnet und auf der Platte aufgelöst); je Kiste
//!   `name`, `pfad` (`../projects/<slug>/server`), `typ`, `commit`;
//! - der Ordner ist die Vorlage `vorlagen/dashboard-lokal` des Code-Repos, Byte
//!   für Byte (Zeilenenden gleich behandelt), bis auf `kisten.toml`,
//!   `Cargo.lock` und die Zeilen der Kisten in `Cargo.toml` und `src/lib.rs`,
//!   die genau aus `kisten.toml` folgen; keine weitere Datei (kein `build.rs`,
//!   keine eigene `.cargo`-Einstellung), keine Verknüpfung; ein Ordner
//!   `target` zählt nicht;
//! - `Cargo.lock`: nur crates.io, und jede Kiste von dort mit derselben
//!   Fassung und Prüfsumme wie in `Cargo.lock` des Code-Repos; aus Pfaden nur
//!   die Bibliothek, das SDK, diese Kiste und die Kisten der Liste;
//! - im Inhalt (dashboard-lokal und der Ordner darüber) gar kein `.cargo`;
//!   dlltool gibt der Bau aus dem Code-Repo mit (`cargo --config
//!   <code-repo>/.cargo/config.toml`); darüber (bis zur Laufwerkswurzel) und
//!   in `CARGO_HOME` nur dlltool (der Name allein, ein Pfad in der aktiven
//!   Toolchain oder `<CARGO_HOME>/bin`), `[net]`, `[http]`, `[term]`; kein
//!   `rust-toolchain` darüber; keine Übersteuerung von rustup für den Ordner;
//! - keine Git-Einstellung (alle Ebenen außer system, mit eingebundenen
//!   Dateien), die beim Prüfen ein Programm starten könnte (Filter,
//!   diff-Programme, fsmonitor, hooksPath, include), und kein Attribut
//!   `filter` an den Dateien einer Kiste, kein Submodul (gitlink) und kein
//!   `.git` in einer Kiste; sonst sofort Schluss, ohne weiteren git-Aufruf;
//! - jeder git-Aufruf mit `--no-optional-locks`, `core.hooksPath` auf einen
//!   frischen, leeren Ordner außerhalb des Inhalts, ohne Transport
//!   (`protocol.allow=never`), mit `GIT_NO_LAZY_FETCH=1` und bei diff und
//!   status `--ignore-submodules=all` (siehe [`git`]);
//! - jede Kiste: die Wächter-Regeln des Code-Repos ([`super::waechter`], mit
//!   dem SDK genau dieses Code-Repos), liegt im Git-Repo des Inhalts, und
//!   seit ihrem `commit` unverändert, auch ohne neue oder von git ignorierte
//!   Dateien.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::dienst::programme::{self, Werkzeug};

use super::waechter;

/// Die einzige Quelle in Cargo.lock.
pub const CRATES_IO: &str = "registry+https://github.com/rust-lang/crates.io-index";

/// Die Vorlage im Code-Repo.
pub const VORLAGE: &str = "vorlagen/dashboard-lokal";

/// Eine Kiste aus `kisten.toml`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Kiste {
    pub name: String,
    pub pfad: String,
    pub typ: String,
    pub commit: String,
}

/// `kisten.toml`, geprüft.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Liste {
    /// `[code].pfad` wie geschrieben (mit `/`).
    pub code: String,
    pub kisten: Vec<Kiste>,
}

fn ist_kistenname(s: &str) -> bool {
    s.strip_prefix("projekt-").is_some_and(|r| {
        !r.is_empty() && !r.starts_with('-') && !r.ends_with('-') && r.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    })
}

fn ist_slug(s: &str) -> bool {
    !s.is_empty() && !s.starts_with(['.', '-', '_']) && s.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

fn ist_typ(s: &str) -> bool {
    let mut z = s.chars();
    z.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_') && z.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn ist_commit(s: &str) -> bool {
    (7..=40).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Liest `kisten.toml` streng. `Err` mit allen Sätzen.
pub fn liste_lesen(text: &str) -> Result<Liste, Vec<String>> {
    let mut f = Vec::new();
    let t: toml::Table =
        toml::from_str(text.strip_prefix('\u{feff}').unwrap_or(text)).map_err(|e| vec![format!("kisten.toml ist kein gültiges TOML: {}", e.message())])?;
    for k in t.keys() {
        if k != "code" && k != "kiste" {
            f.push(format!("kisten.toml: Den Abschnitt [{k}] gibt es nicht (nur [code] und [[kiste]])."));
        }
    }
    let code = match t.get("code").and_then(toml::Value::as_table) {
        Some(c) => {
            for k in c.keys().filter(|k| *k != "pfad") {
                f.push(format!("kisten.toml: [code] kennt nur pfad, nicht {k}."));
            }
            match c.get("pfad").and_then(toml::Value::as_str) {
                Some(p) => p.replace('\\', "/"),
                None => {
                    f.push("kisten.toml: [code].pfad fehlt (der Weg zum Code-Repo).".into());
                    String::new()
                }
            }
        }
        None => {
            f.push("kisten.toml: [code] mit pfad fehlt (der Weg zum Code-Repo).".into());
            String::new()
        }
    };
    let mut kisten = Vec::new();
    let roh = match t.get("kiste") {
        None => Vec::new(),
        Some(toml::Value::Array(a)) => a.clone(),
        Some(_) => {
            f.push("kisten.toml: kiste muss eine Liste [[kiste]] sein.".into());
            Vec::new()
        }
    };
    for (i, k) in roh.iter().enumerate() {
        let nr = i + 1;
        let Some(k) = k.as_table() else {
            f.push(format!("kisten.toml: Kiste {nr} ist keine Tabelle."));
            continue;
        };
        for schluessel in k.keys().filter(|s| !["name", "pfad", "typ", "commit"].contains(&s.as_str())) {
            f.push(format!("kisten.toml: Kiste {nr} kennt {schluessel} nicht (nur name, pfad, typ, commit)."));
        }
        let feld = |n: &str| k.get(n).and_then(toml::Value::as_str).map(str::to_string);
        let (Some(name), Some(pfad), Some(typ), Some(commit)) = (feld("name"), feld("pfad"), feld("typ"), feld("commit")) else {
            f.push(format!("kisten.toml: Kiste {nr} braucht name, pfad, typ und commit (je als Text)."));
            continue;
        };
        let pfad = pfad.replace('\\', "/");
        if !ist_kistenname(&name) {
            f.push(format!("kisten.toml: Kiste {nr}: name „{name}“ hat nicht die Form projekt-<slug> (Kleinbuchstaben, Ziffern, Bindestrich)."));
        }
        let slug_gut = pfad.strip_prefix("../projects/").and_then(|r| r.strip_suffix("/server")).is_some_and(ist_slug);
        if !slug_gut {
            f.push(format!("kisten.toml: {name}: pfad muss genau ../projects/<slug>/server sein."));
        }
        if !ist_typ(&typ) {
            f.push(format!("kisten.toml: {name}: typ „{typ}“ ist kein Rust-Typname."));
        }
        if !ist_commit(&commit) {
            f.push(format!("kisten.toml: {name}: commit „{commit}“ ist kein Commit (7 bis 40 Hex-Zeichen)."));
        }
        if kisten.iter().any(|x: &Kiste| x.name == name || x.pfad.eq_ignore_ascii_case(&pfad)) {
            f.push(format!("kisten.toml: {name} oder sein pfad steht doppelt da."));
        }
        kisten.push(Kiste { name, pfad, typ, commit });
    }
    if f.is_empty() { Ok(Liste { code, kisten }) } else { Err(f) }
}

/// Zeilenenden gleich behandeln (CRLF wie LF).
fn lf(b: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\r' && b.get(i + 1) == Some(&b'\n') {
            i += 1;
            continue;
        }
        v.push(b[i]);
        i += 1;
    }
    v
}

/// Die `Cargo.toml`, die aus der Vorlage und der Liste folgt (wie
/// Inhalt-Umziehen.ps1 sie schreibt).
pub fn erwartete_cargo(vorlage: &str, liste: &Liste) -> String {
    let v = vorlage.replace("\r\n", "\n");
    let v = v
        .replace("\"../../dashboard\"", &format!("\"{}\"", liste.code))
        .replace("\"../../dashboard/crates/dashboard-projekt\"", &format!("\"{}/crates/dashboard-projekt\"", liste.code));
    let mut zeilen: Vec<String> = liste.kisten.iter().map(|k| format!("{} = {{ path = \"{}\" }}", k.name, k.pfad)).collect();
    zeilen.push("# KISTEN-ENDE".into());
    v.replace("# KISTEN-ENDE", &zeilen.join("\n"))
}

/// Die `src/lib.rs`, die aus der Vorlage und der Liste folgt.
pub fn erwartete_lib(vorlage: &str, liste: &Liste) -> String {
    let v = vorlage.replace("\r\n", "\n");
    let mut zeilen: Vec<String> = liste.kisten.iter().map(|k| format!("        dashboard::eintrag({}::{}),", k.name.replace('-', "_"), k.typ)).collect();
    zeilen.push("        // KISTEN-ENDE".into());
    v.replace("        // KISTEN-ENDE", &zeilen.join("\n"))
}

/// Ist `p` eine Verknüpfung (Symlink, Junction, anderer Reparse-Punkt)?
fn ist_reparse(p: &Path) -> bool {
    let Ok(m) = std::fs::symlink_metadata(p) else { return false };
    if m.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        if m.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return true;
        }
    }
    false
}

/// Alle Dateien unter `ordner` (relativ, mit `/`), ohne einer Verknüpfung zu
/// folgen; Verknüpfungen und Unlesbares als Sätze. `ohne` sind Ordnernamen
/// direkt unter `ordner`, die nicht betreten werden.
fn dateien(ordner: &Path, ohne: &[&str], funde: &mut Vec<String>, anzeige: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut offen = vec![(ordner.to_path_buf(), String::new())];
    while let Some((o, rel)) = offen.pop() {
        let rd = match std::fs::read_dir(&o) {
            Ok(r) => r,
            Err(e) => {
                funde.push(format!("{anzeige}/{rel}: lässt sich nicht lesen ({e})."));
                continue;
            }
        };
        for e in rd.filter_map(Result::ok) {
            let name = e.file_name().to_string_lossy().into_owned();
            let r = if rel.is_empty() { name.clone() } else { format!("{rel}/{name}") };
            let p = e.path();
            if ist_reparse(&p) {
                funde.push(format!("{anzeige}/{r}: ist eine Verknüpfung; in der Zusammenbau-Kiste ist keine erlaubt."));
                continue;
            }
            if p.is_dir() {
                if rel.is_empty() && ohne.iter().any(|x| name.eq_ignore_ascii_case(x)) {
                    continue;
                }
                offen.push((p, r));
            } else {
                out.insert(r);
            }
        }
    }
    out
}

/// Vergleicht den Ordner mit der Vorlage des Code-Repos.
fn vorlage_vergleichen(lokal: &Path, vorlage: &Path, liste: &Liste, funde: &mut Vec<String>) {
    let mut soll: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut leer = Vec::new();
    for rel in dateien(vorlage, &[], &mut leer, "Vorlage") {
        if rel == "kisten.toml" {
            continue;
        }
        let Ok(b) = std::fs::read(vorlage.join(&rel)) else {
            funde.push(format!("Die Vorlage {VORLAGE}/{rel} lässt sich nicht lesen."));
            continue;
        };
        let b = match rel.as_str() {
            "Cargo.toml" => erwartete_cargo(&String::from_utf8_lossy(&b), liste).into_bytes(),
            "src/lib.rs" => erwartete_lib(&String::from_utf8_lossy(&b), liste).into_bytes(),
            _ => lf(&b),
        };
        soll.insert(rel, b);
    }
    funde.extend(leer.into_iter().map(|s| format!("Vorlage im Code-Repo: {s}")));
    let ist = dateien(lokal, &["target"], funde, "dashboard-lokal");
    for rel in &ist {
        if rel == "kisten.toml" || rel == "Cargo.lock" {
            continue;
        }
        match soll.get(rel) {
            None => funde.push(format!(
                "dashboard-lokal/{rel}: Diese Datei steht nicht in der Vorlage ({VORLAGE}). In der Zusammenbau-Kiste liegt nur, was die Vorlage hat (kein build.rs, keine eigene .cargo-Einstellung, keine weiteren Module)."
            )),
            Some(b) => match std::fs::read(lokal.join(rel)) {
                Ok(da) if lf(&da) == *b => {}
                Ok(_) => funde.push(format!(
                    "dashboard-lokal/{rel}: weicht von der Vorlage ab ({VORLAGE}; erlaubt sind nur die Zeilen der Kisten aus kisten.toml in Cargo.toml und src/lib.rs, genau in der Form der Vorlage)."
                )),
                Err(e) => funde.push(format!("dashboard-lokal/{rel}: lässt sich nicht lesen ({e}).")),
            },
        }
    }
    for rel in soll.keys().filter(|r| !ist.contains(*r)) {
        funde.push(format!("dashboard-lokal/{rel} fehlt (steht in der Vorlage {VORLAGE})."));
    }
    if !ist.contains("kisten.toml") {
        funde.push("dashboard-lokal/kisten.toml fehlt.".into());
    }
}

/// `Cargo.toml` der Kiste: eigener, leerer Workspace, kein [patch] und
/// Ähnliches (die Vorlage verlangt das schon; hier mit eigenem Satz).
fn cargo_pruefen(text: &str, funde: &mut Vec<String>) {
    let Ok(t) = toml::from_str::<toml::Table>(text) else {
        funde.push("dashboard-lokal/Cargo.toml ist kein gültiges TOML.".into());
        return;
    };
    match t.get("workspace").and_then(toml::Value::as_table) {
        Some(w) if w.is_empty() => {}
        Some(_) => funde.push("dashboard-lokal/Cargo.toml: [workspace] muss leer sein (keine members, exclude oder anderes).".into()),
        None => funde.push("dashboard-lokal/Cargo.toml: [workspace] fehlt (eigener, leerer Workspace).".into()),
    }
    for k in ["patch", "replace", "build-dependencies", "target", "profile"] {
        if t.get(k).is_some() {
            funde.push(format!("dashboard-lokal/Cargo.toml: [{k}] ist nicht erlaubt."));
        }
    }
    if t.get("package").and_then(|p| p.get("build")).is_some() || t.get("package").and_then(|p| p.get("links")).is_some() {
        funde.push("dashboard-lokal/Cargo.toml: kein Bauskript (build, links).".into());
    }
}

/// (Name, Fassung) -> Prüfsumme der Kisten von crates.io in einer Cargo.lock.
fn lock_lesen(text: &str) -> Result<(toml::Table, Vec<toml::Table>), String> {
    let t: toml::Table = toml::from_str(text).map_err(|e| e.message().to_string())?;
    let pakete = t.get("package").and_then(toml::Value::as_array).cloned().unwrap_or_default();
    let pakete = pakete.into_iter().filter_map(|p| p.as_table().cloned()).collect();
    Ok((t, pakete))
}

fn lock_pruefen(lokal: &Path, code_repo: &Path, liste: &Liste, funde: &mut Vec<String>) {
    let datei = lokal.join("Cargo.lock");
    let Ok(text) = std::fs::read_to_string(&datei) else {
        funde.push("dashboard-lokal/Cargo.lock fehlt oder lässt sich nicht lesen; sie gehört eingecheckt (cargo baut mit --locked). Neu schreiben: erst dieser Wächter mit --ohne-lock, dann im Ordner cargo update --workspace --offline (löst nur auf, baut nichts).".into());
        return;
    };
    let (kopf, pakete) = match lock_lesen(&text) {
        Ok(x) => x,
        Err(e) => {
            funde.push(format!("dashboard-lokal/Cargo.lock ist kein gültiges TOML: {e}"));
            return;
        }
    };
    for k in kopf.keys().filter(|k| *k != "version" && *k != "package") {
        funde.push(format!("dashboard-lokal/Cargo.lock: [{k}] ist nicht erlaubt (nur version und package)."));
    }
    let code: BTreeSet<(String, String, String)> =
        match std::fs::read_to_string(code_repo.join("Cargo.lock")).map_err(|e| e.to_string()).and_then(|t| lock_lesen(&t)) {
            Ok((_, p)) => p
                .iter()
                .filter(|p| p.get("source").and_then(toml::Value::as_str) == Some(CRATES_IO))
                .map(|p| {
                    let s = |k: &str| p.get(k).and_then(toml::Value::as_str).unwrap_or("").to_string();
                    (s("name"), s("version"), s("checksum"))
                })
                .collect(),
            Err(e) => {
                funde.push(format!("Cargo.lock des Code-Repos lässt sich nicht lesen ({e}); ohne sie kein Vergleich (geschlossen scheitern)."));
                return;
            }
        };
    let mut erlaubt: BTreeSet<String> = ["dashboard", "dashboard-projekt", "dashboard-lokal"].iter().map(|s| s.to_string()).collect();
    erlaubt.extend(liste.kisten.iter().map(|k| k.name.clone()));
    for p in &pakete {
        let s = |k: &str| p.get(k).and_then(toml::Value::as_str).unwrap_or("").to_string();
        let name = s("name");
        for k in p.keys().filter(|k| !["name", "version", "source", "checksum", "dependencies"].contains(&k.as_str())) {
            funde.push(format!("dashboard-lokal/Cargo.lock: {name} hat {k}; das ist nicht erlaubt."));
        }
        match p.get("source").and_then(toml::Value::as_str) {
            Some(q) if q == CRATES_IO => {
                if !code.iter().any(|(n, _, _)| *n == name) {
                    // Etappe 6a: Fällt eine Kiste aus dem Code-Repo, bleibt sie hier stehen.
                    funde.push(format!(
                        "dashboard-lokal/Cargo.lock: {name} {} nutzt das Code-Repo nicht mehr. Die Cargo.lock der Zusammenbau-Kiste offline mitziehen: in dashboard-lokal cargo tree --offline (mit CARGO_TARGET_DIR und --config des Code-Repos), dann den Wächter erneut.",
                        s("version")
                    ));
                } else if !code.contains(&(name.clone(), s("version"), s("checksum"))) {
                    funde.push(format!(
                        "dashboard-lokal/Cargo.lock: {name} {} steht so (Fassung und Prüfsumme) nicht in Cargo.lock des Code-Repos; die Zusammenbau-Kiste nimmt nur Kisten, die das Code-Repo schon nutzt.",
                        s("version")
                    ));
                }
            }
            Some(q) => funde.push(format!("dashboard-lokal/Cargo.lock: {name} kommt nicht von crates.io ({q}).")),
            None => {
                if !erlaubt.contains(&name) {
                    funde.push(format!("dashboard-lokal/Cargo.lock: {name} kommt aus einem Pfad und steht nicht in kisten.toml."));
                }
            }
        }
    }
}

/// Wo dlltool außer dem bloßen Namen liegen darf: im Ordner der aktiven
/// rustup-Toolchain (`<RUSTUP_HOME>/toolchains/<default_toolchain>`; andere
/// Toolchains verbieten Übersteuerung, Toolchain-Datei und RUSTUP_TOOLCHAIN)
/// oder als `<CARGO_HOME>/bin/llvm-dlltool.exe` (dort liegt die Kopie laut
/// Anleitung, „Bauen mit der GNU-Toolchain (llvm-dlltool)“). Nie im Inhalt.
#[derive(Debug, Clone, Default)]
pub struct DlltoolOrte {
    pub toolchain: Option<PathBuf>,
    pub cargo_bin: Option<PathBuf>,
}

impl DlltoolOrte {
    /// Aus der Umgebung dieses Rechners (RUSTUP_HOME, CARGO_HOME, USERPROFILE).
    pub fn aus_umgebung() -> DlltoolOrte {
        let profil = std::env::var_os("USERPROFILE").map(PathBuf::from);
        let rustup = std::env::var_os("RUSTUP_HOME").map(PathBuf::from).or_else(|| profil.as_ref().map(|p| p.join(".rustup")));
        let cargo = std::env::var_os("CARGO_HOME").map(PathBuf::from).or_else(|| profil.as_ref().map(|p| p.join(".cargo")));
        let toolchain = rustup.as_ref().and_then(|r| {
            let t: toml::Table = toml::from_str(&std::fs::read_to_string(r.join("settings.toml")).ok()?).ok()?;
            let name = t.get("default_toolchain")?.as_str()?.to_string();
            (!name.is_empty() && !name.contains(['/', '\\', ':']) && name != "." && name != "..").then(|| r.join("toolchains").join(name))
        });
        DlltoolOrte { toolchain, cargo_bin: cargo.map(|c| c.join("bin")) }
    }

    /// Ist `wert` (aus `-C dlltool=…`) erlaubt? Der bloße Name
    /// `llvm-dlltool.exe`, ein Pfad im Ordner der aktiven Toolchain oder genau
    /// `<CARGO_HOME>/bin/llvm-dlltool.exe`; nie etwas im Inhalt `inhalt`.
    pub fn erlaubt(&self, wert: &str, inhalt: Option<&Path>) -> bool {
        let w = wert.replace('\\', "/");
        if w.eq_ignore_ascii_case("llvm-dlltool.exe") {
            return true;
        }
        let p = Path::new(wert);
        if !p.is_absolute() || !w.to_ascii_lowercase().ends_with("/llvm-dlltool.exe") {
            return false;
        }
        let Some(pl) = waechter_lexikalisch(p) else { return false };
        let unter = |o: &Path| waechter_lexikalisch(o).is_some_and(|o| pl.starts_with(&format!("{}/", o.trim_end_matches('/'))));
        if inhalt.is_some_and(unter) {
            return false;
        }
        self.toolchain.as_deref().is_some_and(unter)
            || self.cargo_bin.as_deref().is_some_and(|b| waechter_lexikalisch(&b.join("llvm-dlltool.exe")) == Some(pl.clone()))
    }
}

/// Eine Einstellungsdatei von cargo in einem Ordner über dem Inhalt (oder in
/// CARGO_HOME): erlaubt sind nur dlltool als rustflags (wie im Code-Repo,
/// siehe [`DlltoolOrte`]), `[net]`, `[http]`, `[term]`, `[cargo-new]` und
/// `[build] jobs`.
pub fn konfig_pruefen(anzeige: &str, text: &str) -> Vec<String> {
    konfig_pruefen_mit(anzeige, text, &DlltoolOrte::aus_umgebung(), None)
}

/// Wie [`konfig_pruefen`], mit den erlaubten Orten für dlltool und dem Inhalt.
pub fn konfig_pruefen_mit(anzeige: &str, text: &str, orte: &DlltoolOrte, inhalt_ordner: Option<&Path>) -> Vec<String> {
    let mut f = Vec::new();
    let t: toml::Table = match toml::from_str(text) {
        Ok(t) => t,
        Err(e) => return vec![format!("{anzeige} ist kein gültiges TOML ({}); geschlossen scheitern.", e.message())],
    };
    for (k, v) in &t {
        match k.as_str() {
            "net" | "http" | "term" | "cargo-new" => {}
            "build" => {
                for b in v.as_table().map(|b| b.keys().cloned().collect::<Vec<_>>()).unwrap_or_default() {
                    if b != "jobs" {
                        f.push(format!("{anzeige}: [build] {b} ist nicht erlaubt (nur jobs)."));
                    }
                }
            }
            "target" => {
                for (ziel, inhalt) in v.as_table().cloned().unwrap_or_default() {
                    let Some(inhalt) = inhalt.as_table() else {
                        f.push(format!("{anzeige}: [target] {ziel} ist nicht erlaubt."));
                        continue;
                    };
                    for (sk, sv) in inhalt {
                        let gut = sk == "rustflags"
                            && sv.as_array().is_some_and(|a| {
                                a.len() == 2
                                    && a[0].as_str() == Some("-C")
                                    && a[1].as_str().and_then(|s| s.strip_prefix("dlltool=")).is_some_and(|d| orte.erlaubt(d, inhalt_ordner))
                            });
                        if !gut {
                            f.push(format!(
                                "{anzeige}: [target.{ziel}] {sk} ist nicht erlaubt (nur rustflags = [\"-C\", \"dlltool=llvm-dlltool.exe\"], der Name allein oder ein Pfad in der aktiven Toolchain oder <CARGO_HOME>\\bin; nie im Inhalt)."
                            ));
                        }
                    }
                }
            }
            anders => f.push(format!(
                "{anzeige}: [{anders}] ist nicht erlaubt (etwa source, patch, paths, registries, env, alias, unstable; sie änderten, was cargo baut)."
            )),
        }
    }
    f
}

/// Die Ordner, deren `.cargo`-Einstellung für `ordner` gilt: alle darüber bis
/// zur Laufwerkswurzel (ohne `ordner` selbst; seine Datei vergleicht die
/// Vorlage), dazu CARGO_HOME.
fn konfig_orte(ordner: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = ordner.ancestors().skip(1).map(|a| a.join(".cargo")).collect();
    let heim = std::env::var_os("CARGO_HOME").map(PathBuf::from).or_else(|| std::env::var_os("USERPROFILE").map(|u| PathBuf::from(u).join(".cargo")));
    if let Some(h) = heim
        && !v.iter().any(|x| crate::inhalt::seite::gleicher_pfad(x, &h))
    {
        v.push(h);
    }
    v
}

fn umfeld_pruefen(lokal: &Path, funde: &mut Vec<String>) {
    // Im Inhalt (dashboard-lokal und der Ordner darüber) gar keine Einstellung
    // von cargo: dlltool kommt beim Bau aus dem Code-Repo (cargo --config).
    let inhalt = lokal.parent();
    let im_inhalt = |o: &Path| {
        inhalt.is_some_and(|i| match (waechter_lexikalisch(o), waechter_lexikalisch(i)) {
            (Some(o), Some(i)) => o == i || o.starts_with(&format!("{}/", i.trim_end_matches('/'))),
            _ => false,
        })
    };
    let orte = DlltoolOrte::aus_umgebung();
    for o in konfig_orte(lokal) {
        let ordner = o.parent().unwrap_or(&o).to_path_buf();
        if im_inhalt(&ordner) || im_inhalt(&o) {
            if o.exists() {
                funde.push(format!(
                    "{}: Im Inhalt ist keine .cargo-Einstellung erlaubt (auch nicht für dlltool; das kommt beim Bau aus dem Code-Repo).",
                    o.display()
                ));
            }
            continue;
        }
        for name in ["config.toml", "config"] {
            let d = o.join(name);
            if d.is_file() {
                match std::fs::read_to_string(&d) {
                    Ok(t) => funde.extend(konfig_pruefen_mit(&d.display().to_string(), &t, &orte, inhalt)),
                    Err(e) => funde.push(format!("{} lässt sich nicht lesen ({e}); geschlossen scheitern.", d.display())),
                }
            }
        }
    }
    for a in lokal.ancestors().skip(1) {
        for name in ["rust-toolchain", "rust-toolchain.toml"] {
            if a.join(name).exists() {
                funde.push(format!(
                    "{}: Eine Toolchain-Datei über der Zusammenbau-Kiste ist nicht erlaubt (sie wählte, womit gebaut wird).",
                    a.join(name).display()
                ));
            }
        }
    }
    // rustup: Übersteuerungen je Ordner (rustup override set) gelten auch für Unterordner.
    let heim = std::env::var_os("RUSTUP_HOME").map(PathBuf::from).or_else(|| std::env::var_os("USERPROFILE").map(|u| PathBuf::from(u).join(".rustup")));
    if let Some(einst) = heim.map(|h| h.join("settings.toml"))
        && einst.is_file()
    {
        match std::fs::read_to_string(&einst).ok().and_then(|t| toml::from_str::<toml::Table>(&t).ok()) {
            None => funde.push(format!("{} lässt sich nicht lesen; geschlossen scheitern.", einst.display())),
            Some(t) => {
                let lok = waechter_lexikalisch(lokal);
                for p in t.get("overrides").and_then(toml::Value::as_table).map(|o| o.keys().cloned().collect::<Vec<_>>()).unwrap_or_default() {
                    let o = waechter_lexikalisch(Path::new(&p));
                    if let (Some(l), Some(o)) = (&lok, &o)
                        && (l == o || l.starts_with(&format!("{}/", o.trim_end_matches('/'))))
                    {
                        funde.push(format!(
                            "rustup übersteuert die Toolchain für {p} (rustup override); das gilt auch für die Zusammenbau-Kiste und ist nicht erlaubt."
                        ));
                    }
                }
            }
        }
    }
}

fn waechter_lexikalisch(p: &Path) -> Option<String> {
    let p = std::path::absolute(p).ok()?;
    Some(p.to_string_lossy().replace('\\', "/").to_lowercase())
}

/// Ist `pfad` genau `ziel`, Schritt für Schritt gerechnet und auf der Platte aufgelöst?
fn genau(pfad: &Path, ziel: &Path) -> bool {
    let a = std::path::absolute(pfad).ok().map(|p| p.to_string_lossy().replace('\\', "/").trim_end_matches('/').to_lowercase());
    let b = std::path::absolute(ziel).ok().map(|p| p.to_string_lossy().replace('\\', "/").trim_end_matches('/').to_lowercase());
    let c = (std::fs::canonicalize(pfad), std::fs::canonicalize(ziel));
    a.is_some() && a == b && matches!(c, (Ok(x), Ok(y)) if x == y)
}

/// Ein leerer Ordner, den der Wächter selbst anlegt (frisch im Temp-Ordner,
/// nie im Inhalt) und am Ende wieder entfernt. `core.hooksPath` jedes
/// git-Aufrufs zeigt dorthin; so läuft kein Hook aus `.git/hooks` des Inhalts
/// (etwa post-index-change, wenn git status den Index schriebe). Die Prüfung
/// von `core.hooksPath` in der Einstellung sieht diesen Standard-Ordner nicht.
struct LeereHaken(PathBuf);

impl LeereHaken {
    /// Legt den Ordner neu an (nie einen vorhandenen) und prüft, dass er
    /// nicht im Inhalt `inhalt` liegt.
    fn neu(inhalt: &Path) -> Result<LeereHaken, String> {
        let basis = std::env::temp_dir();
        for n in 0..100u32 {
            let zeit = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
            let p = basis.join(format!("dashboard-waechter-haken-{}-{zeit}-{n}", std::process::id()));
            match std::fs::create_dir(&p) {
                Ok(()) => {
                    let h = LeereHaken(p);
                    let im_inhalt = match (waechter_lexikalisch(&h.0), waechter_lexikalisch(inhalt)) {
                        (Some(h), Some(i)) => h.starts_with(&format!("{}/", i.trim_end_matches('/'))),
                        _ => true,
                    };
                    if im_inhalt {
                        return Err("Der leere Ordner für die Hooks läge im Inhalt (Temp-Ordner im Inhalt?); geschlossen scheitern.".into());
                    }
                    return Ok(h);
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(format!("Der leere Ordner für die Hooks lässt sich nicht anlegen ({e}); geschlossen scheitern.")),
            }
        }
        Err("Der leere Ordner für die Hooks lässt sich nicht anlegen; geschlossen scheitern.".into())
    }

    fn pfad(&self) -> &Path {
        &self.0
    }
}

impl Drop for LeereHaken {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Die Schalter, mit denen der Wächter jedes git ruft (vor dem Unterbefehl),
/// ohne `-C`: keine optionalen Sperren (git status schreibt den Index nicht;
/// git diff schreibt ihn trotzdem, dort hält allein der leere Hook-Ordner den
/// Hook post-index-change ab), Hooks nur aus dem leeren Ordner `haken`, kein
/// fsmonitor, kein Transport (auch nicht, wenn der Inhalt `protocol.<name>.allow`
/// setzt; ein Nachladen fehlender Objekte eines partial clone startete sonst
/// git fetch und damit etwa `remote.*.uploadpack`).
fn git_schalter(haken: &Path) -> Vec<String> {
    let mut v: Vec<String> = vec!["--no-optional-locks".into()];
    let mut c = |s: String| {
        v.push("-c".into());
        v.push(s);
    };
    c("core.fsmonitor=false".into());
    c("core.quotepath=off".into());
    c(format!("core.hooksPath={}", haken.to_string_lossy().replace('\\', "/")));
    c("protocol.allow=never".into());
    for p in ["file", "git", "ssh", "http", "https", "ext", "fd"] {
        c(format!("protocol.{p}.allow=never"));
    }
    v
}

/// git ohne Fenster, mit festen Schaltern gegen Programme aus dem Inhalt
/// ([`git_schalter`]), ohne die Umgebungsvariablen GIT_* (GIT_DIR,
/// GIT_CONFIG_*, GIT_EXTERNAL_DIFF …), mit `GIT_NO_LAZY_FETCH=1` (kein
/// Nachladen im partial clone) und bei `diff` und `status` mit
/// `--ignore-submodules=all` (kein git im Submodul mit ungeprüfter Einstellung).
fn git(wo: &Path, haken: &LeereHaken, args: &[&str]) -> Result<std::process::Output, String> {
    // Der Hook-Ordner muss leer sein, bei jedem Aufruf.
    if !std::fs::read_dir(haken.pfad()).is_ok_and(|mut r| r.next().is_none()) {
        return Err(format!("Der Hook-Ordner {} ist nicht mehr leer oder fehlt; geschlossen scheitern.", haken.pfad().display()));
    }
    let mut c = programme::befehl(Werkzeug::Git).map_err(|e| format!("{e}; ohne git kein Bau (geschlossen scheitern)."))?;
    for (k, _) in std::env::vars_os() {
        if k.to_string_lossy().to_ascii_uppercase().starts_with("GIT_") {
            c.env_remove(&k);
        }
    }
    c.env("GIT_NO_LAZY_FETCH", "1");
    c.args(git_schalter(haken.pfad())).arg("-C").arg(wo);
    if let Some((unter, rest)) = args.split_first() {
        c.arg(unter);
        if *unter == "diff" || *unter == "status" {
            c.arg("--ignore-submodules=all");
        }
        c.args(rest);
    }
    c.output().map_err(|e| format!("git lässt sich nicht starten ({e}); ohne git kein Bau (geschlossen scheitern)."))
}

/// Ist eine Git-Einstellung rot, weil sie beim Prüfen (git diff, status) ein
/// Programm starten oder fremde Einstellungen einbinden könnte? `scope` wie
/// `git config --show-scope` (system, global, local, worktree, command).
/// Die Einstellung des Systems (Program Files, nur Administratoren) gilt als
/// sicher; ihre Filter (git-lfs) laufen nur über ein Attribut, und Attribute
/// mit Filter prüft [`filter_attribute_pruefen`].
pub fn git_einstellung_rot(scope: &str, schluessel: &str, wert: &str) -> bool {
    if scope == "system" {
        return false;
    }
    let k = schluessel.to_ascii_lowercase();
    let w = wert.trim().to_ascii_lowercase();
    k.starts_with("filter.")
        || k.starts_with("include.")
        || k.starts_with("includeif.")
        || k == "diff.external"
        || (k.starts_with("diff.") && (k.ends_with(".command") || k.ends_with(".textconv")))
        || (k == "core.fsmonitor" && w != "false")
        || k == "core.attributesfile"
        || (k == "core.hookspath" && (scope == "local" || scope == "worktree"))
}

/// Alle Git-Einstellungen, die im Inhalt gelten, mit Herkunft (`git config
/// --list --show-origin --show-scope`: alle Ebenen, auch worktree und
/// eingebundene Dateien). Liest nur; startet kein Programm. Rot ist jede
/// Einstellung nach [`git_einstellung_rot`]; dann bricht der Wächter sofort
/// ab, ohne weiteren git-Aufruf.
fn git_einstellung_pruefen(inhalt: &Path, haken: &LeereHaken) -> Vec<String> {
    let o = match git(inhalt, haken, &["config", "--list", "--show-origin", "--show-scope", "-z"]) {
        Ok(o) if o.status.success() => o,
        Ok(o) => return vec![format!("git config im Inhalt scheiterte ({}); geschlossen scheitern.", String::from_utf8_lossy(&o.stderr).trim())],
        Err(e) => return vec![e],
    };
    let text = String::from_utf8_lossy(&o.stdout);
    let teile: Vec<&str> = text.split('\0').collect();
    let mut funde = Vec::new();
    for e in teile.chunks(3) {
        let [scope, herkunft, kv] = e else { continue };
        let (k, w) = kv.split_once('\n').unwrap_or((kv, ""));
        if git_einstellung_rot(scope, k, w) {
            funde.push(format!(
                "Git-Einstellung {k} (Ebene {scope}, {herkunft}) ist nicht erlaubt: Sie könnte beim Prüfen ein Programm starten. Der Wächter bricht sofort ab, ohne weiteren git-Aufruf."
            ));
        }
    }
    funde
}

/// Der Index einer Kiste (`git ls-files -s`, `git check-attr`; beides startet
/// keinen Filter): jeder gitlink (Modus 160000, ein Submodul) ist rot, denn
/// git startete darin ein eigenes git mit ungeprüfter Einstellung und
/// ungeprüften Attributen; ebenso jedes gesetzte Attribut `filter`, denn git
/// diff und git status könnten den Filter ausführen.
fn filter_attribute_pruefen(ordner: &Path, haken: &LeereHaken, name: &str) -> Vec<String> {
    let eintraege = match git(ordner, haken, &["ls-files", "-s", "-z", "--", "."]) {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).split('\0').filter(|s| !s.is_empty()).map(String::from).collect::<Vec<_>>(),
        Ok(o) => return vec![format!("{name}: git ls-files scheiterte ({}); geschlossen scheitern.", String::from_utf8_lossy(&o.stderr).trim())],
        Err(e) => return vec![e],
    };
    // Jede Zeile: „<modus> <objekt> <stufe>\t<pfad>“.
    let mut dateien = Vec::new();
    let mut f = Vec::new();
    for e in &eintraege {
        let Some((kopf, pfad)) = e.split_once('\t') else {
            f.push(format!("{name}: git ls-files -s lieferte eine unlesbare Zeile; geschlossen scheitern."));
            continue;
        };
        if kopf.split(' ').next() == Some("160000") {
            f.push(format!(
                "{name}: {pfad} ist ein Submodul (gitlink, Modus 160000). Submodule sind in einer Kiste nicht erlaubt: git startete darin ein eigenes git mit ungeprüfter Einstellung. Der Wächter bricht sofort ab."
            ));
        } else {
            dateien.push(pfad.to_string());
        }
    }
    if !f.is_empty() || dateien.is_empty() {
        return f;
    }
    let mut args: Vec<&str> = vec!["check-attr", "-z", "filter", "--"];
    args.extend(dateien.iter().map(String::as_str));
    match git(ordner, haken, &args) {
        Ok(o) if o.status.success() => {
            let text = String::from_utf8_lossy(&o.stdout);
            let teile: Vec<&str> = text.split('\0').collect();
            let mut f = Vec::new();
            for e in teile.chunks(3) {
                if let [pfad, _, wert] = e
                    && *wert != "unspecified"
                    && *wert != "unset"
                {
                    f.push(format!(
                        "{name}: {pfad} hat das Attribut filter={wert}; ein Filter könnte beim Prüfen ein Programm starten. Der Wächter bricht sofort ab."
                    ));
                }
            }
            f
        }
        Ok(o) => vec![format!("{name}: git check-attr scheiterte ({}); geschlossen scheitern.", String::from_utf8_lossy(&o.stderr).trim())],
        Err(e) => vec![e],
    }
}

/// Jedes `.git` (Datei oder Ordner) in einer Kiste, ohne einer Verknüpfung zu
/// folgen: ein eigenes oder eingebettetes Repo, auch ein ausgechecktes
/// Submodul. git startete darin ein eigenes git mit Einstellung und
/// Attributen, die der Wächter nicht geprüft hat.
fn git_in_kiste(ordner: &Path, anzeige: &str, name: &str) -> Vec<String> {
    let mut f = Vec::new();
    let mut offen = vec![(ordner.to_path_buf(), String::new())];
    while let Some((o, rel)) = offen.pop() {
        let rd = match std::fs::read_dir(&o) {
            Ok(r) => r,
            Err(e) => {
                f.push(format!("{name}: {anzeige}/{rel} lässt sich nicht lesen ({e}); geschlossen scheitern."));
                continue;
            }
        };
        for e in rd.filter_map(Result::ok) {
            let n = e.file_name().to_string_lossy().into_owned();
            let r = if rel.is_empty() { n.clone() } else { format!("{rel}/{n}") };
            let p = e.path();
            if n.eq_ignore_ascii_case(".git") {
                f.push(format!(
                    "{name}: {anzeige}/{r} ist ein eigenes Git-Repo in der Kiste (.git, etwa ein Submodul oder eingebettetes Repo). Eine Kiste gehört ganz zum Git-Repo des Inhalts; darin ruft der Wächter kein git auf."
                ));
                continue;
            }
            if !ist_reparse(&p) && p.is_dir() {
                offen.push((p, r));
            }
        }
    }
    f
}

/// Eine Kiste, erster Teil: Ort, Wächter-Regeln, Git-Repo des Inhalts.
/// `Some(ordner)`, wenn der Git-Stand danach geprüft werden kann.
fn kiste_vorpruefen(lokal: &Path, inhalt: &Path, code_repo: &Path, haken: &LeereHaken, k: &Kiste, funde: &mut Vec<String>) -> Option<PathBuf> {
    let slug = k.pfad.trim_start_matches("../projects/").trim_end_matches("/server");
    let ordner = lokal.join(&k.pfad);
    let soll = inhalt.join("projects").join(slug).join("server");
    let anzeige = format!("projects/{slug}/server");
    if !ordner.join("Cargo.toml").is_file() {
        funde.push(format!("{}: {anzeige}/Cargo.toml fehlt.", k.name));
        return None;
    }
    for p in [inhalt.join("projects"), inhalt.join("projects").join(slug), soll.clone()] {
        if ist_reparse(&p) {
            funde.push(format!("{}: {} ist eine Verknüpfung; Kisten liegen in echten Ordnern des Inhalts.", k.name, p.display()));
            return None;
        }
    }
    if !genau(&ordner, &soll) {
        funde.push(format!("{}: Der Ordner liegt nicht genau unter projects/{slug}/server im Inhalt.", k.name));
        return None;
    }
    for f in waechter::pruefe_modul(&ordner, &anzeige, code_repo) {
        funde.push(format!("{}: {f}", k.name));
    }
    // Vor jedem git-Aufruf in der Kiste: kein eigenes oder eingebettetes Repo.
    let eigene = git_in_kiste(&ordner, &anzeige, &k.name);
    if !eigene.is_empty() {
        funde.extend(eigene);
        return None;
    }
    match git(&ordner, haken, &["rev-parse", "--show-toplevel"]) {
        Ok(o) if o.status.success() => {
            let oben = PathBuf::from(String::from_utf8_lossy(&o.stdout).trim());
            if !genau(&oben, inhalt) {
                funde.push(format!("{}: Der Ordner gehört nicht zum Git-Repo des Inhalts (ein eigenes .git darin?).", k.name));
                return None;
            }
        }
        Ok(_) => {
            funde.push(format!("{}: Der Ordner liegt in keinem Git-Repo; ohne git kein geprüfter Stand.", k.name));
            return None;
        }
        Err(e) => {
            funde.push(e);
            return None;
        }
    }
    Some(ordner)
}

/// Eine Kiste, zweiter Teil (erst wenn Einstellungen und Attribute grün
/// sind): der geprüfte Commit, unverändert seitdem, keine neuen oder
/// ignorierten Dateien.
fn kiste_git_stand(ordner: &Path, haken: &LeereHaken, k: &Kiste, funde: &mut Vec<String>) {
    let lauf = |args: &[&str]| git(ordner, haken, args);
    let ziel = format!("{}^{{commit}}", k.commit);
    match lauf(&["cat-file", "-e", &ziel]) {
        Ok(o) if o.status.success() => {}
        Ok(_) => {
            funde.push(format!("{}: den geprüften Commit {} gibt es im Inhalts-Repo nicht.", k.name, k.commit));
            return;
        }
        Err(e) => {
            funde.push(e);
            return;
        }
    }
    match lauf(&["diff", "--quiet", "--no-ext-diff", "--no-textconv", &k.commit, "--", "."]) {
        Ok(o) if o.status.success() => {}
        Ok(o) if o.status.code() == Some(1) => {
            funde.push(format!("{}: seit dem geprüften Commit {} geändert. Erst prüfen lassen, dann commit in kisten.toml nachtragen.", k.name, k.commit))
        }
        Ok(o) => funde.push(format!("{}: git diff scheiterte ({}); geschlossen scheitern.", k.name, String::from_utf8_lossy(&o.stderr).trim())),
        Err(e) => funde.push(e),
    }
    // Neue Dateien, auch solche, die git ignoriert (target/, *.local …).
    match lauf(&["status", "--porcelain", "--ignored", "--untracked-files=all", "--", "."]) {
        Ok(o) if o.status.success() => {
            let n = String::from_utf8_lossy(&o.stdout).lines().filter(|z| !z.trim().is_empty()).count();
            if n > 0 {
                funde.push(format!(
                    "{}: {n} ungeprüfte Datei(en) im Ordner der Kiste (neu, geändert oder von git ignoriert). Nur der geprüfte Stand wird gebaut.",
                    k.name
                ));
            }
        }
        Ok(o) => funde.push(format!("{}: git status scheiterte ({}); geschlossen scheitern.", k.name, String::from_utf8_lossy(&o.stderr).trim())),
        Err(e) => funde.push(e),
    }
}

/// Prüft die Zusammenbau-Kiste `lokal` gegen das Code-Repo `code_repo`. Leer:
/// grün. `ohne_lock`: Cargo.lock noch nicht prüfen (nur vor `cargo update --workspace --offline`,
/// das die Datei schreibt, ohne etwas zu bauen; danach noch einmal ganz).
pub fn pruefe(lokal: &Path, code_repo: &Path, ohne_lock: bool) -> Vec<String> {
    let (Ok(lokal), Ok(code_repo)) = (std::path::absolute(lokal), std::path::absolute(code_repo)) else {
        return vec!["Die Pfade lassen sich nicht auflösen.".into()];
    };
    let (lokal, code_repo) = (lokal.as_path(), code_repo.as_path());
    let mut funde = Vec::new();
    let vorlage = code_repo.join(VORLAGE);
    if !code_repo.join("Cargo.toml").is_file() || !vorlage.join("Cargo.toml").is_file() {
        return vec![format!("{} ist kein Code-Repo des Dashboards (Cargo.toml oder {VORLAGE} fehlt).", code_repo.display())];
    }
    if !lokal.is_dir() {
        return vec![format!("{} ist kein Ordner.", lokal.display())];
    }
    if ist_reparse(lokal) {
        return vec![format!("{} ist eine Verknüpfung; die Zusammenbau-Kiste liegt in einem echten Ordner.", lokal.display())];
    }
    let (Some(l), Some(c)) = (waechter_lexikalisch(lokal), waechter_lexikalisch(code_repo)) else {
        return vec!["Die Pfade lassen sich nicht auflösen.".into()];
    };
    if l.starts_with(&format!("{c}/")) || c.starts_with(&format!("{l}/")) || l == c {
        return vec!["Die Zusammenbau-Kiste liegt nie im Code-Repo (und das Code-Repo nie in ihr).".into()];
    }
    let liste = match std::fs::read_to_string(lokal.join("kisten.toml")) {
        Ok(t) => match liste_lesen(&t) {
            Ok(l) => l,
            Err(f) => return f,
        },
        Err(e) => return vec![format!("dashboard-lokal/kisten.toml lässt sich nicht lesen ({e}).")],
    };
    // [code].pfad ist genau das Code-Repo.
    if liste.code.is_empty() || liste.code.starts_with('/') || liste.code.contains(':') {
        funde.push("kisten.toml: [code].pfad muss ein relativer Weg sein (kein Laufwerk, nichts Absolutes).".into());
    } else if !genau(&lokal.join(&liste.code), code_repo) {
        funde.push(format!(
            "kisten.toml: [code].pfad „{}“ zeigt nicht genau auf das Code-Repo {} (Schritt für Schritt und aufgelöst).",
            liste.code,
            code_repo.display()
        ));
    }
    vorlage_vergleichen(lokal, &vorlage, &liste, &mut funde);
    if let Ok(t) = std::fs::read_to_string(lokal.join("Cargo.toml")) {
        cargo_pruefen(&t, &mut funde);
    }
    if !ohne_lock {
        lock_pruefen(lokal, code_repo, &liste, &mut funde);
    }
    umfeld_pruefen(lokal, &mut funde);
    if !liste.kisten.is_empty() {
        match lokal.parent() {
            Some(inhalt) => {
                // Jeder git-Aufruf bekommt einen frischen, leeren Hook-Ordner außerhalb des Inhalts.
                let haken = match LeereHaken::neu(inhalt) {
                    Ok(h) => h,
                    Err(e) => {
                        funde.push(e);
                        return funde;
                    }
                };
                // Erst alle Git-Einstellungen (nur lesen); ist eine rot, sofort Schluss.
                let rot = git_einstellung_pruefen(inhalt, &haken);
                if !rot.is_empty() {
                    funde.extend(rot);
                    return funde;
                }
                let mut bereit = Vec::new();
                for k in &liste.kisten {
                    if let Some(o) = kiste_vorpruefen(lokal, inhalt, code_repo, &haken, k, &mut funde) {
                        bereit.push((o, k));
                    }
                }
                // Dann die Filter-Attribute aller Kisten; ist eins gesetzt, kein git diff/status.
                let rot: Vec<String> = bereit.iter().flat_map(|(o, k)| filter_attribute_pruefen(o, &haken, &k.name)).collect();
                if !rot.is_empty() {
                    funde.extend(rot);
                    return funde;
                }
                for (o, k) in &bereit {
                    kiste_git_stand(o, &haken, k, &mut funde);
                }
            }
            None => funde.push("Die Zusammenbau-Kiste hat keinen Ordner darüber (den Inhalt).".into()),
        }
    }
    funde
}

/// Umgebungsvariablen, die ändern würden, womit cargo baut (Wrapper, Flags,
/// Quellen, Toolchain). Erlaubt: CARGO_TARGET_DIR, CARGO_HOME (dessen
/// Einstellung wird geprüft), CARGO_TERM_*, CARGO_NET_*, CARGO_HTTP_*,
/// CARGO_INCREMENTAL, CARGO_BUILD_JOBS.
pub fn umgebung_pruefen(vars: impl IntoIterator<Item = (String, String)>) -> Vec<String> {
    let mut f = Vec::new();
    for (k, _) in vars {
        let n = k.to_ascii_uppercase();
        let erlaubt = ["CARGO_TARGET_DIR", "CARGO_HOME", "CARGO_INCREMENTAL", "CARGO_BUILD_JOBS"].contains(&n.as_str())
            || ["CARGO_TERM_", "CARGO_NET_", "CARGO_HTTP_"].iter().any(|p| n.starts_with(p));
        let verdaechtig =
            n.starts_with("CARGO_") || n.starts_with("RUSTC") || n == "RUSTFLAGS" || n == "RUSTDOCFLAGS" || n == "RUSTDOC" || n == "RUSTUP_TOOLCHAIN";
        if verdaechtig && !erlaubt {
            f.push(format!("Die Umgebungsvariable {n} ist gesetzt; sie änderte, womit cargo baut. Bitte entfernen."));
        }
    }
    f
}

/// `dashboard.exe pruefe-zusammenbau <ordner> --code <code-repo> [--ohne-lock]`.
/// Rückgabe 0 grün, 1 rot, 2 falscher Aufruf.
pub fn ausfuehren(args: &[String]) -> i32 {
    let mut ordner = None;
    let mut code = None;
    let mut ohne_lock = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--code" => {
                code = args.get(i + 1).cloned();
                i += 1;
            }
            "--ohne-lock" => ohne_lock = true,
            a if !a.starts_with("--") && ordner.is_none() => ordner = Some(a.to_string()),
            a => {
                eprintln!(
                    "pruefe-zusammenbau: Das Argument „{a}“ kenne ich nicht. Aufruf: dashboard.exe pruefe-zusammenbau <ordner> --code <code-repo> [--ohne-lock]"
                );
                return 2;
            }
        }
        i += 1;
    }
    let (Some(ordner), Some(code)) = (ordner, code) else {
        eprintln!("pruefe-zusammenbau: Aufruf: dashboard.exe pruefe-zusammenbau <ordner> --code <code-repo> [--ohne-lock]");
        return 2;
    };
    let mut funde = umgebung_pruefen(std::env::vars());
    funde.extend(pruefe(Path::new(&ordner), Path::new(&code), ohne_lock));
    for f in &funde {
        println!("ROT {f}");
    }
    if funde.is_empty() {
        println!("Wächter der Zusammenbau-Kiste: grün{}.", if ohne_lock { " (ohne Cargo.lock)" } else { "" });
        0
    } else {
        println!("Wächter der Zusammenbau-Kiste: rot ({} Fund(e)). Es wird nicht gebaut.", funde.len());
        1
    }
}
