//! Der Verlauf (Etappe 2e, Entscheidung „Neue Zeitachse, Fassung 1“,
//! `[server_zustand]` und `[plan_2e_bestaetigt]`): eine Liste von
//! Ereignissen in einer Form, gleich aus welcher Quelle.
//!
//! Der Server liefert Fakten: wann (mit Genauigkeit), was, woher, welche Art
//! fachlich, und eine stabile Kennung. Er sortiert nicht, gruppiert nicht und
//! wählt keine Farben; das macht die Oberfläche (`[grenze_bestaetigt]`).
//!
//! Hier stehen der Vertrag (`Ereignis`, `Verlauf`) und die Übersetzung der
//! Einträge von Hand: `data/pc.toml` `[[history]]` und `daten/zeitstrahl.toml`
//! `[[eintrag]]`. Die Windows-Ereignisse baut `pc::ereignisse`, Meldungen,
//! Vorgänge und Sicherungen `api::pc`.

use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, NaiveDateTime, SecondsFormat, TimeZone};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::bedeutung::Bedeutung;
use super::daten::{Datei, Zeitstrahl};

/// Ein Ereignis im Verlauf. In TypeScript `VerlaufEreignis`, weil es
/// `Ereignis` schon gibt (das Journal der Vorgänge).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(rename = "VerlaufEreignis")]
pub struct Ereignis {
    /// Stabil, aus der Quelle: `hand:<kennung>`, `update:<KB>:<datum>`,
    /// `start:<zeit>`, `windows-installiert:<datum>`, `meldung:<id>`,
    /// `sicherung:<datei>`, `vorgang:<id>`.
    pub kennung: String,
    /// Beginn, RFC 3339 mit Zone (Ortszeit). Bei `tag`, `monat` und `jahr`
    /// der Anfang des Tages, Monats oder Jahres.
    pub zeit: String,
    pub genauigkeit: Genauigkeit,
    /// Nur bei Spannen: der Anfang des letzten Abschnitts, in derselben
    /// Genauigkeit wie `zeit` (bei „2026-02 – 2026-03“ der 1. März).
    pub bis: Option<String>,
    pub titel: String,
    /// Ungekürzt.
    pub text: Option<String>,
    pub quelle: Quelle,
    pub art: Art,
    /// Adresse im Dashboard, etwa `/vorgaenge` oder `/daten`.
    pub verweis: Option<String>,
}

/// Wie genau `zeit` (und `bis`) bekannt ist. Die Reihenfolge geht von fein
/// nach grob.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum Genauigkeit {
    Minute,
    Tag,
    Monat,
    Jahr,
}

/// Woher ein Ereignis kommt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(rename = "VerlaufQuelle")]
pub enum Quelle {
    /// Von Hand: `data/pc.toml`, `daten/zeitstrahl.toml`, Baustein `timeline`.
    Hand,
    /// Windows wurde installiert (Registry `InstallDate`).
    WindowsInstalliert,
    /// Hotfix oder Update-Ereignis (nach KB und Tag zusammengeführt).
    Update,
    /// Windows wurde gestartet (Kernel-General, Ereignis 12).
    Start,
    /// Systemmeldung des Dashboards (nur mit Anmeldung).
    Meldung,
    /// Sicherung des Datenspeichers.
    Sicherung,
    /// Vorgang des Dashboards (nur mit Anmeldung).
    Vorgang,
}

/// Was das Ereignis fachlich ist; die Farbe macht die Oberfläche daraus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(rename = "VerlaufArt")]
pub enum Art {
    Erledigt,
    Begonnen,
    Geplant,
    Abgebrochen,
    /// Wiederkehrend und ohne Handlungsbedarf (Starts, Defender-Signaturen,
    /// Store-Updates, tägliche Sicherungen).
    Routine,
    Problem,
}

/// Das Wort einer Art, wie in TOML und JSON.
pub fn art_wort(a: Art) -> &'static str {
    match a {
        Art::Erledigt => "erledigt",
        Art::Begonnen => "begonnen",
        Art::Geplant => "geplant",
        Art::Abgebrochen => "abgebrochen",
        Art::Routine => "routine",
        Art::Problem => "problem",
    }
}

