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

/// PowerShell 7 an festem Ort, ohne Fenster (der Prüfweg verlangt 7.7, kein Rückfall auf Windows PowerShell).
fn pwsh() -> Option<Command> {
    programme::befehl(Werkzeug::PowerShell7).ok()
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
        self.lauf_mit(&skript(), &self.cache(), args, eingabe)
    }

    /// Der eigene Cache-Ordner dieses Orts (DASHBOARD_PRUEFER_CACHE).
    fn cache(&self) -> PathBuf {
        self.wurzel.join("cache")
    }

    /// Wie `pruefen`, mit dem Cache sauberer Blobs (wie die Vorprüfung in Hochladen.ps1).
    fn mit_cache(&self) -> (i32, String) {
        self.lauf(&["-Baum", "main", "-Commits", "main", "-AufMain", "-MitCache"], None)
    }

    /// Ein Lauf mit diesem Prüfer und diesem Cache-Ort.
    fn lauf_mit(&self, skript: &Path, cache: &Path, args: &[&str], eingabe: Option<&str>) -> (i32, String) {
        let mut c = pwsh().unwrap();
        c.env("DASHBOARD_PRUEFER_CACHE", cache)
            .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"])
            .arg(skript)
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

/// LICENSE (AGPL), LICENSES/CC0-1.0.txt und LICENSES/GPL-3.0-or-later.txt gehen mit ihrem
/// echten Text durch; jede andere Datei in LICENSES/ steht nicht in der Positivliste.
/// Ebenso die schon committeten Wurzeldateien rustfmt.toml, .git-blame-ignore-revs und
/// CONTRIBUTING.md (die einzige erlaubte *.md, mit ihrem echten Text) und der öffentliche
/// Schlüssel KONTAKT-PGP.asc (nur in der Wurzel; ein privater Block ist überall ein Fund),
/// dazu das Issue-Formular für Merge-Angebote mit seinem echten Text.
#[test]
fn lizenzdateien_gehen_durch() {
    if !bereit() {
        return;
    }
    let wurzel = Path::new(env!("CARGO_MANIFEST_DIR"));
    let o = Ort::neu();
    o.ueber_merge(|o| {
        for pfad in [
            "LICENSE",
            "LICENSES/CC0-1.0.txt",
            "LICENSES/GPL-3.0-or-later.txt",
            "rustfmt.toml",
            ".git-blame-ignore-revs",
            "CONTRIBUTING.md",
            "KONTAKT-PGP.asc",
            ".github/ISSUE_TEMPLATE/merge-angebot.yml",
        ] {
            o.datei(pfad, &std::fs::read(wurzel.join(pfad)).unwrap());
        }
    });
    let (code, text) = o.pruefen();
    assert_eq!(code, 0, "{text}");
    // Das Formular verlangt das Kontrollkästchen zu den Regeln (required: true je Option).
    let formular = std::fs::read_to_string(wurzel.join(".github/ISSUE_TEMPLATE/merge-angebot.yml")).unwrap();
    // Je Option die nächste Zeile, so zählt kein required eines anderen Feldes mit.
    let zeilen: Vec<&str> = formular.lines().map(str::trim).collect();
    let optionen: Vec<usize> = (0..zeilen.len()).filter(|&i| zeilen[i].starts_with("- label:")).collect();
    assert!(!optionen.is_empty(), "Formular ohne Kontrollkästchen");
    for i in optionen {
        assert_eq!(zeilen.get(i + 1), Some(&"required: true"), "Option ohne required: true: {}", zeilen[i]);
    }
    for pfad in ["LICENSES/MIT.txt", "LICENSES/CC0-1.0.txt.bak", "LICENSES/GPL-3.0-only.txt", "docs/LICENSE", "rustfmt.toml.alt"] {
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
/// absteigend, die oberste genau die Cargo-Version. Das Changelog liest der
/// Leser des Dienstes (`dashboard::fassungen::lesen`), jeder Hinweis ist ein Fehler.
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

    // Derselbe Leser wie im Dienst: Unreleased zuerst, Form der Überschriften,
    // absteigend, nur die vier Unterabschnitte; jeder Hinweis ist ein Fehler.
    let a = dashboard::fassungen::lesen(&lesen("CHANGELOG.txt"), cargo);
    fehler.extend(a.hinweise.iter().map(|h| format!("CHANGELOG.txt: {h}")));
    if let Some(oben) = a.fassungen.first().filter(|f| f.version != cargo) {
        fehler.push(format!("CHANGELOG.txt: oberste Fassung {}, Cargo.toml hat {cargo}; Fassung und Version im selben Commit setzen", oben.version));
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
        // macOS und Linux, auch als URL und kodiert.
        "/U§sers/realname/dev",
        "/h§ome/realname/.ssh",
        "file:///U§sers/realname/dev",
        "file:///h§ome/realname",
        "%2FU§sers%2Frealname",
        "%2Fh§ome%2Frealname",
        "\"/U§sers/REALNAME\"",
    ]
    .iter()
    .enumerate()
    {
        abgewiesen(&format!("Benutzerpfad {i}"), "Benutzerpfad", |o| {
            o.datei("src/pfad.rs", format!("// {form}\n").as_bytes());
            o.commit("Pfad", &[]);
        });
    }
    // Platzhalter gehen durch wie bei Windows; Systemorte ohne Heimatordner auch.
    let o = Ort::neu();
    o.ueber_merge(|o| {
        o.datei("src/posix.rs", "// /U§sers/<name>/dev, /h§ome/jemand, /h§ome/$USER, file:///U§sers/name, /usr/bin/git, /var/lib/x\n".as_bytes())
    });
    let (code, text) = o.pruefen();
    assert_eq!(code, 0, "Platzhalter in POSIX-Form: {text}");
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
    // Auch beim ersten Push (ganze Geschichte) nur eine Wurzel.
    abgewiesen("zweite Wurzel beim ersten Push", "Commit ohne Eltern", |o| {
        o.git(&["checkout", "-q", "--orphan", "fremd"]);
        o.datei("src/x.rs", b"// x\n");
        o.commit("fremde Wurzel", &[]);
        o.git(&["checkout", "-q", "main"]);
        let ranke = ["-c", "user.name=Ranke", "-c", "user.email=ranke2@proton.me"];
        o.git(&[&ranke[..], &["merge", "-q", "--no-ff", "--allow-unrelated-histories", "fremd", "-m", "chore(merge): fremd"]].concat());
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
        // Die Ausnahme gilt nur für den Leitfaden in der Wurzel.
        ("docs/CONTRIBUTING.md", "Markdown-Datei"),
        ("web/CONTRIBUTING.md", "Markdown-Datei"),
        ("CONTRIBUTING.md.alt", "Positivliste"),
        ("docs/KONTAKT-PGP.asc", "Positivliste"),
    ] {
        abgewiesen(pfad, satz, |o| {
            o.datei(pfad, b"x\n");
            o.commit("x", &[]);
        });
    }
    // Die Blöcke werden zusammengesetzt, damit diese Datei selbst keinen enthält.
    let strich = "-".repeat(5);
    for (pfad, art) in [("KONTAKT-PGP.asc", "PGP PRIVATE KEY BLOCK"), ("tests/daten/id.txt", "OPENSSH PRIVATE KEY"), ("src/a.rs", "PRIVATE KEY")] {
        let block = format!("{strich}BEGIN {art}{strich}\nabc\n{strich}END {art}{strich}\n");
        abgewiesen(pfad, "privater Schlüssel", |o| {
            o.datei(pfad, block.as_bytes());
            o.commit("x", &[]);
        });
    }
    let putty = format!("PuTTY-User-Key-{}-3: ssh-ed25519\n", "File");
    abgewiesen("PuTTY-Schlüssel", "privater Schlüssel", |o| {
        o.datei("tests/daten/id.ppk.txt", putty.as_bytes());
        o.commit("x", &[]);
    });
    // Nur in einem Zwischencommit, an der Spitze wieder entfernt: die Verlaufsprüfung findet ihn.
    let zwischen = format!("{strich}BEGIN RSA PRIVATE KEY{strich}\nabc\n");
    abgewiesen("privater Block im Zwischencommit", "privater Schlüssel", |o| {
        o.ueber_merge(|o| {
            o.datei("tests/daten/k.txt", zwischen.as_bytes());
            o.commit("x", &[]);
            o.datei("tests/daten/k.txt", b"leer\n");
        });
    });
    let nachricht = format!("fix: x\n\n{strich}BEGIN OPENSSH PRIVATE KEY{strich}\n");
    abgewiesen("privater Block in der Nachricht", "privater Schlüssel", |o| {
        o.datei("src/b.rs", b"// x\n");
        o.commit(&nachricht, &[]);
    });
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
/// einen Slug und dashboard-version.toml, .gitignore, .gitattributes und der Leitfaden
/// CONTRIBUTING.md in der Wurzel; alles andere ist ein Fund, auch nur in der Geschichte.
/// Jede andere *.md auch hier nicht.
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
        ("Leitfaden im eigenen Ordner", "projects/alpha/CONTRIBUTING.md", "Markdown-Datei (*.md)"),
    ] {
        let o = Ort::projekt();
        o.ueber_merge(|o| o.datei(pfad, b"x = 1\n"));
        for (wie, (code, text)) in [("Hochladen", o.pruefen_mit(Some(PROJEKT_ADRESSE))), ("Hook", o.hook_mit(PROJEKT_ADRESSE))] {
            assert_eq!(code, 1, "{name} ({wie}): {text}");
            assert!(text.contains(&format!("ABGEWIESEN {pfad}: {muster}")), "{name} ({wie}): {text}");
            assert_eq!(fundzeilen(&text).len(), 1, "{name} ({wie}): genau ein Fund:\n{text}");
        }
    }
    // In einem anderen Ordner ist er außerhalb der Positivliste und Markdown: zwei Funde.
    let o = Ort::projekt();
    o.ueber_merge(|o| o.datei("docs/CONTRIBUTING.md", b"x\n"));
    let (code, text) = o.pruefen_mit(Some(PROJEKT_ADRESSE));
    assert_eq!(code, 1, "{text}");
    assert!(text.contains(&format!("ABGEWIESEN docs/CONTRIBUTING.md: {ausserhalb}")) && text.contains("ABGEWIESEN docs/CONTRIBUTING.md: Markdown"), "{text}");
    // Der Leitfaden in der Wurzel geht durch, beim Hochladen und im Hook.
    let o = Ort::projekt();
    o.ueber_merge(|o| o.datei("CONTRIBUTING.md", "# Mitwirken\n\nBeiträge als Issue.\n".as_bytes()));
    for (wie, (code, text)) in [("Hochladen", o.pruefen_mit(Some(PROJEKT_ADRESSE))), ("Hook", o.hook_mit(PROJEKT_ADRESSE))] {
        assert_eq!(code, 0, "Leitfaden ({wie}): {text}");
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

/// Kopiert den Prüfer und die Datei, die er lädt (Laufzeit.ps1), in diesen Ordner.
fn prueferdateien_kopieren(nach: &Path) {
    std::fs::create_dir_all(nach).unwrap();
    for name in ["Vor-Push.ps1", "Laufzeit.ps1"] {
        std::fs::copy(skript().with_file_name(name), nach.join(name)).unwrap();
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
    prueferdateien_kopieren(&code.join("scripts"));
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

/// `-Pruefen` prüft ohne -Ref refs/heads/main wie einen ersten Push nach main;
/// main mit Merge-Commits geht durch (-Pruefen, Hook, -Bundle), eine
/// Geschichte mit breitem ＠ und einem Zwischenstand nicht. Nur der lokale
/// Zweig main geht hinaus; im Bundle zählt jede Ref.
#[test]
fn pruefen_und_bundle_nur_main() {
    if !bereit() {
        return;
    }
    let o = Ort::neu();
    o.ueber_merge(|o| o.datei("src/b.rs", b"// b\n"));
    let sha = o.git(&["rev-parse", "main"]);
    let (code, text) = o.lauf(&["-Pruefen"], None);
    assert_eq!(code, 0, "{text}");
    assert!(text.contains("Probelauf") && text.contains("refs/heads/main"), "{text}");
    let (code, text) = o.lauf(&["-Hook"], Some(&format!("refs/heads/main {sha} refs/heads/main {NULL}\n")));
    assert_eq!(code, 0, "Hook: {text}");
    // Gegenprobe: ein anderer lokaler Zweig auf demselben Stand geht nicht als main hinaus.
    o.git(&["branch", "oeffentlich", "main"]);
    let (code, text) = o.lauf(&["-Hook"], Some(&format!("refs/heads/oeffentlich {sha} refs/heads/main {NULL}\n")));
    assert!(code == 1 && text.contains("nur der lokale Zweig main geht hinaus"), "Hook: {text}");
    let (code, text) = o.lauf(&["-Pruefen", "-Ref", "oeffentlich"], None);
    assert!(code == 1 && text.contains("nur der lokale Zweig main geht hinaus"), "-Pruefen: {text}");
    let (code, _) = o.lauf(&["-Pruefen", "-Ref", "gibt-es-nicht"], None);
    assert_eq!(code, 2);

    // Eine Geschichte mit breitem ＠ und einem Zwischenstand: in jedem geprüften Bereich gefunden.
    let alt = Ort::neu();
    alt.git(&["checkout", "-q", "-b", "zweig"]);
    alt.datei("src/kurz.rs", b"// geheim\xc2\xa7projekt\n");
    alt.commit("kurz da", &[("GIT_AUTHOR_EMAIL", "ranke2\u{FF20}proton.me")]);
    std::fs::remove_file(alt.repo().join("src/kurz.rs")).unwrap();
    alt.commit("wieder weg", &[]);
    zusammenfuehren(&alt);
    let (code, text) = alt.lauf(&["-Pruefen"], None);
    assert_eq!(code, 1, "{text}");
    assert!(text.contains("Autor ist nicht Ranke <ranke2@proton.me>") && text.contains("src/kurz.rs"), "{text}");
    assert!(!text.to_lowercase().contains("geheimprojekt"), "{text}");

    o.git(&["tag", "v1", "main"]);
    let bundle = |ort: &Ort, name: &str, refs: &[&str]| -> String {
        let p = o.wurzel.join(name).to_string_lossy().to_string();
        let mut a = vec!["bundle", "create", "-q", p.as_str()];
        a.extend_from_slice(refs);
        ort.git(&a);
        p
    };
    for (ort, name, refs, code_soll, satz) in [
        (&o, "gut", vec!["refs/heads/main"], 0, "Nichts gefunden."),
        (&alt, "alt", vec!["refs/heads/main"], 1, "Autor ist nicht Ranke"),
        (&o, "mit Tag", vec!["refs/heads/main", "refs/tags/v1"], 1, "nur refs/heads/main oder ein Tag refs/tags/v0.MINOR.PATCH"),
        (&o, "anderer Zweig", vec!["refs/heads/zweig"], 1, "nur der lokale Zweig main geht hinaus"),
        (&o, "oeffentlich", vec!["refs/heads/oeffentlich"], 1, "nur der lokale Zweig main geht hinaus"),
    ] {
        let p = bundle(ort, &format!("{name}.bundle"), &refs);
        let (code, text) = o.lauf(&["-Bundle", &p], None);
        assert_eq!(code, code_soll, "{name}: {text}");
        assert!(text.contains(satz), "{name}: {text}");
    }
    let p = bundle(&alt, "alt2.bundle", &["refs/heads/main"]);
    let (_, text) = o.lauf(&["-Bundle", &p], None);
    assert!(text.contains("src/kurz.rs") && !text.to_lowercase().contains("geheimprojekt"), "{text}");
    // Ein Bundle mit Voraussetzungen (nicht ab der Wurzel) scheitert geschlossen.
    let teil = bundle(&o, "teil.bundle", &["main^..main"]);
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
        // Nur main zählt als Linie für Tags; ein anderer Zweig ist selbst ein Fund und trägt keinen Tag.
        ("oeffentlich", vec!["refs/heads/oeffentlich", "refs/tags/v0.1.0"], 1, "Tag ohne main"),
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
    prueferdateien_kopieren(&skripte);
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

/// `Hochladen.ps1` im Code-Repo lädt refs/heads/main nach refs/heads/main,
/// nur als Fast-Forward und nur mit Merge-Commits auf der First-Parent-Linie;
/// -Pruefen lädt nicht hoch, -Wurzel bricht ab, -Tag nur auf dieser Linie.
/// Das Remote ist ein Bare-Repo im Temp-Ordner.
#[test]
fn hochladen_main_nach_main() {
    if !bereit() {
        return;
    }
    let o = Ort::neu();
    let inhalt = o.wurzel.join("inhalt");
    // Skripte und Hooks liegen unversioniert im Repo (Hochladen nimmt den Elternordner als Repo); ausgeschlossen, damit main sauber bleibt.
    let skripte = o.repo().join("scripts");
    std::fs::create_dir_all(skripte.join("hooks")).unwrap();
    for name in ["Vor-Push.ps1", "Hochladen.ps1", "Laufzeit.ps1"] {
        std::fs::copy(skript().with_file_name(name), skripte.join(name)).unwrap();
    }
    for name in ["pre-push", "commit-msg"] {
        std::fs::copy(skript().with_file_name("hooks").join(name), skripte.join("hooks").join(name)).unwrap();
    }
    std::fs::write(o.repo().join(".git").join("info").join("exclude"), "/scripts/\n").unwrap();
    o.git(&["config", "core.hooksPath", "scripts/hooks"]);
    let fern = o.wurzel.join("fern.git");
    assert!(git().args(["init", "-q", "--bare"]).arg(&fern).output().unwrap().status.success());
    let fern_ref = |r: &str| {
        let a = git().arg("-C").arg(&fern).args(["rev-parse", "--verify", "--quiet", r]).output().unwrap();
        String::from_utf8_lossy(&a.stdout).trim().to_string()
    };
    let hochladen = |args: &[&str]| {
        let a = pwsh()
            .unwrap()
            .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"])
            .arg(skripte.join("Hochladen.ps1"))
            .arg("-Inhalt")
            .arg(&inhalt)
            .args(args)
            .env("DASHBOARD_INHALT", &inhalt)
            .env("DASHBOARD_PRUEFER_CACHE", o.cache())
            .output()
            .unwrap();
        (a.status.code().unwrap_or(-1), format!("{}{}", String::from_utf8_lossy(&a.stdout), String::from_utf8_lossy(&a.stderr)))
    };

    // -Wurzel gibt es nicht mehr: Abbruch mit Satz, auch zusammen mit -Pruefen, ohne Prüfung.
    for args in [&["-Wurzel"][..], &["-Wurzel", "-Pruefen"][..]] {
        let (c, text) = hochladen(args);
        assert_eq!(c, 1, "{args:?}: {text}");
        assert!(text.contains("-Wurzel gibt es nicht mehr") && !text.contains("Geprüft als"), "{args:?}: {text}");
    }
    // Ohne Remote: -Pruefen prüft (als Basis), Hochladen bricht ab.
    let (c, text) = hochladen(&["-Pruefen"]);
    // Die Vorprüfung nutzt den Cache (-MitCache), und beide Skripte nennen Laufzeit und Auslastung.
    assert!(c == 0 && text.contains("main ginge so nach main") && text.contains("Blobs aus dem Cache"), "{text}");
    assert!(text.matches("Laufzeit ").count() >= 2, "{text}");
    let (c, text) = hochladen(&[]);
    assert!(c == 1 && text.contains("Es gibt kein Remote origin"), "{text}");
    o.git(&["remote", "add", "origin", &fern.to_string_lossy().replace('\\', "/")]);

    // Erster Push: -Pruefen lädt nicht hoch, dann main nach main (der Hook prüft dabei noch einmal).
    o.ueber_merge(|o| o.datei("src/b.rs", b"// b\n"));
    o.git(&["branch", "oeffentlich", "main"]);
    let (c, text) = hochladen(&["-Pruefen"]);
    assert!(c == 0 && text.contains("Geprüft als Basis-Repo") && text.contains("main ginge so nach main"), "{text}");
    assert!(fern_ref("refs/heads/main").is_empty(), "-Pruefen lädt nicht hoch");
    let (c, text) = hochladen(&[]);
    assert_eq!(c, 0, "{text}");
    assert_eq!(fern_ref("refs/heads/main"), o.git(&["rev-parse", "main"]), "{text}");
    // Die Vorprüfung nimmt den Cache, der Hook beim Push prüft kalt.
    assert!(text.contains("Blobs aus dem Cache") && text.contains("Inhalt kalt:"), "{text}");
    // Fast-Forward mit einem weiteren Merge-Commit.
    o.ueber_merge(|o| o.datei("src/c.rs", b"// c\n"));
    let (c, text) = hochladen(&[]);
    assert_eq!(c, 0, "{text}");
    let stand = o.git(&["rev-parse", "main"]);
    assert_eq!(fern_ref("refs/heads/main"), stand, "{text}");
    // Nur main ging hinaus, kein anderer Zweig.
    let a = git().arg("-C").arg(&fern).args(["for-each-ref", "--format=%(refname)"]).output().unwrap();
    assert_eq!(String::from_utf8_lossy(&a.stdout).trim(), "refs/heads/main");

    // Gegenprobe: ein Commit ohne Merge auf der First-Parent-Linie wird abgewiesen, geprüft und mit Push.
    o.datei("src/d.rs", b"// d\n");
    o.commit("ohne Merge", &[]);
    for args in [&["-Pruefen"][..], &[][..]] {
        let (c, text) = hochladen(args);
        assert_eq!(c, 1, "{args:?}: {text}");
        assert!(text.contains("nur Merge-Commits"), "{args:?}: {text}");
    }
    assert_eq!(fern_ref("refs/heads/main"), stand, "nichts ging hinaus");
    // Gegenprobe: main ohne den fernen Stand (kein Fast-Forward) bricht vor der Prüfung ab.
    o.git(&["reset", "-q", "--hard", "main~2"]);
    o.ueber_merge(|o| o.datei("src/e.rs", b"// e\n"));
    for args in [&["-Pruefen"][..], &[][..]] {
        let (c, text) = hochladen(args);
        assert_eq!(c, 1, "{args:?}: {text}");
        assert!(text.contains("kein Fast-Forward") && !text.contains("Geprüft als"), "{args:?}: {text}");
    }
    assert_eq!(fern_ref("refs/heads/main"), stand, "nichts ging hinaus");
    o.git(&["reset", "-q", "--hard", &stand]);

    // Gegenprobe: eine zweite Wurzel auf main wird abgewiesen, geprüft und mit Push (beides Fast-Forward).
    let ranke = ["-c", "user.name=Ranke", "-c", "user.email=ranke2@proton.me"];
    let zweite_wurzel = |weg: &str| {
        for args in [&["-Pruefen"][..], &[][..]] {
            let (c, text) = hochladen(args);
            assert_eq!(c, 1, "{weg} {args:?}: {text}");
            assert!(text.contains("Commit ohne Eltern: eine zweite Wurzel auf main"), "{weg} {args:?}: {text}");
        }
        assert_eq!(fern_ref("refs/heads/main"), stand, "{weg}: nichts ging hinaus");
    };
    // (a) Neuer Wurzel-Commit als erster Elternteil eines Merges mit dem fernen Stand.
    o.git(&["checkout", "-q", "--orphan", "neu"]);
    o.datei("src/w.rs", b"// w\n");
    o.commit("neue Wurzel", &[]);
    o.git(&[&ranke[..], &["merge", "-q", "--no-ff", "--allow-unrelated-histories", "main", "-m", "chore(merge): main"]].concat());
    o.git(&["checkout", "-q", "main"]);
    o.git(&["reset", "-q", "--hard", "neu"]);
    zweite_wurzel("(a)");
    o.git(&["reset", "-q", "--hard", &stand]);
    // (b) Merge mit einer fremden Geschichte (eigene Wurzel) als zweitem Elternteil.
    o.git(&["checkout", "-q", "--orphan", "fremd"]);
    o.datei("src/x.rs", b"// x\n");
    o.commit("fremde Wurzel", &[]);
    o.git(&["checkout", "-q", "main"]);
    o.git(&[&ranke[..], &["merge", "-q", "--no-ff", "--allow-unrelated-histories", "fremd", "-m", "chore(merge): fremd"]].concat());
    zweite_wurzel("(b)");
    o.git(&["reset", "-q", "--hard", &stand]);

    // Gegenprobe: fremder Autor auf der Seitenlinie eines Merge-Commits wird abgewiesen;
    // auch mit verstellter Remote-Tracking-Ref (Vorprüfung sieht keine neuen Commits) prüft der Hook den echten Bereich.
    o.git(&["checkout", "-q", "-B", "zweig"]);
    o.datei("src/f.rs", b"// f\n");
    o.commit("fremd", &[("GIT_AUTHOR_NAME", "Fremd"), ("GIT_AUTHOR_EMAIL", "fremd@example.org")]);
    o.git(&["checkout", "-q", "main"]);
    o.git(&["-c", "user.name=Ranke", "-c", "user.email=ranke2@proton.me", "merge", "-q", "--no-ff", "zweig", "-m", "chore(merge): zweig"]);
    let (c, text) = hochladen(&["-Pruefen"]);
    assert!(c == 1 && text.contains("Autor ist nicht"), "{text}");
    o.git(&["update-ref", "refs/remotes/origin/main", "main"]);
    let (c, text) = hochladen(&[]);
    assert!(c == 1 && text.contains("Autor ist nicht") && text.contains("Das Hochladen ist gescheitert"), "{text}");
    assert_eq!(fern_ref("refs/heads/main"), stand, "nichts ging hinaus");
    o.git(&["update-ref", "refs/remotes/origin/main", &stand]);
    o.git(&["reset", "-q", "--hard", &stand]);

    // -Tag wie bisher: nur auf der First-Parent-Linie von main, nicht mit -Pruefen.
    o.ueber_merge(|o| fassung_setzen(o, "0.1.0", true));
    let (c, text) = hochladen(&[]);
    assert_eq!(c, 0, "{text}");
    o.git(&["tag", "v0.1.0", "main"]);
    o.git(&["tag", "v0.2.0", "zweig"]);
    let (c, text) = hochladen(&["-Tag", "v0.1.0", "-Pruefen"]);
    assert!(c == 1 && text.contains("nicht zusammen mit -Pruefen"), "{text}");
    assert!(fern_ref("refs/tags/v0.1.0").is_empty());
    let (c, text) = hochladen(&["-Tag", "v0.2.0"]);
    assert!(c == 1 && text.contains("nicht auf der First-Parent-Linie"), "{text}");
    assert!(fern_ref("refs/tags/v0.2.0").is_empty());
    let (c, text) = hochladen(&["-Tag", "v0.1.0"]);
    assert_eq!(c, 0, "{text}");
    assert_eq!(fern_ref("refs/tags/v0.1.0"), o.git(&["rev-parse", "main"]), "{text}");
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
/// der Adresse des Remotes (Art aus repos.toml). Das Remote ist
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
    for name in ["Vor-Push.ps1", "Hochladen.ps1", "Laufzeit.ps1"] {
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
            .env("DASHBOARD_PRUEFER_CACHE", o.cache())
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
    // Gegenprobe: main ohne den fernen Stand (kein Fast-Forward) bricht vor der Prüfung ab.
    o.git(&["reset", "-q", "--hard", &format!("{sauber}~1")]);
    o.ueber_merge(|o| o.datei("projects/alpha/anders.toml", b"y = 1\n"));
    for args in [&["-Pruefen"][..], &[][..]] {
        let (c, text) = hochladen(args);
        assert_eq!(c, 1, "{args:?}: {text}");
        assert!(text.contains("kein Fast-Forward") && !text.contains("Geprüft als"), "{args:?}: {text}");
    }
    assert_eq!(fern_main(), sauber, "nichts ging hinaus");

    // Kein -Wurzel (auch nicht im Projekt-Repo); ohne core.hooksPath auf das Code-Repo bricht es ab.
    let (c, text) = hochladen(&["-Wurzel"]);
    assert_eq!(c, 1, "{text}");
    assert!(text.contains("-Wurzel gibt es nicht mehr"), "{text}");
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

/// Eine erfundene GitHub-noreply-Adresse; das „§“ vor dem @ fällt weg (`beitrag`),
/// so steht hier keine Adresse wörtlich, die der Prüfer im Text meldete.
const NOREPLY: &str = "12345+beitrag§@users.noreply.github.com";

/// Autor und Committer Beitrag mit dieser Adresse.
fn als_beitrag(mail: &str) -> [(&'static str, &str); 4] {
    [("GIT_AUTHOR_NAME", "Beitrag"), ("GIT_AUTHOR_EMAIL", mail), ("GIT_COMMITTER_NAME", "Beitrag"), ("GIT_COMMITTER_EMAIL", mail)]
}

/// Ein Beitrag aus einem Fork: `aendern` auf dem Zweig, ein Commit mit fremdem
/// Autor und Committer (`name`, `mail`; ein „§“ fällt weg), dann ein Merge-Commit
/// (Ranke) nach main. Der Beitrag liegt so außerhalb der First-Parent-Linie.
fn beitrag(o: &Ort, nachricht: &str, name: &str, mail: &str, aendern: impl FnOnce(&Ort)) {
    o.git(&["checkout", "-q", "-B", "zweig"]);
    aendern(o);
    let mail = mail.replace('§', "");
    o.commit(
        &nachricht.replace('§', ""),
        &[("GIT_AUTHOR_NAME", name), ("GIT_AUTHOR_EMAIL", &mail), ("GIT_COMMITTER_NAME", name), ("GIT_COMMITTER_EMAIL", &mail)],
    );
    zusammenfuehren(o);
}

/// Wie `abgewiesen`, nur dass der Fall durchgehen muss.
fn frei(name: &str, vorbereiten: impl FnOnce(&Ort)) {
    let o = Ort::neu();
    vorbereiten(&o);
    let (code, text) = o.pruefen();
    assert_eq!(code, 0, "{name}: muss durchgehen:\n{text}");
}

/// Beiträge aus Forks: Außerhalb der First-Parent-Linie von main darf Autor und
/// Committer eine GitHub-noreply-Adresse oder eine Adresse auf .invalid oder
/// .example haben (verankert, nur ASCII, Groß/Klein egal); auf der Linie bleibt es
/// genau Ranke. Gegenproben: dieselben Adressen auf der Linie, echte und
/// verlängerte Adressen, das breite ＠.
#[test]
fn beitraege_aus_forks_adressen() {
    if !bereit() {
        return;
    }
    for mail in [NOREPLY, "beitrag§@users.noreply.github.com", "12345+BEI-trag§@USERS.NOREPLY.GITHUB.COM", "beitrag@beispiel.invalid", "beitrag@team.example"]
    {
        frei(mail, |o| beitrag(o, "feat: Beitrag", "Beitrag", mail, |o| o.datei("src/b.rs", b"// b\n")));
    }
    for (fall, mail) in [
        ("echte Adresse", "beitrag§@posteo.de"),
        ("verlängert", "x§@users.noreply.github.com.evil.de"),
        ("breites ＠", "x\u{FF20}users.noreply.github.com"),
        // Das Kelvin-Zeichen gilt ohne Groß/Klein-Unterschied als k: nur ASCII.
        ("Kelvin-Zeichen", "\u{212A}x§@users.noreply.github.com"),
        ("Strich am Anfang des Logins", "-x§@users.noreply.github.com"),
        ("doppelter Strich", "a--b§@users.noreply.github.com"),
        ("Adresse von GitHub selbst", "noreply§@github.com"),
        ("invalid nur als Wortteil", "x§@beispiel.invalidx"),
    ] {
        abgewiesen(fall, "und hat keine Beitragsadresse", |o| beitrag(o, "feat: Beitrag", "Beitrag", mail, |o| o.datei("src/b.rs", b"// b\n")));
    }
    // Nur der Committer fremd und ohne Beitragsadresse: auch ein Fund.
    abgewiesen("Committer", "Committer ist nicht Ranke", |o| {
        o.git(&["checkout", "-q", "-B", "zweig"]);
        o.datei("src/b.rs", b"// b\n");
        o.commit("feat: b", &[("GIT_COMMITTER_EMAIL", &"beitrag§@posteo.de".replace('§', ""))]);
        zusammenfuehren(o);
    });
    // Auf der Linie bleibt es genau Ranke: ein Merge-Commit mit Beitragsadresse.
    let mail = NOREPLY.replace('§', "");
    abgewiesen("fremder Autor auf der Linie", "Autor ist nicht Ranke", |o| {
        o.git(&["checkout", "-q", "-B", "zweig"]);
        o.datei("src/b.rs", b"// b\n");
        o.commit("feat: b", &[]);
        o.git(&["checkout", "-q", "main"]);
        o.git(&["-c", "user.name=Beitrag", "-c", &format!("user.email={mail}"), "merge", "-q", "--no-ff", "zweig", "-m", "chore(merge): zweig"]);
    });
    abgewiesen("fremder Commit direkt auf main", "Autor ist nicht Ranke", |o| {
        o.datei("src/b.rs", b"// b\n");
        o.commit("feat: b", &[("GIT_AUTHOR_NAME", "Beitrag"), ("GIT_AUTHOR_EMAIL", &mail)]);
    });
}

/// Was für jeden Commit gilt, gilt auch für Beiträge: Name durch die Textprüfung,
/// Nachricht ohne KI-Namen und Listenwerte, keine Signatur, jeder Zwischenstand
/// geprüft, eine Wurzel. Gegenprobe: derselbe handgebaute Commit ohne Signatur geht durch.
#[test]
fn beitraege_aus_forks_wie_jeder_commit() {
    if !bereit() {
        return;
    }
    for name in ["Vorname Nachname", "Geheimprojekt"] {
        abgewiesen(name, "(Autor), Zeile 1: sensibler Name", |o| beitrag(o, "feat: b", name, NOREPLY, |o| o.datei("src/b.rs", b"// b\n")));
    }
    abgewiesen("KI-Name im Namen", "im Namen", |o| beitrag(o, "feat: b", &"Ge§mini Helfer".replace('§', ""), NOREPLY, |o| o.datei("src/b.rs", b"// b\n")));
    abgewiesen("KI-Name in der Nachricht", "Anhang (Trailer) oder KI-Name", |o| {
        beitrag(o, "feat: mit Cl§aude", "Beitrag", NOREPLY, |o| o.datei("src/b.rs", b"// b\n"))
    });
    abgewiesen("Listenwert in der Nachricht", "sensibler Name", |o| beitrag(o, "feat: Gruenkohlfest", "Beitrag", NOREPLY, |o| o.datei("src/b.rs", b"// b\n")));
    // Auch der Domänenteil einer erfundenen Adresse läuft durch die Listen (Gegenprobe: beitrag@beispiel.invalid geht durch).
    abgewiesen("Listenwert in der Domäne", "(Autor), Zeile 2: sensibler Name", |o| {
        beitrag(o, "feat: b", "Beitrag", "x@geheim§projekt.invalid", |o| o.datei("src/b.rs", b"// b\n"))
    });
    abgewiesen("Signed-off-by", "Anhang (Trailer)", |o| {
        beitrag(o, "feat: b\n\nSigned-off-by: Beitrag <beitrag@beispiel.invalid>", "Beitrag", NOREPLY, |o| o.datei("src/b.rs", b"// b\n"))
    });
    // Signatur: ein handgebauter Commit mit gpgsig auf dem Zweig.
    let mail = NOREPLY.replace('§', "");
    let signiert = |sig: &'static str| {
        let mail = mail.clone();
        move |o: &Ort| {
            let baum = o.git(&["rev-parse", "HEAD^{tree}"]);
            let eltern = o.git(&["rev-parse", "HEAD"]);
            let wer = format!("Beitrag <{mail}> 1700000000 +0100");
            let sha = roh_objekt(o, "commit", format!("tree {baum}\nparent {eltern}\nauthor {wer}\ncommitter {wer}\n{sig}\nfeat: signiert\n").as_bytes());
            o.git(&["branch", "-f", "zweig", &sha]);
            zusammenfuehren(o);
        }
    };
    abgewiesen("Signatur", "Kopfzeile außer tree", signiert("gpgsig -----BEGIN SSH SIGNATURE-----\n x\n -----END SSH SIGNATURE-----\n"));
    // Eine Fortsetzungszeile (Leerzeichen am Anfang) trüge ungeprüften Text hinaus.
    abgewiesen("Fortsetzungszeile", "Kopfzeile außer tree", signiert(" fortgesetzter Text\n"));
    frei("ohne Signatur", signiert(""));
    // Ein verbotener Blob nur in einem Zwischencommit des Beitrags, an der Spitze wieder weg.
    abgewiesen("Zwischenstand", "src/kurz.rs", |o| {
        beitrag(o, "feat: wieder weg", "Beitrag", NOREPLY, |o| {
            o.datei("src/kurz.rs", b"// geheim\xc2\xa7projekt\n");
            o.commit("feat: kurz da", &als_beitrag(&NOREPLY.replace('§', "")));
            std::fs::remove_file(o.repo().join("src/kurz.rs")).unwrap();
        });
    });
    // Ein Beitrag mit eigener Wurzel.
    abgewiesen("eigene Wurzel", "Commit ohne Eltern", |o| {
        o.git(&["checkout", "-q", "--orphan", "zweig"]);
        o.datei("src/x.rs", b"// x\n");
        o.commit("feat: fremde Wurzel", &als_beitrag(&NOREPLY.replace('§', "")));
        o.git(&["checkout", "-q", "main"]);
        o.git(&[
            "-c",
            "user.name=Ranke",
            "-c",
            "user.email=ranke2@proton.me",
            "merge",
            "-q",
            "--no-ff",
            "--allow-unrelated-histories",
            "zweig",
            "-m",
            "chore(merge): zweig",
        ]);
    });
}

/// M1: Ein fremder Commit ändert keinen Prüfpfad (scripts/**, .cargo/**, .github/**,
/// tests/vor_push.rs, KONTAKT-PGP.asc, jede .gitattributes und .npmrc, 8.3-Kurznamen);
/// CONTRIBUTING.md darf er ändern.
/// Gezählt wird, was er gegen jeden Elternteil ändert: Ein Merge von main in den
/// Fork übernimmt eine Änderung des Betreuers an scripts/ ohne Fund, ein Merge, der
/// dort selbst etwas ändert, ist ein Fund. Gegenprobe: dieselbe Änderung von Ranke.
#[test]
fn beitraege_aus_forks_ohne_pruefer() {
    if !bereit() {
        return;
    }
    let satz = "Änderungen am Prüfer übernimmt der Betreuer selbst, in einem eigenen Merge ohne fremde Commits";
    for pfad in [
        "scripts/Vor-Push.ps1",
        "scripts/hooks/pre-push",
        "scripts/Hochladen.ps1",
        "tests/vor_push.rs",
        ".cargo/config.toml",
        ".github/ISSUE_TEMPLATE/merge-angebot.yml",
        "KONTAKT-PGP.asc",
        ".gitattributes",
        // Groß und Kleinschreibung: Unter Windows landen diese Pfade in denselben Ordnern und Dateien.
        "SCRIPTS/x.ps1",
        "Scripts/hooks/pre-push",
        "TESTS/VOR_PUSH.rs",
        ".GITHUB/ISSUE_TEMPLATE/x.yml",
        ".Cargo/config.toml",
        "kontakt-pgp.asc",
        ".GitAttributes",
        // Verschachtelt: Eine .gitattributes mit -diff verbirgt im Diff eine Datei als binär,
        // eine .npmrc schaltet die Paket-Skripte beim npm ci ein.
        "web/.gitattributes",
        "tests/.gitattributes",
        "src/.GitAttributes",
        "web/.npmrc",
        "WEB/.NPMRC",
    ] {
        abgewiesen(pfad, satz, |o| beitrag(o, "fix: Prüfer", "Beitrag", NOREPLY, |o| o.datei(pfad, b"# anders\n")));
        frei(pfad, |o| o.ueber_merge(|o| o.datei(pfad, b"# anders\n")));
    }
    // 8.3-Kurznamen landen unter Windows in der Datei mit langem Namen (CARGO~1/config.toml
    // in .cargo/config.toml). In Unterordnern weist sie schon das Muster für Kurznamen ab,
    // für jeden; an der Wurzel die Positivliste, und M1 meldet sie dort selbst, ebenso
    // verschachtelt (web/NPMRC~1 ist unter Windows web/.npmrc)
    // (§ hält den Prüfer über dieser Datei frei).
    for kurz in ["CAR§GO~1/config.toml", "GIT§ATT~1", "KONTAK§~1.ASC", "web/NPM§RC~1", "a/KONT§AK~1.ASC", "src/LONG§NA~1/x"] {
        let kurz = kurz.replace('§', "");
        abgewiesen(&kurz, &format!("ein fremder Commit ändert den Prüfpfad {kurz}"), |o| {
            beitrag(o, "fix: Prüfer", "Beitrag", NOREPLY, |o| o.datei(&kurz, b"# anders\n"))
        });
    }
    // Gegenprobe: ähnliche Namen ohne Kurznamen und ohne Punkt bleiben frei.
    frei("ähnliche Namen", |o| {
        beitrag(o, "feat: b", "Beitrag", NOREPLY, |o| {
            o.datei("src/gitattributes.rs", b"// a\n");
            o.datei("web/npmrc.txt", b"a\n");
            o.datei("src/a~b.rs", b"// a\n");
        })
    });
    // Der Leitfaden ist ein Textbeitrag: Ein fremder Commit darf ihn ändern.
    frei("CONTRIBUTING.md im Beitrag", |o| beitrag(o, "docs: Leitfaden", "Beitrag", NOREPLY, |o| o.datei("CONTRIBUTING.md", b"# Mitwirken\n")));
    let mail = NOREPLY.replace('§', "");
    // Nur der Committer fremd (Autor Ranke): M1 gilt genauso, schon am Commit selbst
    // (nicht erst am Merge der Linie).
    abgewiesen("nur Committer fremd", "ein fremder Commit ändert den Prüfpfad", |o| {
        o.git(&["checkout", "-q", "-B", "zweig"]);
        o.datei("scripts/Vor-Push.ps1", b"# anders\n");
        o.commit("fix: Prüfer", &[("GIT_COMMITTER_NAME", "Beitrag"), ("GIT_COMMITTER_EMAIL", &mail)]);
        zusammenfuehren(o);
    });
    // Der Fork holt main (mit einer Änderung des Betreuers an scripts/) in seinen Zweig.
    let fork_merge = |boese: bool| {
        let mail = mail.clone();
        move |o: &Ort| {
            o.git(&["checkout", "-q", "-b", "zweig"]);
            o.datei("src/a.rs", b"// a\n");
            o.commit("feat: a", &als_beitrag(&mail));
            o.git(&["checkout", "-q", "main"]);
            o.git(&["checkout", "-q", "-b", "pflege"]);
            o.datei("scripts/p.ps1", b"# p\n");
            o.commit("fix: p", &[]);
            o.git(&["checkout", "-q", "main"]);
            o.git(&["-c", "user.name=Ranke", "-c", "user.email=ranke2@proton.me", "merge", "-q", "--no-ff", "pflege", "-m", "chore(merge): pflege"]);
            o.git(&["checkout", "-q", "zweig"]);
            let wer = ["-c", "user.name=Beitrag", "-c", &format!("user.email={mail}")];
            o.git(&[&wer[..], &["merge", "-q", "--no-ff", "--no-commit", "main"]].concat());
            if boese {
                o.datei("scripts/p.ps1", b"# p, im Merge geaendert\n");
                o.git(&["add", "-A"]);
            }
            o.git(&[&wer[..], &["commit", "-q", "-m", "chore(merge): main"]].concat());
            zusammenfuehren(o);
        }
    };
    frei("Fork holt main", fork_merge(false));
    abgewiesen("Merge im Fork ändert scripts/", satz, fork_merge(true));
    // Eine eigene Änderung an scripts/ geht nur in einem eigenen Merge ohne fremde Commits:
    // im selben Seitenzweig wie ein Beitrag ist sie ein Fund am Merge der Linie.
    let gemischt = |getrennt: bool| {
        let mail = mail.clone();
        move |o: &Ort| {
            if getrennt {
                o.ueber_merge(|o| o.datei("scripts/p.ps1", b"# p\n"));
            }
            o.git(&["checkout", "-q", "-B", "zweig"]);
            if !getrennt {
                o.datei("scripts/p.ps1", b"# p\n");
                o.commit("fix: p", &[]);
            }
            o.datei("src/b.rs", b"// b\n");
            o.commit("feat: b", &als_beitrag(&mail));
            zusammenfuehren(o);
        }
    };
    frei("eigener Merge für scripts/", gemischt(true));
    abgewiesen("scripts/ im Seitenzweig eines Beitrags", "der Merge übernimmt fremde Commits und ändert den Prüfpfad scripts/p.ps1", gemischt(false));
}

/// Ein fremder Commit ändert CHANGELOG.txt nicht (Groß/Klein egal, auch beim Umbenennen
/// hinein und hinaus, auch als Merge im Fork): Die Zeilen stehen im Merge-Angebot, die
/// Betreuer tragen sie ein. Gegenproben: dieselbe Änderung von Ranke, main mit einer
/// Änderung am Changelog in den Fork geholt.
#[test]
fn beitraege_aus_forks_ohne_changelog() {
    if !bereit() {
        return;
    }
    let satz = "CHANGELOG.txt ändern nur die Betreuer; die Zeilen gehören ins Merge-Angebot";
    for pfad in ["CHANGELOG.txt", "changelog.TXT"] {
        abgewiesen(pfad, &format!("ein fremder Commit ändert {pfad}; {satz}"), |o| {
            beitrag(o, "docs: Changelog", "Beitrag", NOREPLY, |o| o.datei(pfad, b"Unreleased\n"))
        });
        frei(pfad, |o| o.ueber_merge(|o| o.datei(pfad, b"Unreleased\n")));
    }
    abgewiesen("umbenannt hinein", &format!("ein fremder Commit ändert Changelog.txt; {satz}"), |o| {
        beitrag(o, "docs: verschoben", "Beitrag", NOREPLY, |o| {
            o.git(&["mv", "LIESMICH.txt", "Changelog.txt"]);
        })
    });
    abgewiesen("umbenannt hinaus", &format!("ein fremder Commit ändert CHANGELOG.txt; {satz}"), |o| {
        o.ueber_merge(|o| o.datei("CHANGELOG.txt", b"Unreleased\n"));
        beitrag(o, "docs: verschoben", "Beitrag", NOREPLY, |o| {
            o.git(&["mv", "CHANGELOG.txt", "src/alt.txt"]);
        })
    });
    let mail = NOREPLY.replace('§', "");
    // Der Fork holt main, auf dem die Betreuer den Changelog geändert haben.
    let fork_merge = |boese: bool| {
        let mail = mail.clone();
        move |o: &Ort| {
            o.git(&["checkout", "-q", "-b", "zweig"]);
            o.datei("src/a.rs", b"// a\n");
            o.commit("feat: a", &als_beitrag(&mail));
            o.git(&["checkout", "-q", "main"]);
            o.git(&["checkout", "-q", "-b", "pflege"]);
            o.datei("CHANGELOG.txt", b"Unreleased\n");
            o.commit("docs: Changelog", &[]);
            o.git(&["checkout", "-q", "main"]);
            o.git(&["-c", "user.name=Ranke", "-c", "user.email=ranke2@proton.me", "merge", "-q", "--no-ff", "pflege", "-m", "chore(merge): pflege"]);
            o.git(&["checkout", "-q", "zweig"]);
            let wer = ["-c", "user.name=Beitrag", "-c", &format!("user.email={mail}")];
            o.git(&[&wer[..], &["merge", "-q", "--no-ff", "--no-commit", "main"]].concat());
            if boese {
                o.datei("CHANGELOG.txt", b"Unreleased\n- im Merge geaendert\n");
                o.git(&["add", "-A"]);
            }
            o.git(&[&wer[..], &["commit", "-q", "-m", "chore(merge): main"]].concat());
            zusammenfuehren(o);
        }
    };
    frei("Fork holt main mit Changelog", fork_merge(false));
    // Der Fund am Fork-Merge selbst, nicht nur der allgemeine Satz am Merge der Linie.
    abgewiesen("Merge im Fork ändert den Changelog", &format!("ein fremder Commit ändert CHANGELOG.txt; {satz}"), fork_merge(true));
    // Eine eigene Änderung am Changelog im selben Seitenzweig wie ein Beitrag: Fund am Merge
    // der Linie (die Zeilen kommen in einem eigenen Commit nach dem Merge).
    abgewiesen("Changelog im Seitenzweig eines Beitrags", &format!("der Merge übernimmt fremde Commits und ändert CHANGELOG.txt; {satz}"), |o| {
        o.git(&["checkout", "-q", "-B", "zweig"]);
        o.datei("CHANGELOG.txt", b"Unreleased\n");
        o.commit("docs: Changelog", &[]);
        o.datei("src/b.rs", b"// b\n");
        o.commit("feat: b", &als_beitrag(&mail));
        zusammenfuehren(o);
    });
}

/// Der Bestand vor der Regel zu CHANGELOG.txt (ein übernommener Fork-Commit und sein
/// Merge, schon veröffentlicht) ist ausgenommen: genau diese zwei vollen Kennungen, nur
/// für CHANGELOG.txt. Ein Test-Repo kann keine Commits mit diesen Kennungen bauen. Darum
/// liest der Test die Menge wörtlich aus dem Prüfer und setzt in einer Kopie die
/// Kennungen einer nachgebauten Geschichte ein, die erst beim Bau entstehen. Der Prüfer
/// selbst hat dafür keinen Schalter (keine Umgebungsvariable, kein Parameter), der ihn
/// im Betrieb abschwächen könnte.
#[test]
fn changelog_bestand_vor_der_regel() {
    if !bereit() {
        return;
    }
    const BESTAND: [&str; 2] = ["8b10863c62ebf05c63bf12b25d918cb648c562ed", "84cd764e5be14be991b65b51176eaff18a48ccc4"];
    let quelle = std::fs::read_to_string(skript()).unwrap();
    // Ohne Sigil und ohne Groß/Klein, weil PowerShell auch $script:…, ${…} und andere Schreibung annimmt.
    let zeilen: Vec<&str> = quelle.lines().filter(|z| z.to_lowercase().contains("changelogbestand")).collect();
    let definition: Vec<&str> = zeilen.iter().copied().filter(|z| z.starts_with("$changelogBestand = ")).collect();
    assert_eq!(definition.len(), 1, "genau eine Festlegung der Menge:\n{zeilen:#?}");
    let kennungen: Vec<&str> = definition[0].split(|c: char| !c.is_ascii_hexdigit()).filter(|t| t.len() == 40).collect();
    assert_eq!(kennungen, BESTAND, "die Menge enthält genau diese zwei Commits");
    // Nur an den zwei Stellen der Regel zu CHANGELOG.txt (fremder Commit, Merge der Linie).
    let nutzung = "$changelogPfad.IsMatch($p) -and -not $changelogBestand.Contains($c)";
    assert_eq!(zeilen.iter().filter(|z| z.contains(nutzung)).count(), 2, "{zeilen:#?}");
    assert_eq!(zeilen.len(), 3, "die Menge gilt für keine andere Regel:\n{zeilen:#?}");
    for k in BESTAND {
        assert_eq!(quelle.matches(k).count(), 1, "{k} steht nur in der Menge");
    }
    let satz = "CHANGELOG.txt ändern nur die Betreuer; die Zeilen gehören ins Merge-Angebot";
    let mail = NOREPLY.replace('§', "");
    // Ein Fork-Commit mit eigenen Changelog-Zeilen (optional auch an scripts/) und der Merge der Linie.
    let bau = |datum: &str, pruefpfad: bool| {
        let o = Ort::neu();
        o.git(&["checkout", "-q", "-B", "zweig"]);
        o.datei("CHANGELOG.txt", b"Unreleased\n\nBehoben\n- eigene Zeile des Beitrags\n");
        if pruefpfad {
            o.datei("scripts/p.ps1", b"# p\n");
        }
        let mut wer = als_beitrag(&mail).to_vec();
        wer.extend([("GIT_AUTHOR_DATE", datum), ("GIT_COMMITTER_DATE", datum)]);
        let fork = o.commit("fix: Beitrag mit Changelog", &wer);
        zusammenfuehren(&o);
        let merge = o.git(&["rev-parse", "main"]);
        (o, fork, merge)
    };
    // Eine Kopie des Prüfers, in der die Menge genau diese zwei Kennungen enthält.
    let kopie = |o: &Ort, fork: &str, merge: &str| {
        let ordner = o.wurzel.join("kopie");
        prueferdateien_kopieren(&ordner);
        let p = ordner.join("Vor-Push.ps1");
        std::fs::write(&p, quelle.replacen(BESTAND[0], fork, 1).replacen(BESTAND[1], merge, 1)).unwrap();
        p
    };
    let args = ["-Baum", "main", "-Commits", "main", "-AufMain"];
    let changelog = |text: &str| -> Vec<String> { text.lines().filter(|z| z.contains(satz)).map(str::to_string).collect() };
    let (a, a_fork, a_merge) = bau("2001-01-02T00:00:00+0000", false);
    // Gegenprobe: Ohne Eintrag in der Menge sind beide ein Fund.
    let (code, text) = a.pruefen();
    assert_eq!(code, 1, "{text}");
    let funde = changelog(&text);
    assert_eq!(funde.len(), 2, "{text}");
    assert!(funde.iter().any(|z| z.contains(&format!("Commit {}", &a_fork[..10])) && z.contains("ein fremder Commit ändert CHANGELOG.txt")), "{text}");
    assert!(funde.iter().any(|z| z.contains(&format!("Commit {}", &a_merge[..10])) && z.contains("der Merge übernimmt fremde Commits")), "{text}");
    // In der Menge: frei.
    let pruefer_a = kopie(&a, &a_fork, &a_merge);
    let (code, text) = a.lauf_mit(&pruefer_a, &a.cache(), &args, None);
    assert_eq!(code, 0, "Bestand bleibt frei:\n{text}");
    // Derselbe Inhalt mit anderer Kennung (anderes Datum): Fund, auch mit der Menge von oben.
    let (b, b_fork, b_merge) = bau("2001-01-03T00:00:00+0000", false);
    assert_ne!(a_fork, b_fork);
    assert_eq!(a.git(&["rev-parse", "main^{tree}"]), b.git(&["rev-parse", "main^{tree}"]), "derselbe Inhalt");
    let (code, text) = b.lauf_mit(&pruefer_a, &b.cache(), &args, None);
    assert_eq!(code, 1, "{text}");
    let funde = changelog(&text);
    assert_eq!(funde.len(), 2, "{text}");
    assert!(
        funde.iter().any(|z| z.contains(&format!("Commit {}", &b_fork[..10]))) && funde.iter().any(|z| z.contains(&format!("Commit {}", &b_merge[..10]))),
        "{text}"
    );
    // Nur für CHANGELOG.txt: Ein Bestands-Commit, der auch scripts/ ändert, bleibt dort ein Fund.
    let (c, c_fork, c_merge) = bau("2001-01-04T00:00:00+0000", true);
    let pruefer_c = kopie(&c, &c_fork, &c_merge);
    let (code, text) = c.lauf_mit(&pruefer_c, &c.cache(), &args, None);
    assert_eq!(code, 1, "{text}");
    assert!(changelog(&text).is_empty(), "{text}");
    assert!(text.contains(&format!("Commit {}", &c_fork[..10])) && text.contains("ein fremder Commit ändert den Prüfpfad scripts/p.ps1"), "{text}");
    assert!(text.contains("der Merge übernimmt fremde Commits und ändert den Prüfpfad scripts/p.ps1"), "{text}");
}

/// Ein Fund aus einem älteren Commit nennt am Ende der Zeile den ältesten Commit des
/// Bereichs, der dieses Paar (Pfad, Blob) einführt, mit 10 Zeichen; dasselbe Paar in einem
/// späteren Commit ergibt keinen zweiten Fund. Ein Fund aus dem Baum (Endstand) bleibt
/// ohne Kennung.
#[test]
fn fund_im_verlauf_nennt_commit() {
    if !bereit() {
        return;
    }
    let geheim: &[u8] = b"// geheim\xc2\xa7projekt\n";
    let o = Ort::neu();
    o.git(&["checkout", "-q", "-B", "zweig"]);
    o.datei("src/kurz.rs", geheim);
    let eins = o.commit("feat: kurz da", &[]);
    std::fs::remove_file(o.repo().join("src/kurz.rs")).unwrap();
    o.commit("fix: weg", &[]);
    o.datei("src/kurz.rs", geheim);
    o.commit("feat: wieder da", &[]);
    std::fs::remove_file(o.repo().join("src/kurz.rs")).unwrap();
    o.commit("fix: wieder weg", &[]);
    zusammenfuehren(&o);
    let (code, text) = o.pruefen();
    assert_eq!(code, 1, "{text}");
    let zeilen: Vec<&str> = text.lines().map(str::trim_end).filter(|z| z.contains("src/kurz.rs")).collect();
    assert_eq!(zeilen.len(), 1, "ein Fund je Paar:\n{text}");
    assert!(zeilen[0].ends_with(&format!(" (Commit {})", &eins[..10])), "Kennung von Commit 1:\n{text}");
    // Zwei Zweige führen dasselbe Paar unabhängig ein, der Seitenzweig früher (nach Datum);
    // nach dem Merge ist es wieder weg. Genannt wird der ältere (--topo-order nennte hier den jüngeren).
    let datum = |d: &'static str| [("GIT_AUTHOR_DATE", d), ("GIT_COMMITTER_DATE", d)];
    let o = Ort::neu();
    o.git(&["checkout", "-q", "-B", "zweig"]);
    o.git(&["checkout", "-q", "-b", "seite"]);
    o.datei("src/doppelt.rs", geheim);
    let alt = o.commit("feat: doppelt auf der Seite", &datum("2001-01-02T00:00:00+0000"));
    o.git(&["checkout", "-q", "zweig"]);
    o.datei("src/doppelt.rs", geheim);
    let jung = o.commit("feat: doppelt im Zweig", &datum("2001-01-03T00:00:00+0000"));
    o.git(&["-c", "user.name=Ranke", "-c", "user.email=ranke2@proton.me", "merge", "-q", "--no-ff", "seite", "-m", "chore(merge): seite"]);
    std::fs::remove_file(o.repo().join("src/doppelt.rs")).unwrap();
    o.commit("fix: doppelt weg", &[]);
    zusammenfuehren(&o);
    let (code, text) = o.pruefen();
    assert_eq!(code, 1, "{text}");
    let zeilen: Vec<&str> = text.lines().map(str::trim_end).filter(|z| z.contains("src/doppelt.rs")).collect();
    assert_eq!(zeilen.len(), 1, "ein Fund je Paar:\n{text}");
    assert!(zeilen[0].ends_with(&format!(" (Commit {})", &alt[..10])), "Kennung des älteren Commits {alt}, nicht {jung}:\n{text}");
    // Endstand: Der Fund steht im Baum und trägt keine Kennung.
    let o = Ort::neu();
    o.ueber_merge(|o| o.datei("src/bleibt.rs", geheim));
    let (code, text) = o.pruefen();
    assert_eq!(code, 1, "{text}");
    let zeilen: Vec<&str> = text.lines().filter(|z| z.contains("src/bleibt.rs")).collect();
    assert!(!zeilen.is_empty() && zeilen.iter().all(|z| !z.contains("(Commit ")), "Fund im Baum ohne Kennung:\n{text}");
    // Derselbe Blob am selben Pfad erst als Datei, dann als Verknüpfung: Der Fund zur
    // Verknüpfung nennt den Commit der Verknüpfung (das Paar zählt mit seinem Modus).
    let o = Ort::neu();
    o.git(&["checkout", "-q", "-B", "zweig"]);
    o.datei("src/v.rs", b"// v\n");
    o.commit("feat: v als Datei", &[]);
    let blob = o.git(&["rev-parse", "HEAD:src/v.rs"]);
    o.git(&["update-index", "--cacheinfo", &format!("120000,{blob},src/v.rs")]);
    commit_wie_index(&o, "test: v als Verknüpfung");
    let verknuepfung = o.git(&["rev-parse", "HEAD"]);
    o.git(&["rm", "-q", "--cached", "src/v.rs"]);
    commit_wie_index(&o, "test: v weg");
    zusammenfuehren(&o);
    let (code, text) = o.pruefen();
    assert_eq!(code, 1, "{text}");
    let zeilen: Vec<&str> = text.lines().map(str::trim_end).filter(|z| z.contains("src/v.rs") && z.contains("Verknüpfung (Symlink)")).collect();
    assert_eq!(zeilen.len(), 1, "ein Fund zur Verknüpfung:\n{text}");
    assert!(zeilen[0].ends_with(&format!(" (Commit {})", &verknuepfung[..10])), "Kennung des Commits mit der Verknüpfung:\n{text}");
}

/// Hook, Hochladen (-Commits fern..main), -Pruefen und -Bundle bestimmen die Linie
/// gleich: ein Beitrag im Seitenzweig geht überall durch, derselbe Beitrag als
/// Fast-Forward auf main (auf der Linie) nirgends.
#[test]
fn beitraege_aus_forks_alle_wege() {
    if !bereit() {
        return;
    }
    for auf_linie in [false, true] {
        let o = Ort::neu();
        let wurzel = o.git(&["rev-parse", "HEAD"]);
        let mail = NOREPLY.replace('§', "");
        if auf_linie {
            o.datei("src/b.rs", b"// b\n");
            o.commit("feat: b", &als_beitrag(&mail));
        } else {
            beitrag(&o, "feat: b", "Beitrag", NOREPLY, |o| o.datei("src/b.rs", b"// b\n"));
        }
        let kopf = o.git(&["rev-parse", "HEAD"]);
        let p = o.wurzel.join("b.bundle").to_string_lossy().to_string();
        o.git(&["bundle", "create", "-q", &p, "refs/heads/main"]);
        for (weg, (code, text)) in [
            ("Hook", o.lauf(&["-Hook"], Some(&format!("refs/heads/main {kopf} refs/heads/main {wurzel}\n")))),
            ("Hook erster Push", o.lauf(&["-Hook"], Some(&format!("refs/heads/main {kopf} refs/heads/main {NULL}\n")))),
            ("Hochladen", o.lauf(&["-Baum", "main", "-Commits", &format!("{wurzel}..main"), "-AufMain"], None)),
            ("-Pruefen", o.lauf(&["-Pruefen"], None)),
            ("-Bundle", o.lauf(&["-Bundle", &p], None)),
        ] {
            if auf_linie {
                assert!(code == 1 && text.contains("Autor ist nicht Ranke"), "{weg} auf der Linie: {text}");
            } else {
                assert_eq!(code, 0, "{weg} im Seitenzweig: {text}");
            }
        }
    }
}

/// Positivliste: genau die Issue-Formulare .github/ISSUE_TEMPLATE/*.yml; der Rest
/// von .github (Workflows) bleibt ein Fund, ebenso ein Leitfaden als .txt.
#[test]
fn mitwirken_dateien_in_der_positivliste() {
    if !bereit() {
        return;
    }
    frei("Formular", |o| o.ueber_merge(|o| o.datei(".github/ISSUE_TEMPLATE/x.yml", b"name: x\n")));
    // Über einen Merge: So ist die Positivliste der einzige Grund für den Fund.
    for pfad in [
        ".github/workflows/x.yml",
        ".github/ISSUE_TEMPLATE/x.yaml",
        ".github/ISSUE_TEMPLATE/a/x.yml",
        ".github/ISSUE_TEMPLATE/config.yml",
        ".github/ISSUE_TEMPLATE/CONFIG.yml",
        ".github/ISSUE_TEMPLATE/Config.YML",
        ".github/x.yml",
        "CONTRIBUTING.txt",
    ] {
        let text = abgewiesen(pfad, "Positivliste", |o| o.ueber_merge(|o| o.datei(pfad, b"x\n")));
        assert!(fundzeilen(&text).iter().all(|z| z.contains("Positivliste")), "{pfad}: nur Funde an der Positivliste:\n{text}");
    }
}

/// M1 auf der Linie: Ein fremder Merge im Fork nimmt einen alten, schon
/// veröffentlichten main-Stand als zweiten Elternteil und übernimmt daraus einen
/// alten Prüfer (und lässt eine spätere Datei unter scripts/ weg). Gegen jeden
/// Elternteil einzeln ändert er so nichts unter scripts/; der Merge des Betreuers
/// ändert es gegenüber main und ist ein Fund. Gegenprobe: derselbe Aufbau ohne
/// den alten Stand geht durch.
#[test]
fn beitraege_aus_forks_alter_stand() {
    if !bereit() {
        return;
    }
    let satz = "Änderungen am Prüfer übernimmt der Betreuer selbst, in einem eigenen Merge ohne fremde Commits";
    let mail = NOREPLY.replace('§', "");
    // `erster_fork`: erster Elternteil ist ein eigener Fork-Commit statt main.
    let aufbau = |alt_zurueck: bool, erster_fork: bool| {
        let mail = mail.clone();
        move |o: &Ort| {
            o.ueber_merge(|o| o.datei("scripts/p.ps1", b"# alt\n"));
            let alt = o.git(&["rev-parse", "HEAD"]);
            o.ueber_merge(|o| {
                o.datei("scripts/p.ps1", b"# neu und strenger\n");
                o.datei("scripts/h.ps1", b"# h\n");
            });
            o.git(&["checkout", "-q", "-B", "fork"]);
            if erster_fork {
                o.datei("src/a.rs", b"// a\n");
                o.commit("feat: a", &als_beitrag(&mail));
            }
            if alt_zurueck {
                o.git(&["checkout", "-q", &alt, "--", "scripts/p.ps1"]);
                o.git(&["rm", "-q", "scripts/h.ps1"]);
            }
            o.datei("src/b.rs", b"// b\n");
            o.git(&["add", "-A"]);
            let baum = o.git(&["write-tree"]);
            let wer = ["-c", "user.name=Beitrag", "-c", &format!("user.email={mail}")];
            let sha = o.git(&[&wer[..], &["commit-tree", &baum, "-p", "HEAD", "-p", &alt, "-m", "chore(merge): main"]].concat());
            o.git(&["reset", "-q", "--hard"]);
            o.git(&["branch", "-f", "zweig", &sha]);
            zusammenfuehren(o);
        }
    };
    for erster_fork in [false, true] {
        let text = abgewiesen("alter Prüfer über einen alten Elternteil", satz, aufbau(true, erster_fork));
        assert!(text.contains("der Merge übernimmt fremde Commits"), "{erster_fork}: Fund am Merge des Betreuers:\n{text}");
        frei("ohne alten Stand", aufbau(false, erster_fork));
    }
}

/// Hook und Hochladen: main setzt die bisherige Linie fort. Ein Merge falsch herum
/// (erster Elternteil ein fremder Commit, der schon draußen liegt) brächte ihn auf
/// die Linie. Gegenprobe: derselbe Bereich mit einem Merge richtig herum.
#[test]
fn main_setzt_die_linie_fort() {
    if !bereit() {
        return;
    }
    for falsch_herum in [true, false] {
        let o = Ort::neu();
        beitrag(&o, "feat: b", "Beitrag", NOREPLY, |o| o.datei("src/b.rs", b"// b\n"));
        let fern = o.git(&["rev-parse", "main"]);
        if falsch_herum {
            o.git(&["checkout", "-q", "zweig"]);
            o.git(&["-c", "user.name=Ranke", "-c", "user.email=ranke2@proton.me", "merge", "-q", "--no-ff", "main", "-m", "chore(merge): main"]);
            let kopf = o.git(&["rev-parse", "HEAD"]);
            o.git(&["checkout", "-q", "main"]);
            o.git(&["reset", "-q", "--hard", &kopf]);
        } else {
            o.ueber_merge(|o| o.datei("src/c.rs", b"// c\n"));
        }
        let kopf = o.git(&["rev-parse", "main"]);
        for (weg, (code, text)) in [
            ("Hook", o.lauf(&["-Hook"], Some(&format!("refs/heads/main {kopf} refs/heads/main {fern}\n")))),
            ("Hochladen", o.lauf(&["-Baum", "main", "-Commits", &format!("{fern}..main"), "-AufMain"], None)),
        ] {
            if falsch_herum {
                assert!(code == 1 && text.contains("setzt die bisherige Linie nicht fort"), "{weg} falsch herum: {text}");
            } else {
                assert_eq!(code, 0, "{weg} richtig herum: {text}");
            }
        }
    }
}

/// Overlay (Projekt-Repo): Die Regel für fremde Autoren gilt dort genauso wie im
/// Code-Repo. Ein noreply-Commit im Seitenzweig geht durch (Vor-Push.ps1 direkt mit den
/// Schaltern, die Hochladen.ps1 gibt, und als Hook, beide mit der Adresse des
/// Projekt-Repos), auch wenn er dashboard-version.toml oder den Leitfaden
/// ändert; derselbe Autor auf der Linie, eine echte Adresse und ein Prüfpfad sind ein Fund.
#[test]
fn beitraege_aus_forks_im_overlay() {
    if !bereit() {
        return;
    }
    let laeufe = |o: &Ort| [("Vor-Push wie Hochladen", o.pruefen_mit(Some(PROJEKT_ADRESSE))), ("Hook", o.hook_mit(PROJEKT_ADRESSE))];
    for pfad in ["projects/alpha/daten/a.toml", "dashboard-version.toml", "CONTRIBUTING.md"] {
        let o = Ort::projekt();
        beitrag(&o, "feat(alpha): wert", "Beitrag", NOREPLY, |o| o.datei(pfad, b"tag = \"v0.2.0\"\n"));
        for (weg, (code, text)) in laeufe(&o) {
            assert_eq!(code, 0, "{pfad} im Seitenzweig ({weg}): {text}");
        }
    }
    let mail = NOREPLY.replace('§', "");
    for fall in ["fremder Autor auf der Linie", "echte Adresse", "Prüfpfad"] {
        let o = Ort::projekt();
        let satz = match fall {
            "fremder Autor auf der Linie" => {
                o.datei("projects/alpha/daten/a.toml", b"wert = 1\n");
                o.commit("feat(alpha): wert", &als_beitrag(&mail));
                "Autor ist nicht Ranke"
            }
            "echte Adresse" => {
                beitrag(&o, "feat(alpha): wert", "Beitrag", "beitrag§@posteo.de", |o| o.datei("projects/alpha/daten/a.toml", b"wert = 1\n"));
                "und hat keine Beitragsadresse"
            }
            _ => {
                beitrag(&o, "fix: Prüfer", "Beitrag", NOREPLY, |o| o.datei("scripts/x.ps1", b"# x\n"));
                "Änderungen am Prüfer übernimmt der Betreuer selbst, in einem eigenen Merge ohne fremde Commits"
            }
        };
        for (weg, (code, text)) in laeufe(&o) {
            assert!(code == 1 && text.contains(satz), "{fall} ({weg}): erwartet „{satz}“:\n{text}");
        }
    }
}

/// Ein echter Fork nach dem Leitfaden: ein zweiter Klon als Fork mit einem Commit
/// eines fremden noreply-Autors, die Ref per git fetch ins Haupt-Repo geholt und mit
/// git merge --no-ff als Ranke übernommen. Hochladen (Bereich alt..main) und Hook
/// lassen das durch. Gegenproben: derselbe Ablauf mit Signatur-Kopfzeile oder mit
/// Trailer im Fork-Commit ist ein Fund.
#[test]
fn echter_fork_nach_leitfaden() {
    if !bereit() {
        return;
    }
    let mail = NOREPLY.replace('§', "");
    for (fall, erwartet) in [("sauber", None), ("Signatur", Some("Kopfzeile außer tree")), ("Trailer", Some("Anhang (Trailer)"))] {
        let o = Ort::neu();
        let alt = o.git(&["rev-parse", "main"]);
        let fork = o.wurzel.join("fork");
        let aus = git().arg("clone").arg("-q").arg(o.repo()).arg(&fork).output().unwrap();
        assert!(aus.status.success(), "{}", String::from_utf8_lossy(&aus.stderr));
        let fg = |args: &[&str]| -> String {
            let aus = git().arg("-C").arg(&fork).args(args).output().unwrap();
            assert!(aus.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&aus.stderr));
            String::from_utf8_lossy(&aus.stdout).trim().to_string()
        };
        // Wie im Leitfaden: Adresse einmal im Klon setzen, unsigniert committen.
        fg(&["config", "user.name", "Beitrag"]);
        fg(&["config", "user.email", &mail]);
        fg(&["config", "commit.gpgsign", "false"]);
        fg(&["switch", "-q", "-c", "beitrag"]);
        std::fs::write(fork.join("src").join("b.rs"), "// b\n").unwrap();
        fg(&["add", "-A"]);
        let nachricht = if fall == "Trailer" { "feat: b\n\nReviewed-by: Beitrag <beitrag@beispiel.invalid>" } else { "feat: b" };
        fg(&["commit", "-q", "-m", nachricht]);
        if fall == "Signatur" {
            let baum = fg(&["rev-parse", "HEAD^{tree}"]);
            let eltern = fg(&["rev-parse", "HEAD^"]);
            let wer = format!("Beitrag <{mail}> 1700000000 +0100");
            let p = o.wurzel.join("signiert.txt");
            let roh = format!(
                "tree {baum}\nparent {eltern}\nauthor {wer}\ncommitter {wer}\ngpgsig -----BEGIN SSH SIGNATURE-----\n x\n -----END SSH SIGNATURE-----\n\nfeat: b\n"
            );
            std::fs::write(&p, roh).unwrap();
            let sha = fg(&["hash-object", "-t", "commit", "--literally", "-w", &p.to_string_lossy()]);
            fg(&["update-ref", "refs/heads/beitrag", &sha]);
        }
        let angebot = fg(&["rev-parse", "beitrag"]);
        // Die Betreuer holen die Ref, ohne etwas auszuführen, und übernehmen sie mit einem Merge-Commit.
        o.git(&["fetch", "-q", &fork.to_string_lossy(), "beitrag"]);
        assert_eq!(o.git(&["rev-parse", "FETCH_HEAD"]), angebot);
        // Gemergt wird genau die gelesene Kennung, nicht der Zweig (der im Fork weiterwandern kann).
        o.git(&["-c", "user.name=Ranke", "-c", "user.email=ranke2@proton.me", "merge", "-q", "--no-ff", &angebot, "-m", "chore(merge): beitrag nach main"]);
        let kopf = o.git(&["rev-parse", "main"]);
        for (weg, (code, text)) in [
            ("Hochladen", o.lauf(&["-Baum", "main", "-Commits", &format!("{alt}..main"), "-AufMain"], None)),
            ("Hook", o.lauf(&["-Hook"], Some(&format!("refs/heads/main {kopf} refs/heads/main {alt}\n")))),
        ] {
            match erwartet {
                None => assert_eq!(code, 0, "{fall} ({weg}): {text}"),
                Some(satz) => assert!(code == 1 && text.contains(satz), "{fall} ({weg}): erwartet „{satz}“:\n{text}"),
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Cache sauberer Blobs (-MitCache, nur Vorprüfungen; der Hook prüft immer kalt)
// ---------------------------------------------------------------------------

/// Die Adresse des Basis-Repos aus `REPOS`.
const BASIS_ADRESSE: &str = "https://git.example.invalid/team/basis.git";

/// Die Zahlen der Zeile „Inhalt: n Blobs aus dem Cache, m geprüft.“ (`None`: kalt).
fn cache_zahlen(text: &str) -> Option<(usize, usize)> {
    let z = text.lines().find_map(|z| z.trim().strip_prefix("Inhalt: "))?;
    let (n, rest) = z.split_once(" Blobs aus dem Cache, ")?;
    Some((n.parse().ok()?, rest.strip_suffix(" geprüft.")?.parse().ok()?))
}

/// Die Regel-Ordner (64 Hex-Zeichen) eines Cache-Orts, sortiert.
fn regel_ordner(cache: &Path) -> Vec<PathBuf> {
    let Ok(rd) = std::fs::read_dir(cache) else { return Vec::new() };
    let mut v: Vec<PathBuf> = rd.map(|e| e.unwrap().path()).filter(|p| p.is_dir() && p.file_name().is_some_and(|n| n.len() == 64)).collect();
    v.sort();
    v
}

/// Die Cache-Datei des einen Regel-Ordners eines Orts.
fn cache_datei(o: &Ort) -> PathBuf {
    let r = regel_ordner(&o.cache());
    assert_eq!(r.len(), 1, "genau ein Regel-Ordner erwartet: {r:?}");
    r[0].join("sauber.txt")
}

/// Ein sauberer erster Lauf mit Cache: gibt die Zahl der geprüften Blobs.
fn cache_fuellen(o: &Ort) -> usize {
    let (c, text) = o.mit_cache();
    assert_eq!(c, 0, "{text}");
    let (aus, geprueft) = cache_zahlen(&text).unwrap_or_else(|| panic!("keine Cache-Zeile: {text}"));
    assert!(aus == 0 && geprueft > 0, "{text}");
    geprueft
}

/// Ein zweiter Lauf mit Cache trifft jeden sauberen Blob und meldet dieselben
/// Funde wie ein kalter Lauf (Text, Binär, nur_basis, ein Zwischenstand).
#[test]
fn cache_warm_meldet_dieselben_funde() {
    if !bereit() {
        return;
    }
    let o = Ort::neu();
    let geprueft = cache_fuellen(&o);
    let (c, text) = o.mit_cache();
    assert_eq!(c, 0, "{text}");
    assert_eq!(cache_zahlen(&text), Some((geprueft, 0)), "der zweite Lauf trifft alles: {text}");
    o.ueber_merge(|o| {
        o.datei("src/a.rs", b"// nebelfink\n");
        o.datei("web/static/icons/b.png", &png(true));
        o.datei("src/zwischen.rs", b"// geheimprojekt\n");
    });
    o.ueber_merge(|o| std::fs::remove_file(o.repo().join("src/zwischen.rs")).unwrap());
    let (ck, kalt) = o.pruefen();
    assert!(ck == 1 && kalt.contains("Inhalt kalt:") && fundzeilen(&kalt).len() >= 3, "{kalt}");
    // Zweimal warm: Auch was ein Lauf mit Funden hinterließe, ändert den nächsten nicht.
    for lauf in 1..=2 {
        let (cw, warm) = o.mit_cache();
        assert_eq!(cw, 1, "Lauf {lauf}: {warm}");
        assert_eq!(fundzeilen(&kalt), fundzeilen(&warm), "Lauf {lauf}, kalt:\n{kalt}\nwarm:\n{warm}");
        assert!(cache_zahlen(&warm).is_some_and(|(n, _)| n > 0), "Lauf {lauf}: der warme Lauf trifft: {warm}");
    }
}

/// Ein Lauf mit Fund schreibt nichts in den Cache; der Blob mit Fund steht nie
/// darin, und jeder weitere Lauf meldet ihn wieder.
#[test]
fn cache_merkt_keinen_blob_mit_fund() {
    if !bereit() {
        return;
    }
    let o = Ort::neu();
    cache_fuellen(&o);
    let datei = cache_datei(&o);
    let vorher = std::fs::read(&datei).unwrap();
    o.ueber_merge(|o| {
        o.datei("src/a.rs", b"// geheimprojekt\n");
        o.datei("src/sauber.rs", b"// sauber\n");
    });
    let sha = o.git(&["rev-parse", "main:src/a.rs"]);
    for lauf in 1..=2 {
        let (c, text) = o.mit_cache();
        assert!(c == 1 && text.contains("ABGEWIESEN src/a.rs, Zeile 1: sensibler Name"), "Lauf {lauf}: {text}");
        let jetzt = std::fs::read(&datei).unwrap();
        assert_eq!(jetzt, vorher, "Lauf {lauf}: ein Lauf mit Fund schreibt nichts");
        assert!(!String::from_utf8_lossy(&jetzt).contains(&sha), "Lauf {lauf}: der Blob mit Fund steht nie im Cache");
    }
}

/// Jede Änderung an dem, wovon die Inhaltsprüfung abhängt (Listen, repos.toml,
/// Prüfer, Laufzeit.ps1, Art, Ausnahmedatei), gibt einen neuen Regel-Schlüssel:
/// kein Treffer. Dieselben Dateien an anderem Ort treffen (nur der Inhalt zählt).
/// Das Aufräumen behält zwei Regel-Ordner.
#[test]
fn cache_neuer_regel_schluessel() {
    if !bereit() {
        return;
    }
    let o = Ort::neu();
    let code = o.wurzel.join("code");
    prueferdateien_kopieren(&code);
    let args = ["-Baum", "main", "-Commits", "main", "-AufMain", "-MitCache"];
    let lauf = |extra: &[&str]| o.lauf_mit(&code.join("Vor-Push.ps1"), &o.cache(), &[&args[..], extra].concat(), None);
    let geprueft = cache_fuellen(&o);
    let (c, text) = lauf(&[]);
    assert!(c == 0 && cache_zahlen(&text) == Some((geprueft, 0)), "dieselben Dateien an anderem Ort treffen: {text}");
    let pruefer = o.wurzel.join("inhalt").join("pruefer");
    // Je Fall eine Zeile an die Datei anhängen.
    for (name, datei, zeile) in [
        ("namen.toml", pruefer.join("namen.toml"), "# geändert\n"),
        ("tokens", pruefer.join("tokens").join("erfunden.toml"), "# geändert\n"),
        ("repos.toml", pruefer.join("repos.toml"), "# geändert\n"),
        ("Vor-Push.ps1", code.join("Vor-Push.ps1"), "\r\n# geändert\r\n"),
        ("Laufzeit.ps1", code.join("Laufzeit.ps1"), "\r\n# geändert\r\n"),
    ] {
        let mut t = std::fs::read(&datei).unwrap();
        t.extend_from_slice(zeile.as_bytes());
        std::fs::write(&datei, t).unwrap();
        let (c, text) = lauf(&[]);
        assert!(c == 0 && cache_zahlen(&text) == Some((0, geprueft)), "{name}: neuer Schlüssel, kein Treffer: {text}");
        let (c, text) = lauf(&[]);
        assert!(c == 0 && cache_zahlen(&text) == Some((geprueft, 0)), "{name}: der neue Schlüssel trifft beim zweiten Lauf: {text}");
        assert!(regel_ordner(&o.cache()).len() <= 2, "{name}: das Aufräumen behält zwei: {:?}", regel_ordner(&o.cache()));
    }
    // Eine andere Art (Inhalt aus der Adresse) ist ein anderer Schlüssel.
    let (c, text) = lauf(&["-RemoteUrl", INHALT_ADRESSE]);
    assert!(c == 0 && text.contains("Geprüft als Inhalts-Repo") && cache_zahlen(&text) == Some((0, geprueft)), "Art: {text}");

    // Im Projekt-Repo: eine Ausnahmedatei (sie gilt nur dort) ist Teil des Schlüssels.
    let p = Ort::projekt();
    let proj = |p: &Ort| p.lauf(&["-Baum", "main", "-Commits", "main", "-AufMain", "-MitCache", "-RemoteUrl", PROJEKT_ADRESSE], None);
    let (c, text) = proj(&p);
    let (_, g) = cache_zahlen(&text).unwrap_or_default();
    assert!(c == 0 && g > 0, "{text}");
    let (c, text) = proj(&p);
    assert!(c == 0 && cache_zahlen(&text) == Some((g, 0)), "{text}");
    p.ausnahmen("projekt", "erlaubt = [\"geheimprojekt\"]\n");
    let (c, text) = proj(&p);
    assert!(c == 0 && text.contains("Ausnahmen für dieses Repo: 1") && cache_zahlen(&text) == Some((0, g)), "Ausnahmedatei: {text}");
}

/// Derselbe Blob an einem anderen Pfad wird neu geprüft: Was unter
/// tests/beispiel*/ erlaubt ist (projects/<allgemeiner slug>), ist unter src/ ein Fund.
#[test]
fn cache_derselbe_blob_an_anderem_pfad() {
    if !bereit() {
        return;
    }
    let o = Ort::neu();
    cache_fuellen(&o);
    o.ueber_merge(|o| o.datei("src/notiz.txt", b"projects/alpha ist hier erlaubt.\n"));
    assert_eq!(o.git(&["rev-parse", "main:src/notiz.txt"]), o.git(&["rev-parse", "main:tests/beispiel/projects/alpha/notiz.txt"]));
    let (c, text) = o.mit_cache();
    assert!(c == 1 && text.contains("ABGEWIESEN src/notiz.txt, Zeile 1: Projektname als Pfad"), "{text}");
    assert_eq!(cache_zahlen(&text).map(|z| z.1), Some(1), "genau der neue Pfad wird geprüft: {text}");
}

/// MUSS (Architektur): Ein vergifteter Eintrag („sauber“ für einen Blob mit
/// Fund, von Hand in den richtigen Regel-Ordner geschrieben) täuscht die
/// Vorprüfung, nie den Hook: Er prüft kalt, auch mit -MitCache.
#[test]
fn cache_vergiftet_der_hook_findet_trotzdem() {
    if !bereit() {
        return;
    }
    let o = Ort::neu();
    cache_fuellen(&o);
    o.ueber_merge(|o| o.datei("src/gift.rs", b"// geheimprojekt\n"));
    let sha = o.git(&["rev-parse", "main:src/gift.rs"]);
    let datei = cache_datei(&o);
    let mut t = std::fs::read_to_string(&datei).unwrap();
    t.push_str(&format!("{sha}\tsrc/gift.rs\n"));
    std::fs::write(&datei, t).unwrap();
    let (c, text) = o.mit_cache();
    assert_eq!(c, 0, "die Vorprüfung übernimmt den Eintrag (darum ist sie nie die letzte Schranke): {text}");
    let (c, text) = o.hook_mit(BASIS_ADRESSE);
    assert!(c == 1 && text.contains("ABGEWIESEN src/gift.rs, Zeile 1: sensibler Name") && text.contains("Inhalt kalt:"), "Hook: {text}");
    let kopf = o.git(&["rev-parse", "HEAD"]);
    let (c, text) = o
        .lauf(&["-Hook", "-RemoteName", "origin", "-RemoteUrl", BASIS_ADRESSE, "-MitCache"], Some(&format!("refs/heads/main {kopf} refs/heads/main {NULL}\n")));
    assert!(
        c == 1 && text.contains("ABGEWIESEN src/gift.rs, Zeile 1: sensibler Name") && text.contains("HINWEIS -MitCache gilt nicht im Hook"),
        "Hook mit -MitCache: {text}"
    );
}

/// Ein kaputter Cache ist nie ein Fehler des Laufs: kein UTF-8 heißt kein
/// Treffer (kalt geprüft, danach neu geschrieben); eine Zeile anderer Form trifft nicht.
#[test]
fn cache_kaputt_ist_kein_treffer() {
    if !bereit() {
        return;
    }
    let o = Ort::neu();
    let geprueft = cache_fuellen(&o);
    let datei = cache_datei(&o);
    let gut = std::fs::read_to_string(&datei).unwrap();
    let mut zeilen: Vec<&str> = gut.lines().collect();
    zeilen.sort_unstable();
    std::fs::write(&datei, b"\xff\xfe kaputt\n").unwrap();
    let (c, text) = o.mit_cache();
    assert!(c == 0 && cache_zahlen(&text) == Some((0, geprueft)) && !text.contains("HINWEIS"), "{text}");
    let neu = std::fs::read_to_string(&datei).unwrap();
    let mut neu_zeilen: Vec<&str> = neu.lines().collect();
    neu_zeilen.sort_unstable();
    assert_eq!(neu_zeilen, zeilen, "nach dem kalten Lauf wieder geschrieben");
    // Die Zeile von src/main.rs in anderer Form (SHA groß) und Unsinn: nur diese Zeile trifft nicht.
    let sha = o.git(&["rev-parse", "main:src/main.rs"]);
    let kaputt = gut.replace(&format!("{sha}\t"), &format!("{}\t", sha.to_uppercase())) + "unsinn\tsrc/main.rs\n\n";
    std::fs::write(&datei, kaputt).unwrap();
    let (c, text) = o.mit_cache();
    assert!(c == 0 && cache_zahlen(&text) == Some((geprueft - 1, 1)), "{text}");
}

/// Eine Junction ohne Administrator (mklink /J); `false`, wenn es nicht ging.
fn junction(link: &Path, ziel: &Path) -> bool {
    let Ok(mut c) = programme::befehl(Werkzeug::Cmd) else { return false };
    c.args(["/c", "mklink", "/J"]).arg(link).arg(ziel).output().is_ok_and(|o| o.status.success()) && link.exists()
}

/// Eine Verknüpfung im Pfad des Caches (als Ort selbst oder darüber): kein
/// Cache, ein Satz, kalt geprüft, nichts geschrieben.
#[test]
fn cache_nie_ueber_eine_verknuepfung() {
    if !bereit() {
        return;
    }
    let o = Ort::neu();
    let ziel = o.wurzel.join("ziel");
    std::fs::create_dir_all(&ziel).unwrap();
    let link = o.wurzel.join("verweis");
    if !junction(&link, &ziel) {
        dashboard::ueberspringen("mklink /J ging nicht");
        return;
    }
    for ort in [link.clone(), link.join("cache")] {
        let (c, text) = o.lauf_mit(&skript(), &ort, &["-Baum", "main", "-Commits", "main", "-AufMain", "-MitCache"], None);
        assert!(c == 0 && text.contains("HINWEIS Im Pfad des Prüfer-Caches liegt eine Verknüpfung") && text.contains("Inhalt kalt:"), "{ort:?}: {text}");
        assert!(std::fs::read_dir(&ziel).unwrap().next().is_none(), "{ort:?}: nichts geschrieben");
    }
}

/// Die Namen der Einträge eines Ordners, sortiert.
fn namen_in(ordner: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(ordner).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    v.sort();
    v
}

/// Datiert einen Ordner um `stunden` in die Zukunft (eine zurückgestellte Uhr, ein kopierter Cache).
fn in_die_zukunft(ordner: &Path, stunden: u32) {
    let befehl = format!("[IO.Directory]::SetLastWriteTimeUtc('{}', [DateTime]::UtcNow.AddHours({stunden}))", ordner.display());
    let o = pwsh().unwrap().args(["-NoProfile", "-NonInteractive", "-Command", &befehl]).output().unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
}

/// Das Aufräumen behält zwei Regel-Ordner: den eigenen (nie gelöscht, auch wenn
/// andere jünger datiert sind) und den zuletzt genutzten anderen. Was nicht wie
/// ein Regel-Ordner heißt oder mehr als Cache-Dateien enthält, bleibt.
#[test]
fn cache_aufraeumen_behaelt_zwei() {
    if !bereit() {
        return;
    }
    let o = Ort::neu();
    for z in ["a", "b", "c"] {
        std::fs::create_dir_all(o.cache().join(z.repeat(64))).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    std::fs::create_dir_all(o.cache().join("fremd")).unwrap();
    let geprueft = cache_fuellen(&o);
    let namen = namen_in(&o.cache());
    assert_eq!(namen.len(), 3, "{namen:?}");
    assert!(namen.contains(&"c".repeat(64)) && namen.contains(&"fremd".to_string()), "{namen:?}");
    let eigen = namen.iter().find(|n| n.len() == 64 && **n != "c".repeat(64)).unwrap().clone();
    // Drei jünger datierte Ordner; f enthält eine fremde Datei und gehört darum nicht dem Cache.
    for (z, stunden, fremd) in [("d", 1, false), ("e", 2, false), ("f", 3, true)] {
        let ordner = o.cache().join(z.repeat(64));
        std::fs::create_dir_all(&ordner).unwrap();
        std::fs::write(ordner.join("sauber.txt"), "").unwrap();
        if fremd {
            std::fs::write(ordner.join("fremd.txt"), "nicht vom Cache\n").unwrap();
        }
        in_die_zukunft(&ordner, stunden);
    }
    let (c, text) = o.mit_cache();
    assert!(c == 0 && cache_zahlen(&text) == Some((geprueft, 0)), "der eigene Ordner bleibt und trifft: {text}");
    let mut erwartet = vec![eigen, "e".repeat(64), "f".repeat(64), "fremd".to_string()];
    erwartet.sort();
    assert_eq!(namen_in(&o.cache()), erwartet);
    assert!(o.cache().join("f".repeat(64)).join("fremd.txt").exists());
}

/// Die Pfadregeln laufen auch für Treffer. Im Projekt-Repo steht der Slug nicht
/// im Regel-Schlüssel: Hat das ferne main projects/beta, meldet der warme Lauf
/// die gemerkten Pfade unter projects/alpha wie der kalte als fremdes Projekt.
#[test]
fn cache_pfadregeln_auch_bei_treffer() {
    if !bereit() {
        return;
    }
    let p = Ort::projekt();
    let (c, text) = p.lauf(&["-Baum", "main", "-Commits", "main", "-AufMain", "-MitCache", "-RemoteUrl", PROJEKT_ADRESSE], None);
    assert!(c == 0 && cache_zahlen(&text).is_some_and(|(_, g)| g > 0), "{text}");
    p.git(&["checkout", "-q", "--orphan", "fern"]);
    p.git(&["rm", "-rqf", "."]);
    p.datei("projects/beta/projekt.toml", b"schema = 3\n");
    p.commit("fern", &[]);
    p.git(&["checkout", "-q", "main"]);
    let bereich = ["-Baum", "main", "-Commits", "fern..main", "-RemoteUrl", PROJEKT_ADRESSE];
    let (ck, kalt) = p.lauf(&bereich, None);
    assert!(ck == 1 && kalt.contains("fremdes Projekt") && kalt.contains("Inhalt kalt:"), "{kalt}");
    let (cw, warm) = p.lauf(&[&bereich[..], &["-MitCache"]].concat(), None);
    assert!(cw == 1 && cache_zahlen(&warm).is_some_and(|(n, _)| n > 0), "der warme Lauf trifft: {warm}");
    assert_eq!(fundzeilen(&kalt), fundzeilen(&warm), "kalt:\n{kalt}\nwarm:\n{warm}");
}

/// Beim Schreiben bleiben die schon gemerkten Einträge: Nach einer Änderung an
/// main trifft der übernächste Lauf wieder alles.
#[test]
fn cache_behaelt_alte_eintraege() {
    if !bereit() {
        return;
    }
    let o = Ort::neu();
    cache_fuellen(&o);
    o.ueber_merge(|o| o.datei("src/neu.rs", b"// sauber\n"));
    let (c, text) = o.mit_cache();
    let (n, m) = cache_zahlen(&text).unwrap_or_default();
    assert!(c == 0 && n > 0 && m > 0, "{text}");
    let (c, text) = o.mit_cache();
    assert!(c == 0 && cache_zahlen(&text) == Some((n + m, 0)), "alte und neue Einträge treffen: {text}");
}

/// Scheitert Laufzeit.ps1, fehlt nur die Zeile zur Laufzeit; die Prüfung geht
/// wie sonst aus (kein Exit 2), mit und ohne Cache.
#[test]
fn laufzeit_kaputt_aendert_nichts() {
    if !bereit() {
        return;
    }
    let o = Ort::neu();
    let code = o.wurzel.join("code");
    prueferdateien_kopieren(&code);
    std::fs::write(code.join("Laufzeit.ps1"), "throw 'kaputt'\r\n").unwrap();
    for extra in [&[][..], &["-MitCache"][..]] {
        let (c, text) = o.lauf_mit(&code.join("Vor-Push.ps1"), &o.cache(), &[&["-Baum", "main", "-Commits", "main", "-AufMain"][..], extra].concat(), None);
        assert!(c == 0 && text.contains("Nichts gefunden.") && text.contains("HINWEIS Laufzeit.ps1 ließ sich nicht laden"), "{extra:?}: {text}");
        assert!(!text.lines().any(|z| z.starts_with("Laufzeit ")), "{extra:?}: {text}");
    }
}

/// Ein Hook oder Prüfer des Forks: schreibt beim Lauf `hook-lief.txt` in den
/// aktuellen Ordner (bei Hooks die Wurzel des Arbeitsbaums).
const FORK_HOOK: &str = "#!/bin/sh\necho lief > hook-lief.txt\n";

/// Die Marke des eigenen post-checkout von main.
const MAIN_MARKE: &str = "main-hook-lief.txt";
const MAIN_HOOK: &str = "#!/bin/sh\necho lief > main-hook-lief.txt\n";

/// Der Probe-Merge nach Abschnitt 15, Teil 5: ohne Arbeitsbaum und ohne Hooks
/// (git merge-tree --write-tree, git commit-tree als Ranke, git branch probe), geprüft
/// mit dem Prüfer aus main (hier dem des Code-Repos) und -Beitrag. Ein Fork, der
/// scripts/hooks/* und scripts/Vor-Push.ps1 ändert, führt dabei nichts aus: keine
/// Marker-Datei, der Arbeitsbaum bleibt auf main mit dessen Prüfer, und der Prüfer
/// meldet jeden dieser Prüfpfade. Ein sauberer Fork geht durch; erst dann entsteht
/// der Arbeitsbaum zum Bauen, auf dem Probe-Commit, ohne Hook des Forks, und main kommt
/// mit git merge --ff-only probe genau auf den geprüften Merge-Commit. Gegenproben: Der
/// alte Weg (git merge im Arbeitsbaum) ruft die Hooks des Forks und legt dessen Prüfer
/// in den Arbeitsbaum; ein Checkout der Kennung vor dem grünen Lauf ruft post-checkout
/// des Forks. Die Befehle kommen aus dem Text des Leitfadens (include_str!), und main hat
/// einen eigenen post-checkout mit eigener Marke, die der Arbeitsbaum der Probe schreibt.
#[test]
fn probe_merge_ohne_arbeitsbaum_nach_leitfaden() {
    if !bereit() {
        return;
    }
    // Die Befehle dieses Tests sind die des Leitfadens (Abschnitt 15, Teil 5), mit dessen
    // Platzhaltern; der Test setzt sie ein und prüft, dass der Text sie wörtlich nennt.
    let leitfaden = include_str!("../docs/anleitung/contributing-15-15-im-team-an-einem-projekt-repo.toml");
    const K: &str = "<kennung>";
    let holen = ["rev-parse", "FETCH_HEAD"];
    let merge_tree = ["merge-tree", "--write-tree", "main", K];
    let commit_tree = ["commit-tree", "$t", "-p", "main", "-p", K, "-m"];
    let branch = ["branch", "probe", "$m"];
    let pruefen = ["-Baum", "probe", "-Commits", "main..probe", "-AufMain", "-Beitrag", K];
    let bauen = ["worktree", "add", "--detach", "<ordner>", "probe"];
    let vorspulen = ["merge", "--ff-only", "probe"];
    for (vorn, befehl) in [
        ("git ", &holen[..]),
        ("git ", &merge_tree),
        // Davor stehen im Leitfaden die -c für Name und Adresse der Betreuer.
        ("", &commit_tree),
        ("git ", &branch),
        ("scripts/Vor-Push.ps1 ", &pruefen),
        ("git ", &bauen),
        ("git ", &vorspulen),
    ] {
        let text = format!("{vorn}{}", befehl.join(" "));
        assert!(leitfaden.contains(&text), "der Leitfaden nennt „{text}“ nicht mehr");
    }
    assert!(!leitfaden.contains(&format!("git merge --no-ff {K}")), "der Leitfaden fällt auf git merge im Arbeitsbaum zurück");
    // Die Schutzangaben um die Befehle: feste Commit-Angabe, Fetch ohne Ziel-Ref, sauberer
    // Arbeitsbaum auf main, -Beitrag Pflicht.
    for satz in [
        format!("git -c user.name=<name der betreuer> -c user.email=<adresse der betreuer> {}", commit_tree.join(" ")),
        "git fetch <fork-url> <ref>, dann".to_string(),
        "ohne „:“".to_string(),
        "mit main ausgecheckt und sauberem Arbeitsbaum".to_string(),
        "Ein Lauf ohne -Beitrag zählt nicht als grün.".to_string(),
    ] {
        assert!(leitfaden.contains(&satz), "der Leitfaden nennt „{satz}“ nicht mehr");
    }
    assert!(!leitfaden.contains("<ref>:"), "der Leitfaden holt mit Ziel-Ref (überschriebe eine lokale Ref)");
    // Die Regeln für Mitwirkende sagen, was der Prüfer durchsetzt: jeder Commit zählt, CHANGELOG.txt nur von den Betreuern.
    // Mit den ** der Regel selbst: Der Querverweis in Ablauf Punkt 3 nennt den Satz ohne sie.
    let regeln = include_str!("../CONTRIBUTING.md");
    for satz in ["liest jeden Commit", "**CHANGELOG.txt ändern nur die Betreuer.**", "neuen Zweig von `main` mit dem bereinigten Stand"] {
        assert!(regeln.contains(satz), "CONTRIBUTING.md nennt „{satz}“ nicht mehr");
    }
    let committen = include_str!("../docs/anleitung/contributing-05-5-committen.toml");
    assert!(committen.contains("Ein Beitrag aus einem Fork ändert CHANGELOG.txt nicht"), "Abschnitt 5 nennt die Changelog-Regel für Beiträge nicht mehr");
    assert!(leitfaden.contains("hilft kein weiterer Commit"), "der Leitfaden sagt nicht mehr, dass nach einem Fund im Commit kein weiterer Commit hilft");
    assert!(leitfaden.contains("neuen Zweig von main mit dem bereinigten Stand"), "der Leitfaden nennt den neuen Zweig nach einem Fund im Commit nicht mehr");
    // Im Projekt-Repo mit dessen Adresse: ohne -RemoteUrl prüfte der Prüfer als Basis-Repo.
    let projekt =
        format!("scripts/Vor-Push.ps1 -Repo <projekt-repo> -RemoteUrl <adresse des projekt-repos, wie in repos.toml> -Art Projekt {}", pruefen.join(" "));
    assert!(leitfaden.contains(&projekt), "der Leitfaden nennt den Probelauf für ein Projekt-Repo nicht mehr");
    // Platzhalter einsetzen; ergibt die Argumente für git oder den Prüfer.
    fn setzen(befehl: &[&str], werte: &[(&str, &str)]) -> Vec<String> {
        befehl.iter().map(|a| werte.iter().find(|(p, _)| p == a).map_or(a.to_string(), |(_, w)| w.to_string())).collect()
    }
    fn als_str(v: &[String]) -> Vec<&str> {
        v.iter().map(String::as_str).collect()
    }
    let mail = NOREPLY.replace('§', "");
    let ranke = ["-c", "user.name=Ranke", "-c", "user.email=ranke2@proton.me"];
    for fall in ["sauber", "Fork ändert Hooks und Prüfer", "alter Weg"] {
        let o = Ort::neu();
        // main hat eigene Hooks (relativ eingehängt wie im Code-Repo) und einen eigenen Prüfer;
        // sein post-checkout schreibt eine eigene Marke (so ist „kein Hook des Forks“ nicht leer).
        o.ueber_merge(|o| {
            o.datei("scripts/hooks/post-merge", b"#!/bin/sh\nexit 0\n");
            o.datei("scripts/hooks/post-checkout", MAIN_HOOK.as_bytes());
            o.datei("scripts/Vor-Push.ps1", b"# Pruefer von main\n");
        });
        o.git(&["config", "core.hooksPath", "scripts/hooks"]);
        let alt = o.git(&["rev-parse", "main"]);
        let fork = o.wurzel.join("fork");
        let aus = git().arg("clone").arg("-q").arg(o.repo()).arg(&fork).output().unwrap();
        assert!(aus.status.success(), "{}", String::from_utf8_lossy(&aus.stderr));
        let fg = |args: &[&str]| -> String {
            let aus = git().arg("-C").arg(&fork).args(args).output().unwrap();
            assert!(aus.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&aus.stderr));
            String::from_utf8_lossy(&aus.stdout).trim().to_string()
        };
        fg(&["config", "user.name", "Beitrag"]);
        fg(&["config", "user.email", &mail]);
        fg(&["config", "commit.gpgsign", "false"]);
        fg(&["switch", "-q", "-c", "beitrag"]);
        std::fs::write(fork.join("src").join("b.rs"), "// b\n").unwrap();
        if fall != "sauber" {
            for hook in ["post-merge", "pre-merge-commit", "commit-msg", "post-checkout", "reference-transaction"] {
                std::fs::write(fork.join("scripts").join("hooks").join(hook), FORK_HOOK).unwrap();
            }
            std::fs::write(fork.join("scripts").join("Vor-Push.ps1"), "# Pruefer des Forks\nexit 0\n").unwrap();
        }
        fg(&["add", "-A"]);
        fg(&["commit", "-q", "-m", "feat: b"]);
        o.git(&["fetch", "-q", &fork.to_string_lossy(), "beitrag"]);
        let kennung = o.git(&holen);
        let marker = o.repo().join("hook-lief.txt");
        let pruefer_im_baum = || std::fs::read_to_string(o.repo().join("scripts").join("Vor-Push.ps1")).unwrap();
        if fall == "alter Weg" {
            o.git(&["switch", "-q", "-c", "probe", "main"]);
            o.git(&[&ranke[..], &["merge", "-q", "--no-ff", &kennung, "-m", "chore(merge): beitrag nach main"]].concat());
            assert!(marker.exists(), "der alte Weg ruft die Hooks des Forks");
            assert!(pruefer_im_baum().contains("des Forks"), "der alte Weg legt den Prüfer des Forks in den Arbeitsbaum");
            continue;
        }
        let baum = o.git(&als_str(&setzen(&merge_tree, &[(K, &kennung)]))).lines().next().unwrap().to_string();
        let commit_tree = setzen(&commit_tree, &[("$t", &baum), (K, &kennung)]);
        let probe = o.git(&[&ranke[..], &als_str(&commit_tree), &["chore(merge): beitrag nach main"]].concat());
        o.git(&als_str(&setzen(&branch, &[("$m", &probe)])));
        assert!(!marker.exists(), "{fall}: beim Bau der Probe läuft kein Hook");
        assert_eq!(o.git(&["rev-parse", "HEAD"]), alt, "{fall}: der Arbeitsbaum bleibt auf main");
        assert!(pruefer_im_baum().contains("von main"), "{fall}: im Arbeitsbaum liegt der Prüfer von main");
        let (code, text) = o.lauf(&als_str(&setzen(&pruefen, &[(K, &kennung)])), None);
        assert!(!marker.exists(), "{fall}: auch der Prüflauf führt nichts aus dem Fork aus");
        // Bauen und Testen in einem eigenen Arbeitsbaum: nach grünem Lauf auf dem Probe-Commit.
        let bau = o.wurzel.join("probe-bau");
        let bau_marker = bau.join("hook-lief.txt");
        if fall == "sauber" {
            assert_eq!(code, 0, "{text}");
            o.git(&als_str(&setzen(&bauen, &[("<ordner>", &bau.to_string_lossy())])));
            assert!(!bau_marker.exists(), "nach grünem Lauf führt der Arbeitsbaum der Probe keinen Hook des Forks aus");
            assert!(bau.join(MAIN_MARKE).exists(), "der Arbeitsbaum der Probe ruft den post-checkout von main");
            o.git(&["worktree", "remove", "--force", &bau.to_string_lossy()]);
            o.git(&vorspulen);
            assert_eq!(o.git(&["rev-parse", "main"]), probe, "main ist genau der geprüfte Merge-Commit");
            let (code, text) = o.lauf(&["-Baum", "main", "-Commits", &format!("{alt}..main"), "-AufMain"], None);
            assert_eq!(code, 0, "nach dem Merge: {text}");
        } else {
            for pfad in [
                "scripts/hooks/post-merge",
                "scripts/hooks/pre-merge-commit",
                "scripts/hooks/commit-msg",
                "scripts/hooks/post-checkout",
                "scripts/hooks/reference-transaction",
                "scripts/Vor-Push.ps1",
            ] {
                assert!(code == 1 && text.contains(&format!("ein fremder Commit ändert den Prüfpfad {pfad}")), "{pfad}: {text}");
            }
            // Gegenprobe: Ein Checkout des Forks vor dem grünen Lauf (hier worktree add der
            // Kennung) ruft schon dessen Hooks auf (reference-transaction im neuen Arbeitsbaum).
            o.git(&["worktree", "add", "-q", "--detach", &bau.to_string_lossy(), &kennung]);
            assert!(bau_marker.exists(), "ein Checkout des Forks ruft dessen Hooks auf");
        }
    }
}

/// git über den vollen Pfad: Eine fremde git.exe (eine Kopie von hostname.exe) in der
/// Wurzel des Repos, die zugleich der aktuelle Ordner ist (wie beim Hook), läuft weder
/// im Prüfer noch in Hochladen.ps1; beide prüfen wie sonst. Ohne die Umgebungsvariable
/// NoDefaultCurrentDirectoryInExePath, die die Suche im aktuellen Ordner abschaltete.
/// Gegenprobe: Ein Start über den Namen (ProcessStartInfo „git“) nimmt dort die fremde.
/// Steht „.“ im PATH, nimmt auch Get-Command die fremde: Dann brechen Prüfer und
/// Hochladen.ps1 ab (Exit 2, jeder nicht voll qualifizierte PATH-Eintrag), ebenso mit dem
/// Repo als vollem Pfad im PATH (Ortsvergleich). Dasselbe mit einer git.cmd, die eine
/// Marke schriebe; die Marke entsteht nie, außer in der Gegenprobe mit „& git“.
#[test]
fn kein_git_aus_dem_arbeitsordner() {
    if !bereit() {
        return;
    }
    let quelle = PathBuf::from(std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into())).join("System32").join("hostname.exe");
    if !quelle.is_file() && dashboard::ueberspringen("hostname.exe fehlt") {
        return;
    }
    let o = Ort::neu();
    let inhalt = o.wurzel.join("inhalt");
    std::fs::copy(&quelle, o.repo().join("git.exe")).unwrap();
    let skripte = o.repo().join("scripts");
    std::fs::create_dir_all(skripte.join("hooks")).unwrap();
    prueferdateien_kopieren(&skripte);
    std::fs::copy(skript().with_file_name("Hochladen.ps1"), skripte.join("Hochladen.ps1")).unwrap();
    for name in ["pre-push", "commit-msg"] {
        std::fs::copy(skript().with_file_name("hooks").join(name), skripte.join("hooks").join(name)).unwrap();
    }
    std::fs::write(o.repo().join(".git").join("info").join("exclude"), "/scripts/\n/git.exe\n").unwrap();
    o.git(&["config", "core.hooksPath", "scripts/hooks"]);
    let im_repo = |args: &[&str]| {
        let a = pwsh()
            .unwrap()
            .args(args)
            .current_dir(o.repo())
            .env("DASHBOARD_INHALT", &inhalt)
            .env("DASHBOARD_PRUEFER_CACHE", o.cache())
            .env_remove("NoDefaultCurrentDirectoryInExePath")
            .output()
            .unwrap();
        (a.status.code().unwrap_or(-1), format!("{}{}", String::from_utf8_lossy(&a.stdout), String::from_utf8_lossy(&a.stderr)))
    };
    let start = ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"];
    let vor_push = skripte.join("Vor-Push.ps1").to_string_lossy().to_string();
    let repo = o.repo().to_string_lossy().to_string();
    let (c, text) = im_repo(&[&start[..], &[&vor_push, "-Repo", &repo, "-Baum", "main", "-Commits", "main", "-AufMain"]].concat());
    assert!(c == 0 && text.contains("Nichts gefunden") && text.contains("Inhalt kalt:"), "Prüfer: {text}");
    let hochladen = skripte.join("Hochladen.ps1").to_string_lossy().to_string();
    // Zweimal: Die Vorprüfung füllt den eigenen Cache-Ordner und trifft ihn dann; Laufzeit.ps1 lädt jedes Mal.
    for lauf in 1..=2 {
        let (c, text) = im_repo(&[&start[..], &[&hochladen, "-Pruefen"]].concat());
        assert!(c == 0 && text.contains("main ginge so nach main"), "Hochladen {lauf}: {text}");
        assert!(cache_zahlen(&text).is_some_and(|(n, _)| (lauf == 1) == (n == 0)), "Hochladen {lauf}: {text}");
        assert!(text.matches("Laufzeit ").count() >= 2 && !text.contains("HINWEIS Laufzeit.ps1"), "Hochladen {lauf}: {text}");
    }
    let (_, text) = im_repo(&[
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        "$p = New-Object Diagnostics.ProcessStartInfo 'git', '--version'; $p.UseShellExecute = $false; $p.RedirectStandardOutput = $true; $x = [Diagnostics.Process]::Start($p); $aus = $x.StandardOutput.ReadToEnd(); $x.WaitForExit(); if ($aus -match 'git version') { 'echtes git' } else { 'fremdes git' }",
    ]);
    assert!(text.contains("fremdes git"), "Gegenprobe: ein Start über den Namen nimmt die git.exe im aktuellen Ordner:\n{text}");
    // Mit „.“ im PATH fände auch Get-Command die fremde git.exe: Der Prüfer bricht dann ab.
    let pfad = format!(".;{}", std::env::var("PATH").unwrap_or_default());
    let mit_punkt = |args: &[&str]| {
        let a = pwsh()
            .unwrap()
            .args(args)
            .current_dir(o.repo())
            .env("DASHBOARD_INHALT", &inhalt)
            .env("DASHBOARD_PRUEFER_CACHE", o.cache())
            .env("PATH", &pfad)
            .output()
            .unwrap();
        (a.status.code().unwrap_or(-1), format!("{}{}", String::from_utf8_lossy(&a.stdout), String::from_utf8_lossy(&a.stderr)))
    };
    let (_, text) = mit_punkt(&["-NoProfile", "-NonInteractive", "-Command", "(Get-Command git -CommandType Application | Select-Object -First 1).Source"]);
    assert!(
        text.trim().to_lowercase().replace('/', "\\").ends_with(&format!("{}\\git.exe", repo.to_lowercase().replace('/', "\\"))),
        "Gegenprobe: mit . im PATH nimmt Get-Command die fremde git.exe:\n{text}"
    );
    // Ohne Umlaut: Hochladen.ps1 schreibt in eine Pipe in der Codepage der Konsole.
    let relativ = "nicht voll qualifizierte";
    let (c, text) = mit_punkt(&[&start[..], &[&vor_push, "-Repo", &repo, "-Baum", "main", "-Commits", "main", "-AufMain"]].concat());
    assert!(c == 2 && text.contains(relativ), "Prüfer mit . im PATH: {text}");
    // Auch mit vollem Cache (die Hochladen-Läufe oben haben ihn gefüllt): der Abbruch kommt vor dem Cache.
    let (c, text) = mit_punkt(&[&start[..], &[&vor_push, "-Repo", &repo, "-Baum", "main", "-Commits", "main", "-AufMain", "-MitCache"]].concat());
    assert!(c == 2 && text.contains(relativ), "Prüfer mit . im PATH und -MitCache: {text}");
    // Hochladen bricht ab, bevor es git aus dem Arbeitsordner startet.
    let (c, text) = mit_punkt(&[&start[..], &[&hochladen, "-Pruefen"]].concat());
    assert!(c == 2 && text.contains(relativ) && !text.contains("ginge so"), "Hochladen mit . im PATH: {text}");
    // Das Repo mit vollem Pfad vorn im PATH: Der Ortsvergleich bricht ab.
    let mit_repo = |args: &[&str]| {
        let a = pwsh()
            .unwrap()
            .args(args)
            .current_dir(o.repo())
            .env("DASHBOARD_INHALT", &inhalt)
            .env("DASHBOARD_PRUEFER_CACHE", o.cache())
            .env("PATH", format!("{repo};{}", std::env::var("PATH").unwrap_or_default()))
            .output()
            .unwrap();
        (a.status.code().unwrap_or(-1), format!("{}{}", String::from_utf8_lossy(&a.stdout), String::from_utf8_lossy(&a.stderr)))
    };
    let (c, text) = mit_repo(&[&start[..], &[&vor_push, "-Repo", &repo, "-Baum", "main", "-Commits", "main", "-AufMain"]].concat());
    assert!(c == 2 && text.contains("git liegt im geprüften Repo oder im aktuellen Ordner"), "Prüfer mit dem Repo im PATH: {text}");
    let (c, text) = mit_repo(&[&start[..], &[&hochladen, "-Pruefen"]].concat());
    assert!(c == 2 && text.contains("git liegt im Repo oder im aktuellen Ordner") && !text.contains("ginge so"), "Hochladen mit dem Repo im PATH: {text}");
    // git.cmd statt git.exe: Sie schriebe eine Marke, die nie entstehen darf.
    std::fs::remove_file(o.repo().join("git.exe")).unwrap();
    let marke = o.wurzel.join("fremdes-git-lief.txt");
    std::fs::write(o.repo().join("git.cmd"), format!("@echo lief> \"{}\"\r\n@exit /b 0\r\n", marke.display())).unwrap();
    std::fs::write(o.repo().join(".git").join("info").join("exclude"), "/scripts/\n/git.cmd\n").unwrap();
    let (c, text) = mit_punkt(&[&start[..], &[&vor_push, "-Repo", &repo, "-Baum", "main", "-Commits", "main", "-AufMain"]].concat());
    assert!(c == 2 && text.contains(relativ) && !marke.exists(), "Prüfer mit git.cmd und . im PATH: {text}");
    let (c, text) = mit_punkt(&[&start[..], &[&hochladen, "-Pruefen"]].concat());
    assert!(c == 2 && text.contains(relativ) && !text.contains("ginge so") && !marke.exists(), "Hochladen mit git.cmd und . im PATH: {text}");
    // Ohne . im PATH gilt git.cmd nie: CreateProcess sucht im aktuellen Ordner nur git.exe.
    let (c, text) = im_repo(&[&start[..], &[&hochladen, "-Pruefen"]].concat());
    assert!(c == 0 && text.contains("main ginge so nach main") && !marke.exists(), "Hochladen mit git.cmd im Repo: {text}");
    // Gegenprobe: Mit . im PATH nimmt PowerShell die git.cmd, und sie schreibt die Marke.
    let (_, text) = mit_punkt(&["-NoProfile", "-NonInteractive", "-Command", "& git --version"]);
    assert!(marke.exists(), "Gegenprobe: mit . im PATH läuft die git.cmd im aktuellen Ordner:\n{text}");
    // pre-push sucht pwsh in sh über den Namen, im Arbeitsbaum als aktuellem Ordner: Ein
    // „pwsh“ dort schriebe eine Marke und ließe den Push durch. Mit . im PATH bricht der
    // Hook vorher ab. git startet aus der Wurzel des Tests (dort liegt keine git.cmd).
    std::fs::remove_file(o.repo().join("git.cmd")).unwrap();
    let pwsh_marke = o.wurzel.join("fremdes-pwsh-lief.txt");
    std::fs::write(o.repo().join("pwsh"), format!("#!/bin/sh\necho lief > '{}'\n", pwsh_marke.to_string_lossy().replace('\\', "/"))).unwrap();
    std::fs::write(o.repo().join(".git").join("info").join("exclude"), "/scripts/\n/pwsh\n").unwrap();
    let fern = o.wurzel.join("fern.git");
    assert!(git().args(["init", "-q", "--bare"]).arg(&fern).output().unwrap().status.success());
    let git_mit_punkt = |args: &[&str]| {
        let a = git().arg("-C").arg(o.repo()).args(args).current_dir(&o.wurzel).env("DASHBOARD_INHALT", &inhalt).env("PATH", &pfad).output().unwrap();
        (a.status.success(), format!("{}{}", String::from_utf8_lossy(&a.stdout), String::from_utf8_lossy(&a.stderr)))
    };
    let (ok, text) = git_mit_punkt(&["push", &fern.to_string_lossy().replace('\\', "/"), "main:main"]);
    assert!(!ok && text.contains("pre-push: PATH") && !pwsh_marke.exists(), "git push mit pwsh im Arbeitsbaum und . im PATH: {text}");
    // Gegenprobe: Ein Shell-Alias (sh in der Wurzel des Arbeitsbaums wie der Hook) nimmt das fremde pwsh.
    let (_, text) = git_mit_punkt(&["-c", "alias.p=!pwsh", "p"]);
    assert!(pwsh_marke.exists(), "Gegenprobe: mit . im PATH startet sh das pwsh im Arbeitsbaum:\n{text}");
}

/// -Beitrag: Ein Commit des Beitrags (main..<kennung>) mit dem Namen oder der Adresse
/// der Betreuer als Autor oder Committer ist ein Fund, auch in anderer Schreibung.
/// Gegenprobe: derselbe Beitrag mit noreply-Angabe geht durch.
#[test]
fn beitrag_mit_angabe_der_betreuer() {
    if !bereit() {
        return;
    }
    let mail = NOREPLY.replace('§', "");
    let gross = "Ranke2§@Proton.me".replace('§', "");
    let faelle = [
        ("Autor und Committer", vec![], Some("Autor des Beitrags trägt die Angabe der Betreuer")),
        ("nur Committer", vec![("GIT_AUTHOR_NAME", "Beitrag"), ("GIT_AUTHOR_EMAIL", &mail)], Some("Committer des Beitrags trägt die Angabe der Betreuer")),
        (
            "nur der Name, klein",
            vec![("GIT_AUTHOR_NAME", "ranke"), ("GIT_AUTHOR_EMAIL", &mail), ("GIT_COMMITTER_NAME", "Beitrag"), ("GIT_COMMITTER_EMAIL", &mail)],
            Some("Autor des Beitrags trägt die Angabe der Betreuer"),
        ),
        (
            "nur die Adresse, groß",
            vec![("GIT_AUTHOR_NAME", "Beitrag"), ("GIT_AUTHOR_EMAIL", &gross), ("GIT_COMMITTER_NAME", "Beitrag"), ("GIT_COMMITTER_EMAIL", &mail)],
            Some("Autor des Beitrags trägt die Angabe der Betreuer"),
        ),
        ("noreply", als_beitrag(&mail).to_vec(), None),
    ];
    for (fall, wer, erwartet) in faelle {
        let o = Ort::neu();
        o.git(&["checkout", "-q", "-b", "zweig"]);
        o.datei("src/b.rs", b"// b\n");
        let kennung = o.commit("feat: b", &wer);
        o.git(&["checkout", "-q", "main"]);
        let (code, text) = o.lauf(&["-Beitrag", &kennung], None);
        match erwartet {
            None => assert!(code == 0 && text.contains("Geprüft: 1 Commit(s) des Beitrags"), "{fall}: {text}"),
            Some(satz) => assert!(code == 1 && text.contains(satz), "{fall}: erwartet „{satz}“:\n{text}"),
        }
    }
}

/// Ein Fall: Name, erwarteter Prüfpfad im Fund, die Änderung.
type Aenderung = (&'static str, &'static str, fn(&Ort));

/// M1 für jede Art der Änderung: Löschen, Umbenennen (hinaus und hinein) und nur der
/// Modus (ausführbar) sind am fremden Commit ein Fund. Gegenprobe: dieselbe Änderung
/// von Ranke über einen eigenen Merge.
#[test]
fn pruefpfade_loeschen_umbenennen_modus() {
    if !bereit() {
        return;
    }
    let vorher = |o: &Ort| {
        o.ueber_merge(|o| {
            o.datei("scripts/x.ps1", b"# x\n");
            o.datei("src/y.ps1", b"# y\n");
        })
    };
    let faelle: [Aenderung; 4] = [
        ("Löschen", "scripts/x.ps1", |o| {
            o.git(&["rm", "-q", "scripts/x.ps1"]);
        }),
        ("Umbenennen hinaus", "scripts/x.ps1", |o| {
            o.git(&["mv", "scripts/x.ps1", "src/x.ps1"]);
        }),
        ("Umbenennen hinein", "scripts/y.ps1", |o| {
            o.git(&["mv", "src/y.ps1", "scripts/y.ps1"]);
        }),
        ("nur Modus", "scripts/x.ps1", |o| {
            o.git(&["update-index", "--chmod=+x", "scripts/x.ps1"]);
        }),
    ];
    for (fall, pfad, aendern) in faelle {
        abgewiesen(fall, &format!("ein fremder Commit ändert den Prüfpfad {pfad}"), |o| {
            vorher(o);
            beitrag(o, "fix: x", "Beitrag", NOREPLY, aendern);
        });
        frei(fall, |o| {
            vorher(o);
            o.ueber_merge(aendern);
        });
    }
}

/// Heimatpfade unter macOS und Linux in weiteren Formen (PATH-Liste, scp-Ziel,
/// Schalter -L und -I, JSON mit \/, WSL-Freigabe, /var/home, /export/home, mehrere
/// oder gemischte Trenner nach dem Ordner), je genau ein Fund; die
/// Windows-Formen mit / zählen nur einmal (nicht zusätzlich als macOS/Linux).
/// Gegenproben bleiben frei: /home und /users hinter einer Domäne, "/home" ohne Namen,
/// /usr/home/<x> (nach einem Buchstaben; FreeBSD gehört nicht zu den Zielsystemen) und
/// Platzhalter in den neuen Formen. Bewusste Grenze, nicht festgehalten: /home hinter einem
/// Host in URLs (ssh://…@host/home/…, …:8080/home/…) und MSYS2 mit \ (…\msys64\home\…);
/// echte Namen fängt dort die Liste benutzer.
#[test]
fn heimatpfade_weitere_formen_je_ein_fund() {
    if !bereit() {
        return;
    }
    for form in [
        "PATH=/usr/bin:/h§ome/zora/bin",
        "scp a.txt rechner:/h§ome/zora/x",
        "cc -L/h§ome/zora/lib",
        "cc -I/U§sers/zora/include",
        r#"{"p": "\/h§ome\/zora\/x"}"#,
        r#"{"p": "\/U§sers\/zora"}"#,
        r"\\wsl.localhost\Ubuntu\h§ome\zora\x",
        r"\\wsl$\Ubuntu\h§ome\zora",
        "//wsl.localhost/Ubuntu/h§ome/zora",
        "/h§ome/zora/x",
        "C:/U§sers/zora/dev",
        r"C:\/U§sers\/zora",
        "file:///C:/U§sers/zora",
        r"\\pc\c$/U§sers/zora",
        "/c/U§sers/zora",
        "/mnt/c/U§sers/zora",
        // Mehrere Trenner nach dem Laufwerk: nur die Windows-Form meldet.
        "C://U§sers/zora",
        "c$//U§sers/zora",
        "c%3a//U§sers/zora",
        r"\\pc\c$//U§sers/zora",
        // /home nach einem Laufwerk oder einem Host aus einem Buchstaben: die Windows-Form kennt nur Users.
        "a:/h§ome/zora",
        "C:/h§ome/zora",
        "PATH=/a:/h§ome/zora",
        "cc -isystem/h§ome/zora",
        // WSL-Freigabe in JSON: nur die WSL-Form meldet.
        r"\/\/wsl$\/U\/h§ome\/zora",
        // Mehrere oder gemischte Trenner nach dem Ordner, /var/home (Fedora Silverblue) und /export/home.
        "/h§ome//zora",
        r"/h§ome\zora",
        "/var/h§ome/zora",
        "/export/h§ome/zora",
        r#"{"p": "\/var\/h§ome\/zora"}"#,
    ] {
        let text = abgewiesen(form, "Benutzerpfad", |o| o.ueber_merge(|o| o.datei("src/pfad.rs", format!("// {form}\n").as_bytes())));
        assert_eq!(fundzeilen(&text).len(), 1, "{form}: genau ein Fund:\n{text}");
    }
    frei("Fehltreffer", |o| {
        o.ueber_merge(|o| {
            o.datei(
                "src/frei.rs",
                r#"// https://example.com/h§ome/zora https://api.github.com/u§sers/zora Path::new("/h§ome") "/h§ome/" /usr/h§ome/zora
// https:\/\/example.com\/h§ome\/zora foo-bar/h§ome/zora
// \\wsl$\Ubuntu\h§ome\<name> \/h§ome\/<name> -L/h§ome/<name> PATH=/usr/bin:/h§ome/$USER/bin
// /var/h§ome/<name> /h§ome//<name> /h§ome\<name> x/var/h§ome/zora /variable/h§ome/zora /var/h§omes/zora
"#
                .as_bytes(),
            )
        })
    });
}

/// Liest die Muster „Benutzerpfad…“ aus Vor-Push.ps1 (Zeilen `'name' = Rx ('…')`, der
/// Platzhalter eingesetzt), dazu das Muster aus $env:ALT, und misst jedes an langen Zeilen.
/// Ausgabe je Paar: `ZEIT|<muster>|<eingabe>|<ms>`.
const MUSTER_MESSEN: &str = r#"
$t = [IO.File]::ReadAllText($env:PRUEFER)
$opt = [Text.RegularExpressions.RegexOptions]'IgnoreCase, CultureInvariant'
$platz = [regex]::Match($t, "(?m)^\`$platzhalter = '(.*)'\r?$").Groups[1].Value.Replace("''", "'")
if (-not $platz) { throw 'Platzhalter nicht gefunden' }
# Nur damit ein exponentielles Muster den Test rot macht statt ihn hängen zu lassen.
$grenze = [TimeSpan]::FromSeconds(30)
$muster = [ordered]@{}
foreach ($m in [regex]::Matches($t, "(?m)^\s*'(Benutzerpfad[^']*)'\s*=\s*Rx \('(.*)'\)\r?$")) {
    $teile = $m.Groups[2].Value -split "' \+ \`$platzhalter \+ '"
    $muster[$m.Groups[1].Value] = [regex]::new((($teile | ForEach-Object { $_.Replace("''", "'") }) -join $platz), $opt, $grenze)
}
$a = [regex]::Match($t, "(?m)^\`$allgemeinPfad = Rx \('(.*)' \+ \(Escape-Liste \`$listen\.allgemein\) \+ '(.*)'\)\r?$")
if (-not $a.Success) { throw 'allgemeinPfad nicht gefunden' }
$muster['allgemeinPfad'] = [regex]::new($a.Groups[1].Value + 'probe' + $a.Groups[2].Value, $opt, $grenze)
$muster['alt'] = [regex]::new($env:ALT.Replace('PLATZ', $platz), $opt, $grenze)
$n = 20000
$h = '/h' + 'ome'
$eingaben = [ordered]@{
    'slash' = '/' * $n
    'gemischt' = '\/' * ($n / 2)
    'home' = ($h + '/') * ($n / 6)
    'home-trenner' = $h + ('/' * $n)
    'laufwerk' = 'c:' + ('/' * $n) + 'u'
    'wsl' = ('/' * $n) + 'wsl$/'
    'kodiert' = 'home' + ('%2f' * ($n / 3)) + '/'
    'var' = '/var' * ($n / 4)
    'laufwerk-rueck' = 'c:' + ('\' * $n) + 'x'
    'users-rueck' = 'c:\users' + ('\' * $n) + 'public'
    'projects-rueck' = 'projects' + ('\' * $n) + 'x'
}
foreach ($k in $muster.Keys) {
    foreach ($e in $eingaben.Keys) {
        if ($k -eq 'alt' -and $e -ne 'slash') { continue }
        $null = $muster[$k].Matches('/x').Count
        $uhr = [Diagnostics.Stopwatch]::StartNew()
        try { $null = $muster[$k].Matches($eingaben[$e]).Count } catch { if ($_.Exception.GetBaseException() -isnot [Text.RegularExpressions.RegexMatchTimeoutException]) { throw } }
        "ZEIT|$k|$e|$($uhr.ElapsedMilliseconds)"
    }
}
"#;

/// Das Muster „Benutzerpfad (macOS/Linux)“ vor der Behebung: Der Lookbehind über die
/// Trenner stand vor dem Literal und lief hinter jedem / zurück (quadratisch).
const ALTES_MAC_MUSTER: &str =
    r#"(?:(?<![a-z0-9]\\?)|(?<=(?<![a-z0-9])-[a-z]+))/(?:(?<!(?<![a-z0-9])[a-z](?::|\$|%3a)[\\/]*)[u]sers|[h]ome)\\?/(?!PLATZ)[^\\/\s"'<>*|:]+"#;

/// Die Heimatpfad-Muster laufen auf einer Zeile aus 20 000 Zeichen linear: Je Muster und
/// Eingabe (lauter /, \/ im Wechsel, viele Stücke /home und /, /home mit langen Trennern,
/// Laufwerk mit langen Trennern, WSL- und URL-kodierte Formen) unter 500 ms (gemessen etwa
/// 1 ms; großzügig gegen Last). Lange \-Läufe nach Laufwerk, Users und projects prüfen die
/// Windows-Form und $allgemeinPfad (mit (?:\\\\|\\|/)+ waren sie exponentiell). Gegenprobe:
/// das alte Muster braucht auf 20 000 / über 1 s (gemessen etwa 5 s).
#[test]
fn heimatpfad_muster_ohne_quadratische_laufzeit() {
    if !bereit() {
        return;
    }
    let aus = pwsh()
        .unwrap()
        .args(["-NoProfile", "-NonInteractive", "-Command", MUSTER_MESSEN])
        .env("PRUEFER", skript())
        .env("ALT", ALTES_MAC_MUSTER)
        .output()
        .unwrap();
    let text = format!("{}{}", String::from_utf8_lossy(&aus.stdout), String::from_utf8_lossy(&aus.stderr));
    assert!(aus.status.success(), "{text}");
    let mut namen = std::collections::BTreeSet::new();
    for zeile in text.lines().filter_map(|z| z.trim().strip_prefix("ZEIT|")) {
        let teile: Vec<&str> = zeile.split('|').collect();
        let (muster, eingabe, ms) = (teile[0], teile[1], teile[2].parse::<u64>().unwrap());
        if muster == "alt" {
            assert!(ms >= 1000, "Gegenprobe: das alte Muster läuft auf 20 000 / quadratisch, gemessen {ms} ms");
        } else {
            assert!(ms < 500, "{muster} auf {eingabe}: {ms} ms\n{text}");
        }
        namen.insert(muster.to_string());
    }
    for name in [
        "alt",
        "Benutzerpfad",
        "Benutzerpfad (Git-Bash-Form)",
        "Benutzerpfad (macOS/Linux)",
        "Benutzerpfad (WSL-Freigabe)",
        "Benutzerpfad (URL-kodiert)",
        "allgemeinPfad",
    ] {
        assert!(namen.contains(name), "{name} nicht gemessen:\n{text}");
    }
}

/// Ein Fall: Name, erwarteter Fund, die Änderung (gibt die Kennung für -Beitrag).
type CommitFall = (&'static str, &'static str, fn(&Ort) -> Option<String>);

/// Der Cache überspringt nur die Inhaltsprüfung von Blobs: Die Regel für Prüfpfade an
/// fremden Commits, die Autor-Regel auf der Linie und -Beitrag melden mit vollem Cache
/// (jeder Blob ein Treffer) dieselben Funde wie kalt.
#[test]
fn cache_ueberspringt_keine_commit_regel() {
    if !bereit() {
        return;
    }
    let faelle: [CommitFall; 3] = [
        ("Prüfpfad", "ein fremder Commit ändert den Prüfpfad scripts/x.ps1", |o| {
            beitrag(o, "fix: x", "Beitrag", NOREPLY, |o| {
                o.git(&["rm", "-q", "scripts/x.ps1"]);
            });
            None
        }),
        ("Autor auf der Linie", "ist nicht Ranke", |o| {
            o.commit("feat: leer", &als_beitrag(&NOREPLY.replace('§', "")));
            None
        }),
        ("-Beitrag", "Autor des Beitrags trägt die Angabe der Betreuer", |o| {
            o.git(&["checkout", "-q", "-b", "beitrag"]);
            let kennung = o.commit("feat: leer", &[]);
            o.git(&["checkout", "-q", "main"]);
            Some(kennung)
        }),
    ];
    for (fall, muster, aendern) in faelle {
        let o = Ort::neu();
        o.ueber_merge(|o| o.datei("scripts/x.ps1", b"# x\n"));
        cache_fuellen(&o);
        let kennung = aendern(&o);
        let mut args = vec!["-Baum", "main", "-Commits", "main", "-AufMain"];
        if let Some(k) = &kennung {
            args.extend(["-Beitrag", k.as_str()]);
        }
        let (ck, kalt) = o.lauf(&args, None);
        assert!(ck == 1 && kalt.contains(muster) && kalt.contains("Inhalt kalt:"), "{fall} kalt: {kalt}");
        args.push("-MitCache");
        let (cw, warm) = o.lauf(&args, None);
        assert!(cw == 1 && cache_zahlen(&warm).is_some_and(|(n, g)| n > 0 && g == 0), "{fall}: jeder Blob ein Treffer: {warm}");
        assert_eq!(fundzeilen(&kalt), fundzeilen(&warm), "{fall}, kalt:\n{kalt}\nwarm:\n{warm}");
    }
}
