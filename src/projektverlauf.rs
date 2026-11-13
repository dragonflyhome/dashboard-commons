//! Die Quellen im Verlauf eines Projekts (Etappe B8,
//! `GET /api/projekte/<slug>/verlauf`): Arbeiten, Schritte, Antworten auf
//! Fragen, Bauten, Vorgänge, Entscheidungen und Fristen als Ereignisse des
//! Verlauf-Vertrags (`vertrag::verlauf`). Rahmen, Zeitraum und Hinweise baut
//! `api::pc::verlauf_bauen` für beide Wege; hier steht nur die Übersetzung.
//! Der Server liefert Fakten, verdichtet und sortiert nichts.

use chrono::NaiveDate;

use crate::api::pc::{aus_meldung, aus_vorgang};
use crate::api::system_seiten::projekte_der;
use crate::speicher::antworten::aus_bezug;
use crate::speicher::meldungen::schluessel;
use crate::vertrag::anstehend::{Arbeit, ArbeitsStand, Meldung};
use crate::vertrag::api::{BauZustand, MeldungsArt, Vorgang, VorgangsArt};
use crate::vertrag::daten::Datei;
use crate::vertrag::fristen::FristenListe;
use crate::vertrag::plan::{EtappenStand, Plan};
use crate::vertrag::system_seiten::EntscheidungsListe;
use crate::vertrag::verlauf::{Art, Ereignis, Genauigkeit, Quelle, datum_lesen, tagesanfang, zeit_text};

fn ereignis(kennung: String, zeit: String, titel: &str, text: Option<String>, quelle: Quelle, art: Art, verweis: String) -> Ereignis {
    Ereignis { kennung, zeit, genauigkeit: Genauigkeit::Minute, bis: None, titel: titel.to_string(), text, quelle, art, verweis: Some(verweis) }
}

/// Satz zu einem Fertig-Zeitpunkt ohne Eintrag im Verlauf (`plan::zeiten`).
pub const SATZ_UNGEFAEHR: &str = "Zeitpunkt ungefähr: die letzte Änderung der Arbeit.";

/// Aus allen Arbeiten (`true`: im Archiv): Beginn und Ende der Arbeiten von
/// `slug`, ihre Schritt-Einträge und die fertigen Etappen seines Plans (dort
/// zählen wie in `/plan` auch Arbeiten anderer Projekte). Der Verweis
/// `#arbeit-<id>` steht nur, wo die Seite die Arbeit zeigt: offen in der
/// Aufgabenliste, fertig oder archiviert nur im Plan (also mit Etappe).
pub fn aus_arbeiten(slug: &str, plan: Option<&Datei<Plan>>, arbeiten: &[(Arbeit, bool)]) -> Vec<Ereignis> {
    let mut liste = Vec::new();
    let p = crate::plan::plan(slug, plan, arbeiten);
    let im_plan: std::collections::BTreeSet<&str> = p.etappen.iter().flat_map(|e| &e.arbeiten).chain(&p.ohne_etappe).map(|a| a.id.as_str()).collect();
    for (a, archiviert) in arbeiten.iter().filter(|(a, _)| a.projekt == slug) {
        let in_liste = a.stand != ArbeitsStand::Fertig && !archiviert;
        let ziel = format!("/projects/{slug}#arbeit-{}", a.id);
        let z = crate::plan::zeiten(a);
        if let Some(b) = z.begonnen {
            liste.push(ereignis(format!("arbeit:{}:begonnen", a.id), b, &a.titel, None, Quelle::Arbeit, Art::Begonnen, ziel.clone()));
        }
        if let Some(f) = z.fertig {
            let text = z.ungefaehr.then(|| SATZ_UNGEFAEHR.to_string());
            liste.push(ereignis(format!("arbeit:{}:fertig", a.id), f, &a.titel, text, Quelle::Arbeit, Art::Erledigt, ziel.clone()));
        }
        for (n, e) in a.verlauf.iter().enumerate() {
            let Some(nr) = e.schritt else { continue };
            let titel = a.schritte.iter().find(|s| s.nr == nr).map_or(e.text.as_str(), |s| s.titel.as_str());
            // `neu` beginnt bei Schritt-Einträgen mit dem Stand-Wort (wie in `plan::zeiten`).
            let fertig = e.neu.split(',').next().is_some_and(|w| w.trim() == ArbeitsStand::Fertig.wort());
            let art = if fertig { Art::Erledigt } else { Art::Routine };
            let kennung = format!("schritt:{}/{nr}:{n}", a.id);
            let text = format!("Bei „{}“: {}", a.titel, e.text);
            liste.push(ereignis(kennung, e.zeit.clone(), titel, Some(text), Quelle::Schritt, art, ziel.clone()));
        }
        if !in_liste && !im_plan.contains(a.id.as_str()) {
            // Fertig ohne Etappe: kein Ziel auf der Seite, also kein toter Anker.
            let eigene = format!("arbeit:{}:", a.id);
            let schritte = format!("schritt:{}/", a.id);
            liste.iter_mut().filter(|e| e.kennung.starts_with(&eigene) || e.kennung.starts_with(&schritte)).for_each(|e| e.verweis = None);
        }
    }
    for e in p.etappen.iter().filter(|e| e.stand == EtappenStand::Fertig) {
        let Some(f) = &e.fertig else { continue };
        let ziel = format!("/projects/{slug}#etappe-{}", e.kennung);
        liste.push(ereignis(format!("etappe:{}", e.kennung), f.clone(), &e.titel, Some(e.ziel.clone()), Quelle::Arbeit, Art::Erledigt, ziel));
    }
    liste
}

