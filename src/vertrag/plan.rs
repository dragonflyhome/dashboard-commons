//! `daten/plan.toml` (Etappe B8): die Etappen eines Projekts, von Hand und
//! nur als Absicht (was entstehen soll, in welcher Reihenfolge, warum). Stände,
//! Zählungen und Zeitpunkte rechnet der Server aus dem Tracker; hier stehen
//! keine. Eine Arbeit zeigt mit `etappe = "<projekt>/<kennung>"` auf eine
//! Etappe; der Server prüft beim Schreiben, dass es sie hier gibt.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::daten::{Baustein, Datei, ist_datum, ist_kennung};
use super::lesen::{LeseFehler, Mangel};

/// `daten/plan.toml`: die Etappen eines Projekts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    #[serde(default)]
    pub etappe: Vec<Etappe>,
}

/// Eine Etappe: Absicht und Zuordnung, kein Stand.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Etappe {
    /// Bleibt gleich; Kleinbuchstaben, Ziffern, Bindestriche; in der Datei nur einmal.
    pub kennung: String,
    pub titel: String,
    /// Ein Satz: was die Etappe erreichen soll.
    pub ziel: String,
    /// Ab 1; in der Datei nur einmal.
    pub reihenfolge: u32,
    /// JJJJ-MM: der Monat eines echten Termins (kein geschätztes Datum).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub ab: Option<String>,
    /// Kennungen von Arbeiten (`ar-JJJJMMTT-NNNN`), die schon im Archiv liegen
    /// oder vor dem Feld `etappe` entstanden sind.
    #[serde(default)]
    pub arbeiten: Vec<String>,
    /// Nur für Etappen ohne jede Arbeit im Tracker: vor dem Tracker abgeschlossen.
    #[serde(default)]
    pub abgeschlossen: bool,
}

/// `ar-JJJJMMTT-NNNN`, wie der Server Arbeiten benennt.
pub fn ist_arbeitskennung(s: &str) -> bool {
    let b = s.as_bytes();
    s.len() == 16 && s.starts_with("ar-") && b[11] == b'-' && b[3..11].iter().chain(&b[12..]).all(u8::is_ascii_digit)
}

impl Baustein for Plan {
    const DATEI: &'static str = "plan";

    /// Je Etappe: Pflichtfelder nicht leer, Form von Kennung, `ab` und
    /// `arbeiten`, `reihenfolge` ab 1, `abgeschlossen` nur ohne `arbeiten`.
    /// Doppelte Kennungen und Reihenfolgen prüft [`plan_ueber_die_datei`].
    fn pruefen(&self) -> Result<(), Mangel> {
        for e in &self.etappe {
            let k = &e.kennung;
            if !ist_kennung(k) {
                return Err(Mangel::bei(
                    "kennung",
                    k,
                    format!(
                        "Die Etappe „{}“ hat keine einfache `kennung`. Erlaubt sind Kleinbuchstaben, Ziffern und Bindestriche, etwa `kennung = \"b8\"`.",
                        e.titel
                    ),
                ));
            }
            for (feld, wert) in [("titel", &e.titel), ("ziel", &e.ziel)] {
                if wert.trim().is_empty() {
                    return Err(Mangel::bei("kennung", k, format!("Die Etappe „{k}“ hat kein `{feld}`.")));
                }
            }
            if e.reihenfolge == 0 {
                return Err(Mangel::bei("kennung", k, format!("Bei der Etappe „{k}“ steht `reihenfolge = 0`. Die Reihenfolge beginnt bei 1.")));
            }
            if let Some(ab) = e.ab.as_deref().filter(|a| a.len() != 7 || !ist_datum(a, true)) {
                return Err(Mangel::bei(
                    "ab",
                    ab,
                    format!("Bei der Etappe „{k}“ steht `ab = \"{ab}\"`. Erlaubt ist ein Monat JJJJ-MM, etwa `ab = \"2026-12\"`."),
                ));
            }
            if let Some(a) = e.arbeiten.iter().find(|a| !ist_arbeitskennung(a)) {
                // Die Zeile der Etappe: eine Liste findet die Zeilensuche nicht.
                return Err(Mangel::bei(
                    "kennung",
                    k,
                    format!("Bei der Etappe „{k}“ steht in `arbeiten` „{a}“. Erlaubt sind Kennungen von Arbeiten, etwa \"ar-20261113-0009\"."),
                ));
            }
            if e.abgeschlossen && !e.arbeiten.is_empty() {
                return Err(Mangel::bei(
                    "kennung",
                    k,
                    format!("Die Etappe „{k}“ hat `abgeschlossen = true` und `arbeiten`. `abgeschlossen` gilt nur für Etappen ohne Arbeit im Tracker."),
                ));
            }
        }
        Ok(())
    }
}

