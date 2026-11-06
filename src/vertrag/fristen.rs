//! Modul Fristen (Etappe B3, Entscheidung gemeinsame-bausteine,
//! Wunsch): feste und bewegliche Fristen. Hier stehen die
//! Datenformen und das Rechnen mit dem Abstand; was zusammengeführt wird
//! (Aufgaben, Fragen, Arbeiten, `daten/fristen.toml`), rechnet `crate::fristen`.
//!
//! - Ein **Ereignis** ([`FristEreignis`]) hat eine Kennung, einen Titel und ein
//!   Datum, sobald es eingetreten ist. Es steht in `daten/fristen.toml`; das
//!   Datum setzt der Chat dort und committet (der Server schreibt nie in den Inhalt).
//! - Eine **Frist** ist fest (ein Datum) oder beweglich ([`FristBezug`]: die
//!   Kennung eines Ereignisses und ein Abstand wie `+14 Tage` oder `+1 Monat`).
//!   Sobald das Ereignis ein Datum hat, rechnet der Server daraus das Datum der
//!   Frist ([`aufloesen`]); vorher „hängt sie an“ dem Ereignis.
//!
//! Fristen stehen an Aufgaben (im Datenspeicher, per API angelegt, fest oder
//! beweglich) und in `daten/fristen.toml` ([`Frist`], für Fristen, die
//! laufen, ohne dass der Inhaber gerade etwas tun muss).
//!
//! Abstand: Vorzeichen, Zahl, Leerzeichen, Einheit; Einheiten Tag(e),
//! Woche(n), Monat(e), Jahr(e). Monate und Jahre enden sauber am Monatsende
//! (31.01. + 1 Monat = 28.02., im Schaltjahr 29.02.). Werktage gibt es nicht:
//! Sie hingen an Feiertagen je Bundesland; gerechnet wird in Kalendertagen,
//! eine Verschiebung auf den nächsten Werktag macht der Server nicht (das
//! errechnete Datum liegt so nie nach dem echten Fristende).
//!
//! Rückwärts (Abstand mit Minus, Auflage der Architektur zu B3): Gemeint ist
//! „mindestens N volle Tage (Wochen, Monate, Jahre) vor dem Ereignis“. Zwischen
//! dem Ende des Fristtags und dem Ereignis müssen also N volle Einheiten
//! liegen; darum endet die Frist einen Tag früher als das bloße Abziehen
//! (Ereignis 20.11., `-3 Tage`: 16.11.; `-1 Monat`: 19.10.). So liegt die
//! Anzeige auch rückwärts nie nach dem echten Fristende.

use std::collections::BTreeSet;

use chrono::{Months, NaiveDate};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::anstehend::FristStufe;
use super::daten::{Baustein, Datei, ist_datum, ist_kennung};
use super::lesen::{LeseFehler, Mangel};

// ----- Die Datei `daten/fristen.toml` -----

/// `daten/fristen.toml`: Ereignisse und Fristen eines Projekts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Fristen {
    #[serde(default)]
    pub ereignis: Vec<FristEreignis>,
    #[serde(default)]
    pub frist: Vec<Frist>,
}

/// Ein benanntes Ereignis, an dem bewegliche Fristen hängen können.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FristEreignis {
    /// Bleibt gleich; Kleinbuchstaben, Ziffern, Bindestriche; in der ganzen
    /// Datei nur einmal (auch nicht als Kennung einer Frist).
    pub kennung: String,
    pub titel: String,
    /// JJJJ-MM-TT, sobald das Ereignis eingetreten ist; vorher fehlt es.
    #[serde(default)]
    pub datum: Option<String>,
    #[serde(default)]
    pub hinweis: Option<String>,
}

/// Eine Frist in `daten/fristen.toml`: entweder `datum` (fest) oder
/// `ereignis` mit `abstand` (beweglich).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Frist {
    /// Bleibt gleich; in der ganzen Datei nur einmal.
    pub kennung: String,
    pub titel: String,
    /// Feste Frist: JJJJ-MM-TT.
    #[serde(default)]
    pub datum: Option<String>,
    /// Bewegliche Frist: die Kennung eines Ereignisses dieser Datei.
    #[serde(default)]
    pub ereignis: Option<String>,
    /// Nur mit `ereignis`: der Abstand, etwa `+14 Tage`, `+1 Monat`, `-3 Tage`.
    #[serde(default)]
    pub abstand: Option<String>,
    /// JJJJ-MM-TT, wenn die Frist gewahrt oder erledigt ist; dann zählt sie
    /// nirgends mehr (nicht in der nächsten Frist, nicht in der Ampel).
    #[serde(default)]
    pub erledigt: Option<String>,
    #[serde(default)]
    pub hinweis: Option<String>,
}