/// `GET /api/pc/verlauf`: die Ereignisse im Zeitraum, ohne zugesagte
/// Reihenfolge, und Hinweise zur Vollständigkeit.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct Verlauf {
    /// Ohne zugesagte Reihenfolge und ohne Gruppen.
    pub ereignisse: Vec<Ereignis>,
    /// Der Zeitraum, RFC 3339 mit Zone.
    pub seit: String,
    pub bis: String,
    /// Was fehlt oder nur teilweise da ist (etwa ohne Anmeldung keine
    /// Meldungen und Vorgänge): je eine fachliche `art` und der Satz dazu.
    pub hinweise: Vec<VerlaufHinweis>,
}

/// Ein Hinweis zur Vollständigkeit des Verlaufs (Etappe 3a-4). Die
/// Oberfläche filtert nach `art`; eine Art, die sie nicht kennt, zeigt sie
/// mit dem Satz unverändert.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct VerlaufHinweis {
    pub art: VerlaufHinweisArt,
    /// Ein Satz in einfachem Deutsch.
    pub text: String,
}

impl VerlaufHinweis {
    pub fn neu(art: VerlaufHinweisArt, text: impl Into<String>) -> Self {
        Self { art, text: text.into() }
    }
}

/// Was an einem Verlauf fehlt, als Kennung in kebab-case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum VerlaufHinweisArt {
    /// Ohne Anmeldung fehlen Starts, Meldungen und Vorgänge.
    OhneAnmeldung,
    /// Starts und Update-Ereignisse gibt es erst ab einem Datum.
    BeginntAb,
    /// Der Datenspeicher ist nicht offen.
    OhneSpeicher,
    /// Der letzte Lauf des Sammlers der Windows-Ereignisse war unvollständig.
    SammlerUnvollstaendig,
    /// Der Sicherungsordner ließ sich nicht lesen.
    SicherungenFehlen,
    /// `data/pc.toml` ließ sich nicht lesen: die Einträge von Hand fehlen.
    HandFehlt,
    /// Ein Eintrag von Hand ohne lesbares Datum fehlt.
    DatumUnlesbar,
    /// Einträge von Hand ohne `kennung` (bis Schritt 6 erlaubt).
    OhneKennung,
    /// Eine `kennung` steht doppelt da.
    KennungDoppelt,
}

// ----- Zeit -----

/// Zeit als Text: RFC 3339, Sekunden, mit Zone (`2026-10-30T08:12:33+01:00`).
pub fn zeit_text<Tz: TimeZone>(t: &DateTime<Tz>) -> String
where
    Tz::Offset: std::fmt::Display,
{
    t.to_rfc3339_opts(SecondsFormat::Secs, false)
}

/// Mitternacht am Anfang von `tag` in Ortszeit. Gibt es sie wegen einer
/// Zeitumstellung nicht, die erste gültige Zeit danach.
pub fn tagesanfang(tag: NaiveDate) -> DateTime<Local> {
    let mitternacht = tag.and_hms_opt(0, 0, 0).unwrap_or_default();
    ortszeit(mitternacht)
}

/// Eine Ortszeit ohne Zone in Ortszeit mit Zone (bei Doppeldeutigkeit die
/// frühere, in einer Lücke eine Stunde später).
pub fn ortszeit(t: NaiveDateTime) -> DateTime<Local> {
    Local
        .from_local_datetime(&t)
        .earliest()
        .or_else(|| Local.from_local_datetime(&(t + Duration::hours(1))).earliest())
        .unwrap_or_else(|| Local.from_utc_datetime(&t))
}

/// Das Ende eines Ereignisses für Zeiträume: das Ende des Abschnitts von
/// `bis` (oder `zeit`) in seiner Genauigkeit; bei `minute` die Zeit selbst.
pub fn ende(e: &Ereignis) -> Option<DateTime<chrono::FixedOffset>> {
    let letzte = DateTime::parse_from_rfc3339(e.bis.as_deref().unwrap_or(&e.zeit)).ok()?;
    let tag = letzte.date_naive();
    let naechster = match e.genauigkeit {
        Genauigkeit::Minute => return Some(letzte),
        Genauigkeit::Tag => tag.succ_opt()?,
        Genauigkeit::Monat => monat_danach(tag)?,
        Genauigkeit::Jahr => NaiveDate::from_ymd_opt(tag.year() + 1, 1, 1)?,
    };
    Some(tagesanfang(naechster).fixed_offset() - Duration::seconds(1))
}

