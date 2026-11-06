//! Live-Lücken (Entscheidung `immer-aktuell`, Punkt 2):
//! Änderungen, die nicht aus einer Datei unter `projects/` oder `data/` kommen,
//! als Ereignis mit Bereich.
//!
//! Drei Arten auf dem geschützten Kanal, alle mit `{ "bereich": … }`:
//!
//! - `speicher`: der Datenspeicher hat geschrieben (Aufgaben, Arbeiten,
//!   Einstufung, Meldungen, Antworten, Vorgänge, Passkeys, Sicherung,
//!   PC-Messung, Windows-Ereignisse, Tabellen der Projekte). Der Speicher meldet
//!   selbst nach jedem Festschreiben (`Speicher::festschreiben`).
//! - `pc-benutzer`: Benutzer-Daten der PC-Seite (Ordnergrößen, Systemprogramme,
//!   Tresor, Bote da oder weg).
//! - `docs`: eine Datei in `docs/entscheidungen` oder `docs/zielbild` ist neu,
//!   geändert oder weg (der Beobachter in `api::events` schaut alle 2 s).
//!
//! Gebündelt: Was innerhalb von [`BUENDELN`] zusammenkommt, geht je Bereich
//! einmal hinaus. Ein Sammler, der hundert Zeilen schreibt, löst so ein
//! Ereignis aus, nicht hundert. Keine der drei Arten darf auf den freien Kanal
//! (die Daten dahinter sind geschützt; `api::events::FREIER_KANAL`).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use tokio::sync::broadcast;
use ts_rs::TS;

/// So lange sammelt der Melder, bevor er schickt.
pub const BUENDELN: Duration = Duration::from_millis(250);

/// Was sich geändert hat. Im JSON klein mit Bindestrich (`pc-messung`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(rename = "LiveBereich")]
pub enum Bereich {
    // Ereignis `speicher`
    Aufgaben,
    Arbeiten,
    Einstufung,
    Meldungen,
    Antworten,
    Vorgaenge,
    Passkeys,
    Sicherung,
    PcMessung,
    WindowsEreignisse,
    ProjektTabellen,
    // Ereignis `pc-benutzer`
    Inventar,
    Systemprogramme,
    Tresor,
    Bote,
    // Ereignis `docs`
    Entscheidungen,
    Zielbild,
}

/// Zu welcher Art von Ereignis ein Bereich gehört.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gruppe {
    Speicher,
    PcBenutzer,
    Docs,
}

impl Bereich {
    pub const ALLE: [Bereich; 17] = [
        Bereich::Aufgaben,
        Bereich::Arbeiten,
        Bereich::Einstufung,
        Bereich::Meldungen,
        Bereich::Antworten,
        Bereich::Vorgaenge,
        Bereich::Passkeys,
        Bereich::Sicherung,
        Bereich::PcMessung,
        Bereich::WindowsEreignisse,
        Bereich::ProjektTabellen,
        Bereich::Inventar,
        Bereich::Systemprogramme,
        Bereich::Tresor,
        Bereich::Bote,
        Bereich::Entscheidungen,
        Bereich::Zielbild,
    ];

    pub fn gruppe(self) -> Gruppe {
        match self {
            Bereich::Aufgaben
            | Bereich::Arbeiten
            | Bereich::Einstufung
            | Bereich::Meldungen
            | Bereich::Antworten
            | Bereich::Vorgaenge
            | Bereich::Passkeys
            | Bereich::Sicherung
            | Bereich::PcMessung
            | Bereich::WindowsEreignisse
            | Bereich::ProjektTabellen => Gruppe::Speicher,
            Bereich::Inventar | Bereich::Systemprogramme | Bereich::Tresor | Bereich::Bote => Gruppe::PcBenutzer,
            Bereich::Entscheidungen | Bereich::Zielbild => Gruppe::Docs,
        }
    }
}

