//! Gegentests für `scripts/Freigeben.ps1`: die ganze Folge 0 bis 6 an einem
//! Wegwerf-Aufbau im Temp-Ordner. Ein kleines Code-Repo mit Kopien der Skripte
//! und Hooks, ein lokales bare Repo als origin, ein Inhalt mit erfundenen
//! Namenslisten und einer Zusammenbau-Kiste dashboard-lokal, ein Ersatz für
//! Dienst.ps1 und ein kleiner HTTP-Server für /api/betrieb. Nichts geht an ein
//! echtes Remote, kein echter Dienst startet neu. Die Prüfungen laufen
//! wirklich (cargo fmt, clippy, cargo test, npm run verify, pruefe als
//! Ersatzprogramm dashboard, Vor-Push über Hochladen.ps1).

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use dashboard::dienst::programme::{self, Werkzeug};

const KOPF: &str = "\u{feff}CHANGELOG\n=========\n\nKopf.\n\n";
const ALT: &str = "0.3.4 - 2026-01-02\n------------------\n\nHinzugefügt\n- Alt.\n";
const LISTE: &str = "# erfunden\nsensibel = [\"geheimwortfreigabe\"]\nallgemein = [\"projektfreigabe\"]\ntabu = [\"tabufreigabe\"]\nbenutzer = [\"nutzerfreigabe\"]\nrechner = [\"pc-freigabe-erfunden\"]\nwlan = [\"wlan-freigabe-erfunden\"]\nschluessel = [\"dienst-freigabe-erfunden\"]\nmac = []\nnur_basis = [\"basisfreigabe\"]\n";
const PRUEFE: &str = "fn main() {\n    let a: Vec<String> = std::env::args().skip(1).collect();\n    let ok = a.len() == 3 && a[1] == \"--system\";\n    if ok && std::path::Path::new(&a[2]).is_dir() {\n        println!(\"0 Fehler, 0 Hinweise\");\n    } else {\n        std::process::exit(2);\n    }\n}\n";
const SKRIPTE: [&str; 8] =
    ["Freigeben.ps1", "Fassung-Festschreiben.ps1", "Hochladen.ps1", "Vor-Push.ps1", "Anfrage.ps1", "hooks/pre-commit", "hooks/commit-msg", "hooks/pre-push"];

fn pwsh() -> Option<Command> {
    [Werkzeug::PowerShell7, Werkzeug::WindowsPowerShell].into_iter().find_map(|w| programme::befehl(w).ok())
}

fn bereit() -> bool {
    let fehlt = [Werkzeug::Git, Werkzeug::Node, Werkzeug::Cargo, Werkzeug::Cmd].into_iter().any(|w| programme::befehl(w).is_err());
    if pwsh().is_none() || fehlt || !Path::new(env!("CARGO_MANIFEST_DIR")).join("web/node_modules/prettier").is_dir() {
        return !dashboard::ueberspringen("PowerShell, git, Node, cargo, cmd oder Prettier in web/node_modules fehlen");
    }
    true
}

static ZAEHLER: AtomicUsize = AtomicUsize::new(0);

/// Wurzel mit `dashboard` (Code-Repo), `dashboard-inhalt`, `fern.git` und dem Dienst-Ersatz.
struct Probe(PathBuf);

impl Drop for Probe {
    fn drop(&mut self) {
        // Erst die Verknüpfung lösen, nie ihr Ziel.
        let _ = std::fs::remove_dir(self.repo().join("web").join("node_modules"));
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn git_in(wo: &Path, args: &[&str]) -> String {
    let a = programme::befehl(Werkzeug::Git).unwrap().arg("-C").arg(wo).args(args).output().unwrap();
    assert!(a.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&a.stderr));
    String::from_utf8_lossy(&a.stdout).trim().to_string()
}

fn schreiben(p: &Path, inhalt: &str) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, inhalt).unwrap();
}

fn cargo_lock(wo: &Path) {
    let a = programme::befehl(Werkzeug::Cargo).unwrap().current_dir(wo).args(["generate-lockfile", "--offline"]).output().unwrap();
    assert!(a.status.success(), "{}", String::from_utf8_lossy(&a.stderr));
}

/// Antwortet auf jede Anfrage als /api/betrieb mit diesem Körper; gibt den Port.
fn betrieb(koerper: &'static str) -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for s in l.incoming().flatten() {
            let mut s = s;
            let mut puffer = [0u8; 4096];
            let _ = s.read(&mut puffer);
            let _ = write!(s, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{koerper}", koerper.len());
        }
    });
    port
}

