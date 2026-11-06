//! Die Seite des Dienstes: verbindet sich mit der Pipe des Boten, prüft vor
//! dem ersten Senden, wem sie gehört, und stellt Anfragen mit Zeitlimit.
//!
//! Prüfung (Auflage der Entscheidung), auf zwei unabhängigen Wegen, die
//! beide passen müssen:
//!
//! 1. GetNamedPipeServerProcessId, dann muss das Programm des Servers
//!    `bote.programm` sein (kanonisch, Groß/Klein egal) und das Konto seines
//!    Tokens `konten.inhaber_sid`. Dafür muss der Bote dem Dienstkonto das
//!    Abfragen erlaubt haben (`win::pruefen_erlauben`); sonst verweigert
//!    Windows den Zugriff, und die Verbindung wird abgelehnt (`SATZ_PROZESS_ZUGRIFF`).
//! 2. Der Besitzer der Pipe (GetSecurityInfo auf dem Pipe-Handle) muss
//!    `konten.inhaber_sid` sein. Er wird zuerst geprüft, weil er keine Rechte
//!    am Server-Prozess braucht.
//!
//! Sonst wird nichts gesendet. Tests setzen `programm` auf den gebauten
//! Boten und `inhaber_sid` auf die eigene SID; die Prüfung bleibt dabei scharf.
//!
//! Gleichzeitigkeit: höchstens `PLAETZE` Verbindungen sind offen, ruhende
//! mitgezählt (jede Verbindung hält einen Platz der Semaphore, auch wenn sie
//! ruht). Der Bote hat mehr Instanzen (`win::MAX_INSTANZEN`), also ist er nie
//! „beschäftigt“, solange nur der Dienst fragt. Weitere Anfragen warten in der
//! Schlange, bis eine Verbindung frei wird oder ein Platz; das Zeitlimit der
//! Anfrage (`frist`) schließt das Warten ein. Ruhende Verbindungen werden
//! wiederverwendet, aber nach `RUHEND_HOECHSTENS` verworfen (der Bote trennt
//! nach `LEERLAUF`); nach einem Fehler wird neu verbunden.
//!
//! Der Stand (`BoteStand`) steht für /api/betrieb bereit und unterscheidet
//! verbunden, getrennt und überlastet (alle Plätze belegt oder Zeitlimit):
//! überlastet heißt, der Bote ist da, nur gerade beschäftigt.

use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use tokio::sync::{Notify, OwnedSemaphorePermit, Semaphore};
use tokio::time::Instant;
use tracing::{info, warn};
use zeroize::Zeroizing;

use crate::dienst::einstellungen::{Einstellungen, Satz};
use crate::vertrag::bote::{Anfrage, Antwort, BauFortschritt, BoteSitzung, MAX_ZEILE};

/// Zeitlimit für kurze Anfragen.
pub const FRIST_KURZ: Duration = Duration::from_secs(10);
/// Zeitlimit für winget, Ordnergrößen, Werkzeuge und Git.
pub const FRIST_LANG: Duration = Duration::from_secs(300);
/// Zeitlimit für den Bau der Oberfläche: so lange, wie der Bote höchstens
/// braucht (`aufgaben::BAU_HOECHSTENS`), und eine Minute mehr. Der Bau hält
/// solange eine der `PLAETZE`; die anderen Anfragen laufen daneben weiter
/// (der Bote bedient jede Verbindung in einem eigenen Faden).
pub const FRIST_BAU: Duration = Duration::from_secs(super::aufgaben::BAU_HOECHSTENS.as_secs() + 60);
/// Zeitlimit für `Fortschritt` (Zweig C): Der Dienst fragt alle 1,5 s, eine
/// spätere Antwort braucht niemand.
pub const FRIST_FORTSCHRITT: Duration = Duration::from_secs(2);
/// So viele Verbindungen sind höchstens offen (aktive und ruhende zusammen).
pub const PLAETZE: usize = 8;
/// Ruhende Verbindungen, die länger nicht benutzt wurden, werden verworfen,
/// bevor der Bote sie trennt (eine Minute Abstand zu `LEERLAUF`).
pub const RUHEND_HOECHSTENS: Duration = Duration::from_secs(super::LEERLAUF.as_secs() - 60);

/// Der Satz, wenn die Pipe nicht dem Boten gehört.
pub const SATZ_FREMD: &str = "Die Pipe gehört nicht dem Boten; nichts gesendet.";

