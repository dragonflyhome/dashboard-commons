//! Verblendetes Öffnen mit dem Sicherheitschip des Macs.
//!
//! Der Server hat verschlossene Nutzlasten. Öffnen kann sie nur, wer den
//! privaten Schlüssel im Sicherheitschip des Macs benutzen darf; den hat das
//! Hilfsprogramm. Der Helfer soll dem Server das Öffnen ermöglichen, **ohne
//! den Schlüssel zu kennen oder zu senden**.
//!
//! So geht es:
//!
//! - **Verschließen** (einmal): Der Server kennt nur den öffentlichen Teil
//!   `P` des Chip-Schlüssels. Er würfelt `e`, rechnet die Kapsel `E = e·G`
//!   und das Geheimnis `S = e·P`, leitet daraus einen Schlüssel ab,
//!   verschließt die Nutzlast und vergisst `e`. Ab jetzt kann er sie allein
//!   nicht mehr öffnen. Abgelegt wird `E`, eine Nonce und das Chiffrat.
//! - **Öffnen**: Der Server würfelt eine Blende `r` und schickt dem Helfer
//!   nur `E' = r·E`. Der Chip rechnet seine gewöhnliche Schlüsselvereinbarung
//!   mit `E'` und gibt die x-Koordinate von `d·E'` zurück. Der Server nimmt
//!   die Blende wieder heraus (`r⁻¹`) und hat die x-Koordinate von `S`.
//!
//! Der Helfer sieht nie `E` und nie `S`, nur zufällig aussehende Punkte; was
//! über die Leitung geht, ist ohne `r` wertlos. Der Chip gibt nur die
//! x-Koordinate zurück; das genügt, weil `P` und `-P` dieselbe haben.
//!
//! Grenzen, ehrlich: Das ist ein eigener Zusammenbau aus Standardteilen
//! (P-256, HKDF-SHA256, XChaCha20-Poly1305), kein fertiges Verfahren aus
//! einer Bibliothek. Und die Blende ersetzt keine Prüfung der Gegenseite:
//! Wer den Helfer fragen darf **und** die verschlossenen Dateien hat, kann
//! dieselbe Rechnung machen. Der Helfer muss also wissen, wem er antwortet:
//! Er prüft den Fingerabdruck der Leitung, bevor er einem Chip etwas vorlegt
//! (`helfer::leitung`).
//!
//! Reine Logik mit eingespeistem Zufall; die Tests laufen auf jeder
//! Plattform, mit einem Chip aus Software.

use chacha20poly1305::{AeadInOut, KeyInit, XChaCha20Poly1305, XNonce};
use hmac::{Hmac, Mac};
use p256::elliptic_curve::group::Group;
use p256::elliptic_curve::point::{AffineCoordinates, DecompressPoint};
use p256::elliptic_curve::sec1::{FromEncodedPoint, ToEncodedPoint};
use p256::elliptic_curve::subtle::Choice;
use p256::{AffinePoint, EncodedPoint, FieldBytes, NonZeroScalar, ProjectivePoint};
use rand_core::{CryptoRng, RngCore};
use zeroize::Zeroizing;

/// Länge eines Punkts in der unkomprimierten Form (0x04, x, y).
pub const PUNKT: usize = 65;
/// Länge der Nonce von XChaCha20.
const NONCE: usize = 24;
/// Länge der Prüfsumme von Poly1305.
const PRUEFSUMME: usize = 16;

/// Warum Verschließen oder Öffnen scheitert. Nie mit Inhalt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fehler {
    /// Kein gültiger Punkt auf der Kurve (oder der Punkt im Unendlichen).
    Punkt,
    /// Die verschlossene Datei ist zu kurz.
    Format,
    /// Die Prüfsumme stimmt nicht: falscher Chip, falsche Antwort, falscher
    /// Zweck oder veränderte Datei.
    Pruefsumme,
    /// Das System liefert keinen Zufall.
    Zufall,
}

