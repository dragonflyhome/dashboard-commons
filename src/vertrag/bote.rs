//! Protokoll zwischen Dienst und Bote (Etappe 7a1): eine JSON-Zeile je
//! Anfrage und Antwort über die benannte Pipe, getrennt durch `\n`, jede Zeile
//! höchstens `MAX_ZEILE` Bytes. Der Dienst fragt, der Bote antwortet.
//!
//! Form: `{"art":"tresor_info","daten":{"ziel":"ranke/x"}}`, ohne Daten nur
//! `{"art":"hallo"}`. Diese Typen gehen nicht an die Oberfläche (kein
//! TypeScript); die Nutzdaten sind die vorhandenen Typen aus `pc` und `git`.
//! Ausnahme: [`BauFortschritt`] geht auch als Ereignis `bau-fortschritt` hinaus.
//!
//! Der Wert beim Tresor-Schreiben steckt in `Wert`: er wird nach Gebrauch
//! überschrieben und erscheint weder in `Debug` noch in einer Antwort.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use ts_rs::TS;
use zeroize::Zeroizing;

use crate::git::RepoStatus;
use crate::pc::inventory::Inventory;
use crate::pc::sysapps::{AutostartEntry, Tool, WingetEntry};

/// Höchstlänge einer Zeile (Anfrage oder Antwort) in Bytes, ohne `\n`.
pub const MAX_ZEILE: usize = 1024 * 1024;

/// Ein Geheimnis auf dem Weg zum Tresor. Wird beim Wegwerfen überschrieben;
/// `Debug` zeigt nur, dass es da ist.
#[derive(Clone, PartialEq, Eq)]
pub struct Wert(pub Zeroizing<String>);

impl Wert {
    pub fn neu(s: &str) -> Wert {
        Wert(Zeroizing::new(s.to_string()))
    }

    pub fn als_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Wert {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Wert(verborgen)")
    }
}

impl Serialize for Wert {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Wert {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Wert, D::Error> {
        Ok(Wert(Zeroizing::new(String::deserialize(d)?)))
    }
}

/// Was der Dienst vom Boten will.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "art", content = "daten", rename_all = "snake_case", deny_unknown_fields)]
pub enum Anfrage {
    /// Lebenszeichen; die Antwort nennt Fassung, Konto und Startzeit.
    Hallo,
    /// Gibt es den Eintrag `ziel` (`ranke/<name>`) im Tresor? Nie der Wert.
    TresorInfo { ziel: String },
    /// Wert nach `ziel` schreiben.
    TresorSchreiben { ziel: String, benutzer: String, wert: Wert },
    /// `winget list`.
    Winget,
    /// Autostart (Registry und Startordner).
    Autostart,
    /// Ordnergrößen; ohne Wurzel das Profil des Boten.
    Inventar { wurzel: Option<String> },
    /// Versionen der Werkzeuge aus dem PATH des Boten.
    Werkzeuge,
    /// Git-Stand eines Repos.
    GitStatus { pfad: String, privat: bool },
    /// Fingerabdrücke von Repos (Etappe 2d): je Repo eine Zahl aus Zeitstempeln
    /// und Größen (oben im Repo, HEAD, index, Zweige; mit `tief` auch Dateien
    /// darunter). Der Dienst sieht so Änderungen in Repos, die er selbst nicht
    /// lesen darf. Nur Repos, die eine Projektseite nennt. Ein älterer Bote
    /// kennt die Anfrage nicht und antwortet mit `Fehler`; dann schaut der
    /// Dienst selbst.
    RepoStempel { pfade: Vec<String>, tief: bool },
    /// Die Oberfläche bauen (Etappe 4v): Bauliste schreiben, `npm run build`
    /// in einen neuen Ordner unter `web\builds\`, dann den Zeiger
    /// `web\builds\aktuell` umstellen. Ohne Argumente: Repo und Pfade kennt
    /// der Bote aus seinen Einstellungen.
    Bau,
    /// Eine Datei oder einen Ordner in der Sitzung des Inhabers öffnen
    /// (Entscheidung lokal-oeffnen). Der Dienst hat den Pfad schon
    /// geprüft; der Bote prüft Pfad, Wurzel, Tabu und Endung noch einmal selbst
    /// (`bote::oeffnen`) und antwortet nur `Geoeffnet` oder mit einem Satz.
    Oeffnen { pfad: String },
    /// Bau-Tempo, Zweig C: Wie weit ist der laufende Bau? Kommt über eine
    /// zweite Verbindung neben `Bau`. Ein älterer Bote kennt die Anfrage nicht
    /// und antwortet mit `Fehler`; für den Dienst heißt das „kein Fortschritt“.
    Fortschritt,
}

impl Anfrage {
    /// Kurzer Name für Protokoll und Meldungen (ohne Inhalt).
    pub fn art(&self) -> &'static str {
        match self {
            Anfrage::Hallo => "hallo",
            Anfrage::TresorInfo { .. } => "tresor_info",
            Anfrage::TresorSchreiben { .. } => "tresor_schreiben",
            Anfrage::Winget => "winget",
            Anfrage::Autostart => "autostart",
            Anfrage::Inventar { .. } => "inventar",
            Anfrage::Werkzeuge => "werkzeuge",
            Anfrage::GitStatus { .. } => "git_status",
            Anfrage::RepoStempel { .. } => "repo_stempel",
            Anfrage::Bau => "bau",
            Anfrage::Oeffnen { .. } => "oeffnen",
            Anfrage::Fortschritt => "fortschritt",
        }
    }
}

