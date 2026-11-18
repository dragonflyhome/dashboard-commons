//! Etappe 7a der macOS-Folge: der lokale Kanal.
//!
//! Außerhalb von Windows gibt es keine Anmeldung, an der das Betriebssystem
//! das Konto bezeugt. Diesen Platz nimmt ein Unix-Socket ein: Er liegt im
//! Schlüsselordner (0700, eigener Besitz) neben dem Schlüssel der Ausweise,
//! hat die Rechte 0600, und bei jeder Verbindung nennt das Betriebssystem die
//! Benutzernummer der Gegenseite (`peer_cred`). Nur die eigene Nummer wird
//! bedient. Gegen root ist der Kanal keine Grenze und behauptet das nicht.
//!
//! Auf dem Kanal läuft kein HTTP und nicht der Router, sondern ein eigenes
//! Zeilenprotokoll: eine Zeile JSON hin, eine Zeile JSON zurück, dann Schluss.
//! Keine Antwort auf dem Kanal meldet einen Browser an. In 7a gibt es genau
//! einen Befehl, `wer`. Den Einmal-Code (`code`) bringt 7b. Schritt 7c bringt
//! den Beleg der zwei Adressen (`beleg`; `code` verlangt seitdem die `frage`
//! eines bestandenen Belegs) und den Befehl `dashboard einmal-code` im
//! Terminal, der den Kanal anspricht (Untermodul `befehl`).
//!
//! Die reinen Teile (Ort, Anfrage lesen, Antwort bauen, Urteil über eine
//! vorhandene Datei, über den Socket und über die Gegenseite) stehen unter
//! `cfg(any(unix, test))` und werden auf jeder Plattform geprüft. Ein- und
//! Ausgabe gibt es nur unter `cfg(unix)`. Unter Windows gibt es den Kanal nicht.

use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Name des Sockets im Schlüsselordner.
pub const KANAL_DATEI: &str = "kanal";

/// Eine Anfrage hat mit Zeilenende höchstens so viele Bytes.
pub const HOECHSTENS_BYTES: usize = 1024;

/// Satz, wenn es für den Kanal keinen Ort gibt.
pub const KEIN_ORT_SATZ: &str = "Für den lokalen Kanal gibt es keinen Ort; den lokalen Kanal gibt es in diesem Lauf nicht.";

pub const SATZ_ZU_LANG: &str = "Die Anfrage ist zu lang.";
pub const SATZ_OHNE_ENDE: &str = "Die Anfrage endet ohne Zeilenende.";
pub const SATZ_KEIN_UTF8: &str = "Die Anfrage ist kein UTF-8.";
pub const SATZ_KEIN_JSON: &str = "Die Anfrage ist kein gültiges JSON.";
pub const SATZ_KEIN_OBJEKT: &str = "Die Anfrage muss ein Objekt mit dem Feld befehl sein.";
pub const SATZ_FELD: &str = "Die Anfrage hat nicht genau das Feld „befehl“.";
pub const SATZ_BEFEHL: &str = "Diesen Befehl gibt es nicht.";
pub const SATZ_FRAGE: &str = "Nur der Befehl „code“ nimmt das Feld „frage“, als Zeichenkette.";

/// Der Ort des Sockets: immer neben dem Schlüssel der Ausweise. Ohne diesen
/// Ort, oder wenn der Name mit dem einer Schlüsseldatei zusammenfiele, gibt
/// es keinen Kanal. Groß- und Kleinschreibung zählen nicht, wie bei den
/// Schlüsseln (APFS unterscheidet sie gewöhnlich nicht).
pub fn ort_neben(jwt_datei: Option<&Path>) -> Option<PathBuf> {
    let jwt = jwt_datei?;
    let name = jwt.file_name()?.to_string_lossy().to_lowercase();
    if name == KANAL_DATEI {
        return None;
    }
    Some(jwt.with_file_name(KANAL_DATEI))
}

/// Die Gegenseite einer Verbindung auf dem Kanal, nachdem das Betriebssystem
/// ihre Benutzernummer bezeugt hat und sie die erwartete ist. Eine Befähigung:
/// Das Feld ist privat, den Wert erzeugt nur der Lauscher nach der Prüfung.
#[derive(Debug)]
pub struct KanalKonto {
    uid: u32,
}

impl KanalKonto {
    /// Nur nach [`gegenseite_urteil`]; darum nicht öffentlich.
    fn nach_pruefung(uid: u32) -> Self {
        Self { uid }
    }

    /// Die bezeugte Benutzernummer.
    pub fn uid(&self) -> u32 {
        self.uid
    }
}

/// Nur für Tests: die Befähigung, wie der Lauscher sie nach der Prüfung
/// gibt. Außerhalb des Testbaus entsteht sie nur dort. Gebraucht von den
/// Tests der Wege über einen Einmal-Code, die es nur unter Unix gibt; darum
/// auch nur dort vorhanden (unter Windows wäre die Funktion unbenutzt).
#[cfg(all(test, unix))]
pub(crate) fn konto_fuer_tests() -> KanalKonto {
    KanalKonto::nach_pruefung(0)
}

/// Darf die Gegenseite bedient werden? Nur wenn das Betriebssystem ihre
/// Benutzernummer nennt und sie die erwartete ist. Gibt dann die Befähigung.
/// Nicht öffentlich: Außerhalb dieses Moduls entsteht kein [`KanalKonto`].
fn gegenseite_urteil(bezeugt: Option<u32>, erwartet: u32) -> Option<KanalKonto> {
    bezeugt.filter(|&uid| uid == erwartet).map(KanalKonto::nach_pruefung)
}

/// Die Befehle des Kanals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Befehl {
    /// Nennt Fassung und Konto; ändert nichts.
    Wer,
    /// Gibt eine Frage für den Beleg der zwei Adressen aus (Schritt 7c).
    Beleg,
    /// Gibt einen Einmal-Code für das Anlegen eines Passkeys aus (Schritt
    /// 7b), seit 7c nur mit der `frage` eines bestandenen Belegs.
    Code,
}

/// Eine gelesene Anfrage: der Befehl, bei `code` die Frage des Belegs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Anfrage {
    pub befehl: Befehl,
    /// Nur bei `code`; fehlt sie, gibt es keinen Code.
    pub frage: Option<String>,
}