fn monat_danach(tag: NaiveDate) -> Option<NaiveDate> {
    if tag.month() == 12 { NaiveDate::from_ymd_opt(tag.year() + 1, 1, 1) } else { NaiveDate::from_ymd_opt(tag.year(), tag.month() + 1, 1) }
}

/// Liegt das Ereignis (mit seiner Spanne) ganz oder teilweise im Zeitraum?
pub fn im_zeitraum(e: &Ereignis, seit: &DateTime<chrono::FixedOffset>, bis: &DateTime<chrono::FixedOffset>) -> bool {
    let Ok(anfang) = DateTime::parse_from_rfc3339(&e.zeit) else { return false };
    let schluss = ende(e).unwrap_or(anfang);
    anfang <= *bis && schluss >= *seit
}

// ----- Daten von Hand -----

/// Ein Zeitpunkt von Hand: `JJJJ-MM-TT`, `JJJJ-MM` oder `JJJJ`.
fn punkt_lesen(s: &str) -> Option<(NaiveDate, Genauigkeit)> {
    let s = s.trim();
    let zahl = |t: &str, n: usize| t.len() == n && t.chars().all(|c| c.is_ascii_digit());
    let teile: Vec<&str> = s.split('-').collect();
    match teile.as_slice() {
        [j] if zahl(j, 4) => Some((NaiveDate::from_ymd_opt(j.parse().ok()?, 1, 1)?, Genauigkeit::Jahr)),
        [j, m] if zahl(j, 4) && zahl(m, 2) => Some((NaiveDate::from_ymd_opt(j.parse().ok()?, m.parse().ok()?, 1)?, Genauigkeit::Monat)),
        [j, m, t] if zahl(j, 4) && zahl(m, 2) && zahl(t, 2) => {
            Some((NaiveDate::from_ymd_opt(j.parse().ok()?, m.parse().ok()?, t.parse().ok()?)?, Genauigkeit::Tag))
        }
        _ => None,
    }
}

/// Ein Datum von Hand mit Genauigkeit und, bei Spannen, dem letzten
/// Abschnitt. Erlaubt: `2026-10-24`, `2026-10`, `2026`, Spannen mit
/// Gedankenstrich oder „bis“ (`2019 – 2021`, `2025–2026`, `2028-09 bis
/// 2029-04`) und offene Spannen (`2026–`, dann ohne Ende). Die Genauigkeit
/// einer Spanne ist die gröbere der beiden Seiten. `None`: kein Datum
/// (etwa „später“) oder das Ende liegt vor dem Anfang.
pub fn datum_lesen(text: &str) -> Option<(NaiveDate, Genauigkeit, Option<NaiveDate>)> {
    let t = text.trim();
    let geteilt = ["–", "—", " bis ", " - "].iter().find_map(|trenner| t.split_once(trenner));
    let Some((links, rechts)) = geteilt else {
        let (tag, g) = punkt_lesen(t)?;
        return Some((tag, g, None));
    };
    let (von, g_von) = punkt_lesen(links)?;
    if rechts.trim().is_empty() {
        return Some((von, g_von, None));
    }
    let (bis, g_bis) = punkt_lesen(rechts)?;
    if bis < von {
        return None;
    }
    Some((von, g_von.max(g_bis), Some(bis)))
}

/// Text als Teil einer Kennung: Kleinbuchstaben, Ziffern und Bindestriche,
/// Umlaute ausgeschrieben, höchstens 60 Zeichen.
pub fn kennung_aus_text(text: &str) -> String {
    let mut aus = String::new();
    for c in text.trim().chars().flat_map(char::to_lowercase) {
        let ersatz = match c {
            'ä' => "ae",
            'ö' => "oe",
            'ü' => "ue",
            'ß' => "ss",
            _ => "",
        };
        if !ersatz.is_empty() {
            aus.push_str(ersatz);
        } else if c.is_ascii_lowercase() || c.is_ascii_digit() {
            aus.push(c);
        } else if !aus.ends_with('-') {
            aus.push('-');
        }
    }
    let kurz: String = aus.trim_matches('-').chars().take(60).collect();
    kurz.trim_end_matches('-').to_string()
}