fn punkt_lesen(roh: &[u8]) -> Result<AffinePoint, Fehler> {
    let kodiert = EncodedPoint::from_bytes(roh).map_err(|_| Fehler::Punkt)?;
    let punkt = Option::<AffinePoint>::from(AffinePoint::from_encoded_point(&kodiert)).ok_or(Fehler::Punkt)?;
    // Der Punkt im Unendlichen ist kein Schlüssel.
    if bool::from(ProjectivePoint::from(punkt).is_identity()) {
        return Err(Fehler::Punkt);
    }
    Ok(punkt)
}

fn punkt_schreiben(punkt: &ProjectivePoint) -> [u8; PUNKT] {
    let mut aus = [0u8; PUNKT];
    aus.copy_from_slice(punkt.to_affine().to_encoded_point(false).as_bytes());
    aus
}

/// HKDF-SHA256 (RFC 5869) für 32 Bytes Ausgabe: ein Block genügt.
fn hkdf(geheim: &[u8], salz: &[u8], zweck: &[u8]) -> Zeroizing<[u8; 32]> {
    type H = Hmac<sha2_010::Sha256>;
    let mut extract = <H as Mac>::new_from_slice(salz).expect("HMAC nimmt jede Schlüssellänge");
    extract.update(geheim);
    let prk = Zeroizing::new(extract.finalize().into_bytes());
    let mut expand = <H as Mac>::new_from_slice(&prk).expect("HMAC nimmt jede Schlüssellänge");
    expand.update(zweck);
    expand.update(&[1u8]);
    let mut aus = Zeroizing::new([0u8; 32]);
    aus.copy_from_slice(&expand.finalize().into_bytes());
    aus
}

/// Der Schlüssel aus der x-Koordinate des Geheimnisses. Kapsel und
/// öffentlicher Chip-Schlüssel gehen als Salz ein, der Zweck als Angabe: Der
/// Schlüssel gilt nur für genau diese Kapsel, diesen Chip und diesen Zweck.
fn schluessel(x: &FieldBytes, kapsel: &[u8; PUNKT], chip_oeffentlich: &[u8; PUNKT], zweck: &[u8]) -> Zeroizing<[u8; 32]> {
    let mut salz = [0u8; 2 * PUNKT];
    salz[..PUNKT].copy_from_slice(kapsel);
    salz[PUNKT..].copy_from_slice(chip_oeffentlich);
    hkdf(x, &salz, zweck)
}

fn zufall(rng: &mut (impl RngCore + CryptoRng), ziel: &mut [u8]) -> Result<(), Fehler> {
    rng.try_fill_bytes(ziel).map_err(|_| Fehler::Zufall)
}

fn skalar(rng: &mut (impl RngCore + CryptoRng)) -> Result<NonZeroScalar, Fehler> {
    // Verwerfen, bis die Bytes ein gültiger Skalar ungleich null sind (fast immer beim ersten Mal).
    for _ in 0..64 {
        let mut b = Zeroizing::new([0u8; 32]);
        zufall(rng, b.as_mut_slice())?;
        if let Some(s) = Option::<NonZeroScalar>::from(NonZeroScalar::from_repr((*b).into())) {
            return Ok(s);
        }
    }
    Err(Fehler::Zufall)
}

/// Verschließt `klartext` für den Chip mit dem öffentlichen Schlüssel
/// `chip_oeffentlich` (unkomprimierter Punkt). Das Ergebnis ist Kapsel,
/// Nonce und Chiffrat hintereinander. Wer es schreibt, kann es danach nicht
/// mehr öffnen: Der Einmalwert wird verworfen.
pub fn verschliessen(chip_oeffentlich: &[u8], zweck: &[u8], klartext: &[u8], rng: &mut (impl RngCore + CryptoRng)) -> Result<Vec<u8>, Fehler> {
    let p = punkt_lesen(chip_oeffentlich)?;
    let chip: [u8; PUNKT] = punkt_schreiben(&p.into());
    let e = skalar(rng)?;
    let kapsel = punkt_schreiben(&(ProjectivePoint::GENERATOR * *e));
    let geheim = (ProjectivePoint::from(p) * *e).to_affine();
    let k = schluessel(&geheim.x(), &kapsel, &chip, zweck);
    let mut nonce = [0u8; NONCE];
    zufall(rng, &mut nonce)?;
    let mut puffer = Zeroizing::new(klartext.to_vec());
    XChaCha20Poly1305::new((&*k).into()).encrypt_in_place(&XNonce::from(nonce), zweck, &mut *puffer).map_err(|_| Fehler::Pruefsumme)?;
    let mut aus = Vec::with_capacity(PUNKT + NONCE + puffer.len());
    aus.extend_from_slice(&kapsel);
    aus.extend_from_slice(&nonce);
    aus.extend_from_slice(&puffer);
    Ok(aus)
}