/// Bau-Tempo, Zweig C: der Schritt des laufenden Baus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "BauFortschrittSchritt")]
pub enum FortschrittSchritt {
    /// Prüfung je Projekt in der Warteschlange (`fertig` von `gesamt`).
    Pruefung,
    /// Sammelprüfung: alle `gesamt` Projekte in einem Lauf.
    Sammelpruefung,
    /// Gesamtprüfung neben `vite build`.
    Gesamtpruefung,
    /// `vite build` noch einmal nach einem Ausschluss.
    ViteBuild,
}

/// Bau-Tempo, Zweig C: Stand des laufenden Baus, nur Zahlen (kein Slug, kein
/// Satz). Als Ereignis `bau-fortschritt` nur im geschützten Kanal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(rename = "BauFortschrittEvent")]
pub struct BauFortschritt {
    pub schritt: FortschrittSchritt,
    pub fertig: u32,
    pub gesamt: u32,
}

/// Fingerabdruck eines Repos (`Anfrage::RepoStempel`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoFingerabdruck {
    pub pfad: String,
    pub stempel: u64,
}

/// Höchstens so viele Repos je Anfrage `RepoStempel`.
pub const STEMPEL_HOECHSTENS: usize = 64;

/// Ein Projekt, das bei diesem Bau rot wurde (Etappe 4b): Es bleibt in seiner
/// letzten grünen Fassung (oder fehlt, wenn es keine gibt).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RotesProjekt {
    pub slug: String,
    /// Ein Satz in einfachem Deutsch, mit dem ersten Fund.
    pub satz: String,
}

/// Wie lange ein Schritt des Baus gedauert hat (Etappe 4b, für die Messung).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BauSchritt {
    pub name: String,
    pub ms: u64,
}

