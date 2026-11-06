//! Handgepflegte Eckdaten für die PC-Seite aus `data/pc.toml`, dazu der
//! Backup-Stand (`BackupLage`): die Kopien mit Stand und wie weit die
//! 3-2-1-Regel erfüllt ist.

use std::path::Path;

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::vertrag::lesen::{LeseFehler, toml_lesen};

/// Name der Datei in Meldungen.
pub const DATEI: &str = "data/pc.toml";

#[derive(Debug, Clone, Serialize, Deserialize, Default, TS)]
#[serde(deny_unknown_fields)]
pub struct PcData {
    #[serde(default)]
    pub backup: Backup,
    #[serde(default)]
    pub history: Vec<HistoryEntry>,
    #[serde(default)]
    pub steps: Vec<Step>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, TS)]
#[serde(deny_unknown_fields)]
pub struct Backup {
    /// Veraltet: zählt nicht mehr. Kopien und 3-2-1-Regel kommen aus
    /// `external_disk` und `cloud_stand`. Wird weiter gelesen, damit alte
    /// pc.toml-Dateien gültig bleiben.
    #[serde(default)]
    pub copies: u32,
    #[serde(default)]
    pub external_disk: bool,
    /// Freier Text zur Cloud-Kopie (wo, wie viel).
    #[serde(default)]
    pub cloud: String,
    /// Stand der Cloud-Kopie: `vorhanden`, `teilweise` oder `fehlt`.
    /// Fehlt das Feld, gilt `teilweise`, wenn `cloud` etwas sagt, sonst `fehlt`
    /// (das rechnet der Server in `cloud_wirksam`).
    #[serde(default)]
    pub cloud_stand: Option<KopieStand>,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub plan: Vec<String>,
}

impl Backup {
    /// Der Cloud-Stand, der zählt: `cloud_stand`, sonst aus `cloud` geschlossen.
    pub fn cloud_wirksam(&self) -> KopieStand {
        match self.cloud_stand {
            Some(s) => s,
            None if !self.cloud.trim().is_empty() => KopieStand::Teilweise,
            None => KopieStand::Fehlt,
        }
    }

    /// Die drei Kopien und die 3-2-1-Regel (drei Kopien, zwei Medien, eine
    /// außer Haus), nur aus den Fakten, nicht aus `copies`. Die Kopie auf
    /// dem PC ist immer vorhanden, das zweite Medium aus `external_disk`, der
    /// andere Ort aus [`Backup::cloud_wirksam`].
    pub fn lage(&self) -> BackupLage {
        let medium = if self.external_disk { KopieStand::Vorhanden } else { KopieStand::Fehlt };
        let cloud_text = self.cloud.trim();
        let kopien = vec![
            BackupKopie { ort: KopieOrt::Pc, stand: KopieStand::Vorhanden, text: None },
            BackupKopie { ort: KopieOrt::ZweitesMedium, stand: medium, text: None },
            BackupKopie { ort: KopieOrt::AndererOrt, stand: self.cloud_wirksam(), text: (!cloud_text.is_empty()).then(|| cloud_text.to_string()) },
        ];
        let regel_321 = Regel321::aus(&kopien);
        BackupLage { kopien, regel_321 }
    }
}

/// Der Backup-Stand: die drei Kopien und wie weit die 3-2-1-Regel erfüllt ist.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct BackupLage {
    /// Die Kopien in der Ordnung der 3-2-1-Regel: PC, zweites Medium,
    /// anderer Ort.
    pub kopien: Vec<BackupKopie>,
    pub regel_321: Regel321,
}

/// Eine Kopie: wo sie liegt und ob es sie gibt.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct BackupKopie {
    pub ort: KopieOrt,
    pub stand: KopieStand,
    /// Notiz aus `data/pc.toml`, soweit es eine gibt; beim anderen Ort der
    /// Text aus `cloud` (wo, wie viel, seit wann).
    pub text: Option<String>,
}

/// Wo eine Kopie liegt, nach der 3-2-1-Regel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum KopieOrt {
    /// Die Arbeitsdaten auf diesem PC.
    Pc,
    /// Ein zweites Medium im Haus (externe Platte).
    ZweitesMedium,
    /// Ein anderer Ort außer Haus (Cloud).
    AndererOrt,
}

