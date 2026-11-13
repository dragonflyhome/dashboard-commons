//! Der Scanner für Svelte- und TS-Dateien (Etappe 4v, Teil B): je verbotene
//! Form ein Fall mit Satz und Zeile, die Daten-Regel mit ihren Grenzfällen,
//! einmalige Links, svelte-check (Ausgabe lesen, echter Lauf nur mit Node)
//! und die Bauliste. Testdaten in `tests/beispiel/scanner/` und in Strings.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use dashboard::inhalt::pruefung::{self, Befund};
use dashboard::werkzeug::{pruefe, scanner};

fn beispiel() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("beispiel").join("scanner")
}

fn svelte(text: &str) -> Vec<Befund> {
    scanner::pruefe_datei("projects/t/Seite.svelte", text)
}

fn saetze(b: &[Befund]) -> Vec<String> {
    b.iter().map(|b| format!("{} {b}", if b.ist_fehler() { "FEHLER" } else { "HINWEIS" })).collect()
}

/// Genau ein Fehler, in Zeile `zeile`, mit `stueck` im Satz.
fn ein_fehler(b: &[Befund], zeile: u32, stueck: &str) {
    let f: Vec<&Befund> = b.iter().filter(|b| b.ist_fehler()).collect();
    assert_eq!(f.len(), 1, "{:#?}", saetze(b));
    assert_eq!(f[0].zeile, Some(zeile), "{:#?}", saetze(b));
    assert!(f[0].text.contains(stueck), "„{stueck}“ fehlt: {:#?}", saetze(b));
    assert_eq!(f[0].datei, "projects/t/Seite.svelte");
}

/// Ein Testfall je verbotener Form: Svelte-Text, Zeile, Stück des Satzes.
macro_rules! verboten {
    ($($name:ident: $text:expr, $zeile:expr, $satz:expr;)*) => {
        $(
            #[test]
            fn $name() {
                ein_fehler(&svelte($text), $zeile, $satz);
            }
        )*
    };
}

verboten! {
    fetch_im_skript: "<script>\n\tconst r = fetch('/api/daten');\n</script>\n", 2, "`fetch` ist in Projektseiten verboten. Daten kommen nur über `ladeDaten` aus '$projekt'.";
    fetch_auf_schluessel: "<script lang=\"ts\">\n\n\tconst a = await fetch('/api/schluessel');\n</script>\n", 3, "`fetch` ist in Projektseiten verboten.";
    fetch_im_markup: "<p>\n{#await fetch('/x') then r}{r.ok}{/await}\n</p>", 2, "`fetch` ist in Projektseiten verboten.";
    xml_http_request: "<script>\n\tconst x = new XMLHttpRequest();\n</script>", 2, "`XMLHttpRequest` ist verboten.";
    event_source: "<script>\n\tconst q = new EventSource('/x');\n</script>", 2, "`EventSource` ist verboten.";
    web_socket: "<script>\n\tconst w = new WebSocket('wss://example.org');\n</script>", 2, "`WebSocket` ist verboten.";
    send_beacon: "<script>\n\tnavigator.sendBeacon('/x', '');\n</script>", 2, "`navigator.sendBeacon` ist verboten.";
    html_im_markup: "<div>\n\t{@html inhalt}\n</div>", 2, "`{@html …}` ist verboten.";
    inner_html: "<script>\n\tlet el: HTMLElement;\n\tel.innerHTML = 'x';\n</script>", 3, "`innerHTML`, `outerHTML`, `insertAdjacentHTML` und `document.write` sind verboten";
    document_write: "<script>\n\tdocument.write('x');\n</script>", 2, "`document.write` sind verboten";
    eval_aufruf: "<script>\n\teval('a');\n</script>", 2, "`eval` ist verboten.";
    new_function: "<script>\n\tconst f = new Function('return a');\n</script>", 2, "`new Function` ist verboten.";
    local_storage: "<script>\n\tlocalStorage.setItem('a', 'b');\n</script>", 2, "`localStorage` ist verboten.";
    session_storage: "<script>\n\tsessionStorage.getItem('a');\n</script>", 2, "`sessionStorage` ist verboten.";
    document_cookie: "<script>\n\tconst c = document.cookie;\n</script>", 2, "`document.cookie` ist verboten.";
    url_http_im_css: "<p>x</p>\n<style>\n\tp { background: url(https://example.org/bild.png); }\n</style>", 3, "`url(http…)` lädt eine Datei aus dem Netz.";
    css_import: "<style>\n\t@import 'fremd.css';\n</style>", 2, "`@import` im CSS ist verboten.";
    script_src: "<script src=\"./fremd.js\"></script>\n<p>x</p>", 1, "`<script src=…>` ist verboten.";
    script_im_markup: "<div>\n\t<script>let a;</script>\n</div>", 2, "Ein `<script>` im Markup ist verboten.";
    javascript_adresse: "<p>\n\t<a href=\"javascript:alert(1)\">x</a>\n</p>", 2, "Die Adresse „javascript:alert(1)“ ist verboten.";
    javascript_adresse_im_text: "<script>\n\tconst ziel = 'JavaScript:void(0)';\n</script>", 2, "ist verboten. Links nur mit https://";
    data_adresse: "<a href=\"data:text/html,x\">x</a>", 1, "Die Adresse „data:text/html,x“ ist verboten.";
    link_ohne_https: "<a href=\"http://example.org\">x</a>", 1, "Links nach außen nur mit https://.";
    bild_aus_dem_netz: "<p>x</p>\n<img src=\"https://example.org/a.png\" alt=\"\" />", 2, "lädt eine Datei aus dem Netz.";
    iframe: "<iframe title=\"x\" src=\"/projects/t\"></iframe>", 1, "`<iframe>`, `<object>`, `<embed>` und `<base>` sind verboten.";
    ereignis_als_text: "<button onclick=\"machWas()\">x</button>", 1, "Ereignisse wie `onclick` nur mit {…}";
    farbe_hex: "<style>\n\tp { color: #ff0000; }\n</style>", 2, "Freie Farbe „#ff0000“. Farben nur über die Design-Token (var(--…)), nicht frei.";
    farbe_rgb_im_style_attribut: "<p style=\"color: rgb(255 0 0)\">x</p>", 1, "Freie Farbe „rgb(“.";
    farbe_oklch: "<style>\n\tp {\n\t\tborder-color: oklch(70% 0.1 200);\n\t}\n</style>", 3, "Freie Farbe „oklch(“.";
    farbe_benannt: "<style>\n\tp { background: white; }\n</style>", 2, "Freie Farbe „white“.";
    farbe_im_fallback: "<style>\n\tp { color: var(--text, #000); }\n</style>", 2, "Freie Farbe „#000“.";
    farbe_im_skript: "<script>\n\tconst c = '#ff0000';\n</script>", 2, "Freie Farbe „#ff0000“.";
    farbe_svg_fill: "<svg>\n\t<circle fill=\"red\" />\n</svg>", 2, "Freie Farbe „red“.";
    farbe_style_direktive: "<p style:color=\"hsl(0 50% 50%)\">x</p>", 1, "Freie Farbe „hsl(“.";
    import_aus_fremdem_projekt: "<script>\n\timport K from '../anderes/Karte.svelte';\n</script>", 2, "Der Import „../anderes/Karte.svelte“ führt aus dem eigenen Projektordner hinaus";
    import_aus_web_src: "<script>\n\timport { x } from '$lib/api';\n</script>", 2, "Der Import „$lib/api“ ist nicht erlaubt.";
    import_paket: "<script>\n\timport _ from 'lodash';\n</script>", 2, "Der Import „lodash“ ist nicht erlaubt.";
    import_svelte_intern: "<script>\n\timport { x } from 'svelte/internal';\n</script>", 2, "Der Import „svelte/internal“ ist nicht erlaubt.";
    import_ohne_namen: "<script>\n\timport 'https://example.org/x.js';\n</script>", 2, "ist nicht erlaubt.";
    export_from: "<script module>\n\texport { x } from '../anderes/x';\n</script>", 2, "führt aus dem eigenen Projektordner hinaus";
    import_mit_variable: "<script>\n\tconst m = await import(name);\n</script>", 2, "`import()` nur mit einer festen Adresse in Anführungszeichen";
    import_mit_vorlage: "<script>\n\tconst m = await import(`./${name}.svelte`);\n</script>", 2, "`import()` nur mit einer festen Adresse";
    import_dynamisch_fremd: "<script>\n\tconst m = await import('../anderes/x.svelte');\n</script>", 2, "führt aus dem eigenen Projektordner hinaus";
    datum_im_markup: "<h2>Plan</h2>\n<p>Abgabe am 2026-11-02</p>", 2, "Eingetipptes Datum „2026-11-02“. Termine gehören in daten/*.toml.";
    monat_im_skript: "<script>\n\tconst start = '2026-11';\n</script>", 2, "Eingetipptes Datum „2026-11“.";
    datum_deutsch_im_markup: "<p>Stand: 2.11.2026</p>", 1, "Eingetipptes Datum „2.11.2026“.";
    datum_in_attribut: "<p title=\"Stand 2026-10-27\">x</p>", 1, "Eingetipptes Datum „2026-10-27“.";
    new_date_mit_werten: "<script>\n\tconst d = new Date(2026, 10, 2);\n</script>", 2, "`new Date(…)` mit festen Werten ist ein eingetippter Termin.";
    date_utc: "<script>\n\tconst d = Date.UTC(2026, 10);\n</script>", 2, "`new Date(…)` mit festen Werten";
    reihe_in_liste: "<script>\n\tconst r = $state([3, 5, 8]);\n</script>", 2, "Eine Reihe mit 3 eingetippten Zahlen. Zahlen und Reihen gehören in daten/*.toml.";
    reihe_in_objekt: "<script>\n\tconst o = { a: 1.5, b: 2, c: 3 };\n</script>", 2, "Eine Reihe mit 3 eingetippten Zahlen.";
    reihe_verschachtelt: "<script>\n\tconst p = [\n\t\t[2, 3],\n\t\t[4, 5]\n\t];\n</script>", 2, "Eine Reihe mit 4 eingetippten Zahlen.";
    reihe_im_markup: "{#each [10, 20, 30] as x}\n\t<p>{x}</p>\n{/each}", 1, "Eine Reihe mit 3 eingetippten Zahlen.";
    svg_mit_pfad: "<svg viewBox=\"0 0 10 10\">\n\t<path d=\"M0 0 L10 10\" />\n</svg>", 2, "Eingebettetes SVG mit Koordinaten (`d=`). Abbildungen gehören als Datei nach assets/ oder als Daten nach daten/*.toml.";
    svg_mit_punkten: "<svg>\n\t<polyline points=\"0,0 5,5\" />\n</svg>", 2, "Eingebettetes SVG mit Koordinaten (`points=`).";
    // Nach der Prüfung der Architektur-Rolle (4v-Nachbesserung).
    import_meta_glob: "<script>\n\tconst m = import.meta.glob('./*.svelte');\n</script>", 2, "`import.meta` ist verboten (etwa `import.meta.glob` bindet beliebige Dateien ein). Erlaubt ist nur `import.meta.env`.";
    import_meta_url: "<script>\n\tconst u = new URL('./x', import.meta.url);\n</script>", 2, "`import.meta` ist verboten";
    import_meta_im_markup: "<p>{import.meta.glob('./x.ts')}</p>", 1, "`import.meta` ist verboten";
    global_im_style: "<p>x</p>\n<style>\n\t:global(nav) { display: none; }\n</style>", 3, "`:global` ist verboten. Stil gilt nur in der eigenen Datei; `:global(nav) { display: none }` bliebe sonst auf allen Seiten des Dashboards aktiv.";
    global_als_block: "<style>\n\tp {\n\t\t:global { a { margin: 0; } }\n\t}\n</style>", 3, "`:global` ist verboten.";
    global_keyframes: "<style>\n\t@keyframes -global-drehen { }\n</style>", 2, "`:global` ist verboten.";
    global_im_style_attribut: "<p style=\"display: none; :global(nav)\">x</p>", 1, "`:global` ist verboten.";
    global_in_style_direktive: "<p style:display=\":global(x)\">x</p>", 1, "`:global` ist verboten.";
    style_global_attribut: "<style global>\n\tp { margin: 0; }\n</style>", 1, "`:global` ist verboten.";
    svelte_window: "<svelte:window onkeydown={weg} />\n<p>x</p>", 1, "`<svelte:window>` ist verboten. Eine Projektseite hängt nichts an Fenster oder Dokument des Dashboards";
    svelte_body: "<p>x</p>\n<svelte:body onclick={weg} />", 2, "`<svelte:body>` ist verboten.";
    svelte_document: "<svelte:document onvisibilitychange={weg} />", 1, "`<svelte:document>` ist verboten.";
    svelte_head: "<svelte:head>\n\t<title>Mein Titel</title>\n</svelte:head>", 1, "`<svelte:head>` ist verboten. Titel und Kopf der Seite gehören dem Dashboard.";
    svelte_element: "<p>x</p>\n<svelte:element this={tag}>x</svelte:element>", 2, "`<svelte:element>` ist verboten. Elemente stehen fest im Markup";
    indexed_db: "<script>\n\tconst r = indexedDB.open('x');\n</script>", 2, "`indexedDB` ist verboten.";
    caches_open: "<script>\n\tconst c = await caches.open('x');\n</script>", 2, "`caches` ist verboten.";
    new_worker: "<script>\n\tconst w = new Worker('./w.ts');\n</script>", 2, "`new Worker`, `SharedWorker` und Service Worker sind verboten.";
    new_shared_worker: "<script>\n\tconst w = new SharedWorker('./w.ts');\n</script>", 2, "`new Worker`, `SharedWorker` und Service Worker sind verboten.";
    service_worker: "<script>\n\tnavigator.serviceWorker.register('/sw.js');\n</script>", 2, "Service Worker sind verboten.";
    set_timeout_mit_text: "<script>\n\tsetTimeout('machWas()', 10);\n</script>", 2, "`setTimeout` mit Text als erstem Wert ist verboten.";
    set_interval_mit_vorlage: "<script>\n\twindow.setInterval(`machWas()`);\n</script>", 2, "`setInterval` mit Text als erstem Wert ist verboten.";
    url_http_in_style_direktive: "<p style:background=\"url(https://example.org/a.png)\">x</p>", 1, "`url(http…)` lädt eine Datei aus dem Netz.";
}

