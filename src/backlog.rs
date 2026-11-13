//! Das Modul Backlog (Etappe B2, Entscheidung gemeinsame-bausteine):
//! Aufgaben, Arbeiten und Fragen eines Projekts als Spalten nach Stand und
//! Status, dazu die wartenden Einträge je Verantwortlichem.
//!
//! Die Kanban-Sicht ergibt sich allein aus dem Datenspeicher (Aufgaben,
//! Arbeiten, Antworten und Einstufungen) und aus `fragen.toml`; `kanban.toml`
//! von Hand läuft aus (`pruefe` gibt einen Hinweis, lesen und zeigen geht
//! vorerst weiter). Der Server rechnet nur und schreibt nichts, weder in den
//! Datenspeicher noch in den Inhalt.
//!
//! Spalten (feste Folge, [`BacklogSpaltenArt`]):
//!
//! - **Für dich:** offene Aufgaben (nicht beim Chat), die eine sichtbare Frage
//!   (Platz 1 der Warteschlange, [`crate::anstehend::fragen_warteschlange`])
//!   mit dem Satz, wie viele dahinter warten, und Arbeiten mit `wartet_auf =
//!   inhaber` ohne Verweis auf eine offene Aufgabe oder Frage.
//! - **Läuft** und **Als Nächstes:** Arbeiten mit diesem Stand.
//! - **Wartet:** Arbeiten mit `wartet` (auf einen Chat, extern oder auf den
//!   Inhaber mit Verweis) und Aufgaben beim Chat (eigene Worte geschickt).
//!
//! Etappe B8: Die Spalte „Fertig“ entfällt; fertige Arbeiten zeigt der Plan
//! (`/api/projekte/<slug>/plan`), ihre Zahl der Stand (`fertig_7_tage`).
//! Karten von Arbeiten tragen Etappe, Besitzer und Schritte.
//!
//! In einer Spalte stehen die Karten wie in „Anstehend“ (in „Als Nächstes“
//! zuerst nach der Reihenfolge der Etappe): wirksame Stufe,
//! Frist (ohne Frist zuletzt), Alter, Kennung. Wartende Einträge stehen
//! zusätzlich je Verantwortlichem ([`WartendGruppe`]): Inhaber, dann die
//! Chats nach Name, dann extern.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use crate::anstehend::{INHABER, Inhalt, ZUSTAND_BEIM_CHAT, alle_eintraege, fragen_auswahl, ist_sichtbar};
use crate::vertrag::api::{AnstehendEintrag, ArbeitsStand, EintragsArt};
use crate::vertrag::backlog::{Backlog, BacklogKarte, BacklogSpalte, BacklogSpaltenArt, Verantwortlich, WartendGruppe};

/// „noch 1 Frage wartet“, „noch 3 Fragen warten“ (derselbe Satz wie in der Oberfläche).
pub fn weitere_fragen_satz(n: u64) -> String {
    if n == 1 { "noch 1 Frage wartet".to_string() } else { format!("noch {n} Fragen warten") }
}

/// Die Rolle zu einem Wert aus `wartet_auf`.
pub fn rolle(wartet_auf: &str) -> Verantwortlich {
    if wartet_auf == INHABER {
        Verantwortlich::Inhaber
    } else if wartet_auf.starts_with("chat:") {
        Verantwortlich::Chat
    } else {
        Verantwortlich::Extern
    }
}

/// Die Überschrift einer Gruppe.
fn gruppen_titel(wer: &str) -> String {
    match rolle(wer) {
        Verantwortlich::Inhaber => "Wartet auf dich".to_string(),
        Verantwortlich::Chat => format!("Wartet auf den Chat „{}“", wer.trim_start_matches("chat:")),
        Verantwortlich::Extern => "Wartet auf jemanden draußen".to_string(),
    }
}

