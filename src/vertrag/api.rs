//! Alle JSON-Typen der API. Hier stehen nur Datenformen, keine Handler.
//! Die TypeScript-Typen dazu erzeugt `export.rs` nach `web/src/lib/vertrag/`.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::abgeleitet::Abgeleitet;
use super::bedeutung::Bedeutung;
use super::daten::{DatenInhalt, ProjektDaten};
use super::projekt::{Link, Projekt, Seite};
use crate::git::RepoStatus;
use crate::inhalt::seite::{self, Broken, Project};
use crate::pc::{inventory, pcdata, sysapps, system};
use crate::speicher::tabellen::PcMesswert;

/// Die Typen für „Anstehend“ (Etappe A1) stehen in `anstehend.rs`; hier als Teil der API.
pub use super::anstehend::*;
/// Das Backlog eines Projekts (Etappe B2) steht in `backlog.rs`; hier als Teil der API.
pub use super::backlog::*;
/// Die Fassungen aus dem Changelog stehen in `fassungen.rs`; hier als Teil der API.
pub use super::fassungen::*;
/// Die System-Seiten (Etappe 4d) stehen in `system_seiten.rs`; hier als Teil der API.
pub use super::system_seiten::*;
/// Befund und Schwere stehen bei den Regeln (`inhalt::pruefung`); hier als Teil der API.
pub use crate::inhalt::pruefung::Befund;

/// Kopf eines Projekts, wie die Startseite ihn zeigt.
#[derive(Debug, Clone, Serialize, TS)]
pub struct ProjectSummary {
    pub slug: String,
    pub title: String,
    /// Das Wort aus dem Kopf (aktiv, pausiert, geplant, fertig, problem, archiviert).
    pub status: String,
    /// Farbe des Status als Bedeutung.
    pub bedeutung: Bedeutung,
    pub last_change: String,
    pub summary: String,
    pub chat: Option<String>,
    /// Das Wort aus dem Kopf: rot, gelb, grün.
    pub backup: Option<String>,
    pub backup_bedeutung: Option<Bedeutung>,
    pub backup_note: String,
    /// Adresse des Entwurfs (`/projects/<slug>/assets/<datei>`), wenn `vorschau` gesetzt ist.
    pub vorschau: Option<String>,
    pub order: i32,
    /// `open = [...]` aus dem Kopf. Etappe B2: nur mit Windows-Anmeldung;
    /// ohne ist die Liste leer, und es bleibt `open_zahl`.
    pub open: Vec<String>,
    /// Etappe B2: wie viele offene Punkte (`open`) der Kopf hat, auch ohne
    /// Anmeldung. Steht immer im JSON; in TypeScript optional (ältere Beispiele).
    #[ts(as = "Option<u64>", optional)]
    pub open_zahl: u64,
    pub repos: Vec<RepoStatus>,
    /// Ortszeit, RFC 3339.
    #[ts(type = "string | null")]
    pub file_modified: Option<chrono::DateTime<chrono::Local>>,
    /// Kopfleiste, Fassung 3: Name des Schwerpunkts aus `data/schwerpunkte.toml`,
    /// sonst „Sonstiges“ (derselbe Fakt wie `schwerpunkt` in „Anstehend“ und im
    /// Stand des Projekts, beide schon frei). Die Seitenleiste gruppiert danach.
    /// Steht immer im JSON; in TypeScript optional (ältere Beispiele).
    #[ts(as = "Option<String>", optional)]
    pub schwerpunkt: String,
    /// `reihenfolge` dieses Schwerpunkts (kleiner steht weiter vorn); „Sonstiges“
    /// hat eins mehr als der letzte.
    #[ts(as = "Option<i64>", optional)]
    pub schwerpunkt_reihenfolge: i64,
    /// Etappe 4c: Bau-Stand der Seite (nur Projekte mit `Seite.svelte`; frei,
    /// ohne Satz). Fehlt, wenn das Projekt keine eigene Seite hat oder der
    /// Stand des Baumeisters nicht lesbar ist.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub bau: Option<ProjektBau>,
}

impl ProjectSummary {
    /// Etappe 4c: setzt den Bau-Stand (aus `baustand`).
    pub fn mit_bau(mut self, bau: Option<ProjektBau>) -> Self {
        self.bau = bau;
        self
    }

    /// Etappe B2: ohne Windows-Anmeldung keine Texte der offenen Punkte, nur ihre Zahl.
    pub fn ohne_texte(mut self, angemeldet: bool) -> Self {
        if !angemeldet {
            self.open.clear();
        }
        self
    }

    /// Setzt den Schwerpunkt (Name und `reihenfolge`) aus `data/schwerpunkte.toml`.
    pub fn mit_schwerpunkt(mut self, (name, reihenfolge): (String, i64)) -> Self {
        self.schwerpunkt = name;
        self.schwerpunkt_reihenfolge = reihenfolge;
        self
    }

    pub fn from_project(p: &Project, repos: Vec<RepoStatus>) -> Self {
        Self {
            slug: p.slug.clone(),
            title: p.meta.title.clone(),
            status: p.meta.status.clone(),
            bedeutung: Bedeutung::aus_wort_oder_neutral(&p.meta.status),
            last_change: p.last_change(),
            summary: p.meta.summary.clone(),
            chat: p.meta.chat.clone(),
            backup: p.meta.backup.clone(),
            backup_bedeutung: p.meta.backup.as_deref().map(Bedeutung::aus_wort_oder_neutral),
            backup_note: p.meta.backup_note.clone(),
            vorschau: p.meta.vorschau.as_deref().and_then(seite::preview_file).map(|f| format!("/projects/{}/assets/{f}", p.slug)),
            order: p.meta.order,
            open: p.meta.open.clone(),
            open_zahl: p.meta.open.len() as u64,
            repos,
            file_modified: p.file_modified,
            // Den Schwerpunkt setzt die Liste (`mit_schwerpunkt`); ohne Datei „Sonstiges“.
            schwerpunkt: crate::vertrag::schwerpunkte::SONSTIGES.to_string(),
            schwerpunkt_reihenfolge: 1,
            bau: None,
        }
    }
}

/// `GET /api/projekte/<slug>`: der Kopf aus `projekt.toml` und welche Daten
/// es gibt.
#[derive(Debug, Clone, Serialize, TS)]
pub struct ProjektAntwort {
    pub slug: String,
    /// `projekt.toml`, geprüft mit dem Vertrag.
    pub kopf: Projekt,
    /// Namen der Dateien in `daten/` ohne `.toml` (`gantt`, `eigen-notizen`),
    /// nach dem Alphabet; abrufbar unter `/api/projekte/<slug>/daten/<name>`.
    pub daten: Vec<String>,
    /// Etappe B2: wie viele offene Punkte `kopf.open` hat. Ohne Anmeldung ist
    /// `kopf.open` leer, die Zahl bleibt. In TypeScript optional (ältere Beispiele).
    #[ts(as = "Option<u64>", optional)]
    pub open_zahl: u64,
}

