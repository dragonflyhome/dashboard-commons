//! Prüfung der WebAuthn-Antworten des Browsers, selbst gebaut und eng
//! (`[passkey]`, bauplan; webauthn-rs scheidet aus, es braucht OpenSSL).
//!
//! Angenommen wird nur genau das, was Edge mit Windows Hello, Handy oder
//! Sicherheitsschlüssel auf `http://localhost:<port>` schickt:
//! - `clientDataJSON` mit der richtigen Art (`webauthn.create` oder
//!   `webauthn.get`), genau der Herkunft `http://localhost:<port>`, ohne
//!   fremden Rahmen (`crossOrigin`, `topOrigin`); die Challenge prüft der
//!   Aufrufer gegen den Datenspeicher (einmal gültig, 5 Minuten);
//! - Authenticator-Daten mit dem rpIdHash von `localhost` und den Flags UP
//!   (anwesend) und UV (geprüft, etwa Fingerabdruck oder PIN);
//! - beim Anlegen nur Attestation `none` (kein Zertifikat, nichts zu prüfen,
//!   nichts zu vertrauen) und nur die Verfahren ES256 und RS256;
//! - beim Anmelden eine gültige Signatur über Authenticator-Daten und den
//!   SHA-256 von `clientDataJSON`, und ein Zähler, der steigt, wenn er nicht 0 ist.
//!
//! Alles hier ist rein (keine Uhr, kein Datenspeicher), damit die Tests jede
//! Abweisung einzeln zeigen.

use ciborium::Value;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use super::Abweisung;

/// COSE-Kennung für ECDSA mit P-256 und SHA-256.
pub const ES256: i64 = -7;
/// COSE-Kennung für RSASSA-PKCS1-v1_5 mit SHA-256 (Windows Hello nutzt es oft).
pub const RS256: i64 = -257;
/// Die erlaubten Verfahren, in der Reihenfolge, in der der Browser sie anbieten soll.
pub const VERFAHREN: [i64; 2] = [ES256, RS256];

/// Flags in den Authenticator-Daten.
pub const FLAG_UP: u8 = 0x01;
pub const FLAG_UV: u8 = 0x04;
pub const FLAG_AT: u8 = 0x40;
pub const FLAG_ED: u8 = 0x80;

/// Längste Credential-ID, die WebAuthn erlaubt.
pub const ID_HOECHSTENS: usize = 1023;
/// RSA-Schlüssel: mindestens 2048 Bit (kleiner gilt als unsicher).
pub const RSA_BITS_MINDESTENS: usize = 2048;

/// Was der Server erwartet: Herkunft und Name der Website.
#[derive(Debug, Clone, Copy)]
pub struct Erwartung<'a> {
    /// Genau `http://localhost:<port>`.
    pub herkunft: &'a str,
    /// `localhost`.
    pub rp_id: &'a str,
}

#[derive(Deserialize)]
struct ClientDaten {
    #[serde(rename = "type")]
    art: String,
    challenge: String,
    origin: String,
    #[serde(rename = "crossOrigin", default)]
    cross_origin: Option<bool>,
    #[serde(rename = "topOrigin", default)]
    top_origin: Option<String>,
}

/// Prüft `clientDataJSON` und gibt die Challenge zurück (Base64url, wie der
/// Browser sie schickt). Ob es diese Challenge gibt, prüft der Aufrufer.
pub fn client_daten_pruefen(roh: &[u8], art: &str, herkunft: &str) -> Result<String, Abweisung> {
    let c: ClientDaten = serde_json::from_slice(roh).map_err(|_| Abweisung::Unlesbar("clientDataJSON"))?;
    if c.art != art {
        return Err(Abweisung::FalscheArt);
    }
    if c.origin != herkunft {
        return Err(Abweisung::FalscheHerkunft);
    }
    if c.cross_origin == Some(true) || c.top_origin.is_some() {
        return Err(Abweisung::FremderRahmen);
    }
    if c.challenge.is_empty() || super::b64_lesen(&c.challenge).is_none() {
        return Err(Abweisung::Unlesbar("challenge"));
    }
    Ok(c.challenge)
}

/// Nur die Challenge aus `clientDataJSON`, ohne weitere Prüfung (damit eine
/// Challenge auch verbraucht wird, wenn die Prüfung danach scheitert).
pub fn challenge_aus(roh: &[u8]) -> Option<String> {
    #[derive(Deserialize)]
    struct NurChallenge {
        challenge: String,
    }
    serde_json::from_slice::<NurChallenge>(roh).ok().map(|c| c.challenge).filter(|c| !c.is_empty())
}

