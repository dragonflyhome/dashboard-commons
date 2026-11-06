//! Gegentests für den Pre-Push-Prüfer `scripts/Vor-Push.ps1`. Jeder
//! Fall baut ein kleines Wegwerf-Repo im Temp-Ordner, prüft es wie der Hook
//! oder `Hochladen.ps1` und muss abgewiesen werden; ein sauberes Repo geht
//! durch. Die Namenslisten sind erfunden und liegen nur im Temp-Ordner.
//! Die Liste `nur_basis` gilt nur im Basis-Repo, nicht in einem Projekt-Repo
//! oder im Inhalt. Welche Art ein Repo hat, sagt nur die Adresse des Remotes
//! (`-RemoteUrl`, vom Hook) mit der Liste `pruefer/repos.toml`, nie der Baum.
//! Alle Namen und Adressen hier sind erfunden.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use dashboard::dienst::programme::{self, Werkzeug};
use std::sync::atomic::{AtomicUsize, Ordering};

const NULL: &str = "0000000000000000000000000000000000000000";
/// Die einzige erlaubte Commit-Adresse.
const MAIL: &str = "ranke2@proton.me";
const TYPEN: [&str; 11] = ["feat", "fix", "docs", "style", "refactor", "perf", "test", "build", "ci", "chore", "revert"];
const SENSIBEL: &str = "sensibel = [\"geheimprojekt\", \"vorname nachname\", \"gruenkohlfest\", \"kaffee-runde\"]";
const LISTE: &str = "# erfundene Listen\nsensibel = [\"geheimprojekt\", \"vorname nachname\", \"gruenkohlfest\", \"kaffee-runde\"]\nallgemein = [\"alpha\", \"beta-zwei\"]\ntabu = [\"nie hier\"]\nbenutzer = [\"realname\"]\nrechner = [\"pc-erfunden\"]\nwlan = [\"wlan-erfunden\"]\nschluessel = [\"dienst-erfunden\"]\nmac = []\nnur_basis = [\"nebelfink\", \"kanal-erfunden\"]\n";
/// Erfundene Adressen der Remotes: je eine für Basis, Projekt und Inhalt, mit
/// Groß/Klein im Rechnernamen und / oder .git am Ende (das zählt nicht).
const REPOS: &str = "# erfundene Adressen\n[repos]\n\"https://git.example.invalid/team/basis.git\" = \"Basis\"   # Basis\n\"https://Git.Example.invalid/Team/projekt.git/\" = \"Projekt\"\n\"git@git.example.invalid:team/inhalt.git\" = \"Inhalt\"\n";
/// Erfundene Liste eines Chats in `pruefer/tokens/` (vereint mit `sensibel`).
const TOKENS: &str = "# erfunden\nsensibel = [\n  \"tokenwort\",  # Kommentar\n]\n";
/// Die Adresse des Projekt-Repos, wie der Hook sie bekommen könnte.
const PROJEKT_ADRESSE: &str = "https://git.example.INVALID/Team/projekt";
/// Die Adresse des Inhalts-Repos in der scp-Form, mit / am Ende.
const INHALT_ADRESSE: &str = "git@GIT.example.invalid:team/inhalt/";

/// PowerShell 7, sonst Windows PowerShell (beide an festen Orten, ohne Fenster).
fn pwsh() -> Option<Command> {
    [Werkzeug::PowerShell7, Werkzeug::WindowsPowerShell].into_iter().find_map(|w| programme::befehl(w).ok())
}

/// git an festem Ort, ohne Fenster.
fn git() -> Command {
    programme::befehl(Werkzeug::Git).expect("git an festem Ort")
}

fn skript() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scripts").join("Vor-Push.ps1")
}

/// Ein Wegwerf-Repo mit Listen daneben.
struct Ort {
    wurzel: PathBuf,
}

static ZAEHLER: AtomicUsize = AtomicUsize::new(0);

impl Ort {
    fn neu() -> Ort {
        let o = Ort::leer();
        // Ein sauberer Grundstock, der auch Fehltreffer prüft (Platzhalter, erlaubte Adressen).
        o.datei("Cargo.toml", b"[package]\nname = \"x\"\n");
        o.datei("src/main.rs", b"// Pfad: %USERPROFILE%\\dev\\dashboard oder C:/Users/<name>/dev, Test C:\\Users\\x\\a.md\nfn main() { let _ = \"ranke2@proton.me\"; let _ = \"a@b.invalid\"; }\n");
        o.datei("src/usePasswordManager.svelte", b"<p>Code, kein Geheimnis</p>\n");
        o.datei("tests/beispiel/projects/alpha/notiz.txt", b"projects/alpha ist hier erlaubt.\n");
        o.datei("LIESMICH.txt", b"Klartext statt Markdown.\n");
        o.datei("projects/_vorlage/projekt.toml", b"schema = 3\n");
        o.datei("docs/zielbild/a-grundsatz.toml", b"titel = \"Grundsatz\"\n");
        o.datei("docs/anleitung/a-start.toml", b"titel = \"Start\"\n");
        o.datei("web/static/icons/a.png", &png(false));
        o.commit("Wurzel", &[]);
        o
    }

    /// Ein Projekt-Repo (Overlay): nur projects/alpha/** und die drei Wurzeldateien.
    fn projekt() -> Ort {
        let o = Ort::leer();
        o.datei("dashboard-version.toml", b"tag = \"v0.1.0\"\n");
        o.datei(".gitignore", b"/ziel\n");
        o.datei(".gitattributes", b"* text=auto eol=lf\n");
        o.datei("projects/alpha/projekt.toml", b"schema = 3\n");
        o.datei("projects/alpha/assets/a.png", &png(false));
        o.datei("projects/alpha/server/src/lib.rs", b"// Domaenenlogik\n");
        o.commit("Wurzel", &[]);
        o
    }

    /// Repo und Listen ohne Dateien und ohne Commit.
    fn leer() -> Ort {
        let n = ZAEHLER.fetch_add(1, Ordering::SeqCst);
        let wurzel = std::env::temp_dir().join(format!("dashboard-vorpush-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&wurzel);
        std::fs::create_dir_all(wurzel.join("repo")).unwrap();
        std::fs::create_dir_all(wurzel.join("inhalt").join("pruefer")).unwrap();
        std::fs::write(wurzel.join("inhalt").join("pruefer").join("namen.toml"), LISTE).unwrap();
        std::fs::write(wurzel.join("inhalt").join("pruefer").join("repos.toml"), REPOS).unwrap();
        let tokens = wurzel.join("inhalt").join("pruefer").join("tokens");
        std::fs::create_dir_all(&tokens).unwrap();
        std::fs::write(tokens.join("erfunden.toml"), TOKENS).unwrap();
        // Kein TOML: zählt nicht (nur *.toml).
        std::fs::write(tokens.join("LIESMICH.txt"), "Je Chat eine Datei <slug>.toml.\n").unwrap();
        let o = Ort { wurzel };
        o.git(&["init", "-q", "-b", "main"]);
        o.git(&["config", "core.autocrlf", "false"]);
        o
    }

    /// Schreibt `pruefer/ausnahmen/<name>.toml` im Inhalt.
    fn ausnahmen(&self, name: &str, inhalt: &str) {
        let ordner = self.wurzel.join("inhalt").join("pruefer").join("ausnahmen");
        std::fs::create_dir_all(&ordner).unwrap();
        std::fs::write(ordner.join(format!("{name}.toml")), inhalt).unwrap();
    }

    fn repo(&self) -> PathBuf {
        self.wurzel.join("repo")
    }

    fn git(&self, args: &[&str]) -> String {
        let o = git().arg("-C").arg(self.repo()).args(args).output().unwrap();
        assert!(o.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&o.stderr));
        String::from_utf8_lossy(&o.stdout).trim().to_string()
    }

    /// Schreibt eine Datei. Ein „§“ in Text fällt weg: So stehen die verbotenen
    /// Muster nicht wörtlich in diesem Test (sonst wiese der Prüfer ihn selbst ab).
    fn datei(&self, pfad: &str, inhalt: &[u8]) {
        let inhalt = match std::str::from_utf8(inhalt) {
            Ok(s) => s.replace('§', "").into_bytes(),
            Err(_) => inhalt.to_vec(),
        };
        let p = self.repo().join(pfad);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, inhalt).unwrap();
    }

    /// Commit mit Ranke (oder anderen Angaben in `wer`: Name, Adresse für Autor und Committer).
    fn commit(&self, nachricht: &str, wer: &[(&str, &str)]) -> String {
        // Der Push prüft die erste Zeile (Conventional Commits): ohne Typ davor „test: “.
        let typ = nachricht.split([':', '(', '!']).next().unwrap_or_default();
        let nachricht = &if TYPEN.contains(&typ) { nachricht.to_string() } else { format!("test: {nachricht}") };
        let mut c = git();
        c.arg("-C").arg(self.repo());
        for (k, v) in [("GIT_AUTHOR_NAME", "Ranke"), ("GIT_AUTHOR_EMAIL", MAIL), ("GIT_COMMITTER_NAME", "Ranke"), ("GIT_COMMITTER_EMAIL", MAIL)] {
            c.env(k, v);
        }
        for (k, v) in wer {
            c.env(k, v);
        }
        c.args(["add", "-A"]).output().unwrap();
        let mut c2 = git();
        c2.arg("-C").arg(self.repo()).args(["commit", "-q", "--allow-empty", "-m", nachricht]);
        for (k, v) in [("GIT_AUTHOR_NAME", "Ranke"), ("GIT_AUTHOR_EMAIL", MAIL), ("GIT_COMMITTER_NAME", "Ranke"), ("GIT_COMMITTER_EMAIL", MAIL)] {
            c2.env(k, v);
        }
        for (k, v) in wer {
            c2.env(k, v);
        }
        let o = c2.output().unwrap();
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        self.git(&["rev-parse", "HEAD"])
    }

    /// Wie `Hochladen.ps1`: ganzer Baum von main, alle Commits, nur Merge-Commits auf main.
    fn pruefen(&self) -> (i32, String) {
        self.lauf(&["-Baum", "main", "-Commits", "main", "-AufMain"], None)
    }

    /// Wie `pruefen`, aber mit der Zusicherung `-Art` (Basis, Projekt oder Inhalt).
    fn pruefen_als(&self, art: &str) -> (i32, String) {
        self.lauf(&["-Art", art, "-Baum", "main", "-Commits", "main", "-AufMain"], None)
    }

    /// Wie `pruefen`, mit der Adresse des Remotes (oder ohne, wie von Hand).
    fn pruefen_mit(&self, adresse: Option<&str>) -> (i32, String) {
        let mut args = vec!["-Baum", "main", "-Commits", "main", "-AufMain"];
        if let Some(a) = adresse {
            args.extend(["-RemoteUrl", a]);
        }
        self.lauf(&args, None)
    }

    /// Wie der Hook beim Push von main an das Remote mit dieser Adresse.
    fn hook_mit(&self, adresse: &str) -> (i32, String) {
        let kopf = self.git(&["rev-parse", "HEAD"]);
        self.lauf(&["-Hook", "-RemoteName", "origin", "-RemoteUrl", adresse], Some(&format!("refs/heads/main {kopf} refs/heads/main {NULL}\n")))
    }

    /// Ersetzt `pruefer/repos.toml` (`None`: löscht sie).
    fn repos(&self, inhalt: Option<&str>) {
        let p = self.wurzel.join("inhalt").join("pruefer").join("repos.toml");
        match inhalt {
            Some(t) => std::fs::write(p, t).unwrap(),
            None => std::fs::remove_file(p).unwrap(),
        }
    }

    /// Ändert auf einem Zweig und führt ihn mit einem Merge-Commit (Ranke) in main
    /// zusammen, damit die Regel „nur Merge-Commits auf main“ nichts findet.
    fn ueber_merge(&self, aendern: impl FnOnce(&Ort)) {
        self.ueber_merge_mit("auf dem Zweig", aendern);
    }

    /// Wie `ueber_merge`, mit dieser Nachricht für den Commit auf dem Zweig.
    fn ueber_merge_mit(&self, nachricht: &str, aendern: impl FnOnce(&Ort)) {
        // -B: ein Test kann mehrmals über einen Merge ändern; zweig bleibt danach stehen.
        self.git(&["checkout", "-q", "-B", "zweig"]);
        aendern(self);
        self.commit(nachricht, &[]);
        self.git(&["checkout", "-q", "main"]);
        let mut c = git();
        c.arg("-C").arg(self.repo()).args(["merge", "-q", "--no-ff", "zweig", "-m", "chore(merge): zweig"]);
        for (k, v) in [("GIT_AUTHOR_NAME", "Ranke"), ("GIT_AUTHOR_EMAIL", MAIL), ("GIT_COMMITTER_NAME", "Ranke"), ("GIT_COMMITTER_EMAIL", MAIL)] {
            c.env(k, v);
        }
        let aus = c.output().unwrap();
        assert!(aus.status.success(), "{}", String::from_utf8_lossy(&aus.stderr));
    }

    /// Ersetzt die Namensliste (ein „§“ fällt weg wie in `datei`).
    fn liste(&self, inhalt: &str) {
        std::fs::write(self.wurzel.join("inhalt").join("pruefer").join("namen.toml"), inhalt.replace('§', "")).unwrap();
    }

    fn lauf(&self, args: &[&str], eingabe: Option<&str>) -> (i32, String) {
        let mut c = pwsh().unwrap();
        c.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"])
            .arg(skript())
            .arg("-Repo")
            .arg(self.repo())
            .arg("-Inhalt")
            .arg(self.wurzel.join("inhalt"))
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut kind = c.spawn().unwrap();
        {
            use std::io::Write;
            let mut ein = kind.stdin.take().unwrap();
            if let Some(t) = eingabe {
                ein.write_all(t.as_bytes()).unwrap();
            }
        }
        let o: Output = kind.wait_with_output().unwrap();
        (o.status.code().unwrap_or(-1), format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr)))
    }
}

impl Drop for Ort {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.wurzel);
    }
}

fn png(mit_text: bool) -> Vec<u8> {
    let mut v = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    let mut chunk = |typ: &[u8; 4], daten: &[u8]| {
        v.extend_from_slice(&(daten.len() as u32).to_be_bytes());
        v.extend_from_slice(typ);
        v.extend_from_slice(daten);
        v.extend_from_slice(&[0, 0, 0, 0]);
    };
    chunk(b"IHDR", &[0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0, 0, 0]);
    if mit_text {
        chunk(b"tEXt", b"Author\0jemand");
    }
    chunk(b"IEND", &[]);
    v
}

fn bereit() -> bool {
    if pwsh().is_none() || programme::befehl(Werkzeug::Git).is_err() {
        return !dashboard::ueberspringen("PowerShell oder git fehlen");
    }
    true
}

/// Ein Fall: `vorbereiten` ändert das saubere Repo; dann muss die Prüfung mit
/// `code` enden und `muster` nennen. Der gefundene Wert steht nie in der Ausgabe.
fn abgewiesen(name: &str, muster: &str, vorbereiten: impl FnOnce(&Ort)) -> String {
    let o = Ort::neu();
    vorbereiten(&o);
    let (code, text) = o.pruefen();
    assert_eq!(code, 1, "{name}: muss abgewiesen werden:\n{text}");
    assert!(text.contains(muster), "{name}: erwartet „{muster}“:\n{text}");
    for wert in [
        "realname",
        "geheimprojekt",
        "gruenkohlfest",
        "grünkohlfest",
        "kaffee",
        "jemand@",
        "nie hier",
        "pc-erfunden",
        "wlan-erfunden",
        "dienst-erfunden",
        "1234567890",
        "kanal-erfunden",
        "kanal erfunden",
        "nebelfink",
        "tokenwort",
    ] {
        assert!(!text.to_lowercase().contains(wert), "{name}: der Wert „{wert}“ darf nicht in der Ausgabe stehen:\n{text}");
    }
    text
}

#[test]
fn sauberes_repo_geht_durch() {
    if !bereit() {
        return;
    }
    let o = Ort::neu();
    let (code, text) = o.pruefen();
    assert_eq!(code, 0, "{text}");
    assert!(text.contains("Nichts gefunden."), "{text}");
}

/// LICENSE (AGPL) und LICENSES/CC0-1.0.txt gehen mit ihrem
/// echten Text durch; jede andere Datei in LICENSES/ steht nicht in der Positivliste.
/// Ebenso die schon committeten Wurzeldateien rustfmt.toml und .git-blame-ignore-revs.
#[test]
fn lizenzdateien_gehen_durch() {
    if !bereit() {
        return;
    }
    let wurzel = Path::new(env!("CARGO_MANIFEST_DIR"));
    let o = Ort::neu();
    o.ueber_merge(|o| {
        for pfad in ["LICENSE", "LICENSES/CC0-1.0.txt", "rustfmt.toml", ".git-blame-ignore-revs"] {
            o.datei(pfad, &std::fs::read(wurzel.join(pfad)).unwrap());
        }
    });
    let (code, text) = o.pruefen();
    assert_eq!(code, 0, "{text}");
    for pfad in ["LICENSES/MIT.txt", "LICENSES/CC0-1.0.txt.bak", "docs/LICENSE", "rustfmt.toml.alt"] {
        abgewiesen(pfad, "Positivliste", |o| {
            o.datei(pfad, b"x\n");
            o.commit("x", &[]);
        });
    }
}

/// `x.y.z` aus Ziffern ohne führende Null (wie der Tag-Name im Prüfer).
fn fassung(t: &str) -> Option<[u64; 3]> {
    let z: Vec<&str> = t.split('.').collect();
    if z.len() != 3 || z.iter().any(|s| s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) || (s.len() > 1 && s.starts_with('0'))) {
        return None;
    }
    Some([z[0].parse().ok()?, z[1].parse().ok()?, z[2].parse().ok()?])
}

/// Teil 4 (Versionsregel 0.x, Anleitung „5. Committen“): eine Version in
/// Cargo.toml, Cargo.lock, web/package.json und web/package-lock.json, erste Zahl
/// 0; CHANGELOG.txt beginnt mit Unreleased, danach „x.y.z - JJJJ-MM-TT“
/// absteigend, die oberste genau die Cargo-Version (vor der ersten Fassung keine).
#[test]
fn versionen_gleich_und_changelog_geordnet() {
    let wurzel = Path::new(env!("CARGO_MANIFEST_DIR"));
    let lesen = |p: &str| std::fs::read_to_string(wurzel.join(p)).unwrap_or_else(|e| panic!("{p}: {e}"));
    let cargo = env!("CARGO_PKG_VERSION");
    assert_eq!(fassung("0.10.2"), Some([0, 10, 2]));
    for falsch in ["0.01.0", "0.1", "0.1.0.0", "0.1.x", "0..1"] {
        assert_eq!(fassung(falsch), None, "{falsch}");
    }
    let mut fehler = Vec::new();
    let soll = fassung(cargo);
    if soll.is_none_or(|v| v[0] != 0) {
        fehler.push(format!("Cargo.toml: version = \"{cargo}\"; vor 1.0 nur 0.MINOR.PATCH (Ziffern ohne führende Null)"));
    }
    let json = |p: &str| serde_json::from_str::<serde_json::Value>(&lesen(p)).unwrap_or_else(|e| panic!("{p}: {e}"));
    let paket = json("web/package.json");
    let mut weitere = vec![("web/package.json", paket["version"].clone())];
    if wurzel.join("web/package-lock.json").exists() {
        let lock = json("web/package-lock.json");
        weitere.push(("web/package-lock.json (version)", lock["version"].clone()));
        weitere.push(("web/package-lock.json (packages.\"\".version)", lock["packages"][""]["version"].clone()));
    }
    for (wo, v) in weitere {
        if v.as_str() != Some(cargo) {
            fehler.push(format!("{wo}: Version {v} statt \"{cargo}\" wie in Cargo.toml; alle gleich setzen"));
        }
    }
    let lock: toml::Table = lesen("Cargo.lock").parse().expect("Cargo.lock ist TOML");
    let eigen = lock["package"].as_array().into_iter().flatten().find(|p| p["name"].as_str() == Some(env!("CARGO_PKG_NAME")));
    if eigen.and_then(|p| p["version"].as_str()) != Some(cargo) {
        fehler.push(format!("Cargo.lock: Eintrag {} nicht mit Version {cargo}; cargo check schreibt ihn nach", env!("CARGO_PKG_NAME")));
    }

    // Überschrift ist jede Zeile, unter der nur Striche stehen (wie unter Unreleased).
    let text = lesen("CHANGELOG.txt");
    let zeilen: Vec<&str> = text.trim_start_matches('\u{feff}').lines().collect();
    let titel: Vec<&str> =
        zeilen.windows(2).filter(|w| !w[0].trim().is_empty() && w[1].len() >= 3 && w[1].bytes().all(|b| b == b'-')).map(|w| w[0].trim_end()).collect();
    if titel.first() != Some(&"Unreleased") {
        fehler.push("CHANGELOG.txt: der erste Abschnitt nach dem Kopf muss Unreleased sein (Zeile Unreleased, darunter ----------)".into());
    }
    let mut vorige: Option<[u64; 3]> = None;
    for t in titel.iter().skip(1) {
        let v = t.split_once(" - ").and_then(|(v, d)| {
            let datum = d.len() == 10 && chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").is_ok();
            fassung(v).filter(|_| datum)
        });
        let Some(v) = v else {
            fehler.push(format!("CHANGELOG.txt: Überschrift „{t}“ nicht in der Form x.y.z - JJJJ-MM-TT (etwa 0.2.0 - 2026-11-20)"));
            continue;
        };
        if vorige.is_none() && Some(v) != soll {
            fehler.push(format!("CHANGELOG.txt: oberste Fassung {t}, Cargo.toml hat {cargo}; Fassung und Version im selben Commit setzen"));
        }
        if vorige.is_some_and(|w| v >= w) {
            fehler.push(format!("CHANGELOG.txt: „{t}“ steht nicht absteigend; neueste Fassung oben"));
        }
        vorige = Some(v);
    }
    assert!(fehler.is_empty(), "Versionsregel 0.x:\n{}", fehler.join("\n"));
}

