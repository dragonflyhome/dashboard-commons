//! Die Schleife des Boten: Pipe anlegen (erste Instanz, DACL aus `sddl`), auf
//! Clients warten, je Verbindung ein Faden. Ein Faden liest Zeilen, führt die
//! Anfrage aus (`aufgaben`) und schreibt die Antwort, bis der Client trennt.
//!
//! Höchstens `MAX_INSTANZEN` Instanzen. Eine getrennte Instanz geht an die
//! Schleife zurück und wird wieder verwendet, statt geschlossen zu werden: so
//! gibt es den Namen ohne Lücke, bis der Bote endet, und niemand kann ihn
//! zwischendurch neu anlegen.

use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;

use tracing::{debug, error, info, warn};

use super::LEERLAUF;
use super::aufgaben::{self, Kontext};
use super::win::{self, Beschreibung, Gelesen, Rohr, Zeilen};
use crate::dienst::einstellungen::Einstellungen;
use crate::vertrag::bote::{Anfrage, Antwort, MAX_ZEILE};

/// Exit-Codes des Boten.
pub const ENDE_GUT: i32 = 0;
pub const ENDE_AUFRUF: i32 = 2;
pub const ENDE_PIPE_BELEGT: i32 = 4;
pub const ENDE_SONST: i32 = 9;

/// Warum der Bote nicht starten konnte: Exit-Code und Satz.
#[derive(Debug)]
pub struct StartFehler {
    pub code: i32,
    pub satz: String,
}

/// Die erste Instanz der Pipe samt Sicherheitsbeschreibung.
pub struct Pipe {
    pub name: String,
    pub beschreibung: Arc<Beschreibung>,
    pub erste: Rohr,
}

/// Legt die erste Instanz an. Gibt es den Namen schon (anderer Bote oder ein
/// Fremder), scheitert es mit `ENDE_PIPE_BELEGT`.
pub fn pipe_belegen(name: &str, eigene_sid: &str, dienst_sid: Option<&str>) -> Result<Pipe, StartFehler> {
    let sddl = super::sddl(eigene_sid, dienst_sid);
    let beschreibung = Beschreibung::aus_sddl(&sddl).map_err(|e| StartFehler {
        code: ENDE_SONST,
        satz: format!("Die Rechte für die Pipe lassen sich nicht bilden ({e}). Stimmen die SIDs in der Einstellungsdatei?"),
    })?;
    let erste = win::pipe_anlegen(name, &beschreibung, true).map_err(|e| {
        if e.raw_os_error() == Some(5) || win::ist_belegt(&e) {
            StartFehler { code: ENDE_PIPE_BELEGT, satz: format!("Die Pipe {} gibt es schon. Läuft bereits ein Bote?", win::pipe_pfad(name)) }
        } else {
            StartFehler { code: ENDE_SONST, satz: format!("Die Pipe {} lässt sich nicht anlegen ({e}).", win::pipe_pfad(name)) }
        }
    })?;
    Ok(Pipe { name: name.to_string(), beschreibung: Arc::new(beschreibung), erste })
}

/// Der Bote mit Einstellungen: Pipe belegen und bedienen, bis der Prozess
/// endet (Strg+C, Abmelden). Gibt den Exit-Code zurück.
pub fn laufen(e: &Einstellungen) -> i32 {
    let kontext = Kontext::jetzt().mit_einstellungen(e);
    if kontext.konto_sid.is_empty() {
        error!("Das eigene Konto lässt sich nicht lesen; der Bote startet nicht.");
        return ENDE_SONST;
    }
    let dienst_sid = Some(e.konten.dienst_sid.as_str()).filter(|s| !s.is_empty());
    match dienst_sid {
        None => warn!("In der Einstellungsdatei fehlt konten.dienst_sid: nur der Inhaber darf an die Pipe, der Dienst kann nicht verbinden."),
        // Ohne diese Rechte scheitert die Prüfung des Dienstes (OpenProcess,
        // OpenProcessToken) und er lehnt die Pipe ab; der Bote läuft trotzdem.
        Some(d) => match win::pruefen_erlauben(d) {
            Ok(()) => info!("Das Dienstkonto darf Programm und Konto des Boten abfragen."),
            Err(err) => error!("Dem Dienstkonto ließ sich das Abfragen von Prozess und Token nicht erlauben ({err}); der Dienst wird die Pipe ablehnen."),
        },
    }
    if !e.konten.inhaber_sid.is_empty() && e.konten.inhaber_sid != kontext.konto_sid {
        warn!("Der Bote läuft nicht unter der Inhaber-SID aus der Einstellungsdatei; der Dienst wird ihn ablehnen.");
    }
    let pipe = match pipe_belegen(&e.bote.pipe, &kontext.konto_sid, dienst_sid) {
        Ok(p) => p,
        Err(f) => {
            error!("{}", f.satz);
            return f.code;
        }
    };
    let rt = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
        Ok(rt) => rt,
        Err(e) => {
            error!("Die Tokio-Laufzeit lässt sich nicht starten: {e}");
            return ENDE_SONST;
        }
    };
    rt.spawn(async {
        if tokio::signal::ctrl_c().await.is_ok() {
            info!("Bote beendet (Strg+C).");
            std::process::exit(ENDE_GUT);
        }
    });
    info!("Bote bereit: {} (Fassung {})", win::pipe_pfad(&pipe.name), env!("CARGO_PKG_VERSION"));
    bedienen(pipe, Arc::new(kontext), rt.handle().clone());
    ENDE_GUT
}