impl Probe {
    fn neu() -> Probe {
        let n = ZAEHLER.fetch_add(1, Ordering::SeqCst);
        let o = Probe(std::env::temp_dir().join(format!("dashboard-freigabe-{}-{n}", std::process::id())));
        let _ = std::fs::remove_dir_all(&o.0);
        std::fs::create_dir_all(o.repo()).unwrap();
        git_in(&o.0, &["init", "-q", "--bare", "-b", "main", "fern.git"]);

        let r = o.repo();
        let quelle = Path::new(env!("CARGO_MANIFEST_DIR"));
        for s in SKRIPTE {
            let ziel = r.join("scripts").join(s);
            std::fs::create_dir_all(ziel.parent().unwrap()).unwrap();
            std::fs::copy(quelle.join("scripts").join(s), ziel).unwrap();
        }
        schreiben(
            &r.join("Cargo.toml"),
            "[package]\nname = \"dashboard\"\nversion = \"0.3.4\"\nedition = \"2021\"\n\n[workspace]\nmembers = [\".\", \"crates/dashboard-projekt\"]\n\n[dependencies]\n",
        );
        schreiben(&r.join("src/lib.rs"), "//! Probe.\n");
        schreiben(&r.join("src/main.rs"), PRUEFE);
        schreiben(&r.join("crates/dashboard-projekt/Cargo.toml"), "[package]\nname = \"dashboard-projekt\"\nversion = \"0.1.0\"\nedition = \"2021\"\n");
        schreiben(&r.join("crates/dashboard-projekt/src/lib.rs"), "//! Probe.\n");
        schreiben(&r.join(".gitignore"), "/target\n/web/node_modules\n");
        schreiben(&r.join("docs/anleitung/a.toml"), "titel = \"A\"\n");
        schreiben(&r.join("docs/zielbild/a.toml"), "titel = \"A\"\n");
        schreiben(
            &r.join("web/package.json"),
            "{\n  \"name\": \"probe-web\",\n  \"private\": true,\n  \"version\": \"0.3.4\",\n  \"scripts\": {\n    \"verify\": \"node --version\"\n  }\n}\n",
        );
        schreiben(
            &r.join("web/package-lock.json"),
            "{\n  \"name\": \"probe-web\",\n  \"version\": \"0.3.4\",\n  \"lockfileVersion\": 3,\n  \"requires\": true,\n  \"packages\": {\n    \"\": {\n      \"name\": \"probe-web\",\n      \"version\": \"0.3.4\"\n    }\n  }\n}\n",
        );
        o.changelog("");
        cargo_lock(&r);
        git_in(&r, &["init", "-q", "-b", "main"]);
        for (k, v) in [("core.autocrlf", "false"), ("user.name", "Ranke"), ("user.email", "ranke2@proton.me")] {
            git_in(&r, &["config", k, v]);
        }
        o.commit("chore(release): v0.3.4");
        git_in(&r, &["tag", "v0.3.4"]);
        git_in(&r, &["remote", "add", "origin", o.0.join("fern.git").to_str().unwrap()]);
        git_in(&r, &["push", "-q", "origin", "main", "v0.3.4"]);
        git_in(&r, &["fetch", "-q", "origin"]);
        git_in(&r, &["config", "core.hooksPath", "scripts/hooks"]);
        // Prettier für den pre-commit-Hook im Release-Commit.
        let nm = r.join("web").join("node_modules");
        let a = programme::befehl(Werkzeug::Cmd).unwrap().args(["/c", "mklink", "/J"]).arg(&nm).arg(quelle.join("web").join("node_modules")).output().unwrap();
        assert!(a.status.success(), "{}", String::from_utf8_lossy(&a.stderr));

        let i = o.inhalt();
        schreiben(&i.join("pruefer/namen.toml"), LISTE);
        schreiben(&i.join("pruefer/repos.toml"), "[repos]\n");
        schreiben(&i.join("pruefer/tokens/erfunden.toml"), "sensibel = [\"tokenfreigabe\"]\n");
        schreiben(
            &i.join("dashboard-lokal/Cargo.toml"),
            "[package]\nname = \"dashboard-lokal\"\nversion = \"0.1.0\"\nedition = \"2021\"\npublish = false\n\n[workspace]\n\n[dependencies]\ndashboard = { path = \"../../dashboard\" }\n\n[dev-dependencies]\ndashboard-projekt = { path = \"../../dashboard/crates/dashboard-projekt\" }\n",
        );
        schreiben(&i.join("dashboard-lokal/src/main.rs"), "fn main() {}\n");
        cargo_lock(&i.join("dashboard-lokal"));
        git_in(&i, &["init", "-q", "-b", "main"]);
        for (k, v) in [("core.autocrlf", "false"), ("user.name", "Ranke"), ("user.email", "ranke2@proton.me")] {
            git_in(&i, &["config", k, v]);
        }
        git_in(&i, &["add", "-A"]);
        git_in(&i, &["commit", "-q", "-m", "chore: Inhalt"]);

        o.antwort("{\"version\":\"0.4.0\"}");
        o.dienst(0);
        o
    }

