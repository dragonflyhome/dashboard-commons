//! Namensraum der Backend-Module im Datenspeicher (Etappe 4b).
//!
//! Jedes Modul nennt seine Tabellen in `Modul::tabellen()`. Beim Start legt
//! `projekt_tabellen_einrichten` sie als `projekt/<slug>/<name>` an (Schlüssel
//! Text, Wert JSON) und trägt sie mit Fassung ins Verzeichnis
//! `projekt_tabellen` ein. Nur was dort steht, lässt sich lesen und schreiben;
//! anlegen kann ein Modul eine Tabelle nur über `tabellen()`.
//!
//! Steigt die Fassung einer Tabelle, wird erst gesichert
//! (`dashboard-JJJJ-MM-TT-vor-projekt-<slug>-<name>-<alt>.redb`), dann jeder
//! Eintrag mit dem Umbauschritt des Moduls umgebaut, alles in einer
//! Transaktion. Ist die Fassung in der Datei höher als im Modul, bleibt die
//! Tabelle unberührt, und das Modul bekommt einen Fehler mit Satz.
//!
//! Die Tabellen stehen auf /daten, in der Sicherung und im Export (siehe
//! `tabellen_info`, `sicherung::kopie_schreiben`, `export`).

use dashboard_projekt::tabellen::{self as modul_tabellen, TabellenDef};
use redb::{ReadTransaction, ReadableDatabase, ReadableTable, ReadableTableMetadata, TableDefinition, WriteTransaction};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use super::tabellen::{self, PROJEKT_TABELLEN};
use super::{Speicher, SpeicherFehler};

/// Eintrag im Verzeichnis `projekt_tabellen` (Schlüssel `<slug>/<name>`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjektTabelle {
    pub slug: String,
    pub name: String,
    pub fassung: u32,
    pub beschreibung: String,
    /// Wann sie angelegt wurde (Ortszeit, RFC 3339).
    pub angelegt: String,
}

impl ProjektTabelle {
    /// `projekt/<slug>/<name>`.
    pub fn voller_name(&self) -> String {
        modul_tabellen::voller_name(&self.slug, &self.name)
    }
}

/// Was beim Einrichten mit einer Tabelle geschah.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Einrichtung {
    Angelegt,
    Unveraendert,
    Umgebaut { von: u32, nach: u32, eintraege: u64 },
}

/// Die redb-Definition zu einem vollen Namen.
pub fn definition(voll: &str) -> TableDefinition<'_, &'static str, &'static [u8]> {
    TableDefinition::new(voll)
}

fn verzeichnis_schluessel(slug: &str, name: &str) -> String {
    format!("{slug}/{name}")
}

/// Alle Einträge des Verzeichnisses (fehlt es, leer).
pub fn verzeichnis(txn: &ReadTransaction) -> Result<Vec<ProjektTabelle>, SpeicherFehler> {
    let Some(t) = tabellen::lesen(PROJEKT_TABELLEN, txn)? else { return Ok(Vec::new()) };
    let mut out = Vec::new();
    for e in t.iter()? {
        let (k, w) = e?;
        match serde_json::from_slice::<ProjektTabelle>(w.value()) {
            Ok(p) if modul_tabellen::ist_name(&p.slug) && modul_tabellen::ist_name(&p.name) => out.push(p),
            Ok(_) => warn!("projekt_tabellen: Eintrag {} hat ungültige Namen", k.value()),
            Err(e) => warn!("projekt_tabellen: Eintrag {} unlesbar: {e}", k.value()),
        }
    }
    Ok(out)
}

fn eintrag_im_verzeichnis(txn: &ReadTransaction, slug: &str, name: &str) -> Result<Option<ProjektTabelle>, SpeicherFehler> {
    let Some(t) = tabellen::lesen(PROJEKT_TABELLEN, txn)? else { return Ok(None) };
    match t.get(verzeichnis_schluessel(slug, name).as_str())? {
        Some(w) => Ok(Some(serde_json::from_slice(w.value())?)),
        None => Ok(None),
    }
}