/// Gelesene Authenticator-Daten.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthDaten {
    pub flags: u8,
    pub zaehler: u32,
    /// Nur beim Anlegen: Credential-ID und öffentlicher Schlüssel (COSE, roh).
    pub neu: Option<(Vec<u8>, Vec<u8>)>,
}

/// Liest Authenticator-Daten und prüft rpIdHash, UP und UV. `anlegen`: Es
/// müssen Credential-Daten folgen (Flag AT); beim Anmelden dürfen keine
/// folgen. Erweiterungen (Flag ED) müssen genau eine CBOR-Karte sein; ohne
/// ED darf nichts übrig bleiben.
pub fn auth_daten_lesen(b: &[u8], rp_id: &str, anlegen: bool) -> Result<AuthDaten, Abweisung> {
    if b.len() < 37 {
        return Err(Abweisung::Unlesbar("authenticatorData zu kurz"));
    }
    if b[..32] != Sha256::digest(rp_id.as_bytes())[..] {
        return Err(Abweisung::FalscheWebsite);
    }
    let flags = b[32];
    if flags & FLAG_UP == 0 {
        return Err(Abweisung::OhneAnwesenheit);
    }
    if flags & FLAG_UV == 0 {
        return Err(Abweisung::OhneNutzerpruefung);
    }
    let zaehler = u32::from_be_bytes([b[33], b[34], b[35], b[36]]);
    let mut rest = &b[37..];
    let neu = if anlegen {
        if flags & FLAG_AT == 0 {
            return Err(Abweisung::Unlesbar("Credential-Daten fehlen"));
        }
        // AAGUID (16) und Länge der ID (2).
        if rest.len() < 18 {
            return Err(Abweisung::Unlesbar("Credential-Daten zu kurz"));
        }
        let laenge = usize::from(u16::from_be_bytes([rest[16], rest[17]]));
        rest = &rest[18..];
        if laenge == 0 || laenge > ID_HOECHSTENS || rest.len() < laenge {
            return Err(Abweisung::Unlesbar("Credential-ID"));
        }
        let id = rest[..laenge].to_vec();
        rest = &rest[laenge..];
        let (_, ende) = cbor_eins(rest).ok_or(Abweisung::Unlesbar("öffentlicher Schlüssel"))?;
        let schluessel = rest[..ende].to_vec();
        rest = &rest[ende..];
        Some((id, schluessel))
    } else {
        if flags & FLAG_AT != 0 {
            return Err(Abweisung::Unlesbar("Credential-Daten beim Anmelden"));
        }
        None
    };
    if flags & FLAG_ED != 0 {
        match cbor_eins(rest) {
            Some((Value::Map(_), ende)) if ende == rest.len() => {}
            _ => return Err(Abweisung::Unlesbar("Erweiterungen")),
        }
    } else if !rest.is_empty() {
        return Err(Abweisung::Unlesbar("Daten übrig"));
    }
    Ok(AuthDaten { flags, zaehler, neu })
}

/// Ein CBOR-Wert vom Anfang von `b` und wie viele Bytes er belegt.
fn cbor_eins(b: &[u8]) -> Option<(Value, usize)> {
    let mut c = std::io::Cursor::new(b);
    let wert: Value = ciborium::de::from_reader(&mut c).ok()?;
    Some((wert, usize::try_from(c.position()).ok()?))
}

/// Ein öffentlicher Schlüssel eines Passkeys.
#[derive(Debug, Clone)]
pub enum Schluessel {
    Es256(p256::ecdsa::VerifyingKey),
    Rs256(rsa::RsaPublicKey),
}

impl Schluessel {
    pub fn verfahren(&self) -> i64 {
        match self {
            Self::Es256(_) => ES256,
            Self::Rs256(_) => RS256,
        }
    }
}

fn ganzzahl(v: &Value) -> Option<i128> {
    match v {
        Value::Integer(i) => Some(i128::from(*i)),
        _ => None,
    }
}