/// `GET /api/projekte/<slug>/daten/<name>`: eine Datei aus `daten/` und was
/// der Server daraus rechnet.
#[derive(Debug, Clone, Serialize, TS)]
pub struct DatenAntwort {
    /// Name ohne `.toml`, wie in der Adresse.
    pub name: String,
    /// Der Inhalt wie in `ProjektDaten` für diese Datei; Kennzahlen mit
    /// `berechnet` haben ihren `wert` schon.
    pub inhalt: DatenInhalt,
    /// Bei jeder Anfrage aus genau dieser Datei gerechnet.
    pub abgeleitet: Abgeleitet,
}

/// Fehlerantwort, wenn eine Datei den Vertrag bricht (422, `art = "vertrag"`):
/// derselbe Satz wie in der Prüfung, dazu Datei und Zeile einzeln.
#[derive(Debug, Clone, Serialize, TS)]
pub struct VertragsFehler {
    /// Ganzer Satz mit Datei und Zeile: `projects/x/daten/gantt.toml, Zeile 3: …`.
    pub fehler: String,
    /// Immer `vertrag`.
    pub art: String,
    /// Pfad wie `projects/<slug>/daten/gantt.toml`.
    pub datei: String,
    /// Zeile ab 1, wenn sie sich bestimmen lässt.
    pub zeile: Option<u32>,
    /// Nur der Satz, ohne Datei und Zeile.
    pub satz: String,
}

/// `GET /api/projects`: alle Projekte außer den archivierten (Entscheidung
/// archiv-status; die stehen in `/api/archiv`).
#[derive(Debug, Serialize, TS)]
pub struct ProjectList {
    pub projects: Vec<ProjectSummary>,
    pub broken: Vec<Broken>,
    /// Projektgruppen aus `data/gruppen.toml` (Entscheidung
    /// projektgruppe): je Gruppe Titel und die sichtbaren Teile in
    /// der Reihenfolge der Seite. Ohne Datei leer.
    pub gruppen: Vec<super::gruppen::GruppeAnzeige>,
}

/// `GET /api/archiv` (Entscheidung archiv-status): die
/// archivierten Projekte für die Archivansicht `/projekte/archiviert`, nach Titel.
#[derive(Debug, Clone, Serialize, TS)]
pub struct ArchivListe {
    pub projekte: Vec<ArchivProjekt>,
}

/// Ein archiviertes Projekt in der Archivansicht.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct ArchivProjekt {
    pub slug: String,
    pub titel: String,
    /// Seit wann archiviert (JJJJ-MM-TT): `archiviert_seit` aus `projekt.toml`,
    /// sonst die letzte Änderung von `projekt.toml`, denn dort steht der Status.
    /// `null`, wenn sich die Zeit nicht lesen lässt.
    pub seit: Option<String>,
}

#[derive(Debug, Serialize, TS)]
pub struct CheckResult {
    pub name: String,
    pub url: String,
    pub ok: bool,
    pub status: Option<u16>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize, TS)]
pub struct ProcResult {
    pub name: String,
    pub running: bool,
}

#[derive(Debug, Serialize, TS)]
pub struct ProjectDetail {
    #[serde(flatten)]
    pub head: ProjectSummary,
    pub links: Vec<Link>,
    pub checks: Vec<CheckResult>,
    pub processes: Vec<ProcResult>,
    /// Unterseiten aus `projekt.toml`, nach `order`.
    pub seiten: Vec<Seite>,
    /// Alle `daten/*.toml`.
    pub daten: Option<ProjektDaten>,
}

/// Ob alle angeforderten Schlüssel im Tresor stehen (Etappe 3a-4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum Vollstaendig {
    /// Keiner fehlt.
    Ja,
    /// Mindestens einer fehlt (`fehlen` größer 0).
    Nein,
    /// Nicht bekannt; warum, steht in `grund`.
    Unbekannt,
}

/// Warum der Stand der Schlüssel unbekannt ist. `anmeldung` und
/// `fremdes-konto` liefert der Server als Fehlerart von `/api/pc/benutzer`
/// (401 und 403, `art`), dort gibt es keinen Stand; in `SchluesselStand.grund`
/// stehen `bote-fehlt` und `liste-unlesbar`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum SchluesselGrund {
    /// Ohne Windows-Anmeldung (401).
    Anmeldung,
    /// Ein anderes Windows-Konto (403).
    FremdesKonto,
    /// Der Bote hat nicht geantwortet (läuft nicht oder ist ausgelastet).
    BoteFehlt,
    /// `data/schluessel.toml` ließ sich nicht lesen.
    ListeUnlesbar,
    /// Der Bote antwortet, meldet aber für mindestens einen Schlüssel einen
    /// Fehler beim Lesen des Tresors (Etappe 3a+: unbekannt, nicht „fehlt“).
    TresorFehler,
}

/// Stand der Schlüssel für `/api/pc/benutzer`: wie viele gespeichert sind,
/// wie viele fehlen, und ob damit alle da sind. Bei `unbekannt` sind beide
/// Zahlen 0 und `grund` sagt warum.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct SchluesselStand {
    pub vollstaendig: Vollstaendig,
    pub gespeichert: usize,
    pub fehlen: usize,
    /// Nur bei `unbekannt`.
    pub grund: Option<SchluesselGrund>,
}

impl SchluesselStand {
    /// Aus den Zahlen: fehlt einer, `nein`, sonst `ja` (auch ohne Anforderung).
    pub fn aus_zahlen(gespeichert: usize, fehlen: usize) -> Self {
        let vollstaendig = if fehlen > 0 { Vollstaendig::Nein } else { Vollstaendig::Ja };
        Self { vollstaendig, gespeichert, fehlen, grund: None }
    }

    pub fn unbekannt(grund: SchluesselGrund) -> Self {
        Self { vollstaendig: Vollstaendig::Unbekannt, gespeichert: 0, fehlen: 0, grund: Some(grund) }
    }
}

#[derive(Debug, Serialize, TS)]
pub struct ProjectBackup {
    pub slug: String,
    pub title: String,
    pub chat: Option<String>,
    pub backup: String,
    pub bedeutung: Bedeutung,
    pub backup_note: String,
}

/// `/api/pc`: die Rechner-Werte, frei für jeden (`[anmeldung]`). Die
/// Benutzer-Daten stehen seit Etappe 7b getrennt in `PcBenutzer`
/// (`/api/pc/benutzer`, nur für das Windows-Konto des Inhabers).
#[derive(Debug, Serialize, TS)]
pub struct PcReport {
    pub snapshot: system::Snapshot,
    pub sysapps: SysAppsRechner,
    pub data: pcdata::PcData,
    pub data_error: Option<String>,
    /// Backup-Stand (Kopien und 3-2-1-Regel) aus `data.backup`; `null`, wenn
    /// `data/pc.toml` einen Fehler hat.
    pub backup_stand: Option<pcdata::BackupLage>,
    pub project_backups: Vec<ProjectBackup>,
}