#[test]
fn benutzerpfade_in_allen_schreibformen() {
    if !bereit() {
        return;
    }
    for (i, form) in [
        r"C:\U§sers\realname\dev",
        r"C:\\U§sers\\realname\\dev",
        "C:/U§sers/realname/dev",
        "/c/U§sers/realname/dev",
        "file:///C:/U§sers/realname/dev",
        "C:%5CU§sers%5Crealname",
        "C:%2FU§sers%2Frealname",
        r"c:\u§sers\REALNAME",
        r"C:\U§sers\REALNA§~1\dev",
    ]
    .iter()
    .enumerate()
    {
        abgewiesen(&format!("Benutzerpfad {i}"), "Benutzerpfad", |o| {
            o.datei("src/pfad.rs", format!("// {form}\n").as_bytes());
            o.commit("Pfad", &[]);
        });
    }
    // UTF-16 (wie PowerShell es leicht schreibt) ist kein UTF-8, keine zweite Lesart.
    abgewiesen("UTF-16", "scripts/notiz.txt, Zeile 1: kein UTF-8", |o| {
        let mut b = vec![0xFF, 0xFE];
        for u in "C:\\U§sers\\realname\\x\r\n".replace('§', "").encode_utf16() {
            b.extend_from_slice(&u.to_le_bytes());
        }
        o.datei("scripts/notiz.txt", &b);
        o.commit("UTF-16", &[]);
    });
}

#[test]
fn adressen_namen_und_kennungen() {
    if !bereit() {
        return;
    }
    abgewiesen("sensibler Name im Text", "sensibler Name", |o| {
        o.datei("src/a.rs", b"// GeheimProjekt Notiz\n");
        o.commit("x", &[]);
    });
    // tests/beispiel* ist nur für allgemeine Projektnamen ausgenommen, nicht
    // für sensible Namen und nicht für Benutzerpfade.
    abgewiesen("sensibler Name in tests/beispiel", "sensibler Name", |o| {
        o.datei("tests/beispiel/projects/alpha/notiz.md", b"geheimprojekt\n");
        o.commit("x", &[]);
    });
    abgewiesen("Benutzerpfad in tests/beispiel", "Benutzerpfad", |o| {
        o.datei("tests/beispiel/dienst.toml", "repo = \"C:/U§sers/realname/dev/dashboard\"\n".as_bytes());
        o.commit("x", &[]);
    });
    abgewiesen("sensibler Name im Pfad", "sensibler Name", |o| {
        o.datei("src/geheimprojekt/a.rs", b"// x\n");
        o.commit("x", &[]);
    });
    abgewiesen("sensibler Name in der Commit-Nachricht", "Nachricht", |o| {
        o.commit("Geheimprojekt: Bericht", &[]);
    });
    abgewiesen("SID", "SID eines Kontos", |o| {
        o.datei("src/a.rs", "// S-1-5-21-§1234567890-2345678901-3456789012-1001\n".as_bytes());
        o.commit("x", &[]);
    });
    abgewiesen("MAC", "MAC-Adresse", |o| {
        o.datei("src/a.rs", "// 3c:52:82:1a:2b:§3c\n".as_bytes());
        o.commit("x", &[]);
    });
    abgewiesen("WLAN", "WLAN-Name", |o| {
        o.datei("src/a.rs", b"// wlan-erfunden\n");
        o.commit("x", &[]);
    });
    abgewiesen("Rechner", "Rechnername", |o| {
        o.datei("src/a.rs", b"// PC-ERFUNDEN\n");
        o.commit("x", &[]);
    });
    abgewiesen("Schlüssel", "Schlüssel-Name", |o| {
        o.datei("src/a.rs", b"// dienst-erfunden\n");
        o.commit("x", &[]);
    });
    abgewiesen("Tabu im Pfad", "Name eines Tabu-Ordners", |o| {
        o.datei("src/Nie Hier/a.rs", b"// x\n");
        o.commit("x", &[]);
    });
}

#[test]
fn commits_und_refs() {
    if !bereit() {
        return;
    }
    abgewiesen("fremder Autor", "Autor ist nicht Ranke", |o| {
        o.datei("src/b.rs", b"// b\n");
        o.commit("b", &[("GIT_AUTHOR_NAME", "Jemand"), ("GIT_AUTHOR_EMAIL", "jemand@example.org")]);
    });
    abgewiesen("fremder Committer", "Committer ist nicht Ranke", |o| {
        o.datei("src/b.rs", b"// b\n");
        o.commit("b", &[("GIT_COMMITTER_EMAIL", "ranke2@proton.me.example")]);
    });
    abgewiesen("Fast-Forward auf main", "nur Merge-Commits", |o| {
        o.datei("src/b.rs", b"// b\n");
        o.commit("ohne Merge", &[]);
    });
    // Ein Merge-Commit auf main geht durch.
    let o = Ort::neu();
    o.git(&["checkout", "-q", "-b", "zweig"]);
    o.datei("src/b.rs", b"// b\n");
    o.commit("auf dem Zweig", &[]);
    o.git(&["checkout", "-q", "main"]);
    let mut c = git();
    c.arg("-C").arg(o.repo()).args(["merge", "-q", "--no-ff", "zweig", "-m", "chore(merge): zweig"]);
    for (k, v) in [("GIT_AUTHOR_NAME", "Ranke"), ("GIT_AUTHOR_EMAIL", MAIL), ("GIT_COMMITTER_NAME", "Ranke"), ("GIT_COMMITTER_EMAIL", MAIL)] {
        c.env(k, v);
    }
    assert!(c.output().unwrap().status.success());
    let (code, text) = o.pruefen();
    assert_eq!(code, 0, "Merge-Commit auf main geht durch: {text}");

    // Hook: nur refs/heads/main hinaus; keine Tags, keine Zweige, kein Löschen.
    let o = Ort::neu();
    let kopf = o.git(&["rev-parse", "HEAD"]);
    let (code, text) = o.lauf(&["-Hook"], Some(&format!("refs/heads/main {kopf} refs/heads/main {NULL}\n")));
    assert_eq!(code, 0, "Wurzel-Commit auf main: {text}");
    for (zeile, satz) in [
        (format!("refs/heads/projekt/x {kopf} refs/heads/projekt/x {NULL}\n"), "nur refs/heads/main"),
        (format!("refs/tags/v1 {kopf} refs/tags/v1 {NULL}\n"), "nur refs/heads/main"),
        (format!("refs/heads/anders {kopf} refs/heads/main {NULL}\n"), "nur der lokale Zweig main"),
        (format!("(delete) {NULL} refs/heads/main {kopf}\n"), "Löschen"),
    ] {
        let (code, text) = o.lauf(&["-Hook"], Some(&zeile));
        assert_eq!(code, 1, "{zeile}: {text}");
        assert!(text.contains(satz), "{zeile}: {text}");
    }
    let (code, text) = o.lauf(&["-Hook"], Some(""));
    assert_eq!(code, 2, "ohne Ref-Zeilen scheitert der Hook geschlossen: {text}");
}

