//! Systemaufrufe unter macOS und Linux, je ein kleiner Block an einer Stelle.
//! Alles andere im Programm bleibt ohne `unsafe` außerhalb von Windows.

/// Die Benutzernummer dieses Prozesses.
pub fn eigene_uid() -> u32 {
    // SAFETY: `getuid` hat keine Vorbedingung, liest nur die Benutzernummer
    // des Prozesses und kann nicht scheitern.
    unsafe { libc::getuid() }
}

/// Der nice-Wert für Kindprozesse mit niedriger Priorität (Bau der
/// Oberfläche, Wächter). Entspricht `BELOW_NORMAL_PRIORITY_CLASS` unter
/// Windows; Kinder erben ihn.
pub const NIEDRIG: libc::c_int = 10;

/// Stellt das Kind von `befehl` auf niedrige Priorität. Scheitert das, startet
/// das Kind trotzdem: Ein Bau soll nie an der Priorität hängen.
pub fn niedrige_prioritaet(befehl: &mut std::process::Command) {
    use std::os::unix::process::CommandExt;
    // SAFETY: Der Abschluss läuft im Kind zwischen `fork` und `exec`. Er ruft
    // dort nur `setpriority` und gibt `Ok` zurück: kein Speicher wird
    // angelegt, keine Sperre genommen, nichts aus dem Elternprozess benutzt.
    // `setpriority` steht formal nicht in der POSIX-Liste der asynchron
    // signalsicheren Funktionen; unter macOS und Linux ist es aber nur ein
    // Systemaufruf, ohne Speicher und ohne Sperre.
    unsafe {
        befehl.pre_exec(prioritaet_senken);
    }
}

/// Wie [`niedrige_prioritaet`], für einen tokio-Befehl.
pub fn niedrige_prioritaet_async(befehl: &mut tokio::process::Command) {
    // SAFETY: wie in `niedrige_prioritaet`; der Abschluss ist derselbe.
    unsafe {
        befehl.pre_exec(prioritaet_senken);
    }
}

/// Läuft im Kind vor `exec`: senkt die eigene Priorität. Das Ergebnis zählt
/// nicht; ein Fehler bricht den Start nicht ab.
fn prioritaet_senken() -> std::io::Result<()> {
    // SAFETY: `setpriority` mit `PRIO_PROCESS` und 0 trifft nur den eigenen
    // Prozess; ein Systemaufruf ohne Zeiger.
    unsafe {
        libc::setpriority(libc::PRIO_PROCESS, 0, NIEDRIG);
    }
    Ok(())
}

/// Beendet alle Prozesse der Gruppe `gruppe` sofort (SIGKILL), wenn
/// `dienst::programme::gruppe_erlaubt` es zulässt. Für den Bau
/// nach dem Zeitlimit: Das Kind wurde mit `process_group(0)` gestartet, seine
/// Gruppennummer ist seine Prozessnummer. Aufrufen, bevor das Kind abgeholt
/// ist: Solange es nicht abgeholt ist, hält es die Gruppennummer, und sie
/// kann nicht an eine fremde Gruppe gegangen sein.
pub fn gruppe_beenden(gruppe: i32) -> Result<(), String> {
    // SAFETY: `getpgrp` hat keine Vorbedingung und kann nicht scheitern.
    let eigene = unsafe { libc::getpgrp() };
    if !crate::dienst::programme::gruppe_erlaubt(gruppe, eigene) {
        return Err(format!("Die Prozessgruppe {gruppe} wird nicht beendet (bis 1 oder die eigene)."));
    }
    // SAFETY: `killpg` ist ein Systemaufruf ohne Zeiger; die Nummer ist oben
    // geprüft (größer als 1, nicht die eigene Gruppe).
    let ergebnis = unsafe { libc::killpg(gruppe, libc::SIGKILL) };
    if ergebnis == 0 { Ok(()) } else { Err(format!("Die Prozessgruppe {gruppe} ließ sich nicht beenden: {}", std::io::Error::last_os_error().kind())) }
}

/// Nur für Tests: Gibt es den Prozess `pid` noch? Ein nicht abgeholter
/// Prozess (Zombie) zählt dabei als vorhanden.
#[cfg(test)]
pub fn prozess_da(pid: i32) -> bool {
    // SAFETY: `kill` mit Signal 0 schickt nichts, es prüft nur, ob es den Prozess gibt.
    let ergebnis = unsafe { libc::kill(pid, 0) };
    ergebnis == 0 || std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
}

