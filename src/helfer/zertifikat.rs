//! Das Zertifikat der Helfer-Leitung: aus einem Geheimnis von 32 Bytes
//! abgeleitet, bei jedem Start dasselbe.
//!
//! Die Leitung beglaubigt sich nicht über eine Zertifizierungsstelle: Der
//! Helfer kennt den Fingerabdruck (SHA-256 über das Zertifikat) und prüft
//! ihn nach dem Aufbau. Damit der Fingerabdruck über Neustarts hinweg gleich
//! bleibt, entsteht das Zertifikat ohne Zufall und ohne Uhr: Der Schlüssel
//! ist das Geheimnis selbst (P-256), die Gültigkeit steht fest, und die
//! Signatur ist nach RFC 6979 bestimmt. Auf der Platte liegt nur das
//! Geheimnis, wie die anderen Schlüssel im Schlüsselordner.

use std::str::FromStr;

use p256::ecdsa::{DerSignature, SigningKey};
use p256::pkcs8::EncodePrivateKey;
use sha2::Digest;
use x509_cert::builder::{Builder, CertificateBuilder, Profile};
use x509_cert::der::asn1::{GeneralizedTime, Ia5String};
use x509_cert::der::{DateTime, Encode};
use x509_cert::ext::pkix::SubjectAltName;
use x509_cert::ext::pkix::name::GeneralName;
use x509_cert::name::Name;
use x509_cert::serial_number::SerialNumber;
use x509_cert::spki::SubjectPublicKeyInfoOwned;
use x509_cert::time::{Time, Validity};

/// Name im Zertifikat. Geprüft wird der Fingerabdruck, nicht der Name.
pub const NAME: &str = "CN=dashboard-helfer-leitung";

/// Zertifikat und privater Schlüssel der Leitung.
pub struct Paar {
    /// Das Zertifikat, DER.
    pub zertifikat: Vec<u8>,
    /// Der private Schlüssel, PKCS#8 DER.
    pub schluessel: zeroize::Zeroizing<Vec<u8>>,
    /// SHA-256 über das Zertifikat.
    pub fingerabdruck: [u8; 32],
}

impl std::fmt::Debug for Paar {
    /// Nie den Schlüssel zeigen.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Paar").field("fingerabdruck", &hex(&self.fingerabdruck)).finish_non_exhaustive()
    }
}

/// Bytes als kleine Hexziffern.
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Leitet Zertifikat und Schlüssel aus dem Geheimnis ab. Dasselbe Geheimnis
/// ergibt immer dasselbe Zertifikat.
pub fn ableiten(geheim: &[u8; 32]) -> Result<Paar, String> {
    let fehler = |was: &str, e: &dyn std::fmt::Display| format!("Das Zertifikat der Helfer-Leitung ließ sich nicht bilden ({was}: {e}).");
    let signieren = SigningKey::from_slice(geheim).map_err(|e| fehler("Schlüssel", &e))?;
    let schluessel = signieren.to_pkcs8_der().map_err(|e| fehler("PKCS#8", &e))?;
    let oeffentlich = SubjectPublicKeyInfoOwned::from_key(*signieren.verifying_key()).map_err(|e| fehler("öffentlicher Schlüssel", &e))?;
    let name = Name::from_str(NAME).map_err(|e| fehler("Name", &e))?;
    let zeit = |jahr, monat, tag, stunde, minute, sekunde| {
        DateTime::new(jahr, monat, tag, stunde, minute, sekunde).map(|z| Time::GeneralTime(GeneralizedTime::from_date_time(z))).map_err(|e| fehler("Zeit", &e))
    };
    // Fest statt „ab jetzt“: Sonst wäre der Fingerabdruck bei jedem Start ein anderer.
    // Das Ende ist der Wert, den RFC 5280 für „kein festes Ende“ vorsieht.
    let gueltig = Validity { not_before: zeit(2026, 1, 1, 0, 0, 0)?, not_after: zeit(9999, 12, 31, 23, 59, 59)? };
    // Selbst ausgestellt: Aussteller und Inhaber sind derselbe Name.
    let profil = Profile::Leaf { issuer: name.clone(), enable_key_agreement: false, enable_key_encipherment: false };
    let nummer = SerialNumber::new(&[1]).map_err(|e| fehler("Nummer", &e))?;
    let mut bau = CertificateBuilder::new(profil, nummer, gueltig, name, oeffentlich, &signieren).map_err(|e| fehler("Aufbau", &e))?;
    let localhost = Ia5String::new("localhost").map_err(|e| fehler("Name", &e))?;
    bau.add_extension(&SubjectAltName(vec![GeneralName::DnsName(localhost)])).map_err(|e| fehler("Erweiterung", &e))?;
    let zertifikat = bau.build::<DerSignature>().map_err(|e| fehler("Signatur", &e))?.to_der().map_err(|e| fehler("DER", &e))?;
    let fingerabdruck = sha2::Sha256::digest(&zertifikat).into();
    Ok(Paar { zertifikat, schluessel: zeroize::Zeroizing::new(schluessel.as_bytes().to_vec()), fingerabdruck })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dasselbe_geheimnis_ergibt_dasselbe_zertifikat() {
        let a = ableiten(&[7u8; 32]).unwrap();
        let b = ableiten(&[7u8; 32]).unwrap();
        assert_eq!(a.zertifikat, b.zertifikat);
        assert_eq!(a.fingerabdruck, b.fingerabdruck);
        assert_eq!(*a.schluessel, *b.schluessel);
        let c = ableiten(&[8u8; 32]).unwrap();
        assert_ne!(a.fingerabdruck, c.fingerabdruck);
        // Der Schlüssel steht weder im Zertifikat noch in der Ausgabe für das Protokoll.
        assert!(!a.zertifikat.windows(32).any(|w| w == [7u8; 32]));
        assert!(!format!("{a:?}").contains("0707"));
        assert_eq!(hex(&a.fingerabdruck).len(), 64);
    }

    #[test]
    fn ungueltiges_geheimnis_wird_abgewiesen() {
        // Null ist kein Schlüssel auf P-256.
        assert!(ableiten(&[0u8; 32]).is_err());
    }

    #[test]
    fn die_tls_bibliothek_nimmt_zertifikat_und_schluessel_an() {
        use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
        let p = ableiten(&[7u8; 32]).unwrap();
        let schluessel = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(p.schluessel.to_vec()));
        let ergebnis = rustls::ServerConfig::builder_with_provider(std::sync::Arc::new(rustls_rustcrypto::provider()))
            .with_protocol_versions(&[&rustls::version::TLS13])
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(vec![CertificateDer::from(p.zertifikat)], schluessel);
        assert!(ergebnis.is_ok(), "{:?}", ergebnis.err());
    }
}