#[test]
fn orte_und_dateien() {
    if !bereit() {
        return;
    }
    for (pfad, satz) in [
        ("projects/alpha/page.md", "gehört in den Inhalt"),
        ("projects/neu/server/src/lib.rs", "gehört in den Inhalt"),
        ("data/pc.toml", "gehört in den Inhalt"),
        ("docs/entscheidungen/2026-11-01-x.toml", "gehört in den Inhalt"),
        ("docs/architektur-stand.md", "gehört in den Inhalt"),
        ("web/src/lib/projekte.generated.ts", "projekte.generated.ts"),
        ("web/src/lib/vertrag/projekte/alpha/X.ts", "vertrag/projekte"),
        (".assistent/settings.json", "Positivliste"),
        ("notizen.txt", "Positivliste"),
        ("web/build/index.html", "Positivliste"),
        ("src/.env", "Geheimnis-Datei"),
        ("scripts/zugang.pfx", "Geheimnis-Datei"),
        ("scripts/conversations.json", "Chat-Export"),
        ("scripts/chat-export-2026.txt", "Chat-Export"),
        ("scripts/notizen-chat-25.md", "Chat-Export"),
        // Etappe 6d: jede *.md im Code-Repo ist ein Fund, auch in erlaubten Ordnern.
        ("README.md", "Markdown-Datei"),
        ("web/README.md", "Markdown-Datei"),
        ("docs/anleitung/notiz.MD", "Markdown-Datei"),
    ] {
        abgewiesen(pfad, satz, |o| {
            o.datei(pfad, b"x\n");
            o.commit("x", &[]);
        });
    }
    abgewiesen("Projektname als Pfad", "Projektname als Pfad", |o| {
        o.datei("src/a.rs", b"// siehe projects\\beta-zwei\\Seite.svelte\n");
        o.commit("x", &[]);
    });
    abgewiesen("Projektmodul in Cargo.toml", "Projektmodul", |o| {
        o.datei("Cargo.toml", b"[package]\nname = \"x\"\n[dependencies]\nm = { path = \"projects/neu/server\" }\n");
        o.commit("x", &[]);
    });
    abgewiesen("Projektkiste in Rust", "Projektmodul", |o| {
        o.datei("src/a.rs", b"fn m() { let _ = dashboard::eintrag(projekt_beta_zwei::Modul); }\n");
        o.commit("x", &[]);
    });
    abgewiesen("Chat-Export JSON", "Chat-Export (JSON", |o| {
        o.datei("tests/daten.json", br#"{"mapping": {"a": {"message": {"author": {"role": "user"}}}}}"#);
        o.commit("x", &[]);
    });
    abgewiesen("Chat-Export Markdown", "Chat-Export (mehrere", |o| {
        o.datei("scripts/verlauf.md", "## Du\nFrage\n\n## Assistant\nAntwort\n\n## Du\nNoch eine\n".as_bytes());
        o.commit("x", &[]);
    });
    abgewiesen("Chat-Export Text", "Chat-Export (mehrere", |o| {
        o.datei("scripts/verlauf.txt", b"User: Hallo\nAssistant: Hallo\nUser: Danke\n");
        o.commit("x", &[]);
    });
}

#[test]
fn binaerdateien() {
    if !bereit() {
        return;
    }
    abgewiesen("PNG mit tEXt", "PNG mit Metadaten", |o| {
        o.datei("web/static/icons/b.png", &png(true));
        o.commit("x", &[]);
    });
    abgewiesen("Binär außerhalb der Liste", "Binärdatei außerhalb", |o| {
        o.datei("src/daten.png", &[0, 1, 2, 3]);
        o.commit("x", &[]);
    });
    abgewiesen("PDF mit Author", "PDF mit Author", |o| {
        o.datei("web/static/hilfe.pdf", b"%PDF-1.4\n1 0 obj << /Author (jemand) >> endobj\n");
        o.commit("x", &[]);
    });
    abgewiesen("SVG mit Metadaten", "SVG mit Metadaten", |o| {
        o.datei("web/static/logo2.svg", b"<svg xmlns=\"http://www.w3.org/2000/svg\"><metadata>x</metadata></svg>\n");
        o.commit("x", &[]);
    });
}

/// Ein WebP (RIFF) mit den Stücken `VP8 ` und, wenn gegeben, einem weiteren.
fn webp(extra: Option<(&[u8; 4], &[u8])>) -> Vec<u8> {
    let mut stuecke = Vec::new();
    let mut stueck = |kennung: &[u8; 4], daten: &[u8]| {
        stuecke.extend_from_slice(kennung);
        stuecke.extend_from_slice(&(daten.len() as u32).to_le_bytes());
        stuecke.extend_from_slice(daten);
        if daten.len() % 2 == 1 {
            stuecke.push(0);
        }
    };
    stueck(b"VP8 ", &[1, 2, 3]);
    if let Some((k, d)) = extra {
        stueck(k, d);
    }
    let mut v = b"RIFF".to_vec();
    v.extend_from_slice(&((stuecke.len() + 4) as u32).to_le_bytes());
    v.extend_from_slice(b"WEBP");
    v.extend_from_slice(&stuecke);
    v
}

/// Ein ICO mit einem eingebetteten PNG (Kopf, ein Eintrag, dann das PNG).
fn ico(mit_text: bool) -> Vec<u8> {
    let bild = png(mit_text);
    let mut v = vec![0, 0, 1, 0, 1, 0];
    v.extend_from_slice(&[1, 1, 0, 0, 1, 0, 32, 0]);
    v.extend_from_slice(&(bild.len() as u32).to_le_bytes());
    v.extend_from_slice(&22u32.to_le_bytes());
    v.extend_from_slice(&bild);
    v
}

/// Metadaten auch in WebP, ICO und als XMP; lokale
/// Dateien (`*.local.*`) gehen nie hinaus; saubere WebP und ICO gehen durch.
#[test]
fn weitere_metadaten_und_lokale_dateien() {
    if !bereit() {
        return;
    }
    abgewiesen("WebP mit EXIF", "WebP mit Metadaten (EXIF)", |o| {
        o.datei("web/static/icons/c.webp", &webp(Some((b"EXIF", b"Exif\0\0jemand"))));
        o.commit("x", &[]);
    });
    abgewiesen("WebP mit XMP", "WebP mit Metadaten (XMP)", |o| {
        o.datei("web/static/icons/d.webp", &webp(Some((b"XMP ", b"<x:xmpmeta>jemand</x:xmpmeta>"))));
        o.commit("x", &[]);
    });
    abgewiesen("ICO mit PNG-Text", "ICO mit Metadaten (tEXt)", |o| {
        o.datei("web/static/favicon2.ico", &ico(true));
        o.commit("x", &[]);
    });
    abgewiesen("JPEG mit XMP", "XMP-Metadaten", |o| {
        let mut j = vec![0xFF, 0xD8, 0xFF, 0xE1, 0, 40];
        j.extend_from_slice(b"http://ns.adobe.com/xap/1.0/\0<x:xmpmeta/>");
        j.extend_from_slice(&[0xFF, 0xD9]);
        o.datei("web/static/icons/e.jpg", &j);
        o.commit("x", &[]);
    });
    for pfad in ["web/src/einstellungen.local.json", "scripts/umgebung.local", ".assistent/settings.local.json"] {
        abgewiesen(pfad, "Lokale Datei (*.local.*)", |o| {
            o.datei(pfad, b"{}\n");
            o.commit("x", &[]);
        });
    }
    // Sauber: WebP und ICO ohne Metadaten, und ein Name mit „locale“ ist keine lokale Datei.
    let o = Ort::neu();
    o.ueber_merge(|o| {
        o.datei("web/static/icons/f.webp", &webp(None));
        o.datei("web/static/favicon3.ico", &ico(false));
        o.datei("web/src/locale.ts", b"export const x = 1;\n");
    });
    let (code, text) = o.pruefen();
    assert_eq!(code, 0, "{text}");
}

/// Die FEHLER-Zeile (etwa „Die Namensliste … fehlt“) und
/// der HINWEIS zu repos.toml nennen das Benutzerprofil nie, nur %USERPROFILE%.
#[test]
fn fehlerzeile_ohne_benutzerprofil() {
    if !bereit() {
        return;
    }
    let profil = std::env::var("USERPROFILE").unwrap_or_default();
    let o = Ort::neu();
    o.repos(None);
    std::fs::remove_file(o.wurzel.join("inhalt").join("pruefer").join("namen.toml")).unwrap();
    let (code, text) = o.pruefen();
    assert_eq!(code, 2, "{text}");
    assert!(text.contains("FEHLER") && text.contains("namen.toml"), "{text}");
    if !profil.is_empty() && o.wurzel.starts_with(&profil) {
        assert!(!text.to_lowercase().contains(&profil.to_lowercase()), "das Profil steht in der Ausgabe:\n{text}");
        assert!(text.contains("%USERPROFILE%"), "{text}");
    }
    // Der HINWEIS zu einer fehlenden repos.toml ebenso.
    let o = Ort::neu();
    o.repos(None);
    let (_, text) = o.pruefen();
    assert!(text.contains("HINWEIS"), "{text}");
    if !profil.is_empty() && o.wurzel.starts_with(&profil) {
        assert!(!text.to_lowercase().contains(&profil.to_lowercase()), "{text}");
    }
}

/// Sensible Namen in Freitext-Schreibweisen (Umlaut
/// gegen ASCII in der Liste, auch zerlegt als NFD; Leerzeichen oder
/// Unterstrich statt Bindestrich).
#[test]
fn freitext_schreibweisen_sensibler_namen() {
    if !bereit() {
        return;
    }
    for (name, text) in [
        ("Umlaut", "// Das Grünkohlfest ist heute\n".to_string()),
        ("Umlaut zerlegt (NFD)", "// Gru\u{0308}nkohlfest\n".to_string()),
        ("Leerzeichen statt Bindestrich", "// Kaffee Runde am Montag\n".to_string()),
        ("Unterstrich", "let kaffee_runde = 1;\n".to_string()),
        ("ohne Trenner", "// KaffeeRunde\n".to_string()),
        ("Bindestrich statt Leerzeichen", "// Vorname-Nachname\n".to_string()),
    ] {
        abgewiesen(name, "sensibler Name", |o| {
            o.datei("src/a.rs", text.as_bytes());
            o.commit("x", &[]);
        });
    }
}

/// Weitere Pfadformen (VS-Code-URI mit c%3A, doppelt
/// kodiert, WSL, Cygwin, Admin-Freigabe c$).
#[test]
fn weitere_pfadformen() {
    if !bereit() {
        return;
    }
    for (i, form) in [
        "file:///c%3A/U§sers/realname/dev",
        "C:%5C%5CU§sers%5C%5Crealname",
        "C:%255CU§sers%255Crealname",
        "/mnt/c/U§sers/realname/dev",
        "/cygdrive/c/U§sers/realname/dev",
        r"\\localhost\c$\U§sers\realname\dev",
        r"\\pc\d$\U§sers\realname",
    ]
    .iter()
    .enumerate()
    {
        abgewiesen(&format!("Pfadform {i}"), "Benutzerpfad", |o| {
            o.datei("src/pfad.rs", format!("// {form}\n").as_bytes());
            o.commit("Pfad", &[]);
        });
    }
}

/// Jede Zeile je Muster, nicht nur die erste.
#[test]
fn alle_zeilen_je_muster() {
    if !bereit() {
        return;
    }
    let o = Ort::neu();
    o.datei("src/a.rs", "// projects/beta-zwei/a\nfn x() {}\n// projects/beta-zwei/b\n\n// projects\\beta-zwei\\c\n".as_bytes());
    o.commit("x", &[]);
    let (code, text) = o.pruefen();
    assert_eq!(code, 1, "{text}");
    for z in ["Zeile 1:", "Zeile 3:", "Zeile 5:"] {
        assert!(text.contains(&format!("src/a.rs, {z} Projektname als Pfad")), "{z}: {text}");
    }
}

/// Der Windows-Benutzername auch ohne Pfad.
#[test]
fn benutzername_ohne_pfad() {
    if !bereit() {
        return;
    }
    abgewiesen("Benutzername im Text", "Benutzername", |o| {
        o.datei("scripts/notiz.md", b"Gebaut von RealName am Abend.\n");
        o.commit("x", &[]);
    });
    abgewiesen("Benutzername in der Nachricht", "Nachricht", |o| {
        o.commit("Stand von realname", &[]);
    });
}

/// Geschlossen scheitern: fehlende, leere oder kaputte Namensliste, Exit 2.
#[test]
fn namensliste_fehlt_leer_kaputt() {
    if !bereit() {
        return;
    }
    let liste = |o: &Ort| o.wurzel.join("inhalt").join("pruefer").join("namen.toml");
    for (name, inhalt) in [
        ("fehlt", None),
        ("leer", Some("")),
        ("benutzer leer", Some(&*LISTE.replace("benutzer = [\"realname\"]", "benutzer = []"))),
        ("benutzer fehlt", Some(&*LISTE.replace("benutzer = [\"realname\"]\n", ""))),
        ("tabu leer", Some(&*LISTE.replace("tabu = [\"nie hier\"]", "tabu = []"))),
        ("nur_basis leer", Some(&*LISTE.replace("[\"nebelfink\", \"kanal-erfunden\"]", "[]"))),
        ("nur_basis fehlt", Some(&*LISTE.replace("nur_basis = [\"nebelfink\", \"kanal-erfunden\"]\n", ""))),
        ("kaputt", Some("sensibel = [\"a\"\nallgemein = \n")),
        ("unbekannte Liste", Some(&*format!("{LISTE}anders = [\"x\"]\n"))),
        ("kein Text", Some(&*LISTE.replace("[\"pc-erfunden\"]", "[pc-erfunden]"))),
    ] {
        let o = Ort::neu();
        match inhalt {
            None => std::fs::remove_file(liste(&o)).unwrap(),
            Some(t) => std::fs::write(liste(&o), t).unwrap(),
        }
        let (code, text) = o.pruefen();
        assert_eq!(code, 2, "{name}: {text}");
        assert!(text.contains("Es geht nichts hinaus"), "{name}: {text}");
    }
    // Ein eigener Fehler (Stand gibt es nicht) scheitert ebenso geschlossen.
    let o = Ort::neu();
    let (code, _) = o.lauf(&["-Baum", "gibt-es-nicht"], None);
    assert_eq!(code, 2);
    let _ = Path::new(".");
}

/// Die Liste `nur_basis` zählt nur im Basis-Repo: Teil eines Wortes, Groß und
/// Klein egal, Trenner gefaltet; ohne Adresse des Remotes gilt das Repo als
/// Basis-Repo.
#[test]
fn nur_basis_im_basis_repo_abgewiesen() {
    if !bereit() {
        return;
    }
    for (name, text) in [
        ("klein", "// nebelfink\n"),
        ("groß im Bezeichner", "let ist_NEBELFINK = 1;\n"),
        ("gemischt mitten im Wort", "// xNeBelFinKy\n"),
        ("Trenner gefaltet", "// Kanal Erfunden\n"),
    ] {
        abgewiesen(name, "nur im Basis-Repo", |o| {
            o.datei("src/a.rs", text.as_bytes());
            o.commit("x", &[]);
        });
    }
    abgewiesen("im Pfad", "nur im Basis-Repo", |o| {
        o.datei("src/NEBELFINK/a.rs", b"// x\n");
        o.commit("x", &[]);
    });
    abgewiesen("in der Nachricht", "Nachricht", |o| {
        o.commit("Stand für Nebelfink", &[]);
    });
    // Mit der Zusicherung -Art Basis: ebenso.
    let o = Ort::neu();
    o.ueber_merge(|o| o.datei("src/a.rs", b"// nebelfink\n"));
    let (code, text) = o.pruefen_als("Basis");
    assert_eq!(code, 1, "{text}");
    assert!(text.contains("nur im Basis-Repo") && text.contains("Geprüft als Basis-Repo"), "{text}");
}

/// Nur die Zeilen der Funde („ABGEWIESEN …“), zum Vergleich zweier Läufe.
fn fundzeilen(text: &str) -> Vec<String> {
    text.lines().filter(|z| z.starts_with("ABGEWIESEN ")).map(str::to_string).collect()
}

/// Ein Basis-Repo mit Verstößen, die wie Belege einer anderen Art aussehen
/// (Ordner projects/x, Datei pruefer/namen.toml), dazu ein Name aus
/// `nur_basis`, ein Projektname als Pfad, ein Projektmodul und eine
/// Binärdatei außerhalb der Positivliste.
fn basis_mit_verstoessen() -> Ort {
    let o = Ort::neu();
    o.ueber_merge(|o| {
        o.datei("projects/x/page.md", b"# Seite\n");
        o.datei("pruefer/namen.toml", b"# Marke\n");
        o.datei("src/a.rs", b"// nebelfink\n// projects/beta-zwei/a\n");
        o.datei("src/daten.png", &[0, 1, 2, 3]);
        o.datei("Cargo.toml", b"[package]\nname = \"x\"\n[dependencies]\nm = { path = \"projects/x/server\" }\n");
    });
    o
}

/// Was nur im Basis-Repo gilt, muss in `basis_mit_verstoessen` gefunden werden.
const NUR_BASIS_FUNDE: [&str; 6] = [
    "projects/x/page.md: projects/<slug> gehört in den Inhalt",
    "pruefer/namen.toml: Ordner oder Datei nicht in der Positivliste",
    "src/a.rs, Zeile 1: Name, der nur im Basis-Repo verboten ist",
    "src/a.rs, Zeile 2: Projektname als Pfad",
    "src/daten.png: Binärdatei außerhalb der Positivliste",
    "Cargo.toml, Zeile 4: Projektmodul",
];

/// Der Baum wählt die Art nie: ohne Adresse, mit unbekannter Adresse (auch
/// nur im Pfad anders geschrieben) und mit einer als Basis eingetragenen
/// Adresse bleiben alle Funde des Basis-Repos, Zeile für Zeile gleich.
#[test]
fn art_nie_aus_dem_baum() {
    if !bereit() {
        return;
    }
    let o = basis_mit_verstoessen();
    let (code, ohne) = o.pruefen_mit(None);
    assert_eq!(code, 1, "{ohne}");
    assert!(ohne.contains("Geprüft als Basis-Repo"), "{ohne}");
    for f in NUR_BASIS_FUNDE {
        assert!(ohne.contains(&format!("ABGEWIESEN {f}")), "ohne Adresse: erwartet „{f}“:\n{ohne}");
    }
    for (name, adresse, hinweis) in [
        ("unbekannt", "https://git.example.invalid/fremd/anders.git", true),
        ("Pfad mit anderem Groß/Klein", "https://git.example.invalid/team/projekt", true),
        ("als Basis eingetragen", "https://GIT.example.invalid/team/basis/", false),
    ] {
        let (code, text) = o.pruefen_mit(Some(adresse));
        assert_eq!(code, 1, "{name}: {text}");
        assert!(text.contains("Geprüft als Basis-Repo"), "{name}: {text}");
        assert_eq!(text.contains("HINWEIS Die Adresse des Remotes steht nicht in repos.toml"), hinweis, "{name}: {text}");
        assert_eq!(fundzeilen(&text), fundzeilen(&ohne), "{name}: dieselben Funde wie ohne Adresse");
        assert!(!text.contains("example.invalid"), "{name}: die Adresse steht nie in der Ausgabe:\n{text}");
    }
    // Auch als Hook mit der Basis-Adresse.
    let (code, text) = o.hook_mit("https://git.example.invalid/team/basis.git");
    assert_eq!(code, 1, "{text}");
    for f in NUR_BASIS_FUNDE {
        assert!(text.contains(&format!("ABGEWIESEN {f}")), "Hook: erwartet „{f}“:\n{text}");
    }
}

/// Ein frisches Repo der Art: Projekt (Overlay) oder sonst der Grundstock.
fn ort_fuer(art: &str) -> Ort {
    if art == "Projekt" { Ort::projekt() } else { Ort::neu() }
}

/// Steht die Adresse des Remotes als Projekt oder Inhalt in repos.toml (der
/// Rechnername in anderem Groß/Klein, / oder .git am Ende), sind die Namen aus
/// `nur_basis` und die Orte des Projekts erlaubt; alles andere gilt dort weiter.
#[test]
fn art_aus_der_adresse_projekt_und_inhalt() {
    if !bereit() {
        return;
    }
    for (art, adresse) in [("Projekt", PROJEKT_ADRESSE), ("Inhalt", INHALT_ADRESSE)] {
        let o = ort_fuer(art);
        o.ueber_merge(|o| {
            o.datei("projects/alpha/notiz.txt", b"nebelfink, ist_NEBELFINK, xNeBelFinKy, Kanal Erfunden\n");
            // Im Inhalt sind auch Markdown und Orte außerhalb von projects/ erlaubt, im Projekt nicht.
            if art == "Inhalt" {
                o.datei("projects/alpha/page.md", b"# Seite\n");
                o.datei("pruefer/namen.toml", b"# Marke\n");
                o.datei("src/a.rs", b"// nebelfink\n");
            }
        });
        let (code, text) = o.hook_mit(adresse);
        assert_eq!(code, 0, "{art}: {text}");
        assert!(text.contains(&format!("Geprüft als {art}")), "{art}: {text}");
        let (code, text) = o.pruefen_mit(Some(adresse));
        assert_eq!(code, 0, "{art}, wie Hochladen mit Adresse: {text}");

        // Abgewiesen: Benutzerpfad, sensibler Name und Benutzername auch hier.
        for (name, inhalt, muster) in [
            ("Benutzerpfad", r"// C:\U§sers\realname\dev", "Benutzerpfad"),
            ("sensibel", "// Geheimprojekt", "sensibler Name"),
            ("Benutzername", "// RealName", "Benutzername"),
        ] {
            let o = ort_fuer(art);
            o.ueber_merge(|o| o.datei("projects/alpha/a.txt", format!("{inhalt}\n").as_bytes()));
            let (code, text) = o.hook_mit(adresse);
            assert_eq!(code, 1, "{art}, {name}: {text}");
            assert!(text.contains(muster) && text.contains(&format!("Geprüft als {art}")), "{art}, {name}: {text}");
            assert!(!text.to_lowercase().contains("realname") && !text.to_lowercase().contains("geheimprojekt"), "{art}, {name}: {text}");
        }
    }
}

/// `-Art` ist nur eine Zusicherung: weicht sie von der Art aus der Adresse ab,
/// Exit 2; passt sie, prüft er wie ohne.
#[test]
fn art_als_zusicherung() {
    if !bereit() {
        return;
    }
    let o = Ort::neu();
    for (art, adresse) in
        [("Projekt", None), ("Inhalt", Some("https://git.example.invalid/fremd/anders")), ("Basis", Some(PROJEKT_ADRESSE)), ("Projekt", Some(INHALT_ADRESSE))]
    {
        let mut args = vec!["-Art", art, "-Baum", "main"];
        if let Some(a) = adresse {
            args.extend(["-RemoteUrl", a]);
        }
        let (code, text) = o.lauf(&args, None);
        assert_eq!(code, 2, "-Art {art}, {adresse:?}: {text}");
        assert!(text.contains(&format!("-Art {art} passt nicht")) && text.contains("Es geht nichts hinaus"), "{text}");
    }
    for (art, adresse) in [("Basis", None), ("Projekt", Some(PROJEKT_ADRESSE)), ("Inhalt", Some(INHALT_ADRESSE))] {
        let mut args = vec!["-Art", art, "-Baum", "main"];
        if let Some(a) = adresse {
            args.extend(["-RemoteUrl", a]);
        }
        let (code, text) = ort_fuer(art).lauf(&args, None);
        assert_eq!(code, 0, "-Art {art}, {adresse:?}: {text}");
    }
    // Eine unbekannte Art lässt PowerShell gar nicht erst zu.
    let (code, _) = o.lauf(&["-Art", "Anders", "-Baum", "main"], None);
    assert_ne!(code, 0);
}

/// Positivliste der Art Projekt (Overlay): nur projects/<slug>/** für genau
/// einen Slug und dashboard-version.toml, .gitignore, .gitattributes; alles
/// andere ist ein Fund, auch nur in der Geschichte. *.md auch hier nicht.
#[test]
fn projekt_positivliste() {
    if !bereit() {
        return;
    }
    let o = Ort::projekt();
    let (code, text) = o.pruefen_mit(Some(PROJEKT_ADRESSE));
    assert_eq!(code, 0, "sauberes Projekt-Repo: {text}");
    assert!(text.contains("Geprüft als Projekt-Repo"), "{text}");
    let fremd = "fremdes Projekt (ein zweiter Ordner unter projects/)";
    let ausserhalb = "Ordner oder Datei nicht in der Positivliste des Projekt-Repos";
    for (name, pfad, muster) in [
        ("Code wie im Basis-Repo", "src/a.rs", ausserhalb),
        ("Cargo.toml in der Wurzel", "Cargo.toml", ausserhalb),
        ("Datei direkt unter projects/", "projects/x.toml", ausserhalb),
        ("Wurzeldatei des Basis-Repos", "LIESMICH.txt", ausserhalb),
        ("Wurzeldatei in einem Ordner", "docs/.gitignore", ausserhalb),
        ("zweiter Slug", "projects/beta/projekt.toml", fremd),
        ("zweiter Slug hinter dem ersten", "projects/zeta/projekt.toml", fremd),
        ("Markdown im eigenen Ordner", "projects/alpha/notiz.md", "Markdown-Datei (*.md)"),
    ] {
        let o = Ort::projekt();
        o.ueber_merge(|o| o.datei(pfad, b"x = 1\n"));
        for (wie, (code, text)) in [("Hochladen", o.pruefen_mit(Some(PROJEKT_ADRESSE))), ("Hook", o.hook_mit(PROJEKT_ADRESSE))] {
            assert_eq!(code, 1, "{name} ({wie}): {text}");
            assert!(text.contains(&format!("ABGEWIESEN {pfad}: {muster}")), "{name} ({wie}): {text}");
            assert_eq!(fundzeilen(&text).len(), 1, "{name} ({wie}): genau ein Fund:\n{text}");
        }
    }
    // Sortiert der zweite Slug vor den ersten, gilt der eigene als fremd: abgewiesen wird so oder so.
    let o = Ort::projekt();
    o.ueber_merge(|o| o.datei("projects/_vorlage/projekt.toml", b"schema = 3\n"));
    let (code, text) = o.pruefen_mit(Some(PROJEKT_ADRESSE));
    assert_eq!(code, 1, "{text}");
    assert!(text.contains("ABGEWIESEN projects/alpha/projekt.toml: fremdes Projekt"), "{text}");
    // Ein zweiter Slug nur in einem Zwischenstand geht mit der Geschichte hinaus: auch ein Fund.
    let o = Ort::projekt();
    o.ueber_merge(|o| {
        o.datei("projects/beta/a.toml", b"x = 1\n");
        o.commit("zwischendurch", &[]);
        std::fs::remove_dir_all(o.repo().join("projects").join("beta")).unwrap();
    });
    let (code, text) = o.pruefen_mit(Some(PROJEKT_ADRESSE));
    assert_eq!(code, 1, "{text}");
    assert!(text.contains(&format!("ABGEWIESEN projects/beta/a.toml: {fremd}")), "{text}");
    // Hat das Remote schon main, gilt dessen Slug: Ein Merge, der alpha löscht und beta anlegt, ist ein Fund.
    for (name, tauschen) in [("Tausch", true), ("Folgepush", false)] {
        let o = Ort::projekt();
        let fern = o.git(&["rev-parse", "HEAD"]);
        o.ueber_merge(|o| {
            if tauschen {
                std::fs::remove_dir_all(o.repo().join("projects").join("alpha")).unwrap();
                o.datei("projects/beta/projekt.toml", b"schema = 3\n");
            } else {
                o.datei("projects/alpha/b.toml", b"x = 1\n");
            }
        });
        let kopf = o.git(&["rev-parse", "HEAD"]);
        let hook =
            o.lauf(&["-Hook", "-RemoteName", "origin", "-RemoteUrl", PROJEKT_ADRESSE], Some(&format!("refs/heads/main {kopf} refs/heads/main {fern}\n")));
        let bereich = format!("{fern}..main");
        let hochladen = o.lauf(&["-RemoteUrl", PROJEKT_ADRESSE, "-Baum", "main", "-Commits", &bereich, "-AufMain"], None);
        for (wie, (code, text)) in [("Hook", hook), ("Hochladen", hochladen)] {
            if tauschen {
                assert_eq!(code, 1, "{name} ({wie}): {text}");
                assert!(text.contains(&format!("ABGEWIESEN projects/beta/projekt.toml: {fremd}")), "{name} ({wie}): {text}");
            } else {
                assert_eq!(code, 0, "{name} ({wie}): {text}");
            }
        }
    }
}

/// Ausnahmedatei `pruefer/ausnahmen/<name>.toml`: nur in
/// der Art Projekt, nur für die Adresse aus repos.toml, nur genau gleich
/// gefaltete Werte aus sensibel (mit tokens) oder nur_basis; die Ausgabe nennt
/// nur die Zahl, nie einen Wert.
#[test]
fn projekt_ausnahmedatei() {
    if !bereit() {
        return;
    }
    let text_mit_werten = "Tokenwort, Geheimprojekt und Vorname Nachname\n";
    let werte = ["tokenwort", "geheimprojekt", "vorname", "gruenkohlfest", "grünkohlfest", "kaffee", "gibtsnicht", "nebelfink"];
    let ohne_werte = |name: &str, text: &str| {
        for w in werte {
            assert!(!text.to_lowercase().contains(w), "{name}: der Wert „{w}“ darf nicht in der Ausgabe stehen:\n{text}");
        }
    };
    // Groß/Klein und Trenner wie beim Prüfer; nebelfink steht in nur_basis.
    let erlaubt = "# erfunden\nerlaubt = [\n  \"TOKENWORT\",  # aus tokens\n  \"Geheimprojekt\",\n  \"vorname_nachname\",\n  \"nebelfink\",\n]\n";
    let projekt = |ausnahme: Option<&str>| {
        let o = Ort::projekt();
        o.ueber_merge(|o| o.datei("projects/alpha/notiz.txt", text_mit_werten.as_bytes()));
        if let Some(a) = ausnahme {
            o.ausnahmen("projekt", a);
        }
        o
    };

    // Ohne Datei sperren die Tokens; kein Scheitern, keine Zeile zu Ausnahmen.
    let o = projekt(None);
    let (code, text) = o.hook_mit(PROJEKT_ADRESSE);
    assert_eq!(code, 1, "{text}");
    assert!(text.contains("projects/alpha/notiz.txt, Zeile 1: sensibler Name") && !text.contains("Ausnahmen"), "{text}");
    ohne_werte("ohne Datei", &text);

    // Mit Datei geht es im Projekt-Repo durch, als Hook und wie Hochladen.
    let o = projekt(Some(erlaubt));
    for (wie, (code, text)) in [("Hook", o.hook_mit(PROJEKT_ADRESSE)), ("Hochladen", o.pruefen_mit(Some(PROJEKT_ADRESSE)))] {
        assert_eq!(code, 0, "{wie}: {text}");
        assert!(text.contains("Ausnahmen für dieses Repo: 4 angewandt."), "{wie}: {text}");
        ohne_werte(wie, &text);
    }
    // Ein ausgenommener Wert bleibt in Fundzeilen verborgen.
    let o = projekt(Some(erlaubt));
    o.ueber_merge(|o| o.datei("projects/alpha/Tokenwort-notiz.md", b"x\n"));
    let (code, text) = o.hook_mit(PROJEKT_ADRESSE);
    assert_eq!(code, 1, "{text}");
    assert!(text.contains("Markdown-Datei (*.md)"), "{text}");
    ohne_werte("Pfad mit ausgenommenem Wert", &text);
    // Was nicht in der Datei steht, sperrt weiter.
    for (name, inhalt) in [("anderes Token", "Grünkohlfest"), ("anderes Token mit Trenner", "Kaffee Runde")] {
        let o = projekt(Some(erlaubt));
        o.ueber_merge(|o| o.datei("projects/alpha/b.txt", format!("{inhalt}\n").as_bytes()));
        let (code, text) = o.hook_mit(PROJEKT_ADRESSE);
        assert_eq!(code, 1, "{name}: {text}");
        assert!(text.contains("projects/alpha/b.txt, Zeile 1: sensibler Name"), "{name}: {text}");
    }
    // Nur genau gleich gefaltet: eine andere Trennung oder ein Wortteil ist kein Listeneintrag.
    let o = projekt(Some("erlaubt = [\"geheim projekt\", \"tokenwort\", \"vorname-nachname\", \"kaffee\"]\n"));
    o.ueber_merge(|o| o.datei("projects/alpha/b.txt", b"Kaffee-Runde\n"));
    let (code, text) = o.hook_mit(PROJEKT_ADRESSE);
    assert_eq!(code, 1, "„geheim projekt“ ist nicht „geheimprojekt“: {text}");
    for f in [
        "Ausnahmedatei: Ausnahme ohne Listeneintrag, Nr. 1",
        "Ausnahmedatei: Ausnahme ohne Listeneintrag, Nr. 4",
        "projects/alpha/notiz.txt, Zeile 1: sensibler Name",
        "projects/alpha/b.txt, Zeile 1: sensibler Name",
    ] {
        assert!(text.contains(&format!("ABGEWIESEN {f}")), "erwartet „{f}“: {text}");
    }
    assert!(text.contains("Ausnahmen für dieses Repo: 2 angewandt."), "{text}");

    // Ein Eintrag ohne Listeneintrag ist ein Fund (ohne den Wert).
    let o = projekt(Some("erlaubt = [\"tokenwort\", \"geheimprojekt\", \"vorname nachname\", \"gibtsnicht\"]\n"));
    let (code, text) = o.hook_mit(PROJEKT_ADRESSE);
    assert_eq!(code, 1, "{text}");
    assert_eq!(fundzeilen(&text), vec!["ABGEWIESEN Ausnahmedatei: Ausnahme ohne Listeneintrag, Nr. 4".to_string()], "{text}");
    assert!(text.contains("Ausnahmen für dieses Repo: 3 angewandt."), "{text}");
    ohne_werte("ohne Listeneintrag", &text);

    // Nicht für eine andere Projekt-Adresse, auch wenn eine Datei unter ihrem Namen fehlt.
    let o = projekt(Some(erlaubt));
    o.repos(Some(&format!("{REPOS}\"https://git.example.invalid/team/anderes.git\" = \"Projekt\"\n")));
    let (code, text) = o.hook_mit("https://git.example.invalid/team/anderes");
    assert_eq!(code, 1, "andere Adresse: {text}");
    assert!(text.contains("Geprüft als Projekt-Repo") && text.contains("sensibler Name") && !text.contains("Ausnahmen"), "{text}");
    // Dieselbe letzte Pfadstelle unter einer Adresse, die nicht in repos.toml steht: Basis, nie gelesen.
    let (code, text) = o.hook_mit("https://git.example.invalid/fremd/projekt.git");
    assert_eq!(code, 1, "{text}");
    assert!(text.contains("Geprüft als Basis-Repo") && text.contains("sensibler Name") && !text.contains("Ausnahmen"), "{text}");

    // In Basis und Inhalt wird sie nie gelesen, auch kaputt nicht (Exit 1, nicht 2).
    for (art, adresse, name) in [("Basis", "https://git.example.invalid/team/basis.git", "basis"), ("Inhalt", INHALT_ADRESSE, "inhalt")] {
        let o = Ort::neu();
        o.ueber_merge(|o| o.datei("src/b.rs", text_mit_werten.replace("Tokenwort", "// Tokenwort").as_bytes()));
        for datei in [erlaubt, "kaputt"] {
            o.ausnahmen(name, datei);
            let (code, text) = o.hook_mit(adresse);
            assert_eq!(code, 1, "{art}, {datei}: {text}");
            assert!(text.contains(&format!("Geprüft als {art}")) && text.contains("sensibler Name") && !text.contains("Ausnahmen"), "{art}: {text}");
        }
    }

    // Kaputt: Abbruch (Exit 2), ohne Namen des Repos und ohne Werte.
    for (name, inhalt) in [
        ("kein TOML", "erlaubt = tokenwort\n"),
        ("unbekannte Liste", "sensibel = [\"tokenwort\"]\n"),
        ("zweimal", "erlaubt = [\"tokenwort\"]\nerlaubt = [\"geheimprojekt\"]\n"),
        ("Escape", "erlaubt = [\"token\\u0077ort\"]\n"),
    ] {
        let o = projekt(Some(inhalt));
        let (code, text) = o.hook_mit(PROJEKT_ADRESSE);
        assert_eq!(code, 2, "{name}: {text}");
        assert!(text.contains("<repo>.toml ist kaputt") && text.contains("Es geht nichts hinaus"), "{name}: {text}");
        assert!(!text.contains("ausnahmen\\projekt") && !text.contains("example"), "{name}: {text}");
        ohne_werte(name, &text);
    }
    // Leere Liste: gültig, keine Ausnahme.
    let o = projekt(Some("# leer\nerlaubt = []\n"));
    let (code, text) = o.hook_mit(PROJEKT_ADRESSE);
    assert_eq!(code, 1, "{text}");
    assert!(text.contains("Ausnahmen für dieses Repo: 0 angewandt.") && text.contains("sensibler Name"), "{text}");
}

/// Ein ausgenommener Wert ist nur alleinstehend frei.
/// Das Muster trifft Wortteile; mit Buchstabe oder Ziffer davor oder dahinter
/// (gefaltet) bleibt eine Zusammensetzung ein Fund, in Text, Pfad und Commit.
#[test]
fn ausnahme_nur_alleinstehend() {
    if !bereit() {
        return;
    }
    let erlaubt = "erlaubt = [\"geheimprojekt\", \"vorname nachname\"]\n";
    let ohne_werte = |name: &str, text: &str| {
        for w in ["geheimprojekt", "vorname", "nachname"] {
            assert!(!text.to_lowercase().contains(w), "{name}: der Wert „{w}“ darf nicht in der Ausgabe stehen:\n{text}");
        }
    };
    // Alleinstehend (Satzzeichen, Anführungszeichen oder Trenner daneben, Groß/Klein egal): frei.
    let o = Ort::projekt();
    o.ueber_merge_mit("test: Geheimprojekt bleibt frei", |o| {
        o.datei(
            "projects/alpha/frei.txt",
            "Geheimprojekt.\n(geheimprojekt)\nVorname_Nachname\ngeheim-projekt und VORNAME NACHNAME\n\"Geheimprojekt\"-Liste\n".as_bytes(),
        );
        o.datei("projects/alpha/geheimprojekt.txt", b"x\n");
    });
    o.ausnahmen("projekt", erlaubt);
    for (wie, (code, text)) in [("Hook", o.hook_mit(PROJEKT_ADRESSE)), ("Hochladen", o.pruefen_mit(Some(PROJEKT_ADRESSE)))] {
        assert_eq!(code, 0, "{wie}: {text}");
        assert!(text.contains("Ausnahmen für dieses Repo: 2 angewandt."), "{wie}: {text}");
        ohne_werte(wie, &text);
    }

    // Zusammensetzung davor und danach, auch mit Ziffer und über weggelassene Trenner: Fund.
    let o = Ort::projekt();
    o.ueber_merge_mit("test: Geheimprojekte im Zweig", |o| {
        o.datei(
            "projects/alpha/b.txt",
            // 8-11: Nachbar über einen Umbruch am Strich; 12-14: Marke (U+0332), Buchstabe und Ideogramm außerhalb der BMP.
            "Supergeheimprojekt\nGeheimprojekte\ngeheimprojekt2\n2geheimprojekt\nVornameNachnamen\nXvorname-nachname\nGEHEIMPROJEKTÄ\nSuper-\ngeheimprojekt\ngeheimprojekt-\nstudie\nx\u{332}geheimprojekt\ngeheimprojekt\u{1D41A}\n\u{20000}geheimprojekt\n".as_bytes(),
        );
        o.datei("projects/alpha/supergeheimprojekt.txt", b"x\n");
    });
    o.ausnahmen("projekt", erlaubt);
    for (wie, (code, text)) in [("Hook", o.hook_mit(PROJEKT_ADRESSE)), ("Hochladen", o.pruefen_mit(Some(PROJEKT_ADRESSE)))] {
        assert_eq!(code, 1, "{wie}: {text}");
        assert!(text.contains("Ausnahmen für dieses Repo: 2 angewandt."), "{wie}: {text}");
        for n in (1..=7).chain([9, 10, 12, 13, 14]) {
            assert!(text.contains(&format!("ABGEWIESEN projects/alpha/b.txt, Zeile {n}: sensibler Name")), "{wie}, Zeile {n}: {text}");
        }
        let zeilen = fundzeilen(&text);
        assert!(zeilen.iter().any(|z| z.starts_with("ABGEWIESEN projects/alpha/super") && z.ends_with(": sensibler Name")), "{wie}, Pfad: {text}");
        assert!(zeilen.iter().any(|z| z.starts_with("ABGEWIESEN Commit ") && z.ends_with("(Nachricht), Zeile 1: sensibler Name")), "{wie}, Commit: {text}");
        ohne_werte(wie, &text);
    }
}

/// Kopiert einen Ordner samt Unterordnern.
fn ordner_kopieren(von: &Path, nach: &Path) {
    std::fs::create_dir_all(nach).unwrap();
    for e in std::fs::read_dir(von).unwrap() {
        let e = e.unwrap();
        if e.file_type().unwrap().is_dir() {
            ordner_kopieren(&e.path(), &nach.join(e.file_name()));
        } else {
            std::fs::copy(e.path(), nach.join(e.file_name())).unwrap();
        }
    }
}

/// Ohne -Inhalt gilt dashboard-inhalt neben dem
/// Code-Repo des Prüfers, nie neben dem geprüften Repo (beim Overlay ein Klon,
/// daneben vielleicht eine alte Kopie); DASHBOARD_INHALT geht weiter vor.
#[test]
fn inhalt_neben_dem_code_repo() {
    if !bereit() {
        return;
    }
    let o = Ort::neu();
    o.ueber_merge(|o| o.datei("src/b.rs", b"// Geheimprojekt\n"));
    // Neben dem geprüften Repo ein Köder ohne den Wert, neben dem Code-Repo die echten Listen.
    let koeder = o.wurzel.join("dashboard-inhalt");
    ordner_kopieren(&o.wurzel.join("inhalt"), &koeder);
    std::fs::write(koeder.join("pruefer").join("namen.toml"), LISTE.replace(SENSIBEL, "sensibel = [\"nurimkoeder\"]")).unwrap();
    let code = o.wurzel.join("ablage").join("code");
    std::fs::create_dir_all(code.join("scripts")).unwrap();
    std::fs::copy(skript(), code.join("scripts").join("Vor-Push.ps1")).unwrap();
    ordner_kopieren(&o.wurzel.join("inhalt"), &o.wurzel.join("ablage").join("dashboard-inhalt"));
    let lauf = |umgebung: Option<&Path>| {
        let mut c = pwsh().unwrap();
        c.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"])
            .arg(code.join("scripts").join("Vor-Push.ps1"))
            .arg("-Repo")
            .arg(o.repo())
            .args(["-Baum", "main", "-Commits", "main", "-AufMain"])
            .env_remove("DASHBOARD_INHALT");
        if let Some(p) = umgebung {
            c.env("DASHBOARD_INHALT", p);
        }
        let a = c.output().unwrap();
        (a.status.code().unwrap_or(-1), format!("{}{}", String::from_utf8_lossy(&a.stdout), String::from_utf8_lossy(&a.stderr)))
    };
    let (c, text) = lauf(None);
    assert_eq!(c, 1, "die Listen neben dem Code-Repo gelten: {text}");
    assert!(text.contains("ABGEWIESEN src/b.rs, Zeile 1: sensibler Name"), "{text}");
    let (c, text) = lauf(Some(&koeder));
    assert_eq!(c, 0, "DASHBOARD_INHALT geht vor: {text}");
}

/// Fehlt repos.toml, gilt jede Adresse als Basis (mit HINWEIS); eine kaputte
/// repos.toml bricht ab (Exit 2), auch ohne Adresse.
#[test]
fn repos_liste_fehlt_oder_kaputt() {
    if !bereit() {
        return;
    }
    let o = basis_mit_verstoessen();
    o.repos(None);
    let (code, text) = o.hook_mit(PROJEKT_ADRESSE);
    assert_eq!(code, 1, "{text}");
    assert!(text.contains("HINWEIS Die Liste") && text.contains("fehlt") && text.contains("Geprüft als Basis-Repo"), "{text}");
    for f in NUR_BASIS_FUNDE {
        assert!(text.contains(&format!("ABGEWIESEN {f}")), "ohne repos.toml: erwartet „{f}“:\n{text}");
    }
    // Nur Kommentare und [repos], keine Zeile: gültig, jede Adresse ist unbekannt.
    let o = Ort::neu();
    o.repos(Some("# leer\n\n[repos]\n"));
    let (code, text) = o.pruefen_mit(Some(PROJEKT_ADRESSE));
    assert_eq!(code, 0, "{text}");
    assert!(text.contains("Geprüft als Basis-Repo") && text.contains("HINWEIS Die Adresse"), "{text}");
    for (name, inhalt) in [
        ("ohne [repos]", "\"https://a.example.invalid/x\" = \"Projekt\"\n"),
        ("Adresse ohne Anführungszeichen", "[repos]\nhttps://a.example.invalid/x = \"Projekt\"\n"),
        ("unbekannte Art", "[repos]\n\"https://a.example.invalid/x\" = \"Fork\"\n"),
        ("Art klein geschrieben", "[repos]\n\"https://a.example.invalid/x\" = \"projekt\"\n"),
        ("Art ohne Anführungszeichen", "[repos]\n\"https://a.example.invalid/x\" = Projekt\n"),
        ("Adresse leer", "[repos]\n\"\" = \"Projekt\"\n"),
        ("zweimal dieselbe Adresse", "[repos]\n\"https://a.example.invalid/x.git\" = \"Basis\"\n\"https://A.example.invalid/x/\" = \"Projekt\"\n"),
        ("[repos] zweimal", "[repos]\n[repos]\n"),
        ("anderer Abschnitt", "[repos]\n[anders]\n"),
    ] {
        let o = Ort::neu();
        o.repos(Some(inhalt));
        for adresse in [None, Some(PROJEKT_ADRESSE)] {
            let (code, text) = o.pruefen_mit(adresse);
            assert_eq!(code, 2, "{name}, {adresse:?}: {text}");
            assert!(text.contains("repos.toml ist kaputt") && text.contains("Es geht nichts hinaus"), "{name}: {text}");
        }
    }
}

/// Die Commit-Adresse geht als ganze Adresse durch, auch wenn ihre Teile auf
/// einer Liste stehen; der Name allein woanders nicht.
#[test]
fn commit_adresse_als_ausnahme() {
    if !bereit() {
        return;
    }
    let mit_teilen = LISTE.replace("\"kaffee-runde\"]", "\"kaffee-runde\", \"ranke\", \"proton\"]");
    // Das saubere Repo nennt die Adresse in src/main.rs; Commits von Ranke.
    let o = Ort::neu();
    o.liste(&mit_teilen);
    o.ueber_merge(|o| o.datei("src/b.rs", b"// Kontakt: Ranke2@Proton.me.\n"));
    let (code, text) = o.pruefen();
    assert_eq!(code, 0, "die ganze Adresse geht durch: {text}");
    for (name, inhalt) in [
        ("Name allein", "// gebaut von Ranke"),
        ("Name im Wort", "// Rankenwerk"),
        ("Adresse mit Vorsatz", "// xranke2@pro§ton.me"),
        ("Adresse verlängert", "// ranke2@pro§ton.me.example"),
        ("andere Adresse", "// ranke2@pro§ton.invalid"),
    ] {
        let o = Ort::neu();
        o.liste(&mit_teilen);
        o.ueber_merge(|o| o.datei("src/b.rs", format!("{inhalt}\n").as_bytes()));
        let (code, text) = o.pruefen();
        assert_eq!(code, 1, "{name}: {text}");
        assert!(text.contains("src/b.rs, Zeile 1: sensibler Name"), "{name}: {text}");
    }
    // Ohne die Teile auf einer Liste geht der Name wie bisher durch.
    let o = Ort::neu();
    o.ueber_merge(|o| o.datei("src/b.rs", b"// gebaut von Ranke\n"));
    let (code, text) = o.pruefen();
    assert_eq!(code, 0, "{text}");
}

/// Eine kaputte Struktur von PNG, ICO oder
/// WebP geht nicht still durch (dahinter sähe die Suche nach Metadaten nichts).
#[test]
fn kaputte_bilder_sind_ein_fund() {
    if !bereit() {
        return;
    }
    let gut = png(false);
    let mut abgeschnitten = gut.clone();
    abgeschnitten.truncate(gut.len() - 6);
    abgewiesen("PNG abgeschnitten", "PNG kaputt (", |o| {
        o.datei("web/static/icons/k1.png", &abgeschnitten);
        o.commit("x", &[]);
    });
    let mut nach_iend = gut.clone();
    nach_iend.extend_from_slice(b"angehaengt");
    abgewiesen("PNG mit Daten nach IEND", "PNG kaputt (endet nicht sauber mit IEND)", |o| {
        o.datei("web/static/icons/k2.png", &nach_iend);
        o.commit("x", &[]);
    });
    let ohne_iend = gut[..gut.len() - 12].to_vec();
    abgewiesen("PNG ohne IEND", "PNG kaputt (abgeschnitten, ohne IEND am Ende)", |o| {
        o.datei("web/static/icons/k3.png", &ohne_iend);
        o.commit("x", &[]);
    });
    let mut zu_lang = gut.clone();
    zu_lang[8..12].copy_from_slice(&0x0100_0000u32.to_be_bytes());
    abgewiesen("PNG mit zu langem Stück", "PNG kaputt (abgeschnittenes Stück)", |o| {
        o.datei("web/static/icons/k4.png", &zu_lang);
        o.commit("x", &[]);
    });
    abgewiesen("PNG ohne Signatur", "PNG kaputt (keine PNG-Signatur)", |o| {
        o.datei("web/static/icons/k5.png", b"\0\0kein png");
        o.commit("x", &[]);
    });
    let mut riff = webp(None);
    riff.extend_from_slice(&[0, 0]);
    abgewiesen("WebP: Länge passt nicht", "WebP kaputt (die Länge passt nicht zum RIFF-Ende)", |o| {
        o.datei("web/static/icons/k6.webp", &riff);
        o.commit("x", &[]);
    });
    // Ein Stück, das mehr Daten ankündigt, als da sind (die RIFF-Länge stimmt zum Ende).
    let mut stueck = webp(None);
    stueck.extend_from_slice(b"ALPH");
    stueck.extend_from_slice(&100u32.to_le_bytes());
    stueck.extend_from_slice(&[1, 2]);
    let n = (stueck.len() - 8) as u32;
    stueck[4..8].copy_from_slice(&n.to_le_bytes());
    abgewiesen("WebP: Stück abgeschnitten", "WebP kaputt (abgeschnittenes Stück)", |o| {
        o.datei("web/static/icons/k7.webp", &stueck);
        o.commit("x", &[]);
    });
    let mut ico_aussen = ico(false);
    ico_aussen[14..18].copy_from_slice(&5000u32.to_le_bytes());
    abgewiesen("ICO: Bild außerhalb", "ICO kaputt (Bild außerhalb der Datei)", |o| {
        o.datei("web/static/k8.ico", &ico_aussen);
        o.commit("x", &[]);
    });
    // Ein ICO mit einem PNG ohne IEND (Größe im Verzeichnis passt zur Datei).
    let bild = ohne_iend.clone();
    let mut ico_kaputt = vec![0, 0, 1, 0, 1, 0, 1, 1, 0, 0, 1, 0, 32, 0];
    ico_kaputt.extend_from_slice(&(bild.len() as u32).to_le_bytes());
    ico_kaputt.extend_from_slice(&22u32.to_le_bytes());
    ico_kaputt.extend_from_slice(&bild);
    abgewiesen("ICO mit kaputtem PNG", "ICO kaputt (eingebettetes PNG", |o| {
        o.datei("web/static/k9.ico", &ico_kaputt);
        o.commit("x", &[]);
    });
    abgewiesen("ICO ohne Kopf", "ICO kaputt (Kopf)", |o| {
        o.datei("web/static/k10.ico", b"\x01\x02\x03");
        o.commit("x", &[]);
    });
}

/// Geprüft werden die Blobs ALLER neuen
/// Commits des Push-Bereichs, nicht nur die Spitze. Was auf dem Zweig kurz
/// stand und wieder weg ist, geht mit der Geschichte hinaus und wird
/// abgewiesen; beim ersten Push mit neuer Wurzel (alt = 0000…) reicht die Spitze.
#[test]
fn alle_neuen_commits_des_bereichs() {
    if !bereit() {
        return;
    }
    let o = Ort::neu();
    let wurzel = o.git(&["rev-parse", "HEAD"]);
    o.git(&["checkout", "-q", "-b", "zweig"]);
    o.datei("src/kurz.rs", b"// geheim\xc2\xa7projekt\n");
    o.datei("web/static/icons/kurz.png", &png(false)[..20]);
    o.commit("kurz da", &[]);
    std::fs::remove_file(o.repo().join("src/kurz.rs")).unwrap();
    std::fs::remove_file(o.repo().join("web/static/icons/kurz.png")).unwrap();
    o.commit("wieder weg", &[]);
    o.git(&["checkout", "-q", "main"]);
    let mut c = git();
    c.arg("-C").arg(o.repo()).args(["merge", "-q", "--no-ff", "zweig", "-m", "chore(merge): zweig"]);
    for (k, v) in [("GIT_AUTHOR_NAME", "Ranke"), ("GIT_AUTHOR_EMAIL", MAIL), ("GIT_COMMITTER_NAME", "Ranke"), ("GIT_COMMITTER_EMAIL", MAIL)] {
        c.env(k, v);
    }
    assert!(c.output().unwrap().status.success());
    let kopf = o.git(&["rev-parse", "HEAD"]);

    // Die Spitze allein ist sauber.
    let (code, text) = o.lauf(&["-Baum", "main"], None);
    assert_eq!(code, 0, "Spitze sauber: {text}");
    // Erster Push mit neuer Wurzel (alt = 0000…) und -Commits ohne „..“: die
    // ganze erreichbare Geschichte, nicht nur die Spitze.
    for (wie, code_text) in [
        ("erster Push", o.lauf(&["-Hook"], Some(&format!("refs/heads/main {kopf} refs/heads/main {NULL}\n")))),
        ("Hochladen ohne Remote", o.lauf(&["-Baum", "main", "-Commits", "main", "-AufMain"], None)),
        ("Hook", o.lauf(&["-Hook"], Some(&format!("refs/heads/main {kopf} refs/heads/main {wurzel}\n")))),
        ("Hochladen", o.lauf(&["-Baum", "main", "-Commits", &format!("{wurzel}..main"), "-AufMain"], None)),
    ] {
        let (code, text) = code_text;
        assert_eq!(code, 1, "{wie}: {text}");
        assert!(text.contains("src/kurz.rs") && text.contains("sensibler Name"), "{wie}: {text}");
        assert!(text.contains("web/static/icons/kurz.png") && text.contains("PNG kaputt"), "{wie}: {text}");
        assert!(!text.to_lowercase().contains("geheimprojekt"), "{wie}: kein Wert in der Ausgabe: {text}");
    }
    // Ohne Zwischenstände (nur ein sauberer Merge) geht derselbe Bereich durch.
    let o = Ort::neu();
    let wurzel = o.git(&["rev-parse", "HEAD"]);
    o.ueber_merge(|o| o.datei("src/b.rs", b"// b\n"));
    let (code, text) = o.lauf(&["-Baum", "main", "-Commits", &format!("{wurzel}..main"), "-AufMain"], None);
    assert_eq!(code, 0, "{text}");
    let kopf = o.git(&["rev-parse", "HEAD"]);
    let (code, text) = o.lauf(&["-Hook"], Some(&format!("refs/heads/main {kopf} refs/heads/main {NULL}\n")));
    assert_eq!(code, 0, "saubere Geschichte beim ersten Push: {text}");
}

/// Commit mit Ranke, ohne vorher alles hinzuzufügen (der Index bleibt, wie er ist).
fn commit_wie_index(o: &Ort, nachricht: &str) {
    let mut c = git();
    c.arg("-C").arg(o.repo()).args(["commit", "-q", "-m", nachricht]);
    for (k, v) in [("GIT_AUTHOR_NAME", "Ranke"), ("GIT_AUTHOR_EMAIL", MAIL), ("GIT_COMMITTER_NAME", "Ranke"), ("GIT_COMMITTER_EMAIL", MAIL)] {
        c.env(k, v);
    }
    let aus = c.output().unwrap();
    assert!(aus.status.success(), "{}", String::from_utf8_lossy(&aus.stderr));
}

/// Führt `zweig` mit einem Merge-Commit (Ranke) in main zusammen.
fn zusammenfuehren(o: &Ort) {
    o.git(&["checkout", "-q", "main"]);
    let mut c = git();
    c.arg("-C").arg(o.repo()).args(["merge", "-q", "--no-ff", "zweig", "-m", "chore(merge): zweig"]);
    for (k, v) in [("GIT_AUTHOR_NAME", "Ranke"), ("GIT_AUTHOR_EMAIL", MAIL), ("GIT_COMMITTER_NAME", "Ranke"), ("GIT_COMMITTER_EMAIL", MAIL)] {
        c.env(k, v);
    }
    assert!(c.output().unwrap().status.success());
}

/// Jeder Pfad eines Zwischenstands
/// zählt, auch wenn sein Blob unverändert woanders an der Spitze steht
/// (verschoben, leere Datei), und Verknüpfungen (120000) und Gitlinks
/// (160000) mit ihrem echten Modus.
#[test]
fn pfade_und_modi_der_zwischenstaende() {
    if !bereit() {
        return;
    }
    // Verschoben: projects/<slug>/x.md, danach unverändert nach docs/zielbild/x.toml.
    let o = Ort::neu();
    let wurzel = o.git(&["rev-parse", "HEAD"]);
    o.git(&["checkout", "-q", "-b", "zweig"]);
    o.datei("projects/privat/notiz.md", b"# Notiz\n");
    o.commit("privat", &[]);
    std::fs::create_dir_all(o.repo().join("docs/zielbild")).unwrap();
    o.git(&["mv", "projects/privat/notiz.md", "docs/zielbild/notiz.toml"]);
    commit_wie_index(&o, "test: verschoben");
    zusammenfuehren(&o);
    let (code, text) = o.lauf(&["-Baum", "main"], None);
    assert_eq!(code, 0, "Spitze sauber: {text}");
    let (code, text) = o.lauf(&["-Baum", "main", "-Commits", &format!("{wurzel}..main"), "-AufMain"], None);
    assert_eq!(code, 1, "verschoben: {text}");
    assert!(text.contains("projects/privat/notiz.md"), "verschoben: {text}");

    // Leere Datei: derselbe leere Blob steht an der Spitze unter einem erlaubten Pfad.
    let o = Ort::neu();
    o.datei("docs/zielbild/leer.toml", b"");
    o.commit("leer erlaubt", &[]);
    let wurzel = o.git(&["rev-parse", "HEAD"]);
    o.git(&["checkout", "-q", "-b", "zweig"]);
    o.datei("projects/privat/leer.md", b"");
    o.commit("leer privat", &[]);
    std::fs::remove_file(o.repo().join("projects/privat/leer.md")).unwrap();
    o.commit("wieder weg", &[]);
    zusammenfuehren(&o);
    let (code, text) = o.lauf(&["-Baum", "main", "-Commits", &format!("{wurzel}..main"), "-AufMain"], None);
    assert_eq!(code, 1, "leere Datei: {text}");
    assert!(text.contains("projects/privat/leer.md"), "leere Datei: {text}");

    // Verknüpfung und Gitlink in einem Zwischenstand, an der Spitze wieder weg.
    let o = Ort::neu();
    let wurzel = o.git(&["rev-parse", "HEAD"]);
    o.git(&["checkout", "-q", "-b", "zweig"]);
    let blob = o.git(&["hash-object", "-w", "Cargo.toml"]);
    o.git(&["update-index", "--add", "--cacheinfo", &format!("120000,{blob},src/verweis")]);
    o.git(&["update-index", "--add", "--cacheinfo", &format!("160000,{wurzel},src/modul")]);
    commit_wie_index(&o, "test: Verweis und Modul");
    o.git(&["rm", "-q", "--cached", "src/verweis", "src/modul"]);
    commit_wie_index(&o, "test: wieder weg");
    zusammenfuehren(&o);
    let (code, text) = o.lauf(&["-Baum", "main"], None);
    assert_eq!(code, 0, "Spitze sauber: {text}");
    let (code, text) = o.lauf(&["-Baum", "main", "-Commits", &format!("{wurzel}..main"), "-AufMain"], None);
    assert_eq!(code, 1, "Modi: {text}");
    assert!(text.contains("src/verweis") && text.contains("Verknüpfung (Symlink)"), "Symlink: {text}");
    assert!(text.contains("src/modul") && text.contains("kein normaler Dateieintrag"), "Gitlink: {text}");
}

/// Der frische Wurzel-Commit wie `Hochladen.ps1 -Wurzel`: der Baum von main,
/// ohne Eltern, auf refs/heads/oeffentlich, mit Name und Adresse für Autor
/// und Committer.
fn wurzel(o: &Ort, nachricht: &str, name: &str, mail: &str) -> String {
    let baum = o.git(&["rev-parse", "main^{tree}"]);
    let mut c = git();
    c.arg("-C").arg(o.repo()).args(["commit-tree", &baum, "-m", nachricht]);
    for (k, v) in [("GIT_AUTHOR_NAME", name), ("GIT_AUTHOR_EMAIL", mail), ("GIT_COMMITTER_NAME", name), ("GIT_COMMITTER_EMAIL", mail)] {
        c.env(k, v);
    }
    let aus = c.output().unwrap();
    assert!(aus.status.success(), "{}", String::from_utf8_lossy(&aus.stderr));
    let sha = String::from_utf8_lossy(&aus.stdout).trim().to_string();
    o.git(&["update-ref", "refs/heads/oeffentlich", &sha]);
    sha
}

/// Autor und Committer genau Ranke <ranke2@proton.me>,
/// Zeichen für Zeichen. Ein breites ＠ (U+FF20, wie in alten Commits), ein
/// breiter Buchstabe im Namen und die alte Adresse werden abgewiesen.
#[test]
fn adresse_genau_und_nur_ascii() {
    if !bereit() {
        return;
    }
    let alt = "ranke@orches§tration.local".replace('§', "");
    for (name, wer) in [
        ("breites ＠ beim Autor", vec![("GIT_AUTHOR_EMAIL", "ranke2\u{FF20}proton.me")]),
        ("breites ＠ beim Committer", vec![("GIT_COMMITTER_EMAIL", "ranke2\u{FF20}proton.me")]),
        ("breites R im Namen", vec![("GIT_AUTHOR_NAME", "\u{FF32}anke")]),
        ("alte Adresse", vec![("GIT_AUTHOR_EMAIL", alt.as_str()), ("GIT_COMMITTER_EMAIL", alt.as_str())]),
    ] {
        abgewiesen(name, "ist nicht Ranke <ranke2@proton.me>", |o| {
            o.datei("src/b.rs", b"// b\n");
            o.commit("b", &wer);
        });
    }
}

/// Keine Anhänge (Trailer) und keine KI-Namen in
/// Commit-Nachrichten, Groß und Klein egal.
#[test]
fn keine_anhaenge_in_nachrichten() {
    if !bereit() {
        return;
    }
    for (i, nachricht) in [
        "feat: x\n\nCo-Authored-By: Jemand <jemand@example.org>",
        "fix: y\n\nco-authored-by: jemand <jemand@example.org>",
        "fix: y\n\nSigned-off-by: Jemand <jemand@example.org>",
        // KI-Namen mit „§“ (fällt weg): So stehen sie nicht wörtlich in diesem Test.
        "chore: z\n\nGenerated with Cl§aude Code",
        "docs: mit G§PT-4o geschrieben",
        "refactor: Hilfe von O§PUS",
        "test: Son§net oder Hai§ku",
        "chore: Anth§ropic",
        // Striche, Leerzeichen, Buchstaben dahinter, jeder Trailer.
        "fix: y\n\nCo\u{2010}Authored\u{2010}By: Jemand <jemand@example.org>",
        "fix: y\n\nCo Authored By Jemand",
        "chore: mit Cl§audeCode",
        "fix: y\n\nReviewed-by: Jemand <jemand@example.org>",
        "fix: y\n\nChange-Id: I1234",
    ]
    .iter()
    .enumerate()
    {
        abgewiesen(&format!("Anhang {i}"), "Anhang (Trailer)", |o| {
            o.commit(&nachricht.replace('§', ""), &[]);
        });
    }
}

/// KI-Namen (Assistenten, Modelle, Anbieter) auch in Dateien: im Inhalt, über
/// einen Zeilenumbruch, im Dateinamen und im Inhalt einer Binärdatei; der Name
/// steht nie in der Ausgabe. Eine Adresse eines Anbieters ist keine Ausnahme mehr.
/// Gegenproben: ein Name nur als Wortteil, Cargo.lock in der Wurzel mit einem
/// Paket Dritter, Prüfsummen in web/package-lock.json, zufällige Bytes in einem
/// Bild, eine erfundene Adresse und Fachwörter in einem Projekt-Repo gehen durch.
/// Ein „§“ fällt weg (wie in `datei`).
#[test]
fn ki_namen_in_dateien() {
    if !bereit() {
        return;
    }
    let ohne_namen = |text: &str| {
        for n in [
            "cl§aude",
            "anth§ropic",
            "o§pus",
            "son§net",
            "hai§ku",
            "chat§g§pt",
            "g§pt",
            "open§ai",
            "gem§ini",
            "co§pilot",
            "co§dex",
            "lla§ma",
            "mis§tral",
            "fa§ble",
        ] {
            assert!(!text.to_lowercase().contains(&n.replace('§', "")), "der Name darf nicht in der Ausgabe stehen:\n{text}");
        }
    };
    let text = abgewiesen("KI-Name im Inhalt", "KI-Name", |o| {
        o.datei("src/a.rs", "// geschrieben mit Cl§aude\n".as_bytes());
        o.commit("x", &[]);
    });
    ohne_namen(&text);
    let text = abgewiesen("KI-Name mit Buchstaben dahinter", "KI-Name", |o| {
        o.datei("docs/anleitung/b.toml", "text = 'die App Son§netPro'\n".as_bytes());
        o.commit("x", &[]);
    });
    ohne_namen(&text);
    let text = abgewiesen("KI-Name über einen Zeilenumbruch", "KI-Name", |o| {
        o.datei("docs/anleitung/c.toml", "text = '''mit Hai-\n§ku gebaut'''\n".as_bytes());
        o.commit("x", &[]);
    });
    ohne_namen(&text);
    let text = abgewiesen("KI-Name im Dateinamen", "KI-Name", |o| {
        o.datei(&"web/src/lib/o§pus-hilfe.ts".replace('§', ""), b"export const x = 1;\n");
        o.commit("x", &[]);
    });
    ohne_namen(&text);
    assert!(text.to_lowercase().contains("-hilfe.ts (pfad)"), "{text}");
    let text = abgewiesen("KI-Name in einer Binärdatei", "KI-Name (im Inhalt der Binärdatei)", |o| {
        let mut b = png(false);
        b.extend_from_slice("anth§ropic".replace('§', "").as_bytes());
        o.datei("web/static/icons/k.png", &b);
        o.commit("x", &[]);
    });
    ohne_namen(&text);
    abgewiesen("Adresse eines Anbieters", "E-Mail-Adresse", |o| {
        o.datei("src/d.rs", "// noreply@anth§ropic.com\n".as_bytes());
        o.commit("x", &[]);
    });
    // Mit Strich oder Leerzeichen zwischen den Wortteilen.
    for (i, wort) in ["Open§ AI", "Co§-Pilot", "OPEN§_AI"].into_iter().enumerate() {
        let text = abgewiesen(&format!("KI-Name mit Trenner {i}"), "KI-Name", |o| {
            o.datei("src/t.rs", format!("// mit {wort} gebaut\n").as_bytes());
            o.commit("x", &[]);
        });
        ohne_namen(&text);
    }
    // Cargo.lock nur in der Wurzel und nur im Inhalt ausgenommen.
    abgewiesen("verschachtelte Cargo.lock", "ABGEWIESEN crates/x/Cargo.lock, Zeile 2: KI-Name", |o| {
        o.datei("crates/x/Cargo.lock", "[[package]]\nname = \"cl§aude-hilfe\"\n".as_bytes());
        o.commit("x", &[]);
    });
    abgewiesen("KI-Name im Pfad einer Cargo.lock", "/cargo.lock (pfad), Zeile 1: KI-Name", |o| {
        o.datei(&"crates/fa§ble/Cargo.lock".replace('§', ""), b"[[package]]\nname = \"x\"\n");
        o.commit("x", &[]);
    });
    abgewiesen("Chat-Export mit KI-Namen als Rolle", "Chat-Export (mehrere", |o| {
        o.datei("scripts/verlauf2.md", "## Du\nFrage\n\n## Cl§aude\nAntwort\n\n## Du\nNoch eine\n".as_bytes());
        o.commit("x", &[]);
    });
    // Gegenproben: Wortteil (Buchstabe davor), Base64 (Ziffer davor), Cargo.lock,
    // erfundene Adresse, „Open Air“, Prüfsumme mit „/“ davor, Name in Bildbytes.
    let mut bild = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    for (typ, daten) in
        [(*b"IHDR", vec![0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0, 0, 0]), (*b"IDAT", vec![0x01, b'g', b'p', b't', 0x02, b'G', b'P', b'T']), (*b"IEND", vec![])]
    {
        bild.extend_from_slice(&(daten.len() as u32).to_be_bytes());
        bild.extend_from_slice(&typ);
        bild.extend_from_slice(&daten);
        bild.extend_from_slice(&[0, 0, 0, 0]);
    }
    let integritaet = format!("sha512-{}/g§pt{}==", "A".repeat(40), "B".repeat(42));
    let o = Ort::neu();
    o.ueber_merge(|o| {
        o.datei("src/e.rs", "// ein Magnumo§pus, ein Sonett, Prüfsumme 6g§pTPD4, ein Open§-Air\n// jemand@example.org\n".as_bytes());
        o.datei("Cargo.lock", "[[package]]\nname = \"zeitzone-hai§ku\"\nversion = \"0.1.2\"\n".as_bytes());
        o.datei("web/package-lock.json", format!("{{\n  \"integrity\": \"{integritaet}\"\n}}\n").as_bytes());
        o.datei("web/static/icons/z.png", &bild);
    });
    let (code, text) = o.pruefen();
    assert_eq!(code, 0, "{text}");
    // Im Projekt-Repo sind die Namen oft Fachwörter (Sternzeichen, Gedicht, Codec): kein Fund.
    let o = Ort::projekt();
    o.ueber_merge(|o| o.datei("projects/alpha/notiz.txt", "Ge§mini, ein Hai§ku, der Co§-Pilot, O§pus-Codec\n".as_bytes()));
    let (code, text) = o.pruefen_mit(Some(PROJEKT_ADRESSE));
    assert_eq!(code, 0, "{text}");
}

/// `namen.toml` vereint mit jeder Datei in pruefer/tokens/
/// (nur *.toml, LIESMICH.txt zählt nicht). Fehlt der Ordner, ist eine Datei
/// leer, kaputt oder ohne sensibel, scheitert der Prüfer geschlossen und
/// nennt die Datei, nie ihren Inhalt.
#[test]
fn tokens_vereint_und_geschlossen() {
    if !bereit() {
        return;
    }
    abgewiesen("Wort aus tokens", "sensibler Name", |o| {
        o.datei("src/a.rs", b"// TokenWort\n");
        o.commit("x", &[]);
    });
    // sensibel darf in namen.toml leer sein, wenn tokens Werte hat.
    let o = Ort::neu();
    o.liste(&LISTE.replace(SENSIBEL, "sensibel = []"));
    let (code, text) = o.pruefen();
    assert_eq!(code, 0, "{text}");
    let tokens = |o: &Ort| o.wurzel.join("inhalt").join("pruefer").join("tokens");
    for (name, datei, inhalt) in [
        ("leer", "leer.toml", ""),
        ("nur Kommentar", "kommentar.toml", "# geheimwert\n"),
        ("sensibel leer", "leer2.toml", "sensibel = []\n"),
        ("kaputt", "kaputt.toml", "sensibel = [\"geheimwert\"\n"),
        ("andere Liste", "anders.toml", "allgemein = [\"geheimwert\"]\n"),
        ("ohne Anführungszeichen", "ohne.toml", "sensibel = [geheimwert]\n"),
    ] {
        let o = Ort::neu();
        std::fs::write(tokens(&o).join(datei), inhalt).unwrap();
        let (code, text) = o.pruefen();
        assert_eq!(code, 2, "{name}: {text}");
        assert!(text.contains(datei) && text.contains("Es geht nichts hinaus"), "{name}: {text}");
        assert!(!text.contains("geheimwert"), "{name}: kein Inhalt in der Ausgabe: {text}");
    }
    let o = Ort::neu();
    std::fs::remove_dir_all(tokens(&o)).unwrap();
    let (code, text) = o.pruefen();
    assert_eq!(code, 2, "Ordner fehlt: {text}");
    assert!(text.contains("tokens") && text.contains("fehlt"), "{text}");
    let o = Ort::neu();
    std::fs::remove_file(tokens(&o).join("erfunden.toml")).unwrap();
    let (code, text) = o.pruefen();
    assert_eq!(code, 2, "nur LIESMICH.txt: {text}");
}

/// Der frische Wurzel-Commit auf refs/heads/oeffentlich
/// geht durch (-Pruefen, Hook, -Bundle); die alte Geschichte von main mit
/// breitem ＠ und einem Zwischenstand nicht. Im Bundle zählt jede Ref.
#[test]
fn frischer_wurzel_commit_pruefen_und_bundle() {
    if !bereit() {
        return;
    }
    let o = Ort::neu();
    o.git(&["checkout", "-q", "-b", "zweig"]);
    o.datei("src/kurz.rs", b"// geheim\xc2\xa7projekt\n");
    o.commit("kurz da", &[("GIT_AUTHOR_EMAIL", "ranke2\u{FF20}proton.me")]);
    std::fs::remove_file(o.repo().join("src/kurz.rs")).unwrap();
    o.commit("wieder weg", &[]);
    zusammenfuehren(&o);
    let sha = wurzel(&o, "chore: erste öffentliche Fassung", "Ranke", MAIL);
    assert_eq!(o.git(&["rev-list", "--count", "oeffentlich"]), "1");
    let (code, text) = o.lauf(&["-Pruefen"], None);
    assert_eq!(code, 0, "{text}");
    assert!(text.contains("Probelauf"), "{text}");
    let (code, text) = o.lauf(&["-Hook"], Some(&format!("refs/heads/oeffentlich {sha} refs/heads/main {NULL}\n")));
    assert_eq!(code, 0, "Hook: {text}");
    // Die alte Geschichte: in jedem geprüften Bereich gefunden.
    let (code, text) = o.lauf(&["-Pruefen", "-Ref", "main"], None);
    assert_eq!(code, 1, "{text}");
    assert!(text.contains("Autor ist nicht Ranke <ranke2@proton.me>") && text.contains("src/kurz.rs"), "{text}");
    assert!(!text.to_lowercase().contains("geheimprojekt"), "{text}");
    let (code, _) = o.lauf(&["-Pruefen", "-Ref", "gibt-es-nicht"], None);
    assert_eq!(code, 2);
    // Der Wurzel-Commit selbst mit Anhang oder breitem ＠.
    wurzel(&o, "chore: x\n\nCo-Authored-By: Jemand <jemand@example.org>", "Ranke", MAIL);
    let (code, text) = o.lauf(&["-Pruefen"], None);
    assert!(code == 1 && text.contains("Anhang (Trailer)"), "{text}");
    wurzel(&o, "chore: x", "Ranke", "ranke2\u{FF20}proton.me");
    let (code, text) = o.lauf(&["-Pruefen"], None);
    assert!(code == 1 && text.contains("Committer ist nicht"), "{text}");

    // Bundle: nur der saubere Wurzel-Commit geht durch.
    wurzel(&o, "chore: erste öffentliche Fassung", "Ranke", MAIL);
    o.git(&["tag", "v1", "oeffentlich"]);
    let bundle = |name: &str, refs: &[&str]| -> String {
        let p = o.wurzel.join(name).to_string_lossy().to_string();
        let mut a = vec!["bundle", "create", "-q", p.as_str()];
        a.extend_from_slice(refs);
        o.git(&a);
        p
    };
    for (name, refs, code_soll, satz) in [
        ("gut", vec!["refs/heads/oeffentlich"], 0, "Nichts gefunden."),
        ("alt", vec!["refs/heads/main"], 1, "Autor ist nicht Ranke"),
        ("mit Tag", vec!["refs/heads/oeffentlich", "refs/tags/v1"], 1, "nur refs/heads/main oder ein Tag refs/tags/v0.MINOR.PATCH"),
        ("anderer Zweig", vec!["refs/heads/zweig"], 1, "nur der lokale Zweig main oder oeffentlich"),
    ] {
        let p = bundle(&format!("{name}.bundle"), &refs);
        let (code, text) = o.lauf(&["-Bundle", &p], None);
        assert_eq!(code, code_soll, "{name}: {text}");
        assert!(text.contains(satz), "{name}: {text}");
    }
    let alt = bundle("alt2.bundle", &["refs/heads/main"]);
    let (_, text) = o.lauf(&["-Bundle", &alt], None);
    assert!(text.contains("src/kurz.rs") && !text.to_lowercase().contains("geheimprojekt"), "{text}");
    // Ein Bundle mit Voraussetzungen (nicht ab der Wurzel) scheitert geschlossen.
    let teil = bundle("teil.bundle", &["main^..main"]);
    let (code, text) = o.lauf(&["-Bundle", &teil], None);
    assert_eq!(code, 2, "{text}");
}

/// Ausnahmen, je eng: die Datei der Anleitung zu Geheimnissen, die
/// SRI-Prüfsumme in web/vendor/<paket>/HERKUNFT.toml, das Feld mappings einer
/// Quellkarte in web/vendor, ein Platzhalter {NAME} im Format von Rust und
/// PROGRA~1. Daneben bleibt alles ein Fund.
#[test]
fn enge_ausnahmen() {
    if !bereit() {
        return;
    }
    let sri = format!("sha512-{:A<86}==", "tokenwortQ");
    let o = Ort::neu();
    o.ueber_merge(|o| {
        o.datei("docs/anleitung/contributing-07-7-geheimnisse-passwoerter-nie-im-terminal.toml", b"titel = \"Geheimnisse\"\n");
        o.datei("web/vendor/paket/HERKUNFT.toml", format!("pruefsumme = \"{sri}\"\n").as_bytes());
        o.datei("src/a.rs", "// C:\\U§sers\\{NAME}\\dev, /c/U§sers/{NAME}/x, C:\\PROGRA~1\\x.pdf\n".as_bytes());
        o.datei("web/vendor/paket/dist/a.min.js.map", b"{\"version\":3,\"mappings\":\"AAAA,CAACtokenwort;EAAE\"}");
    });
    let (code, text) = o.pruefen();
    assert_eq!(code, 0, "{text}");
    let kurz = "sha512-tokenwort==".to_string();
    for (name, pfad, inhalt, muster) in [
        ("andere Datei der Anleitung", "docs/anleitung/passwoerter.toml", "x\n".to_string(), "Geheimnis-Datei"),
        ("Prüfsumme außerhalb von web/vendor", "scripts/pruefsumme.txt", format!("{sri}\n"), "sensibler Name"),
        ("keine echte Prüfsumme", "web/vendor/paket/HERKUNFT.toml", format!("pruefsumme = \"{kurz}\"\n"), "sensibler Name"),
        ("Platzhalter mit mehr", "src/a.rs", "// C:\\U§sers\\{NAME}x\\dev\n".to_string(), "Benutzerpfad"),
        (
            "Quellkarte außerhalb von mappings",
            "web/vendor/paket/a.js.map",
            "{\"sources\":[\"tokenwort.js\"],\"mappings\":\"AAAA\"}".to_string(),
            "sensibler Name",
        ),
        ("mappings außerhalb von web/vendor", "web/src/a.js.map", "{\"mappings\":\"AAAAtokenwort\"}".to_string(), "sensibler Name"),
    ] {
        abgewiesen(name, muster, |o| {
            o.datei(pfad, inhalt.as_bytes());
            o.commit("x", &[]);
        });
    }
}

/// Binärdateien der Positivliste ganz gegen die Listen
/// (Latin-1), PNG nur mit Stücken der Positivliste, GIF ohne Kommentar, JPEG
/// ohne COM. Saubere GIF und JPEG gehen durch. Ohne Binär-Endung ist eine Datei
/// Text: UTF-16 ohne BOM darin ist „kein UTF-8“.
#[test]
fn binaerdateien_ganz_gegen_die_listen() {
    if !bereit() {
        return;
    }
    let mut gif = b"GIF89a\x01\x00\x01\x00\x00\x00\x00".to_vec();
    let bild: &[u8] = b"\x2c\x00\x00\x00\x00\x01\x00\x01\x00\x00\x02\x02\x44\x01\x00\x3b";
    let jpeg = |extra: &[u8]| -> Vec<u8> {
        let mut v = b"\xff\xd8\xff\xe0\x00\x10JFIF\x00\x01\x01\x00\x00\x01\x00\x01\x00\x00".to_vec();
        v.extend_from_slice(extra);
        v.extend_from_slice(b"\xff\xda\x00\x08\x01\x01\x00\x00\x3f\x00\x12\x34\xff\xd9");
        v
    };
    let o = Ort::neu();
    let mut sauber = gif.clone();
    sauber.extend_from_slice(bild);
    o.ueber_merge(|o| {
        o.datei("web/static/g.gif", &sauber);
        o.datei("web/static/j.jpg", &jpeg(b""));
    });
    let (code, text) = o.pruefen();
    assert_eq!(code, 0, "saubere GIF und JPEG: {text}");

    let mut prvt = png(false);
    let ende = prvt.len() - 12;
    let mut stueck = (9u32).to_be_bytes().to_vec();
    stueck.extend_from_slice(b"prVttokenwort\0\0\0\0");
    prvt.splice(ende..ende, stueck);
    gif.extend_from_slice(b"\x21\xfe\x05hallo\x00");
    gif.extend_from_slice(bild);
    let utf16: Vec<u8> = "x tokenwort\n".encode_utf16().flat_map(|c| c.to_le_bytes()).collect();
    let mut verschoben = vec![b'x'];
    verschoben.extend_from_slice(&utf16);
    for (name, pfad, inhalt, muster) in [
        ("privates PNG-Stück", "web/static/p.png", prvt, "PNG mit Metadaten (prVt)"),
        (
            "Name im privaten PNG-Stück",
            "web/static/p.png",
            png(false).into_iter().chain(b"tokenwort".iter().copied()).collect(),
            "sensibler Name (im Inhalt der Binärdatei)",
        ),
        ("GIF-Kommentar", "web/static/g.gif", gif.clone(), "GIF mit Metadaten"),
        ("JPEG-Kommentar", "web/static/j.jpg", jpeg(b"\xff\xfe\x00\x07hallo"), "JPEG mit Kommentar"),
        ("Schrift ist Text", "web/static/f.woff2", b"wOF2\0\0x\0".to_vec(), "web/static/f.woff2, Zeile 1: kein UTF-8"),
        ("UTF-16 mit Binär-Anschein", "web/static/x.bin", utf16.clone(), "web/static/x.bin, Zeile 1: kein UTF-8"),
        ("Umlaut-Name in .zip", "web/static/y.zip", "x grünkohlfest\n".as_bytes().to_vec(), "web/static/y.zip, Zeile 1: sensibler Name"),
        ("Umlaut-Name in UTF-8 im PDF", "web/static/x.pdf", "%PDF-1.4\nx grünkohlfest\n".as_bytes().to_vec(), "sensibler Name (im Inhalt der Binärdatei)"),
        ("Text als PDF", "web/static/t.pdf", b"x\n".to_vec(), "PDF kaputt (keine PDF-Signatur)"),
        ("Text als JPEG", "web/static/t.jpg", b"x\n".to_vec(), "JPEG kaputt (keine JPEG-Signatur)"),
        ("UTF-16 ohne BOM", "web/static/u.dat", utf16, "web/static/u.dat, Zeile 1: kein UTF-8"),
        ("UTF-16 verschoben", "web/static/v.dat", verschoben, "web/static/v.dat, Zeile 1: kein UTF-8"),
    ] {
        abgewiesen(name, muster, |o| {
            o.datei(pfad, &inhalt);
            o.commit("x", &[]);
        });
    }
}

/// Lange Zeilen ganz, Werte über Zeilenumbrüche.
#[test]
fn lange_zeilen_und_umbrueche() {
    if !bereit() {
        return;
    }
    let lang = "a".repeat(20050);
    for (name, inhalt, muster) in [
        ("Name hinter Zeichen 20000", format!("{lang}tokenwort\n"), "sensibler Name"),
        ("Adresse hinter Zeichen 40000", format!("{lang}{lang} x@firma-erfun§den.de\n"), "E-Mail-Adresse"),
        ("Umbruch", "Vorname\nNachname\n".to_string(), "sensibler Name"),
        ("Trennung", "// geheim-\nprojekt\n".to_string(), "sensibler Name"),
    ] {
        abgewiesen(name, muster, |o| {
            o.datei("src/l.js", inhalt.as_bytes());
            o.commit("x", &[]);
        });
    }
    abgewiesen("Umbruch in der Nachricht", "(Nachricht), Zeile 1: sensibler Name", |o| {
        o.commit("fix: gruss an vorname\nnachname", &[]);
    });
    // Ein anderes Wort dazwischen: kein Wert über die Grenze.
    let o = Ort::neu();
    o.ueber_merge(|o| o.datei("src/l.js", b"vorname sonstwas\nnachname\n"));
    let (code, text) = o.pruefen();
    assert_eq!(code, 0, "{text}");
}

/// Ein Ersatz (git replace) zeigt dem Prüfer nicht das
/// saubere Objekt; eine eigene Kopfzeile im Commit ist ein Fund.
#[test]
fn ersatz_und_kopfzeilen() {
    if !bereit() {
        return;
    }
    abgewiesen("git replace", "sensibler Name", |o| {
        let sauber = o.git(&["rev-parse", "HEAD"]);
        o.ueber_merge(|o| o.datei("src/k.txt", b"tokenwort\n"));
        let kopf = o.git(&["rev-parse", "HEAD"]);
        o.git(&["replace", &kopf, &sauber]);
    });
    abgewiesen("eigene Kopfzeile", "Kopfzeile außer tree", |o| {
        let baum = o.git(&["rev-parse", "HEAD^{tree}"]);
        let eltern = o.git(&["rev-parse", "HEAD"]);
        let wer = format!("Ranke <{MAIL}> 1700000000 +0100");
        let p = o.wurzel.join("commit.txt");
        std::fs::write(&p, format!("tree {baum}\nparent {eltern}\nauthor {wer}\ncommitter {wer}\nx-notiz tokenwort\n\nfix: x\n")).unwrap();
        let sha = o.git(&["hash-object", "-t", "commit", "--literally", "-w", &p.to_string_lossy()]);
        o.git(&["update-ref", "refs/heads/main", &sha]);
    });
}

/// git mit Bytes auf der Standardeingabe (für mktree mit Pfaden, die kein UTF-8 sind).
fn git_ein(o: &Ort, args: &[&str], ein: &[u8]) -> String {
    use std::io::Write;
    let mut k = git().arg("-C").arg(o.repo()).args(args).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    k.stdin.take().unwrap().write_all(ein).unwrap();
    let aus = k.wait_with_output().unwrap();
    assert!(aus.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&aus.stderr));
    String::from_utf8_lossy(&aus.stdout).trim().to_string()
}

