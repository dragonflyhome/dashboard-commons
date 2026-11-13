//! Der Plan eines Projekts (Etappe B8, `GET /api/projekte/<slug>/plan`): die
//! Etappen aus `daten/plan.toml` mit Stand, Zählung und Zeitpunkten aus dem
//! Tracker. Von Hand steht nur die Absicht; alles hier rechnet der Server bei
//! jedem Abruf neu und schreibt nichts.
//!
//! Zuordnung einer Arbeit zu einer Etappe: ihr Feld `etappe` (auch aus einem
//! anderen Projekt, dann mit `aus`), sonst die Liste `arbeiten` der Etappe in
//! `plan.toml` (für archivierte und ältere Arbeiten). Zeigt `etappe` auf dieses
//! Projekt, die Etappe steht aber nicht in `plan.toml`, kommt die Arbeit in die
//! Gruppe „ohne gültige Etappe“.
//!
//! Zeitpunkte einer Arbeit stehen im Verlauf: Einträge `stand` (Schema 11),
//! für den Altbestand das erste Wort von `neu` der Schritt-Einträge. Ohne
//! Eintrag gilt für eine fertige Arbeit `geaendert`, als „ungefähr“ markiert;
//! für den Beginn einer begonnenen Arbeit `erstellt`.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, FixedOffset};

use crate::vertrag::anstehend::{Arbeit, ArbeitVerlaufArt, ArbeitsStand};
use crate::vertrag::daten::Datei;
use crate::vertrag::plan::{Etappe, EtappenStand, Plan, PlanArbeit, PlanEtappe, ProjektPlan};

fn zeit(s: &str) -> Option<DateTime<FixedOffset>> {
    DateTime::parse_from_rfc3339(s).ok()
}

/// Der früheste (oder späteste) lesbare Zeitpunkt.
fn grenzwert<'a>(zeiten: impl Iterator<Item = &'a String>, spaeteste: bool) -> Option<String> {
    let lesbar = zeiten.filter_map(|z| zeit(z).map(|t| (t, z)));
    let beste = if spaeteste { lesbar.max_by_key(|(t, _)| *t) } else { lesbar.min_by_key(|(t, _)| *t) };
    beste.map(|(_, z)| z.clone())
}

/// Beginn und Ende einer Arbeit aus ihrem Verlauf (auch für den Verlauf des
/// Projekts, `projektverlauf`).
pub(crate) struct Zeiten {
    pub begonnen: Option<String>,
    pub fertig: Option<String>,
    pub ungefaehr: bool,
}

pub(crate) fn zeiten(a: &Arbeit) -> Zeiten {
    let mut begonnen = None;
    let (mut fertig_stand, mut fertig_schritt) = (None, None);
    for e in &a.verlauf {
        // SchrittAngelegt trägt in `neu` meist den Titel, keinen Stand.
        let (neu, aus_stand) = match e.art {
            ArbeitVerlaufArt::Stand => (e.neu.as_str(), true),
            ArbeitVerlaufArt::SchrittUebernommen | ArbeitVerlaufArt::SchrittGeaendert | ArbeitVerlaufArt::SchrittGeweckt => {
                (e.neu.split(',').next().unwrap_or_default().trim(), false)
            }
            _ => continue,
        };
        if begonnen.is_none() && matches!(ArbeitsStand::aus_wort(neu), Some(ArbeitsStand::Laeuft | ArbeitsStand::Wartet | ArbeitsStand::Fertig)) {
            begonnen = Some(e.zeit.clone());
        }
        if neu == ArbeitsStand::Fertig.wort() {
            *(if aus_stand { &mut fertig_stand } else { &mut fertig_schritt }) = Some(e.zeit.clone());
        }
    }
    if a.stand != ArbeitsStand::Fertig {
        let begonnen = begonnen.or_else(|| (a.stand != ArbeitsStand::AlsNaechstes).then(|| a.erstellt.clone()));
        return Zeiten { begonnen, fertig: None, ungefaehr: false };
    }
    let genau = fertig_stand.or(fertig_schritt);
    Zeiten { begonnen: begonnen.or_else(|| Some(a.erstellt.clone())), ungefaehr: genau.is_none(), fertig: Some(genau.unwrap_or_else(|| a.geaendert.clone())) }
}

fn plan_arbeit(a: &Arbeit, archiviert: bool, slug: &str, z: &Zeiten) -> PlanArbeit {
    PlanArbeit {
        id: a.id.clone(),
        titel: a.titel.clone(),
        stand: a.stand,
        aus: (a.projekt != slug).then(|| a.projekt.clone()),
        archiviert,
        etappe: a.etappe.clone(),
        fertig: z.fertig.clone(),
        ungefaehr: z.ungefaehr,
    }
}