    /// /api/betrieb antwortet ab jetzt mit diesem Körper (ein neuer Ersatz-Server, dienst.toml zeigt auf ihn).
    fn antwort(&self, koerper: &'static str) {
        schreiben(&self.0.join("dienst.toml"), &format!("port = {}\n", betrieb(koerper)));
    }

    /// Bauordner je Probe: Gleichzeitige Proben derselben Kiste dashboard teilten sonst
    /// Testprogramme (etwa ein roter cargo test der einen im Lauf der anderen).
    /// Die laufende Nummer statt der Prozessnummer, damit die Ordner warm wiederkommen.
    fn ziel(&self) -> PathBuf {
        let n = self.0.to_string_lossy().rsplit('-').next().unwrap().to_string();
        std::env::var_os("CARGO_TARGET_DIR").map(PathBuf::from).unwrap_or_else(std::env::temp_dir).join(format!("freigabe-probe-{n}"))
    }

    fn repo(&self) -> PathBuf {
        self.0.join("dashboard")
    }

    fn inhalt(&self) -> PathBuf {
        // Nicht der Nachbarordner dashboard-inhalt: -Inhalt muss auch die Hooks beim Push erreichen.
        self.0.join("inhalt-anderswo")
    }

    fn fern(&self) -> PathBuf {
        self.0.join("fern.git")
    }

    fn git(&self, args: &[&str]) -> String {
        git_in(&self.repo(), args)
    }

    fn changelog(&self, unreleased: &str) {
        schreiben(&self.repo().join("CHANGELOG.txt"), &format!("{KOPF}Unreleased\n----------\n\n{unreleased}\n{ALT}"));
    }

    /// Commit ohne Hooks: die Vorbereitung darf auch Rotes einbringen.
    fn commit(&self, betreff: &str) {
        self.git(&["add", "-A"]);
        self.git(&["commit", "-q", "--no-verify", "--allow-empty", "-m", betreff]);
    }

    /// Ein Zweig mit einem Commit, per Merge-Commit auf main (nur Merges auf main).
    fn zweig(&self, betreff: &str, aendern: impl FnOnce(&Probe)) {
        self.git(&["checkout", "-q", "-b", "arbeit"]);
        aendern(self);
        self.commit(betreff);
        self.git(&["checkout", "-q", "main"]);
        self.git(&["merge", "-q", "--no-ff", "--no-verify", "-m", "chore(merge): arbeit nach main", "arbeit"]);
        self.git(&["branch", "-q", "-d", "arbeit"]);
    }

    /// Der Dienst-Ersatz: schreibt eine Zeile ins Protokoll und endet mit `code`.
    fn dienst(&self, code: i32) {
        let log = self.0.join("dienst.log");
        schreiben(
            &self.0.join("dienst.ps1"),
            &format!("param([switch]$Neustart)\nAdd-Content -LiteralPath '{}' -Value \"Neustart $Neustart\"\nexit {code}\n", log.display()),
        );
    }

    fn neustarts(&self) -> usize {
        std::fs::read_to_string(self.0.join("dienst.log")).map(|t| t.lines().filter(|l| *l == "Neustart True").count()).unwrap_or(0)
    }

    /// Alles, was eine Wirkung zeigen würde: main und Tags lokal, das ferne Repo, der Inhalt, der Dienst.
    fn stand(&self) -> String {
        format!(
            "{}\n{}\n{}\n{}\n{}",
            // refs/remotes nicht: git fetch darf sie nachziehen.
            self.git(&["for-each-ref", "--format=%(refname) %(objectname)", "refs/heads", "refs/tags"]),
            git_in(&self.fern(), &["for-each-ref", "--format=%(refname) %(objectname)"]),
            git_in(&self.inhalt(), &["rev-parse", "HEAD"]),
            git_in(&self.inhalt(), &["status", "--porcelain"]),
            self.neustarts()
        )
    }

    fn lauf(&self, pruefen: bool) -> (i32, String) {
        let mut c = pwsh().unwrap();
        c.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"])
            .arg(self.repo().join("scripts").join("Freigeben.ps1"))
            .arg("-Inhalt")
            .arg(self.inhalt())
            .arg("-Dienst")
            .arg(self.0.join("dienst.ps1"))
            .arg("-Einstellungen")
            .arg(self.0.join("dienst.toml"));
        if pruefen {
            c.arg("-Pruefen");
        }
        // Ohne DASHBOARD_INHALT: der Inhalt kommt nur über -Inhalt.
        c.env_remove("DASHBOARD_INHALT").env("CARGO_TARGET_DIR", self.ziel());
        let a = c.output().unwrap();
        (a.status.code().unwrap_or(-1), format!("{}{}", String::from_utf8_lossy(&a.stdout), String::from_utf8_lossy(&a.stderr)))
    }
}