/// Ein handgebautes Objekt (`art` commit oder tag) aus rohen Bytes; gibt die SHA.
fn roh_objekt(o: &Ort, art: &str, b: &[u8]) -> String {
    let p = o.wurzel.join("roh-objekt.txt");
    std::fs::write(&p, b).unwrap();
    o.git(&["hash-object", "-t", art, "--literally", "-w", &p.to_string_lossy()])
}

/// Ein handgebauter Commit auf HEAD mit roher Nachricht und Kopfzeile `wer` (Bytes), wird main.
fn roh_commit(o: &Ort, wer: &[u8], nachricht: &[u8]) {
    let baum = o.git(&["rev-parse", "HEAD^{tree}"]);
    let eltern = o.git(&["rev-parse", "HEAD"]);
    let mut b = format!("tree {baum}\nparent {eltern}\nauthor ").into_bytes();
    b.extend_from_slice(wer);
    b.extend_from_slice(b"\ncommitter ");
    b.extend_from_slice(wer);
    b.extend_from_slice(b"\n\n");
    b.extend_from_slice(nachricht);
    let sha = roh_objekt(o, "commit", &b);
    o.git(&["update-ref", "refs/heads/main", &sha]);
}

/// Eine erfundene tokens-Datei mit „müller“ (UTF-8, im Quelltext nur als Escape).
fn mueller_liste(o: &Ort) {
    std::fs::write(o.wurzel.join("inhalt").join("pruefer").join("tokens").join("m.toml"), "sensibel = [\"m\u{fc}ller\"]\n").unwrap();
}