/// Kennung eines Eintrags von Hand: mit `kennung` `hand:<kennung>`, sonst
/// (alte Form, bis Schritt 6 erlaubt) `hand:<datum>:<titel>` aus Datum und Titel.
pub fn hand_kennung(kennung: Option<&str>, datum: &str, titel: &str) -> String {
    match kennung.map(str::trim).filter(|k| !k.is_empty()) {
        Some(k) => format!("hand:{k}"),
        None => format!("hand:{}:{}", kennung_aus_text(datum), kennung_aus_text(titel)),
    }
}

/// `art` aus der alten `farbe` (bis Schritt 6): problem zu problem, wartet
/// zu begonnen, plan (Bedeutung `neutral`) zu geplant, sonst erledigt.
/// Zwei Verfeinerungen: `laeuft` (aktiv, in Arbeit) ist begonnen, und ohne
/// Farbe oder `neutral` ist nur geplant, was noch kommt (sonst erledigt;
/// „grau“ für 2017 heißt nicht geplant).
pub fn art_aus_farbe(farbe: Option<Bedeutung>, kommt_noch: bool) -> Art {
    match farbe {
        Some(Bedeutung::Problem) => Art::Problem,
        Some(Bedeutung::Wartet | Bedeutung::Laeuft) => Art::Begonnen,
        Some(Bedeutung::Neutral) | None if kommt_noch => Art::Geplant,
        _ => Art::Erledigt,
    }
}

/// Ein Eintrag von Hand als Ereignis; `None`, wenn das Datum keins ist.
/// `art` fehlt: aus `farbe` (siehe [`art_aus_farbe`]).
pub struct Hand<'a> {
    pub kennung: Option<&'a str>,
    pub datum: &'a str,
    pub titel: &'a str,
    pub text: Option<&'a str>,
    pub art: Option<Art>,
    pub farbe: Option<Bedeutung>,
}

impl Hand<'_> {
    pub fn ereignis(&self, heute: NaiveDate) -> Option<Ereignis> {
        let (von, genauigkeit, bis) = datum_lesen(self.datum)?;
        let art = self.art.unwrap_or_else(|| art_aus_farbe(self.farbe, von > heute));
        Some(Ereignis {
            kennung: hand_kennung(self.kennung, self.datum, self.titel),
            zeit: zeit_text(&tagesanfang(von)),
            genauigkeit,
            bis: bis.map(|b| zeit_text(&tagesanfang(b))),
            titel: self.titel.to_string(),
            text: self.text.map(str::to_string).filter(|t| !t.trim().is_empty()),
            quelle: Quelle::Hand,
            art,
            verweis: None,
        })
    }
}

/// Doppelte Kennungen in einer Liste eindeutig machen: das erste Vorkommen
/// behält seine Kennung, jedes weitere bekommt die kleinste Endung `-2`,
/// `-3`, …, die keine echte Kennung der Liste belegt. Darum sammelt die
/// Funktion erst alle Kennungen: Teil A mit zweimal `schritt` und danach
/// Teil B mit `schritt-2` ergibt A `schritt` und `schritt-3`, B behält
/// `schritt-2` (Auflage aus der Abnahme 3a-3). Gibt die Zahl der geänderten
/// zurück.
pub fn eindeutig_machen(liste: &mut [Ereignis]) -> usize {
    let mut belegt: std::collections::HashSet<String> = liste.iter().map(|e| e.kennung.clone()).collect();
    let mut behalten = std::collections::HashSet::new();
    let mut geaendert = 0;
    for e in liste.iter_mut() {
        if behalten.insert(e.kennung.clone()) {
            continue;
        }
        let mut n = 2;
        while belegt.contains(&format!("{}-{n}", e.kennung)) {
            n += 1;
        }
        e.kennung = format!("{}-{n}", e.kennung);
        belegt.insert(e.kennung.clone());
        behalten.insert(e.kennung.clone());
        geaendert += 1;
    }
    geaendert
}

