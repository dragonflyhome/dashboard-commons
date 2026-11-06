//! Der Ausweis nach einer Anmeldung mit Passkey: ein JWT, signiert mit
//! Ed25519 (`[passkey]`, bauplan; jsonwebtoken scheidet aus, es zieht ring
//! oder aws-lc, beide brauchen C).
//!
//! Inhalt: `sub` inhaber, `aud` dashboard, `iat`, `exp`, `jti`, `amr`
//! [passkey], `cid` (Kennung des Passkeys) und `auth_time` (wann die Anmeldung
//! mit Passkey war). Ein Ausweis gilt 15 Minuten und wird bei Nutzung erneuert,
//! höchstens bis 8 Stunden nach `auth_time`; beim Erneuern bleibt die `jti`
//! gleich, so beendet eine gesperrte `jti` die ganze Sitzung.
//!
//! Angenommen wird nur `alg` EdDSA mit genau diesem Schlüssel: kein `none`,
//! kein HS256, kein Wechsel des Verfahrens, keine Kopfzeile `crit`. Ob die
//! `jti` gesperrt ist und der Passkey noch registriert, prüft die Schleuse
//! mit dem Datenspeicher.

use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};

use super::{AUSWEIS_SEKUNDEN, SITZUNG_HOECHSTENS_SEKUNDEN, b64, b64_lesen};

/// `sub`: es gibt nur eine Person.
pub const SUB: &str = "inhaber";
/// `aud`: nur dieses Dashboard.
pub const AUD: &str = "dashboard";
/// `amr`: angemeldet mit Passkey.
pub const AMR: &str = "passkey";
/// Kopf jedes Ausweises.
const KOPF: &str = r#"{"alg":"EdDSA","typ":"JWT"}"#;
/// Längster Ausweis, der überhaupt gelesen wird.
pub const LAENGE_HOECHSTENS: usize = 2048;
/// So viel darf die Uhr bei `iat` vorgehen.
const UHR_SPIEL: i64 = 60;

/// Der Inhalt eines Ausweises.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ausweis {
    pub sub: String,
    pub aud: String,
    pub iat: i64,
    pub exp: i64,
    pub jti: String,
    pub amr: Vec<String>,
    /// Kennung des Passkeys (Credential-ID, Base64url).
    pub cid: String,
    /// Wann die Anmeldung mit Passkey war (Sekunden seit 1970).
    pub auth_time: i64,
}

impl Ausweis {
    /// Ein neuer Ausweis direkt nach der Anmeldung mit dem Passkey `cid`.
    pub fn neu(cid: &str, jetzt: i64) -> Self {
        Self {
            sub: SUB.into(),
            aud: AUD.into(),
            iat: jetzt,
            exp: jetzt + AUSWEIS_SEKUNDEN,
            jti: b64(&super::zufall(16)),
            amr: vec![AMR.into()],
            cid: cid.into(),
            auth_time: jetzt,
        }
    }

    /// Derselbe Ausweis, erneuert: neue `iat` und `exp`, höchstens bis 8
    /// Stunden nach der Anmeldung; `jti`, `cid` und `auth_time` bleiben.
    pub fn erneuert(&self, jetzt: i64) -> Self {
        Self { sub: SUB.into(), iat: jetzt, exp: (jetzt + AUSWEIS_SEKUNDEN).min(self.sitzung_ende()), ..self.clone() }
    }

    /// Spätestes Ende der Sitzung: 8 Stunden nach der Anmeldung.
    pub fn sitzung_ende(&self) -> i64 {
        self.auth_time + SITZUNG_HOECHSTENS_SEKUNDEN
    }
}

/// Warum ein Ausweis nicht gilt (fürs Protokoll).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum JwtFehler {
    #[error("Form falsch")]
    Form,
    #[error("Verfahren nicht EdDSA")]
    Verfahren,
    #[error("Signatur falsch")]
    Signatur,
    #[error("Inhalt falsch")]
    Inhalt,
    #[error("abgelaufen")]
    Abgelaufen,
}

