//! Feste Tabellen des Datenspeichers (Zielbild, Kapitel c3). Jede Tabelle ist
//! hier einmal definiert; Werte liegen als JSON der Vertragstypen darin.
//!
//! Etappe 2a: `meta` und `pc_verlauf`. Etappe 2b (Schema 2): `ereignisse`
//! (Journal, nur anhängen), `vorgaenge`, `vorgaenge_je_slug` und `warter`.
//! Etappe 5a (Schema 3): `aufgaben`, `aufgaben_links` (verschlüsselt) und
//! `aufgaben_archiv` (abgeschlossene Aufgaben nach sieben Tagen).
//! Etappe A1 (Schema 4): `arbeiten`, `arbeiten_archiv` und `einstufungen`.
//! Etappe A2 (Schema 5): `antworten` und `antworten_archiv`.
//! Etappe A3 (Schema 6): `meldungen` und `meldungen_archiv`.
//! Etappe 4b (Schema 7): `projekt_tabellen` (Verzeichnis der Tabellen der
//! Backend-Module) und je Modul-Tabelle eine Tabelle `projekt/<slug>/<name>`
//! (`speicher::projekte`), die nicht in `ALLE` steht, aber in /daten,
//! Sicherung und Export mitläuft.
//! Etappe 7c (Schema 8): `passkeys`, `passkey_aufgaben`, `passkey_antraege`
//! und `jwt_sperrliste`; im Export nur `passkeys` (öffentliche Daten).
//! Etappe 2e (Schema 9): `pc_ereignisse` und `pc_ereignisse_archiv`
//! (Windows-Ereignisse für den Verlauf, `speicher::pc_ereignisse`).
//! Zweig teilschritte (Schema 10): keine neue Tabelle; `Arbeit` in Fassung 2
//! mit `besitzer` und `schritte` (`speicher::schritte`).
//! Später kommen `wunsch_zustand` und `bau_laeufe` dazu.
//!
//! Eine neue Tabelle braucht genau einen Eintrag in `ALLE`: daraus lesen die
//! Seite /daten (Zahl der Einträge), die Sicherungskopie und der Export. Eine
//! Tabelle in der Datei, die hier fehlt, bricht die Sicherung mit Fehler ab.

use redb::{Key, ReadOnlyTable, ReadTransaction, ReadableTable, ReadableTableMetadata, TableDefinition, TableError, Value, WriteTransaction};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tracing::warn;
use ts_rs::TS;

use super::SpeicherFehler;
use super::pc_ereignisse::PcEreignisEintrag;
use crate::api::vertrag::{Antwort, Arbeit, Aufgabe, Einstufung, Ereignis, Meldung, Vorgang};
use crate::vertrag::passkey::Passkey;

/// Verwaltung: `schema`, `fassung.<tabelle>` und `letzte_sicherung` (JJJJ-MM-TT).
pub const META: TableDefinition<&str, &str> = TableDefinition::new("meta");

/// PC-Messwerte. Schlüssel: Zeit als RFC 3339 in UTC mit fester Breite
/// (`2026-10-25T20:35:00Z`), damit die Sortierung der Zeit folgt.
/// Wert: JSON von `PcMesswert`.
pub const PC_VERLAUF: TableDefinition<&str, &[u8]> = TableDefinition::new("pc_verlauf");

/// Das Journal der Vorgänge: Ereignisnummer → JSON von `vertrag::Ereignis`.
/// Wird nur angehängt, nie geändert, nie gelöscht.
pub const EREIGNISSE: TableDefinition<u64, &[u8]> = TableDefinition::new("ereignisse");

/// Jetziger Stand je Vorgang: Kennung → JSON von `vertrag::Vorgang`.
pub const VORGAENGE: TableDefinition<&str, &[u8]> = TableDefinition::new("vorgaenge");

/// Verzeichnis: (Slug, Kennung) → nichts. Findet die Vorgänge eines Chats, ohne alle zu lesen.
pub const VORGAENGE_JE_SLUG: TableDefinition<(&str, &str), ()> = TableDefinition::new("vorgaenge_je_slug");

/// Je Chat: Slug → JSON von `Warter` (wann zuletzt übernommen, wann zuletzt geweckt).
pub const WARTER: TableDefinition<&str, &[u8]> = TableDefinition::new("warter");

/// Aufgaben für den Inhaber (Etappe 5a): Kennung → JSON von `vertrag::Aufgabe`, ohne Link.
pub const AUFGABEN: TableDefinition<&str, &[u8]> = TableDefinition::new("aufgaben");

/// Die Links der offenen Aufgaben: Kennung → mit DPAPI verschlüsselte Bytes
/// (`verschluesselung`). Nach Ablauf, Haken oder Zurückziehen gelöscht. Im
/// Export und auf /daten nur als Anzahl.
pub const AUFGABEN_LINKS: TableDefinition<&str, &[u8]> = TableDefinition::new("aufgaben_links");

/// Archiv der Aufgaben: Kennung → JSON von `vertrag::Aufgabe`, wie in `AUFGABEN`.
/// Erledigte, abgelaufene und zurückgezogene Aufgaben ziehen sieben Tage
/// nach ihrer letzten Änderung hierher um (`aufgaben::ARCHIVIEREN_NACH_TAGEN`).
/// Daten werden nie gelöscht: Das Archiv steht in Sicherung und
/// Export, nur Listen und Zähler der API lesen es nicht. Ein Link steht hier nie.
pub const AUFGABEN_ARCHIV: TableDefinition<&str, &[u8]> = TableDefinition::new("aufgaben_archiv");

