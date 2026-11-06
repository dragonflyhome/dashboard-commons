//! Passkeys (Etappe 7c, Schema 8): Tabellen `passkeys`, `passkey_aufgaben`,
//! `passkey_antraege` und `jwt_sperrliste`.
//!
//! - Eine **Aufgabe** ist eine Challenge für WebAuthn (Anlegen oder Anmelden),
//!   einmal gültig und 5 Minuten lang. Beim Benutzen wird sie entfernt, auch
//!   wenn der Schritt danach scheitert; abgelaufene räumt jeder neue Eintrag
//!   mit weg. Höchstens [`AUFGABEN_HOECHSTENS`] offene auf einmal.
//! - Ein **Antrag** entsteht, wenn ein anderes Windows-Konto einen Passkey
//!   anlegt: Der Passkey steht dann mit Stand `ausstehend` in `passkeys` und
//!   berechtigt zu nichts, der Antrag (Konto, Vergleichszahl, Ablauf) in
//!   `passkey_antraege`, dazu ein Vorgang `passkey-antrag`. Grenzen: höchstens
//!   5 offene insgesamt, 1 je Konto, Ablauf nach 24 Stunden, nach einer
//!   Ablehnung erst nach einer Stunde neu.
//! - **Entscheiden** dürfen nur das Hauptkonto oder ein registrierter Passkey,
//!   dessen Anmeldung höchstens 5 Minuten alt ist; nie ein Passkey über sich
//!   selbst. Freigeben setzt `registriert`, Ablehnen `abgelehnt`, Widerrufen
//!   `widerrufen`. Daten werden nie gelöscht: Abgelehnte und
//!   abgelaufene Anträge bleiben mit ihrem Stand in `passkeys` (der Bauplan
//!   sah „Ablehnung löscht den Eintrag“ vor; das widerspräche dieser Regel).
//!   Der Antrag selbst verlässt `passkey_antraege`, sein Ergebnis steht in
//!   `passkeys`, der Vorgang wird mit Notiz erledigt.
//! - Die **Sperrliste** hält beendete Sitzungen (`jti`) und den Zeitpunkt von
//!   „alle Sitzungen beenden“ (`alle`), bis die Ausweise ohnehin abgelaufen sind.
//!
//! Alle Methoden bekommen „jetzt“ als Eingabe, damit Tests die Uhr stellen.

use chrono::{DateTime, Duration, SecondsFormat};
use redb::{ReadableDatabase, ReadableTable, WriteTransaction};
use serde::{Deserialize, Serialize};
use tracing::warn;

use super::aufgaben::Zeit;
use super::tabellen::{self, JWT_SPERRLISTE, PASSKEY_ANTRAEGE, PASSKEY_AUFGABEN, PASSKEYS};
use super::vorgaenge::{json_lesen, vorgang_anlegen_in, vorgang_weiter_in};
use super::{Speicher, SpeicherFehler};
use crate::api::vertrag::{Ereignis, VorgangsArt, Zustand};
use crate::passkey::webauthn::{self, Erwartung, NeuerPasskey};
use crate::passkey::{
    self, ANTRAEGE_HOECHSTENS, ANTRAG_STUNDEN, Abweisung, CHALLENGE_SEKUNDEN, FRISCH_SEKUNDEN, NACH_ABLEHNUNG_MINUTEN, SITZUNG_HOECHSTENS_SEKUNDEN,
};
use crate::vertrag::passkey::{Passkey, PasskeyStatus};

/// Höchstens so viele offene Challenges (Schutz gegen Flut, `/anmelden` ist frei).
pub const AUFGABEN_HOECHSTENS: usize = 200;
/// Slug der Vorgänge `passkey-antrag` (kein Chat wartet darauf).
pub const SLUG: &str = "passkey";
/// Schlüssel in `jwt_sperrliste` für „alle Sitzungen beenden“.
pub const ALLE: &str = "alle";

fn zeit_text(z: &Zeit) -> String {
    z.to_rfc3339_opts(SecondsFormat::Secs, false)
}

fn zeit_lesen(text: &str) -> Option<Zeit> {
    DateTime::parse_from_rfc3339(text).ok()
}

/// Wofür eine Challenge gilt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AufgabenArt {
    Anlegen,
    Anmelden,
}

/// Eintrag in `passkey_aufgaben` (Schlüssel: die Challenge, Base64url).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PasskeyAufgabe {
    pub art: AufgabenArt,
    pub angelegt: String,
    pub ablauf: String,
    /// Beim Anlegen: das Windows-Konto aus der Anmeldung (Negotiate).
    pub konto: Option<String>,
    pub konto_sid: Option<String>,
    /// Beim Anlegen: im Hauptkonto (dann sofort registriert).
    pub hauptkonto: bool,
    /// Beim Anlegen: die zufällige Benutzerkennung (user.id).
    pub benutzer: Option<String>,
    /// Beim Anlegen: Name des Geräts.
    pub name: Option<String>,
}

impl PasskeyAufgabe {
    /// Eine Challenge zum Anmelden.
    pub fn anmelden(jetzt: &Zeit) -> Self {
        Self {
            art: AufgabenArt::Anmelden,
            angelegt: zeit_text(jetzt),
            ablauf: zeit_text(&(*jetzt + Duration::seconds(CHALLENGE_SEKUNDEN))),
            konto: None,
            konto_sid: None,
            hauptkonto: false,
            benutzer: None,
            name: None,
        }
    }

    /// Eine Challenge zum Anlegen für das Konto `konto`.
    pub fn anlegen(konto: &crate::anmeldung::WindowsKonto, name: &str, jetzt: &Zeit) -> Self {
        Self {
            art: AufgabenArt::Anlegen,
            angelegt: zeit_text(jetzt),
            ablauf: zeit_text(&(*jetzt + Duration::seconds(CHALLENGE_SEKUNDEN))),
            konto: Some(konto.name.clone()),
            konto_sid: Some(konto.sid.clone()),
            hauptkonto: konto.inhaber,
            benutzer: Some(passkey::b64(&passkey::zufall(16))),
            name: Some(name.to_string()),
        }
    }
}

/// Eintrag in `passkey_antraege` (Schlüssel: Credential-ID).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PasskeyAntrag {
    pub id: String,
    pub konto: String,
    pub konto_sid: String,
    pub vergleichszahl: String,
    pub angelegt: String,
    pub ablauf: String,
    pub vorgang: Option<String>,
}

/// Eintrag in `jwt_sperrliste`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sperre {
    /// Bis wann der Eintrag gebraucht wird (danach ist jeder betroffene Ausweis abgelaufen).
    pub bis: String,
    /// Wann gesperrt; bei `alle`: Ausweise mit Anmeldung bis zu diesem Zeitpunkt gelten nicht mehr.
    pub gesperrt: String,
}

