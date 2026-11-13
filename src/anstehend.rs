//! „Anstehendes“ rechnen (Etappe A1, Fassung 3): Aus
//! den TOML-Dateien (Projektköpfe, `daten/fragen.toml`,
//! `data/schwerpunkte.toml`) und dem Datenspeicher (offene Aufgaben,
//! Arbeiten, die Einstufungen des Inhabers) entsteht eine Liste mit wirksamer Stufe,
//! Schwerpunkt und Ziel und je Projekt der Stand für das Unter-Dashboard.
//!
//! Grenze (Etappe 3a+, Zielbild „Zustand im Server, Auswertung im
//! Frontend“): Der Server liefert alle offenen Einträge mit Fakten und
//! Regel-Ergebnissen (wirksame Stufe, Frist-Stufe, Tage bis zur Frist,
//! wartet_auf_dich, Schwerpunkt mit reihenfolge) in einer festen, neutralen
//! Ordnung (Projekt, Art, Kennung). Ordnen, „eine Frage auf einmal“,
//! Gruppieren und Zählen macht die Oberfläche an einer Stelle
//! (`web/src/lib/anstehend.ts`). Ausnahme: `/stand` ohne Anmeldung liefert
//! keine Einträge, darum dort die Zahlen als Fakt ([`stand_zahlen`]).
//!
//! Etappe B2 (Fragen-Warteschlange): Welche Frage eines Projekts sichtbar
//! ist, rechnet der Server an einer Stelle ([`fragen_warteschlange`]) und
//! schreibt jeder offenen Frage ihren Platz an (`warteschlange`). Die
//! Oberfläche (`auswaehlen`), die Frist-Meldungen, die Zahlen ohne
//! Anmeldung, das Backlog und der Baustein `Fragen` nehmen alle diesen Platz.
//!
//! Eine Quelle: `/api/anstehend` und `/api/projekte/<slug>/stand` rufen beide
//! [`anstehend`] bzw. [`stand`] mit einem [`Inhalt`]. Seit Etappe 2d kommt
//! der Teil aus den Dateien aus dem ContentStore ([`AusDenDateien::aus_bestand`],
//! dieselbe Rechnung wie vorher, nur auf dem gespeicherten Stand); der Teil
//! aus dem Datenspeicher wird weiter je Anfrage gelesen. Die Rechnung selbst
//! liest nichts und bekommt „heute“ (das lokale Datum) mit dem Inhalt, damit
//! Tests den Tag stellen können.
//!
//! Stufenregel ([prioritaet] regel): die Einstufung des Inhabers gilt vor dem Vorschlag
//! des Chats (ohne Angabe bald). Die Frist hebt an, auch über die
//! Einstufung des Inhabers: heute oder vorbei heißt jetzt (Grund `frist`), in höchstens
//! sieben Tagen mindestens bald. Eine Frist senkt nie. Wie nah
//! eine Frist ist, rechnet [`frist_stufe`] (Etappe 3a-4); jeder Eintrag
//! trägt sie als `frist_stufe`, und aus ihr hebt [`wirksame_stufe`] die
//! Stufe. Rand und Hervorhebung in der Oberfläche und die Stufe kommen so
//! aus einer Quelle.
//!
//! Klick-Links von Aufgaben kommen hier nie vor (der Datenspeicher liefert
//! Aufgaben ohne Link, siehe `Speicher::anstehend_lesen`); jeder Eintrag hat
//! nur sein `ziel` im Dashboard.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

use chrono::NaiveDate;

use crate::fristen::FristenDatei;
use crate::inhalt::store::{Abgleich, Bestand, ContentStore};
use crate::speicher::antworten::antwort_schluessel;
use crate::speicher::arbeiten::{AusDemSpeicher, einstufung_schluessel};
use crate::speicher::meldungen;
use crate::vertrag::api::{
    Aktualisierung, AmpelGrund, AnstehendEintrag, AnstehendListe, ArbeitsStand, ArchivFrist, EintragsArt, FrageAngabe, FragenAuswahl, FragenPlatz, FristStufe,
    GesamtStand, Meldung, MeldungAngabe, MeldungsZahlen, ProjektStand, StandAmpel, StandZahlen, Stufe, StufenAngabe, StufenGrund,
};
use crate::vertrag::bedeutung::Bedeutung;
use crate::vertrag::daten::{DatenInhalt, Frage};
use crate::vertrag::fristen::FristZeile;
use crate::vertrag::gruppen::{self, GruppeAnzeige};
use crate::vertrag::schwerpunkte::{self, SONSTIGES, Schwerpunkte};

/// Eine Frist in höchstens so vielen Tagen hebt mindestens auf bald.
pub const FRIST_BALD_TAGE: i64 = 7;
/// Vermerk an einer Arbeit, die auf eine offene Frage oder Aufgabe wartet.
pub const VERMERK_WARTET: &str = "wartet auf deine Antwort";
/// `wartet_auf` für alles, was auf den Inhaber wartet.
pub const INHABER: &str = "inhaber";

// ----- Die Stufenregel -----

/// Tage von `heute` bis `frist` (0: heute, negativ: vorbei).
pub fn tage_bis(frist: NaiveDate, heute: NaiveDate) -> i64 {
    (frist - heute).num_days()
}

/// Wie nah eine Frist ist (Etappe 3a-4): vorbei, heute, in höchstens
/// [`FRIST_BALD_TAGE`] Tagen, später. Die eine Quelle für die Hebung der
/// Stufe ([`wirksame_stufe`]), die rote Ampel des Unter-Dashboards und den
/// Rand in der Oberfläche.
pub fn frist_stufe(frist: NaiveDate, heute: NaiveDate) -> FristStufe {
    match tage_bis(frist, heute) {
        t if t < 0 => FristStufe::Ueberfaellig,
        0 => FristStufe::Heute,
        t if t <= FRIST_BALD_TAGE => FristStufe::Bald,
        _ => FristStufe::Spaeter,
    }
}

/// Die wirksame Stufe nach [prioritaet] regel, mit Grund.
pub fn wirksame_stufe(vorschlag: Stufe, inhaber: Option<Stufe>, frist: Option<NaiveDate>, heute: NaiveDate) -> StufenAngabe {
    let (mut wirksam, mut grund) = match inhaber {
        Some(s) => (s, StufenGrund::Inhaber),
        None => (vorschlag, StufenGrund::Vorschlag),
    };
    match frist.map(|f| frist_stufe(f, heute)) {
        // Heute oder vorbei: immer jetzt, mit dem Vermerk „Frist“.
        Some(FristStufe::Ueberfaellig | FristStufe::Heute) => {
            wirksam = Stufe::Jetzt;
            grund = StufenGrund::Frist;
        }
        Some(FristStufe::Bald) if wirksam == Stufe::Irgendwann => {
            wirksam = Stufe::Bald;
            grund = StufenGrund::Frist;
        }
        _ => {}
    }
    StufenAngabe { vorschlag, inhaber, wirksam, grund }
}

/// JJJJ-MM-TT als Datum; anderes zählt als keine Frist.
fn datum(text: Option<&str>) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(text?, "%Y-%m-%d").ok()
}

/// Die feste, fachlich neutrale Ordnung der Antworten (Etappe 3a+): Projekt,
/// Art, Kennung. Gleiche Daten geben dieselbe Antwort; wie der Inhaber die
/// Einträge sieht (nach Stufe, Frist, Alter), ordnet die Oberfläche.
pub fn neutral_ordnen(liste: &mut [AnstehendEintrag]) {
    liste.sort_by(|a, b| (&a.projekt, a.art, &a.id).cmp(&(&b.projekt, b.art, &b.id)));
}

/// Die Warteschlange der Fragen eines Projekts (Etappe B2, Entscheidung
/// gemeinsame-bausteine: „Je Projekt ist eine Frage sichtbar;
/// weitere warten in der Warteschlange“). Eingabe: je offene Frage ihre
/// wirksame Stufe und ihre Frist (JJJJ-MM-TT), in der Reihenfolge der Datei
/// (`fragen.toml`, Teile in ihrer Folge). Ausgabe: ihre Indizes in der Folge der Warteschlange; der
/// erste ist die sichtbare Frage.
///
/// Regel: zuerst die wirksame Stufe (Jetzt vor Bald vor Irgendwann; darin
/// stecken der Vorschlag `prioritaet`, die Einstufung des Inhabers und die
/// Frist), dann die Frist (die frühere zuerst, ohne Frist danach; Auflage der
/// Architektur: eine überfällige Frage darf nicht unsichtbar hinter einer
/// anderen gleicher Stufe warten), dann die Reihenfolge in der Datei. Warum so: Die Stufe ist das
/// einzige Feld, mit dem Chat und Inhaber Dringlichkeit ausdrücken (stuft
/// der Inhaber eine wartende Frage hoch, rückt sie vor). Bei gleicher Stufe
/// entscheidet der Chat mit der Reihenfolge, in der er die Fragen ablegt:
/// Das ist eine Warteschlange, die er ohne Datum und ohne Kennungstrick
/// ordnen kann. Das Datum `gestellt` taugt dafür nicht (oft mehrere Fragen
/// am selben Tag, und neu gestellt verschöbe es die Frage nach hinten), die
/// Kennung auch nicht (ein Name, keine Reihenfolge). Eine beantwortete Frage
/// ist nicht mehr offen; die nächste rückt sofort nach.
pub fn fragen_warteschlange(fragen: &[(Stufe, Option<&str>)]) -> Vec<usize> {
    let mut folge: Vec<usize> = (0..fragen.len()).collect();
    // Ohne Frist nach allen mit Frist: (true, …) steht hinter (false, …).
    folge.sort_by_key(|&i| (fragen[i].0, fragen[i].1.is_none(), fragen[i].1, i));
    folge
}

/// „Eine Frage auf einmal“ als Regel für den Server selbst: je Projekt nur
/// die sichtbare Frage (Platz 1 in [`fragen_warteschlange`]); alle anderen
/// Einträge bleiben. Gebraucht dort, wo der Server selbst etwas daraus macht:
/// Frist-Meldungen (eine Meldung zeigt auf die Frage, die der Inhaber sieht),
/// die Zahlen des freien Wegs und das Backlog. Die Oberfläche
/// (`web/src/lib/anstehend.ts`, `auswaehlen`) nimmt denselben Platz.
pub fn eine_frage_je_projekt(liste: &[AnstehendEintrag]) -> Vec<&AnstehendEintrag> {
    liste.iter().filter(|e| e.art != EintragsArt::Frage || ist_sichtbar(e)).collect()
}

/// Ist der Eintrag die sichtbare Frage seines Projekts (Platz 1)?
pub fn ist_sichtbar(e: &AnstehendEintrag) -> bool {
    e.art == EintragsArt::Frage && e.warteschlange.is_some_and(|w| w.platz == 1)
}

/// Die Warteschlange eines Projekts in Kürze (für das Abgeleitete von
/// `fragen.toml` und das Backlog): die sichtbare Frage und wie viele warten.
pub fn fragen_auswahl(liste: &[AnstehendEintrag], projekt: &str) -> FragenAuswahl {
    let fragen: Vec<&AnstehendEintrag> = liste.iter().filter(|e| e.art == EintragsArt::Frage && e.projekt == projekt).collect();
    let sichtbar = fragen.iter().find(|e| ist_sichtbar(e));
    FragenAuswahl {
        sichtbar: sichtbar.map(|e| e.id.clone()),
        gestellt: sichtbar.and_then(|e| e.gestellt.clone()),
        weitere: (fragen.len() as u64).saturating_sub(1),
    }
}

/// Zustand einer offenen Aufgabe, auf die der Inhaber mit „Etwas anderes …“
/// geantwortet hat: Sie liegt beim Chat (`wartet_auf = chat:<projekt>`).
pub const ZUSTAND_BEIM_CHAT: &str = "beim-chat";

/// Wartet der Eintrag auf den Inhaber? Fragen immer, Aufgaben außer beim Chat; Arbeiten mit
/// `wartet_auf = inhaber`, außer sie verweisen auf eine offene Frage oder
/// Aufgabe (die zählt dann, nicht doppelt). Meldungen warten nicht auf sie,
/// sie sagen nur, was passiert ist.
pub fn wartet_auf_dich(e: &AnstehendEintrag) -> bool {
    match e.art {
        EintragsArt::Frage => true,
        // Eine Aufgabe beim Chat (eigene Worte geschickt) wartet nicht auf den Inhaber.
        EintragsArt::Aufgabe => e.zustand != ZUSTAND_BEIM_CHAT,
        EintragsArt::Arbeit => e.wartet_auf.as_deref() == Some(INHABER) && e.vermerk.is_none(),
        EintragsArt::Meldung => false,
    }
}

/// Ziel eines Eintrags im Dashboard.
pub fn ziel(projekt: &str, art: EintragsArt, id: &str) -> String {
    format!("/projects/{projekt}#{}-{id}", art.wort())
}

/// Ziel einer Meldung: die Projektseite, Systemmeldungen die PC-Seite ([ziele]).
pub fn meldung_ziel(m: &Meldung) -> String {
    if m.system { format!("/pc#meldung-{}", m.id) } else { ziel(&m.projekt, EintragsArt::Meldung, &m.id) }
}

/// Zeit zum Vergleichen (über Zeitumstellungen hinweg); unlesbar ganz früh.
fn zeit_wert(t: &str) -> i64 {
    chrono::DateTime::parse_from_rfc3339(t).map(|z| z.timestamp()).unwrap_or(i64::MIN)
}

// ----- Der Inhalt -----

/// Der Kopf eines Projekts, so weit „Anstehend“ und das Unter-Dashboard ihn brauchen.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjektKopf {
    pub slug: String,
    pub titel: String,
    pub status: String,
    /// `open = [...]`.
    pub notizen: Vec<String>,
    /// Letzte Änderung der Seite.
    pub zuletzt: String,
    /// Archiviert (Entscheidung archiv-status; aus dem ContentStore,
    /// auch wenn der Kopf gerade kaputt ist).
    pub archiviert: bool,
}

/// Die Fragen aus `daten/fragen.toml` eines Projekts (alle Teile).
#[derive(Debug, Clone, PartialEq)]
pub struct FragenDatei {
    pub projekt: String,
    /// `projects/<slug>/daten/fragen.toml`.
    pub datei: String,
    pub fragen: Vec<Frage>,
}

