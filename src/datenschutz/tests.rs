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