/// Eine laufende Anfrage: die Blende, die der Server für sich behält. Sie
/// gilt für genau eine Antwort und wird beim Öffnen verbraucht.
pub struct Blende {
    /// `r⁻¹`: nimmt die Blende aus der Antwort wieder heraus.
    r_inv: NonZeroScalar,
}

impl std::fmt::Debug for Blende {
    /// Nie den Wert zeigen.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Blende").finish_non_exhaustive()
    }
}

fn kapsel_von(verschlossen: &[u8]) -> Result<[u8; PUNKT], Fehler> {
    if verschlossen.len() < PUNKT + NONCE + PRUEFSUMME {
        return Err(Fehler::Format);
    }
    let mut kapsel = [0u8; PUNKT];
    kapsel.copy_from_slice(&verschlossen[..PUNKT]);
    Ok(kapsel)
}

/// Beginnt das Öffnen: würfelt eine Blende und gibt den verblendeten Punkt
/// zurück, den der Helfer dem Chip vorlegt. Der Punkt verrät weder die
/// Kapsel noch das Geheimnis.
pub fn anfragen(verschlossen: &[u8], rng: &mut (impl RngCore + CryptoRng)) -> Result<(Blende, [u8; PUNKT]), Fehler> {
    let kapsel = punkt_lesen(&kapsel_von(verschlossen)?)?;
    let r = skalar(rng)?;
    let verblendet = punkt_schreiben(&(ProjectivePoint::from(kapsel) * *r));
    let r_inv = Option::from(r.invert()).and_then(|inv| Option::<NonZeroScalar>::from(NonZeroScalar::new(inv))).ok_or(Fehler::Zufall)?;
    Ok((Blende { r_inv }, verblendet))
}

/// Schließt das Öffnen ab. `antwort_x` ist, was der Chip auf den
/// verblendeten Punkt geantwortet hat: die x-Koordinate seiner
/// Schlüsselvereinbarung (32 Bytes). Passt irgendetwas nicht (anderer Chip,
/// falsche oder gefälschte Antwort, anderer Zweck, veränderte Datei),
/// scheitert die Prüfsumme, und es kommt kein Klartext heraus.
pub fn oeffnen(blende: Blende, verschlossen: &[u8], chip_oeffentlich: &[u8], zweck: &[u8], antwort_x: &[u8]) -> Result<Zeroizing<Vec<u8>>, Fehler> {
    let kapsel = kapsel_von(verschlossen)?;
    let chip: [u8; PUNKT] = punkt_schreiben(&punkt_lesen(chip_oeffentlich)?.into());
    let x: [u8; 32] = antwort_x.try_into().map_err(|_| Fehler::Punkt)?;
    // Aus der x-Koordinate wieder ein Punkt; welches der beiden y, ist gleich:
    // Der andere Punkt ist der negative und führt zur selben x-Koordinate.
    let verblendet = Option::<AffinePoint>::from(AffinePoint::decompress(&x.into(), Choice::from(0))).ok_or(Fehler::Punkt)?;
    let geheim = (ProjectivePoint::from(verblendet) * *blende.r_inv).to_affine();
    let k = schluessel(&geheim.x(), &kapsel, &chip, zweck);
    let mut nonce = [0u8; NONCE];
    nonce.copy_from_slice(&verschlossen[PUNKT..PUNKT + NONCE]);
    let mut puffer = Zeroizing::new(verschlossen[PUNKT + NONCE..].to_vec());
    XChaCha20Poly1305::new((&*k).into()).decrypt_in_place(&XNonce::from(nonce), zweck, &mut *puffer).map_err(|_| Fehler::Pruefsumme)?;
    Ok(puffer)
}

