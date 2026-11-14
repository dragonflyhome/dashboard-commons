//! Einheitstests für [`super::verbergen_mit`]: alle Formen aus dem Kopf des
//! Moduls, mit erfundenen Konten (`beispiel`, `beispiel konto`, `prüfkonto`).

use std::path::Path;

use serde_json::json;

use super::*;

/// Ohne bekannten Inhaber: nur die allgemeinen Formen.
fn leer() -> Bekannt {
    Bekannt::default()
}

/// Ein Inhaber mit Profil außerhalb von `\Users`.
fn aussen() -> Bekannt {
    Bekannt::aus(Some(Path::new(r"D:\Profile\beispiel")), Some(r"DOMAENE\beispiel"))
}

fn v(text: &str) -> String {
    verbergen_mit(text, &leer())
}

#[test]
fn laufwerk_mit_users_und_benutzer() {
    assert_eq!(v(r"C:\Users\beispiel\Documents\x.pdf"), r"%USERPROFILE%\Documents\x.pdf");
    assert_eq!(v("C:/Users/beispiel/Documents/x.pdf"), "%USERPROFILE%/Documents/x.pdf");
    assert_eq!(v(r"c:\users\beispiel"), "%USERPROFILE%");
    assert_eq!(v(r"C:\Benutzer\beispiel\x"), r"%USERPROFILE%\x");
    assert_eq!(v(r"D:\USERS\beispiel\x"), r"%USERPROFILE%\x");
    // Doppelte Rückstriche (Debug-Ausgabe, JSON im Text) und gemischte Trenner.
    assert_eq!(v(r"C:\\Users\\beispiel\\x"), r"%USERPROFILE%\\x");
    assert_eq!(v(r"C:/Users\beispiel/x"), "%USERPROFILE%/x");
}

#[test]
fn shell_formen() {
    assert_eq!(v("/c/Users/beispiel/dev"), "%USERPROFILE%/dev");
    assert_eq!(v("/mnt/c/Users/beispiel/dev"), "%USERPROFILE%/dev");
    assert_eq!(v("/cygdrive/c/Users/beispiel/dev"), "%USERPROFILE%/dev");
    assert_eq!(v("cd /c/users/beispiel && ls"), "cd %USERPROFILE% && ls");
}

#[test]
fn netzwerkpfade() {
    assert_eq!(v(r"\\rechner\c$\Users\beispiel\x.txt"), r"%USERPROFILE%\x.txt");
    assert_eq!(v("//rechner/c$/Users/beispiel/x.txt"), "%USERPROFILE%/x.txt");
    assert_eq!(v(r"Freigabe \\rechner-2\D$\Benutzer\beispiel"), "Freigabe %USERPROFILE%");
}

#[test]
fn url_kodiert_und_file() {
    assert_eq!(v("C%3A%5CUsers%5Cbeispiel%5Cx.pdf"), "%USERPROFILE%%5Cx.pdf");
    assert_eq!(v("c%3a%2fUsers%2fbeispiel%2fx"), "%USERPROFILE%%2fx");
    assert_eq!(v("file:///C:/Users/beispiel/x.pdf"), "%USERPROFILE%/x.pdf");
    assert_eq!(v("<file:///C:%5CUsers%5Cbeispiel%5Cx.pdf>"), "<%USERPROFILE%%5Cx.pdf>");
    assert_eq!(v("file://rechner/c$/Users/beispiel/x"), "%USERPROFILE%/x");
    assert_eq!(v("%5C%5Crechner%5Cc%24%5CUsers%5Cbeispiel%5Cx"), "%USERPROFILE%%5Cx");
    assert_eq!(v("http://127.0.0.1/api?pfad=C%3A%5CUsers%5Cbeispiel&x=1"), "http://127.0.0.1/api?pfad=%USERPROFILE%&x=1");
}