/// Nur für Tests: der nice-Wert des Prozesses `pid`.
#[cfg(test)]
pub fn prioritaet_von(pid: u32) -> i32 {
    // SAFETY: `getpriority` ist ein Systemaufruf ohne Zeiger; er liest nur.
    unsafe { libc::getpriority(libc::PRIO_PROCESS, pid as libc::id_t) }
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::MetadataExt;

    /// Eine Datei, die dieser Prozess anlegt, gehört seiner Benutzernummer.
    #[test]
    fn eigene_uid_ist_die_des_prozesses() {
        let datei = std::env::temp_dir().join(format!("dashboard-uid-{}", std::process::id()));
        std::fs::write(&datei, b"x").unwrap();
        let uid = std::fs::metadata(&datei).unwrap().uid();
        let _ = std::fs::remove_file(&datei);
        assert_eq!(super::eigene_uid(), uid);
    }

    use super::{prioritaet_von, prozess_da};
    use crate::dienst::programme::{self, Werkzeug};
    use std::io::BufRead;
    use std::os::unix::process::CommandExt;
    use std::process::Stdio;
    use std::time::{Duration, Instant};

    /// Wartet mit Frist, bis `bedingung` gilt.
    fn bis(frist: Duration, bedingung: impl Fn() -> bool) -> bool {
        let ende = Instant::now() + frist;
        while Instant::now() < ende {
            if bedingung() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        bedingung()
    }

    /// Ein Kind über `befehl_niedrig` läuft mit dem nice-Wert 10; ein Kind
    /// über `befehl` behält die Priorität dieses Prozesses.
    #[test]
    fn niedrige_prioritaet_gilt_im_kind() {
        let eigene = prioritaet_von(std::process::id());
        let mut niedrig = programme::befehl_niedrig(Werkzeug::Sh).unwrap().args(["-c", "sleep 30"]).stdin(Stdio::null()).spawn().unwrap();
        let mut normal = programme::befehl(Werkzeug::Sh).unwrap().args(["-c", "sleep 30"]).stdin(Stdio::null()).spawn().unwrap();
        let (n, m) = (prioritaet_von(niedrig.id()), prioritaet_von(normal.id()));
        for kind in [&mut niedrig, &mut normal] {
            let _ = kind.kill();
            let _ = kind.wait();
        }
        assert_eq!(n, super::NIEDRIG, "befehl_niedrig");
        assert_eq!(m, eigene, "befehl ohne Senken");
        assert!(super::NIEDRIG > eigene, "sonst prüft der Test nichts (eigene Priorität {eigene})");
    }

    /// Dasselbe für die zwei tokio-Fassungen.
    #[tokio::test]
    async fn niedrige_prioritaet_gilt_im_kind_async() {
        let mut a = programme::befehl_niedrig_async(Werkzeug::Sh).unwrap();
        let mut b = programme::befehl_async(Werkzeug::Sh).unwrap();
        programme::niedrig_async(&mut b);
        for befehl in [&mut a, &mut b] {
            let mut kind = befehl.args(["-c", "sleep 30"]).stdin(Stdio::null()).kill_on_drop(true).spawn().unwrap();
            let wert = prioritaet_von(kind.id().unwrap());
            let _ = kind.kill().await;
            assert_eq!(wert, super::NIEDRIG);
        }
    }

    /// Nach dem Beenden der Gruppe lebt weder das Kind noch sein Enkel. Der
    /// Enkel wird mit Frist geprüft, nachdem das Kind abgeholt ist (ein nicht
    /// abgeholter Prozess zählte sonst als lebend).
    #[test]
    fn gruppe_beenden_trifft_kind_und_enkel() {
        let mut befehl = programme::befehl(Werkzeug::Sh).unwrap();
        befehl.args(["-c", "sleep 60 & echo $!; wait"]).stdin(Stdio::null()).stdout(Stdio::piped()).process_group(0);
        let mut kind = befehl.spawn().unwrap();
        let mut zeile = String::new();
        std::io::BufReader::new(kind.stdout.take().unwrap()).read_line(&mut zeile).unwrap();
        let enkel: i32 = zeile.trim().parse().expect("die Prozessnummer des Enkels");
        assert!(prozess_da(enkel), "der Enkel läuft");
        // Vor dem Abholen des Kindes: Bis dahin hält es die Gruppennummer.
        super::gruppe_beenden(kind.id() as i32).unwrap();
        let status = kind.wait().unwrap();
        assert!(!status.success(), "das Kind endet durch das Signal");
        assert!(bis(Duration::from_secs(5), || !prozess_da(enkel)), "der Enkel {enkel} lebt noch");
    }

    /// Ohne das Beenden der Gruppe überlebt der Enkel das Ende des Kindes:
    /// die Gegenprobe zum Test davor.
    #[test]
    fn ohne_gruppe_ueberlebt_der_enkel() {
        let mut befehl = programme::befehl(Werkzeug::Sh).unwrap();
        befehl.args(["-c", "sleep 60 & echo $!; wait"]).stdin(Stdio::null()).stdout(Stdio::piped()).process_group(0);
        let mut kind = befehl.spawn().unwrap();
        let mut zeile = String::new();
        std::io::BufReader::new(kind.stdout.take().unwrap()).read_line(&mut zeile).unwrap();
        let enkel: i32 = zeile.trim().parse().expect("die Prozessnummer des Enkels");
        kind.kill().unwrap();
        kind.wait().unwrap();
        std::thread::sleep(Duration::from_millis(200));
        let lebt = prozess_da(enkel);
        // Aufräumen: Der Enkel ist noch in der Gruppe des Kindes.
        // SAFETY: `kill` mit einer geprüften Prozessnummer größer als 1; trifft nur den Enkel dieses Tests.
        if enkel > 1 {
            unsafe {
                libc::kill(enkel, libc::SIGKILL);
            }
        }
        assert!(lebt, "nur das Kind zu beenden lässt den Enkel laufen");
    }

    /// Die eigene Gruppe und Nummern bis 1 werden abgelehnt, und nichts endet.
    #[test]
    fn gruppe_beenden_lehnt_eigene_und_kleine_nummern_ab() {
        // SAFETY: `getpgrp` hat keine Vorbedingung.
        let eigene = unsafe { libc::getpgrp() };
        for gruppe in [0, 1, -1, -eigene, eigene] {
            assert!(super::gruppe_beenden(gruppe).is_err(), "{gruppe}");
        }
        assert!(prozess_da(std::process::id() as i32), "dieser Prozess lebt");
    }
}