#[test]
fn script_in_svelte_head() {
    let b = svelte("<svelte:head>\n\t<script>let a;</script>\n</svelte:head>");
    assert_eq!(
        saetze(&b),
        [
            "FEHLER projects/t/Seite.svelte, Zeile 1: `<svelte:head>` ist verboten. Titel und Kopf der Seite gehören dem Dashboard.",
            "FEHLER projects/t/Seite.svelte, Zeile 2: Im `<svelte:head>` ist kein `<script>` erlaubt.",
        ]
    );
}

/// Was erlaubt bleibt: `import.meta.env`, Funktionen in setTimeout, eigene
/// Namen wie `caches` oder `worker`.
#[test]
fn erlaubte_nachbarn_der_neuen_regeln() {
    let text = "<script lang=\"ts\">\n\tconst modus = import.meta.env.MODE;\n\tsetTimeout(() => weiter(), 0);\n\tsetInterval(weiter);\n\tconst caches = { a: 0 };\n\tconst worker = caches.a;\n\tlet global = $state(0);\n</script>\n<p class=\"global\">{modus} {worker} {global}</p>\n<style>\n\t.global { margin: 0; }\n</style>\n";
    let b = svelte(text);
    assert!(b.is_empty(), "{:#?}", saetze(&b));
}

#[test]
fn gute_dateien_ohne_befund() {
    let dir = beispiel().join("projects");
    let b = scanner::pruefe_projekt(&dir, "gut");
    assert!(b.is_empty(), "{:#?}", saetze(&b));
}

/// Eine Datei oben im Projekt `erstes` (Tiefe 0) und eine in `seiten/` (Tiefe 1).
const OBEN: &str = "projects/erstes/Seite.svelte";
const UNTEN: &str = "projects/erstes/seiten/plan.svelte";

#[test]
fn erlaubte_importe() {
    for (adresse, datei) in [
        ("$katalog", OBEN),
        ("$projekt", OBEN),
        ("$vertrag", OBEN),
        ("$vertrag/projekte/erstes", OBEN),
        ("$vertrag/projekte/erstes", UNTEN),
        ("svelte", OBEN),
        ("svelte/transition", OBEN),
        ("svelte/motion", OBEN),
        ("./Karte.svelte", OBEN),
        ("./teile/Karte.svelte", OBEN),
        ("./teile/../Karte.svelte", OBEN),
        ("../Teil.svelte", UNTEN),
        ("./typen.ts", OBEN),
        ("./zustand.svelte.ts", OBEN),
        // Ohne Endung: erlaubt, wenn es die .ts gibt (prüft erst pruefe_projekt).
        ("./typen", OBEN),
    ] {
        assert_eq!(scanner::import_mangel(adresse, datei), None, "{adresse}");
    }
    for (adresse, datei) in [
        ("../x.svelte", OBEN),
        ("../../x.svelte", UNTEN),
        ("./a/../../x", OBEN),
        ("$katalogx", OBEN),
        ("$lib", OBEN),
        ("$app/navigation", OBEN),
        ("#lib/x", OBEN),
        ("/projects/x/Seite.svelte", OBEN),
        ("C:/x.svelte", OBEN),
        (".\\x.svelte", OBEN),
        ("svelte/store", OBEN),
        ("bits-ui", OBEN),
        // Etappe 4v, nach der Prüfung: Aliase ohne Unterpfad, nur .svelte und .ts.
        ("./bild.png?url", OBEN),
        ("./Teil.svelte?raw", OBEN),
        ("./x.js", OBEN),
        ("./x.svelte.js", OBEN),
        ("./x.json", OBEN),
        ("./teile/", OBEN),
        ("./assets/x.ts", OBEN),
        ("../daten/x.ts", UNTEN),
        ("./server/x.ts", OBEN),
        ("$vertrag/projekte/fremd", OBEN),
        ("$vertrag/projekte", OBEN),
        ("$vertrag/projekte/erstes/x", OBEN),
        ("$katalog/Karte.svelte", OBEN),
        ("$projekt/laden.svelte", OBEN),
    ] {
        assert!(scanner::import_mangel(adresse, datei).is_some(), "{adresse}");
    }
}

/// Punkt 1 der Prüfung: Aliase mit Unterpfad kämen an `web/src/lib/api`
/// (postJson, api.keys()) heran.
#[test]
fn alias_mit_unterpfad_ist_verboten() {
    for adresse in ["$projekt/../api/client", "$katalog/../api/client", "$vertrag/../x", "$vertrag/projekte/fremd"] {
        let satz = scanner::import_mangel(adresse, OBEN).unwrap_or_else(|| panic!("{adresse} geht durch"));
        assert_eq!(
            satz,
            format!(
                "Der Import „{adresse}“ ist nicht erlaubt. $katalog und $projekt nur ohne Unterpfad, $vertrag nur ohne Unterpfad oder als $vertrag/projekte/<slug> des eigenen Projekts."
            )
        );
        // Auch als dynamischer Import und als export … from.
        let b = svelte(&format!("<script>\n\tconst m = await import('{adresse}');\n\texport {{ x }} from '{adresse}';\n</script>\n"));
        assert_eq!(b.iter().filter(|b| b.ist_fehler()).count(), 2, "{:#?}", saetze(&b));
    }
    // Der eigene Slug ist erlaubt, ein fremder nicht (Datei in projects/t/).
    assert!(svelte("<script>\n\timport type { P } from '$vertrag/projekte/t';\n</script>\n").is_empty());
}

#[test]
fn relative_importe_nur_auf_svelte_und_ts() {
    let b = svelte(
        "<script>\n\timport './stil.css';\n\timport x from './x.js';\n\timport y from './y.mjs';\n\timport z from './z.svelte.js';\n\timport d from './d.json';\n</script>\n",
    );
    let s = saetze(&b);
    assert_eq!(s.len(), 5, "{s:#?}");
    assert_eq!(s[0], "FEHLER projects/t/Seite.svelte, Zeile 2: CSS-Importe sind verboten. Stil gehört ins <style> der Datei.");
    assert_eq!(
        s[1],
        "FEHLER projects/t/Seite.svelte, Zeile 3: Der Import „./x.js“ zeigt auf eine .js-Datei. Aus dem Projektordner nur .svelte- und .ts-Dateien importieren."
    );
    assert!(s[2].contains(".mjs-Datei") && s[3].contains(".js-Datei") && s[4].contains(".json-Datei"), "{s:#?}");
}

// ----- Daten-Regel -----

#[test]
fn testseite_mit_datum_und_reihe_ist_rot() {
    let b = scanner::pruefe_projekt(&beispiel().join("projects"), "daten-rot");
    let s = saetze(&b);
    assert_eq!(
        s,
        [
            "FEHLER projects/daten-rot/Seite.svelte, Zeile 2: Eine Reihe mit 3 eingetippten Zahlen. Zahlen und Reihen gehören in daten/*.toml.",
            "HINWEIS projects/daten-rot/Seite.svelte, Zeile 3: Eingetippte Zahl 240. Diese Zahl gehört in daten/*.toml.",
            "FEHLER projects/daten-rot/Seite.svelte, Zeile 6: Eingetipptes Datum „2026-11-02“. Termine gehören in daten/*.toml.",
            "HINWEIS projects/daten-rot/Seite.svelte, Zeile 7: Eingetippte Zahl 12. Diese Zahl gehört in daten/*.toml.",
            "HINWEIS projects/daten-rot/Seite.svelte, Zeile 8: Eingetippte Zahl 100 im Attribut `viewbox`. Diese Zahl gehört in daten/*.toml.",
            "FEHLER projects/daten-rot/Seite.svelte, Zeile 9: Eingebettetes SVG mit Koordinaten (`d=`). Abbildungen gehören als Datei nach assets/ oder als Daten nach daten/*.toml.",
        ],
        "{s:#?}"
    );
}

#[test]
fn einzelne_zahl_gibt_einen_hinweis() {
    let b = svelte("<script>\n\tconst breite = 240;\n</script>\n");
    assert_eq!(pruefung::zaehlen(&b), (0, 1), "{:#?}", saetze(&b));
    assert_eq!(b[0].zeile, Some(2));
    assert!(b[0].text.ends_with(scanner::SATZ_ZAHL), "{:#?}", saetze(&b));
    // Auch in .ts-Dateien und negativ.
    let b = scanner::pruefe_datei("projects/t/rechnen.ts", "export const a = -2;\n");
    assert_eq!(saetze(&b), ["HINWEIS projects/t/rechnen.ts, Zeile 1: Eingetippte Zahl -2. Diese Zahl gehört in daten/*.toml."]);
}

#[test]
fn null_eins_minus_eins_ohne_befund() {
    let text = "<script lang=\"ts\">\n\tlet a = 0;\n\tlet b = 1;\n\tlet c = -1;\n\tlet d = a - 1 + b * 1.0 + 0x0;\n\tconst l = [0, 1, -1, 0];\n\tconst o = { x: 0, y: 0, z: 1 };\n\tconst e = xs[0];\n</script>\n<p tabindex=\"-1\" data-n=\"0\">{a + 1} von 1</p>\n";
    let b = svelte(text);
    assert!(b.is_empty(), "{:#?}", saetze(&b));
}