/// Wer entscheidet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Freigeber {
    /// Das Hauptkonto (Windows-Anmeldung mit der SID des Inhabers).
    Hauptkonto,
    /// Ein Passkey-Ausweis: Kennung und Name des Passkeys, Zeit der Anmeldung (Sekunden seit 1970).
    Passkey { cid: String, name: String, anmeldung: i64 },
}

impl Freigeber {
    fn wort(&self) -> String {
        match self {
            Self::Hauptkonto => "hauptkonto".to_string(),
            Self::Passkey { name, .. } => format!("passkey:{name}"),
        }
    }

    fn satz(&self) -> String {
        match self {
            Self::Hauptkonto => "im Hauptkonto".to_string(),
            Self::Passkey { name, .. } => format!("mit dem Passkey „{name}“"),
        }
    }
}

/// Was entschieden wird.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entscheidung {
    Freigeben { vergleichszahl: String },
    Ablehnen,
    Widerrufen,
}

/// Fehler der Passkey-Schritte.
#[derive(Debug, thiserror::Error)]
pub enum PasskeyFehler {
    /// WebAuthn-Prüfung oder Challenge abgewiesen.
    #[error("{0}")]
    Abgewiesen(Abweisung),
    /// Eine Grenze für Anträge (Satz für den Inhaber).
    #[error("{0}")]
    Grenze(String),
    #[error("Diesen Passkey gibt es nicht.")]
    NichtGefunden,
    /// Passkey-Anmeldung älter als 5 Minuten.
    #[error("Bitte zuerst mit dem Passkey bestätigen (höchstens 5 Minuten alt).")]
    NichtFrisch,
    /// Ein Passkey entscheidet über sich selbst.
    #[error("Ein Passkey kann nicht über sich selbst entscheiden.")]
    Selbst,
    #[error("Die Vergleichszahl passt nicht zu diesem Antrag.")]
    Vergleichszahl,
    /// Der Stand passt nicht (Satz für den Inhaber).
    #[error("{0}")]
    Zustand(String),
    #[error("{0}")]
    Speicher(#[from] SpeicherFehler),
}

impl From<Abweisung> for PasskeyFehler {
    fn from(a: Abweisung) -> Self {
        Self::Abgewiesen(a)
    }
}

macro_rules! ueber_speicher {
    ($($typ:ty),*) => {
        $(impl From<$typ> for PasskeyFehler {
            fn from(e: $typ) -> Self {
                Self::Speicher(e.into())
            }
        })*
    };
}
ueber_speicher!(redb::StorageError, redb::TransactionError, redb::TableError, redb::CommitError, serde_json::Error);

/// Alle Einträge einer Tabelle mit JSON-Werten; unlesbare stehen im Protokoll.
fn alle_aus<T: serde::de::DeserializeOwned, Tab: ReadableTable<&'static str, &'static [u8]>>(
    tabelle: &Tab,
    name: &str,
) -> Result<Vec<(String, T)>, SpeicherFehler> {
    let mut out = Vec::new();
    for eintrag in tabelle.iter()? {
        let (k, w) = eintrag?;
        match json_lesen::<T>(w.value(), &format!("{name}/{}", k.value())) {
            Ok(t) => out.push((k.value().to_string(), t)),
            Err(e) => warn!("{name}: {} ({})", e, e.einzelheiten()),
        }
    }
    Ok(out)
}

fn passkey_aus<Tab: ReadableTable<&'static str, &'static [u8]>>(tabelle: &Tab, id: &str) -> Result<Option<Passkey>, SpeicherFehler> {
    match tabelle.get(id)? {
        Some(w) => Ok(Some(json_lesen(w.value(), &format!("passkeys/{id}"))?)),
        None => Ok(None),
    }
}

fn passkey_schreiben(txn: &WriteTransaction, p: &Passkey) -> Result<(), SpeicherFehler> {
    txn.open_table(PASSKEYS)?.insert(p.id.as_str(), serde_json::to_vec(p)?.as_slice())?;
    Ok(())
}

/// Abgelaufene Anträge: Passkey auf `abgelaufen`, Antrag entfernen, Vorgang
/// mit Notiz erledigen. Gibt die Ereignisse der Vorgänge zurück.
fn antraege_ablaufen_in(txn: &WriteTransaction, jetzt: &Zeit) -> Result<Vec<Ereignis>, SpeicherFehler> {
    let abgelaufen: Vec<PasskeyAntrag> = {
        let t = txn.open_table(PASSKEY_ANTRAEGE)?;
        alle_aus::<PasskeyAntrag, _>(&t, "passkey_antraege")?
            .into_iter()
            .map(|(_, a)| a)
            .filter(|a| zeit_lesen(&a.ablauf).is_none_or(|ab| ab <= *jetzt))
            .collect()
    };
    let mut ereignisse = Vec::new();
    for a in abgelaufen {
        txn.open_table(PASSKEY_ANTRAEGE)?.remove(a.id.as_str())?;
        let p = passkey_aus(&txn.open_table(PASSKEYS)?, &a.id)?;
        if let Some(mut p) = p.filter(|p| p.status == PasskeyStatus::Ausstehend) {
            p.status = PasskeyStatus::Abgelaufen;
            p.entschieden = Some(zeit_text(jetzt));
            p.entschieden_von = Some("server".into());
            passkey_schreiben(txn, &p)?;
        }
        if let Some(v) = &a.vorgang {
            let notiz = format!("Abgelaufen: Der Antrag von {} lag 24 Stunden ohne Entscheidung.", a.konto);
            if let Some((_, e)) = vorgang_weiter_in(txn, v, Zustand::Erledigt, "server", Some(&notiz))? {
                ereignisse.push(e);
            }
        }
    }
    Ok(ereignisse)
}

/// Prüft die Grenzen für einen neuen Antrag des Kontos `sid` (nach dem Ablauf).
fn grenzen_pruefen_in(txn: &WriteTransaction, sid: &str, jetzt: &Zeit) -> Result<(), PasskeyFehler> {
    let offen: Vec<PasskeyAntrag> = alle_aus::<PasskeyAntrag, _>(&txn.open_table(PASSKEY_ANTRAEGE)?, "passkey_antraege")?.into_iter().map(|(_, a)| a).collect();
    if offen.iter().any(|a| a.konto_sid.eq_ignore_ascii_case(sid)) {
        return Err(PasskeyFehler::Grenze(
            "Für dieses Windows-Konto gibt es schon einen offenen Antrag. Bitte warten, bis er freigegeben oder abgelehnt ist.".into(),
        ));
    }
    if offen.len() >= ANTRAEGE_HOECHSTENS {
        return Err(PasskeyFehler::Grenze(format!("Es gibt schon {ANTRAEGE_HOECHSTENS} offene Anträge. Bitte im Hauptkonto erst über diese entscheiden.")));
    }
    let grenze = *jetzt - Duration::minutes(NACH_ABLEHNUNG_MINUTEN);
    let passkeys = alle_aus::<Passkey, _>(&txn.open_table(PASSKEYS)?, "passkeys")?;
    let kuerzlich_abgelehnt = passkeys.iter().any(|(_, p)| {
        p.status == PasskeyStatus::Abgelehnt
            && p.konto_sid.eq_ignore_ascii_case(sid)
            && p.entschieden.as_deref().and_then(zeit_lesen).is_some_and(|z| z > grenze)
    });
    if kuerzlich_abgelehnt {
        return Err(PasskeyFehler::Grenze("Der letzte Antrag dieses Kontos wurde abgelehnt. Ein neuer Antrag geht frühestens nach einer Stunde.".into()));
    }
    Ok(())
}

impl Speicher {
    /// Legt eine Challenge an und gibt sie zurück (32 Zufallsbytes, Base64url).
    /// Räumt abgelaufene mit weg; sind dann noch [`AUFGABEN_HOECHSTENS`] offen, 429.
    pub fn passkey_aufgabe_anlegen(&self, a: &PasskeyAufgabe, jetzt: &Zeit) -> Result<String, PasskeyFehler> {
        let txn = self.db.begin_write()?;
        let challenge = passkey::b64(&passkey::zufall(32));
        {
            let mut t = txn.open_table(PASSKEY_AUFGABEN)?;
            let alt: Vec<String> = alle_aus::<PasskeyAufgabe, _>(&t, "passkey_aufgaben")?
                .into_iter()
                .filter(|(_, a)| zeit_lesen(&a.ablauf).is_none_or(|ab| ab <= *jetzt))
                .map(|(k, _)| k)
                .collect();
            for k in alt {
                t.remove(k.as_str())?;
            }
            if redb::ReadableTableMetadata::len(&t)? as usize >= AUFGABEN_HOECHSTENS {
                return Err(PasskeyFehler::Grenze("Gerade laufen zu viele Anmeldungen. Bitte in ein paar Minuten noch einmal.".into()));
            }
            t.insert(challenge.as_str(), serde_json::to_vec(a)?.as_slice())?;
        }
        txn.commit()?;
        Ok(challenge)
    }