/// Probe, dann ein Lauf, der am Push scheitert (lokal festgeschrieben, nichts
/// draußen), die Wiederaufnahme bis zum Ende und ein dritter Lauf mit rotem Dienst.
#[test]
fn folge_probe_und_wiederaufnahme() {
    if !bereit() {
        return;
    }
    let o = Probe::neu();
    o.zweig("feat(web): neu", |o| o.changelog("Hinzugefügt\n- Neu.\n"));
    let alt = o.git(&["rev-parse", "main"]);
    let lock_alt = git_in(&o.inhalt(), &["rev-parse", "HEAD"]);

    let vorher = o.stand();
    let (code, text) = o.lauf(true);
    assert_eq!(code, 0, "{text}");
    assert!(text.contains("Probe: Fassung 0.4.0 ginge so hinaus"), "{text}");
    for schritt in
        ["cargo fmt --check ...", "cargo clippy -D warnings ...", "cargo test ...", "npm run verify ...", "pruefe --system docs/zielbild ...", "Nur geprüft"]
    {
        assert!(text.contains(schritt), "{schritt} lief nicht:\n{text}");
    }
    assert_eq!(o.stand(), vorher, "-Pruefen ändert nichts");

    // Etwas Fremdes ist im Inhalt vorgemerkt, und der Lock-Commit scheitert am
    // pre-commit des Inhalts: lokal festgeschrieben, Cargo.lock offen, draußen nichts.
    schreiben(&o.inhalt().join("pruefer/fremd.toml"), "x = 1\n");
    git_in(&o.inhalt(), &["add", "pruefer/fremd.toml"]);
    let inhalt_haken = o.inhalt().join(".git").join("hooks").join("pre-commit");
    schreiben(&inhalt_haken, "#!/bin/sh\nexit 1\n");
    let fern_vorher = git_in(&o.fern(), &["for-each-ref", "--format=%(refname) %(objectname)"]);
    let (code, text) = o.lauf(false);
    assert_eq!(code, 1, "{text}");
    assert!(text.contains("Der Commit der Lock-Datei im Inhalt ging nicht durch"), "{text}");
    assert!(text.contains("main trägt Fassung 0.4.0 lokal (Merge und Tag). Es ging nichts hinaus."), "{text}");
    assert_eq!(git_in(&o.inhalt(), &["rev-parse", "HEAD"]), lock_alt);
    assert_eq!(git_in(&o.fern(), &["for-each-ref", "--format=%(refname) %(objectname)"]), fern_vorher, "draußen nichts");
    std::fs::remove_file(&inhalt_haken).unwrap();

    // -Pruefen auf der fertigen, nicht hochgeladenen Fassung sagt das, statt „kein Commit seit v0.4.0“.
    let vorher = o.stand();
    let (code, text) = o.lauf(true);
    assert_eq!(code, 0, "{text}");
    assert!(text.contains("Probe: Fassung 0.4.0 ist lokal fertig") && text.contains("Es ging nichts hinaus."), "{text}");
    assert_eq!(o.stand(), vorher, "-Pruefen ändert nichts");

    // Das ferne Repo weist jeden Push ab. Die offene Cargo.lock aus dem Lauf davor
    // hält nicht auf: Sie wird festgeschrieben, nur sie; das Fremde bleibt vorgemerkt.
    let haken = o.fern().join("hooks").join("pre-receive");
    schreiben(&haken, "#!/bin/sh\nexit 1\n");
    let (code, text) = o.lauf(false);
    assert_eq!(code, 1, "{text}");
    assert!(text.contains("main ging nicht hinaus"), "{text}");
    assert_eq!(git_in(&o.fern(), &["for-each-ref", "--format=%(refname) %(objectname)"]), fern_vorher, "draußen nichts");
    assert_eq!(o.neustarts(), 0);
    let kopf = o.git(&["rev-parse", "main"]);
    assert_eq!(o.git(&["rev-parse", "v0.4.0"]), kopf);
    assert_eq!(o.git(&["rev-parse", "main^1"]), alt);
    assert_eq!(o.git(&["log", "-1", "--format=%s", "main"]), "chore(release): v0.4.0");
    assert_eq!(git_in(&o.inhalt(), &["rev-parse", "HEAD^"]), lock_alt, "genau ein Commit im Inhalt");
    assert_eq!(
        git_in(&o.inhalt(), &["show", "--name-only", "--format=%s", "HEAD"]),
        "chore(dashboard-lokal): Cargo.lock auf Fassung 0.4.0\n\ndashboard-lokal/Cargo.lock"
    );
    assert_eq!(git_in(&o.inhalt(), &["diff", "--cached", "--name-only"]), "pruefer/fremd.toml", "Fremdes bleibt vorgemerkt");
    let lock = std::fs::read_to_string(o.inhalt().join("dashboard-lokal/Cargo.lock")).unwrap();
    assert!(lock.contains("name = \"dashboard\"\nversion = \"0.4.0\""), "{lock}");
    let inhalt_kopf = git_in(&o.inhalt(), &["rev-parse", "HEAD"]);

    // Wiederaufnahme mit schmutzigem Baum, main noch nicht draußen: Die Prüfungen liefen auf dem Baum, also Abbruch vorher.
    std::fs::remove_file(&haken).unwrap();
    schreiben(&o.repo().join("docs/zielbild/a.toml"), "titel = \"B\"\n");
    let (code, text) = o.lauf(false);
    assert_eq!(code, 1, "{text}");
    assert!(text.contains("steht nicht sauber auf main") && text.contains("Es ging nichts hinaus."), "{text}");
    assert_eq!(git_in(&o.fern(), &["for-each-ref", "--format=%(refname) %(objectname)"]), fern_vorher, "draußen nichts");
    assert_eq!(o.neustarts(), 0);
    o.git(&["checkout", "--", "docs/zielbild/a.toml"]);

    // Wiederaufnahme: Prüfungen (main ist nicht draußen), Lock-Datei (schon da), 5 und 6.
    let (code, text) = o.lauf(false);
    assert_eq!(code, 0, "{text}");
    assert!(text.contains("Wiederaufnahme") && text.contains("erst die Prüfungen") && text.contains("cargo test ..."), "{text}");
    assert!(text.contains("steht schon auf 0.4.0"), "{text}");
    assert!(text.contains("Freigegeben: Fassung 0.4.0") && text.contains("Es läuft Fassung 0.4.0."), "{text}");
    assert_eq!(git_in(&o.fern(), &["rev-parse", "main"]), kopf);
    assert_eq!(git_in(&o.fern(), &["rev-parse", "v0.4.0"]), kopf);
    assert_eq!(git_in(&o.inhalt(), &["rev-parse", "HEAD"]), inhalt_kopf);
    assert_eq!(o.neustarts(), 1);
    assert_eq!(o.git(&["rev-parse", "main"]), kopf);

    // Alles draußen, Baum schmutzig: keine Prüfungen, kein Neustart (Dienst.ps1 baut aus dem Baum).
    schreiben(&o.repo().join("docs/zielbild/a.toml"), "titel = \"B\"\n");
    let (code, text) = o.lauf(false);
    assert_eq!(code, 1, "{text}");
    assert!(text.contains("der Dienst wurde nicht neu gestartet") && text.contains("main ist schon draußen."), "{text}");
    assert!(!text.contains("cargo test ..."), "{text}");
    assert_eq!(o.neustarts(), 1);
    o.git(&["checkout", "--", "docs/zielbild/a.toml"]);

    // Dienst rot: hochgeladen ist schon; der Lauf nennt trotzdem die laufende Fassung.
    o.dienst(1);
    let (code, text) = o.lauf(false);
    assert_eq!(code, 1, "{text}");
    assert!(text.contains("aber der Dienst ist rot") && text.contains("Es läuft Fassung 0.4.0."), "{text}");
    assert!(text.contains("main ist schon draußen.") && text.contains("Tag v0.4.0 ist schon draußen."), "{text}");
    assert_eq!(o.neustarts(), 2);
    assert_eq!(git_in(&o.fern(), &["rev-parse", "main"]), kopf);
    assert_eq!(git_in(&o.inhalt(), &["rev-parse", "HEAD"]), inhalt_kopf);

    // Dienst.ps1 grün, aber /api/betrieb nennt eine andere oder keine Fassung.
    o.dienst(0);
    for (koerper, muster) in [("{\"version\":\"0.3.4\"}", "Es läuft Fassung 0.3.4, nicht 0.4.0."), ("{}", "Welche Fassung läuft, ist unklar")] {
        o.antwort(koerper);
        let (code, text) = o.lauf(false);
        assert_eq!(code, 1, "{text}");
        assert!(text.contains(muster) && text.contains("nicht bestätigt (Dienst.ps1 Exit 0)"), "{muster}:\n{text}");
        assert!(text.contains("Fassung 0.4.0 ist draußen (main und Tag v0.4.0)") && !text.contains("Freigegeben"), "{text}");
    }
    assert_eq!(o.neustarts(), 4);

    // Alles draußen, dashboard-lokal offen: kein „nichts hinaus“; nur gemeldet, der Dienst startet trotzdem neu.
    o.antwort("{\"version\":\"0.4.0\"}");
    let p = o.inhalt().join("dashboard-lokal/Cargo.toml");
    let t = std::fs::read_to_string(&p).unwrap();
    schreiben(&p, &format!("{t}\n"));
    let (code, text) = o.lauf(false);
    assert_eq!(code, 1, "{text}");
    assert!(text.contains("main und Tag v0.4.0 sind schon draußen.") && !text.contains("Es ging nichts hinaus."), "{text}");
    assert!(text.contains("Die Lock-Datei im Inhalt ist nicht nachgezogen: dashboard-lokal im Inhalt hat offene Änderungen"), "{text}");
    assert!(text.contains("Es läuft Fassung 0.4.0."), "{text}");
    assert_eq!(o.neustarts(), 5);
    assert_eq!(git_in(&o.inhalt(), &["rev-parse", "HEAD"]), inhalt_kopf);
}