/// Wer Frage und Einmal-Code ausgibt. Der Kanal selbst kennt den Zustand des
/// Servers nicht; die Quelle bekommt er beim Start. Sie verlangt die
/// Befähigung der geprüften Gegenseite und gibt die fertige Antwortzeile
/// zurück (ohne Zeilenende).
pub trait CodeQuelle: Send + Sync {
    /// Der Befehl `beleg`: eine neue Frage und der Port.
    fn beleg<'a>(&'a self, konto: &'a KanalKonto) -> std::pin::Pin<Box<dyn std::future::Future<Output = String> + Send + 'a>>;
    /// Der Befehl `code`: ein Einmal-Code, nur nach bestandenem Beleg mit `frage`.
    fn code<'a>(&'a self, konto: &'a KanalKonto, frage: Option<&'a str>) -> std::pin::Pin<Box<dyn std::future::Future<Output = String> + Send + 'a>>;
}

/// Satz für die Befehle `beleg` und `code`, wenn der Kanal keine Quelle hat.
pub const SATZ_KEIN_CODE: &str = "Einen Einmal-Code gibt es in diesem Lauf nicht.";

/// Liest die Anfrage aus dem, was von der Verbindung kam (höchstens
/// [`HOECHSTENS_BYTES`], bis zum ersten Zeilenende). Jeder Fehler ist ein
/// fester Satz; nichts aus der Anfrage geht in ihn ein.
pub fn anfrage_lesen(roh: &[u8]) -> Result<Anfrage, &'static str> {
    let Some(zeile) = roh.strip_suffix(b"\n") else {
        return Err(if roh.len() >= HOECHSTENS_BYTES { SATZ_ZU_LANG } else { SATZ_OHNE_ENDE });
    };
    let text = std::str::from_utf8(zeile).map_err(|_| SATZ_KEIN_UTF8)?;
    // Erst die Form: ein Objekt mit genau dem Feld „befehl“. Ohne diesen
    // Schritt läse serde auch eine Liste wie `["wer"]` als Anfrage.
    let wert: serde_json::Value = serde_json::from_str(text).map_err(|_| SATZ_KEIN_JSON)?;
    let serde_json::Value::Object(felder) = wert else {
        return Err(SATZ_KEIN_OBJEKT);
    };
    if !felder.contains_key("befehl") || felder.keys().any(|k| k != "befehl" && k != "frage") {
        return Err(SATZ_FELD);
    }
    let befehl = serde_json::from_value::<Befehl>(felder["befehl"].clone()).map_err(|_| SATZ_BEFEHL)?;
    let frage = match felder.get("frage") {
        None => None,
        Some(serde_json::Value::String(f)) if befehl == Befehl::Code => Some(f.clone()),
        Some(_) => return Err(SATZ_FRAGE),
    };
    Ok(Anfrage { befehl, frage })
}

/// Die Antwort auf `wer`, eine Zeile JSON ohne Zeilenende. Wer hier
/// ankommt, ist das Konto, in dem der Server läuft.
pub fn antwort_wer(_konto: &KanalKonto) -> String {
    serde_json::json!({ "fassung": crate::fassungen::LAUFEND, "hauptkonto": true }).to_string()
}

/// Die Antwort auf eine Anfrage, die nicht ausgeführt wird.
pub fn fehler_antwort(satz: &'static str) -> String {
    serde_json::json!({ "fehler": satz }).to_string()
}

/// Was am Ort des Sockets schon liegt (aus `lstat`, ohne einer Verknüpfung zu folgen).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Vorhanden {
    pub ist_socket: bool,
    pub uid: u32,
}

/// Ausgang des Verbindungsversuchs zu einem vorhandenen Socket.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Probe {
    /// Genau `ECONNREFUSED`: Niemand lauscht mehr.
    Abgewiesen,
    /// Die Verbindung kam zustande: Ein anderer Lauf bedient den Kanal.
    Antwortet,
    /// Jeder andere Fehler.
    Sonst,
}

/// Was mit dem geschieht, was am Ort des Sockets liegt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AltUrteil {
    /// Ein eigener Socket, auf dem niemand mehr lauscht: entfernen und neu binden.
    Ersetzen,
    /// Ein eigener Socket, der antwortet: bleibt, dieser Lauf hat keinen Kanal.
    Lebt,
    /// Alles andere (gewöhnliche Datei, Verknüpfung, FIFO, Ordner, fremder
    /// Besitz, anderer Fehler): bleibt unverändert, kein Kanal.
    Bleibt,
}

/// Urteil über das, was am Ort liegt. `proben` wird nur für einen eigenen
/// Socket aufgerufen; zu allem anderen wird nie eine Verbindung versucht.
pub fn alt_urteil(vorhanden: Vorhanden, eigene_uid: u32, proben: &dyn Fn() -> Probe) -> AltUrteil {
    if !vorhanden.ist_socket || vorhanden.uid != eigene_uid {
        return AltUrteil::Bleibt;
    }
    match proben() {
        Probe::Abgewiesen => AltUrteil::Ersetzen,
        Probe::Antwortet => AltUrteil::Lebt,
        Probe::Sonst => AltUrteil::Bleibt,
    }
}

/// Ist der eben gebundene Socket in Ordnung? Ein Socket, der der eigenen
/// Benutzernummer gehört und Gruppe und anderen kein Recht gibt.
pub fn socket_urteil(ist_socket: bool, uid: u32, modus: u32, eigene_uid: u32) -> bool {
    ist_socket && uid == eigene_uid && modus & 0o077 == 0
}

#[cfg(unix)]
pub use lauscher::{FRIST, HOECHSTENS_ZUGLEICH, Kanal, SATZ_FRIST, starten};

