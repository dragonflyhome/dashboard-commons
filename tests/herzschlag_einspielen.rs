//! `scripts/Herzschlag-Einspielen.ps1` gegen Wegwerf-Wurzeln: eine
//! Einstellungsdatei im Temp-Ordner und ein eigener Registry-Schlüssel unter
//! HKCU\Software statt der Benutzer-Umgebung. Ein, zweimal Ein, Aus, fremde
//! Hooks bleiben, kaputtes JSON bricht ohne Schreiben ab. Dazu die Prüfung,
//! dass die echte Benutzer-Umgebung dabei unberührt bleibt (nur gelesen).
//! Das Skript verlangt PowerShell 7; ohne sie wird übersprungen.

#![cfg(windows)]

use std::path::{Path, PathBuf};
use std::process::Output;

use dashboard::dienst::programme::{self, Werkzeug};
use serde_json::{Value, json};
use winreg::RegKey;
use winreg::enums::{HKEY_CURRENT_USER, KEY_READ};

const NAME: &str = "DASHBOARD_SITZUNG_VARIABLE";

fn skript() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scripts").join("Herzschlag-Einspielen.ps1")
}

/// Der Wert in der echten Benutzer-Umgebung, nur gelesen.
fn echt() -> Option<String> {
    RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags("Environment", KEY_READ).ok()?.get_value(NAME).ok()
}

struct Ort {
    ordner: PathBuf,
    schluessel: String,
}

impl Ort {
    fn neu() -> Ort {
        let ordner = std::env::temp_dir().join(format!("dashboard-herzschlag-einspielen-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&ordner);
        std::fs::create_dir_all(ordner.join("bin")).unwrap();
        std::fs::write(ordner.join("bin").join("dashboard-herzschlag.exe"), b"").unwrap();
        let schluessel = format!("Software\\dashboard-herzschlag-test-{}", std::process::id());
        RegKey::predef(HKEY_CURRENT_USER).create_subkey(&schluessel).unwrap();
        Ort { ordner, schluessel }
    }

    fn datei(&self) -> PathBuf {
        self.ordner.join("einstellungen.json")
    }

    fn laufen(&self, args: &[&str]) -> Option<Output> {
        let mut c = programme::befehl(Werkzeug::PowerShell7).ok()?;
        let umgebung = format!("HKCU:\\{}", self.schluessel);
        let programm = self.ordner.join("bin").join("dashboard-herzschlag.exe");
        c.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"]).arg(skript());
        c.arg("-Umgebung").arg(umgebung).arg("-Programm").arg(programm).args(args);
        Some(c.output().unwrap())
    }

    fn wert(&self) -> Option<String> {
        RegKey::predef(HKEY_CURRENT_USER).open_subkey(&self.schluessel).ok()?.get_value(NAME).ok()
    }

    fn json(&self) -> Value {
        serde_json::from_slice(&std::fs::read(self.datei()).unwrap()).unwrap()
    }

    fn sicherungen(&self) -> Vec<PathBuf> {
        let mut v: Vec<PathBuf> =
            std::fs::read_dir(&self.ordner).unwrap().filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|e| e == "bak")).collect();
        v.sort();
        v
    }
}

impl Drop for Ort {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.ordner);
        let _ = RegKey::predef(HKEY_CURRENT_USER).delete_subkey_all(&self.schluessel);
    }
}

fn gut(o: &Output, fall: &str) -> String {
    let text = String::from_utf8_lossy(&o.stdout).into_owned() + &String::from_utf8_lossy(&o.stderr);
    assert_eq!(o.status.code(), Some(0), "{fall}: {text}");
    text
}

fn rot(o: &Output, fall: &str) -> String {
    let text = String::from_utf8_lossy(&o.stdout).into_owned() + &String::from_utf8_lossy(&o.stderr);
    assert_eq!(o.status.code(), Some(1), "{fall}: {text}");
    text
}

fn eigene(v: &Value) -> Vec<Value> {
    v["hooks"]["Stop"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|g| g["hooks"].as_array().cloned().unwrap_or_default())
        .filter(|h| h["command"].as_str().is_some_and(|c| c.contains("dashboard-herzschlag.exe")))
        .collect()
}

fn bytes(p: &Path) -> Vec<u8> {
    std::fs::read(p).unwrap()
}