/// Stand einer Kopie; dieselben Wörter gelten für `cloud_stand` in `[backup]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum KopieStand {
    Vorhanden,
    Teilweise,
    Fehlt,
}

/// Wie weit die 3-2-1-Regel erfüllt ist.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum Regel321 {
    /// Alle drei Kopien sind vorhanden.
    Erfuellt,
    /// Außer der auf dem PC ist mindestens eine Kopie vorhanden oder teilweise.
    Teilweise,
    /// Nur die Kopie auf dem PC.
    NichtErfuellt,
}

impl Regel321 {
    pub fn aus(kopien: &[BackupKopie]) -> Self {
        if kopien.len() == 3 && kopien.iter().all(|k| k.stand == KopieStand::Vorhanden) {
            Self::Erfuellt
        } else if kopien.iter().any(|k| k.ort != KopieOrt::Pc && k.stand != KopieStand::Fehlt) {
            Self::Teilweise
        } else {
            Self::NichtErfuellt
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct HistoryEntry {
    /// Stabile Kennung (Etappe 2e): Kleinbuchstaben, Ziffern, Bindestriche,
    /// in der Datei nur einmal. Fehlt sie (alte Form, bis Schritt 6 erlaubt),
    /// bildet der Verlauf sie aus Datum und Titel; die Prüfung gibt einen Hinweis.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub kennung: Option<String>,
    pub date: String,
    pub title: String,
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Step {
    pub title: String,
    #[serde(default = "default_step_status")]
    pub status: String,
    #[serde(default)]
    pub when: String,
    #[serde(default)]
    pub note: String,
}

fn default_step_status() -> String {
    "offen".to_string()
}

/// Liest den Text von `data/pc.toml`; Fehler als deutscher Satz mit Zeile.
pub fn lesen(text: &str) -> Result<PcData, LeseFehler> {
    toml_lesen(DATEI, text)
}

/// Der Satz, wenn sich die Datei nicht lesen lässt (wie bei [`load`]). Er
/// nennt `data/pc.toml`, nie den ganzen Pfad: der enthält den Windows-
/// Benutzernamen, und `/api/pc` ist frei lesbar.
pub fn lesefehler_satz(e: &std::io::Error) -> String {
    format!("{DATEI} lässt sich nicht lesen: {e}")
}

pub fn load(path: &Path) -> Result<PcData> {
    let text = std::fs::read_to_string(path).map_err(|e| anyhow!(lesefehler_satz(&e)))?;
    Ok(lesen(&text)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn backup(text: &str) -> Backup {
        lesen(&format!("[backup]\n{text}")).unwrap().backup
    }

    #[test]
    fn repo_file_parses() {
        let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/beispiel/data/pc.toml");
        let d = load(&p).unwrap();
        assert_eq!(d.history.len(), 3);
        assert_eq!(d.steps.len(), 3);
        assert_eq!(d.backup.cloud_stand, Some(KopieStand::Fehlt));
        assert_eq!(d.backup.lage().regel_321, Regel321::NichtErfuellt);
    }

    /// Aus der Abnahme des Backups: `/api/pc` ist frei lesbar, der Satz darf
    /// den ganzen Pfad (mit dem Windows-Benutzernamen) nicht nennen.
    #[test]
    fn lesefehler_nennt_nur_data_pc_toml() {
        let ordner = std::env::temp_dir().join(format!("dashboard-test-{}-pc-lesefehler", std::process::id()));
        let pfad = ordner.join("geheimer-name").join("pc.toml");
        let satz = load(&pfad).unwrap_err().to_string();
        assert!(satz.starts_with("data/pc.toml lässt sich nicht lesen: "), "{satz}");
        assert!(!satz.contains("geheimer-name") && !satz.contains("dashboard-test"), "{satz}");
        let e = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "Zugriff verweigert");
        assert_eq!(lesefehler_satz(&e), "data/pc.toml lässt sich nicht lesen: Zugriff verweigert");
    }

    #[test]
    fn kennung_in_der_historie() {
        let d =
            lesen("[[history]]\nkennung = \"netzteil\"\ndate = \"2026-10-12\"\ntitle = \"Netzteil\"\n\n[[history]]\ndate = \"2026-10-23\"\ntitle = \"Alt\"\n")
                .unwrap();
        assert_eq!(d.history[0].kennung.as_deref(), Some("netzteil"));
        assert_eq!(d.history[1].kennung, None);
        let json = serde_json::to_value(&d.history).unwrap();
        assert!(json[1].get("kennung").is_none(), "ohne kennung bleibt das Feld weg: {json}");
    }

    #[test]
    fn cloud_stand_gelesen_und_vorgabe() {
        assert_eq!(backup("cloud_stand = \"vorhanden\"").cloud_wirksam(), KopieStand::Vorhanden);
        assert_eq!(backup("cloud_stand = \"teilweise\"").cloud_wirksam(), KopieStand::Teilweise);
        // Ausdrücklich `fehlt` gewinnt auch gegen einen Text in `cloud`.
        assert_eq!(backup("cloud = \"Cloud\"\ncloud_stand = \"fehlt\"").cloud_wirksam(), KopieStand::Fehlt);
        // Ohne Feld: Text in `cloud` heißt teilweise, sonst fehlt.
        assert_eq!(backup("cloud = \"Cloud-Speicher: 40,2 GB\"").cloud_stand, None);
        assert_eq!(backup("cloud = \"Cloud-Speicher: 40,2 GB\"").cloud_wirksam(), KopieStand::Teilweise);
        assert_eq!(backup("cloud = \"  \"").cloud_wirksam(), KopieStand::Fehlt);
        assert_eq!(backup("copies = 2").cloud_wirksam(), KopieStand::Fehlt);
    }

    #[test]
    fn falscher_cloud_stand_ist_ein_satz_mit_zeile() {
        let e = lesen("[backup]\ncopies = 1\ncloud_stand = \"voll\"\n").unwrap_err();
        assert_eq!(e.to_string(), "data/pc.toml, Zeile 3: Bei `cloud_stand` steht „voll“. Erlaubt sind: vorhanden, teilweise, fehlt.");
    }

    fn staende(text: &str) -> Vec<KopieStand> {
        backup(text).lage().kopien.iter().map(|k| k.stand).collect()
    }

    fn regel(text: &str) -> Regel321 {
        backup(text).lage().regel_321
    }

    #[test]
    fn kopien_aus_den_fakten() {
        use KopieStand::*;
        // Der gemeldete Fall: zwei Kopien gezählt, aber keine Platte. Die
        // Platte darf nicht als vorhanden gelten.
        assert_eq!(staende("copies = 2\nexternal_disk = false"), [Vorhanden, Fehlt, Fehlt]);
        assert_eq!(staende("copies = 1\ncloud = \"Cloud-Speicher\""), [Vorhanden, Fehlt, Teilweise]);
        assert_eq!(staende("external_disk = true\ncloud_stand = \"vorhanden\""), [Vorhanden, Vorhanden, Vorhanden]);
        let l = backup("cloud = \"Cloud-Speicher: 40,2 GB\"").lage();
        let orte: Vec<_> = l.kopien.iter().map(|k| (k.ort, k.text.as_deref())).collect();
        assert_eq!(orte, [(KopieOrt::Pc, None), (KopieOrt::ZweitesMedium, None), (KopieOrt::AndererOrt, Some("Cloud-Speicher: 40,2 GB"))]);
        let json = serde_json::to_value(&l).unwrap();
        assert_eq!(json["kopien"][1]["ort"], "zweites-medium");
        assert_eq!(json["kopien"][2]["stand"], "teilweise");
        assert_eq!(json["regel_321"], "teilweise");
    }

    #[test]
    fn regel_321_aus_den_fakten() {
        use Regel321::*;
        assert_eq!(regel(""), NichtErfuellt);
        assert_eq!(regel("copies = 2"), NichtErfuellt);
        assert_eq!(regel("cloud = \"Cloud-Speicher\""), Teilweise);
        assert_eq!(regel("cloud_stand = \"teilweise\""), Teilweise);
        assert_eq!(regel("cloud_stand = \"vorhanden\""), Teilweise);
        assert_eq!(regel("external_disk = true"), Teilweise);
        assert_eq!(regel("external_disk = true\ncloud_stand = \"teilweise\""), Teilweise);
        assert_eq!(regel("external_disk = true\ncloud_stand = \"vorhanden\""), Erfuellt);
        assert_eq!(serde_json::to_value(NichtErfuellt).unwrap(), "nicht-erfuellt");
    }
}
