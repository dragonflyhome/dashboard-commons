//! Teilschritte einer Arbeit (Zweig teilschritte, Etappe T2): das Dashboard
//! als Mutex zwischen den Chats (`docs/zielbild/f4-teilschritte.toml`).
//!
//! Anlegen, übernehmen und ändern laufen je in einer Schreib-Transaktion:
//! Prüfen und Schreiben zusammen, darum gewinnt von zwei Chats, die zugleich
//! übernehmen, genau einer. Abgelehnt wird mit `SpeicherFehler::Besetzt`
//! (ein anderer ist dran, 409), `Ungueltig` (gegen die Regeln, 422) oder
//! `Grenze` (429); dann ist nichts geschrieben. Hat eine Arbeit Schritte,
//! rechnet der Server ihren Stand daraus (`stand_ableiten`). Jede Änderung
//! steht im Verlauf der Arbeit.
//!
//! Wecken (T3): Wer einen Schritt angeboten bekommt oder dessen wartender
//! Schritt wieder läuft, bekommt in derselben Transaktion einen Vorgang der Art
//! `schritt` (höchstens einen je Chat und Anfrage, Kennung in `Schritt::vorgang`).
//! Ändert sich ein Schritt mit offenem Weck-Vorgang (übernommen, Angebot
//! umgelenkt oder zurückgenommen, fertig, eine Notiz seines Besitzers), ist der
//! Vorgang erledigt, ebenfalls in derselben Transaktion.

use std::collections::BTreeMap;

use chrono::DateTime;

use super::arbeiten::{
    OFFEN_JE_PROJEKT, SATZ_IM_ARCHIV, arbeit_aus, braucht_besitzer, grenze_satz, offene_im_projekt, stand_verlauf, verlauf_anhaengen, zeit_text,
};
use super::aufgaben::Zeit;
use super::tabellen::{ARBEITEN, ARBEITEN_ARCHIV};
use super::vorgaenge::{vorgang_anlegen_in, vorgang_weiter_in};
use super::{Bereich, Speicher, SpeicherFehler};
use crate::api::vertrag::{Arbeit, ArbeitVerlauf, ArbeitVerlaufArt, ArbeitsStand, Ereignis, Schritt, VorgangsArt, Zustand};

/// Höchstens so viele Schritte je Arbeit (darüber 429).
pub const SCHRITTE_HOECHSTENS: usize = 20;
/// Ohne Schreiben des Besitzers gilt ein Schritt nach so vielen Stunden als verwaist.
pub const VERWAIST_NACH_STUNDEN: i64 = 24;
/// Titel des Schritts, den der Server beim ersten Schritt einer laufenden
/// oder wartenden Arbeit anlegt.
pub const TITEL_BISHER: &str = "bisherige Arbeit";
/// Titel eines Vorgangs höchstens so viele Zeichen (wie `POST /api/vorgaenge`).
const VORGANG_TITEL_HOECHSTENS: usize = 200;

const INHABER: &str = crate::anstehend::INHABER;
const EXTERN: &str = "extern";
const SERVER: &str = "server";

/// Warum ein Chat geweckt werden soll.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeckGrund {
    /// Ihm wurde ein Schritt angeboten.
    Angeboten,
    /// Der Schritt, auf den sein Schritt wartete, ist fertig oder frei.
    Zurueckgeweckt,
}

/// Ein Chat, der mit einem Vorgang geweckt wird.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wecken {
    pub nr: u32,
    pub chat: String,
    pub grund: WeckGrund,
}

/// Ergebnis einer Schritt-Anfrage.
#[derive(Debug)]
pub struct SchrittErgebnis {
    /// Die Arbeit danach (bei `geaendert = false` unverändert).
    pub arbeit: Arbeit,
    /// Der angelegte oder angefragte Schritt.
    pub nr: u32,
    /// `false`: alles war schon so, nichts geschrieben.
    pub geaendert: bool,
    /// Wer geweckt werden soll (angeboten, zurückgeweckt).
    pub wecken: Vec<Wecken>,
    /// Ereignisse der Weck-Vorgänge (angelegt, erledigt), für `vorgaenge::melden`.
    pub ereignisse: Vec<Ereignis>,
    /// Die Arbeit ist mit dieser Anfrage fertig geworden (für die Meldung „fertig“).
    pub wurde_fertig: bool,
    /// Die Arbeit war fertig und ist mit dieser Anfrage wieder offen (Meldung „fertig“ behoben).
    pub wieder_offen: bool,
}

/// Änderung an einem Schritt, schon geprüft (Form der Werte). Äußeres
/// `None`: bleibt; `Some(None)`: leeren.
#[derive(Debug, Clone, Default)]
pub struct SchrittAenderung {
    pub stand: Option<ArbeitsStand>,
    pub wartet_auf: Option<Option<String>>,
    pub verweis: Option<Option<String>>,
    pub notiz: Option<Option<String>>,
    /// `chat:<name>`, dem der Schritt übergeben werden soll.
    pub anbieten: Option<String>,
    pub freigeben: bool,
}

impl SchrittAenderung {
    /// Ändert sie mehr als nur das Angebot?
    fn mehr_als_anbieten(&self) -> bool {
        self.stand.is_some() || self.wartet_auf.is_some() || self.verweis.is_some() || self.notiz.is_some() || self.freigeben
    }
}

fn ungueltig(satz: impl Into<String>) -> SpeicherFehler {
    SpeicherFehler::Ungueltig(satz.into())
}

/// Wer gerade dran ist: wem er angeboten ist, sonst sein Besitzer.
fn halter(s: &Schritt) -> Option<&String> {
    s.angeboten_an.as_ref().or(s.besitzer.as_ref())
}

/// Frei: niemandem angeboten und ohne Besitzer. Ein fertiger behält seinen
/// Besitzer; wieder auf geht er über ihn oder über Anbieten.
fn frei(s: &Schritt) -> bool {
    s.angeboten_an.is_none() && s.besitzer.is_none()
}

fn besetzt(s: &Schritt) -> SpeicherFehler {
    SpeicherFehler::Besetzt { besitzer: halter(s).cloned().unwrap_or_default(), seit: s.seit.clone() }
}

/// `schritt:<nr>` → `nr`.
fn schritt_nr(wartet_auf: Option<&str>) -> Option<u32> {
    wartet_auf?.strip_prefix("schritt:")?.parse().ok()
}

/// Verwaist: über 24 Stunden kein Schreiben an einem laufenden Schritt mit
/// Besitzer oder an einem angebotenen. Wartende nie (sie werden geweckt).
fn verwaist(s: &Schritt, jetzt: &Zeit) -> bool {
    (s.angeboten_an.is_some() || (s.stand == ArbeitsStand::Laeuft && s.besitzer.is_some()))
        && DateTime::parse_from_rfc3339(&s.seit).is_ok_and(|t| t + chrono::Duration::hours(VERWAIST_NACH_STUNDEN) < *jetzt)
}

/// Stand, Besitzer, Warten und Angebot in Worten, für den Verlauf.
fn zustand(s: &Schritt) -> String {
    let mut teile = vec![s.stand.wort().to_string()];
    if let Some(b) = &s.besitzer {
        teile.push(format!("dran: {b}"));
    }
    if let Some(w) = &s.wartet_auf {
        teile.push(format!("wartet auf {w}"));
    }
    if let Some(an) = &s.angeboten_an {
        teile.push(format!("angeboten an {an}"));
    }
    teile.join(", ")
}

fn eintrag(zeit: &str, von: &str, art: ArbeitVerlaufArt, nr: u32, alt: String, neu: String, text: String) -> ArbeitVerlauf {
    ArbeitVerlauf { zeit: zeit.to_string(), von: von.to_string(), art, schritt: Some(nr), alt, neu, text }
}

fn index(a: &Arbeit, nr: u32) -> Result<usize, SpeicherFehler> {
    a.schritte.iter().position(|s| s.nr == nr).ok_or_else(|| SpeicherFehler::NichtGefunden(format!("{}/schritte/{nr}", a.id)))
}

/// Stand und `wartet_auf` der Arbeit aus ihren Schritten: wartet einer auf
/// den Inhaber, wartet sie auf ihn (mit dem `verweis` jenes Schritts), auch
/// wenn ein anderer läuft; sonst läuft einer, läuft sie; sonst wartet einer,
/// wartet sie (worauf der erste wartende wartet, `schritt:<nr>` als der Chat,
/// der an jenem dran ist); alle fertig: fertig; sonst als nächstes. Ohne
/// Schritte bleibt alles. Ob weitere auf dich wartende Schritte auf etwas
/// Offenes verweisen, prüft `anstehend` je Schritt (`offene_verweise`).
pub fn stand_ableiten(a: &mut Arbeit) {
    let s = &a.schritte;
    if s.is_empty() {
        return;
    }
    let inhaber = s.iter().find(|x| x.stand == ArbeitsStand::Wartet && x.wartet_auf.as_deref() == Some(INHABER));
    if let Some(x) = inhaber {
        // Sonst verdeckt ein laufender Schritt, dass der Inhaber gebraucht wird.
        a.verweis = x.verweis.clone();
        a.stand = ArbeitsStand::Wartet;
        a.wartet_auf = Some(INHABER.to_string());
        return;
    }
    let hat = |stand| s.iter().any(|x| x.stand == stand);
    a.stand = if hat(ArbeitsStand::Laeuft) {
        ArbeitsStand::Laeuft
    } else if hat(ArbeitsStand::Wartet) {
        ArbeitsStand::Wartet
    } else if s.iter().all(|x| x.stand == ArbeitsStand::Fertig) {
        ArbeitsStand::Fertig
    } else {
        ArbeitsStand::AlsNaechstes
    };
    a.wartet_auf = (a.stand == ArbeitsStand::Wartet).then(|| {
        let w = s.iter().find(|x| x.stand == ArbeitsStand::Wartet).and_then(|x| x.wartet_auf.clone()).unwrap_or_else(|| EXTERN.to_string());
        match schritt_nr(Some(&w)).and_then(|n| s.iter().find(|x| x.nr == n)).and_then(halter) {
            Some(chat) => chat.clone(),
            None => w,
        }
    });
}

