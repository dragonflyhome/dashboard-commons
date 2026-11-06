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
use crate::vertrag::api::{AnstehendEintrag, AufgabeAnsicht, EintragsArt};
use crate::vertrag::daten::{Baustein, Datei, DatenInhalt};
use crate::vertrag::fristen::{EreignisStand, FristBezug, FristQuelle, FristZeile, Fristen, FristenListe, aufloesen};

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

/// Eine bewegliche Frist eines Projekts auflösen: wie es um ihr Ereignis
/// steht (aus `fristen.toml` des Projekts) und ihr Datum, sobald das Ereignis
/// eines hat. Ohne Datei oder mit unbekanntem Ereignis hängt sie (Titel `null`).
pub fn bezug_aufloesen(dateien: &[FristenDatei], projekt: &str, bezug: &FristBezug) -> (EreignisStand, Option<NaiveDate>) {
    let stand = match dateien.iter().find(|d| d.projekt == projekt) {
        Some(d) => d.inhalt.ereignis_stand(&bezug.ereignis),
        None => EreignisStand { kennung: bezug.ereignis.clone(), titel: None, datum: None },
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
            Some(d) => d.ereignis_stand(&bezug.ereignis),
            None => EreignisStand { kennung: bezug.ereignis.clone(), titel: None, datum: None },
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
                let stand = d.inhalt.ereignis_stand(e);
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
    let archiviert = archivierte(inhalt);
    let gilt = |p: &str| match projekt {
        Some(s) => s == p,
        None => !archiviert.contains(p),
    };
    let alle = alle_eintraege(inhalt);
    let mut zeilen: Vec<FristZeile> = eine_frage_je_projekt(&alle).into_iter().filter(|e| gilt(&e.projekt)).filter_map(zeile_aus_eintrag).collect();
    for d in inhalt.fristen.iter().filter(|d| gilt(&d.projekt)) {
        zeilen.extend(datei_zeilen(inhalt, d));
    }
    ordnen(&mut zeilen);
    let hinweise = inhalt
        .hinweise
        .iter()
        .filter(|h| h.contains("/daten/fristen.toml") && inhalt.projekte.iter().any(|p| gilt(&p.slug) && h.contains(&format!("projects/{}/", p.slug))))
        .cloned()
        .collect();
    FristenListe { heute: text(inhalt.heute), frist_bald_tage: FRIST_BALD_TAGE, zeilen, hinweise, speicher_fehlt: inhalt.speicher_fehlt }
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
        let frei = stand(&i, "fest", false).unwrap();
        let z = frei.zahlen.unwrap();
        assert_eq!((z.naechste_frist.as_deref(), z.tage_bis), (Some("2026-11-10"), Some(6)), "die Datei-Frist ist früher");
        assert!(frei.fristen.is_empty(), "ohne Anmeldung keine Titel");
        let voll = stand(&i, "fest", true).unwrap();
        let ids: Vec<&str> = voll.fristen.iter().map(|z| z.id.as_str()).collect();
        assert_eq!(ids, ["plakate", "standanmeldung"], "nur mit Datum, nach Datum");
        assert_eq!(voll.gesamt.ampel, StandAmpel::Gelb);
        // Eine vorbei: Ampel rot, ohne Art, Titel nur angemeldet.
        let vorbei = BEISPIEL.replace("datum = \"2026-11-10\"", "datum = \"2026-11-03\"");
        let i = inhalt(Vec::new(), vec![datei("fest", &vorbei)], vec![kopf("fest", false)]);
        let s = stand(&i, "fest", true).unwrap();
        assert_eq!(
            (s.gesamt.ampel, s.gesamt.grund, s.gesamt.bezug_art, s.gesamt.bezug.as_deref()),
            (StandAmpel::Rot, AmpelGrund::FristVorbei, None, Some("Plakate bestellen"))
        );
        assert_eq!(stand(&i, "fest", false).unwrap().gesamt.bezug, None);
        // Erledigt zählt nicht.
        let erledigt = vorbei.replace("datum = \"2026-11-03\"", "datum = \"2026-11-03\"\nerledigt = \"2026-11-02\"");
        let i = inhalt(Vec::new(), vec![datei("fest", &erledigt)], vec![kopf("fest", false)]);
        let s = stand(&i, "fest", false).unwrap();
        assert_eq!((s.gesamt.ampel, s.zahlen.unwrap().naechste_frist.as_deref()), (StandAmpel::Gruen, Some("2026-11-30")));
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
