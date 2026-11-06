//! Welche Anfrage gehört zu welcher TCP-Verbindung, und wie weit ist die
//! Anmeldung auf ihr? NTLM braucht zwei Runden auf derselben Verbindung
//! (Typ 1 → 401 mit Typ 2 → Typ 3), danach gilt die ganze Verbindung als
//! angemeldet (Etappe 7b, `[anmeldung]`, bauplan).
//!
//! Die Kennung: `dienst::betreiben` startet den Router mit
//! `into_make_service_with_connect_info::<Verbindung>()`. axum ruft dafür je
//! angenommener Verbindung einmal `Connected::connect_info`, und `Verbindung`
//! holt sich dort eine fortlaufende Nummer aus einem Zähler für beide Lauscher.
//! Bewusst nicht die Socket-Adresse: Ports werden wiederverwendet, eine neue
//! Verbindung erbte sonst die Anmeldung einer alten.
//!
//! Der Vorrat (nur Windows): eine Karte Nummer → Zustand (offene Aushandlung,
//! angemeldet als Inhaber, fremdes Konto). Offene Aushandlungen verfallen nach
//! 30 s. Offene und angemeldete Einträge haben je eine eigene Obergrenze:
//! höchstens 256 offene Aushandlungen (darüber fällt die am längsten
//! unbenutzte offene heraus) und höchstens 1024 angemeldete Verbindungen (der
//! Inhaber, fremd; darüber fällt die am längsten unbenutzte angemeldete
//! heraus). So kann eine Flut angefangener Aushandlungen keine Anmeldung
//! verdrängen, und eine fertige Anmeldung keine laufende Aushandlung einer
//! anderen Verbindung. Das Zweite war früher anders (eine gemeinsame Grenze,
//! offene fielen zuerst): Bei vollem Vorrat nahm dann jede fertige Anmeldung
//! einer parallel laufenden die offene Aushandlung weg; SSPI löschte deren
//! Server-Kontext, und der Client scheiterte im zweiten Schritt mit
//! SEC_E_NO_CREDENTIALS (0x8009030E). Wer herausfällt,
//! muss sich neu anmelden (der Browser tut das ohne Rückfrage). Das Ende einer Verbindung meldet axum nicht; ihr Eintrag
//! bleibt, bis er so herausfällt. Weil jede Verbindung eine neue Nummer
//! bekommt, erbt keine andere ihn. Kerberos braucht keinen offenen Zustand
//! (ein Nachweis, sofort fertig).

use std::sync::atomic::{AtomicU64, Ordering};

use axum::extract::connect_info::Connected;
use axum::serve::IncomingStream;
use tokio::net::TcpListener;

/// Kennung einer TCP-Verbindung, für Handler und Schleusen als
/// `ConnectInfo<Verbindung>` in den Erweiterungen der Anfrage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Verbindung {
    pub nr: u64,
}

static ZAEHLER: AtomicU64 = AtomicU64::new(0);

impl Verbindung {
    /// Die nächste Nummer (ab 1).
    pub fn neu() -> Verbindung {
        Verbindung { nr: ZAEHLER.fetch_add(1, Ordering::Relaxed) + 1 }
    }
}

impl Connected<IncomingStream<'_, TcpListener>> for Verbindung {
    fn connect_info(_stream: IncomingStream<'_, TcpListener>) -> Self {
        Verbindung::neu()
    }
}

#[cfg(windows)]
pub use vorrat::*;

#[cfg(windows)]
mod vorrat {
    use std::collections::HashMap;
    use std::sync::{Mutex, MutexGuard, OnceLock};
    use std::time::{Duration, Instant};

    use crate::anmeldung::WindowsKonto;
    use crate::anmeldung::sspi::Kontext;

    /// Höchstens so viele angemeldete Verbindungen (Inhaber, fremd) im Vorrat;
    /// die am längsten unbenutzte fällt heraus. Offene Aushandlungen zählen nicht mit.
    pub const HOECHSTENS: usize = 1024;
    /// Höchstens so viele offene Aushandlungen; die am längsten unbenutzte fällt heraus.
    pub const HOECHSTENS_OFFEN: usize = 256;
    /// So lange darf eine Aushandlung zwischen zwei Runden ruhen.
    pub const ABLAUF: Duration = Duration::from_secs(30);

    enum Zustand {
        Offen {
            kontext: Kontext,
            seit: Instant,
        },
        /// Fertig angemeldet; `konto.inhaber` sagt, ob es der Inhaber war (sonst fremd).
        Fertig(WindowsKonto),
    }