/// Jeder rote Schritt bricht ab, bevor etwas geschieht: main, Tags, das ferne
/// Repo, der Inhalt und der Dienst bleiben, wie sie waren.
#[test]
fn rot_vor_jeder_aussenwirkung() {
    if !bereit() {
        return;
    }
    type Vorbereiten = fn(&Probe);
    let faelle: [(&str, &str, Vorbereiten); 13] = [
        ("Zweig ohne Tag", "Der Zweig freigabe/v0.4.0 liegt ohne Tag v0.4.0 da", |o| {
            o.git(&["branch", "freigabe/v0.4.0"]);
        }),
        ("ohne Verbindung", "git fetch origin ging nicht", |o| {
            o.git(&["remote", "set-url", "origin", o.0.join("gibt-es-nicht.git").to_str().unwrap()]);
        }),
        ("nicht auf main", "nicht auf main", |o| {
            o.git(&["checkout", "-q", "-b", "seite"]);
        }),
        ("schmutzig", "nicht sauber", |o| schreiben(&o.repo().join("docs/zielbild/a.toml"), "titel = \"B\"\n")),
        ("fern voraus", "enthält origin/main nicht", |o| {
            let z = o.0.join("zweit");
            git_in(&o.0, &["clone", "-q", o.fern().to_str().unwrap(), z.to_str().unwrap()]);
            git_in(&z, &["-c", "user.name=Ranke", "-c", "user.email=ranke2@proton.me", "commit", "-q", "--allow-empty", "-m", "chore: daneben"]);
            git_in(&z, &["push", "-q", "origin", "main"]);
        }),
        ("letzter Tag nur lokal", "Der letzte Tag v0.3.9 auf main ist nicht auf origin", |o| {
            // Eine Freigabe, deren Hochladen scheiterte; danach kam mehr auf main.
            o.git(&["tag", "v0.3.9"]);
            o.zweig("fix: b", |o| o.changelog("Hinzugefügt\n- Neu.\n\nBehoben\n- A.\n- B.\n"));
        }),
        ("letzter Tag fern anders", "Der letzte Tag v0.3.9 auf main zeigt auf origin auf einen anderen Commit", |o| {
            o.git(&["tag", "v0.3.9"]);
            let ich = ["-c", "user.name=Ranke", "-c", "user.email=ranke2@proton.me"];
            let sha = git_in(&o.fern(), &[&ich[..], &["commit-tree", "main^{tree}", "-m", "daneben"]].concat());
            git_in(&o.fern(), &["tag", "v0.3.9", &sha]);
            o.zweig("fix: b", |o| o.changelog("Hinzugefügt\n- Neu.\n\nBehoben\n- A.\n- B.\n"));
        }),
        ("Lock offen", "dashboard-lokal im Inhalt hat offene Änderungen außer an Cargo.lock", |o| {
            let p = o.inhalt().join("dashboard-lokal/Cargo.toml");
            let t = std::fs::read_to_string(&p).unwrap();
            schreiben(&p, &format!("{t}\n"));
        }),
        ("Tag fern", "Den Tag v0.4.0 gibt es auf origin schon", |o| {
            // Ein ferner Tag auf einem Commit außerhalb von main: git fetch holt ihn nicht.
            let ich = ["-c", "user.name=Ranke", "-c", "user.email=ranke2@proton.me"];
            let sha = git_in(&o.fern(), &[&ich[..], &["commit-tree", "main^{tree}", "-m", "daneben"]].concat());
            git_in(&o.fern(), &["tag", "v0.4.0", &sha]);
        }),
        ("Zählregel", "fix ohne Zeile unter Behoben; ausgelöst von „fix: still“", |o| o.zweig("fix: still", |o| o.changelog("Hinzugefügt\n- Neu.\n"))),
        ("Format", "cargo fmt --check ist rot", |o| o.zweig("fix: a", |o| schreiben(&o.repo().join("src/lib.rs"), "pub fn  a( ) {}\n"))),
        ("pruefe", "pruefe --system docs/zielbild ist rot", |o| o.zweig("fix: a", |o| std::fs::remove_dir_all(o.repo().join("docs/zielbild")).unwrap())),
        ("Prüfer", "Hochladen.ps1 -Pruefen ist rot", |o| {
            o.zweig("fix: a", |o| schreiben(&o.repo().join("docs/anleitung/b.toml"), "titel = \"geheimwortfreigabe\"\n"))
        }),
    ];
    for (name, muster, vorbereiten) in faelle {
        let o = Probe::neu();
        o.zweig("feat(web): neu", |o| o.changelog("Hinzugefügt\n- Neu.\n\nBehoben\n- A.\n"));
        vorbereiten(&o);
        let vorher = o.stand();
        let (code, text) = o.lauf(false);
        assert_eq!(code, 1, "{name}:\n{text}");
        assert!(text.contains(muster), "{name}: erwartet „{muster}“:\n{text}");
        assert!(text.contains("Es ging nichts hinaus."), "{name}:\n{text}");
        assert_eq!(o.stand(), vorher, "{name}: nichts geändert");
        assert_eq!(o.git(&["tag", "--list", "v0.4.0"]), "", "{name}: kein Tag");
    }
}

