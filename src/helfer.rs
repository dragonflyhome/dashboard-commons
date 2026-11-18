//! Das Mac-Hilfsprogramm: Sperre mit dem Rechner und Schlüssel aus dem Sicherheitschip.
//!
//! Die Sperre mit dem Rechner ist eine Einstellung: Wer sie einschaltet,
//! will, dass sich das Dashboard sperrt, sobald der Rechner gesperrt wird,
//! und dass danach eine neue Anmeldung nötig ist. Von der Sperre erfährt der
//! Server nicht selbst: Ein kleines Mac-Programm meldet sie, getrennt vom Kern.
//! Dasselbe Programm spricht mit dem Sicherheitschip und hilft dem Server,
//! seine Schlüssel zu öffnen, ohne sie je zu sehen.
//!
//! Aufbau in drei Teilen:
//!
//! - **Reine Logik** (dieses Modul): der Stand der Sperre, der Schlüsselbund,
//!   der Schein des Helfers und die Prüfung seiner Anmeldung.
//! - **Endpunkte** (`leitung::router`): gewöhnliche HTTP-Wege. Kein eigenes
//!   Leitungsformat.
//! - **Leitung** (`leitung::starten`): TLS 1.3 an einer eigenen Adresse, nur
//!   auf dem eigenen Rechner. Die Endpunkte gibt es nur dort, nie am Web-Port.
//!
//! Der Helfer ruft immer an; die Antwort kommt auf derselben Verbindung zurück.
//!
//! **Anmeldung des Helfers.** Die Leitung entsteht zuerst unbeglaubigt; dann
//! prüfen beide Seiten einander. Der Helfer vergleicht den Fingerabdruck des
//! Zertifikats mit dem, den er kennt. Der Server verlangt eine Signatur über
//! den Bindungswert genau dieser Verbindung (RFC 9266, `tls-exporter`), von
//! einem Schlüssel, der in seiner Liste der zugelassenen Helfer steht. Eine
//! mitgeschnittene Anmeldung taugt so für keine andere Verbindung. Danach
//! bekommt der Helfer einen **Schein**: ein signiertes Token nur für diese
//! Aufgabe, nie der Ausweis einer Person. Der Helfer kann nur sperren, sein
//! Dasein melden und beim Öffnen helfen; eine Anmeldung ersetzt er nie.
//!
//! **Schlüssel.** Der Server führt einen Schlüsselbund: Jeder Schlüssel steht
//! unter dem Pfad der Nutzlast, die er öffnet (etwa `speicher/aufgaben/links`),
//! und trägt einen Namen und ein Etikett. Es kann viele Nutzlasten geben, und
//! nicht jeder Helfer öffnet jede. Den Schlüssel würfelt der Server selbst und
//! verschließt ihn an den öffentlichen Schlüssel eines Chips (`verblendung`);
//! auf der Platte liegt nur diese Ablage. Zum Öffnen legt der Server dem
//! Helfer einen verblendeten Punkt vor, der Chip rechnet damit, und nur der
//! Server kann aus der Antwort den Schlüssel gewinnen. Der Helfer lernt weder
//! den Schlüssel noch die Ablage. Der geöffnete Schlüssel liegt nur im
//! Arbeitsspeicher; nach einem Neustart nennt die Antwort auf das
//! Lebenszeichen die offenen Anfragen.
//!
//! Im Zweifel gesperrt: Meldet sich der Helfer nicht mehr, gilt der Rechner
//! als gesperrt.

use std::time::{Duration, Instant};

/// Meldet sich der Helfer so lange nicht, gilt der Rechner als gesperrt.
pub const ZEICHEN_FRIST: Duration = Duration::from_secs(20);

/// Der Stand der Sperre mit dem Rechner. Reine Logik mit eingespeisten
/// Zeiten: `jetzt` ist die stetige Uhr (für das Lebenszeichen), `wand` die
/// Wanduhr in Sekunden (für den Vergleich mit dem Zeitpunkt einer Anmeldung).
#[derive(Debug, Default)]
pub struct SperrStand {
    eingeschaltet: bool,
    /// Der Rechner ist laut Helfer gerade gesperrt.
    gesperrt: bool,
    /// Jede Anmeldung bis zu diesem Zeitpunkt gilt nicht mehr.
    grenze: i64,
    letztes_zeichen: Option<Instant>,
}

impl SperrStand {
    /// Schaltet die Sperre mit dem Rechner ein. Alles, was bis jetzt
    /// angemeldet war, gilt nicht mehr; bis zum ersten Lebenszeichen ist gesperrt.
    pub fn einschalten(&mut self, wand: i64) {
        self.eingeschaltet = true;
        self.grenze = self.grenze.max(wand);
    }

    pub fn eingeschaltet(&self) -> bool {
        self.eingeschaltet
    }

    /// Der Helfer meldet: Der Rechner ist jetzt gesperrt.
    pub fn sperre(&mut self, jetzt: Instant, wand: i64) {
        self.gesperrt = true;
        self.grenze = self.grenze.max(wand);
        self.letztes_zeichen = Some(jetzt);
    }

    /// Der Helfer meldet: Der Rechner ist wieder entsperrt. Anmeldungen bis
    /// zu diesem Zeitpunkt gelten nicht; danach ist eine neue nötig.
    pub fn entsperrt(&mut self, jetzt: Instant, wand: i64) {
        self.gesperrt = false;
        self.grenze = self.grenze.max(wand);
        self.letztes_zeichen = Some(jetzt);
    }

    /// Der Helfer meldet: Ich bin noch da.
    pub fn zeichen(&mut self, jetzt: Instant) {
        self.letztes_zeichen = Some(jetzt);
    }

    /// Ob der Helfer sich rechtzeitig gemeldet hat.
    fn helfer_lebt(&self, jetzt: Instant) -> bool {
        self.letztes_zeichen.is_some_and(|z| jetzt.saturating_duration_since(z) < ZEICHEN_FRIST)
    }