/// In welche Spalte ein offener Eintrag gehört; `None`: nicht im Backlog
/// (Meldungen, wartende Fragen hinter der sichtbaren).
fn spalte_von(e: &AnstehendEintrag) -> Option<BacklogSpaltenArt> {
    match e.art {
        EintragsArt::Aufgabe if e.zustand == ZUSTAND_BEIM_CHAT => Some(BacklogSpaltenArt::Wartet),
        EintragsArt::Aufgabe => Some(BacklogSpaltenArt::FuerDich),
        EintragsArt::Frage => ist_sichtbar(e).then_some(BacklogSpaltenArt::FuerDich),
        EintragsArt::Arbeit => match ArbeitsStand::aus_wort(&e.zustand)? {
            ArbeitsStand::Laeuft => Some(BacklogSpaltenArt::Laeuft),
            ArbeitsStand::AlsNaechstes => Some(BacklogSpaltenArt::AlsNaechstes),
            ArbeitsStand::Wartet if e.wartet_auf_dich => Some(BacklogSpaltenArt::FuerDich),
            ArbeitsStand::Wartet => Some(BacklogSpaltenArt::Wartet),
            ArbeitsStand::Fertig => None,
        },
        EintragsArt::Meldung => None,
    }
}

/// Eine Karte aus einem offenen Eintrag; bei Arbeiten mit Etappe und
/// Besitzer aus dem Datenspeicher.
fn karte(inhalt: &Inhalt, e: &AnstehendEintrag, spalte: BacklogSpaltenArt) -> BacklogKarte {
    let arbeit = (e.art == EintragsArt::Arbeit).then(|| inhalt.speicher.arbeiten.iter().find(|a| a.id == e.id)).flatten();
    let wartet_auf = match spalte {
        BacklogSpaltenArt::FuerDich | BacklogSpaltenArt::Wartet => e.wartet_auf.clone(),
        _ => None,
    };
    let hinweis = match e.art {
        EintragsArt::Frage => e.warteschlange.map(|w| w.offen.saturating_sub(1)).filter(|n| *n > 0).map(weitere_fragen_satz),
        _ => e.vermerk.clone(),
    };
    BacklogKarte {
        art: e.art,
        id: e.id.clone(),
        titel: e.titel.clone(),
        zustand: e.zustand.clone(),
        wartet_auf,
        frist: e.frist.clone(),
        frist_stufe: e.frist_stufe,
        hinweis,
        ziel: e.ziel.clone(),
        geaendert: e.erstellt.clone(),
        etappe: arbeit.and_then(|a| a.etappe.clone()),
        besitzer: arbeit.and_then(|a| a.besitzer.clone()),
        schritte: e.schritte.clone(),
    }
}

/// Wie „Anstehend“ ordnet: wirksame Stufe, Frist (ohne zuletzt), Alter, Art, Kennung.
fn vergleich(a: &AnstehendEintrag, b: &AnstehendEintrag) -> Ordering {
    let frist = |e: &AnstehendEintrag| (e.frist.is_none(), e.frist.clone());
    a.stufe
        .wirksam
        .cmp(&b.stufe.wirksam)
        .then_with(|| frist(a).cmp(&frist(b)))
        .then_with(|| a.erstellt.cmp(&b.erstellt))
        .then_with(|| a.art.cmp(&b.art))
        .then_with(|| a.id.cmp(&b.id))
}

/// Die Projekte, auf deren Etappen Arbeiten von `slug` zeigen (deren
/// `plan.toml` braucht `backlog` für die Reihenfolge).
pub fn etappen_projekte<'a>(inhalt: &'a Inhalt, slug: &str) -> BTreeSet<&'a str> {
    inhalt.speicher.arbeiten.iter().filter(|a| a.projekt == slug).filter_map(|a| a.etappe.as_deref()?.split_once('/').map(|(p, _)| p)).collect()
}