/// Inhalt einer Modul-Tabelle für den Export: Schlüssel → Wert (JSON;
/// unlesbare Werte als Text).
pub fn exportieren(txn: &ReadTransaction, voll: &str) -> Result<serde_json::Value, SpeicherFehler> {
    let mut karte = serde_json::Map::new();
    if let Some(t) = tabellen::lesen(definition(voll), txn)? {
        for e in t.iter()? {
            let (k, w) = e?;
            let wert = serde_json::from_slice(w.value()).unwrap_or_else(|_| serde_json::Value::String(String::from_utf8_lossy(w.value()).into_owned()));
            karte.insert(k.value().to_string(), wert);
        }
    }
    Ok(serde_json::Value::Object(karte))
}

fn nicht_eingerichtet(slug: &str, name: &str) -> SpeicherFehler {
    SpeicherFehler::Hintergrund(format!("Die Tabelle projekt/{slug}/{name} ist nicht eingerichtet (nur Tabellen aus tabellen() des Moduls)."))
}

/// Baut alle Einträge einer Tabelle in einer Schreibtransaktion um.
fn umbauen_in(txn: &WriteTransaction, voll: &str, def: &TabellenDef, alt: u32) -> Result<u64, SpeicherFehler> {
    let mut t = txn.open_table(definition(voll))?;
    let mut neu: Vec<(String, Vec<u8>)> = Vec::new();
    for e in t.iter()? {
        let (k, w) = e?;
        let schluessel = k.value().to_string();
        let wert: serde_json::Value = serde_json::from_slice(w.value())?;
        let wert = modul_tabellen::eintrag_umbauen(def, alt, wert).map_err(|e| SpeicherFehler::Hintergrund(format!("{voll}, Eintrag {schluessel}: {e}")))?;
        neu.push((schluessel, serde_json::to_vec(&wert)?));
    }
    for (k, w) in &neu {
        t.insert(k.as_str(), w.as_slice())?;
    }
    Ok(neu.len() as u64)
}