    /// Ist gerade gesperrt? Ausgeschaltet: nie. Sonst: wenn der Helfer die
    /// Sperre gemeldet hat oder sich nicht mehr meldet.
    pub fn ist_gesperrt(&self, jetzt: Instant) -> bool {
        self.eingeschaltet && (self.gesperrt || !self.helfer_lebt(jetzt))
    }

    /// Gilt eine Anmeldung vom Zeitpunkt `anmeldung` noch? Solange gesperrt
    /// ist, gilt keine, und die Grenze rückt mit: Auch eine Anmeldung, die
    /// währenddessen entsteht, gilt danach nicht.
    pub fn gilt(&mut self, anmeldung: i64, jetzt: Instant, wand: i64) -> bool {
        if !self.eingeschaltet {
            return true;
        }
        if self.ist_gesperrt(jetzt) {
            self.grenze = self.grenze.max(wand);
            return false;
        }
        anmeldung > self.grenze
    }
}

/// Ein Schlüssel von 32 Bytes in einem Puffer, der beim Freigeben überschrieben wird.
pub type Wert = zeroize::Zeroizing<[u8; 32]>;

/// Längster Name und längstes Etikett eines Schlüssels, in Zeichen.
pub const NAME_HOECHSTENS: usize = 60;
pub const ETIKETT_HOECHSTENS: usize = 120;

/// Was ein Angebot oder eine Antwort des Helfers zu einem Pfad ergibt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Uebernahme {
    /// Der Schlüssel für diesen Pfad liegt jetzt vor.
    Gesetzt,
    /// Für diesen Pfad gibt es schon eine Ablage (beim Angebot) oder der
    /// Schlüssel liegt schon vor (bei der Antwort). Nichts geändert: Ein
    /// stiller Tausch machte alles unlesbar, was bisher verschlossen wurde.
    Vorhanden,
    /// Der Server erwartet unter diesem Pfad keine Nutzlast.
    Unbekannt,
    /// Name, Etikett oder der öffentliche Schlüssel des Chips sind nicht lesbar.
    Unlesbar,
    /// Eine Antwort ohne offene Anfrage.
    Unerwartet,
    /// Die Antwort öffnet die Ablage nicht (anderer Chip, falsche Antwort
    /// oder veränderte Ablage). Die Anfrage ist verbraucht.
    Falsch,
}

/// Was der Server über einen Pfad seines Schlüsselbunds sagt; nie der Wert.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Beschreibung {
    /// Der Pfad der Nutzlast, die der Schlüssel öffnet.
    pub pfad: String,
    /// Name und Etikett aus dem Angebot des Helfers; leer ohne Ablage.
    pub name: String,
    pub etikett: String,
    /// Der öffentliche Schlüssel des Chips, an den verschlossen ist (Base64url); leer ohne Ablage.
    pub chip: String,
    /// Der Schlüssel liegt gerade geöffnet im Arbeitsspeicher.
    pub vorhanden: bool,
}

/// Was vom Schlüssel einer Nutzlast auf der Platte liegt: verschlossen an
/// den öffentlichen Schlüssel eines Chips. Ohne den Chip ist sie wertlos.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ablage {
    pub pfad: String,
    pub name: String,
    pub etikett: String,
    /// Öffentlicher Schlüssel des Chips, 65 Bytes, Base64url.
    pub chip: String,
    /// Ausgabe von `verblendung::verschliessen`, Base64url.
    pub verschlossen: String,
}

impl Ablage {
    fn teile(&self) -> Option<(Vec<u8>, Vec<u8>)> {
        let chip = crate::passkey::b64_lesen(&self.chip).filter(|c| c.len() == crate::verblendung::PUNKT)?;
        Some((chip, crate::passkey::b64_lesen(&self.verschlossen)?))
    }

    /// Lesbar und in den Grenzen?
    fn gut(&self) -> bool {
        beschriftung_gut(&self.name, NAME_HOECHSTENS) && beschriftung_gut(&self.etikett, ETIKETT_HOECHSTENS) && self.teile().is_some()
    }
}

/// Der Dateiname der Ablage zu einem Pfad: fest, ohne den Pfad zu zeigen.
pub fn ablage_datei(pfad: &str) -> String {
    use sha2::Digest;
    let h = sha2::Sha256::digest(pfad.as_bytes());
    format!("helfer-nutzlast-{}.json", h[..8].iter().map(|b| format!("{b:02x}")).collect::<String>())
}

/// Eine offene Anfrage an den Helfer: Wer den Chip mit diesem öffentlichen
/// Schlüssel hat, legt ihm den Punkt vor und schickt die Antwort zurück.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Anfrage {
    pub pfad: String,
    /// Öffentlicher Schlüssel des Chips, Base64url.
    pub chip: String,
    /// Der verblendete Punkt, 65 Bytes, Base64url.
    pub punkt: String,
}

/// Ein angenommenes Angebot, bevor seine Ablage auf der Platte liegt.
pub struct Vorbereitet {
    ablage: Ablage,
    wert: Wert,
}

impl Vorbereitet {
    pub fn ablage(&self) -> &Ablage {
        &self.ablage
    }
}

impl std::fmt::Debug for Vorbereitet {
    /// Nie den Wert zeigen.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Vorbereitet").field("pfad", &self.ablage.pfad).finish_non_exhaustive()
    }
}

/// Der Schlüsselbund des Servers: die Pfade der Nutzlasten, für die er
/// Schlüssel über den Helfer führt, ihre Ablagen, die offenen Anfragen und
/// was gerade geöffnet ist. Reine Logik; der Zufall wird eingespeist.
#[derive(Default)]
pub struct SchluesselBund {
    erwartet: std::collections::BTreeSet<String>,
    ablagen: std::collections::BTreeMap<String, Ablage>,
    offen: std::collections::BTreeMap<String, (crate::verblendung::Blende, String)>,
    vorhanden: std::collections::BTreeMap<String, Wert>,
}