/// Die wartenden Karten je Verantwortlichem: Inhaber, Chats nach Name, extern.
pub fn wartend_gruppen(karten: &[BacklogKarte]) -> Vec<WartendGruppe> {
    let mut gruppen: Vec<WartendGruppe> = Vec::new();
    for k in karten {
        let Some(wer) = k.wartet_auf.as_deref() else { continue };
        match gruppen.iter_mut().find(|g| g.wer == wer) {
            Some(g) => g.karten.push(k.clone()),
            None => gruppen.push(WartendGruppe { rolle: rolle(wer), wer: wer.to_string(), titel: gruppen_titel(wer), karten: vec![k.clone()] }),
        }
    }
    gruppen.sort_by(|a, b| (a.rolle, &a.wer).cmp(&(b.rolle, &b.wer)));
    gruppen
}

/// Das Backlog eines Projekts; `None`, wenn es das Projekt nicht gibt.
/// `kanban_datei`: Das Projekt hat noch `daten/kanban.toml`. `reihenfolge`:
/// `<projekt>/<kennung>` einer Etappe → ihre Reihenfolge in `plan.toml`
/// ([`etappen_projekte`]); „Als Nächstes“ steht zuerst danach, ohne Etappe zuletzt.
pub fn backlog(inhalt: &Inhalt, slug: &str, kanban_datei: bool, reihenfolge: &BTreeMap<String, u32>) -> Option<Backlog> {
    inhalt.projekte.iter().find(|p| p.slug == slug)?;
    let mut offen: Vec<AnstehendEintrag> = alle_eintraege(inhalt).into_iter().filter(|e| e.projekt == slug).collect();
    offen.sort_by(vergleich);
    let mut spalten: Vec<BacklogSpalte> = BacklogSpaltenArt::ALLE
        .into_iter()
        .map(|art| BacklogSpalte { art, titel: art.titel().to_string(), bedeutung: art.bedeutung(), karten: Vec::new() })
        .collect();
    let mut in_spalte = |art: BacklogSpaltenArt, k: BacklogKarte| {
        if let Some(s) = spalten.iter_mut().find(|s| s.art == art) {
            s.karten.push(k);
        }
    };
    for e in &offen {
        if let Some(art) = spalte_von(e) {
            in_spalte(art, karte(inhalt, e, art));
        }
    }
    if let Some(s) = spalten.iter_mut().find(|s| s.art == BacklogSpaltenArt::AlsNaechstes) {
        // Stabil: innerhalb einer Etappe bleibt die Folge aus `vergleich`.
        // shortcut: Etappen anderer Projekte zählen nach ihrer eigenen Nummer; eigene Regel, wenn das stört.
        s.karten.sort_by_key(|k| k.etappe.as_ref().and_then(|e| reihenfolge.get(e)).map_or((true, 0), |r| (false, *r)));
    }
    let wartende: Vec<BacklogKarte> =
        spalten.iter().filter(|s| matches!(s.art, BacklogSpaltenArt::FuerDich | BacklogSpaltenArt::Wartet)).flat_map(|s| s.karten.iter().cloned()).collect();
    Some(Backlog {
        slug: slug.to_string(),
        heute: inhalt.heute.format("%Y-%m-%d").to_string(),
        wartend: wartend_gruppen(&wartende),
        spalten,
        fragen_weitere: fragen_auswahl(&offen, slug).weitere,
        kanban_datei,
        speicher_fehlt: inhalt.speicher_fehlt,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anstehend::{FragenDatei, Lesefehler, ProjektKopf};
    use crate::speicher::arbeiten::AusDemSpeicher;
    use crate::vertrag::api::{Arbeit, Aufgabe, AufgabenZustand, Einstufung, Stufe};
    use crate::vertrag::daten::{Frage, FrageArt};
    use chrono::NaiveDate;

    const HEUTE: &str = "2026-11-03";

    fn kopf(slug: &str) -> ProjektKopf {
        ProjektKopf {
            slug: slug.into(),
            titel: format!("Projekt {slug}"),
            status: "aktiv".into(),
            notizen: Vec::new(),
            zuletzt: "2026-11-03 10:00".into(),
            archiviert: false,
        }
    }

    fn frage(id: &str, prioritaet: Option<Stufe>) -> Frage {
        Frage {
            id: id.into(),
            text: format!("Frage {id}?"),
            art: FrageArt::JaNein,
            optionen: Vec::new(),
            empfehlung: None,
            gestellt: "2026-11-01".into(),
            prioritaet,
            frist: None,
        }
    }

    fn aufgabe(id: &str, zustand: AufgabenZustand, antwort: Option<&str>, geaendert: &str) -> Aufgabe {
        Aufgabe {
            id: id.into(),
            projekt: "beispiel".into(),
            von: "chat:beispiel".into(),
            titel: format!("Aufgabe {id}"),
            knopf: None,
            frist: None,
            ablauf: None,
            zustand,
            antwort: antwort.map(str::to_string),
            eigene: antwort.filter(|a| *a == crate::vertrag::daten::ETWAS_ANDERES).map(|_| "Lieber später.".to_string()),
            erstellt: "2026-11-02T09:00:00+01:00".into(),
            geaendert: geaendert.into(),
            hat_link: false,
            vorgang: None,
            prioritaet: None,
            verlauf: Vec::new(),
            frist_bezug: None,
        }
    }

    fn arbeit(id: &str, stand: ArbeitsStand, wartet_auf: Option<&str>, verweis: Option<&str>) -> Arbeit {
        Arbeit {
            id: id.into(),
            projekt: "beispiel".into(),
            von: "chat:beispiel".into(),
            titel: format!("Arbeit {id}"),
            stand,
            wartet_auf: wartet_auf.map(str::to_string),
            verweis: verweis.map(str::to_string),
            prioritaet: Stufe::Bald,
            frist: None,
            notiz: None,
            erstellt: "2026-11-02T08:00:00+01:00".into(),
            geaendert: "2026-11-02T08:00:00+01:00".into(),
            verlauf: Vec::new(),
            besitzer: None,
            schritte: Vec::new(),
            etappe: None,
        }
    }

    /// Ein erfundenes Projekt mit allem, was das Backlog kennt.
    fn inhalt() -> Inhalt {
        Inhalt {
            heute: NaiveDate::parse_from_str(HEUTE, "%Y-%m-%d").unwrap(),
            projekte: vec![kopf("beispiel"), kopf("anderes")],
            fragen: vec![FragenDatei {
                projekt: "beispiel".into(),
                datei: "projects/beispiel/daten/fragen.toml".into(),
                // In der Datei: erst „farbe“, dann „name“, dann „termin“ (irgendwann).
                fragen: vec![frage("farbe", Some(Stufe::Bald)), frage("name", Some(Stufe::Bald)), frage("termin", Some(Stufe::Irgendwann))],
            }],
            fristen: Vec::new(),
            speicher: AusDemSpeicher {
                aufgaben: vec![
                    aufgabe("a-20261102-0001", AufgabenZustand::Offen, None, "2026-11-02T09:00:00+01:00"),
                    aufgabe("a-20261102-0002", AufgabenZustand::Offen, Some(crate::vertrag::daten::ETWAS_ANDERES), "2026-11-02T10:00:00+01:00"),
                ],
                arbeiten: vec![
                    arbeit("ar-20261102-0001", ArbeitsStand::Laeuft, None, None),
                    arbeit("ar-20261102-0002", ArbeitsStand::Wartet, Some("extern"), None),
                    arbeit("ar-20261102-0003", ArbeitsStand::Wartet, Some("chat:zweites"), None),
                    arbeit("ar-20261102-0004", ArbeitsStand::Wartet, Some("inhaber"), Some("farbe")),
                    arbeit("ar-20261102-0005", ArbeitsStand::Wartet, Some("inhaber"), None),
                    arbeit("ar-20261102-0006", ArbeitsStand::AlsNaechstes, None, None),
                    arbeit("ar-20261102-0007", ArbeitsStand::Fertig, None, None),
                    arbeit("ar-20261102-0008", ArbeitsStand::Wartet, Some("chat:erstes"), None),
                ],
                ..Default::default()
            },
            speicher_fehlt: false,
            schwerpunkte: Default::default(),
            gruppen: Vec::new(),
            hinweise: Vec::new(),
            lesefehler: Lesefehler::default(),
        }
    }

    fn ids(s: &BacklogSpalte) -> Vec<&str> {
        s.karten.iter().map(|k| k.id.as_str()).collect()
    }

    /// Spalten aus Stand und Status: jede Art landet in ihrer Spalte, die
    /// Fragen-Warteschlange zeigt nur eine Frage mit dem Satz zu den übrigen.
    #[test]
    fn spalten_aus_stand_und_status() {
        let b = backlog(&inhalt(), "beispiel", true, &BTreeMap::new()).unwrap();
        let arten: Vec<BacklogSpaltenArt> = b.spalten.iter().map(|s| s.art).collect();
        assert_eq!(arten, BacklogSpaltenArt::ALLE, "alle vier, auch leere, in fester Folge");
        assert_eq!(arten.len(), 4, "Etappe B8: ohne „Fertig“");
        let s = |a: BacklogSpaltenArt| b.spalten.iter().find(|s| s.art == a).unwrap();
        // Gleiche Stufe: nach Alter (die Frage ist am ältesten, Arbeiten um 8, Aufgaben um 9 Uhr).
        assert_eq!(ids(s(BacklogSpaltenArt::FuerDich)), ["farbe", "ar-20261102-0005", "a-20261102-0001"]);
        assert_eq!(ids(s(BacklogSpaltenArt::Laeuft)), ["ar-20261102-0001"]);
        assert_eq!(ids(s(BacklogSpaltenArt::Wartet)), ["ar-20261102-0002", "ar-20261102-0003", "ar-20261102-0004", "ar-20261102-0008", "a-20261102-0002"]);
        assert_eq!(ids(s(BacklogSpaltenArt::AlsNaechstes)), ["ar-20261102-0006"]);
        assert!(!b.spalten.iter().flat_map(|s| &s.karten).any(|k| k.id == "ar-20261102-0007"), "fertige Arbeiten nur im Plan");
        let frage = &s(BacklogSpaltenArt::FuerDich).karten[0];
        assert_eq!(frage.hinweis.as_deref(), Some("noch 2 Fragen warten"));
        assert_eq!(frage.ziel, "/projects/beispiel#frage-farbe");
        assert!(!b.spalten.iter().flat_map(|s| &s.karten).any(|k| k.id == "name" || k.id == "termin"), "wartende Fragen ohne Text");
        assert_eq!(b.fragen_weitere, 2);
        assert!(b.kanban_datei);
        let beim_chat = &s(BacklogSpaltenArt::Wartet).karten[4];
        assert_eq!((beim_chat.zustand.as_str(), beim_chat.wartet_auf.as_deref()), ("beim-chat", Some("chat:beispiel")));
        // Ein anderes Projekt ist leer, ein unbekanntes `None`.
        assert!(backlog(&inhalt(), "anderes", false, &BTreeMap::new()).unwrap().spalten.iter().all(|s| s.karten.is_empty()));
        assert!(backlog(&inhalt(), "gibt-es-nicht", false, &BTreeMap::new()).is_none());
    }

    /// Etappe B8: Karten von Arbeiten tragen Etappe, Besitzer und Schritte;
    /// andere Karten nicht.
    #[test]
    fn karten_mit_etappe_besitzer_schritten() {
        let mut i = inhalt();
        let a = &mut i.speicher.arbeiten[0];
        a.etappe = Some("beispiel/b8".into());
        a.besitzer = Some("chat:beispiel".into());
        a.schritte = vec![crate::vertrag::api::Schritt {
            nr: 1,
            titel: "Vertrag".into(),
            stand: ArbeitsStand::Laeuft,
            besitzer: Some("chat:beispiel".into()),
            wartet_auf: None,
            verweis: None,
            angeboten_an: None,
            fuer: None,
            seit: "2026-11-02T08:00:00+01:00".into(),
            notiz: None,
            vorgang: None,
        }];
        let b = backlog(&i, "beispiel", false, &BTreeMap::new()).unwrap();
        let k = &b.spalten[1].karten[0];
        assert_eq!((k.id.as_str(), k.etappe.as_deref(), k.besitzer.as_deref()), ("ar-20261102-0001", Some("beispiel/b8"), Some("chat:beispiel")));
        assert_eq!(k.schritte.iter().map(|s| s.titel.as_str()).collect::<Vec<_>>(), ["Vertrag"]);
        let andere: Vec<&BacklogKarte> = b.spalten.iter().flat_map(|s| &s.karten).filter(|k| k.id != "ar-20261102-0001").collect();
        assert!(andere.iter().all(|k| k.etappe.is_none() && k.besitzer.is_none() && k.schritte.is_empty()));
    }

    /// Etappe B8: „Als Nächstes“ zuerst nach der Reihenfolge der Etappe, auch
    /// gegen die Stufe; ohne (bekannte) Etappe zuletzt, dort weiter nach Stufe.
    #[test]
    fn als_naechstes_nach_etappen_reihenfolge() {
        let mut i = inhalt();
        let mut neu = |id: &str, stufe: Stufe, etappe: Option<&str>| {
            let mut a = arbeit(id, ArbeitsStand::AlsNaechstes, None, None);
            a.prioritaet = stufe;
            a.etappe = etappe.map(str::to_string);
            i.speicher.arbeiten.push(a);
        };
        neu("ar-20261102-0101", Stufe::Jetzt, Some("beispiel/spaet"));
        neu("ar-20261102-0102", Stufe::Irgendwann, Some("beispiel/frueh"));
        neu("ar-20261102-0103", Stufe::Jetzt, Some("beispiel/unbekannt"));
        assert_eq!(etappen_projekte(&i, "beispiel").into_iter().collect::<Vec<_>>(), ["beispiel"]);
        let reihenfolge = BTreeMap::from([("beispiel/frueh".to_string(), 1), ("beispiel/spaet".to_string(), 2)]);
        let b = backlog(&i, "beispiel", false, &reihenfolge).unwrap();
        let s = b.spalten.iter().find(|s| s.art == BacklogSpaltenArt::AlsNaechstes).unwrap();
        let ids: Vec<&str> = s.karten.iter().map(|k| k.id.as_str()).collect();
        assert_eq!(ids, ["ar-20261102-0102", "ar-20261102-0101", "ar-20261102-0103", "ar-20261102-0006"]);
    }

    /// Etappe B8, Gleichheit von Stand und Backlog: Die Zahlen des Stands
    /// sind die Größen der Gruppen im Backlog (läuft, als Nächstes, wartet auf
    /// dich); `fertig_7_tage` zählt die fertigen Arbeiten, die das Backlog nicht
    /// mehr zeigt (heute bis sechs Tage zurück, nicht sieben).
    #[test]
    fn stand_zahlen_gleich_backlog() {
        let mut i = inhalt();
        // Ohne Verlauf gilt `geaendert` als Fertig-Zeitpunkt (ungefähr).
        i.speicher.arbeiten[6].geaendert = "2026-10-28T08:00:00+01:00".into();
        let mut alt = arbeit("ar-20261027-0001", ArbeitsStand::Fertig, None, None);
        alt.geaendert = "2026-10-27T23:00:00+01:00".into();
        i.speicher.arbeiten.push(alt);
        let b = backlog(&i, "beispiel", false, &BTreeMap::new()).unwrap();
        let groesse = |a: BacklogSpaltenArt| b.spalten.iter().find(|s| s.art == a).unwrap().karten.len() as u64;
        for sichtbar in [false, true] {
            let s = crate::anstehend::stand(&i, "beispiel", sichtbar).unwrap();
            assert_eq!(s.fertig_7_tage, 1, "sichtbar {sichtbar}");
        }
        let z = crate::anstehend::stand(&i, "beispiel", false).unwrap().zahlen.unwrap();
        assert_eq!(
            (z.laeuft, z.als_naechstes, z.wartet_auf_dich, z.fertig_7_tage),
            (groesse(BacklogSpaltenArt::Laeuft), groesse(BacklogSpaltenArt::AlsNaechstes), groesse(BacklogSpaltenArt::FuerDich), 1)
        );
    }

    /// Wartende je Verantwortlichem: Inhaber zuerst (auch die Frage und die
    /// Arbeit mit Verweis), dann die Chats nach Name, dann extern.
    #[test]
    fn wartende_je_verantwortlichem() {
        let b = backlog(&inhalt(), "beispiel", false, &BTreeMap::new()).unwrap();
        let gruppen: Vec<(&str, Vec<&str>)> = b.wartend.iter().map(|g| (g.wer.as_str(), g.karten.iter().map(|k| k.id.as_str()).collect())).collect();
        assert_eq!(
            gruppen,
            [
                ("inhaber", vec!["farbe", "ar-20261102-0005", "a-20261102-0001", "ar-20261102-0004"]),
                ("chat:beispiel", vec!["a-20261102-0002"]),
                ("chat:erstes", vec!["ar-20261102-0008"]),
                ("chat:zweites", vec!["ar-20261102-0003"]),
                ("extern", vec!["ar-20261102-0002"]),
            ]
        );
        let titel: Vec<&str> = b.wartend.iter().map(|g| g.titel.as_str()).collect();
        assert_eq!(
            titel,
            [
                "Wartet auf dich",
                "Wartet auf den Chat „beispiel“",
                "Wartet auf den Chat „erstes“",
                "Wartet auf den Chat „zweites“",
                "Wartet auf jemanden draußen"
            ]
        );
        let mit_verweis = &b.wartend[0].karten[3];
        assert_eq!(mit_verweis.hinweis.as_deref(), Some(crate::anstehend::VERMERK_WARTET));
    }

    /// Nach der Antwort rückt die nächste Frage nach; stuft der Inhaber eine
    /// wartende hoch, wird sie die sichtbare.
    #[test]
    fn warteschlange_rueckt_nach() {
        let mut i = inhalt();
        i.speicher.beantwortet.insert(crate::speicher::antworten::antwort_schluessel("beispiel", "farbe", "2026-11-01"));
        let b = backlog(&i, "beispiel", false, &BTreeMap::new()).unwrap();
        let fuer_dich = &b.spalten[0];
        assert!(fuer_dich.karten.iter().any(|k| k.id == "name" && k.hinweis.as_deref() == Some("noch 1 Frage wartet")));
        assert_eq!(b.fragen_weitere, 1);
        i.speicher.einstufungen.push(Einstufung {
            projekt: "beispiel".into(),
            art: EintragsArt::Frage,
            id: "termin".into(),
            gestellt: Some("2026-11-01".into()),
            stufe: Some(Stufe::Jetzt),
            zeit: "2026-11-03T08:00:00+01:00".into(),
        });
        let b = backlog(&i, "beispiel", false, &BTreeMap::new()).unwrap();
        assert!(b.spalten[0].karten.iter().any(|k| k.id == "termin"));
        assert!(!b.spalten[0].karten.iter().any(|k| k.id == "name"));
    }

    /// Ohne Datenspeicher: nur die Frage aus der Datei, `speicher_fehlt`.
    #[test]
    fn ohne_datenspeicher() {
        let mut i = inhalt();
        i.speicher = AusDemSpeicher::default();
        i.speicher_fehlt = true;
        let b = backlog(&i, "beispiel", false, &BTreeMap::new()).unwrap();
        assert!(b.speicher_fehlt);
        let alle: Vec<&str> = b.spalten.iter().flat_map(|s| s.karten.iter().map(|k| k.id.as_str())).collect();
        assert_eq!(alle, ["farbe"]);
    }
}