/// Das Ereignis (`speicher`, `pc-benutzer` oder `docs`): nur der Bereich,
/// nie Inhalt. Die Seite holt dann neu, was sie aus diesem Bereich zeigt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "BereichEvent")]
pub struct BereichEreignis {
    pub bereich: Bereich,
}

/// Sammelt gemeldete Bereiche und schickt sie gebündelt.
pub struct Melder {
    offen: Mutex<BTreeSet<Bereich>>,
    tx: broadcast::Sender<BereichEreignis>,
}

impl Melder {
    pub fn neu() -> Arc<Melder> {
        Arc::new(Melder { offen: Mutex::new(BTreeSet::new()), tx: broadcast::channel(64).0 })
    }

    pub fn abonnieren(&self) -> broadcast::Receiver<BereichEreignis> {
        self.tx.subscribe()
    }

    /// Meldet eine Änderung. Der erste Bereich einer Runde stellt einen
    /// Wecker auf [`BUENDELN`]; alles bis dahin geht mit hinaus. Ohne
    /// Tokio-Laufzeit (einige Tests) geht es sofort.
    pub fn melden(self: &Arc<Self>, b: Bereich) {
        let erster = {
            let mut o = self.offen.lock().unwrap_or_else(|e| e.into_inner());
            let leer = o.is_empty();
            o.insert(b);
            leer
        };
        if !erster {
            return;
        }
        match tokio::runtime::Handle::try_current() {
            Ok(h) => {
                let m = self.clone();
                h.spawn(async move {
                    tokio::time::sleep(BUENDELN).await;
                    m.ausschicken();
                });
            }
            Err(_) => self.ausschicken(),
        }
    }

    fn ausschicken(&self) {
        let alle = std::mem::take(&mut *self.offen.lock().unwrap_or_else(|e| e.into_inner()));
        for bereich in alle {
            tracing::debug!("Live: Bereich {bereich:?} geändert");
            let _ = self.tx.send(BereichEreignis { bereich });
        }
    }
}

/// Fingerabdruck eines Ordners: je Datei Name, Zeit, Größe.
pub type Stempel = Vec<(String, Option<std::time::SystemTime>, u64)>;

/// So viele Einträge sieht [`ordner_stempel`] höchstens an (je Ordner, alle
/// 2 s). Ein Ordner mit mehr Einträgen bremst den Beobachter so nicht aus;
/// Änderungen hinter dieser Grenze fallen dann erst beim nächsten Neuladen auf.
pub const ORDNER_HOECHSTENS: usize = 2000;

/// Fingerabdruck eines Ordners (nur die oberste Ebene): Name, Zeit, Größe,
/// höchstens [`ORDNER_HOECHSTENS`] Einträge. Fehlt der Ordner, ist er leer.
/// Blockiert.
pub fn ordner_stempel(ordner: &Path) -> Stempel {
    ordner_stempel_bis(ordner, ORDNER_HOECHSTENS)
}

