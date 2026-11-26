//! `scripts/Staedte-Umwandeln.ps1` gegen erfundene Eingaben im Temp-Ordner:
//! Textdatei und ZIP-Datei mit GeoNames-Spalten, eine Zeile ohne Zone, eine zu
//! kurze Zeile. Geprüft: die 7 Spalten in Reihenfolge, der Kopf mit Quelle und
//! Prüfsumme der Eingabe, Abbruch ohne Projekt kalender und bei falscher
//! `-Sha256`, Standard für den Inhalt aus DASHBOARD_INHALT. Ohne PowerShell 7
//! wird übersprungen.

#![cfg(windows)]

use std::path::{Path, PathBuf};
use std::process::Output;

use dashboard::dienst::programme::{self, Werkzeug};
use sha2::{Digest, Sha256};

/// Zwei gültige Zeilen (19 Spalten), eine ohne Zone, eine zu kurze.
const EINGABE: &str = "1\tÖrtchen\tOertchen\tOe,Ort Klein\t50.5\t7.25\tP\tPPL\tDE\t\t07\t\t\t\t20000\t\t100\tEurope/Berlin\t2026-01-01\r\n\
2\tZonenlos\tZonenlos\t\t10\t20\tP\tPPL\tXX\t\t01\t\t\t\t20000\t\t5\t\t2026-01-01\r\n\
3\tKurz\tKurz\n\
4\tSüdhafen\tSuedhafen\t\t-33.9\t18.4\tP\tPPLA\tZA\t\t11\t\t\t\t500000\t\t20\tAfrica/Johannesburg\t2026-01-01\n";
const LISTE: &str = "Örtchen\tOe,Ort Klein\tDE\t07\t50.5\t7.25\tEurope/Berlin\nSüdhafen\t\tZA\t11\t-33.9\t18.4\tAfrica/Johannesburg\n";

fn skript() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scripts").join("Staedte-Umwandeln.ps1")
}

fn hex(b: &[u8]) -> String {
    Sha256::digest(b).iter().map(|x| format!("{x:02x}")).collect()
}

struct Ort(PathBuf);

