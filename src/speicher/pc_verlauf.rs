//! Tabelle `pc_verlauf`: PC-Werte alle 5 Minuten, 90 Tage lang. Der Sammler
//! liest den Snapshot aus `pc::system`, der ohnehin regelmäßig erneuert wird.

use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use redb::{ReadableDatabase, ReadableTable, ReadableTableMetadata, TableError, WriteTransaction};
use tracing::{debug, warn};

use super::tabellen::{PC_VERLAUF, PcMesswert, zeit_schluessel};
use super::{Speicher, SpeicherFehler};
use crate::pc::system::{SharedSnapshot, Snapshot};

/// So lange bleiben Messwerte.
pub const AUFBEWAHREN_TAGE: i64 = 90;
/// Abstand zwischen zwei Messwerten.
pub const ABSTAND: Duration = Duration::from_secs(5 * 60);
/// Erste Messung nach dem Start (dann hat der Snapshot schon Werte).
pub const ERSTE_MESSUNG: Duration = Duration::from_secs(30);

/// Erster und letzter Zeitpunkt und Zahl der Einträge.
pub type Eckdaten = (Option<String>, Option<String>, u64);

/// Hängt einen Messwert an (Schlüssel aus `m.zeit`) und löscht dabei alles,
/// was älter als 90 Tage ist.
pub fn anhaengen(txn: &WriteTransaction, m: &PcMesswert) -> Result<(), SpeicherFehler> {
    let zeit = DateTime::parse_from_rfc3339(&m.zeit).map_err(|e| SpeicherFehler::Beschaedigt(format!("Zeit {:?} unlesbar: {e}", m.zeit)))?.with_timezone(&Utc);
    if zeit_schluessel(zeit) != m.zeit {
        return Err(SpeicherFehler::Beschaedigt(format!("Zeit {:?} nicht in fester Form (UTC, Sekunden)", m.zeit)));
    }
    let grenze = zeit_schluessel(Utc::now() - chrono::Duration::days(AUFBEWAHREN_TAGE));
    let json = serde_json::to_vec(m)?;
    let mut tabelle = txn.open_table(PC_VERLAUF)?;
    tabelle.insert(m.zeit.as_str(), json.as_slice())?;
    tabelle.retain_in::<&str, _>(..grenze.as_str(), |_, _| false)?;
    Ok(())
}

impl Speicher {
    /// Alle Messwerte ab `seit`, in Zeitreihenfolge. Beschädigte Einträge
    /// werden übersprungen und ins Protokoll geschrieben.
    pub fn pc_verlauf_lesen(&self, seit: DateTime<Utc>) -> Result<Vec<PcMesswert>, SpeicherFehler> {
        let txn = self.db.begin_read()?;
        let tabelle = match txn.open_table(PC_VERLAUF) {
            Ok(t) => t,
            Err(TableError::TableDoesNotExist(_)) => return Ok(Vec::new()),
            Err(e) => return Err(e.into()),
        };
        let ab = zeit_schluessel(seit);
        let mut out = Vec::new();
        for eintrag in tabelle.range::<&str>(ab.as_str()..)? {
            let (schluessel, wert) = eintrag?;
            match serde_json::from_slice::<PcMesswert>(wert.value()) {
                Ok(m) => out.push(m),
                Err(e) => warn!("pc_verlauf: Eintrag {} unlesbar: {e}", schluessel.value()),
            }
        }
        Ok(out)
    }

    /// Erster und letzter Zeitpunkt und Zahl der Messwerte.
    pub fn pc_verlauf_eckdaten(&self) -> Result<Eckdaten, SpeicherFehler> {
        let txn = self.db.begin_read()?;
        let tabelle = match txn.open_table(PC_VERLAUF) {
            Ok(t) => t,
            Err(TableError::TableDoesNotExist(_)) => return Ok((None, None, 0)),
            Err(e) => return Err(e.into()),
        };
        let erster = tabelle.first()?.map(|(k, _)| k.value().to_string());
        let letzter = tabelle.last()?.map(|(k, _)| k.value().to_string());
        Ok((erster, letzter, tabelle.len()?))
    }
}

/// Messwert aus dem Snapshot; `None`, solange noch nicht gemessen wurde.
/// Ist die Prozessorlast unbekannt (erste Messung, GetSystemTimes
/// scheiterte, Folge von genau 0), steht `cpu` leer: eine Lücke, keine 0.
pub fn aus_snapshot(snap: &Snapshot, jetzt: DateTime<Utc>) -> Option<PcMesswert> {
    snap.taken_at?;
    let platte = snap.disks.first();
    Some(PcMesswert {
        zeit: zeit_schluessel(jetzt),
        cpu: snap.cpu_gemessen().then_some(snap.cpu_percent),
        mem: snap.mem_percent(),
        platte_belegt: platte.map(|d| d.used),
        platte_gesamt: platte.map(|d| d.total),
    })
}