impl std::fmt::Debug for SchluesselBund {
    /// Nie die Werte zeigen.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SchluesselBund").field("erwartet", &self.erwartet).field("vorhanden", &self.vorhanden.keys().collect::<Vec<_>>()).finish()
    }
}

fn beschriftung_gut(text: &str, hoechstens: usize) -> bool {
    !text.trim().is_empty() && text.chars().count() <= hoechstens && !text.chars().any(char::is_control)
}

impl SchluesselBund {
    /// Ein Bund, der genau diese Pfade erwartet.
    pub fn fuer(pfade: &[&str]) -> Self {
        Self { erwartet: pfade.iter().map(|p| (*p).to_string()).collect(), ..Default::default() }
    }

    /// Die erwarteten Pfade, sortiert.
    pub fn erwartet(&self) -> Vec<String> {
        self.erwartet.iter().cloned().collect()
    }

    /// Nimmt eine Ablage von der Platte auf. Nur für erwartete Pfade, nur
    /// lesbare, und nur die erste je Pfad.
    pub fn ablage_laden(&mut self, ablage: Ablage) -> bool {
        if !self.erwartet.contains(&ablage.pfad) || self.ablagen.contains_key(&ablage.pfad) || !ablage.gut() {
            return false;
        }
        self.ablagen.insert(ablage.pfad.clone(), ablage);
        true
    }

    /// Die erwarteten Pfade ohne Ablage: Für sie fehlt noch das Angebot eines Helfers.
    pub fn neu(&self) -> Vec<String> {
        self.erwartet.iter().filter(|p| !self.ablagen.contains_key(*p)).cloned().collect()
    }

    /// Die offenen Anfragen: je Pfad mit Ablage und ohne geöffneten
    /// Schlüssel eine. Eine schon gestellte Anfrage bleibt, bis eine Antwort
    /// sie verbraucht; so stört ein zweites Lebenszeichen keine laufende Antwort.
    pub fn anfragen(&mut self, rng: &mut (impl rand_core::RngCore + rand_core::CryptoRng)) -> Vec<Anfrage> {
        let mut aus = Vec::new();
        for (pfad, ablage) in &self.ablagen {
            if self.vorhanden.contains_key(pfad) {
                continue;
            }
            if !self.offen.contains_key(pfad) {
                let Some((_, verschlossen)) = ablage.teile() else { continue };
                let Ok((blende, punkt)) = crate::verblendung::anfragen(&verschlossen, rng) else { continue };
                self.offen.insert(pfad.clone(), (blende, crate::passkey::b64(&punkt)));
            }
            if let Some((_, punkt)) = self.offen.get(pfad) {
                aus.push(Anfrage { pfad: pfad.clone(), chip: ablage.chip.clone(), punkt: punkt.clone() });
            }
        }
        aus
    }

    /// Prüft ein Angebot: Ein Helfer bietet für `pfad` den öffentlichen
    /// Schlüssel eines Chips an. Der Server würfelt den Schlüssel der
    /// Nutzlast und verschließt ihn daran. Am Bund ändert sich noch nichts:
    /// Erst muss die Ablage auf der Platte liegen, dann [`Self::einsetzen`].
    pub fn vorbereiten(
        &self,
        pfad: &str,
        name: &str,
        etikett: &str,
        chip: &[u8],
        rng: &mut (impl rand_core::RngCore + rand_core::CryptoRng),
    ) -> Result<Vorbereitet, Uebernahme> {
        if !self.erwartet.contains(pfad) {
            return Err(Uebernahme::Unbekannt);
        }
        if self.ablagen.contains_key(pfad) {
            return Err(Uebernahme::Vorhanden);
        }
        if !beschriftung_gut(name, NAME_HOECHSTENS) || !beschriftung_gut(etikett, ETIKETT_HOECHSTENS) || chip.len() != crate::verblendung::PUNKT {
            return Err(Uebernahme::Unlesbar);
        }
        let mut wert: Wert = zeroize::Zeroizing::new([0u8; 32]);
        rng.try_fill_bytes(wert.as_mut_slice()).map_err(|_| Uebernahme::Unlesbar)?;
        let verschlossen = crate::verblendung::verschliessen(chip, pfad.as_bytes(), wert.as_slice(), rng).map_err(|_| Uebernahme::Unlesbar)?;
        let ablage = Ablage {
            pfad: pfad.to_string(),
            name: name.trim().to_string(),
            etikett: etikett.trim().to_string(),
            chip: crate::passkey::b64(chip),
            verschlossen: crate::passkey::b64(&verschlossen),
        };
        Ok(Vorbereitet { ablage, wert })
    }

    /// Setzt ein vorbereitetes Angebot ein, nachdem seine Ablage geschrieben ist.
    pub fn einsetzen(&mut self, v: Vorbereitet) -> Uebernahme {
        if self.ablagen.contains_key(&v.ablage.pfad) {
            return Uebernahme::Vorhanden;
        }
        self.vorhanden.insert(v.ablage.pfad.clone(), v.wert);
        self.ablagen.insert(v.ablage.pfad.clone(), v.ablage);
        Uebernahme::Gesetzt
    }

    /// Nimmt die Antwort des Chips auf die offene Anfrage zu `pfad`: die
    /// x-Koordinate seiner Schlüsselvereinbarung. Die Anfrage ist danach
    /// verbraucht, ob die Antwort öffnet oder nicht.
    pub fn antworten(&mut self, pfad: &str, x: &[u8]) -> Uebernahme {
        if !self.erwartet.contains(pfad) {
            return Uebernahme::Unbekannt;
        }
        if self.vorhanden.contains_key(pfad) {
            return Uebernahme::Vorhanden;
        }
        let (Some((blende, _)), Some((chip, verschlossen))) = (self.offen.remove(pfad), self.ablagen.get(pfad).and_then(Ablage::teile)) else {
            return Uebernahme::Unerwartet;
        };
        let offen = crate::verblendung::oeffnen(blende, &verschlossen, &chip, pfad.as_bytes(), x).ok();
        match offen.and_then(|k| <[u8; 32]>::try_from(k.as_slice()).ok()) {
            Some(k) => {
                self.vorhanden.insert(pfad.to_string(), zeroize::Zeroizing::new(k));
                Uebernahme::Gesetzt
            }
            None => Uebernahme::Falsch,
        }
    }