/// „Müller“ in Windows-1252 (4D FC 6C 6C 65 72) darf weder in der Ausgabe stehen noch durchgehen.
fn ohne_mueller(name: &str, text: &str) {
    let t = text.to_lowercase();
    for w in ["m\u{fc}ller", "mueller", "muller", "m\u{fffd}ller"] {
        assert!(!t.contains(w), "{name}: „{w}“ steht in der Ausgabe:\n{text}");
    }
}

/// Eine Regel. Was kein gültiges UTF-8 ist (Blob, Pfad, Meldung,
/// Autor, Tag-Text) oder als Text Steuerzeichen außer Tab, LF, CR hat (NUL
/// aus UTF-16, BOM von UTF-16), ist ein Fund „kein UTF-8“ mit der Stelle, nie
/// dem Inhalt. Es gibt keine zweite Lesart (Windows-1252 findet keinen Namen).
#[test]
fn kodierung_streng_utf8() {
    if !bereit() {
        return;
    }
    let wer = format!("Ranke <{MAIL}> 1700000000 +0100");
    let t = abgewiesen("1252 in .ts", "web/src/lib/a.ts, Zeile 2: kein UTF-8", |o| {
        mueller_liste(o);
        o.datei("web/src/lib/a.ts", b"// x\n// Gruss an M\xfcller\n");
        o.commit("x", &[]);
    });
    assert!(!t.contains("sensibler Name"), "keine zweite Lesart: {t}");
    ohne_mueller("1252 in .ts", &t);
    let t = abgewiesen("1252 in der Nachricht", "kein UTF-8", |o| {
        mueller_liste(o);
        roh_commit(o, wer.as_bytes(), b"fix: Gruss an M\xfcller\n");
    });
    ohne_mueller("1252 in der Nachricht", &t);
    let t = abgewiesen("1252 im Autor", "kein UTF-8", |o| {
        mueller_liste(o);
        let mut w = b"M\xfcller <".to_vec();
        w.extend_from_slice(format!("{MAIL}> 1700000000 +0100").as_bytes());
        roh_commit(o, &w, b"fix: x\n");
    });
    assert!(t.contains("Autor ist nicht Ranke"), "{t}");
    ohne_mueller("1252 im Autor", &t);
    let t = abgewiesen("1252 im Pfad", "scripts/\u{2026}: kein UTF-8 (Pfad)", |o| {
        mueller_liste(o);
        let blob = git_ein(o, &["hash-object", "-w", "--stdin"], b"// x\n");
        let mut eintrag = format!("100644 blob {blob}\tM").into_bytes();
        eintrag.extend_from_slice(b"\xfcller.ts\n");
        let unter = git_ein(o, &["mktree"], &eintrag);
        let mut wurzel = o.git(&["ls-tree", "HEAD"]).into_bytes();
        wurzel.extend_from_slice(format!("\n040000 tree {unter}\tscripts\n").as_bytes());
        let baum = git_ein(o, &["mktree"], &wurzel);
        let eltern = o.git(&["rev-parse", "HEAD"]);
        let sha = o.git(&["commit-tree", &baum, "-p", &eltern, "-m", "test: x"]);
        o.git(&["update-ref", "refs/heads/main", &sha]);
    });
    ohne_mueller("1252 im Pfad", &t);
    // Tag-Text: eine Ref auf ein Tag-Objekt mit Windows-1252 in der Meldung.
    let o = Ort::neu();
    let kopf = o.git(&["rev-parse", "HEAD"]);
    let mut tag = format!("object {kopf}\ntype commit\ntag v2\ntagger {wer}\n\nGruss an M").into_bytes();
    tag.extend_from_slice(b"\xfcller\n");
    let tag = roh_objekt(&o, "tag", &tag);
    let (code, text) = o.lauf(&["-Hook"], Some(&format!("refs/heads/main {tag} refs/heads/main {NULL}\n")));
    assert_eq!(code, 1, "{text}");
    assert!(text.contains("kein UTF-8") && text.contains("zeigt auf kein Commit"), "Tag-Text: {text}");
    ohne_mueller("Tag-Text", &text);
    // Textdatei mit BOM von UTF-16 oder mit NUL: ebenso ein Fund.
    abgewiesen("BOM von UTF-16", "scripts/u.txt, Zeile 1: kein UTF-8 (ung\u{fc}ltige Bytes", |o| {
        o.datei("scripts/u.txt", b"\xff\xfex\0\n\0");
        o.commit("x", &[]);
    });
    abgewiesen("NUL im Text", "src/a.rs, Zeile 2: kein UTF-8 (Steuerzeichen", |o| {
        o.datei("src/a.rs", b"// a\n// b\0c\n");
        o.commit("x", &[]);
    });
    // Gültiges UTF-8 mit Umlaut, auch mit BOM, geht durch.
    let o = Ort::neu();
    mueller_liste(&o);
    o.ueber_merge(|o| {
        o.datei("web/src/lib/a.ts", "// Gr\u{fc}\u{df}e\n".as_bytes());
        o.datei("web/src/lib/b.ts", "\u{feff}// Gr\u{fc}\u{df}e\n".as_bytes());
    });
    let (code, text) = o.pruefen();
    assert_eq!(code, 0, "{text}");
}