// ----- Bewegliche Fristen -----

/// Eine bewegliche Frist: Kennung des Ereignisses (in `daten/fristen.toml`
/// des Projekts) und Abstand. Steht so an Aufgaben (`frist_bezug`) und kommt
/// so über die API (`POST /api/aufgaben`, `POST /api/aufgaben/<id>`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FristBezug {
    pub ereignis: String,
    /// Etwa `+14 Tage`, `+1 Monat`, `-3 Tage` (siehe [`Abstand`]).
    pub abstand: String,
}

/// Wie es um das Ereignis einer beweglichen Frist steht (gerechnet, nie gespeichert).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct EreignisStand {
    pub kennung: String,
    /// Titel aus `daten/fristen.toml`; `null`: Das Ereignis gibt es dort nicht (mehr).
    pub titel: Option<String>,
    /// JJJJ-MM-TT, sobald das Ereignis eingetreten ist; vorher `null`: Die
    /// Frist hängt an ihm.
    pub datum: Option<String>,
}

/// Einheit eines Abstands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Einheit {
    Tag,
    Woche,
    Monat,
    Jahr,
}

/// Ein gelesener Abstand: Zahl mit Vorzeichen und Einheit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Abstand {
    pub zahl: i32,
    pub einheit: Einheit,
}

/// So sieht ein Abstand aus (für Fehlersätze).
pub const ABSTAND_MUSTER: &str = "Vorzeichen, Zahl, Leerzeichen, Einheit, etwa `+14 Tage`, `+2 Wochen`, `+1 Monat`, `+1 Jahr` oder `-3 Tage`";

/// Höchstens so groß darf die Zahl eines Abstands sein (gegen Tippfehler wie `+140 Monate`… und Überläufe).
pub const ABSTAND_HOECHSTENS: i32 = 3660;

impl Abstand {
    /// Liest `+14 Tage`, `-3 Tage`, `+1 Monat`, `+2 Monate`, `+1 Woche`,
    /// `+1 Jahr`. Das Vorzeichen ist Pflicht (sagt, in welche Richtung), die
    /// Einheit darf Einzahl oder Mehrzahl sein. Sonst `None`.
    pub fn lesen(text: &str) -> Option<Abstand> {
        let t = text.trim();
        let (vorzeichen, rest) = match t.chars().next()? {
            '+' => (1, &t[1..]),
            '-' => (-1, &t[1..]),
            _ => return None,
        };
        let (zahl, einheit) = rest.trim_start().split_once(' ')?;
        if zahl.is_empty() || zahl.len() > 4 || !zahl.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        let zahl: i32 = zahl.parse().ok()?;
        if zahl > ABSTAND_HOECHSTENS {
            return None;
        }
        let einheit = match einheit.trim().to_lowercase().as_str() {
            "tag" | "tage" => Einheit::Tag,
            "woche" | "wochen" => Einheit::Woche,
            "monat" | "monate" => Einheit::Monat,
            "jahr" | "jahre" => Einheit::Jahr,
            _ => return None,
        };
        Some(Abstand { zahl: vorzeichen * zahl, einheit })
    }

    /// Das Datum `von` plus diesen Abstand. Monate und Jahre enden am
    /// Monatsende, wenn es den Tag dort nicht gibt (31.01. + 1 Monat =
    /// 28.02. oder 29.02.). Rückwärts (negative Zahl) einen Tag früher, für
    /// alle Einheiten gleich: „mindestens N volle Einheiten vorher“ (siehe oben;
    /// 20.11. `-3 Tage` = 16.11., `-1 Monat` = 19.10.). `None` nur außerhalb des Kalenders.
    pub fn von(self, von: NaiveDate) -> Option<NaiveDate> {
        let ziel = self.ohne_vortag(von)?;
        if self.zahl < 0 { ziel.pred_opt() } else { Some(ziel) }
    }

    /// Nur abziehen oder addieren, ohne den Tag früher bei Rückwärtsfristen.
    fn ohne_vortag(self, von: NaiveDate) -> Option<NaiveDate> {
        let tage = |n: i64| von.checked_add_signed(chrono::Duration::days(n));
        let monate = |n: i32| {
            let m = Months::new(n.unsigned_abs());
            if n >= 0 { von.checked_add_months(m) } else { von.checked_sub_months(m) }
        };
        match self.einheit {
            Einheit::Tag => tage(i64::from(self.zahl)),
            Einheit::Woche => tage(7 * i64::from(self.zahl)),
            Einheit::Monat => monate(self.zahl),
            Einheit::Jahr => monate(12 * self.zahl),
        }
    }
}

