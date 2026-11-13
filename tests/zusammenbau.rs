//! Gegentests für den Wächter der Zusammenbau-Kiste (Nachbesserung zum Umzug
//! des Inhalts, Punkte 1 bis 5): `dashboard::projekte::zusammenbau`, auch als
//! `dashboard.exe pruefe-zusammenbau`. Jeder Fall baut im Temp-Ordner einen
//! kleinen Inhalt mit Git-Repo, einer erfundenen Kiste `projekt-probe` und
//! `dashboard-lokal` aus der Vorlage dieses Code-Repos; das Code-Repo ist
//! dieses hier. Der saubere Fall ist grün, jeder veränderte rot. Gebaut wird
//! nichts: Der Wächter liest nur.

use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use dashboard::dienst::programme::{self, Werkzeug};
use dashboard::projekte::zusammenbau::{self, Kiste, Liste};

static ZAEHLER: AtomicUsize = AtomicUsize::new(0);

fn code_repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Relativer Weg von `von` nach `nach` (beide absolut, gleiches Laufwerk).
fn relativ(von: &Path, nach: &Path) -> String {
    let teile = |p: &Path| -> Vec<String> {
        std::path::absolute(p)
            .unwrap()
            .components()
            .filter_map(|k| match k {
                Component::Normal(n) => Some(n.to_string_lossy().into_owned()),
                Component::Prefix(x) => Some(x.as_os_str().to_string_lossy().to_lowercase()),
                _ => None,
            })
            .collect()
    };
    let (a, b) = (teile(von), teile(nach));
    let gleich = a.iter().zip(&b).take_while(|(x, y)| x.eq_ignore_ascii_case(y)).count();
    let mut v: Vec<String> = std::iter::repeat_n("..".to_string(), a.len() - gleich).collect();
    v.extend(b[gleich..].iter().cloned());
    v.join("/")
}

