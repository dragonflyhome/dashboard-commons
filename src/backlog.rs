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
//! - **Fertig:** fertige Arbeiten und nicht mehr offene Aufgaben, solange sie
//!   nicht im Archiv sind (sieben Tage).
//!
//! In einer Spalte stehen die Karten wie in „Anstehend“: wirksame Stufe,
//! Frist (ohne Frist zuletzt), Alter, Kennung; in „Fertig“ die zuletzt
//! geänderte zuerst. Wartende Einträge stehen zusätzlich je Verantwortlichem
//! ([`WartendGruppe`]): Inhaber, dann die Chats nach Name, dann extern.

use std::cmp::Ordering;

use crate::anstehend::{INHABER, Inhalt, ZUSTAND_BEIM_CHAT, alle_eintraege, fragen_auswahl, ist_sichtbar};
use crate::vertrag::api::{AnstehendEintrag, Arbeit, ArbeitsStand, Aufgabe, AufgabenZustand, EintragsArt};
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
            ArbeitsStand::Fertig => Some(BacklogSpaltenArt::Fertig),
        },
        EintragsArt::Meldung => None,
    }
}

/// Eine Karte aus einem offenen Eintrag.
fn karte(e: &AnstehendEintrag, spalte: BacklogSpaltenArt) -> BacklogKarte {
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

/// Eine fertige Arbeit als Karte.
fn karte_arbeit(a: &Arbeit, slug: &str) -> BacklogKarte {
    BacklogKarte {
        art: EintragsArt::Arbeit,
        id: a.id.clone(),
        titel: a.titel.clone(),
        zustand: a.stand.wort().to_string(),
        wartet_auf: None,
        frist: a.frist.clone(),
        frist_stufe: None,
        hinweis: None,
        ziel: crate::anstehend::ziel(slug, EintragsArt::Arbeit, &a.id),
        geaendert: a.geaendert.clone(),
    }
}

/// Eine abgeschlossene Aufgabe als Karte (ohne Link, wie im Datenspeicher).
fn karte_aufgabe(a: &Aufgabe, slug: &str) -> BacklogKarte {
    let (zustand, hinweis) = match a.zustand {
        AufgabenZustand::Erledigt => ("erledigt", a.antwort.as_ref().map(|x| format!("Antwort: „{x}“"))),
        AufgabenZustand::Zurueckgezogen => ("zurueckgezogen", Some("zurückgezogen".to_string())),
        AufgabenZustand::Abgelaufen => ("abgelaufen", Some("abgelaufen".to_string())),
        AufgabenZustand::Offen => ("offen", None),
    };
    BacklogKarte {
        art: EintragsArt::Aufgabe,
        id: a.id.clone(),
        titel: a.titel.clone(),
        zustand: zustand.to_string(),
        wartet_auf: None,
        frist: a.frist.clone(),
        frist_stufe: None,
        hinweis,
        ziel: crate::anstehend::ziel(slug, EintragsArt::Aufgabe, &a.id),
        geaendert: a.geaendert.clone(),
    }
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
/// `kanban_datei`: Das Projekt hat noch `daten/kanban.toml`.
pub fn backlog(inhalt: &Inhalt, slug: &str, kanban_datei: bool) -> Option<Backlog> {
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
            in_spalte(art, karte(e, art));
        }
    }
    // Fertig: Arbeiten (außerhalb des Archivs) und abgeschlossene Aufgaben, zuletzt geänderte zuerst.
    let mut fertig: Vec<BacklogKarte> = inhalt
        .speicher
        .arbeiten
        .iter()
        .filter(|a| a.projekt == slug && a.stand == ArbeitsStand::Fertig)
        .map(|a| karte_arbeit(a, slug))
        .chain(inhalt.speicher.aufgaben_abgeschlossen.iter().filter(|a| a.projekt == slug).map(|a| karte_aufgabe(a, slug)))
        .collect();
    fertig.sort_by(|a, b| b.geaendert.cmp(&a.geaendert).then_with(|| b.id.cmp(&a.id)));
    for k in fertig {
        in_spalte(BacklogSpaltenArt::Fertig, k);
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
    use crate::vertrag::api::{Einstufung, Stufe};
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
                aufgaben_abgeschlossen: vec![
                    aufgabe("a-20261101-0001", AufgabenZustand::Erledigt, Some("ja"), "2026-11-01T12:00:00+01:00"),
                    aufgabe("a-20261101-0002", AufgabenZustand::Zurueckgezogen, None, "2026-11-02T12:00:00+01:00"),
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
        let b = backlog(&inhalt(), "beispiel", true).unwrap();
        let arten: Vec<BacklogSpaltenArt> = b.spalten.iter().map(|s| s.art).collect();
        assert_eq!(arten, BacklogSpaltenArt::ALLE, "alle fünf, auch leere, in fester Folge");
        let s = |a: BacklogSpaltenArt| b.spalten.iter().find(|s| s.art == a).unwrap();
        // Gleiche Stufe: nach Alter (die Frage ist am ältesten, Arbeiten um 8, Aufgaben um 9 Uhr).
        assert_eq!(ids(s(BacklogSpaltenArt::FuerDich)), ["farbe", "ar-20261102-0005", "a-20261102-0001"]);
        assert_eq!(ids(s(BacklogSpaltenArt::Laeuft)), ["ar-20261102-0001"]);
        assert_eq!(ids(s(BacklogSpaltenArt::Wartet)), ["ar-20261102-0002", "ar-20261102-0003", "ar-20261102-0004", "ar-20261102-0008", "a-20261102-0002"]);
        assert_eq!(ids(s(BacklogSpaltenArt::AlsNaechstes)), ["ar-20261102-0006"]);
        assert_eq!(ids(s(BacklogSpaltenArt::Fertig)), ["a-20261101-0002", "ar-20261102-0007", "a-20261101-0001"], "zuletzt geändert zuerst");
        let frage = &s(BacklogSpaltenArt::FuerDich).karten[0];
        assert_eq!(frage.hinweis.as_deref(), Some("noch 2 Fragen warten"));
        assert_eq!(frage.ziel, "/projects/beispiel#frage-farbe");
        assert!(!b.spalten.iter().flat_map(|s| &s.karten).any(|k| k.id == "name" || k.id == "termin"), "wartende Fragen ohne Text");
        assert_eq!(b.fragen_weitere, 2);
        assert!(b.kanban_datei);
        let erledigt = s(BacklogSpaltenArt::Fertig).karten.iter().find(|k| k.id == "a-20261101-0001").unwrap();
        assert_eq!((erledigt.zustand.as_str(), erledigt.hinweis.as_deref()), ("erledigt", Some("Antwort: „ja“")));
        let beim_chat = &s(BacklogSpaltenArt::Wartet).karten[4];
        assert_eq!((beim_chat.zustand.as_str(), beim_chat.wartet_auf.as_deref()), ("beim-chat", Some("chat:beispiel")));
        // Ein anderes Projekt ist leer, ein unbekanntes `None`.
        assert!(backlog(&inhalt(), "anderes", false).unwrap().spalten.iter().all(|s| s.karten.is_empty()));
        assert!(backlog(&inhalt(), "gibt-es-nicht", false).is_none());
    }

    /// Wartende je Verantwortlichem: Inhaber zuerst (auch die Frage und die
    /// Arbeit mit Verweis), dann die Chats nach Name, dann extern.
    #[test]
    fn wartende_je_verantwortlichem() {
        let b = backlog(&inhalt(), "beispiel", false).unwrap();
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
        let b = backlog(&i, "beispiel", false).unwrap();
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
        let b = backlog(&i, "beispiel", false).unwrap();
        assert!(b.spalten[0].karten.iter().any(|k| k.id == "termin"));
        assert!(!b.spalten[0].karten.iter().any(|k| k.id == "name"));
    }

    /// Ohne Datenspeicher: nur die Frage aus der Datei, `speicher_fehlt`.
    #[test]
    fn ohne_datenspeicher() {
        let mut i = inhalt();
        i.speicher = AusDemSpeicher::default();
        i.speicher_fehlt = true;
        let b = backlog(&i, "beispiel", false).unwrap();
        assert!(b.speicher_fehlt);
        let alle: Vec<&str> = b.spalten.iter().flat_map(|s| s.karten.iter().map(|k| k.id.as_str())).collect();
        assert_eq!(alle, ["farbe"]);
    }
}