/// Die Schleife: kehrt nicht zurück.
fn bedienen(pipe: Pipe, kontext: Arc<Kontext>, rt: tokio::runtime::Handle) {
    let (zurueck_tx, zurueck_rx) = mpsc::channel::<Rohr>();
    let mut lauscher = pipe.erste;
    loop {
        if let Err(e) = win::warten_auf_client(&lauscher) {
            // Etwa ein Client, der sofort wieder getrennt hat: Instanz neu nutzen.
            warn!("Verbindung nicht angenommen: {e}");
            win::trennen(&lauscher);
            continue;
        }
        let verbunden = lauscher;
        let (k, rt2, tx) = (kontext.clone(), rt.clone(), zurueck_tx.clone());
        let gestartet = std::thread::Builder::new().name("bote-verbindung".into()).spawn(move || {
            verbindung(&verbunden, &k, &rt2, LEERLAUF);
            win::trennen(&verbunden);
            let _ = tx.send(verbunden);
        });
        if let Err(e) = gestartet {
            error!("Kein Faden für die Verbindung: {e}");
        }
        lauscher = naechste_instanz(&pipe.name, &pipe.beschreibung, &zurueck_rx);
    }
}

/// Eine freie Instanz: eine zurückgegebene oder eine neue; sind alle belegt,
/// warten, bis eine Verbindung endet.
fn naechste_instanz(name: &str, sd: &Beschreibung, zurueck: &mpsc::Receiver<Rohr>) -> Rohr {
    loop {
        if let Ok(r) = zurueck.try_recv() {
            return r;
        }
        match win::pipe_anlegen(name, sd, false) {
            Ok(r) => return r,
            Err(e) if win::ist_belegt(&e) => {
                if let Ok(r) = zurueck.recv_timeout(Duration::from_secs(1)) {
                    return r;
                }
            }
            Err(e) => {
                warn!("Weitere Pipe-Instanz nicht angelegt: {e}");
                if let Ok(r) = zurueck.recv_timeout(Duration::from_secs(1)) {
                    return r;
                }
            }
        }
    }
}

/// Bedient einen Client, bis er trennt oder `leerlauf` lang keine Anfrage
/// schickt (dann trennt der Bote; der Dienst verbindet bei Bedarf neu).
fn verbindung(rohr: &Rohr, k: &Kontext, rt: &tokio::runtime::Handle, leerlauf: Duration) {
    let mut zeilen = Zeilen::default();
    loop {
        let antwort = match zeilen.naechste_mit_leerlauf(rohr, Some(leerlauf)) {
            Ok(Gelesen::Zeile(z)) => match serde_json::from_slice::<Anfrage>(&z) {
                Ok(a) => {
                    drop(z);
                    // Fortschritt kommt während eines Baus alle 1,5 s: nur im Debug-Protokoll.
                    if matches!(a, Anfrage::Fortschritt) {
                        debug!("Anfrage {}", a.art())
                    } else {
                        info!("Anfrage {}", a.art())
                    }
                    // „Öffnen“ nur vom Dienstkonto (Abnahme): wer am anderen Ende sitzt,
                    // liest der Bote per Impersonation; geht das nicht, gilt der Client als unbekannt.
                    let client = matches!(a, Anfrage::Oeffnen { .. })
                        .then(|| win::client_sid(rohr).map_err(|e| warn!("Konto des Clients nicht lesbar: {e}")).ok())
                        .flatten();
                    rt.block_on(aufgaben::ausfuehren_von(a, k, client))
                }
                Err(_) => Antwort::fehler("Die Anfrage ist unlesbar."),
            },
            Ok(Gelesen::Ende) => return,
            Ok(Gelesen::Leerlauf) => {
                info!("Verbindung ruhte länger als {} s; getrennt.", leerlauf.as_secs());
                return;
            }
            Ok(Gelesen::ZuLang) => {
                let _ = senden(rohr, &Antwort::fehler("Die Anfrage ist länger als 1 MB."));
                return;
            }
            Err(e) => {
                warn!("Lesen von der Pipe: {e}");
                return;
            }
        };
        if let Err(e) = senden(rohr, &antwort) {
            warn!("Antwort nicht gesendet: {e}");
            return;
        }
    }
}