/// Liest einen COSE-Schlüssel (CBOR-Karte mit Zahlen als Schlüsseln). Nur
/// EC2/P-256 mit ES256 und RSA mit RS256 (mindestens 2048 Bit).
pub fn cose_lesen(roh: &[u8]) -> Result<Schluessel, Abweisung> {
    let (wert, ende) = cbor_eins(roh).ok_or(Abweisung::Unlesbar("COSE"))?;
    if ende != roh.len() {
        return Err(Abweisung::Unlesbar("COSE: Daten übrig"));
    }
    let Value::Map(karte) = wert else { return Err(Abweisung::Unlesbar("COSE ist keine Karte")) };
    let mut felder: Vec<(i128, &Value)> = Vec::with_capacity(karte.len());
    for (k, v) in &karte {
        let k = ganzzahl(k).ok_or(Abweisung::Unlesbar("COSE-Schlüssel"))?;
        if felder.iter().any(|(a, _)| *a == k) {
            return Err(Abweisung::Unlesbar("COSE doppelt"));
        }
        felder.push((k, v));
    }
    let feld = |k: i128| felder.iter().find(|(a, _)| *a == k).map(|(_, v)| *v);
    let bytes = |k: i128| match feld(k) {
        Some(Value::Bytes(b)) => Some(b.as_slice()),
        _ => None,
    };
    let kty = feld(1).and_then(ganzzahl);
    let alg = feld(3).and_then(ganzzahl);
    match (kty, alg) {
        (Some(2), Some(a)) if a == i128::from(ES256) => {
            if feld(-1).and_then(ganzzahl) != Some(1) {
                return Err(Abweisung::Verfahren);
            }
            let (Some(x), Some(y)) = (bytes(-2), bytes(-3)) else { return Err(Abweisung::Schluessel) };
            if x.len() != 32 || y.len() != 32 {
                return Err(Abweisung::Schluessel);
            }
            let mut punkt = Vec::with_capacity(65);
            punkt.push(0x04);
            punkt.extend_from_slice(x);
            punkt.extend_from_slice(y);
            let k = p256::ecdsa::VerifyingKey::from_sec1_bytes(&punkt).map_err(|_| Abweisung::Schluessel)?;
            Ok(Schluessel::Es256(k))
        }
        (Some(3), Some(a)) if a == i128::from(RS256) => {
            let (Some(n), Some(e)) = (bytes(-1), bytes(-2)) else { return Err(Abweisung::Schluessel) };
            let n = rsa::BigUint::from_bytes_be(n);
            let e = rsa::BigUint::from_bytes_be(e);
            if n.bits() < RSA_BITS_MINDESTENS {
                return Err(Abweisung::Schluessel);
            }
            let k = rsa::RsaPublicKey::new(n, e).map_err(|_| Abweisung::Schluessel)?;
            Ok(Schluessel::Rs256(k))
        }
        _ => Err(Abweisung::Verfahren),
    }
}

/// Prüft eine Signatur über `nachricht` mit dem Schlüssel.
pub fn signatur_pruefen(schluessel: &Schluessel, nachricht: &[u8], signatur: &[u8]) -> Result<(), Abweisung> {
    match schluessel {
        Schluessel::Es256(k) => {
            use p256::ecdsa::signature::Verifier;
            let s = p256::ecdsa::Signature::from_der(signatur).map_err(|_| Abweisung::Signatur)?;
            k.verify(nachricht, &s).map_err(|_| Abweisung::Signatur)
        }
        Schluessel::Rs256(k) => {
            use rsa::signature::Verifier;
            let s = rsa::pkcs1v15::Signature::try_from(signatur).map_err(|_| Abweisung::Signatur)?;
            let v = rsa::pkcs1v15::VerifyingKey::<sha2_010::Sha256>::new(k.clone());
            v.verify(nachricht, &s).map_err(|_| Abweisung::Signatur)
        }
    }
}

/// Ein geprüfter neuer Passkey (Antwort auf `navigator.credentials.create`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NeuerPasskey {
    pub id: Vec<u8>,
    /// Öffentlicher Schlüssel, COSE wie vom Authenticator.
    pub cose: Vec<u8>,
    pub verfahren: i64,
    pub zaehler: u32,
    /// Die Challenge aus `clientDataJSON` (Base64url), für den Datenspeicher.
    pub challenge: String,
}

/// Prüft die Antwort auf `navigator.credentials.create`: `clientDataJSON` und
/// `attestationObject` (CBOR: `fmt`, `attStmt`, `authData`). Nur `fmt` none
/// mit leerem `attStmt`. `roh_id` ist die ID, die der Browser daneben
/// schickt; sie muss zur ID in den Authenticator-Daten passen.
pub fn anlegen_pruefen(client_daten: &[u8], attestation: &[u8], roh_id: &[u8], e: Erwartung<'_>) -> Result<NeuerPasskey, Abweisung> {
    let challenge = client_daten_pruefen(client_daten, "webauthn.create", e.herkunft)?;
    let (wert, ende) = cbor_eins(attestation).ok_or(Abweisung::Unlesbar("attestationObject"))?;
    if ende != attestation.len() {
        return Err(Abweisung::Unlesbar("attestationObject: Daten übrig"));
    }
    let Value::Map(karte) = wert else { return Err(Abweisung::Unlesbar("attestationObject ist keine Karte")) };
    let mut fmt = None;
    let mut stmt = None;
    let mut auth = None;
    for (k, v) in karte {
        let Value::Text(k) = k else { continue };
        let ziel = match k.as_str() {
            "fmt" => &mut fmt,
            "attStmt" => &mut stmt,
            "authData" => &mut auth,
            _ => continue,
        };
        if ziel.replace(v).is_some() {
            return Err(Abweisung::Unlesbar("attestationObject doppelt"));
        }
    }
    match (fmt, stmt) {
        (Some(Value::Text(f)), Some(Value::Map(s))) if f == "none" && s.is_empty() => {}
        (Some(_), Some(_)) => return Err(Abweisung::Attestation),
        _ => return Err(Abweisung::Unlesbar("attestationObject unvollständig")),
    }
    let Some(Value::Bytes(auth)) = auth else { return Err(Abweisung::Unlesbar("authData")) };
    let daten = auth_daten_lesen(&auth, e.rp_id, true)?;
    let Some((id, cose)) = daten.neu else { return Err(Abweisung::Unlesbar("Credential-Daten fehlen")) };
    if id != roh_id {
        return Err(Abweisung::Unlesbar("Credential-ID passt nicht"));
    }
    let schluessel = cose_lesen(&cose)?;
    Ok(NeuerPasskey { id, cose, verfahren: schluessel.verfahren(), zaehler: daten.zaehler, challenge })
}