/// Über die ganze Datei (alle Teile): jede Kennung und jede Reihenfolge nur
/// einmal; die Zeile zeigt auf das zweite Vorkommen.
pub fn plan_ueber_die_datei(d: &Datei<Plan>, datei: &str, text: &str) -> Result<(), LeseFehler> {
    let quelle = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut kennungen = BTreeSet::new();
    let mut reihenfolgen = BTreeSet::new();
    for e in d.inhalte().into_iter().flat_map(|t| &t.etappe) {
        if !kennungen.insert(e.kennung.as_str()) {
            let k = &e.kennung;
            return Err(Mangel::beim_zweiten("kennung", k, format!("Die Etappe „{k}“ gibt es zweimal. Jede `kennung` nur einmal, auch über Teile hinweg."))
                .in_datei(datei, quelle));
        }
        if !reihenfolgen.insert(e.reihenfolge) {
            let r = e.reihenfolge.to_string();
            return Err(Mangel::beim_zweiten(
                "reihenfolge",
                &r,
                format!("Die Reihenfolge {r} gibt es zweimal (zuletzt bei „{}“). Jede Etappe hat ihren eigenen Platz.", e.kennung),
            )
            .in_datei(datei, quelle));
        }
    }
    Ok(())
}

impl Datei<Plan> {
    /// Die Etappe mit dieser Kennung (in allen Teilen).
    pub fn etappe(&self, kennung: &str) -> Option<&Etappe> {
        self.inhalte().into_iter().flat_map(|t| &t.etappe).find(|e| e.kennung == kennung)
    }
}

// ----- GET /api/projekte/<slug>/plan -----

/// Der Plan eines Projekts, wie der Server ihn aus `plan.toml` und dem
/// Tracker rechnet (`crate::plan`).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct ProjektPlan {
    pub projekt: String,
    /// Nach `reihenfolge`.
    pub etappen: Vec<PlanEtappe>,
    /// Arbeiten, deren `etappe` auf dieses Projekt zeigt, die Etappe aber
    /// nicht (mehr) in `plan.toml` steht: die Gruppe „ohne gültige Etappe“.
    pub ohne_etappe: Vec<PlanArbeit>,
    /// Sätze, etwa wenn `plan.toml` sich nicht lesen lässt oder der
    /// Datenspeicher fehlt.
    pub hinweise: Vec<String>,
}

/// Der gerechnete Stand einer Etappe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum EtappenStand {
    /// Keine Arbeit hat begonnen (oder es gibt noch keine).
    Geplant,
    /// Mindestens eine Arbeit hat begonnen, nicht alle sind fertig.
    Laeuft,
    /// Alle Arbeiten sind fertig.
    Fertig,
    /// `abgeschlossen = true` und keine Arbeit im Tracker: ohne Datum.
    VorDemTracker,
}

/// Eine Etappe mit Stand, Zählung und Zeitpunkten aus dem Tracker.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct PlanEtappe {
    pub kennung: String,
    pub titel: String,
    pub ziel: String,
    pub reihenfolge: u32,
    /// JJJJ-MM aus `plan.toml`: ein echter Termin.
    pub ab: Option<String>,
    pub stand: EtappenStand,
    pub arbeiten_fertig: u32,
    /// Jede Arbeit zählt als eine, mit oder ohne Schritte.
    pub arbeiten_gesamt: u32,
    /// Ortszeit, RFC 3339: als die erste Arbeit begann.
    pub begonnen: Option<String>,
    /// Ortszeit, RFC 3339: als die letzte Arbeit fertig wurde (nur bei `fertig`).
    pub fertig: Option<String>,
    /// `fertig` ist ungefähr: mindestens eine Arbeit hat keinen Eintrag dazu
    /// im Verlauf, für sie gilt ihre letzte Änderung.
    pub ungefaehr: bool,
    /// Ortszeit, RFC 3339: die früheste Zuordnung einer dazugekommenen Arbeit,
    /// sonst leer.
    pub gewachsen_seit: Option<String>,
    /// Wie viele Arbeiten nach `begonnen` dazukamen: ihre Zuordnung (letzter
    /// Eintrag `etappe`, sonst `erstellt`) liegt danach und nach dem Tag der
    /// ersten Zuordnung. Was an diesem Tag zugeordnet wurde, etwa bei der
    /// Umstellung des Bestands, zählt nicht. Arbeiten nur aus der Liste
    /// `arbeiten` in `plan.toml` haben keine Zuordnungszeit: sie zählen nie
    /// und bestimmen den Tag der ersten Zuordnung nicht mit.
    pub dazugekommen: u32,
    /// Nach Kennung, also nach Anlage.
    pub arbeiten: Vec<PlanArbeit>,
}