/// Ergebnis eines Baus der Oberfläche (`Anfrage::Bau`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BauErgebnis {
    /// Grün: `npm run build` lief durch, `index.html` liegt im neuen Ordner,
    /// und `web\builds\aktuell` zeigt jetzt darauf.
    pub gruen: bool,
    /// Dauer des ganzen Baus in Millisekunden.
    pub dauer_ms: u64,
    /// Der neue Ordner unter `web\builds\` (nur der Name), wenn grün.
    pub ordner: Option<String>,
    /// Projekte, die der Scanner rot gefunden hat; sie fehlen in diesem Bau.
    pub rote: Vec<String>,
    /// Ein Satz für den Inhaber und den Chat.
    pub satz: String,
    /// Bei rot: die letzten Zeilen der Ausgabe von npm (höchstens einige KB).
    pub ausgabe_ende: Option<String>,
    /// Etappe 4b: Projekte, die bei diesem Bau rot wurden (je mit Satz).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub neu_rot: Vec<RotesProjekt>,
    /// Etappe 4b: Projekte, die aus ihrer letzten grünen Kopie gebaut sind.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub alte_fassung: Vec<String>,
    /// Etappe 4b: Dauer der Schritte (Prüfung, Gesamtprüfung, vite build …).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub schritte: Vec<BauSchritt>,
    /// Etappe 4b: Der Bote war nicht erreichbar (dann gilt „Bote fehlt“,
    /// keine eigene Bau-Meldung). Nur im Dienst gesetzt, nie vom Boten.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub bote_fehlt: bool,
}

/// Zustand einer Windows-Sitzung (aus `WTS_CONNECTSTATE_CLASS`), so weit er
/// für „Inhaber angemeldet“ zählt. Unbekannte Wörter (eine neuere Gegenseite)
/// werden zu `Anderer`, statt die ganze Antwort unlesbar zu machen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SitzungsZustand {
    /// `WTSActive`: angemeldet, an der Konsole oder fern (auch gesperrt).
    Aktiv,
    /// `WTSDisconnected`: angemeldet, aber getrennt (Benutzer gewechselt).
    Getrennt,
    /// Alles andere (Anmeldebildschirm, im Aufbau, beendet …).
    #[serde(other)]
    Anderer,
}

impl SitzungsZustand {
    /// Zählt als „angemeldet“: aktiv oder getrennt.
    pub fn angemeldet(self) -> bool {
        matches!(self, Self::Aktiv | Self::Getrennt)
    }
}

/// Die Sitzung, in der der Bote läuft (Hotfix Sitzungsabfrage): nur Nummer
/// und Zustand, weder SID noch Name. Der Bote fragt sie von innen ab
/// (`pc::sitzung::eigene_sitzung`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoteSitzung {
    /// Nummer der Sitzung (`ProcessIdToSessionId` des Boten).
    pub id: u32,
    /// Ist es die Sitzung an der Konsole (`WTSGetActiveConsoleSessionId`)?
    pub konsole: bool,
    /// Zustand laut `WTSConnectState`; `null`, wenn der Bote ihn nicht lesen konnte.
    pub zustand: Option<SitzungsZustand>,
}

/// Was der Bote zurückgibt. Ohne `deny_unknown_fields`: Ein älterer Dienst
/// liest auch die Antwort eines neueren Boten (Felder, die er nicht kennt,
/// fallen weg).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "art", content = "daten", rename_all = "snake_case")]
pub enum Antwort {
    /// `sitzung` gibt es seit dem Hotfix Sitzungsabfrage; ein älterer Bote
    /// lässt sie weg, dann ist sie `None` (unbekannt).
    Hallo {
        version: String,
        konto_sid: String,
        gestartet: String,
        #[serde(default)]
        sitzung: Option<BoteSitzung>,
        /// Kleinkram 2: die Fassung des Prüfers im Boten
        /// (`werkzeug::pruefer_fassung`). Ein älterer Bote lässt sie weg (`None`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pruefer: Option<String>,
    },
    TresorInfo {
        vorhanden: bool,
        gespeichert_am: Option<String>,
        benutzer: Option<String>,
    },
    /// Geschrieben.
    Ok,
    Winget(Vec<WingetEntry>),
    Autostart(Vec<AutostartEntry>),
    Inventar(Inventory),
    Werkzeuge(Vec<Tool>),
    GitStatus(RepoStatus),
    /// Fingerabdrücke in der Reihenfolge der Anfrage; ein Repo, das keine
    /// Projektseite nennt, fehlt.
    RepoStempel(Vec<RepoFingerabdruck>),
    Bau(BauErgebnis),
    /// Geöffnet (nie mit Inhalt der Datei).
    Geoeffnet,
    /// Stand des laufenden Baus; `None`: es läuft keiner.
    Fortschritt(Option<BauFortschritt>),
    /// Ging nicht; ein Satz für den Inhaber.
    Fehler {
        satz: String,
    },
}