    /// Nimmt die Challenge heraus (einmal gültig, auch wenn der Schritt danach
    /// scheitert) und prüft Art und Ablauf.
    pub fn passkey_aufgabe_nehmen(&self, challenge: &str, art: AufgabenArt, jetzt: &Zeit) -> Result<PasskeyAufgabe, PasskeyFehler> {
        let txn = self.db.begin_write()?;
        let aufgabe = {
            let mut t = txn.open_table(PASSKEY_AUFGABEN)?;
            let roh = t.remove(challenge)?.map(|w| w.value().to_vec());
            match roh {
                Some(r) => Some(json_lesen::<PasskeyAufgabe>(&r, &format!("passkey_aufgaben/{challenge}"))?),
                None => None,
            }
        };
        txn.commit()?;
        let a = aufgabe.ok_or(Abweisung::ChallengeUnbekannt)?;
        if a.art != art {
            return Err(Abweisung::ChallengeFremd.into());
        }
        if zeit_lesen(&a.ablauf).is_none_or(|ab| ab <= *jetzt) {
            return Err(Abweisung::ChallengeAbgelaufen.into());
        }
        Ok(a)
    }

    /// Vor einem neuen Antrag des Kontos `sid`: abgelaufene Anträge schließen,
    /// dann die Grenzen prüfen. Gibt die Ereignisse der geschlossenen Vorgänge zurück.
    pub fn passkey_antrag_pruefen(&self, sid: &str, jetzt: &Zeit) -> Result<Vec<Ereignis>, PasskeyFehler> {
        let txn = self.db.begin_write()?;
        let ereignisse = antraege_ablaufen_in(&txn, jetzt)?;
        let grenzen = grenzen_pruefen_in(&txn, sid, jetzt);
        // Der Ablauf wird auch dann geschrieben, wenn eine Grenze greift.
        txn.commit()?;
        if !ereignisse.is_empty() {
            self.gemeldet(&[super::Bereich::Passkeys, super::Bereich::Vorgaenge]);
        }
        grenzen.map(|()| ereignisse)
    }

    /// Speichert einen geprüften neuen Passkey. Aus dem Hauptkonto sofort
    /// `registriert`; sonst `ausstehend` mit Antrag (Grenzen hier noch einmal
    /// in derselben Transaktion), Vergleichszahl und Vorgang `passkey-antrag`.
    pub fn passkey_anlegen(&self, neu: &NeuerPasskey, aufgabe: &PasskeyAufgabe, jetzt: &Zeit) -> Result<(Passkey, Vec<Ereignis>), PasskeyFehler> {
        let (Some(konto), Some(sid), Some(benutzer)) = (&aufgabe.konto, &aufgabe.konto_sid, &aufgabe.benutzer) else {
            return Err(Abweisung::ChallengeFremd.into());
        };
        let id = passkey::b64(&neu.id);
        let txn = self.db.begin_write()?;
        if passkey_aus(&txn.open_table(PASSKEYS)?, &id)?.is_some() {
            return Err(Abweisung::GibtEsSchon.into());
        }
        let mut ereignisse = Vec::new();
        let name = aufgabe.name.clone().unwrap_or_else(|| "Passkey".into());
        let mut p = Passkey {
            id: id.clone(),
            status: PasskeyStatus::Registriert,
            name: name.clone(),
            konto: konto.clone(),
            konto_sid: sid.clone(),
            hauptkonto: aufgabe.hauptkonto,
            verfahren: neu.verfahren,
            schluessel: passkey::b64(&neu.cose),
            benutzer: benutzer.clone(),
            zaehler: neu.zaehler,
            angelegt: zeit_text(jetzt),
            zuletzt_benutzt: None,
            vergleichszahl: None,
            ablauf: None,
            entschieden: Some(zeit_text(jetzt)),
            entschieden_von: Some("hauptkonto".into()),
            vorgang: None,
        };
        if !aufgabe.hauptkonto {
            ereignisse.extend(antraege_ablaufen_in(&txn, jetzt)?);
            grenzen_pruefen_in(&txn, sid, jetzt)?;
            let zahl = passkey::vergleichszahl();
            let ablauf = zeit_text(&(*jetzt + Duration::hours(ANTRAG_STUNDEN)));
            let titel = format!("Passkey-Antrag von {konto} (Gerät „{name}“), Vergleichszahl {zahl}");
            let (v, e) = vorgang_anlegen_in(&txn, VorgangsArt::PasskeyAntrag, SLUG, &titel, Some(&id), "server")?;
            ereignisse.push(e);
            let antrag = PasskeyAntrag {
                id: id.clone(),
                konto: konto.clone(),
                konto_sid: sid.clone(),
                vergleichszahl: zahl.clone(),
                angelegt: zeit_text(jetzt),
                ablauf: ablauf.clone(),
                vorgang: Some(v.id.clone()),
            };
            txn.open_table(PASSKEY_ANTRAEGE)?.insert(id.as_str(), serde_json::to_vec(&antrag)?.as_slice())?;
            p.status = PasskeyStatus::Ausstehend;
            p.vergleichszahl = Some(zahl);
            p.ablauf = Some(ablauf);
            p.entschieden = None;
            p.entschieden_von = None;
            p.vorgang = Some(v.id);
        }
        passkey_schreiben(&txn, &p)?;
        self.festschreiben(txn, &[super::Bereich::Passkeys, super::Bereich::Vorgaenge])?;
        Ok((p, ereignisse))
    }