/// Arbeiten der Chats (Etappe A1): Kennung → JSON von `vertrag::Arbeit`.
pub const ARBEITEN: TableDefinition<&str, &[u8]> = TableDefinition::new("arbeiten");

/// Archiv der Arbeiten: Kennung → JSON von `vertrag::Arbeit`, wie in `ARBEITEN`.
/// Fertige Arbeiten ziehen sieben Tage nach ihrer letzten Änderung hierher
/// (`arbeiten::ARCHIVIEREN_NACH_TAGEN`); nie gelöscht, nie überschrieben.
pub const ARBEITEN_ARCHIV: TableDefinition<&str, &[u8]> = TableDefinition::new("arbeiten_archiv");

/// Die Einstufungen des Inhabers: Schlüssel `projekt/art/id` (bei Fragen
/// `projekt/frage/id/gestellt`) → JSON von `vertrag::Einstufung`. „Zurück zum
/// Vorschlag“ setzt `stufe` auf null; der Eintrag bleibt.
pub const EINSTUFUNGEN: TableDefinition<&str, &[u8]> = TableDefinition::new("einstufungen");

/// Die Antworten des Inhabers auf Fragen (Etappe A2): Schlüssel `projekt/id/gestellt`
/// → JSON von `vertrag::Antwort`. Eine Antwort wird nie überschrieben.
pub const ANTWORTEN: TableDefinition<&str, &[u8]> = TableDefinition::new("antworten");

/// Archiv der Antworten: derselbe Schlüssel und Wert wie in `ANTWORTEN`.
/// Antworten ziehen sieben Tage nach dem Antworten hierher
/// (`antworten::ARCHIVIEREN_NACH_TAGEN`); nie gelöscht, nie überschrieben.
/// Eine Frage mit Antwort im Archiv bleibt beantwortet.
pub const ANTWORTEN_ARCHIV: TableDefinition<&str, &[u8]> = TableDefinition::new("antworten_archiv");

/// Meldungen für den Inhaber (Etappe A3): Kennung → JSON von `vertrag::Meldung`.
/// Wiederholungen mit demselben `schluessel` zählen `anzahl` hoch.
pub const MELDUNGEN: TableDefinition<&str, &[u8]> = TableDefinition::new("meldungen");

/// Archiv der Meldungen: Kennung → JSON von `vertrag::Meldung` (mit `archiviert`).
/// Gelesene und behobene Meldungen ziehen sieben Tage danach hierher
/// (`meldungen::ARCHIVIEREN_NACH_TAGEN`); nie gelöscht, nie überschrieben.
pub const MELDUNGEN_ARCHIV: TableDefinition<&str, &[u8]> = TableDefinition::new("meldungen_archiv");

/// Verzeichnis der Tabellen der Backend-Module (Etappe 4b): Schlüssel
/// `<slug>/<name>` → JSON von `speicher::projekte::ProjektTabelle` (Fassung,
/// Beschreibung). Nur was hier steht, gibt es als `projekt/<slug>/<name>`.
pub const PROJEKT_TABELLEN: TableDefinition<&str, &[u8]> = TableDefinition::new("projekt_tabellen");

/// Passkeys (Etappe 7c): Credential-ID → JSON von `vertrag::passkey::Passkey`
/// (öffentlicher Schlüssel, Zähler, Name, Konto, Stand, Zeiten). Nie gelöscht;
/// Ablehnung, Ablauf und Widerruf bleiben als Stand stehen.
pub const PASSKEYS: TableDefinition<&str, &[u8]> = TableDefinition::new("passkeys");

/// Offene Challenges für WebAuthn: Challenge → JSON von
/// `speicher::passkeys::PasskeyAufgabe`. Einmal gültig, 5 Minuten; beim Benutzen
/// und nach Ablauf entfernt. Nicht im Export.
pub const PASSKEY_AUFGABEN: TableDefinition<&str, &[u8]> = TableDefinition::new("passkey_aufgaben");

/// Offene Anträge auf einen Passkey: Credential-ID → JSON von
/// `speicher::passkeys::PasskeyAntrag` (Konto, Vergleichszahl, Ablauf). Nach
/// der Entscheidung oder dem Ablauf entfernt; das Ergebnis steht in `passkeys`.
/// Nicht im Export.
pub const PASSKEY_ANTRAEGE: TableDefinition<&str, &[u8]> = TableDefinition::new("passkey_antraege");

/// Gesperrte Ausweise (JWT): `jti` (oder `alle` für „alle Sitzungen beenden“)
/// → JSON von `speicher::passkeys::Sperre`. Einträge fallen nach Ablauf des
/// Ausweises weg. Nicht im Export.
pub const JWT_SPERRLISTE: TableDefinition<&str, &[u8]> = TableDefinition::new("jwt_sperrliste");

/// Windows-Ereignisse für den Verlauf (Etappe 2e): Kennung → JSON von
/// `speicher::pc_ereignisse::PcEreignisEintrag` (Ereignis, zuerst und zuletzt
/// gesehen). Updates, Starts, Installation der letzten 90 Tage.
pub const PC_EREIGNISSE: TableDefinition<&str, &[u8]> = TableDefinition::new("pc_ereignisse");

/// Archiv der Windows-Ereignisse: derselbe Schlüssel und Wert. Was älter als
/// 90 Tage ist, zieht hierher; nie gelöscht. `/api/pc/verlauf` liest es, wenn
/// `seit` vor jetzt minus 90 Tage liegt, also auch beim Standard (`seit` ist
/// dann der Tagesanfang vor 90 Tagen).
pub const PC_EREIGNISSE_ARCHIV: TableDefinition<&str, &[u8]> = TableDefinition::new("pc_ereignisse_archiv");

