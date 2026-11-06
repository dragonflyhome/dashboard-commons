//! Funktionsprüfung von `Inhalt-Unsicher` (scripts/Inhalt-Orte.ps1), die
//! Dienst-Einrichten.ps1 -Inhalt und Inhalt-Umziehen.ps1 nutzen (Nachbesserung
//! zum Umzug des Inhalts, Punkt 6). Nur mit Wegwerf-Pfaden im Temp-Ordner: ein
//! erfundenes Profil, ein erfundenes Code-Repo darin und eine Verknüpfung.
//! Läuft in PowerShell 7 und in Windows PowerShell 5.1 (darin läuft
//! Dienst-Einrichten.ps1 als Administrator).

use std::path::PathBuf;

use dashboard::dienst::programme::{self, Werkzeug};

fn skript() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scripts").join("Inhalt-Orte.ps1")
}

/// Ruft Inhalt-Unsicher für jeden Fall auf; je Fall eine Zeile „name=OK“ oder
/// „name=ROT“.
fn laufen(w: Werkzeug, profil: &str, repo: &str, faelle: &[(&str, String)]) -> Option<String> {
    let mut c = programme::befehl(w).ok()?;
    let mut s = format!(". '{}'\n", skript().display());
    for (name, pfad) in faelle {
        s.push_str(&format!(
            "$u = Inhalt-Unsicher '{}' '{}' '{}'; if ($u) {{ Write-Output '{name}=ROT' }} else {{ Write-Output '{name}=OK' }}\n",
            pfad.replace('\'', "''"),
            profil,
            repo
        ));
    }
    let o = c.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", &s]).output().ok()?;
    Some(String::from_utf8_lossy(&o.stdout).into_owned() + &String::from_utf8_lossy(&o.stderr))
}

#[test]
fn inhalt_ordner_nur_sicher() {
    let basis = std::env::temp_dir().join(format!("dashboard-inhalt-orte-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&basis);
    let profil = basis.join("profil");
    let dev = profil.join("dev");
    let repo = dev.join("code");
    std::fs::create_dir_all(&repo).unwrap();
    std::fs::create_dir_all(dev.join("echt")).unwrap();
    let junction = dashboard_junction(&profil.join("verweis"), &dev);
    let p = |rel: &str| format!("{}\\{rel}", profil.display());
    let mut faelle: Vec<(&str, String, bool)> = vec![
        ("gut", p("dev\\inhalt"), true),
        ("gut_schraeg", p("dev\\inhalt").replace('\\', "/"), true),
        ("gut_da", p("dev\\echt"), true),
        ("relativ", "dev\\inhalt".into(), false),
        ("punktpunkt", p("dev\\..\\dev\\inhalt"), false),
        ("punkt", p("dev\\.\\inhalt"), false),
        ("laufwerk", "C:\\".into(), false),
        ("unc", "\\\\server\\freigabe\\inhalt".into(), false),
        ("profil", profil.display().to_string(), false),
        ("ausserhalb", basis.join("anders").display().to_string(), false),
        ("repo", repo.display().to_string(), false),
        ("im_repo", format!("{}\\inhalt", repo.display()), false),
        ("ueber_repo", dev.display().to_string(), false),
        ("kurzname", p("dev\\INHALT~1"), false),
        ("leer", String::new(), false),
    ];
    if junction {
        faelle.push(("verknuepfung", p("verweis\\inhalt"), false));
    }
    let liste: Vec<(&str, String)> = faelle.iter().map(|(n, p, _)| (*n, p.clone())).collect();
    let mut gelaufen = 0;
    for w in [Werkzeug::PowerShell7, Werkzeug::WindowsPowerShell] {
        let Some(aus) = laufen(w, &profil.display().to_string(), &repo.display().to_string(), &liste) else { continue };
        gelaufen += 1;
        for (name, _, gut) in &faelle {
            let soll = format!("{name}={}", if *gut { "OK" } else { "ROT" });
            assert!(aus.lines().any(|z| z.trim() == soll), "{}: erwartet {soll}:\n{aus}", w.name());
        }
    }
    let _ = std::fs::remove_dir_all(&basis);
    if gelaufen == 0 {
        dashboard::ueberspringen("keine PowerShell gefunden");
    }
}

/// Eine Junction ohne Administrator (mklink /J); `false`, wenn es nicht ging.
fn dashboard_junction(link: &std::path::Path, ziel: &std::path::Path) -> bool {
    let Ok(mut c) = programme::befehl(Werkzeug::Cmd) else { return false };
    c.args(["/c", "mklink", "/J"]).arg(link).arg(ziel).output().is_ok_and(|o| o.status.success()) && link.exists()
}