/// Eine Arbeit im Plan: nur Titel und Stand, auch aus dem Archiv.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct PlanArbeit {
    pub id: String,
    pub titel: String,
    pub stand: super::anstehend::ArbeitsStand,
    /// Das Projekt der Arbeit, wenn es ein anderes ist („aus <Projekt>“).
    pub aus: Option<String>,
    pub archiviert: bool,
    /// `etappe` der Arbeit, wie sie gespeichert ist (leer: über `plan.toml` zugeordnet).
    pub etappe: Option<String>,
    /// Ortszeit, RFC 3339: wann sie fertig wurde (nur bei `fertig`).
    pub fertig: Option<String>,
    /// `fertig` ist die letzte Änderung, kein Eintrag im Verlauf.
    pub ungefaehr: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vertrag::daten::ProjektDaten;

    fn lesen(text: &str) -> Result<ProjektDaten, LeseFehler> {
        let mut d = ProjektDaten::default();
        d.datei_einlesen("plan.toml", "projects/x/daten/plan.toml", text)?;
        Ok(d)
    }

    const B8: &str = "[[etappe]]\nkennung = \"b8\"\ntitel = \"Projektseiten neu\"\nziel = \"Stand aus dem Tracker.\"\nreihenfolge = 1\n";

    /// Pflichtfelder, Form, doppelte Kennung und doppelte Reihenfolge (mit Zeile
    /// des zweiten Vorkommens), auch über Teile; Gegenprobe: gültige Dateien.
    #[test]
    fn plan_pruefung() {
        let d = lesen(&format!("{B8}ab = \"2026-12\"\narbeiten = [\"ar-20261104-0001\"]\n\n[[etappe]]\nkennung = \"c\"\ntitel = \"C\"\nziel = \"Z\"\nreihenfolge = 2\nabgeschlossen = true\n")).unwrap();
        let p = d.plan.unwrap();
        assert_eq!(p.etappe("b8").map(|e| e.arbeiten.len()), Some(1));
        assert!(p.etappe("c").unwrap().abgeschlossen);
        assert!(lesen("[teil.a]\n[[teil.a.etappe]]\nkennung = \"a\"\ntitel = \"A\"\nziel = \"Z\"\nreihenfolge = 1\n").is_ok());
        for (text, satz, zeile) in [
            ("[[etappe]]\nkennung = \"b8\"\ntitel = \"T\"\nreihenfolge = 1\n", "ziel", None),
            (&format!("{B8}\n[[etappe]]\nkennung = \"b8\"\ntitel = \"T\"\nziel = \"Z\"\nreihenfolge = 2\n") as &str, "„b8“ gibt es zweimal", Some(8)),
            (&format!("{B8}\n[[etappe]]\nkennung = \"c\"\ntitel = \"T\"\nziel = \"Z\"\nreihenfolge = 1\n"), "Reihenfolge 1 gibt es zweimal", Some(11)),
            ("[[etappe]]\nkennung = \"B8\"\ntitel = \"T\"\nziel = \"Z\"\nreihenfolge = 1\n", "keine einfache `kennung`", Some(2)),
            ("[[etappe]]\nkennung = \"b8\"\ntitel = \" \"\nziel = \"Z\"\nreihenfolge = 1\n", "kein `titel`", Some(2)),
            ("[[etappe]]\nkennung = \"b8\"\ntitel = \"T\"\nziel = \"Z\"\nreihenfolge = 0\n", "beginnt bei 1", Some(2)),
            (&format!("{B8}ab = \"2026-12-01\"\n"), "Monat JJJJ-MM", Some(6)),
            (&format!("{B8}arbeiten = [\"b7\"]\n"), "Kennungen von Arbeiten", Some(2)),
            (&format!("{B8}abgeschlossen = true\narbeiten = [\"ar-20261104-0001\"]\n"), "nur für Etappen ohne Arbeit", Some(2)),
            (&format!("{B8}stand = \"laeuft\"\n"), "stand", None),
            (
                "[teil.a]\n[[teil.a.etappe]]\nkennung = \"a\"\ntitel = \"A\"\nziel = \"Z\"\nreihenfolge = 1\n[teil.b]\n[[teil.b.etappe]]\nkennung = \"a\"\ntitel = \"B\"\nziel = \"Z\"\nreihenfolge = 2\n",
                "„a“ gibt es zweimal",
                Some(9),
            ),
        ] {
            let e = lesen(text).unwrap_err();
            assert!(e.text.contains(satz), "{text}: {e}");
            if zeile.is_some() {
                assert_eq!(e.zeile, zeile, "{text}: {e}");
            }
        }
    }

    #[test]
    fn arbeitskennung() {
        assert!(ist_arbeitskennung("ar-20261113-0009"));
        for f in ["ar-2026111-00009", "ar-20261113_0009", "a-20261113-0009", "ar-20261113-009", "ar-2026111x-0009"] {
            assert!(!ist_arbeitskennung(f), "{f}");
        }
    }
}
