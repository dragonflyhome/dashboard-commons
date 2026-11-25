//! Das Modul Fristen (Etappe B3, Entscheidung gemeinsame-bausteine,
//! Wunsch): Fristen aus einer Quelle je Art zusammenführen und
//! bewegliche Fristen auflösen. Die Datenformen stehen in `vertrag::fristen`.
//!
//! Quellen einer Frist (Klärung der Architektur: „Nächste Frist nur aus
//! Aufgaben und Ereignissen, nicht aus zeitstrahl.toml“):
//!
//! - offene Aufgaben (Datenspeicher), fest (`frist`) oder beweglich (`frist_bezug`),
//! - die sichtbare Frage und offene Arbeiten mit `frist` (wie bisher in Anstehend),
//! - `daten/fristen.toml` des Projekts: Fristen fest oder beweglich, solange
//!   sie nicht `erledigt` sind.
//!
//! `zeitstrahl.toml` bleibt Plan und Rückblick und zählt hier nie.
//!
//! Der Server liest und rechnet nur. Ein Ereignis tritt ein, indem der Chat
//! sein `datum` in `fristen.toml` setzt und committet; der Server schreibt
//! weder dorthin noch in den Datenspeicher.

use chrono::NaiveDate;

use crate::anstehend::{FRIST_BALD_TAGE, Inhalt, alle_eintraege, archivierte, eine_frage_je_projekt, frist_stufe, tage_bis};
use crate::inhalt::store::Bestand;
use crate::kalender::{Art, Katalog, OsterRechnung, Regel};
use crate::vertrag::api::{AnstehendEintrag, Aufgabe, AufgabeAnsicht, AufgabeVerlaufArt, EintragsArt};
use crate::vertrag::daten::{Baustein, Datei, DatenInhalt};
use crate::vertrag::fristen::{Abstand, Einheit, EreignisStand, FristBezug, FristQuelle, FristZeile, Fristen, FristenListe, aufloesen};

/// `daten/fristen.toml` eines Projekts, wie gelesen und geprüft.
#[derive(Debug, Clone, PartialEq)]
pub struct FristenDatei {
    pub projekt: String,
    /// `projects/<slug>/daten/fristen.toml`.
    pub datei: String,
    pub inhalt: Datei<Fristen>,
}