/// Das Datum einer beweglichen Frist: Ereignisdatum (JJJJ-MM-TT) plus Abstand.
/// `None`, solange das Ereignis kein Datum hat oder ein Teil nicht lesbar ist.
pub fn aufloesen(ereignis_datum: Option<&str>, abstand: &str) -> Option<NaiveDate> {
    let d = NaiveDate::parse_from_str(ereignis_datum?, "%Y-%m-%d").ok()?;
    Abstand::lesen(abstand)?.von(d)
}

// ----- Die Fristenliste -----

/// Woher eine Frist der Fristenliste kommt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum FristQuelle {
    /// `daten/fristen.toml`.
    Datei,
    Aufgabe,
    /// Nur die sichtbare Frage des Projekts (Warteschlange, Etappe B2).
    Frage,
    Arbeit,
}

/// Eine Zeile der Fristenliste.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct FristZeile {
    pub quelle: FristQuelle,
    /// Kennung in `fristen.toml` oder des Eintrags (`a-…`, `ar-…`, Frage-`id`).
    pub id: String,
    pub projekt: String,
    /// Titel des Projekts; `null`, wenn es das Projekt nicht (mehr) gibt.
    pub projekt_titel: Option<String>,
    pub titel: String,
    /// JJJJ-MM-TT: fest oder aufgelöst; `null`: Die Frist hängt an einem Ereignis ohne Datum.
    pub frist: Option<String>,
    /// Tage von heute bis zur Frist (0 heute, negativ: vorbei).
    pub tage_bis: Option<i64>,
    pub frist_stufe: Option<FristStufe>,
    /// Nur bewegliche Fristen: Ereignis und Abstand.
    pub frist_bezug: Option<FristBezug>,
    /// Nur bewegliche Fristen: das Ereignis mit Titel und Datum.
    pub ereignis: Option<EreignisStand>,
    pub hinweis: Option<String>,
    /// Im Dashboard: `/projects/<slug>#frist-<kennung>` (Datei) oder das Ziel des Eintrags.
    pub ziel: String,
}

/// Die Fristen eines Projekts (`abgeleitet.fristen` von `daten/fristen.toml`)
/// oder aller sichtbaren Projekte (`GET /api/fristen`): offene Fristen nach
/// Datum (vorbei zuerst), bewegliche ohne Datum am Ende. Nur mit
/// Windows-Anmeldung (Titel sind persönlicher Inhalt).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct FristenListe {
    /// Das lokale Datum, mit dem gerechnet wurde (JJJJ-MM-TT).
    pub heute: String,
    /// Bis zu so vielen Tagen ist eine Frist `bald`.
    pub frist_bald_tage: i64,
    pub zeilen: Vec<FristZeile>,
    /// Sätze zu Dateien, die sich nicht lesen ließen.
    pub hinweise: Vec<String>,
    /// Der Datenspeicher fehlt: Aufgaben und Arbeiten fehlen in der Liste.
    pub speicher_fehlt: bool,
}

// ----- Prüfen beim Lesen (Fehler; der Hinweis zu einem Datum in der Zukunft steht in `inhalt::pruefung`) -----

impl Baustein for Fristen {
    const DATEI: &'static str = "fristen";