#[test]
fn style_block_zaehlt_nicht_fuer_zahlen() {
    let b = svelte("<p>x</p>\n<style>\n\tp {\n\t\tmargin: 12px 24px;\n\t\twidth: 300px;\n\t\tz-index: 5;\n\t\tcolor: var(--text);\n\t}\n</style>\n");
    assert!(b.is_empty(), "{:#?}", saetze(&b));
    // Die Token-Regel gilt dort weiter.
    let b = svelte("<style>\n\tp { margin: 12px; color: #abc; }\n</style>\n");
    assert_eq!(saetze(&b), ["FEHLER projects/t/Seite.svelte, Zeile 2: Freie Farbe „#abc“. Farben nur über die Design-Token (var(--…)), nicht frei."]);
}

#[test]
fn attribute_und_svelte_syntax_hoechstens_hinweis() {
    let text = "<svg viewBox=\"0 0 100 50\" width=\"100\" height=\"50\"></svg>\n<textarea rows=\"4\" cols=\"40\"></textarea>\n<td colspan=\"2\" tabindex=\"3\">x</td>\n{#each xs as x, i}\n\t{#if i > 3}<p>{x}</p>{/if}\n{/each}\n<Karte breite={300} />\n";
    let b = svelte(text);
    let (f, h) = pruefung::zaehlen(&b);
    assert_eq!(f, 0, "{:#?}", saetze(&b));
    assert_eq!(h, 9, "{:#?}", saetze(&b));
    assert!(b.iter().any(|b| b.zeile == Some(1) && b.text.contains("im Attribut `viewbox`")), "{:#?}", saetze(&b));
    assert!(b.iter().any(|b| b.zeile == Some(5) && b.text == "Eingetippte Zahl 3. Diese Zahl gehört in daten/*.toml."), "{:#?}", saetze(&b));
    assert!(b.iter().any(|b| b.zeile == Some(7) && b.text.contains("300")), "{:#?}", saetze(&b));
}

#[test]
fn bild_datei_ist_ein_dateiname_kein_datum() {
    // `datei=` am Baustein Bild ist ein Name in assets/; Ziffern und Termine
    // darin (Handy-Fotos, „2025-06-01-zeugnis.jpg“) sind keine Daten der Seite.
    let text = "<Bild datei=\"foto.jpg\" alt=\"Porträt\" />\n<Bild datei=\"2025-06-01-zeugnis.jpg\" alt=\"Zeugnis\" />\n<Bild datei=\"01.jpg\" alt=\"Titel\" breite=\"klein\" />\n";
    let b = svelte(text);
    assert!(b.is_empty(), "{:#?}", saetze(&b));
    // Andere Attribute am Bild prüft der Scanner weiter.
    let b = svelte("<Bild datei=\"a.jpg\" alt=\"Stand 2025-06-01\" />\n");
    ein_fehler(&b, 1, "Eingetipptes Datum „2025-06-01“.");
    // Und `datei=` an anderen Elementen auch.
    let b = svelte("<Karte datei=\"2025-06-01.jpg\" />\n");
    ein_fehler(&b, 1, "Eingetipptes Datum „2025-06-01“.");
}

#[test]
fn groessen_eigenschaften_der_bausteine_sind_keine_daten() {
    // `mindestbreite` an der Tabelle ist eine Breite in rem, keine Zahl für
    // daten/*.toml (Auflage 3a-3); ebenso `linienHoehe` an der Zeitachse.
    let text = "<Tabelle beschriftung=\"Wo was liegt\" mindestbreite={28}>\n</Tabelle>\n<Tabelle mindestbreite=\"36\" />\n<Zeitachse linienHoehe={2.5} />\n";
    let b = svelte(text);
    assert!(b.is_empty(), "{:#?}", saetze(&b));
    // Ein Ausdruck statt einer reinen Zahl wird weiter geprüft.
    let b = svelte("<Tabelle mindestbreite={28 * 2} />\n");
    assert_eq!(
        saetze(&b),
        [
            "HINWEIS projects/t/Seite.svelte, Zeile 1: Eingetippte Zahl 28. Diese Zahl gehört in daten/*.toml.",
            "HINWEIS projects/t/Seite.svelte, Zeile 1: Eingetippte Zahl 2. Diese Zahl gehört in daten/*.toml.",
        ],
        "{:#?}",
        saetze(&b)
    );
    // Und dieselbe Eigenschaft an anderen Elementen auch.
    let b = svelte("<Karte mindestbreite={28} />\n");
    assert_eq!(saetze(&b), ["HINWEIS projects/t/Seite.svelte, Zeile 1: Eingetippte Zahl 28. Diese Zahl gehört in daten/*.toml."], "{:#?}", saetze(&b));
}

#[test]
fn fliesstext_zahlen_sind_hinweis_woerter_nicht() {
    let b = svelte("<p>Noch 12 Tage bis Etappe 4v, H2O, col-2, 3a und &#169;.</p>\n<p>Anteil 70 %</p>\n");
    let s = saetze(&b);
    assert_eq!(
        s,
        [
            "HINWEIS projects/t/Seite.svelte, Zeile 1: Eingetippte Zahl 12. Diese Zahl gehört in daten/*.toml.",
            "HINWEIS projects/t/Seite.svelte, Zeile 2: Eingetippte Zahl 70. Diese Zahl gehört in daten/*.toml.",
        ],
        "{s:#?}"
    );
}

#[test]
fn kommentare_werden_nicht_geprueft() {
    let text = "<!-- fetch('/x') 2026-11-02 #fff {@html x} <script src=\"x\"></script> -->\n<script>\n\t// fetch('/x') 2026-11-02 [3, 5, 8] localStorage\n\t/* eval('x') new Date(2026, 1, 1)\n\t   import x from 'lodash' */\n</script>\n<style>\n\t/* color: red; url(https://example.org) */\n</style>\n";
    let b = svelte(text);
    assert!(b.is_empty(), "{:#?}", saetze(&b));
}

#[test]
fn ts_datei_mit_regeln() {
    let text = "// Typen und Hilfen.\nimport type { Plan } from '$vertrag';\nimport { x } from '../anderes/x';\nexport const farben = ['#fff'];\nexport const r = [3, 5, 8];\nexport const f = () => fetch('/x');\n";
    let b = scanner::pruefe_datei("projects/t/hilfe.ts", text);
    let zeilen: Vec<(Option<u32>, bool)> = b.iter().map(|b| (b.zeile, b.ist_fehler())).collect();
    assert_eq!(zeilen, [(Some(3), true), (Some(4), true), (Some(5), true), (Some(6), true)], "{:#?}", saetze(&b));
}

#[test]
fn unterseite_darf_in_den_projektordner() {
    let b = scanner::pruefe_datei(
        "projects/t/seiten/plan.svelte",
        "<script>\n\timport T from '../Teil.svelte';\n\timport U from '../../u/Teil.svelte';\n</script>\n",
    );
    assert_eq!(b.len(), 1, "{:#?}", saetze(&b));
    assert_eq!(b[0].zeile, Some(3));
    assert!(b[0].text.contains("führt aus dem eigenen Projektordner hinaus"));
}

#[test]
fn projekte_aus_dem_beispiel() {
    let dir = beispiel().join("projects");
    let s = saetze(&scanner::pruefe_projekt(&dir, "schluessel"));
    assert_eq!(
        s,
        ["FEHLER projects/schluessel/Seite.svelte, Zeile 2: `fetch` ist in Projektseiten verboten. Daten kommen nur über `ladeDaten` aus '$projekt'."]
    );
    let s = saetze(&scanner::pruefe_projekt(&dir, "fremd"));
    assert_eq!(s.len(), 1, "{s:#?}");
    assert!(
        s[0].starts_with("FEHLER projects/fremd/Seite.svelte, Zeile 2: Der Import „../gut/Teil.svelte“ führt aus dem eigenen Projektordner hinaus"),
        "{s:#?}"
    );
}

// ----- Einmalige Links -----

#[test]
fn einmalige_links_sind_hinweis() {
    let b = scanner::pruefe_projekt(&beispiel().join("projects"), "links");
    let s = saetze(&b);
    assert_eq!(
        s,
        [
            "HINWEIS projects/links/daten/notiz.toml, Zeile 3: Die Adresse enthält `api_key=`. Einmalige Links gehören in eine Aufgabe, nicht ins Repo.",
            "HINWEIS projects/links/projekt.toml, Zeile 11: Die Adresse enthält `token=`. Einmalige Links gehören in eine Aufgabe, nicht ins Repo.",
        ],
        "{s:#?}"
    );
    // Der Wert selbst steht nie im Befund.
    assert!(!s.iter().any(|z| z.contains("GEHEIMWERT")));
    for (text, n) in [
        ("x = \"https://a.org/?code=1\"", 1),
        ("x = \"https://a.org/?sig=1&b=2\"", 1),
        ("x = \"https://a.org/p?Password=x\"", 1),
        ("x = \"https://a.org/p?keyword=x&monkey=y\"", 0),
        ("x = \"https://a.org/p#token=x\"", 0),
        ("token=abc ohne Adresse", 0),
    ] {
        let b = scanner::pruefe_datei("projects/t/daten/x.toml", text);
        assert_eq!(b.len(), n, "{text}: {:#?}", saetze(&b));
        assert!(b.iter().all(|b| !b.ist_fehler()));
    }
}

// ----- pruefe -----

/// Repo in einem frischen Ordner unter temp; räumt beim Freigeben auf.
struct Repo(PathBuf);