/// Alles, woraus gerechnet wird. In A1 je Anfrage gelesen; die Rechnung
/// ([`anstehend`], [`stand`]) liest selbst nichts.
#[derive(Debug, Clone, Default)]
pub struct Inhalt {
    /// Das lokale Datum, mit dem gerechnet wird.
    pub heute: NaiveDate,
    pub projekte: Vec<ProjektKopf>,
    pub fragen: Vec<FragenDatei>,
    /// `daten/fristen.toml` je Projekt (Etappe B3): Ereignisse und Fristen.
    pub fristen: Vec<FristenDatei>,
    pub speicher: AusDemSpeicher,
    /// Der Datenspeicher fehlt: `speicher` ist leer (keine Aufgaben, Arbeiten,
    /// Einstufungen); Notizen und Fragen aus den Dateien stehen trotzdem da.
    pub speicher_fehlt: bool,
    pub schwerpunkte: Schwerpunkte,
    /// Projektgruppen mit ihren sichtbaren Teilen (`Bestand::gruppen_sichtbar`).
    pub gruppen: Vec<GruppeAnzeige>,
    /// Sätze zu Dateien, die sich nicht lesen ließen.
    pub hinweise: Vec<String>,
    /// Was sich diesmal nicht lesen ließ (für den Fristen-Lauf: ein Lesefehler
    /// ist kein Beheben).
    pub lesefehler: Lesefehler,
}

/// Lesefehler beim Sammeln: Projekte, deren `daten/fragen.toml` sich nicht
/// lesen ließ, und ob schon der Projektordner selbst nicht lesbar war. Die
/// Fragen dieser Projekte fehlen dann in der Liste, sind aber nicht erledigt.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Lesefehler {
    pub fragen: BTreeSet<String>,
    /// Etappe B3: Projekte, deren `daten/fristen.toml` sich nicht lesen ließ
    /// (auch ohne letzten guten Stand); bei archivierten steht dafür eine
    /// neutrale Zeile in Anstehend ([`ARCHIV_FRISTEN_UNLESBAR`]).
    pub fristen: BTreeSet<String>,
    pub ordner: bool,
}

/// Was aus den Dateien kommt (ohne Datenspeicher).
#[derive(Debug, Clone, Default)]
pub struct AusDenDateien {
    pub projekte: Vec<ProjektKopf>,
    pub fragen: Vec<FragenDatei>,
    pub fristen: Vec<FristenDatei>,
    pub schwerpunkte: Schwerpunkte,
    pub gruppen: Vec<GruppeAnzeige>,
    pub hinweise: Vec<String>,
    pub lesefehler: Lesefehler,
}

impl AusDenDateien {
    /// Liest Projektköpfe (auch kaputte), ihre
    /// `daten/fragen.toml` und `data/schwerpunkte.toml` von der Platte
    /// (über einen eigenen ContentStore). Blockiert (std::fs). Der Dienst
    /// nimmt [`Self::aus_bestand`].
    pub fn lesen(projekte_ordner: &Path, schwerpunkte_datei: &Path, heute: NaiveDate) -> Self {
        let pc = schwerpunkte_datei.parent().map(|p| p.join("pc.toml")).unwrap_or_default();
        let store = ContentStore::laden(projekte_ordner.to_path_buf(), pc, schwerpunkte_datei.to_path_buf(), Abgleich::BeiJedemZugriff);
        Self::aus_bestand(&store.bestand(), heute)
    }

    /// Dasselbe aus dem gespeicherten Stand (Etappe 2d). Lesefehler, deren
    /// letzter guter Stand gilt, zählen hier nicht als Lesefehler: Die Fragen
    /// sind ja da (der Fristen-Lauf behebt also nichts).
    pub fn aus_bestand(b: &Bestand, _heute: NaiveDate) -> Self {
        let mut aus = Self::default();
        // Ein leerer Bestand hieße sonst „alle weg“.
        aus.lesefehler.ordner = !b.ordner_lesbar;
        let (projekte, kaputte) = b.liste();
        for p in &projekte {
            aus.projekte.push(ProjektKopf {
                slug: p.slug.clone(),
                titel: p.meta.title.clone(),
                status: p.meta.status.clone(),
                notizen: p.meta.open.clone(),
                zuletzt: p.last_change(),
                archiviert: b.archiviert(&p.slug),
            });
        }
        for k in &kaputte {
            // Mit lesbarem Kopf (kaputt ist etwa eine Datei in daten/): Titel und Status von dort.
            let kopf = b.kopf(&k.slug).and_then(Result::ok);
            aus.projekte.push(match kopf {
                Some((p, zeit)) => ProjektKopf {
                    slug: k.slug.clone(),
                    titel: p.title.clone(),
                    status: p.status.wort().to_string(),
                    notizen: p.open.clone(),
                    zuletzt: zeit.map_or_else(|| "unbekannt".to_string(), |t| t.format("%Y-%m-%d %H:%M").to_string()),
                    archiviert: b.archiviert(&k.slug),
                },
                None => ProjektKopf {
                    slug: k.slug.clone(),
                    titel: k.slug.clone(),
                    status: "problem".to_string(),
                    notizen: Vec::new(),
                    zuletzt: "unbekannt".to_string(),
                    archiviert: b.archiviert(&k.slug),
                },
            });
        }
        aus.projekte.sort_by(|a, b| a.slug.cmp(&b.slug));
        for p in &aus.projekte {
            match b.daten(&p.slug, "fragen") {
                Ok(Some((DatenInhalt::Fragen(d), _))) => {
                    let fragen: Vec<Frage> = d.inhalte().into_iter().flat_map(|t| t.frage.clone()).collect();
                    if !fragen.is_empty() {
                        aus.fragen.push(FragenDatei { projekt: p.slug.clone(), datei: format!("projects/{}/daten/fragen.toml", p.slug), fragen });
                    }
                }
                Ok(_) => {}
                Err(e) => {
                    aus.lesefehler.fragen.insert(p.slug.clone());
                    aus.hinweise.push(format!("{e} Die Fragen dieses Projekts fehlen in der Liste."));
                }
            }
            // Etappe B3: Ereignisse und Fristen. Lässt sich die Datei nicht lesen (und
            // gibt es keinen letzten guten Stand), fehlen ihre Fristen mit Hinweis.
            match b.daten(&p.slug, "fristen") {
                Ok(Some((DatenInhalt::Fristen(d), _))) => {
                    aus.fristen.push(FristenDatei { projekt: p.slug.clone(), datei: format!("projects/{}/daten/fristen.toml", p.slug), inhalt: d });
                }
                Ok(_) => {}
                Err(e) => {
                    aus.lesefehler.fristen.insert(p.slug.clone());
                    aus.hinweise.push(format!("{e} Die Fristen aus dieser Datei fehlen in der Liste."));
                }
            }
        }
        match &b.schwerpunkte {
            Ok(None) => aus.hinweise.push(format!("{} fehlt; alle Projekte stehen unter „{SONSTIGES}“.", schwerpunkte::DATEI)),
            Ok(Some(s)) => aus.schwerpunkte = s.clone(),
            Err(e) => aus.hinweise.push(format!("{e} Bis das behoben ist, stehen alle Projekte unter „{SONSTIGES}“.")),
        }
        // Projektgruppen: eine fehlende Datei heißt keine Gruppen (kein Hinweis).
        if let Err(e) = &b.gruppen {
            aus.hinweise.push(format!("{e} Bis das behoben ist, stehen alle Projekte ohne Gruppe da."));
        }
        aus.hinweise.extend(b.gruppen_abgelehnt());
        aus.gruppen = b.gruppen_sichtbar();
        aus
    }
}

impl Inhalt {
    pub fn aus(dateien: AusDenDateien, speicher: AusDemSpeicher, heute: NaiveDate) -> Self {
        Self {
            heute,
            projekte: dateien.projekte,
            fragen: dateien.fragen,
            fristen: dateien.fristen,
            speicher,
            speicher_fehlt: false,
            schwerpunkte: dateien.schwerpunkte,
            gruppen: dateien.gruppen,
            hinweise: dateien.hinweise,
            lesefehler: dateien.lesefehler,
        }
    }
}

// ----- Die Rechnung -----

/// Wonach `GET /api/anstehend` filtert.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Filter {
    pub stufe: Option<Stufe>,
    pub schwerpunkt: Option<String>,
    pub art: Option<EintragsArt>,
    pub projekt: Option<String>,
    pub nur_fuer_mich: bool,
}

/// Ist die Frage beantwortet (Etappe A2)? Dann ist sie nicht mehr offen, auch
/// wenn sie noch in fragen.toml steht, bis der Chat sie herausnimmt.
pub fn beantwortet(inhalt: &Inhalt, projekt: &str, f: &Frage) -> bool {
    inhalt.speicher.beantwortet.contains(&antwort_schluessel(projekt, &f.id, &f.gestellt))
}

/// Die offenen (nicht beantworteten) Fragen einer Datei.
pub fn offene_fragen<'a>(inhalt: &'a Inhalt, d: &'a FragenDatei) -> impl Iterator<Item = &'a Frage> + 'a {
    d.fragen.iter().filter(move |f| !beantwortet(inhalt, &d.projekt, f))
}

/// Alle offenen Einträge (Aufgaben, alle offenen Fragen, nicht fertige
/// Arbeiten, offene Meldungen) mit ihren Fakten, neutral geordnet ([`neutral_ordnen`]).
pub fn alle_eintraege(inhalt: &Inhalt) -> Vec<AnstehendEintrag> {
    let heute = inhalt.heute;
    let inhaber: HashMap<String, Option<Stufe>> =
        inhalt.speicher.einstufungen.iter().map(|e| (einstufung_schluessel(&e.projekt, e.art, &e.id, e.gestellt.as_deref()), e.stufe)).collect();
    let einstufung = |projekt: &str, art: EintragsArt, id: &str, gestellt: Option<&str>| -> Option<Stufe> {
        inhaber.get(&einstufung_schluessel(projekt, art, id, gestellt)).copied().flatten()
    };
    let karte = inhalt.schwerpunkte.zuordnung_mit_reihenfolge();
    let sonstiges = (SONSTIGES.to_string(), inhalt.schwerpunkte.sonstiges_reihenfolge());
    let titel: BTreeMap<&str, &str> = inhalt.projekte.iter().map(|p| (p.slug.as_str(), p.titel.as_str())).collect();
    let basis = |art: EintragsArt, projekt: &str, id: &str, text: &str, stufe: StufenAngabe, frist: Option<&str>| AnstehendEintrag {
        art,
        id: id.to_string(),
        projekt: projekt.to_string(),
        projekt_titel: titel.get(projekt).map(|t| t.to_string()),
        gruppe: gruppen::bezug(&inhalt.gruppen, projekt),
        schwerpunkt: karte.get(projekt).unwrap_or(&sonstiges).0.clone(),
        schwerpunkt_reihenfolge: karte.get(projekt).unwrap_or(&sonstiges).1,
        wartet_auf_dich: false,
        titel: text.to_string(),
        stufe,
        frist: datum(frist).map(|_| frist.unwrap_or_default().to_string()),
        tage_bis: datum(frist).map(|f| tage_bis(f, heute)),
        frist_stufe: datum(frist).map(|f| frist_stufe(f, heute)),
        zustand: "offen".to_string(),
        wartet_auf: Some(INHABER.to_string()),
        verweis: None,
        vermerk: None,
        ziel: ziel(projekt, art, id),
        ziel_da: titel.contains_key(projekt),
        quelle: String::new(),
        hat_link: false,
        knopf: None,
        gestellt: None,
        frage: None,
        meldung: None,
        von: None,
        erstellt: String::new(),
        eigene: None,
        warteschlange: None,
        frist_bezug: None,
        ereignis: None,
        schritte: Vec::new(),
        offene_verweise: Vec::new(),
    };

    let mut liste = Vec::new();
    for a in &inhalt.speicher.aufgaben {
        // Etappe B3: Eine bewegliche Frist hat ihr Datum, sobald ihr Ereignis
        // eines hat; bis dahin keine Frist (sie hebt nichts an), aber `frist_bezug`.
        let bezug = a.frist_bezug.as_ref().map(|b| (b, crate::fristen::bezug_aufloesen(&inhalt.fristen, &a.projekt, b)));
        let frist = match &bezug {
            Some((_, (_, d))) => d.map(|d| d.format("%Y-%m-%d").to_string()),
            None => a.frist.clone(),
        };
        let stufe = wirksame_stufe(a.prioritaet.unwrap_or_default(), einstufung(&a.projekt, EintragsArt::Aufgabe, &a.id, None), datum(frist.as_deref()), heute);
        let mut e = basis(EintragsArt::Aufgabe, &a.projekt, &a.id, &a.titel, stufe, frist.as_deref());
        if let Some((b, (stand, _))) = bezug {
            e.frist_bezug = Some(b.clone());
            e.ereignis = Some(stand);
        }
        e.quelle = "datenspeicher:aufgaben".to_string();
        e.hat_link = a.hat_link;
        e.knopf = a.knopf.clone();
        e.von = Some(a.von.clone());
        e.erstellt = a.erstellt.clone();
        // Mit eigenen Worten beantwortet: beim Chat, bis er sie abschließt oder neu stellt.
        if crate::speicher::aufgaben::beim_chat(a) {
            e.zustand = ZUSTAND_BEIM_CHAT.to_string();
            e.wartet_auf = Some(format!("chat:{}", a.projekt));
            e.eigene = a.eigene.clone();
        }
        liste.push(e);
    }
    // Alle offenen Fragen (Etappe 3a+), je mit ihrem Platz in der
    // Warteschlange des Projekts (Etappe B2, [`fragen_warteschlange`]).
    // Beantwortete Fragen (Etappe A2) fehlen, auch solange sie noch in
    // fragen.toml stehen; die nächste rückt so sofort nach.
    for d in &inhalt.fragen {
        let mut fragen = Vec::new();
        for f in offene_fragen(inhalt, d) {
            let inhaber = einstufung(&d.projekt, EintragsArt::Frage, &f.id, Some(&f.gestellt));
            let stufe = wirksame_stufe(f.prioritaet.unwrap_or_default(), inhaber, datum(f.frist.as_deref()), heute);
            let mut e = basis(EintragsArt::Frage, &d.projekt, &f.id, &f.text, stufe, f.frist.as_deref());
            e.quelle = d.datei.clone();
            e.gestellt = Some(f.gestellt.clone());
            e.frage = Some(FrageAngabe { art: f.art, optionen: f.optionen.clone(), empfehlung: f.empfehlung.clone() });
            e.erstellt = f.gestellt.clone();
            fragen.push(e);
        }
        let folge = {
            let schluessel: Vec<(Stufe, Option<&str>)> = fragen.iter().map(|e| (e.stufe.wirksam, e.frist.as_deref())).collect();
            fragen_warteschlange(&schluessel)
        };
        let offen = fragen.len() as u64;
        for (platz, i) in folge.into_iter().enumerate() {
            fragen[i].warteschlange = Some(FragenPlatz { platz: platz as u64 + 1, offen });
        }
        liste.extend(fragen);
    }
    // Worauf eine Arbeit verweisen kann: offene Fragen und Aufgaben, je Projekt.
    let offen_im_projekt = |projekt: &str, id: &str| {
        inhalt.speicher.aufgaben.iter().any(|a| a.projekt == projekt && a.id == id)
            || inhalt.fragen.iter().any(|d| d.projekt == projekt && offene_fragen(inhalt, d).any(|f| f.id == id))
    };
    for a in inhalt.speicher.arbeiten.iter().filter(|a| a.stand != ArbeitsStand::Fertig) {
        let stufe = wirksame_stufe(a.prioritaet, einstufung(&a.projekt, EintragsArt::Arbeit, &a.id, None), datum(a.frist.as_deref()), heute);
        let mut e = basis(EintragsArt::Arbeit, &a.projekt, &a.id, &a.titel, stufe, a.frist.as_deref());
        e.zustand = a.stand.wort().to_string();
        e.wartet_auf = a.wartet_auf.clone();
        e.verweis = a.verweis.clone();
        // Jeder auf dich wartende Schritt zählt, nicht nur der erste (Auflage aus T4).
        let wartende: Vec<_> = a.schritte.iter().filter(|s| s.stand == ArbeitsStand::Wartet && s.wartet_auf.as_deref() == Some(INHABER)).collect();
        let offen = |s: &crate::vertrag::anstehend::Schritt| s.verweis.as_deref().is_some_and(|v| offen_im_projekt(&a.projekt, v));
        for v in wartende.iter().filter(|s| offen(s)).filter_map(|s| s.verweis.as_deref()) {
            if !e.offene_verweise.iter().any(|o| o == v) {
                e.offene_verweise.push(v.to_string());
            }
        }
        // Vermerk (die Arbeit zählt nicht selbst) nur, wenn jeder wartende Schritt auf etwas Offenes zeigt (Auflage aus T5).
        let verweist =
            if wartende.is_empty() { a.verweis.as_deref().is_some_and(|v| offen_im_projekt(&a.projekt, v)) } else { wartende.iter().all(|s| offen(s)) };
        if a.wartet_auf.as_deref() == Some(INHABER) && verweist {
            e.vermerk = Some(VERMERK_WARTET.to_string());
        }
        e.quelle = "datenspeicher:arbeiten".to_string();
        e.von = Some(a.von.clone());
        e.erstellt = a.erstellt.clone();
        e.schritte = a.schritte.clone();
        liste.push(e);
    }
    // Meldungen (Etappe A3): die offenen (ungelesen, nicht behoben), mit fester Stufe nach Art.
    for m in inhalt.speicher.meldungen.iter().filter(|m| meldungen::offen(m)) {
        let s = m.art.stufe();
        let stufe = StufenAngabe { vorschlag: s, inhaber: None, wirksam: s, grund: StufenGrund::Fest };
        let mut e = basis(EintragsArt::Meldung, &m.projekt, &m.id, &m.titel, stufe, None);
        e.zustand = "ungelesen".to_string();
        e.wartet_auf = None;
        e.verweis = m.verweis.clone();
        e.ziel = meldung_ziel(m);
        e.ziel_da = m.system || e.ziel_da;
        e.quelle = "datenspeicher:meldungen".to_string();
        e.meldung = Some(MeldungAngabe {
            art: m.art,
            anzahl: m.anzahl,
            zuletzt: m.zuletzt.clone(),
            text: m.text.clone(),
            system: m.system,
            schluessel: m.schluessel.clone(),
        });
        e.von = Some(m.von.clone());
        e.erstellt = m.zuletzt.clone();
        liste.push(e);
    }
    for e in &mut liste {
        e.wartet_auf_dich = wartet_auf_dich(e);
    }
    neutral_ordnen(&mut liste);
    liste
}

