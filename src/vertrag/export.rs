//! TypeScript aus den Rust-Typen (ts-rs): `cargo test` schreibt je Typ eine
//! Datei `web/src/lib/vertrag/<Typ>.ts` und dazu `index.ts`, die alles
//! wieder ausführt. Die Dateien sind eingecheckt; die Oberfläche importiert
//! sie als `$vertrag`.
//!
//! Der Test `vertrag_export_und_pruefung` vergleicht erst, was auf der Platte
//! liegt, mit dem, was Rust jetzt erzeugt, schreibt dann den neuen Stand und
//! schlägt fehl, wenn etwas abwich. So ist der zweite Lauf grün, und die
//! Dateien sind danach immer aktuell; einchecken muss man sie selbst.

use std::any::TypeId;
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use ts_rs::{Config, TS, TypeVisitor};

use crate::api::events::{BauEreignis, Change, DatenEreignis, FassungEreignis, NachholEreignis, PcTick, ProjektEreignis, SammelEreignis, VorgangEreignis};
use crate::vertrag::abgeleitet::Abgeleitet;
use crate::vertrag::api::*;
use crate::vertrag::bausteine::Block;
use crate::vertrag::bedeutung::Bedeutung;
use crate::vertrag::daten::ProjektDaten;
use crate::vertrag::passkey::*;
use crate::vertrag::projekt::{BackupStand, Projekt, Status};
use crate::vertrag::schwerpunkte::Schwerpunkte;
use crate::vertrag::verlauf::Verlauf;

/// Der Ordner, relativ zum Repo (für Meldungen).
const ORT: &str = "web/src/lib/vertrag";

const KOPF_INDEX: &str = "// Erzeugt von cargo test (src/vertrag/export.rs). Nicht von Hand ändern.\n";

fn ordner() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(ORT)
}

fn config() -> Config {
    // u64 als `number`: Zähler und Größen bleiben weit unter 2^53.
    Config::new().with_out_dir(ordner()).with_large_int("number")
}

/// Sammelt alle Typen samt Abhängigkeiten: Dateiname → (TS-Name, Inhalt).
struct Sammler<'a> {
    cfg: &'a Config,
    gesehen: HashSet<TypeId>,
    dateien: BTreeMap<String, (String, String)>,
    fehler: Vec<String>,
}

impl TypeVisitor for Sammler<'_> {
    fn visit<T: TS + 'static + ?Sized>(&mut self) {
        self.aufnehmen::<T>();
    }
}

impl Sammler<'_> {
    fn aufnehmen<T: TS + 'static + ?Sized>(&mut self) {
        let Some(pfad) = T::output_path() else { return };
        if !self.gesehen.insert(TypeId::of::<T>()) {
            return;
        }
        let datei = pfad.to_string_lossy().replace('\\', "/");
        let name = T::ident(self.cfg);
        match T::export_to_string(self.cfg) {
            Ok(text) => match self.dateien.get(&datei) {
                Some((anderer, _)) if *anderer != name => {
                    self.fehler.push(format!("{datei}: zwei Typen heißen in TypeScript gleich ({anderer}, {name})"));
                }
                Some((_, alt)) if *alt != text => {
                    self.fehler.push(format!("{datei}: {name} ergibt zwei verschiedene Fassungen"));
                }
                Some(_) => {}
                None => {
                    self.dateien.insert(datei, (name, text));
                }
            },
            Err(e) => self.fehler.push(format!("{datei}: {e}")),
        }
        T::visit_dependencies(self);
    }
}