impl Repo {
    fn neu(name: &str) -> Repo {
        let o = std::env::temp_dir().join(format!("dashboard-scanner-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&o);
        std::fs::create_dir_all(o.join("projects")).unwrap();
        Repo(o)
    }

    fn datei(&self, rest: &str, text: &str) {
        let p = self.0.join(rest);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

const KOPF: &str = "schema = 3\ntitle = \"P\"\nstatus = \"aktiv\"\nsummary = \"S\"\n";

#[test]
fn pruefe_gibt_scanner_befunde_aus() {
    let r = Repo::neu("pruefe");
    r.datei("projects/p/projekt.toml", KOPF);
    r.datei("projects/p/Seite.svelte", "<script>\n\tconst a = fetch('/api/schluessel');\n</script>\n<p>Noch 12 Tage</p>\n");
    r.datei("projects/q/projekt.toml", KOPF);
    let dir = r.0.join("projects");
    let (text, code) = pruefe::lauf(&dir, &["p".to_string(), "--ohne-svelte".to_string(), "--ohne-stil".to_string()]);
    assert_eq!(code, 1, "{text}");
    assert_eq!(
        text,
        "FEHLER projects/p/Seite.svelte, Zeile 2: `fetch` ist in Projektseiten verboten. Daten kommen nur über `ladeDaten` aus '$projekt'.\n\
         HINWEIS projects/p/Seite.svelte, Zeile 4: Eingetippte Zahl 12. Diese Zahl gehört in daten/*.toml.\n\
         1 Fehler, 1 Hinweis\n"
    );
    // Ohne --ohne-svelte läuft für ein Projekt mit Svelte-Dateien svelte-check
    // mit, ab 3b auch die Stil-Prüfung, ab 4b ESLint; hier fehlt web/, also je
    // ein Hinweis mehr, kein Fehler.
    let (mit, code) = pruefe::lauf(&dir, &["p".to_string()]);
    assert_eq!(code, 1, "{mit}");
    assert!(mit.contains("HINWEIS projects/p: Die Stil-Prüfung (Stylelint) ist übersprungen") && mit.ends_with("1 Fehler, 4 Hinweise\n"), "{mit}");
    assert!(mit.contains("HINWEIS projects/p: ESLint ist übersprungen"), "{mit}");
    let (alle, code) = pruefe::lauf(&dir, &["--alle".to_string()]);
    assert_eq!(code, 1);
    assert!(alle.contains("FEHLER projects/p/Seite.svelte, Zeile 2: `fetch`"), "{alle}");
    assert_eq!(pruefe::lauf(&dir, &["--bauliste".to_string(), "x".to_string()]).1, 2);
    // Ohne web/ ist --bauliste ein Aufruffehler.
    assert_eq!(pruefe::lauf(&dir, &["--bauliste".to_string()]).1, 2);
}

// ----- Alle Dateien im Projektordner (Punkt 2 der Prüfung) -----

#[test]
fn alle_dateien_im_projektordner_werden_geprueft() {
    let r = Repo::neu("alle-dateien");
    r.datei("projects/p/Seite.svelte", "<script lang=\"ts\">\n\timport type { A } from './typen';\n\timport type { B } from './fehlt';\n\timport K from './teile/tief/Karte.svelte';\n</script>\n<K />\n");
    r.datei("projects/p/typen.ts", "export type A = string;\n");
    r.datei("projects/p/teile/tief/Karte.svelte", "<script>\n\tconst r = fetch('/api/daten/sichern');\n</script>\n");
    r.datei("projects/p/teile/hilfe.ts", "export const m = import.meta.glob('../*.svelte');\n");
    r.datei("projects/p/hilfe.js", "export const a = 1;\n");
    r.datei("projects/p/teile/stil.css", "p { margin: 0; }\n");
    r.datei("projects/p/seiten/x.html", "<p>x</p>\n");
    r.datei("projects/p/notizen.txt", "fetch('/x') zählt hier nicht\n");
    // assets/ und daten/ prüft inhalt::pruefung; server/ (4b) nur auf Verknüpfungen.
    r.datei("projects/p/assets/bericht.html", "<script>fetch('/x')</script>\n");
    r.datei("projects/p/server/modul.js", "fetch('/x');\n");
    r.datei("projects/p/server/teil.svelte", "<script>fetch('/x')</script>\n");
    let dir = r.0.join("projects");
    let s = saetze(&scanner::pruefe_projekt(&dir, "p"));
    let andere = "Programm steht nur in .svelte- und .ts-Dateien, Stil im <style> der Datei; Dateien zum Ansehen gehören nach assets/.";
    assert_eq!(
        s,
        [
            format!("FEHLER projects/p/hilfe.js: Eine .js-Datei gehört nicht in den Projektordner. {andere}"),
            format!("FEHLER projects/p/seiten/x.html: Eine .html-Datei gehört nicht in den Projektordner. {andere}"),
            format!("FEHLER projects/p/teile/stil.css: Eine .css-Datei gehört nicht in den Projektordner. {andere}"),
            "FEHLER projects/p/Seite.svelte, Zeile 3: Der Import „./fehlt“ hat keine Endung; dann muss es die Datei fehlt.ts geben. Bitte die Datei mit Endung nennen (.svelte oder .ts).".to_string(),
            "FEHLER projects/p/teile/hilfe.ts, Zeile 1: `import.meta` ist verboten (etwa `import.meta.glob` bindet beliebige Dateien ein). Erlaubt ist nur `import.meta.env`.".to_string(),
            "FEHLER projects/p/teile/tief/Karte.svelte, Zeile 2: `fetch` ist in Projektseiten verboten. Daten kommen nur über `ladeDaten` aus '$projekt'.".to_string(),
        ],
        "{s:#?}"
    );
    assert!(scanner::hat_svelte(&dir, "p"));
    // Ein Import ohne Endung, zu dem es nur eine .js gibt, ist auch ein Fehler.
    r.datei("projects/q/Seite.svelte", "<script>\n\timport { a } from './hilfe';\n</script>\n");
    r.datei("projects/q/hilfe.js", "export const a = 0;\n");
    let s = saetze(&scanner::pruefe_projekt(&dir, "q"));
    assert_eq!(s.len(), 2, "{s:#?}");
    assert!(s[1].contains("dann muss es die Datei hilfe.ts geben"), "{s:#?}");
}

/// Legt eine Junction an (über cmd.exe aus `dienst::programme`, ohne Fenster).
fn junction(link: &Path, ziel: &Path) -> bool {
    use dashboard::dienst::programme::{self, Werkzeug};
    let Ok(mut cmd) = programme::befehl(Werkzeug::Cmd) else { return false };
    cmd.args(["/c", "mklink", "/J"])
        .arg(link)
        .arg(ziel)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
        && link.exists()
}

#[test]
fn verknuepfung_in_jedem_unterordner_ist_fehler() {
    let r = Repo::neu("junction-tief");
    r.datei("projects/p/Seite.svelte", "<p>Seite</p>\n");
    r.datei("projects/p/teile/Karte.svelte", "<p>Karte</p>\n");
    r.datei("fremd/Boese.svelte", "<script>\n\tconst r = fetch('/api/schluessel');\n</script>\n");
    std::fs::create_dir_all(r.0.join("projects/p/server")).unwrap();
    // Pfade Stück für Stück bauen: cmd.exe läse `/p` als Schalter.
    let p = r.0.join("projects").join("p");
    let links = [p.join("teile").join("fremd"), p.join("server").join("fremd")];
    let angelegt = links.iter().all(|l| junction(l, &r.0.join("fremd")));
    let dir = r.0.join("projects");
    let s = saetze(&scanner::pruefe_projekt(&dir, "p"));
    let (gruen, rote) = scanner::bauliste(&r.0);
    // Nur die Junctions lösen (nicht rekursiv), dann räumt Repo den Rest weg.
    for l in &links {
        if std::fs::symlink_metadata(l).is_ok() {
            std::fs::remove_dir(l).unwrap();
        }
    }
    assert!(r.0.join("fremd/Boese.svelte").is_file(), "das Ziel der Junction bleibt");
    if !angelegt {
        eprintln!("verknuepfung_in_jedem_unterordner_ist_fehler übersprungen: Junction ließ sich nicht anlegen");
        return;
    }
    assert_eq!(
        s,
        [
            "FEHLER projects/p/server/fremd: Der Ordner server/fremd ist eine Verknüpfung (Symlink oder Junction); das Dashboard liest nur echte Ordner.",
            "FEHLER projects/p/teile/fremd: Der Ordner teile/fremd ist eine Verknüpfung (Symlink oder Junction); das Dashboard liest nur echte Ordner.",
        ],
        "{s:#?}"
    );
    assert!(gruen.is_empty() && rote.contains_key("p"), "{gruen:?} {rote:?}");
}

// ----- Bauliste -----

/// Der Einspiel-Weg (`pruefe --bauliste --svelte`) fällt das Urteil des
/// Boten: Fehlen die Werkzeuge (hier: web/node_modules), ist nichts geprüft,
/// also nichts grün; die LEERE Liste wird geschrieben, nicht die alte stehen
/// gelassen und nie der Ordner nach dem Scanner allein.
#[test]
fn bauliste_mit_svelte_ohne_werkzeuge_ist_leer() {
    let r = Repo::neu("bauliste-gescheitert");
    r.datei("projects/gut/Seite.svelte", "<p>Gut</p>\n");
    let ziel = r.0.join(pruefe::BAULISTE_DATEI);
    std::fs::create_dir_all(r.0.join("web")).unwrap();
    // Ohne --svelte prüft nur der Scanner: gut ist grün.
    let (aus, code) = pruefe::lauf(&r.0.join("projects"), &["--bauliste".to_string()]);
    assert_eq!(code, 0, "{aus}");
    assert!(aus.starts_with("GRÜN gut\n"), "{aus}");
    // Mit --svelte: gescheitert, Liste leer, Rückgabe 1.
    r.datei(pruefe::BAULISTE_DATEI, "// alte Liste\nexport const projektSeiten = { gut: () => import('x') };\n");
    let (aus, code) = pruefe::lauf(&r.0.join("projects"), &["--bauliste".to_string(), "--svelte".to_string()]);
    assert_eq!(code, 1, "{aus}");
    assert!(aus.starts_with("GESCHEITERT Die Prüfung für die Bauliste ist gescheitert: Die Prüfung lässt sich nicht machen: "), "{aus}");
    assert!(aus.contains("svelte-check ist nicht da (web/node_modules fehlt)") || aus.contains("Node nicht gefunden"), "{aus}");
    assert!(aus.ends_with("Die Bauliste ist deshalb leer: gebaut wird ohne Projektseiten.\nweb/src/lib/projekte.generated.ts geschrieben (leer).\n"), "{aus}");
    let text = std::fs::read_to_string(&ziel).unwrap();
    assert!(
        text.ends_with("export const projektSeiten: Record<string, Laden> = {};\n\nexport const projektUnterseiten: Record<string, Laden> = {};\n"),
        "{text}"
    );
    assert!(!r.0.join("web/.projekte-gruen/gut").exists(), "keine grüne Kopie ohne Prüfung");
}

/// Probe für scripts/Dienst.ps1 -Neustart: Es nimmt den Einspiel-Weg mit
/// `--svelte`, hält den Boten VOR der Bauliste an (sonst schreibt er dieselbe
/// Liste, Kopien und stand.json, während vite liest). Ein `try`/`finally`
/// vom Anhalten bis nach dem Austausch startet ihn auf jedem Weg wieder
/// (`Ende 1`, Ausnahme; Strg+C ist nicht zugesagt), und nur dort (nie
/// doppelt), aber nur, wenn keiner mehr läuft (sonst zwei Wächter). Den
/// Dienst startet das `finally` nur, solange `$dienstAus` gilt (von
/// DienstStoppen bis vor das DienstStarten am Ende des `try`); sonst startet
/// im Abschnitt nur dieses eine DienstStarten. Läuft der Bote nach
/// dem Anhalten noch oder vor der Prüfung wieder, bricht es ab. Dazu liest
/// es `FEHLT` und `GEBAUT` (nie `GRÜN` für den Bau), und die Prüfung sieht
/// immer alle Inhaltsquellen.
#[test]
fn dienst_ps1_haelt_den_boten_vor_der_bauliste_an() {
    let ps1 = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts").join("Dienst.ps1")).unwrap().replace("\r\n", "\n");
    let neustart = ps1.split("# -Neustart\n").nth(1).expect("Abschnitt -Neustart");
    let stelle = |s: &str| neustart.find(s).unwrap_or_else(|| panic!("{s} fehlt im Neustart"));
    let (versuch, stopp) = (stelle("\n    $dienstAus = $false\n    try {\n"), stelle("$nochDa = BoteStoppen"));
    let (wieder, aufruf) = (stelle("(BoteProzesse).Count -gt 0) { Ende 1"), stelle("& $dashExe pruefe --bauliste --svelte 2>&1"));
    let (dienst, tausch, start) = (
        stelle("        $dienstAus = $true\n        $fehler = DienstStoppen\n"),
        stelle("$fehler = TauscheProgramme $quelle"),
        stelle("        $dienstAus = $false\n        $fehler = DienstStarten\n    } finally {\n"),
    );
    let schluss = stelle("\n    } finally {\n        if ($dienstAus) {\n");
    let ende = schluss + 1 + neustart[schluss + 1..].find("\n    }\n").expect("Ende des finally");
    // Die Sperrdatei im Bauordner des Dienstes steht vor dem ersten cargo-Aufruf und fällt im äußeren finally.
    let (sperre, bau) = (stelle("\nSet-Content -LiteralPath $sperre "), stelle("$fassung = Fassung-Bauen $repo $script:inhalt.Lokal $bauordner\n"));
    assert!(neustart.contains("$bauordner = Join-Path $env:LOCALAPPDATA 'Dashboard\\cache\\dienst'\n$sperre = Join-Path $bauordner 'EINSPIELEN.lock'\n"));
    // Ohne CACHEDIR.TAG überspringt die tägliche Bereinigung den Platz für immer (cargo legt es nur in eigene neue Ordner).
    assert!(neustart[..sperre].contains("$tag = Join-Path $bauordner 'CACHEDIR.TAG'\n"), "Dienst.ps1 legt CACHEDIR.TAG an");
    assert!(sperre < bau && bau < versuch && neustart[sperre..bau].ends_with("\ntry {\n    "), "Sperre vor dem Bau, im äußeren try");
    assert!(
        neustart[ende..].starts_with("\n    }\n} finally {\n")
            && neustart[ende..].contains("    Remove-Item -LiteralPath $sperre -Force -ErrorAction SilentlyContinue\n}\n"),
        "äußeres finally entfernt die Sperre"
    );
    assert!(!neustart.contains("'target\\release"), "nie target des Repos");
    // Das Einrichten spielt aus demselben Bauordner ein wie -Neustart.
    let einrichten = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts").join("Dienst-Einrichten.ps1")).unwrap();
    assert!(einrichten.contains("$quelle = Join-Path $env:LOCALAPPDATA 'Dashboard\\cache\\dienst\\release'") && !einrichten.contains("'target\\release'"));
    assert!(versuch < stopp && stopp < wieder && wieder < aufruf && aufruf < dienst && dienst < tausch && tausch < start && start < schluss, "Reihenfolge");
    assert!(neustart[stopp..wieder].contains("if ($nochDa) { Ende 1 \"Der Bote ließ sich nicht anhalten, nichts eingespielt"));
    assert!(!neustart[versuch..schluss].contains("BoteStarten"), "im Abschnitt startet nur das finally den Boten");
    assert_eq!(neustart[versuch..schluss].matches("DienstStarten").count(), 1, "im try nur das DienstStarten am Ende");
    let fin = &neustart[schluss..ende];
    assert!(
        fin.contains("if ((DienstStatus) -eq 'StopPending') { $null = WarteAufStatus 'Stopped' 20 }\n            if ((DienstStatus) -ne 'Running') {\n")
            && fin.contains("$startFehler = DienstStarten -ohneEnde\n"),
        "{fin}"
    );
    assert!(
        fin.ends_with("\n        }\n        if ((BoteProzesse).Count -eq 0) { BoteStarten } else { Write-Host 'Bote läuft noch, nicht neu gestartet.' }"),
        "{fin}"
    );
    assert!(!fin.contains("Ende 1") && !fin.contains("Ende 0") && !fin.contains("exit"), "kein Ende im finally: {fin}");
    assert!(!ps1.contains("AbbruchMitBote"));
    assert!(neustart.contains("\n    $env:DASHBOARD_PROJECTS_DIR = ($script:inhalt.Projekte -join ';')\n"), "immer gesetzt");
    assert!(neustart.contains("'^FEHLT (.+)$'") && neustart.contains("'^GEBAUT (.+)$'") && !neustart.contains("'^GRÜN"), "Dienst.ps1 liest FEHLT und GEBAUT");
}

/// Nachbildung zu `dienst_ps1_haelt_den_boten_vor_der_bauliste_an` mit dem
/// echten `Ende`, `DienstStarten`, `WarteAufStatus` und dem echten `finally`
/// des Einspielens aus Dienst.ps1 (Attrappen: ScExe, DienstStatus,
/// BoteStarten, BoteProzesse) in powershell.exe 5.1. Nach `Ende 1` und nach
/// einer Ausnahme startet BoteStarten genau einmal, wenn kein Bote läuft.
/// Den Dienst startet das `finally` nur zwischen DienstStoppen und dem
/// DienstStarten am Ende des `try` (`$dienstAus`), dann genau einmal und erst
/// nach einem StopPending; davor (der Dienst läuft) und auf dem Erfolgsweg
/// nie: Dort startet der Abschnitt selbst, und scheitert der Start (auch
/// Code 5), versucht es das `finally` nicht noch einmal (sonst liefe die
/// neue Fassung doppelt an, bevor der Rückweg übernimmt).
#[cfg(windows)]
#[test]
fn dienst_ps1_finally_startet_den_boten_nach_ende_und_ausnahme() {
    let ps1 = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts").join("Dienst.ps1")).unwrap().replace("\r\n", "\n");
    let funktion = |name: &str| {
        let kopf = format!("\nfunction {name}(");
        let rumpf = ps1.split(kopf.as_str()).nth(1).and_then(|r| r.split("\n}\n").next()).unwrap_or_else(|| panic!("function {name}"));
        format!("function {name}({rumpf}\n}}\n")
    };
    let neustart = ps1.split("# -Neustart\n").nth(1).expect("Abschnitt -Neustart");
    let finale = neustart.find("\n    } finally {\n        if ($dienstAus) {\n").expect("finally des Einspielens") + 1;
    let schluss = &neustart[finale..finale + neustart[finale..].find("\n    }\n").expect("Ende des finally") + 7];
    let ordner = std::env::temp_dir().join(format!("dashboard-dienst-finally-{}", std::process::id()));
    std::fs::create_dir_all(&ordner).unwrap();
    let skript = ordner.join("probe.ps1");
    let text = format!(
        "\u{feff}$ErrorActionPreference = 'Stop'\n$Dienstname = 'Probe'\n{}{}{}\
function ScExe([string]$argumente) {{ [IO.File]::AppendAllText($env:PROBE_DATEI, 'dienst;'); if ((DienstStatus) -eq 'StopPending') {{ return [pscustomobject]@{{ Code = 1061; Text = 'FEHLER 1061' }} }}; [pscustomobject]@{{ Code = [int]$env:PROBE_SC; Text = 'sc' }} }}\n\
$script:abfragen = 0\n\
function DienstStatus {{ $script:abfragen++; if ($env:PROBE_DIENST -eq 'halt') {{ if ($script:abfragen -le 2) {{ 'StopPending' }} else {{ 'Stopped' }} }} elseif ($env:PROBE_DIENST) {{ 'Running' }} else {{ 'Stopped' }} }}\n\
function BoteStarten {{ [IO.File]::AppendAllText($env:PROBE_DATEI, 'bote;') }}\n\
function BoteProzesse {{ if ($env:PROBE_BOTE) {{ @(1) }} else {{ @() }} }}\n\
$dienstAus = $false\n\
try {{\n    if ($env:PROBE_ART -eq 'vor') {{ Ende 1 'Der Bote ließ sich nicht anhalten.' }}\n    $dienstAus = $true\n\
    if ($env:PROBE_ART -eq 'ende') {{ Ende 1 'Der Dienst hielt nicht an.' }}\n    if ($env:PROBE_ART -eq 'wurf') {{ throw 'unerwartet' }}\n\
    $dienstAus = $false\n    $fehler = DienstStarten\n\
{schluss}Write-Host \"weiter: $fehler\"\nexit 0\n",
        funktion("Ende"),
        funktion("DienstStarten"),
        funktion("WarteAufStatus")
    );
    std::fs::write(&skript, text).unwrap();
    let ohne_recht = "Keine Berechtigung, den Dienst zu starten";
    // (Weg, sc-Code, Dienst läuft, Bote läuft, Exit, gestartet, in der Ausgabe)
    let faelle = [
        ("vor", "0", "1", "", 1, "bote;", "nicht anhalten"),
        ("vor", "0", "1", "1", 1, "", "nicht anhalten"),
        ("ende", "0", "", "", 1, "dienst;bote;", "hielt nicht an"),
        ("ende", "5", "", "", 1, "dienst;bote;", "Der Dienst startet nicht: Keine Berechtigung"),
        ("ende", "0", "halt", "", 1, "dienst;bote;", "hielt nicht an"),
        ("ende", "0", "1", "1", 1, "", "hielt nicht an"),
        ("wurf", "0", "", "", 1, "dienst;bote;", "unerwartet"),
        ("wurf", "5", "", "", 1, "dienst;bote;", "unerwartet"),
        ("wurf", "0", "halt", "", 1, "dienst;bote;", "unerwartet"),
        ("gut", "0", "", "", 0, "dienst;bote;", "weiter: "),
        ("gut", "0", "", "1", 0, "dienst;", "weiter: "),
        ("gut", "1053", "", "", 0, "dienst;bote;", "weiter: sc start meldet: sc"),
        ("gut", "5", "", "", 1, "dienst;bote;", ohne_recht),
    ];
    for (i, (art, sc, dienst, bote, code, erwartet, satz)) in faelle.into_iter().enumerate() {
        let datei = ordner.join(format!("fall{i}.txt"));
        let aus = dashboard::dienst::programme::eigener_befehl(r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"])
            .arg(&skript)
            .env("PROBE_ART", art)
            .env("PROBE_SC", sc)
            .env("PROBE_DIENST", dienst)
            .env("PROBE_BOTE", bote)
            .env("PROBE_DATEI", &datei)
            .output()
            .expect("powershell.exe startet");
        let ausgabe = format!("{}{}", String::from_utf8_lossy(&aus.stdout), String::from_utf8_lossy(&aus.stderr));
        assert_eq!(aus.status.code(), Some(code), "Fall {i} ({art}, sc {sc}): {ausgabe}");
        assert!(ausgabe.contains(satz), "Fall {i} ({art}, sc {sc}): {satz:?} fehlt in {ausgabe}");
        assert_eq!(ausgabe.matches(ohne_recht).count(), usize::from(sc == "5"), "Fall {i}: Satz zu Code 5 genau einmal: {ausgabe}");
        assert!(!ausgabe.contains("1061"), "Fall {i}: Start erst nach dem Stopp: {ausgabe}");
        assert_eq!(std::fs::read_to_string(&datei).unwrap_or_default(), erwartet, "Fall {i} ({art}, Dienst {dienst:?}, Bote {bote:?})");
    }
    let _ = std::fs::remove_dir_all(&ordner);
}

/// Der Rückweg zur vorigen Fassung in scripts/Dienst.ps1 -Neustart folgt dem
/// Muster des Einspielens: ein `try`/`finally` von BoteStoppen bis nach dem
/// Zurückholen der *.alt.exe. Nur das `finally` startet: den Dienst, wenn er
/// nicht läuft (DienstStarten -ohneEnde, nie Ende), den Boten, wenn keiner
/// läuft. Davor und danach startet im Rückweg nichts (nie doppelt).
#[test]
fn dienst_ps1_rueckweg_startet_im_finally() {
    let ps1 = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts").join("Dienst.ps1")).unwrap().replace("\r\n", "\n");
    let rueck = ps1.split("Hole die vorige Fassung zurück ...\"\n").nth(1).expect("Rückweg in Dienst.ps1");
    let finale = rueck.find("\n} finally {\n").expect("finally des Rückwegs");
    let schluss = finale + 1 + rueck[finale + 1..].find("\n}\n").expect("Ende des finally");
    assert!(rueck.starts_with("$neuGeblieben = @()\ntry {\n"), "try direkt nach der Meldung");
    let versuch = &rueck[..finale];
    assert!(versuch.contains("$nochDa = BoteStoppen") && versuch.contains("$null = DienstStoppen") && versuch.contains("'.alt.exe'"), "{versuch}");
    assert!(!versuch.contains("DienstStarten") && !versuch.contains("BoteStarten"), "im try startet nichts");
    let fin = &rueck[finale..schluss];
    assert!(fin.contains("if ((DienstStatus) -eq 'StopPending') { $null = WarteAufStatus 'Stopped' 20 }\n    if ((DienstStatus) -ne 'Running') {\n        $fehler2 = DienstStarten -ohneEnde\n"), "{fin}");
    assert!(fin.contains("if ((BoteProzesse).Count -eq 0) { BoteStarten } else {"), "{fin}");
    assert!(!fin.contains("Ende 1") && !fin.contains("Ende 0") && !fin.contains("exit"), "kein Ende im finally: {fin}");
    let rest = &rueck[schluss..];
    assert!(!rest.contains("DienstStarten") && !rest.contains("BoteStarten"), "nach dem finally startet nichts");
    assert!(rest.contains("ZurueckSatz"));
}

/// Die Oberfläche des Dienstes ist eine Kopie in bin\web, die nur
/// Dienst.ps1 -Neustart zusammen mit den Programmen wechselt: bereitgelegt
/// (bin\web.neu) nach dem Bau und vor dem Anhalten des Dienstes, getauscht in
/// TauscheProgramme, auf dem Rückweg mit den Programmen zurück (nur wenn
/// dieser Lauf sie getauscht hat und dashboard.exe wieder die vorige ist).
/// Dazu läuft tests/skripte/Dienst-Oberflaeche.ps1 (die echten Funktionen an
/// einer Testwurzel) in powershell.exe 5.1.
#[cfg(windows)]
#[test]
fn dienst_ps1_spielt_die_oberflaeche_mit_den_programmen_ein() {
    let wurzel = Path::new(env!("CARGO_MANIFEST_DIR"));
    let ps1 = std::fs::read_to_string(wurzel.join("scripts").join("Dienst.ps1")).unwrap().replace("\r\n", "\n");
    let neustart = ps1.split("# -Neustart\n").nth(1).expect("Abschnitt -Neustart");
    let stelle = |s: &str| neustart.find(s).unwrap_or_else(|| panic!("{s} fehlt im Neustart"));
    let (bau, bereit, aus, tausch) = (
        stelle("if ($code -ne 0) { Ende 1 'npm run build ist gescheitert."),
        stelle("$f = OberflaecheBereitlegen (Join-Path $web 'build')\n"),
        stelle("$dienstAus = $true\n"),
        stelle("$fehler = TauscheProgramme $quelle $oberflaeche\n"),
    );
    assert!(bau < bereit && bereit < aus && aus < tausch, "Reihenfolge Bau, Kopie, Anhalten, Tausch");
    let rueck = ps1.split("Hole die vorige Fassung zurück ...\"\n").nth(1).expect("Rückweg in Dienst.ps1");
    let versuch = &rueck[..rueck.find("\n} finally {\n").expect("finally des Rückwegs")];
    let (alt, zurueck) = (versuch.find("'.alt.exe'").expect("*.alt.exe"), versuch.find("$f = OberflaecheZurueck").expect("OberflaecheZurueck im Rückweg"));
    assert!(alt < zurueck && versuch.contains("if ($script:oberflaecheGetauscht -and -not ($neuGeblieben -contains 'dashboard.exe')) {"), "{versuch}");
    let aus = dashboard::dienst::programme::eigener_befehl(r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(wurzel.join("tests").join("skripte").join("Dienst-Oberflaeche.ps1"))
        .output()
        .expect("powershell.exe startet");
    let ausgabe = format!("{}{}", String::from_utf8_lossy(&aus.stdout), String::from_utf8_lossy(&aus.stderr));
    assert_eq!(aus.status.code(), Some(0), "{ausgabe}");
    assert!(ausgabe.contains("Alle Pr") && !ausgabe.contains("FEHLER"), "{ausgabe}");
}

/// Nachbildung zu `dienst_ps1_rueckweg_startet_im_finally` mit dem echten
/// `Ende`, dem echten `DienstStarten` und dem echten `finally` des Rückwegs
/// aus Dienst.ps1 (Attrappen: ScExe, DienstStatus, BoteStarten,
/// BoteProzesse) in powershell.exe 5.1: Nach `Ende 1` (etwa Code 5 aus
/// DienstStoppen), nach einer Ausnahme und auf dem Erfolgsweg startet es
/// Dienst und Boten genau einmal, wenn sie nicht laufen, sonst gar nicht.
/// Code 5 beim Start im finally endet nicht (die Ausnahme bleibt sichtbar,
/// der Erfolgsweg geht weiter mit dem Satz). Steht der Dienst nach einem
/// Abbruch in DienstStoppen noch auf StopPending, wartet das finally den Stopp
/// ab und startet erst dann (sonst meldete sc start 1061 und er bliebe aus).
#[cfg(windows)]
#[test]
fn dienst_ps1_rueckweg_finally_startet_dienst_und_boten() {
    let ps1 = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts").join("Dienst.ps1")).unwrap().replace("\r\n", "\n");
    let funktion = |name: &str| {
        let kopf = format!("\nfunction {name}(");
        let rumpf = ps1.split(kopf.as_str()).nth(1).and_then(|r| r.split("\n}\n").next()).unwrap_or_else(|| panic!("function {name}"));
        format!("function {name}({rumpf}\n}}\n")
    };
    let rueck = ps1.split("Hole die vorige Fassung zurück ...\"\n").nth(1).expect("Rückweg in Dienst.ps1");
    let finale = rueck.find("\n} finally {\n").expect("finally des Rückwegs") + 1;
    let schluss = &rueck[finale..finale + rueck[finale..].find("\n}\n").expect("Ende des finally") + 3];
    let ordner = std::env::temp_dir().join(format!("dashboard-dienst-rueckweg-{}", std::process::id()));
    std::fs::create_dir_all(&ordner).unwrap();
    let skript = ordner.join("probe.ps1");
    let text = format!(
        "\u{feff}$ErrorActionPreference = 'Stop'\n$Dienstname = 'Probe'\n{}{}{}\
function ScExe([string]$argumente) {{ [IO.File]::AppendAllText($env:PROBE_DATEI, 'dienst;'); if ((DienstStatus) -eq 'StopPending') {{ return [pscustomobject]@{{ Code = 1061; Text = 'FEHLER 1061' }} }}; [pscustomobject]@{{ Code = [int]$env:PROBE_SC; Text = 'sc' }} }}\n\
$script:abfragen = 0\n\
function DienstStatus {{ $script:abfragen++; if ($env:PROBE_DIENST -eq 'halt') {{ if ($script:abfragen -le 2) {{ 'StopPending' }} else {{ 'Stopped' }} }} elseif ($env:PROBE_DIENST) {{ 'Running' }} else {{ 'Stopped' }} }}\n\
function BoteStarten {{ [IO.File]::AppendAllText($env:PROBE_DATEI, 'bote;') }}\n\
function BoteProzesse {{ if ($env:PROBE_BOTE) {{ @(1) }} else {{ @() }} }}\n\
try {{\n    if ($env:PROBE_ART -eq 'ende') {{ Ende 1 'Keine Berechtigung, den Dienst zu stoppen.' }}\n    if ($env:PROBE_ART -eq 'wurf') {{ throw 'unerwartet' }}\n\
{schluss}Write-Host \"weiter: $fehler2\"\nexit 0\n",
        funktion("Ende"),
        funktion("DienstStarten"),
        funktion("WarteAufStatus")
    );
    std::fs::write(&skript, text).unwrap();
    let ohne_recht = "Keine Berechtigung, den Dienst zu starten";
    // (Weg, sc-Code, Dienst läuft, Bote läuft, Exit, gestartet, in der Ausgabe)
    let faelle = [
        ("ende", "0", "", "", 1, "dienst;bote;", "zu stoppen"),
        ("ende", "5", "", "", 1, "dienst;bote;", ohne_recht),
        ("ende", "0", "1", "1", 1, "", "zu stoppen"),
        ("wurf", "0", "", "", 1, "dienst;bote;", "unerwartet"),
        ("wurf", "5", "", "", 1, "dienst;bote;", "unerwartet"),
        ("wurf", "0", "1", "1", 1, "", "unerwartet"),
        ("gut", "0", "", "", 0, "dienst;bote;", "weiter: "),
        ("gut", "5", "", "", 0, "dienst;bote;", "weiter: Keine Berechtigung, den Dienst zu starten"),
        ("gut", "0", "1", "1", 0, "", "weiter: "),
        ("gut", "0", "1", "", 0, "bote;", "weiter: "),
        ("ende", "0", "halt", "", 1, "dienst;bote;", "zu stoppen"),
        ("wurf", "0", "halt", "", 1, "dienst;bote;", "unerwartet"),
        ("gut", "0", "halt", "", 0, "dienst;bote;", "weiter: "),
    ];
    for (i, (art, sc, dienst, bote, code, erwartet, satz)) in faelle.into_iter().enumerate() {
        let datei = ordner.join(format!("fall{i}.txt"));
        let aus = dashboard::dienst::programme::eigener_befehl(r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"])
            .arg(&skript)
            .env("PROBE_ART", art)
            .env("PROBE_SC", sc)
            .env("PROBE_DIENST", dienst)
            .env("PROBE_BOTE", bote)
            .env("PROBE_DATEI", &datei)
            .output()
            .expect("powershell.exe startet");
        let ausgabe = format!("{}{}", String::from_utf8_lossy(&aus.stdout), String::from_utf8_lossy(&aus.stderr));
        assert_eq!(aus.status.code(), Some(code), "Fall {i} ({art}, sc {sc}): {ausgabe}");
        assert!(ausgabe.contains(satz), "Fall {i} ({art}, sc {sc}): {satz:?} fehlt in {ausgabe}");
        assert_eq!(ausgabe.contains(ohne_recht), sc == "5", "Fall {i}: Satz des finally nur bei Code 5: {ausgabe}");
        assert!(!ausgabe.contains("1061"), "Fall {i}: Start erst nach dem Stopp: {ausgabe}");
        assert_eq!(std::fs::read_to_string(&datei).unwrap_or_default(), erwartet, "Fall {i} ({art}, Dienst {dienst:?}, Bote {bote:?})");
    }
    let _ = std::fs::remove_dir_all(&ordner);
}

/// Antwortet nach dem Rückweg nichts, nennt der Endsatz die Ursache: einen
/// Dienst, der nach über 40 s noch in StopPending hängt, eine dashboard.alt.exe,
/// die sich nicht zurückholen ließ (dann läuft die neue Fassung), sonst die
/// vorige Fassung selbst (ein nicht zurückgeholter Bote wird nur mitgenannt). Echter Block aus Dienst.ps1 mit dem echten `Ende`
/// in powershell.exe 5.1 (Attrappe: DienstStatus).
#[cfg(windows)]
#[test]
fn dienst_ps1_rueckweg_endsatz_nennt_die_ursache() {
    let ps1 = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts").join("Dienst.ps1")).unwrap().replace("\r\n", "\n");
    let ende = ps1.split("\nfunction Ende(").nth(1).and_then(|r| r.split("\n}\n").next()).expect("function Ende");
    let rueck = ps1.split("Hole die vorige Fassung zurück ...\"\n").nth(1).expect("Rückweg in Dienst.ps1");
    let anfang = rueck.find("\nif ($fehler2) {\n").expect("Endsatz des Rückwegs") + 1;
    let block = &rueck[anfang..anfang + rueck[anfang..].find("\n}\n").expect("Ende des Blocks") + 3];
    let ordner = std::env::temp_dir().join(format!("dashboard-dienst-endsatz-{}", std::process::id()));
    std::fs::create_dir_all(&ordner).unwrap();
    let skript = ordner.join("probe.ps1");
    let text = format!(
        "\u{feff}$ErrorActionPreference = 'Stop'\nfunction Ende({ende}\n}}\nfunction DienstStatus {{ $env:PROBE_DIENST }}\n\
$neuGeblieben = @($env:PROBE_NEU -split ',' | Where-Object {{ $_ }})\n$fehler2 = 'sc start meldet: x'\n$dauer = 7\n$protokoll = 'P'\n{block}exit 0\n"
    );
    std::fs::write(&skript, text).unwrap();
    // Nur ASCII vergleichen: powershell.exe schreibt in der OEM-Codepage.
    let faelle = [
        ("StopPending", "", "40 s noch StopPending), darum startete keine Fassung (sc start meldet: x)."),
        ("StopPending", "dashboard.exe", "noch StopPending), darum startete keine Fassung"),
        ("Stopped", "dashboard.exe", "(dashboard.exe blieb die neue), und das Dashboard antwortet nicht (sc start meldet: x)."),
        ("Running", "dashboard.exe,dashboard-bote.exe", "(dashboard.exe, dashboard-bote.exe blieb die neue)"),
        // Nur der Bote blieb neu: / und /api/daten liefert dashboard.exe, also die vorige Fassung selbst.
        ("Running", "dashboard-bote.exe", "Auch die vorige Fassung antwortet nicht (nur dashboard-bote.exe blieb die neue) (sc start meldet: x)."),
        ("Running", "", "Auch die vorige Fassung antwortet nicht (sc start meldet: x). Das Dashboard ist seit 7 s weg."),
    ];
    for (dienst, neu, satz) in faelle {
        let aus = dashboard::dienst::programme::eigener_befehl(r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"])
            .arg(&skript)
            .env("PROBE_DIENST", dienst)
            .env("PROBE_NEU", neu)
            .output()
            .expect("powershell.exe startet");
        let ausgabe = format!("{}{}", String::from_utf8_lossy(&aus.stdout), String::from_utf8_lossy(&aus.stderr));
        assert_eq!(aus.status.code(), Some(1), "{dienst} {neu:?}: {ausgabe}");
        assert!(ausgabe.contains(satz), "{dienst} {neu:?}: {satz:?} fehlt in {ausgabe}");
    }
    let _ = std::fs::remove_dir_all(&ordner);
}

/// scripts/Dienst.ps1 schreibt im Notfall die leere Bauliste selbst: Zeile
/// für Zeile dieselbe wie aus Rust.
#[test]
fn dienst_ps1_leere_bauliste_wie_rust() {
    let r = Repo::neu("leer-ps1");
    let ziel = r.0.join("web").join("src").join("lib").join("projekte.generated.ts");
    scanner::bauliste_leer_schreiben(&r.0.join("projects"), &ziel).unwrap();
    let rust = std::fs::read_to_string(&ziel).unwrap();
    let ps1 = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts").join("Dienst.ps1")).unwrap();
    let block = ps1.split("$leereBauliste = @(").nth(1).and_then(|b| b.split(") -join").next()).expect("$leereBauliste in Dienst.ps1");
    let zeilen: Vec<String> =
        block.lines().map(str::trim).filter(|z| !z.is_empty()).map(|z| z.trim_end_matches(',').trim_matches(|c| c == '\'' || c == '"').to_string()).collect();
    // Das letzte '' ergibt mit -join "`n" den Zeilenumbruch am Ende.
    assert_eq!(zeilen.join("\n"), rust, "{block}");
}

#[test]
fn bauliste_laesst_rotes_projekt_aus() {
    let r = Repo::neu("bauliste");
    r.datei("projects/erstes/Seite.svelte", "<script>\n\timport { Karte } from '$katalog';\n</script>\n<Karte titel=\"A\" />\n");
    r.datei("projects/erstes/seiten/plan.svelte", "<p>Plan</p>\n");
    r.datei("projects/erstes/seiten/Falscher Name.svelte", "<p>x</p>\n");
    r.datei("projects/pruef-zwei/Seite.svelte", "<p>Seite</p>\n");
    r.datei("projects/rot/Seite.svelte", "<script>\n\tconst a = fetch('/api/schluessel');\n</script>\n");
    r.datei("projects/ohne-seite/projekt.toml", KOPF);
    r.datei("projects/_vorlage/Seite.svelte", "<p>Vorlage</p>\n");
    std::fs::create_dir_all(r.0.join("web").join("src").join("lib")).unwrap();

    let (gruen, rote) = scanner::bauliste(&r.0);
    assert_eq!(gruen, ["erstes", "pruef-zwei"]);
    assert_eq!(rote.keys().collect::<Vec<_>>(), ["rot"]);
    assert_eq!(rote["rot"].len(), 1);
    assert_eq!(rote["rot"][0].zeile, Some(2));

    let ziel = r.0.join("web").join("src").join("lib").join("projekte.generated.ts");
    let ((gruen2, _), neu) = scanner::bauliste_schreiben(&r.0, &ziel).unwrap();
    assert!(neu);
    assert_eq!(gruen2, gruen);
    let text = std::fs::read_to_string(&ziel).unwrap();
    // Etappe 4b: absolute Pfade (jede Inhaltsquelle geht), nur explizites import().
    let p = scanner::import_pfad(&r.0.join("projects"));
    assert!(p.contains(":/") && !p.contains('\\'), "{p}");
    assert!(
        text.starts_with(
            "// Erzeugt von dashboard pruefe --bauliste. Nicht von Hand ändern.\n\
         \n\
         type Laden = () => Promise<{ default: import('svelte').Component }>;\n\
         \n\
         export const projektSeiten: Record<string, Laden> = {\n"
        ),
        "{text}"
    );
    let seiten = text.split("export const projektUnterseiten").next().unwrap();
    let a = seiten.find(&format!("import('{p}/erstes/Seite.svelte')")).expect(&text);
    let b = seiten.find(&format!("import('{p}/pruef-zwei/Seite.svelte')")).expect(&text);
    assert!(a < b && seiten.contains("\terstes: () =>") && seiten.contains("\t'pruef-zwei': () =>"), "{text}");
    let unter = text.split("export const projektUnterseiten").nth(1).unwrap();
    assert!(unter.contains(&format!("import('{p}/erstes/seiten/plan.svelte')")) && !unter.contains("Falscher"), "{text}");
    assert!(!text.contains("../") && !text.contains("import.meta"), "{text}");
    assert!(text.lines().all(|z| z.chars().count() + z.matches('\t').count() <= 100 || z.trim_start().starts_with("import(")), "{text}"); // Gleicher Inhalt: nicht neu geschrieben.
    let geaendert = std::fs::metadata(&ziel).unwrap().modified().unwrap();
    let (_, neu) = scanner::bauliste_schreiben(&r.0, &ziel).unwrap();
    assert!(!neu);
    assert_eq!(std::fs::metadata(&ziel).unwrap().modified().unwrap(), geaendert);

    // Über den Befehl: rote Projekte mit Befund, Rückgabe 0.
    let (aus, code) = pruefe::lauf(&r.0.join("projects"), &["--bauliste".to_string()]);
    assert_eq!(code, 0, "{aus}");
    assert_eq!(
        aus,
        "ROT rot: 1 Fehler, wird nicht gebaut.\n\
         FEHLER projects/rot/Seite.svelte, Zeile 2: `fetch` ist in Projektseiten verboten. Daten kommen nur über `ladeDaten` aus '$projekt'.\n\
         GRÜN erstes, pruef-zwei\n\
         web/src/lib/projekte.generated.ts unverändert (2 grün, 1 rot).\n"
    );

    // Wird das rote Projekt grün, kommt es dazu.
    r.datei("projects/rot/Seite.svelte", "<p>Jetzt gut</p>\n");
    let ((gruen, rote), neu) = scanner::bauliste_schreiben(&r.0, &ziel).unwrap();
    assert!(neu && rote.is_empty());
    assert_eq!(gruen, ["erstes", "pruef-zwei", "rot"]);
    assert!(std::fs::read_to_string(&ziel).unwrap().contains(&format!("import('{p}/rot/Seite.svelte')\n}};")));
}

#[test]
fn bauliste_leer_und_lange_namen() {
    let r = Repo::neu("bauliste-leer");
    let ziel = r.0.join("web").join("src").join("lib").join("projekte.generated.ts");
    let ((gruen, rote), _) = scanner::bauliste_schreiben(&r.0, &ziel).unwrap();
    assert!(gruen.is_empty() && rote.is_empty());
    assert!(
        std::fs::read_to_string(&ziel)
            .unwrap()
            .ends_with("export const projektSeiten: Record<string, Laden> = {};\n\nexport const projektUnterseiten: Record<string, Laden> = {};\n")
    );
    // Über 100 Zeichen bricht prettier nach `=>` um; die Datei auch.
    let lang = "ein-sehr-langer-projektname-mit-vielen-teilen";
    r.datei(&format!("projects/{lang}/Seite.svelte"), "<p>x</p>\n");
    scanner::bauliste_schreiben(&r.0, &ziel).unwrap();
    let text = std::fs::read_to_string(&ziel).unwrap();
    assert!(text.contains(&format!("\t'{lang}': () =>\n\t\timport('{}/{lang}/Seite.svelte')\n}};", scanner::import_pfad(&r.0.join("projects")))), "{text}");
}

// ----- svelte-check -----

#[test]
fn svelte_check_ausgabe_lesen() {
    let ausgabe = std::fs::read_to_string(beispiel().join("svelte-check-machine.txt")).unwrap();
    let repo = Path::new(r"C:\Repo");
    let b = scanner::machine_lesen(&ausgabe, repo, Some("erstes"));
    let s = saetze(&b);
    assert_eq!(
        s,
        [
            "FEHLER projects/erstes/Seite.svelte, Zeile 12: svelte-check: Type 'string' is not assignable to type 'number'.",
            "HINWEIS projects/erstes/seiten/plan.svelte, Zeile 3: svelte-check: Unused CSS selector \"p\"",
            "FEHLER projects/erstes/typen.ts, Zeile 4: svelte-check: Absoluter Pfad",
        ],
        "{s:#?}"
    );
    // Ohne Projekt: alles unter projects/, nichts aus web/src.
    let b = scanner::machine_lesen(&ausgabe, repo, None);
    assert_eq!(b.len(), 4);
    assert!(b.iter().all(|b| b.datei.starts_with("projects/")));
    assert!(scanner::machine_lesen("kein machine-Format\n1 ERROR kaputt", repo, None).is_empty());
}

/// Der echte Lauf über Node (ohne Fenster, mit Zeitlimit). Fehlen Node oder
/// web/node_modules, kommt ein Hinweis, und der Test ist damit fertig.
#[test]
fn svelte_check_echt() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    let start = Instant::now();
    let b = scanner::svelte_check_mit(repo, Some("gibt-es-nicht"), Duration::from_secs(120));
    eprintln!("svelte-check: {:?}, {:#?}", start.elapsed(), saetze(&b));
    let node = dashboard::dienst::programme::pfad_von(dashboard::dienst::programme::Werkzeug::Node).is_some();
    let module = repo.join("web").join("node_modules").join("svelte-check").is_dir();
    if !node || !module {
        assert_eq!(b.len(), 1, "{:#?}", saetze(&b));
        assert!(!b[0].ist_fehler() && b[0].text.contains("übersprungen"), "{:#?}", saetze(&b));
        eprintln!("svelte_check_echt übersprungen: Node oder web/node_modules fehlt");
        return;
    }
    assert!(b.is_empty(), "{:#?}", saetze(&b));
}

#[test]
fn svelte_check_ohne_node_modules_ist_hinweis() {
    let r = Repo::neu("ohne-module");
    let b = scanner::svelte_check(&r.0, Some("p"));
    assert_eq!(
        saetze(&b),
        ["HINWEIS projects/p: svelte-check ist nicht da (web/node_modules fehlt). Die Svelte-Prüfung ist übersprungen; der Scanner hat geprüft."]
    );
}

// ----- Schnell -----

#[test]
fn scanner_ist_schnell() {
    // Zwanzig Projekte mit je einer Seite von etwa 500 Zeilen.
    let gut = std::fs::read_to_string(beispiel().join("projects").join("gut").join("Seite.svelte")).unwrap();
    let rot = std::fs::read_to_string(beispiel().join("projects").join("daten-rot").join("Seite.svelte")).unwrap();
    let markup: String = (0..20).map(|_| format!("{}\n", rot.split("<style>").next().unwrap().split("</script>").nth(1).unwrap())).collect();
    let seite = format!("{gut}\n{markup}");
    let start = Instant::now();
    let mut n = 0;
    for i in 0..20 {
        n += scanner::pruefe_datei(&format!("projects/p{i}/Seite.svelte"), &seite).len();
    }
    let zeit = start.elapsed();
    eprintln!("20 Seiten mit je {} Zeilen: {zeit:?}, {n} Befunde", seite.lines().count());
    // Gemessen sind es wenige Millisekunden. Die Grenze fängt nur krasse
    // Ausreißer (etwa eine quadratische Schleife), nicht die Last paralleler
    // Tests oder eines Baus daneben.
    let grenze = if cfg!(debug_assertions) { 5000 } else { 1000 };
    assert!(zeit < Duration::from_millis(grenze), "{zeit:?}");
    // Und das ganze Repo (alle Projekte, ohne svelte-check).
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("projects");
    let start = Instant::now();
    let alle: usize = std::fs::read_dir(&dir)
        .map(|e| e.filter_map(|e| e.ok()).filter_map(|e| e.file_name().into_string().ok()).map(|s| scanner::pruefe_projekt(&dir, &s).len()).sum())
        .unwrap_or(0);
    eprintln!("alle Projekte im Repo: {:?}, {alle} Befunde", start.elapsed());
    // Hier ist es fast nur Dateizugriff (kalter Cache, parallele Tests): mehr Luft.
    let zeit = start.elapsed();
    assert!(zeit < Duration::from_millis(grenze * 20), "{zeit:?}");
}

// ----- Leere Bauliste aus web/scripts/projekte-vorbereiten.mjs -----

/// Das Node-Skript schreibt genau die Form, die Rust ohne Projekte schreibt,
/// und lässt eine vorhandene Datei in Ruhe.
#[test]
fn leere_bauliste_wie_rust() {
    use dashboard::dienst::programme::{self, Werkzeug};
    let Ok(mut node) = programme::befehl(Werkzeug::Node) else {
        dashboard::ueberspringen("node fehlt an den festen Orten.");
        return;
    };
    let r = Repo::neu("leere-bauliste");
    std::fs::create_dir_all(r.0.join("projects")).unwrap();
    let von_rust = r.0.join("rust.ts");
    scanner::bauliste_schreiben(&r.0, &von_rust).unwrap();
    let von_node = r.0.join("lib").join("node.ts");
    let skript = Path::new(env!("CARGO_MANIFEST_DIR")).join("web").join("scripts").join("projekte-vorbereiten.mjs");
    let out = node.arg(&skript).arg(&von_node).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(std::fs::read_to_string(&von_node).unwrap(), std::fs::read_to_string(&von_rust).unwrap());
    // Vorhanden: bleibt.
    std::fs::write(&von_node, "// schon da\n").unwrap();
    let out = programme::befehl(Werkzeug::Node).unwrap().arg(&skript).arg(&von_node).output().unwrap();
    assert!(out.status.success());
    assert_eq!(std::fs::read_to_string(&von_node).unwrap(), "// schon da\n");
}

/// Etappe 3b-2 (Auflage aus der Abnahme 3b): Projektseiten schalten keine
/// Prüfung ab. `@ts-nocheck`, `@ts-ignore`, `@ts-expect-error`,
/// `svelte-ignore`, `eslint-disable…` und ESLint-Regeln im Kommentar sind
/// Fehler, in Svelte und TS; Beispiele in Zeichenketten und normale Kommentare
/// nicht.
#[test]
fn abschalten_in_projekten_ist_verboten() {
    for (text, zeile, stueck) in [
        ("<script lang=\"ts\">\n\t// @ts-nocheck\n</script>\n", 2, "`@ts-nocheck` ist in Projektseiten verboten"),
        ("<script lang=\"ts\">\n\t// @ts-ignore weil\n</script>\n", 2, "`@ts-ignore`"),
        ("<script lang=\"ts\">\n\t// @ts-expect-error -- Grund\n</script>\n", 2, "`@ts-expect-error`"),
        ("<p>x</p>\n<!-- svelte-ignore a11y_click_events_have_key_events (Grund) -->\n<p>y</p>\n", 2, "`svelte-ignore`"),
        ("<script lang=\"ts\">\n\t/* eslint-disable no-console */\n</script>\n", 2, "`eslint-disable`"),
        ("<script lang=\"ts\">\n\t// eslint-disable-next-line no-console -- Grund\n</script>\n", 2, "`eslint-disable`"),
        ("<script lang=\"ts\">\n\t/* eslint no-console: off */\n</script>\n", 2, "`eslint no-console: …`"),
        ("<script lang=\"ts\">\n\t/**\n\t * @ts-nocheck\n\t */\n</script>\n", 3, "`@ts-nocheck`"),
    ] {
        let b = svelte(text);
        ein_fehler(&b, zeile, stueck);
        assert!(b.iter().any(|x| x.text.contains("Eine Projektseite schaltet keine Prüfung ab.")), "{:#?}", saetze(&b));
    }
    // In TS-Dateien genauso.
    let b = scanner::pruefe_datei("projects/t/hilfe.ts", "// @ts-nocheck\nexport const a = 0;\n");
    assert_eq!(b.iter().filter(|x| x.ist_fehler()).count(), 1, "{:#?}", saetze(&b));
    // Nicht: Beispiele in Zeichenketten, normale Kommentare, Adressen.
    for text in [
        "<script lang=\"ts\">\n\tconst t = '// @ts-nocheck';\n</script>\n<p>{t}</p>\n",
        "<script lang=\"ts\">\n\t// Hier steht nichts Besonderes, auch kein eslint-Wort am Anfang.\n</script>\n",
        "<a href=\"https://example.org/svelte-ignore\">x</a>\n",
    ] {
        let b = svelte(text);
        assert!(!b.iter().any(|x| x.text.contains("schaltet keine Prüfung ab")), "{text}: {:#?}", saetze(&b));
    }
}

/// Zweig B: Projekte erklären keine Typen über sich hinaus (`declare
/// global`, `declare module`, `/// <reference …>`, `.d.ts`), auch nicht im
/// `<script>` von `.svelte`; sonst verdeckt ein Projekt im gemeinsamen
/// svelte-check Fehler anderer. Lokale Typen und Beispiele im Text nicht.
#[test]
fn globale_typen_sind_verboten() {
    for (text, zeile) in [
        ("<script lang=\"ts\">\n\tdeclare global {\n\t\tinterface Window {\n\t\t\tprobe: string;\n\t\t}\n\t}\n</script>\n", 2),
        ("<script lang=\"ts\">\n\tdeclare module '$katalog' {\n\t\texport const probe: string;\n\t}\n</script>\n", 2),
        ("<script lang=\"ts\">\n\t/// <reference lib=\"webworker\" />\n\tconst a = 'x';\n</script>\n<p>{a}</p>\n", 2),
        ("<script module lang=\"ts\">\n\t///<reference types=\"node\" />\n</script>\n", 2),
    ] {
        ein_fehler(&svelte(text), zeile, "`declare global`, `declare module`, `/// <reference …>` und `.d.ts`-Dateien sind in Projektseiten verboten");
    }
    for text in [
        "export {};\n\ndeclare global {\n\tinterface Window {\n\t\tprobe: string;\n\t}\n}\n",
        "/// <reference types=\"vite/client\" />\nexport const a = 'x';\n",
    ] {
        let b = scanner::pruefe_datei("projects/t/typen.ts", text);
        assert_eq!(b.iter().filter(|x| x.ist_fehler() && x.text.contains("declare global")).count(), 1, "{text}: {:#?}", saetze(&b));
    }
    // Nicht: lokale Typen, ein Name `declare`, Beispiele in Text und Kommentar.
    for text in [
        "<script lang=\"ts\">\n\ttype Probe = { a: string };\n\tinterface Teil {\n\t\tb: string;\n\t}\n\tconst p: Probe = { a: 'x' };\n\tconst t: Teil = { b: 'y' };\n</script>\n<p>{p.a}{t.b}</p>\n",
        "<script lang=\"ts\">\n\tconst declare = 'x';\n\tconst w = '/// <reference lib=\"dom\" />';\n\t// declare global gehört nicht hierher\n</script>\n<p>{declare}{w}</p>\n",
    ] {
        let b = svelte(text);
        assert!(!b.iter().any(|x| x.text.contains("declare global")), "{text}: {:#?}", saetze(&b));
    }
    // Eine .d.ts im Projektordner ist ein Fehler bei der Datei.
    let r = Repo::neu("d-ts");
    r.datei("projects/p/Seite.svelte", "<p>x</p>\n");
    r.datei("projects/p/teile/typen.d.ts", "export type A = string;\n");
    // Auch `*.d.<x>.ts`: ohne import/export gilt so eine Datei als globales Skript.
    r.datei("projects/p/teile/typen.d.x.ts", "interface Window {\n\tprobe: number;\n}\n");
    let s = saetze(&scanner::pruefe_projekt(&r.0.join("projects"), "p"));
    for d in ["typen.d.ts", "typen.d.x.ts"] {
        assert!(s.iter().any(|x| x.starts_with(&format!("FEHLER projects/p/teile/{d}: `declare global`"))), "{d}: {s:#?}");
    }
}

/// Abnahme 4b: Der Pfad steht roh in `import('…')`. Ein `'` (oder ein
/// anderes Zeichen, das dort Code einschleusen könnte) und Netzlaufwerke
/// werden mit einem Satz abgelehnt; die Bauliste lässt solche Ordner weg.
#[test]
fn bauliste_lehnt_gefaehrliche_pfade_ab() {
    let r = Repo::neu("bauliste-pfade");
    let gut = r.0.join("projects").join("gut");
    let boese = r.0.join("quelle-mit-'-strich").join("boese");
    for o in [&gut, &boese] {
        std::fs::create_dir_all(o).unwrap();
        std::fs::write(o.join("Seite.svelte"), "<p>x</p>\n").unwrap();
    }
    let grund = scanner::import_pfad_pruefen(&boese).unwrap_err();
    assert!(grund.contains("Zeichen „'“") && !grund.contains("bauliste-pfade"), "Satz ohne Pfad: {grund}");
    assert!(scanner::import_pfad_pruefen(&gut).is_ok());
    for schlecht in ["a`b", "a$b", "a{b", "a\"b", "a\nb"] {
        assert!(scanner::import_pfad_pruefen(&r.0.join(schlecht)).is_err(), "{schlecht}");
    }
    assert!(scanner::import_pfad_pruefen(Path::new(r"\\server\freigabe\projekt")).unwrap_err().contains("Netzlaufwerk"));
    assert!(scanner::import_pfad_pruefen(Path::new(r"\\?\UNC\server\freigabe\projekt")).unwrap_err().contains("Netzlaufwerk"));

    let text = scanner::bauliste_text(&[("gut".to_string(), gut.clone()), ("boese".to_string(), boese.clone())]);
    assert!(text.contains("gut: () =>"), "{text}");
    assert!(!text.contains("boese") && !text.contains("-strich"), "der gefährliche Ordner fehlt: {text}");
}