/// Eine Antwort als Zeile; ist sie zu lang, geht ein Satz statt ihrer.
fn senden(rohr: &Rohr, antwort: &Antwort) -> std::io::Result<()> {
    let mut zeile =
        serde_json::to_string(antwort).unwrap_or_else(|_| r#"{"art":"fehler","daten":{"satz":"Die Antwort ließ sich nicht schreiben."}}"#.to_string());
    if zeile.len() > MAX_ZEILE {
        warn!("Antwort {} zu lang ({} Bytes)", antwort.art(), zeile.len());
        zeile = serde_json::to_string(&Antwort::fehler("Die Antwort wäre länger als 1 MB.")).unwrap_or_default();
    }
    zeile.push('\n');
    rohr.schreiben(zeile.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// „Öffnen“ nur vom Dienst: Der Bote liest das Konto des Clients per
    /// Impersonation (hier das eigene, ein Client wie der Dienst mit
    /// SECURITY_IDENTIFICATION) und ist danach wieder er selbst.
    #[test]
    fn konto_des_clients_ueber_die_pipe() {
        let name = format!("dashboard-test-client-{}", std::process::id());
        let sid = win::eigene_sid().unwrap();
        let p = pipe_belegen(&name, &sid, None).unwrap();
        let n2 = name.clone();
        let client = std::thread::spawn(move || {
            let r = win::pipe_oeffnen(&n2, 5000).unwrap();
            r.schreiben(b"x\n").unwrap();
            std::thread::sleep(Duration::from_millis(500));
        });
        win::warten_auf_client(&p.erste).unwrap();
        let mut puffer = [0u8; 4];
        assert!(p.erste.lesen(&mut puffer).unwrap() > 0);
        assert_eq!(win::client_sid(&p.erste).unwrap(), sid);
        assert_eq!(win::eigene_sid().unwrap(), sid, "nach der Impersonation wieder das eigene Konto");
        client.join().unwrap();
        // Ein fremdes Konto (nicht dienst_sid) darf nicht öffnen.
        assert!(!crate::bote::oeffnen::vom_dienst(Some(&sid), Some("S-1-5-80-1-2-3-4-5")));
        assert!(crate::bote::oeffnen::vom_dienst(Some("S-1-5-80-1-2-3-4-5"), Some("S-1-5-80-1-2-3-4-5")));
        assert!(!crate::bote::oeffnen::vom_dienst(None, Some("S-1-5-80-1-2-3-4-5")));
        assert!(!crate::bote::oeffnen::vom_dienst(Some(&sid), None));
    }

    /// Eine zweite erste Instanz mit demselben Namen scheitert.
    #[test]
    fn zweite_erste_instanz_scheitert() {
        let name = format!("dashboard-test-einheit-{}", std::process::id());
        let sid = win::eigene_sid().unwrap();
        let p = pipe_belegen(&name, &sid, None).unwrap();
        let f = pipe_belegen(&name, &sid, None).err().expect("zweite muss scheitern");
        assert_eq!(f.code, ENDE_PIPE_BELEGT, "{}", f.satz);
        // Weitere Instanzen ohne FIRST_PIPE_INSTANCE gehen (der Inhaber hat GA).
        win::pipe_anlegen(&name, &p.beschreibung, false).unwrap();
    }

    /// Wer nur `DIENST_MASKE` hat, kann keine weitere Instanz anlegen; mit der
    /// früheren Maske 0x12019F (enthält FILE_CREATE_PIPE_INSTANCE) ging es.
    /// Geprüft wird mit dem eigenen Konto, das in der DACL nur diese Rechte
    /// bekommt (der Besitzer erhält von selbst nur READ_CONTROL und WRITE_DAC).
    #[test]
    fn dienst_rechte_legen_keine_instanz_an() {
        let sid = win::eigene_sid().unwrap();
        for (maske, geht) in [(crate::bote::DIENST_RECHTE, false), ("0x12019F", true)] {
            let name = format!("dashboard-test-instanz-{}-{maske}", std::process::id());
            let sd = Beschreibung::aus_sddl(&format!("O:{sid}D:(A;;{maske};;;{sid})")).unwrap();
            let _erste = win::pipe_anlegen(&name, &sd, true).unwrap();
            match win::pipe_anlegen(&name, &sd, false) {
                Ok(_) => assert!(geht, "mit {maske} darf keine zweite Instanz entstehen"),
                Err(e) => {
                    assert!(!geht, "mit {maske} sollte es gehen: {e}");
                    assert!(win::zugriff_verweigert(&e), "{e}");
                }
            }
        }
    }

    /// Der Client öffnet mit genau `DIENST_MASKE` und liest den Besitzer der Pipe.
    #[test]
    fn besitzer_der_pipe() {
        let name = format!("dashboard-test-besitzer-{}", std::process::id());
        let sid = win::eigene_sid().unwrap();
        let _p = pipe_belegen(&name, &sid, Some(&sid)).unwrap();
        let rohr = win::pipe_oeffnen(&name, 1000).unwrap();
        assert_eq!(win::pipe_besitzer(&rohr).unwrap(), sid);
        assert_eq!(win::server_pid(&rohr).unwrap(), std::process::id());
    }

    /// Wartezeiten beim Blick in die Pipe: kurz nach Verkehr kurz, nie über 50 ms.
    #[test]
    fn warte_schritte() {
        assert_eq!(win::warte_schritt(Duration::ZERO), Duration::from_millis(1));
        assert_eq!(win::warte_schritt(Duration::from_secs(5)), Duration::from_millis(10));
        assert_eq!(win::warte_schritt(LEERLAUF), Duration::from_millis(50));
        let mut vorher = Duration::ZERO;
        for s in 0..700 {
            let w = win::warte_schritt(Duration::from_secs(s));
            assert!(w >= vorher && w <= Duration::from_millis(50), "{s} s: {w:?}");
            vorher = w;
        }
        assert_eq!(LEERLAUF, Duration::from_secs(600));
    }

    /// Eine ruhende Verbindung trennt der Bote nach der Leerlaufgrenze (hier
    /// 300 ms statt 10 min); eine Anfrage davor wird beantwortet.
    #[test]
    fn leerlauf_trennt_die_verbindung() {
        let name = format!("dashboard-test-leerlauf-{}", std::process::id());
        let sid = win::eigene_sid().unwrap();
        let p = pipe_belegen(&name, &sid, Some(&sid)).unwrap();
        let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(1).enable_all().build().unwrap();
        let handle = rt.handle().clone();
        let kontext = Kontext::jetzt();
        let server = std::thread::spawn(move || {
            win::warten_auf_client(&p.erste).unwrap();
            let start = std::time::Instant::now();
            verbindung(&p.erste, &kontext, &handle, Duration::from_millis(300));
            start.elapsed()
        });
        let client = win::pipe_oeffnen(&name, 2000).unwrap();
        // Erst eine Anfrage nach 100 ms: wird beantwortet.
        std::thread::sleep(Duration::from_millis(100));
        client.schreiben(b"{\"art\":\"hallo\"}\n").unwrap();
        let mut zeilen = Zeilen::default();
        assert!(matches!(zeilen.naechste(&client).unwrap(), Gelesen::Zeile(_)));
        // Dann Ruhe: der Server trennt nach 300 ms. Untere Grenze nur die
        // Leerlaufgrenze selbst: Die Zeit misst der Server erst, wenn er den
        // Client sieht; unter Last ist dann ein Teil der 100 ms schon vorbei.
        let dauer = server.join().unwrap();
        assert!(dauer >= Duration::from_millis(300) && dauer < Duration::from_secs(3), "{dauer:?}");
        assert!(matches!(zeilen.naechste(&client).unwrap(), Gelesen::Ende));
        drop(rt);
    }

    /// Bau-Tempo, Zweig C: Während eine Verbindung belegt ist (der Bau),
    /// beantwortet eine zweite `Fortschritt` nur mit Zahlen. Eine Anfrage, die
    /// der Bote nicht kennt (so sieht ein älterer Bote `fortschritt`), gibt
    /// `Fehler`; für den Dienst heißt das „kein Fortschritt“.
    #[test]
    fn fortschritt_ueber_eine_zweite_verbindung() {
        use crate::bote::client::fortschritt_aus;
        let name = format!("dashboard-test-fortschritt-{}", std::process::id());
        let sid = win::eigene_sid().unwrap();
        let p = pipe_belegen(&name, &sid, Some(&sid)).unwrap();
        let zweite = win::pipe_anlegen(&name, &p.beschreibung, false).unwrap();
        let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(1).enable_all().build().unwrap();
        let server = [p.erste, zweite].map(|rohr| {
            let handle = rt.handle().clone();
            std::thread::spawn(move || {
                // Unter Last kann der Test beide Clients schon geschlossen haben, bevor
                // dieser Faden ConnectNamedPipe ruft (ERROR_NO_DATA); dann gibt es nichts zu bedienen.
                if win::warten_auf_client(&rohr).is_ok() {
                    verbindung(&rohr, &Kontext::jetzt(), &handle, Duration::from_millis(1500));
                }
            })
        });
        // Die erste Verbindung bleibt offen (ohne laufenden Bau; nur: eine zweite Verbindung antwortet) ...
        let belegt = win::pipe_oeffnen(&name, 2000).unwrap();
        // ... die zweite fragt.
        let client = win::pipe_oeffnen(&name, 2000).unwrap();
        let mut zeilen = Zeilen::default();
        let mut fragen = |zeile: &[u8]| {
            client.schreiben(zeile).unwrap();
            match zeilen.naechste(&client).unwrap() {
                Gelesen::Zeile(z) => serde_json::from_slice::<Antwort>(&z).unwrap(),
                g => panic!("keine Zeile: {:?}", std::mem::discriminant(&g)),
            }
        };
        let a = fragen(b"{\"art\":\"fortschritt\"}\n");
        assert!(matches!(a, Antwort::Fortschritt(_)), "{a:?}");
        let text = serde_json::to_string(&a).unwrap();
        assert!(!text.contains("projects") && !text.contains("satz"), "nur Zahlen: {text}");
        let alt = fragen(b"{\"art\":\"gibt_es_noch_nicht\"}\n");
        assert!(matches!(&alt, Antwort::Fehler { satz } if satz.contains("unlesbar")), "{alt:?}");
        assert_eq!(fortschritt_aus(Ok(alt)), None, "unbekannte Anfrage: kein Fortschritt");
        drop((client, belegt));
        for s in server {
            s.join().unwrap();
        }
        drop(rt);
    }

    /// Die Rechte für das Dienstkonto landen als eigener Eintrag in den DACLs
    /// von Prozess und Token (SID erfunden; der Testprozess endet ohnehin).
    #[test]
    fn pruefen_erlauben_erweitert_die_dacls() {
        let fremd = "S-1-5-80-1111111111-2222222222-3333333333-444444444-555555555";
        let (vorher_p, vorher_t) = win::eigene_dacls().unwrap();
        assert!(!vorher_p.contains(fremd) && !vorher_t.contains(fremd));
        win::pruefen_erlauben(fremd).unwrap();
        let (p, t) = win::eigene_dacls().unwrap();
        assert!(p.contains(&format!(";;;{fremd})")), "{p}");
        assert!(t.contains(&format!(";;;{fremd})")), "{t}");
        // Die übrigen Einträge bleiben.
        let eintraege = |s: &str| s.matches("(A;").count();
        assert_eq!(eintraege(&p), eintraege(&vorher_p) + 1, "{vorher_p} -> {p}");
        assert_eq!(eintraege(&t), eintraege(&vorher_t) + 1, "{vorher_t} -> {t}");
        // Zweimal schadet nicht (GRANT_ACCESS führt zusammen).
        win::pruefen_erlauben(fremd).unwrap();
        assert_eq!(eintraege(&win::eigene_dacls().unwrap().0), eintraege(&p));
    }
}