/// Schlüssel in `META`.
pub const META_SCHEMA: &str = "schema";
pub const META_FASSUNG_PC_VERLAUF: &str = "fassung.pc_verlauf";
pub const META_FASSUNG_EREIGNISSE: &str = "fassung.ereignisse";
pub const META_FASSUNG_VORGAENGE: &str = "fassung.vorgaenge";
pub const META_FASSUNG_VORGAENGE_JE_SLUG: &str = "fassung.vorgaenge_je_slug";
pub const META_FASSUNG_WARTER: &str = "fassung.warter";
pub const META_FASSUNG_AUFGABEN: &str = "fassung.aufgaben";
pub const META_FASSUNG_AUFGABEN_LINKS: &str = "fassung.aufgaben_links";
pub const META_FASSUNG_AUFGABEN_ARCHIV: &str = "fassung.aufgaben_archiv";
pub const META_FASSUNG_ARBEITEN: &str = "fassung.arbeiten";
pub const META_FASSUNG_ARBEITEN_ARCHIV: &str = "fassung.arbeiten_archiv";
pub const META_FASSUNG_EINSTUFUNGEN: &str = "fassung.einstufungen";
pub const META_FASSUNG_ANTWORTEN: &str = "fassung.antworten";
pub const META_FASSUNG_ANTWORTEN_ARCHIV: &str = "fassung.antworten_archiv";
pub const META_FASSUNG_MELDUNGEN: &str = "fassung.meldungen";
pub const META_FASSUNG_MELDUNGEN_ARCHIV: &str = "fassung.meldungen_archiv";
pub const META_FASSUNG_PROJEKT_TABELLEN: &str = "fassung.projekt_tabellen";
pub const META_FASSUNG_PASSKEYS: &str = "fassung.passkeys";
pub const META_FASSUNG_PASSKEY_AUFGABEN: &str = "fassung.passkey_aufgaben";
pub const META_FASSUNG_PASSKEY_ANTRAEGE: &str = "fassung.passkey_antraege";
pub const META_FASSUNG_JWT_SPERRLISTE: &str = "fassung.jwt_sperrliste";
pub const META_FASSUNG_PC_EREIGNISSE: &str = "fassung.pc_ereignisse";
pub const META_FASSUNG_PC_EREIGNISSE_ARCHIV: &str = "fassung.pc_ereignisse_archiv";
pub const META_LETZTE_SICHERUNG: &str = "letzte_sicherung";
/// Die Fassung, deren „Neu in …“ die Oberfläche schon gezeigt hat (`/api/fassungen/gezeigt`).
pub const META_NEU_IN_GEZEIGT: &str = "neu_in.gezeigt";
/// Tageszähler für Arbeitskennungen: `arbeit.zaehler.JJJJMMTT` → letzte Nummer des Tages.
pub const META_ARBEIT_ZAEHLER: &str = "arbeit.zaehler.";
/// Tageszähler für Vorgangskennungen: `vorgang.zaehler.JJJJMMTT` → letzte Nummer des Tages.
pub const META_VORGANG_ZAEHLER: &str = "vorgang.zaehler.";
/// Tageszähler für Aufgabenkennungen: `aufgabe.zaehler.JJJJMMTT` → letzte Nummer des Tages.
pub const META_AUFGABE_ZAEHLER: &str = "aufgabe.zaehler.";
/// Tageszähler für Meldungskennungen: `meldung.zaehler.JJJJMMTT` → letzte Nummer des Tages.
pub const META_MELDUNG_ZAEHLER: &str = "meldung.zaehler.";

/// Fassung des Vertragstyps `PcMesswert`; steigt, wenn sich seine Form ändert.
pub const FASSUNG_PC_VERLAUF: u32 = 1;
/// Fassungen der Tabellen aus Etappe 2b (`Ereignis`, `Vorgang`, Verzeichnis, `Warter`).
pub const FASSUNG_EREIGNISSE: u32 = 1;
pub const FASSUNG_VORGAENGE: u32 = 1;
pub const FASSUNG_VORGAENGE_JE_SLUG: u32 = 1;
pub const FASSUNG_WARTER: u32 = 1;
/// Fassungen der Tabellen aus Etappe 5a (`Aufgabe`, verschlüsselte Links).
pub const FASSUNG_AUFGABEN: u32 = 1;
pub const FASSUNG_AUFGABEN_LINKS: u32 = 1;
pub const FASSUNG_AUFGABEN_ARCHIV: u32 = 1;
/// Fassungen der Tabellen aus Etappe A1 (`Arbeit`, `Einstufung`). `Arbeit`
/// in Fassung 2 (Schema 10): dazu `besitzer` und `schritte`; Fassung 3
/// (Schema 11): dazu `etappe` und die Verlaufsarten `stand` und `etappe`.
pub const FASSUNG_ARBEITEN: u32 = 3;
pub const FASSUNG_ARBEITEN_ARCHIV: u32 = 3;
pub const FASSUNG_EINSTUFUNGEN: u32 = 1;
/// Fassungen der Tabellen aus Etappe A2 (`Antwort`).
pub const FASSUNG_ANTWORTEN: u32 = 1;
pub const FASSUNG_ANTWORTEN_ARCHIV: u32 = 1;
/// Fassungen der Tabellen aus Etappe A3 (`Meldung`).
pub const FASSUNG_MELDUNGEN: u32 = 1;
pub const FASSUNG_MELDUNGEN_ARCHIV: u32 = 1;
/// Fassung des Verzeichnisses der Modul-Tabellen (Etappe 4b).
pub const FASSUNG_PROJEKT_TABELLEN: u32 = 1;
/// Fassungen der Tabellen aus Etappe 7c (Passkeys, Challenges, Anträge, Sperrliste).
pub const FASSUNG_PASSKEYS: u32 = 1;
pub const FASSUNG_PASSKEY_AUFGABEN: u32 = 1;
pub const FASSUNG_PASSKEY_ANTRAEGE: u32 = 1;
pub const FASSUNG_JWT_SPERRLISTE: u32 = 1;
/// Fassungen der Tabellen aus Etappe 2e (`PcEreignisEintrag`).
pub const FASSUNG_PC_EREIGNISSE: u32 = 1;
pub const FASSUNG_PC_EREIGNISSE_ARCHIV: u32 = 1;