    /// Der Schlüssel für `pfad`, wenn er geöffnet ist.
    pub fn holen(&self, pfad: &str) -> Option<Wert> {
        self.vorhanden.get(pfad).cloned()
    }

    /// Alle erwarteten Pfade mit Name, Etikett, Chip und ob geöffnet ist; nie die Werte.
    pub fn beschreiben(&self) -> Vec<Beschreibung> {
        self.erwartet
            .iter()
            .map(|pfad| {
                let a = self.ablagen.get(pfad);
                Beschreibung {
                    pfad: pfad.clone(),
                    name: a.map(|a| a.name.clone()).unwrap_or_default(),
                    etikett: a.map(|a| a.etikett.clone()).unwrap_or_default(),
                    chip: a.map(|a| a.chip.clone()).unwrap_or_default(),
                    vorhanden: self.vorhanden.contains_key(pfad),
                }
            })
            .collect()
    }
}

/// Der geteilte Schlüsselbund: zwischen den Endpunkten des Helfers und den
/// Stellen, die Schlüssel brauchen.
pub type Bund = std::sync::Arc<std::sync::Mutex<SchluesselBund>>;

/// Ein geteilter Bund für diese Pfade.
pub fn bund_fuer(pfade: &[&str]) -> Bund {
    std::sync::Arc::new(std::sync::Mutex::new(SchluesselBund::fuer(pfade)))
}

/// Wanduhr in Sekunden seit 1970.
pub fn wand_jetzt() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

// ----- Anmeldung und Schein des Helfers -----

/// So lange gilt ein Schein. Danach meldet sich der Helfer neu an.
pub const SCHEIN_SEKUNDEN: i64 = 12 * 3600;
/// `sub` und `aud` des Scheins. Der Ausweis einer Person trägt andere Werte
/// und andere Felder; keiner von beiden geht als der andere durch.
pub const SCHEIN_SUB: &str = "helfer";
pub const SCHEIN_AUD: &str = "dashboard-helfer";
/// Länge des Bindungswerts einer Verbindung (RFC 9266).
pub const BINDUNG: usize = 32;
/// Marke und Länge für den Bindungswert, wie RFC 9266 sie festlegt.
pub const BINDUNG_MARKE: &[u8] = b"EXPORTER-Channel-Binding";
/// Was vor dem Bindungswert steht, wenn der Helfer seine Anmeldung signiert:
/// Die Signatur taugt so für nichts anderes.
pub const ANMELDE_MARKE: &[u8] = b"dashboard-helfer/anmeldung/v1\0";

/// Der Schein des Helfers: ein signiertes Token nur für die Endpunkte der
/// Helfer-Leitung.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Schein {
    pub sub: String,
    pub aud: String,
    pub iat: i64,
    pub exp: i64,
    pub jti: String,
    /// Der öffentliche Schlüssel, mit dem sich der Helfer angemeldet hat (Base64url).
    pub kid: String,
}

impl Schein {
    /// Ein neuer Schein für den Helfer mit dem öffentlichen Schlüssel `oeffentlich`.
    pub fn neu(oeffentlich: &[u8], jti: &[u8], jetzt: i64) -> Self {
        Self {
            sub: SCHEIN_SUB.into(),
            aud: SCHEIN_AUD.into(),
            iat: jetzt,
            exp: jetzt + SCHEIN_SEKUNDEN,
            jti: crate::passkey::b64(jti),
            kid: crate::passkey::b64(oeffentlich),
        }
    }

    /// Gilt der Schein zur Zeit `jetzt`? Zweck und Zeiten; die Signatur
    /// prüft, wer ihn liest.
    pub fn gilt(&self, jetzt: i64) -> bool {
        self.sub == SCHEIN_SUB
            && self.aud == SCHEIN_AUD
            && self.iat <= jetzt + 60
            && self.iat < self.exp
            && self.exp - self.iat <= SCHEIN_SEKUNDEN
            && jetzt < self.exp
    }
}

/// Die Nachricht, die der Helfer bei der Anmeldung signiert: Marke und
/// Bindungswert der Verbindung.
pub fn anmelde_nachricht(bindung: &[u8; BINDUNG]) -> Vec<u8> {
    [ANMELDE_MARKE, bindung.as_slice()].concat()
}

/// Stimmt die Signatur des Helfers über den Bindungswert dieser Verbindung?
/// `oeffentlich`: P-256, 65 Bytes. `signatur`: ECDSA mit SHA-256, r und s zu je 32 Bytes.
pub fn signatur_gilt(oeffentlich: &[u8], signatur: &[u8], bindung: &[u8; BINDUNG]) -> bool {
    use p256::ecdsa::signature::Verifier;
    let (Ok(schluessel), Ok(signatur)) = (p256::ecdsa::VerifyingKey::from_sec1_bytes(oeffentlich), p256::ecdsa::Signature::from_slice(signatur)) else {
        return false;
    };
    oeffentlich.len() == crate::verblendung::PUNKT && schluessel.verify(&anmelde_nachricht(bindung), &signatur).is_ok()
}