/// Was der Chip tut, in Software: seine Schlüsselvereinbarung mit dem
/// vorgelegten Punkt, zurück kommt nur die x-Koordinate. Nur für Tests und
/// zum Vergleich mit dem echten Chip; im Betrieb rechnet das der Chip.
pub fn chip_antwort(privat: &NonZeroScalar, punkt: &[u8]) -> Result<[u8; 32], Fehler> {
    let p = punkt_lesen(punkt)?;
    Ok((ProjectivePoint::from(p) * **privat).to_affine().x().into())
}

/// Der öffentliche Teil zu einem privaten Schlüssel (für Tests).
pub fn oeffentlich(privat: &NonZeroScalar) -> [u8; PUNKT] {
    punkt_schreiben(&(ProjectivePoint::GENERATOR * **privat))
}

#[cfg(test)]
mod tests {
    use rand_core::OsRng;

    use super::*;

    const ZWECK: &[u8] = b"dashboard/verblendung/test/speicher/aufgaben/links";

    fn chip() -> NonZeroScalar {
        skalar(&mut OsRng).unwrap()
    }

    fn rund(privat: &NonZeroScalar, verschlossen: &[u8], zweck: &[u8]) -> Result<Zeroizing<Vec<u8>>, Fehler> {
        let (blende, punkt) = anfragen(verschlossen, &mut OsRng)?;
        let antwort = chip_antwort(privat, &punkt)?;
        oeffnen(blende, verschlossen, &oeffentlich(privat), zweck, &antwort)
    }

    /// HKDF-SHA256 gegen den ersten Testvektor aus RFC 5869 (auf 32 Bytes gekürzt).
    #[test]
    fn hkdf_nach_rfc_5869() {
        let ikm = [0x0bu8; 22];
        let salz: Vec<u8> = (0x00..=0x0c).collect();
        let zweck: Vec<u8> = (0xf0..=0xf9).collect();
        let erwartet = [
            0x3c, 0xb2, 0x5f, 0x25, 0xfa, 0xac, 0xd5, 0x7a, 0x90, 0x43, 0x4f, 0x64, 0xd0, 0x36, 0x2f, 0x2a, 0x2d, 0x2d, 0x0a, 0x90, 0xcf, 0x1a, 0x5a, 0x4c,
            0x5d, 0xb0, 0x2d, 0x56, 0xec, 0xc4, 0xc5, 0xbf,
        ];
        assert_eq!(*hkdf(&ikm, &salz, &zweck), erwartet);
    }

    #[test]
    fn verschliessen_und_verblendet_oeffnen() {
        let privat = chip();
        let klartext = [0x5au8; 32];
        let verschlossen = verschliessen(&oeffentlich(&privat), ZWECK, &klartext, &mut OsRng).unwrap();
        assert_eq!(verschlossen.len(), PUNKT + NONCE + 32 + PRUEFSUMME);
        // Der Klartext steht nicht in der Datei.
        assert!(!verschlossen.windows(8).any(|w| w == &klartext[..8]));
        assert_eq!(rund(&privat, &verschlossen, ZWECK).unwrap().as_slice(), klartext);
        // Auch andere Längen, auch leer.
        for laenge in [0usize, 1, 31, 33, 1000] {
            let k = vec![7u8; laenge];
            let v = verschliessen(&oeffentlich(&privat), ZWECK, &k, &mut OsRng).unwrap();
            assert_eq!(rund(&privat, &v, ZWECK).unwrap().as_slice(), k);
        }
    }