/// Ein Merge, der während der Prüfungen auf main landet (hier über npm run
/// verify), wurde nicht geprüft: Schritt 4 bricht ab, nichts bekommt Merge oder Tag.
#[test]
fn main_bewegt_sich_waehrend_der_pruefung() {
    if !bereit() {
        return;
    }
    let o = Probe::neu();
    o.zweig("feat(web): neu", |o| {
        o.changelog("Hinzugefügt\n- Neu.\n\nBehoben\n- A.\n");
        let p = o.repo().join("web/package.json");
        let t = std::fs::read_to_string(&p)
            .unwrap()
            .replace("node --version", "git merge -q --no-ff --no-verify -m \\\"chore(merge): nebenher nach main\\\" nebenher");
        schreiben(&p, &t);
    });
    // Der Zweig eines anderen Chats, den npm run verify mitten in den Prüfungen nach main bringt.
    o.git(&["checkout", "-q", "-b", "nebenher"]);
    o.commit("chore: nebenher");
    o.git(&["checkout", "-q", "main"]);
    let fern_vorher = git_in(&o.fern(), &["for-each-ref", "--format=%(refname) %(objectname)"]);
    let (code, text) = o.lauf(false);
    assert_eq!(code, 1, "{text}");
    assert!(text.contains("hat sich main bewegt") && text.contains("Es ging nichts hinaus."), "{text}");
    assert_eq!(o.git(&["log", "-1", "--format=%s", "main"]), "chore(merge): nebenher nach main", "kein Release-Merge");
    assert_eq!(o.git(&["tag", "--list", "v0.4.0"]), "");
    assert_eq!(o.git(&["branch", "--list", "freigabe/*"]), "");
    assert_eq!(git_in(&o.fern(), &["for-each-ref", "--format=%(refname) %(objectname)"]), fern_vorher);
    assert_eq!(o.neustarts(), 0);
}