/// Die Vorgänge von `slug`: Bauten als `bau` (rot `problem`, sonst
/// `routine`, Zeit der letzten Änderung), Antworten auf Fragen als `frage`
/// (`erledigt`), alle anderen wie im Verlauf des PCs. Ohne `schritt` (steht schon als
/// Schritt-Eintrag der Arbeit) und `oeffnen` (Klick-Protokoll, kein Projektereignis).
pub fn aus_vorgaengen(slug: &str, vorgaenge: &[Vorgang]) -> Vec<Ereignis> {
    let mut liste = Vec::new();
    for v in vorgaenge.iter().filter(|v| v.slug == slug && !matches!(v.art, VorgangsArt::Schritt | VorgangsArt::Oeffnen)) {
        let ziel = format!("/vorgaenge#vorgang-{}", v.id);
        let e = match v.art {
            VorgangsArt::Bau => {
                let rot = v.bau.as_ref().is_some_and(|b| b.zustand == BauZustand::Rot);
                let text = v.bau.as_ref().map(|b| b.meldung.clone());
                ereignis(format!("bau:{}", v.id), v.geaendert.clone(), &v.titel, text, Quelle::Bau, if rot { Art::Problem } else { Art::Routine }, ziel)
            }
            VorgangsArt::Antwort if v.bezug.as_deref().and_then(aus_bezug).is_some() => {
                ereignis(format!("frage:{}", v.id), v.erstellt.clone(), &v.titel, v.notiz.clone(), Quelle::Frage, Art::Erledigt, ziel)
            }
            _ => aus_vorgang(v),
        };
        liste.push(e);
    }
    liste
}

/// Die Entscheidungen mit `slug` in `projekte`; ohne lesbares Datum keine.
pub fn aus_entscheidungen(slug: &str, l: &EntscheidungsListe) -> Vec<Ereignis> {
    let mut liste = Vec::new();
    for e in l.eintraege.iter().filter(|e| projekte_der(e).contains(&slug)) {
        let Some((tag, genauigkeit, _)) = e.datum.as_deref().and_then(datum_lesen) else { continue };
        let titel = e.thema.as_deref().unwrap_or(&e.datei);
        let mut x = ereignis(
            format!("entscheidung:{}", e.datei),
            zeit_text(&tagesanfang(tag)),
            titel,
            e.status.clone(),
            Quelle::Entscheidung,
            Art::Erledigt,
            "/entscheidungen".to_string(),
        );
        x.genauigkeit = genauigkeit;
        liste.push(x);
    }
    liste
}

/// Die offenen Fristen mit Datum als `geplant`, verstrichene als `problem` (Tag genau); Fristen, die an
/// einem Ereignis ohne Datum hängen, fehlen.
pub fn aus_fristen(f: &FristenListe) -> Vec<Ereignis> {
    let mut liste = Vec::new();
    for z in &f.zeilen {
        let Some(tag) = z.frist.as_deref().and_then(|t| NaiveDate::parse_from_str(t, "%Y-%m-%d").ok()) else { continue };
        let quelle = format!("{:?}", z.quelle).to_lowercase();
        let mut x = ereignis(
            format!("frist:{quelle}:{}", z.id),
            zeit_text(&tagesanfang(tag)),
            &z.titel,
            z.hinweis.clone(),
            Quelle::Frist,
            if z.tage_bis.is_some_and(|t| t < 0) { Art::Problem } else { Art::Geplant },
            z.ziel.clone(),
        );
        x.genauigkeit = Genauigkeit::Tag;
        liste.push(x);
    }
    liste
}

/// Die Meldungen von `slug` ohne die, die schon eine eigene Zeile haben
/// („Kein Inhalt steht in zwei Teilen“): „Fertig: …“ steht als
/// `arbeit:<id>:fertig`, `<slug>/bau-rot` als roter Bau, sobald einer da ist.
pub fn aus_meldungen(slug: &str, meldungen: &[Meldung], ereignisse: &[Ereignis]) -> Vec<Ereignis> {
    let roter_bau = ereignisse.iter().any(|e| e.quelle == Quelle::Bau && e.art == Art::Problem);
    let bau_rot = schluessel(slug, "bau-rot");
    meldungen.iter().filter(|m| m.projekt == slug && m.art != MeldungsArt::Fertig && !(roter_bau && m.schluessel == bau_rot)).map(aus_meldung).collect()
}