/// Eine Liste,
/// die kein gültiges UTF-8 ist, U+FFFD enthält oder eine Liste zweimal nennt,
/// ist kaputt (Exit 2); ebenso eine Datei im Ordner tokens, die nicht zählt.
#[test]
fn listen_streng_und_einmal() {
    if !bereit() {
        return;
    }
    let tokens = |o: &Ort| o.wurzel.join("inhalt").join("pruefer").join("tokens");
    for (name, datei, inhalt) in [
        ("Windows-1252", "ansi.toml", b"sensibel = [\"m\xfcller\"]\n".to_vec()),
        ("U+FFFD", "ersatz.toml", "sensibel = [\"m\u{fffd}ller\"]\n".as_bytes().to_vec()),
        ("sensibel zweimal", "zweimal.toml", b"sensibel = [\"geheimwert\"]\nsensibel = [\"anderer\"]\n".to_vec()),
        // Ein TOML-Escape außer \\ wirkte still nicht.
        ("TOML-Escape", "escape.toml", b"sensibel = [\"geheimwert\", \"sch\\tckle\"]\n".to_vec()),
        ("TOML-Escape mit Anführungszeichen", "escape2.toml", b"sensibel = [\"geheim\\\"wert\"]\n".to_vec()),
    ] {
        let o = Ort::neu();
        std::fs::write(tokens(&o).join(datei), inhalt).unwrap();
        let (code, text) = o.pruefen();
        assert_eq!(code, 2, "{name}: {text}");
        assert!(text.contains(datei) && text.contains("kaputt") && text.contains("Es geht nichts hinaus"), "{name}: {text}");
        assert!(!text.contains("geheimwert") && !text.to_lowercase().contains("ller\""), "{name}: {text}");
    }
    let o = Ort::neu();
    o.liste(&format!("{LISTE}tabu = [\"anderer\"]\n"));
    let (code, text) = o.pruefen();
    assert!(code == 2 && text.contains("tabu steht zweimal da"), "namen.toml: {text}");
    for (name, anlegen) in [("Datei .bak", true), ("Unterordner", false)] {
        let o = Ort::neu();
        if anlegen {
            std::fs::write(tokens(&o).join("alt.toml.bak"), "sensibel = [\"x\"]\n").unwrap();
        } else {
            std::fs::create_dir_all(tokens(&o).join("unter")).unwrap();
        }
        let (code, text) = o.pruefen();
        assert!(code == 2 && text.contains("das nicht z\u{e4}hlt"), "{name}: {text}");
    }
}