/// Merge und Tag ohne Freigeben.ps1 (Fassung-Festschreiben.ps1 -Basis allein) auf
/// einem roten cargo test: Die Wiederaufnahme prüft vor dem Hinaus und lädt nichts hoch.
#[test]
fn festschreiben_allein_wird_vor_dem_hinaus_geprueft() {
    if !bereit() {
        return;
    }
    let o = Probe::neu();
    o.zweig("feat(web): neu", |o| {
        o.changelog("Hinzugefügt\n- Neu.\n");
        schreiben(&o.repo().join("src/lib.rs"), "//! Probe.\n\n#[test]\nfn rot() {\n    panic!(\"rot\");\n}\n");
    });
    let basis = o.git(&["rev-parse", "main"]);
    let lock_alt = git_in(&o.inhalt(), &["rev-parse", "HEAD"]);
    let a = pwsh()
        .unwrap()
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(o.repo().join("scripts").join("Fassung-Festschreiben.ps1"))
        .args(["-Basis", &basis])
        .env("CARGO_TARGET_DIR", o.ziel())
        .output()
        .unwrap();
    assert!(a.status.success(), "{}{}", String::from_utf8_lossy(&a.stdout), String::from_utf8_lossy(&a.stderr));
    assert_eq!(o.git(&["log", "-1", "--format=%s", "main"]), "chore(release): v0.4.0");
    assert_eq!(o.git(&["rev-parse", "v0.4.0"]), o.git(&["rev-parse", "main"]));

    let fern_vorher = git_in(&o.fern(), &["for-each-ref", "--format=%(refname) %(objectname)"]);
    for pruefen in [true, false] {
        let (code, text) = o.lauf(pruefen);
        assert_eq!(code, 1, "{text}");
        assert!(text.contains("erst die Prüfungen") && text.contains("Abbruch: cargo test ist rot"), "{text}");
        assert!(text.contains("main trägt Fassung 0.4.0 lokal (Merge und Tag). Es ging nichts hinaus."), "{text}");
    }
    assert_eq!(git_in(&o.fern(), &["for-each-ref", "--format=%(refname) %(objectname)"]), fern_vorher, "draußen nichts");
    assert_eq!(git_in(&o.inhalt(), &["rev-parse", "HEAD"]), lock_alt, "keine Lock-Datei vor den Prüfungen");
    assert_eq!(o.neustarts(), 0);
}