    /// Anmeldung mit einem Passkey: nur `registriert`; prüft Signatur, Flags
    /// und Zähler (`webauthn::anmelden_pruefen`) und merkt Zähler und Zeit,
    /// alles in einer Transaktion. Die Challenge hat der Aufrufer schon genommen.
    #[allow(clippy::too_many_arguments)]
    pub fn passkey_anmelden(
        &self,
        id: &str,
        benutzer: Option<&str>,
        client_daten: &[u8],
        auth_daten: &[u8],
        signatur: &[u8],
        e: Erwartung<'_>,
        jetzt: &Zeit,
    ) -> Result<Passkey, PasskeyFehler> {
        let txn = self.db.begin_write()?;
        let mut p = passkey_aus(&txn.open_table(PASSKEYS)?, id)?.ok_or(Abweisung::Unbekannt)?;
        if p.status != PasskeyStatus::Registriert {
            return Err(Abweisung::NichtRegistriert.into());
        }
        if benutzer.is_some_and(|b| b != p.benutzer) {
            return Err(Abweisung::FremderBenutzer.into());
        }
        let cose = passkey::b64_lesen(&p.schluessel).ok_or(Abweisung::Schluessel)?;
        p.zaehler = webauthn::anmelden_pruefen(client_daten, auth_daten, signatur, &cose, p.zaehler, e)?;
        p.zuletzt_benutzt = Some(zeit_text(jetzt));
        passkey_schreiben(&txn, &p)?;
        self.festschreiben(txn, &[super::Bereich::Passkeys])?;
        Ok(p)
    }

    /// Freigeben, ablehnen oder widerrufen (nur als Inhaber, siehe Modul).
    pub fn passkey_entscheiden(&self, id: &str, was: &Entscheidung, wer: &Freigeber, jetzt: &Zeit) -> Result<(Passkey, Vec<Ereignis>), PasskeyFehler> {
        // Erst den Ablauf schreiben (gilt auch, wenn die Entscheidung danach scheitert).
        let mut ereignisse = {
            let txn = self.db.begin_write()?;
            let e = antraege_ablaufen_in(&txn, jetzt)?;
            txn.commit()?;
            if !e.is_empty() {
                self.gemeldet(&[super::Bereich::Passkeys, super::Bereich::Vorgaenge]);
            }
            e
        };
        let txn = self.db.begin_write()?;
        if let Freigeber::Passkey { cid, anmeldung, .. } = wer {
            if cid == id {
                return Err(PasskeyFehler::Selbst);
            }
            if jetzt.timestamp() - anmeldung > FRISCH_SEKUNDEN || *anmeldung > jetzt.timestamp() + 60 {
                return Err(PasskeyFehler::NichtFrisch);
            }
            let eigener = passkey_aus(&txn.open_table(PASSKEYS)?, cid)?;
            if eigener.is_none_or(|p| p.status != PasskeyStatus::Registriert) {
                return Err(PasskeyFehler::Zustand("Der eigene Passkey ist nicht mehr registriert.".into()));
            }
        }
        let mut p = passkey_aus(&txn.open_table(PASSKEYS)?, id)?.ok_or(PasskeyFehler::NichtGefunden)?;
        let antrag = {
            let t = txn.open_table(PASSKEY_ANTRAEGE)?;
            match t.get(id)? {
                Some(w) => Some(json_lesen::<PasskeyAntrag>(w.value(), &format!("passkey_antraege/{id}"))?),
                None => None,
            }
        };
        let (status, notiz) = match was {
            Entscheidung::Freigeben { vergleichszahl } => {
                let Some(a) = antrag.as_ref().filter(|_| p.status == PasskeyStatus::Ausstehend) else {
                    return Err(PasskeyFehler::Zustand(match p.status {
                        PasskeyStatus::Abgelaufen => "Dieser Antrag ist abgelaufen (24 Stunden). Bitte neu beantragen.".into(),
                        _ => "Dieser Passkey wartet nicht auf eine Freigabe.".into(),
                    }));
                };
                if a.vergleichszahl != vergleichszahl.trim() {
                    return Err(PasskeyFehler::Vergleichszahl);
                }
                (PasskeyStatus::Registriert, format!("Freigegeben {}.", wer.satz()))
            }
            Entscheidung::Ablehnen => {
                if p.status != PasskeyStatus::Ausstehend || antrag.is_none() {
                    return Err(PasskeyFehler::Zustand("Dieser Passkey wartet nicht auf eine Entscheidung.".into()));
                }
                (PasskeyStatus::Abgelehnt, format!("Abgelehnt {}.", wer.satz()))
            }
            Entscheidung::Widerrufen => {
                if p.status != PasskeyStatus::Registriert {
                    return Err(PasskeyFehler::Zustand("Nur registrierte Passkeys lassen sich widerrufen; offene Anträge bitte ablehnen.".into()));
                }
                (PasskeyStatus::Widerrufen, format!("Widerrufen {}.", wer.satz()))
            }
        };
        p.status = status;
        p.entschieden = Some(zeit_text(jetzt));
        p.entschieden_von = Some(wer.wort());
        passkey_schreiben(&txn, &p)?;
        if antrag.is_some() {
            txn.open_table(PASSKEY_ANTRAEGE)?.remove(id)?;
        }
        if let Some(v) = &p.vorgang
            && let Some((_, e)) = vorgang_weiter_in(&txn, v, Zustand::Erledigt, "browser", Some(&notiz))?
        {
            ereignisse.push(e);
        }
        self.festschreiben(txn, &[super::Bereich::Passkeys, super::Bereich::Vorgaenge])?;
        Ok((p, ereignisse))
    }