/// Sammler: 30 s nach dem Start, dann alle 5 Minuten ein Messwert.
/// Fehler gehen nur ins Protokoll.
pub fn spawn_sammler(speicher: Arc<Speicher>, snapshot: SharedSnapshot) {
    tokio::spawn(async move {
        let start = tokio::time::Instant::now() + ERSTE_MESSUNG;
        let mut takt = tokio::time::interval_at(start, ABSTAND);
        takt.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            takt.tick().await;
            let messwert = aus_snapshot(&*snapshot.read().await, Utc::now());
            let Some(m) = messwert else {
                debug!("pc_verlauf: noch kein Snapshot, Messung ausgelassen");
                continue;
            };
            if let Err(e) = speicher.pc_messwert_anhaengen(m).await {
                warn!("pc_verlauf: Messwert nicht gespeichert: {e} ({})", e.einzelheiten());
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn messwert(zeit: DateTime<Utc>, cpu: f32) -> PcMesswert {
        PcMesswert { zeit: zeit_schluessel(zeit), cpu: Some(cpu), mem: 40.0, platte_belegt: Some(1), platte_gesamt: Some(2) }
    }

    #[tokio::test]
    async fn alte_werte_fallen_weg_und_reihenfolge_stimmt() {
        let s = Speicher::temporaer();
        let jetzt = Utc::now();
        // Absichtlich nicht in Zeitreihenfolge angehängt.
        s.pc_messwert_anhaengen(messwert(jetzt, 3.0)).await.unwrap();
        s.pc_messwert_anhaengen(messwert(jetzt - chrono::Duration::days(100), 1.0)).await.unwrap();
        s.pc_messwert_anhaengen(messwert(jetzt - chrono::Duration::hours(1), 2.0)).await.unwrap();
        let werte = s.pc_verlauf_lesen(jetzt - chrono::Duration::days(365)).unwrap();
        let cpus: Vec<Option<f32>> = werte.iter().map(|m| m.cpu).collect();
        assert_eq!(cpus, [Some(2.0), Some(3.0)]);
        let (erster, letzter, anzahl) = s.pc_verlauf_eckdaten().unwrap();
        assert_eq!(anzahl, 2);
        assert_eq!(erster.as_deref(), Some(werte[0].zeit.as_str()));
        assert_eq!(letzter.as_deref(), Some(werte[1].zeit.as_str()));
        // Nur ab einem Zeitpunkt.
        let neu = s.pc_verlauf_lesen(jetzt - chrono::Duration::minutes(30)).unwrap();
        assert_eq!(neu.len(), 1);
    }

    #[tokio::test]
    async fn zeit_in_falscher_form_wird_abgelehnt() {
        let s = Speicher::temporaer();
        let mut m = messwert(Utc::now(), 1.0);
        m.zeit = "2026-10-25T20:35:00+02:00".to_string();
        assert!(matches!(s.pc_messwert_anhaengen(m).await, Err(SpeicherFehler::Beschaedigt(_))));
    }

    #[test]
    fn messwert_erst_nach_der_ersten_messung() {
        let mut snap = Snapshot::default();
        assert!(aus_snapshot(&snap, Utc::now()).is_none());
        snap.taken_at = Some(chrono::Local::now());
        snap.cpu_percent = 12.5;
        snap.belegung_rechnen();
        let m = aus_snapshot(&snap, Utc::now()).unwrap();
        assert_eq!(m.cpu, Some(12.5));
        assert_eq!(m.platte_gesamt, None);
        // Etappe 3a+: erste Messung und unbekannte Last sind eine Lücke, keine 0.
        snap.erste_messung();
        let m = aus_snapshot(&snap, Utc::now()).unwrap();
        assert_eq!((m.cpu, m.mem), (None, snap.mem_percent()));
        assert!(serde_json::to_string(&m).unwrap().contains("\"cpu\":null"));
        // Ein alter Eintrag mit Zahl liest sich weiter.
        let alt: PcMesswert = serde_json::from_str(r#"{"zeit":"2026-10-30T10:00:00Z","cpu":7.0,"mem":1.0,"platte_belegt":null,"platte_gesamt":null}"#).unwrap();
        assert_eq!(alt.cpu, Some(7.0));
    }
}