/// Nach dem Push von main: origin bricht weg (der Satz sagt, dass main draußen ist),
/// danach liegt auf origin/main schon ein Commit über dem Release-Merge. Der Merge
/// gilt als draußen, nur der Tag geht noch hinaus.
#[test]
fn main_draussen_origin_weg_und_voraus() {
    if !bereit() {
        return;
    }
    let o = Probe::neu();
    o.zweig("feat(web): neu", |o| o.changelog("Hinzugefügt\n- Neu.\n"));
    // Nach dem Push von main zeigt origin ins Leere (ein Verbindungsabbruch).
    let weg = o.0.join("gibt-es-nicht.git");
    let fern_url = o.fern().to_str().unwrap().replace('\\', "/");
    let haken = o.fern().join("hooks").join("post-receive");
    schreiben(
        &haken,
        &format!(
            "#!/bin/sh\nunset GIT_DIR\ngit -C '{}' remote set-url origin '{}'\n",
            o.repo().to_str().unwrap().replace('\\', "/"),
            weg.to_str().unwrap().replace('\\', "/")
        ),
    );
    let (code, text) = o.lauf(false);
    assert_eq!(code, 1, "{text}");
    assert!(text.contains("origin ist nicht erreichbar") && text.contains("main ist draußen; ob der Tag v0.4.0 draußen ist, ist offen."), "{text}");
    assert!(!text.contains("Es ging nichts hinaus."), "{text}");
    let kopf = o.git(&["rev-parse", "main"]);
    assert_eq!(git_in(&o.fern(), &["rev-parse", "main"]), kopf);
    assert_eq!(git_in(&o.fern(), &["tag", "--list", "v0.4.0"]), "");

    // Ohne Verbindung sagt auch die Wiederaufnahme nur, dass offen ist, was draußen ist.
    let (code, text) = o.lauf(true);
    assert_eq!(code, 1, "{text}");
    assert!(text.contains("was davon schon draußen ist, ist offen.") && !text.contains("Es ging nichts hinaus."), "{text}");
    std::fs::remove_file(&haken).unwrap();
    o.git(&["remote", "set-url", "origin", &fern_url]);

    // Ein anderer Klon legt einen Commit auf den Release-Merge.
    let z = o.0.join("zweit");
    git_in(&o.0, &["clone", "-q", o.fern().to_str().unwrap(), z.to_str().unwrap()]);
    git_in(&z, &["-c", "user.name=Ranke", "-c", "user.email=ranke2@proton.me", "commit", "-q", "--allow-empty", "-m", "chore: daneben"]);
    git_in(&z, &["push", "-q", "origin", "main"]);
    let fern_main = git_in(&o.fern(), &["rev-parse", "main"]);

    let (code, text) = o.lauf(true);
    assert_eq!(code, 0, "{text}");
    assert!(text.contains("main ist schon draußen, der Tag v0.4.0 noch nicht."), "{text}");
    assert!(!text.contains("Es ging nichts hinaus.") && !text.contains("cargo test ..."), "{text}");

    let (code, text) = o.lauf(false);
    assert_eq!(code, 0, "{text}");
    assert!(text.contains("main ist schon draußen.") && text.contains("Freigegeben: Fassung 0.4.0"), "{text}");
    assert_eq!(git_in(&o.fern(), &["rev-parse", "v0.4.0"]), kopf);
    assert_eq!(git_in(&o.fern(), &["rev-parse", "main"]), fern_main, "main draußen unverändert");
    assert_eq!(o.neustarts(), 1);
}