    /// Alle Passkeys, neueste zuerst.
    pub fn passkeys(&self) -> Result<Vec<Passkey>, SpeicherFehler> {
        let txn = self.db.begin_read()?;
        let Some(t) = tabellen::lesen(PASSKEYS, &txn)? else { return Ok(Vec::new()) };
        let mut alle: Vec<Passkey> = alle_aus::<Passkey, _>(&t, "passkeys")?.into_iter().map(|(_, p)| p).collect();
        alle.sort_by(|a, b| b.angelegt.cmp(&a.angelegt).then_with(|| a.id.cmp(&b.id)));
        Ok(alle)
    }

    /// Ein Passkey nach Kennung.
    pub fn passkey(&self, id: &str) -> Result<Option<Passkey>, SpeicherFehler> {
        let txn = self.db.begin_read()?;
        match tabellen::lesen(PASSKEYS, &txn)? {
            Some(t) => passkey_aus(&t, id),
            None => Ok(None),
        }
    }

    /// Zahl der offenen Anträge, die noch nicht abgelaufen sind.
    pub fn passkey_antraege_offen(&self, jetzt: &Zeit) -> Result<u32, SpeicherFehler> {
        let txn = self.db.begin_read()?;
        let Some(t) = tabellen::lesen(PASSKEY_ANTRAEGE, &txn)? else { return Ok(0) };
        let n = alle_aus::<PasskeyAntrag, _>(&t, "passkey_antraege")?.iter().filter(|(_, a)| zeit_lesen(&a.ablauf).is_some_and(|ab| ab > *jetzt)).count();
        Ok(n as u32)
    }

    /// Regelmäßig (jede Minute): abgelaufene Anträge schließen, abgelaufene
    /// Challenges und Sperren entfernen. Gibt die Ereignisse der Vorgänge zurück.
    pub fn passkey_aufraeumen(&self, jetzt: &Zeit) -> Result<Vec<Ereignis>, SpeicherFehler> {
        let txn = self.db.begin_write()?;
        let ereignisse = antraege_ablaufen_in(&txn, jetzt)?;
        {
            let mut t = txn.open_table(PASSKEY_AUFGABEN)?;
            let alt: Vec<String> = alle_aus::<PasskeyAufgabe, _>(&t, "passkey_aufgaben")?
                .into_iter()
                .filter(|(_, a)| zeit_lesen(&a.ablauf).is_none_or(|ab| ab <= *jetzt))
                .map(|(k, _)| k)
                .collect();
            for k in alt {
                t.remove(k.as_str())?;
            }
            let mut s = txn.open_table(JWT_SPERRLISTE)?;
            let alt: Vec<String> = alle_aus::<Sperre, _>(&s, "jwt_sperrliste")?
                .into_iter()
                .filter(|(_, sp)| zeit_lesen(&sp.bis).is_none_or(|b| b <= *jetzt))
                .map(|(k, _)| k)
                .collect();
            for k in alt {
                s.remove(k.as_str())?;
            }
        }
        txn.commit()?;
        // Abgelaufene Challenges und Sperren sieht niemand; nur geschlossene Anträge melden.
        if !ereignisse.is_empty() {
            self.gemeldet(&[super::Bereich::Passkeys, super::Bereich::Vorgaenge]);
        }
        Ok(ereignisse)
    }

    /// Sperrt einen Ausweis (Abmelden, Wechsel), bis `bis` (Sekunden seit 1970).
    pub fn ausweis_sperren(&self, jti: &str, bis: i64, jetzt: &Zeit) -> Result<(), SpeicherFehler> {
        let bis = DateTime::from_timestamp(bis, 0).map(|b| b.with_timezone(&jetzt.timezone())).unwrap_or(*jetzt);
        let sperre = Sperre { bis: zeit_text(&bis), gesperrt: zeit_text(jetzt) };
        let txn = self.db.begin_write()?;
        txn.open_table(JWT_SPERRLISTE)?.insert(jti, serde_json::to_vec(&sperre)?.as_slice())?;
        self.festschreiben(txn, &[super::Bereich::Passkeys])?;
        Ok(())
    }

    /// „Alle Sitzungen beenden“: jeder Ausweis mit Anmeldung bis jetzt gilt nicht mehr.
    pub fn alle_sitzungen_beenden(&self, jetzt: &Zeit) -> Result<(), SpeicherFehler> {
        let sperre = Sperre { bis: zeit_text(&(*jetzt + Duration::seconds(SITZUNG_HOECHSTENS_SEKUNDEN))), gesperrt: zeit_text(jetzt) };
        let txn = self.db.begin_write()?;
        txn.open_table(JWT_SPERRLISTE)?.insert(ALLE, serde_json::to_vec(&sperre)?.as_slice())?;
        self.festschreiben(txn, &[super::Bereich::Passkeys])?;
        Ok(())
    }