/// Der Grund, wenn der Server-Prozess sich nicht öffnen lässt.
pub const SATZ_PROZESS_ZUGRIFF: &str = "Prozessprüfung nicht möglich (Zugriff verweigert); der Bote muss die Rechte erteilen";

/// Der Satz, wenn alle Plätze bis zum Zeitlimit belegt blieben.
pub const SATZ_AUSGELASTET: &str = "Der Bote ist gerade ausgelastet (alle Verbindungen belegt).";

/// Wie es um die Verbindung zum Boten steht.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Lage {
    /// Die letzte Anfrage wurde beantwortet.
    Verbunden,
    /// Keine Pipe, fremde Pipe, Verbindung abgebrochen: kein Bote da.
    #[default]
    Getrennt,
    /// Der Bote ist da, aber alle Verbindungen waren belegt oder er hat nicht
    /// rechtzeitig geantwortet. Kein Grund für „Bote läuft nicht“.
    Ueberlastet,
}

/// Zustand der Verbindung zum Boten, für /api/betrieb.
#[derive(Debug, Clone, Default)]
pub struct BoteStand {
    pub lage: Lage,
    /// Erreichbar: verbunden oder überlastet (`lage != Getrennt`).
    pub verbunden: bool,
    /// Seit wann erreichbar (RFC 3339).
    pub seit: Option<String>,
    pub letzter_fehler: Option<String>,
    /// Letzte gute Antwort (RFC 3339).
    pub zuletzt_gesehen: Option<String>,
    /// Die Sitzung aus dem letzten Hallo (Hotfix Sitzungsabfrage); bleibt
    /// stehen, wenn der Bote fehlt. `None`: noch kein Hallo, oder ein älterer
    /// Bote ohne dieses Feld (unbekannt).
    pub sitzung: Option<BoteSitzung>,
    /// Wann dieses Hallo kam (RFC 3339); `None` ohne Hallo.
    pub sitzung_gemeldet: Option<String>,
    /// Kleinkram 2: Die Fassung des Prüfers aus dem letzten Hallo; `None`
    /// ohne Hallo oder bei einem älteren Boten ohne das Feld (siehe `pruefer_hallo`).
    pub pruefer: Option<String>,
    /// Ob schon ein Hallo kam (auch eines ohne `pruefer`).
    pub pruefer_hallo: bool,
}

/// Trägt die Sitzung aus einem Hallo ein; ein Wechsel kommt einmal ins
/// Protokoll (nur Nummer und Zustand).
fn sitzung_vermerken(s: &mut BoteStand, sitzung: Option<BoteSitzung>, jetzt: &str) {
    if s.sitzung != sitzung {
        match &sitzung {
            Some(b) => info!("Der Bote meldet Sitzung {} (Zustand {:?}, Konsole {}).", b.id, b.zustand, b.konsole),
            None => info!("Der Bote meldet keine Sitzung (ältere Fassung?)."),
        }
    }
    s.sitzung = sitzung;
    s.sitzung_gemeldet = Some(jetzt.to_string());
}

/// Kleinkram 2: Trägt die Fassung des Prüfers aus einem Hallo ein; ein
/// Wechsel und eine Abweichung von der eigenen kommen einmal ins Protokoll.
fn pruefer_vermerken(s: &mut BoteStand, pruefer: Option<String>) {
    if (!s.pruefer_hallo || s.pruefer != pruefer)
        && let Some(satz) = pruefer_abweichung(pruefer.as_deref(), &crate::werkzeug::pruefer_fassung())
    {
        warn!("{satz}");
    }
    s.pruefer = pruefer;
    s.pruefer_hallo = true;
}

/// Kleinkram 2: Der Satz, wenn der Bau-Prüfer (der Bote) mit anderen Regeln
/// prüft als der Dienst (`eigene`); `None`, wenn beide gleich sind.
/// `bote`: die Fassung aus dem Hallo, `None` bei einem älteren Boten.
pub fn pruefer_abweichung(bote: Option<&str>, eigene: &str) -> Option<String> {
    match bote {
        Some(b) if b == eigene => None,
        Some(b) => Some(format!(
            "Der Bote prüft und baut mit einer anderen Fassung des Prüfers ({b}) als der Dienst ({eigene}). Urteile im Bau können veraltet sein; Dienst.ps1 -Neustart erneuert Dienst und Boten zusammen."
        )),
        None => Some(format!(
            "Der Bote nennt keine Fassung des Prüfers (älter als der Dienst, {eigene}). Urteile im Bau können veraltet sein; Dienst.ps1 -Neustart erneuert Dienst und Boten zusammen."
        )),
    }
}

