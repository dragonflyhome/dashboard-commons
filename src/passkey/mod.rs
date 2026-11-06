//! Passkey für andere Konten (Etappe 7c, `[passkey]` in
//! `autostart-dienst`).
//!
//! Zwei Wege führen zum selben Inhaber: Im Hauptkonto meldet Windows ihn
//! von selbst an (Negotiate, 7b). Sitzt er in einem anderen Windows-Konto,
//! meldet sie sich mit einem Passkey an und bekommt einen kurzlebigen,
//! signierten Ausweis (JWT) als Cookie `__Host-dashboard`.
//!
//! - `webauthn`: die Prüfung der Antworten des Browsers, selbst gebaut und eng
//!   (nur Attestation `none`, Herkunft genau `http://localhost:<port>`, UP und
//!   UV Pflicht, nur ES256 und RS256)
//! - `jwt`: Ausweis mit Ed25519 ausstellen und prüfen (nur `alg` EdDSA)
//! - `geheim`: die Datei mit dem JWT-Schlüssel, beim ersten Start angelegt,
//!   nur für das Dienstkonto und die Administratoren lesbar
//! - `cookie`: das Cookie setzen, lesen und löschen
//!
//! Die Tabellen (Passkeys, Challenges, Anträge, Sperrliste) liegen in
//! `speicher::passkeys`, die Wege in `api::passkey`, die Schleuse in
//! `sicherheit::anmeldung_pflicht`.

pub mod cookie;
pub mod geheim;
pub mod jwt;
pub mod webauthn;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;

/// Name der Website für WebAuthn: Passkeys gehen nicht mit einer IP-Adresse.
pub const RP_ID: &str = "localhost";
/// Anzeigename der Website im Dialog von Windows Hello.
pub const RP_NAME: &str = "Dashboard";

/// So lange gilt eine Challenge (Anlegen oder Anmelden), danach ist sie verbraucht.
pub const CHALLENGE_SEKUNDEN: i64 = 5 * 60;
/// So lange gilt ein Ausweis, bis er bei Nutzung erneuert wird.
pub const AUSWEIS_SEKUNDEN: i64 = 15 * 60;
/// Höchstens so lange nach der Anmeldung mit Passkey; dann braucht es einen neuen.
pub const SITZUNG_HOECHSTENS_SEKUNDEN: i64 = 8 * 60 * 60;
/// Freigeben, Ablehnen und Widerrufen mit Passkey nur, wenn die Anmeldung so frisch ist.
pub const FRISCH_SEKUNDEN: i64 = 5 * 60;
/// Ein Ausweis wird erneuert, wenn er älter ist als das (nicht bei jeder Anfrage).
pub const ERNEUERN_NACH_SEKUNDEN: i64 = 60;

/// Höchstens so viele offene Anträge insgesamt.
pub const ANTRAEGE_HOECHSTENS: usize = 5;
/// Ein Antrag verfällt nach so vielen Stunden.
pub const ANTRAG_STUNDEN: i64 = 24;
/// Nach einer Ablehnung darf dasselbe Konto erst nach so vielen Minuten neu beantragen.
pub const NACH_ABLEHNUNG_MINUTEN: i64 = 60;

/// Die Herkunft, die WebAuthn verlangt: genau `http://localhost:<port>`.
pub fn herkunft(port: u16) -> String {
    format!("http://{RP_ID}:{port}")
}

/// Base64url ohne Füllzeichen (WebAuthn, JWT).
pub fn b64(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Base64url ohne Füllzeichen, streng (keine Füllzeichen, kein anderes Alphabet).
pub fn b64_lesen(text: &str) -> Option<Vec<u8>> {
    URL_SAFE_NO_PAD.decode(text.as_bytes()).ok()
}

/// `n` Zufallsbytes aus dem Betriebssystem.
pub fn zufall(n: usize) -> Vec<u8> {
    use rand_core::{OsRng, RngCore};
    let mut v = vec![0u8; n];
    OsRng.fill_bytes(&mut v);
    v
}

/// Vierstellige Vergleichszahl (0000 bis 9999), gleich verteilt.
pub fn vergleichszahl() -> String {
    loop {
        let b = zufall(2);
        let n = u16::from_le_bytes([b[0], b[1]]);
        // 65536 ist kein Vielfaches von 10000: obere Reste verwerfen.
        if n < 60_000 {
            return format!("{:04}", n % 10_000);
        }
    }
}

/// Warum ein Passkey-Schritt abgewiesen wurde. Der Satz ist kurz und
/// technisch, fürs Protokoll; der Inhaber bekommt aus `api::passkey` einen
/// einfachen Satz.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Abweisung {
    #[error("unlesbar: {0}")]
    Unlesbar(&'static str),
    #[error("falsche Art in clientDataJSON")]
    FalscheArt,
    #[error("falsche Herkunft")]
    FalscheHerkunft,
    #[error("aus einem fremden Rahmen (crossOrigin)")]
    FremderRahmen,
    #[error("Challenge unbekannt oder schon benutzt")]
    ChallengeUnbekannt,
    #[error("Challenge abgelaufen")]
    ChallengeAbgelaufen,
    #[error("Challenge gehört zu einem anderen Schritt oder Konto")]
    ChallengeFremd,
    #[error("rpIdHash passt nicht")]
    FalscheWebsite,
    #[error("Nutzer nicht anwesend (UP fehlt)")]
    OhneAnwesenheit,
    #[error("Nutzer nicht geprüft (UV fehlt)")]
    OhneNutzerpruefung,
    #[error("Attestation ist nicht none")]
    Attestation,
    #[error("Verfahren nicht erlaubt (nur ES256 und RS256)")]
    Verfahren,
    #[error("Schlüssel unbrauchbar")]
    Schluessel,
    #[error("Signatur falsch")]
    Signatur,
    #[error("Zähler ist nicht gestiegen")]
    ZaehlerFaellt,
    #[error("Passkey unbekannt")]
    Unbekannt,
    #[error("Passkey nicht registriert")]
    NichtRegistriert,
    #[error("Passkey gibt es schon")]
    GibtEsSchon,
    #[error("Benutzerkennung passt nicht")]
    FremderBenutzer,
}