    /// Gilt die Sitzung noch? Nicht gesperrt (`jti`), nicht durch „alle
    /// beenden“ erfasst und der Passkey `cid` noch registriert. Gibt dann den
    /// Passkey zurück. Wird bei jeder Anfrage mit Ausweis gefragt; so beendet
    /// ein Widerruf sofort alle Sitzungen mit diesem Passkey.
    pub fn sitzung_gilt(&self, jti: &str, cid: &str, anmeldung: i64) -> Result<Option<Passkey>, SpeicherFehler> {
        let txn = self.db.begin_read()?;
        if let Some(s) = tabellen::lesen(JWT_SPERRLISTE, &txn)? {
            if s.get(jti)?.is_some() {
                return Ok(None);
            }
            if let Some(w) = s.get(ALLE)? {
                let sperre: Sperre = json_lesen(w.value(), "jwt_sperrliste/alle")?;
                if zeit_lesen(&sperre.gesperrt).is_none_or(|g| anmeldung <= g.timestamp()) {
                    return Ok(None);
                }
            }
        }
        let Some(t) = tabellen::lesen(PASSKEYS, &txn)? else { return Ok(None) };
        Ok(passkey_aus(&t, cid)?.filter(|p| p.status == PasskeyStatus::Registriert))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anmeldung::WindowsKonto;
    use crate::passkey::webauthn::pruef_authenticator::PruefAuthenticator;

    const HERKUNFT: &str = "http://localhost:8787";
    const E: Erwartung<'static> = Erwartung { herkunft: HERKUNFT, rp_id: "localhost" };

    fn jetzt() -> Zeit {
        DateTime::parse_from_rfc3339("2026-10-30T10:00:00+01:00").unwrap()
    }

    fn konto(n: u32) -> WindowsKonto {
        WindowsKonto { sid: format!("S-1-5-21-1-2-3-{}", 1000 + n), name: format!(r"PC\konto{n}"), inhaber: false }
    }

    /// Ein Passkey über den ganzen Weg: Challenge, Prüfung, Speichern.
    fn anlegen(s: &Speicher, a: &PruefAuthenticator, k: &WindowsKonto, z: &Zeit) -> Result<(Passkey, Vec<Ereignis>), PasskeyFehler> {
        if !k.inhaber {
            s.passkey_antrag_pruefen(&k.sid, z)?;
        }
        let aufgabe = PasskeyAufgabe::anlegen(k, "Testgerät", z);
        let ch = s.passkey_aufgabe_anlegen(&aufgabe, z)?;
        let (c, att) = a.anlegen(&ch, HERKUNFT);
        let neu = webauthn::anlegen_pruefen(&c, &att, &a.id, E)?;
        let aufgabe = s.passkey_aufgabe_nehmen(&neu.challenge, AufgabenArt::Anlegen, z)?;
        s.passkey_anlegen(&neu, &aufgabe, z)
    }

    fn anmelden(s: &Speicher, a: &mut PruefAuthenticator, z: &Zeit) -> Result<Passkey, PasskeyFehler> {
        let ch = s.passkey_aufgabe_anlegen(&PasskeyAufgabe::anmelden(z), z)?;
        let (c, auth, sig) = a.anmelden(&ch, HERKUNFT);
        let gelesen = webauthn::client_daten_pruefen(&c, "webauthn.get", HERKUNFT)?;
        s.passkey_aufgabe_nehmen(&gelesen, AufgabenArt::Anmelden, z)?;
        s.passkey_anmelden(&a.id_b64(), None, &c, &auth, &sig, E, z)
    }

    fn hauptkonto_freigeber() -> Freigeber {
        Freigeber::Hauptkonto
    }

    #[test]
    fn hauptkonto_ist_sofort_registriert_und_kann_sich_anmelden() {
        let s = Speicher::temporaer();
        let mut a = PruefAuthenticator::es256();
        let (p, e) = anlegen(&s, &a, &WindowsKonto::test(true), &jetzt()).unwrap();
        assert_eq!((p.status, p.hauptkonto, e.len()), (PasskeyStatus::Registriert, true, 0));
        assert_eq!(s.passkey_antraege_offen(&jetzt()).unwrap(), 0);
        let p = anmelden(&s, &mut a, &jetzt()).unwrap();
        assert_eq!(p.zuletzt_benutzt.as_deref(), Some("2026-10-30T10:00:00+01:00"));
        // RS256 ebenso (Windows Hello).
        let mut r = PruefAuthenticator::rs256();
        anlegen(&s, &r, &WindowsKonto::test(true), &jetzt()).unwrap();
        assert_eq!(anmelden(&s, &mut r, &jetzt()).unwrap().verfahren, webauthn::RS256);
    }

    #[test]
    fn antrag_ist_ausstehend_berechtigt_zu_nichts_und_wird_ein_vorgang() {
        let s = Speicher::temporaer();
        let mut a = PruefAuthenticator::es256();
        let (p, e) = anlegen(&s, &a, &konto(1), &jetzt()).unwrap();
        assert_eq!(p.status, PasskeyStatus::Ausstehend);
        let zahl = p.vergleichszahl.clone().unwrap();
        assert!(zahl.len() == 4 && zahl.bytes().all(|c| c.is_ascii_digit()), "{zahl}");
        assert_eq!(p.konto, r"PC\konto1");
        assert_eq!(e.len(), 1);
        let (v, _) = s.vorgang(p.vorgang.as_deref().unwrap()).unwrap().unwrap();
        assert_eq!((v.art, v.slug.as_str(), v.zustand), (VorgangsArt::PasskeyAntrag, SLUG, Zustand::Offen));
        assert!(v.titel.contains(&zahl) && v.titel.contains(r"PC\konto1"), "{}", v.titel);
        // Ausstehend: keine Anmeldung, keine Sitzung.
        assert!(matches!(anmelden(&s, &mut a, &jetzt()), Err(PasskeyFehler::Abgewiesen(Abweisung::NichtRegistriert))));
        assert!(s.sitzung_gilt("jti", &p.id, jetzt().timestamp()).unwrap().is_none());
        assert_eq!(s.passkey_antraege_offen(&jetzt()).unwrap(), 1);
    }

    #[test]
    fn freigabe_aus_dem_hauptkonto_macht_ihn_registriert() {
        let s = Speicher::temporaer();
        let mut a = PruefAuthenticator::es256();
        let (p, _) = anlegen(&s, &a, &konto(1), &jetzt()).unwrap();
        let zahl = p.vergleichszahl.clone().unwrap();
        let falsch = if zahl == "0000" { "0001" } else { "0000" };
        assert!(matches!(
            s.passkey_entscheiden(&p.id, &Entscheidung::Freigeben { vergleichszahl: falsch.into() }, &hauptkonto_freigeber(), &jetzt()),
            Err(PasskeyFehler::Vergleichszahl)
        ));
        let (p, e) = s.passkey_entscheiden(&p.id, &Entscheidung::Freigeben { vergleichszahl: zahl }, &hauptkonto_freigeber(), &jetzt()).unwrap();
        assert_eq!((p.status, p.entschieden_von.as_deref()), (PasskeyStatus::Registriert, Some("hauptkonto")));
        assert_eq!(e.len(), 1);
        let (v, _) = s.vorgang(p.vorgang.as_deref().unwrap()).unwrap().unwrap();
        assert_eq!(v.zustand, Zustand::Erledigt);
        assert!(v.notiz.as_deref().unwrap().starts_with("Freigegeben"), "{:?}", v.notiz);
        assert_eq!(s.passkey_antraege_offen(&jetzt()).unwrap(), 0);
        assert!(anmelden(&s, &mut a, &jetzt()).is_ok());
        assert!(s.sitzung_gilt("jti", &p.id, jetzt().timestamp()).unwrap().is_some());
    }

    #[test]
    fn ablehnung_bleibt_als_stand_und_sperrt_das_konto_eine_stunde() {
        let s = Speicher::temporaer();
        let a = PruefAuthenticator::es256();
        let (p, _) = anlegen(&s, &a, &konto(1), &jetzt()).unwrap();
        let (p, _) = s.passkey_entscheiden(&p.id, &Entscheidung::Ablehnen, &hauptkonto_freigeber(), &jetzt()).unwrap();
        assert_eq!(p.status, PasskeyStatus::Abgelehnt);
        // Daten werden nie gelöscht: der Eintrag bleibt mit Stand und Schlüssel.
        let gespeichert = s.passkey(&p.id).unwrap().unwrap();
        assert_eq!((gespeichert.status, gespeichert.schluessel.is_empty()), (PasskeyStatus::Abgelehnt, false));
        let (v, _) = s.vorgang(p.vorgang.as_deref().unwrap()).unwrap().unwrap();
        assert_eq!(v.zustand, Zustand::Erledigt);
        // Dasselbe Konto: erst nach einer Stunde.
        let b = PruefAuthenticator::es256();
        let r = anlegen(&s, &b, &konto(1), &(jetzt() + Duration::minutes(59)));
        assert!(matches!(r, Err(PasskeyFehler::Grenze(ref t)) if t.contains("Stunde")), "{r:?}");
        assert!(anlegen(&s, &b, &konto(1), &(jetzt() + Duration::minutes(61))).is_ok());
        // Ein anderes Konto sofort.
        assert!(anlegen(&s, &PruefAuthenticator::es256(), &konto(2), &jetzt()).is_ok());
    }

    #[test]
    fn grenzen_sechster_antrag_und_zweiter_desselben_kontos() {
        let s = Speicher::temporaer();
        for n in 1..=5 {
            anlegen(&s, &PruefAuthenticator::es256(), &konto(n), &jetzt()).unwrap();
        }
        let r = anlegen(&s, &PruefAuthenticator::es256(), &konto(6), &jetzt());
        assert!(matches!(r, Err(PasskeyFehler::Grenze(ref t)) if t.contains("5 offene")), "{r:?}");
        let r = anlegen(&s, &PruefAuthenticator::es256(), &konto(3), &jetzt());
        assert!(matches!(r, Err(PasskeyFehler::Grenze(ref t)) if t.contains("schon einen offenen")), "{r:?}");
        // Auch ohne die Vorprüfung greift die Grenze beim Speichern.
        let k = konto(7);
        let a = PruefAuthenticator::es256();
        let ch = s.passkey_aufgabe_anlegen(&PasskeyAufgabe::anlegen(&k, "x", &jetzt()), &jetzt()).unwrap();
        let (c, att) = a.anlegen(&ch, HERKUNFT);
        let neu = webauthn::anlegen_pruefen(&c, &att, &a.id, E).unwrap();
        let aufgabe = s.passkey_aufgabe_nehmen(&ch, AufgabenArt::Anlegen, &jetzt()).unwrap();
        assert!(matches!(s.passkey_anlegen(&neu, &aufgabe, &jetzt()), Err(PasskeyFehler::Grenze(_))));
        // Das Hauptkonto hat keine Grenze.
        assert!(anlegen(&s, &PruefAuthenticator::es256(), &WindowsKonto::test(true), &jetzt()).is_ok());
    }

    #[test]
    fn antrag_nach_24_stunden_abgelaufen_und_nicht_mehr_freizugeben() {
        let s = Speicher::temporaer();
        let (p, _) = anlegen(&s, &PruefAuthenticator::es256(), &konto(1), &jetzt()).unwrap();
        let spaeter = jetzt() + Duration::hours(24) + Duration::seconds(1);
        assert_eq!(s.passkey_antraege_offen(&spaeter).unwrap(), 0);
        let r = s.passkey_entscheiden(&p.id, &Entscheidung::Freigeben { vergleichszahl: p.vergleichszahl.clone().unwrap() }, &hauptkonto_freigeber(), &spaeter);
        assert!(matches!(r, Err(PasskeyFehler::Zustand(ref t)) if t.contains("abgelaufen")), "{r:?}");
        let p = s.passkey(&p.id).unwrap().unwrap();
        assert_eq!((p.status, p.entschieden_von.as_deref()), (PasskeyStatus::Abgelaufen, Some("server")));
        let (v, _) = s.vorgang(p.vorgang.as_deref().unwrap()).unwrap().unwrap();
        assert_eq!(v.zustand, Zustand::Erledigt);
        // Danach darf dasselbe Konto neu beantragen.
        assert!(anlegen(&s, &PruefAuthenticator::es256(), &konto(1), &spaeter).is_ok());
    }

    #[test]
    fn freigabe_mit_passkey_nur_frisch_und_nie_ueber_sich_selbst() {
        let s = Speicher::temporaer();
        let mut inhaber = PruefAuthenticator::es256();
        let (eigen, _) = anlegen(&s, &inhaber, &WindowsKonto::test(true), &jetzt()).unwrap();
        anmelden(&s, &mut inhaber, &jetzt()).unwrap();
        let (p, _) = anlegen(&s, &PruefAuthenticator::es256(), &konto(1), &jetzt()).unwrap();
        let zahl = p.vergleichszahl.clone().unwrap();
        let freigeben = Entscheidung::Freigeben { vergleichszahl: zahl };
        let alt = Freigeber::Passkey { cid: eigen.id.clone(), name: eigen.name.clone(), anmeldung: jetzt().timestamp() - 6 * 60 };
        assert!(matches!(s.passkey_entscheiden(&p.id, &freigeben, &alt, &jetzt()), Err(PasskeyFehler::NichtFrisch)));
        assert!(matches!(s.passkey_entscheiden(&p.id, &Entscheidung::Ablehnen, &alt, &jetzt()), Err(PasskeyFehler::NichtFrisch)));
        // Selbstfreigabe und Selbstentscheidung: abgewiesen, auch frisch.
        let frisch = Freigeber::Passkey { cid: eigen.id.clone(), name: eigen.name.clone(), anmeldung: jetzt().timestamp() - 60 };
        assert!(matches!(s.passkey_entscheiden(&eigen.id, &Entscheidung::Widerrufen, &frisch, &jetzt()), Err(PasskeyFehler::Selbst)));
        let ausstehend = Freigeber::Passkey { cid: p.id.clone(), name: p.name.clone(), anmeldung: jetzt().timestamp() };
        assert!(matches!(s.passkey_entscheiden(&p.id, &freigeben, &ausstehend, &jetzt()), Err(PasskeyFehler::Selbst)));
        // Ein anderer, noch ausstehender Passkey als Freigeber: abgewiesen.
        let (q, _) = anlegen(&s, &PruefAuthenticator::es256(), &konto(2), &jetzt()).unwrap();
        let fremd = Freigeber::Passkey { cid: q.id.clone(), name: q.name.clone(), anmeldung: jetzt().timestamp() };
        assert!(matches!(s.passkey_entscheiden(&p.id, &freigeben, &fremd, &jetzt()), Err(PasskeyFehler::Zustand(_))));
        // Frisch und registriert: geht.
        let (p, _) = s.passkey_entscheiden(&p.id, &freigeben, &frisch, &jetzt()).unwrap();
        assert_eq!(p.status, PasskeyStatus::Registriert);
        assert_eq!(p.entschieden_von.as_deref(), Some("passkey:Testgerät"));
    }

    #[test]
    fn widerruf_beendet_die_sitzung_und_behaelt_den_schluessel() {
        let s = Speicher::temporaer();
        let mut a = PruefAuthenticator::es256();
        let (p, _) = anlegen(&s, &a, &WindowsKonto::test(true), &jetzt()).unwrap();
        anmelden(&s, &mut a, &jetzt()).unwrap();
        assert!(s.sitzung_gilt("jti-1", &p.id, jetzt().timestamp()).unwrap().is_some());
        let (w, _) = s.passkey_entscheiden(&p.id, &Entscheidung::Widerrufen, &hauptkonto_freigeber(), &jetzt()).unwrap();
        assert_eq!(w.status, PasskeyStatus::Widerrufen);
        assert_eq!(w.schluessel, p.schluessel, "öffentlicher Schlüssel bleibt");
        assert!(s.sitzung_gilt("jti-1", &p.id, jetzt().timestamp()).unwrap().is_none());
        assert!(matches!(anmelden(&s, &mut a, &jetzt()), Err(PasskeyFehler::Abgewiesen(Abweisung::NichtRegistriert))));
        // Ein zweites Mal: nichts mehr zu widerrufen.
        assert!(matches!(s.passkey_entscheiden(&p.id, &Entscheidung::Widerrufen, &hauptkonto_freigeber(), &jetzt()), Err(PasskeyFehler::Zustand(_))));
    }

    #[test]
    fn challenge_einmal_gueltig_fuenf_minuten_und_nur_fuer_ihre_art() {
        let s = Speicher::temporaer();
        let mut a = PruefAuthenticator::es256();
        anlegen(&s, &a, &WindowsKonto::test(true), &jetzt()).unwrap();
        let ch = s.passkey_aufgabe_anlegen(&PasskeyAufgabe::anmelden(&jetzt()), &jetzt()).unwrap();
        let (c, auth, sig) = a.anmelden(&ch, HERKUNFT);
        s.passkey_aufgabe_nehmen(&ch, AufgabenArt::Anmelden, &jetzt()).unwrap();
        s.passkey_anmelden(&a.id_b64(), None, &c, &auth, &sig, E, &jetzt()).unwrap();
        // Wiederholt: die Challenge ist verbraucht.
        assert!(matches!(s.passkey_aufgabe_nehmen(&ch, AufgabenArt::Anmelden, &jetzt()), Err(PasskeyFehler::Abgewiesen(Abweisung::ChallengeUnbekannt))));
        // Abgelaufen nach 5 Minuten (und danach auch verbraucht).
        let ch = s.passkey_aufgabe_anlegen(&PasskeyAufgabe::anmelden(&jetzt()), &jetzt()).unwrap();
        assert!(matches!(
            s.passkey_aufgabe_nehmen(&ch, AufgabenArt::Anmelden, &(jetzt() + Duration::seconds(300))),
            Err(PasskeyFehler::Abgewiesen(Abweisung::ChallengeAbgelaufen))
        ));
        assert!(s.passkey_aufgabe_nehmen(&ch, AufgabenArt::Anmelden, &jetzt()).is_err());
        // Eine Challenge zum Anmelden taugt nicht zum Anlegen.
        let ch = s.passkey_aufgabe_anlegen(&PasskeyAufgabe::anmelden(&jetzt()), &jetzt()).unwrap();
        assert!(matches!(s.passkey_aufgabe_nehmen(&ch, AufgabenArt::Anlegen, &jetzt()), Err(PasskeyFehler::Abgewiesen(Abweisung::ChallengeFremd))));
        // Ausgedachte Challenge.
        assert!(s.passkey_aufgabe_nehmen("AAAA", AufgabenArt::Anmelden, &jetzt()).is_err());
    }

    #[test]
    fn zaehler_faellt_und_fremder_passkey() {
        let s = Speicher::temporaer();
        let mut a = PruefAuthenticator::es256();
        a.zaehler = 10;
        let (p, _) = anlegen(&s, &a, &WindowsKonto::test(true), &jetzt()).unwrap();
        assert_eq!(anmelden(&s, &mut a, &jetzt()).unwrap().zaehler, 11);
        a.zaehler = 5;
        assert!(matches!(anmelden(&s, &mut a, &jetzt()), Err(PasskeyFehler::Abgewiesen(Abweisung::ZaehlerFaellt))));
        assert_eq!(s.passkey(&p.id).unwrap().unwrap().zaehler, 11, "Zähler bleibt beim Fehlschlag");
        // Unbekannte Credential-ID.
        let mut fremd = PruefAuthenticator::es256();
        assert!(matches!(anmelden(&s, &mut fremd, &jetzt()), Err(PasskeyFehler::Abgewiesen(Abweisung::Unbekannt))));
        // Bekannte ID, aber Signatur eines anderen Schlüssels.
        let mut falsch = PruefAuthenticator::es256();
        falsch.id = a.id.clone();
        falsch.zaehler = 50;
        assert!(matches!(anmelden(&s, &mut falsch, &jetzt()), Err(PasskeyFehler::Abgewiesen(Abweisung::Signatur))));
        // Derselbe Passkey ein zweites Mal anlegen: abgewiesen.
        assert!(matches!(anlegen(&s, &a, &WindowsKonto::test(true), &jetzt()), Err(PasskeyFehler::Abgewiesen(Abweisung::GibtEsSchon))));
    }

    #[test]
    fn sperrliste_jti_und_alle_beenden() {
        let s = Speicher::temporaer();
        let a = PruefAuthenticator::es256();
        let (p, _) = anlegen(&s, &a, &WindowsKonto::test(true), &jetzt()).unwrap();
        let t = jetzt().timestamp();
        s.ausweis_sperren("jti-weg", t + 900, &jetzt()).unwrap();
        assert!(s.sitzung_gilt("jti-weg", &p.id, t).unwrap().is_none());
        assert!(s.sitzung_gilt("jti-da", &p.id, t).unwrap().is_some());
        s.alle_sitzungen_beenden(&jetzt()).unwrap();
        assert!(s.sitzung_gilt("jti-da", &p.id, t).unwrap().is_none());
        assert!(s.sitzung_gilt("jti-neu", &p.id, t + 1).unwrap().is_some(), "neue Anmeldung danach gilt");
        // Aufräumen nach Ablauf.
        s.passkey_aufraeumen(&(jetzt() + Duration::hours(9))).unwrap();
        assert!(s.sitzung_gilt("jti-weg", &p.id, t).unwrap().is_some(), "Eintrag nach Ablauf weg");
    }
}