impl Speicher {
    /// Richtet die Tabellen eines Moduls ein (beim Start des Dienstes): neu
    /// anlegen, gleiche Fassung lassen, ältere umbauen (vorher Sicherung),
    /// zu neue ablehnen. Gibt je Tabelle zurück, was geschah; bricht beim
    /// ersten Fehler ab (die Tabellen davor bleiben eingerichtet).
    pub fn projekt_tabellen_einrichten(&self, slug: &str, defs: &[TabellenDef]) -> Result<Vec<(String, Einrichtung)>, SpeicherFehler> {
        if !modul_tabellen::ist_name(slug) {
            return Err(SpeicherFehler::Hintergrund(format!("Slug ungültig: {slug:?}")));
        }
        modul_tabellen::pruefen(defs).map_err(SpeicherFehler::Hintergrund)?;
        let mut out = Vec::new();
        for def in defs {
            let voll = def.voller_name(slug);
            let alt = {
                let txn = self.db.begin_read()?;
                eintrag_im_verzeichnis(&txn, slug, def.name)?
            };
            let ergebnis = match alt {
                Some(p) if p.fassung > def.fassung => {
                    return Err(SpeicherFehler::Hintergrund(format!(
                        "Die Tabelle {voll} hat in der Datei die Fassung {}, das Modul kennt {}. Sie bleibt unverändert.",
                        p.fassung, def.fassung
                    )));
                }
                Some(p) if p.fassung < def.fassung => {
                    let sicherung = self.sichern_vor(&format!("projekt-{slug}-{}-{}", def.name, p.fassung).replace('_', "-"))?;
                    info!("Vor dem Umbau von {voll} gesichert: {}", sicherung.datei.display());
                    let txn = self.db.begin_write()?;
                    let anzahl = umbauen_in(&txn, &voll, def, p.fassung)?;
                    let neu = ProjektTabelle { fassung: def.fassung, beschreibung: def.beschreibung.to_string(), ..p.clone() };
                    txn.open_table(PROJEKT_TABELLEN)?.insert(verzeichnis_schluessel(slug, def.name).as_str(), serde_json::to_vec(&neu)?.as_slice())?;
                    self.festschreiben(txn, &[super::Bereich::ProjektTabellen])?;
                    info!("{voll}: von Fassung {} auf {} umgebaut ({anzahl} Einträge)", p.fassung, def.fassung);
                    Einrichtung::Umgebaut { von: p.fassung, nach: def.fassung, eintraege: anzahl }
                }
                Some(p) => {
                    if p.beschreibung != def.beschreibung {
                        let neu = ProjektTabelle { beschreibung: def.beschreibung.to_string(), ..p };
                        let txn = self.db.begin_write()?;
                        txn.open_table(PROJEKT_TABELLEN)?.insert(verzeichnis_schluessel(slug, def.name).as_str(), serde_json::to_vec(&neu)?.as_slice())?;
                        self.festschreiben(txn, &[super::Bereich::ProjektTabellen])?;
                    }
                    Einrichtung::Unveraendert
                }
                None => {
                    let neu = ProjektTabelle {
                        slug: slug.to_string(),
                        name: def.name.to_string(),
                        fassung: def.fassung,
                        beschreibung: def.beschreibung.to_string(),
                        angelegt: super::vorgaenge::jetzt(),
                    };
                    let txn = self.db.begin_write()?;
                    txn.open_table(definition(&voll))?;
                    txn.open_table(PROJEKT_TABELLEN)?.insert(verzeichnis_schluessel(slug, def.name).as_str(), serde_json::to_vec(&neu)?.as_slice())?;
                    self.festschreiben(txn, &[super::Bereich::ProjektTabellen])?;
                    info!("{voll} angelegt (Fassung {})", def.fassung);
                    Einrichtung::Angelegt
                }
            };
            out.push((voll, ergebnis));
        }
        Ok(out)
    }

    /// Die Tabelle steht im Verzeichnis und hat genau die Fassung, die das Modul
    /// nennt. Sonst ist sie für das Modul gesperrt (etwa nach einem abgelehnten
    /// oder gescheiterten Umbau), bis die Fassungen wieder passen.
    fn eingerichtet(&self, txn: &ReadTransaction, slug: &str, name: &str, fassung: u32) -> Result<String, SpeicherFehler> {
        match eintrag_im_verzeichnis(txn, slug, name)? {
            Some(p) if p.fassung == fassung => Ok(p.voller_name()),
            Some(p) => Err(SpeicherFehler::Hintergrund(format!(
                "Die Tabelle projekt/{slug}/{name} hat im Datenspeicher die Fassung {}, das Modul nennt {fassung}; sie ist gesperrt.",
                p.fassung
            ))),
            None => Err(nicht_eingerichtet(slug, name)),
        }
    }

    /// Ein Eintrag einer Modul-Tabelle.
    pub fn projekt_eintrag_lesen(&self, slug: &str, name: &str, fassung: u32, schluessel: &str) -> Result<Option<Vec<u8>>, SpeicherFehler> {
        let txn = self.db.begin_read()?;
        let voll = self.eingerichtet(&txn, slug, name, fassung)?;
        let Some(t) = tabellen::lesen(definition(&voll), &txn)? else { return Ok(None) };
        Ok(t.get(schluessel)?.map(|w| w.value().to_vec()))
    }

    /// Schreibt (oder ersetzt) einen Eintrag einer Modul-Tabelle.
    pub fn projekt_eintrag_schreiben(&self, slug: &str, name: &str, fassung: u32, schluessel: &str, wert: &[u8]) -> Result<(), SpeicherFehler> {
        let voll = {
            let txn = self.db.begin_read()?;
            self.eingerichtet(&txn, slug, name, fassung)?
        };
        let txn = self.db.begin_write()?;
        txn.open_table(definition(&voll))?.insert(schluessel, wert)?;
        self.festschreiben(txn, &[super::Bereich::ProjektTabellen])?;
        Ok(())
    }