impl Drop for Ort {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Das Skript mit diesen Argumenten; `inhalt_env` setzt DASHBOARD_INHALT.
fn laufen(args: &[&str], inhalt_env: Option<&Path>) -> Option<Output> {
    let mut c = programme::befehl(Werkzeug::PowerShell7).ok()?;
    c.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"]).arg(skript()).args(args);
    match inhalt_env {
        Some(p) => c.env("DASHBOARD_INHALT", p),
        None => c.env_remove("DASHBOARD_INHALT"),
    };
    Some(c.output().unwrap())
}

fn text(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned() + &String::from_utf8_lossy(&o.stderr)
}

fn kopf(inhalt: &Path) -> String {
    std::fs::read_to_string(inhalt.join("projects/kalender/staedte/staedte-kopf.toml")).unwrap()
}

#[test]
fn umwandeln_txt_zip_und_abbrueche() {
    let ort = Ort(std::env::temp_dir().join(format!("dashboard-staedte-umwandeln-{}", std::process::id())));
    let _ = std::fs::remove_dir_all(&ort.0);
    let inhalt = ort.0.join("inhalt");
    let staedte = inhalt.join("projects/kalender/staedte");
    std::fs::create_dir_all(inhalt.join("projects/kalender")).unwrap();
    std::fs::write(inhalt.join("projects/kalender/projekt.toml"), "titel = \"Kalender\"\n").unwrap();
    let txt = ort.0.join("cities15000.txt");
    std::fs::write(&txt, EINGABE).unwrap();
    let txt_s = txt.to_str().unwrap();
    let inhalt_s = inhalt.to_str().unwrap();

    // Textdatei: 7 Spalten, Zeile ohne Zone und zu kurze Zeile übersprungen, Kopf ehrlich.
    let Some(o) = laufen(&["-Quelle", txt_s, "-Inhalt", inhalt_s, "-Abgerufen", "2027-01-02"], None) else {
        dashboard::ueberspringen("keine PowerShell 7 gefunden");
        return;
    };
    assert_eq!(o.status.code(), Some(0), "txt: {}", text(&o));
    // Nur Ziffern und Satzzeichen: die Umlaute hängen an der Kodierung der Konsole.
    assert!(text(&o).contains(": 2, ") && text(&o).contains(": 2."), "{}", text(&o));
    assert!(text(&o).contains("Laufzeit"), "{}", text(&o));
    let liste = std::fs::read(staedte.join("staedte.tsv")).unwrap();
    assert_eq!(String::from_utf8(liste.clone()).unwrap(), LISTE);
    let k = kopf(&inhalt);
    assert!(k.contains("quelle = \"lokale Datei cities15000.txt\"\n"), "{k}");
    assert!(k.contains("abgerufen = 2027-01-02\n") && k.contains("lizenz = \"CC BY 4.0\"\n"), "{k}");
    assert!(k.contains(&format!("sha256 = \"{}\"\n", hex(EINGABE.as_bytes()))), "{k}");
    assert!(k.contains(&format!("sha256_liste = \"{}\"\n", hex(&liste))), "{k}");

    // ZIP-Datei über DASHBOARD_INHALT als Standard, mit passender -Sha256: Adresse von GeoNames, Prüfsumme der ZIP.
    let zip = ort.0.join("cities15000.zip");
    let mut c = programme::befehl(Werkzeug::PowerShell7).unwrap();
    let q = |p: &str| format!("'{}'", p.replace('\'', "''"));
    let befehl = format!("Compress-Archive -LiteralPath {} -DestinationPath {}", q(txt_s), q(zip.to_str().unwrap()));
    let packen = c.args(["-NoProfile", "-NonInteractive", "-Command", &befehl]).output().unwrap();
    assert!(packen.status.success(), "{}", text(&packen));
    std::fs::remove_dir_all(&staedte).unwrap();
    let zip_hash = hex(&std::fs::read(&zip).unwrap());
    let o = laufen(&["-Quelle", zip.to_str().unwrap(), "-Abgerufen", "2027-01-02", "-Sha256", &zip_hash.to_uppercase()], Some(&inhalt)).unwrap();
    assert_eq!(o.status.code(), Some(0), "zip: {}", text(&o));
    assert_eq!(std::fs::read_to_string(staedte.join("staedte.tsv")).unwrap(), LISTE);
    let k = kopf(&inhalt);
    assert!(k.contains("quelle = \"https://download.geonames.org/export/dump/cities15000.zip\"\n"), "{k}");
    assert!(k.contains(&format!("sha256 = \"{zip_hash}\"\n")), "{k}");

    // Falsche -Sha256: Abbruch, nichts geschrieben.
    std::fs::remove_dir_all(&staedte).unwrap();
    let o = laufen(&["-Quelle", zip.to_str().unwrap(), "-Inhalt", inhalt_s, "-Sha256", &"0".repeat(64)], None).unwrap();
    assert_eq!(o.status.code(), Some(1), "sha256: {}", text(&o));
    assert!(text(&o).contains("weicht von -Sha256 ab"), "{}", text(&o));
    assert!(!staedte.exists());

    // Ohne Projekt kalender: Abbruch, nichts geschrieben.
    let leer = ort.0.join("leer");
    std::fs::create_dir_all(&leer).unwrap();
    let o = laufen(&["-Quelle", txt_s, "-Inhalt", leer.to_str().unwrap()], None).unwrap();
    assert_eq!(o.status.code(), Some(1), "ohne projekt: {}", text(&o));
    assert!(text(&o).contains("kein Projekt kalender"), "{}", text(&o));
    assert!(!leer.join("projects").exists());
}