/// Die Aktualisierungen eines Projekts ([unter_dashboard] aufbau): alle
/// Meldungen des Projekts außerhalb des Archivs (auch gelesene) und die
/// Vorgänge der letzten sieben Tage, neutral nach Quelle und Kennung
/// (Etappe 3a+: welche fünf zuerst stehen, wählt die Oberfläche). Nur mit
/// Windows-Anmeldung: Titel und Texte von Meldungen und Vorgängen nennen
/// Aufgaben, Antworten, Dateipfade und was die Chats schreiben. Ohne
/// Anmeldung gibt es nur [`meldungs_zahlen`].
pub fn aktualisierungen(inhalt: &Inhalt, slug: &str) -> Vec<Aktualisierung> {
    let mut liste: Vec<Aktualisierung> = inhalt
        .speicher
        .meldungen
        .iter()
        .filter(|m| m.projekt == slug)
        .map(|m| Aktualisierung {
            quelle: "meldung".to_string(),
            id: m.id.clone(),
            art: m.art.wort().to_string(),
            titel: m.titel.clone(),
            zeit: m.zuletzt.clone(),
            ziel: meldung_ziel(m),
            gelesen: m.gelesen.is_some(),
            anzahl: m.anzahl,
            behoben: m.behoben.is_some(),
            text: m.text.clone(),
        })
        .collect();
    // Gleiche Vorgänge (Art und Titel) sind eine Zeile mit Anzahl, wie Meldungen:
    // die neueste (Zeit, dann Kennung) bleibt mit Ziel und Zeit.
    let mut vorgaenge: BTreeMap<(String, String), Aktualisierung> = BTreeMap::new();
    for v in inhalt.speicher.vorgaenge.iter().filter(|v| v.slug == slug) {
        let neu = Aktualisierung {
            quelle: "vorgang".to_string(),
            id: v.id.clone(),
            art: v.art.wort().to_string(),
            titel: v.titel.clone(),
            zeit: v.geaendert.clone(),
            ziel: format!("/vorgaenge#vorgang-{}", v.id),
            gelesen: true,
            anzahl: 1,
            behoben: false,
            text: None,
        };
        match vorgaenge.entry((neu.art.clone(), neu.titel.clone())) {
            std::collections::btree_map::Entry::Vacant(e) => {
                e.insert(neu);
            }
            std::collections::btree_map::Entry::Occupied(mut e) => {
                let alt = e.get_mut();
                let anzahl = alt.anzahl + 1;
                // Als Zeitpunkt vergleichen (Sommer- und Winterzeit); Unlesbares gilt als alt, wie in der Oberfläche.
                let wann = |a: &Aktualisierung| (chrono::DateTime::parse_from_rfc3339(&a.zeit).ok(), a.id.clone());
                if wann(&neu) > wann(alt) {
                    *alt = neu;
                }
                alt.anzahl = anzahl;
            }
        }
    }
    liste.extend(vorgaenge.into_values());
    liste.sort_by(|a, b| (&a.quelle, &a.id).cmp(&(&b.quelle, &b.id)));
    liste
}

/// Wie viele Meldungen das Projekt hat (außerhalb des Archivs) und wie viele
/// davon ungelesen sind: nur Zahlen, für den freien Weg.
pub fn meldungs_zahlen(inhalt: &Inhalt, slug: &str) -> MeldungsZahlen {
    let eigene = inhalt.speicher.meldungen.iter().filter(|m| m.projekt == slug);
    let (mut neu, mut gesamt) = (0, 0);
    for m in eigene {
        gesamt += 1;
        if m.gelesen.is_none() {
            neu += 1;
        }
    }
    MeldungsZahlen { neu, gesamt }
}

/// `GET /api/anstehend`: alle Einträge nach `filter`, neutral geordnet,
/// dazu Hinweise und mit `projekt` die Aktualisierungen. Keine Zahlen,
/// keine Auswahl (Etappe 3a+).
///
/// Entscheidung archiv-status: Ohne Filter `projekt` fehlen alle
/// Einträge archivierter Projekte (Fragen, Aufgaben, Arbeiten, Meldungen)
/// und Hinweise zu ihren Dateien; nur ihre nahen Fristen stehen neutral in
/// `archiv_fristen` ([`archiv_fristen`]). Mit `projekt` (die Projektseite
/// selbst) steht alles da wie sonst.
pub fn anstehend(inhalt: &Inhalt, filter: &Filter) -> AnstehendListe {
    let alle = alle_eintraege(inhalt);
    let archiviert = archivierte(inhalt);
    let (archiv_fristen, hinweise) = match filter.projekt {
        Some(_) => (Vec::new(), inhalt.hinweise.clone()),
        None => (
            archiv_fristen(inhalt, &alle, &archiviert),
            inhalt.hinweise.iter().filter(|h| !archiviert.iter().any(|s| h.contains(&format!("projects/{s}/")))).cloned().collect(),
        ),
    };
    let eintraege = alle
        .into_iter()
        .filter(|e| match filter.projekt.as_deref() {
            Some(p) => p == e.projekt,
            None => !archiviert.contains(e.projekt.as_str()),
        })
        .filter(|e| filter.schwerpunkt.as_deref().is_none_or(|s| s == e.schwerpunkt))
        .filter(|e| filter.stufe.is_none_or(|s| s == e.stufe.wirksam))
        .filter(|e| filter.art.is_none_or(|a| a == e.art))
        .filter(|e| !filter.nur_fuer_mich || e.wartet_auf_dich)
        .collect();
    AnstehendListe {
        heute: inhalt.heute.format("%Y-%m-%d").to_string(),
        frist_bald_tage: FRIST_BALD_TAGE,
        eintraege,
        hinweise,
        aktualisierungen: filter.projekt.as_deref().map(|p| aktualisierungen(inhalt, p)).unwrap_or_default(),
        archiv_fristen,
    }
}

/// Die Slugs der archivierten Projekte (Status `archiviert` im Kopf, alte
/// oder neue Form). Ein Eintrag, dessen Projektordner fehlt, gehört nicht dazu.
pub fn archivierte(inhalt: &Inhalt) -> BTreeSet<&str> {
    inhalt.projekte.iter().filter(|p| p.archiviert).map(|p| p.slug.as_str()).collect()
}

/// Der neutrale Satz zu einer Frist in einem archivierten Projekt, ohne Projekttitel.
pub fn archiv_frist_satz(frist: NaiveDate, tage_bis: i64) -> String {
    let datum = frist.format("%d.%m.%Y");
    if tage_bis < 0 {
        format!("Eine Frist in einem archivierten Projekt ist am {datum} abgelaufen.")
    } else {
        format!("Eine Frist in einem archivierten Projekt läuft am {datum} ab.")
    }
}

/// Der neutrale Satz zu einer Frist in einem archivierten Projekt, die an
/// einem Ereignis ohne Datum hängt (Etappe B3), ohne Projekttitel.
pub const ARCHIV_FRIST_HAENGT: &str = "Eine Frist in einem archivierten Projekt hängt an einem Ereignis.";

/// Der neutrale Satz, wenn sich `daten/fristen.toml` eines archivierten
/// Projekts nicht lesen lässt (Auflage der Architektur zu B3): Die Fristen
/// verschwinden dann nicht still. Ohne Projekttitel, wie die Archiv-Regel.
pub const ARCHIV_FRISTEN_UNLESBAR: &str = "Fristen eines archivierten Projekts lassen sich nicht lesen.";

/// Sicherheitsregel (Entscheidung archiv-status, `fristen`): Ein
/// Eintrag mit echter Frist (Aufgabe, Frage oder Arbeit, nicht Meldung) und
/// seit Etappe B3 eine offene Frist aus `daten/fristen.toml` in einem
/// archivierten Projekt erscheint [`FRIST_BALD_TAGE`] Tage vor Ablauf (und
/// danach, solange sie offen ist) als neutraler Satz mit Link zur
/// Projektseite; je Projekt und Datum einmal. Eine Frist in acht Tagen
/// bleibt still. Etappe B3: Eine bewegliche Frist, deren Ereignis noch kein
/// Datum hat, steht als „hängt an einem Ereignis“ da (je Projekt einmal,
/// ohne Datum), bis sie aufgelöst ist; dann gilt die Regel der sieben Tage.
/// Lässt sich `fristen.toml` eines archivierten Projekts nicht lesen, steht
/// [`ARCHIV_FRISTEN_UNLESBAR`] da (je Projekt einmal, ohne Datum), bis die
/// Datei wieder lesbar ist. Geordnet: mit Datum nach Frist, dann die ohne
/// Datum. Die Startseite zeigt nur die mit Datum (die anderen haben kein Ende).
pub fn archiv_fristen(inhalt: &Inhalt, alle: &[AnstehendEintrag], archiviert: &BTreeSet<&str>) -> Vec<ArchivFrist> {
    // (Projekt, Frist oder None bei hängender, Tage bis)
    let mut kandidaten: Vec<(String, Option<NaiveDate>, Option<i64>)> = Vec::new();
    for e in alle.iter().filter(|e| e.art != EintragsArt::Meldung && archiviert.contains(e.projekt.as_str())) {
        match (datum(e.frist.as_deref()), e.tage_bis) {
            (Some(f), Some(t)) => kandidaten.push((e.projekt.clone(), Some(f), Some(t))),
            _ if e.frist_bezug.is_some() => kandidaten.push((e.projekt.clone(), None, None)),
            _ => {}
        }
    }
    for d in inhalt.fristen.iter().filter(|d| archiviert.contains(d.projekt.as_str())) {
        for z in crate::fristen::datei_zeilen(inhalt, d) {
            kandidaten.push((d.projekt.clone(), datum(z.frist.as_deref()), z.tage_bis));
        }
    }
    let mut gesehen = BTreeSet::new();
    let mut aus: Vec<ArchivFrist> = Vec::new();
    for (projekt, frist, tage) in kandidaten {
        let satz = match (frist, tage) {
            (Some(f), Some(t)) if t <= FRIST_BALD_TAGE => archiv_frist_satz(f, t),
            (Some(_), _) => continue,
            (None, _) => ARCHIV_FRIST_HAENGT.to_string(),
        };
        if !gesehen.insert((projekt.clone(), frist)) {
            continue;
        }
        aus.push(ArchivFrist { satz, frist: frist.map(|f| f.format("%Y-%m-%d").to_string()), tage_bis: tage, ziel: format!("/projects/{projekt}") });
    }
    for projekt in inhalt.lesefehler.fristen.iter().filter(|p| archiviert.contains(p.as_str())) {
        aus.push(ArchivFrist { satz: ARCHIV_FRISTEN_UNLESBAR.to_string(), frist: None, tage_bis: None, ziel: format!("/projects/{projekt}") });
    }
    aus.sort_by(|a, b| (a.frist.is_none(), &a.frist, &a.ziel).cmp(&(b.frist.is_none(), &b.frist, &b.ziel)));
    aus
}