    /// Was der Helfer sieht und was er antwortet, ist jedes Mal anders, und
    /// nichts davon ist die Kapsel oder das unverblendete Geheimnis.
    #[test]
    fn der_helfer_sieht_weder_kapsel_noch_geheimnis() {
        let privat = chip();
        let verschlossen = verschliessen(&oeffentlich(&privat), ZWECK, &[1u8; 32], &mut OsRng).unwrap();
        let kapsel = &verschlossen[..PUNKT];
        let unverblendet = chip_antwort(&privat, kapsel).unwrap();
        let mut gesehen = Vec::new();
        for _ in 0..8 {
            let (blende, punkt) = anfragen(&verschlossen, &mut OsRng).unwrap();
            let antwort = chip_antwort(&privat, &punkt).unwrap();
            assert_ne!(&punkt[..], kapsel, "der Helfer bekäme die Kapsel selbst");
            assert_ne!(antwort, unverblendet, "der Helfer rechnete das echte Geheimnis");
            assert!(!gesehen.contains(&(punkt, antwort)), "zweimal dieselbe Anfrage");
            gesehen.push((punkt, antwort));
            assert_eq!(oeffnen(blende, &verschlossen, &oeffentlich(&privat), ZWECK, &antwort).unwrap().as_slice(), [1u8; 32]);
        }
    }

    /// Die Antwort gehört zu ihrer Blende: Mit der Blende einer anderen
    /// Anfrage geht nichts auf.
    #[test]
    fn antwort_einer_anderen_anfrage_oeffnet_nicht() {
        let privat = chip();
        let verschlossen = verschliessen(&oeffentlich(&privat), ZWECK, &[2u8; 32], &mut OsRng).unwrap();
        let (blende_a, _punkt_a) = anfragen(&verschlossen, &mut OsRng).unwrap();
        let (_blende_b, punkt_b) = anfragen(&verschlossen, &mut OsRng).unwrap();
        let antwort_b = chip_antwort(&privat, &punkt_b).unwrap();
        assert_eq!(oeffnen(blende_a, &verschlossen, &oeffentlich(&privat), ZWECK, &antwort_b).unwrap_err(), Fehler::Pruefsumme);
    }

    #[test]
    fn ein_anderer_chip_oeffnet_nicht() {
        let (richtig, falsch) = (chip(), chip());
        let verschlossen = verschliessen(&oeffentlich(&richtig), ZWECK, &[3u8; 32], &mut OsRng).unwrap();
        // Der falsche Chip antwortet; der Server rechnet mit dem richtigen öffentlichen Teil.
        let (blende, punkt) = anfragen(&verschlossen, &mut OsRng).unwrap();
        let antwort = chip_antwort(&falsch, &punkt).unwrap();
        assert_eq!(oeffnen(blende, &verschlossen, &oeffentlich(&richtig), ZWECK, &antwort).unwrap_err(), Fehler::Pruefsumme);
        // Der richtige Chip antwortet; der Server rechnet mit einem anderen öffentlichen Teil.
        let (blende, punkt) = anfragen(&verschlossen, &mut OsRng).unwrap();
        let antwort = chip_antwort(&richtig, &punkt).unwrap();
        assert_eq!(oeffnen(blende, &verschlossen, &oeffentlich(&falsch), ZWECK, &antwort).unwrap_err(), Fehler::Pruefsumme);
    }

    #[test]
    fn anderer_zweck_oeffnet_nicht() {
        let privat = chip();
        let verschlossen = verschliessen(&oeffentlich(&privat), ZWECK, &[4u8; 32], &mut OsRng).unwrap();
        assert_eq!(rund(&privat, &verschlossen, b"dashboard/verblendung/test/anderer/pfad").unwrap_err(), Fehler::Pruefsumme);
    }