/// Der Rechner-Teil der Systemanwendungen: Windows und Dienste. Hotfixes,
/// Updates und Starts stehen seit Etappe 3a-1 nur im Verlauf (`/api/pc/verlauf`,
/// Datenspeicher `pc_ereignisse`); die Laufzeit steht in `snapshot.uptime_secs`.
#[derive(Debug, Serialize, TS)]
pub struct SysAppsRechner {
    #[ts(type = "string | null")]
    pub refreshed_at: Option<chrono::DateTime<chrono::Local>>,
    pub windows: sysapps::WindowsInfo,
    pub services_running: Vec<sysapps::Service>,
    pub services_total: usize,
    /// Fehler der eigenen Messung; der Satz zu den Benutzer-Daten steht nicht hier.
    pub errors: Vec<String>,
}

/// `/api/pc/benutzer` (geschützt, nur das Windows-Konto des Inhabers): was der Bote
/// unter dem Konto des Inhabers liest (Ordnergrößen, winget, Autostart, Werkzeuge),
/// dazu der Stand der Schlüssel.
#[derive(Debug, Serialize, TS)]
pub struct PcBenutzer {
    pub inventory: inventory::Inventory,
    pub sysapps: SysAppsBenutzer,
    pub schluessel: SchluesselStand,
    /// `da`; `bote-fehlt`, wenn der Bote die Daten gerade nicht geliefert hat
    /// (dann steht der letzte Stand da, `stand` sagt von wann).
    #[ts(type = "\"da\" | \"bote-fehlt\"")]
    pub bote: &'static str,
    /// Wann winget, Autostart und Werkzeuge zuletzt kamen; `null`: noch nie.
    #[ts(type = "string | null")]
    pub stand: Option<chrono::DateTime<chrono::Local>>,
}

/// Der Benutzer-Teil der Systemanwendungen.
#[derive(Debug, Serialize, TS)]
pub struct SysAppsBenutzer {
    pub autostart: Vec<sysapps::AutostartEntry>,
    pub tools: Vec<sysapps::Tool>,
    pub winget: Vec<sysapps::WingetEntry>,
    pub winget_error: Option<String>,
}

/// Status eines Schlüssels: nie der Wert, nie der Benutzername.
#[derive(Debug, Serialize, TS)]
pub struct KeyStatus {
    pub name: String,
    pub zweck: String,
    pub hinweis: String,
    pub angefordert_von: String,
    pub target: String,
    #[ts(type = "string | null")]
    pub saved_at: Option<chrono::DateTime<chrono::Local>>,
    /// Stand im Tresor (Etappe 3a+): `unbekannt`, wenn der Bote für diesen
    /// Schlüssel einen Fehler meldet oder nicht antwortet; nie als `fehlt` gezählt.
    pub stand: TresorStand,
}

/// Ob ein Schlüssel im Windows-Tresor steht.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum TresorStand {
    Gespeichert,
    Fehlt,
    Unbekannt,
}

#[derive(Debug, Serialize, TS)]
pub struct KeyList {
    pub keys: Vec<KeyStatus>,
    pub error: Option<String>,
}

/// Eine Tabelle des Datenspeichers, für die Seite /daten.
#[derive(Debug, Serialize, TS)]
pub struct TabelleInfo {
    pub name: String,
    pub beschreibung: String,
    pub eintraege: u64,
}

/// Erster und letzter Messwert im PC-Verlauf (Zeit in UTC, RFC 3339).
#[derive(Debug, Serialize, TS)]
pub struct PcVerlaufEckdaten {
    pub erster: Option<String>,
    pub letzter: Option<String>,
    pub eintraege: u64,
}

/// `/api/daten`: Ort, Größe und Stand des Datenspeichers. Pfade im
/// Benutzerordner stehen als `%USERPROFILE%\…` da.
#[derive(Debug, Serialize, TS)]
pub struct DatenBericht {
    pub pfad: String,
    pub groesse_bytes: u64,
    pub schema: u32,
    /// Datum der letzten Sicherung (JJJJ-MM-TT).
    pub letzte_sicherung: Option<String>,
    /// `ok` = heute oder gestern, `warn` = 2 bis 7 Tage alt, `bad` = älter oder nie.
    #[ts(type = "\"ok\" | \"warn\" | \"bad\"")]
    pub sicherung_klasse: &'static str,
    pub sicherungs_ordner: String,
    pub tabellen: Vec<TabelleInfo>,
    pub pc_verlauf: PcVerlaufEckdaten,
    /// Gesetzt, wenn sich der Datenspeicher beim Start nicht öffnen ließ: Satz
    /// für den Inhaber mit dem Sicherungsordner. Dann ist `schema` 0, `tabellen` leer
    /// und `sicherung_klasse` `bad`. Sonst `null`.
    pub fehler: Option<String>,
}

/// `/api/daten/pc-verlauf`: Messwerte in Zeitreihenfolge.
#[derive(Debug, Serialize, TS)]
pub struct PcVerlaufAntwort {
    pub werte: Vec<PcMesswert>,
}

/// `POST /api/daten/sichern`: wie die Sicherung heißt. Nur Dateinamen ohne
/// Ordner; der Ordner steht in `DatenBericht::sicherungs_ordner`.
#[derive(Debug, Serialize, TS)]
pub struct SicherungAntwort {
    /// Dateiname der Kopie, etwa `dashboard-2026-10-25.redb`.
    pub datei: String,
    /// Dateiname des Exports, etwa `dashboard-2026-10-25.toml`.
    pub export: String,
    /// Tag der Sicherung, JJJJ-MM-TT.
    pub datum: String,
}

// ----- Vorgänge (Zielbild, Kapitel c2) -----

/// Welche Art von Vorgang; im JSON klein geschrieben (`"antwort"`, …).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum VorgangsArt {
    Antwort,
    Frage,
    Wunsch,
    Bau,
    Abnahme,
    /// Merge-Request eines Projekt-Chats für sein Backend-Modul (Etappe 4b).
    Merge,
    /// Ein anderes Windows-Konto beantragt einen Passkey (Etappe 7c). Legt nur
    /// der Server selbst an (Slug `passkey`); Freigabe, Ablehnung, Ablauf und
    /// Widerruf schließen ihn mit Notiz.
    #[serde(rename = "passkey-antrag")]
    PasskeyAntrag,
    /// Der Inhaber hat einen Ort geöffnet (Entscheidung lokal-oeffnen).
    /// Nur der Server legt ihn an, gleich als `erledigt` (ein Eintrag im Journal
    /// mit Zeit, Projekt und Kennung, nie dem Pfad); weckt keinen Chat, keine Meldung.
    Oeffnen,
    /// Ein Teilschritt weckt einen Chat (Zweig teilschritte, T3): ihm wurde ein
    /// Schritt angeboten, oder sein wartender Schritt läuft wieder. Nur der Server
    /// legt ihn an (`bezug` = `schritt:<arbeit>/<nr>`) und erledigt ihn, sobald sich
    /// der Schritt ändert.
    Schritt,
}