/// Der Eintrag mit der frühesten Frist unter `kandidaten`; bei gleicher
/// Frist der, den der Inhaber zuerst sieht (Stufe, Alter, Art, Kennung).
fn frueheste_frist<'a>(kandidaten: impl Iterator<Item = &'a AnstehendEintrag>) -> Option<&'a AnstehendEintrag> {
    kandidaten
        .filter(|e| e.frist.is_some())
        .min_by(|a, b| (&a.frist, a.stufe.wirksam, &a.erstellt, a.art, &a.id).cmp(&(&b.frist, b.stufe.wirksam, &b.erstellt, b.art, &b.id)))
}

/// Die Zahlen des freien Wegs (Ausnahme aus dem Zielbild). Dieselbe
/// Zählung macht die Oberfläche mit Anmeldung aus `eintraege` und
/// `aktualisierungen` (`web/src/lib/anstehend.ts`, `standZahlen`); ein
/// vitest-Fall hält beide gleich.
pub fn stand_zahlen(inhalt: &Inhalt, slug: &str, eintraege: &[AnstehendEintrag], datei_fristen: &[FristZeile]) -> StandZahlen {
    let auswahl = eine_frage_je_projekt(eintraege);
    let arbeiten = |z: ArbeitsStand| eintraege.iter().filter(|e| e.art == EintragsArt::Arbeit && e.zustand == z.wort()).count() as u64;
    let naechste = frueheste_frist(auswahl.iter().copied());
    // Etappe B3: eine offene Frist aus fristen.toml, wenn sie früher ist (bei
    // gleichem Datum bleibt der Eintrag). Nie aus zeitstrahl.toml.
    let datei = datei_fristen.iter().filter(|z| z.frist.is_some()).min_by(|a, b| a.frist.cmp(&b.frist));
    let (naechste_frist, tage, stufe) = match (naechste, datei) {
        (Some(e), Some(z)) if z.frist < e.frist => (z.frist.clone(), z.tage_bis, z.frist_stufe),
        (Some(e), _) => (e.frist.clone(), e.tage_bis, e.frist_stufe),
        (None, Some(z)) => (z.frist.clone(), z.tage_bis, z.frist_stufe),
        (None, None) => (None, None, None),
    };
    StandZahlen {
        wartet_auf_dich: auswahl.iter().filter(|e| e.wartet_auf_dich).count() as u64,
        aufgaben: eintraege.iter().filter(|e| e.art == EintragsArt::Aufgabe).count() as u64,
        fragen: eintraege.iter().filter(|e| e.art == EintragsArt::Frage).count() as u64,
        laeuft: arbeiten(ArbeitsStand::Laeuft),
        wartet: arbeiten(ArbeitsStand::Wartet),
        als_naechstes: arbeiten(ArbeitsStand::AlsNaechstes),
        meldungen: meldungs_zahlen(inhalt, slug),
        naechste_frist,
        tage_bis: tage,
        frist_stufe: stufe,
    }
}