/// Satz für die Oberfläche, wenn der Bote überlastet ist: mit der Zeit der
/// letzten guten Antwort (`zuletzt_gesehen`, RFC 3339).
pub fn satz_ausgelastet(zuletzt_gesehen: Option<&str>) -> String {
    let zeit = zuletzt_gesehen
        .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
        .map(|t| t.with_timezone(&chrono::Local).format("%d.%m.%Y %H:%M").to_string());
    match zeit {
        Some(z) => format!("Der Bote ist gerade ausgelastet; letzter Stand von {z}."),
        None => "Der Bote ist gerade ausgelastet; einen früheren Stand gibt es noch nicht.".to_string(),
    }
}

/// Das Zeitlimit für eine Anfrage.
pub fn frist(a: &Anfrage) -> Duration {
    match a {
        Anfrage::Winget | Anfrage::Inventar { .. } | Anfrage::Werkzeuge | Anfrage::GitStatus { .. } => FRIST_LANG,
        Anfrage::Bau => FRIST_BAU,
        Anfrage::Fortschritt => FRIST_FORTSCHRITT,
        _ => FRIST_KURZ,
    }
}

/// Zweig C: Die Antwort auf `Fortschritt` als Stand. Alles andere heißt „kein
/// Fortschritt“, nie ein Fehler: ein älterer Bote kennt die Anfrage nicht
/// (`Fehler` „unlesbar“), kein Bote, keine Antwort in der Frist.
pub fn fortschritt_aus(r: Result<Antwort, Satz>) -> Option<BauFortschritt> {
    match r {
        Ok(Antwort::Fortschritt(f)) => f,
        _ => None,
    }
}

/// Die Anfrage als Zeile mit `\n`, in einem Puffer, der von Anfang an groß
/// genug ist und nie wächst (sonst blieben beim Umkopieren Reste eines
/// Tresor-Werts im freigegebenen Speicher). Zu lang: Satz.
pub fn zeile_bauen(a: &Anfrage) -> Result<Zeroizing<Vec<u8>>, Satz> {
    let zu_lang = || Satz("Die Anfrage ist länger als 1 MB.".to_string());
    let mut zeile = Zeroizing::new(Vec::with_capacity(MAX_ZEILE + 16));
    serde_json::to_writer(Begrenzt(&mut zeile), a).map_err(|e| if e.is_io() { zu_lang() } else { Satz(format!("Anfrage nicht schreibbar: {e}")) })?;
    if zeile.len() + 1 > MAX_ZEILE {
        return Err(zu_lang());
    }
    zeile.push(b'\n');
    Ok(zeile)
}

/// Schreibt in einen Vec, aber nie über dessen Kapazität hinaus.
struct Begrenzt<'a>(&'a mut Vec<u8>);

impl Write for Begrenzt<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if self.0.len() + buf.len() > self.0.capacity() {
            return Err(std::io::Error::other("zu lang"));
        }
        self.0.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Ein Fehler beim Fragen, mit Lage (getrennt oder überlastet).
struct Fehler {
    satz: Satz,
    /// Die Verbindung ging nicht (eine ruhende): mit einer neuen nochmal.
    nochmal: bool,
    lage: Lage,
}

impl Fehler {
    fn getrennt(satz: impl Into<String>, nochmal: bool) -> Fehler {
        Fehler { satz: Satz(satz.into()), nochmal, lage: Lage::Getrennt }
    }

    fn ueberlastet(satz: impl Into<String>) -> Fehler {
        Fehler { satz: Satz(satz.into()), nochmal: false, lage: Lage::Ueberlastet }
    }
}

/// Der Bote aus Sicht des Dienstes.
pub struct Bote {
    ziel: Arc<Ziel>,
    /// `PLAETZE` Plätze; jede offene Verbindung hält einen.
    plaetze: Arc<Semaphore>,
    /// Ruhende Verbindungen mit dem Zeitpunkt, seit dem sie ruhen.
    frei: Mutex<Vec<(Verbindung, Instant)>>,
    /// Weckt Wartende, wenn eine Verbindung zurückkommt.
    zurueck: Notify,
    stand: Arc<RwLock<BoteStand>>,
}