/// Wie [`ordner_stempel`], mit eigener Obergrenze (Tests).
fn ordner_stempel_bis(ordner: &Path, hoechstens: usize) -> Stempel {
    let mut v: Vec<_> = std::fs::read_dir(ordner)
        .map(|rd| {
            rd.flatten()
                .take(hoechstens)
                .filter_map(|e| {
                    let m = e.metadata().ok()?;
                    m.is_file().then(|| (e.file_name().to_string_lossy().to_string(), m.modified().ok(), m.len()))
                })
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

/// Wacht über `docs/entscheidungen` und `docs/zielbild` (Punkt 3).
pub struct DocsWache {
    ordner: Vec<(Bereich, PathBuf, Stempel)>,
}

impl DocsWache {
    /// Liest den ersten Stand (blockiert): `docs/entscheidungen` und `docs/zielbild`.
    pub fn neu(docs: &Path) -> DocsWache {
        Self::mit(&docs.join("entscheidungen"), &docs.join("zielbild"))
    }

    /// Wie [`DocsWache::neu`], mit getrennten Ordnern (seit dem Umzug liegen
    /// die Entscheidungen im privaten Inhalt, das Zielbild im Code-Repo).
    pub fn mit(entscheidungen: &Path, zielbild: &Path) -> DocsWache {
        let ordner = [(Bereich::Entscheidungen, entscheidungen), (Bereich::Zielbild, zielbild)]
            .into_iter()
            .map(|(b, p)| {
                let s = ordner_stempel(p);
                (b, p.to_path_buf(), s)
            })
            .collect();
        DocsWache { ordner }
    }

    /// Welche Ordner sich seit dem letzten Blick geändert haben (blockiert).
    pub fn pruefen(&mut self) -> Vec<Bereich> {
        let mut geaendert = Vec::new();
        for (b, p, alt) in &mut self.ordner {
            let neu = ordner_stempel(p);
            if neu != *alt {
                *alt = neu;
                geaendert.push(*b);
            }
        }
        geaendert
    }
}

/// Wacht über `assets/` aller Projekte (Punkt 5): je Slug ein Fingerabdruck.
#[derive(Default)]
pub struct AssetWache {
    stand: std::collections::HashMap<String, Stempel>,
}

impl AssetWache {
    /// Vergleicht `assets/` der Projekte (Slug und Projektordner) mit dem
    /// letzten Blick; gibt die Slugs mit Änderung zurück, je Slug die Namen
    /// der neuen, geänderten oder gelöschten Dateien (sortiert). Beim ersten
    /// Blick auf ein Projekt ist nichts geändert. Blockiert.
    pub fn pruefen(&mut self, projekte: &[(String, PathBuf)]) -> Vec<(String, Vec<String>)> {
        let mut geaendert = Vec::new();
        let mut neu_stand = std::collections::HashMap::new();
        for (slug, ordner) in projekte {
            let s = ordner_stempel(&ordner.join("assets"));
            if let Some(alt) = self.stand.get(slug).filter(|alt| **alt != s) {
                geaendert.push((slug.clone(), geaenderte_dateien(alt, &s)));
            }
            neu_stand.insert(slug.clone(), s);
        }
        self.stand = neu_stand;
        geaendert
    }
}

/// Namen der Dateien, die nur in einem der beiden Fingerabdrücke so stehen
/// (neu, geändert oder gelöscht), sortiert und ohne Doppel.
fn geaenderte_dateien(alt: &Stempel, neu: &Stempel) -> Vec<String> {
    let a: BTreeSet<_> = alt.iter().collect();
    let n: BTreeSet<_> = neu.iter().collect();
    let namen: BTreeSet<String> = a.symmetric_difference(&n).map(|(name, _, _)| name.clone()).collect();
    namen.into_iter().collect()
}

/// So oft schaut die Bote-Wache auf den Stand des Boten.
pub const BOTE_TAKT: Duration = if cfg!(test) { Duration::from_millis(40) } else { Duration::from_secs(2) };

/// Bote da oder weg (Punkt 4): schaut alle [`BOTE_TAKT`] auf den Stand des
/// Boten und meldet `pc-benutzer` mit `bote` bei jedem Wechsel. Endet beim
/// Abschaltsignal.
pub fn spawn_bote_wache(
    bote: Arc<crate::bote::client::Bote>,
    melder: Arc<Melder>,
    abschaltung: impl std::future::Future<Output = ()> + Send + 'static,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        tokio::pin!(abschaltung);
        let mut zuletzt = bote.stand().verbunden;
        loop {
            tokio::select! {
                _ = tokio::time::sleep(BOTE_TAKT) => {}
                _ = &mut abschaltung => return,
            }
            let jetzt = bote.stand().verbunden;
            if jetzt != zuletzt {
                zuletzt = jetzt;
                melder.melden(Bereich::Bote);
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Viele Meldungen in kurzer Zeit: je Bereich genau ein Ereignis.
    #[tokio::test]
    async fn buendelt_je_bereich() {
        let m = Melder::neu();
        let mut rx = m.abonnieren();
        for _ in 0..100 {
            m.melden(Bereich::PcMessung);
        }
        m.melden(Bereich::Aufgaben);
        m.melden(Bereich::PcMessung);
        assert!(rx.try_recv().is_err(), "noch nichts vor Ablauf der Bündelzeit");
        tokio::time::sleep(BUENDELN + Duration::from_millis(150)).await;
        let mut gesehen = Vec::new();
        while let Ok(e) = rx.try_recv() {
            gesehen.push(e.bereich);
        }
        assert_eq!(gesehen, [Bereich::Aufgaben, Bereich::PcMessung]);
        // Die nächste Runde beginnt neu.
        m.melden(Bereich::Meldungen);
        tokio::time::sleep(BUENDELN + Duration::from_millis(150)).await;
        assert_eq!(rx.try_recv().unwrap().bereich, Bereich::Meldungen);
    }

    #[test]
    fn ohne_laufzeit_sofort() {
        let m = Melder::neu();
        let mut rx = m.abonnieren();
        m.melden(Bereich::Tresor);
        assert_eq!(rx.try_recv().unwrap(), BereichEreignis { bereich: Bereich::Tresor });
    }

    #[test]
    fn namen_im_json() {
        let j = |b| serde_json::to_string(&BereichEreignis { bereich: b }).unwrap();
        assert_eq!(j(Bereich::PcMessung), r#"{"bereich":"pc-messung"}"#);
        assert_eq!(j(Bereich::WindowsEreignisse), r#"{"bereich":"windows-ereignisse"}"#);
        assert_eq!(j(Bereich::ProjektTabellen), r#"{"bereich":"projekt-tabellen"}"#);
        assert_eq!(j(Bereich::Entscheidungen), r#"{"bereich":"entscheidungen"}"#);
        let namen: BTreeSet<String> = Bereich::ALLE.iter().map(|b| j(*b)).collect();
        assert_eq!(namen.len(), Bereich::ALLE.len());
    }

    /// Eine neue, geänderte oder gelöschte Datei in docs/entscheidungen oder
    /// docs/zielbild: genau dieser Bereich.
    #[test]
    fn docs_wache_sieht_neue_und_geaenderte_dateien() {
        let basis = std::env::temp_dir().join(format!("dash-docs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&basis);
        std::fs::create_dir_all(basis.join("entscheidungen")).unwrap();
        std::fs::write(basis.join("entscheidungen/a.toml"), "a = 1").unwrap();
        let mut w = DocsWache::neu(&basis);
        assert!(w.pruefen().is_empty());
        std::fs::write(basis.join("entscheidungen/b.toml"), "b = 1").unwrap();
        assert_eq!(w.pruefen(), [Bereich::Entscheidungen]);
        assert!(w.pruefen().is_empty(), "nur einmal");
        std::fs::create_dir_all(basis.join("zielbild")).unwrap();
        std::fs::write(basis.join("zielbild/x.toml"), "x = 1").unwrap();
        assert_eq!(w.pruefen(), [Bereich::Zielbild]);
        std::fs::write(basis.join("entscheidungen/a.toml"), "a = 22").unwrap();
        assert_eq!(w.pruefen(), [Bereich::Entscheidungen]);
        std::fs::remove_file(basis.join("entscheidungen/b.toml")).unwrap();
        assert_eq!(w.pruefen(), [Bereich::Entscheidungen]);
        let _ = std::fs::remove_dir_all(&basis);
    }

    #[test]
    fn asset_wache_meldet_nur_geaenderte_projekte() {
        let basis = std::env::temp_dir().join(format!("dash-assets-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&basis);
        for s in ["a", "b"] {
            std::fs::create_dir_all(basis.join(s).join("assets")).unwrap();
            std::fs::write(basis.join(s).join("assets/bild.png"), "1").unwrap();
        }
        let projekte: Vec<(String, PathBuf)> = ["a", "b"].iter().map(|s| (s.to_string(), basis.join(s))).collect();
        let mut w = AssetWache::default();
        assert!(w.pruefen(&projekte).is_empty(), "erster Blick");
        std::fs::write(basis.join("b/assets/bild.png"), "22").unwrap();
        assert_eq!(w.pruefen(&projekte), [("b".to_string(), vec!["bild.png".to_string()])]);
        assert!(w.pruefen(&projekte).is_empty());
        std::fs::write(basis.join("a/assets/neu.png"), "1").unwrap();
        assert_eq!(w.pruefen(&projekte), [("a".to_string(), vec!["neu.png".to_string()])], "nur die neue Datei, bild.png nicht");
        std::fs::remove_file(basis.join("a/assets/bild.png")).unwrap();
        assert_eq!(w.pruefen(&projekte), [("a".to_string(), vec!["bild.png".to_string()])], "gelöscht");
        let _ = std::fs::remove_dir_all(&basis);
    }

    /// Kleinigkeit aus der Abnahme: `ordner_stempel` sieht höchstens so viele
    /// Einträge an, wie die Obergrenze sagt; darunter alle.
    #[test]
    fn ordner_stempel_hat_eine_obergrenze() {
        let basis = std::env::temp_dir().join(format!("dash-grenze-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&basis);
        std::fs::create_dir_all(&basis).unwrap();
        for n in 0..12 {
            std::fs::write(basis.join(format!("{n:02}.txt")), "x").unwrap();
        }
        assert_eq!(ordner_stempel_bis(&basis, 5).len(), 5);
        assert_eq!(ordner_stempel_bis(&basis, 100).len(), 12);
        assert_eq!(ordner_stempel(&basis).len(), 12);
        const { assert!(ORDNER_HOECHSTENS >= 1000, "für docs/ und assets/ reicht das weit") };
        let _ = std::fs::remove_dir_all(&basis);
    }

    /// Abnahme-Auflage: die Bote-Wache mit falschem Boten-Stand. Ein Wechsel
    /// (da, weg) gibt genau ein `pc-benutzer` mit Bereich `bote`, ohne Wechsel
    /// kommt nichts, und beim Abschaltsignal endet die Wache.
    #[tokio::test]
    async fn bote_wache_meldet_wechsel_und_endet_bei_abschaltung() {
        use crate::bote::client::{Bote, Lage};
        let bote = Arc::new(Bote::fuer_tests());
        let melder = Melder::neu();
        let mut rx = melder.abonnieren();
        let (aus_tx, mut aus_rx) = tokio::sync::watch::channel(false);
        let aus = async move {
            let _ = aus_rx.wait_for(|&a| a).await;
        };
        let wache = spawn_bote_wache(bote.clone(), melder.clone(), aus);
        let warten = || tokio::time::sleep(BOTE_TAKT * 3 + BUENDELN + Duration::from_millis(100));

        warten().await;
        assert!(rx.try_recv().is_err(), "ohne Wechsel nichts");
        bote.lage_setzen(Lage::Verbunden);
        warten().await;
        assert_eq!(rx.try_recv().unwrap(), BereichEreignis { bereich: Bereich::Bote });
        assert!(rx.try_recv().is_err(), "einmal je Wechsel");
        // Überlastet zählt als erreichbar: kein Wechsel.
        bote.lage_setzen(Lage::Ueberlastet);
        warten().await;
        assert!(rx.try_recv().is_err(), "überlastet ist kein Wechsel");
        bote.lage_setzen(Lage::Getrennt);
        warten().await;
        assert_eq!(rx.try_recv().unwrap().bereich, Bereich::Bote);

        // Als SSE: Ereignis `pc-benutzer` mit `{"bereich":"bote"}`.
        assert_eq!(crate::api::events::Art::zu_bereich(Bereich::Bote).name(), "pc-benutzer");

        aus_tx.send(true).unwrap();
        tokio::time::timeout(Duration::from_secs(2), wache).await.expect("Wache endet beim Abschaltsignal").unwrap();
    }
}