impl VorgangsArt {
    pub const ALLE: [VorgangsArt; 9] =
        [Self::Antwort, Self::Frage, Self::Wunsch, Self::Bau, Self::Abnahme, Self::Merge, Self::PasskeyAntrag, Self::Oeffnen, Self::Schritt];

    pub fn wort(self) -> &'static str {
        match self {
            Self::Antwort => "antwort",
            Self::Frage => "frage",
            Self::Wunsch => "wunsch",
            Self::Bau => "bau",
            Self::Abnahme => "abnahme",
            Self::Merge => "merge",
            Self::PasskeyAntrag => "passkey-antrag",
            Self::Oeffnen => "oeffnen",
            Self::Schritt => "schritt",
        }
    }

    pub fn aus_wort(wort: &str) -> Option<Self> {
        Self::ALLE.into_iter().find(|a| a.wort() == wort)
    }
}

/// Zustand eines Vorgangs, immer nur vorwärts (die Reihenfolge von `Ord`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(rename = "VorgangsZustand")]
pub enum Zustand {
    Offen,
    Zugestellt,
    Uebernommen,
    Erledigt,
}

/// Wie es um einen Bau steht (nur bei Vorgängen der Art `bau`, Etappe 4v).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum BauZustand {
    /// Der Dienst hat den Vorgang angenommen; der Bote baut.
    Laeuft,
    /// Gebaut; die neue Oberfläche wird schon ausgeliefert.
    Gruen,
    /// Nicht gebaut; die bisherige Oberfläche bleibt.
    Rot,
}

/// Der Bau zu einem Vorgang der Art `bau`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct BauStand {
    pub zustand: BauZustand,
    /// Ein Satz; bei rot danach die wichtigsten Zeilen der Ausgabe.
    pub meldung: String,
    /// Projekte, die rot waren und darum in diesem Bau fehlen.
    pub rote: Vec<String>,
}

/// Etappe 4c: Wie es um die Seite eines Projekts steht (Projekte mit
/// `Seite.svelte`). `laeuft` heißt auch „wartet auf den Bau“.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum SeitenBau {
    /// Geprüft und gebaut; die Seite ist die aktuelle.
    Gruen,
    /// Die neue Fassung hat einen Fehler; gezeigt wird die letzte gute (wenn es eine gibt).
    Rot,
    /// Ein Bau läuft oder ist bestellt.
    Laeuft,
    /// Noch nie gebaut (neue Seite, oder der Baumeister lief noch nie).
    NochNicht,
    /// Der Stand des Baumeisters ist gerade nicht lesbar (Abnahme 4c).
    Unbekannt,
    /// Zweig A: Ein Prüfwerkzeug lief nicht, die neue Fassung ist nicht
    /// geprüft. Kein Rot; gezeigt wird die letzte gute Fassung, ohne sie fehlt
    /// die Seite (`letzte_gute` ist dann `null`).
    NichtGeprueft,
}

/// Etappe 4c: der Bau-Stand eines Projekts, frei in `/api/projects`.
/// Ohne Satz: den gibt es nur geschützt unter `/api/bauen`.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct ProjektBau {
    pub zustand: SeitenBau,
    /// Wann die letzte gute Fassung entstand (Ortszeit, RFC 3339); `null`,
    /// wenn es keine gibt.
    #[ts(type = "string | null")]
    pub letzte_gute: Option<chrono::DateTime<chrono::Local>>,
    /// Wann die Seite zuletzt in einen geltenden
    /// Bau kam (Ortszeit, RFC 3339), bei jedem Bau neu, auch ohne Änderung.
    /// Die Oberfläche zeigt es nicht mehr; es bleibt für Chats
    /// in `/api/projects` (sehen, ob ein bestellter Bau die Seite erreicht hat).
    /// Fehlt, wenn die Seite gerade nicht zu sehen ist. Nur eine Zeit, kein
    /// Satz: darum frei.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "string")]
    pub gebaut_um: Option<chrono::DateTime<chrono::Local>>,
}

/// Etappe 4c: ein Projekt in `GET /api/bauen` (geschützt), mit Satz.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct ProjektBauSatz {
    pub slug: String,
    pub zustand: SeitenBau,
    #[ts(type = "string | null")]
    pub letzte_gute: Option<chrono::DateTime<chrono::Local>>,
    /// Wie `ProjektBau::gebaut_um`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "string")]
    pub gebaut_um: Option<chrono::DateTime<chrono::Local>>,
    /// Bei `rot` und `nicht_geprueft` ein Satz in einfachem Deutsch, ohne
    /// Pfade und Fachwörter; sonst `null`. Nur hier (geschützt), nie in `/api/projects`.
    pub satz: Option<String>,
}

/// Etappe 4c: `GET /api/bauen` (nur der Inhaber): der Bau-Stand aller Projekte
/// mit eigener Seite, nach Slug.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct BauListe {
    pub projekte: Vec<ProjektBauSatz>,
}

/// Antwort auf `POST /api/oeffnen` (Entscheidung lokal-oeffnen):
/// nur ein Satz, nie ein Pfad oder Inhalt der Datei.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct OeffnenAntwort {
    pub projekt: String,
    pub kennung: String,
    /// „Geöffnet.“
    pub satz: String,
}

/// Ein Vorgang mit seinem jetzigen Zustand (Tabelle `vorgaenge`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct Vorgang {
    /// Kennung `v-JJJJMMTT-NNNN`.
    pub id: String,
    pub art: VorgangsArt,
    /// Der Chat, der handeln muss.
    pub slug: String,
    pub titel: String,
    /// Worauf er sich bezieht, etwa `antwort:beispiel/schlusssatz`.
    pub bezug: Option<String>,
    pub zustand: Zustand,
    /// Ortszeit, RFC 3339.
    pub erstellt: String,
    pub geaendert: String,
    /// Nummer des letzten Ereignisses dieses Vorgangs.
    pub letzte_nr: u64,
    /// Wer ihn angelegt hat: `browser`, `baumeister`, `server`, `chat` oder `chat:<slug>`.
    pub von: String,
    /// Letzte Notiz (etwa beim Erledigen).
    pub notiz: Option<String>,
    /// Nur bei der Art `bau`, sobald der Dienst ihn angenommen hat. Ältere
    /// Einträge haben das Feld nicht (darum freiwillig, ohne neue Tabellen-Fassung).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub bau: Option<BauStand>,
    /// Nur bei der Art `merge` (Etappe 4b): was der Projekt-Chat einreicht.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub merge: Option<MergeAntrag>,
}