impl Antwort {
    pub fn art(&self) -> &'static str {
        match self {
            Antwort::Hallo { .. } => "hallo",
            Antwort::TresorInfo { .. } => "tresor_info",
            Antwort::Ok => "ok",
            Antwort::Winget(_) => "winget",
            Antwort::Autostart(_) => "autostart",
            Antwort::Inventar(_) => "inventar",
            Antwort::Werkzeuge(_) => "werkzeuge",
            Antwort::GitStatus(_) => "git_status",
            Antwort::RepoStempel(_) => "repo_stempel",
            Antwort::Bau(_) => "bau",
            Antwort::Geoeffnet => "geoeffnet",
            Antwort::Fortschritt(_) => "fortschritt",
            Antwort::Fehler { .. } => "fehler",
        }
    }

    pub fn fehler(satz: impl Into<String>) -> Antwort {
        Antwort::Fehler { satz: satz.into() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anfragen_hin_und_zurueck() {
        let alle = [
            Anfrage::Hallo,
            Anfrage::TresorInfo { ziel: "ranke/test-x".into() },
            Anfrage::TresorSchreiben { ziel: "ranke/test-x".into(), benutzer: "b".into(), wert: Wert::neu("geheim-123") },
            Anfrage::Winget,
            Anfrage::Autostart,
            Anfrage::Inventar { wurzel: None },
            Anfrage::Werkzeuge,
            Anfrage::GitStatus { pfad: "C:/x".into(), privat: true },
            Anfrage::RepoStempel { pfade: vec!["C:/x".into()], tief: true },
            Anfrage::Bau,
            Anfrage::Oeffnen { pfad: "C:/Users/beispiel/x.pdf".into() },
            Anfrage::Fortschritt,
        ];
        for a in alle {
            let zeile = serde_json::to_string(&a).unwrap();
            assert!(!zeile.contains('\n'), "{zeile}");
            let zurueck: Anfrage = serde_json::from_str(&zeile).unwrap();
            assert_eq!(zurueck, a);
        }
        assert_eq!(serde_json::to_string(&Anfrage::Hallo).unwrap(), r#"{"art":"hallo"}"#);
        assert_eq!(serde_json::to_string(&Anfrage::TresorInfo { ziel: "ranke/a".into() }).unwrap(), r#"{"art":"tresor_info","daten":{"ziel":"ranke/a"}}"#);
        assert!(serde_json::from_str::<Anfrage>(r#"{"art":"loeschen"}"#).is_err());
        assert_eq!(serde_json::to_string(&Anfrage::Bau).unwrap(), r#"{"art":"bau"}"#);
        // Der Bau nimmt keine Pfade vom Dienst an.
        assert!(serde_json::from_str::<Anfrage>(r#"{"art":"bau","daten":{"repo":"C:/x"}}"#).is_err());
        // Öffnen nimmt nur den Pfad, keine Argumente oder Schalter.
        assert!(serde_json::from_str::<Anfrage>(r#"{"art":"oeffnen","daten":{"pfad":"C:/x","argumente":"/e"}}"#).is_err());
    }

    #[test]
    fn antworten_hin_und_zurueck() {
        let alle = [
            Antwort::Hallo {
                version: "0.1.0".into(),
                konto_sid: "S-1-5-21-1".into(),
                gestartet: "2026-10-26T10:00:00+01:00".into(),
                sitzung: Some(BoteSitzung { id: 2, konsole: true, zustand: Some(SitzungsZustand::Getrennt) }),
                pruefer: Some("0123456789abcdef".into()),
            },
            Antwort::TresorInfo { vorhanden: false, gespeichert_am: None, benutzer: None },
            Antwort::Ok,
            Antwort::Winget(vec![WingetEntry { name: "Git".into(), id: "Git.Git".into(), version: "2".into(), available: String::new() }]),
            Antwort::Autostart(vec![AutostartEntry { name: "a".into(), command: "b".into(), source: "Benutzer".into() }]),
            Antwort::Inventar(Inventory::default()),
            Antwort::Werkzeuge(vec![Tool { name: "Git".into(), version: None }]),
            Antwort::RepoStempel(vec![RepoFingerabdruck { pfad: "C:/x".into(), stempel: 42 }]),
            Antwort::Bau(BauErgebnis {
                gruen: false,
                dauer_ms: 1200,
                ordner: None,
                rote: vec!["x".into()],
                satz: "Bau rot.".into(),
                ausgabe_ende: Some("error".into()),
                neu_rot: vec![RotesProjekt { slug: "x".into(), satz: "Rot.".into() }],
                alte_fassung: vec!["x".into()],
                schritte: vec![BauSchritt { name: "vite build".into(), ms: 900 }],
                bote_fehlt: false,
            }),
            Antwort::Geoeffnet,
            Antwort::Fortschritt(None),
            Antwort::Fortschritt(Some(BauFortschritt { schritt: FortschrittSchritt::Pruefung, fertig: 7, gesamt: 21 })),
            Antwort::Fehler { satz: "Ging nicht.".into() },
        ];
        for a in alle {
            let zeile = serde_json::to_string(&a).unwrap();
            let zurueck: Antwort = serde_json::from_str(&zeile).unwrap();
            assert_eq!(zurueck.art(), a.art());
            assert_eq!(serde_json::to_string(&zurueck).unwrap(), zeile);
        }
    }

    /// Hotfix Sitzungsabfrage: Ein älterer Bote ohne `sitzung` geht weiter
    /// (dann unbekannt), ein unbekannter Zustand macht die Antwort nicht
    /// unlesbar, und Felder, die der Dienst nicht kennt, fallen weg.
    #[test]
    fn hallo_mit_und_ohne_sitzung() {
        let alt = r#"{"art":"hallo","daten":{"version":"0.1.0","konto_sid":"S-1-5-21-1","gestartet":"2026-10-26T10:00:00+01:00"}}"#;
        match serde_json::from_str::<Antwort>(alt).unwrap() {
            Antwort::Hallo { sitzung, version, pruefer, .. } => assert_eq!((sitzung, version.as_str(), pruefer), (None, "0.1.0", None)),
            a => panic!("{a:?}"),
        }
        let neu = r#"{"art":"hallo","daten":{"version":"0.2.0","konto_sid":"S-1-5-21-1","gestartet":"x","sitzung":{"id":3,"konsole":false,"zustand":"aktiv"},"zukunft":1}}"#;
        match serde_json::from_str::<Antwort>(neu).unwrap() {
            Antwort::Hallo { sitzung, .. } => {
                assert_eq!(sitzung, Some(BoteSitzung { id: 3, konsole: false, zustand: Some(SitzungsZustand::Aktiv) }))
            }
            a => panic!("{a:?}"),
        }
        let fremd = r#"{"art":"hallo","daten":{"version":"0.3.0","konto_sid":"S","gestartet":"x","sitzung":{"id":4,"konsole":true,"zustand":"im_aufbau"}}}"#;
        match serde_json::from_str::<Antwort>(fremd).unwrap() {
            Antwort::Hallo { sitzung: Some(s), .. } => assert_eq!(s.zustand, Some(SitzungsZustand::Anderer)),
            a => panic!("{a:?}"),
        }
        assert!(SitzungsZustand::Aktiv.angemeldet() && SitzungsZustand::Getrennt.angemeldet());
        assert!(!SitzungsZustand::Anderer.angemeldet());
    }

    #[test]
    fn wert_nie_in_debug() {
        let a = Anfrage::TresorSchreiben { ziel: "ranke/test-x".into(), benutzer: "b".into(), wert: Wert::neu("geheim-123") };
        let text = format!("{a:?} {a:#?}");
        assert!(!text.contains("geheim-123"), "{text}");
        assert!(text.contains("verborgen"));
        assert_eq!(a.art(), "tresor_schreiben");
    }
}