/// Wohin, und wem die Pipe gehören muss.
struct Ziel {
    pipe: String,
    programm: PathBuf,
    inhaber_sid: String,
}

/// Eine geprüfte Verbindung, mit ihrem Platz (wenn sie über `anfragen` kam).
pub struct Verbindung {
    #[cfg(windows)]
    rohr: super::win::Rohr,
    #[cfg(windows)]
    zeilen: super::win::Zeilen,
    platz: Option<OwnedSemaphorePermit>,
}

/// Was `platz_holen` bringt: eine ruhende Verbindung oder einen Platz für eine neue.
enum Holung {
    Ruhend(Verbindung),
    Neu(OwnedSemaphorePermit),
}

/// Merkt sich das Handle, solange darauf gelesen oder geschrieben wird, damit
/// das Zeitlimit die Ein-/Ausgabe abbrechen kann.
#[derive(Default)]
struct Wache(Mutex<Option<usize>>);

impl Wache {
    fn setzen(&self, h: Option<usize>) {
        *self.0.lock().unwrap_or_else(|e| e.into_inner()) = h;
    }

    fn abbrechen(&self) {
        let g = self.0.lock().unwrap_or_else(|e| e.into_inner());
        // Das Handle ist offen, solange es hier steht (wird vor dem Schließen gelöscht).
        #[cfg(windows)]
        if let Some(h) = *g {
            super::win::abbrechen(h);
        }
        drop(g);
    }
}

impl Bote {
    pub fn neu(e: &Einstellungen) -> Bote {
        Bote {
            ziel: Arc::new(Ziel { pipe: e.bote.pipe.clone(), programm: e.bote.programm.clone(), inhaber_sid: e.konten.inhaber_sid.clone() }),
            plaetze: Arc::new(Semaphore::new(PLAETZE)),
            frei: Mutex::new(Vec::new()),
            zurueck: Notify::new(),
            stand: Arc::new(RwLock::new(BoteStand::default())),
        }
    }

    /// Für Tests: ein Bote ohne Pipe (jede Anfrage scheitert mit „läuft
    /// nicht“); den Stand setzt der Test mit [`Bote::lage_setzen`].
    #[cfg(test)]
    pub fn fuer_tests() -> Bote {
        Bote {
            ziel: Arc::new(Ziel {
                pipe: format!("dashboard-bote-test-gibt-es-nicht-{}", std::process::id()),
                programm: PathBuf::from("C:/gibt-es-nicht/dashboard-bote.exe"),
                inhaber_sid: String::new(),
            }),
            plaetze: Arc::new(Semaphore::new(PLAETZE)),
            frei: Mutex::new(Vec::new()),
            zurueck: Notify::new(),
            stand: Arc::new(RwLock::new(BoteStand::default())),
        }
    }

    /// Für Tests: ein Hallo mit dieser Fassung des Prüfers, ohne Pipe.
    #[cfg(test)]
    pub fn pruefer_setzen(&self, pruefer: Option<String>) {
        let mut s = self.stand.write().unwrap_or_else(|e| e.into_inner());
        pruefer_vermerken(&mut s, pruefer);
    }

    /// Für Tests: ein falscher Stand (da, weg, überlastet), ohne Pipe.
    #[cfg(test)]
    pub fn lage_setzen(&self, lage: Lage) {
        let mut s = self.stand.write().unwrap_or_else(|e| e.into_inner());
        s.lage = lage;
        s.verbunden = lage != Lage::Getrennt;
    }