    /// Höchstens `hoechstens` Einträge in Schlüsselreihenfolge (mit `rueckwaerts` die letzten zuerst).
    pub fn projekt_eintraege(
        &self,
        slug: &str,
        name: &str,
        fassung: u32,
        hoechstens: usize,
        rueckwaerts: bool,
    ) -> Result<Vec<(String, Vec<u8>)>, SpeicherFehler> {
        let txn = self.db.begin_read()?;
        let voll = self.eingerichtet(&txn, slug, name, fassung)?;
        let Some(t) = tabellen::lesen(definition(&voll), &txn)? else { return Ok(Vec::new()) };
        let alle = t.iter()?;
        let reihe: Box<dyn Iterator<Item = _>> = if rueckwaerts { Box::new(alle.rev()) } else { Box::new(alle) };
        let mut out = Vec::new();
        for e in reihe.take(hoechstens) {
            let (k, w) = e?;
            out.push((k.value().to_string(), w.value().to_vec()));
        }
        Ok(out)
    }

    /// Zahl der Einträge einer Modul-Tabelle.
    pub fn projekt_anzahl(&self, slug: &str, name: &str, fassung: u32) -> Result<u64, SpeicherFehler> {
        let txn = self.db.begin_read()?;
        let voll = self.eingerichtet(&txn, slug, name, fassung)?;
        match tabellen::lesen(definition(&voll), &txn)? {
            Some(t) => Ok(t.len()?),
            None => Ok(0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use redb::ReadOnlyDatabase;

    fn umbau(von: u32, mut w: serde_json::Value) -> Result<serde_json::Value, String> {
        match von {
            1 => {
                w["gesamt"] = serde_json::json!(0);
                Ok(w)
            }
            _ => Err("unbekannt".into()),
        }
    }

    /// Namensraum: nur über tabellen() anlegbar, in /daten (Tabellenliste),
    /// Sicherung und Export; Umbau mit Sicherung davor; zu neu abgelehnt.
    #[test]
    fn namensraum_einrichten_umbauen_sichern_exportieren() {
        let s = Speicher::temporaer();
        let defs = [TabellenDef::neu("verlauf", "Probe: Verlauf")];
        let r = s.projekt_tabellen_einrichten("probe", &defs).unwrap();
        assert_eq!(r, vec![("projekt/probe/verlauf".to_string(), Einrichtung::Angelegt)]);
        // Nicht eingerichtete Tabellen gibt es nicht, auch nicht für fremde Slugs.
        assert!(s.projekt_eintrag_schreiben("probe", "fremd", 1, "k", b"1").is_err());
        assert!(s.projekt_eintrag_schreiben("andere", "verlauf", 1, "k", b"1").is_err());
        s.projekt_eintrag_schreiben("probe", "verlauf", 1, "2026-10-29", br#"{"abgenommen":3}"#).unwrap();
        s.projekt_eintrag_schreiben("probe", "verlauf", 1, "2026-10-30", br#"{"abgenommen":4}"#).unwrap();
        assert_eq!(s.projekt_anzahl("probe", "verlauf", 1).unwrap(), 2);
        let letzte = s.projekt_eintraege("probe", "verlauf", 1, 1, true).unwrap();
        assert_eq!(letzte[0].0, "2026-10-30");
        assert_eq!(s.projekt_eintraege("probe", "verlauf", 1, 10, false).unwrap().len(), 2);
        // Zweites Einrichten: nichts geändert.
        assert_eq!(s.projekt_tabellen_einrichten("probe", &defs).unwrap()[0].1, Einrichtung::Unveraendert);
        // /daten, Export, Sicherung.
        let info = s.tabellen_info().unwrap();
        assert!(info.iter().any(|(n, b, e)| n == "projekt/probe/verlauf" && b == "Probe: Verlauf" && *e == 2), "{info:?}");
        let export: serde_json::Value = serde_json::from_str(&s.export().unwrap().als_json()).unwrap();
        assert_eq!(export["projekt/probe/verlauf"]["2026-10-30"]["abgenommen"], 4, "{export}");
        assert_eq!(export["projekt_tabellen"][0]["slug"], "probe");
        assert!(s.export().unwrap().als_toml().contains("[\"projekt/probe/verlauf\""), "{}", s.export().unwrap().als_toml());
        let sicherung = s.sichern().unwrap();
        let kopie = ReadOnlyDatabase::open(&sicherung.datei).unwrap();
        assert_eq!(kopie.begin_read().unwrap().open_table(definition("projekt/probe/verlauf")).unwrap().len().unwrap(), 2);
        drop(kopie);
        // Umbau auf Fassung 2: Sicherung davor, jeder Eintrag umgebaut.
        let zwei = [TabellenDef::neu("verlauf", "Probe: Verlauf").mit_fassung(2, umbau)];
        let r = s.projekt_tabellen_einrichten("probe", &zwei).unwrap();
        assert_eq!(r[0].1, Einrichtung::Umgebaut { von: 1, nach: 2, eintraege: 2 });
        let w: serde_json::Value = serde_json::from_slice(&s.projekt_eintrag_lesen("probe", "verlauf", 2, "2026-10-29").unwrap().unwrap()).unwrap();
        assert_eq!(w, serde_json::json!({ "abgenommen": 3, "gesamt": 0 }));
        let vor = s.sicherungs_ordner().join(format!("dashboard-{}-vor-projekt-probe-verlauf-1.redb", super::super::sicherung::heute()));
        assert!(vor.is_file(), "Sicherung vor dem Umbau fehlt: {}", vor.display());
        // Zurück auf Fassung 1: abgelehnt, die Datei bleibt.
        let e = s.projekt_tabellen_einrichten("probe", &defs).unwrap_err();
        assert!(e.einzelheiten().contains("Fassung 2, das Modul kennt 1"), "{}", e.einzelheiten());
        assert_eq!(s.projekt_anzahl("probe", "verlauf", 2).unwrap(), 2);
        // Das Modul mit Fassung 1 kommt an die Tabelle nicht mehr heran (gesperrt).
        assert!(s.projekt_anzahl("probe", "verlauf", 1).is_err());
        assert!(s.projekt_eintrag_schreiben("probe", "verlauf", 1, "x", b"1").is_err());
    }

    #[test]
    fn umbau_mit_fehler_aendert_nichts() {
        let s = Speicher::temporaer();
        s.projekt_tabellen_einrichten("probe", &[TabellenDef::neu("t", "x")]).unwrap();
        s.projekt_eintrag_schreiben("probe", "t", 1, "a", br#"{"a":1}"#).unwrap();
        let drei = [TabellenDef::neu("t", "x").mit_fassung(3, umbau)];
        assert!(s.projekt_tabellen_einrichten("probe", &drei).is_err(), "Schritt 2 → 3 fehlt");
        assert_eq!(s.projekt_eintrag_lesen("probe", "t", 1, "a").unwrap().as_deref(), Some(&br#"{"a":1}"#[..]));
        // Gescheiterter Umbau: für das Modul mit Fassung 3 bleibt die Tabelle gesperrt.
        assert!(s.projekt_eintrag_lesen("probe", "t", 3, "a").is_err());
        assert!(s.projekt_eintrag_schreiben("probe", "t", 3, "b", b"1").is_err());
        let txn = s.db.begin_read().unwrap();
        assert_eq!(eintrag_im_verzeichnis(&txn, "probe", "t").unwrap().unwrap().fassung, 1);
    }
}