/// Ein Merge-Request (Vorgang der Art `merge`, Etappe 4b): ein Projekt-Chat
/// reicht sein Backend-Modul beim Betreuer des Dashboards zur Prüfung ein.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct MergeAntrag {
    /// Zweig, etwa `projekt/architektur-stufen`.
    pub zweig: String,
    /// Spitze des Zweigs (Commit-Hash, 7 bis 40 Zeichen).
    pub commit: String,
    /// Slug des Projekts, dessen Modul es ist.
    pub projekt: String,
    /// Was sich ändert; die Endpunkte als TOML-Beispiel.
    pub beschreibung: String,
    /// Ausgabe der eigenen Prüfschritte (cargo test, pruefe, …).
    pub pruefbericht: String,
}

/// Ein Eintrag im Journal (Tabelle `ereignisse`): wird nur angehängt, nie geändert.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct Ereignis {
    /// Fortlaufende Nummer, zugleich der Cursor.
    pub nr: u64,
    /// Ortszeit, RFC 3339.
    pub zeit: String,
    /// Kennung des Vorgangs.
    pub vorgang: String,
    pub art: VorgangsArt,
    pub slug: String,
    /// Zustand, in den der Vorgang mit diesem Ereignis kam.
    pub zustand: Zustand,
    pub titel: String,
    pub bezug: Option<String>,
    /// Wer den Schritt getan hat.
    pub von: String,
    pub notiz: Option<String>,
    /// Bei einem Schritt des Baus (Art `bau`): wie der Bau jetzt steht. Der
    /// Zustand des Vorgangs bleibt dabei, wie er war.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub bau: Option<BauZustand>,
}

/// `POST /api/vorgaenge`: der neue Vorgang und sein erstes Ereignis.
#[derive(Debug, Serialize, TS)]
pub struct VorgangAngelegt {
    pub vorgang: Vorgang,
    pub ereignis: Ereignis,
}

/// `POST /api/vorgaenge/<id>/uebernommen` und `/erledigt`: der Vorgang danach
/// und ob sich etwas geändert hat (`false`: war schon so weit oder weiter).
#[derive(Debug, Serialize, TS)]
pub struct VorgangGeaendert {
    pub vorgang: Vorgang,
    pub geaendert: bool,
}

/// `GET /api/vorgaenge?fuer=…` und `GET /api/vorgaenge/warten?fuer=…`.
/// `ereignisse` ist nur mit `seit=<nr>` gefüllt; dann stehen in `vorgaenge`
/// die betroffenen Vorgänge (jeder einmal). `cursor` ist die höchste
/// Ereignisnummer zum Zeitpunkt des Lesens.
#[derive(Debug, Clone, Serialize, TS)]
pub struct VorgangsListe {
    pub vorgaenge: Vec<Vorgang>,
    pub ereignisse: Vec<Ereignis>,
    pub cursor: u64,
}

/// `GET /api/vorgaenge/<id>`: ein Vorgang mit allen Ereignissen, nach Nummer.
#[derive(Debug, Serialize, TS)]
pub struct VorgangDetail {
    pub vorgang: Vorgang,
    pub ereignisse: Vec<Ereignis>,
    /// Nur beim Vorgang `antwort` zu einer Frage (Bezug `antwort:<slug>/<id>`,
    /// Etappe A2): die Antwort des Inhabers, auch aus dem Archiv.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub antwort: Option<Antwort>,
}

/// Zahlen oben auf der Seite /vorgaenge.
#[derive(Debug, Clone, Default, PartialEq, Serialize, TS)]
pub struct VorgangsZaehler {
    /// Vorgänge, die jetzt `offen` sind.
    pub offen: u64,
    /// Vorgänge, die jetzt `zugestellt` sind.
    pub zugestellt: u64,
    /// Schritte nach `uebernommen` bzw. `erledigt` heute (Ortszeit).
    pub uebernommen_heute: u64,
    pub erledigt_heute: u64,
}

/// Eine Zeile je Chat auf der Seite /vorgaenge.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct ChatZeile {
    pub slug: String,
    /// Seit wann der älteste Warter dieses Chats wartet (Ortszeit, RFC 3339); `null`: wartet nicht.
    pub wartet_seit: Option<String>,
    pub zuletzt_uebernommen: Option<String>,
    /// Vorgänge, die dieser Chat noch nicht übernommen hat.
    pub offen: u64,
}

/// `GET /api/vorgaenge/uebersicht`: alles für die Seite /vorgaenge.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(rename = "VorgangsUebersicht")]
pub struct Uebersicht {
    pub zaehler: VorgangsZaehler,
    /// Nach Slug sortiert.
    pub chats: Vec<ChatZeile>,
    /// Die 100 zuletzt geänderten Vorgänge, neueste zuerst.
    pub vorgaenge: Vec<Vorgang>,
    /// Offene Merge-Requests (Art `merge`, noch nicht erledigt), neueste zuerst (Etappe 4b).
    pub merge_offen: Vec<Vorgang>,
    pub cursor: u64,
}

// ----- Aufgaben für den Inhaber (Etappe 5a) -----

/// Zustand einer Aufgabe. Nur aus `offen` geht es weiter, und nur einmal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum AufgabenZustand {
    /// Wartet auf den Inhaber.
    Offen,
    /// Der Inhaber hat abgehakt (oder einen Knopf gedrückt).
    Erledigt,
    /// Der Ablauf ist vorbei; der Link ist gelöscht.
    Abgelaufen,
    /// Der Chat hat die Aufgabe zurückgezogen.
    Zurueckgezogen,
}