/// Die Einträge von `daten/zeitstrahl.toml` (alle Teile) als Ereignisse.
/// Das Datum prüft schon der Vertrag; was trotzdem keins ist, fehlt.
pub fn aus_zeitstrahl(d: &Datei<Zeitstrahl>, heute: NaiveDate) -> Vec<Ereignis> {
    let mut liste: Vec<Ereignis> = d
        .inhalte()
        .into_iter()
        .flat_map(|z| &z.eintrag)
        .filter_map(|e| {
            Hand { kennung: e.kennung.as_deref(), datum: &e.datum, titel: &e.titel, text: e.text.as_deref(), art: e.art, farbe: e.farbe }.ereignis(heute)
        })
        .collect();
    eindeutig_machen(&mut liste);
    liste
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tag(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn daten_von_hand() {
        use Genauigkeit::*;
        assert_eq!(datum_lesen("2026-10-24"), Some((tag("2026-10-24"), Tag, None)));
        assert_eq!(datum_lesen("2026-11"), Some((tag("2026-11-01"), Monat, None)));
        assert_eq!(datum_lesen(" 2028 "), Some((tag("2028-01-01"), Jahr, None)));
        assert_eq!(datum_lesen("2019 – 2021"), Some((tag("2019-01-01"), Jahr, Some(tag("2021-01-01")))));
        assert_eq!(datum_lesen("2025–2026"), Some((tag("2025-01-01"), Jahr, Some(tag("2026-01-01")))));
        assert_eq!(datum_lesen("2028-09 bis 2029-04"), Some((tag("2028-09-01"), Monat, Some(tag("2029-04-01")))));
        assert_eq!(datum_lesen("2026-06 – 2026-09"), Some((tag("2026-06-01"), Monat, Some(tag("2026-09-01")))));
        assert_eq!(datum_lesen("2026-02-10 - 2026-03"), Some((tag("2026-02-10"), Monat, Some(tag("2026-03-01")))));
        assert_eq!(datum_lesen("2026–"), Some((tag("2026-01-01"), Jahr, None)));
        for kein in ["später", "offen", "", "2026-13", "2026-02-30", "24.10.2026", "2021 – 2019"] {
            assert_eq!(datum_lesen(kein), None, "{kein}");
        }
    }

    #[test]
    fn kennung_aus_datum_und_titel() {
        assert_eq!(kennung_aus_text("Größe prüfen: Über 100 %!"), "groesse-pruefen-ueber-100");
        assert_eq!(hand_kennung(None, "2019 – 2021", "Lehrgang"), "hand:2019-2021:lehrgang");
        assert_eq!(hand_kennung(Some("ranke"), "2026-10-24", "egal"), "hand:ranke");
        assert_eq!(hand_kennung(Some(" "), "2026-10-24", "A"), "hand:2026-10-24:a");
        assert_eq!(kennung_aus_text(&"x".repeat(80)).len(), 60);
    }

    #[test]
    fn farbe_wird_art() {
        use Bedeutung::*;
        assert_eq!(art_aus_farbe(Some(Problem), false), Art::Problem);
        assert_eq!(art_aus_farbe(Some(Wartet), true), Art::Begonnen);
        assert_eq!(art_aus_farbe(Some(Laeuft), false), Art::Begonnen);
        assert_eq!(art_aus_farbe(Some(Neutral), true), Art::Geplant);
        assert_eq!(art_aus_farbe(Some(Neutral), false), Art::Erledigt);
        assert_eq!(art_aus_farbe(None, true), Art::Geplant);
        assert_eq!(art_aus_farbe(None, false), Art::Erledigt);
        assert_eq!(art_aus_farbe(Some(Ok), true), Art::Erledigt);
        assert_eq!(art_aus_farbe(Some(Info), false), Art::Erledigt);
        // Das Wort „plan“ aus dem Markdown ist die Bedeutung `neutral`.
        assert_eq!(Bedeutung::aus_wort("plan"), Some(Neutral));
    }

    #[test]
    fn eintrag_von_hand_als_ereignis() {
        let heute = tag("2026-10-30");
        let e = Hand { kennung: None, datum: "2026-02 – 2026-03", titel: "Pilot", text: Some(" "), art: None, farbe: None }.ereignis(heute).unwrap();
        assert_eq!(e.kennung, "hand:2026-02-2026-03:pilot");
        assert_eq!(e.genauigkeit, Genauigkeit::Monat);
        assert!(e.zeit.starts_with("2026-02-01T00:00:00"), "{}", e.zeit);
        assert!(e.bis.as_deref().unwrap().starts_with("2026-03-01T00:00:00"));
        assert_eq!((e.text, e.art, e.quelle), (None, Art::Erledigt, Quelle::Hand));
        DateTime::parse_from_rfc3339(&e.zeit).expect("RFC 3339 mit Zone");
        let k = Hand { kennung: Some("k"), datum: "2027", titel: "T", text: None, art: Some(Art::Abgebrochen), farbe: None };
        assert_eq!(k.ereignis(heute).unwrap().art, Art::Abgebrochen);
        let json = serde_json::to_value(k.ereignis(heute).unwrap()).unwrap();
        assert_eq!((json["quelle"].as_str(), json["art"].as_str(), json["genauigkeit"].as_str()), (Some("hand"), Some("abgebrochen"), Some("jahr")));
        assert_eq!(serde_json::to_value(Quelle::WindowsInstalliert).unwrap(), "windows-installiert");
    }

    #[test]
    fn spannen_im_zeitraum() {
        let heute = tag("2026-10-30");
        let e = Hand { kennung: None, datum: "2026-06 – 2026-09", titel: "S", text: None, art: None, farbe: None }.ereignis(heute).unwrap();
        let t = |s: &str| tagesanfang(tag(s)).fixed_offset();
        assert!(im_zeitraum(&e, &t("2026-09-20"), &t("2026-10-30")), "der September gehört noch dazu");
        assert!(!im_zeitraum(&e, &t("2026-10-01"), &t("2026-10-30")));
        assert!(im_zeitraum(&e, &t("2026-01-01"), &t("2026-06-01")));
        assert!(!im_zeitraum(&e, &t("2026-01-01"), &t("2026-05-31")));
    }

    #[test]
    fn doppelte_kennungen_werden_eindeutig() {
        let heute = tag("2026-10-30");
        let z = |d| Hand { kennung: None, datum: d, titel: "Gleich", text: None, art: None, farbe: Some(Bedeutung::Ok) }.ereignis(heute);
        assert!(z("später").is_none(), "ohne lesbares Datum kein Ereignis");
        let mut l: Vec<Ereignis> = ["2026-10-24", "2026-10-24", "2026-10-24"].into_iter().filter_map(z).collect();
        assert_eq!(eindeutig_machen(&mut l), 2);
        let k: Vec<&str> = l.iter().map(|e| e.kennung.as_str()).collect();
        assert_eq!(k, ["hand:2026-10-24:gleich", "hand:2026-10-24:gleich-2", "hand:2026-10-24:gleich-3"]);
    }

    #[test]
    fn endungen_belegen_keine_echte_kennung() {
        // Reihenfolge A, A, B: Teil A hat zweimal `schritt`, Teil B danach
        // `schritt-2`. B behält seine echte Kennung; A weicht auf -3 aus.
        let heute = tag("2026-10-30");
        let h = |k, titel| Hand { kennung: Some(k), datum: "2026-10-24", titel, text: None, art: None, farbe: None }.ereignis(heute).unwrap();
        let mut l = vec![h("schritt", "A eins"), h("schritt", "A zwei"), h("schritt-2", "B")];
        assert_eq!(eindeutig_machen(&mut l), 1);
        let k: Vec<(&str, &str)> = l.iter().map(|e| (e.kennung.as_str(), e.titel.as_str())).collect();
        assert_eq!(k, [("hand:schritt", "A eins"), ("hand:schritt-3", "A zwei"), ("hand:schritt-2", "B")]);
        // Auch eine später doppelte echte Kennung bekommt keine schon belegte Endung.
        let mut l = vec![h("x", "1"), h("x", "2"), h("x-2", "3"), h("x-2", "4"), h("x-3", "5")];
        assert_eq!(eindeutig_machen(&mut l), 2);
        let k: Vec<&str> = l.iter().map(|e| e.kennung.as_str()).collect();
        assert_eq!(k, ["hand:x", "hand:x-4", "hand:x-2", "hand:x-2-2", "hand:x-3"]);
    }
}
