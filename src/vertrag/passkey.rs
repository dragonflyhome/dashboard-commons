//! Passkey für andere Konten (Etappe 7c): was in der Tabelle `passkeys` steht
//! und was die Wege `/api/passkey/*` und `/api/schluessel/passkeys*`
//! schicken und annehmen. Alle binären Werte (Credential-ID, Challenge,
//! Schlüssel, Antworten des Authenticators) sind Base64url ohne Füllzeichen.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Stand eines Passkeys. Daten werden nie gelöscht: Eine
/// Ablehnung oder ein Ablauf bleibt als Stand stehen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum PasskeyStatus {
    /// Beantragt aus einem anderen Konto; berechtigt zu nichts.
    Ausstehend,
    /// Freigegeben (oder im Hauptkonto angelegt): Anmeldung damit geht.
    Registriert,
    /// Widerrufen; der öffentliche Schlüssel bleibt zur Nachvollziehbarkeit.
    Widerrufen,
    /// Abgelehnt; dasselbe Konto darf frühestens nach einer Stunde neu beantragen.
    Abgelehnt,
    /// Der Antrag lag 24 Stunden, ohne dass jemand entschieden hat.
    Abgelaufen,
}

/// Ein Passkey, wie er in der Tabelle `passkeys` steht (nur öffentliche
/// Daten; als einzige Passkey-Tabelle auch im Export).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Passkey {
    /// Credential-ID.
    pub id: String,
    pub status: PasskeyStatus,
    /// Name des Geräts, selbst gewählt (etwa „Handy“ oder „Laptop Windows Hello“).
    pub name: String,
    /// Windows-Konto, aus dem der Antrag kam (`PC\Name`), aus der Windows-Anmeldung.
    pub konto: String,
    /// SID dieses Kontos.
    pub konto_sid: String,
    /// Im Hauptkonto angelegt (sofort registriert, ohne Antrag).
    pub hauptkonto: bool,
    /// COSE-Verfahren: -7 (ES256) oder -257 (RS256).
    pub verfahren: i64,
    /// Öffentlicher Schlüssel (COSE).
    pub schluessel: String,
    /// Benutzerkennung (user.id), die der Authenticator beim Anmelden zurückgibt.
    pub benutzer: String,
    /// Zähler des Authenticators bei der letzten Anmeldung.
    pub zaehler: u32,
    /// Ortszeit RFC 3339.
    pub angelegt: String,
    pub zuletzt_benutzt: Option<String>,
    /// Vierstellige Vergleichszahl des Antrags (bleibt nach der Entscheidung stehen).
    pub vergleichszahl: Option<String>,
    /// Wann der Antrag verfällt (nur bei Anträgen).
    pub ablauf: Option<String>,
    /// Wann freigegeben, abgelehnt, widerrufen oder abgelaufen.
    pub entschieden: Option<String>,
    /// Wer entschieden hat: `hauptkonto`, `passkey:<Name>` oder `server` (Ablauf).
    pub entschieden_von: Option<String>,
    /// Der Vorgang `passkey-antrag` dazu.
    pub vorgang: Option<String>,
}

/// Eine Zeile der Liste auf /schluessel (ohne Schlüssel und SID).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct PasskeyZeile {
    pub id: String,
    pub status: PasskeyStatus,
    pub name: String,
    pub konto: String,
    pub hauptkonto: bool,
    /// `ES256` oder `RS256`.
    pub verfahren: String,
    pub angelegt: String,
    pub zuletzt_benutzt: Option<String>,
    /// Nur bei ausstehenden Anträgen.
    pub vergleichszahl: Option<String>,
    pub ablauf: Option<String>,
    pub entschieden: Option<String>,
    pub entschieden_von: Option<String>,
}

impl From<&Passkey> for PasskeyZeile {
    fn from(p: &Passkey) -> Self {
        let ausstehend = p.status == PasskeyStatus::Ausstehend;
        Self {
            id: p.id.clone(),
            status: p.status,
            name: p.name.clone(),
            konto: p.konto.clone(),
            hauptkonto: p.hauptkonto,
            verfahren: if p.verfahren == -257 { "RS256" } else { "ES256" }.to_string(),
            angelegt: p.angelegt.clone(),
            zuletzt_benutzt: p.zuletzt_benutzt.clone(),
            vergleichszahl: if ausstehend { p.vergleichszahl.clone() } else { None },
            ablauf: if ausstehend { p.ablauf.clone() } else { None },
            entschieden: p.entschieden.clone(),
            entschieden_von: p.entschieden_von.clone(),
        }
    }
}

/// Wie die Anfrage als Inhaber gilt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum AnmeldeArt {
    /// Windows-Anmeldung mit dem Konto des Inhabers (7b).
    Hauptkonto,
    /// Ausweis nach Anmeldung mit Passkey.
    Passkey,
    /// Gar nicht.
    Keine,
}

/// `GET /api/schluessel/passkeys`: alle Passkeys, neueste zuerst.
#[derive(Debug, Clone, Serialize, TS)]
pub struct PasskeyListe {
    pub passkeys: Vec<PasskeyZeile>,
    /// Offene Anträge.
    pub offen: u32,
    /// Wie diese Anfrage als Inhaber gilt.
    pub wie: AnmeldeArt,
    /// Mit Passkey: bis wann Freigeben, Ablehnen und Widerrufen ohne neue
    /// Bestätigung gehen (5 Minuten nach der Anmeldung). Im Hauptkonto `null`.
    pub frisch_bis: Option<String>,
    /// Mit Passkey: die eigene Credential-ID (nicht sich selbst freigeben).
    pub eigener: Option<String>,
}