/// Steht `oeffentlich` in der Liste der zugelassenen Helfer? Die Liste hat
/// je Zeile einen öffentlichen Schlüssel (Base64url) und danach einen Namen;
/// leere Zeilen und Zeilen mit `#` zählen nicht. Gibt den Namen zurück.
pub fn zugelassen(liste: &str, oeffentlich: &[u8]) -> Option<String> {
    liste.lines().map(str::trim).filter(|z| !z.is_empty() && !z.starts_with('#')).find_map(|zeile| {
        let (schluessel, name) = zeile.split_once(char::is_whitespace).unwrap_or((zeile, ""));
        (crate::passkey::b64_lesen(schluessel).as_deref() == Some(oeffentlich)).then(|| name.trim().to_string())
    })
}

#[cfg(unix)]
pub mod leitung;
#[cfg(unix)]
pub mod zertifikat;

#[cfg(test)]
mod tests {
    use super::*;

    fn nach(t: Instant, s: u64) -> Instant {
        t + Duration::from_secs(s)
    }

    #[test]
    fn ausgeschaltet_gilt_jede_anmeldung_und_nichts_ist_gesperrt() {
        let mut s = SperrStand::default();
        let t = Instant::now();
        assert!(!s.eingeschaltet() && !s.ist_gesperrt(t));
        assert!(s.gilt(1, t, 1000));
        // Auch Meldungen des Helfers ändern daran nichts.
        s.sperre(t, 1000);
        assert!(!s.ist_gesperrt(t));
        assert!(s.gilt(1, t, 1001));
    }

    #[test]
    fn eingeschaltet_ohne_helfer_ist_gesperrt() {
        let mut s = SperrStand::default();
        let t = Instant::now();
        s.einschalten(1000);
        assert!(s.ist_gesperrt(t));
        // Auch eine Anmeldung von eben gilt nicht, solange der Helfer fehlt.
        assert!(!s.gilt(1005, t, 1005));
    }

    #[test]
    fn mit_lebenszeichen_gilt_nur_eine_anmeldung_nach_dem_einschalten() {
        let mut s = SperrStand::default();
        let t = Instant::now();
        s.einschalten(1000);
        s.zeichen(t);
        assert!(!s.ist_gesperrt(t));
        assert!(!s.gilt(999, t, 1001), "Anmeldung vor dem Einschalten");
        assert!(!s.gilt(1000, t, 1001), "Anmeldung in derselben Sekunde");
        assert!(s.gilt(1001, t, 1001));
    }

    #[test]
    fn sperre_entwertet_jede_anmeldung_bis_dahin_und_entsperren_verlangt_eine_neue() {
        let mut s = SperrStand::default();
        let t = Instant::now();
        s.einschalten(1000);
        s.zeichen(t);
        assert!(s.gilt(1010, t, 1010));
        s.sperre(nach(t, 60), 1060);
        assert!(s.ist_gesperrt(nach(t, 60)));
        assert!(!s.gilt(1010, nach(t, 61), 1061));
        // Entsperrt: Die alte Anmeldung gilt nicht mehr, eine neue schon.
        s.entsperrt(nach(t, 120), 1120);
        assert!(!s.ist_gesperrt(nach(t, 121)));
        assert!(!s.gilt(1010, nach(t, 121), 1121));
        assert!(!s.gilt(1120, nach(t, 121), 1121), "Anmeldung in der Sekunde des Entsperrens");
        assert!(s.gilt(1121, nach(t, 121), 1121));
    }

    /// Eine Anmeldung, die während der Sperre entsteht, gilt danach nicht.
    #[test]
    fn anmeldung_waehrend_der_sperre_gilt_auch_danach_nicht() {
        let mut s = SperrStand::default();
        let t = Instant::now();
        s.einschalten(1000);
        s.sperre(t, 1050);
        assert!(!s.gilt(1055, nach(t, 5), 1055));
        // Der Rechner wird entsperrt, ohne dass der Helfer es meldet (er meldet nur noch sein Dasein).
        s.entsperrt(nach(t, 10), 1056);
        assert!(!s.gilt(1055, nach(t, 11), 1057));
        assert!(s.gilt(1057, nach(t, 11), 1057));
    }

    #[test]
    fn ohne_lebenszeichen_ist_nach_der_frist_gesperrt() {
        let mut s = SperrStand::default();
        let t = Instant::now();
        s.einschalten(1000);
        s.zeichen(t);
        assert!(s.gilt(1010, nach(t, 10), 1010));
        // Kurz vor der Frist noch offen, an der Frist gesperrt.
        assert!(!s.ist_gesperrt(t + ZEICHEN_FRIST - Duration::from_secs(1)));
        assert!(s.ist_gesperrt(t + ZEICHEN_FRIST));
        let spaet = t + ZEICHEN_FRIST + Duration::from_secs(5);
        assert!(!s.gilt(1010, spaet, 1030));
    }

    /// Kommt der Helfer nach einer Lücke wieder, ist eine neue Anmeldung nötig:
    /// Was in der Lücke geschah, weiß niemand.
    #[test]
    fn nach_einer_luecke_im_lebenszeichen_ist_eine_neue_anmeldung_noetig() {
        let mut s = SperrStand::default();
        let t = Instant::now();
        s.einschalten(1000);
        s.zeichen(t);
        assert!(s.gilt(1010, nach(t, 5), 1010));
        // Lücke: Eine Anfrage in dieser Zeit rückt die Grenze nach.
        let luecke = t + ZEICHEN_FRIST + Duration::from_secs(10);
        assert!(!s.gilt(1010, luecke, 1040));
        // Der Helfer ist wieder da.
        s.zeichen(luecke + Duration::from_secs(1));
        let danach = luecke + Duration::from_secs(2);
        assert!(!s.ist_gesperrt(danach));
        assert!(!s.gilt(1010, danach, 1042), "die alte Anmeldung lebt nicht wieder auf");
        assert!(s.gilt(1041, danach, 1042));
    }