/// Eine Aufgabe für den Inhaber (Tabelle `aufgaben`). Der Link selbst steht nie
/// hier, sondern verschlüsselt in `aufgaben_links`; `hat_link` sagt nur, ob
/// es einen gab.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct Aufgabe {
    /// Kennung `a-JJJJMMTT-NNNN`.
    pub id: String,
    /// Slug des Projekts; dessen Chat wird beim Haken geweckt.
    pub projekt: String,
    /// Welcher Chat sie angelegt hat (eine Angabe, kein Nachweis).
    pub von: String,
    /// Ein Satz, was zu tun ist (höchstens 200 Zeichen).
    pub titel: String,
    /// Antworten mit einem Klick (etwa „ja“ und „nein“); `null`: nur der Haken „erledigt“.
    pub knopf: Option<Vec<String>>,
    /// Bis wann (JJJJ-MM-TT).
    pub frist: Option<String>,
    /// Ab wann der Link nicht mehr gilt (RFC 3339).
    pub ablauf: Option<String>,
    pub zustand: AufgabenZustand,
    /// Die gewählte Antwort aus `knopf`; `etwas-anderes`, wenn der Inhaber
    /// zuletzt „Etwas anderes …“ geschrieben hat (die Aufgabe bleibt dann offen).
    pub antwort: Option<String>,
    /// Die eigenen Worte des Inhabers bei „Etwas anderes …“ (die letzte
    /// Rückmeldung); sonst `null`. Ältere Einträge haben das Feld nicht.
    #[serde(default)]
    pub eigene: Option<String>,
    /// Ortszeit, RFC 3339.
    pub erstellt: String,
    pub geaendert: String,
    /// Ob die Aufgabe mit einem Link angelegt wurde (auch, wenn er schon gelöscht ist).
    pub hat_link: bool,
    /// Kennung des Vorgangs (Art `antwort`), der beim Haken entstand.
    pub vorgang: Option<String>,
    /// Vorschlag des Chats für die Stufe (Etappe A1). Ältere Einträge haben
    /// das Feld nicht (darum freiwillig, ohne neue Tabellen-Fassung); dann gilt `bald`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub prioritaet: Option<Stufe>,
    /// Verlauf der Aufgabe (Etappe B2, „Aufgabe ändern ohne neue Nummer“),
    /// ältester Eintrag zuerst: geänderter Titel, Knöpfe oder Frist. Fehlt,
    /// solange er leer ist (auch bei älteren Aufgaben); nichts fällt weg.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(as = "Option<Vec<AufgabeVerlauf>>", optional)]
    pub verlauf: Vec<AufgabeVerlauf>,
    /// Bewegliche Frist (Etappe B3): Ereignis aus `daten/fristen.toml` des
    /// Projekts und Abstand. Dann ist `frist` gespeichert leer; die API zeigt
    /// in `frist` das aufgelöste Datum, sobald das Ereignis eines hat. Fehlt bei
    /// fester Frist und bei älteren Aufgaben.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub frist_bezug: Option<super::fristen::FristBezug>,
}

/// Was ein Eintrag im Verlauf einer Aufgabe festhält (Etappe B2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum AufgabeVerlaufArt {
    /// Der Chat hat den Titel geändert.
    Titel,
    /// Der Chat hat die Knöpfe geändert (`alt`/`neu` als JSON-Liste, `null`: nur der Haken).
    Knopf,
    /// Der Chat hat die Frist geändert (`null`: ohne Frist).
    Frist,
    /// Eine Aufgabe „beim Chat“ (eigene Worte des Inhabers) steht nach dem
    /// Ändern von Titel oder Knöpfen wieder für den Inhaber offen.
    WiederOffen,
}

/// Ein Eintrag im Verlauf einer Aufgabe. Er erzeugt keine Meldung und weckt niemanden.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct AufgabeVerlauf {
    /// Ortszeit, RFC 3339.
    pub zeit: String,
    /// Wer geändert hat (`chat:<projekt>`).
    pub von: String,
    pub art: AufgabeVerlaufArt,
    /// Der Wert vorher (`null`: nicht gesetzt).
    pub alt: Option<String>,
    /// Der Wert danach (`null`: nicht gesetzt).
    pub neu: Option<String>,
    /// Ein Satz für Menschen, etwa `Titel geändert: „alt“ → „neu“`.
    pub text: String,
}

/// Eine Aufgabe, wie die API sie zeigt: dazu der Link im Klartext, aber nur,
/// solange sie offen und nicht abgelaufen ist; sonst `null`.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct AufgabeAnsicht {
    #[serde(flatten)]
    pub aufgabe: Aufgabe,
    pub link: Option<String>,
    /// Wie nah die Frist ist (Etappe 3a+, dieselbe Regel wie in Anstehend,
    /// am lokalen Datum gerechnet); `null` ohne lesbare Frist. Die Oberfläche
    /// lädt um Mitternacht neu.
    pub frist_stufe: Option<FristStufe>,
    /// Nur bei beweglicher Frist (Etappe B3): das Ereignis mit Titel und
    /// Datum; ohne Datum hängt die Frist an ihm (`frist` ist dann `null`).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub ereignis: Option<super::fristen::EreignisStand>,
}

/// `POST /api/aufgaben`: die Kennung der neuen Aufgabe.
#[derive(Debug, Serialize, TS)]
pub struct AufgabeAngelegt {
    pub id: String,
}

/// `GET /api/aufgaben`: offene nach Frist (ohne Frist zuletzt), dann nach Alter.
#[derive(Debug, Serialize, TS)]
pub struct AufgabenListe {
    pub aufgaben: Vec<AufgabeAnsicht>,
}

/// `GET /api/aufgaben/anzahl`: nur die Zahl der offenen Aufgaben (für den
/// Seitenkopf). Der Server zählt sie, ohne einen Link zu entschlüsseln.
#[derive(Debug, Serialize, TS)]
pub struct AufgabenAnzahl {
    pub offen: u64,
}

/// `POST /api/aufgaben/<id>/erledigt` und `/zurueckgezogen`: die Aufgabe
/// danach und ob sich etwas geändert hat (`false`: war schon nicht mehr offen).
#[derive(Debug, Serialize, TS)]
pub struct AufgabeGeaendert {
    pub aufgabe: AufgabeAnsicht,
    pub geaendert: bool,
}

// ----- Prüfung (Zielbild, Abschnitt d, „Prüfung je Projekt“) -----

/// Die Befunde eines Projekts (`GET /api/pruefung/<slug>` und je Zeile in
/// [`PruefungsBericht`]).
#[derive(Debug, Clone, Serialize, TS)]
pub struct PruefungProjekt {
    pub slug: String,
    /// Erst die Kopfdatei, dann `daten/`, dann `assets/`. Etappe B2: nur mit
    /// Windows-Anmeldung; ohne ist die Liste leer, es bleiben die Zahlen.
    pub befunde: Vec<Befund>,
    /// Etappe B2: Zahl der Fehler unter den Befunden, auch ohne Anmeldung.
    /// In TypeScript optional (ältere Beispiele).
    #[ts(as = "Option<u64>", optional)]
    pub fehler: u64,
    /// Etappe B2: Zahl der Hinweise, auch ohne Anmeldung.
    #[ts(as = "Option<u64>", optional)]
    pub hinweise: u64,
}

impl PruefungProjekt {
    /// Mit den Zahlen aus den Befunden.
    pub fn neu(slug: String, befunde: Vec<Befund>) -> Self {
        let (f, h) = crate::inhalt::pruefung::zaehlen(&befunde);
        Self { slug, befunde, fehler: f as u64, hinweise: h as u64 }
    }

    /// Zahlen neu aus den Befunden (nach dem Anhängen weiterer).
    pub fn zahlen_neu(&mut self) {
        let (f, h) = crate::inhalt::pruefung::zaehlen(&self.befunde);
        (self.fehler, self.hinweise) = (f as u64, h as u64);
    }

    /// Etappe B2: Ohne Windows-Anmeldung nur die Zahlen, keine Befund-Sätze.
    pub fn ohne_saetze(mut self, angemeldet: bool) -> Self {
        if !angemeldet {
            self.befunde.clear();
        }
        self
    }
}