/// Schritt anlegen: frei, `als-naechstes`. Beim ersten Schritt einer
/// laufenden oder wartenden Arbeit zuerst Schritt 1 „bisherige Arbeit“ mit
/// ihrem Besitzer und Stand.
fn anlegen_in(a: &mut Arbeit, von: &str, titel: &str, fuer: Option<String>, zeit: &str) -> Result<u32, SpeicherFehler> {
    let bisher = a.schritte.is_empty() && braucht_besitzer(a.stand);
    if a.schritte.len() + 1 + usize::from(bisher) > SCHRITTE_HOECHSTENS {
        return Err(SpeicherFehler::Grenze(format!(
            "Die Arbeit {} hat schon {} Schritte; mehr als {SCHRITTE_HOECHSTENS} gehen nicht. Was wegfällt, bitte mit Notiz auf „fertig“ setzen.",
            a.id,
            a.schritte.len()
        )));
    }
    if bisher {
        // Wer vor Schema 10 ohne besitzer lief, gilt wie bei der Umstellung als von.
        let besitzer = a.besitzer.clone().unwrap_or_else(|| a.von.clone());
        let s = Schritt {
            nr: 1,
            titel: TITEL_BISHER.to_string(),
            stand: a.stand,
            besitzer: Some(besitzer),
            wartet_auf: a.wartet_auf.clone(),
            verweis: a.verweis.clone(),
            angeboten_an: None,
            fuer: None,
            seit: a.geaendert.clone(),
            notiz: None,
            vorgang: None,
        };
        let text = format!("Schritt 1 „{TITEL_BISHER}“ vom Server angelegt ({}).", zustand(&s));
        let e = eintrag(zeit, von, ArbeitVerlaufArt::SchrittAngelegt, 1, String::new(), zustand(&s), text);
        a.schritte.push(s);
        verlauf_anhaengen(a, e)?;
    }
    let nr = a.schritte.iter().map(|s| s.nr).max().unwrap_or(0) + 1;
    let s = Schritt {
        nr,
        titel: titel.to_string(),
        stand: ArbeitsStand::AlsNaechstes,
        besitzer: None,
        wartet_auf: None,
        verweis: None,
        angeboten_an: None,
        fuer,
        seit: zeit.to_string(),
        notiz: None,
        vorgang: None,
    };
    let text = format!("Schritt {nr} „{titel}“ angelegt.");
    let e = eintrag(zeit, von, ArbeitVerlaufArt::SchrittAngelegt, nr, String::new(), titel.to_string(), text);
    a.schritte.push(s);
    verlauf_anhaengen(a, e)?;
    Ok(nr)
}

/// Übernehmen: ein freier, dem Chat angebotener oder schon eigener Schritt
/// wird seiner und läuft. Ein fremder nur, wenn er verwaist ist und `statt`
/// den nennt, der dran ist. Gibt `false` zurück, wenn er schon lief.
fn uebernehmen_in(a: &mut Arbeit, nr: u32, von: &str, statt: Option<&str>, jetzt: &Zeit) -> Result<bool, SpeicherFehler> {
    let i = index(a, nr)?;
    let s = &a.schritte[i];
    if let Some(f) = s.fuer.as_deref().filter(|f| *f != von) {
        return Err(ungueltig(format!("Schritt {nr} ist für {f} bestimmt; nur dieser Chat darf ihn übernehmen.")));
    }
    let eigen = s.besitzer.as_deref() == Some(von);
    if eigen && s.angeboten_an.is_none() && s.stand == ArbeitsStand::Laeuft {
        return Ok(false);
    }
    let angeboten = s.angeboten_an.as_deref() == Some(von);
    let mut statt_text = String::new();
    if !(eigen || angeboten || frei(s)) {
        let alt = halter(s).map(String::as_str);
        if statt.is_none() || statt != alt || !verwaist(s, jetzt) {
            return Err(besetzt(s));
        }
        statt_text = format!(" statt {} (seit über {VERWAIST_NACH_STUNDEN} Stunden unverändert)", alt.unwrap_or_default());
    }
    let zeit = zeit_text(jetzt);
    let vorher = zustand(s);
    let s = &mut a.schritte[i];
    s.besitzer = Some(von.to_string());
    s.stand = ArbeitsStand::Laeuft;
    s.wartet_auf = None;
    s.angeboten_an = None;
    s.seit = zeit.clone();
    let text = format!("{von} hat Schritt {nr} übernommen{statt_text}.");
    let e = eintrag(&zeit, von, ArbeitVerlaufArt::SchrittUebernommen, nr, vorher, zustand(s), text);
    verlauf_anhaengen(a, e)?;
    Ok(true)
}

/// Prüft `wartet_auf = schritt:<ziel>` für Schritt `nr`: nicht auf sich
/// selbst, kein Kreis, und das Ziel läuft mit Besitzer oder ist angeboten.
fn warten_pruefen(a: &Arbeit, nr: u32, ziel: u32) -> Result<(), SpeicherFehler> {
    if ziel == nr {
        return Err(ungueltig(format!("Schritt {nr} kann nicht auf sich selbst warten.")));
    }
    let finde = |n: u32| a.schritte.iter().find(|s| s.nr == n);
    let Some(z) = finde(ziel) else {
        return Err(ungueltig(format!("Schritt {ziel} gibt es in dieser Arbeit nicht.")));
    };
    let mut kette = vec![ziel];
    let mut jetzt = z;
    while jetzt.stand == ArbeitsStand::Wartet {
        let Some(weiter) = schritt_nr(jetzt.wartet_auf.as_deref()) else {
            break;
        };
        if weiter == nr {
            let ueber = kette.iter().map(|n| format!("Schritt {n}")).collect::<Vec<_>>().join(" → ");
            return Err(ungueltig(format!(
                "{ueber} wartet schon auf Schritt {nr}; ihr würdet im Kreis aufeinander warten. \
                 Biete zuerst Schritt {nr} an (oder setze ihn fertig), dann wartet niemand im Kreis."
            )));
        }
        if kette.contains(&weiter) {
            break;
        }
        kette.push(weiter);
        let Some(n) = finde(weiter) else { break };
        jetzt = n;
    }
    let laeuft = z.stand == ArbeitsStand::Laeuft && z.besitzer.is_some();
    if !laeuft && z.angeboten_an.is_none() {
        return Err(ungueltig(format!(
            "Auf Schritt {ziel} kann niemand warten: an ihm arbeitet keiner, und er ist niemandem angeboten. \
             Biete Schritt {ziel} zuerst an, dann warte darauf."
        )));
    }
    Ok(())
}

/// Wer den angebotenen Schritt `nr` angeboten hat: der letzte Eintrag zu ihm im
/// Verlauf. Solange er angeboten ist, schreibt an ihm nur ein Anbieten (er hat
/// keinen Besitzer, wartet nicht, und nur Anbieten oder Übernehmen ändern ihn).
fn angeboten_von(a: &Arbeit, nr: u32) -> Option<&str> {
    a.verlauf.iter().rev().find(|v| v.schritt == Some(nr)).map(|v| v.von.as_str())
}

/// Ändern: Stand, Warten, Verweis, Notiz, Anbieten, Freigeben. Nur der
/// Besitzer; einen freien oder fertigen Schritt anbieten darf auch, wer in
/// derselben Arbeit einen Schritt besitzt (auch einen fertigen). Ein Angebot
/// umlenken oder zurücknehmen (freigeben) darf auch, wer es zuletzt gemacht
/// hat, solange der Empfänger nicht übernommen hat. Anbieten, freigeben und
/// `als-naechstes` lassen den Schritt ohne Besitzer. Wird der Schritt fertig
/// oder frei, laufen die auf ihn wartenden Schritte wieder.
fn aendern_in(a: &mut Arbeit, nr: u32, von: &str, aenderung: &SchrittAenderung, zeit: &str) -> Result<Vec<Wecken>, SpeicherFehler> {
    let i = index(a, nr)?;
    let alt = a.schritte[i].clone();
    if alt.besitzer.as_deref() != Some(von) {
        let darf_anbieten = (alt.besitzer.is_none() || alt.stand == ArbeitsStand::Fertig) && a.schritte.iter().any(|s| s.besitzer.as_deref() == Some(von));
        // Auflage teilschritte_1_runde_2: sonst hinge ein falsches Angebot bis zum Verwaisen.
        let anbieter = alt.angeboten_an.is_some() && angeboten_von(a, nr) == Some(von);
        let nur_angebot = aenderung.stand.is_none() && aenderung.wartet_auf.is_none() && aenderung.verweis.is_none() && aenderung.notiz.is_none();
        match (&aenderung.anbieten, aenderung.mehr_als_anbieten()) {
            _ if anbieter && nur_angebot && (aenderung.anbieten.is_some() || aenderung.freigeben) => {}
            (Some(_), false) if darf_anbieten => {}
            _ if alt.angeboten_an.as_deref() == Some(von) => {
                return Err(ungueltig(format!("Schritt {nr} ist dir angeboten; erst übernehmen, dann ändern.")));
            }
            _ if halter(&alt).is_some() && !frei(&alt) => return Err(besetzt(&alt)),
            (Some(_), false) => {
                return Err(ungueltig(format!("Schritt {nr} anbieten darf nur, wer in dieser Arbeit selbst einen Schritt hat; sonst erst übernehmen.")));
            }
            _ => {
                return Err(ungueltig(format!("Schritt {nr} ist frei; erst übernehmen, dann ändern.")));
            }
        }
    }
    if (aenderung.anbieten.is_some() || aenderung.freigeben) && (aenderung.stand.is_some() || aenderung.wartet_auf.is_some() || aenderung.verweis.is_some()) {
        return Err(ungueltig("„anbieten“ und „freigeben“ gehen nur ohne „stand“, „wartet_auf“ und „verweis“."));
    }
    if aenderung.anbieten.is_some() && aenderung.freigeben {
        return Err(ungueltig("Entweder „anbieten“ oder „freigeben“."));
    }
    let mut s = alt.clone();
    let mut wecken = Vec::new();
    if let Some(n) = &aenderung.notiz {
        s.notiz = n.clone();
    }
    if aenderung.freigeben {
        s.besitzer = None;
        s.angeboten_an = None;
        s.wartet_auf = None;
        if s.stand != ArbeitsStand::Fertig {
            s.stand = ArbeitsStand::AlsNaechstes;
        }
    }
    if let Some(an) = &aenderung.anbieten {
        if an == von {
            return Err(ungueltig(format!("Sich selbst anbieten geht nicht; Schritt {nr} bitte übernehmen.")));
        }
        if let Some(f) = s.fuer.as_deref().filter(|f| f != an) {
            return Err(ungueltig(format!("Schritt {nr} ist für {f} bestimmt; nur an diesen Chat anbieten.")));
        }
        // Angeboten heißt frei, bis der Empfänger übernimmt; ein fertiger öffnet sich
        // wieder. Wer dran war, steht im Verlauf.
        s.besitzer = None;
        s.stand = ArbeitsStand::AlsNaechstes;
        s.wartet_auf = None;
        if s.angeboten_an.as_ref() != Some(an) {
            wecken.push(Wecken { nr, chat: an.clone(), grund: WeckGrund::Angeboten });
        }
        s.angeboten_an = Some(an.clone());
    }
    if let Some(st) = aenderung.stand {
        s.stand = st;
        if st == ArbeitsStand::AlsNaechstes {
            // Wie freigeben: sonst bliebe er besetzt, ohne dass ihn je einer übernehmen könnte.
            s.besitzer = None;
        } else {
            s.angeboten_an = None;
        }
    }
    if let Some(w) = &aenderung.wartet_auf {
        s.wartet_auf = w.clone();
    }
    if let Some(v) = &aenderung.verweis {
        s.verweis = v.clone();
    }
    // Ein unverändertes Warten nicht neu prüfen: Ändert sich das Ziel danach oder
    // kam es von der Arbeit (Schritt 1, etwa chat:<name>), bleibt eine Notiz möglich.
    // Ein neuer verweis zählt nur beim Warten auf den Inhaber (er braucht einen).
    let warten_wie_vorher =
        alt.stand == ArbeitsStand::Wartet && aenderung.wartet_auf.is_none() && (aenderung.verweis.is_none() || s.wartet_auf.as_deref() != Some(INHABER));
    if s.stand == ArbeitsStand::Wartet {
        match s.wartet_auf.as_deref() {
            _ if warten_wie_vorher => {}
            None => {
                return Err(ungueltig("Bei „wartet“ fehlt „wartet_auf“: schritt:<nr>, inhaber oder extern."));
            }
            Some(INHABER) if s.verweis.is_none() => {
                return Err(ungueltig("Wer auf den Inhaber wartet, nennt in „verweis“ die Frage oder Aufgabe."));
            }
            Some(INHABER | EXTERN) => {}
            Some(w) => match schritt_nr(Some(w)) {
                Some(ziel) => warten_pruefen(a, nr, ziel)?,
                None => {
                    return Err(ungueltig("„wartet_auf“ ist bei Schritten schritt:<nr>, inhaber oder extern."));
                }
            },
        }
    } else if aenderung.wartet_auf.as_ref().is_some_and(Option::is_some) {
        return Err(ungueltig("„wartet_auf“ gibt es nur beim Stand „wartet“."));
    } else {
        s.wartet_auf = None;
    }
    if s == alt {
        return Ok(Vec::new());
    }
    s.seit = zeit.to_string();
    let text = if zustand(&s) == zustand(&alt) {
        format!("{von} hat die Notiz an Schritt {nr} geändert.")
    } else {
        format!("{von}: Schritt {nr} {} → {}.", zustand(&alt), zustand(&s))
    };
    let e = eintrag(zeit, von, ArbeitVerlaufArt::SchrittGeaendert, nr, zustand(&alt), zustand(&s), text);
    // Frei geworden (freigeben, als-naechstes, ein Angebot zurückgenommen); ein
    // angebotener bleibt ein Ziel zum Warten.
    let frei_geworden = halter(&alt).is_some() && frei(&s);
    let erledigt = (s.stand == ArbeitsStand::Fertig && alt.stand != ArbeitsStand::Fertig) || frei_geworden;
    a.schritte[i] = s;
    if erledigt {
        // Fertig oder frei und das Wecken zählen nicht gegen die Grenze (wie Auflage
        // teilschritte_1_runde_2): sonst hinge, wer darauf wartet. Je Schritt höchstens
        // einmal über der Grenze, denn wieder dran kommt nur, wer über verlauf_anhaengen schreibt.
        a.verlauf.push(e);
        wecken.extend(zurueckwecken(a, nr, zeit));
    } else {
        verlauf_anhaengen(a, e)?;
    }
    Ok(wecken)
}