    struct Eintrag {
        zustand: Zustand,
        zuletzt: Instant,
    }

    /// Wie weit die Anmeldung auf einer Verbindung ist.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Stand {
        /// Nichts bekannt (neu, verfallen oder herausgefallen).
        Neu,
        /// Eine Aushandlung wartet auf die nächste Runde.
        Offen,
        /// Angemeldet als Inhaber (SID geprüft).
        Inhaber,
        /// Angemeldet, aber mit einem anderen Konto.
        Fremd,
    }

    fn vorrat() -> MutexGuard<'static, HashMap<u64, Eintrag>> {
        static V: OnceLock<Mutex<HashMap<u64, Eintrag>>> = OnceLock::new();
        let mut v = V.get_or_init(|| Mutex::new(HashMap::new())).lock().unwrap_or_else(|e| e.into_inner());
        aufraeumen(&mut v, HOECHSTENS, HOECHSTENS_OFFEN, None);
        v
    }

    fn ist_offen(e: &Eintrag) -> bool {
        matches!(e.zustand, Zustand::Offen { .. })
    }

    /// Der am längsten unbenutzte Eintrag, auf den `passt` zutrifft.
    fn aeltester(v: &HashMap<u64, Eintrag>, passt: impl Fn(u64, &Eintrag) -> bool) -> Option<u64> {
        v.iter().filter(|(nr, e)| passt(**nr, e)).min_by_key(|(_, e)| e.zuletzt).map(|(nr, _)| *nr)
    }

    /// Verfallene offene Aushandlungen weg, dann höchstens `hoechstens_offen`
    /// offene und höchstens `hoechstens` angemeldete Einträge. Jede Art hat
    /// ihre eigene Grenze, beim Überlauf fällt nur ein Eintrag derselben Art
    /// heraus (der am längsten unbenutzte): Eine fertige Anmeldung verdrängt nie
    /// die laufende Aushandlung einer anderen Verbindung (deren Client
    /// scheiterte sonst mit SEC_E_NO_CREDENTIALS), eine Flut offener nie eine
    /// Anmeldung. `schonen` (der Eintrag, der gerade geschrieben wird) fällt
    /// nie heraus.
    fn aufraeumen(v: &mut HashMap<u64, Eintrag>, hoechstens: usize, hoechstens_offen: usize, schonen: Option<u64>) {
        v.retain(|_, e| !matches!(e.zustand, Zustand::Offen { seit, .. } if seit.elapsed() >= ABLAUF));
        let geschont = |nr: u64| Some(nr) == schonen;
        for (offen, grenze) in [(true, hoechstens_offen), (false, hoechstens)] {
            let mut zahl = v.values().filter(|e| ist_offen(e) == offen).count();
            while zahl > grenze {
                let Some(nr) = aeltester(v, |nr, e| !geschont(nr) && ist_offen(e) == offen) else { break };
                v.remove(&nr);
                zahl -= 1;
            }
        }
    }

    fn setzen(nr: u64, zustand: Zustand) {
        let mut v = vorrat();
        v.insert(nr, Eintrag { zustand, zuletzt: Instant::now() });
        aufraeumen(&mut v, HOECHSTENS, HOECHSTENS_OFFEN, Some(nr));
    }

    /// Der Stand der Verbindung `nr`; zählt als Benutzung.
    pub fn stand(nr: u64) -> Stand {
        let mut v = vorrat();
        let Some(e) = v.get_mut(&nr) else { return Stand::Neu };
        e.zuletzt = Instant::now();
        match &e.zustand {
            Zustand::Offen { .. } => Stand::Offen,
            Zustand::Fertig(k) if k.inhaber => Stand::Inhaber,
            Zustand::Fertig(_) => Stand::Fremd,
        }
    }

    /// Nimmt die offene Aushandlung der Verbindung heraus (für eine Fortsetzung).
    /// Ein Stand „Inhaber“ oder „fremd“ bleibt dabei unberührt.
    pub fn offen_nehmen(nr: u64) -> Option<Kontext> {
        let mut v = vorrat();
        if !matches!(v.get(&nr).map(|e| &e.zustand), Some(Zustand::Offen { .. })) {
            return None;
        }
        match v.remove(&nr).map(|e| e.zustand) {
            Some(Zustand::Offen { kontext, .. }) => Some(kontext),
            _ => None,
        }
    }

    /// Legt eine offene Aushandlung für die Verbindung ab (nach `Schritt::Weiter`).
    pub fn offen_ablegen(nr: u64, kontext: Kontext) {
        setzen(nr, Zustand::Offen { kontext, seit: Instant::now() });
    }

    /// Merkt sich das Ergebnis einer fertigen Aushandlung (mit Konto, Etappe 7c).
    pub fn merken(nr: u64, konto: WindowsKonto) {
        setzen(nr, Zustand::Fertig(konto));
    }

    /// Das Windows-Konto einer fertig angemeldeten Verbindung (der Inhaber oder fremd);
    /// zählt nicht als Benutzung.
    pub fn konto(nr: u64) -> Option<WindowsKonto> {
        match vorrat().get(&nr).map(|e| &e.zustand) {
            Some(Zustand::Fertig(k)) => Some(k.clone()),
            _ => None,
        }
    }

    #[cfg(test)]
    mod tests {
        use super::super::Verbindung;
        use super::*;

        #[test]
        fn stand_je_verbindung() {
            let (a, b) = (Verbindung::neu().nr, Verbindung::neu().nr);
            assert_ne!(a, b, "jede Verbindung bekommt eine eigene Nummer");
            assert_eq!(stand(a), Stand::Neu);
            offen_ablegen(a, Kontext::server().unwrap());
            assert_eq!(stand(a), Stand::Offen);
            assert_eq!(stand(b), Stand::Neu, "die offene Aushandlung gehört nur zu a");
            assert!(offen_nehmen(b).is_none());
            assert!(offen_nehmen(a).is_some());
            assert!(offen_nehmen(a).is_none(), "nur einmal");
            merken(a, WindowsKonto::test(true));
            merken(b, WindowsKonto::test(false));
            assert_eq!(konto(b).map(|k| k.name), Some(r"PC\fremd".to_string()));
            assert_eq!(stand(a), Stand::Inhaber);
            assert_eq!(stand(b), Stand::Fremd);
            assert!(offen_nehmen(a).is_none());
            assert_eq!(stand(a), Stand::Inhaber, "Fortsetzung ohne offene Aushandlung ändert den Stand nicht");
        }

        /// Auf einer eigenen Karte, damit der gemeinsame Vorrat anderer Tests unberührt bleibt.
        #[test]
        fn hoechstens_so_viele_und_der_am_laengsten_unbenutzte_faellt_heraus() {
            let mut v = HashMap::new();
            let start = Instant::now();
            for nr in 1..=5u64 {
                v.insert(nr, Eintrag { zustand: Zustand::Fertig(WindowsKonto::test(true)), zuletzt: start + Duration::from_millis(nr) });
            }
            // Nummer 1 wurde zuletzt benutzt: dann fällt 2 heraus, nicht 1.
            v.get_mut(&1).unwrap().zuletzt = start + Duration::from_secs(1);
            aufraeumen(&mut v, 4, 4, None);
            let mut uebrig: Vec<u64> = v.keys().copied().collect();
            uebrig.sort();
            assert_eq!(uebrig, [1, 3, 4, 5]);
            // Verfallene offene Aushandlungen fallen immer heraus.
            let alt = Instant::now().checked_sub(ABLAUF + Duration::from_secs(1)).unwrap();
            v.insert(9, Eintrag { zustand: Zustand::Offen { kontext: Kontext::server().unwrap(), seit: alt }, zuletzt: start });
            v.insert(10, Eintrag { zustand: Zustand::Offen { kontext: Kontext::server().unwrap(), seit: Instant::now() }, zuletzt: start });
            aufraeumen(&mut v, 100, 100, None);
            assert!(!v.contains_key(&9) && v.contains_key(&10));
        }

        fn offen(zuletzt: Instant) -> Eintrag {
            Eintrag { zustand: Zustand::Offen { kontext: Kontext::server().unwrap(), seit: Instant::now() }, zuletzt }
        }

        fn sortiert(v: &HashMap<u64, Eintrag>) -> Vec<u64> {
            let mut k: Vec<u64> = v.keys().copied().collect();
            k.sort();
            k
        }

        /// Zu viele Angemeldete: Es fällt der am längsten unbenutzte Angemeldete
        /// heraus, nie eine offene Aushandlung, auch wenn sie älter ist.
        #[test]
        fn ueberlauf_der_angemeldeten_verdraengt_keine_offene() {
            let start = Instant::now();
            let ms = |n: u64| start + Duration::from_millis(n);
            let mut v = HashMap::new();
            // Die offenen sind die am längsten unbenutzten Einträge.
            v.insert(1, offen(ms(1)));
            v.insert(2, offen(ms(2)));
            for nr in 3..=6u64 {
                v.insert(nr, Eintrag { zustand: Zustand::Fertig(WindowsKonto::test(nr % 2 == 0)), zuletzt: ms(nr) });
            }
            aufraeumen(&mut v, 3, 100, None);
            assert_eq!(sortiert(&v), [1, 2, 4, 5, 6], "nur 3, der älteste Angemeldete");
            // Eine neue Anmeldung bei vollem Vorrat (geschont): wieder fällt ein Angemeldeter.
            v.insert(7, Eintrag { zustand: Zustand::Fertig(WindowsKonto::test(true)), zuletzt: ms(0) });
            aufraeumen(&mut v, 3, 100, Some(7));
            assert_eq!(sortiert(&v), [1, 2, 5, 6, 7]);
        }

        /// Ein voller Vorrat aus lauter Angemeldeten: Die neue Aushandlung
        /// bleibt, und kein Angemeldeter muss für sie gehen.
        #[test]
        fn neue_aushandlung_bleibt_bei_vollem_vorrat() {
            let start = Instant::now();
            let ms = |n: u64| start + Duration::from_millis(n);
            for schonen in [Some(9), None] {
                let mut v = HashMap::new();
                for nr in 1..=3u64 {
                    v.insert(nr, Eintrag { zustand: Zustand::Fertig(WindowsKonto::test(true)), zuletzt: ms(nr) });
                }
                v.insert(9, offen(ms(9)));
                aufraeumen(&mut v, 3, 100, schonen);
                assert_eq!(sortiert(&v), [1, 2, 3, 9], "schonen: {schonen:?}");
            }
        }

        /// Der Wackler von `anmeldung_ueber_den_router_als_eigenes_konto` im
        /// vollen Lauf: Eine NTLM-Aushandlung ist offen (der Client hat die
        /// Challenge), da meldet sich bei vollem Vorrat eine andere Verbindung
        /// fertig an. Die offene Aushandlung muss bleiben; fiel sie heraus,
        /// löschte `Drop` den Server-Kontext, und der zweite Schritt des
        /// Clients scheiterte mit 0x8009030E (SEC_E_NO_CREDENTIALS).
        /// Auf einer eigenen Karte mit kleinen Grenzen, wie `setzen` sie aufräumt.
        #[test]
        fn fertige_anmeldung_nimmt_einer_laufenden_den_kontext_nicht() {
            use crate::anmeldung::sspi::Schritt;
            use crate::anmeldung::sspi::client::Client;

            let mut client = Client::neu("HTTP/localhost").unwrap();
            let mut server = Kontext::server().unwrap();
            let (nachweis, _) = client.schritt(None).unwrap();
            let Schritt::Weiter(challenge) = server.annehmen(&nachweis).unwrap() else {
                return; // Kerberos: eine Runde, nichts offen zu halten.
            };
            let start = Instant::now();
            let mut v = HashMap::new();
            v.insert(1, Eintrag { zustand: Zustand::Offen { kontext: server, seit: start }, zuletzt: start });
            for nr in 2..=4u64 {
                v.insert(nr, Eintrag { zustand: Zustand::Fertig(WindowsKonto::test(true)), zuletzt: Instant::now() });
                aufraeumen(&mut v, 3, 100, Some(nr));
            }
            // Vorrat voll (3 Angemeldete), jetzt die vierte fertige Anmeldung.
            v.insert(5, Eintrag { zustand: Zustand::Fertig(WindowsKonto::test(true)), zuletzt: Instant::now() });
            aufraeumen(&mut v, 3, 100, Some(5));
            let Some(Eintrag { zustand: Zustand::Offen { mut kontext, .. }, .. }) = v.remove(&1) else {
                panic!("die offene Aushandlung ist herausgefallen");
            };
            let (antwort, _) = client.schritt(Some(&challenge)).expect("zweiter Schritt des Clients");
            assert!(matches!(kontext.annehmen(&antwort).unwrap(), Schritt::Fertig { .. }));
        }

        /// Offene Aushandlungen haben eine eigene Obergrenze; Angemeldete
        /// bleiben davon unberührt.
        #[test]
        fn eigene_obergrenze_fuer_offene() {
            assert_eq!(HOECHSTENS_OFFEN, 256);
            let start = Instant::now();
            let ms = |n: u64| start + Duration::from_millis(n);
            let mut v = HashMap::new();
            v.insert(1, Eintrag { zustand: Zustand::Fertig(WindowsKonto::test(true)), zuletzt: ms(0) });
            for nr in 2..=6u64 {
                v.insert(nr, offen(ms(nr)));
            }
            aufraeumen(&mut v, 100, 2, None);
            assert_eq!(sortiert(&v), [1, 5, 6]);
        }
    }
}