    #[test]
    fn lebenszeichen_hebt_eine_gemeldete_sperre_nicht_auf() {
        let mut s = SperrStand::default();
        let t = Instant::now();
        s.einschalten(1000);
        s.sperre(t, 1010);
        s.zeichen(nach(t, 5));
        assert!(s.ist_gesperrt(nach(t, 6)));
        assert!(!s.gilt(1020, nach(t, 6), 1020));
    }

    #[test]
    fn die_grenze_geht_nie_zurueck() {
        let mut s = SperrStand::default();
        let t = Instant::now();
        s.einschalten(2000);
        s.zeichen(t);
        // Eine Sperre mit früherer Wanduhr (Uhr zurückgestellt) senkt die Grenze nicht.
        s.sperre(t, 1500);
        s.entsperrt(t, 1600);
        assert!(!s.gilt(1900, t, 1601));
        assert!(s.gilt(2001, t, 1601));
    }

    // ----- Schlüsselbund -----

    use p256::NonZeroScalar;
    use rand_core::OsRng;

    const LINKS: &str = "speicher/aufgaben/links";

    fn chip() -> (NonZeroScalar, [u8; 65]) {
        let privat = NonZeroScalar::random(&mut OsRng);
        let oeffentlich = crate::verblendung::oeffentlich(&privat);
        (privat, oeffentlich)
    }

    /// Was der Helfer tut: dem Chip den Punkt vorlegen.
    fn chip_antwort(privat: &NonZeroScalar, a: &Anfrage) -> [u8; 32] {
        crate::verblendung::chip_antwort(privat, &crate::passkey::b64_lesen(&a.punkt).unwrap()).unwrap()
    }

    #[test]
    fn angebot_setzt_den_schluessel_und_die_ablage_oeffnet_ihn_nach_dem_neustart_wieder() {
        let (privat, oeffentlich) = chip();
        let mut bund = SchluesselBund::fuer(&[LINKS]);
        assert_eq!(bund.neu(), [LINKS]);
        assert!(bund.anfragen(&mut OsRng).is_empty(), "ohne Ablage keine Anfrage");
        let v = bund.vorbereiten(LINKS, "links", "Aufgaben-Links", &oeffentlich, &mut OsRng).unwrap();
        assert!(bund.holen(LINKS).is_none(), "vor dem Einsetzen ändert sich nichts");
        let ablage = v.ablage().clone();
        assert_eq!(bund.einsetzen(v), Uebernahme::Gesetzt);
        let erster = bund.holen(LINKS).unwrap();
        assert!(bund.neu().is_empty());
        assert!(bund.anfragen(&mut OsRng).is_empty(), "geöffnet: keine Anfrage");
        // Der Schlüssel steht nicht in der Ablage.
        let roh = crate::passkey::b64_lesen(&ablage.verschlossen).unwrap();
        assert!(!roh.windows(8).any(|w| w == &erster[..8]));
        // Neustart: nur die Ablage ist geblieben.
        let mut bund = SchluesselBund::fuer(&[LINKS]);
        assert!(bund.ablage_laden(ablage.clone()));
        assert!(!bund.ablage_laden(ablage), "nur die erste je Pfad");
        assert!(bund.holen(LINKS).is_none());
        let anfragen = bund.anfragen(&mut OsRng);
        assert_eq!(anfragen.len(), 1);
        assert_eq!(anfragen[0].chip, crate::passkey::b64(&oeffentlich));
        // Ein zweites Lebenszeichen stellt dieselbe Anfrage noch einmal.
        assert_eq!(bund.anfragen(&mut OsRng), anfragen);
        assert_eq!(bund.antworten(LINKS, &chip_antwort(&privat, &anfragen[0])), Uebernahme::Gesetzt);
        assert_eq!(*bund.holen(LINKS).unwrap(), *erster);
        assert_eq!(bund.antworten(LINKS, &[0u8; 32]), Uebernahme::Vorhanden);
    }

    #[test]
    fn falsche_antwort_oeffnet_nicht_und_verbraucht_die_anfrage() {
        let (privat, oeffentlich) = chip();
        let (fremd, _) = chip();
        let erst = SchluesselBund::fuer(&[LINKS]);
        let v = erst.vorbereiten(LINKS, "links", "Aufgaben-Links", &oeffentlich, &mut OsRng).unwrap();
        let ablage = v.ablage().clone();
        let mut bund = SchluesselBund::fuer(&[LINKS]);
        assert_eq!(bund.antworten(LINKS, &[1u8; 32]), Uebernahme::Unerwartet, "ohne Ablage keine Antwort");
        assert!(bund.ablage_laden(ablage));
        assert_eq!(bund.antworten(LINKS, &[1u8; 32]), Uebernahme::Unerwartet, "ohne Anfrage keine Antwort");
        let a = bund.anfragen(&mut OsRng).remove(0);
        // Ein anderer Chip antwortet.
        assert_eq!(bund.antworten(LINKS, &chip_antwort(&fremd, &a)), Uebernahme::Falsch);
        assert!(bund.holen(LINKS).is_none());
        // Die alte Anfrage ist verbraucht: Auch die richtige Antwort darauf zählt nicht mehr.
        assert_eq!(bund.antworten(LINKS, &chip_antwort(&privat, &a)), Uebernahme::Unerwartet);
        // Die nächste Anfrage ist eine neue, und der richtige Chip öffnet.
        let b = bund.anfragen(&mut OsRng).remove(0);
        assert_ne!(a.punkt, b.punkt);
        assert_eq!(bund.antworten(LINKS, &chip_antwort(&privat, &b)), Uebernahme::Gesetzt);
        // Kein Punkt, zu kurz: falsch, nie ein Absturz.
        let mut bund2 = SchluesselBund::fuer(&[LINKS]);
        let v = erst.vorbereiten(LINKS, "links", "Aufgaben-Links", &oeffentlich, &mut OsRng).unwrap();
        assert!(bund2.ablage_laden(v.ablage().clone()));
        bund2.anfragen(&mut OsRng);
        assert_eq!(bund2.antworten(LINKS, b"kurz"), Uebernahme::Falsch);
    }