/// Auch Adressen und MAC in einem Pfad oder Ref-Namen
/// stehen nie im Klartext in der Fundzeile.
#[test]
fn fundzeilen_maskieren_auch_adressen() {
    if !bereit() {
        return;
    }
    let mail = "jemand@firma-erfun§den.de".replace('§', "");
    let mac = "3c-52-82-1a-2b-§3c".replace('§', "");
    let o = Ort::neu();
    o.ueber_merge(|o| {
        o.datei(&format!("src/{mail}/a.rs"), b"// x\n");
        o.datei(&format!("src/{mac}/b.rs"), b"// x\n");
    });
    let (code, text) = o.pruefen();
    assert_eq!(code, 1, "{text}");
    assert!(text.contains("E-Mail-Adresse") && text.contains("MAC-Adresse"), "{text}");
    assert!(!text.contains("firma-erfunden") && !text.contains("3c-52"), "{text}");
    o.git(&["branch", &mail, "main"]);
    let (code, text) = o.lauf(&["-Pruefen", "-Ref", &format!("refs/heads/{mail}")], None);
    assert!(code == 1 && text.contains("E-Mail-Adresse") && !text.contains("firma-erfunden"), "Ref: {text}");
}

/// Mit Funden nennt der Bericht von Wurzel-Entwurf.ps1
/// keine Ordner und Dateien; ohne Funde die ganze Liste.
#[test]
fn wurzel_entwurf_ohne_dateien_bei_funden() {
    if !bereit() {
        return;
    }
    let entwurf = |o: &Ort| -> (i32, String) {
        let ziel = o.wurzel.join("klon");
        let bericht = o.wurzel.join("bericht.md");
        let aus = pwsh()
            .unwrap()
            .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"])
            .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scripts").join("Wurzel-Entwurf.ps1"))
            .arg("-Repo")
            .arg(o.repo())
            .arg("-Ziel")
            .arg(&ziel)
            .arg("-Inhalt")
            .arg(o.wurzel.join("inhalt"))
            .arg("-Bericht")
            .arg(&bericht)
            .stdin(Stdio::null())
            .output()
            .unwrap();
        (aus.status.code().unwrap_or(-1), std::fs::read_to_string(&bericht).unwrap_or_default())
    };
    let o = Ort::neu();
    let (code, bericht) = entwurf(&o);
    assert_eq!(code, 0, "{bericht}");
    assert!(bericht.contains("src/main.rs"), "{bericht}");
    let o = Ort::neu();
    o.datei("src/tokenwort/a.rs", b"// x\n");
    o.commit("x", &[]);
    let (code, bericht) = entwurf(&o);
    assert_eq!(code, 1, "{bericht}");
    assert!(bericht.contains("stehen hier nicht"), "{bericht}");
    assert!(!bericht.contains("src/main.rs") && !bericht.to_lowercase().contains("tokenwort"), "{bericht}");
}

/// Trenner . + / und jeder
/// Strich wie U+2010 und U+2013; UTF-32 mit BOM ist kein UTF-8.
#[test]
fn trenner_und_utf32() {
    if !bereit() {
        return;
    }
    for (name, inhalt) in [
        ("Punkt", "// vorname.nachname\n"),
        ("Plus", "// vorname+nachname\n"),
        ("Schrägstrich", "// vorname/nachname\n"),
        ("U+2010", "// kaffee\u{2010}runde\n"),
        ("U+2013", "// kaffee\u{2013}runde\n"),
    ] {
        abgewiesen(name, "sensibler Name", |o| {
            o.datei("web/src/lib/a.ts", inhalt.as_bytes());
            o.commit("x", &[]);
        });
    }
    for (name, bom, gross) in [("UTF-32 LE", [0xFFu8, 0xFE, 0, 0], false), ("UTF-32 BE", [0, 0, 0xFE, 0xFF], true)] {
        abgewiesen(name, "scripts/u.txt, Zeile 1: kein UTF-8", |o| {
            let mut b = bom.to_vec();
            for c in "x tokenwort\n".chars() {
                let u = c as u32;
                b.extend_from_slice(&if gross { u.to_be_bytes() } else { u.to_le_bytes() });
            }
            o.datei("scripts/u.txt", &b);
            o.commit("x", &[]);
        });
    }
}

/// Die Nachricht aus dem rohen Objekt (auch hinter
/// NUL, das selbst „kein UTF-8“ ist), Anhang über einen Zeilenumbruch, die
/// Kopfzeile author genau.
#[test]
fn nachricht_roh_und_kopfzeile_genau() {
    if !bereit() {
        return;
    }
    let wer = format!("Ranke <{MAIL}> 1700000000 +0100");
    let t = abgewiesen("hinter NUL", "sensibler Name", |o| roh_commit(o, wer.as_bytes(), b"fix: x\0tokenwort\n"));
    assert!(t.contains("(Nachricht), Zeile 1: kein UTF-8 (Steuerzeichen"), "{t}");
    abgewiesen("Anhang über den Umbruch", "(\u{fc}ber einen Zeilenumbruch)", |o| {
        o.commit("fix: y\n\nCo-Authored-\nBy Jemand", &[]);
    });
    abgewiesen("Kopfzeile nicht genau", "Kopfzeile author oder committer ist nicht genau", |o| {
        roh_commit(o, format!("Ranke  <{MAIL}> 1700000000 +0100").as_bytes(), b"fix: x\n");
    });
}

/// NUL zwischen den Buchstaben (UTF-16 oder UTF-32 ohne
/// BOM) hinter 8000 Bytes Text, in einer Nachricht und in web/static ist
/// „kein UTF-8“ mit der Zeile.
#[test]
fn nul_zwischen_den_buchstaben() {
    if !bereit() {
        return;
    }
    let utf16 = |t: &str| -> Vec<u8> { t.encode_utf16().flat_map(|c| c.to_le_bytes()).collect() };
    let mut lang = "// x\n".repeat(1700).into_bytes();
    lang.extend_from_slice(&utf16("tokenwort\n"));
    abgewiesen("UTF-16 hinter 8000 Bytes", "web/src/lib/a.ts, Zeile 1701: kein UTF-8", |o| {
        o.datei("web/src/lib/a.ts", &lang);
        o.commit("x", &[]);
    });
    let wer = format!("Ranke <{MAIL}> 1700000000 +0100");
    abgewiesen("UTF-16 in der Nachricht", "(Nachricht), Zeile 2: kein UTF-8", |o| {
        let mut n = b"fix: Probe\n".to_vec();
        n.extend_from_slice(&utf16("tokenwort"));
        roh_commit(o, wer.as_bytes(), &n);
    });
    abgewiesen("UTF-32 ohne BOM in web/static", "web/static/y.txt, Zeile 1: kein UTF-8", |o| {
        let b: Vec<u8> = "x tokenwort\n".chars().flat_map(|c| (c as u32).to_le_bytes()).collect();
        o.datei("web/static/y.txt", &b);
        o.commit("x", &[]);
    });
}

/// Ein URL-kodierter Benutzerpfad (einfach und doppelt)
/// und eine SID im Pfad stehen in keinem Teil in der Fundzeile.
#[test]
fn fundzeilen_maskieren_kodierte_benutzerpfade() {
    if !bereit() {
        return;
    }
    let o = Ort::neu();
    let pfade =
        ["src/c%3A%5CUs§ers%5Cmaxmustermann.rs", "src/c%253A%255CUs§ers%255Cerikaerfunden.rs", "src/S-1-5-21-§1234567890-2345678901-3456789012-1013.rs"]
            .map(|p| p.replace('§', ""));
    o.ueber_merge(|o| {
        for p in &pfade {
            o.datei(p, b"// x\n");
        }
    });
    let (code, text) = o.pruefen();
    assert_eq!(code, 1, "{text}");
    assert!(text.contains("Benutzerpfad") && text.contains("SID eines Kontos"), "{text}");
    for w in ["mustermann", "erfunden", "1234567890", "2345678901", "3456789012"] {
        assert!(!text.contains(w), "„{w}“ steht in der Ausgabe:\n{text}");
    }
}

/// Auch die FEHLER-Zeile von Wurzel-Entwurf.ps1 nennt das
/// Benutzerprofil nicht.
#[test]
fn wurzel_entwurf_fehlerzeile_ohne_profil() {
    if !bereit() {
        return;
    }
    let o = Ort::neu();
    let aus = pwsh()
        .unwrap()
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scripts").join("Wurzel-Entwurf.ps1"))
        .arg("-Repo")
        .arg(o.wurzel.join("fehlt"))
        .arg("-Ziel")
        .arg(o.wurzel.join("klon"))
        .arg("-Bericht")
        .arg(o.wurzel.join("bericht.md"))
        .stdin(Stdio::null())
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&aus.stdout);
    assert_eq!(aus.status.code(), Some(2), "{text}");
    assert!(text.contains("FEHLER"), "{text}");
    if let Ok(profil) = std::env::var("USERPROFILE") {
        assert!(!text.to_lowercase().contains(&profil.to_lowercase()), "{text}");
    }
}

/// Teil 4: Cargo.toml mit der Version unter [package] (davor eine andere unter
/// [workspace.package]) und CHANGELOG.txt, mit oder ohne Abschnitt der Fassung.
fn fassung_setzen(o: &Ort, v: &str, abschnitt: bool) {
    o.datei("Cargo.toml", format!("[workspace.package]\nversion = \"0.9.0\"\n\n[package]\nname = \"x\"\nversion = \"{v}\"\n").as_bytes());
    let fassung = if abschnitt { format!("{v} - 2026-11-05\n------------------\n\nBehoben\n- Ein Fehler.\n") } else { String::new() };
    o.datei("CHANGELOG.txt", format!("\u{feff}CHANGELOG\n=========\n\nUnreleased\n----------\n\n{fassung}").as_bytes());
}

/// Teil 3: Tags. Hinaus geht nur ein neuer, leichter Tag refs/tags/v0.MINOR.PATCH,
/// lokal und fern derselbe Name, auf der First-Parent-Linie von main (aus
/// demselben Push oder fern schon da); im Bundle auf der Linie seiner Ref main.
/// Teil 4: im getaggten Commit hat Cargo.toml die Version des Tags und
/// CHANGELOG.txt ihren Abschnitt.
#[test]
fn tags_nur_v0_leicht_neu_auf_main() {
    if !bereit() {
        return;
    }
    let o = Ort::neu();
    // Die Wurzel trägt schon die Fassung 0.1.0, main und der Zweig erben sie.
    fassung_setzen(&o, "0.1.0", true);
    o.git(&["add", "-A"]);
    let mut c = git();
    c.arg("-C").arg(o.repo()).args(["commit", "-q", "--amend", "--no-edit"]);
    for (k, v) in [("GIT_AUTHOR_NAME", "Ranke"), ("GIT_AUTHOR_EMAIL", MAIL), ("GIT_COMMITTER_NAME", "Ranke"), ("GIT_COMMITTER_EMAIL", MAIL)] {
        c.env(k, v);
    }
    assert!(c.output().unwrap().status.success());
    o.ueber_merge(|o| o.datei("src/b.rs", b"// b\n"));
    let main = o.git(&["rev-parse", "main"]);
    let wurzel = o.git(&["rev-parse", "main^1"]);
    let seitlich = o.git(&["rev-parse", "zweig"]);
    let haupt = format!("refs/heads/main {main} refs/heads/main {NULL}\n");
    let hook = |zeilen: &str| o.lauf(&["-Hook", "-RemoteName", "origin"], Some(zeilen));
    // Angenommen: Merge-Commit von main mit main im selben Push; der Wurzel-Commit auch.
    for ziel in [&main, &wurzel] {
        let (code, text) = hook(&format!("{haupt}refs/tags/v0.1.0 {ziel} refs/tags/v0.1.0 {NULL}\n"));
        assert_eq!(code, 0, "Tag auf {ziel}: {text}");
    }
    // Je Fassung ein eigenes Repo; ohne Abschnitt im Changelog oder mit anderer Version ein Fund.
    for (name, abschnitt, satz) in [("0.0.0", true, None), ("0.10.2", true, None), ("0.2.0", false, Some("ohne Abschnitt \"0.2.0 - JJJJ-MM-TT\""))] {
        let f = Ort::neu();
        f.ueber_merge(|f| fassung_setzen(f, name, abschnitt));
        let spitze = f.git(&["rev-parse", "main"]);
        let zeilen = |tag: &str| format!("refs/heads/main {spitze} refs/heads/main {NULL}\nrefs/tags/v{tag} {spitze} refs/tags/v{tag} {NULL}\n");
        let (code, text) = f.lauf(&["-Hook", "-RemoteName", "origin"], Some(&zeilen(name)));
        match satz {
            None => assert_eq!(code, 0, "{name}: {text}"),
            Some(s) => assert!(code == 1 && text.contains(s) && !text.contains("passt nicht zur Version"), "{name}: erwartet „{s}“:\n{text}"),
        }
        // Die Version aus [workspace.package] zählt nicht, nur die unter [package].
        let (code, text) = f.lauf(&["-Hook", "-RemoteName", "origin"], Some(&zeilen("0.9.0")));
        assert!(code == 1 && text.contains("Tag passt nicht zur Version") && text.contains("version = \"0.9.0\""), "{name} als 0.9.0: {text}");
    }
    // Gegenprobe: v0.5.0 mit Version nur in einem String oder unter [[bin]], Abschnitt ohne
    // Strichzeile, ungültiges Datum, nicht die oberste Fassung.
    let paket = "[package]\nname = \"x\"\nversion = \"0.5.0\"\n";
    let abschnitt = "0.5.0 - 2026-11-05\n------------------\n";
    for (wo, cargo, fassungen, satz) in [
        ("String", "[package]\nname = \"x\"\ndescription = \"\"\"\nversion = \"0.5.0\"\n\"\"\"\nversion = \"0.1.0\"\n", abschnitt, "passt nicht zur Version"),
        (
            "[[bin]]",
            "[workspace.package]\nversion = \"0.1.0\"\n\n[package]\nname = \"x\"\nversion.workspace = true\n\n[[bin]]\nname = \"x\"\nversion = \"0.5.0\"\n",
            abschnitt,
            "passt nicht zur Version",
        ),
        ("ohne Striche", paket, "Geändert\n- Text:\n0.5.0 - 2026-11-05\n", "ohne Abschnitt"),
        ("Datum", paket, "0.5.0 - 2026-99-99\n------------------\n", "ohne Abschnitt"),
        ("nicht oben", paket, "0.6.0 - 2026-11-06\n------------------\n\n0.5.0 - 2026-11-05\n------------------\n", "ohne Abschnitt"),
    ] {
        let f = Ort::neu();
        f.ueber_merge(|f| {
            f.datei("Cargo.toml", cargo.as_bytes());
            f.datei("CHANGELOG.txt", format!("CHANGELOG\n=========\n\nUnreleased\n----------\n\n{fassungen}").as_bytes());
        });
        let spitze = f.git(&["rev-parse", "main"]);
        let zeilen = format!("refs/heads/main {spitze} refs/heads/main {NULL}\nrefs/tags/v0.5.0 {spitze} refs/tags/v0.5.0 {NULL}\n");
        let (code, text) = f.lauf(&["-Hook", "-RemoteName", "origin"], Some(&zeilen));
        assert!(code == 1 && text.contains(satz), "{wo}: erwartet „{satz}“:\n{text}");
    }
    // Ohne main im Push und ohne main fern: geschlossen ein Fund.
    let allein = format!("refs/tags/v0.1.0 {main} refs/tags/v0.1.0 {NULL}\n");
    let (code, text) = hook(&allein);
    assert!(code == 1 && text.contains("Tag ohne main"), "{text}");
    // Tag allein: main fern fragt der Prüfer beim Remote selbst (ls-remote), nicht die
    // Remote-Tracking-Ref. Die ist lokal frei setzbar; zeigt sie auf ein Seiten-Commit,
    // ginge sonst dessen ganze Geschichte ungeprüft hinaus.
    let leer = o.wurzel.join("leer.git").to_string_lossy().to_string();
    let fern = o.wurzel.join("fern.git").to_string_lossy().to_string();
    for p in [&leer, &fern] {
        assert!(git().args(["init", "-q", "--bare", p]).status().unwrap().success());
    }
    o.git(&["push", "-q", &fern, "main:main"]);
    o.git(&["update-ref", "refs/remotes/origin/main", &seitlich]);
    let nach = |url: &str, zeilen: &str| o.lauf(&["-Hook", "-RemoteName", "origin", "-RemoteUrl", url], Some(zeilen));
    let (code, text) = nach(&fern, &allein);
    assert_eq!(code, 0, "Tag allein, main fern: {text}");
    let (code, text) = nach(&fern, &format!("refs/tags/v0.4.0 {seitlich} refs/tags/v0.4.0 {NULL}\n"));
    assert!(code == 1 && text.contains("nicht auf der First-Parent-Linie"), "Tracking-Ref auf Seiten-Commit: {text}");
    let (code, text) = nach(&leer, &allein);
    assert!(code == 1 && text.contains("Tag ohne main"), "Remote ohne main: {text}");
    // Ebenso über die Zeile nach main, deren fernSha schon main ist.
    let (code, text) =
        o.lauf(&["-Hook"], Some(&format!("refs/heads/main {main} refs/heads/main {wurzel}\nrefs/tags/v0.1.0 {wurzel} refs/tags/v0.1.0 {NULL}\n")));
    assert_eq!(code, 0, "Tag auf dem fernen main: {text}");

    o.git(&["-c", "user.name=Ranke", "-c", "user.email=ranke2@proton.me", "tag", "-a", "v0.3.0", "-m", "chore: Fassung", &main]);
    let objekt = o.git(&["rev-parse", "refs/tags/v0.3.0"]);
    for (zeile, satz) in [
        (format!("refs/tags/v1.0.0 {main} refs/tags/v1.0.0 {NULL}\n"), "Tag refs/tags/v0.MINOR.PATCH"),
        (format!("refs/tags/v0.01.0 {main} refs/tags/v0.01.0 {NULL}\n"), "Tag refs/tags/v0.MINOR.PATCH"),
        (format!("refs/tags/v0.1 {main} refs/tags/v0.1 {NULL}\n"), "Tag refs/tags/v0.MINOR.PATCH"),
        (format!("refs/tags/v0.3.0 {objekt} refs/tags/v0.3.0 {NULL}\n"), "Tag-Objekt"),
        (format!("refs/tags/v0.1.0 {seitlich} refs/tags/v0.1.0 {NULL}\n"), "nicht auf der First-Parent-Linie"),
        (format!("refs/tags/v0.1.0 {main} refs/tags/v0.1.0 {wurzel}\n"), "fern schon"),
        (format!("(delete) {NULL} refs/tags/v0.1.0 {main}\n"), "Löschen eines Tags"),
        (format!("refs/tags/v0.2.0 {main} refs/tags/v0.1.0 {NULL}\n"), "anderem Namen"),
        (format!("refs/tags/v0.5.0 {main} refs/tags/v0.5.0 {NULL}\n"), "Tag passt nicht zur Version"),
    ] {
        let (code, text) = hook(&format!("{haupt}{zeile}"));
        assert_eq!(code, 1, "{zeile}: {text}");
        assert!(text.contains(satz), "{zeile}: erwartet „{satz}“:\n{text}");
    }

    // Bundle: Tags nach derselben Regel, gegen die Ref main des Bundles.
    o.git(&["tag", "v0.1.0", &main]);
    o.git(&["tag", "v0.4.0", &seitlich]);
    o.git(&["branch", "oeffentlich", &main]);
    for (name, refs, code_soll, satz) in [
        ("gut", vec!["refs/heads/main", "refs/tags/v0.1.0"], 0, "Nichts gefunden."),
        ("oeffentlich", vec!["refs/heads/oeffentlich", "refs/tags/v0.1.0"], 0, "Nichts gefunden."),
        ("seitlich", vec!["refs/heads/main", "refs/tags/v0.4.0"], 1, "nicht auf der First-Parent-Linie"),
        ("Tag-Objekt", vec!["refs/heads/main", "refs/tags/v0.3.0"], 1, "Tag-Objekt"),
        ("nur Tag", vec!["refs/tags/v0.1.0"], 1, "Tag ohne main"),
    ] {
        let p = o.wurzel.join(format!("tag-{name}.bundle")).to_string_lossy().to_string();
        o.git(&[&["bundle", "create", "-q", &p][..], &refs].concat());
        let (code, text) = o.lauf(&["-Bundle", &p], None);
        assert_eq!(code, code_soll, "Bundle {name}: {text}");
        assert!(text.contains(satz), "Bundle {name}: {text}");
    }
}

