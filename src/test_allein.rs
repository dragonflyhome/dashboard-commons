//! Nur für Tests: einen Test allein in einem eigenen Prozess ausführen.
//!
//! Gebraucht von den Tests, die das Protokoll mitschreiben. `tracing` merkt
//! sich je Stelle im Code prozessweit, ob jemand zuhört, und entscheidet das,
//! wenn die Stelle zum ersten Mal erreicht wird oder sich die Zuhörer ändern.
//! Eine Mitschrift, die nur für einen Thread gilt, verliert darum Zeilen,
//! sobald andere Tests im selben Prozess dieselben Stellen erreichen: Der
//! Test bestünde dann, ohne etwas zu belegen. Allein im Prozess gibt es
//! diesen Wettlauf nicht.
//!
//! Ein solcher Test ruft zuerst [`im_eigenen_prozess`]. Im gewöhnlichen Lauf
//! startet das dasselbe Testprogramm noch einmal, nur mit diesem einen Test,
//! verlangt, dass er dort besteht, und gibt `false` zurück: Der Test ist
//! fertig. Im eigenen Prozess gibt es `true` zurück, und der Test tut seine
//! eigentliche Arbeit.

/// Gesetzt im Prozess, der nur den einen Test ausführt.
const UMGEBUNG: &str = "DASHBOARD_TEST_ALLEIN";

/// Obere Grenze für den Prozess mit dem einen Test; er braucht sonst Sekunden.
const FRIST: std::time::Duration = std::time::Duration::from_secs(300);

/// Siehe Modulkommentar. `name` ist der volle Name des Tests, wie `cargo test` ihn zeigt.
pub fn im_eigenen_prozess(name: &str) -> bool {
    if std::env::var_os(UMGEBUNG).is_some() {
        return true;
    }
    let programm = std::env::current_exe().expect("Pfad des Testprogramms");
    // Die Ausgabe geht in eine Datei: Volle Pipes hielten den Prozess an, während hier gewartet wird.
    let ausgabe = std::env::temp_dir().join(format!("dk-allein-{}-{}.txt", std::process::id(), name.rsplit("::").next().unwrap_or(name)));
    let datei = std::fs::File::create(&ausgabe).expect("Datei für die Ausgabe des Testprogramms");
    let mut kind = crate::dienst::programme::eigener_befehl(programm)
        .args([name, "--exact", "--test-threads=1", "--nocapture"])
        .env(UMGEBUNG, "1")
        .stdout(datei.try_clone().expect("Ausgabe teilen"))
        .stderr(datei)
        .spawn()
        .expect("das Testprogramm startet");
    let ende = std::time::Instant::now() + FRIST;
    let status = loop {
        if let Some(s) = kind.try_wait().expect("Warten auf das Testprogramm") {
            break Some(s);
        }
        if std::time::Instant::now() >= ende {
            let _ = kind.kill();
            let _ = kind.wait();
            break None;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    };
    let text = String::from_utf8_lossy(&std::fs::read(&ausgabe).unwrap_or_default()).into_owned();
    let _ = std::fs::remove_file(&ausgabe);
    let status = status.unwrap_or_else(|| panic!("{name} läuft allein im eigenen Prozess länger als {} s:\n{text}", FRIST.as_secs()));
    // Genau ein Test muss gelaufen sein: Mit falschem Namen bestünde der Lauf leer.
    assert!(status.success() && text.contains("test result: ok. 1 passed"), "{name} besteht allein im eigenen Prozess nicht:\n{text}");
    false
}