fn git(wo: &Path, args: &[&str]) -> String {
    let o = programme::befehl(Werkzeug::Git)
        .unwrap()
        .arg("-C")
        .arg(wo)
        .args(["-c", "user.name=Ranke", "-c", "user.email=ranke2@proton.me", "-c", "core.autocrlf=false"])
        .args(args)
        .output()
        .unwrap();
    assert!(o.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8_lossy(&o.stdout).trim().to_string()
}

/// Ein Wegwerf-Inhalt: `<wurzel>/inhalt` (Git-Repo) mit `projects/probe/server`
/// und `dashboard-lokal`.
struct Fall {
    wurzel: PathBuf,
    commit: String,
}

impl Fall {
    fn neu() -> Fall {
        let n = ZAEHLER.fetch_add(1, Ordering::SeqCst);
        let wurzel = std::env::temp_dir().join(format!("dashboard-zusammenbau-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&wurzel);
        let inhalt = wurzel.join("inhalt");
        let server = inhalt.join("projects").join("probe").join("server");
        std::fs::create_dir_all(server.join("src")).unwrap();
        let sdk = relativ(&server, &code_repo().join("crates").join("dashboard-projekt"));
        std::fs::write(
            server.join("Cargo.toml"),
            format!("[package]\nname = \"projekt-probe\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\ndashboard-projekt = {{ path = \"{sdk}\" }}\nserde = {{ version = \"1\", features = [\"derive\"] }}\n"),
        )
        .unwrap();
        std::fs::write(server.join("src").join("lib.rs"), "#![forbid(unsafe_code)]\n//! Erfundene Kiste für die Gegentests.\npub struct Probe;\n").unwrap();
        std::fs::write(inhalt.join(".gitignore"), "/dashboard-lokal/target/\n/projects/*/server/target/\n*.geheim\n").unwrap();
        git(&inhalt, &["init", "-q", "-b", "main"]);
        git(&inhalt, &["add", "-A"]);
        git(&inhalt, &["commit", "-q", "-m", "Inhalt"]);
        let commit = git(&inhalt, &["rev-parse", "--short=12", "HEAD"]);
        let f = Fall { wurzel, commit };
        f.lokal_schreiben(&f.liste());
        f
    }

    fn inhalt(&self) -> PathBuf {
        self.wurzel.join("inhalt")
    }
    fn lokal(&self) -> PathBuf {
        self.inhalt().join("dashboard-lokal")
    }
    fn server(&self) -> PathBuf {
        self.inhalt().join("projects").join("probe").join("server")
    }

    fn liste(&self) -> Liste {
        Liste {
            code: relativ(&self.lokal(), &code_repo()),
            kisten: vec![Kiste { name: "projekt-probe".into(), pfad: "../projects/probe/server".into(), typ: "Probe".into(), commit: self.commit.clone() }],
        }
    }

    /// dashboard-lokal wie Inhalt-Umziehen.ps1 es anlegt: Vorlage, Kisten,
    /// Cargo.lock aus dem Code-Repo ohne dessen Vorlage-Kiste.
    fn lokal_schreiben(&self, liste: &Liste) {
        let vorlage = code_repo().join(zusammenbau::VORLAGE);
        let lokal = self.lokal();
        let mut offen = vec![PathBuf::new()];
        while let Some(rel) = offen.pop() {
            for e in std::fs::read_dir(vorlage.join(&rel)).unwrap() {
                let e = e.unwrap();
                let r = rel.join(e.file_name());
                if e.path().is_dir() {
                    offen.push(r);
                    continue;
                }
                let text = std::fs::read_to_string(e.path()).unwrap();
                let neu = match r.to_string_lossy().replace('\\', "/").as_str() {
                    "Cargo.toml" => zusammenbau::erwartete_cargo(&text, liste),
                    "src/lib.rs" => zusammenbau::erwartete_lib(&text, liste),
                    "kisten.toml" => continue,
                    _ => text,
                };
                let ziel = lokal.join(&r);
                std::fs::create_dir_all(ziel.parent().unwrap()).unwrap();
                std::fs::write(ziel, neu).unwrap();
            }
        }
        let mut k = format!("[code]\npfad = \"{}\"\n", liste.code);
        for x in &liste.kisten {
            k.push_str(&format!("\n[[kiste]]\nname = \"{}\"\npfad = \"{}\"\ntyp = \"{}\"\ncommit = \"{}\"\n", x.name, x.pfad, x.typ, x.commit));
        }
        std::fs::write(lokal.join("kisten.toml"), k).unwrap();
        let code_lock = std::fs::read_to_string(code_repo().join("Cargo.lock")).unwrap().replace("\r\n", "\n");
        let lock: Vec<&str> = code_lock.split("\n[[package]]\n").filter(|b| !b.starts_with("name = \"projekt-vorlage\"")).collect();
        std::fs::write(lokal.join("Cargo.lock"), lock.join("\n[[package]]\n")).unwrap();
    }

    fn pruefen(&self) -> String {
        zusammenbau::pruefe(&self.lokal(), &code_repo(), false).join("\n")
    }

    fn anhaengen(&self, datei: &Path, text: &str) {
        let alt = std::fs::read_to_string(datei).unwrap_or_default();
        std::fs::write(datei, alt + text).unwrap();
    }
}

impl Drop for Fall {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.wurzel);
    }
}

fn bereit() -> bool {
    if programme::befehl(Werkzeug::Git).is_err() {
        return !dashboard::ueberspringen("git fehlt");
    }
    true
}

/// Ein Fall: `aendern` macht den sauberen Inhalt kaputt; dann nennt der
/// Wächter `satz`.
fn rot(name: &str, satz: &str, aendern: impl FnOnce(&Fall)) {
    let f = Fall::neu();
    aendern(&f);
    let s = f.pruefen();
    assert!(s.contains(satz), "{name}: erwartet „{satz}“, bekam:\n{s}");
}

#[test]
fn sauberer_zusammenbau_ist_gruen() {
    if !bereit() {
        return;
    }
    let f = Fall::neu();
    assert_eq!(f.pruefen(), "");
    // Ein Ordner target (Bauordner) zählt nicht.
    std::fs::create_dir_all(f.lokal().join("target").join("debug")).unwrap();
    std::fs::write(f.lokal().join("target").join("debug").join("x"), "x").unwrap();
    assert_eq!(f.pruefen(), "");
    // Ohne Kisten (nur die Vorlage) auch.
    let mut leer = f.liste();
    leer.kisten.clear();
    f.lokal_schreiben(&leer);
    assert_eq!(f.pruefen(), "");
}

#[test]
fn git_und_fremde_pfade_sind_rot() {
    if !bereit() {
        return;
    }
    rot("git-Abhängigkeit in dashboard-lokal", "Cargo.toml: weicht von der Vorlage ab", |f| {
        let c = f.lokal().join("Cargo.toml");
        let t = std::fs::read_to_string(&c).unwrap().replace("# KISTEN-ENDE", "fremd = { git = \"https://example.invalid/fremd\" }\n# KISTEN-ENDE");
        std::fs::write(c, t).unwrap();
    });
    rot("git-Abhängigkeit in der Kiste", "kein git", |f| {
        f.anhaengen(&f.server().join("Cargo.toml"), "serde_json = { git = \"https://example.invalid/x\" }\n");
    });
    rot("Pfad außerhalb der Liste in Cargo.toml", "Cargo.toml: weicht von der Vorlage ab", |f| {
        let c = f.lokal().join("Cargo.toml");
        let t = std::fs::read_to_string(&c).unwrap().replace("# KISTEN-ENDE", "projekt-fremd = { path = \"../projects/fremd/server\" }\n# KISTEN-ENDE");
        std::fs::write(c, t).unwrap();
    });
    rot("Pfad außerhalb der Liste in Cargo.lock", "projekt-fremd kommt aus einem Pfad", |f| {
        f.anhaengen(&f.lokal().join("Cargo.lock"), "\n[[package]]\nname = \"projekt-fremd\"\nversion = \"0.1.0\"\n");
    });
    rot("fremde Quelle in Cargo.lock", "kommt nicht von crates.io", |f| {
        f.anhaengen(
            &f.lokal().join("Cargo.lock"),
            "\n[[package]]\nname = \"fremd\"\nversion = \"0.1.0\"\nsource = \"git+https://example.invalid/fremd#0123abc\"\n",
        );
    });
    rot("andere Fassung als im Code-Repo", "steht so (Fassung und Prüfsumme) nicht", |f| {
        f.anhaengen(
            &f.lokal().join("Cargo.lock"),
            &format!("\n[[package]]\nname = \"serde\"\nversion = \"0.0.1\"\nsource = \"{}\"\nchecksum = \"00\"\n", zusammenbau::CRATES_IO),
        );
    });
    rot("Kiste, die das Code-Repo nicht mehr nutzt", "nutzt das Code-Repo nicht mehr. Die Cargo.lock der Zusammenbau-Kiste offline mitziehen", |f| {
        f.anhaengen(
            &f.lokal().join("Cargo.lock"),
            &format!("\n[[package]]\nname = \"gibt-es-im-code-nicht\"\nversion = \"0.13.5\"\nsource = \"{}\"\nchecksum = \"00\"\n", zusammenbau::CRATES_IO),
        );
    });
    rot("Cargo.lock fehlt", "Cargo.lock fehlt", |f| std::fs::remove_file(f.lokal().join("Cargo.lock")).unwrap());
    rot("Kiste nicht in der Liste im Pfad von kisten.toml", "pfad muss genau ../projects/<slug>/server sein", |f| {
        let mut l = f.liste();
        l.kisten[0].pfad = "../../anderswo/server".into();
        f.lokal_schreiben(&l);
    });
    rot("Typ mit Code statt Name", "ist kein Rust-Typname", |f| {
        let mut l = f.liste();
        l.kisten[0].typ = "Probe), (fremd::X".into();
        f.lokal_schreiben(&l);
    });
}

#[test]
fn patch_workspace_und_einstellungen_sind_rot() {
    if !bereit() {
        return;
    }
    rot("[patch] in dashboard-lokal", "[patch] ist nicht erlaubt", |f| {
        f.anhaengen(&f.lokal().join("Cargo.toml"), "\n[patch.crates-io]\nserde = { path = \"../fremd\" }\n");
    });
    rot("[patch] in der Kiste", "Kein [patch]", |f| {
        f.anhaengen(&f.server().join("Cargo.toml"), "\n[patch.crates-io]\nserde = { path = \"../fremd\" }\n");
    });
    rot("workspace members", "[workspace] muss leer sein", |f| {
        let c = f.lokal().join("Cargo.toml");
        let t = std::fs::read_to_string(&c).unwrap().replace("[workspace]\n", "[workspace]\nmembers = [\"../projects/probe/server\"]\n");
        std::fs::write(c, t).unwrap();
    });
    rot(".cargo/config.toml im Ordner darüber", "[build] rustc-wrapper ist nicht erlaubt", |f| {
        std::fs::create_dir_all(f.wurzel.join(".cargo")).unwrap();
        std::fs::write(f.wurzel.join(".cargo").join("config.toml"), "[build]\nrustc-wrapper = \"fremd.exe\"\n").unwrap();
    });
    // Im Inhalt gar kein .cargo (Nachbesserung 2, Punkt 2), auch nicht mit erlaubtem Inhalt.
    rot(".cargo/config im Inhalt (ohne Endung)", "Im Inhalt ist keine .cargo-Einstellung erlaubt", |f| {
        std::fs::create_dir_all(f.inhalt().join(".cargo")).unwrap();
        std::fs::write(f.inhalt().join(".cargo").join("config"), "[source.crates-io]\nreplace-with = \"fremd\"\n").unwrap();
    });
    rot("dlltool im Inhalt, auch nur der Name", "Im Inhalt ist keine .cargo-Einstellung erlaubt", |f| {
        std::fs::create_dir_all(f.inhalt().join(".cargo")).unwrap();
        std::fs::write(f.inhalt().join(".cargo").join("config.toml"), "[target.x86_64-pc-windows-gnu]\nrustflags = [\"-C\", \"dlltool=llvm-dlltool.exe\"]\n")
            .unwrap();
    });
    rot("linker darüber", "linker ist nicht erlaubt", |f| {
        std::fs::create_dir_all(f.wurzel.join(".cargo")).unwrap();
        std::fs::write(f.wurzel.join(".cargo").join("config.toml"), "[target.x86_64-pc-windows-gnu]\nlinker = \"fremd.exe\"\n").unwrap();
    });
    // Ein dlltool-Pfad über dem Inhalt, der in den Inhalt zeigt.
    rot("dlltool zeigt in den Inhalt", "nur rustflags", |f| {
        let fremd = f.inhalt().join("werkzeug").join("llvm-dlltool.exe").to_string_lossy().replace('\\', "/");
        std::fs::create_dir_all(f.wurzel.join(".cargo")).unwrap();
        std::fs::write(f.wurzel.join(".cargo").join("config.toml"), format!("[target.x86_64-pc-windows-gnu]\nrustflags = [\"-C\", \"dlltool={fremd}\"]\n"))
            .unwrap();
    });
    rot("dlltool irgendwo anders", "nur rustflags", |f| {
        std::fs::create_dir_all(f.wurzel.join(".cargo")).unwrap();
        std::fs::write(
            f.wurzel.join(".cargo").join("config.toml"),
            "[target.x86_64-pc-windows-gnu]\nrustflags = [\"-C\", \"dlltool=C:/fremd/llvm-dlltool.exe\"]\n",
        )
        .unwrap();
    });
    rot("eigene .cargo in dashboard-lokal", "steht nicht in der Vorlage", |f| {
        std::fs::create_dir_all(f.lokal().join(".cargo")).unwrap();
        std::fs::write(f.lokal().join(".cargo").join("config.toml"), "[target.x86_64-pc-windows-gnu]\nrustflags = [\"-C\", \"dlltool=llvm-dlltool.exe\"]\n")
            .unwrap();
    });
    rot("rust-toolchain darüber", "Toolchain-Datei", |f| {
        std::fs::write(f.inhalt().join("rust-toolchain.toml"), "[toolchain]\npath = \"C:/fremd\"\n").unwrap();
    });
    // Erlaubt darüber: nur dlltool, net, http, term.
    let f = Fall::neu();
    std::fs::create_dir_all(f.wurzel.join(".cargo")).unwrap();
    std::fs::write(
        f.wurzel.join(".cargo").join("config.toml"),
        "[target.x86_64-pc-windows-gnu]\nrustflags = [\"-C\", \"dlltool=llvm-dlltool.exe\"]\n[net]\noffline = true\n",
    )
    .unwrap();
    assert_eq!(f.pruefen(), "");
}

#[test]
fn bauskripte_und_makros_sind_rot() {
    if !bereit() {
        return;
    }
    rot("build.rs in dashboard-lokal", "dashboard-lokal/build.rs: Diese Datei steht nicht in der Vorlage", |f| {
        std::fs::write(f.lokal().join("build.rs"), "fn main() {}\n").unwrap();
    });
    rot("weiteres Modul in dashboard-lokal", "src/fremd.rs: Diese Datei steht nicht in der Vorlage", |f| {
        std::fs::write(f.lokal().join("src").join("fremd.rs"), "pub fn x() {}\n").unwrap();
    });
    rot("main.rs geändert", "src/main.rs: weicht von der Vorlage ab", |f| {
        f.anhaengen(&f.lokal().join("src").join("main.rs"), "\nfn fremd() {}\n");
    });
    rot("build.rs in der Kiste", "Kein Bauskript (build.rs)", |f| {
        std::fs::write(f.server().join("build.rs"), "fn main() {}\n").unwrap();
    });
    rot("proc-macro in der Kiste", "Kein proc-macro", |f| {
        f.anhaengen(&f.server().join("Cargo.toml"), "\n[lib]\nproc-macro = true\n");
    });
}

#[test]
fn geaenderte_und_ignorierte_dateien_sind_rot() {
    if !bereit() {
        return;
    }
    rot("geänderte Kiste", "seit dem geprüften Commit", |f| {
        f.anhaengen(&f.server().join("src").join("lib.rs"), "pub struct Neu;\n");
    });
    rot("geändert und committet", "seit dem geprüften Commit", |f| {
        f.anhaengen(&f.server().join("src").join("lib.rs"), "pub struct Neu;\n");
        git(&f.inhalt(), &["add", "-A", "projects"]);
        git(&f.inhalt(), &["commit", "-q", "-m", "neu"]);
    });
    rot("neue Datei", "ungeprüfte Datei(en)", |f| {
        std::fs::write(f.server().join("src").join("neu.txt"), "x").unwrap();
    });
    // Punkt 4: auch Dateien, die git ignoriert.
    rot("ignorierte Datei", "ungeprüfte Datei(en)", |f| {
        std::fs::write(f.server().join("notiz.geheim"), "x").unwrap();
    });
    rot("ignorierter Bauordner der Kiste", "ungeprüfte Datei(en)", |f| {
        std::fs::create_dir_all(f.server().join("target")).unwrap();
        std::fs::write(f.server().join("target").join("x.rlib"), "x").unwrap();
    });
    rot("Commit gibt es nicht", "gibt es im Inhalts-Repo nicht", |f| {
        let mut l = f.liste();
        l.kisten[0].commit = "0123456789ab".into();
        f.lokal_schreiben(&l);
    });
    rot("eigenes .git in der Kiste", "projects/probe/server/.git ist ein eigenes Git-Repo in der Kiste", |f| {
        git(&f.server(), &["init", "-q"]);
    });
    // Nachbesserung 3, Punkt 2: auch tiefer und als Datei (wie bei einem Submodul).
    rot(".git als Datei tief in der Kiste", "src/fremd/.git ist ein eigenes Git-Repo in der Kiste", |f| {
        std::fs::create_dir_all(f.server().join("src").join("fremd")).unwrap();
        std::fs::write(f.server().join("src").join("fremd").join(".git"), "gitdir: ../../../../../fremd.git\n").unwrap();
    });
    rot("gitlink ohne ausgechecktes Submodul", "ist ein Submodul (gitlink, Modus 160000)", |f| {
        let ziel = git(&f.inhalt(), &["rev-parse", "HEAD"]);
        git(&f.inhalt(), &["update-index", "--add", "--cacheinfo", &format!("160000,{ziel},projects/probe/server/fremd")]);
        git(&f.inhalt(), &["commit", "-q", "-m", "gitlink"]);
        let mut l = f.liste();
        l.kisten[0].commit = git(&f.inhalt(), &["rev-parse", "--short=12", "HEAD"]);
        f.lokal_schreiben(&l);
    });
    rot("Filter in der Git-Einstellung des Inhalts", "filter.", |f| {
        git(&f.inhalt(), &["config", "filter.x.clean", "fremd.exe"]);
    });
}

/// Gleichen Inhalt mit neuer Zeit schreiben: git muss die Datei neu rechnen
/// (Filter) und würde den Index neu schreiben (Hook post-index-change).
fn anfassen(datei: &Path) {
    std::thread::sleep(std::time::Duration::from_millis(1100));
    let alt = std::fs::read(datei).unwrap();
    std::fs::write(datei, alt).unwrap();
}

/// git ohne Prüfung des Ausgangs (für die Gegenproben).
fn git_roh(wo: &Path, args: &[&str]) -> std::process::Output {
    programme::befehl(Werkzeug::Git).unwrap().arg("-C").arg(wo).args(args).output().unwrap()
}

/// Ein Programm für die Git-Einstellung, das die Markendatei schreibt (und als
/// Filter den Inhalt durchreicht).
fn marken_programm(marke: &Path) -> String {
    format!("echo lief > '{}'; cat", marke.to_string_lossy().replace('\\', "/"))
}

/// Ein Hook, der die Markendatei schreibt.
fn marken_hook(datei: &Path, marke: &Path) {
    std::fs::create_dir_all(datei.parent().unwrap()).unwrap();
    std::fs::write(datei, format!("#!/bin/sh\necho lief > '{}'\n", marke.to_string_lossy().replace('\\', "/"))).unwrap();
}

/// Rote Einstellungen include und hooksPath (Nachbesserung 2): Der Wächter
/// bricht ab, und nichts davon lief. Gegenprobe: git status von Hand führte
/// es aus.
#[test]
fn rote_einstellung_startet_nichts() {
    if !bereit() {
        return;
    }
    for fall in ["include", "hooksPath"] {
        let f = Fall::neu();
        let marke = f.wurzel.join("marke.txt");
        match fall {
            "include" => {
                // Die eingebundene Datei setzt einen Filter; das Attribut dazu ist eingecheckt.
                std::fs::write(f.inhalt().join(".gitattributes"), "*.rs filter=probe\n").unwrap();
                git(&f.inhalt(), &["add", ".gitattributes"]);
                git(&f.inhalt(), &["commit", "-q", "-m", "Attribute"]);
                let fremd = f.wurzel.join("fremd.gitconfig");
                std::fs::write(&fremd, format!("[filter \"probe\"]\n\tclean = \"{}\"\n", marken_programm(&marke))).unwrap();
                git(&f.inhalt(), &["config", "include.path", "../../fremd.gitconfig"]);
            }
            _ => {
                marken_hook(&f.inhalt().join("fremd").join("post-index-change"), &marke);
                git(&f.inhalt(), &["config", "core.hooksPath", "fremd"]);
            }
        }
        let lib = f.server().join("src").join("lib.rs");
        anfassen(&lib);
        let s = f.pruefen();
        let erwartet = if fall == "include" { "include.path" } else { "core.hookspath" };
        assert!(s.contains(erwartet), "{fall}: {s}");
        assert!(!s.contains("seit dem geprüften Commit") && !s.contains("ungeprüfte Datei"), "{fall}: kein git diff/status nach rotem Fund: {s}");
        assert!(!marke.exists(), "{fall}: es lief ein Programm aus dem Inhalt");
        // Gegenprobe: git status von Hand startete das Programm.
        git_roh(&f.server(), &["status", "--porcelain"]);
        assert!(marke.exists(), "{fall}: Gegenprobe, git status liefe es");
    }
}

/// Nachbesserung 3, Punkt 1: Ein Hook post-index-change im Standard-Ordner
/// `.git/hooks` (den die Prüfung von core.hooksPath nicht sieht) läuft nie:
/// git status schreibt den Index nicht (`--no-optional-locks`), und Hooks
/// kommen nur aus einem leeren Ordner. Der Inhalt ist sonst sauber (grün).
#[test]
fn git_hook_laeuft_nie() {
    if !bereit() {
        return;
    }
    let f = Fall::neu();
    let marke = f.wurzel.join("marke.txt");
    marken_hook(&f.inhalt().join(".git").join("hooks").join("post-index-change"), &marke);
    let lib = f.server().join("src").join("lib.rs");
    anfassen(&lib);
    let s = f.pruefen();
    assert!(!marke.exists(), "der Hook lief: {s}");
    assert_eq!(s, "");
    // Gegenprobe: git status von Hand schreibt den Index und ruft den Hook.
    // (Neu anfassen: git diff des Wächters schreibt den Index trotz
    // --no-optional-locks; dort hält nur der leere Hook-Ordner den Hook ab.)
    anfassen(&lib);
    git_roh(&f.server(), &["status", "--porcelain"]);
    assert!(marke.exists(), "Gegenprobe, git status liefe den Hook");
}

/// Nachbesserung 3, Punkte 1 und 2: Ein Submodul in der Kiste, dessen eigene
/// Einstellung und Attribute einen Filter setzen (die der Wächter im Inhalt
/// nicht sieht): rot (.git in der Kiste), und der Filter läuft nie (auch
/// git diff und status gingen nicht ins Submodul, `--ignore-submodules=all`).
#[test]
fn submodul_laeuft_nie() {
    if !bereit() {
        return;
    }
    let f = Fall::neu();
    let marke = f.wurzel.join("marke.txt");
    let sub = f.wurzel.join("sub");
    std::fs::create_dir_all(&sub).unwrap();
    git(&sub, &["init", "-q", "-b", "main"]);
    std::fs::write(sub.join(".gitattributes"), "*.txt filter=probe\n").unwrap();
    std::fs::write(sub.join("a.txt"), "x\n").unwrap();
    git(&sub, &["add", "-A"]);
    git(&sub, &["commit", "-q", "-m", "Submodul"]);
    let quelle = sub.to_string_lossy().replace('\\', "/");
    git(&f.inhalt(), &["-c", "protocol.file.allow=always", "submodule", "add", "-q", &quelle, "projects/probe/server/sub"]);
    git(&f.inhalt(), &["commit", "-q", "-m", "Submodul"]);
    let mut l = f.liste();
    l.kisten[0].commit = git(&f.inhalt(), &["rev-parse", "--short=12", "HEAD"]);
    f.lokal_schreiben(&l);
    let im_sub = f.server().join("sub");
    git(&im_sub, &["config", "filter.probe.clean", &marken_programm(&marke)]);
    anfassen(&im_sub.join("a.txt"));
    let s = f.pruefen();
    assert!(!marke.exists(), "der Filter im Submodul lief");
    assert!(s.contains("sub/.git ist ein eigenes Git-Repo in der Kiste"), "{s}");
    // Gegenprobe: git status von Hand ging ins Submodul und führte den Filter aus.
    git_roh(&f.server(), &["status", "--porcelain"]);
    assert!(marke.exists(), "Gegenprobe, git status liefe den Filter im Submodul");
}

/// Nachbesserung 3, Punkt 1: In einem partial clone (`--filter=blob:none`)
/// fehlt der Inhalt alter Fassungen; git diff gegen den geprüften Commit lüde
/// ihn nach und startete dafür git fetch mit `remote.origin.uploadpack` aus
/// dem Inhalt. Der Wächter lädt nie nach (GIT_NO_LAZY_FETCH, kein Transport).
#[test]
fn partial_clone_laedt_nie_nach() {
    if !bereit() {
        return;
    }
    let f = Fall::neu();
    let marke = f.wurzel.join("marke.txt");
    // Neuer Stand; kisten.toml nennt weiter den alten Commit (dessen lib.rs dem Klon fehlt).
    let lib = f.server().join("src").join("lib.rs");
    f.anhaengen(&lib, "pub struct Neu;\n");
    git(&f.inhalt(), &["add", "-A", "projects"]);
    git(&f.inhalt(), &["commit", "-q", "-m", "neu"]);
    let quelle = f.wurzel.join("quelle.git");
    git(&f.wurzel, &["clone", "-q", "--bare", "inhalt", "quelle.git"]);
    git(&quelle, &["config", "uploadpack.allowFilter", "true"]);
    std::fs::rename(f.inhalt(), f.wurzel.join("vorher")).unwrap();
    let url = format!("file:///{}", quelle.to_string_lossy().replace('\\', "/"));
    git(&f.wurzel, &["clone", "-q", "--filter=blob:none", &url, "inhalt"]);
    f.lokal_schreiben(&f.liste());
    let k = git(&f.inhalt(), &["config", "remote.origin.promisor"]);
    assert_eq!(k, "true", "ein partial clone");
    git(&f.inhalt(), &["config", "remote.origin.uploadpack", &format!("{}; git-upload-pack", marken_programm(&marke).trim_end_matches("; cat"))]);
    anfassen(&lib);
    let s = f.pruefen();
    assert!(!marke.exists(), "git lud nach und startete ein Programm aus dem Inhalt: {s}");
    assert!(!s.is_empty(), "rot (geändert seit dem Commit oder git diff gescheitert): {s}");
    // Gegenprobe: git diff von Hand lädt nach und startet uploadpack.
    git_roh(&f.server(), &["diff", "--quiet", &f.commit, "--", "."]);
    assert!(marke.exists(), "Gegenprobe, git diff lüde nach");
}

/// Nachbesserung 3, Punkt 4: Fehlt dashboard.exe (der Wächter), baut
/// Fassung-Bauen.ps1 dashboard-lokal nicht; cargo ist hier eine Attrappe, die
/// nur ihre Aufrufe aufschreibt.
#[test]
fn fassung_bauen_ohne_waechter_baut_nicht() {
    let Some(mut ps) = [Werkzeug::PowerShell7, Werkzeug::WindowsPowerShell].into_iter().find_map(|w| programme::befehl(w).ok()) else {
        dashboard::ueberspringen("PowerShell fehlt");
        return;
    };
    let n = ZAEHLER.fetch_add(1, Ordering::SeqCst);
    let wurzel = std::env::temp_dir().join(format!("dashboard-fassung-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&wurzel);
    let werkzeug = wurzel.join("werkzeug");
    for d in [&werkzeug, &wurzel.join("repo"), &wurzel.join("lokal"), &wurzel.join("bau")] {
        std::fs::create_dir_all(d).unwrap();
    }
    let protokoll = wurzel.join("cargo.txt");
    std::fs::write(werkzeug.join("cargo.cmd"), format!("@echo %*>>\"{}\"\r\n@exit /b 0\r\n", protokoll.display())).unwrap();
    let pfad = format!("{};{}", werkzeug.display(), std::env::var("PATH").unwrap_or_default());
    let skript = code_repo().join("scripts").join("Fassung-Bauen.ps1");
    let befehl = format!(
        "[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding($false); . '{}'; $r = Fassung-Bauen '{}' '{}' '{}'; [Console]::Out.WriteLine('CODE=' + $r.Code + ' LEC=' + $global:LASTEXITCODE); [Console]::Out.WriteLine($r.Satz)",
        skript.display(),
        wurzel.join("repo").display(),
        wurzel.join("lokal").display(),
        wurzel.join("bau").display()
    );
    let o = ps.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", &befehl]).env("PATH", pfad).output().unwrap();
    let aus = String::from_utf8_lossy(&o.stdout).into_owned() + &String::from_utf8_lossy(&o.stderr);
    let aufrufe = std::fs::read_to_string(&protokoll).unwrap_or_default();
    let _ = std::fs::remove_dir_all(&wurzel);
    assert!(aus.contains("CODE=1 LEC=1"), "{aus}");
    assert!(aus.contains("Der Wächter fehlt"), "{aus}");
    assert_eq!(aufrufe.lines().count(), 1, "nur der Bau des Code-Repos, kein cargo in dashboard-lokal: {aufrufe}");
    assert!(!aufrufe.contains("--config"), "{aufrufe}");
}

/// Ohne Kiste räumt Fassung-Bauen.ps1 ein altes release\zusammenbau weg:
/// Dienst-Einrichten.ps1 nimmt es, sobald dort dashboard.exe liegt, und
/// spielte sonst eine ältere Fassung ein als Dienst.ps1 -Neustart.
#[test]
fn fassung_bauen_ohne_kiste_raeumt_zusammenbau_weg() {
    let Some(mut ps) = [Werkzeug::PowerShell7, Werkzeug::WindowsPowerShell].into_iter().find_map(|w| programme::befehl(w).ok()) else {
        dashboard::ueberspringen("PowerShell fehlt");
        return;
    };
    let n = ZAEHLER.fetch_add(1, Ordering::SeqCst);
    let wurzel = std::env::temp_dir().join(format!("dashboard-fassung-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&wurzel);
    let werkzeug = wurzel.join("werkzeug");
    let alt = wurzel.join("bau").join("release").join("zusammenbau");
    for d in [&werkzeug, &wurzel.join("repo"), &alt.join("web")] {
        std::fs::create_dir_all(d).unwrap();
    }
    std::fs::write(alt.join("dashboard.exe"), "alt").unwrap();
    std::fs::write(werkzeug.join("cargo.cmd"), "@exit /b 0\r\n").unwrap();
    let pfad = format!("{};{}", werkzeug.display(), std::env::var("PATH").unwrap_or_default());
    let skript = code_repo().join("scripts").join("Fassung-Bauen.ps1");
    let befehl = format!(
        "[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding($false); . '{}'; $r = Fassung-Bauen '{}' '' '{}'; [Console]::Out.WriteLine('CODE=' + $r.Code + ' QUELLE=' + $r.Quelle)",
        skript.display(),
        wurzel.join("repo").display(),
        wurzel.join("bau").display()
    );
    let o = ps.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", &befehl]).env("PATH", pfad).output().unwrap();
    let aus = String::from_utf8_lossy(&o.stdout).into_owned() + &String::from_utf8_lossy(&o.stderr);
    let noch_da = alt.exists();
    let _ = std::fs::remove_dir_all(&wurzel);
    assert!(aus.contains(&format!("CODE=0 QUELLE={}", wurzel.join("bau").join("release").display())), "{aus}");
    assert!(!noch_da, "release\\zusammenbau ist noch da: {aus}");
}

/// Nachbesserung 2, Punkt 1: Ein Filter, dessen Programm eine Markendatei
/// schriebe, läuft nie: Der Wächter liest die Einstellungen aller Ebenen
/// (auch worktree) und die Attribute, bricht sofort ab und ruft kein git
/// diff und kein git status. Zur Gegenprobe zeigt ein git status von Hand,
/// dass der Filter wirklich liefe.
#[test]
fn git_filter_laeuft_nie() {
    if !bereit() {
        return;
    }
    for ebene in ["local", "worktree", "nur Attribut"] {
        let f = Fall::neu();
        let marke = f.wurzel.join("marke.txt");
        let m = marke.to_string_lossy().replace('\\', "/");
        // Das Attribut gehört zum geprüften Stand (eingecheckt); die Kiste ist sonst unverändert.
        std::fs::write(f.inhalt().join(".gitattributes"), if ebene == "nur Attribut" { "*.rs filter=lfs\n" } else { "*.rs filter=probe\n" }).unwrap();
        git(&f.inhalt(), &["add", ".gitattributes"]);
        git(&f.inhalt(), &["commit", "-q", "-m", "Attribute"]);
        let programm = format!("echo lief > '{m}'; cat");
        match ebene {
            "local" => {
                git(&f.inhalt(), &["config", "filter.probe.clean", &programm]);
            }
            "worktree" => {
                git(&f.inhalt(), &["config", "extensions.worktreeConfig", "true"]);
                git(&f.inhalt(), &["config", "--worktree", "filter.probe.clean", &programm]);
            }
            _ => {}
        }
        // Die Kiste anfassen, damit git diff/status neu rechnen (und den Filter riefen).
        let lib = f.server().join("src").join("lib.rs");
        let alt = std::fs::read_to_string(&lib).unwrap();
        std::fs::write(&lib, &alt).unwrap();
        let s = f.pruefen();
        let erwartet = if ebene == "nur Attribut" { "filter=lfs" } else { "filter.probe.clean" };
        assert!(s.contains(erwartet), "{ebene}: {s}");
        if ebene == "worktree" {
            assert!(s.contains("Ebene worktree"), "{s}");
        }
        assert!(!s.contains("seit dem geprüften Commit") && !s.contains("ungeprüfte Datei"), "{ebene}: kein git diff/status nach rotem Fund: {s}");
        assert!(!marke.exists(), "{ebene}: der Filter lief");
        if ebene != "nur Attribut" {
            // Gegenprobe: git status selbst führte den Filter aus.
            // Gleicher Inhalt, neue Zeit: git muss die Datei durch den Filter neu rechnen.
            std::thread::sleep(std::time::Duration::from_millis(1100));
            std::fs::write(&lib, &alt).unwrap();
            git(&f.server(), &["status", "--porcelain"]);
            assert!(marke.exists(), "{ebene}: Gegenprobe, der Filter liefe bei git status");
        }
    }
}

/// Nachbesserung 2, Punkt 2: dlltool nur als Name, im Ordner der aktiven
/// Toolchain oder als <CARGO_HOME>/bin/llvm-dlltool.exe; nie im Inhalt.
#[test]
fn dlltool_nur_von_sicheren_orten() {
    let orte = zusammenbau::DlltoolOrte {
        toolchain: Some(PathBuf::from(r"C:\rust\toolchains\stable-x86_64-pc-windows-gnu")),
        cargo_bin: Some(PathBuf::from(r"C:\rust\cargo\bin")),
    };
    let inhalt = Path::new(r"C:\Benutzer\dev\dashboard-inhalt");
    for gut in [
        "llvm-dlltool.exe",
        "C:/rust/toolchains/stable-x86_64-pc-windows-gnu/lib/rustlib/x86_64-pc-windows-gnu/bin/llvm-dlltool.exe",
        r"C:\rust\cargo\bin\llvm-dlltool.exe",
    ] {
        assert!(orte.erlaubt(gut, Some(inhalt)), "{gut}");
    }
    for schlecht in [
        r"C:\Benutzer\dev\dashboard-inhalt\werkzeug\llvm-dlltool.exe",
        "C:/Benutzer/dev/dashboard-inhalt/projects/x/llvm-dlltool.exe",
        r"C:\rust\cargo\bin\..\..\..\Benutzer\dev\dashboard-inhalt\llvm-dlltool.exe",
        r"C:\rust\cargo\bin\unter\llvm-dlltool.exe",
        r"C:\fremd\llvm-dlltool.exe",
        r"C:\rust\toolchains\andere\llvm-dlltool.exe",
        "werkzeug/llvm-dlltool.exe",
        r"C:\rust\toolchains\stable-x86_64-pc-windows-gnu\fremd.exe",
    ] {
        assert!(!orte.erlaubt(schlecht, Some(inhalt)), "{schlecht}");
    }
}

#[test]
fn falscher_code_pfad_ist_rot() {
    if !bereit() {
        return;
    }
    rot("Attrappe des Code-Repos", "zeigt nicht genau auf das Code-Repo", |f| {
        let attrappe = f.wurzel.join("dashboard");
        std::fs::create_dir_all(attrappe.join("crates").join("dashboard-projekt")).unwrap();
        let mut l = f.liste();
        l.code = relativ(&f.lokal(), &attrappe);
        f.lokal_schreiben(&l);
    });
    rot("absoluter Weg", "relativer Weg", |f| {
        let mut l = f.liste();
        l.code = code_repo().to_string_lossy().replace('\\', "/");
        f.lokal_schreiben(&l);
    });
    // Punkt 3: SDK-Weg der Kiste in einen gleichnamigen Ordner im Inhalt.
    rot("SDK im Inhalt", "genau dieses Code-Repos", |f| {
        let attrappe = f.inhalt().join("dashboard").join("crates").join("dashboard-projekt");
        std::fs::create_dir_all(&attrappe).unwrap();
        let c = f.server().join("Cargo.toml");
        let alt = std::fs::read_to_string(&c).unwrap();
        let sdk = relativ(&f.server(), &code_repo().join("crates").join("dashboard-projekt"));
        std::fs::write(c, alt.replace(&sdk, "../../../dashboard/crates/dashboard-projekt")).unwrap();
    });
    // Die Kiste liegt nie im Code-Repo.
    let s = zusammenbau::pruefe(&code_repo().join("vorlagen").join("dashboard-lokal"), &code_repo(), false).join("\n");
    assert!(s.contains("nie im Code-Repo"), "{s}");
}

#[test]
fn kisten_toml_streng() {
    for (text, satz) in [
        ("[code]\npfad = \"../x\"\nanders = 1\n", "kennt nur pfad"),
        ("[code]\npfad = \"../x\"\n[fremd]\n", "[fremd] gibt es nicht"),
        ("[code]\npfad = \"../x\"\n[[kiste]]\nname = \"projekt-a\"\npfad = \"../projects/a/server\"\ntyp = \"A\"\n", "braucht name, pfad, typ und commit"),
        (
            "[code]\npfad = \"../x\"\n[[kiste]]\nname = \"fremd\"\npfad = \"../projects/a/server\"\ntyp = \"A\"\ncommit = \"0123abc\"\n",
            "nicht die Form projekt-<slug>",
        ),
        ("[code]\npfad = \"../x\"\n[[kiste]]\nname = \"projekt-a\"\npfad = \"../projects/a/server\"\ntyp = \"A\"\ncommit = \"xyz\"\n", "ist kein Commit"),
        (
            "[code]\npfad = \"../x\"\n[[kiste]]\nname = \"projekt-a\"\npfad = \"../projects/a/server\"\ntyp = \"A\"\ncommit = \"0123abc\"\nbuild = \"x\"\n",
            "kennt build nicht",
        ),
        ("[kiste]\n", "[code] mit pfad fehlt"),
    ] {
        let f = zusammenbau::liste_lesen(text).unwrap_err().join("\n");
        assert!(f.contains(satz), "{text}: {f}");
    }
}

#[test]
fn umgebung_von_cargo() {
    let v = |k: &str| (k.to_string(), "x".to_string());
    for k in [
        "RUSTC_WRAPPER",
        "RUSTFLAGS",
        "CARGO_BUILD_RUSTC_WRAPPER",
        "CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER",
        "CARGO_REGISTRIES_FREMD_INDEX",
        "RUSTUP_TOOLCHAIN",
        "cargo_encoded_rustflags",
    ] {
        assert_eq!(zusammenbau::umgebung_pruefen([v(k)]).len(), 1, "{k}");
    }
    for k in ["CARGO_TARGET_DIR", "CARGO_HOME", "CARGO_TERM_COLOR", "PATH", "RUST_LOG"] {
        assert!(zusammenbau::umgebung_pruefen([v(k)]).is_empty(), "{k}");
    }
    for (k, satz) in [
        ("[unstable]\nbuild-std = []\n", "[unstable]"),
        ("[env]\nX = \"1\"\n", "[env]"),
        ("[target.x86_64-pc-windows-gnu]\nrustflags = [\"-C\", \"linker=fremd.exe\"]\n", "nur rustflags"),
        ("[target.x86_64-pc-windows-gnu]\nrunner = \"fremd.exe\"\n", "runner"),
        ("paths = [\"../fremd\"]\n", "[paths]"),
    ] {
        let f = zusammenbau::konfig_pruefen("x", k).join("\n");
        assert!(f.contains(satz), "{k}: {f}");
    }
}

/// Als Teil von dashboard.exe: 0 grün, 1 rot, 2 falscher Aufruf; gesetzte
/// Umgebung von cargo ist rot.
#[test]
fn aufruf_ueber_dashboard_exe() {
    if !bereit() {
        return;
    }
    let f = Fall::neu();
    let lauf = |args: &[&str], extra: Option<(&str, &str)>| {
        let mut c = programme::eigener_befehl(env!("CARGO_BIN_EXE_dashboard"));
        // Die Umgebung von cargo test selbst (CARGO_PKG_… usw.) nicht mitgeben.
        for (k, _) in std::env::vars() {
            let n = k.to_ascii_uppercase();
            if n.starts_with("CARGO") || n.starts_with("RUST") {
                c.env_remove(&k);
            }
        }
        if let Some((k, w)) = extra {
            c.env(k, w);
        }
        let o = c.arg("pruefe-zusammenbau").args(args).output().unwrap();
        (o.status.code(), String::from_utf8_lossy(&o.stdout).into_owned() + &String::from_utf8_lossy(&o.stderr))
    };
    let lokal = f.lokal().to_string_lossy().into_owned();
    let code = code_repo().to_string_lossy().into_owned();
    let (c, t) = lauf(&[&lokal, "--code", &code], None);
    assert_eq!(c, Some(0), "{t}");
    assert!(t.contains("grün"), "{t}");
    let (c, t) = lauf(&[&lokal, "--code", &code], Some(("RUSTC_WRAPPER", "fremd.exe")));
    assert_eq!(c, Some(1), "{t}");
    assert!(t.contains("RUSTC_WRAPPER"), "{t}");
    std::fs::write(f.lokal().join("build.rs"), "fn main() {}\n").unwrap();
    let (c, t) = lauf(&[&lokal, "--code", &code], None);
    assert_eq!(c, Some(1), "{t}");
    assert!(t.contains("Es wird nicht gebaut"), "{t}");
    let (c, _) = lauf(&[&lokal], None);
    assert_eq!(c, Some(2));
}