/// Zahlen über alle Projekte.
#[derive(Debug, Clone, Default, PartialEq, Serialize, TS)]
pub struct PruefungGesamt {
    pub fehler: u64,
    pub hinweise: u64,
    /// Wie viele Projekte geprüft wurden (auch die ohne Befund).
    pub projekte: u64,
}

/// `GET /api/pruefung`: alle Projekte; in `projekte` nur die mit Befunden,
/// nach Slug sortiert.
#[derive(Debug, Clone, Serialize, TS)]
pub struct PruefungsBericht {
    pub gesamt: PruefungGesamt,
    pub projekte: Vec<PruefungProjekt>,
}

// ----- Betrieb (Etappe 7a) -----

/// `GET /api/betrieb`: wie der Server läuft. Lebenszeichen für Skripte
/// (neben `/healthz`) und später die Seite /betrieb.
#[derive(Debug, Clone, Serialize, TS)]
pub struct Betrieb {
    /// `dienst` (Windows-Dienst), `konsole` (`--konsole --einstellungen`),
    /// `umgebung` (ohne Argumente, Pfade aus Umgebungsvariablen).
    #[ts(type = "\"dienst\" | \"konsole\" | \"umgebung\"")]
    pub modus: String,
    /// Fassung aus Cargo.toml.
    pub version: String,
    /// Datum dieser Fassung aus dem eingebetteten Changelog (`JJJJ-MM-TT`);
    /// `null`, wenn das Changelog sie nicht nennt.
    pub version_datum: Option<String>,
    /// `true`, wenn Unreleased im eingebetteten Changelog Einträge hat: Es
    /// läuft ein Bau zwischen zwei Fassungen.
    pub unveroeffentlicht: bool,
    /// Commit, wenn beim Bauen `DASHBOARD_COMMIT` gesetzt war; sonst `null`.
    pub commit: Option<String>,
    /// Startzeit des Servers, Ortszeit, RFC 3339.
    pub gestartet: String,
    /// Die Einstellungsdatei (`dienst.toml`); `null` im Modus `umgebung`.
    /// Nur in `/api/betrieb/voll` (geschützt); auf dem freien Weg immer `null`
    /// (ein Pfad, Kopfleiste Fassung 2, Abnahme).
    pub einstellungen: Option<String>,
    /// Zustand des Boten: `verbunden` (letzte Anfrage beantwortet),
    /// `getrennt` (Bote vorgesehen, aber nicht erreichbar) oder
    /// `nicht vorgesehen` (ohne Einstellungsdatei, Altweg). Auf dem freien Weg
    /// statt `verbunden` und `getrennt` nur `vorgesehen`: Der Bote läuft nur in
    /// der Sitzung des Inhabers (Abnahme Kopfleiste 2); den Zustand nennt `/api/betrieb/voll`.
    #[ts(type = "\"verbunden\" | \"getrennt\" | \"vorgesehen\" | \"nicht vorgesehen\"")]
    pub bote: String,
    /// Bei `verbunden`: seit wann; bei `getrennt`: wann er zuletzt antwortete
    /// (RFC 3339, `null` wenn nie). Sonst `null`. Nur in `/api/betrieb/voll`
    /// (der Bote läuft in der Sitzung des Inhabers; seine Zeiten sagen, wann sie sich
    /// an- oder abgemeldet hat); auf dem freien Weg immer `null`.
    pub bote_seit: Option<String>,
    /// Der Datenspeicher. Der Server öffnet ihn in jedem Modus beim Start
    /// (auch im Altweg ohne Einstellungsdatei); `offen: false` heißt also
    /// immer, dass etwas nicht stimmt.
    pub speicher: SpeicherStand,
    /// Die Sitzungsabfrage für die Meldung „Bote fehlt“ (Etappe A3). Nur in
    /// `/api/betrieb/voll` (geschützt: ob der Inhaber an der Konsole angemeldet
    /// ist); auf dem freien Weg `null`.
    pub sitzung: Option<SitzungsStand>,
    /// Der Inhalt im Arbeitsspeicher (Etappe 2d).
    pub inhalt: InhaltStand,
    /// Die Fassung der Oberfläche, aus der gerade ausgeliefert wird: der
    /// Ordner unter `web/builds` (nur der Name) oder `build` für den Rückfall
    /// `web/build` (Kopfleiste, Fassung 2: Abschnitt „Dashboard“ auf /pc).
    pub fassung: String,
    /// Wann diese Fassung gebaut wurde (Änderungszeit ihrer `index.html`,
    /// RFC 3339); `null`, wenn sie sich nicht lesen ließ.
    pub fassung_gebaut: Option<String>,
    /// Ob sich die Ordner lesen lassen, die der Server liest (dieselbe
    /// Prüfung wie beim Start, Hotfix 4d), nur mit kurzem Namen, nie mit Pfad.
    pub ordner: Vec<OrdnerStand>,
    /// Woher der Server den Inhalt liest (Umzug des Inhalts). Nur in
    /// `/api/betrieb/voll` (Pfade); auf dem freien Weg immer `null`.
    pub quellen: Option<QuellenStand>,
    /// Kleinkram 2: Störungen im Betrieb, je ein Satz (leer: keine). Bisher:
    /// Der Bote (der Bau-Prüfer) prüft mit einer anderen Fassung des Prüfers
    /// als der Dienst, oder ein älterer Bote nennt keine. Erst nach dem ersten
    /// Hallo des Boten; ohne Boten nie.
    pub stoerungen: Vec<String>,
}

/// Woher der Server den privaten Inhalt liest (Teil von [`Betrieb`], nur in
/// `/api/betrieb/voll`). Der zweite Durchgang von Inhalt-Umziehen.ps1 prüft
/// damit, dass der Dienst schon aus `dashboard-inhalt` liest. Pfade im
/// Benutzerordner wie überall als `%USERPROFILE%\…`.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct QuellenStand {
    /// Steht in der Einstellungsdatei ein Abschnitt `[inhalt]` mit
    /// `projekte`? `false` auch ohne Einstellungsdatei (Modus `umgebung`).
    pub inhalt_abschnitt: bool,
    /// Die Inhaltsquellen, aus denen gerade gelesen wird, in ihrer Reihenfolge.
    pub projekte: Vec<String>,
    /// Der Daten-Ordner (`pc.toml`, `schluessel.toml`, …).
    pub daten: String,
    /// Der Ordner der Entscheidungen.
    pub entscheidungen: String,
}

/// `GET /api/hallo`: welches Projekt den Begrüßungstext des Reiters „Hallo“
/// trägt. Das Code-Repo nennt kein Projekt: Es ist das erste Projekt (nach
/// Slug) im Inhalt mit der Handeingabe `daten/<daten>.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct HalloOrt {
    /// Der Slug, oder `null`, wenn kein Projekt die Datei hat.
    pub projekt: Option<String>,
    /// Name der Datei ohne `.toml` (immer `eigen-hallo`).
    pub daten: String,
}