/// Der Schlüssel des Dienstes für die Ausweise.
pub struct JwtSchluessel {
    signieren: SigningKey,
    pruefen: VerifyingKey,
}

impl std::fmt::Debug for JwtSchluessel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("JwtSchluessel(verborgen)")
    }
}

impl JwtSchluessel {
    /// Aus den 32 Bytes der Schlüsseldatei.
    pub fn aus_bytes(bytes: &[u8; 32]) -> Self {
        let signieren = SigningKey::from_bytes(bytes);
        Self { pruefen: signieren.verifying_key(), signieren }
    }

    /// Ein neuer zufälliger Schlüssel (Tests; im Betrieb `geheim::laden_oder_anlegen`).
    pub fn zufaellig() -> Self {
        let mut b = [0u8; 32];
        b.copy_from_slice(&super::zufall(32));
        Self::aus_bytes(&b)
    }

    /// Stellt den Ausweis aus: `<kopf>.<inhalt>.<signatur>`, je Base64url.
    pub fn ausstellen(&self, a: &Ausweis) -> String {
        let inhalt = serde_json::to_vec(a).unwrap_or_default();
        let teil = format!("{}.{}", b64(KOPF.as_bytes()), b64(&inhalt));
        let sig = self.signieren.sign(teil.as_bytes());
        format!("{teil}.{}", b64(&sig.to_bytes()))
    }