/// Der Zähler muss steigen, wenn einer von beiden nicht 0 ist (Authenticatoren
/// ohne Zähler schicken immer 0). Ein fallender oder gleicher Zähler deutet
/// auf einen kopierten Passkey.
pub fn zaehler_pruefen(alt: u32, neu: u32) -> Result<(), Abweisung> {
    if (alt == 0 && neu == 0) || neu > alt { Ok(()) } else { Err(Abweisung::ZaehlerFaellt) }
}

/// Prüft die Antwort auf `navigator.credentials.get` gegen den gespeicherten
/// Schlüssel (COSE) und Zähler. Gibt den neuen Zähler zurück. Die Challenge
/// aus `clientDataJSON` muss der Aufrufer vorher gegen den Datenspeicher
/// geprüft haben (`client_daten_pruefen`).
pub fn anmelden_pruefen(client_daten: &[u8], auth_daten: &[u8], signatur: &[u8], cose: &[u8], zaehler_alt: u32, e: Erwartung<'_>) -> Result<u32, Abweisung> {
    client_daten_pruefen(client_daten, "webauthn.get", e.herkunft)?;
    let daten = auth_daten_lesen(auth_daten, e.rp_id, false)?;
    let schluessel = cose_lesen(cose)?;
    let mut nachricht = Vec::with_capacity(auth_daten.len() + 32);
    nachricht.extend_from_slice(auth_daten);
    nachricht.extend_from_slice(&Sha256::digest(client_daten));
    signatur_pruefen(&schluessel, &nachricht, signatur)?;
    zaehler_pruefen(zaehler_alt, daten.zaehler)?;
    Ok(daten.zaehler)
}

/// Ein Authenticator nur für Tests: legt Passkeys mit ES256 oder RS256 an und
/// meldet sich damit an, so wie es der Browser täte. Flags und Zähler lassen
/// sich verstellen, um jede Abweisung zu zeigen.
#[cfg(test)]
pub mod pruef_authenticator {
    use super::*;
    use rand_core::OsRng;