/// Die Fassungen der Vorgangs-Tabellen (Schema 2).
pub const FASSUNGEN_VORGAENGE: &[(&str, &str, u32)] = &[
    ("ereignisse", META_FASSUNG_EREIGNISSE, FASSUNG_EREIGNISSE),
    ("vorgaenge", META_FASSUNG_VORGAENGE, FASSUNG_VORGAENGE),
    ("vorgaenge_je_slug", META_FASSUNG_VORGAENGE_JE_SLUG, FASSUNG_VORGAENGE_JE_SLUG),
    ("warter", META_FASSUNG_WARTER, FASSUNG_WARTER),
];

/// Die Fassungen der Aufgaben-Tabellen (Schema 3).
pub const FASSUNGEN_AUFGABEN: &[(&str, &str, u32)] = &[
    ("aufgaben", META_FASSUNG_AUFGABEN, FASSUNG_AUFGABEN),
    ("aufgaben_links", META_FASSUNG_AUFGABEN_LINKS, FASSUNG_AUFGABEN_LINKS),
    ("aufgaben_archiv", META_FASSUNG_AUFGABEN_ARCHIV, FASSUNG_AUFGABEN_ARCHIV),
];

/// Die Fassungen der Tabellen für „Anstehend“ (Schema 4).
pub const FASSUNGEN_ANSTEHEND: &[(&str, &str, u32)] = &[
    ("arbeiten", META_FASSUNG_ARBEITEN, FASSUNG_ARBEITEN),
    ("arbeiten_archiv", META_FASSUNG_ARBEITEN_ARCHIV, FASSUNG_ARBEITEN_ARCHIV),
    ("einstufungen", META_FASSUNG_EINSTUFUNGEN, FASSUNG_EINSTUFUNGEN),
];

/// Die Fassungen der Tabellen für Antworten auf Fragen (Schema 5).
pub const FASSUNGEN_ANTWORTEN: &[(&str, &str, u32)] =
    &[("antworten", META_FASSUNG_ANTWORTEN, FASSUNG_ANTWORTEN), ("antworten_archiv", META_FASSUNG_ANTWORTEN_ARCHIV, FASSUNG_ANTWORTEN_ARCHIV)];

/// Die Fassungen der Tabellen für Meldungen (Schema 6).
pub const FASSUNGEN_MELDUNGEN: &[(&str, &str, u32)] =
    &[("meldungen", META_FASSUNG_MELDUNGEN, FASSUNG_MELDUNGEN), ("meldungen_archiv", META_FASSUNG_MELDUNGEN_ARCHIV, FASSUNG_MELDUNGEN_ARCHIV)];

/// Die Fassung des Verzeichnisses der Modul-Tabellen (Schema 7).
pub const FASSUNGEN_PROJEKTE: &[(&str, &str, u32)] = &[("projekt_tabellen", META_FASSUNG_PROJEKT_TABELLEN, FASSUNG_PROJEKT_TABELLEN)];

/// Die Fassungen der Tabellen für Passkeys (Schema 8).
pub const FASSUNGEN_PASSKEY: &[(&str, &str, u32)] = &[
    ("passkeys", META_FASSUNG_PASSKEYS, FASSUNG_PASSKEYS),
    ("passkey_aufgaben", META_FASSUNG_PASSKEY_AUFGABEN, FASSUNG_PASSKEY_AUFGABEN),
    ("passkey_antraege", META_FASSUNG_PASSKEY_ANTRAEGE, FASSUNG_PASSKEY_ANTRAEGE),
    ("jwt_sperrliste", META_FASSUNG_JWT_SPERRLISTE, FASSUNG_JWT_SPERRLISTE),
];

/// Die Fassungen der Windows-Ereignisse (Schema 9).
pub const FASSUNGEN_PC_EREIGNISSE: &[(&str, &str, u32)] = &[
    ("pc_ereignisse", META_FASSUNG_PC_EREIGNISSE, FASSUNG_PC_EREIGNISSE),
    ("pc_ereignisse_archiv", META_FASSUNG_PC_EREIGNISSE_ARCHIV, FASSUNG_PC_EREIGNISSE_ARCHIV),
];