/// Schritte, die auf `nr` warten, laufen wieder (ihr Besitzer bleibt); die
/// Einträge im Verlauf an der Grenze vorbei (siehe `aendern_in`).
fn zurueckwecken(a: &mut Arbeit, nr: u32, zeit: &str) -> Vec<Wecken> {
    let ziel = format!("schritt:{nr}");
    let mut wecken = Vec::new();
    let mut eintraege = Vec::new();
    for s in a.schritte.iter_mut() {
        if s.stand != ArbeitsStand::Wartet || s.wartet_auf.as_deref() != Some(ziel.as_str()) {
            continue;
        }
        let vorher = zustand(s);
        s.stand = ArbeitsStand::Laeuft;
        s.wartet_auf = None;
        s.seit = zeit.to_string();
        let text = format!("Schritt {} läuft wieder: Schritt {nr} ist fertig oder frei.", s.nr);
        eintraege.push(eintrag(zeit, SERVER, ArbeitVerlaufArt::SchrittGeweckt, s.nr, vorher, zustand(s), text));
        if let Some(b) = &s.besitzer {
            wecken.push(Wecken { nr: s.nr, chat: b.clone(), grund: WeckGrund::Zurueckgeweckt });
        }
    }
    a.verlauf.extend(eintraege);
    wecken
}

/// Titel des Weck-Vorgangs für die Schritte `liste` (alle für denselben Chat).
fn weck_titel(a: &Arbeit, liste: &[&Wecken]) -> String {
    let w = liste[0];
    let titel = a.schritte.iter().find(|s| s.nr == w.nr).map(|s| s.titel.as_str()).unwrap_or_default();
    let mut t = match w.grund {
        WeckGrund::Angeboten => format!("Dir angeboten: Schritt {} „{titel}“; bitte übernehmen.", w.nr),
        WeckGrund::Zurueckgeweckt => format!("Läuft wieder: dein Schritt {} „{titel}“; worauf er wartete, ist fertig oder frei.", w.nr),
    };
    if liste.len() > 1 {
        let weitere: Vec<String> = liste[1..].iter().map(|w| w.nr.to_string()).collect();
        t.push_str(&format!(" Dazu Schritt {}.", weitere.join(", ")));
    }
    t.chars().take(VORGANG_TITEL_HOECHSTENS).collect()
}

/// Weck-Vorgänge in der offenen Transaktion: Jeder geänderte Schritt mit offenem
/// Vorgang erledigt ihn; je Chat in `wecken` (außer `von` selbst) ein neuer
/// Vorgang der Art `schritt`, seine Kennung an den Schritten. Gibt die Ereignisse zurück.
/// Weckt eine Anfrage mehrere Schritte desselben Chats, nennt `bezug` nur den ersten.
fn wecken_in(txn: &redb::WriteTransaction, vorher: &Arbeit, a: &mut Arbeit, von: &str, wecken: &[Wecken]) -> Result<Vec<Ereignis>, SpeicherFehler> {
    let mut ereignisse = Vec::new();
    let mut erledigt = Vec::new();
    for s in a.schritte.iter_mut() {
        if vorher.schritte.iter().any(|x| x.nr == s.nr && x == s) {
            continue;
        }
        let Some(v) = s.vorgang.take() else { continue };
        match vorgang_weiter_in(txn, &v, Zustand::Erledigt, SERVER, Some(&format!("Schritt {} hat sich geändert.", s.nr))) {
            Ok(Some((_, e))) => ereignisse.push(e),
            // Schon erledigt oder nicht mehr da: nichts zu tun.
            Ok(None) | Err(SpeicherFehler::NichtGefunden(_)) => {}
            Err(e) => return Err(e),
        }
        erledigt.push(v);
    }
    // Ein Vorgang für mehrere Schritte desselben Chats: auch an den anderen nicht mehr offen.
    for s in a.schritte.iter_mut().filter(|s| s.vorgang.as_ref().is_some_and(|v| erledigt.contains(v))) {
        s.vorgang = None;
    }
    let mut je_chat: BTreeMap<&str, Vec<&Wecken>> = BTreeMap::new();
    for w in wecken.iter().filter(|w| w.chat != von) {
        je_chat.entry(w.chat.as_str()).or_default().push(w);
    }
    for (chat, liste) in je_chat {
        // Nur chat:<name> lässt sich wecken (ältere Besitzer können anders heißen).
        let Some(slug) = chat.strip_prefix("chat:") else { continue };
        let bezug = format!("schritt:{}/{}", a.id, liste[0].nr);
        let (v, e) = vorgang_anlegen_in(txn, VorgangsArt::Schritt, slug, &weck_titel(a, &liste), Some(&bezug), von)?;
        for w in &liste {
            if let Some(s) = a.schritte.iter_mut().find(|s| s.nr == w.nr) {
                s.vorgang = Some(v.id.clone());
            }
        }
        ereignisse.push(e);
    }
    Ok(ereignisse)
}

impl Speicher {
    /// Liest die Arbeit in einer Schreib-Transaktion, wendet `f` an und
    /// schreibt sie nur, wenn sich etwas geändert hat; danach Stand aus den
    /// Schritten, die Grenze offener Arbeiten beim Wiederöffnen und die
    /// Weck-Vorgänge (`wecken_in`), alles in derselben Transaktion.
    fn schritt_schreiben(
        &self,
        id: &str,
        von: &str,
        jetzt: &Zeit,
        f: impl FnOnce(&mut Arbeit) -> Result<(u32, Vec<Wecken>), SpeicherFehler>,
    ) -> Result<SchrittErgebnis, SpeicherFehler> {
        let txn = self.db.begin_write()?;
        let Some(mut a) = arbeit_aus(&txn.open_table(ARBEITEN)?, id)? else {
            return Err(match arbeit_aus(&txn.open_table(ARBEITEN_ARCHIV)?, id)? {
                Some(_) => ungueltig(SATZ_IM_ARCHIV),
                None => SpeicherFehler::NichtGefunden(id.to_string()),
            });
        };
        let vorher = a.clone();
        let (nr, wecken) = f(&mut a)?;
        if a == vorher {
            return Ok(SchrittErgebnis {
                arbeit: a,
                nr,
                geaendert: false,
                wecken: Vec::new(),
                ereignisse: Vec::new(),
                wurde_fertig: false,
                wieder_offen: false,
            });
        }
        stand_ableiten(&mut a);
        if vorher.stand == ArbeitsStand::Fertig
            && a.stand != ArbeitsStand::Fertig
            && offene_im_projekt(&txn.open_table(ARBEITEN)?, &a.projekt)? >= OFFEN_JE_PROJEKT
        {
            return Err(grenze_satz(&a.projekt));
        }
        a.geaendert = zeit_text(jetzt);
        if a.stand != vorher.stand {
            // An der Grenze vorbei: Ein abgeleiteter Wechsel folgt immer einem Schritt-Eintrag,
            // der schon über die Grenze ging oder (fertig, frei) sie auslassen darf.
            a.verlauf.push(stand_verlauf(&a.geaendert, von, vorher.stand, a.stand));
        }
        let ereignisse = wecken_in(&txn, &vorher, &mut a, von, &wecken)?;
        txn.open_table(ARBEITEN)?.insert(a.id.as_str(), serde_json::to_vec(&a)?.as_slice())?;
        let bereiche: &[Bereich] = if ereignisse.is_empty() { &[Bereich::Arbeiten] } else { &[Bereich::Arbeiten, Bereich::Vorgaenge] };
        self.festschreiben(txn, bereiche)?;
        let wurde_fertig = a.stand == ArbeitsStand::Fertig && vorher.stand != ArbeitsStand::Fertig;
        let wieder_offen = vorher.stand == ArbeitsStand::Fertig && a.stand != ArbeitsStand::Fertig;
        Ok(SchrittErgebnis { arbeit: a, nr, geaendert: true, wecken, ereignisse, wurde_fertig, wieder_offen })
    }