/// Alle Wurzeltypen; was sie enthalten, kommt von selbst mit.
fn alle_dateien(cfg: &Config) -> Result<BTreeMap<String, String>, Vec<String>> {
    let mut s = Sammler { cfg, gesehen: HashSet::new(), dateien: BTreeMap::new(), fehler: Vec::new() };
    macro_rules! wurzeln {
        ($($t:ty),* $(,)?) => { $( s.aufnehmen::<$t>(); )* };
    }
    wurzeln!(
        // Projekte
        ProjectList,
        ArchivListe,
        ProjectDetail,
        // Bau-Stand (Etappe 4c)
        BauListe,
        // Lokal öffnen (Entscheidung lokal-oeffnen)
        OeffnenAntwort,
        Block,
        Projekt,
        Status,
        BackupStand,
        ProjektDaten,
        Bedeutung,
        // Neue Form: allgemeine Endpunkte (Etappe 4v)
        ProjektAntwort,
        DatenAntwort,
        Abgeleitet,
        VertragsFehler,
        // PC und Schlüssel
        PcReport,
        PcBenutzer,
        Verlauf,
        KeyList,
        // Datenspeicher
        DatenBericht,
        PcVerlaufAntwort,
        SicherungAntwort,
        // Vorgänge
        VorgangAngelegt,
        VorgangGeaendert,
        VorgangsListe,
        VorgangDetail,
        Uebersicht,
        // Aufgaben für den Inhaber
        Aufgabe,
        AufgabeAngelegt,
        AufgabenListe,
        AufgabenAnzahl,
        AufgabeGeaendert,
        // Anstehend (Etappe A1)
        Arbeit,
        ArbeitAngelegt,
        ArbeitGeaendert,
        ArbeitenListe,
        // Herzschlag: offene Arbeit einer Sitzung
        SitzungOffen,
        // Teilschritte (T3)
        SchrittAngelegt,
        Besetzt,
        Einstufung,
        EinstufungAntwort,
        AnstehendListe,
        ProjektStand,
        // Backlog (Etappe B2)
        Backlog,
        // Plan (Etappe B8): GET /api/projekte/<slug>/plan
        crate::vertrag::plan::ProjektPlan,
        // Fristen (Etappe B3): GET /api/fristen; die Datei steht in ProjektDaten
        crate::vertrag::fristen::FristenListe,
        crate::vertrag::fristen::Fristen,
        Schwerpunkte,
        // Fragen und Antworten (Etappe A2)
        Antwort,
        AntwortGespeichert,
        AntwortenListe,
        // Meldungen (Etappe A3)
        Meldung,
        MeldungAngelegt,
        MeldungenListe,
        MeldungGelesen,
        // Prüfung
        PruefungsBericht,
        PruefungProjekt,
        // System-Seiten (Etappe 4d)
        EntscheidungsListe,
        ZielbildAntwort,
        // Etappe 6b: /api/anleitung aus docs/anleitung
        AnleitungAntwort,
        // Passkey für andere Konten (Etappe 7c)
        Passkey,
        PasskeyListe,
        PasskeyAnzahl,
        PasskeyStand,
        AnmeldeOptionen,
        AnmeldenEnde,
        AntragBeginn,
        AntragOptionen,
        AntragEnde,
        AntragErgebnis,
        EigeneAntraege,
        Freigabe,
        // Betrieb
        Betrieb,
        // Fassungen aus dem Changelog (Freigabe in einem Schritt)
        FassungenAntwort,
        NeuIn,
        // Reiter „Hallo“: welches Projekt den Begrüßungstext trägt
        HalloOrt,
        // Kopfleiste, Fassung 3: Stand für die Aktualitätsprüfung
        ServerStand,
        // Live-Kanal
        Change,
        PcTick,
        BauEreignis,
        // Bau-Tempo, Zweig C (nur geschützt)
        crate::vertrag::bote::BauFortschritt,
        FassungEreignis,
        VorgangEreignis,
        NachholEreignis,
        // Freier Kanal (Kopfleiste, Fassung 2)
        AnmeldungEreignis,
        Anmeldewort,
        ProjektEreignis,
        DatenEreignis,
        SammelEreignis,
        // Live-Lücken: Ereignisse mit Bereich (speicher, pc-benutzer, docs)
        crate::aenderungen::BereichEreignis,
    );
    if !s.fehler.is_empty() {
        return Err(s.fehler);
    }
    let mut dateien: BTreeMap<String, String> = BTreeMap::new();
    let mut index = String::from(KOPF_INDEX);
    let mut namen: Vec<(&String, &String)> = s.dateien.iter().map(|(d, (n, _))| (n, d)).collect();
    namen.sort();
    for (name, datei) in namen {
        let modul = datei.trim_end_matches(".ts");
        index.push_str(&format!("export type {{ {name} }} from './{modul}';\n"));
    }
    for (datei, (_, text)) in s.dateien {
        dateien.insert(datei, text);
    }
    dateien.insert("index.ts".to_string(), index);
    Ok(dateien)
}