/// `GET /api/projekte/<slug>/stand` und `/stand/voll`: der Zustand des
/// Projekts. `None`, wenn es das Projekt nicht gibt. Mit Windows-Anmeldung
/// (`sichtbar`) alle offenen Einträge und Aktualisierungen, neutral
/// geordnet, ohne Zahlen; ohne Anmeldung keine Einträge und keine Titel,
/// nur die Zahlen ([`stand_zahlen`]) und die Ampel.
///
/// Die Ampel ist eine Regel und bleibt hier: rot bei einer offenen
/// Fehler-Meldung oder einer Frist, die vorbei ist (bei Fragen nur die
/// Frage, die der Inhaber sieht; seit Etappe B3 auch eine offene Frist aus
/// `daten/fristen.toml`, dann ohne `bezug_art`), gelb, wenn etwas auf den Inhaber wartet, sonst grün;
/// ohne Datenspeicher unbekannt statt grün.
pub fn stand(inhalt: &Inhalt, slug: &str, sichtbar: bool) -> Option<ProjektStand> {
    let kopf = inhalt.projekte.iter().find(|p| p.slug == slug)?;
    let eintraege: Vec<AnstehendEintrag> = alle_eintraege(inhalt).into_iter().filter(|e| e.projekt == slug).collect();
    let auswahl = eine_frage_je_projekt(&eintraege);
    let wartet = auswahl.iter().any(|e| e.wartet_auf_dich);
    let vorbei = frueheste_frist(auswahl.iter().copied().filter(|e| e.frist_stufe == Some(FristStufe::Ueberfaellig)));
    // Etappe B3: offene Fristen aus fristen.toml mit Datum (fest oder aufgelöst), nach Datum.
    let datei = crate::fristen::datei_fristen_mit_datum(inhalt, slug);
    let datei_vorbei = datei.iter().find(|z| z.frist_stufe == Some(FristStufe::Ueberfaellig));
    // Etappe A3: eine offene Fehler-Meldung (nicht behoben, auch gelesen) macht die Ampel rot.
    let fehler = inhalt.speicher.meldungen.iter().filter(|m| m.projekt == slug && meldungen::fehler_offen(m)).max_by_key(|m| zeit_wert(&m.zuletzt));
    let (ampel, grund, bezug_art, bezug) = match (fehler, vorbei) {
        (Some(m), _) => (StandAmpel::Rot, AmpelGrund::FehlerOffen, Some(EintragsArt::Meldung), Some(m.titel.clone())),
        (None, Some(e)) => (StandAmpel::Rot, AmpelGrund::FristVorbei, Some(e.art), Some(e.titel.clone())),
        (None, None) if datei_vorbei.is_some() => (StandAmpel::Rot, AmpelGrund::FristVorbei, None, datei_vorbei.map(|z| z.titel.clone())),
        // Ohne Datenspeicher fehlen Aufgaben, Fristen und Meldungen: grün wäre geraten (Auflage aus A1).
        // Was wartet (Fragen aus den Dateien), bleibt gelb.
        (None, None) if !wartet && inhalt.speicher_fehlt => (StandAmpel::Unbekannt, AmpelGrund::SpeicherFehlt, None, None),
        (None, None) if !wartet => (StandAmpel::Gruen, AmpelGrund::NichtsWartet, None, None),
        (None, None) => (StandAmpel::Gelb, AmpelGrund::Wartet, None, None),
    };
    let zahlen = (!sichtbar).then(|| stand_zahlen(inhalt, slug, &eintraege, &datei));
    Some(ProjektStand {
        slug: slug.to_string(),
        schwerpunkt: inhalt.schwerpunkte.von(slug).0,
        gesamt: GesamtStand {
            titel: kopf.titel.clone(),
            status: kopf.status.clone(),
            bedeutung: Bedeutung::aus_wort_oder_neutral(&kopf.status),
            ampel,
            grund,
            bezug_art,
            // Ohne Anmeldung kein Titel einer Meldung oder eines Eintrags.
            bezug: bezug.filter(|_| sichtbar),
            zuletzt: kopf.zuletzt.clone(),
        },
        frist_bald_tage: FRIST_BALD_TAGE,
        sichtbar,
        aktualisierungen: if sichtbar { aktualisierungen(inhalt, slug) } else { Vec::new() },
        eintraege: if sichtbar { eintraege } else { Vec::new() },
        zahlen,
        // Etappe B2: open[] ist persönlicher Text, ohne Anmeldung nur die Zahl.
        notizen: if sichtbar { kopf.notizen.clone() } else { Vec::new() },
        notizen_zahl: kopf.notizen.len() as u64,
        speicher_fehlt: inhalt.speicher_fehlt,
        fristen: if sichtbar { datei } else { Vec::new() },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vertrag::api::{Arbeit, Aufgabe, AufgabenZustand, Einstufung};
    use crate::vertrag::daten::FrageArt;

    fn tag(t: &str) -> NaiveDate {
        NaiveDate::parse_from_str(t, "%Y-%m-%d").unwrap()
    }

    const HEUTE: &str = "2026-10-29";

    /// Die Stufenregel als Tabelle: Vorschlag, Inhaber, Frist → wirksam, Grund.
    #[test]
    fn stufenregel_tabelle() {
        use Stufe::{Bald as B, Irgendwann as I, Jetzt as J};
        use StufenGrund::{Frist as F, Inhaber as L, Vorschlag as V};
        type Fall<'a> = (Stufe, Option<Stufe>, Option<&'a str>, Stufe, StufenGrund);
        let faelle: &[Fall<'_>] = &[
            // Ohne Frist: Der Inhaber vor Vorschlag.
            (B, None, None, B, V),
            (I, None, None, I, V),
            (J, None, None, J, V),
            (B, Some(I), None, I, L),
            (I, Some(J), None, J, L),
            (J, Some(I), None, I, L),
            // Frist heute oder vorbei: jetzt, auch über die Einstufung des Inhabers.
            (I, None, Some("2026-10-29"), J, F),
            (B, Some(I), Some("2026-10-29"), J, F),
            (J, None, Some("2026-10-01"), J, F),
            (I, Some(I), Some("2026-10-28"), J, F),
            // Frist morgen bis in sieben Tagen: mindestens bald.
            (I, None, Some("2026-10-30"), B, F),
            (I, Some(I), Some("2026-11-05"), B, F),
            (B, None, Some("2026-10-30"), B, V),
            (J, None, Some("2026-11-05"), J, V),
            (B, Some(J), Some("2026-11-05"), J, L),
            // Frist in acht Tagen und später: zählt nicht; senkt nie.
            (I, None, Some("2026-11-06"), I, V),
            (J, None, Some("2027-01-01"), J, V),
            (B, Some(I), Some("2026-12-24"), I, L),
        ];
        for (n, (vorschlag, inhaber, frist, wirksam, grund)) in faelle.iter().enumerate() {
            let s = wirksame_stufe(*vorschlag, *inhaber, frist.map(tag), tag(HEUTE));
            assert_eq!((s.wirksam, s.grund), (*wirksam, *grund), "Fall {}: {vorschlag:?} {inhaber:?} {frist:?}", n + 1);
            assert_eq!((s.vorschlag, s.inhaber), (*vorschlag, *inhaber), "Fall {}", n + 1);
        }
    }

    /// Etappe 3a-4, Grenzfälle der Frist-Stufe: gestern, heute, morgen, genau
    /// an der Schwelle von bald (7 Tage) und einen Tag danach.
    #[test]
    fn frist_stufe_grenzfaelle() {
        use FristStufe::*;
        assert_eq!(FRIST_BALD_TAGE, 7);
        let heute = tag(HEUTE);
        for (frist, soll, wirksam) in [
            ("2026-10-28", Ueberfaellig, Stufe::Jetzt),
            ("2026-10-29", Heute, Stufe::Jetzt),
            ("2026-10-30", Bald, Stufe::Bald),
            ("2026-11-05", Bald, Stufe::Bald),
            ("2026-11-06", Spaeter, Stufe::Irgendwann),
        ] {
            assert_eq!(frist_stufe(tag(frist), heute), soll, "{frist}");
            // Dieselbe Quelle hebt die Stufe (hier von irgendwann aus).
            assert_eq!(wirksame_stufe(Stufe::Irgendwann, None, Some(tag(frist)), heute).wirksam, wirksam, "{frist}");
        }
        assert_eq!(serde_json::to_value(Ueberfaellig).unwrap(), "ueberfaellig");
        assert_eq!(serde_json::to_value(Spaeter).unwrap(), "spaeter");
    }

    /// „Heute“ ist das lokale Datum: um 23:59 ist eine Frist für morgen noch
    /// bald, um 0:00 (lokal, in UTC noch der Vortag) schon jetzt.
    #[test]
    fn tageswechsel_lokal() {
        use crate::vertrag::abgeleitet::heute_von;
        use chrono::{FixedOffset, TimeZone};
        let ortszeit = FixedOffset::east_opt(3600).unwrap();
        let spaet = ortszeit.with_ymd_and_hms(2026, 10, 29, 23, 59, 0).unwrap();
        let frueh = ortszeit.with_ymd_and_hms(2026, 10, 30, 0, 0, 0).unwrap();
        assert_eq!(frueh.naive_utc().date(), tag("2026-10-29"), "in UTC ist noch der Vortag");
        let frist = Some(tag("2026-10-30"));
        let s = wirksame_stufe(Stufe::Irgendwann, None, frist, heute_von(&spaet));
        assert_eq!((s.wirksam, tage_bis(tag("2026-10-30"), heute_von(&spaet))), (Stufe::Bald, 1));
        let s = wirksame_stufe(Stufe::Irgendwann, None, frist, heute_von(&frueh));
        assert_eq!((s.wirksam, s.grund), (Stufe::Jetzt, StufenGrund::Frist));
    }

    fn aufgabe(projekt: &str, id: &str, frist: Option<&str>, prioritaet: Option<Stufe>, erstellt: &str) -> Aufgabe {
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
            erstellt: erstellt.into(),
            geaendert: erstellt.into(),
            hat_link: true,
            vorgang: None,
            prioritaet,
            verlauf: Vec::new(),
            frist_bezug: None,
        }
    }

    fn arbeit(projekt: &str, id: &str, stand: ArbeitsStand, prioritaet: Stufe, erstellt: &str) -> Arbeit {
        Arbeit {
            id: id.into(),
            projekt: projekt.into(),
            von: format!("chat:{projekt}"),
            titel: format!("Arbeit {id}"),
            stand,
            wartet_auf: (stand == ArbeitsStand::Wartet).then(|| INHABER.to_string()),
            verweis: None,
            prioritaet,
            frist: None,
            notiz: None,
            erstellt: erstellt.into(),
            geaendert: erstellt.into(),
            verlauf: Vec::new(),
            besitzer: None,
            schritte: Vec::new(),
        }
    }

    fn frage(id: &str, gestellt: &str, prioritaet: Option<Stufe>, frist: Option<&str>) -> Frage {
        Frage {
            id: id.into(),
            text: format!("Frage {id}?"),
            art: FrageArt::Text,
            optionen: Vec::new(),
            empfehlung: None,
            gestellt: gestellt.into(),
            prioritaet,
            frist: frist.map(str::to_string),
        }
    }

    fn kopf(slug: &str) -> ProjektKopf {
        ProjektKopf {
            slug: slug.into(),
            titel: format!("Projekt {slug}"),
            status: "aktiv".into(),
            notizen: vec!["Notiz".into()],
            zuletzt: "2026-10-29 10:00".into(),
            archiviert: false,
        }
    }

    fn inhalt() -> Inhalt {
        let schwerpunkte = Schwerpunkte::lesen(
            "[[schwerpunkt]]\nname = \"Arbeit\"\nreihenfolge = 2\nprojekte = [\"beispiel\"]\n\n\
             [[schwerpunkt]]\nname = \"PC\"\nreihenfolge = 5\nprojekte = [\"dashboard\"]\n",
        )
        .unwrap();
        Inhalt {
            heute: tag(HEUTE),
            projekte: vec![kopf("beispiel"), kopf("dashboard"), kopf("hallo")],
            fragen: vec![FragenDatei {
                projekt: "beispiel".into(),
                datei: "projects/beispiel/daten/fragen.toml".into(),
                fragen: vec![frage("schlusssatz", "2026-10-25", None, None), frage("eilig", "2026-10-28", Some(Stufe::Irgendwann), Some("2026-10-30"))],
            }],
            fristen: Vec::new(),
            speicher: AusDemSpeicher {
                aufgaben: vec![
                    aufgabe("dashboard", "a-20261029-0001", None, Some(Stufe::Irgendwann), "2026-10-29T09:00:00+01:00"),
                    aufgabe("beispiel", "a-20261029-0002", Some("2026-10-29"), None, "2026-10-29T09:30:00+01:00"),
                    aufgabe("gibt-es-nicht", "a-20261029-0003", None, Some(Stufe::Jetzt), "2026-10-29T09:40:00+01:00"),
                ],
                arbeiten: vec![
                    arbeit("dashboard", "ar-20261029-0001", ArbeitsStand::Laeuft, Stufe::Bald, "2026-10-29T08:00:00+01:00"),
                    arbeit("dashboard", "ar-20261029-0002", ArbeitsStand::Fertig, Stufe::Jetzt, "2026-10-29T08:00:00+01:00"),
                    arbeit("hallo", "ar-20261029-0003", ArbeitsStand::AlsNaechstes, Stufe::Irgendwann, "2026-10-28T08:00:00+01:00"),
                ],
                einstufungen: Vec::new(),
                beantwortet: Default::default(),
                meldungen: Vec::new(),
                vorgaenge: Vec::new(),
                aufgaben_abgeschlossen: Vec::new(),
            },
            speicher_fehlt: false,
            schwerpunkte,
            gruppen: Vec::new(),
            hinweise: Vec::new(),
            lesefehler: Lesefehler::default(),
        }
    }

    fn ids(l: &[AnstehendEintrag]) -> Vec<String> {
        l.iter().map(|e| e.id.clone()).collect()
    }

    /// Etappe 3a+: vollständig (alle offenen Fragen, keine fertige Arbeit),
    /// in der neutralen Ordnung Projekt, Art, Kennung, mit allen Fakten.
    /// Die Sortier-Fälle von früher (Stufe, Frist, Alter, eine Frage je
    /// Projekt) stehen jetzt in web/src/lib/anstehend.spec.ts.
    #[test]
    fn vollstaendig_neutral_geordnet_mit_fakten() {
        let i = inhalt();
        let l = alle_eintraege(&i);
        assert_eq!(
            ids(&l),
            [
                "a-20261029-0002",  // beispiel, aufgabe
                "eilig",            // beispiel, frage
                "schlusssatz",      // beispiel, frage
                "a-20261029-0001",  // dashboard, aufgabe
                "ar-20261029-0001", // dashboard, arbeit (die fertige fehlt)
                "a-20261029-0003",  // gibt-es-nicht, aufgabe
                "ar-20261029-0003", // hallo, arbeit
            ]
        );
        let e = &l[0];
        assert_eq!((e.stufe.wirksam, e.stufe.grund, e.tage_bis, e.frist_stufe), (Stufe::Jetzt, StufenGrund::Frist, Some(0), Some(FristStufe::Heute)));
        assert_eq!(e.ziel, "/projects/beispiel#aufgabe-a-20261029-0002");
        assert_eq!((e.schwerpunkt.as_str(), e.schwerpunkt_reihenfolge, e.projekt_titel.as_deref(), e.ziel_da), ("Arbeit", 2, Some("Projekt beispiel"), true));
        let fremd = l.iter().find(|e| e.id == "a-20261029-0003").unwrap();
        assert_eq!((fremd.schwerpunkt.as_str(), fremd.schwerpunkt_reihenfolge, fremd.projekt_titel.clone(), fremd.ziel_da), (SONSTIGES, 6, None, false));
        let f = l.iter().find(|e| e.id == "schlusssatz").unwrap();
        assert_eq!(
            (f.ziel.as_str(), f.gestellt.as_deref(), f.quelle.as_str()),
            ("/projects/beispiel#frage-schlusssatz", Some("2026-10-25"), "projects/beispiel/daten/fragen.toml")
        );
        assert_eq!(
            (f.vermerk.clone(), f.wartet_auf_dich, f.erstellt.as_str()),
            (None, true, "2026-10-25"),
            "den Vermerk zu weiteren Fragen setzt die Oberfläche"
        );
        let eilig = l.iter().find(|e| e.id == "eilig").unwrap();
        assert_eq!((eilig.stufe.wirksam, eilig.stufe.grund, eilig.frist_stufe), (Stufe::Bald, StufenGrund::Frist, Some(FristStufe::Bald)));
        let a = l.iter().find(|e| e.id == "ar-20261029-0001").unwrap();
        assert_eq!(
            (a.ziel.as_str(), a.zustand.as_str(), a.wartet_auf.clone(), a.wartet_auf_dich),
            ("/projects/dashboard#arbeit-ar-20261029-0001", "laeuft", None, false)
        );
        assert_eq!((a.schwerpunkt.as_str(), a.schwerpunkt_reihenfolge), ("PC", 5));
        // Kein Eintrag zeigt auf eine fremde Adresse.
        assert!(l.iter().all(|e| e.ziel.starts_with("/projects/")));
        let wartet: Vec<&str> = l.iter().filter(|e| e.wartet_auf_dich).map(|e| e.id.as_str()).collect();
        assert_eq!(wartet, ["a-20261029-0002", "eilig", "schlusssatz", "a-20261029-0001", "a-20261029-0003"]);
        // Gleiche Daten, gleiche Antwort.
        assert_eq!(alle_eintraege(&i), l);
    }

    /// Die Server-Regel „eine Frage auf einmal“ (Platz 1 der Warteschlange):
    /// höchste Stufe, dann die frühere Frist, dann die Datei. „eilig“ (bald
    /// durch die Frist) steht so vor „schlusssatz“ (bald, ohne Frist).
    #[test]
    fn eine_frage_je_projekt_waehlt_wie_die_oberflaeche() {
        let mut i = inhalt();
        let l = alle_eintraege(&i);
        let a: Vec<&str> = eine_frage_je_projekt(&l).iter().map(|e| e.id.as_str()).collect();
        assert_eq!(a, ["a-20261029-0002", "eilig", "a-20261029-0001", "ar-20261029-0001", "a-20261029-0003", "ar-20261029-0003"]);
        // Der Inhaber stuft „eilig“ auf jetzt: dann ist sie die Frage des Projekts.
        i.speicher.einstufungen.push(Einstufung {
            projekt: "beispiel".into(),
            art: EintragsArt::Frage,
            id: "eilig".into(),
            gestellt: Some("2026-10-28".into()),
            stufe: Some(Stufe::Jetzt),
            zeit: "2026-10-29T10:00:00+01:00".into(),
        });
        let l = alle_eintraege(&i);
        assert!(eine_frage_je_projekt(&l).iter().any(|e| e.id == "eilig"));
        assert!(!eine_frage_je_projekt(&l).iter().any(|e| e.id == "schlusssatz"));
    }

    /// Der Inhaber stuft um: die Stufe ändert sich (die Ordnung bleibt neutral);
    /// „zurück zum Vorschlag“ stellt den alten Stand wieder her.
    /// Etappe B2, Fragen-Warteschlange: wirksame Stufe, dann die Reihenfolge
    /// in der Datei (nicht `gestellt`, nicht die Kennung). Jede offene Frage
    /// trägt ihren Platz; die nächste rückt nach der Antwort nach.
    #[test]
    fn fragen_warteschlange_nach_stufe_und_datei() {
        use Stufe::{Bald, Irgendwann, Jetzt};
        assert_eq!(fragen_warteschlange(&[]), Vec::<usize>::new());
        assert_eq!(fragen_warteschlange(&[(Bald, None), (Bald, None), (Bald, None)]), [0, 1, 2], "gleiche Stufe: wie in der Datei");
        assert_eq!(fragen_warteschlange(&[(Irgendwann, None), (Bald, None), (Jetzt, None), (Bald, None)]), [2, 1, 3, 0]);
        // Gleiche Stufe: die frühere Frist zuerst, ohne Frist danach, dann die Datei.
        assert_eq!(fragen_warteschlange(&[(Jetzt, None), (Jetzt, Some("2026-11-20")), (Jetzt, Some("2026-10-30")), (Jetzt, None)]), [2, 1, 0, 3]);
        // Die Stufe geht vor: eine spätere Frist mit Jetzt vor einer früheren mit Bald.
        assert_eq!(fragen_warteschlange(&[(Bald, Some("2026-11-01")), (Jetzt, Some("2026-12-01"))]), [1, 0]);
        let mut i = inhalt();
        // In der Datei zuerst „zweite“ (jüngeres Datum, Kennung später im Alphabet), dann „erste“.
        i.fragen[0].fragen = vec![
            frage("zweite", "2026-10-28", Some(Bald), None),
            frage("erste", "2026-10-20", Some(Bald), None),
            frage("spaeter", "2026-10-01", Some(Irgendwann), None),
        ];
        let l = alle_eintraege(&i);
        let platz = |id: &str| l.iter().find(|e| e.id == id).and_then(|e| e.warteschlange).map(|w| (w.platz, w.offen));
        assert_eq!((platz("zweite"), platz("erste"), platz("spaeter")), (Some((1, 3)), Some((2, 3)), Some((3, 3))));
        assert!(l.iter().filter(|e| e.art != EintragsArt::Frage).all(|e| e.warteschlange.is_none()));
        let sichtbar: Vec<&str> = eine_frage_je_projekt(&l).iter().filter(|e| e.art == EintragsArt::Frage).map(|e| e.id.as_str()).collect();
        assert_eq!(sichtbar, ["zweite"]);
        assert_eq!(fragen_auswahl(&l, "beispiel"), FragenAuswahl { sichtbar: Some("zweite".into()), gestellt: Some("2026-10-28".into()), weitere: 2 });
        assert_eq!(fragen_auswahl(&l, "dashboard"), FragenAuswahl { sichtbar: None, gestellt: None, weitere: 0 });
        // Beantwortet: die nächste rückt nach.
        i.speicher.beantwortet.insert(antwort_schluessel("beispiel", "zweite", "2026-10-28"));
        let l = alle_eintraege(&i);
        assert_eq!(fragen_auswahl(&l, "beispiel").sichtbar.as_deref(), Some("erste"));
        assert_eq!(l.iter().find(|e| e.id == "erste").and_then(|e| e.warteschlange), Some(FragenPlatz { platz: 1, offen: 2 }));
        // Die Zahlen ohne Anmeldung zählen je Projekt eine Frage.
        let s = stand(&i, "beispiel", false).unwrap();
        let z = s.zahlen.unwrap();
        assert_eq!((z.fragen, z.wartet_auf_dich), (2, 2), "zwei offene Fragen, davon wartet eine; dazu die Aufgabe");
    }

    #[test]
    fn einstufung_aendert_die_stufe() {
        let mut i = inhalt();
        let vorher = alle_eintraege(&i);
        i.speicher.einstufungen.push(Einstufung {
            projekt: "dashboard".into(),
            art: EintragsArt::Aufgabe,
            id: "a-20261029-0001".into(),
            gestellt: None,
            stufe: Some(Stufe::Jetzt),
            zeit: "2026-10-29T10:00:00+01:00".into(),
        });
        let l = alle_eintraege(&i);
        assert_eq!(ids(&l), ids(&vorher), "die Ordnung ist neutral");
        let e = l.iter().find(|e| e.id == "a-20261029-0001").unwrap();
        assert_eq!((e.stufe.wirksam, e.stufe.inhaber, e.stufe.grund), (Stufe::Jetzt, Some(Stufe::Jetzt), StufenGrund::Inhaber));
        i.speicher.einstufungen[0].stufe = None;
        assert_eq!(alle_eintraege(&i), vorher);
        // Frage: eine Einstufung mit altem `gestellt` gilt nicht für die neu gestellte Frage.
        i.speicher.einstufungen.push(Einstufung {
            projekt: "beispiel".into(),
            art: EintragsArt::Frage,
            id: "schlusssatz".into(),
            gestellt: Some("2026-10-01".into()),
            stufe: Some(Stufe::Jetzt),
            zeit: "2026-10-01T10:00:00+01:00".into(),
        });
        let f = alle_eintraege(&i).into_iter().find(|e| e.id == "schlusssatz").unwrap();
        assert_eq!(f.stufe.inhaber, None);
    }

    /// Filter der API (Projekt, Schwerpunkt, Stufe, Art, nur für mich); keine Zahlen mehr.
    #[test]
    fn filter() {
        let i = inhalt();
        let alle = anstehend(&i, &Filter::default());
        assert_eq!(alle.eintraege.len(), 7);
        assert_eq!((alle.heute.as_str(), alle.frist_bald_tage), (HEUTE, FRIST_BALD_TAGE));
        let f = Filter { art: Some(EintragsArt::Frage), ..Default::default() };
        assert_eq!(ids(&anstehend(&i, &f).eintraege), ["eilig", "schlusssatz"]);
        let f = Filter { schwerpunkt: Some("PC".into()), ..Default::default() };
        assert_eq!(ids(&anstehend(&i, &f).eintraege), ["a-20261029-0001", "ar-20261029-0001"]);
        let f = Filter { stufe: Some(Stufe::Jetzt), projekt: Some("beispiel".into()), ..Default::default() };
        assert_eq!(ids(&anstehend(&i, &f).eintraege), ["a-20261029-0002"]);
        let f = Filter { nur_fuer_mich: true, ..Default::default() };
        let l = anstehend(&i, &f);
        assert!(l.eintraege.iter().all(|e| e.art != EintragsArt::Arbeit && e.wartet_auf_dich), "{:?}", ids(&l.eintraege));
        assert_eq!(l.eintraege.len(), 5);
        let json = serde_json::to_value(&alle).unwrap();
        assert!(json.get("zaehler").is_none() && json.get("schwerpunkte").is_none(), "{json}");
    }

    /// Eine Arbeit, die mit Verweis auf eine offene Frage auf den Inhaber wartet,
    /// wartet nicht doppelt und trägt den Vermerk; ohne passenden Verweis wartet sie.
    #[test]
    fn arbeit_mit_verweis_wartet_nicht_doppelt() {
        let mut i = inhalt();
        let mut a = arbeit("beispiel", "ar-20261029-0009", ArbeitsStand::Wartet, Stufe::Bald, "2026-10-29T11:00:00+01:00");
        a.verweis = Some("schlusssatz".into());
        i.speicher.arbeiten.push(a.clone());
        let l = alle_eintraege(&i);
        let e = l.iter().find(|e| e.id == a.id).unwrap();
        assert_eq!((e.vermerk.as_deref(), e.wartet_auf_dich), (Some(VERMERK_WARTET), false));
        i.speicher.arbeiten.last_mut().unwrap().verweis = Some("gibt-es-nicht".into());
        let l = alle_eintraege(&i);
        let e = l.iter().find(|e| e.id == a.id).unwrap();
        assert_eq!((e.vermerk.clone(), e.wartet_auf_dich), (None, true));
    }

    fn schritt(nr: u32, stand: ArbeitsStand, besitzer: Option<&str>, wartet_auf: Option<&str>, verweis: Option<&str>) -> crate::vertrag::api::Schritt {
        crate::vertrag::api::Schritt {
            nr,
            titel: format!("Schritt {nr}"),
            stand,
            besitzer: besitzer.map(str::to_string),
            wartet_auf: wartet_auf.map(str::to_string),
            verweis: verweis.map(str::to_string),
            angeboten_an: None,
            fuer: None,
            seit: "2026-10-29T10:00:00+01:00".into(),
            notiz: None,
            vorgang: None,
        }
    }

    /// T4: Wartet ein Schritt auf den Inhaber, wartet die Arbeit auf ihn, auch
    /// wenn ein anderer läuft (Stand, Zahlen, Ampel, verweis); die Schritte
    /// stehen am Eintrag, ohne Anmeldung nur in den Zahlen.
    #[test]
    fn arbeit_mit_schritten_in_anstehend_und_stand() {
        let mut i = inhalt();
        i.speicher.arbeiten.retain(|a| a.projekt != "hallo");
        let mut a = arbeit("hallo", "ar-20261029-0010", ArbeitsStand::AlsNaechstes, Stufe::Bald, "2026-10-29T08:00:00+01:00");
        a.schritte = vec![schritt(1, ArbeitsStand::Laeuft, Some("chat:hallo"), None, None), schritt(2, ArbeitsStand::AlsNaechstes, None, None, None)];
        crate::speicher::schritte::stand_ableiten(&mut a);
        i.speicher.arbeiten.push(a.clone());
        // Nur einer läuft: die Arbeit läuft, nichts wartet auf dich, grün.
        let s = stand(&i, "hallo", true).unwrap();
        assert_eq!((s.gesamt.ampel, s.eintraege.len()), (StandAmpel::Gruen, 1));
        assert_eq!((s.eintraege[0].zustand.as_str(), s.eintraege[0].schritte.clone()), ("laeuft", a.schritte.clone()));
        let z = stand(&i, "hallo", false).unwrap().zahlen.unwrap();
        assert_eq!((z.laeuft, z.wartet, z.wartet_auf_dich), (1, 0, 0));

        // Schritt 2 wartet auf den Inhaber, Schritt 1 läuft weiter: die Arbeit wartet auf dich.
        a.schritte[1] = schritt(2, ArbeitsStand::Wartet, Some("chat:architektur"), Some(INHABER), Some("gibt-es-nicht"));
        crate::speicher::schritte::stand_ableiten(&mut a);
        *i.speicher.arbeiten.last_mut().unwrap() = a.clone();
        let l = anstehend(&i, &Filter { nur_fuer_mich: true, ..Default::default() });
        let e = l.eintraege.iter().find(|e| e.id == a.id).expect("wartet auf dich");
        assert_eq!((e.zustand.as_str(), e.wartet_auf.as_deref(), e.verweis.as_deref()), ("wartet", Some(INHABER), Some("gibt-es-nicht")));
        assert_eq!(e.schritte.len(), 2);
        let s = stand(&i, "hallo", true).unwrap();
        assert_eq!((s.gesamt.ampel, s.gesamt.grund), (StandAmpel::Gelb, AmpelGrund::Wartet));
        let ohne = stand(&i, "hallo", false).unwrap();
        assert!(ohne.eintraege.is_empty() && ohne.gesamt.bezug.is_none());
        let z = ohne.zahlen.as_ref().unwrap();
        assert_eq!((z.laeuft, z.wartet, z.wartet_auf_dich), (0, 1, 1), "die Arbeit zählt einmal, mit ihrem abgeleiteten Stand");
        let json = serde_json::to_string(&ohne).unwrap();
        assert!(!json.contains("Schritt 2") && !json.contains("chat:architektur"), "ohne Anmeldung keine Titel und Besitzer: {json}");

        // Verweist der Schritt auf eine offene Aufgabe, zählt jene (nicht doppelt) und die Arbeit trägt den Vermerk.
        i.speicher.aufgaben.push(aufgabe("hallo", "a-20261029-0020", None, None, "2026-10-29T09:00:00+01:00"));
        a.schritte[1].verweis = Some("a-20261029-0020".into());
        crate::speicher::schritte::stand_ableiten(&mut a);
        *i.speicher.arbeiten.last_mut().unwrap() = a.clone();
        let z = stand(&i, "hallo", false).unwrap().zahlen.unwrap();
        assert_eq!((z.aufgaben, z.wartet, z.wartet_auf_dich), (1, 1, 1));
        let l = alle_eintraege(&i);
        let e = l.iter().find(|e| e.id == a.id).unwrap();
        assert_eq!((e.vermerk.as_deref(), e.wartet_auf_dich, e.verweis.as_deref()), (Some(VERMERK_WARTET), false, Some("a-20261029-0020")));
        // Ohne Schritte fehlt das Feld im JSON (ältere Arbeiten unverändert).
        let alt = l.iter().find(|e| e.art == EintragsArt::Arbeit && e.id != a.id).unwrap();
        assert!(serde_json::to_value(alt).unwrap().get("schritte").is_none());
    }

    /// T5 (Auflage aus T4): Warten zwei Schritte auf dich, zählt jeder mit
    /// seinem eigenen Verweis, nicht nur der erste. (i) Der erste verweist auf
    /// eine beantwortete Frage, der zweite auf eine offene Aufgabe: Vermerk,
    /// nur die Aufgabe ist offen. (ii) Beide offen: beide, die Arbeit zählt
    /// nicht mit (keine Doppelzählung), auf dieselbe Aufgabe zweimal: einmal.
    #[test]
    fn zwei_schritte_warten_auf_dich() {
        let mut i = inhalt();
        let auf = "a-20261105-0031";
        i.speicher.aufgaben.push(aufgabe("beispiel", auf, None, None, "2026-11-05T09:00:00+01:00"));
        let vorher = stand(&i, "beispiel", false).unwrap().zahlen.unwrap().wartet_auf_dich;
        let mut a = arbeit("beispiel", "ar-20261105-0030", ArbeitsStand::AlsNaechstes, Stufe::Bald, "2026-11-05T08:00:00+01:00");
        let wartend = |nr, v: &str| schritt(nr, ArbeitsStand::Wartet, Some("chat:dashboard"), Some(INHABER), Some(v));
        a.schritte = vec![wartend(1, "schlusssatz"), wartend(2, auf)];
        crate::speicher::schritte::stand_ableiten(&mut a);
        i.speicher.arbeiten.push(a.clone());
        let finde = |i: &Inhalt| alle_eintraege(i).into_iter().find(|e| e.id == "ar-20261105-0030").unwrap();

        // (ii) beide offen
        let e = finde(&i);
        assert_eq!((e.vermerk.as_deref(), e.wartet_auf_dich), (Some(VERMERK_WARTET), false));
        assert_eq!(e.offene_verweise, ["schlusssatz", auf]);
        assert_eq!(stand(&i, "beispiel", false).unwrap().zahlen.unwrap().wartet_auf_dich, vorher, "die Arbeit zählt nicht zusätzlich");

        // (i) erste Frage beantwortet: Schritt 1 zeigt auf nichts Offenes mehr, darum
        // kein Vermerk und die Arbeit zählt selbst (Auflage aus T5); der Link an Schritt 2 bleibt.
        i.speicher.beantwortet.insert(antwort_schluessel("beispiel", "schlusssatz", "2026-10-25"));
        let e = finde(&i);
        assert_eq!(e.verweis.as_deref(), Some("schlusssatz"), "die Arbeit trägt weiter den Verweis des ersten Schritts");
        assert_eq!((e.vermerk.as_deref(), e.wartet_auf_dich, e.offene_verweise.clone()), (None, true, vec![auf.to_string()]));
        assert_eq!(stand(&i, "beispiel", false).unwrap().zahlen.unwrap().wartet_auf_dich, vorher + 1, "die Arbeit zählt");

        // Ein wartender Schritt ohne Verweis (alter Schritt 1 mit übernommenem Warten) neben einem offenen: die Arbeit zählt.
        i.speicher.arbeiten.last_mut().unwrap().schritte[0].verweis = None;
        let e = finde(&i);
        assert_eq!((e.vermerk.as_deref(), e.wartet_auf_dich, e.offene_verweise.clone()), (None, true, vec![auf.to_string()]));
        i.speicher.arbeiten.last_mut().unwrap().schritte[0].verweis = Some("schlusssatz".into());

        // Zwei Schritte auf dieselbe Aufgabe: sie steht einmal da.
        i.speicher.arbeiten.last_mut().unwrap().schritte[0].verweis = Some(auf.into());
        assert_eq!(finde(&i).offene_verweise, [auf]);

        // Keiner offen: kein Vermerk, die Arbeit wartet selbst auf dich; das Feld fehlt im JSON.
        i.speicher.aufgaben.retain(|x| x.id != auf);
        let e = finde(&i);
        assert_eq!((e.vermerk.clone(), e.wartet_auf_dich), (None, true));
        assert!(serde_json::to_value(&e).unwrap().get("offene_verweise").is_none());
    }

    /// Dateien lesen: Fragen aus zwei Projekten; eine kaputte
    /// fragen.toml und eine fehlende schwerpunkte.toml werden Hinweise, nicht Abbruch.
    #[test]
    fn dateien_lesen_mit_hinweisen() {
        let basis = std::env::temp_dir().join(format!("dashboard-test-{}-anstehend", std::process::id()));
        let _ = std::fs::remove_dir_all(&basis);
        let projekte = basis.join("projects");
        let frage = "[[frage]]\nid = \"f\"\ntext = \"Passt das?\"\nart = \"ja-nein\"\ngestellt = \"2026-10-28\"\nprioritaet = \"jetzt\"\n";
        std::fs::create_dir_all(projekte.join("alt").join("daten")).unwrap();
        std::fs::write(projekte.join("alt").join("projekt.toml"), "schema = 3\ntitle = \"Alt\"\nstatus = \"aktiv\"\nsummary = \"S\"\nopen = [\"eins\"]\n")
            .unwrap();
        std::fs::write(projekte.join("alt").join("daten").join("fragen.toml"), frage).unwrap();
        std::fs::create_dir_all(projekte.join("neu").join("daten")).unwrap();
        std::fs::write(projekte.join("neu").join("projekt.toml"), "schema = 3\ntitle = \"Neu\"\nstatus = \"pausiert\"\nsummary = \"S\"\n").unwrap();
        std::fs::write(projekte.join("neu").join("daten").join("fragen.toml"), format!("{frage}\n{frage}")).unwrap();
        let aus = AusDenDateien::lesen(&projekte, &basis.join("data").join("schwerpunkte.toml"), tag(HEUTE));
        assert_eq!(aus.projekte.iter().map(|p| (p.slug.as_str(), p.status.as_str())).collect::<Vec<_>>(), [("alt", "aktiv"), ("neu", "pausiert")]);
        assert_eq!(aus.fragen.len(), 1, "nur die lesbare Datei");
        assert_eq!(aus.lesefehler, Lesefehler { fragen: BTreeSet::from(["neu".to_string()]), fristen: BTreeSet::new(), ordner: false });
        assert_eq!((aus.fragen[0].projekt.as_str(), aus.fragen[0].fragen[0].prioritaet), ("alt", Some(Stufe::Jetzt)));
        assert_eq!(aus.hinweise.len(), 2, "{:?}", aus.hinweise);
        assert!(aus.hinweise[0].starts_with("projects/neu/daten/fragen.toml, Zeile 9: Die Frage „f“ gibt es zweimal."), "{:?}", aus.hinweise);
        assert!(aus.hinweise[1].starts_with("data/schwerpunkte.toml fehlt"), "{:?}", aus.hinweise);
        let i = Inhalt::aus(aus, AusDemSpeicher::default(), tag(HEUTE));
        let l = anstehend(&i, &Filter::default());
        assert_eq!((l.eintraege.len(), l.eintraege[0].stufe.wirksam, l.eintraege[0].schwerpunkt.as_str()), (1, Stufe::Jetzt, SONSTIGES));
        assert_eq!(stand(&i, "alt", true).unwrap().notizen, ["eins"]);
        // Eine kaputte schwerpunkte.toml: Hinweis mit Zeile, alles unter „Sonstiges“.
        std::fs::create_dir_all(basis.join("data")).unwrap();
        std::fs::write(basis.join("data").join("schwerpunkte.toml"), "[[schwerpunkt]]\nname = \"A\"\n").unwrap();
        let aus = AusDenDateien::lesen(&projekte, &basis.join("data").join("schwerpunkte.toml"), tag(HEUTE));
        assert!(aus.hinweise.iter().any(|h| h.starts_with("data/schwerpunkte.toml") && h.contains("reihenfolge")), "{:?}", aus.hinweise);
        std::fs::remove_dir_all(&basis).unwrap();
    }

    /// Etappe A2: Eine beantwortete Frage ist nicht mehr offen, auch wenn sie
    /// noch in fragen.toml steht; Stand und Zahlen folgen. Eine Antwort mit
    /// altem `gestellt` gilt nicht. Welche Frage nachrückt, wählt die
    /// Oberfläche (vitest, Fall „beantwortet“).
    #[test]
    fn beantwortete_frage_fehlt() {
        let mut i = inhalt();
        i.fragen[0].fragen.push(frage("dritte", "2026-10-29", Some(Stufe::Irgendwann), None));
        let s = stand(&i, "beispiel", true).unwrap();
        let fragen: Vec<&str> = s.eintraege.iter().filter(|e| e.art == EintragsArt::Frage).map(|e| e.id.as_str()).collect();
        assert_eq!(fragen, ["dritte", "eilig", "schlusssatz"]);
        let f = s.eintraege.iter().find(|e| e.id == "schlusssatz").unwrap();
        assert_eq!(f.frage, Some(FrageAngabe { art: FrageArt::Text, optionen: Vec::new(), empfehlung: None }));
        let z = stand(&i, "beispiel", false).unwrap().zahlen.unwrap();
        assert_eq!((z.fragen, z.wartet_auf_dich), (3, 2), "je Projekt zählt höchstens eine Frage");
        // Eine Antwort zu einem alten `gestellt` ändert nichts.
        i.speicher.beantwortet.insert(antwort_schluessel("beispiel", "schlusssatz", "2026-10-01"));
        assert!(alle_eintraege(&i).iter().any(|e| e.id == "schlusssatz"));
        // Beantwortet: weg aus Liste und Stand.
        i.speicher.beantwortet.insert(antwort_schluessel("beispiel", "schlusssatz", "2026-10-25"));
        let l = alle_eintraege(&i);
        assert!(l.iter().all(|e| e.id != "schlusssatz"), "{:?}", ids(&l));
        let z = stand(&i, "beispiel", false).unwrap().zahlen.unwrap();
        assert_eq!((z.fragen, z.wartet_auf_dich), (2, 2));
        // Alle beantwortet: keine Frage mehr, die Ampel hängt nur noch an der Aufgabe.
        i.speicher.beantwortet.insert(antwort_schluessel("beispiel", "eilig", "2026-10-28"));
        i.speicher.beantwortet.insert(antwort_schluessel("beispiel", "dritte", "2026-10-29"));
        let z = stand(&i, "beispiel", false).unwrap().zahlen.unwrap();
        assert_eq!((z.fragen, z.wartet_auf_dich), (0, 1));
        i.speicher.aufgaben.retain(|a| a.projekt != "beispiel");
        let s = stand(&i, "beispiel", true).unwrap();
        assert_eq!((s.gesamt.ampel, s.gesamt.grund, s.eintraege.len()), (StandAmpel::Gruen, AmpelGrund::NichtsWartet, 0));
        // Eine Arbeit, die auf eine beantwortete Frage verweist, trägt den Vermerk nicht mehr.
        let mut a = arbeit("beispiel", "ar-20261029-0009", ArbeitsStand::Wartet, Stufe::Bald, "2026-10-29T11:00:00+01:00");
        a.verweis = Some("schlusssatz".into());
        i.speicher.arbeiten.push(a);
        let l = alle_eintraege(&i);
        assert_eq!(l.iter().find(|e| e.id == "ar-20261029-0009").unwrap().vermerk, None);
    }

    /// Unter-Dashboard: Ampel als Regel; mit Anmeldung alle Einträge ohne
    /// Zahlen, ohne Anmeldung keine Einträge, nur Zahlen und kein Titel.
    #[test]
    fn stand_eines_projekts() {
        let mut i = inhalt();
        let s = stand(&i, "dashboard", true).unwrap();
        assert_eq!((s.gesamt.ampel, s.gesamt.grund, s.gesamt.bezug.clone()), (StandAmpel::Gelb, AmpelGrund::Wartet, None));
        assert_eq!((ids(&s.eintraege), s.sichtbar, s.zahlen.clone()), (vec!["a-20261029-0001".to_string(), "ar-20261029-0001".to_string()], true, None));
        assert_eq!((s.notizen.clone(), s.schwerpunkt.as_str(), s.frist_bald_tage), (vec!["Notiz".to_string()], "PC", FRIST_BALD_TAGE));
        assert!(s.aktualisierungen.is_empty());
        let ohne = stand(&i, "dashboard", false).unwrap();
        assert!(ohne.eintraege.is_empty() && !ohne.sichtbar);
        let z = ohne.zahlen.clone().unwrap();
        assert_eq!((z.aufgaben, z.fragen, z.laeuft, z.wartet, z.als_naechstes, z.wartet_auf_dich), (1, 0, 1, 0, 0, 1));
        // Leeres Projekt: grün.
        let leer = stand(&i, "hallo", false).unwrap();
        assert_eq!((leer.gesamt.ampel, leer.zahlen.unwrap().als_naechstes), (StandAmpel::Gruen, 1));
        // Frist vorbei: rot; ohne Anmeldung ohne den Titel.
        i.speicher.aufgaben[1].frist = Some("2026-10-27".into());
        let rot = stand(&i, "beispiel", true).unwrap();
        assert_eq!((rot.gesamt.ampel, rot.gesamt.grund, rot.gesamt.bezug_art), (StandAmpel::Rot, AmpelGrund::FristVorbei, Some(EintragsArt::Aufgabe)));
        assert_eq!(rot.gesamt.bezug.as_deref(), Some("Aufgabe a-20261029-0002"));
        let rot = stand(&i, "beispiel", false).unwrap();
        assert_eq!((rot.gesamt.grund, rot.gesamt.bezug_art, rot.gesamt.bezug.clone()), (AmpelGrund::FristVorbei, Some(EintragsArt::Aufgabe), None));
        let z = rot.zahlen.unwrap();
        assert_eq!((z.naechste_frist.as_deref(), z.tage_bis, z.frist_stufe), (Some("2026-10-27"), Some(-2), Some(FristStufe::Ueberfaellig)));
        assert!(stand(&i, "gibt-es-nicht", true).is_none());
        // Alle Aufgaben, keine Auswahl (früher höchstens drei; jetzt vitest).
        for n in 4..9 {
            i.speicher.aufgaben.push(aufgabe("dashboard", &format!("a-20261029-000{n}"), None, None, "2026-10-29T12:00:00+01:00"));
        }
        let s = stand(&i, "dashboard", true).unwrap();
        assert_eq!(s.eintraege.iter().filter(|e| e.art == EintragsArt::Aufgabe).count(), 6);
        assert_eq!(stand(&i, "dashboard", false).unwrap().zahlen.unwrap().aufgaben, 6);
    }

    fn meldung(projekt: &str, id: &str, art: crate::vertrag::api::MeldungsArt, zuletzt: &str) -> Meldung {
        Meldung {
            id: id.into(),
            projekt: projekt.into(),
            art,
            schluessel: format!("{projekt}/{id}"),
            titel: format!("Meldung {id}"),
            text: None,
            verweis: None,
            system: false,
            geschuetzt: false,
            von: "server".into(),
            erstellt: zuletzt.into(),
            zuletzt: zuletzt.into(),
            anzahl: 1,
            gelesen: None,
            behoben: None,
            archiviert: None,
            zusammengefasst: false,
        }
    }

    /// Etappe A3: offene Meldungen stehen in der Liste mit fester Stufe nach
    /// Art; gelesene und behobene nicht; sie warten nicht auf den Inhaber; das Ziel
    /// ist die Projektseite, Systemmeldungen die PC-Seite.
    #[test]
    fn meldungen_in_der_liste() {
        use crate::vertrag::api::MeldungsArt as M;
        let mut i = inhalt();
        i.speicher.meldungen = vec![
            meldung("beispiel", "m-1", M::Fehler, "2026-10-29T08:00:00+01:00"),
            meldung("beispiel", "m-2", M::Fertig, "2026-10-29T09:00:00+01:00"),
            meldung("hallo", "m-3", M::Info, "2026-10-29T10:00:00+01:00"),
            meldung("dashboard", "m-4", M::Frist, "2026-10-29T11:00:00+01:00"),
            meldung("beispiel", "m-5", M::Info, "2026-10-29T12:00:00+01:00"),
            meldung("dashboard", "m-6", M::Fehler, "2026-10-29T13:00:00+01:00"),
        ];
        i.speicher.meldungen[4].gelesen = Some("2026-10-29T12:30:00+01:00".into());
        i.speicher.meldungen[5].behoben = Some("2026-10-29T13:30:00+01:00".into());
        let mut system = meldung("dashboard", "m-7", M::Fehler, "2026-10-29T14:00:00+01:00");
        system.system = true;
        i.speicher.meldungen.push(system);
        let l = alle_eintraege(&i);
        let m: Vec<&AnstehendEintrag> = l.iter().filter(|e| e.art == EintragsArt::Meldung).collect();
        assert_eq!(m.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(), ["m-1", "m-2", "m-4", "m-7", "m-3"], "neutral: Projekt, dann Kennung");
        let stufen: Vec<(Stufe, StufenGrund)> = m.iter().map(|e| (e.stufe.wirksam, e.stufe.grund)).collect();
        assert_eq!(stufen[0], (Stufe::Jetzt, StufenGrund::Fest));
        assert_eq!(stufen[1].0, Stufe::Bald);
        assert_eq!(stufen[4].0, Stufe::Irgendwann);
        assert_eq!(m[0].ziel, "/projects/beispiel#meldung-m-1");
        assert_eq!((m[3].ziel.as_str(), m[3].ziel_da), ("/pc#meldung-m-7", true));
        assert!(m.iter().all(|e| e.wartet_auf.is_none() && !e.wartet_auf_dich && e.meldung.is_some() && e.zustand == "ungelesen"));
        // Filter nach Art.
        let f = Filter { art: Some(EintragsArt::Meldung), projekt: Some("beispiel".into()), ..Default::default() };
        assert_eq!(ids(&anstehend(&i, &f).eintraege), ["m-1", "m-2"]);
    }

    /// Gleiche Vorgänge (Art und Titel, etwa zwei Bauten, die ein Lauf
    /// zugleich abschließt) sind eine Zeile: Anzahl ist die Summe, Zeit und
    /// Ziel vom neuesten (Zeit, bei gleicher Zeit die höhere Kennung).
    #[test]
    fn gleiche_vorgaenge_eine_zeile() {
        use crate::vertrag::api::{Vorgang, VorgangsArt, Zustand};
        let mut i = inhalt();
        let vorgang = |id: &str, art: VorgangsArt, titel: &str, zeit: &str| Vorgang {
            id: id.into(),
            art,
            slug: "hallo".into(),
            titel: titel.into(),
            bezug: None,
            zustand: Zustand::Erledigt,
            erstellt: zeit.into(),
            geaendert: zeit.into(),
            letzte_nr: 1,
            von: "browser".into(),
            notiz: None,
            bau: None,
            merge: None,
        };
        i.speicher.vorgaenge.extend([
            vorgang("v-20261031-0009", VorgangsArt::Bau, "Seite neu bauen", "2026-11-13T00:04:34+01:00"),
            vorgang("v-20261031-0010", VorgangsArt::Bau, "Seite neu bauen", "2026-11-13T00:04:34+01:00"),
            vorgang("v-20261106-0033", VorgangsArt::Schritt, "Dir angeboten: Schritt 12", "2026-11-06T20:10:00+01:00"),
            vorgang("v-20261106-0035", VorgangsArt::Schritt, "Dir angeboten: Schritt 12", "2026-11-06T19:25:00+01:00"),
            vorgang("v-20261106-0037", VorgangsArt::Schritt, "Dir angeboten: Schritt 12", "2026-11-06T18:41:00+01:00"),
            vorgang("v-20261106-0040", VorgangsArt::Wunsch, "Seite neu bauen", "2026-11-06T18:00:00+01:00"),
        ]);
        let a = aktualisierungen(&i, "hallo");
        let zeilen: Vec<(&str, &str, u64, &str)> = a.iter().map(|x| (x.id.as_str(), x.art.as_str(), x.anzahl, x.zeit.as_str())).collect();
        assert_eq!(
            zeilen,
            [
                ("v-20261031-0010", "bau", 2, "2026-11-13T00:04:34+01:00"),
                ("v-20261106-0033", "schritt", 3, "2026-11-06T20:10:00+01:00"),
                ("v-20261106-0040", "wunsch", 1, "2026-11-06T18:00:00+01:00"),
            ],
            "gleiche Zeit: höhere Kennung; sonst die neueste Zeit; andere Art bleibt eigene Zeile"
        );
        assert_eq!(a[0].ziel, "/vorgaenge#vorgang-v-20261031-0010");
    }

    /// Ampel: rot bei offenem Fehler, auch gelesen; behoben nicht mehr.
    /// Aktualisierungen: alle, neutral nach Quelle und Kennung (welche fünf
    /// zuerst, wählt die Oberfläche), nur mit Anmeldung; ohne nur die Zahlen.
    #[test]
    fn ampel_und_aktualisierungen() {
        use crate::vertrag::api::{MeldungsArt as M, Vorgang, VorgangsArt, Zustand};
        let mut i = inhalt();
        let mut f = meldung("hallo", "m-1", M::Fehler, "2026-10-29T08:00:00+01:00");
        f.gelesen = Some("2026-10-29T08:30:00+01:00".into());
        i.speicher.meldungen.push(f);
        i.speicher.meldungen[0].text = Some("projects/hallo/daten/geheim.toml, Zeile 1".into());
        let s = stand(&i, "hallo", true).unwrap();
        assert_eq!((s.gesamt.ampel, s.gesamt.grund, s.gesamt.bezug.as_deref()), (StandAmpel::Rot, AmpelGrund::FehlerOffen, Some("Meldung m-1")));
        let s = stand(&i, "hallo", false).unwrap();
        assert_eq!(
            (s.gesamt.ampel, s.gesamt.grund, s.gesamt.bezug_art, s.gesamt.bezug.clone()),
            (StandAmpel::Rot, AmpelGrund::FehlerOffen, Some(EintragsArt::Meldung), None),
            "ohne Anmeldung kein Titel"
        );
        i.speicher.meldungen[0].behoben = Some("2026-10-29T09:00:00+01:00".into());
        assert_eq!(stand(&i, "hallo", true).unwrap().gesamt.ampel, StandAmpel::Gruen);
        for (n, t) in ["09", "10", "11", "12", "13"].iter().enumerate() {
            let mut m = meldung("hallo", &format!("m-{}", n + 2), M::Info, &format!("2026-10-29T{t}:00:00+01:00"));
            m.geschuetzt = n == 4;
            i.speicher.meldungen.push(m);
        }
        i.speicher.vorgaenge.push(Vorgang {
            id: "v-20261029-0001".into(),
            art: VorgangsArt::Antwort,
            slug: "hallo".into(),
            titel: "Aufgabe erledigt: Geheimer Titel".into(),
            bezug: None,
            zustand: Zustand::Offen,
            erstellt: "2026-10-29T12:30:00+01:00".into(),
            geaendert: "2026-10-29T12:30:00+01:00".into(),
            letzte_nr: 1,
            von: "browser".into(),
            notiz: None,
            bau: None,
            merge: None,
        });
        let a = aktualisierungen(&i, "hallo");
        assert_eq!(a.iter().map(|x| x.id.as_str()).collect::<Vec<_>>(), ["m-1", "m-2", "m-3", "m-4", "m-5", "m-6", "v-20261029-0001"]);
        assert_eq!((a[6].quelle.as_str(), a[6].ziel.as_str(), a[6].gelesen), ("vorgang", "/vorgaenge#vorgang-v-20261029-0001", true));
        assert_eq!((a[5].titel.as_str(), a[5].gelesen, a[5].ziel.as_str()), ("Meldung m-6", false, "/projects/hallo#meldung-m-6"));
        let mit = stand(&i, "hallo", true).unwrap();
        assert_eq!(mit.aktualisierungen, a);
        let ohne = stand(&i, "hallo", false).unwrap();
        assert!(ohne.aktualisierungen.is_empty() && ohne.eintraege.is_empty(), "ohne Anmeldung keine Titel und Texte");
        assert_eq!(ohne.zahlen.clone().unwrap().meldungen, MeldungsZahlen { neu: 5, gesamt: 6 }, "nur Zahlen");
        let json = serde_json::to_string(&ohne).unwrap();
        for geheim in ["Meldung m-", "geheim.toml", "Geheimer Titel", "Arbeit ar-"] {
            assert!(!json.contains(geheim), "{geheim} in {json}");
        }
        let l = anstehend(&i, &Filter { projekt: Some("hallo".into()), ..Default::default() });
        assert_eq!(l.aktualisierungen, a);
        assert!(anstehend(&i, &Filter::default()).aktualisierungen.is_empty(), "nur mit Filter projekt");
        // Ohne Datenspeicher: unbekannt statt grün.
        let mut leer = inhalt();
        leer.speicher = AusDemSpeicher::default();
        leer.fragen.clear();
        leer.speicher_fehlt = true;
        let s = stand(&leer, "hallo", true).unwrap();
        assert_eq!((s.gesamt.ampel, s.gesamt.grund), (StandAmpel::Unbekannt, AmpelGrund::SpeicherFehlt));
    }

    /// Die feste Testdatenlage für die Vorlagen (Etappe 3a+): mehr als drei
    /// Aufgaben, mehr als zwei Arbeiten je Gruppe, drei Fragen in einem
    /// Projekt, eine Arbeit mit Verweis, Meldungen und Vorgänge über fünf,
    /// ein unbekanntes Projekt, eine Einstufung durch den Inhaber, eine Frist vorbei.
    fn vorlage_inhalt() -> Inhalt {
        use crate::vertrag::api::{MeldungsArt as M, Vorgang, VorgangsArt, Zustand};
        let mut i = inhalt();
        i.fragen[0].fragen.push(frage("dritte", "2026-10-29", Some(Stufe::Irgendwann), None));
        i.fragen.push(FragenDatei {
            projekt: "hallo".into(),
            datei: "projects/hallo/daten/fragen.toml".into(),
            fragen: vec![frage("gruss", "2026-10-27", Some(Stufe::Jetzt), None)],
        });
        for n in 4..9 {
            i.speicher.aufgaben.push(aufgabe(
                "dashboard",
                &format!("a-20261029-000{n}"),
                if n == 5 { Some("2026-11-03") } else { None },
                None,
                &format!("2026-10-29T1{n}:00:00+01:00"),
            ));
        }
        i.speicher.aufgaben.push(aufgabe("zeta", "a-20261029-0010", Some("2026-11-20"), Some(Stufe::Bald), "2026-10-28T07:00:00+01:00"));
        for (n, stand) in
            [(4, ArbeitsStand::Laeuft), (5, ArbeitsStand::Laeuft), (6, ArbeitsStand::Wartet), (7, ArbeitsStand::AlsNaechstes), (8, ArbeitsStand::Wartet)]
        {
            i.speicher.arbeiten.push(arbeit(
                "dashboard",
                &format!("ar-20261029-000{n}"),
                stand,
                if n % 2 == 0 { Stufe::Jetzt } else { Stufe::Bald },
                &format!("2026-10-2{n}T08:00:00+01:00"),
            ));
        }
        let mut verweis = arbeit("beispiel", "ar-20261029-0009", ArbeitsStand::Wartet, Stufe::Bald, "2026-10-29T11:00:00+01:00");
        verweis.verweis = Some("schlusssatz".into());
        i.speicher.arbeiten.push(verweis);
        let mut vorbei = arbeit("hallo", "ar-20261029-0010", ArbeitsStand::Laeuft, Stufe::Irgendwann, "2026-10-20T08:00:00+01:00");
        vorbei.frist = Some("2026-10-27".into());
        i.speicher.arbeiten.push(vorbei);
        i.speicher.einstufungen.push(Einstufung {
            projekt: "dashboard".into(),
            art: EintragsArt::Aufgabe,
            id: "a-20261029-0006".into(),
            gestellt: None,
            stufe: Some(Stufe::Irgendwann),
            zeit: "2026-10-29T10:00:00+01:00".into(),
        });
        i.speicher.meldungen = vec![
            meldung("beispiel", "m-1", M::Fehler, "2026-10-29T08:00:00+01:00"),
            meldung("beispiel", "m-2", M::Fertig, "2026-10-29T09:00:00+01:00"),
            meldung("hallo", "m-3", M::Info, "2026-10-29T10:00:00+01:00"),
            meldung("dashboard", "m-4", M::Frist, "2026-10-29T11:00:00+01:00"),
            meldung("beispiel", "m-5", M::Info, "2026-10-29T12:00:00+01:00"),
            meldung("dashboard", "m-6", M::Fehler, "2026-10-29T13:00:00+01:00"),
        ];
        i.speicher.meldungen[4].gelesen = Some("2026-10-29T12:30:00+01:00".into());
        i.speicher.meldungen[5].behoben = Some("2026-10-29T13:30:00+01:00".into());
        let mut system = meldung("dashboard", "m-7", M::Fehler, "2026-10-29T14:00:00+01:00");
        system.system = true;
        i.speicher.meldungen.push(system);
        for (n, t) in ["07", "15", "16", "17", "18"].iter().enumerate() {
            let mut m = meldung("hallo", &format!("m-{}", n + 8), M::Info, &format!("2026-10-29T{t}:00:00+01:00"));
            m.gelesen = (n % 2 == 1).then(|| format!("2026-10-29T{t}:30:00+01:00"));
            i.speicher.meldungen.push(m);
        }
        for (n, t) in ["2026-10-28T12:30:00+01:00", "2026-10-29T16:30:00+01:00"].iter().enumerate() {
            i.speicher.vorgaenge.push(Vorgang {
                id: format!("v-20261029-000{}", n + 1),
                art: VorgangsArt::Antwort,
                slug: "hallo".into(),
                titel: format!("Vorgang {}", n + 1),
                bezug: None,
                zustand: Zustand::Offen,
                erstellt: (*t).into(),
                geaendert: (*t).into(),
                letzte_nr: 1,
                von: "browser".into(),
                notiz: None,
                bau: None,
                merge: None,
            });
        }
        i
    }

    /// Die Antworten des neuen Servers für die Vorlagen und die vitest-Fälle
    /// (Etappe 3a+): Name der Datei unter `tests/vorlagen/` → JSON.
    /// `neu/` gehört zu [`vorlage_inhalt`] (dieselben Daten wie `alt/`, das
    /// vor dem Umbau einmal geschrieben wurde und so bleibt); `faelle/` sind
    /// die Fälle, die früher hier die Sortierung prüften.
    fn vorlagen_neu() -> Vec<(String, serde_json::Value)> {
        use crate::vertrag::api::MeldungsArt as M;
        fn wert<T: serde::Serialize>(v: &T) -> serde_json::Value {
            serde_json::to_value(v).unwrap()
        }
        let mut aus: Vec<(String, serde_json::Value)> = Vec::new();
        let i = vorlage_inhalt();
        aus.push(("neu/anstehend.json".into(), wert(&anstehend(&i, &Filter::default()))));
        for slug in ["beispiel", "dashboard", "hallo"] {
            let f = Filter { projekt: Some(slug.into()), ..Default::default() };
            aus.push((format!("neu/anstehend-{slug}.json"), wert(&anstehend(&i, &f))));
            aus.push((format!("neu/stand-{slug}-voll.json"), wert(&stand(&i, slug, true).unwrap())));
            aus.push((format!("neu/stand-{slug}-frei.json"), wert(&stand(&i, slug, false).unwrap())));
        }
        // Fälle aus den früheren Sortier-Tests.
        let mut i = inhalt();
        aus.push(("faelle/grund.json".into(), wert(&anstehend(&i, &Filter::default()))));
        i.speicher.einstufungen.push(Einstufung {
            projekt: "dashboard".into(),
            art: EintragsArt::Aufgabe,
            id: "a-20261029-0001".into(),
            gestellt: None,
            stufe: Some(Stufe::Jetzt),
            zeit: "2026-10-29T10:00:00+01:00".into(),
        });
        aus.push(("faelle/einstufung.json".into(), wert(&anstehend(&i, &Filter::default()))));
        let mut i = inhalt();
        let mut a = arbeit("beispiel", "ar-20261029-0009", ArbeitsStand::Wartet, Stufe::Bald, "2026-10-29T11:00:00+01:00");
        a.verweis = Some("schlusssatz".into());
        i.speicher.arbeiten.push(a);
        aus.push(("faelle/verweis.json".into(), wert(&anstehend(&i, &Filter::default()))));
        let mut i = inhalt();
        i.fragen[0].fragen.push(frage("dritte", "2026-10-29", Some(Stufe::Irgendwann), None));
        aus.push(("faelle/beantwortet-0.json".into(), wert(&stand(&i, "beispiel", true).unwrap())));
        i.speicher.beantwortet.insert(antwort_schluessel("beispiel", "schlusssatz", "2026-10-25"));
        aus.push(("faelle/beantwortet-1.json".into(), wert(&stand(&i, "beispiel", true).unwrap())));
        let mut i = inhalt();
        for n in 4..9 {
            i.speicher.aufgaben.push(aufgabe("dashboard", &format!("a-20261029-000{n}"), None, None, "2026-10-29T12:00:00+01:00"));
        }
        aus.push(("faelle/viele-aufgaben.json".into(), wert(&stand(&i, "dashboard", true).unwrap())));
        let mut i = inhalt();
        i.speicher.meldungen = vec![
            meldung("beispiel", "m-1", M::Fehler, "2026-10-29T08:00:00+01:00"),
            meldung("beispiel", "m-2", M::Fertig, "2026-10-29T09:00:00+01:00"),
            meldung("hallo", "m-3", M::Info, "2026-10-29T10:00:00+01:00"),
            meldung("dashboard", "m-4", M::Frist, "2026-10-29T11:00:00+01:00"),
        ];
        let mut system = meldung("dashboard", "m-7", M::Fehler, "2026-10-29T14:00:00+01:00");
        system.system = true;
        i.speicher.meldungen.push(system);
        for (n, t) in ["09", "10", "11", "12", "13"].iter().enumerate() {
            i.speicher.meldungen.push(meldung("hallo", &format!("m-{}", n + 10), M::Info, &format!("2026-10-29T{t}:00:00+01:00")));
        }
        aus.push(("faelle/meldungen.json".into(), wert(&anstehend(&i, &Filter::default()))));
        aus.push(("faelle/meldungen-hallo.json".into(), wert(&anstehend(&i, &Filter { projekt: Some("hallo".into()), ..Default::default() }))));
        aus
    }

    /// Die Vorlagen unter `tests/vorlagen/neu` und `faelle` passen
    /// zur Rechnung. Wie beim Vertrag: weicht etwas ab, schreibt der Test den
    /// neuen Stand und schlägt fehl; der zweite Lauf ist grün.
    #[test]
    fn vorlagen_neu_aktuell() {
        let ordner = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/vorlagen");
        let mut abweichend = Vec::new();
        for (name, wert) in vorlagen_neu() {
            let pfad = ordner.join(&name);
            let text = serde_json::to_string_pretty(&wert).unwrap() + "\n";
            if std::fs::read_to_string(&pfad).ok().as_deref() != Some(text.as_str()) {
                std::fs::create_dir_all(pfad.parent().unwrap()).unwrap();
                std::fs::write(&pfad, &text).unwrap();
                abweichend.push(name);
            }
        }
        assert!(abweichend.is_empty(), "Vorlagen neu geschrieben (cargo test noch einmal, dann einchecken): {abweichend:?}");
        assert!(ordner.join("alt/anstehend.json").exists(), "die alten Vorlagen bleiben als Bezug");
    }
}