    #[test]
    fn mehrere_nutzlasten_und_nicht_jeder_chip_oeffnet_jede() {
        let (privat_a, oeffentlich_a) = chip();
        let (privat_b, oeffentlich_b) = chip();
        let pfade = [LINKS, "speicher/zweite/nutzlast"];
        let mut erst = SchluesselBund::fuer(&pfade);
        let va = erst.vorbereiten(pfade[0], "links", "Aufgaben-Links", &oeffentlich_a, &mut OsRng).unwrap();
        let vb = erst.vorbereiten(pfade[1], "zweite", "Zweite Nutzlast", &oeffentlich_b, &mut OsRng).unwrap();
        let (aa, ab) = (va.ablage().clone(), vb.ablage().clone());
        assert_eq!(erst.einsetzen(va), Uebernahme::Gesetzt);
        assert_eq!(erst.neu(), [pfade[1]]);
        assert_eq!(erst.einsetzen(vb), Uebernahme::Gesetzt);
        assert_ne!(*erst.holen(pfade[0]).unwrap(), *erst.holen(pfade[1]).unwrap(), "je Pfad ein eigener Schlüssel");
        let mut bund = SchluesselBund::fuer(&pfade);
        assert!(bund.ablage_laden(aa) && bund.ablage_laden(ab));
        let anfragen = bund.anfragen(&mut OsRng);
        assert_eq!(anfragen.iter().map(|a| a.pfad.as_str()).collect::<Vec<_>>(), [LINKS, "speicher/zweite/nutzlast"]);
        // Helfer A hat nur Chip A: Er öffnet nur, was an seinen Schlüssel verschlossen ist.
        let fuer_a: Vec<&Anfrage> = anfragen.iter().filter(|a| a.chip == crate::passkey::b64(&oeffentlich_a)).collect();
        assert_eq!(fuer_a.len(), 1);
        assert_eq!(bund.antworten(&fuer_a[0].pfad, &chip_antwort(&privat_a, fuer_a[0])), Uebernahme::Gesetzt);
        assert!(bund.holen(pfade[0]).is_some() && bund.holen(pfade[1]).is_none());
        // Die Antwort von Chip A auf die Anfrage der zweiten Nutzlast öffnet sie nicht.
        assert_eq!(bund.antworten(pfade[1], &chip_antwort(&privat_a, &anfragen[1])), Uebernahme::Falsch);
        let b = bund.anfragen(&mut OsRng).remove(0);
        assert_eq!(b.pfad, pfade[1]);
        assert_eq!(bund.antworten(pfade[1], &chip_antwort(&privat_b, &b)), Uebernahme::Gesetzt);
    }

    #[test]
    fn bund_nimmt_nur_erwartete_pfade_und_lesbare_angebote_und_tauscht_nie() {
        let (_, oeffentlich) = chip();
        let (_, anderer) = chip();
        let mut bund = SchluesselBund::fuer(&[LINKS]);
        assert_eq!(bund.vorbereiten("fremder/pfad", "n", "e", &oeffentlich, &mut OsRng).unwrap_err(), Uebernahme::Unbekannt);
        assert_eq!(bund.antworten("fremder/pfad", &[0u8; 32]), Uebernahme::Unbekannt);
        for (name, etikett) in [("", "e"), ("n", " "), ("n\u{7}", "e"), (&*"x".repeat(NAME_HOECHSTENS + 1), "e"), ("n", &*"x".repeat(ETIKETT_HOECHSTENS + 1))] {
            assert_eq!(bund.vorbereiten(LINKS, name, etikett, &oeffentlich, &mut OsRng).unwrap_err(), Uebernahme::Unlesbar);
        }
        assert_eq!(bund.vorbereiten(LINKS, "n", "e", &oeffentlich[..64], &mut OsRng).unwrap_err(), Uebernahme::Unlesbar);
        assert_eq!(bund.vorbereiten(LINKS, "n", "e", &[4u8; 65], &mut OsRng).unwrap_err(), Uebernahme::Unlesbar, "kein Punkt auf der Kurve");
        // Zwei Angebote zugleich: Das erste, das eingesetzt wird, gilt.
        let v1 = bund.vorbereiten(LINKS, "eins", "Erster", &oeffentlich, &mut OsRng).unwrap();
        let v2 = bund.vorbereiten(LINKS, "zwei", "Zweiter", &anderer, &mut OsRng).unwrap();
        assert_eq!(bund.einsetzen(v1), Uebernahme::Gesetzt);
        let erster = bund.holen(LINKS).unwrap();
        assert_eq!(bund.einsetzen(v2), Uebernahme::Vorhanden);
        assert_eq!(*bund.holen(LINKS).unwrap(), *erster);
        assert_eq!(bund.vorbereiten(LINKS, "drei", "Dritter", &anderer, &mut OsRng).unwrap_err(), Uebernahme::Vorhanden);
        // Die Beschreibung nennt Name, Etikett und Chip, nie den Wert.
        let b = bund.beschreiben();
        assert_eq!(b.len(), 1);
        assert_eq!((b[0].name.as_str(), b[0].etikett.as_str(), b[0].vorhanden), ("eins", "Erster", true));
        assert_eq!(b[0].chip, crate::passkey::b64(&oeffentlich));
        let text = format!("{b:?} {bund:?} {}", serde_json::to_string(&b).unwrap());
        assert!(!text.contains(&crate::passkey::b64(erster.as_slice())));
    }