/// Teil 3: Hook commit-msg über `Vor-Push.ps1 -Nachricht` (ohne Namenslisten):
/// erste Zeile nach Conventional Commits, keine Anhänge, keine KI-Namen, kein
/// Fuß BREAKING CHANGE; Kommentarzeilen und alles ab der Schere zählen nicht.
#[test]
fn commit_nachricht_nach_conventional_commits() {
    if !bereit() {
        return;
    }
    let o = Ort::neu();
    // Ein KI-Name, zusammengesetzt, damit er nicht wörtlich hier steht.
    let ki = ["Son", "net"].concat();
    let schere = "# ------------------------ >8 ------------------------\n# Do not modify or remove the line above.\n";
    let nachricht = |text: &str| {
        let p = o.wurzel.join("COMMIT_EDITMSG");
        std::fs::write(&p, text).unwrap();
        o.lauf(&["-Nachricht", &p.to_string_lossy()], None)
    };
    for gut in [
        "feat: neue Seite\n".to_string(),
        "fix(web): Knopf reagiert wieder auf Enter\n\nLanger Text dazu.\n".to_string(),
        "feat!: Tags nur noch v0.x\n".to_string(),
        "refactor(api)!: Antwort neu\r\n".to_string(),
        "chore(merge): zweig nach main\n".to_string(),
        "revert: x\n".to_string(),
        "fix(web-ui): x\n".to_string(),
        format!("\n# Kommentar mit {ki}\n# Co-Authored-By: Jemand\nchore: aufgeräumt\n"),
        format!("docs: Anleitung\n{schere}diff --git a/x b/x\n+Co-Authored-By: Jemand <jemand@example.org>\n+{ki}\n"),
    ] {
        let (code, text) = nachricht(&gut);
        assert_eq!(code, 0, "{gut:?}: {text}");
    }
    for (schlecht, satz) in [
        ("neue Seite\n".to_string(), "Conventional Commits"),
        ("Feat: x\n".to_string(), "Conventional Commits"),
        ("feature: x\n".to_string(), "Conventional Commits"),
        ("fix:x\n".to_string(), "Conventional Commits"),
        ("fix:  x\n".to_string(), "Conventional Commits"),
        // Nur ein Formatzeichen (Zero Width Space) hinter dem Doppelpunkt: kein sichtbarer Text.
        ("feat: \u{200b}\n".to_string(), "Conventional Commits"),
        ("Merge pull request #1\n".to_string(), "Conventional Commits"),
        // Keine Ausnahme mehr für die Standardmeldungen von git.
        ("Merge branch 'zweig'\n".to_string(), "Conventional Commits"),
        ("Merge commit 'abc123'\n".to_string(), "Conventional Commits"),
        ("# fix: nur Kommentar\n".to_string(), "Conventional Commits"),
        ("fix: x\n\nReviewed-by: Jemand <jemand@example.org>\n".to_string(), "Anhang (Trailer)"),
        ("fix: x\n\nCo-Authored-By: Jemand <jemand@example.org>\n".to_string(), "Anhang (Trailer)"),
        (format!("fix: mit {ki} geschrieben\n"), "KI-Name"),
        (format!("fix: a\n\nmit {}-\n{} geschrieben\n", "Son", "net"), "KI-Name"),
        ("feat: x\n\nBREAKING CHANGE: alte Form weg\n".to_string(), "Anhang (Trailer)"),
    ] {
        let (code, text) = nachricht(&schlecht);
        assert_eq!(code, 1, "{schlecht:?}: {text}");
        assert!(text.contains(satz) && text.contains("Beispiel: fix(web)"), "{schlecht:?}: erwartet „{satz}“ und ein Beispiel:\n{text}");
    }
    // #-Zeilen vor oder zwischen dem Text meldet er; die von git dahinter und ab der Schere nicht.
    for (text, hinweis) in [
        ("# x\nfix: a\n".to_string(), true),
        ("\n\n# x\nfix: a\n".to_string(), true),
        ("fix: a\n\n# x\nmehr Text\n".to_string(), true),
        ("fix: a\n\n# Bitte Nachricht eingeben\n# x\n".to_string(), false),
        (format!("fix: a\n\nText\n{schere}+x\n"), false),
    ] {
        let (code, aus) = nachricht(&text);
        assert_eq!(code, 0, "{text:?}: {aus}");
        assert_eq!(aus.contains("HINWEIS Kommentarzeilen (#) entfernt."), hinweis, "{text:?}: {aus}");
    }

    // Ein echter git commit: Der Hook bekommt die Nachricht vor dem Aufräumen (Kommentare noch drin).
    let skripte = o.repo().join("scripts");
    std::fs::create_dir_all(skripte.join("hooks")).unwrap();
    std::fs::copy(skript(), skripte.join("Vor-Push.ps1")).unwrap();
    std::fs::copy(skript().with_file_name("hooks").join("commit-msg"), skripte.join("hooks").join("commit-msg")).unwrap();
    o.git(&["config", "core.hooksPath", "scripts/hooks"]);
    let datei = o.wurzel.join("nachricht.txt").to_string_lossy().to_string();
    let committen = |text: &[u8], args: &[&str]| {
        std::fs::write(&datei, text).unwrap();
        let mut c = git();
        c.arg("-C").arg(o.repo()).args(["commit", "-q", "--allow-empty"]).args(args);
        for (k, v) in [("GIT_AUTHOR_NAME", "Ranke"), ("GIT_AUTHOR_EMAIL", MAIL), ("GIT_COMMITTER_NAME", "Ranke"), ("GIT_COMMITTER_EMAIL", MAIL)] {
            c.env(k, v);
        }
        let aus = c.output().unwrap();
        (aus.status.success(), format!("{}{}", String::from_utf8_lossy(&aus.stdout), String::from_utf8_lossy(&aus.stderr)))
    };
    let mit_editor = ["--cleanup=strip", "-F", &datei];
    let (ok, text) = committen(b"falsch\n", &mit_editor);
    assert!(!ok && text.contains("Conventional Commits"), "{text}");
    let (ok, text) = committen(format!("# {ki}\nfix: Hook geht\n").as_bytes(), &mit_editor);
    assert!(ok, "{text}");
    assert_eq!(o.git(&["log", "-1", "--format=%B"]), "fix: Hook geht");
    // Bei -m und -F ohne Editor lässt git #-Zeilen und Schere stehen: Der Hook schreibt
    // sie heraus, gespeichert wird genau, was er geprüft hat. Ein BOM fällt ebenso weg.
    let kopf = format!("# {ki}");
    let (ok, text) = committen(b"", &["-m", &kopf, "-m", "feat: y"]);
    assert!(ok && text.contains("HINWEIS Kommentarzeilen"), "{text}");
    assert_eq!(o.git(&["log", "-1", "--format=%B"]), "feat: y");
    let (ok, text) = committen(format!("feat: z\n{schere}+{ki}\n").as_bytes(), &["-F", &datei]);
    assert!(ok, "{text}");
    assert_eq!(o.git(&["log", "-1", "--format=%B"]), "feat: z");
    let (ok, text) = committen(b"\xEF\xBB\xBFfeat: a\n", &["-F", &datei]);
    assert!(ok, "{text}");
    assert_eq!(o.git(&["log", "-1", "--format=%B"]), "feat: a");
}

/// Die erste Zeile prüft auch der Push, nicht nur der Hook (Commit ohne Hook,
/// cherry-pick, rebase, Klon ohne hooksPath), Merges eingeschlossen.
#[test]
fn push_prueft_erste_zeile() {
    if !bereit() {
        return;
    }
    let ranke = ["-c", "user.name=Ranke", "-c", "user.email=ranke2@proton.me"];
    abgewiesen("Commit ohne Hook", "erste Zeile nicht nach Conventional Commits", |o| {
        o.git(&[&ranke[..], &["commit", "-q", "--allow-empty", "-m", "falsche Form"]].concat());
    });
    abgewiesen("Merge mit Standardmeldung", "erste Zeile nicht nach Conventional Commits", |o| {
        o.git(&["checkout", "-q", "-b", "zweig"]);
        o.commit("fix: auf dem Zweig", &[]);
        o.git(&["checkout", "-q", "main"]);
        o.git(&[&ranke[..], &["merge", "-q", "--no-ff", "--no-edit", "zweig"]].concat());
        assert!(o.git(&["log", "-1", "--format=%s"]).starts_with("Merge branch"));
    });
}

/// `Hochladen.ps1 -Wurzel` prüft die Nachricht vor `update-ref`: Eine falsche
/// Form lässt den zuletzt gebauten Wurzel-Commit auf `oeffentlich` stehen.
#[test]
fn hochladen_wurzel_prueft_nachricht_vorher() {
    if !bereit() {
        return;
    }
    let o = Ort::neu();
    // Die Skripte liegen unversioniert im Repo (Hochladen nimmt den Elternordner als Repo); main bleibt sauber.
    let skripte = o.repo().join("scripts");
    std::fs::create_dir_all(&skripte).unwrap();
    for name in ["Vor-Push.ps1", "Hochladen.ps1"] {
        std::fs::copy(skript().with_file_name(name), skripte.join(name)).unwrap();
    }
    o.git(&["config", "core.hooksPath", "scripts/hooks"]);
    let inhalt = o.wurzel.join("inhalt").to_string_lossy().to_string();
    let wurzel = |nachricht: &str| {
        let aus = pwsh()
            .unwrap()
            .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"])
            .arg(skripte.join("Hochladen.ps1"))
            .args(["-Wurzel", "-Inhalt", &inhalt, "-Datum", "2026-11-05T12:00:00+01:00", "-Nachricht", nachricht])
            .output()
            .unwrap();
        (aus.status.code().unwrap_or(-1), format!("{}{}", String::from_utf8_lossy(&aus.stdout), String::from_utf8_lossy(&aus.stderr)))
    };
    let (code, text) = wurzel("feat: gut");
    assert_eq!(code, 0, "{text}");
    let gut = o.git(&["rev-parse", "refs/heads/oeffentlich"]);
    assert_eq!(o.git(&["log", "-1", "--format=%B", &gut]), "feat: gut");
    // Leerraum davor räumt -Nachricht wie der Hook weg (" feat: x" geht darum durch).
    for schlecht in ["falsche Form", "Merge branch 'x'", "# feat: nur Kommentar", "feat: \u{200b}"] {
        let (code, text) = wurzel(schlecht);
        assert_eq!(code, 1, "{schlecht:?}: {text}");
        assert!(text.contains("Conventional Commits"), "{schlecht:?}: {text}");
        assert_eq!(o.git(&["rev-parse", "refs/heads/oeffentlich"]), gut, "{schlecht:?}: oeffentlich verschoben");
    }
}

/// Die Anleitung verlangt keinen Anhang mehr in Merge-Nachrichten: Der Hook
/// commit-msg läuft auch bei git merge und wiese ihn ab.
#[test]
fn anleitung_ohne_anhang_in_nachrichten() {
    let ordner = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docs").join("anleitung");
    for e in std::fs::read_dir(&ordner).unwrap() {
        let p = e.unwrap().path();
        let text = std::fs::read_to_string(&p).unwrap().to_lowercase();
        assert!(!text.contains("mit der zeile co-authored-by"), "{}: verlangt einen Anhang", p.display());
    }
}

/// Overlay (Projekt-Repo ohne scripts/): core.hooksPath zeigt absolut auf die
/// Hooks des Code-Repos; commit-msg und pre-push finden den Prüfer neben sich
/// und prüfen das Overlay. `Hochladen.ps1 -Repo` lädt dort main nach main, mit
/// der Adresse des Remotes (Art aus repos.toml), ohne -Wurzel. Das Remote ist
/// ein Bare-Repo im Temp-Ordner.
#[test]
fn overlay_hooks_und_hochladen() {
    if !bereit() {
        return;
    }
    let o = Ort::projekt();
    let inhalt = o.wurzel.join("inhalt");
    // Das Code-Repo: nur scripts/ mit Prüfer, Hochladen und den beiden Hooks.
    let code = o.wurzel.join("code");
    let hooks = code.join("scripts").join("hooks");
    std::fs::create_dir_all(&hooks).unwrap();
    for name in ["Vor-Push.ps1", "Hochladen.ps1"] {
        std::fs::copy(skript().with_file_name(name), code.join("scripts").join(name)).unwrap();
    }
    for name in ["pre-push", "commit-msg"] {
        std::fs::copy(skript().with_file_name("hooks").join(name), hooks.join(name)).unwrap();
    }
    let fern = o.wurzel.join("fern.git");
    let fern_adresse = fern.to_string_lossy().replace('\\', "/");
    assert!(git().args(["init", "-q", "--bare"]).arg(&fern).output().unwrap().status.success());
    o.git(&["remote", "add", "origin", &fern_adresse]);
    o.repos(Some(&format!("{REPOS}\"{fern_adresse}\" = \"Projekt\"\n")));
    o.git(&["config", "core.hooksPath", &hooks.to_string_lossy().replace('\\', "/")]);
    let fern_main = || {
        let a = git().arg("-C").arg(&fern).args(["rev-parse", "--verify", "--quiet", "refs/heads/main"]).output().unwrap();
        String::from_utf8_lossy(&a.stdout).trim().to_string()
    };
    let mit_git = |args: &[&str]| {
        let mut c = git();
        c.arg("-C").arg(o.repo()).args(args).env("DASHBOARD_INHALT", &inhalt);
        for (k, v) in [("GIT_AUTHOR_NAME", "Ranke"), ("GIT_AUTHOR_EMAIL", MAIL), ("GIT_COMMITTER_NAME", "Ranke"), ("GIT_COMMITTER_EMAIL", MAIL)] {
            c.env(k, v);
        }
        let a = c.output().unwrap();
        (a.status.success(), format!("{}{}", String::from_utf8_lossy(&a.stdout), String::from_utf8_lossy(&a.stderr)))
    };
    let hochladen = |args: &[&str]| {
        let a = pwsh()
            .unwrap()
            .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"])
            .arg(code.join("scripts").join("Hochladen.ps1"))
            .arg("-Repo")
            .arg(o.repo())
            .arg("-Inhalt")
            .arg(&inhalt)
            .args(args)
            .env("DASHBOARD_INHALT", &inhalt)
            .output()
            .unwrap();
        (a.status.code().unwrap_or(-1), format!("{}{}", String::from_utf8_lossy(&a.stdout), String::from_utf8_lossy(&a.stderr)))
    };

    // commit-msg aus dem Code-Repo prüft die Nachricht im Overlay.
    let (ok, text) = mit_git(&["commit", "-q", "--allow-empty", "-m", "falsche Form"]);
    assert!(!ok && text.contains("Conventional Commits"), "{text}");

    // Hochladen -Repo: erst prüfen, dann main nach main (der Hook prüft dabei noch einmal).
    let (code_p, text) = hochladen(&["-Pruefen"]);
    assert_eq!(code_p, 0, "{text}");
    assert!(text.contains("Geprüft als Projekt-Repo") && text.contains("main ginge so nach main"), "{text}");
    assert!(fern_main().is_empty(), "-Pruefen lädt nicht hoch");
    let (code_h, text) = hochladen(&[]);
    assert_eq!(code_h, 0, "{text}");
    assert_eq!(fern_main(), o.git(&["rev-parse", "main"]), "{text}");

    // Gepusht wird an pushurl; geprüft wird genau diese Adresse.
    // Eine pushurl, die nicht in repos.toml steht, prüft er als Basis (das Overlay fällt durch).
    let anders = o.wurzel.join("anders.git").to_string_lossy().replace('\\', "/");
    o.git(&["remote", "set-url", "--push", "origin", &anders]);
    let (c, text) = hochladen(&["-Pruefen"]);
    assert_eq!(c, 1, "pushurl außerhalb von repos.toml: {text}");
    assert!(text.contains("Geprüft als Basis-Repo"), "{text}");
    // Zwei verschiedene Push-Adressen: Abbruch mit Satz, ohne Prüfung.
    o.git(&["remote", "set-url", "--push", "--add", "origin", &fern_adresse]);
    let (c, text) = hochladen(&["-Pruefen"]);
    assert_eq!(c, 1, "{text}");
    assert!(text.contains("mehrere verschiedene Push-Adressen") && !text.contains("Geprüft als"), "{text}");
    o.git(&["config", "--unset-all", "remote.origin.pushurl"]);

    // pre-push aus dem Code-Repo: ein sauberer Merge geht, ein Ort außerhalb nicht.
    o.ueber_merge(|o| o.datei("projects/alpha/neu.toml", b"x = 1\n"));
    let (ok, text) = mit_git(&["push", "origin", "main:main"]);
    assert!(ok, "{text}");
    assert!(text.contains("Geprüft als Projekt-Repo"), "{text}");
    let sauber = fern_main();
    o.ueber_merge(|o| o.datei("src/a.rs", b"fn a() {}\n"));
    let fund = "ABGEWIESEN src/a.rs: Ordner oder Datei nicht in der Positivliste des Projekt-Repos";
    let (ok, text) = mit_git(&["push", "origin", "main:main"]);
    assert!(!ok && text.contains(fund), "{text}");
    for args in [&["-Pruefen"][..], &[][..]] {
        let (c, text) = hochladen(args);
        assert_eq!(c, 1, "{args:?}: {text}");
        assert!(text.contains(fund), "{args:?}: {text}");
    }
    assert_eq!(fern_main(), sauber, "nichts ging hinaus");

    // Kein -Wurzel im Projekt-Repo; ohne core.hooksPath auf das Code-Repo bricht es ab.
    let (c, text) = hochladen(&["-Wurzel"]);
    assert_eq!(c, 1, "{text}");
    assert!(text.contains("-Wurzel gibt es nur im Code-Repo"), "{text}");
    for haken in ["scripts/hooks", "C:/anders/scripts/hooks"] {
        o.git(&["config", "core.hooksPath", haken]);
        let (c, text) = hochladen(&["-Pruefen"]);
        assert_eq!(c, 1, "{haken}: {text}");
        assert!(text.contains("core.hooksPath dieses Repos zeigt nicht auf die Hooks des Code-Repos"), "{haken}: {text}");
    }
    // Ohne Remote prüfte er als Basis: darum auch -Pruefen nicht ohne Remote.
    o.git(&["config", "core.hooksPath", &hooks.to_string_lossy().replace('\\', "/")]);
    o.git(&["remote", "remove", "origin"]);
    let (c, text) = hochladen(&["-Pruefen"]);
    assert_eq!(c, 1, "{text}");
    assert!(text.contains("Das Projekt-Repo hat kein Remote origin"), "{text}");
}