    /// Je Teil: Form von Kennung, Titel, Daten und Abstand; eine Frist hat
    /// entweder `datum` oder `ereignis` mit `abstand`. Doppelte Kennungen und
    /// unbekannte Ereignisse prüft [`fristen_ueber_die_datei`] (auch über Teile).
    fn pruefen(&self) -> Result<(), Mangel> {
        for e in &self.ereignis {
            kennung_und_titel("Das Ereignis", &e.kennung, &e.titel)?;
            if let Some(d) = e.datum.as_deref().filter(|d| !ist_datum(d, false)) {
                return Err(Mangel::bei(
                    "datum",
                    d,
                    format!(
                        "Beim Ereignis „{}“ steht `datum = \"{d}\"`. Erlaubt ist JJJJ-MM-TT, etwa `datum = \"2026-11-04\"`; solange es nicht eingetreten ist, `datum` weglassen.",
                        e.kennung
                    ),
                ));
            }
        }
        for f in &self.frist {
            kennung_und_titel("Die Frist", &f.kennung, &f.titel)?;
            for (feld, wert) in [("datum", &f.datum), ("erledigt", &f.erledigt)] {
                if let Some(d) = wert.as_deref().filter(|d| !ist_datum(d, false)) {
                    return Err(Mangel::bei(
                        feld,
                        d,
                        format!("Bei der Frist „{}“ steht `{feld} = \"{d}\"`. Erlaubt ist JJJJ-MM-TT, etwa `{feld} = \"2026-11-20\"`.", f.kennung),
                    ));
                }
            }
            let k = &f.kennung;
            match (&f.datum, &f.ereignis, &f.abstand) {
                (Some(_), Some(_), _) => {
                    return Err(Mangel::bei(
                        "kennung",
                        k,
                        format!(
                            "Die Frist „{k}“ hat `datum` und `ereignis`. Eine Frist ist entweder fest (`datum`) oder beweglich (`ereignis` mit `abstand`)."
                        ),
                    ));
                }
                (Some(_), None, Some(_)) => {
                    return Err(Mangel::bei(
                        "kennung",
                        k,
                        format!("Die Frist „{k}“ hat `abstand`, aber kein `ereignis`. Den Abstand gibt es nur bei einer beweglichen Frist."),
                    ));
                }
                (None, None, _) => {
                    return Err(Mangel::bei(
                        "kennung",
                        k,
                        format!(
                            "Die Frist „{k}“ hat weder `datum` noch `ereignis`. Fest: `datum = \"2026-11-20\"`; beweglich: `ereignis = \"<kennung>\"` und `abstand = \"+14 Tage\"`."
                        ),
                    ));
                }
                (None, Some(_), None) => {
                    return Err(Mangel::bei("kennung", k, format!("Bei der Frist „{k}“ fehlt `abstand` zum Ereignis, etwa `abstand = \"+14 Tage\"`.")));
                }
                (None, Some(_), Some(a)) if Abstand::lesen(a).is_none() => {
                    return Err(Mangel::bei(
                        "abstand",
                        a,
                        format!("Bei der Frist „{k}“ steht `abstand = \"{a}\"`. Erlaubt ist: {ABSTAND_MUSTER}. Werktage gibt es nicht."),
                    ));
                }
                _ => {}
            }
        }
        Ok(())
    }
}

fn kennung_und_titel(was: &str, kennung: &str, titel: &str) -> Result<(), Mangel> {
    if !ist_kennung(kennung) {
        return Err(Mangel::bei(
            "kennung",
            kennung,
            format!("{was} „{titel}“ hat keine einfache `kennung`. Erlaubt sind Kleinbuchstaben, Ziffern und Bindestriche, etwa `kennung = \"zusage-da\"`."),
        ));
    }
    if titel.trim().is_empty() {
        return Err(Mangel::bei("kennung", kennung, format!("{was} „{kennung}“ hat keinen `titel`.")));
    }
    Ok(())
}

/// Über die ganze Datei (alle Teile): jede Kennung nur einmal (Ereignisse und
/// Fristen zusammen; die Zeile zeigt auf das zweite Vorkommen), und jede
/// bewegliche Frist nennt ein Ereignis, das es in der Datei gibt.
pub fn fristen_ueber_die_datei(d: &Datei<Fristen>, datei: &str, text: &str) -> Result<(), LeseFehler> {
    let quelle = text.strip_prefix('\u{feff}').unwrap_or(text);
    let teile = d.inhalte();
    let mut gesehen = BTreeSet::new();
    let kennungen = teile.iter().flat_map(|t| t.ereignis.iter().map(|e| e.kennung.as_str()).chain(t.frist.iter().map(|f| f.kennung.as_str())));
    for k in kennungen {
        if !gesehen.insert(k) {
            return Err(Mangel::beim_zweiten(
                "kennung",
                k,
                format!("Die Kennung „{k}“ gibt es zweimal. Jede `kennung` nur einmal, Ereignisse und Fristen zusammen, auch über Teile hinweg."),
            )
            .in_datei(datei, quelle));
        }
    }
    let ereignisse: BTreeSet<&str> = teile.iter().flat_map(|t| &t.ereignis).map(|e| e.kennung.as_str()).collect();
    for f in teile.iter().flat_map(|t| &t.frist) {
        if let Some(e) = f.ereignis.as_deref().filter(|e| !ereignisse.contains(e)) {
            let bekannt = if ereignisse.is_empty() {
                "In der Datei steht noch kein `[[ereignis]]`.".to_string()
            } else {
                format!("Vorhanden: {}.", ereignisse.iter().copied().collect::<Vec<_>>().join(", "))
            };
            return Err(Mangel::bei("ereignis", e, format!("Die Frist „{}“ hängt an „{e}“, aber dieses Ereignis gibt es nicht. {bekannt}", f.kennung))
                .in_datei(datei, quelle));
        }
    }
    Ok(())
}