    /// Der aktuelle Stand (Kopie).
    pub fn stand(&self) -> BoteStand {
        self.stand.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Wie viele Verbindungen gerade ruhen (für Tests).
    pub fn ruhende(&self) -> usize {
        self.frei.lock().unwrap_or_else(|e| e.into_inner()).len()
    }

    /// Wie viele Plätze frei sind, ohne neue Verbindung (für Tests).
    pub fn freie_plaetze(&self) -> usize {
        self.plaetze.available_permits()
    }

    /// Verbindet und prüft den Server (blockierend, wartet höchstens 2 s auf
    /// eine freie Instanz). Gibt die Verbindung nur zurück, wenn Programm und
    /// Konto stimmen; sonst ist sie schon geschlossen und nichts gesendet.
    /// Zum Prüfen und für Tests; sie zählt nicht zu den `PLAETZE`.
    pub fn verbinden(&self) -> Result<Verbindung, Satz> {
        self.ziel.verbinden(2000).map_err(|f| f.satz)
    }

    /// Anfrage mit dem Zeitlimit nach Art (`frist`).
    pub async fn anfragen(&self, a: &Anfrage) -> Result<Antwort, Satz> {
        self.anfragen_mit_frist(a, frist(a)).await
    }

    /// Zweig C: Stand des laufenden Baus (eigene Verbindung neben `Bau`);
    /// `None` ohne Bau, ohne Boten oder bei einem älteren Boten.
    pub async fn fortschritt(&self) -> Option<BauFortschritt> {
        fortschritt_aus(self.anfragen(&Anfrage::Fortschritt).await)
    }

    /// Anfrage mit eigenem Zeitlimit (Warten auf einen Platz und Verbinden
    /// eingeschlossen). Eine ruhende Verbindung, die nicht mehr geht, wird
    /// durch eine andere oder eine neue ersetzt.
    pub async fn anfragen_mit_frist(&self, a: &Anfrage, frist: Duration) -> Result<Antwort, Satz> {
        self.anfragen_mit_lage(a, frist).await.map_err(|(satz, _)| satz)
    }

    /// Wie `anfragen_mit_frist`, im Fehlerfall mit der Lage (getrennt oder
    /// überlastet), damit der Aufrufer „ausgelastet“ von „weg“ trennen kann.
    pub async fn anfragen_mit_lage(&self, a: &Anfrage, frist: Duration) -> Result<Antwort, (Satz, Lage)> {
        let zeile = Arc::new(zeile_bauen(a).map_err(|s| (s, Lage::Getrennt))?);
        let ende = Instant::now() + frist;
        let mut ergebnis = Err(Fehler::getrennt("Der Bote hat nicht geantwortet.", false));
        // Jede gescheiterte ruhende Verbindung ist danach zu; nach höchstens
        // PLAETZE Runden kommt eine neue.
        for _ in 0..=PLAETZE {
            let holung = match self.platz_holen(ende).await {
                Ok(h) => h,
                Err(f) => {
                    ergebnis = Err(f);
                    break;
                }
            };
            let war_ruhend = matches!(holung, Holung::Ruhend(_));
            ergebnis = self.einmal(holung, zeile.clone(), ende).await;
            match &ergebnis {
                Err(f) if f.nochmal && war_ruhend => continue,
                _ => break,
            }
        }
        self.vermerken(&ergebnis);
        ergebnis.map_err(|f| (f.satz, f.lage))
    }

    /// Eine ruhende Verbindung, die nicht zu alt ist; zu alte werden verworfen
    /// (und geben ihren Platz frei).
    fn ruhende_nehmen(&self) -> Option<Verbindung> {
        let mut frei = self.frei.lock().unwrap_or_else(|e| e.into_inner());
        frei.retain(|(_, seit)| seit.elapsed() < RUHEND_HOECHSTENS);
        frei.pop().map(|(v, _)| v)
    }

    /// Wartet, bis eine ruhende Verbindung zurückkommt oder ein Platz frei
    /// wird, höchstens bis `ende`.
    async fn platz_holen(&self, ende: Instant) -> Result<Holung, Fehler> {
        loop {
            // Erst anmelden, dann nachsehen: so geht kein Zurückgeben verloren.
            let zurueck = self.zurueck.notified();
            tokio::pin!(zurueck);
            zurueck.as_mut().enable();
            if let Some(v) = self.ruhende_nehmen() {
                return Ok(Holung::Ruhend(v));
            }
            tokio::select! {
                p = self.plaetze.clone().acquire_owned() => {
                    return p.map(Holung::Neu).map_err(|_| Fehler::getrennt("Der Bote ist geschlossen.", false));
                }
                _ = &mut zurueck => continue,
                _ = tokio::time::sleep_until(ende) => return Err(Fehler::ueberlastet(SATZ_AUSGELASTET)),
            }
        }
    }

    /// Eine Verbindung ruht wieder; Wartende werden geweckt.
    fn zuruecklegen(&self, v: Verbindung) {
        self.frei.lock().unwrap_or_else(|e| e.into_inner()).push((v, Instant::now()));
        self.zurueck.notify_one();
    }

    /// Ein Versuch.
    async fn einmal(&self, holung: Holung, zeile: Arc<Zeroizing<Vec<u8>>>, ende: Instant) -> Result<Antwort, Fehler> {
        let wache = Arc::new(Wache::default());
        let w = wache.clone();
        let ziel = self.ziel.clone();
        let warten_ms = ende.saturating_duration_since(Instant::now()).as_millis().min(u32::MAX as u128) as u32;
        let arbeit = tokio::task::spawn_blocking(move || -> Result<(Antwort, Verbindung), Fehler> {
            let mut v = match holung {
                Holung::Ruhend(v) => v,
                Holung::Neu(platz) => {
                    let mut v = ziel.verbinden(warten_ms)?;
                    v.platz = Some(platz);
                    v
                }
            };
            let r = v.austauschen(&zeile, &w);
            // Erst die Wache leeren, dann darf die Verbindung (bei Fehler) zugehen.
            w.setzen(None);
            r.map(|a| (a, v))
        });
        let rest = ende.saturating_duration_since(Instant::now());
        match tokio::time::timeout(rest, arbeit).await {
            Ok(Ok(Ok((antwort, v)))) => {
                self.zuruecklegen(v);
                Ok(antwort)
            }
            Ok(Ok(Err(f))) => Err(f),
            Ok(Err(e)) => Err(Fehler::getrennt(format!("Die Anfrage an den Boten ist abgestürzt ({e})."), false)),
            Err(_) => {
                wache.abbrechen();
                Err(Fehler::ueberlastet("Der Bote hat nicht rechtzeitig geantwortet."))
            }
        }
    }

    /// Stand nachführen; Wechsel ins Protokoll.
    fn vermerken(&self, ergebnis: &Result<Antwort, Fehler>) {
        let jetzt = chrono::Local::now().to_rfc3339();
        let mut s = self.stand.write().unwrap_or_else(|e| e.into_inner());
        match ergebnis {
            // Auch eine Fehler-Antwort heißt: der Bote ist da.
            Ok(a) => {
                if s.lage != Lage::Verbunden {
                    info!("Bote verbunden.");
                }
                if let Antwort::Hallo { sitzung, pruefer, .. } = a {
                    sitzung_vermerken(&mut s, *sitzung, &jetzt);
                    pruefer_vermerken(&mut s, pruefer.clone());
                }
                if s.seit.is_none() {
                    s.seit = Some(jetzt.clone());
                }
                s.lage = Lage::Verbunden;
                s.zuletzt_gesehen = Some(jetzt);
                s.letzter_fehler = None;
            }
            Err(f) if f.lage == Lage::Ueberlastet => {
                if s.lage != Lage::Ueberlastet {
                    warn!("Bote ausgelastet: {}", f.satz);
                }
                if s.seit.is_none() {
                    s.seit = Some(jetzt);
                }
                s.lage = Lage::Ueberlastet;
                s.letzter_fehler = Some(f.satz.0.clone());
            }
            Err(f) => {
                if s.lage != Lage::Getrennt || s.letzter_fehler.as_deref() != Some(f.satz.0.as_str()) {
                    warn!("Bote nicht erreichbar: {}", f.satz);
                }
                s.lage = Lage::Getrennt;
                s.seit = None;
                s.letzter_fehler = Some(f.satz.0.clone());
            }
        }
        s.verbunden = s.lage != Lage::Getrennt;
    }
}

impl Ziel {
    /// Öffnet und prüft; wartet höchstens `warten_ms` auf eine freie Instanz.
    #[cfg(windows)]
    fn verbinden(&self, warten_ms: u32) -> Result<Verbindung, Fehler> {
        use super::win;
        let rohr = win::pipe_oeffnen(&self.pipe, warten_ms).map_err(|e| {
            if win::fehlt(&e) {
                Fehler::getrennt("Der Bote läuft nicht (keine Pipe).", false)
            } else if win::ist_belegt(&e) {
                Fehler::ueberlastet("Der Bote ist beschäftigt (alle Verbindungen belegt).")
            } else {
                Fehler::getrennt(format!("Die Pipe des Boten lässt sich nicht öffnen ({e})."), false)
            }
        })?;
        // Ab hier: bei jedem Fehler geht `rohr` zu, ohne dass etwas gesendet wurde.
        let fremd = |grund: String| {
            warn!("{SATZ_FREMD} ({grund})");
            Fehler::getrennt(format!("{SATZ_FREMD} ({grund})"), false)
        };
        // Scheitert das Öffnen des Server-Prozesses an den Rechten, hat der
        // Bote dem Dienstkonto die Abfrage nicht erlaubt (oder es ist kein Bote).
        let prozess_fehler = |was: &str, e: std::io::Error| {
            if win::zugriff_verweigert(&e) { fremd(SATZ_PROZESS_ZUGRIFF.to_string()) } else { fremd(format!("{was} nicht lesbar: {e}")) }
        };
        if self.inhaber_sid.is_empty() {
            return Err(fremd("in der Einstellungsdatei fehlt konten.inhaber_sid".to_string()));
        }
        // Weg 2: Besitzer der Pipe selbst.
        let besitzer = win::pipe_besitzer(&rohr).map_err(|e| fremd(format!("Besitzer der Pipe nicht lesbar: {e}")))?;
        if besitzer != self.inhaber_sid {
            // Ohne die SID des Besitzers: Sie ginge in Protokoll und Fehlersatz.
            return Err(fremd("Besitzer der Pipe ist nicht der Inhaber".to_string()));
        }
        // Weg 1: Programm und Konto des Server-Prozesses.
        let pid = win::server_pid(&rohr).map_err(|e| fremd(format!("Server nicht feststellbar: {e}")))?;
        let programm = win::prozess_programm(pid).map_err(|e| prozess_fehler("Programm des Servers", e))?;
        if !super::gleicher_pfad(&programm, &self.programm) {
            return Err(fremd(format!("Programm {} statt {}", programm.display(), self.programm.display())));
        }
        let sid = win::prozess_sid(pid).map_err(|e| prozess_fehler("Konto des Servers", e))?;
        if sid != self.inhaber_sid {
            return Err(fremd("Server-Prozess läuft unter einem anderen Konto als dem des Inhabers".to_string()));
        }
        Ok(Verbindung { rohr, zeilen: Default::default(), platz: None })
    }