#[test]
fn vertrag_export_und_pruefung() {
    let cfg = config();
    let soll = alle_dateien(&cfg).unwrap_or_else(|f| panic!("Vertrag lässt sich nicht erzeugen:\n{}", f.join("\n")));
    let dir = ordner();

    // 1. Was weicht ab?
    let mut abweichungen = Vec::new();
    for (datei, text) in &soll {
        match std::fs::read(dir.join(datei)) {
            Ok(da) if da == text.as_bytes() => {}
            Ok(_) => abweichungen.push(format!("{ORT}/{datei} ist veraltet")),
            Err(_) => abweichungen.push(format!("{ORT}/{datei} fehlt")),
        }
    }
    let ueberzaehlig: Vec<String> = std::fs::read_dir(&dir)
        .map(|rd| rd.filter_map(|e| e.ok()).filter_map(|e| e.file_name().into_string().ok()).filter(|n| n.ends_with(".ts") && !soll.contains_key(n)).collect())
        .unwrap_or_default();
    for n in &ueberzaehlig {
        abweichungen.push(format!("{ORT}/{n} gehört zu keinem Rust-Typ mehr"));
    }

    // 2. Neuen Stand schreiben (LF, UTF-8), Überzähliges entfernen.
    std::fs::create_dir_all(&dir).unwrap();
    for (datei, text) in &soll {
        let pfad = dir.join(datei);
        if std::fs::read(&pfad).ok().as_deref() != Some(text.as_bytes()) {
            std::fs::write(&pfad, text).unwrap();
        }
    }
    for n in &ueberzaehlig {
        std::fs::remove_file(dir.join(n)).unwrap();
    }

    // 3. Fehlschlagen, wenn etwas abwich.
    assert!(
        abweichungen.is_empty(),
        "Der TypeScript-Vertrag passte nicht zu Rust und ist jetzt neu geschrieben:\n{}\n\
         cargo test ausführen und das Ergebnis einchecken.",
        abweichungen.iter().map(|a| format!("  {a}: cargo test ausführen und das Ergebnis einchecken")).collect::<Vec<_>>().join("\n")
    );
}

#[test]
fn vertrag_enthaelt_die_wichtigen_typen() {
    let soll = alle_dateien(&config()).unwrap();
    for t in ["ProjectDetail", "ProjektDaten", "Bedeutung", "Vorgang", "Kennzahlen", "Datei", "Block", "PcReport", "RepoStatus"] {
        assert!(soll.contains_key(&format!("{t}.ts")), "{t} fehlt: {:?}", soll.keys());
    }
    assert!(soll["Bedeutung.ts"].contains("\"ok\" | \"laeuft\" | \"wartet\" | \"problem\" | \"info\" | \"neutral\""), "{}", soll["Bedeutung.ts"]);
    // Zeiten als Text, große Zahlen als number.
    assert!(soll["Snapshot.ts"].contains("taken_at: string | null"), "{}", soll["Snapshot.ts"]);
    assert!(!soll.values().any(|t| t.contains("bigint")), "bigint im Vertrag");
    assert!(soll.values().all(|t| !t.contains('\r')), "nur LF");
    assert!(soll["index.ts"].starts_with(KOPF_INDEX));
    assert!(soll["index.ts"].contains("export type { Vorgang } from './Vorgang';"));
}