fn etappe_rechnen(e: &Etappe, arbeiten: &[(&Arbeit, bool)], slug: &str) -> PlanEtappe {
    let zeiten: Vec<Zeiten> = arbeiten.iter().map(|(a, _)| zeiten(a)).collect();
    let gesamt = arbeiten.len() as u32;
    let fertig_n = arbeiten.iter().filter(|(a, _)| a.stand == ArbeitsStand::Fertig).count() as u32;
    let begonnen = grenzwert(zeiten.iter().filter_map(|z| z.begonnen.as_ref()), false);
    let stand = match () {
        _ if gesamt == 0 && e.abgeschlossen => EtappenStand::VorDemTracker,
        _ if gesamt == 0 => EtappenStand::Geplant,
        _ if fertig_n == gesamt => EtappenStand::Fertig,
        _ if begonnen.is_some() => EtappenStand::Laeuft,
        _ => EtappenStand::Geplant,
    };
    let ist_fertig = stand == EtappenStand::Fertig;
    let beginn = begonnen.as_deref().and_then(zeit);
    let dazugekommen = arbeiten.iter().filter(|(a, _)| beginn.is_some_and(|b| zeit(&a.erstellt).is_some_and(|t| t > b))).count() as u32;
    PlanEtappe {
        kennung: e.kennung.clone(),
        titel: e.titel.clone(),
        ziel: e.ziel.clone(),
        reihenfolge: e.reihenfolge,
        ab: e.ab.clone(),
        stand,
        arbeiten_fertig: fertig_n,
        arbeiten_gesamt: gesamt,
        fertig: if ist_fertig { grenzwert(zeiten.iter().filter_map(|z| z.fertig.as_ref()), true) } else { None },
        ungefaehr: ist_fertig && zeiten.iter().any(|z| z.ungefaehr),
        gewachsen_seit: if dazugekommen > 0 { begonnen.clone() } else { None },
        begonnen,
        dazugekommen,
        arbeiten: arbeiten.iter().zip(&zeiten).map(|((a, arch), z)| plan_arbeit(a, *arch, slug, z)).collect(),
    }
}