/// Jede Tabelle mit Fassung: Name, Schlüssel in `META`, Fassung im Code.
/// Daraus legt `migration` neue Dateien an und prüft bestehende.
pub const FASSUNGEN: &[(&str, &str, u32)] = &[
    ("pc_verlauf", META_FASSUNG_PC_VERLAUF, FASSUNG_PC_VERLAUF),
    ("ereignisse", META_FASSUNG_EREIGNISSE, FASSUNG_EREIGNISSE),
    ("vorgaenge", META_FASSUNG_VORGAENGE, FASSUNG_VORGAENGE),
    ("vorgaenge_je_slug", META_FASSUNG_VORGAENGE_JE_SLUG, FASSUNG_VORGAENGE_JE_SLUG),
    ("warter", META_FASSUNG_WARTER, FASSUNG_WARTER),
    ("aufgaben", META_FASSUNG_AUFGABEN, FASSUNG_AUFGABEN),
    ("aufgaben_links", META_FASSUNG_AUFGABEN_LINKS, FASSUNG_AUFGABEN_LINKS),
    ("aufgaben_archiv", META_FASSUNG_AUFGABEN_ARCHIV, FASSUNG_AUFGABEN_ARCHIV),
    ("arbeiten", META_FASSUNG_ARBEITEN, FASSUNG_ARBEITEN),
    ("arbeiten_archiv", META_FASSUNG_ARBEITEN_ARCHIV, FASSUNG_ARBEITEN_ARCHIV),
    ("einstufungen", META_FASSUNG_EINSTUFUNGEN, FASSUNG_EINSTUFUNGEN),
    ("antworten", META_FASSUNG_ANTWORTEN, FASSUNG_ANTWORTEN),
    ("antworten_archiv", META_FASSUNG_ANTWORTEN_ARCHIV, FASSUNG_ANTWORTEN_ARCHIV),
    ("meldungen", META_FASSUNG_MELDUNGEN, FASSUNG_MELDUNGEN),
    ("meldungen_archiv", META_FASSUNG_MELDUNGEN_ARCHIV, FASSUNG_MELDUNGEN_ARCHIV),
    ("projekt_tabellen", META_FASSUNG_PROJEKT_TABELLEN, FASSUNG_PROJEKT_TABELLEN),
    ("passkeys", META_FASSUNG_PASSKEYS, FASSUNG_PASSKEYS),
    ("passkey_aufgaben", META_FASSUNG_PASSKEY_AUFGABEN, FASSUNG_PASSKEY_AUFGABEN),
    ("passkey_antraege", META_FASSUNG_PASSKEY_ANTRAEGE, FASSUNG_PASSKEY_ANTRAEGE),
    ("jwt_sperrliste", META_FASSUNG_JWT_SPERRLISTE, FASSUNG_JWT_SPERRLISTE),
    ("pc_ereignisse", META_FASSUNG_PC_EREIGNISSE, FASSUNG_PC_EREIGNISSE),
    ("pc_ereignisse_archiv", META_FASSUNG_PC_EREIGNISSE_ARCHIV, FASSUNG_PC_EREIGNISSE_ARCHIV),
];

/// So steht ein Link im Export und auf der Seite: nie der Inhalt.
pub const LINK_VERBORGEN: &str = "Link (verborgen)";

/// Inhalt einer Tabelle für den Export.
pub type Exportieren = fn(&ReadTransaction) -> Result<serde_json::Value, SpeicherFehler>;

/// Eine Tabelle mit allem, was Seite, Sicherung und Export über sie wissen.
pub struct Tabelle {
    /// Name in der Datei, zugleich das Feld im Export.
    pub name: &'static str,
    /// Für die Seite /daten, in einfachem Deutsch.
    pub beschreibung: &'static str,
    /// Zahl der Einträge; fehlt die Tabelle in der Datei, 0.
    pub eintraege: fn(&ReadTransaction) -> Result<u64, SpeicherFehler>,
    /// Alle Einträge in die Sicherungskopie. Legt die Tabelle dort auch an,
    /// wenn sie in der Datei (noch) fehlt.
    pub kopieren: fn(&ReadTransaction, &WriteTransaction) -> Result<(), SpeicherFehler>,
    /// Inhalt für den Export (Feld `name`). `None` nur für eine Tabelle mit
    /// heiklen Werten, die bewusst nicht in den Export gehört.
    pub exportieren: Option<Exportieren>,
}