    /// Fester RSA-Schlüssel (2048 Bit, PKCS#1 DER, nur für Tests): ein neuer
    /// zu erzeugen dauert ohne Optimierung zu lange.
    const RSA_PKCS1: [&str; 17] = [
        "MIIEpAIBAAKCAQEA1bPYSVMk/kPU4PC5o6soHezv+pc9Gs3XE3d850tGm5NXy6LZOHzM9sgudkHJSy/AlDNxuAqHYas4MaT+",
        "ktC7ZnpnuMpcIQ0fCUP0M/hWMvslWM91Vip8xpYbrhdkpp1UUEY18OBs7iq0ClbB3jP3PgLMfJVOR9NOT95/owJkx7FrMWbI",
        "z/m+BCUt+J1ZxWokSiqezcT/PZRkGEumtMu7uvECIxgnCY2yiVvHDI3RwFbTauHBzgfUD4/ithNh7td4PlaVMGNl691zzV7v",
        "GP9JAKq3KQeCphlmOnQrZs5WLnow6FGHrIXue/Ok2LAIm+K5+CHIHaBx/ofWTNtabS7WLwIDAQABAoIBAFHTHlYZEVmQFGMa",
        "mSILZ31p2ws6vGlWdmscPcV8A6mPFbdBf+XKl1pIlM2v68ivnI/CUOEUmvixK/+KOvVxGcgnxy4wwWzgTsln7Qm3JR45Hngu",
        "WzGmbAdHs4qJp37KT926vG2iKONak3w/8tRHVnw2ayCqtIH47QvU2E9Mm9T/rPhPkvaQPZMJYTdS/KVAJC6sIrYoq3/YUO5S",
        "jv2YqY3kiESbyxFGWCfJG45uwoh+ML/Qnad4VInZbDE5YO0VC64S3anl/37Mr3JzZ9xlba8kw4eUem1S6WTZ8dg3UjXT2zKV",
        "xifMgDvGyqExp+AXSdRfNjCR9vSKMoLqxl0yJjkCgYEA9t9kUR1yGJ1J1w2wD0eacqDt/1YKSC6kQcF8DmRm+TXHOtrwtphX",
        "wKx2900FVXFj4iyYDbAJXWl1NEN1tMmcCX2qtF8ai0FukxF6nEgEcMl8u/aOpd5LMGQijoxPCrKap3Fu2zODoJe8ASkhAXvn",
        "JrOsCmVbEnBM7kpaawAt0icCgYEA3ZqA5W/uHfwZnTYfZcyjVkj5W7WJ1228Rnzo6arFKm41SiJmySlxD2FtqnCKyVRqNvpc",
        "PEz73fS8P2TfkYt0dp9D1lzvnVbkuiD32gyFHU6K2MxSKrBbs9uezEE2fCuw7KrwzlEKq2DbrlD1jPHzSC9wSFBo1vfX2Ima",
        "kISxSLkCgYEAlur56JyXo3SEL3OuraiYQnsfA3kV0AqzX39o12OJj8/Qe6pcfUOkYaUNFSGAoTswVJPuYG+9Sd2uSMT0sBCP",
        "0z1/ncSF9ZuKfV2yFtfv3wNNvZp8menLDUop1frwVxnYcYRmwJo9XW9k/1ClXMLiDEbKZZCQnDUK0qGhpMf3OwECgYEAqT1t",
        "AUypWwcEfNcvxWwythwHLG4Jdphc+Kw+qO39pDkEw7hgLXNT4D41JbzLkZTBhXlQ5dCZblqRRg0A3UPWzx01DsptSQlqp2e0",
        "QjfblZU9XvM0MVv8vPCWZ0HKnaw0c5rdS4QDFP1k6Mz1qw3BEbkBFIkseDhSBRKaE5LzXikCgYBlHwH/NC5il9GcqRMiw4s/",
        "oTiZthjSU0Zn1JOFOdKD+X33DD5Io04DvdMzqRT+nh4JKkROCw+HyuWH7qUlY6lM3JJRN07uzlascNv47Eg5jwRG7kh/oJwz",
        "VRFgdRD1ejtT0Eql74yqZA1/jd2INWKBpgwUqZbsLRdRKc10hlDQYw==",
    ];

    enum Privat {
        Es256(p256::ecdsa::SigningKey),
        Rs256(Box<rsa::RsaPrivateKey>),
    }

    /// Ein Authenticator mit genau einem Passkey.
    pub struct PruefAuthenticator {
        privat: Privat,
        pub id: Vec<u8>,
        pub zaehler: u32,
        /// Flags der Authenticator-Daten (Standard UP und UV).
        pub flags: u8,
        /// Für `fmt` in attestationObject (Standard `none`).
        pub fmt: &'static str,
        /// Für `type` in clientDataJSON beim Anmelden (Standard `webauthn.get`).
        pub art_anmelden: &'static str,
        pub rp_id: &'static str,
    }

    impl PruefAuthenticator {
        pub fn es256() -> Self {
            Self::mit(Privat::Es256(p256::ecdsa::SigningKey::random(&mut OsRng)))
        }

        pub fn rs256() -> Self {
            use base64::Engine;
            use rsa::pkcs1::DecodeRsaPrivateKey;
            let der = base64::engine::general_purpose::STANDARD.decode(RSA_PKCS1.concat()).unwrap();
            Self::mit(Privat::Rs256(Box::new(rsa::RsaPrivateKey::from_pkcs1_der(&der).unwrap())))
        }

        fn mit(privat: Privat) -> Self {
            Self { privat, id: crate::passkey::zufall(16), zaehler: 0, flags: FLAG_UP | FLAG_UV, fmt: "none", art_anmelden: "webauthn.get", rp_id: "localhost" }
        }

        pub fn id_b64(&self) -> String {
            crate::passkey::b64(&self.id)
        }