#[test]
fn einspielen_ein_ein_aus_und_kaputtes_json() {
    let vorher_echt = echt();
    let ort = Ort::neu();
    let fremd = json!({
        "model": "x",
        "datum": "2026-01-01T00:00:00Z",
        "zahl": 1.5,
        "hooks": {
            "Stop": [{"hooks": [{"type": "command", "command": "fremd.exe"}]}],
            "PreToolUse": [{"matcher": "Werkzeug", "hooks": [{"type": "command", "command": "p.exe"}]}]
        }
    });
    std::fs::write(ort.datei(), serde_json::to_vec_pretty(&fremd).unwrap()).unwrap();
    let urtext = bytes(&ort.datei());

    // Ohne -Einstellungen: kein Standard, nichts geschrieben.
    let Some(o) = ort.laufen(&["-Variable", "TEST_SITZUNG"]) else {
        dashboard::ueberspringen("keine PowerShell 7 gefunden");
        return;
    };
    assert!(rot(&o, "ohne -Einstellungen").contains("-Einstellungen"));
    assert_eq!(ort.wert(), None);

    let datei = ort.datei().display().to_string();
    // Ein.
    gut(&ort.laufen(&["-Einstellungen", &datei, "-Variable", "TEST_SITZUNG"]).unwrap(), "Ein");
    let ein = ort.json();
    let e = eigene(&ein);
    assert_eq!(e.len(), 1, "{ein:#}");
    assert_eq!(e[0]["type"], "command");
    assert_eq!(e[0]["timeout"], 30);
    let befehl = e[0]["command"].as_str().unwrap();
    assert!(befehl.starts_with('"') && befehl.ends_with("/bin/dashboard-herzschlag.exe\"") && !befehl.contains('\\'), "{befehl}");
    assert_eq!(ein["hooks"]["Stop"][0], fremd["hooks"]["Stop"][0], "fremder Stop-Hook bleibt vorne");
    assert_eq!(ein["hooks"]["PreToolUse"], fremd["hooks"]["PreToolUse"]);
    for k in ["model", "datum", "zahl"] {
        assert_eq!(ein[k], fremd[k], "{k}");
    }
    assert_eq!(ort.wert().as_deref(), Some("TEST_SITZUNG"));
    let bak = ort.sicherungen();
    assert_eq!(bak.len(), 1, "{bak:?}");
    assert_eq!(std::fs::read(&bak[0]).unwrap(), urtext, "Sicherung ist die Datei vor der Änderung");

    // Zweimal Ein: nichts geschrieben, keine weitere Sicherung.
    let stand = bytes(&ort.datei());
    let text = gut(&ort.laufen(&["-Einstellungen", &datei, "-Variable", "TEST_SITZUNG"]).unwrap(), "Ein 2");
    assert!(text.contains("schon eingetragen"), "{text}");
    assert_eq!(bytes(&ort.datei()), stand);
    assert_eq!(ort.sicherungen().len(), 1);

    // Aus: nur der eigene Eintrag und die Variable gehen.
    gut(&ort.laufen(&["-Einstellungen", &datei, "-Aus"]).unwrap(), "Aus");
    assert_eq!(ort.json(), fremd, "nach Aus wie vorher");
    assert_eq!(ort.wert(), None);
    assert_eq!(ort.sicherungen().len(), 2);
    let text = gut(&ort.laufen(&["-Einstellungen", &datei, "-Aus"]).unwrap(), "Aus 2");
    assert!(text.contains("schon ausgetragen") && text.contains("schon entfernt"), "{text}");

    // Kaputtes JSON und eine fremde Form von hooks: Abbruch ohne Schreiben.
    for (fall, inhalt) in [("kaputt", "{ \"hooks\": "), ("kommentar", "// x\n{}"), ("form", "{\"hooks\": []}"), ("gruppe", "{\"hooks\": {\"Stop\": [1]}}")] {
        std::fs::write(ort.datei(), inhalt).unwrap();
        let zahl = ort.sicherungen().len();
        let text = rot(&ort.laufen(&["-Einstellungen", &datei, "-Variable", "TEST_SITZUNG"]).unwrap(), fall);
        assert!(text.contains("Nichts geändert"), "{fall}: {text}");
        assert_eq!(std::fs::read_to_string(ort.datei()).unwrap(), inhalt, "{fall}");
        assert_eq!(ort.sicherungen().len(), zahl, "{fall}");
        assert_eq!(ort.wert(), None, "{fall}");
    }

    // Ohne Datei: Ein legt sie an, Aus lässt ein leeres Objekt.
    std::fs::remove_file(ort.datei()).unwrap();
    gut(&ort.laufen(&["-Einstellungen", &datei, "-Variable", "TEST_SITZUNG"]).unwrap(), "Ein neu");
    assert_eq!(eigene(&ort.json()).len(), 1);
    gut(&ort.laufen(&["-Einstellungen", &datei, "-Aus"]).unwrap(), "Aus neu");
    assert_eq!(ort.json(), json!({}));

    drop(ort);
    assert_eq!(echt(), vorher_echt, "die echte Benutzer-Umgebung bleibt unberührt");
}