    #[test]
    fn jedes_veraenderte_byte_faellt_auf() {
        let privat = chip();
        let verschlossen = verschliessen(&oeffentlich(&privat), ZWECK, &[5u8; 32], &mut OsRng).unwrap();
        // Hinter der Kapsel: Nonce, Chiffrat, Prüfsumme.
        for i in PUNKT..verschlossen.len() {
            let mut v = verschlossen.clone();
            v[i] ^= 1;
            assert_eq!(rund(&privat, &v, ZWECK).unwrap_err(), Fehler::Pruefsumme, "Byte {i}");
        }
        // In der Kapsel: kein Punkt mehr, oder ein anderer Punkt und damit ein anderer Schlüssel.
        for i in 0..PUNKT {
            let mut v = verschlossen.clone();
            v[i] ^= 1;
            assert!(matches!(rund(&privat, &v, ZWECK), Err(Fehler::Punkt | Fehler::Pruefsumme)), "Byte {i}");
        }
    }

    #[test]
    fn gefaelschte_antworten_oeffnen_nicht() {
        let privat = chip();
        let verschlossen = verschliessen(&oeffentlich(&privat), ZWECK, &[6u8; 32], &mut OsRng).unwrap();
        let p = oeffentlich(&privat);
        let neu = || anfragen(&verschlossen, &mut OsRng).unwrap().0;
        // Zu kurz, zu lang, leer.
        for antwort in [&[][..], &[1u8; 31], &[1u8; 33]] {
            assert_eq!(oeffnen(neu(), &verschlossen, &p, ZWECK, antwort).unwrap_err(), Fehler::Punkt);
        }
        // 32 Bytes, die keine x-Koordinate eines Punkts sind, oder eine beliebige.
        for antwort in [[0u8; 32], [0xffu8; 32], [7u8; 32]] {
            assert!(matches!(oeffnen(neu(), &verschlossen, &p, ZWECK, &antwort), Err(Fehler::Punkt | Fehler::Pruefsumme)));
        }
    }

    #[test]
    fn ungueltige_punkte_und_zu_kurze_dateien_werden_abgewiesen() {
        let privat = chip();
        let p = oeffentlich(&privat);
        // Kein Punkt als öffentlicher Schlüssel.
        for schlecht in [&[][..], &[4u8; 65], &[0u8; 65], &p[..64], &[0u8]] {
            assert_eq!(verschliessen(schlecht, ZWECK, &[1], &mut OsRng).unwrap_err(), Fehler::Punkt, "{schlecht:?}");
        }
        // Zu kurze Datei.
        let v = verschliessen(&p, ZWECK, &[], &mut OsRng).unwrap();
        assert_eq!(anfragen(&v[..v.len() - 1], &mut OsRng).unwrap_err(), Fehler::Format);
        assert_eq!(anfragen(&[], &mut OsRng).unwrap_err(), Fehler::Format);
        // Der Chip in Software weist ungültige Punkte ab, wie der echte.
        assert_eq!(chip_antwort(&privat, &[4u8; 65]).unwrap_err(), Fehler::Punkt);
    }

    /// Wer verschließt, kann danach nicht öffnen: Aus der Datei und dem
    /// öffentlichen Teil allein kommt der Klartext nicht heraus.
    #[test]
    fn ohne_den_chip_geht_nichts_auf() {
        let privat = chip();
        let p = oeffentlich(&privat);
        let verschlossen = verschliessen(&p, ZWECK, &[8u8; 32], &mut OsRng).unwrap();
        // Ein Angreifer mit Datei und öffentlichem Teil antwortet sich selbst,
        // etwa mit der x-Koordinate des öffentlichen Schlüssels oder der Kapsel.
        let (blende, _) = anfragen(&verschlossen, &mut OsRng).unwrap();
        assert!(oeffnen(blende, &verschlossen, &p, ZWECK, &p[1..33]).is_err());
        let (blende, punkt) = anfragen(&verschlossen, &mut OsRng).unwrap();
        assert!(oeffnen(blende, &verschlossen, &p, ZWECK, &punkt[1..33]).is_err());
    }

    #[test]
    fn die_blende_zeigt_ihren_wert_nicht() {
        let privat = chip();
        let verschlossen = verschliessen(&oeffentlich(&privat), ZWECK, &[9u8; 32], &mut OsRng).unwrap();
        let (blende, _) = anfragen(&verschlossen, &mut OsRng).unwrap();
        assert_eq!(format!("{blende:?}"), "Blende { .. }");
    }
}