        /// Öffentlicher Schlüssel als COSE.
        pub fn cose(&self) -> Vec<u8> {
            let karte = match &self.privat {
                Privat::Es256(k) => {
                    let p = k.verifying_key().to_encoded_point(false);
                    vec![
                        (Value::from(1), Value::from(2)),
                        (Value::from(3), Value::from(ES256)),
                        (Value::from(-1), Value::from(1)),
                        (Value::from(-2), Value::Bytes(p.x().unwrap().to_vec())),
                        (Value::from(-3), Value::Bytes(p.y().unwrap().to_vec())),
                    ]
                }
                Privat::Rs256(k) => {
                    use rsa::traits::PublicKeyParts;
                    vec![
                        (Value::from(1), Value::from(3)),
                        (Value::from(3), Value::from(RS256)),
                        (Value::from(-1), Value::Bytes(k.n().to_bytes_be())),
                        (Value::from(-2), Value::Bytes(k.e().to_bytes_be())),
                    ]
                }
            };
            cbor(&Value::Map(karte))
        }

        pub fn client_daten(art: &str, challenge: &str, herkunft: &str) -> Vec<u8> {
            serde_json::to_vec(&serde_json::json!({"type": art, "challenge": challenge, "origin": herkunft, "crossOrigin": false})).unwrap()
        }

        fn auth_kopf(&self) -> Vec<u8> {
            let mut a = Sha256::digest(self.rp_id.as_bytes()).to_vec();
            a.push(self.flags);
            a.extend_from_slice(&self.zaehler.to_be_bytes());
            a
        }

        /// `(clientDataJSON, attestationObject)` für `navigator.credentials.create`.
        pub fn anlegen(&self, challenge: &str, herkunft: &str) -> (Vec<u8>, Vec<u8>) {
            let mut auth = self.auth_kopf();
            auth[32] |= FLAG_AT;
            auth.extend_from_slice(&[0u8; 16]);
            auth.extend_from_slice(&(self.id.len() as u16).to_be_bytes());
            auth.extend_from_slice(&self.id);
            auth.extend_from_slice(&self.cose());
            let att = Value::Map(vec![
                (Value::from("fmt"), Value::from(self.fmt)),
                (Value::from("attStmt"), Value::Map(vec![])),
                (Value::from("authData"), Value::Bytes(auth)),
            ]);
            (Self::client_daten("webauthn.create", challenge, herkunft), cbor(&att))
        }

        /// `(clientDataJSON, authenticatorData, signature)` für
        /// `navigator.credentials.get`; zählt vorher hoch (außer bei Zähler 0).
        pub fn anmelden(&mut self, challenge: &str, herkunft: &str) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
            if self.zaehler != 0 {
                self.zaehler += 1;
            }
            let client = Self::client_daten(self.art_anmelden, challenge, herkunft);
            let auth = self.auth_kopf();
            let sig = self.signieren(&auth, &client);
            (client, auth, sig)
        }