/// Lesbarkeit eines Ordners (Teil von [`Betrieb`]).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct OrdnerStand {
    /// Kurzer Name, etwa `projects` oder `docs\entscheidungen`.
    pub name: String,
    /// `lesbar`, `fehlt` (die Seiten zeigen dann ihren Platzhalter) oder
    /// `nicht lesbar` (kein Recht oder anderer Lesefehler).
    #[ts(type = "\"lesbar\" | \"fehlt\" | \"nicht lesbar\"")]
    pub stand: String,
}

/// `GET /api/stand` und `/api/stand/voll` (Kopfleiste, Fassung 3): der
/// gemeinsame Stand für die Aktualitätsprüfung im letzten Brotkrumen-Glied.
/// Nur Zahlen und der Name der Fassung (wie in `/api/betrieb`).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct ServerStand {
    /// Ereignisnummer: wie viele Ereignisse mit Daten der Server seit dem Start
    /// geschickt hat. Frei nur die, die auch der freie Kanal trägt; geschützt
    /// alle (siehe `api::stand`). Ohne Neustart nie kleiner.
    pub ereignis: u64,
    /// Version des Inhalts (wie `inhalt.version` in `/api/betrieb`).
    pub inhalt: u64,
    /// Die Fassung der Oberfläche (Ordnername oder `build`).
    pub fassung: String,
}

/// Wie es um den ContentStore steht (Teil von [`Betrieb`], Etappe 2d): nur
/// Zahlen, keine Namen und Inhalte.
#[derive(Debug, Clone, Default, PartialEq, Serialize, TS)]
pub struct InhaltStand {
    /// Version des Inhalts; steigt mit jeder Änderung.
    pub version: u64,
    /// Projekte im Speicher (auch kaputte).
    pub projekte: u64,
    /// So lange dauerte das erste volle Lesen beim Start, in ms.
    pub geladen_in_ms: u64,
    /// Dateien, deren letzter guter Stand gerade gilt, weil sie sich nicht lesen ließen.
    pub lesefehler: u64,
}

/// Wie es um die Sitzungsabfrage steht (Teil von [`Betrieb`], Etappe A3,
/// Hotfix Sitzungsabfrage): ob der Inhaber angemeldet ist und woher der Dienst das
/// weiß. Davon hängt die Meldung „Bote fehlt“ ab; scheitert die Abfrage, gäbe
/// es sie sonst still nie. Der Dienst fragt Windows nach der Sitzung, die der
/// Bote zuletzt gemeldet hat (oder der Konsolensitzung); geht das nicht,
/// gilt der Bericht des Boten nur so weit, dass eine Abmeldung nie wie sein
/// Absturz aussieht (siehe `quelle`). Ohne SID und ohne Namen. Gefragt wird jede Minute, wenn ein Bote vorgesehen ist; sonst
/// bleibt alles `null`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, TS)]
pub struct SitzungsStand {
    /// Ob die letzte Abfrage bei Windows eine Antwort gab; `null`, solange
    /// nicht gefragt wurde.
    pub geht: Option<bool>,
    /// Ob der Inhaber angemeldet ist (aus der Quelle `quelle`); `null`, wenn es
    /// keine Quelle gibt.
    pub inhaber_angemeldet: Option<bool>,
    /// Woher `inhaber_angemeldet` kommt: `windows` (die Abfrage ging), `bote`
    /// (sie scheiterte; es gilt der Bericht des Boten über seine Sitzung:
    /// verbunden, oder seine Sitzung ist noch die an der Konsole, oder ohne
    /// Konsolenbezug höchstens 5 Minuten nach dem letzten Hallo),
    /// `unbekannt` (sonst; dann keine Meldung „Bote fehlt“, damit eine
    /// Abmeldung nie wie ein Absturz des Boten aussieht); `null`, solange
    /// nicht gefragt wurde.
    #[ts(type = "\"windows\" | \"bote\" | \"unbekannt\" | null")]
    pub quelle: Option<String>,
    /// Art des Fehlers der letzten Abfrage, `null`, wenn sie ging:
    /// `sid-fehlt` (`konten.inhaber_sid` leer), `wts` (ein WTS-Aufruf für
    /// eine einzelne Sitzung scheiterte: Zustand oder Konto, oder das Konto
    /// ließ sich nicht auflösen), `abgebrochen`, `nur-windows`, oder
    /// `sieht-inhaber-nicht`: Die Abfrage ging, sah den Inhaber aber nicht, obwohl
    /// der Bote (der in ihrer Sitzung läuft) verbunden ist.
    #[ts(type = "\"sid-fehlt\" | \"wts\" | \"abgebrochen\" | \"nur-windows\" | \"sieht-inhaber-nicht\" | null")]
    pub fehler: Option<String>,
    /// Wann zuletzt gefragt wurde, Ortszeit, RFC 3339.
    pub geprueft: Option<String>,
}

/// Wie es um den Datenspeicher steht (Teil von [`Betrieb`]). Dienst.ps1
/// verlangt nach jedem Start `offen: true` und `fehler: null`.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct SpeicherStand {
    /// Ob der Server den Datenspeicher geöffnet hat.
    pub offen: bool,
    /// Schema der Datei: bei offenem Speicher das gelesene, bei einer zu
    /// neuen Datei das Schema der Datei; sonst `null`.
    pub schema: Option<u32>,
    /// Welches Schema dieses Programm schreibt und versteht.
    pub programm_schema: u32,
    /// Ob die Datei von einer neueren Fassung stammt (Schema über
    /// `programm_schema`); dann läuft der Server ohne Datenspeicher.
    pub zu_neu: bool,
    /// Satz für den Inhaber, wenn der Speicher fehlt oder nicht lesbar ist; sonst `null`.
    pub fehler: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Etappe 3a-4: vollständig aus den Zahlen; unbekannt immer mit Grund und ohne Zahlen.
    #[test]
    fn schluessel_vollstaendig() {
        assert_eq!(SchluesselStand::aus_zahlen(2, 0).vollstaendig, Vollstaendig::Ja);
        assert_eq!(SchluesselStand::aus_zahlen(0, 0).vollstaendig, Vollstaendig::Ja);
        assert_eq!(SchluesselStand::aus_zahlen(2, 1).vollstaendig, Vollstaendig::Nein);
        let u = SchluesselStand::unbekannt(SchluesselGrund::BoteFehlt);
        assert_eq!((u.vollstaendig, u.gespeichert, u.fehlen), (Vollstaendig::Unbekannt, 0, 0));
        let json = serde_json::to_value(&u).unwrap();
        assert_eq!(json, serde_json::json!({"vollstaendig": "unbekannt", "gespeichert": 0, "fehlen": 0, "grund": "bote-fehlt"}));
        assert_eq!(serde_json::to_value(SchluesselGrund::FremdesKonto).unwrap(), "fremdes-konto");
        assert_eq!(serde_json::to_value(SchluesselStand::aus_zahlen(1, 0)).unwrap()["grund"], serde_json::Value::Null);
    }
}