/// Alle Tabellen, in der Reihenfolge der Seite /daten.
pub const ALLE: &[Tabelle] = &[
    Tabelle {
        name: "meta",
        beschreibung: "Verwaltung: Schema-Fassung und Datum der letzten Sicherung",
        eintraege: |txn| zaehlen(META, txn),
        kopieren: |von, nach| kopieren(META, von, nach),
        exportieren: Some(|txn| texte_exportieren(META, txn)),
    },
    Tabelle {
        name: "pc_verlauf",
        beschreibung: "PC-Werte alle 5 Minuten, 90 Tage",
        eintraege: |txn| zaehlen(PC_VERLAUF, txn),
        kopieren: |von, nach| kopieren(PC_VERLAUF, von, nach),
        exportieren: Some(|txn| json_exportieren::<PcMesswert>(PC_VERLAUF, txn)),
    },
    Tabelle {
        name: "pc_ereignisse",
        beschreibung: "Windows-Ereignisse für den Verlauf (Updates, Starts, Installation) der letzten 90 Tage, mit Kennung und wann zuerst und zuletzt gesehen",
        eintraege: |txn| zaehlen(PC_EREIGNISSE, txn),
        kopieren: |von, nach| kopieren(PC_EREIGNISSE, von, nach),
        exportieren: Some(|txn| json_exportieren::<PcEreignisEintrag>(PC_EREIGNISSE, txn)),
    },
    Tabelle {
        name: "pc_ereignisse_archiv",
        beschreibung: "Ältere Windows-Ereignisse, nach 90 Tagen hierher verschoben; nie gelöscht",
        eintraege: |txn| zaehlen(PC_EREIGNISSE_ARCHIV, txn),
        kopieren: |von, nach| kopieren(PC_EREIGNISSE_ARCHIV, von, nach),
        exportieren: Some(|txn| json_exportieren::<PcEreignisEintrag>(PC_EREIGNISSE_ARCHIV, txn)),
    },
    Tabelle {
        name: "ereignisse",
        beschreibung: "Journal der Vorgänge: jeder Schritt mit Nummer, Zeit und Herkunft, nur angehängt",
        eintraege: |txn| zaehlen(EREIGNISSE, txn),
        kopieren: |von, nach| kopieren(EREIGNISSE, von, nach),
        exportieren: Some(|txn| json_werte_exportieren::<u64, Ereignis>(EREIGNISSE, txn)),
    },
    Tabelle {
        name: "vorgaenge",
        beschreibung: "Vorgänge zwischen Dashboard, dir und den Chats, mit ihrem jetzigen Zustand",
        eintraege: |txn| zaehlen(VORGAENGE, txn),
        kopieren: |von, nach| kopieren(VORGAENGE, von, nach),
        exportieren: Some(|txn| json_exportieren::<Vorgang>(VORGAENGE, txn)),
    },
    Tabelle {
        name: "vorgaenge_je_slug",
        beschreibung: "Verzeichnis: welche Vorgänge zu welchem Chat gehören",
        eintraege: |txn| zaehlen(VORGAENGE_JE_SLUG, txn),
        kopieren: |von, nach| kopieren(VORGAENGE_JE_SLUG, von, nach),
        exportieren: Some(|txn| paare_exportieren(VORGAENGE_JE_SLUG, txn)),
    },
    Tabelle {
        name: "warter",
        beschreibung: "Je Chat: wann er zuletzt etwas übernommen hat und wann sein Warten zuletzt etwas lieferte",
        eintraege: |txn| zaehlen(WARTER, txn),
        kopieren: |von, nach| kopieren(WARTER, von, nach),
        exportieren: Some(|txn| json_karte_exportieren::<Warter>(WARTER, txn)),
    },
    Tabelle {
        name: "aufgaben",
        beschreibung: "Aufgaben für dich mit Titel, Projekt und Zustand; ein Link steht hier nur als „Link (verborgen)“",
        eintraege: |txn| zaehlen(AUFGABEN, txn),
        kopieren: |von, nach| kopieren(AUFGABEN, von, nach),
        exportieren: Some(|txn| aufgaben_exportieren_aus(AUFGABEN, txn)),
    },
    Tabelle {
        name: "aufgaben_links",
        beschreibung: "Links der offenen Aufgaben, verschlüsselt (nur die Anzahl, nie der Inhalt)",
        eintraege: |txn| zaehlen(AUFGABEN_LINKS, txn),
        // Die Kopie bleibt verschlüsselt; lesen kann sie nur das Konto des Dienstes.
        kopieren: |von, nach| kopieren(AUFGABEN_LINKS, von, nach),
        exportieren: Some(|txn| Ok(serde_json::json!({ "anzahl": zaehlen(AUFGABEN_LINKS, txn)? }))),
    },
    Tabelle {
        name: "aufgaben_archiv",
        beschreibung: "Abgeschlossene Aufgaben, sieben Tage nach ihrer letzten Änderung hierher verschoben; nie gelöscht",
        eintraege: |txn| zaehlen(AUFGABEN_ARCHIV, txn),
        kopieren: |von, nach| kopieren(AUFGABEN_ARCHIV, von, nach),
        exportieren: Some(|txn| aufgaben_exportieren_aus(AUFGABEN_ARCHIV, txn)),
    },
    Tabelle {
        name: "arbeiten",
        beschreibung: "Arbeiten der Chats: was läuft, wartet, als Nächstes kommt oder fertig ist",
        eintraege: |txn| zaehlen(ARBEITEN, txn),
        kopieren: |von, nach| kopieren(ARBEITEN, von, nach),
        exportieren: Some(|txn| json_exportieren::<Arbeit>(ARBEITEN, txn)),
    },
    Tabelle {
        name: "arbeiten_archiv",
        beschreibung: "Fertige Arbeiten, sieben Tage nach ihrer letzten Änderung hierher verschoben; nie gelöscht",
        eintraege: |txn| zaehlen(ARBEITEN_ARCHIV, txn),
        kopieren: |von, nach| kopieren(ARBEITEN_ARCHIV, von, nach),
        exportieren: Some(|txn| json_exportieren::<Arbeit>(ARBEITEN_ARCHIV, txn)),
    },
    Tabelle {
        name: "einstufungen",
        beschreibung: "Deine Einstufungen (Jetzt, Bald, Irgendwann) je Eintrag; ohne Stufe gilt der Vorschlag des Chats",
        eintraege: |txn| zaehlen(EINSTUFUNGEN, txn),
        kopieren: |von, nach| kopieren(EINSTUFUNGEN, von, nach),
        exportieren: Some(|txn| json_exportieren::<Einstufung>(EINSTUFUNGEN, txn)),
    },
    Tabelle {
        name: "antworten",
        beschreibung: "Deine Antworten auf Fragen der Chats, mit Zeit, Herkunft und dem Vorgang, der den Chat weckt",
        eintraege: |txn| zaehlen(ANTWORTEN, txn),
        kopieren: |von, nach| kopieren(ANTWORTEN, von, nach),
        exportieren: Some(|txn| json_exportieren::<Antwort>(ANTWORTEN, txn)),
    },
    Tabelle {
        name: "antworten_archiv",
        beschreibung: "Ältere Antworten auf Fragen, sieben Tage nach dem Antworten hierher verschoben; nie gelöscht",
        eintraege: |txn| zaehlen(ANTWORTEN_ARCHIV, txn),
        kopieren: |von, nach| kopieren(ANTWORTEN_ARCHIV, von, nach),
        exportieren: Some(|txn| json_exportieren::<Antwort>(ANTWORTEN_ARCHIV, txn)),
    },
    Tabelle {
        name: "meldungen",
        beschreibung: "Meldungen für dich (Bau oder Prüfung rot, Frist, Bote fehlt, fertig): mit Anzahl, gelesen und behoben",
        eintraege: |txn| zaehlen(MELDUNGEN, txn),
        kopieren: |von, nach| kopieren(MELDUNGEN, von, nach),
        exportieren: Some(|txn| json_exportieren::<Meldung>(MELDUNGEN, txn)),
    },
    Tabelle {
        name: "meldungen_archiv",
        beschreibung: "Gelesene und behobene Meldungen, sieben Tage danach hierher verschoben; nie gelöscht",
        eintraege: |txn| zaehlen(MELDUNGEN_ARCHIV, txn),
        kopieren: |von, nach| kopieren(MELDUNGEN_ARCHIV, von, nach),
        exportieren: Some(|txn| json_exportieren::<Meldung>(MELDUNGEN_ARCHIV, txn)),
    },
    Tabelle {
        name: "projekt_tabellen",
        beschreibung: "Verzeichnis der Tabellen der Backend-Module (projekt/<slug>/<name>) mit Fassung",
        eintraege: |txn| zaehlen(PROJEKT_TABELLEN, txn),
        kopieren: |von, nach| kopieren(PROJEKT_TABELLEN, von, nach),
        exportieren: Some(|txn| json_exportieren::<super::projekte::ProjektTabelle>(PROJEKT_TABELLEN, txn)),
    },
    Tabelle {
        name: "passkeys",
        beschreibung: "Passkeys für die Anmeldung aus anderen Windows-Konten: öffentlicher Schlüssel, Konto, Gerät, Stand und Zeiten",
        eintraege: |txn| zaehlen(PASSKEYS, txn),
        kopieren: |von, nach| kopieren(PASSKEYS, von, nach),
        exportieren: Some(|txn| json_exportieren::<Passkey>(PASSKEYS, txn)),
    },
    Tabelle {
        name: "passkey_aufgaben",
        beschreibung: "Offene Challenges für Passkeys, einmal gültig, 5 Minuten (nicht im Export)",
        eintraege: |txn| zaehlen(PASSKEY_AUFGABEN, txn),
        kopieren: |von, nach| kopieren(PASSKEY_AUFGABEN, von, nach),
        exportieren: None,
    },
    Tabelle {
        name: "passkey_antraege",
        beschreibung: "Offene Anträge auf einen Passkey aus anderen Konten, mit Vergleichszahl und Ablauf (nicht im Export)",
        eintraege: |txn| zaehlen(PASSKEY_ANTRAEGE, txn),
        kopieren: |von, nach| kopieren(PASSKEY_ANTRAEGE, von, nach),
        exportieren: None,
    },
    Tabelle {
        name: "jwt_sperrliste",
        beschreibung: "Beendete Sitzungen (abgemeldet oder alle beendet), bis ihr Ausweis abläuft (nicht im Export)",
        eintraege: |txn| zaehlen(JWT_SPERRLISTE, txn),
        kopieren: |von, nach| kopieren(JWT_SPERRLISTE, von, nach),
        exportieren: None,
    },
];