/// Ein- und Ausgabe des Kanals.
#[cfg(unix)]
mod lauscher {
    use std::future::Future;
    use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::time::Duration;

    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt};
    use tokio::net::{UnixListener, UnixStream};
    use tokio::sync::Semaphore;
    use tracing::{info, warn};

    use super::{AltUrteil, Anfrage, Befehl, CodeQuelle, HOECHSTENS_BYTES, KanalKonto, Probe, Vorhanden};
    use crate::passkey::geheim::{self, Rahmen, Stand};

    /// Frist für die ganze Verbindung.
    pub const FRIST: Duration = Duration::from_secs(2);

    /// So viele Verbindungen werden zugleich bedient; weitere werden sofort geschlossen.
    pub const HOECHSTENS_ZUGLEICH: usize = 4;

    pub const SATZ_FRIST: &str = "Die Anfrage kam nicht rechtzeitig.";

    /// Pause nach einem Fehler beim Annehmen, damit die Schleife nicht leer dreht.
    const PAUSE_NACH_FEHLER: Duration = Duration::from_millis(100);

    /// Die Sätze zum Schlüsselordner, hier mit der Folge für den Kanal.
    const RAHMEN_KANAL: Rahmen = Rahmen {
        folge: "Den lokalen Kanal gibt es in diesem Lauf nicht",
        folge_im_satz: "den lokalen Kanal gibt es in diesem Lauf nicht",
        name: "Lokaler Kanal",
        nach_dem_entfernen: "",
        fehlt: "Der Schlüsselordner fehlt; den lokalen Kanal gibt es in diesem Lauf nicht",
        ausnahme_bei_neuer_datei: false,
    };

    /// Der laufende Kanal. Beim Freigeben endet das Annehmen, und der eigene
    /// Socket wird entfernt.
    #[derive(Debug)]
    pub struct Kanal {
        ort: PathBuf,
        /// Gerät und Inode des gebundenen Sockets: Entfernt wird nur genau er.
        kennung: (u64, u64),
        aufgabe: tokio::task::JoinHandle<()>,
    }

    impl Kanal {
        /// Wo der Socket liegt.
        pub fn ort(&self) -> &Path {
            &self.ort
        }
    }

    impl Drop for Kanal {
        fn drop(&mut self) {
            self.aufgabe.abort();
            if let Ok(m) = std::fs::symlink_metadata(&self.ort)
                && m.file_type().is_socket()
                && (m.dev(), m.ino()) == self.kennung
            {
                let _ = std::fs::remove_file(&self.ort);
            }
        }
    }

    fn zeigen(p: &Path) -> String {
        crate::datenschutz::verbergen(&p.display().to_string())
    }

    /// Startet den Kanal am Ort `ort` und bedient nur Verbindungen der
    /// Benutzernummer `erwartete_uid` (im Betrieb die eigene). Ist `ende`
    /// fertig (im Betrieb das Abschaltsignal), nimmt der Kanal keine neue
    /// Verbindung mehr an; der Socket verschwindet, wenn der Wert freigegeben
    /// wird. Muss in einer Tokio-Laufzeit aufgerufen werden. Jeder Fehler ist ein Satz fürs
    /// Protokoll; der Server läuft dann ohne Kanal weiter.
    ///
    /// Die Schranken sind der Ordner (0700, eigener Besitz) und `peer_cred`;
    /// die Rechte 0600 am Socket sind eine dritte. Kein `umask`: Er gälte für
    /// den ganzen Prozess, auch für die anderen Threads.
    pub fn starten(
        ort: &Path,
        erwartete_uid: u32,
        ende: impl Future<Output = ()> + Send + 'static,
        quelle: Option<Arc<dyn CodeQuelle>>,
    ) -> Result<Kanal, String> {
        let eigene_uid = crate::unix::eigene_uid();
        let ordner =
            ort.parent().filter(|p| !p.as_os_str().is_empty()).ok_or_else(|| format!("{} nennt keinen Ordner; {}", zeigen(ort), RAHMEN_KANAL.folge_im_satz))?;
        // Der Ordner, unmittelbar vor dem Binden. Angelegt wird hier nichts.
        let m = match std::fs::symlink_metadata(ordner) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(RAHMEN_KANAL.fehlt.to_string()),
            Err(e) => return Err(format!("Der Schlüsselordner {} ist nicht lesbar: {e}. {}.", zeigen(ordner), RAHMEN_KANAL.folge)),
        };
        geheim::ordner_urteil(Stand { richtige_art: m.file_type().is_dir(), uid: m.uid(), modus: m.mode(), groesse: 0 }, eigene_uid)
            .map_err(|mangel| geheim::mangel_satz_mit(mangel, &zeigen(ordner), &zeigen(ort), &RAHMEN_KANAL))?;
        // Was am Ort schon liegt.
        match std::fs::symlink_metadata(ort) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("{} ist nicht lesbar: {e}. {}.", zeigen(ort), RAHMEN_KANAL.folge)),
            Ok(m) => {
                let vorhanden = Vorhanden { ist_socket: m.file_type().is_socket(), uid: m.uid() };
                let proben = || match std::os::unix::net::UnixStream::connect(ort) {
                    Ok(_) => Probe::Antwortet,
                    Err(e) if e.raw_os_error() == Some(libc::ECONNREFUSED) => Probe::Abgewiesen,
                    Err(_) => Probe::Sonst,
                };
                match super::alt_urteil(vorhanden, eigene_uid, &proben) {
                    AltUrteil::Ersetzen => {
                        std::fs::remove_file(ort)
                            .map_err(|e| format!("Der alte Socket {} ließ sich nicht entfernen: {e}. {}.", zeigen(ort), RAHMEN_KANAL.folge))?;
                        info!("Lokaler Kanal: alter Socket ohne Lauscher entfernt.");
                    }
                    AltUrteil::Lebt => {
                        return Err(format!("Am Ort des lokalen Kanals ({}) antwortet schon ein anderer Lauf. {}.", zeigen(ort), RAHMEN_KANAL.folge));
                    }
                    AltUrteil::Bleibt => {
                        return Err(format!(
                            "Am Ort des lokalen Kanals ({}) liegt etwas, das kein eigener, verlassener Socket ist; es bleibt unverändert. {}.",
                            zeigen(ort),
                            RAHMEN_KANAL.folge
                        ));
                    }
                }
            }
        }
        // Binden. Einen zu langen Pfad weist schon die Bibliothek ab.
        let lauscher = std::os::unix::net::UnixListener::bind(ort).map_err(|e| {
            if e.kind() == std::io::ErrorKind::InvalidInput {
                format!("Der Pfad des lokalen Kanals ist zu lang für einen Socket. {}.", RAHMEN_KANAL.folge)
            } else {
                format!("Der lokale Kanal {} ließ sich nicht binden: {e}. {}.", zeigen(ort), RAHMEN_KANAL.folge)
            }
        })?;
        // Ab hier gibt es den Socket: Bei jedem Fehler wieder entfernen.
        let aufraeumen = |satz: String| {
            let _ = std::fs::remove_file(ort);
            satz
        };
        std::fs::set_permissions(ort, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| aufraeumen(format!("Die Rechte am lokalen Kanal {} ließen sich nicht setzen: {e}. {}.", zeigen(ort), RAHMEN_KANAL.folge)))?;
        // Art, Besitz und Rechte am Pfad, bevor die erste Verbindung angenommen wird.
        let m = std::fs::symlink_metadata(ort).map_err(|e| aufraeumen(format!("{} ist nicht lesbar: {e}. {}.", zeigen(ort), RAHMEN_KANAL.folge)))?;
        if !super::socket_urteil(m.file_type().is_socket(), m.uid(), m.mode(), eigene_uid) {
            return Err(aufraeumen(format!(
                "Der lokale Kanal {} ist nach dem Binden kein eigener Socket mit den Rechten 600. {}.",
                zeigen(ort),
                RAHMEN_KANAL.folge
            )));
        }
        let kennung = (m.dev(), m.ino());
        lauscher.set_nonblocking(true).map_err(|e| aufraeumen(format!("Der lokale Kanal ließ sich nicht einrichten: {e}. {}.", RAHMEN_KANAL.folge)))?;
        let lauscher =
            UnixListener::from_std(lauscher).map_err(|e| aufraeumen(format!("Der lokale Kanal ließ sich nicht einrichten: {e}. {}.", RAHMEN_KANAL.folge)))?;
        let aufgabe = tokio::spawn(annehmen(lauscher, erwartete_uid, ende, quelle));
        info!("Lokaler Kanal: {}", zeigen(ort));
        Ok(Kanal { ort: ort.to_path_buf(), kennung, aufgabe })
    }

    /// Nimmt Verbindungen an, bis `ende` fertig ist (oder die Aufgabe
    /// abgebrochen wird). Danach ist der Lauscher zu: Eine neue Verbindung
    /// wird nicht mehr angenommen, schon angenommene bekommen ihre Antwort.
    /// Je Verbindung: erst die Benutzernummer der Gegenseite, dann ein Platz,
    /// dann erst wird gelesen.
    async fn annehmen(lauscher: UnixListener, erwartete_uid: u32, ende: impl Future<Output = ()> + Send + 'static, quelle: Option<Arc<dyn CodeQuelle>>) {
        let plaetze = Arc::new(Semaphore::new(HOECHSTENS_ZUGLEICH));
        tokio::pin!(ende);
        loop {
            let angenommen = tokio::select! {
                // Erst das Ende: Liegt beides an, wird nichts mehr angenommen.
                biased;
                () = &mut ende => break,
                a = lauscher.accept() => a,
            };
            let verbindung = match angenommen {
                Ok((v, _)) => v,
                Err(e) => {
                    warn!("Lokaler Kanal: Annehmen gescheitert ({e}).");
                    tokio::time::sleep(PAUSE_NACH_FEHLER).await;
                    continue;
                }
            };
            // Die Prüfung ist nie abschaltbar: Sie bleibt die Schranke, falls
            // eine ACL am Ordner mehr erlaubt, als die Rechte zeigen.
            let bezeugt = verbindung.peer_cred().ok().map(|c| c.uid());
            let Some(konto) = super::gegenseite_urteil(bezeugt, erwartete_uid) else {
                warn!("Lokaler Kanal: Verbindung abgewiesen, die Gegenseite ist nicht das eigene Konto.");
                continue;
            };
            // Kein Platz frei: sofort schließen, ohne Lesen und ohne Warteschlange.
            let Ok(platz) = plaetze.clone().try_acquire_owned() else {
                continue;
            };
            let quelle = quelle.clone();
            tokio::spawn(async move {
                let _platz = platz;
                bedienen(verbindung, konto, quelle).await;
            });
        }
    }

    /// Eine Anfrage, eine Antwort, dann Schluss; alles innerhalb von [`FRIST`].
    async fn bedienen(mut verbindung: UnixStream, konto: KanalKonto, quelle: Option<Arc<dyn CodeQuelle>>) {
        let bis = tokio::time::Instant::now() + FRIST;
        let gelesen = tokio::time::timeout_at(bis, async {
            let mut roh = Vec::new();
            let mut leser = tokio::io::BufReader::new((&mut verbindung).take(HOECHSTENS_BYTES as u64));
            leser.read_until(b'\n', &mut roh).await.map(|_| roh)
        })
        .await;
        let antwort = match gelesen {
            Ok(Ok(roh)) => match super::anfrage_lesen(&roh) {
                Ok(Anfrage { befehl: Befehl::Wer, .. }) => super::antwort_wer(&konto),
                // Beide laufen innerhalb derselben Frist; die Ausgabe des Codes schreibt erst ins Journal.
                Ok(Anfrage { befehl, frage }) => match &quelle {
                    Some(q) => {
                        let antwort = match befehl {
                            Befehl::Beleg => q.beleg(&konto),
                            _ => q.code(&konto, frage.as_deref()),
                        };
                        match tokio::time::timeout_at(bis, antwort).await {
                            Ok(antwort) => antwort,
                            Err(_) => super::fehler_antwort(SATZ_FRIST),
                        }
                    }
                    None => super::fehler_antwort(super::SATZ_KEIN_CODE),
                },
                Err(satz) => super::fehler_antwort(satz),
            },
            // Die Verbindung ist gestört: nichts mehr zu sagen.
            Ok(Err(_)) => return,
            Err(_) => {
                // Die Frist ist um: ein Versuch ohne Warten, dann Schluss.
                let _ = verbindung.try_write(format!("{}\n", super::fehler_antwort(SATZ_FRIST)).as_bytes());
                return;
            }
        };
        let _ = tokio::time::timeout_at(bis, async {
            verbindung.write_all(format!("{antwort}\n").as_bytes()).await?;
            verbindung.shutdown().await
        })
        .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ort_liegt_neben_dem_schluessel_der_ausweise() {
        let jwt = Path::new("/a/dashboard-schluessel/jwt-schluessel");
        assert_eq!(ort_neben(Some(jwt)), Some(PathBuf::from("/a/dashboard-schluessel/kanal")));
    }

    #[test]
    fn ohne_ort_des_schluessels_oder_bei_gleichem_namen_kein_ort() {
        assert_eq!(ort_neben(None), None);
        // Der Schlüssel der Ausweise hieße selbst wie der Socket, auch in anderer Schreibung.
        assert_eq!(ort_neben(Some(Path::new("/a/geheim/kanal"))), None);
        assert_eq!(ort_neben(Some(Path::new("/a/geheim/Kanal"))), None);
        // Ein Pfad ohne Dateinamen.
        assert_eq!(ort_neben(Some(Path::new("/"))), None);
    }

    #[test]
    fn gegenseite_nur_mit_bezeugter_und_erwarteter_nummer() {
        assert_eq!(gegenseite_urteil(Some(501), 501).map(|k| k.uid()), Some(501));
        assert!(gegenseite_urteil(Some(502), 501).is_none());
        assert!(gegenseite_urteil(Some(0), 501).is_none());
        // Das Betriebssystem nennt keine Nummer.
        assert!(gegenseite_urteil(None, 501).is_none());
    }

    fn nur(befehl: Befehl) -> Result<Anfrage, &'static str> {
        Ok(Anfrage { befehl, frage: None })
    }

    #[test]
    fn anfrage_wer_wird_gelesen() {
        assert_eq!(anfrage_lesen(b"{\"befehl\":\"wer\"}\n"), nur(Befehl::Wer));
        assert_eq!(anfrage_lesen(b" { \"befehl\" : \"wer\" } \n"), nur(Befehl::Wer));
        assert_eq!(anfrage_lesen(b"{\"befehl\":\"beleg\"}\n"), nur(Befehl::Beleg));
        // `code` ohne Frage wird gelesen; einen Code gibt es dann nicht (das sagt die Quelle).
        assert_eq!(anfrage_lesen(b"{\"befehl\":\"code\"}\n"), nur(Befehl::Code));
    }

    #[test]
    fn anfrage_code_mit_frage_wird_gelesen() {
        let frage = "0123456789abcdef0123456789abcdef";
        let roh = format!("{{\"befehl\":\"code\",\"frage\":\"{frage}\"}}\n");
        assert_eq!(anfrage_lesen(roh.as_bytes()), Ok(Anfrage { befehl: Befehl::Code, frage: Some(frage.into()) }));
        // Auch eine Frage in falscher Form wird gelesen; sie passt dann nur nicht.
        assert_eq!(anfrage_lesen(b"{\"frage\":\"x\",\"befehl\":\"code\"}\n"), Ok(Anfrage { befehl: Befehl::Code, frage: Some("x".into()) }));
    }

    #[test]
    fn frage_nur_bei_code_und_nur_als_zeichenkette() {
        for roh in [
            &b"{\"befehl\":\"wer\",\"frage\":\"x\"}\n"[..],
            b"{\"befehl\":\"beleg\",\"frage\":\"x\"}\n",
            b"{\"befehl\":\"code\",\"frage\":7}\n",
            b"{\"befehl\":\"code\",\"frage\":null}\n",
        ] {
            assert_eq!(anfrage_lesen(roh), Err(SATZ_FRAGE), "{roh:?}");
        }
        // Ohne Befehl hilft auch die Frage nicht.
        assert_eq!(anfrage_lesen(b"{\"frage\":\"x\"}\n"), Err(SATZ_FELD));
        assert_eq!(anfrage_lesen(b"{\"befehl\":\"code\",\"frage\":\"x\",\"mehr\":1}\n"), Err(SATZ_FELD));
    }

    #[test]
    fn anfrage_ohne_zeilenende_vor_dem_ende() {
        assert_eq!(anfrage_lesen(b"{\"befehl\":\"wer\"}"), Err(SATZ_OHNE_ENDE));
        assert_eq!(anfrage_lesen(b""), Err(SATZ_OHNE_ENDE));
    }

    #[test]
    fn anfrage_zu_lang() {
        // Höchstens so viele Bytes werden gelesen; ist dann noch kein Zeilenende da, ist sie zu lang.
        assert_eq!(anfrage_lesen(&vec![b'a'; HOECHSTENS_BYTES]), Err(SATZ_ZU_LANG));
        // Genau an der Grenze mit Zeilenende wird sie gelesen (und ist hier kein JSON).
        let mut grenze = vec![b'a'; HOECHSTENS_BYTES - 1];
        grenze.push(b'\n');
        assert_eq!(anfrage_lesen(&grenze), Err(SATZ_KEIN_JSON));
    }

    #[test]
    fn anfrage_kein_utf8() {
        assert_eq!(anfrage_lesen(b"\xff\xfe\n"), Err(SATZ_KEIN_UTF8));
    }

    #[test]
    fn anfrage_kein_json() {
        for roh in [&b"\n"[..], b"wer\n", b"{\"befehl\":\n", b"{\"befehl\":\"wer\"\n"] {
            assert_eq!(anfrage_lesen(roh), Err(SATZ_KEIN_JSON), "{roh:?}");
        }
    }

    /// Gültiges JSON, das kein Objekt ist, auch wenn „wer“ darin steht. Ohne
    /// die Prüfung der Form läse serde die Liste als Anfrage.
    #[test]
    fn anfrage_gueltiges_json_aber_kein_objekt() {
        for roh in [&b"[\"wer\"]\n"[..], b"\"wer\"\n", b"null\n", b"7\n", b"true\n", b"[]\n"] {
            assert_eq!(anfrage_lesen(roh), Err(SATZ_KEIN_OBJEKT), "{roh:?}");
        }
    }

    #[test]
    fn anfrage_mit_unbekanntem_oder_fehlendem_feld() {
        assert_eq!(anfrage_lesen(b"{\"befehl\":\"wer\",\"mehr\":1}\n"), Err(SATZ_FELD));
        assert_eq!(anfrage_lesen(b"{\"anders\":\"wer\"}\n"), Err(SATZ_FELD));
        assert_eq!(anfrage_lesen(b"{}\n"), Err(SATZ_FELD));
    }

    #[test]
    fn anfrage_mit_unbekanntem_befehl() {
        assert_eq!(anfrage_lesen(b"{\"befehl\":\"anders\"}\n"), Err(SATZ_BEFEHL));
        assert_eq!(anfrage_lesen(b"{\"befehl\":\"CODE\"}\n"), Err(SATZ_BEFEHL));
        assert_eq!(anfrage_lesen(b"{\"befehl\":\"Wer\"}\n"), Err(SATZ_BEFEHL));
        assert_eq!(anfrage_lesen(b"{\"befehl\":7}\n"), Err(SATZ_BEFEHL));
    }

    #[test]
    fn antwort_auf_wer_nennt_fassung_und_hauptkonto() {
        let konto = gegenseite_urteil(Some(501), 501).unwrap();
        let antwort: serde_json::Value = serde_json::from_str(&antwort_wer(&konto)).unwrap();
        assert_eq!(antwort, serde_json::json!({ "fassung": env!("CARGO_PKG_VERSION"), "hauptkonto": true }));
    }

    #[test]
    fn fehler_antwort_ist_eine_zeile_mit_dem_festen_satz() {
        for satz in [SATZ_ZU_LANG, SATZ_OHNE_ENDE, SATZ_KEIN_UTF8, SATZ_KEIN_JSON, SATZ_KEIN_OBJEKT, SATZ_FELD, SATZ_BEFEHL, SATZ_FRAGE] {
            let antwort = fehler_antwort(satz);
            assert!(!antwort.contains('\n'));
            assert_eq!(serde_json::from_str::<serde_json::Value>(&antwort).unwrap(), serde_json::json!({ "fehler": satz }));
        }
    }

    #[test]
    fn alter_eigener_socket_ohne_lauscher_wird_ersetzt_ein_lebender_bleibt() {
        let eigener = Vorhanden { ist_socket: true, uid: 501 };
        assert_eq!(alt_urteil(eigener, 501, &|| Probe::Abgewiesen), AltUrteil::Ersetzen);
        assert_eq!(alt_urteil(eigener, 501, &|| Probe::Antwortet), AltUrteil::Lebt);
        // Jeder andere Fehler beim Verbinden: nichts anfassen.
        assert_eq!(alt_urteil(eigener, 501, &|| Probe::Sonst), AltUrteil::Bleibt);
    }

    #[test]
    fn zu_fremdem_oder_anderem_wird_keine_verbindung_versucht() {
        let versuche = std::cell::Cell::new(0);
        let proben = || {
            versuche.set(versuche.get() + 1);
            Probe::Abgewiesen
        };
        // Kein Socket (Datei, Verknüpfung, FIFO, Ordner) oder fremder Besitz.
        assert_eq!(alt_urteil(Vorhanden { ist_socket: false, uid: 501 }, 501, &proben), AltUrteil::Bleibt);
        assert_eq!(alt_urteil(Vorhanden { ist_socket: true, uid: 502 }, 501, &proben), AltUrteil::Bleibt);
        assert_eq!(versuche.get(), 0);
    }

    #[test]
    fn socket_nur_eigener_mit_rechten_600() {
        assert!(socket_urteil(true, 501, 0o140600, 501));
        assert!(!socket_urteil(false, 501, 0o100600, 501));
        assert!(!socket_urteil(true, 502, 0o140600, 501));
        for modus in [0o140660, 0o140604, 0o140601, 0o140777] {
            assert!(!socket_urteil(true, 501, modus, 501), "{modus:o}");
        }
    }
}