    #[cfg(not(windows))]
    fn verbinden(&self, _warten_ms: u32) -> Result<Verbindung, Fehler> {
        Err(Fehler::getrennt("Den Boten gibt es nur unter Windows.", false))
    }
}

impl Verbindung {
    /// Zeile senden, Antwortzeile lesen.
    #[cfg(windows)]
    fn austauschen(&mut self, zeile: &[u8], wache: &Wache) -> Result<Antwort, Fehler> {
        use super::win::Gelesen;
        wache.setzen(Some(self.rohr.roh()));
        self.rohr.schreiben(zeile).map_err(|e| Fehler::getrennt(format!("Senden an den Boten ging nicht ({e})."), true))?;
        match self.zeilen.naechste(&self.rohr) {
            Ok(Gelesen::Zeile(z)) => {
                serde_json::from_slice::<Antwort>(&z).map_err(|e| Fehler::getrennt(format!("Die Antwort des Boten ist unlesbar ({e})."), false))
            }
            Ok(Gelesen::Ende) | Ok(Gelesen::Leerlauf) => Err(Fehler::getrennt("Der Bote hat die Verbindung getrennt.", true)),
            Ok(Gelesen::ZuLang) => Err(Fehler::getrennt("Die Antwort des Boten ist länger als 1 MB.", false)),
            Err(e) => Err(Fehler::getrennt(format!("Lesen vom Boten ging nicht ({e})."), true)),
        }
    }