    /// Legt einen Schritt an (frei, `als-naechstes`); `fuer`: nur dieser Chat
    /// darf ihn bekommen. Titel und Chat-Namen kommen geprüft.
    pub fn schritt_anlegen(&self, id: &str, von: &str, titel: &str, fuer: Option<String>, jetzt: &Zeit) -> Result<SchrittErgebnis, SpeicherFehler> {
        let zeit = zeit_text(jetzt);
        self.schritt_schreiben(id, von, jetzt, |a| Ok((anlegen_in(a, von, titel, fuer, &zeit)?, Vec::new())))
    }

    /// `von` übernimmt Schritt `nr`; ein zweites Mal ändert nichts
    /// (`geaendert = false`). Ein fremder Schritt: `Besetzt`, außer er ist
    /// verwaist und `statt` nennt, wer dran ist.
    pub fn schritt_uebernehmen(&self, id: &str, nr: u32, von: &str, statt: Option<&str>, jetzt: &Zeit) -> Result<SchrittErgebnis, SpeicherFehler> {
        self.schritt_schreiben(id, von, jetzt, |a| {
            uebernehmen_in(a, nr, von, statt, jetzt)?;
            Ok((nr, Vec::new()))
        })
    }

    /// Ändert Schritt `nr` im Namen von `von` (siehe `SchrittAenderung`).
    pub fn schritt_aendern(&self, id: &str, nr: u32, von: &str, aenderung: &SchrittAenderung, jetzt: &Zeit) -> Result<SchrittErgebnis, SpeicherFehler> {
        let zeit = zeit_text(jetzt);
        self.schritt_schreiben(id, von, jetzt, |a| Ok((nr, aendern_in(a, nr, von, aenderung, &zeit)?)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::vertrag::Stufe;
    use crate::speicher::arbeiten::{ArbeitAenderung, Fortschreiben, NeueArbeit, VERLAUF_HOECHSTENS};
    use chrono::{FixedOffset, TimeZone};
    use redb::ReadableDatabase;

    const DASH: &str = "chat:dashboard";
    const ARCH: &str = "chat:architektur";

    fn zeit(tag: u32, stunde: u32) -> Zeit {
        FixedOffset::east_opt(3600).unwrap().with_ymd_and_hms(2026, 11, tag, stunde, 0, 0).unwrap()
    }

    fn arbeit(s: &Speicher, stand: ArbeitsStand) -> Arbeit {
        let neu = NeueArbeit {
            projekt: "dashboard".into(),
            von: DASH.into(),
            titel: "Etappe 6".into(),
            stand,
            wartet_auf: None,
            verweis: None,
            prioritaet: Stufe::Bald,
            frist: None,
            notiz: None,
            etappe: None,
        };
        s.arbeit_anlegen(neu, &zeit(5, 8)).unwrap()
    }

    fn stand(st: ArbeitsStand) -> SchrittAenderung {
        SchrittAenderung { stand: Some(st), ..Default::default() }
    }

    fn anbieten(an: &str) -> SchrittAenderung {
        SchrittAenderung { anbieten: Some(an.into()), ..Default::default() }
    }

    fn warte_auf(w: &str) -> SchrittAenderung {
        SchrittAenderung { stand: Some(ArbeitsStand::Wartet), wartet_auf: Some(Some(w.into())), ..Default::default() }
    }

    fn schritt(a: &Arbeit, nr: u32) -> &Schritt {
        a.schritte.iter().find(|s| s.nr == nr).unwrap()
    }

    /// Die Bytes der Arbeit in der Datei.
    fn roh(s: &Speicher, id: &str) -> Vec<u8> {
        let txn = s.db.begin_read().unwrap();
        let t = txn.open_table(ARBEITEN).unwrap();
        redb::ReadableTable::get(&t, id).unwrap().unwrap().value().to_vec()
    }

    /// Fall 6b aus dem Zielbild mit Funde-Runde: endet ohne Stillstand, mit
    /// richtigem abgeleitetem Stand; die Zahl der Schritte wächst nicht;
    /// gegenseitiges Warten lehnt der Server ab (422).
    #[test]
    fn fall_6b_mit_funde_runde_endet_ohne_stillstand() {
        let s = Speicher::temporaer();
        let a = arbeit(&s, ArbeitsStand::Laeuft);
        let t = zeit(5, 9);
        // 1. Dashboard legt an; Schritt 1 „bisherige Arbeit“ legt der Server an.
        let r = s.schritt_anlegen(&a.id, DASH, "6b bauen", None, &t).unwrap();
        assert_eq!((r.nr, r.arbeit.schritte.len()), (2, 2));
        let eins = schritt(&r.arbeit, 1);
        assert_eq!((eins.titel.as_str(), eins.stand, eins.besitzer.as_deref()), (TITEL_BISHER, ArbeitsStand::Laeuft, Some(DASH)));
        assert_eq!(s.schritt_anlegen(&a.id, DASH, "6b Abnahme", Some(ARCH.into()), &t).unwrap().nr, 3);
        assert_eq!(s.schritt_anlegen(&a.id, DASH, "6b zusammenführen", None, &t).unwrap().nr, 4);
        assert_eq!(s.schritt_anlegen(&a.id, DASH, "6d bauen", None, &t).unwrap().nr, 5);
        // 2. Dashboard übernimmt 2, bietet 3 an Architektur an, setzt 2 fertig.
        s.schritt_uebernehmen(&a.id, 2, DASH, None, &t).unwrap();
        let r = s.schritt_aendern(&a.id, 3, DASH, &anbieten(ARCH), &t).unwrap();
        assert_eq!(r.wecken, [Wecken { nr: 3, chat: ARCH.into(), grund: WeckGrund::Angeboten }]);
        assert_eq!(schritt(&r.arbeit, 3).angeboten_an.as_deref(), Some(ARCH));
        s.schritt_aendern(&a.id, 2, DASH, &stand(ArbeitsStand::Fertig), &t).unwrap();
        // Dashboard kann den angebotenen Schritt 3 nicht für sich nehmen (fuer).
        assert!(matches!(s.schritt_uebernehmen(&a.id, 3, DASH, None, &t), Err(SpeicherFehler::Ungueltig(_))));
        // 4. Architektur übernimmt 3: erst jetzt ist die Übergabe vollzogen.
        let r = s.schritt_uebernehmen(&a.id, 3, ARCH, None, &t).unwrap();
        let drei = schritt(&r.arbeit, 3);
        assert_eq!((drei.besitzer.as_deref(), drei.stand, drei.angeboten_an.as_deref()), (Some(ARCH), ArbeitsStand::Laeuft, None));
        // 5b. Funde: Architektur bietet den fertigen Schritt 2 Dashboard wieder an und wartet darauf.
        let r = s.schritt_aendern(&a.id, 2, ARCH, &anbieten(DASH), &t).unwrap();
        let zwei = schritt(&r.arbeit, 2);
        assert_eq!((zwei.stand, zwei.besitzer.as_deref(), zwei.angeboten_an.as_deref()), (ArbeitsStand::AlsNaechstes, None, Some(DASH)));
        s.schritt_aendern(&a.id, 3, ARCH, &warte_auf("schritt:2"), &t).unwrap();
        // 6. Dashboard übernimmt 2 und will auf Schritt 3 warten: Kreis, 422 mit Satz.
        s.schritt_uebernehmen(&a.id, 2, DASH, None, &t).unwrap();
        let vorher = roh(&s, &a.id);
        let e = s.schritt_aendern(&a.id, 2, DASH, &warte_auf("schritt:3"), &t).unwrap_err();
        assert!(
            matches!(&e, SpeicherFehler::Ungueltig(satz) if satz.contains("Schritt 3 wartet schon auf Schritt 2") && satz.contains("Biete zuerst Schritt 2 an")),
            "{e}"
        );
        assert_eq!(roh(&s, &a.id), vorher, "nichts geschrieben");
        // Dashboard behebt und setzt 2 fertig: Schritt 3 läuft wieder, Architektur wird geweckt.
        let r = s.schritt_aendern(&a.id, 2, DASH, &stand(ArbeitsStand::Fertig), &zeit(5, 10)).unwrap();
        assert_eq!(r.wecken, [Wecken { nr: 3, chat: ARCH.into(), grund: WeckGrund::Zurueckgeweckt }]);
        let drei = schritt(&r.arbeit, 3);
        assert_eq!((drei.stand, drei.wartet_auf.as_deref(), drei.besitzer.as_deref()), (ArbeitsStand::Laeuft, None, Some(ARCH)));
        assert!(r.arbeit.verlauf.iter().any(|v| v.art == ArbeitVerlaufArt::SchrittGeweckt && v.schritt == Some(3)));
        // 5a. Ja: Architektur bietet 4 Dashboard an und setzt 3 fertig.
        s.schritt_aendern(&a.id, 4, ARCH, &anbieten(DASH), &t).unwrap();
        s.schritt_aendern(&a.id, 3, ARCH, &stand(ArbeitsStand::Fertig), &t).unwrap();
        for nr in [4, 5, 1] {
            s.schritt_uebernehmen(&a.id, nr, DASH, None, &t).unwrap();
            let r = s.schritt_aendern(&a.id, nr, DASH, &stand(ArbeitsStand::Fertig), &t).unwrap();
            let fertig = nr == 1;
            assert_eq!(r.arbeit.stand == ArbeitsStand::Fertig, fertig, "nach Schritt {nr}");
        }
        let a = s.arbeit(&a.id).unwrap().unwrap();
        assert_eq!((a.stand, a.schritte.len()), (ArbeitsStand::Fertig, 5), "die Zahl der Schritte wächst nicht");
        assert!(a.schritte.iter().all(|x| x.stand == ArbeitsStand::Fertig));
    }

    /// Abgeleiteter Stand: wartet einer auf den Inhaber, wartet sie (auch wenn
    /// einer läuft); sonst läuft einer, läuft sie; sonst wartet einer; alle
    /// fertig: fertig; sonst als nächstes. Direkt setzen: 422.
    #[test]
    fn abgeleiteter_stand_und_direkt_setzen_abgelehnt() {
        let s = Speicher::temporaer();
        let a = arbeit(&s, ArbeitsStand::AlsNaechstes);
        let t = zeit(5, 9);
        let r = s.schritt_anlegen(&a.id, DASH, "eins", None, &t).unwrap();
        assert_eq!((r.nr, r.arbeit.schritte.len(), r.arbeit.stand), (1, 1, ArbeitsStand::AlsNaechstes), "kein Schritt „bisherige Arbeit“");
        s.schritt_anlegen(&a.id, DASH, "zwei", None, &t).unwrap();
        assert_eq!(s.schritt_uebernehmen(&a.id, 1, DASH, None, &t).unwrap().arbeit.stand, ArbeitsStand::Laeuft);
        let w = SchrittAenderung {
            stand: Some(ArbeitsStand::Wartet),
            wartet_auf: Some(Some(INHABER.into())),
            verweis: Some(Some("a-20261105-0001".into())),
            ..Default::default()
        };
        let r = s.schritt_aendern(&a.id, 1, DASH, &w, &t).unwrap();
        assert_eq!((r.arbeit.stand, r.arbeit.wartet_auf.as_deref()), (ArbeitsStand::Wartet, Some(INHABER)));
        // Ohne verweis geht warten auf den Inhaber nicht.
        let ohne = warte_auf(INHABER);
        s.schritt_uebernehmen(&a.id, 2, ARCH, None, &t).unwrap();
        assert!(matches!(s.schritt_aendern(&a.id, 2, ARCH, &ohne, &t), Err(SpeicherFehler::Ungueltig(_))));
        // Einer läuft, einer wartet auf den Inhaber: die Arbeit wartet auf ihn,
        // mit dem verweis des wartenden Schritts.
        let a2 = s.arbeit(&a.id).unwrap().unwrap();
        assert_eq!(
            (a2.stand, a2.wartet_auf.as_deref(), a2.verweis.as_deref(), schritt(&a2, 2).stand),
            (ArbeitsStand::Wartet, Some(INHABER), Some("a-20261105-0001"), ArbeitsStand::Laeuft)
        );
        let direkt = ArbeitAenderung { stand: Some(ArbeitsStand::Fertig), von: DASH.into(), ..Default::default() };
        let verweis = ArbeitAenderung { verweis: Some(Some("a-20261105-0002".into())), von: DASH.into(), ..Default::default() };
        for aenderung in [direkt, verweis] {
            assert!(matches!(
                s.arbeit_fortschreiben(&a.id, &aenderung, &t).unwrap(),
                Fortschreiben::Ungueltig(satz) if satz == crate::speicher::arbeiten::SATZ_STAND_AUS_SCHRITTEN
            ));
        }
        // Die Notiz der Arbeit geht weiter (der Besitzer ist verantwortlich).
        let notiz = ArbeitAenderung { notiz: Some(Some("n".into())), von: DASH.into(), ..Default::default() };
        assert!(matches!(s.arbeit_fortschreiben(&a.id, &notiz, &t).unwrap(), Fortschreiben::Geaendert(_)));
        for (nr, von) in [(1, DASH), (2, ARCH)] {
            s.schritt_aendern(&a.id, nr, von, &stand(ArbeitsStand::Fertig), &t).unwrap();
        }
        let a = s.arbeit(&a.id).unwrap().unwrap();
        assert_eq!((a.stand, a.wartet_auf), (ArbeitsStand::Fertig, None));
    }

    /// Ein fremder Besitzer: übernehmen und ändern 409 mit Besitzer und seit,
    /// nichts geschrieben. Ein zweites Übernehmen durch denselben: unverändert.
    #[test]
    fn fremd_409_ohne_schreiben_und_doppelt_unveraendert() {
        let s = Speicher::temporaer();
        let a = arbeit(&s, ArbeitsStand::AlsNaechstes);
        s.schritt_anlegen(&a.id, DASH, "bauen", None, &zeit(5, 9)).unwrap();
        let r = s.schritt_uebernehmen(&a.id, 1, DASH, None, &zeit(5, 10)).unwrap();
        assert!(r.geaendert);
        let vorher = roh(&s, &a.id);
        let zweit = s.schritt_uebernehmen(&a.id, 1, DASH, None, &zeit(5, 11)).unwrap();
        assert!(!zweit.geaendert && zweit.wecken.is_empty());
        assert_eq!(roh(&s, &a.id), vorher, "doppelt: nichts geschrieben");
        let besetzt = |e: SpeicherFehler| match e {
            SpeicherFehler::Besetzt { besitzer, seit } => (besitzer, seit),
            e => panic!("{e:?}"),
        };
        let erwartet = (DASH.to_string(), "2026-11-05T10:00:00+01:00".to_string());
        assert_eq!(besetzt(s.schritt_uebernehmen(&a.id, 1, ARCH, None, &zeit(5, 11)).unwrap_err()), erwartet);
        for aenderung in [stand(ArbeitsStand::Fertig), anbieten(ARCH), SchrittAenderung { freigeben: true, ..Default::default() }] {
            assert_eq!(besetzt(s.schritt_aendern(&a.id, 1, ARCH, &aenderung, &zeit(5, 11)).unwrap_err()), erwartet);
        }
        // Mit statt, aber nicht verwaist: auch 409.
        assert_eq!(besetzt(s.schritt_uebernehmen(&a.id, 1, ARCH, Some(DASH), &zeit(5, 12)).unwrap_err()), erwartet);
        assert_eq!(roh(&s, &a.id), vorher, "fremd: nichts geschrieben");
        // Unbekannter Schritt und Arbeit: nicht gefunden.
        assert!(matches!(s.schritt_uebernehmen(&a.id, 9, DASH, None, &zeit(5, 12)), Err(SpeicherFehler::NichtGefunden(_))));
        assert!(matches!(s.schritt_anlegen("ar-19990101-0001", DASH, "x", None, &zeit(5, 12)), Err(SpeicherFehler::NichtGefunden(_))));
        // Ein freier Schritt lässt sich nicht ändern, ohne ihn zu übernehmen.
        s.schritt_anlegen(&a.id, DASH, "frei", None, &zeit(5, 12)).unwrap();
        assert!(matches!(s.schritt_aendern(&a.id, 2, ARCH, &stand(ArbeitsStand::Laeuft), &zeit(5, 12)), Err(SpeicherFehler::Ungueltig(_))));
        // Anbieten darf nur, wer in der Arbeit einen Schritt hat.
        assert!(matches!(s.schritt_aendern(&a.id, 2, ARCH, &anbieten(DASH), &zeit(5, 12)), Err(SpeicherFehler::Ungueltig(_))));
        assert!(s.schritt_aendern(&a.id, 2, DASH, &anbieten(ARCH), &zeit(5, 12)).unwrap().geaendert);
    }

    /// Zwei Chats übernehmen zugleich denselben Schritt: genau einer gewinnt,
    /// der andere bekommt 409 (echte Fäden gegen denselben Speicher).
    #[test]
    fn zwei_zugleich_genau_ein_409() {
        let s = std::sync::Arc::new(Speicher::temporaer());
        for runde in 0..10 {
            let a = arbeit(&s, ArbeitsStand::AlsNaechstes);
            s.schritt_anlegen(&a.id, DASH, "bauen", None, &zeit(5, 9)).unwrap();
            let start = std::sync::Barrier::new(2);
            let ergebnisse: Vec<Result<SchrittErgebnis, SpeicherFehler>> = std::thread::scope(|sc| {
                let faeden: Vec<_> = [DASH, ARCH]
                    .into_iter()
                    .map(|von| {
                        let (s, start, id) = (&s, &start, &a.id);
                        sc.spawn(move || {
                            start.wait();
                            s.schritt_uebernehmen(id, 1, von, None, &zeit(5, 10))
                        })
                    })
                    .collect();
                faeden.into_iter().map(|f| f.join().unwrap()).collect()
            });
            let gewonnen: Vec<&SchrittErgebnis> = ergebnisse.iter().filter_map(|r| r.as_ref().ok()).collect();
            assert_eq!(gewonnen.len(), 1, "Runde {runde}: {ergebnisse:?}");
            let sieger = gewonnen[0].arbeit.schritte[0].besitzer.clone().unwrap();
            let verlierer = ergebnisse.iter().find_map(|r| r.as_ref().err()).unwrap();
            assert!(matches!(verlierer, SpeicherFehler::Besetzt { besitzer, .. } if *besitzer == sieger), "{verlierer:?}");
            assert_eq!(s.arbeit(&a.id).unwrap().unwrap().schritte[0].besitzer, Some(sieger));
        }
    }

    /// Warten: nicht auf sich selbst, nur auf einen laufenden oder
    /// angebotenen Schritt, keine Kette zu sich selbst zurück (422).
    #[test]
    fn warten_auf_schritte_und_kreis() {
        let s = Speicher::temporaer();
        let a = arbeit(&s, ArbeitsStand::AlsNaechstes);
        let t = zeit(5, 9);
        for titel in ["a", "b", "c"] {
            s.schritt_anlegen(&a.id, DASH, titel, None, &t).unwrap();
        }
        s.schritt_uebernehmen(&a.id, 1, DASH, None, &t).unwrap();
        s.schritt_uebernehmen(&a.id, 2, ARCH, None, &t).unwrap();
        let satz = |e: SpeicherFehler| match e {
            SpeicherFehler::Ungueltig(satz) => satz,
            e => panic!("{e:?}"),
        };
        assert!(satz(s.schritt_aendern(&a.id, 1, DASH, &warte_auf("schritt:1"), &t).unwrap_err()).contains("nicht auf sich selbst"));
        let e = satz(s.schritt_aendern(&a.id, 1, DASH, &warte_auf("schritt:3"), &t).unwrap_err());
        assert!(e.contains("Biete Schritt 3 zuerst an"), "{e}");
        assert!(satz(s.schritt_aendern(&a.id, 1, DASH, &warte_auf("schritt:9"), &t).unwrap_err()).contains("gibt es"));
        assert!(satz(s.schritt_aendern(&a.id, 1, DASH, &warte_auf("chat:x"), &t).unwrap_err()).contains("schritt:<nr>"));
        // 2 wartet auf den laufenden 1; dann will 1 auf 2 warten: Kreis.
        s.schritt_aendern(&a.id, 2, ARCH, &warte_auf("schritt:1"), &t).unwrap();
        let e = satz(s.schritt_aendern(&a.id, 1, DASH, &warte_auf("schritt:2"), &t).unwrap_err());
        assert!(e.contains("Schritt 2 wartet schon auf Schritt 1") && e.contains("Biete zuerst Schritt 1 an"), "{e}");
        // Auf einen angebotenen Schritt warten geht; freigeben weckt den Wartenden.
        s.schritt_aendern(&a.id, 3, DASH, &anbieten("chat:bauplan"), &t).unwrap();
        s.schritt_aendern(&a.id, 1, DASH, &warte_auf("schritt:3"), &t).unwrap();
        // 1 wartet jetzt; 2 wartete auf 1 und wartet weiter (1 hat einen Besitzer).
        let a2 = s.arbeit(&a.id).unwrap().unwrap();
        assert_eq!((a2.stand, a2.wartet_auf.as_deref()), (ArbeitsStand::Wartet, Some("chat:bauplan")));
        let r = s.schritt_aendern(&a.id, 1, DASH, &SchrittAenderung { freigeben: true, ..Default::default() }, &t).unwrap();
        assert_eq!(r.wecken, [Wecken { nr: 2, chat: ARCH.into(), grund: WeckGrund::Zurueckgeweckt }]);
        assert_eq!((schritt(&r.arbeit, 1).besitzer.clone(), schritt(&r.arbeit, 2).stand), (None, ArbeitsStand::Laeuft));
    }

    /// Wechselt das Ziel nach Beginn des Wartens, bleibt die Notiz am
    /// Wartenden möglich. Einen fertigen fremden Schritt übernimmt man nicht
    /// (409, nichts geschrieben). Wer etwas angeboten bekam, ändert erst nach
    /// dem Übernehmen (422, kein 409 mit sich selbst).
    #[test]
    fn wartender_notiz_fertig_fremd_und_angeboten() {
        let s = Speicher::temporaer();
        let a = arbeit(&s, ArbeitsStand::AlsNaechstes);
        let t = zeit(5, 9);
        for titel in ["bauen", "abnahme", "rest"] {
            s.schritt_anlegen(&a.id, DASH, titel, None, &t).unwrap();
        }
        s.schritt_uebernehmen(&a.id, 1, DASH, None, &t).unwrap();
        s.schritt_uebernehmen(&a.id, 2, ARCH, None, &t).unwrap();
        s.schritt_aendern(&a.id, 2, ARCH, &warte_auf("schritt:1"), &t).unwrap();
        s.schritt_aendern(&a.id, 1, DASH, &warte_auf(EXTERN), &t).unwrap();
        let n = SchrittAenderung { notiz: Some(Some("noch da".into())), ..Default::default() };
        let r = s.schritt_aendern(&a.id, 2, ARCH, &n, &zeit(5, 10)).unwrap();
        let zwei = schritt(&r.arbeit, 2);
        assert_eq!((zwei.notiz.as_deref(), zwei.seit.as_str()), (Some("noch da"), "2026-11-05T10:00:00+01:00"));
        // Ein neues Warten auf den nicht mehr laufenden Schritt 1 prüft weiter.
        assert!(matches!(s.schritt_aendern(&a.id, 2, ARCH, &warte_auf("schritt:1"), &t), Err(SpeicherFehler::Ungueltig(_))));
        // Fertig mit Besitzer: ein anderer bekommt 409.
        s.schritt_aendern(&a.id, 1, DASH, &stand(ArbeitsStand::Fertig), &t).unwrap();
        let vorher = roh(&s, &a.id);
        assert!(matches!(
            s.schritt_uebernehmen(&a.id, 1, "chat:x", None, &t),
            Err(SpeicherFehler::Besetzt { besitzer, .. }) if besitzer == DASH
        ));
        assert_eq!(roh(&s, &a.id), vorher, "nichts geschrieben");
        // Angeboten an Architektur: dessen Notiz vor dem Übernehmen gibt 422.
        s.schritt_aendern(&a.id, 3, DASH, &anbieten(ARCH), &t).unwrap();
        let e = s.schritt_aendern(&a.id, 3, ARCH, &n, &t).unwrap_err();
        assert!(matches!(&e, SpeicherFehler::Ungueltig(satz) if satz.contains("erst übernehmen")), "{e:?}");
        // Der eigene fertige Schritt geht über den Besitzer wieder auf.
        assert!(s.schritt_uebernehmen(&a.id, 1, DASH, None, &t).unwrap().geaendert);
    }

    /// Verwaist: nach 24 Stunden ohne Schreiben darf ein anderer übernehmen,
    /// wenn er den alten nennt (statt); im Verlauf. Nie bei wartenden
    /// Schritten (extern, auf einen Schritt); bei fuer nur dieser Chat.
    /// Angebote ebenso.
    #[test]
    fn verwaist_nach_24_stunden_mit_statt() {
        let s = Speicher::temporaer();
        let a = arbeit(&s, ArbeitsStand::AlsNaechstes);
        let t = zeit(5, 9);
        for (titel, fuer) in [("läuft", None), ("wartet extern", None), ("angeboten", None), ("für", Some(ARCH)), ("wartet auf 1", None)] {
            s.schritt_anlegen(&a.id, DASH, titel, fuer.map(str::to_string), &t).unwrap();
        }
        s.schritt_uebernehmen(&a.id, 1, DASH, None, &t).unwrap();
        s.schritt_uebernehmen(&a.id, 2, DASH, None, &t).unwrap();
        s.schritt_aendern(&a.id, 2, DASH, &warte_auf(EXTERN), &t).unwrap();
        s.schritt_aendern(&a.id, 3, DASH, &anbieten("chat:bauplan"), &t).unwrap();
        s.schritt_uebernehmen(&a.id, 4, ARCH, None, &t).unwrap();
        s.schritt_uebernehmen(&a.id, 5, DASH, None, &t).unwrap();
        s.schritt_aendern(&a.id, 5, DASH, &warte_auf("schritt:1"), &t).unwrap();
        let knapp = zeit(6, 9);
        assert!(matches!(s.schritt_uebernehmen(&a.id, 1, ARCH, Some(DASH), &knapp), Err(SpeicherFehler::Besetzt { .. })));
        let spaeter = zeit(6, 10);
        assert!(matches!(s.schritt_uebernehmen(&a.id, 1, ARCH, None, &spaeter), Err(SpeicherFehler::Besetzt { .. })), "ohne statt");
        assert!(matches!(s.schritt_uebernehmen(&a.id, 1, ARCH, Some("chat:x"), &spaeter), Err(SpeicherFehler::Besetzt { .. })), "falsches statt");
        let r = s.schritt_uebernehmen(&a.id, 1, ARCH, Some(DASH), &spaeter).unwrap();
        assert_eq!(schritt(&r.arbeit, 1).besitzer.as_deref(), Some(ARCH));
        let v = r.arbeit.verlauf.last().unwrap();
        assert_eq!((v.art, v.schritt, v.von.as_str()), (ArbeitVerlaufArt::SchrittUebernommen, Some(1), ARCH));
        assert!(v.text.contains("statt chat:dashboard"), "{}", v.text);
        // Wartet extern oder auf einen Schritt: nie verwaist.
        for nr in [2, 5] {
            assert!(matches!(s.schritt_uebernehmen(&a.id, nr, ARCH, Some(DASH), &zeit(9, 9)), Err(SpeicherFehler::Besetzt { .. })), "Schritt {nr}");
        }
        // Ein Angebot, über 24 Stunden nicht übernommen: statt nennt den Empfänger.
        let r = s.schritt_uebernehmen(&a.id, 3, ARCH, Some("chat:bauplan"), &spaeter).unwrap();
        assert_eq!(schritt(&r.arbeit, 3).besitzer.as_deref(), Some(ARCH));
        // fuer: nur dieser Chat, auch wenn verwaist.
        assert!(matches!(s.schritt_uebernehmen(&a.id, 4, DASH, Some(ARCH), &spaeter), Err(SpeicherFehler::Ungueltig(_))));
        // Eine Notiz zählt als Schreiben: danach nicht mehr verwaist.
        let n = SchrittAenderung { notiz: Some(Some("noch dran".into())), ..Default::default() };
        s.schritt_aendern(&a.id, 1, ARCH, &n, &zeit(7, 9)).unwrap();
        assert!(matches!(s.schritt_uebernehmen(&a.id, 1, DASH, Some(ARCH), &zeit(7, 20)), Err(SpeicherFehler::Besetzt { .. })));
    }

    /// Grenzen: 20 Schritte je Arbeit (mit „bisherige Arbeit“), 500 Einträge
    /// im Verlauf, 30 offene Arbeiten auch beim Wiederöffnen; je 429 und
    /// nichts geschrieben.
    #[test]
    fn grenzen_schritte_verlauf_und_wiederoeffnen() {
        let s = Speicher::temporaer();
        let t = zeit(5, 9);
        let a = arbeit(&s, ArbeitsStand::Laeuft);
        for i in 0..SCHRITTE_HOECHSTENS - 1 {
            s.schritt_anlegen(&a.id, DASH, &format!("s{i}"), None, &t).unwrap();
        }
        assert_eq!(s.arbeit(&a.id).unwrap().unwrap().schritte.len(), SCHRITTE_HOECHSTENS, "mit Schritt 1");
        let vorher = roh(&s, &a.id);
        let e = s.schritt_anlegen(&a.id, DASH, "zu viel", None, &t).unwrap_err();
        assert!(matches!(&e, SpeicherFehler::Grenze(satz) if satz.contains("20")), "{e}");
        assert_eq!(roh(&s, &a.id), vorher);
        // Verlauf: bei 500 Einträgen ist Schluss.
        let b = arbeit(&s, ArbeitsStand::AlsNaechstes);
        s.schritt_anlegen(&b.id, DASH, "n", None, &t).unwrap();
        s.schritt_uebernehmen(&b.id, 1, DASH, None, &t).unwrap();
        {
            let mut voll = s.arbeit(&b.id).unwrap().unwrap();
            let muster = voll.verlauf[0].clone();
            voll.verlauf.resize(VERLAUF_HOECHSTENS - 1, muster);
            let txn = s.db.begin_write().unwrap();
            txn.open_table(ARBEITEN).unwrap().insert(b.id.as_str(), serde_json::to_vec(&voll).unwrap().as_slice()).unwrap();
            txn.commit().unwrap();
        }
        let n = |text: &str| SchrittAenderung { notiz: Some(Some(text.into())), ..Default::default() };
        s.schritt_aendern(&b.id, 1, DASH, &n("499 → 500"), &t).unwrap();
        let vorher = roh(&s, &b.id);
        assert!(matches!(s.schritt_aendern(&b.id, 1, DASH, &n("501"), &t), Err(SpeicherFehler::Grenze(_))));
        assert_eq!(roh(&s, &b.id), vorher);
        // 30 offene: eine fertige Arbeit öffnet sich über einen neuen Schritt nicht wieder.
        let fertig = arbeit(&s, ArbeitsStand::Fertig);
        for _ in 0..OFFEN_JE_PROJEKT - 2 {
            arbeit(&s, ArbeitsStand::AlsNaechstes);
        }
        let vorher = roh(&s, &fertig.id);
        assert!(matches!(s.schritt_anlegen(&fertig.id, DASH, "wieder auf", None, &t), Err(SpeicherFehler::Grenze(_))));
        assert_eq!(roh(&s, &fertig.id), vorher);
    }

    /// Etappe B8: Ein aus den Schritten abgeleiteter Wechsel des Arbeitsstands
    /// steht im Verlauf (von, wer den Schritt geändert hat); ein Schritt ohne
    /// Wechsel des Arbeitsstands schreibt keinen.
    #[test]
    fn abgeleiteter_stand_wechsel_im_verlauf() {
        let s = Speicher::temporaer();
        let t = zeit(5, 9);
        let a = arbeit(&s, ArbeitsStand::AlsNaechstes);
        s.schritt_anlegen(&a.id, DASH, "a", None, &t).unwrap();
        s.schritt_anlegen(&a.id, DASH, "b", None, &t).unwrap();
        s.schritt_uebernehmen(&a.id, 1, ARCH, None, &t).unwrap();
        s.schritt_uebernehmen(&a.id, 2, DASH, None, &t).unwrap();
        s.schritt_aendern(&a.id, 1, ARCH, &stand(ArbeitsStand::Fertig), &t).unwrap();
        let r = s.schritt_aendern(&a.id, 2, DASH, &stand(ArbeitsStand::Fertig), &t).unwrap();
        let staende: Vec<(&str, &str, &str)> =
            r.arbeit.verlauf.iter().filter(|v| v.art == ArbeitVerlaufArt::Stand).map(|v| (v.von.as_str(), v.alt.as_str(), v.neu.as_str())).collect();
        assert_eq!(staende, [(ARCH, "als-naechstes", "laeuft"), (DASH, "laeuft", "fertig")]);
        assert!(r.arbeit.verlauf.iter().filter(|v| v.art == ArbeitVerlaufArt::Stand).all(|v| v.schritt.is_none()));
    }

    /// Voller Verlauf (500): fertig weckt den Wartenden trotzdem, mit
    /// Weck-Vorgang (sonst hinge er); eine Notiz bleibt bei 429.
    #[test]
    fn voller_verlauf_fertig_weckt_trotzdem() {
        let s = Speicher::temporaer();
        let t = zeit(5, 9);
        let a = arbeit(&s, ArbeitsStand::AlsNaechstes);
        for titel in ["a", "b", "c"] {
            s.schritt_anlegen(&a.id, DASH, titel, None, &t).unwrap();
        }
        s.schritt_uebernehmen(&a.id, 2, DASH, None, &t).unwrap();
        s.schritt_uebernehmen(&a.id, 3, ARCH, None, &t).unwrap();
        s.schritt_aendern(&a.id, 3, ARCH, &warte_auf("schritt:2"), &t).unwrap();
        {
            let mut voll = s.arbeit(&a.id).unwrap().unwrap();
            let muster = voll.verlauf[0].clone();
            voll.verlauf.resize(VERLAUF_HOECHSTENS, muster);
            let txn = s.db.begin_write().unwrap();
            txn.open_table(ARBEITEN).unwrap().insert(a.id.as_str(), serde_json::to_vec(&voll).unwrap().as_slice()).unwrap();
            txn.commit().unwrap();
        }
        let n = SchrittAenderung { notiz: Some(Some("n".into())), ..Default::default() };
        assert!(matches!(s.schritt_aendern(&a.id, 2, DASH, &n, &t), Err(SpeicherFehler::Grenze(_))));
        let r = s.schritt_aendern(&a.id, 2, DASH, &stand(ArbeitsStand::Fertig), &zeit(5, 10)).unwrap();
        assert_eq!(r.wecken, [Wecken { nr: 3, chat: ARCH.into(), grund: WeckGrund::Zurueckgeweckt }]);
        let drei = schritt(&r.arbeit, 3);
        assert_eq!((drei.stand, drei.vorgang.is_some(), r.ereignisse.len()), (ArbeitsStand::Laeuft, true, 1));
        assert_eq!(r.arbeit.verlauf.len(), VERLAUF_HOECHSTENS + 2, "fertig und geweckt");
    }

    /// Ohne Schritte (T3, von Pflicht): nur der Besitzer ändert (409, auch den
    /// Titel), den Titel einer Arbeit ohne Besitzer nur der Chat des Projekts;
    /// beides in der Transaktion geprüft, nichts geschrieben. Eine Arbeit ohne
    /// Besitzer übernimmt, wer sie auf laeuft setzt.
    #[test]
    fn arbeit_ohne_schritte_besitzer_und_titel() {
        let s = Speicher::temporaer();
        let t = zeit(5, 9);
        let a = arbeit(&s, ArbeitsStand::Laeuft);
        assert_eq!(a.besitzer.as_deref(), Some(DASH));
        let vorher = roh(&s, &a.id);
        let fremd = ArbeitAenderung { stand: Some(ArbeitsStand::Fertig), von: ARCH.into(), ..Default::default() };
        let fremder_titel = ArbeitAenderung { titel: Some("neu".into()), von: ARCH.into(), ..Default::default() };
        for aenderung in [fremd, fremder_titel] {
            assert!(matches!(s.arbeit_fortschreiben(&a.id, &aenderung, &t), Err(SpeicherFehler::Besetzt { besitzer, .. }) if besitzer == DASH));
        }
        assert_eq!(roh(&s, &a.id), vorher);
        let b = arbeit(&s, ArbeitsStand::AlsNaechstes);
        assert_eq!(b.besitzer, None);
        let titel = |von: &str| ArbeitAenderung { titel: Some("neu".into()), von: von.into(), ..Default::default() };
        assert!(matches!(s.arbeit_fortschreiben(&b.id, &titel(ARCH), &t).unwrap(), Fortschreiben::FremderTitel));
        assert!(matches!(s.arbeit_fortschreiben(&b.id, &titel(DASH), &t).unwrap(), Fortschreiben::Geaendert(_)));
        let auf = ArbeitAenderung { stand: Some(ArbeitsStand::Laeuft), von: ARCH.into(), ..Default::default() };
        let Fortschreiben::Geaendert(b) = s.arbeit_fortschreiben(&b.id, &auf, &t).unwrap() else { panic!() };
        assert_eq!(b.besitzer.as_deref(), Some(ARCH));
        assert_eq!(b.verlauf.last().map(|v| v.art), Some(ArbeitVerlaufArt::Besitzer));
        // Fertig oder als nächstes: der Besitzer wird frei (steht im Verlauf), ein anderer kann starten.
        for (st, wer) in [(ArbeitsStand::Fertig, ARCH), (ArbeitsStand::AlsNaechstes, DASH)] {
            let zu = ArbeitAenderung { stand: Some(st), von: wer.into(), ..Default::default() };
            let Fortschreiben::Geaendert(b) = s.arbeit_fortschreiben(&b.id, &zu, &t).unwrap() else { panic!() };
            let v = b.verlauf.last().unwrap();
            assert_eq!((b.besitzer, v.art, v.alt.as_str()), (None, ArbeitVerlaufArt::Besitzer, wer));
            let neu = ArbeitAenderung { stand: Some(ArbeitsStand::Laeuft), von: if wer == ARCH { DASH } else { ARCH }.into(), ..Default::default() };
            assert!(matches!(s.arbeit_fortschreiben(&b.id, &neu, &t).unwrap(), Fortschreiben::Geaendert(_)));
        }
    }

    /// Y wartet auf schritt:X, X wartet danach auf extern: Y setzt seinen
    /// verweis (unverändertes Warten, keine neue Prüfung); beim Inhaber zählt er.
    #[test]
    fn verweis_am_unveraenderten_warten() {
        let s = Speicher::temporaer();
        let a = arbeit(&s, ArbeitsStand::AlsNaechstes);
        let t = zeit(5, 9);
        for titel in ["x", "y"] {
            s.schritt_anlegen(&a.id, DASH, titel, None, &t).unwrap();
        }
        s.schritt_uebernehmen(&a.id, 1, DASH, None, &t).unwrap();
        s.schritt_uebernehmen(&a.id, 2, ARCH, None, &t).unwrap();
        s.schritt_aendern(&a.id, 2, ARCH, &warte_auf("schritt:1"), &t).unwrap();
        s.schritt_aendern(&a.id, 1, DASH, &warte_auf(EXTERN), &t).unwrap();
        let v = SchrittAenderung { verweis: Some(Some("a-20261105-0001".into())), ..Default::default() };
        assert!(s.schritt_aendern(&a.id, 2, ARCH, &v, &t).unwrap().geaendert);
        let mut inhaber = warte_auf(INHABER);
        inhaber.verweis = Some(Some("a-20261105-0002".into()));
        s.schritt_aendern(&a.id, 1, DASH, &inhaber, &t).unwrap();
        let weg = SchrittAenderung { verweis: Some(None), ..Default::default() };
        assert!(matches!(s.schritt_aendern(&a.id, 1, DASH, &weg, &t), Err(SpeicherFehler::Ungueltig(_))));
    }

    /// Der Besitzer setzt seinen Schritt auf als nächstes: wie freigeben. Der
    /// auf ihn Wartende läuft wieder, ein anderer Chat kann ihn übernehmen.
    #[test]
    fn als_naechstes_gibt_frei_und_weckt() {
        let s = Speicher::temporaer();
        let a = arbeit(&s, ArbeitsStand::AlsNaechstes);
        let t = zeit(5, 9);
        for titel in ["x", "y"] {
            s.schritt_anlegen(&a.id, DASH, titel, None, &t).unwrap();
        }
        s.schritt_uebernehmen(&a.id, 1, DASH, None, &t).unwrap();
        s.schritt_uebernehmen(&a.id, 2, ARCH, None, &t).unwrap();
        s.schritt_aendern(&a.id, 2, ARCH, &warte_auf("schritt:1"), &t).unwrap();
        let r = s.schritt_aendern(&a.id, 1, DASH, &stand(ArbeitsStand::AlsNaechstes), &t).unwrap();
        assert_eq!(r.wecken, [Wecken { nr: 2, chat: ARCH.into(), grund: WeckGrund::Zurueckgeweckt }]);
        let (x, y) = (schritt(&r.arbeit, 1), schritt(&r.arbeit, 2));
        assert_eq!((x.stand, x.besitzer.as_deref(), y.stand, y.wartet_auf.as_deref()), (ArbeitsStand::AlsNaechstes, None, ArbeitsStand::Laeuft, None));
        assert!(r.arbeit.verlauf.iter().any(|v| v.schritt == Some(1) && v.alt.contains("dran: chat:dashboard") && !v.neu.contains("dran")));
        assert!(s.schritt_uebernehmen(&a.id, 1, "chat:x", None, &t).unwrap().geaendert);
    }

    /// Wer seinen laufenden Schritt anbietet, ist nicht mehr dran (angeboten
    /// heißt frei); wer auf ihn wartet, wartet weiter auf den Empfänger.
    #[test]
    fn anbieten_gibt_den_besitz_ab() {
        let s = Speicher::temporaer();
        let a = arbeit(&s, ArbeitsStand::AlsNaechstes);
        let t = zeit(5, 9);
        for titel in ["x", "y"] {
            s.schritt_anlegen(&a.id, DASH, titel, None, &t).unwrap();
        }
        s.schritt_uebernehmen(&a.id, 1, DASH, None, &t).unwrap();
        s.schritt_uebernehmen(&a.id, 2, ARCH, None, &t).unwrap();
        s.schritt_aendern(&a.id, 2, ARCH, &warte_auf("schritt:1"), &t).unwrap();
        let r = s.schritt_aendern(&a.id, 1, DASH, &anbieten("chat:bauplan"), &t).unwrap();
        assert_eq!(r.wecken, [Wecken { nr: 1, chat: "chat:bauplan".into(), grund: WeckGrund::Angeboten }]);
        let x = schritt(&r.arbeit, 1);
        assert_eq!((x.besitzer.as_deref(), x.angeboten_an.as_deref()), (None, Some("chat:bauplan")));
        assert_eq!(schritt(&r.arbeit, 2).stand, ArbeitsStand::Wartet);
        let zuletzt = r.arbeit.verlauf.iter().rev().find(|v| v.art == ArbeitVerlaufArt::SchrittGeaendert).unwrap();
        assert_eq!(zuletzt.alt, "laeuft, dran: chat:dashboard");
        // Der alte Besitzer ändert ihn nicht mehr; umlenken darf, wer in der Arbeit einen Schritt hat.
        assert!(s.schritt_aendern(&a.id, 1, DASH, &stand(ArbeitsStand::Laeuft), &t).is_err());
        assert!(s.schritt_aendern(&a.id, 1, ARCH, &anbieten(DASH), &t).unwrap().geaendert);
    }

    /// Schritt 1 übernimmt ein altes Warten (chat:<name>, inhaber ohne
    /// verweis); eine Notiz daran geht, ein neues Warten wird geprüft.
    #[test]
    fn schritt_eins_mit_altem_warten_notiz_geht() {
        let s = Speicher::temporaer();
        let t = zeit(5, 9);
        for w in ["chat:x", INHABER] {
            let neu = NeueArbeit {
                projekt: "dashboard".into(),
                von: DASH.into(),
                titel: "alt".into(),
                stand: ArbeitsStand::Wartet,
                wartet_auf: Some(w.into()),
                verweis: None,
                prioritaet: Stufe::Bald,
                frist: None,
                notiz: None,
                etappe: None,
            };
            let a = s.arbeit_anlegen(neu, &t).unwrap();
            s.schritt_anlegen(&a.id, DASH, "neu", None, &t).unwrap();
            let n = SchrittAenderung { notiz: Some(Some("n".into())), ..Default::default() };
            let r = s.schritt_aendern(&a.id, 1, DASH, &n, &t).unwrap();
            assert_eq!((schritt(&r.arbeit, 1).wartet_auf.as_deref(), schritt(&r.arbeit, 1).notiz.as_deref()), (Some(w), Some("n")), "{w}");
            assert!(matches!(s.schritt_aendern(&a.id, 1, DASH, &warte_auf(w), &t), Err(SpeicherFehler::Ungueltig(_))), "{w}");
        }
    }

    /// Auflage teilschritte_1_runde_2: Wer zuletzt angeboten hat, lenkt um
    /// oder gibt frei, solange der Empfänger nicht übernommen hat, auch ohne
    /// eigenen Schritt in der Arbeit. Ein anderer ohne Schritt nicht. Wer auf
    /// den Schritt wartete, läuft wieder, wenn das Angebot zurückgenommen ist.
    #[test]
    fn anbieter_ohne_eigenen_schritt_lenkt_um_und_gibt_frei() {
        let s = Speicher::temporaer();
        let a = arbeit(&s, ArbeitsStand::AlsNaechstes);
        let t = zeit(5, 9);
        for titel in ["x", "y"] {
            s.schritt_anlegen(&a.id, DASH, titel, None, &t).unwrap();
        }
        s.schritt_uebernehmen(&a.id, 1, DASH, None, &t).unwrap();
        s.schritt_uebernehmen(&a.id, 2, ARCH, None, &t).unwrap();
        s.schritt_aendern(&a.id, 2, ARCH, &warte_auf("schritt:1"), &t).unwrap();
        // Tippfehler im Empfänger; danach hat Dashboard keinen Schritt mehr.
        s.schritt_aendern(&a.id, 1, DASH, &anbieten("chat:falsch"), &t).unwrap();
        assert!(!s.arbeit(&a.id).unwrap().unwrap().schritte.iter().any(|x| x.besitzer.as_deref() == Some(DASH)));
        // Ein Chat ohne Schritt, der nicht angeboten hat: nicht (das Angebot hält ein anderer, 409).
        assert!(matches!(s.schritt_aendern(&a.id, 1, "chat:x", &anbieten("chat:y"), &t), Err(SpeicherFehler::Besetzt { .. })));
        // Nur das Angebot, nicht mehr.
        let mehr = SchrittAenderung { anbieten: Some("chat:bauplan".into()), notiz: Some(Some("n".into())), ..Default::default() };
        assert!(s.schritt_aendern(&a.id, 1, DASH, &mehr, &t).is_err());
        let r = s.schritt_aendern(&a.id, 1, DASH, &anbieten("chat:bauplan"), &t).unwrap();
        assert_eq!(r.wecken, [Wecken { nr: 1, chat: "chat:bauplan".into(), grund: WeckGrund::Angeboten }]);
        assert_eq!(schritt(&r.arbeit, 1).angeboten_an.as_deref(), Some("chat:bauplan"));
        let frei = SchrittAenderung { freigeben: true, ..Default::default() };
        let r = s.schritt_aendern(&a.id, 1, DASH, &frei, &t).unwrap();
        let (x, y) = (schritt(&r.arbeit, 1), schritt(&r.arbeit, 2));
        assert_eq!((x.angeboten_an.as_deref(), x.besitzer.as_deref(), y.stand), (None, None, ArbeitsStand::Laeuft));
        assert_eq!(r.wecken, [Wecken { nr: 2, chat: ARCH.into(), grund: WeckGrund::Zurueckgeweckt }]);
        // Nicht mehr angeboten: Dashboard hat nichts mehr daran.
        assert!(s.schritt_aendern(&a.id, 1, DASH, &anbieten(ARCH), &t).is_err());
    }

    /// Weck-Vorgänge (T3) in derselben Transaktion: Anbieten legt einen für den
    /// Empfänger an (Art schritt, Kennung am Schritt); umlenken erledigt den
    /// alten; übernehmen erledigt ihn. Zurückwecken: einer je Chat und Anfrage,
    /// auch für zwei wartende Schritte; eine Notiz des Geweckten erledigt ihn.
    #[test]
    fn weck_vorgaenge_angelegt_und_erledigt() {
        let s = Speicher::temporaer();
        let a = arbeit(&s, ArbeitsStand::AlsNaechstes);
        let t = zeit(5, 9);
        for titel in ["bauen", "warten a", "warten b"] {
            s.schritt_anlegen(&a.id, DASH, titel, None, &t).unwrap();
        }
        let zustand = |id: &str| s.vorgang(id).unwrap().unwrap().0.zustand;
        s.schritt_uebernehmen(&a.id, 1, DASH, None, &t).unwrap();
        let r = s.schritt_aendern(&a.id, 1, DASH, &anbieten(ARCH), &t).unwrap();
        let alt = schritt(&r.arbeit, 1).vorgang.clone().unwrap();
        let (v, _) = s.vorgang(&alt).unwrap().unwrap();
        assert_eq!((v.art, v.slug.as_str(), v.zustand, v.von.as_str()), (VorgangsArt::Schritt, "architektur", Zustand::Offen, DASH));
        assert_eq!(v.bezug, Some(format!("schritt:{}/1", a.id)));
        assert!(v.titel.starts_with("Dir angeboten: Schritt 1 „bauen“"), "{}", v.titel);
        assert_eq!(r.ereignisse.len(), 1);
        // Umlenken: der alte ist erledigt, der neue gehört bauplan.
        let r = s.schritt_aendern(&a.id, 1, DASH, &anbieten("chat:bauplan"), &t).unwrap();
        let neu = schritt(&r.arbeit, 1).vorgang.clone().unwrap();
        assert_eq!((zustand(&alt), zustand(&neu), r.ereignisse.len()), (Zustand::Erledigt, Zustand::Offen, 2));
        assert_eq!(s.vorgang(&neu).unwrap().unwrap().0.slug, "bauplan");
        // Zweimal dasselbe Angebot: nichts Neues.
        assert!(!s.schritt_aendern(&a.id, 1, DASH, &anbieten("chat:bauplan"), &t).unwrap().geaendert);
        // Der Empfänger übernimmt: erledigt, keine Kennung mehr am Schritt.
        let r = s.schritt_uebernehmen(&a.id, 1, "chat:bauplan", None, &t).unwrap();
        assert_eq!((zustand(&neu), schritt(&r.arbeit, 1).vorgang.clone(), r.ereignisse.len()), (Zustand::Erledigt, None, 1));
        // Zwei Schritte von Architektur warten auf 1; 1 wird fertig: ein Vorgang für beide.
        for nr in [2, 3] {
            s.schritt_uebernehmen(&a.id, nr, ARCH, None, &t).unwrap();
            s.schritt_aendern(&a.id, nr, ARCH, &warte_auf("schritt:1"), &t).unwrap();
        }
        let vorher = s.vorgaenge_fuer("architektur", true).unwrap().len();
        let r = s.schritt_aendern(&a.id, 1, "chat:bauplan", &stand(ArbeitsStand::Fertig), &t).unwrap();
        let geweckt = schritt(&r.arbeit, 2).vorgang.clone().unwrap();
        assert_eq!(schritt(&r.arbeit, 3).vorgang.as_ref(), Some(&geweckt));
        assert_eq!(s.vorgaenge_fuer("architektur", true).unwrap().len(), vorher + 1, "einer je Chat und Anfrage");
        let v = s.vorgang(&geweckt).unwrap().unwrap().0;
        assert!(v.titel.starts_with("Läuft wieder: dein Schritt 2") && v.titel.ends_with("Dazu Schritt 3."), "{}", v.titel);
        // Architektur schreibt an Schritt 2: erledigt, auch an Schritt 3 keine Kennung mehr.
        let n = SchrittAenderung { notiz: Some(Some("wach".into())), ..Default::default() };
        let r = s.schritt_aendern(&a.id, 2, ARCH, &n, &t).unwrap();
        assert_eq!((zustand(&geweckt), schritt(&r.arbeit, 3).vorgang.clone()), (Zustand::Erledigt, None));
        // Wer sich selbst zurückweckt, bekommt keinen Vorgang.
        s.schritt_anlegen(&a.id, DASH, "eigen", None, &t).unwrap();
        s.schritt_uebernehmen(&a.id, 4, ARCH, None, &t).unwrap();
        s.schritt_aendern(&a.id, 3, ARCH, &warte_auf("schritt:4"), &t).unwrap();
        let r = s.schritt_aendern(&a.id, 4, ARCH, &stand(ArbeitsStand::Fertig), &t).unwrap();
        assert_eq!((r.wecken.len(), r.ereignisse.len(), schritt(&r.arbeit, 3).stand), (1, 0, ArbeitsStand::Laeuft));
    }
}