/// Die Aufgaben im Export: wie gespeichert, dazu `link = "Link (verborgen)"`,
/// wenn es einen Link gab. Der Link selbst steht nie im Export.
fn aufgaben_exportieren_aus(tabelle: TableDefinition<&'static str, &'static [u8]>, txn: &ReadTransaction) -> Result<serde_json::Value, SpeicherFehler> {
    let mut liste = json_exportieren::<Aufgabe>(tabelle, txn)?;
    if let Some(eintraege) = liste.as_array_mut() {
        for e in eintraege {
            if e.get("hat_link").and_then(serde_json::Value::as_bool) == Some(true)
                && let Some(map) = e.as_object_mut()
            {
                map.insert("link".into(), serde_json::Value::String(LINK_VERBORGEN.into()));
            }
        }
    }
    Ok(liste)
}

/// Eintrag in `WARTER` (Schlüssel: Slug des Chats).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Warter {
    /// Wann der Chat zuletzt einen Vorgang übernommen hat (Ortszeit, RFC 3339).
    pub zuletzt_uebernommen: Option<String>,
    /// Wann sein Warten zuletzt Vorgänge geliefert hat (Ortszeit, RFC 3339).
    pub zuletzt_geweckt: Option<String>,
    /// Kleinkram 2: Bis zu dieser Ereignisnummer hat das Warten die Ergebnisse
    /// der Bau-Vorgänge geliefert, die der Baumeister selbst erledigt hat.
    /// Ältere Einträge haben das Feld nicht (darum freiwillig, ohne neue Tabellen-Fassung).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bau_bis: Option<u64>,
}

/// Ein Messwert des PCs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct PcMesswert {
    /// RFC 3339 in UTC mit fester Breite, zugleich der Schlüssel.
    pub zeit: String,
    /// Prozessorlast in Prozent; `null`, wenn sie unbekannt war (erste
    /// Messung, GetSystemTimes scheiterte, mehrmals genau 0): eine Lücke,
    /// keine 0 (Etappe 3a+). Ältere Einträge haben immer eine Zahl.
    pub cpu: Option<f32>,
    /// Belegter Arbeitsspeicher in Prozent.
    pub mem: f32,
    /// Erste Platte: belegte und gesamte Bytes.
    pub platte_belegt: Option<u64>,
    pub platte_gesamt: Option<u64>,
}