/// Der Plan von `slug` aus seiner `plan.toml` (fehlt sie: keine Etappen) und
/// allen Arbeiten aller Projekte (`true`: im Archiv).
pub fn plan(slug: &str, datei: Option<&Datei<Plan>>, arbeiten: &[(Arbeit, bool)]) -> ProjektPlan {
    let mut etappen: Vec<&Etappe> = datei.map(|d| d.inhalte().into_iter().flat_map(|t| &t.etappe).collect()).unwrap_or_default();
    etappen.sort_by_key(|e| e.reihenfolge);
    let kennungen: BTreeSet<&str> = etappen.iter().map(|e| e.kennung.as_str()).collect();
    // Aus der Liste `arbeiten`: steht eine Kennung zweimal, gilt die frühere Etappe.
    let mut gelistet: BTreeMap<&str, &str> = BTreeMap::new();
    for e in &etappen {
        for id in &e.arbeiten {
            gelistet.entry(id.as_str()).or_insert(e.kennung.as_str());
        }
    }
    let mut je_etappe: BTreeMap<&str, Vec<(&Arbeit, bool)>> = BTreeMap::new();
    let mut ohne = Vec::new();
    for (a, arch) in arbeiten {
        let kennung = match a.etappe.as_deref() {
            // Das Feld gilt vor der Liste; eine Etappe eines anderen Projekts zählt dort.
            Some(e) => match e.strip_prefix(slug).and_then(|r| r.strip_prefix('/')) {
                Some(k) if kennungen.contains(k) => k,
                Some(_) => {
                    ohne.push((a, *arch));
                    continue;
                }
                None => continue,
            },
            None => match gelistet.get(a.id.as_str()) {
                Some(k) => k,
                None => continue,
            },
        };
        je_etappe.entry(kennung).or_default().push((a, *arch));
    }
    for liste in je_etappe.values_mut().chain(std::iter::once(&mut ohne)) {
        liste.sort_by(|x, y| x.0.id.cmp(&y.0.id));
    }
    ProjektPlan {
        projekt: slug.to_string(),
        etappen: etappen.iter().map(|e| etappe_rechnen(e, je_etappe.get(e.kennung.as_str()).map_or(&[], Vec::as_slice), slug)).collect(),
        ohne_etappe: ohne.iter().map(|(a, arch)| plan_arbeit(a, *arch, slug, &zeiten(a))).collect(),
        hinweise: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vertrag::anstehend::{ArbeitVerlauf, Stufe};
    use crate::vertrag::daten::ProjektDaten;

    fn datei(text: &str) -> Datei<Plan> {
        let mut d = ProjektDaten::default();
        d.datei_einlesen("plan.toml", "projects/p/daten/plan.toml", text).unwrap();
        d.plan.unwrap()
    }

    const PLAN: &str = "\
[[etappe]]\nkennung = \"zwei\"\ntitel = \"Zwei\"\nziel = \"Z.\"\nreihenfolge = 2\narbeiten = [\"ar-20261001-0001\"]\n\
[[etappe]]\nkennung = \"eins\"\ntitel = \"Eins\"\nziel = \"Z.\"\nreihenfolge = 1\nab = \"2026-12\"\n\
[[etappe]]\nkennung = \"alt\"\ntitel = \"Alt\"\nziel = \"Z.\"\nreihenfolge = 3\nabgeschlossen = true\n\
[[etappe]]\nkennung = \"leer\"\ntitel = \"Leer\"\nziel = \"Z.\"\nreihenfolge = 4\n";

    fn t(tag: u32, stunde: u32) -> String {
        format!("2026-11-{tag:02}T{stunde:02}:00:00+01:00")
    }

    fn arbeit(id: &str, projekt: &str, stand: ArbeitsStand, etappe: Option<&str>, erstellt: String, geaendert: String) -> Arbeit {
        Arbeit {
            id: id.to_string(),
            projekt: projekt.to_string(),
            von: "chat:p".to_string(),
            besitzer: None,
            titel: format!("Titel {id}"),
            stand,
            wartet_auf: None,
            verweis: None,
            prioritaet: Stufe::Bald,
            frist: None,
            notiz: None,
            erstellt,
            geaendert,
            verlauf: Vec::new(),
            schritte: Vec::new(),
            etappe: etappe.map(str::to_string),
        }
    }

    fn eintrag(art: ArbeitVerlaufArt, zeit: String, alt: &str, neu: &str) -> ArbeitVerlauf {
        ArbeitVerlauf { zeit, von: "chat:p".to_string(), art, schritt: None, alt: alt.to_string(), neu: neu.to_string(), text: String::new() }
    }

    /// Reihenfolge, die vier Stände, Zählung, Herkunft und die Gruppe ohne
    /// gültige Etappe; eine Etappe eines fremden Projekts zählt hier nicht.
    #[test]
    fn plan_staende_und_zuordnung() {
        let mut laeuft = arbeit("ar-20261102-0001", "p", ArbeitsStand::Laeuft, Some("p/eins"), t(2, 9), t(3, 9));
        laeuft.verlauf.push(eintrag(ArbeitVerlaufArt::Stand, t(2, 10), "als-naechstes", "laeuft"));
        let fertig = arbeit("ar-20261101-0001", "p", ArbeitsStand::Fertig, Some("p/eins"), t(1, 9), t(4, 9));
        let fremd = arbeit("ar-20261103-0001", "q", ArbeitsStand::AlsNaechstes, Some("p/eins"), t(3, 9), t(3, 9));
        let anderswo = arbeit("ar-20261103-0002", "p", ArbeitsStand::Laeuft, Some("q/eins"), t(3, 9), t(3, 9));
        let weg = arbeit("ar-20261103-0003", "p", ArbeitsStand::Laeuft, Some("p/gibt-es-nicht"), t(3, 9), t(3, 9));
        let mut archiv = arbeit("ar-20261001-0001", "p", ArbeitsStand::Fertig, None, t(1, 8), t(5, 8));
        archiv.verlauf.push(eintrag(ArbeitVerlaufArt::Stand, t(1, 12), "laeuft", "fertig"));
        let ohne_etappe = arbeit("ar-20261103-0004", "p", ArbeitsStand::Laeuft, None, t(3, 9), t(3, 9));
        let alle = vec![(laeuft, false), (fertig, false), (fremd, false), (anderswo, false), (weg, false), (archiv, true), (ohne_etappe, false)];
        let p = plan("p", Some(&datei(PLAN)), &alle);
        let k: Vec<(&str, EtappenStand)> = p.etappen.iter().map(|e| (e.kennung.as_str(), e.stand)).collect();
        assert_eq!(k, [("eins", EtappenStand::Laeuft), ("zwei", EtappenStand::Fertig), ("alt", EtappenStand::VorDemTracker), ("leer", EtappenStand::Geplant)]);
        let eins = &p.etappen[0];
        assert_eq!((eins.arbeiten_fertig, eins.arbeiten_gesamt, eins.ab.as_deref()), (1, 3, Some("2026-12")));
        let ids: Vec<&str> = eins.arbeiten.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(ids, ["ar-20261101-0001", "ar-20261102-0001", "ar-20261103-0001"]);
        assert_eq!(eins.arbeiten[2].aus.as_deref(), Some("q"), "aus einem anderen Projekt");
        assert_eq!(eins.arbeiten[0].aus, None);
        // Beginn: die fertige ohne Verlauf ab erstellt; nicht fertig, also kein Fertig-Zeitpunkt.
        assert_eq!((eins.begonnen.clone(), eins.fertig.clone()), (Some(t(1, 9)), None));
        // Zwei Arbeiten kamen nach dem Beginn dazu.
        assert_eq!((eins.gewachsen_seit.clone(), eins.dazugekommen), (Some(t(1, 9)), 2));
        // Über die Liste in plan.toml, aus dem Archiv, Fertig-Zeitpunkt aus dem Verlauf.
        let zwei = &p.etappen[1];
        assert_eq!((zwei.arbeiten_fertig, zwei.arbeiten_gesamt), (1, 1));
        assert!(zwei.arbeiten[0].archiviert);
        assert_eq!((zwei.fertig.clone(), zwei.ungefaehr, zwei.gewachsen_seit.clone()), (Some(t(1, 12)), false, None));
        assert_eq!((p.etappen[2].begonnen.clone(), p.etappen[2].fertig.clone()), (None, None), "vor dem Tracker ohne Datum");
        let ohne: Vec<(&str, Option<&str>)> = p.ohne_etappe.iter().map(|a| (a.id.as_str(), a.etappe.as_deref())).collect();
        assert_eq!(ohne, [("ar-20261103-0003", Some("p/gibt-es-nicht"))]);
        // Ohne plan.toml: keine Etappen, alles mit etappe auf p ist ohne gültige Etappe.
        let p = plan("p", None, &alle);
        assert!(p.etappen.is_empty());
        assert_eq!(p.ohne_etappe.len(), 4);
    }

    /// Altbestand ohne Einträge `stand`: Fertig-Zeitpunkt aus dem letzten
    /// Schritt-Eintrag mit „fertig“ (erstes Wort von neu); ganz ohne Eintrag
    /// gilt geaendert als „ungefähr“. Ein Eintrag `stand` geht vor.
    #[test]
    fn fertig_zeitpunkt_im_altbestand() {
        let mut aus_schritten = arbeit("ar-20261101-0001", "p", ArbeitsStand::Fertig, Some("p/eins"), t(1, 9), t(9, 9));
        aus_schritten.verlauf = vec![
            eintrag(ArbeitVerlaufArt::SchrittAngelegt, t(1, 10), "", "fertig machen"),
            eintrag(ArbeitVerlaufArt::SchrittUebernommen, t(2, 10), "als-naechstes", "laeuft, dran: chat:p"),
            eintrag(ArbeitVerlaufArt::SchrittGeaendert, t(3, 10), "laeuft, dran: chat:p", "fertig, dran: chat:p"),
            eintrag(ArbeitVerlaufArt::Titel, t(4, 10), "fertig", "fertig"),
        ];
        let z = zeiten(&aus_schritten);
        assert_eq!((z.begonnen, z.fertig, z.ungefaehr), (Some(t(2, 10)), Some(t(3, 10)), false));
        let mut gemischt = aus_schritten.clone();
        gemischt.verlauf.push(eintrag(ArbeitVerlaufArt::Stand, t(5, 10), "laeuft", "fertig"));
        assert_eq!(zeiten(&gemischt).fertig, Some(t(5, 10)), "stand geht vor");
        let ohne = arbeit("ar-20261102-0001", "p", ArbeitsStand::Fertig, Some("p/eins"), t(2, 9), t(8, 9));
        let z = zeiten(&ohne);
        assert_eq!((z.fertig, z.ungefaehr), (Some(t(8, 9)), true));
        let p = plan("p", Some(&datei(PLAN)), &[(aus_schritten, false), (ohne, false)]);
        let eins = &p.etappen[0];
        assert_eq!((eins.stand, eins.fertig.clone(), eins.ungefaehr), (EtappenStand::Fertig, Some(t(8, 9)), true));
        // Eine geplante Arbeit hat keinen Beginn; die Etappe bleibt geplant.
        let geplant = arbeit("ar-20261103-0001", "p", ArbeitsStand::AlsNaechstes, Some("p/eins"), t(3, 9), t(3, 9));
        let p = plan("p", Some(&datei(PLAN)), &[(geplant, false)]);
        assert_eq!((p.etappen[0].stand, p.etappen[0].begonnen.clone(), p.etappen[0].dazugekommen), (EtappenStand::Geplant, None, 0));
    }
}