/// Datum als JJJJ-MM-TT.
fn text(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

/// Ab welchem Tag die bewegliche Frist einer Aufgabe ihr Vorkommen sucht:
/// der Tag, an dem sie zuletzt gesetzt wurde (Verlauf, Art `frist`), sonst
/// der Anlegetag. Eine Aufgabe gilt für ein Vorkommen eines Regel-Ereignisses:
/// Versäumt, rückt sie nicht weiter, sondern wird überfällig.
pub fn bezug_ab(a: &Aufgabe, heute: NaiveDate) -> NaiveDate {
    let zeit = a.verlauf.iter().rev().find(|v| v.art == AufgabeVerlaufArt::Frist).map_or(a.erstellt.as_str(), |v| v.zeit.as_str());
    zeit.get(..10).and_then(|t| NaiveDate::parse_from_str(t, "%Y-%m-%d").ok()).unwrap_or(heute)
}

/// Eine bewegliche Frist eines Projekts auflösen: wie es um ihr Ereignis
/// steht (aus `fristen.toml` des Projekts) und ihr Datum, sobald das Ereignis
/// eines hat. Ohne Datei oder mit unbekanntem Ereignis hängt sie (Titel `null`).
/// `ab`: bei einem Ereignis mit Regel das früheste Vorkommen ab hier ([`bezug_ab`]).
pub fn bezug_aufloesen(dateien: &[FristenDatei], projekt: &str, bezug: &FristBezug, ab: NaiveDate) -> (EreignisStand, Option<NaiveDate>) {
    let stand = match dateien.iter().find(|d| d.projekt == projekt) {
        Some(d) => d.inhalt.ereignis_stand(&bezug.ereignis, &bezug.abstand, ab),
        None => EreignisStand { kennung: bezug.ereignis.clone(), titel: None, datum: None, herkunft: None, zeitraum: None },
    };
    let datum = aufloesen(stand.datum.as_deref(), &bezug.abstand);
    (stand, datum)
}

/// Die Ereignisse eines Projekts aus dem gespeicherten Stand des Inhalts;
/// ohne Datei (oder unlesbar ohne letzten guten Stand) `None`.
pub fn aus_bestand(b: &Bestand, projekt: &str) -> Option<Datei<Fristen>> {
    match b.daten(projekt, Fristen::DATEI) {
        Ok(Some((DatenInhalt::Fristen(d), _))) => Some(d),
        _ => None,
    }
}

/// Etappe B3, `/api/aufgaben`: In der Ansicht einer Aufgabe mit beweglicher
/// Frist steht in `frist` das aufgelöste Datum (sonst `null`), dazu
/// `frist_stufe` und `ereignis`. Gespeichert bleibt nur der Bezug.
pub fn ansichten_aufloesen(b: &Bestand, ansichten: &mut [AufgabeAnsicht], heute: NaiveDate) {
    for a in ansichten.iter_mut() {
        let Some(bezug) = a.aufgabe.frist_bezug.clone() else { continue };
        let stand = match aus_bestand(b, &a.aufgabe.projekt) {
            Some(d) => d.ereignis_stand(&bezug.ereignis, &bezug.abstand, bezug_ab(&a.aufgabe, heute)),
            None => EreignisStand { kennung: bezug.ereignis.clone(), titel: None, datum: None, herkunft: None, zeitraum: None },
        };
        let datum = aufloesen(stand.datum.as_deref(), &bezug.abstand);
        a.aufgabe.frist = datum.map(text);
        a.frist_stufe = datum.map(|d| frist_stufe(d, heute));
        a.ereignis = Some(stand);
    }
}

/// Die offenen (nicht erledigten) Fristen einer `fristen.toml` als Zeilen,
/// in der Reihenfolge der Datei; bewegliche aufgelöst, sobald ihr Ereignis ein Datum hat.
pub fn datei_zeilen(inhalt: &Inhalt, d: &FristenDatei) -> Vec<FristZeile> {
    let projekt_titel = inhalt.projekte.iter().find(|p| p.slug == d.projekt).map(|p| p.titel.clone());
    let mut aus = Vec::new();
    for f in d.inhalt.inhalte().into_iter().flat_map(|t| &t.frist).filter(|f| f.erledigt.is_none()) {
        let (frist_bezug, ereignis, datum) = match (&f.ereignis, &f.abstand) {
            (Some(e), Some(a)) => {
                let bezug = FristBezug { ereignis: e.clone(), abstand: a.clone() };
                let stand = d.inhalt.ereignis_stand(e, a, inhalt.heute);
                let datum = aufloesen(stand.datum.as_deref(), a);
                (Some(bezug), Some(stand), datum)
            }
            _ => (None, None, f.datum.as_deref().and_then(|t| NaiveDate::parse_from_str(t, "%Y-%m-%d").ok())),
        };
        aus.push(FristZeile {
            quelle: FristQuelle::Datei,
            id: f.kennung.clone(),
            projekt: d.projekt.clone(),
            projekt_titel: projekt_titel.clone(),
            titel: f.titel.clone(),
            frist: datum.map(text),
            tage_bis: datum.map(|x| tage_bis(x, inhalt.heute)),
            frist_stufe: datum.map(|x| frist_stufe(x, inhalt.heute)),
            frist_bezug,
            ereignis,
            hinweis: f.hinweis.clone(),
            ziel: format!("/projects/{}#frist-{}", d.projekt, f.kennung),
        });
    }
    aus
}

/// Höchstens so viele Tage voraus stehen Feste in der Fristenliste aller
/// Projekte („Alle Fristen“ in Anstehend); auf der Seite des Projekts alle.
pub const FEST_VORSCHAU_TAGE: i64 = 30;

/// Kalender: je Fest der Kalender in `kalender = [...]` einer
/// `fristen.toml` eine Zeile mit dem nächsten Vorkommen ab heute und seiner
/// Herkunft; ein Mondmonat mit seinem Zeitraum (als Tag zählt sein Anfang).
/// Je Zeile, nach welcher Rechnung sie ein Ostern ist (für [`ostern_zusammenlegen`]).
/// Feste sind keine Fristen: ohne Stufe, nie überfällig, nicht in Ampel und
/// nächster Frist. Dazu, ob ein Fest kein Datum mehr hat (nach 2100).
pub fn fest_zeilen(katalog: &Katalog, inhalt: &Inhalt, d: &FristenDatei) -> (Vec<(FristZeile, Option<OsterRechnung>)>, bool) {
    let projekt_titel = inhalt.projekte.iter().find(|p| p.slug == d.projekt).map(|p| p.titel.clone());
    let ab_heute = Abstand { zahl: 0, einheit: Einheit::Tag };
    let mut aus = Vec::new();
    let mut fehlt = false;
    for k in d.inhalt.inhalte().into_iter().flat_map(|t| &t.kalender).filter_map(|n| katalog.kalender().iter().find(|k| &k.kennung == n)) {
        for f in &k.fest {
            let kennung = format!("{}.{}", k.kennung, f.kennung);
            let regel = Regel { fest: Some(kennung.clone()), ..Default::default() };
            let Some((tag, herkunft)) = katalog.naechstes(None, &regel, ab_heute, inhalt.heute) else {
                fehlt = true;
                continue;
            };
            let ostern = match f.regel.art() {
                Ok(Art::Ostern(r)) => Some(r),
                _ => None,
            };
            let zeitraum = katalog.zeitraum(None, &regel, tag);
            aus.push((
                FristZeile {
                    quelle: FristQuelle::Kalender,
                    id: kennung.clone(),
                    projekt: d.projekt.clone(),
                    projekt_titel: projekt_titel.clone(),
                    titel: f.titel.clone(),
                    frist: Some(text(tag)),
                    tage_bis: Some(tage_bis(tag, inhalt.heute)),
                    frist_stufe: None,
                    frist_bezug: None,
                    ereignis: Some(EreignisStand { kennung, titel: Some(k.titel.clone()), datum: Some(text(tag)), herkunft: Some(herkunft), zeitraum }),
                    hinweis: None,
                    ziel: format!("/projects/{}", d.projekt),
                },
                ostern,
            ));
        }
    }
    (aus, fehlt)
}

/// Festzeilen zusammenführen: jedes Fest einmal (die erste Zeile gilt), und fällt
/// Ostern nach dem Mond auf denselben Sonntag wie das westliche oder ein zweites
/// Ostern nach dem Mond, steht eine Zeile mit allen Herkünften, das westliche
/// vorn. Das orthodoxe Ostern bleibt immer eine eigene Zeile. Für die Liste eines
/// Projekts und, über alle Projekte, für „Alle Fristen“.
pub fn ostern_zusammenlegen(zeilen: Vec<(FristZeile, Option<OsterRechnung>)>) -> Vec<FristZeile> {
    let mut gesehen = std::collections::BTreeSet::new();
    let zeilen: Vec<_> = zeilen.into_iter().filter(|(z, _)| gesehen.insert(z.id.clone())).collect();
    let mit_mond: std::collections::BTreeSet<Option<String>> =
        zeilen.iter().filter(|(_, o)| *o == Some(OsterRechnung::Astronomisch)).map(|(z, _)| z.frist.clone()).collect();
    let mut aus: Vec<FristZeile> = Vec::new();
    // Je Tag die Osterzeilen, die zu einer zusammengelegt werden.
    type Osterzeile = (Option<OsterRechnung>, FristZeile);
    let mut gruppen: Vec<(Option<String>, Vec<Osterzeile>)> = Vec::new();
    for (z, o) in zeilen {
        let zusammen = matches!(o, Some(OsterRechnung::Westlich | OsterRechnung::Astronomisch)) && mit_mond.contains(&z.frist);
        if !zusammen {
            aus.push(z);
        } else if let Some((_, g)) = gruppen.iter_mut().find(|(t, _)| *t == z.frist) {
            g.push((o, z));
        } else {
            gruppen.push((z.frist.clone(), vec![(o, z)]));
        }
    }
    for (_, mut g) in gruppen {
        // Feste Rangfolge für den Kopf: das westliche Ostern, dann eines des Kirchenjahrs, dann nach
        // Kennung; so hängt nichts an der Reihenfolge der Projekte oder Kalender.
        g.sort_by_key(|(o, z)| (*o != Some(OsterRechnung::Westlich), !z.id.starts_with("kirche."), z.id.clone()));
        let mut teile = g.into_iter().map(|(_, z)| z);
        let Some(mut kopf) = teile.next() else { continue };
        for z in teile {
            if let (Some(e), Some(h)) = (kopf.ereignis.as_mut(), z.ereignis.and_then(|e| e.herkunft)) {
                e.herkunft = Some(format!("{} · zugleich {h}", e.herkunft.take().unwrap_or_default()));
            }
        }
        aus.push(kopf);
    }
    aus
}
/// Satz, wenn Feste kein Datum mehr haben (nichts geraten, kein Fehler).
pub const SATZ_FESTE_GRENZE: &str = "Feste rechnet das Dashboard für die Jahre 1912 bis 2100; spätere Termine fehlen in dieser Liste.";

/// Die offenen Fristen aus `fristen.toml` eines Projekts mit Datum (fest oder
/// aufgelöst), nach Datum: für die nächste Frist, die Ampel und das Unter-Dashboard.
pub fn datei_fristen_mit_datum(inhalt: &Inhalt, slug: &str) -> Vec<FristZeile> {
    let mut z: Vec<FristZeile> =
        inhalt.fristen.iter().filter(|d| d.projekt == slug).flat_map(|d| datei_zeilen(inhalt, d)).filter(|z| z.frist.is_some()).collect();
    ordnen(&mut z);
    z
}

/// Eine Zeile aus einem offenen Eintrag (Aufgabe, sichtbare Frage, Arbeit).
fn zeile_aus_eintrag(e: &AnstehendEintrag) -> Option<FristZeile> {
    let quelle = match e.art {
        EintragsArt::Aufgabe => FristQuelle::Aufgabe,
        EintragsArt::Frage => FristQuelle::Frage,
        EintragsArt::Arbeit => FristQuelle::Arbeit,
        EintragsArt::Meldung => return None,
    };
    if e.frist.is_none() && e.frist_bezug.is_none() {
        return None;
    }
    Some(FristZeile {
        quelle,
        id: e.id.clone(),
        projekt: e.projekt.clone(),
        projekt_titel: e.projekt_titel.clone(),
        titel: e.titel.clone(),
        frist: e.frist.clone(),
        tage_bis: e.tage_bis,
        frist_stufe: e.frist_stufe,
        frist_bezug: e.frist_bezug.clone(),
        ereignis: e.ereignis.clone(),
        hinweis: None,
        ziel: e.ziel.clone(),
    })
}

/// Die feste Ordnung der Fristenliste: mit Datum nach Datum (vorbei zuerst),
/// dann die beweglichen ohne Datum; darin nach Projekt, Quelle und Kennung.
pub fn ordnen(zeilen: &mut [FristZeile]) {
    zeilen.sort_by(|a, b| (a.frist.is_none(), &a.frist, &a.projekt, a.quelle, &a.id).cmp(&(b.frist.is_none(), &b.frist, &b.projekt, b.quelle, &b.id)));
}

/// Die Fristenliste: mit `projekt` die eines Projekts (auch eines
/// archivierten, auf seiner eigenen Seite), sonst die aller Projekte außer
/// den archivierten (für die gilt die Sicherheitsregel in Anstehend,
/// `anstehend::archiv_fristen`). Von den Fragen zählt je Projekt nur die
/// sichtbare (Warteschlange); Meldungen haben keine Frist.
pub fn fristenliste(inhalt: &Inhalt, projekt: Option<&str>) -> FristenListe {
    fristenliste_mit(Katalog::eingebaut(), inhalt, projekt)
}

/// [`fristenliste`] mit einem gegebenen Katalog (Tests rechnen so mit fester Ortszeit).
// shortcut: Feste werden je Abruf neu gerechnet (je Projekt mit Kalender einige zehntausend
// Winkelrechnungen); aufrüsten mit einem Zwischenspeicher je Kalender und Tag, wenn Abrufe spürbar langsam werden.
pub fn fristenliste_mit(katalog: &Katalog, inhalt: &Inhalt, projekt: Option<&str>) -> FristenListe {
    let archiviert = archivierte(inhalt);
    let gilt = |p: &str| match projekt {
        Some(s) => s == p,
        None => !archiviert.contains(p),
    };
    let alle = alle_eintraege(inhalt);
    let mut zeilen: Vec<FristZeile> = eine_frage_je_projekt(&alle).into_iter().filter(|e| gilt(&e.projekt)).filter_map(zeile_aus_eintrag).collect();
    let mut feste_fehlen = false;
    let mut feste = Vec::new();
    for d in inhalt.fristen.iter().filter(|d| gilt(&d.projekt)) {
        zeilen.extend(datei_zeilen(inhalt, d));
        let (f, fehlt) = fest_zeilen(katalog, inhalt, d);
        feste_fehlen |= fehlt;
        // Übergreifend nur die nahen Feste, sonst stünde dort das ganze Jahr.
        feste.extend(f.into_iter().filter(|(z, _)| projekt.is_some() || z.tage_bis.is_some_and(|t| t <= FEST_VORSCHAU_TAGE)));
    }
    // Jedes Fest einmal, auch wenn mehrere Projekte seinen Kalender nennen; Ostern über alle zusammen.
    zeilen.extend(ostern_zusammenlegen(feste));
    ordnen(&mut zeilen);
    let mut hinweise: Vec<String> = inhalt
        .hinweise
        .iter()
        .filter(|h| h.contains("/daten/fristen.toml") && inhalt.projekte.iter().any(|p| gilt(&p.slug) && h.contains(&format!("projects/{}/", p.slug))))
        .cloned()
        .collect();
    if feste_fehlen && projekt.is_some() {
        hinweise.push(SATZ_FESTE_GRENZE.to_string());
    }
    // Der laufende Mond nur auf der Seite eines Projekts, das einen Kalender mit Jahreszeiten zeigt.
    let mond = projekt.and_then(|p| {
        inhalt
            .fristen
            .iter()
            .filter(|d| d.projekt == p)
            .flat_map(|d| d.inhalt.inhalte().into_iter().flat_map(|t| t.kalender.iter()))
            .find_map(|k| katalog.mond(k, inhalt.heute))
    });
    FristenListe { heute: text(inhalt.heute), frist_bald_tage: FRIST_BALD_TAGE, zeilen, hinweise, speicher_fehlt: inhalt.speicher_fehlt, mond }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::anstehend::{ARCHIV_FRIST_HAENGT, Filter, ProjektKopf, anstehend, stand};
    use crate::speicher::arbeiten::AusDemSpeicher;
    use crate::vertrag::api::{AmpelGrund, Aufgabe, AufgabenZustand, FristStufe, StandAmpel};
    use crate::vertrag::daten::ProjektDaten;

    const HEUTE: &str = "2026-11-04";

    fn tag(t: &str) -> NaiveDate {
        NaiveDate::parse_from_str(t, "%Y-%m-%d").unwrap()
    }

    fn kopf(slug: &str, archiviert: bool) -> ProjektKopf {
        ProjektKopf {
            slug: slug.into(),
            titel: format!("Projekt {slug}"),
            status: if archiviert { "archiviert" } else { "aktiv" }.into(),
            notizen: Vec::new(),
            zuletzt: "2026-11-01 10:00".into(),
            archiviert,
        }
    }

    /// Eine `fristen.toml`, gelesen wie im Dienst (mit allen Prüfungen).
    fn datei(projekt: &str, toml: &str) -> FristenDatei {
        let pfad = format!("projects/{projekt}/daten/fristen.toml");
        let mut d = ProjektDaten::default();
        d.datei_einlesen("fristen.toml", &pfad, toml).unwrap();
        FristenDatei { projekt: projekt.into(), datei: pfad, inhalt: d.fristen.unwrap() }
    }

    fn aufgabe(projekt: &str, id: &str, frist: Option<&str>, bezug: Option<(&str, &str)>) -> Aufgabe {
        Aufgabe {
            id: id.into(),
            projekt: projekt.into(),
            von: format!("chat:{projekt}"),
            titel: format!("Aufgabe {id}"),
            knopf: None,
            frist: frist.map(str::to_string),
            ablauf: None,
            zustand: AufgabenZustand::Offen,
            antwort: None,
            eigene: None,
            erstellt: "2026-11-01T09:00:00+01:00".into(),
            geaendert: "2026-11-01T09:00:00+01:00".into(),
            hat_link: false,
            vorgang: None,
            prioritaet: None,
            verlauf: Vec::new(),
            frist_bezug: bezug.map(|(e, a)| FristBezug { ereignis: e.into(), abstand: a.into() }),
        }
    }

    /// Erfundene Beispiele: ein Vereinsfest mit Zusage (eingetreten) und Aufbau (noch offen).
    const BEISPIEL: &str = r#"
[[ereignis]]
kennung = "zusage-da"
titel = "Zusage ist da"
datum = "2026-10-31"

[[ereignis]]
kennung = "aufbau"
titel = "Termin für den Aufbau"

[[frist]]
kennung = "standanmeldung"
titel = "Stand anmelden"
ereignis = "zusage-da"
abstand = "+1 Monat"

[[frist]]
kennung = "plakate"
titel = "Plakate bestellen"
datum = "2026-11-10"

[[frist]]
kennung = "helferliste"
titel = "Helferliste vorher verschicken"
ereignis = "aufbau"
abstand = "-3 Tage"

[[frist]]
kennung = "alt"
titel = "Schon gewahrt"
datum = "2026-10-01"
erledigt = "2026-09-30"
"#;

    fn inhalt(aufgaben: Vec<Aufgabe>, fristen: Vec<FristenDatei>, projekte: Vec<ProjektKopf>) -> Inhalt {
        Inhalt { heute: tag(HEUTE), projekte, fristen, speicher: AusDemSpeicher { aufgaben, ..Default::default() }, ..Default::default() }
    }

    /// Die Fristenliste: nach Datum, bewegliche ohne Datum am Ende, erledigte fehlen;
    /// 31.10. + 1 Monat endet am 30.11.
    #[test]
    fn liste_nach_datum_bewegliche_am_ende() {
        let i = inhalt(
            vec![aufgabe("fest", "a-20261104-0001", Some("2026-11-20"), None), aufgabe("fest", "a-20261104-0002", None, Some(("aufbau", "+14 Tage")))],
            vec![datei("fest", BEISPIEL)],
            vec![kopf("fest", false)],
        );
        let l = fristenliste(&i, Some("fest"));
        let ids: Vec<(&str, Option<&str>)> = l.zeilen.iter().map(|z| (z.id.as_str(), z.frist.as_deref())).collect();
        assert_eq!(
            ids,
            [
                ("plakate", Some("2026-11-10")),
                ("a-20261104-0001", Some("2026-11-20")),
                ("standanmeldung", Some("2026-11-30")),
                ("helferliste", None),
                ("a-20261104-0002", None),
            ]
        );
        let w = &l.zeilen[2];
        assert_eq!((w.quelle, w.tage_bis, w.frist_stufe), (FristQuelle::Datei, Some(26), Some(FristStufe::Spaeter)));
        assert_eq!(w.ereignis.as_ref().and_then(|e| e.titel.as_deref()), Some("Zusage ist da"));
        assert_eq!(w.ziel, "/projects/fest#frist-standanmeldung");
        let haengt = &l.zeilen[3];
        assert_eq!(
            (haengt.ereignis.as_ref().unwrap().titel.as_deref(), haengt.ereignis.as_ref().unwrap().datum.as_deref()),
            (Some("Termin für den Aufbau"), None)
        );
        assert_eq!(l.zeilen[4].quelle, FristQuelle::Aufgabe);
        assert_eq!(l.zeilen[4].ziel, "/projects/fest#aufgabe-a-20261104-0002");
    }

    /// Sobald das Ereignis ein Datum hat, hat auch die bewegliche Frist eines
    /// (an der Aufgabe und in der Datei); vorher hebt sie keine Stufe.
    #[test]
    fn aufloesen_sobald_das_ereignis_eintritt() {
        let vorher = BEISPIEL.to_string();
        let nachher = BEISPIEL.replace("titel = \"Termin für den Aufbau\"\n", "titel = \"Termin für den Aufbau\"\ndatum = \"2026-11-02\"\n");
        let a = || vec![aufgabe("fest", "a-20261104-0002", None, Some(("aufbau", "+3 Tage")))];
        let i = inhalt(a(), vec![datei("fest", &vorher)], vec![kopf("fest", false)]);
        let e = &alle_eintraege(&i)[0];
        assert_eq!((e.frist.as_deref(), e.frist_stufe, e.frist_bezug.is_some()), (None, None, true));
        let i = inhalt(a(), vec![datei("fest", &nachher)], vec![kopf("fest", false)]);
        let e = &alle_eintraege(&i)[0];
        assert_eq!((e.frist.as_deref(), e.tage_bis, e.frist_stufe), (Some("2026-11-05"), Some(1), Some(FristStufe::Bald)));
        assert_eq!(e.ereignis.as_ref().and_then(|x| x.datum.as_deref()), Some("2026-11-02"));
        let l = fristenliste(&i, Some("fest"));
        let s = l.zeilen.iter().find(|z| z.id == "helferliste").unwrap();
        assert_eq!(s.frist.as_deref(), Some("2026-10-29"), "2.11. − 3 volle Tage: 29.10. (Auflage B3)");
        // Ohne fristen.toml im Projekt hängt die Aufgabe, mit unbekanntem Ereignis.
        let i = inhalt(a(), Vec::new(), vec![kopf("fest", false)]);
        let e = &alle_eintraege(&i)[0];
        assert_eq!((e.frist.as_deref(), e.ereignis.as_ref().and_then(|x| x.titel.clone())), (None, None));
    }

    /// Nächste Frist und Ampel: aus Aufgaben und fristen.toml, früheste zuerst.
    #[test]
    fn naechste_frist_und_ampel() {
        let i = inhalt(vec![aufgabe("fest", "a-20261104-0001", Some("2026-11-20"), None)], vec![datei("fest", BEISPIEL)], vec![kopf("fest", false)]);
        let voll = stand(&i, "fest").unwrap();
        let ids: Vec<&str> = voll.fristen.iter().map(|z| z.id.as_str()).collect();
        assert_eq!(ids, ["plakate", "standanmeldung"], "nur mit Datum, nach Datum");
        assert_eq!((voll.fristen[0].frist.as_deref(), voll.fristen[0].tage_bis), (Some("2026-11-10"), Some(6)), "die Datei-Frist ist früher");
        assert_eq!(voll.eintraege[0].frist.as_deref(), Some("2026-11-20"));
        assert_eq!(voll.gesamt.ampel, StandAmpel::Gelb);
        // Eine vorbei: Ampel rot, ohne Art, mit Titel.
        let vorbei = BEISPIEL.replace("datum = \"2026-11-10\"", "datum = \"2026-11-03\"");
        let i = inhalt(Vec::new(), vec![datei("fest", &vorbei)], vec![kopf("fest", false)]);
        let s = stand(&i, "fest").unwrap();
        assert_eq!(
            (s.gesamt.ampel, s.gesamt.grund, s.gesamt.bezug_art, s.gesamt.bezug.as_deref()),
            (StandAmpel::Rot, AmpelGrund::FristVorbei, None, Some("Plakate bestellen"))
        );
        // Erledigt zählt nicht.
        let erledigt = vorbei.replace("datum = \"2026-11-03\"", "datum = \"2026-11-03\"\nerledigt = \"2026-11-02\"");
        let i = inhalt(Vec::new(), vec![datei("fest", &erledigt)], vec![kopf("fest", false)]);
        let s = stand(&i, "fest").unwrap();
        assert_eq!((s.gesamt.ampel, s.fristen.first().and_then(|z| z.frist.as_deref())), (StandAmpel::Gruen, Some("2026-11-30")));
    }

    /// Kalender, Stufe 1: ein Ereignis mit Regel und die Feste aktiver Kalender.
    const MIT_KALENDER: &str = r#"
kalender = ["kirche", "chinesisch"]

[[ereignis]]
kennung = "neujahr"
titel = "Neujahrsfest der Familie"
regel = { fest = "chinesisch.neujahr" }

[[frist]]
kennung = "einkauf"
titel = "Einkauf fürs Neujahrsfest"
ereignis = "neujahr"
abstand = "-3 Tage"
"#;

    #[test]
    fn regel_ereignis_und_feste() {
        // Feste Ortszeit (Mitteleuropa), nicht die des Rechners.
        let berlin = Katalog::berlin();
        let i = inhalt(Vec::new(), vec![datei("kal", MIT_KALENDER)], vec![kopf("kal", false)]);
        let l = fristenliste_mit(&berlin, &i, Some("kal"));
        let einkauf = l.zeilen.iter().find(|z| z.id == "einkauf").unwrap();
        // Neujahr 6.2.2027, drei volle Tage vorher: 2.2.2027.
        assert_eq!((einkauf.frist.as_deref(), einkauf.quelle), (Some("2027-02-02"), FristQuelle::Datei));
        let e = einkauf.ereignis.as_ref().unwrap();
        assert_eq!(e.datum.as_deref(), Some("2027-02-06"));
        assert_eq!(e.herkunft.as_deref(), Some("Chinesischer Kalender · Chinesisches Neujahr · 1. Tag des 1. Monats im chinesischen Kalender"));
        let feste: Vec<(&str, &str)> =
            l.zeilen.iter().filter(|z| z.quelle == FristQuelle::Kalender).map(|z| (z.id.as_str(), z.frist.as_deref().unwrap())).collect();
        assert_eq!(feste.len(), 15, "11 aus dem Kirchenjahr, 4 chinesische: {feste:?}");
        assert_eq!(feste[0], ("kirche.heiligabend", "2026-12-24"), "nach Datum, das nächste zuerst");
        assert!(feste.contains(&("chinesisch.neujahr", "2027-02-06")) && feste.contains(&("kirche.ostern", "2027-03-28")));
        let fest = l.zeilen.iter().find(|z| z.id == "kirche.pfingsten").unwrap();
        assert_eq!((fest.frist_stufe, fest.frist_bezug.as_ref(), fest.ziel.as_str()), (None, None, "/projects/kal"), "ein Fest hat keine Stufe");
        assert_eq!(fest.ereignis.as_ref().and_then(|e| e.herkunft.as_deref()), Some("Kirchenjahr · Pfingstsonntag · 49 Tage nach Ostersonntag (28.3.2027)"));
        assert!(l.hinweise.is_empty(), "{:?}", l.hinweise);
        // Feste sind keine Fristen: nicht im Verlauf des Projekts, die Frist daran schon.
        let verlauf: Vec<String> = crate::projektverlauf::aus_fristen(&l).into_iter().map(|e| e.kennung).collect();
        assert_eq!(verlauf, ["frist:datei:einkauf"]);
        // Übergreifend nur Feste der nächsten 30 Tage: am 4.11. keines, am 1.12. Heiligabend und Weihnachten.
        assert!(fristenliste_mit(&berlin, &i, None).zeilen.iter().all(|z| z.quelle != FristQuelle::Kalender));
        let dezember = Inhalt { heute: tag("2026-12-01"), ..inhalt(Vec::new(), vec![datei("kal", MIT_KALENDER)], vec![kopf("kal", false)]) };
        let nah: Vec<String> =
            fristenliste_mit(&berlin, &dezember, None).zeilen.iter().filter(|z| z.quelle == FristQuelle::Kalender).map(|z| z.id.clone()).collect();
        assert_eq!(nah, ["kirche.heiligabend", "kirche.weihnachten"]);
        // Nennen zwei Projekte denselben Kalender, steht jedes Fest übergreifend nur einmal da.
        let nur_kirche = "kalender = [\"kirche\"]\n";
        let zwei = Inhalt {
            heute: tag("2026-12-01"),
            ..inhalt(Vec::new(), vec![datei("kal", MIT_KALENDER), datei("zwei", nur_kirche)], vec![kopf("kal", false), kopf("zwei", false)])
        };
        let nah: Vec<String> =
            fristenliste_mit(&berlin, &zwei, None).zeilen.iter().filter(|z| z.quelle == FristQuelle::Kalender).map(|z| z.id.clone()).collect();
        assert_eq!(nah, ["kirche.heiligabend", "kirche.weihnachten"]);
        assert_eq!(fristenliste_mit(&berlin, &zwei, Some("zwei")).zeilen.len(), 11, "auf der Projektseite alle seine Feste");
        // Nach 2100 fehlen Feste leise, mit einem Hinweis auf der Projektseite.
        let spaet = Inhalt { heute: tag("2100-12-26"), ..inhalt(Vec::new(), vec![datei("kal", MIT_KALENDER)], vec![kopf("kal", false)]) };
        let l = fristenliste_mit(&berlin, &spaet, Some("kal"));
        assert!(l.zeilen.iter().all(|z| z.quelle != FristQuelle::Kalender));
        assert_eq!(l.hinweise, [SATZ_FESTE_GRENZE]);
        let einkauf = l.zeilen.iter().find(|z| z.id == "einkauf").unwrap();
        assert_eq!((einkauf.frist.as_deref(), einkauf.ereignis.as_ref().unwrap().herkunft.as_deref()), (None, None), "hängt, ohne zu raten");
    }

    /// Kalender, Stufe 2: Mondmonate mit Zeitraum, der laufende Mond auf der
    /// Projektseite und eine Zeile, wenn Osterregeln auf denselben Sonntag fallen.
    /// Gerechnet mit fester Ortszeit (Mitteleuropa), nicht der des Rechners.
    #[test]
    fn jahreskreis_in_der_liste() {
        let berlin = Katalog::berlin();
        let liste = |kalender: &[(&str, &str)], heute: &str, projekt: Option<&str>| {
            let dateien = kalender.iter().map(|(p, k)| datei(p, &format!("kalender = {k}\n"))).collect();
            let koepfe = kalender.iter().map(|(p, _)| kopf(p, false)).collect();
            fristenliste_mit(&berlin, &Inhalt { heute: tag(heute), ..inhalt(Vec::new(), dateien, koepfe) }, projekt)
        };
        let zeile = |l: &FristenListe, id: &str| l.zeilen.iter().find(|z| z.id == id).cloned();
        let beide = [("kal", r#"["kirche", "jahreskreis"]"#)];
        let l = liste(&beide, "2027-03-01", Some("kal"));
        let ostern = zeile(&l, "kirche.ostern").unwrap();
        assert_eq!(ostern.frist.as_deref(), Some("2027-03-28"));
        let h = ostern.ereignis.unwrap().herkunft.unwrap();
        assert!(h.starts_with("Kirchenjahr · Ostersonntag · "), "{h}");
        assert!(h.contains("zugleich Kirchenjahr · Ostersonntag nach dem Mond") && h.contains("zugleich Jahreskreis · Ostern nach dem Mond"), "{h}");
        assert!(zeile(&l, "kirche.ostern-astronomisch").is_none() && zeile(&l, "jahreskreis.ostern").is_none());
        let dreimilch = zeile(&l, "jahreskreis.dreimilch").unwrap();
        assert_eq!(dreimilch.frist.as_deref(), Some("2027-04-07"));
        let z = dreimilch.ereignis.unwrap().zeitraum.unwrap();
        assert_eq!((z.anfang.as_str(), z.vollmond.as_str(), z.ende.as_str()), ("2027-04-07", "2027-04-21", "2027-05-06"));
        assert!(zeile(&l, "jahreskreis.yule").unwrap().ereignis.unwrap().zeitraum.is_none(), "ein Sonnentag ist kein Monat");
        let m = l.mond.as_ref().unwrap();
        assert_eq!((m.nummer, m.name.as_str(), m.namen[0].name.as_str()), (3, "1. Frühlingsvollmond", "Hrēþmōnaþ"));
        assert!(liste(&beide, "2027-03-01", None).mond.is_none(), "der laufende Mond nur auf der Projektseite");
        assert!(crate::projektverlauf::aus_fristen(&l).is_empty(), "Feste und Monate nicht im Verlauf");
        // Der Kopf ist das westliche Ostern, auch wenn der Jahreskreis zuerst steht.
        let l = liste(&[("kal", r#"["jahreskreis", "kirche"]"#)], "2027-03-01", Some("kal"));
        assert!(zeile(&l, "kirche.ostern").is_some() && zeile(&l, "jahreskreis.ostern").is_none());
        // 2038 weichen die Ostern ab: zwei Zeilen; das nach dem Mond trägt beide Kalender.
        let l = liste(&beide, "2038-03-01", Some("kal"));
        assert_eq!(zeile(&l, "kirche.ostern").unwrap().frist.as_deref(), Some("2038-04-25"));
        let astro = zeile(&l, "kirche.ostern-astronomisch").unwrap();
        assert_eq!(astro.frist.as_deref(), Some("2038-03-28"));
        assert!(astro.ereignis.unwrap().herkunft.unwrap().contains("zugleich Jahreskreis"));
        // Fallen nur die zwei Ostern nach dem Mond zusammen, steht das des Kirchenjahrs vorn,
        // auch wenn der Jahreskreis zuerst genannt ist.
        let l = liste(&[("kal", r#"["jahreskreis", "kirche"]"#)], "2038-03-01", Some("kal"));
        assert!(zeile(&l, "kirche.ostern-astronomisch").is_some() && zeile(&l, "jahreskreis.ostern").is_none());
        // 16.4.2028: westlich, orthodox und nach dem Mond am selben Sonntag. Das orthodoxe
        // bleibt eine eigene Zeile; nur das nach dem Mond wird zum westlichen gelegt.
        let l = liste(&[("kal", r#"["kirche", "orthodox"]"#)], "2028-03-01", Some("kal"));
        assert_eq!(zeile(&l, "kirche.ostern").unwrap().frist.as_deref(), Some("2028-04-16"));
        assert_eq!(zeile(&l, "orthodox.ostern").unwrap().frist.as_deref(), Some("2028-04-16"));
        assert!(zeile(&l, "kirche.ostern").unwrap().ereignis.unwrap().herkunft.unwrap().contains("zugleich Kirchenjahr · Ostersonntag nach dem Mond"));
        assert!(zeile(&l, "kirche.ostern-astronomisch").is_none());
        // Übergreifend („Alle Fristen“): in beiden Reihenfolgen der Projekte und mit Projekten,
        // die nur einen der Kalender nennen, steht am 28.3.2027 genau eine Zeile, das westliche vorn.
        for projekte in [
            [("a", r#"["kirche", "jahreskreis"]"#), ("b", r#"["jahreskreis"]"#)],
            [("a", r#"["jahreskreis"]"#), ("b", r#"["kirche", "jahreskreis"]"#)],
            [("a", r#"["kirche"]"#), ("b", r#"["jahreskreis"]"#)],
            [("a", r#"["jahreskreis"]"#), ("b", r#"["kirche"]"#)],
        ] {
            let l = liste(&projekte, "2027-03-10", None);
            let am_28: Vec<&FristZeile> = l.zeilen.iter().filter(|z| z.frist.as_deref() == Some("2027-03-28")).collect();
            assert_eq!(am_28.iter().map(|z| z.id.as_str()).collect::<Vec<_>>(), ["kirche.ostern"], "{projekte:?}");
            let h = am_28[0].ereignis.as_ref().and_then(|e| e.herkunft.as_deref()).unwrap_or_default();
            assert!(h.contains("zugleich Jahreskreis · Ostern nach dem Mond"), "{projekte:?}: {h}");
        }
        // Nur der Jahreskreis: das Ostern nach dem Mond allein.
        let l = liste(&[("a", r#"["jahreskreis"]"#), ("b", r#"["jahreskreis"]"#)], "2027-03-10", None);
        let am_28: Vec<&str> = l.zeilen.iter().filter(|z| z.frist.as_deref() == Some("2027-03-28")).map(|z| z.id.as_str()).collect();
        assert_eq!(am_28, ["jahreskreis.ostern"]);
    }
    /// Abnahme Runde 1: Eine Aufgabe an einem Regel-Ereignis gilt für das
    /// Vorkommen ab dem Tag, an dem ihre Frist gesetzt wurde; versäumt wird sie
    /// überfällig (Ampel rot) statt aufs nächste Jahr zu springen.
    #[test]
    fn aufgabe_an_regel_ereignis_gilt_fuer_ein_vorkommen() {
        let mut a = aufgabe("kal", "a-20260201-0001", None, Some(("neujahr", "+0 Tage")));
        a.erstellt = "2026-02-01T09:00:00+01:00".into();
        let mit = |a: Aufgabe, heute: &str| Inhalt { heute: tag(heute), ..inhalt(vec![a], vec![datei("kal", MIT_KALENDER)], vec![kopf("kal", false)]) };
        let i = mit(a.clone(), "2026-02-20");
        let e = &alle_eintraege(&i)[0];
        assert_eq!((e.frist.as_deref(), e.frist_stufe), (Some("2026-02-17"), Some(FristStufe::Ueberfaellig)), "Neujahr 2026, versäumt");
        assert_eq!(stand(&i, "kal").unwrap().gesamt.ampel, StandAmpel::Rot);
        // Später angelegt: das nächste Neujahr.
        let mut spaet = a.clone();
        spaet.erstellt = "2026-02-18T09:00:00+01:00".into();
        assert_eq!(alle_eintraege(&mit(spaet, "2026-02-20"))[0].frist.as_deref(), Some("2027-02-06"));
        // Die Frist neu gesetzt (Verlauf Art frist): ab diesem Tag.
        let mut neu = a;
        neu.verlauf.push(crate::vertrag::api::AufgabeVerlauf {
            zeit: "2026-03-01T10:00:00+01:00".into(),
            von: "chat:kal".into(),
            art: AufgabeVerlaufArt::Frist,
            alt: None,
            neu: Some("neujahr +0 Tage".into()),
            text: "Frist gesetzt.".into(),
        });
        assert_eq!(alle_eintraege(&mit(neu, "2026-03-02"))[0].frist.as_deref(), Some("2027-02-06"));
    }

    /// Auflage der Architektur: Ampel und nächste Frist sind mit und ohne aktive Kalender gleich.
    #[test]
    fn feste_aendern_ampel_und_naechste_frist_nicht() {
        let ohne = MIT_KALENDER.replace("kalender = [\"kirche\", \"chinesisch\"]\n", "");
        for heute in ["2026-11-04", "2026-12-24", "2027-02-01", "2027-02-03"] {
            let mit_i = Inhalt { heute: tag(heute), ..inhalt(Vec::new(), vec![datei("kal", MIT_KALENDER)], vec![kopf("kal", false)]) };
            let ohne_i = Inhalt { heute: tag(heute), ..inhalt(Vec::new(), vec![datei("kal", &ohne)], vec![kopf("kal", false)]) };
            let (m, o) = (stand(&mit_i, "kal").unwrap(), stand(&ohne_i, "kal").unwrap());
            assert_eq!((m.gesamt.ampel, m.gesamt.grund, &m.fristen), (o.gesamt.ampel, o.gesamt.grund, &o.fristen), "{heute}");
            assert!(m.fristen.iter().all(|z| z.quelle != FristQuelle::Kalender));
        }
    }

    /// Die Prüfung: Regel und Datum schließen sich aus, `erledigt` an einer
    /// wiederkehrenden Frist gibt es nicht, unbekannte Kalender ohne ihren Namen im Satz.
    #[test]
    fn pruefung_der_regeln() {
        let lesen = |t: &str| {
            let mut d = ProjektDaten::default();
            d.datei_einlesen("fristen.toml", "projects/kal/daten/fristen.toml", t).map(|_| ()).map_err(|e| e.text)
        };
        assert_eq!(lesen(MIT_KALENDER), Ok(()));
        let beides = MIT_KALENDER.replace("regel = {", "datum = \"2026-01-01\"\nregel = {");
        assert!(lesen(&beides).unwrap_err().contains("`datum` und `regel`"));
        let erledigt = MIT_KALENDER.replace("abstand = \"-3 Tage\"", "abstand = \"-3 Tage\"\nerledigt = \"2026-02-12\"");
        assert!(lesen(&erledigt).unwrap_err().contains("kehrt darum wieder"));
        let unbekannt = MIT_KALENDER.replace("\"chinesisch\"]", "\"geheimbund\"]");
        let satz = lesen(&unbekannt).unwrap_err();
        assert!(satz.contains("den es nicht gibt") && !satz.contains("geheimbund"), "{satz}");
        let doppelt = MIT_KALENDER.replace("\"chinesisch\"]", "\"kirche\"]");
        assert!(lesen(&doppelt).unwrap_err().contains("zweimal"));
        let falsch = MIT_KALENDER.replace("fest = \"chinesisch.neujahr\"", "fest = \"neujahr\"");
        assert!(lesen(&falsch).unwrap_err().contains("passt die `regel` nicht"));
        // Abnahme Runde 2: die Zeile des betroffenen Kalenders, ohne ihn im Satz; auch über Teile.
        let fehler = |t: &str| ProjektDaten::default().datei_einlesen("fristen.toml", "projects/kal/daten/fristen.toml", t).unwrap_err();
        let e = fehler("# Kalender\nkalender = [\"kirche\", \"geheimbund\"]\n");
        assert_eq!(e.zeile, Some(2), "{e:?}");
        let e = fehler("[teil.a]\nkalender = [\"kirche\"]\n\n[teil.b]\nkalender = ['orthodox', 'kirche']\n");
        assert_eq!((e.zeile, e.text.contains("zweimal"), e.text.contains("kirche")), (Some(5), true, false), "{e:?}");
        // Typfehler nennen bei kalender und regel nur die Art, auch ohne Vorschlag mit Wert.
        let e = fehler("kalender = \"geheimbund\"\n");
        assert!(e.text.contains("ein Text") && !e.text.contains("geheimbund"), "{e:?}");
        let e = fehler("[[ereignis]]\nkennung = \"x\"\ntitel = \"X\"\nregel = { fest = 1234 }\n");
        assert!(e.text.contains("eine ganze Zahl") && !e.text.contains("1234") && !e.text.contains("Anführungszeichen"), "{e:?}");
    }

    /// Auflage zu B3: Lässt sich fristen.toml eines archivierten Projekts nicht
    /// lesen, steht eine neutrale Zeile in Anstehend (ohne Projekt, ohne Datum),
    /// statt dass die Fristen still verschwinden. Bei einem aktiven Projekt
    /// bleibt es beim Hinweis zur Datei.
    #[test]
    fn archiv_fristen_unlesbar() {
        use crate::anstehend::{ARCHIV_FRISTEN_UNLESBAR, AusDenDateien};
        let basis = std::env::temp_dir().join(format!("dashboard-test-{}-fristen-unlesbar", std::process::id()));
        let _ = std::fs::remove_dir_all(&basis);
        let projekte = basis.join("projects");
        for (slug, status) in [("ruhig", "archiviert"), ("laut", "aktiv")] {
            std::fs::create_dir_all(projekte.join(slug).join("daten")).unwrap();
            std::fs::write(projekte.join(slug).join("projekt.toml"), format!("schema = 3\ntitle = \"Erfunden\"\nstatus = \"{status}\"\nsummary = \"S\"\n"))
                .unwrap();
            // Ein unbekanntes Ereignis: Lesefehler, kein letzter guter Stand.
            std::fs::write(
                projekte.join(slug).join("daten").join("fristen.toml"),
                "[[frist]]\nkennung = \"geheim\"\ntitel = \"Geheimer Titel\"\nereignis = \"fehlt\"\nabstand = \"+1 Tag\"\n",
            )
            .unwrap();
        }
        let dateien = AusDenDateien::lesen(&projekte, &basis.join("data").join("schwerpunkte.toml"), tag(HEUTE));
        assert_eq!(dateien.lesefehler.fristen.iter().map(String::as_str).collect::<Vec<_>>(), ["laut", "ruhig"]);
        let i = Inhalt::aus(dateien, AusDemSpeicher::default(), tag(HEUTE));
        let l = anstehend(&i, &Filter::default());
        let _ = std::fs::remove_dir_all(&basis);
        assert_eq!(l.archiv_fristen.len(), 1, "{:?}", l.archiv_fristen);
        let a = &l.archiv_fristen[0];
        assert_eq!((a.satz.as_str(), a.frist.as_deref(), a.tage_bis, a.ziel.as_str()), (ARCHIV_FRISTEN_UNLESBAR, None, None, "/projects/ruhig"));
        // Hinweise des archivierten Projekts fehlen in Anstehend (sie nennen es), der des aktiven steht da.
        assert!(l.hinweise.iter().all(|h| !h.contains("projects/ruhig/")), "{:?}", l.hinweise);
        assert!(l.hinweise.iter().any(|h| h.contains("projects/laut/daten/fristen.toml")), "{:?}", l.hinweise);
        assert!(l.archiv_fristen.iter().all(|a| !a.satz.contains("Geheim") && !a.satz.contains("ruhig")));
    }

    /// Archiv-Regel: eine hängende bewegliche Frist steht neutral in Anstehend,
    /// je Projekt einmal und ohne Datum; aufgelöste folgen der Regel der sieben Tage.
    #[test]
    fn archiv_haengt_an_einem_ereignis() {
        let nur_haengend = r#"
[[ereignis]]
kennung = "saatgut"
titel = "Saatgut ist da"

[[frist]]
kennung = "aussaat"
titel = "Aussaat planen"
ereignis = "saatgut"
abstand = "+1 Monat"
"#;
        let i = inhalt(
            vec![aufgabe("ruhig", "a-20261104-0003", None, Some(("saatgut", "+14 Tage")))],
            vec![datei("ruhig", nur_haengend)],
            vec![kopf("ruhig", true), kopf("laut", false)],
        );
        let l = anstehend(&i, &Filter::default());
        assert!(l.eintraege.is_empty(), "nichts vom archivierten Projekt");
        assert_eq!(l.archiv_fristen.len(), 1, "je Projekt einmal: {:?}", l.archiv_fristen);
        let a = &l.archiv_fristen[0];
        assert_eq!((a.satz.as_str(), a.frist.as_deref(), a.tage_bis, a.ziel.as_str()), (ARCHIV_FRIST_HAENGT, None, None, "/projects/ruhig"));
        assert!(!a.satz.contains("ruhig") && !a.satz.contains("Saatgut") && !a.satz.contains("Aussaat"));
        // Übergreifend fehlt das archivierte Projekt in der Fristenliste.
        assert!(fristenliste(&i, None).zeilen.is_empty());
        assert_eq!(fristenliste(&i, Some("ruhig")).zeilen.len(), 2, "auf seiner Seite schon");
        // Ereignis eingetreten: in 8 Tagen still, in 7 Tagen mit Datum.
        let still = nur_haengend.replace("titel = \"Saatgut ist da\"\n", "titel = \"Saatgut ist da\"\ndatum = \"2026-10-12\"\n");
        let i = inhalt(Vec::new(), vec![datei("ruhig", &still)], vec![kopf("ruhig", true)]);
        assert!(anstehend(&i, &Filter::default()).archiv_fristen.is_empty(), "12.10. + 1 Monat = 12.11., in 8 Tagen");
        let nah = still.replace("2026-10-12", "2026-10-11");
        let i = inhalt(Vec::new(), vec![datei("ruhig", &nah)], vec![kopf("ruhig", true)]);
        let l = anstehend(&i, &Filter::default());
        assert_eq!(l.archiv_fristen.len(), 1);
        assert_eq!((l.archiv_fristen[0].frist.as_deref(), l.archiv_fristen[0].tage_bis), (Some("2026-11-11"), Some(7)));
        assert_eq!(l.archiv_fristen[0].satz, "Eine Frist in einem archivierten Projekt läuft am 11.11.2026 ab.");
    }
}