/// Der Kanal mit echten Verbindungen; nur wo es Unix-Sockets gibt.
#[cfg(all(test, unix))]
mod unix_tests {
    use std::os::unix::fs::{DirBuilderExt, FileTypeExt, MetadataExt, PermissionsExt};
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::UnixStream;

    use super::*;

    const WER: &[u8] = b"{\"befehl\":\"wer\"}\n";

    /// Ein Testordner, der am Ende des Tests wieder verschwindet.
    struct Ordner(PathBuf);

    impl std::ops::Deref for Ordner {
        type Target = Path;
        fn deref(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for Ordner {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// Ein frischer Ordner mit 0700. Kurzer Name: Der Pfad eines Sockets hat
    /// auf dem Mac höchstens 104 Zeichen.
    fn ordner(name: &str) -> Ordner {
        let o = std::env::temp_dir().join(format!("dk-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&o);
        std::fs::DirBuilder::new().mode(0o700).create(&o).unwrap();
        Ordner(o)
    }

    /// Der Kanal ohne Abschaltsignal: Er läuft, bis der Wert freigegeben wird.
    fn starte(ort: &Path, erwartete_uid: u32) -> Result<Kanal, String> {
        starten(ort, erwartete_uid, std::future::pending(), None)
    }

    fn eigene() -> u32 {
        crate::unix::eigene_uid()
    }

    /// Schickt `anfrage` und liest alles bis zum Ende der Verbindung.
    async fn fragen(ort: &Path, anfrage: &[u8]) -> String {
        let mut v = UnixStream::connect(ort).await.unwrap();
        // Die Gegenseite darf schon geschlossen haben (abgewiesen): dann scheitert das Schreiben.
        let _ = v.write_all(anfrage).await;
        alles_lesen(&mut v).await
    }

    async fn alles_lesen(v: &mut UnixStream) -> String {
        let mut antwort = Vec::new();
        let _ = tokio::time::timeout(Duration::from_secs(10), v.read_to_end(&mut antwort)).await.expect("die Verbindung endet");
        String::from_utf8(antwort).unwrap()
    }

    fn fehler_zeile(satz: &'static str) -> String {
        format!("{}\n", fehler_antwort(satz))
    }

    fn wer_zeile() -> String {
        format!("{}\n", serde_json::json!({ "fassung": env!("CARGO_PKG_VERSION"), "hauptkonto": true }))
    }

    #[test]
    fn name_des_sockets_ist_keiner_der_schluesseldateien() {
        assert_ne!(KANAL_DATEI, crate::speicher::verschluesselung::LINKS_SCHLUESSEL_DATEI.to_lowercase());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn eigene_nummer_wird_bedient() {
        let o = ordner("eigen");
        let ort = o.join(KANAL_DATEI);
        let kanal = starte(&ort, eigene()).unwrap();
        assert_eq!(kanal.ort(), ort);
        assert_eq!(fragen(&ort, WER).await, wer_zeile());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn socket_hat_art_und_rechte_600_bevor_eine_verbindung_kommt() {
        let o = ordner("rechte");
        let ort = o.join(KANAL_DATEI);
        let _kanal = starte(&ort, eigene()).unwrap();
        // Gleich nach dem Start, ohne dass je eine Verbindung angenommen wurde.
        let m = std::fs::symlink_metadata(&ort).unwrap();
        assert!(m.file_type().is_socket());
        assert_eq!(m.uid(), eigene());
        assert_eq!(m.mode() & 0o777, 0o600);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn fremde_nummer_wird_ohne_antwort_geschlossen() {
        let o = ordner("fremd");
        let ort = o.join(KANAL_DATEI);
        // Der Lauscher erwartet eine andere Nummer als die des Tests: Die echte
        // Verbindung ist für ihn die eines fremden Kontos.
        let _kanal = starte(&ort, eigene().wrapping_add(1)).unwrap();
        assert_eq!(fragen(&ort, WER).await, "");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn ordner_zu_offen_kein_kanal_und_kein_socket() {
        let o = ordner("offen");
        std::fs::set_permissions(&*o, std::fs::Permissions::from_mode(0o750)).unwrap();
        let ort = o.join(KANAL_DATEI);
        let satz = starte(&ort, eigene()).unwrap_err();
        assert!(satz.contains("für andere Konten offen") && satz.contains("Den lokalen Kanal gibt es in diesem Lauf nicht"), "{satz}");
        assert!(std::fs::symlink_metadata(&ort).is_err());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn ordner_ist_eine_verknuepfung_kein_kanal() {
        let o = ordner("verkn");
        let ziel = o.join("ziel");
        std::fs::DirBuilder::new().mode(0o700).create(&ziel).unwrap();
        let verknuepft = o.join("v");
        std::os::unix::fs::symlink(&ziel, &verknuepft).unwrap();
        let satz = starte(&verknuepft.join(KANAL_DATEI), eigene()).unwrap_err();
        assert!(satz.contains("kein Ordner oder eine Verknüpfung"), "{satz}");
        assert!(std::fs::symlink_metadata(ziel.join(KANAL_DATEI)).is_err());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn ordner_fehlt_kein_kanal_und_nichts_wird_angelegt() {
        let oben = ordner("fehlt");
        let o = oben.join("gibt-es-nicht");
        let satz = starte(&o.join(KANAL_DATEI), eigene()).unwrap_err();
        assert!(satz.contains("Der Schlüsselordner fehlt"), "{satz}");
        assert!(std::fs::symlink_metadata(&o).is_err());
    }

    /// Was am Ort liegt und kein eigener, verlassener Socket ist, bleibt, wie es ist.
    #[tokio::test(flavor = "multi_thread")]
    async fn am_ort_liegt_etwas_anderes_es_bleibt_unveraendert() {
        type Legen = fn(&Path);
        type NochDa = fn(&std::fs::Metadata) -> bool;
        let faelle: [(&str, Legen, NochDa); 5] = [
            ("datei", |p| std::fs::write(p, b"inhalt").unwrap(), |m| m.file_type().is_file() && m.len() == 6),
            ("ordner", |p| std::fs::create_dir(p).unwrap(), |m| m.file_type().is_dir()),
            ("verkn-ohne-ziel", |p| std::os::unix::fs::symlink("gibt-es-nicht", p).unwrap(), |m| m.file_type().is_symlink()),
            (
                "verkn-mit-ziel",
                |p| {
                    std::fs::write(p.with_file_name("ziel"), b"x").unwrap();
                    std::os::unix::fs::symlink("ziel", p).unwrap();
                },
                |m| m.file_type().is_symlink(),
            ),
            ("fifo", |p| assert!(crate::dienst::programme::eigener_befehl("mkfifo").arg(p).status().unwrap().success()), |m| m.file_type().is_fifo()),
        ];
        for (name, legen, noch_da) in faelle {
            let o = ordner(name);
            let ort = o.join(KANAL_DATEI);
            legen(&ort);
            let satz = starte(&ort, eigene()).unwrap_err();
            assert!(satz.contains("es bleibt unverändert") && satz.contains("Den lokalen Kanal gibt es in diesem Lauf nicht"), "{name}: {satz}");
            assert!(noch_da(&std::fs::symlink_metadata(&ort).unwrap()), "{name}: verändert");
            // Das Ziel der Verknüpfung ist auch noch da.
            if name == "verkn-mit-ziel" {
                assert_eq!(std::fs::read(o.join("ziel")).unwrap(), b"x");
            }
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn toter_socket_nach_einem_abbruch_wird_ersetzt() {
        let o = ordner("tot");
        let ort = o.join(KANAL_DATEI);
        // Gebunden und ohne Aufräumen fallen gelassen: Die Datei bleibt, niemand lauscht.
        drop(std::os::unix::net::UnixListener::bind(&ort).unwrap());
        assert!(std::fs::symlink_metadata(&ort).unwrap().file_type().is_socket());
        let _kanal = starte(&ort, eigene()).unwrap();
        assert_eq!(fragen(&ort, WER).await, wer_zeile());
        assert_eq!(std::fs::symlink_metadata(&ort).unwrap().mode() & 0o777, 0o600);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn lebender_socket_bleibt_und_bleibt_erreichbar() {
        let o = ordner("lebt");
        let ort = o.join(KANAL_DATEI);
        let erster = starte(&ort, eigene()).unwrap();
        let vorher = std::fs::symlink_metadata(&ort).unwrap().ino();
        let satz = starte(&ort, eigene()).unwrap_err();
        assert!(satz.contains("antwortet schon ein anderer Lauf"), "{satz}");
        assert_eq!(std::fs::symlink_metadata(&ort).unwrap().ino(), vorher);
        assert_eq!(fragen(&ort, WER).await, wer_zeile());
        drop(erster);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn zu_langer_pfad_fester_satz() {
        let oben = ordner("lang");
        let o = oben.join("a".repeat(120));
        std::fs::DirBuilder::new().mode(0o700).create(&o).unwrap();
        let satz = starte(&o.join(KANAL_DATEI), eigene()).unwrap_err();
        assert_eq!(satz, "Der Pfad des lokalen Kanals ist zu lang für einen Socket. Den lokalen Kanal gibt es in diesem Lauf nicht.");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn kaputte_anfragen_bekommen_je_ihren_festen_satz() {
        let o = ordner("kaputt");
        let ort = o.join(KANAL_DATEI);
        let _kanal = starte(&ort, eigene()).unwrap();
        assert_eq!(fragen(&ort, &vec![b'a'; 2000]).await, fehler_zeile(SATZ_ZU_LANG));
        assert_eq!(fragen(&ort, b"\xff\xfe\n").await, fehler_zeile(SATZ_KEIN_UTF8));
        assert_eq!(fragen(&ort, b"{\"befehl\":\n").await, fehler_zeile(SATZ_KEIN_JSON));
        assert_eq!(fragen(&ort, b"[\"wer\"]\n").await, fehler_zeile(SATZ_KEIN_OBJEKT));
        assert_eq!(fragen(&ort, b"{\"befehl\":\"wer\",\"mehr\":1}\n").await, fehler_zeile(SATZ_FELD));
        assert_eq!(fragen(&ort, b"{\"befehl\":\"anders\"}\n").await, fehler_zeile(SATZ_BEFEHL));
        assert_eq!(fragen(&ort, b"{\"befehl\":\"wer\",\"frage\":\"x\"}\n").await, fehler_zeile(SATZ_FRAGE));
        // Ohne Quelle gibt der Kanal weder Frage noch Code aus.
        assert_eq!(fragen(&ort, b"{\"befehl\":\"code\"}\n").await, fehler_zeile(SATZ_KEIN_CODE));
        assert_eq!(fragen(&ort, b"{\"befehl\":\"beleg\"}\n").await, fehler_zeile(SATZ_KEIN_CODE));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn zeile_ohne_zeilenende_vor_dem_ende_fester_satz() {
        let o = ordner("ohneende");
        let ort = o.join(KANAL_DATEI);
        let _kanal = starte(&ort, eigene()).unwrap();
        let mut v = UnixStream::connect(&ort).await.unwrap();
        v.write_all(b"{\"befehl\":\"wer\"}").await.unwrap();
        // Die Schreibseite schließen: Für den Server ist das das Ende der Anfrage.
        v.shutdown().await.unwrap();
        assert_eq!(alles_lesen(&mut v).await, fehler_zeile(SATZ_OHNE_ENDE));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn eine_zweite_zeile_wird_nicht_beantwortet() {
        let o = ordner("zwei");
        let ort = o.join(KANAL_DATEI);
        let _kanal = starte(&ort, eigene()).unwrap();
        let antwort = fragen(&ort, b"{\"befehl\":\"wer\"}\n{\"befehl\":\"wer\"}\n").await;
        assert_eq!(antwort, wer_zeile());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn stille_verbindung_endet_nach_der_frist_und_haelt_andere_nicht_auf() {
        let o = ordner("still");
        let ort = o.join(KANAL_DATEI);
        let _kanal = starte(&ort, eigene()).unwrap();
        let start = std::time::Instant::now();
        let mut still = UnixStream::connect(&ort).await.unwrap();
        // Währenddessen bekommt eine andere Verbindung sofort ihre Antwort.
        assert_eq!(fragen(&ort, WER).await, wer_zeile());
        assert!(start.elapsed() < FRIST, "die zweite Verbindung wartete auf die stille");
        assert_eq!(alles_lesen(&mut still).await, fehler_zeile(SATZ_FRIST));
        let dauer = start.elapsed();
        assert!(dauer >= FRIST - Duration::from_millis(200) && dauer < FRIST + Duration::from_secs(3), "Ende nach {dauer:?}");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn fuenfte_gleichzeitige_verbindung_wird_sofort_geschlossen() {
        let o = ordner("fuenf");
        let ort = o.join(KANAL_DATEI);
        let _kanal = starte(&ort, eigene()).unwrap();
        let mut vier = Vec::new();
        for _ in 0..HOECHSTENS_ZUGLEICH {
            vier.push(UnixStream::connect(&ort).await.unwrap());
        }
        // Die vier sind angenommen und halten ihre Plätze, sobald der Lauscher sie gesehen hat.
        tokio::time::sleep(Duration::from_millis(300)).await;
        let start = std::time::Instant::now();
        assert_eq!(fragen(&ort, WER).await, "");
        assert!(start.elapsed() < Duration::from_secs(1), "die fünfte wurde nicht sofort geschlossen");
        // Die ersten vier bekommen ihre Antwort.
        for v in &mut vier {
            v.write_all(WER).await.unwrap();
            assert_eq!(alles_lesen(v).await, wer_zeile());
        }
        // Danach ist wieder Platz.
        assert_eq!(fragen(&ort, WER).await, wer_zeile());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn nach_dem_ende_ist_der_socket_weg_und_ein_neuer_start_bekommt_ihn_wieder() {
        let o = ordner("ende");
        let ort = o.join(KANAL_DATEI);
        let kanal = starte(&ort, eigene()).unwrap();
        assert_eq!(fragen(&ort, WER).await, wer_zeile());
        drop(kanal);
        assert!(std::fs::symlink_metadata(&ort).is_err(), "der Socket liegt noch da");
        let _neu = starte(&ort, eigene()).unwrap();
        assert_eq!(fragen(&ort, WER).await, wer_zeile());
    }

    /// Nach dem Abschaltsignal nimmt der Kanal keine neue Verbindung mehr an,
    /// auch solange der Wert noch lebt; eine schon angenommene bekommt ihre Antwort.
    #[tokio::test(flavor = "multi_thread")]
    async fn nach_dem_abschaltsignal_wird_keine_neue_verbindung_bedient() {
        let o = ordner("signal");
        let ort = o.join(KANAL_DATEI);
        let (signal, ende) = tokio::sync::oneshot::channel::<()>();
        let ende = async move {
            let _ = ende.await;
        };
        let kanal = starten(&ort, eigene(), ende, None).unwrap();
        assert_eq!(fragen(&ort, WER).await, wer_zeile());
        // Eine Verbindung ist schon angenommen, hat aber noch nichts geschickt.
        let mut offen = UnixStream::connect(&ort).await.unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;
        signal.send(()).unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;
        // Der Wert lebt noch, die Datei liegt noch da, aber niemand nimmt mehr an.
        assert!(std::fs::symlink_metadata(&ort).unwrap().file_type().is_socket());
        let fehler = UnixStream::connect(&ort).await.expect_err("nach dem Signal kam noch eine Verbindung zustande");
        assert_eq!(fehler.kind(), std::io::ErrorKind::ConnectionRefused);
        // Die schon angenommene wird noch bedient.
        offen.write_all(WER).await.unwrap();
        assert_eq!(alles_lesen(&mut offen).await, wer_zeile());
        drop(kanal);
        assert!(std::fs::symlink_metadata(&ort).is_err());
    }

    /// Liegt am Ende nicht mehr der eigene Socket am Ort, wird nichts entfernt.
    #[tokio::test(flavor = "multi_thread")]
    async fn am_ende_wird_nur_der_eigene_socket_entfernt() {
        let o = ordner("nureigen");
        let ort = o.join(KANAL_DATEI);
        let kanal = starte(&ort, eigene()).unwrap();
        std::fs::remove_file(&ort).unwrap();
        std::fs::write(&ort, b"anderes").unwrap();
        drop(kanal);
        assert_eq!(std::fs::read(&ort).unwrap(), b"anderes");
    }

    /// Schreibt das Protokoll dieses Threads mit.
    #[derive(Clone, Default)]
    struct Mitschrift(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

    impl std::io::Write for Mitschrift {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// Bediente Verbindungen mit kaputter und mit gültiger Anfrage, dann
    /// eine abgewiesene Gegenseite; alle tragen die Marke.
    async fn anfragen_mit_marke(name: &str, marke: &str) {
        let o = ordner(name);
        let ort = o.join(KANAL_DATEI);
        {
            let _kanal = starte(&ort, eigene()).unwrap();
            assert_eq!(fragen(&ort, format!("{{\"befehl\":\"{marke}\"}}\n").as_bytes()).await, fehler_zeile(SATZ_BEFEHL));
            assert_eq!(fragen(&ort, format!("{marke}\n").as_bytes()).await, fehler_zeile(SATZ_KEIN_JSON));
            assert_eq!(fragen(&ort, WER).await, wer_zeile());
        }
        {
            let _kanal = starte(&ort, eigene().wrapping_add(1)).unwrap();
            assert_eq!(fragen(&ort, format!("{marke}\n").as_bytes()).await, "");
        }
    }

    /// Die Mitschrift gilt für diesen Thread; die Laufzeit des Tests hat nur
    /// ihn, so erfasst sie auch die Aufgaben des Lauschers.
    ///
    /// Läuft allein in einem eigenen Prozess (`crate::test_allein`): Neben
    /// anderen Tests verlöre die Mitschrift Zeilen und bewiese nichts.
    #[tokio::test]
    async fn nichts_aus_der_anfrage_steht_im_protokoll() {
        if !crate::test_allein::im_eigenen_prozess("kanal::unix_tests::nichts_aus_der_anfrage_steht_im_protokoll") {
            return;
        }
        let mitschrift = Mitschrift::default();
        let schreiber = mitschrift.clone();
        let abonnent = tracing_subscriber::fmt().with_ansi(false).with_writer(move || schreiber.clone()).finish();
        let _wache = tracing::subscriber::set_default(abonnent);
        let marke = "GEHEIME-MARKE-4711";
        anfragen_mit_marke("protokoll", marke).await;
        let text = String::from_utf8(mitschrift.0.lock().unwrap().clone()).unwrap();
        assert!(text.contains("Lokaler Kanal:"), "die Mitschrift erfasst den Kanal nicht: {text}");
        assert!(text.contains("Verbindung abgewiesen"), "{text}");
        assert!(!text.contains(marke), "{text}");
    }
}