    #[test]
    fn ablage_laden_weist_fremdes_und_unlesbares_ab() {
        let (_, oeffentlich) = chip();
        let erst = SchluesselBund::fuer(&[LINKS]);
        let gut = erst.vorbereiten(LINKS, "links", "Aufgaben-Links", &oeffentlich, &mut OsRng).unwrap().ablage().clone();
        let mut bund = SchluesselBund::fuer(&[LINKS]);
        assert!(!bund.ablage_laden(Ablage { pfad: "fremder/pfad".into(), ..gut.clone() }));
        assert!(!bund.ablage_laden(Ablage { chip: "kein base64!".into(), ..gut.clone() }));
        assert!(!bund.ablage_laden(Ablage { chip: crate::passkey::b64(&oeffentlich[..10]), ..gut.clone() }));
        assert!(!bund.ablage_laden(Ablage { name: String::new(), ..gut.clone() }));
        assert!(bund.neu() == [LINKS]);
        assert!(bund.ablage_laden(gut));
        // Unbekannte Felder in der Datei: abgewiesen statt überlesen.
        assert!(serde_json::from_str::<Ablage>(r#"{"pfad":"a","name":"n","etikett":"e","chip":"c","verschlossen":"v","mehr":1}"#).is_err());
        // Der Dateiname ist fest je Pfad und zeigt den Pfad nicht.
        assert_eq!(ablage_datei(LINKS), ablage_datei(LINKS));
        assert_ne!(ablage_datei(LINKS), ablage_datei("speicher/zweite/nutzlast"));
        assert!(!ablage_datei(LINKS).contains("links"));
    }

    // ----- Anmeldung und Schein -----

    fn helfer_schluessel() -> (p256::ecdsa::SigningKey, Vec<u8>) {
        let s = p256::ecdsa::SigningKey::random(&mut OsRng);
        let o = s.verifying_key().to_encoded_point(false).as_bytes().to_vec();
        (s, o)
    }

    fn signieren(s: &p256::ecdsa::SigningKey, bindung: &[u8; BINDUNG]) -> Vec<u8> {
        use p256::ecdsa::signature::Signer;
        let sig: p256::ecdsa::Signature = s.sign(&anmelde_nachricht(bindung));
        sig.to_bytes().to_vec()
    }

    #[test]
    fn die_signatur_gilt_nur_fuer_diese_verbindung_und_diesen_schluessel() {
        let (s, o) = helfer_schluessel();
        let (_, fremd) = helfer_schluessel();
        let bindung = [7u8; BINDUNG];
        let sig = signieren(&s, &bindung);
        assert!(signatur_gilt(&o, &sig, &bindung));
        assert!(!signatur_gilt(&o, &sig, &[8u8; BINDUNG]), "andere Verbindung");
        assert!(!signatur_gilt(&fremd, &sig, &bindung), "anderer Schlüssel");
        let mut kaputt = sig.clone();
        kaputt[10] ^= 1;
        assert!(!signatur_gilt(&o, &kaputt, &bindung));
        assert!(!signatur_gilt(&o, &sig[..63], &bindung));
        assert!(!signatur_gilt(&o[..64], &sig, &bindung));
        assert!(!signatur_gilt(&[], &[], &bindung));
        // Eine Signatur über den bloßen Bindungswert, ohne Marke, gilt nicht.
        use p256::ecdsa::signature::Signer;
        let ohne_marke: p256::ecdsa::Signature = s.sign(&bindung);
        assert!(!signatur_gilt(&o, &ohne_marke.to_bytes(), &bindung));
    }

    #[test]
    fn zugelassen_liest_die_liste_zeile_fuer_zeile() {
        let (_, a) = helfer_schluessel();
        let (_, b) = helfer_schluessel();
        let (_, c) = helfer_schluessel();
        let liste = format!("# Helfer dieses Rechners\n\n{}  Schreibtisch \n{}\n", crate::passkey::b64(&a), crate::passkey::b64(&b));
        assert_eq!(zugelassen(&liste, &a).as_deref(), Some("Schreibtisch"));
        assert_eq!(zugelassen(&liste, &b).as_deref(), Some(""));
        assert_eq!(zugelassen(&liste, &c), None);
        assert_eq!(zugelassen("", &a), None);
        assert_eq!(zugelassen(&format!("#{}", crate::passkey::b64(&a)), &a), None, "auskommentiert zählt nicht");
    }

    #[test]
    fn der_schein_gilt_nur_in_seiner_zeit_und_fuer_seinen_zweck() {
        let s = Schein::neu(&[4u8; 65], &[1u8; 16], 1000);
        assert!(s.gilt(1000));
        assert!(s.gilt(1000 + SCHEIN_SEKUNDEN - 1));
        assert!(!s.gilt(1000 + SCHEIN_SEKUNDEN));
        assert!(!s.gilt(900), "aus der Zukunft");
        assert!(!Schein { aud: "dashboard".into(), ..s.clone() }.gilt(1000));
        assert!(!Schein { sub: "inhaber".into(), ..s.clone() }.gilt(1000));
        assert!(!Schein { exp: s.exp + 1, ..s.clone() }.gilt(1000), "länger als erlaubt");
    }

    #[test]
    fn schein_und_ausweis_gehen_nie_als_der_andere_durch() {
        use crate::passkey::jwt::{Ausweis, JwtSchluessel};
        let k = JwtSchluessel::zufaellig();
        let jetzt = 1_800_000_000;
        // Selbst mit demselben Schlüssel signiert: Der Schein ist kein Ausweis …
        let schein = k.signieren(&Schein::neu(&[4u8; 65], &[1u8; 16], jetzt));
        assert!(k.pruefen(&schein, jetzt).is_err());
        assert!(k.lesen::<Schein>(&schein).unwrap().gilt(jetzt));
        // … und der Ausweis kein Schein.
        let ausweis = k.ausstellen(&Ausweis::neu("cid", jetzt));
        assert!(k.pruefen(&ausweis, jetzt).is_ok());
        assert!(k.lesen::<Schein>(&ausweis).is_err());
        // Ein Schein von einem anderen Schlüssel gilt nicht.
        assert!(JwtSchluessel::zufaellig().lesen::<Schein>(&schein).is_err());
    }
}