#[test]
fn namen_mit_leerzeichen_und_satzende() {
    // Mit Leerzeichen, wenn danach ein Trenner kommt.
    assert_eq!(v(r"C:\Users\beispiel konto\x"), r"%USERPROFILE%\x");
    // Sonst endet der Name am Leerzeichen, der Satz bleibt.
    assert_eq!(v(r"C:\Users\beispiel ist nicht lesbar"), "%USERPROFILE% ist nicht lesbar");
    assert_eq!(v(r"Ordner C:\Users\beispiel."), "Ordner %USERPROFILE%.");
    assert_eq!(v(r"„C:\Users\beispiel“"), "„%USERPROFILE%“");
    assert_eq!(v(r#""C:\Users\beispiel": (os error 5)"#), r#""%USERPROFILE%": (os error 5)"#);
    assert_eq!(v(r"C:\Users\beispiel-prüf\x"), r"%USERPROFILE%\x");
}

#[test]
fn mehrere_stellen_in_einem_text() {
    assert_eq!(
        v(r"von C:\Users\beispiel\a nach /c/Users/beispiel2/b und \\pc\c$\Users\beispiel3"),
        "von %USERPROFILE%\\a nach %USERPROFILE%/b und %USERPROFILE%"
    );
}

#[test]
fn was_bleibt() {
    for text in [
        r"C:\Windows\System32\cmd.exe",
        r"C:\Users\",
        r"C:\Users",
        "https://example.org/users/beispiel",
        "Das sind 3 Users",
        r"C:\ProgramData\Dashboard\daten",
        "50% erledigt, 5C kalt",
        "",
    ] {
        assert_eq!(v(text), text, "{text}");
    }
}

#[test]
fn bekanntes_profil_ausserhalb_von_users() {
    let b = aussen();
    let w = |t: &str| verbergen_mit(t, &b);
    assert_eq!(w(r"D:\Profile\beispiel\Documents\x.pdf"), r"%USERPROFILE%\Documents\x.pdf");
    assert_eq!(w("d:/profile/BEISPIEL/x"), "%USERPROFILE%/x");
    assert_eq!(w("/d/Profile/beispiel/x"), "%USERPROFILE%/x");
    assert_eq!(w("/mnt/d/Profile/beispiel/x"), "%USERPROFILE%/x");
    assert_eq!(w("/cygdrive/d/Profile/beispiel"), "%USERPROFILE%");
    assert_eq!(w(r"\\rechner\d$\Profile\beispiel\x"), r"%USERPROFILE%\x");
    assert_eq!(w("file:///D:/Profile/beispiel/x"), "%USERPROFILE%/x");
    assert_eq!(w("D%3A%5CProfile%5Cbeispiel%5Cx"), "%USERPROFILE%%5Cx");
    // Nur der ganze Ordner: „beispiel2“ und „beispiel-alt“ sind andere Ordner.
    assert_eq!(w(r"D:\Profile\beispiel2\x"), r"D:\Profile\beispiel2\x");
    assert_eq!(w(r"D:\Profile\beispiel-alt\x"), r"D:\Profile\%USERNAME%-alt\x");
    assert_eq!(w(r"„D:\Profile\beispiel“"), "„%USERPROFILE%“");
    // Anderes Laufwerk: kein Profil, aber der Name allein.
    assert_eq!(w(r"E:\Profile\beispiel\x"), r"E:\Profile\%USERNAME%\x");
}

#[test]
fn name_allein() {
    let b = aussen();
    let w = |t: &str| verbergen_mit(t, &b);
    assert_eq!(w(r"Konto DOMAENE\beispiel angemeldet"), r"Konto DOMAENE\%USERNAME% angemeldet");
    assert_eq!(w("Beispiel, BEISPIEL; beispiel."), "%USERNAME%, %USERNAME%; %USERNAME%.");
    assert_eq!(w("sicherung_beispiel_alt"), "sicherung_%USERNAME%_alt");
    // Teil eines Wortes bleibt.
    assert_eq!(w("beispielhaft, Gegenbeispiel"), "beispielhaft, Gegenbeispiel");
    // Profil und Name zusammen.
    assert_eq!(w(r"D:\Profile\beispiel von beispiel"), "%USERPROFILE% von %USERNAME%");
    // Nicht-ASCII ohne Groß/Klein.
    let u = Bekannt::aus(None, Some("Prüfkonto"));
    assert_eq!(verbergen_mit("PRÜFKONTO und prüfkonto", &u), "%USERNAME% und %USERNAME%");
}

#[test]
fn bekannt_aus_profil_und_name() {
    let b = Bekannt::aus(Some(Path::new(r"C:\Users\beispiel\")), Some(r"DOMÄNE\konto-x"));
    assert_eq!(b.profile, vec![r"C:\Users\beispiel".to_string()]);
    assert_eq!(b.namen, vec!["beispiel".to_string(), "konto-x".to_string()]);
    // Kurze Namen und relative Profile zählen nicht; doppelte Namen nur einmal.
    let b = Bekannt::aus(Some(Path::new("beispiel")), Some("ab"));
    assert_eq!(b, Bekannt::default());
    let b = Bekannt::aus(Some(Path::new(r"C:\Users\Beispiel")), Some("beispiel"));
    assert_eq!(b.namen.len(), 1);
}

#[test]
fn json_mit_schluesseln_und_listen() {
    let mut j = json!({
        "root": r"C:\Users\beispiel\dev",
        "liste": ["/c/Users/beispiel/a", 3, null, {"tief": "file:///C:/Users/beispiel/x"}],
        r"C:\Users\beispiel\schluessel": true,
        "zahl": 1
    });
    json_verbergen_mit(&mut j, &leer());
    assert_eq!(
        j,
        json!({
            "root": r"%USERPROFILE%\dev",
            "liste": ["%USERPROFILE%/a", 3, null, {"tief": "%USERPROFILE%/x"}],
            r"%USERPROFILE%\schluessel": true,
            "zahl": 1
        })
    );
}

/// Ohne gesetzten Inhaber gilt das eigene Profil: Es verschwindet, auch wenn
/// es nicht unter `\Users` liegt.
#[test]
fn eigenes_profil_ohne_setzen() {
    if let Some(p) = std::env::var_os("USERPROFILE") {
        let text = format!(r"{}\x.txt", p.to_string_lossy());
        assert_eq!(verbergen(&text), r"%USERPROFILE%\x.txt");
    }
}

/// Auflage der Abnahme: doppelt kodiert (`%255C`, `%253A`, `%2524`)
/// wie im Vor-Push-Prüfer, auch hinter einem kodierten Trenner.
#[test]
fn doppelt_kodiert() {
    assert_eq!(v("C%253A%255CUsers%255Cbeispiel%255Cx"), "%USERPROFILE%%255Cx");
    assert_eq!(v("c%253a%252fusers%252fbeispiel"), "%USERPROFILE%");
    assert_eq!(v("%255C%255Crechner%255Cc%2524%255CUsers%255Cbeispiel"), "%USERPROFILE%");
    assert_eq!(v("file%3A%2F%2F%2FC%3A%5CUsers%5Cbeispiel%5Cx"), "file%3A%2F%2F%2F%USERPROFILE%%5Cx");
    // Ein einfaches %25 ohne Hex danach bleibt.
    assert_eq!(v("100%25 fertig"), "100%25 fertig");
}

/// Auflage: Ein Name nach „%5C“ (oder `%2F`, `%3A`) ist ein eigenes Wort.
#[test]
fn name_nach_kodiertem_trenner() {
    let b = Bekannt::aus(None, Some("beispiel"));
    let w = |t: &str| verbergen_mit(t, &b);
    assert_eq!(w("DOMAENE%5Cbeispiel"), "DOMAENE%5C%USERNAME%");
    assert_eq!(w("konto%2Fbeispiel%2Fx"), "konto%2F%USERNAME%%2Fx");
    assert_eq!(w("DOMAENE%255Cbeispiel"), "DOMAENE%255C%USERNAME%");
    // Ohne Trenner davor bleibt es Teil eines Wortes.
    assert_eq!(w("x5Cbeispiel"), "x5Cbeispiel");
}

/// Auflage: Profile als Netzwerkpfad und mit `\\?\` vorn.
#[test]
fn unc_und_geraete_profile() {
    let unc = Bekannt::aus(Some(Path::new(r"\\server\profile$\beispiel")), None);
    assert_eq!(profil_teile(&unc), [ProfilForm::Unc(vec!["server".into(), "profile$".into(), "beispiel".into()])]);
    let w = |t: &str| verbergen_mit(t, &unc);
    assert_eq!(w(r"\\server\profile$\beispiel\Dokumente\x.pdf"), r"%USERPROFILE%\Dokumente\x.pdf");
    assert_eq!(w("//SERVER/Profile$/beispiel/x"), "%USERPROFILE%/x");
    assert_eq!(w("file://server/profile$/beispiel/x"), "%USERPROFILE%/x");
    assert_eq!(w("%5C%5Cserver%5Cprofile$%5Cbeispiel%5Cx"), "%USERPROFILE%%5Cx");
    // Ein anderer Ordner auf derselben Freigabe ist kein Profil (der Name allein wird verborgen).
    assert_eq!(w(r"\\server\profile$\beispiel2\x"), r"\\server\profile$\beispiel2\x");
    let geraet = Bekannt::aus(Some(Path::new(r"\\?\D:\Profile\beispiel")), None);
    assert_eq!(verbergen_mit(r"D:\Profile\beispiel\x", &geraet), r"%USERPROFILE%\x");
    let geraet_unc = Bekannt::aus(Some(Path::new(r"\\?\UNC\server\profile$\beispiel")), None);
    assert_eq!(verbergen_mit(r"\\server\profile$\beispiel\x", &geraet_unc), r"%USERPROFILE%\x");
}

/// Auflage: Ein zweites Setzen geht nicht still durch; mit demselben Wert
/// eine Warnung (Ok), mit einem anderen ein Fehler, und der erste Wert bleibt.
#[test]
fn zweites_setzen_nie_still() {
    let zelle = OnceLock::new();
    let a = Bekannt::aus(Some(Path::new(r"C:\Users\beispiel")), None);
    let b = Bekannt::aus(Some(Path::new(r"D:\Profile\anders")), None);
    assert_eq!(setzen_in(&zelle, a.clone()), Ok(()));
    assert_eq!(setzen_in(&zelle, a.clone()), Ok(()));
    assert!(setzen_in(&zelle, b).unwrap_err().contains("schon anders gesetzt"));
    assert_eq!(zelle.get(), Some(&a));
}

// ----- Formen von macOS und Linux (macOS-Schritt 1) -----

/// Profil unter `/Users` und `/home`, auch hinter den Vorsätzen von macOS;
/// der Rest des Pfads bleibt mit seinen Schrägstrichen.
#[test]
fn posix_users_und_home() {
    assert_eq!(v("/Users/beispiel/Documents/x.pdf"), "%USERPROFILE%/Documents/x.pdf");
    assert_eq!(v("/home/beispiel/dev"), "%USERPROFILE%/dev");
    assert_eq!(v("/Users/beispiel"), "%USERPROFILE%");
    assert_eq!(v("cd /Users/beispiel && ls"), "cd %USERPROFILE% && ls");
    assert_eq!(v("/System/Volumes/Data/Users/beispiel/x"), "%USERPROFILE%/x");
    assert_eq!(v("/Volumes/Platte/Users/beispiel/x"), "%USERPROFILE%/x");
    // Ohne Groß/Klein, wie der Datenträger von macOS vergleicht.
    assert_eq!(v("/users/Beispiel/x"), "%USERPROFILE%/x");
    // Mit Leerzeichen nur vor einem Trenner; sonst endet der Name dort.
    assert_eq!(v("/Users/beispiel konto/x"), "%USERPROFILE%/x");
    assert_eq!(v("/Users/beispiel ist nicht lesbar"), "%USERPROFILE% ist nicht lesbar");
    assert_eq!(v("Ordner /home/beispiel."), "Ordner %USERPROFILE%.");
    assert_eq!(v("„/Users/beispiel“"), "„%USERPROFILE%“");
    assert_eq!(v(r#""/Users/beispiel": (os error 13)"#), r#""%USERPROFILE%": (os error 13)"#);
    // Gegenprobe: ohne Namen dahinter bleibt es stehen.
    assert_eq!(v("/Users/"), "/Users/");
    assert_eq!(v("/Users"), "/Users");
    assert_eq!(v("/home"), "/home");
}

/// Das Profil von root, mit und ohne `/private` davor.
#[test]
fn posix_root() {
    assert_eq!(v("/root/.cargo/bin"), "%USERPROFILE%/.cargo/bin");
    assert_eq!(v("/var/root/Library"), "%USERPROFILE%/Library");
    assert_eq!(v("/private/var/root"), "%USERPROFILE%");
    // Gegenprobe: ein anderer Ordner, der nur so anfängt.
    assert_eq!(v("/rootfs/x"), "/rootfs/x");
    assert_eq!(v("/var/rootless"), "/var/rootless");
}

/// Datenträger, Laufzeitordner und Temp-Ordner je Konto: kein Profil, aber
/// zum Konto oder Rechner gehörig, je mit eigenem Platzhalter.
#[test]
fn posix_volume_uid_und_temp() {
    assert_eq!(v("/Volumes/Beispiel Platte/Fotos/x.jpg"), "/Volumes/%VOLUME%/Fotos/x.jpg");
    assert_eq!(v("/Volumes/Beispiels-Platte"), "/Volumes/%VOLUME%");
    assert_eq!(v("/run/user/1000/bus"), "/run/user/%UID%/bus");
    assert_eq!(v("/run/user/501"), "/run/user/%UID%");
    assert_eq!(v("/var/folders/ab/c9d8e7f6_0000gn/T/dashboard-1"), "%TMPDIR%/T/dashboard-1");
    assert_eq!(v("/private/var/folders/ab/c9d8e7f6_0000gn/T/x"), "%TMPDIR%/T/x");
    // Gegenprobe: was nur ähnlich aussieht, bleibt.
    assert_eq!(v("/Volumes/"), "/Volumes/");
    assert_eq!(v("/run/user/abc"), "/run/user/abc");
    assert_eq!(v("/run/users/1000"), "/run/users/1000");
    assert_eq!(v("/var/folders/"), "/var/folders/");
    assert_eq!(v("/var/log/system.log"), "/var/log/system.log");
}

/// `file:` davor fällt weg wie bei den Laufwerken; kodierte Schrägstriche zählen.
#[test]
fn posix_file_und_kodiert() {
    assert_eq!(v("file:///Users/beispiel/x.pdf"), "%USERPROFILE%/x.pdf");
    assert_eq!(v("<file:///home/beispiel/x.pdf>"), "<%USERPROFILE%/x.pdf>");
    assert_eq!(v("%2FUsers%2Fbeispiel%2Fx"), "%USERPROFILE%%2Fx");
    assert_eq!(v("%252FUsers%252Fbeispiel"), "%USERPROFILE%");
    assert_eq!(v("http://127.0.0.1/api?pfad=%2FUsers%2Fbeispiel&x=1"), "http://127.0.0.1/api?pfad=%USERPROFILE%&x=1");
    // Kodiertes `file:`: was vor dem Pfad steht, bleibt (der dritte Schrägstrich gehört zum Pfad).
    assert_eq!(v("file%3A%2F%2F%2FUsers%2Fbeispiel%2Fx"), "file%3A%2F%2F%USERPROFILE%%2Fx");
}

/// Gegentests: Adressen im Netz, Git-Remotes und Wege im Dashboard sind
/// keine lokalen Pfade und bleiben unverändert.
#[test]
fn posix_was_bleibt() {
    for text in [
        "https://example.org/users/beispiel",
        "https://example.org/home/beispiel/x",
        "http://example.org/Users/beispiel",
        "https://example.org/root/x",
        "https://example.org/Volumes/x",
        "ssh://host/Users/beispiel/repo.git",
        "ssh://konto@host:22/home/beispiel/repo.git",
        "git@host:/Users/beispiel/repo.git",
        "host.example:/home/beispiel/repo.git",
        "http://users/beispiel",
        "//rechner/Users/beispiel",
        "a/Users/beispiel",
        "pfad_/home/beispiel",
        "/projekte/x",
        "/projects/beispiel/assets/x.pdf",
        "/anstehend",
        "/usr/local/bin/git",
        "/opt/homebrew/bin/node",
        "/Userspace/x",
        "/homepage/x",
    ] {
        assert_eq!(v(text), text, "{text}");
    }
}

/// Ein bekanntes Profil ohne Laufwerk, auch außerhalb von `/Users`.
#[test]
fn posix_bekanntes_profil() {
    let b = Bekannt::aus(Some(Path::new("/Volumes/Daten/heim/beispiel")), Some("beispiel"));
    assert_eq!(profil_teile(&b), [ProfilForm::Posix(vec!["Volumes".into(), "Daten".into(), "heim".into(), "beispiel".into()])]);
    let w = |t: &str| verbergen_mit(t, &b);
    assert_eq!(w("/Volumes/Daten/heim/beispiel/Documents/x.pdf"), "%USERPROFILE%/Documents/x.pdf");
    assert_eq!(w("file:///Volumes/Daten/heim/beispiel/x"), "%USERPROFILE%/x");
    assert_eq!(w("/volumes/daten/HEIM/Beispiel"), "%USERPROFILE%");
    // Nur der ganze Ordner: „beispiel2“ ist ein anderer; der Datenträger wird trotzdem verborgen.
    assert_eq!(w("/Volumes/Daten/heim/beispiel2/x"), "/Volumes/%VOLUME%/heim/beispiel2/x");
    // Der Name allein, auch in einem Rechnernamen als ganzes Wort.
    assert_eq!(w("Konto beispiel angemeldet"), "Konto %USERNAME% angemeldet");
    // Gegenprobe: ohne bekannten Inhaber greift nur die allgemeine Form.
    assert_eq!(v("/Volumes/Daten/heim/beispiel/x"), "/Volumes/%VOLUME%/heim/beispiel/x");
}

/// Ob ein Profil absolut ist, entscheidet sein Text: auf jeder Plattform gleich.
#[test]
fn absolut_als_text_auf_jeder_plattform() {
    for ja in [
        r"C:\Users\beispiel",
        "C:/Users/beispiel",
        "/Users/beispiel",
        "/home/beispiel",
        r"\\server\profile$\beispiel",
        r"\\?\D:\Profile\beispiel",
        "//server/x",
    ] {
        assert!(absolut_als_text(ja), "{ja}");
    }
    for nein in ["beispiel", "Users/beispiel", r"Users\beispiel", "C:beispiel", "C:", "", "~/x", "./x"] {
        assert!(!absolut_als_text(nein), "{nein}");
    }
    // Beide Schreibweisen ergeben auf jeder Plattform Profil und Namen.
    let win = Bekannt::aus(Some(Path::new(r"C:\Users\beispiel\")), None);
    assert_eq!((win.profile, win.namen), (vec![r"C:\Users\beispiel".to_string()], vec!["beispiel".to_string()]));
    let mac = Bekannt::aus(Some(Path::new("/Users/beispiel/")), None);
    assert_eq!((mac.profile, mac.namen), (vec!["/Users/beispiel".to_string()], vec!["beispiel".to_string()]));
    // Gegenprobe: die Wurzel allein ist kein Profil und gibt keinen Namen.
    assert_eq!(Bekannt::aus(Some(Path::new("/")), None), Bekannt::default());
    assert_eq!(Bekannt::aus(Some(Path::new(r"C:\")), None), Bekannt::default());
}

/// Der Inhaber aus der Umgebung: unter Windows nur `USERPROFILE` und
/// `USERNAME` (wie bisher), sonst `HOME` und `USER`/`LOGNAME`.
#[test]
fn inhaber_aus_werten_der_umgebung() {
    let pfad = |t: &str| Some(PathBuf::from(t));
    let text = |t: &str| Some(t.to_string());
    // Windows: beides gesetzt, HOME und USER zählen nicht (etwa in der Git Bash).
    let b = Bekannt::aus_werten(pfad(r"C:\Users\beispiel"), pfad("/c/Users/beispiel2"), text("beispiel"), &[text("drittes")]);
    assert_eq!((b.profile, b.namen), (vec![r"C:\Users\beispiel".to_string()], vec!["beispiel".to_string()]));
    // macOS und Linux: nur HOME, USER und LOGNAME.
    let b = Bekannt::aus_werten(None, pfad("/Users/beispiel"), None, &[text("beispiel"), text("zweitname")]);
    assert_eq!((b.profile, b.namen), (vec!["/Users/beispiel".to_string()], vec!["beispiel".to_string(), "zweitname".to_string()]));
    // Leere Werte zählen als fehlend.
    let b = Bekannt::aus_werten(pfad(""), pfad("/home/beispiel"), text(" "), &[None, text("konto-x")]);
    assert_eq!((b.profile, b.namen), (vec!["/home/beispiel".to_string()], vec!["beispiel".to_string(), "konto-x".to_string()]));
    // Gegenprobe: nichts gesetzt, nichts bekannt.
    assert_eq!(Bekannt::aus_werten(None, None, None, &[None, None]), Bekannt::default());
}

/// Ohne `USERPROFILE` steht das Profil aus `HOME` unter den bekannten
/// Profilen dieser Umgebung (macOS, Linux).
#[test]
fn eigenes_profil_aus_home_ohne_setzen() {
    if std::env::var_os("USERPROFILE").is_none()
        && let Some(p) = std::env::var_os("HOME").filter(|p| p.len() > 3)
    {
        let heim = p.to_string_lossy().trim_end_matches('/').to_string();
        assert!(Bekannt::aus_umgebung().profile.contains(&heim));
    }
}

/// Der Ordnername von `HOME` zählt nur als Name, wenn `HOME` direkt unter
/// `/Users` oder `/home` liegt (kein Dienstkonto, nicht root).
#[test]
fn ordnername_nur_bei_heim_eines_kontos() {
    let pfad = |t: &str| Some(PathBuf::from(t));
    let b = Bekannt::aus_werten(None, pfad("/var/lib/dienst"), None, &[None]);
    assert_eq!((b.profile, b.namen), (vec!["/var/lib/dienst".to_string()], Vec::<String>::new()));
    let b = Bekannt::aus_werten(None, pfad("/var/root"), None, &[Some("root".to_string())]);
    assert_eq!((b.profile, b.namen), (vec!["/var/root".to_string()], vec!["root".to_string()]));
    let b = Bekannt::aus_werten(None, pfad("/home/beispiel/unter"), None, &[None]);
    assert_eq!(b.namen, Vec::<String>::new());
    for heim in ["/Users/beispiel", "/home/beispiel/"] {
        assert_eq!(Bekannt::aus_werten(None, pfad(heim), None, &[None]).namen, vec!["beispiel".to_string()], "{heim}");
    }
    // Unter Windows bleibt es, wie es war: der Ordnername von USERPROFILE zählt immer.
    assert_eq!(Bekannt::aus_werten(pfad(r"D:\Profile\beispiel"), None, None, &[None]).namen, vec!["beispiel".to_string()]);
}

/// Eine Liste von Pfaden mit Doppelpunkt (`PATH`): jeder Pfad wird für sich
/// geschwärzt, auch hinter einer Variable. Ein Ziel in der scp-Form
/// (`host:/pfad`) bleibt.
#[test]
fn posix_pfadliste() {
    assert_eq!(v("PATH=/usr/bin:/Users/beispiel/.cargo/bin"), "PATH=/usr/bin:%USERPROFILE%/.cargo/bin");
    assert_eq!(v("/usr/bin:/home/beispiel/bin"), "/usr/bin:%USERPROFILE%/bin");
    assert_eq!(v("PATH=/Users/beispiel/bin:/usr/bin:/home/beispiel2/bin:/root/bin"), "PATH=%USERPROFILE%/bin:/usr/bin:%USERPROFILE%/bin:%USERPROFILE%/bin");
    assert_eq!(v("Pfad ist /usr/bin:/Volumes/Platte/x"), "Pfad ist /usr/bin:/Volumes/%VOLUME%/x");
    assert_eq!(v("export PATH=$PATH:/Users/beispiel/.cargo/bin"), "export PATH=$PATH:%USERPROFILE%/.cargo/bin");
    assert_eq!(v("PATH=${PATH}:/home/beispiel/bin:~/bin:/root/bin"), "PATH=${PATH}:%USERPROFILE%/bin:~/bin:%USERPROFILE%/bin");
    assert_eq!(v("%PATH%:/Users/beispiel/bin"), "%PATH%:%USERPROFILE%/bin");
    // Laufwerke in einer Liste: das Laufwerk gehört zum Profil.
    assert_eq!(v("PATH=C:/bin:C:/Users/beispiel/bin"), "PATH=C:/bin:%USERPROFILE%/bin");
    // Eine Adresse später im selben Wort ändert nichts (kompaktes JSON).
    assert_eq!(
        v(r#"{"path":"/usr/bin:/Users/beispiel2/bin","url":"https://example.org"}"#),
        r#"{"path":"/usr/bin:%USERPROFILE%/bin","url":"https://example.org"}"#
    );
    for text in [
        "git@host:/Users/beispiel/repo.git",
        "host.example:/home/beispiel/repo.git",
        "konto@host-1.example:/home/beispiel/repo.git",
        "ssh://konto@host:22/home/beispiel/repo.git",
        r#"{"remote":"git@host:/Users/beispiel/repo.git"}"#,
        "https://example.org/ab:/Users/beispiel",
        "ssh://host/x:/home/beispiel",
    ] {
        assert_eq!(v(text), text, "{text}");
    }
}

/// Zwei Pfade hintereinander bleiben zwei Pfade.
#[test]
fn posix_zwei_pfade() {
    assert_eq!(v("ls /Users/beispiel /Users/beispiel2"), "ls %USERPROFILE% %USERPROFILE%");
    assert_eq!(v("/home/a;/home/b"), "%USERPROFILE%;%USERPROFILE%");
}

/// `file://localhost` vor einem Pfad ohne Laufwerk, ohne Groß/Klein.
#[test]
fn posix_file_localhost() {
    assert_eq!(v("file://localhost/Users/beispiel/x"), "file://localhost%USERPROFILE%/x");
    assert_eq!(v("FILE://LOCALHOST/home/beispiel"), "FILE://LOCALHOST%USERPROFILE%");
    assert_eq!(v(r#""file:\/\/localhost\/home\/beispiel\/x""#), r#""file:\/\/localhost%USERPROFILE%\/x""#);
    assert_eq!(v(r#"{"cwd":"file:\/\/\/Users\/beispiel2\/x"}"#), r#"{"cwd":"%USERPROFILE%\/x"}"#);
    assert_eq!(v("file:/Users/beispiel/x.pdf"), "%USERPROFILE%/x.pdf");
    // Gegenprobe: ein anderer Rechner ist eine Adresse und bleibt.
    assert_eq!(v("file://rechner/Users/beispiel/x"), "file://rechner/Users/beispiel/x");
    assert_eq!(v("http://localhost/Users/beispiel"), "http://localhost/Users/beispiel");
}

/// Schrägstriche wie in JSON (`\/`).
#[test]
fn posix_json_escape() {
    assert_eq!(v(r#""\/Users\/beispiel\/x""#), r#""%USERPROFILE%\/x""#);
    assert_eq!(v(r#""\/home\/beispiel""#), r#""%USERPROFILE%""#);
    // Gegenprobe: eine Domäne davor bleibt eine Adresse.
    assert_eq!(v(r"example.org\/home\/beispiel"), r"example.org\/home\/beispiel");
}

/// Ein Laufwerk beginnt auch nach einem Escape (`\n`, `\t`, `\u0022`,
/// `\x22`, oktal), einem kodierten Zeichen (`%22`, `%2522`) oder einem
/// Farbcode (`ESC[34m`).
#[test]
fn laufwerk_nach_escape_oder_kodiert() {
    assert_eq!(v(r"Fehler:\nC:\\Users\\beispiel\\x"), r"Fehler:\n%USERPROFILE%\\x");
    assert_eq!(v("%22C:/Users/beispiel/x"), "%22%USERPROFILE%/x");
    assert_eq!(v("%2522C:/Users/beispiel/x"), "%2522%USERPROFILE%/x");
    assert_eq!(v(r"x\tC:\Users\beispiel"), r"x\t%USERPROFILE%");
    assert_eq!(v(r"\u0022C:\\Users\\beispiel"), r"\u0022%USERPROFILE%");
    assert_eq!(v(r"\x22C:\\Users\\beispiel"), r"\x22%USERPROFILE%");
    assert_eq!(v(r"\042C:\Users\beispiel"), r"\042%USERPROFILE%");
    assert_eq!(v("\x1b[34mC:\\Users\\beispiel"), "\x1b[34m%USERPROFILE%");
    assert_eq!(v("\x1b[1;34mC:/Users/beispiel"), "\x1b[1;34m%USERPROFILE%");
    // Gegenprobe: mitten im Wort ohne Escape ist es kein Laufwerk.
    for text in [r"abC:\Users\beispiel", r"am[34mC:\Users\beispiel", r"\u00zzC:\Users\beispiel"] {
        assert_eq!(v(text), text, "{text}");
    }
}

/// Ein Schalter direkt vor dem Pfad (`-I`, `-L`, `-isystem`).
#[test]
fn schalter_vor_dem_pfad() {
    assert_eq!(v(r"cc -IC:\Users\beispiel\include -LC:/Users/beispiel/lib"), r"cc -I%USERPROFILE%\include -L%USERPROFILE%/lib");
    assert_eq!(v("cc -I/Users/beispiel/include -isystem/home/beispiel/x"), "cc -I%USERPROFILE%/include -isystem%USERPROFILE%/x");
    assert_eq!(v("--prefix/Users/beispiel"), "--prefix%USERPROFILE%");
    // Gegenprobe: ein Rechnername mit Bindestrich ist kein Schalter.
    for text in ["konto@mein-host/Users/beispiel", "a-b/Users/beispiel", r"mein-hostC:\Users\beispiel"] {
        assert_eq!(v(text), text, "{text}");
    }
}

/// Ein langer Text ohne Leerraum wird nur einmal gelesen: Die Zeit wächst
/// mit der Länge, nicht mit ihrem Quadrat.
#[test]
fn langer_text_ohne_leerraum() {
    let n = 100_000;
    let proben = [
        "/a:".repeat(n / 3),
        "file:".repeat(n / 5),
        r"C:\Users\".repeat(n / 9),
        format!("{}:/Users/beispiel", "a".repeat(n)),
        format!("{}:/Users/beispiel", "/a".repeat(n / 2)),
        format!("/Users/{name}", name = "a ".repeat(n / 2)),
        format!("{}/Users/beispiel", "$a:".repeat(n / 3)),
        format!("{}C:/Users/beispiel", r"\u0022".repeat(n / 6)),
        format!("\x1b[{}mC:/Users/beispiel", "1;".repeat(n / 2)),
        "-".repeat(n),
        format!("{}/Users/beispiel", "-I".repeat(n / 2)),
    ];
    // Ohne Optimierung gebaut; mit allen Tests zugleich streut die Messung.
    let grenze = crate::zeitgrenze(std::time::Duration::from_millis(200), std::time::Duration::from_millis(2000));
    for p in &proben {
        let beginn = std::time::Instant::now();
        let _ = v(p);
        let dauer = beginn.elapsed();
        assert!(dauer < grenze, "{}… {dauer:?}", &p[..12]);
    }
    // Doppelt so lang, höchstens rund doppelt so lange (Spielraum: Faktor 4).
    let messen = |text: &str| {
        let beginn = std::time::Instant::now();
        let _ = v(text);
        beginn.elapsed()
    };
    let (kurz, lang) = ("/a:".repeat(40_000), "/a:".repeat(80_000));
    let (a, b) = (messen(&kurz).max(std::time::Duration::from_millis(1)), messen(&lang));
    assert!(b < a * 4 + grenze / 4, "{a:?} und {b:?}");
}

/// Ein Name endet an einem kodierten Doppelpunkt: Das Laufwerk dahinter
/// wird für sich erkannt.
#[test]
fn name_endet_am_kodierten_doppelpunkt() {
    assert_eq!(v("/Users/a und C%3A%5CUsers%5Cbeispiel"), "%USERPROFILE% und %USERPROFILE%");
}