    /// Prüft Form, Kopf (nur EdDSA), Signatur und Inhalt zur Zeit `jetzt`
    /// (Sekunden seit 1970).
    pub fn pruefen(&self, token: &str, jetzt: i64) -> Result<Ausweis, JwtFehler> {
        if token.len() > LAENGE_HOECHSTENS {
            return Err(JwtFehler::Form);
        }
        let mut teile = token.split('.');
        let (Some(kopf), Some(inhalt), Some(sig), None) = (teile.next(), teile.next(), teile.next(), teile.next()) else {
            return Err(JwtFehler::Form);
        };
        let kopf_json = b64_lesen(kopf).ok_or(JwtFehler::Form)?;
        let kopf_wert: serde_json::Map<String, serde_json::Value> = serde_json::from_slice(&kopf_json).map_err(|_| JwtFehler::Form)?;
        if kopf_wert.get("alg").and_then(|a| a.as_str()) != Some("EdDSA") {
            return Err(JwtFehler::Verfahren);
        }
        // Nur alg und typ; alles andere (crit, kid, jwk, jku …) wird abgewiesen.
        if kopf_wert.keys().any(|k| k != "alg" && k != "typ") || kopf_wert.get("typ").is_some_and(|t| t.as_str() != Some("JWT")) {
            return Err(JwtFehler::Form);
        }
        let sig = b64_lesen(sig).ok_or(JwtFehler::Form)?;
        let sig: [u8; 64] = sig.try_into().map_err(|_| JwtFehler::Signatur)?;
        let teil = &token[..kopf.len() + 1 + inhalt.len()];
        self.pruefen.verify_strict(teil.as_bytes(), &Signature::from_bytes(&sig)).map_err(|_| JwtFehler::Signatur)?;
        let a: Ausweis = serde_json::from_slice(&b64_lesen(inhalt).ok_or(JwtFehler::Form)?).map_err(|_| JwtFehler::Inhalt)?;
        let inhalt_ok = a.sub == SUB
            && a.aud == AUD
            && a.amr.iter().any(|m| m == AMR)
            && !a.jti.is_empty()
            && a.jti.len() <= 64
            && !a.cid.is_empty()
            && a.cid.len() <= 1400
            && a.auth_time <= a.iat
            && a.iat < a.exp
            && a.exp - a.iat <= AUSWEIS_SEKUNDEN
            && a.exp <= a.sitzung_ende()
            && a.iat <= jetzt + UHR_SPIEL;
        if !inhalt_ok {
            return Err(JwtFehler::Inhalt);
        }
        if a.exp <= jetzt {
            return Err(JwtFehler::Abgelaufen);
        }
        Ok(a)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const JETZT: i64 = 1_793_000_000;

    fn roh(kopf: &str, inhalt: &serde_json::Value, sig: &[u8]) -> String {
        format!("{}.{}.{}", b64(kopf.as_bytes()), b64(&serde_json::to_vec(inhalt).unwrap()), b64(sig))
    }

    #[test]
    fn ausstellen_und_pruefen() {
        let k = JwtSchluessel::zufaellig();
        let a = Ausweis::neu("cid-1", JETZT);
        let t = k.ausstellen(&a);
        assert_eq!(k.pruefen(&t, JETZT + 10), Ok(a.clone()));
        assert_eq!(t.split('.').count(), 3);
        assert!(!t.contains('='), "Base64url ohne Füllzeichen");
    }

    #[test]
    fn alg_none_und_hs256_werden_abgewiesen() {
        let k = JwtSchluessel::zufaellig();
        let inhalt = serde_json::to_value(Ausweis::neu("c", JETZT)).unwrap();
        // alg none ohne Signatur.
        let t = format!("{}.{}.", b64(br#"{"alg":"none","typ":"JWT"}"#), b64(&serde_json::to_vec(&inhalt).unwrap()));
        assert_eq!(k.pruefen(&t, JETZT), Err(JwtFehler::Verfahren));
        // HS256 mit dem öffentlichen Schlüssel als HMAC-Geheimnis (klassischer Wechsel-Angriff).
        let t = roh(r#"{"alg":"HS256","typ":"JWT"}"#, &inhalt, &[7; 32]);
        assert_eq!(k.pruefen(&t, JETZT), Err(JwtFehler::Verfahren));
        // Groß/Klein zählt.
        let t = roh(r#"{"alg":"eddsa"}"#, &inhalt, &[7; 64]);
        assert_eq!(k.pruefen(&t, JETZT), Err(JwtFehler::Verfahren));
        // Ohne alg.
        let t = roh(r#"{"typ":"JWT"}"#, &inhalt, &[7; 64]);
        assert_eq!(k.pruefen(&t, JETZT), Err(JwtFehler::Verfahren));
        // EdDSA, aber mit crit oder eigenem Schlüssel im Kopf.
        for kopf in [r#"{"alg":"EdDSA","crit":["x"]}"#, r#"{"alg":"EdDSA","jwk":{}}"#, r#"{"alg":"EdDSA","typ":"at+jwt"}"#] {
            let t = roh(kopf, &inhalt, &[7; 64]);
            assert_eq!(k.pruefen(&t, JETZT), Err(JwtFehler::Form), "{kopf}");
        }
    }

    #[test]
    fn manipulierte_signatur_und_fremder_schluessel() {
        let k = JwtSchluessel::zufaellig();
        let t = k.ausstellen(&Ausweis::neu("c", JETZT));
        // Ein Zeichen der Signatur geändert.
        let mut b = t.clone().into_bytes();
        let n = b.len() - 2;
        b[n] = if b[n] == b'A' { b'B' } else { b'A' };
        assert_eq!(k.pruefen(&String::from_utf8(b).unwrap(), JETZT), Err(JwtFehler::Signatur));
        // Inhalt geändert (anderer Passkey), Signatur alt.
        let teile: Vec<&str> = t.split('.').collect();
        let anders = serde_json::to_vec(&Ausweis { cid: "anderer".into(), ..k.pruefen(&t, JETZT).unwrap() }).unwrap();
        let t2 = format!("{}.{}.{}", teile[0], b64(&anders), teile[2]);
        assert_eq!(k.pruefen(&t2, JETZT), Err(JwtFehler::Signatur));
        // Von einem anderen Schlüssel ausgestellt.
        let fremd = JwtSchluessel::zufaellig().ausstellen(&Ausweis::neu("c", JETZT));
        assert_eq!(k.pruefen(&fremd, JETZT), Err(JwtFehler::Signatur));
        // Form.
        for kaputt in ["", "a.b", "a.b.c.d", "...", &format!("{t}=")] {
            assert!(k.pruefen(kaputt, JETZT).is_err(), "{kaputt}");
        }
    }

    #[test]
    fn abgelaufen_und_hoechstens_acht_stunden() {
        let k = JwtSchluessel::zufaellig();
        let a = Ausweis::neu("c", JETZT);
        let t = k.ausstellen(&a);
        assert_eq!(k.pruefen(&t, a.exp - 1).map(|a| a.jti), Ok(a.jti.clone()));
        assert_eq!(k.pruefen(&t, a.exp), Err(JwtFehler::Abgelaufen));
        assert_eq!(k.pruefen(&t, a.exp + 3600), Err(JwtFehler::Abgelaufen));
        // Erneuern hält jti, endet spätestens 8 Stunden nach der Anmeldung.
        let spaet = a.erneuert(JETZT + SITZUNG_HOECHSTENS_SEKUNDEN - 60);
        assert_eq!((spaet.jti.as_str(), spaet.exp), (a.jti.as_str(), a.sitzung_ende()));
        assert!(k.pruefen(&k.ausstellen(&spaet), spaet.iat).is_ok());
        assert_eq!(k.pruefen(&k.ausstellen(&spaet), a.sitzung_ende()), Err(JwtFehler::Abgelaufen));
        // Ein selbst gebauter Ausweis über die 8 Stunden hinaus gilt nicht, auch richtig signiert.
        let zu_lang = Ausweis { iat: JETZT + 9 * 3600, exp: JETZT + 9 * 3600 + 60, ..a.clone() };
        assert_eq!(k.pruefen(&k.ausstellen(&zu_lang), JETZT + 9 * 3600), Err(JwtFehler::Inhalt));
        // Länger als 15 Minuten am Stück gilt nicht.
        let lang = Ausweis { exp: JETZT + 3600, ..a.clone() };
        assert_eq!(k.pruefen(&k.ausstellen(&lang), JETZT), Err(JwtFehler::Inhalt));
        // Aus der Zukunft.
        let zukunft = Ausweis { iat: JETZT + 600, exp: JETZT + 900, auth_time: JETZT + 600, ..a.clone() };
        assert_eq!(k.pruefen(&k.ausstellen(&zukunft), JETZT), Err(JwtFehler::Inhalt));
    }

    #[test]
    fn falscher_inhalt() {
        let k = JwtSchluessel::zufaellig();
        let a = Ausweis::neu("c", JETZT);
        for falsch in [
            Ausweis { sub: "jemand".into(), ..a.clone() },
            Ausweis { aud: "anderes".into(), ..a.clone() },
            Ausweis { amr: vec!["pwd".into()], ..a.clone() },
            Ausweis { jti: String::new(), ..a.clone() },
            Ausweis { cid: String::new(), ..a.clone() },
        ] {
            assert_eq!(k.pruefen(&k.ausstellen(&falsch), JETZT), Err(JwtFehler::Inhalt), "{falsch:?}");
        }
        // Unbekanntes Feld im Inhalt.
        let mut v = serde_json::to_value(&a).unwrap();
        v["admin"] = serde_json::json!(true);
        let teil = format!("{}.{}", b64(KOPF.as_bytes()), b64(&serde_json::to_vec(&v).unwrap()));
        let sig = k.signieren.sign(teil.as_bytes());
        assert_eq!(k.pruefen(&format!("{teil}.{}", b64(&sig.to_bytes())), JETZT), Err(JwtFehler::Inhalt));
    }
}