/// `GET /api/schluessel/passkeys/anzahl`: für den Hinweis in der Kopfzeile.
#[derive(Debug, Clone, Serialize, TS)]
pub struct PasskeyAnzahl {
    pub offen: u32,
}

/// `GET /api/passkey/stand`: ist diese Sitzung angemeldet, und wie?
#[derive(Debug, Clone, Serialize, TS)]
pub struct PasskeyStand {
    pub wie: AnmeldeArt,
    /// Mit Passkey: Name des Passkeys.
    pub passkey: Option<String>,
    /// Mit Passkey: bis wann die Sitzung höchstens dauert (8 Stunden).
    pub sitzung_bis: Option<String>,
    /// Mit Passkey: bis wann sie als frische Bestätigung zählt.
    pub frisch_bis: Option<String>,
    /// Ob es auf dem Server einen Schlüssel für Ausweise gibt (sonst geht
    /// die Anmeldung mit Passkey nicht).
    pub verfuegbar: bool,
}

/// `POST /api/passkey/anmelden/beginn`: Angaben für `navigator.credentials.get`.
#[derive(Debug, Clone, Serialize, TS)]
pub struct AnmeldeOptionen {
    pub challenge: String,
    pub rp_id: String,
    pub zeitlimit_ms: u32,
}

/// `POST /api/passkey/anmelden/ende`: die Antwort des Authenticators.
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct AnmeldenEnde {
    /// Credential-ID (rawId).
    pub id: String,
    pub client_daten: String,
    pub auth_daten: String,
    pub signatur: String,
    /// userHandle, wenn der Authenticator eins schickt.
    #[serde(default)]
    #[ts(optional)]
    pub benutzer: Option<String>,
}

/// `POST /api/passkey/antrag/beginn`: Name des Geräts.
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct AntragBeginn {
    pub name: String,
}

/// Antwort auf `POST /api/passkey/antrag/beginn`: Angaben für `navigator.credentials.create`.
#[derive(Debug, Clone, Serialize, TS)]
pub struct AntragOptionen {
    pub challenge: String,
    pub rp_id: String,
    pub rp_name: String,
    /// user.id (zufällig je Passkey, nie die SID).
    pub benutzer_id: String,
    /// user.name: das Windows-Konto.
    pub benutzer_name: String,
    /// user.displayName: „Dashboard (Gerätename)“.
    pub anzeige_name: String,
    /// COSE-Verfahren in dieser Reihenfolge: ES256, RS256.
    pub verfahren: Vec<i64>,
    /// Passkeys dieses Kontos, die es schon gibt (nicht doppelt anlegen).
    pub ausschliessen: Vec<String>,
    pub zeitlimit_ms: u32,
    /// Im Hauptkonto: sofort registriert, ohne Antrag.
    pub hauptkonto: bool,
}

/// `POST /api/passkey/antrag/ende`: die Antwort des Authenticators.
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct AntragEnde {
    pub id: String,
    pub client_daten: String,
    pub attestation: String,
}

/// Antwort auf `POST /api/passkey/antrag/ende`.
#[derive(Debug, Clone, Serialize, TS)]
pub struct AntragErgebnis {
    pub status: PasskeyStatus,
    /// Bei einem Antrag: die Zahl, die auch im Hauptkonto steht.
    pub vergleichszahl: Option<String>,
    pub ablauf: Option<String>,
}

/// `GET /api/passkey/antrag`: die Passkeys des eigenen Windows-Kontos.
#[derive(Debug, Clone, Serialize, TS)]
pub struct EigeneAntraege {
    pub konto: String,
    pub hauptkonto: bool,
    pub passkeys: Vec<PasskeyZeile>,
    /// Der Anmeldestand dieser Anfrage in einem Wort, wie ihn auch der freie
    /// Kanal nennt (Kopfleiste, Fassung 2). Die Seite fragt hier nach, wenn
    /// ein fremdes Windows-Konto erkannt ist; der freie Kanal sieht die
    /// Windows-Anmeldung nur auf einer schon angemeldeten Verbindung.
    pub anmeldung: Anmeldewort,
}

/// Der Anmeldestand in einem Wort (Kopfleiste, Fassung 2,
/// `[fassung_2_live_zustaende]`). Bestimmt der Server, nie der Browser.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum Anmeldewort {
    /// Als Inhaber: Windows-Anmeldung mit seinem Konto oder gültiger Passkey.
    Angemeldet,
    /// Nicht angemeldet, aber eine Anmeldung ist möglich (das Konto des Inhabers in
    /// einem Browser ohne Windows-Anmeldung, oder ein fremdes Konto mit
    /// registriertem Passkey).
    NichtAngemeldet,
    /// Ein fremdes Windows-Konto ohne registrierten Passkey.
    NichtRegistriert,
}

/// Ereignis `anmeldung` auf dem freien Kanal (`/api/events/frei`): der
/// Anmeldestand des Anfragenden, einmal gleich nach dem Verbinden.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "AnmeldungEvent")]
pub struct AnmeldungEreignis {
    pub stand: Anmeldewort,
}

/// `POST /api/schluessel/passkeys/<id>/freigeben`: die Zahl, die der Inhaber
/// verglichen hat (muss zum Antrag passen).
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Freigabe {
    pub vergleichszahl: String,
}