impl Datei<Fristen> {
    /// Das Ereignis mit dieser Kennung (in allen Teilen).
    pub fn ereignis(&self, kennung: &str) -> Option<&FristEreignis> {
        self.inhalte().into_iter().flat_map(|t| &t.ereignis).find(|e| e.kennung == kennung)
    }

    /// Wie es um ein Ereignis steht (Titel und Datum, sonst `null`).
    pub fn ereignis_stand(&self, kennung: &str) -> EreignisStand {
        let e = self.ereignis(kennung);
        EreignisStand {
            kennung: kennung.to_string(),
            titel: e.map(|e| e.titel.clone()),
            datum: e.and_then(|e| e.datum.clone()).filter(|d| ist_datum(d, false)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tag(t: &str) -> NaiveDate {
        NaiveDate::parse_from_str(t, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn abstand_lesen() {
        let a = |z, e| Some(Abstand { zahl: z, einheit: e });
        assert_eq!(Abstand::lesen("+14 Tage"), a(14, Einheit::Tag));
        assert_eq!(Abstand::lesen("+1 Tag"), a(1, Einheit::Tag));
        assert_eq!(Abstand::lesen("-3 Tage"), a(-3, Einheit::Tag));
        assert_eq!(Abstand::lesen(" +2 Wochen "), a(2, Einheit::Woche));
        assert_eq!(Abstand::lesen("+1 Monat"), a(1, Einheit::Monat));
        assert_eq!(Abstand::lesen("+3 monate"), a(3, Einheit::Monat));
        assert_eq!(Abstand::lesen("+1 Jahr"), a(1, Einheit::Jahr));
        assert_eq!(Abstand::lesen("+0 Tage"), a(0, Einheit::Tag));
        for falsch in ["14 Tage", "+14", "+14Tage", "+vierzehn Tage", "+14 Werktage", "+1,5 Monate", "", "+", "+12345 Tage", "+3661 Tage", "+-3 Tage"] {
            assert_eq!(Abstand::lesen(falsch), None, "{falsch}");
        }
    }

    /// Monatsende sauber: 31.01. + 1 Monat ist der letzte Februartag.
    #[test]
    fn aufloesen_mit_monatsende() {
        let f = |e: &str, a: &str| aufloesen(Some(e), a).map(|d| d.format("%Y-%m-%d").to_string());
        assert_eq!(f("2026-01-31", "+1 Monat").as_deref(), Some("2026-02-28"));
        assert_eq!(f("2028-01-31", "+1 Monat").as_deref(), Some("2028-02-29"), "Schaltjahr");
        assert_eq!(f("2026-03-31", "+1 Monat").as_deref(), Some("2026-04-30"));
        assert_eq!(f("2026-03-31", "-1 Monat").as_deref(), Some("2026-02-27"), "28.02. (Monatsende), dann ein Tag früher");
        assert_eq!(f("2028-02-29", "+1 Jahr").as_deref(), Some("2029-02-28"));
        assert_eq!(f("2026-11-04", "+14 Tage").as_deref(), Some("2026-11-18"));
        assert_eq!(f("2026-11-04", "+2 Wochen").as_deref(), Some("2026-11-18"));
        assert_eq!(f("2026-12-20", "+14 Tage").as_deref(), Some("2027-01-03"), "über den Jahreswechsel");
        // Rückwärts: mindestens N volle Einheiten vorher, also einen Tag früher (Auflage B3).
        assert_eq!(f("2026-11-20", "-3 Tage").as_deref(), Some("2026-11-16"));
        assert_eq!(f("2026-11-20", "-1 Woche").as_deref(), Some("2026-11-12"));
        assert_eq!(f("2026-11-20", "-1 Monat").as_deref(), Some("2026-10-19"));
        assert_eq!(f("2026-11-20", "-1 Jahr").as_deref(), Some("2025-11-19"));
        assert_eq!(f("2026-03-01", "-1 Tag").as_deref(), Some("2026-02-27"), "über den Monatswechsel");
        assert_eq!(f("2026-11-04", "+0 Tage").as_deref(), Some("2026-11-04"));
        assert_eq!(aufloesen(None, "+14 Tage"), None, "ohne Datum hängt die Frist");
        assert_eq!(aufloesen(Some("2026-11"), "+14 Tage"), None);
        assert_eq!(aufloesen(Some("2026-11-04"), "14 Tage"), None);
        assert_eq!(Abstand { zahl: 1, einheit: Einheit::Monat }.von(tag("2026-08-31")), Some(tag("2026-09-30")));
    }
}