    #[cfg(not(windows))]
    fn austauschen(&mut self, _zeile: &[u8], _wache: &Wache) -> Result<Antwort, Fehler> {
        let _ = &self.platz;
        Err(Fehler::getrennt("Den Boten gibt es nur unter Windows.", false))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vertrag::bote::Wert;

    #[test]
    fn fristen() {
        assert_eq!(frist(&Anfrage::Hallo), FRIST_KURZ);
        assert_eq!(frist(&Anfrage::TresorInfo { ziel: "ranke/x".into() }), FRIST_KURZ);
        assert_eq!(frist(&Anfrage::Winget), FRIST_LANG);
        assert_eq!(frist(&Anfrage::GitStatus { pfad: "x".into(), privat: false }), FRIST_LANG);
        assert_eq!(frist(&Anfrage::Bau), FRIST_BAU);
        assert_eq!(FRIST_BAU, Duration::from_secs(7 * 60));
        assert_eq!(frist(&Anfrage::Fortschritt), FRIST_FORTSCHRITT);
        assert!(RUHEND_HOECHSTENS < super::super::LEERLAUF);
    }

    /// Zweig C: Ohne Boten, bei einem älteren Boten (der die Anfrage nicht
    /// kennt und mit `Fehler` antwortet) und bei jeder anderen Antwort gibt es
    /// keinen Fortschritt, nie einen Fehler; nur `Fortschritt` trägt Zahlen.
    #[tokio::test]
    async fn fortschritt_ohne_oder_mit_altem_boten_ist_keiner() {
        use crate::vertrag::bote::FortschrittSchritt;
        assert_eq!(Bote::fuer_tests().fortschritt().await, None, "ohne Pipe");
        assert_eq!(fortschritt_aus(Ok(Antwort::fehler("Die Anfrage ist unlesbar."))), None, "alter Bote");
        assert_eq!(fortschritt_aus(Ok(Antwort::Ok)), None);
        assert_eq!(fortschritt_aus(Err(Satz("Der Bote hat nicht rechtzeitig geantwortet.".into()))), None);
        let f = BauFortschritt { schritt: FortschrittSchritt::Pruefung, fertig: 2, gesamt: 5 };
        assert_eq!(fortschritt_aus(Ok(Antwort::Fortschritt(Some(f)))), Some(f));
        assert_eq!(fortschritt_aus(Ok(Antwort::Fortschritt(None))), None);
    }

    #[test]
    fn zeile_waechst_nicht_und_endet_mit_zeilenumbruch() {
        let z = zeile_bauen(&Anfrage::Hallo).unwrap();
        assert_eq!(&z[..], b"{\"art\":\"hallo\"}\n");
        assert_eq!(z.capacity(), MAX_ZEILE + 16, "von Anfang an voll angelegt, nie gewachsen");
        let gross = Anfrage::TresorSchreiben { ziel: "ranke/test-x".into(), benutzer: "x".repeat(MAX_ZEILE), wert: Wert::neu("geheim") };
        assert_eq!(zeile_bauen(&gross).unwrap_err().0, "Die Anfrage ist länger als 1 MB.");
    }

    /// Die Sitzung aus dem Hallo steht im Stand; ein älterer Bote ohne sie
    /// macht sie unbekannt (`None`), mit der Zeit des Hallo.
    #[test]
    fn sitzung_aus_dem_hallo() {
        use crate::vertrag::bote::SitzungsZustand;
        let mut s = BoteStand::default();
        let b = BoteSitzung { id: 2, konsole: true, zustand: Some(SitzungsZustand::Aktiv) };
        sitzung_vermerken(&mut s, Some(b), "2026-10-30T08:00:00+01:00");
        assert_eq!((s.sitzung, s.sitzung_gemeldet.as_deref()), (Some(b), Some("2026-10-30T08:00:00+01:00")));
        sitzung_vermerken(&mut s, None, "2026-10-30T08:01:00+01:00");
        assert_eq!((s.sitzung, s.sitzung_gemeldet.as_deref()), (None, Some("2026-10-30T08:01:00+01:00")));
    }

    /// Kleinkram 2: Die Fassung des Prüfers aus dem Hallo steht im Stand;
    /// gleich wie die eigene ist keine Abweichung, eine andere oder keine schon.
    #[test]
    fn pruefer_aus_dem_hallo() {
        let eigene = crate::werkzeug::pruefer_fassung();
        let mut s = BoteStand::default();
        assert!(!s.pruefer_hallo);
        pruefer_vermerken(&mut s, Some(eigene.clone()));
        assert_eq!((s.pruefer.as_deref(), s.pruefer_hallo), (Some(eigene.as_str()), true));
        assert_eq!(pruefer_abweichung(Some(&eigene), &eigene), None);
        let anders = pruefer_abweichung(Some("0000000000000001"), &eigene).unwrap();
        assert!(anders.contains("0000000000000001") && anders.contains(&eigene) && anders.contains("Dienst.ps1 -Neustart"), "{anders}");
        assert!(pruefer_abweichung(None, &eigene).unwrap().starts_with("Der Bote nennt keine Fassung des Prüfers"));
        pruefer_vermerken(&mut s, None);
        assert_eq!((s.pruefer, s.pruefer_hallo), (None, true));
    }

    #[test]
    fn satz_bei_ueberlastung_nennt_den_letzten_stand() {
        let t = chrono::Local::now();
        let s = satz_ausgelastet(Some(&t.to_rfc3339()));
        assert_eq!(s, format!("Der Bote ist gerade ausgelastet; letzter Stand von {}.", t.format("%d.%m.%Y %H:%M")));
        assert!(satz_ausgelastet(None).starts_with("Der Bote ist gerade ausgelastet;"));
    }
}