        pub fn signieren(&self, auth: &[u8], client: &[u8]) -> Vec<u8> {
            let mut nachricht = auth.to_vec();
            nachricht.extend_from_slice(&Sha256::digest(client));
            match &self.privat {
                Privat::Es256(k) => {
                    use p256::ecdsa::signature::Signer;
                    let s: p256::ecdsa::Signature = k.sign(&nachricht);
                    s.to_der().as_bytes().to_vec()
                }
                Privat::Rs256(k) => {
                    use rsa::signature::{SignatureEncoding, Signer};
                    let sk = rsa::pkcs1v15::SigningKey::<sha2_010::Sha256>::new((**k).clone());
                    sk.sign(&nachricht).to_vec()
                }
            }
        }
    }

    pub fn cbor(v: &Value) -> Vec<u8> {
        let mut out = Vec::new();
        ciborium::ser::into_writer(v, &mut out).unwrap();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::pruef_authenticator::{PruefAuthenticator, cbor};
    use super::*;

    const HERKUNFT: &str = "http://localhost:8787";
    const E: Erwartung<'static> = Erwartung { herkunft: HERKUNFT, rp_id: "localhost" };
    const CH: &str = "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8";

    fn angelegt(a: &PruefAuthenticator) -> NeuerPasskey {
        let (c, att) = a.anlegen(CH, HERKUNFT);
        anlegen_pruefen(&c, &att, &a.id, E).unwrap()
    }

    #[test]
    fn es256_und_rs256_anlegen_und_anmelden() {
        for mut a in [PruefAuthenticator::es256(), PruefAuthenticator::rs256()] {
            a.zaehler = 5;
            let neu = angelegt(&a);
            assert_eq!(neu.challenge, CH);
            assert_eq!(neu.zaehler, 5);
            assert_eq!(neu.id, a.id);
            let (c, auth, sig) = a.anmelden(CH, HERKUNFT);
            assert_eq!(anmelden_pruefen(&c, &auth, &sig, &neu.cose, neu.zaehler, E), Ok(6));
        }
        assert_eq!(angelegt(&PruefAuthenticator::es256()).verfahren, ES256);
        assert_eq!(angelegt(&PruefAuthenticator::rs256()).verfahren, RS256);
    }

    #[test]
    fn falsche_herkunft_wird_abgewiesen() {
        let a = PruefAuthenticator::es256();
        for falsch in ["http://127.0.0.1:8787", "http://localhost:8788", "https://localhost:8787", "http://localhost:8787/", "http://evil.localhost:8787"] {
            let (c, att) = a.anlegen(CH, falsch);
            assert_eq!(anlegen_pruefen(&c, &att, &a.id, E), Err(Abweisung::FalscheHerkunft), "{falsch}");
        }
        let neu = angelegt(&a);
        let mut a = a;
        let (c, auth, sig) = a.anmelden(CH, "http://127.0.0.1:8787");
        assert_eq!(anmelden_pruefen(&c, &auth, &sig, &neu.cose, 0, E), Err(Abweisung::FalscheHerkunft));
    }

    #[test]
    fn fremder_rahmen_und_falsche_art_werden_abgewiesen() {
        let c = serde_json::to_vec(&serde_json::json!({"type": "webauthn.get", "challenge": CH, "origin": HERKUNFT, "crossOrigin": true})).unwrap();
        assert_eq!(client_daten_pruefen(&c, "webauthn.get", HERKUNFT), Err(Abweisung::FremderRahmen));
        let c = serde_json::to_vec(&serde_json::json!({"type": "webauthn.get", "challenge": CH, "origin": HERKUNFT, "topOrigin": "http://evil"})).unwrap();
        assert_eq!(client_daten_pruefen(&c, "webauthn.get", HERKUNFT), Err(Abweisung::FremderRahmen));
        let c = PruefAuthenticator::client_daten("webauthn.create", CH, HERKUNFT);
        assert_eq!(client_daten_pruefen(&c, "webauthn.get", HERKUNFT), Err(Abweisung::FalscheArt));
        assert!(client_daten_pruefen(b"kein json", "webauthn.get", HERKUNFT).is_err());
        let c = PruefAuthenticator::client_daten("webauthn.get", "nicht+base64/", HERKUNFT);
        assert_eq!(client_daten_pruefen(&c, "webauthn.get", HERKUNFT), Err(Abweisung::Unlesbar("challenge")));
    }

    #[test]
    fn ohne_uv_oder_up_wird_abgewiesen() {
        let mut a = PruefAuthenticator::es256();
        let neu = angelegt(&a);
        a.flags = FLAG_UP;
        let (c, att) = a.anlegen(CH, HERKUNFT);
        assert_eq!(anlegen_pruefen(&c, &att, &a.id, E), Err(Abweisung::OhneNutzerpruefung));
        let (c, auth, sig) = a.anmelden(CH, HERKUNFT);
        assert_eq!(anmelden_pruefen(&c, &auth, &sig, &neu.cose, 0, E), Err(Abweisung::OhneNutzerpruefung));
        a.flags = FLAG_UV;
        let (c, auth, sig) = a.anmelden(CH, HERKUNFT);
        assert_eq!(anmelden_pruefen(&c, &auth, &sig, &neu.cose, 0, E), Err(Abweisung::OhneAnwesenheit));
    }

    #[test]
    fn falsche_website_wird_abgewiesen() {
        let mut a = PruefAuthenticator::es256();
        a.rp_id = "evil.example";
        let (c, att) = a.anlegen(CH, HERKUNFT);
        assert_eq!(anlegen_pruefen(&c, &att, &a.id, E), Err(Abweisung::FalscheWebsite));
    }

    #[test]
    fn nur_attestation_none() {
        let mut a = PruefAuthenticator::es256();
        a.fmt = "packed";
        let (c, att) = a.anlegen(CH, HERKUNFT);
        assert_eq!(anlegen_pruefen(&c, &att, &a.id, E), Err(Abweisung::Attestation));
        // none mit nicht leerem attStmt
        let (c, _) = PruefAuthenticator::es256().anlegen(CH, HERKUNFT);
        let att = cbor(&Value::Map(vec![
            (Value::from("fmt"), Value::from("none")),
            (Value::from("attStmt"), Value::Map(vec![(Value::from("x5c"), Value::Array(vec![]))])),
            (Value::from("authData"), Value::Bytes(vec![0; 40])),
        ]));
        assert_eq!(anlegen_pruefen(&c, &att, &a.id, E), Err(Abweisung::Attestation));
    }

    #[test]
    fn nur_es256_und_rs256() {
        // Ed25519 (alg -8, OKP) wird nicht angenommen.
        let eddsa = cbor(&Value::Map(vec![
            (Value::from(1), Value::from(1)),
            (Value::from(3), Value::from(-8)),
            (Value::from(-1), Value::from(6)),
            (Value::from(-2), Value::Bytes(vec![1; 32])),
        ]));
        assert_eq!(cose_lesen(&eddsa).err(), Some(Abweisung::Verfahren));
        // ES256-Kennung mit RSA-Typ.
        let falsch = cbor(&Value::Map(vec![(Value::from(1), Value::from(3)), (Value::from(3), Value::from(ES256))]));
        assert_eq!(cose_lesen(&falsch).err(), Some(Abweisung::Verfahren));
        // Ein Punkt, der nicht auf der Kurve liegt.
        let kaputt = cbor(&Value::Map(vec![
            (Value::from(1), Value::from(2)),
            (Value::from(3), Value::from(ES256)),
            (Value::from(-1), Value::from(1)),
            (Value::from(-2), Value::Bytes(vec![1; 32])),
            (Value::from(-3), Value::Bytes(vec![2; 32])),
        ]));
        assert_eq!(cose_lesen(&kaputt).err(), Some(Abweisung::Schluessel));
        // RSA unter 2048 Bit.
        let klein = cbor(&Value::Map(vec![
            (Value::from(1), Value::from(3)),
            (Value::from(3), Value::from(RS256)),
            (Value::from(-1), Value::Bytes(vec![0xff; 128])),
            (Value::from(-2), Value::Bytes(vec![1, 0, 1])),
        ]));
        assert_eq!(cose_lesen(&klein).err(), Some(Abweisung::Schluessel));
    }

    #[test]
    fn zaehler_muss_steigen() {
        assert_eq!(zaehler_pruefen(0, 0), Ok(()));
        assert_eq!(zaehler_pruefen(0, 1), Ok(()));
        assert_eq!(zaehler_pruefen(5, 6), Ok(()));
        assert_eq!(zaehler_pruefen(5, 5), Err(Abweisung::ZaehlerFaellt));
        assert_eq!(zaehler_pruefen(5, 4), Err(Abweisung::ZaehlerFaellt));
        assert_eq!(zaehler_pruefen(5, 0), Err(Abweisung::ZaehlerFaellt));
        let mut a = PruefAuthenticator::es256();
        a.zaehler = 10;
        let neu = angelegt(&a);
        a.zaehler = 3;
        let (c, auth, sig) = a.anmelden(CH, HERKUNFT);
        assert_eq!(anmelden_pruefen(&c, &auth, &sig, &neu.cose, neu.zaehler, E), Err(Abweisung::ZaehlerFaellt));
    }

    #[test]
    fn fremder_passkey_und_manipulierte_signatur() {
        let a = PruefAuthenticator::es256();
        let neu = angelegt(&a);
        // Ein anderer Authenticator signiert: der gespeicherte Schlüssel passt nicht.
        let mut fremd = PruefAuthenticator::es256();
        let (c, auth, sig) = fremd.anmelden(CH, HERKUNFT);
        assert_eq!(anmelden_pruefen(&c, &auth, &sig, &neu.cose, 0, E), Err(Abweisung::Signatur));
        let mut fremd = PruefAuthenticator::rs256();
        let (c, auth, sig) = fremd.anmelden(CH, HERKUNFT);
        assert_eq!(anmelden_pruefen(&c, &auth, &sig, &neu.cose, 0, E), Err(Abweisung::Signatur));
        // Geänderte Client-Daten nach dem Signieren.
        let mut a = a;
        let (_, auth, sig) = a.anmelden(CH, HERKUNFT);
        let anders = PruefAuthenticator::client_daten("webauthn.get", "BBECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8", HERKUNFT);
        assert_eq!(anmelden_pruefen(&anders, &auth, &sig, &neu.cose, 0, E), Err(Abweisung::Signatur));
        // Ein Bit der Signatur gekippt.
        let (c, auth, mut sig) = a.anmelden(CH, HERKUNFT);
        let n = sig.len();
        sig[n - 1] ^= 1;
        assert_eq!(anmelden_pruefen(&c, &auth, &sig, &neu.cose, 0, E), Err(Abweisung::Signatur));
    }

    #[test]
    fn credential_id_muss_passen_und_nichts_darf_uebrig_bleiben() {
        let a = PruefAuthenticator::es256();
        let (c, att) = a.anlegen(CH, HERKUNFT);
        assert!(anlegen_pruefen(&c, &att, b"andere", E).is_err());
        let mut mehr = att.clone();
        mehr.push(0);
        assert!(anlegen_pruefen(&c, &mehr, &a.id, E).is_err());
        let mut b = PruefAuthenticator::es256();
        let neu = angelegt(&b);
        let (c, mut auth, _) = b.anmelden(CH, HERKUNFT);
        auth.push(0);
        let sig = b.signieren(&auth, &c);
        assert_eq!(anmelden_pruefen(&c, &auth, &sig, &neu.cose, 0, E), Err(Abweisung::Unlesbar("Daten übrig")));
    }
}