/// Zeit als Schlüssel: RFC 3339 in UTC, sekundengenau, feste Breite.
pub fn zeit_schluessel(zeit: chrono::DateTime<chrono::Utc>) -> String {
    zeit.format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

// ----- Bausteine für die Einträge in `ALLE` -----

/// Öffnet eine Tabelle zum Lesen; fehlt sie in der Datei, `None`.
pub(super) fn lesen<K: Key + 'static, V: Value + 'static>(
    def: TableDefinition<K, V>,
    txn: &ReadTransaction,
) -> Result<Option<ReadOnlyTable<K, V>>, SpeicherFehler> {
    match txn.open_table(def) {
        Ok(t) => Ok(Some(t)),
        Err(TableError::TableDoesNotExist(_)) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Zahl der Einträge; fehlt die Tabelle, 0.
pub fn zaehlen<K: Key + 'static, V: Value + 'static>(def: TableDefinition<K, V>, txn: &ReadTransaction) -> Result<u64, SpeicherFehler> {
    match lesen(def, txn)? {
        Some(t) => Ok(t.len()?),
        None => Ok(0),
    }
}

/// Kopiert jeden Eintrag unverändert; die Tabelle entsteht in der Kopie auch, wenn sie leer ist.
pub fn kopieren<K: Key + 'static, V: Value + 'static>(
    def: TableDefinition<K, V>,
    von: &ReadTransaction,
    nach: &WriteTransaction,
) -> Result<(), SpeicherFehler> {
    let mut ziel = nach.open_table(def)?;
    let Some(quelle) = lesen(def, von)? else { return Ok(()) };
    for eintrag in quelle.iter()? {
        let (k, v) = eintrag?;
        ziel.insert(k.value(), v.value())?;
    }
    Ok(())
}

/// Text zu Text als Tabelle im Export (`[meta]` im TOML).
pub fn texte_exportieren(def: TableDefinition<&str, &str>, txn: &ReadTransaction) -> Result<serde_json::Value, SpeicherFehler> {
    let mut map = serde_json::Map::new();
    if let Some(t) = lesen(def, txn)? {
        for eintrag in t.iter()? {
            let (k, v) = eintrag?;
            map.insert(k.value().to_string(), serde_json::Value::String(v.value().to_string()));
        }
    }
    Ok(serde_json::Value::Object(map))
}

/// Werte als JSON von `T`, als Liste in Schlüsselreihenfolge. Unlesbare
/// Einträge fehlen im Export und stehen im Protokoll.
pub fn json_exportieren<T: DeserializeOwned + Serialize>(
    def: TableDefinition<&str, &[u8]>,
    txn: &ReadTransaction,
) -> Result<serde_json::Value, SpeicherFehler> {
    json_werte_exportieren::<&str, T>(def, txn)
}

/// Wie `json_exportieren`, für jede Art von Schlüssel (etwa Nummern in `ereignisse`).
pub fn json_werte_exportieren<K: Key + 'static, T: DeserializeOwned + Serialize>(
    def: TableDefinition<K, &[u8]>,
    txn: &ReadTransaction,
) -> Result<serde_json::Value, SpeicherFehler> {
    let mut liste: Vec<T> = Vec::new();
    if let Some(t) = lesen(def, txn)? {
        for (stelle, eintrag) in t.iter()?.enumerate() {
            let (_, v) = eintrag?;
            match serde_json::from_slice::<T>(v.value()) {
                Ok(wert) => liste.push(wert),
                Err(e) => warn!("Export: {}-Eintrag Nr. {} unlesbar: {e}", def, stelle + 1),
            }
        }
    }
    // Über den Text: so bleibt eine f32 wie 12.3 auch 12.3 (direkt in einen
    // JSON-Wert würde sie zu 12.300000190734863).
    Ok(serde_json::from_str(&serde_json::to_string(&liste)?)?)
}

/// Werte als JSON von `T`, als Tabelle Schlüssel → Wert (für Tabellen, deren
/// Schlüssel nicht im Wert steht, etwa `warter`).
pub fn json_karte_exportieren<T: DeserializeOwned + Serialize>(
    def: TableDefinition<&str, &[u8]>,
    txn: &ReadTransaction,
) -> Result<serde_json::Value, SpeicherFehler> {
    let mut karte: std::collections::BTreeMap<String, T> = std::collections::BTreeMap::new();
    if let Some(t) = lesen(def, txn)? {
        for eintrag in t.iter()? {
            let (k, v) = eintrag?;
            match serde_json::from_slice::<T>(v.value()) {
                Ok(wert) => {
                    karte.insert(k.value().to_string(), wert);
                }
                Err(e) => warn!("Export: {}-Eintrag {} unlesbar: {e}", def, k.value()),
            }
        }
    }
    Ok(serde_json::from_str(&serde_json::to_string(&karte)?)?)
}

/// Ein Verzeichnis aus Paaren ohne Wert, als Liste `[[a, b], …]`.
pub fn paare_exportieren(def: TableDefinition<(&str, &str), ()>, txn: &ReadTransaction) -> Result<serde_json::Value, SpeicherFehler> {
    let mut liste = Vec::new();
    if let Some(t) = lesen(def, txn)? {
        for eintrag in t.iter()? {
            let (k, _) = eintrag?;
            let (a, b) = k.value();
            liste.push(serde_json::json!([a, b]));
        }
    }
    Ok(serde_json::Value::Array(liste))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn zeit_schluessel_hat_feste_breite() {
        let t = chrono::Utc.with_ymd_and_hms(2026, 10, 5, 7, 3, 9).unwrap();
        assert_eq!(zeit_schluessel(t), "2026-10-05T07:03:09Z");
        assert_eq!(zeit_schluessel(t).len(), 20);
    }

    #[test]
    fn namen_eindeutig() {
        for (i, t) in ALLE.iter().enumerate() {
            assert!(ALLE[i + 1..].iter().all(|u| u.name != t.name), "{} doppelt", t.name);
        }
    }
}
