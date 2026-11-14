#Requires -Version 7.7
<#
.SYNOPSIS
    Pre-Push-Prüfer des Code-Repos: Was hinausgeht, darf nichts Privates enthalten.
.DESCRIPTION
    Prüft vor jedem Push (Hook scripts\hooks\pre-push, core.hooksPath) und in
    scripts\Hochladen.ps1, dem einzigen Weg nach draußen. Grundlage:
    die Trennung von Code und Inhalt: Was privat ist, liegt nur im
    Inhalts-Repo; hinaus geht nur der Kern.

    Abgewiesen wird:
      Refs       auf dem Remote alles außer refs/heads/main und Tags
                 refs/tags/v0.MINOR.PATCH (Ziffern ohne führende Null), lokal
                 alles außer refs/heads/main, Löschen; auf main nur
                 Merge-Commits (kein Fast-Forward), außer dem Wurzel-Commit;
                 ein Commit ohne Eltern außer der einen Wurzel beim ersten Push.
      Tags       (Teil 3) lokal ein anderer Name als fern; ein Tag-Objekt
                 (git tag -a/-s: Tagger, Text und Signatur gingen ungeprüft
                 hinaus), nur leichte Tags; ein Tag, den es fern schon gibt
                 (kein Verschieben), Löschen; ein Commit, das nicht auf der
                 First-Parent-Linie von main liegt: main aus demselben Push,
                 sonst fern (Zeile nach refs/heads/main, ohne sie git
                 ls-remote <RemoteUrl>; nie die lokale Remote-Tracking-Ref);
                 ohne main ein Fund. Im Bundle
                 die Linie seiner Ref main. (Teil 4) Ein Tag
                 v0.MINOR.PATCH, wenn im getaggten Commit Cargo.toml unter
                 [package] nicht genau eine Zeile version = "0.MINOR.PATCH"
                 hat oder in CHANGELOG.txt die oberste Überschrift nach
                 Unreleased nicht "0.MINOR.PATCH - JJJJ-MM-TT" ist (gültiges
                 Datum, Strichzeile darunter).
      Commits    auf der Linie (First-Parent-Kette des Bereichs: die Merges
                 des Betreuers; bei -Pruefen und im Bundle die Kette der
                 Ref) Autor oder Committer nicht genau Ranke <ranke2@proton.me>
                 (Zeichen für Zeichen: ein breites ＠ oder ein anderes
                 Nicht-ASCII-Zeichen ist ein Fund). Außerhalb der Linie (über
                 zweite Eltern: Seitenzweige, Beiträge aus Forks) auch eine
                 Beitragsadresse, verankert, nur ASCII, Groß/Klein egal:
                 GitHub-noreply (<zahl>+<login>@users.noreply.github.com oder
                 <login>@users.noreply.github.com, Login
                 nach GitHub-Regeln) oder eine Adresse auf .invalid oder
                 .example; der Name und die Adresse (ohne die feste
                 noreply-Endung) laufen durch die Textprüfung (Listen,
                 KI-Namen, Steuerzeichen). Ein Commit mit fremdem Autor oder
                 Committer ändert keinen Prüfpfad (scripts/**, .cargo/**,
                 .github/**, tests/vor_push.rs, KONTAKT-PGP.asc,
                 jede .gitattributes und .npmrc, jeder Pfad mit einem
                 8.3-Kurznamen wie GITATT~1; Groß/Klein egal; in jeder
                 Art, auch im Projekt-Repo; gezählt wird, was er gegen jeden
                 Elternteil ändert), und ein Merge der Linie, der fremde
                 Commits hereinholt, ändert gegenüber seinem ersten
                 Elternteil keinen Prüfpfad: Änderungen am Prüfer übernimmt der
                 Betreuer selbst, in einem eigenen Merge ohne fremde Commits.
                 Grenze: Die Regel hängt an der Angabe von Autor und
                 Committer, die niemanden nachweist; ein Fork-Commit, der die
                 feste Angabe fälscht, gilt als eigener; dagegen steht
                 -Beitrag im Probelauf vor dem Merge (Anleitung Abschnitt
                 15). Mit -AufMain und einem Bereich
                 a..b setzt die Linie a fort (erster Elternteil des ältesten
                 neuen Linien-Commits ist a). Für alle Commits: eine erste Zeile nicht
                 nach Conventional Commits (wie bei Nachricht, auch für
                 Merges); Nachrichten mit
                 einem der Text-Muster unten oder mit Anhängen (Trailer) und
                 KI-Namen (Co-Authored-By, Signed-off-by, Generated with,
                 ein Fuß BREAKING CHANGE: (Brüche nur mit ! in Zeile 1),
                 Namen von KI-Assistenten, Modellen und ihren Anbietern,
                 Groß/Klein egal, auch mit Strichen oder Leerzeichen dazwischen) und
                 jeder Trailer, den git erkennt; Kopfzeilen außer tree,
                 parent, author, committer (etwa eine Signatur) und
                 Fortsetzungszeilen im Kopf; eine Ref,
                 die auf kein Commit zeigt (etwa ein Tag-Objekt).
                 git replace und Grafts gelten nicht (geprüft wird, was ein
                 Push überträgt).
      Nachricht  mit -Nachricht <datei> (Hook scripts\hooks\commit-msg) nur
                 die Nachricht, ohne Namenslisten und Remote: erste Zeile nicht
                 nach Conventional Commits (typ(bereich)!: text, auch bei
                 Merges, etwa chore(merge): <zweig> nach main), Anhänge und
                 KI-Namen wie oben. Kommentarzeilen (#) und alles ab der
                 Schere von git commit -v schreibt er aus der Datei heraus
                 (UTF-8 ohne BOM, LF): Bei -m und -F entfernt git sie nicht,
                 geprüft wird so genau, was git speichert. Steht eine
                 #-Zeile vor oder zwischen dem Text, meldet er das (HINWEIS).
      Bundle     mit -Bundle <datei>: jede Ref des Bundles wie ein erster Push
                 nach main (Name, alle Commits, Baum, ganze Geschichte), Tags
                 nach der Regel oben, dazu
                 jedes Objekt, das keine Ref erreicht, als Fund; ein Bundle
                 mit Voraussetzungen (nicht ab der Wurzel) bricht ab (Exit 2).
      Pfade      außerhalb der Positivliste der Ordner und Wurzeldateien (von
                 .github nur die Issue-Formulare .github/ISSUE_TEMPLATE/*.yml
                 ohne config.yml); projects\<slug> (außer
                 _vorlage), data\, docs\entscheidungen, docs\architektur-stand.md,
                 projekte.generated.ts, web\src\lib\vertrag\projekte\; jede
                 *.md (seit Etappe 6d) außer CONTRIBUTING.md in der Wurzel; ein privater
                 Schlüssel (BEGIN … PRIVATE KEY, PuTTY) in jedem Text; Geheimnis-Dateinamen (feste Liste, Code und Beispiele
                 ausgenommen); Namen der Tabu-Ordner; Chat-Exporte
                 (conversations.json, chat-export*, *-chat-*.md); lokale
                 Dateien (*.local.*, *.local); sensible Namen;
                 projects/<slug> eines allgemeinen Projektnamens.
      Binär      Binärdatei ist nur png, jpg, gif, ico, webp und pdf, jede
                 mit passender Signatur; alles andere (zip, woff, bin …) ist Text.
                 Jede Binärdatei außerhalb der Positivliste (web\static,
                 Assets der Test-Beispiele); in der Liste jeder Name der
                 Listen im Inhalt (Byte für Byte als Latin-1) und Metadaten: PNG
                 jedes Stück außer IHDR, PLTE, IDAT, IEND, tRNS, gAMA, cHRM,
                 sRGB, pHYs; JPEG Exif und COM; GIF Kommentar, Klartext,
                 Anwendung außer NETSCAPE2.0; PDF Author/Creator, WebP
                 EXIF/XMP, ICO mit PNG-Metadaten, XMP in jeder Datei, SVG
                 metadata/inkscape/sodipodi; kaputte Struktur von PNG, ICO
                 und WebP (abgeschnittene Stücke, kein sauberes Ende an IEND,
                 RIFF-Länge passt nicht zum Ende).
      Verlauf    Pfad, Verknüpfung, Gitlink, Binär und Text gelten für jedes
                 Paar (Pfad, Blob) mit echtem Modus, das ein neuer Commit des
                 Bereichs hinzufügt oder ändert (git log --raw -m --root
                 --no-renames alt..neu), nicht nur für die Spitze; beim ersten
                 Push mit neuer Wurzel (alt = 0000…) und bei -Commits ohne
                 „..“ für die ganze erreichbare Geschichte.
      Kodierung  jede Git-Ausgabe (Pfade, Baum-Einträge,
                 Commit-Meldungen, Autor und Committer, Tag-Texte) und jede
                 Textdatei, die kein gültiges UTF-8 ist (UTF-8 mit BOM geht;
                 Windows-1252, UTF-16 und UTF-32 nicht), und jede Textdatei
                 oder Meldung mit Steuerzeichen außer Tab, LF und CR (etwa
                 NUL aus UTF-16 ohne BOM): ein Fund „kein UTF-8“ mit Datei
                 oder Stelle, nie dem Inhalt. Es gibt keine zweite Lesart.
      Text       Benutzerpfade in allen
                 Schreibformen (\, \\, /, /c/Users, /mnt/c/Users (WSL),
                 /cygdrive/c/Users, \\rechner\c$\Users, /Users/<name> (macOS),
                 /home/<name> (Linux, auch /var/home und /export/home und
                 mit mehreren Trennern; auch hinter : wie in PATH-Listen und
                 scp-Zielen, hinter Schaltern wie -L oder -isystem und als
                 JSON \/home\/<name>; nicht hinter Buchstabe oder Ziffer,
                 also nicht in URLs mit Host davor),
                 \\wsl.localhost\<distro>\home\<name> und \\wsl$\…, file:///, %5C, %2F,
                 auch doppelt kodiert und c%3A, 8.3-Kurznamen); der
                 Benutzername auch ohne Pfad (Liste benutzer); E-Mail-Adressen außer ranke2@proton.me
                 und erfundenen (.invalid, .example,
                 .test, example.org/.com/.net); SID eines Kontos; MAC-Adressen;
                 Namen von KI-Assistenten, Modellen und ihren Anbietern (fest im
                 Prüfer, gefaltet wie die Listen, auch mit Buchstaben dahinter;
                 auch in Pfaden und im Inhalt von Binärdateien; nicht im
                 Inhalt von Cargo.lock in der Wurzel; nur im Basis-Repo);
                 sensible Namen; Tabu-Namen; Rechnernamen, WLAN-Namen,
                 Schlüssel-Namen und MAC-Adressen aus der Liste;
                 projects/<allgemeiner slug> (außer in tests\beispiel*);
                 Projektmodule (Cargo.toml mit Pfad in projects\, Kisten
                 projekt-<allgemeiner slug> in Rust); Chat-Exporte (JSON mit
                 Rollen, Text mit mehreren Rollenwechseln); nur im Basis-Repo
                 die Namen der Liste nur_basis.

    Art des Repos: Basis (das Code-Repo), Projekt (ein Projekt-Repo als
    Overlay mit projects\<slug>) oder Inhalt (dashboard-inhalt). Nur im
    Basis-Repo gelten die Liste nur_basis und die Ortsregeln des Code-Repos
    (Positivliste, Orte des Inhalts, projects/<allgemeiner slug>,
    Projektmodule, Binärdateien außerhalb der Positivliste). Im Projekt-Repo
    gilt eine eigene Positivliste: projects/<slug>/** für genau einen Slug (der
    auf main des Remotes, sonst der erste, den der Lauf sieht; jeder weitere
    ist ein Fund „fremdes Projekt“)
    und die Wurzeldateien dashboard-version.toml, .gitignore, .gitattributes,
    CONTRIBUTING.md; jede andere *.md ist auch dort ein Fund. Dazu liest er dort
    <Inhalt>\pruefer\ausnahmen\<name>.toml (<name> der letzte Teil der
    Adresse ohne .git), Form erlaubt = ["…", …]: Jeder Eintrag, der gefaltet
    genau einem Wert aus sensibel (mit tokens) oder nur_basis gleicht, gilt
    für diesen Lauf nur noch alleinstehend als frei: Steht ein Buchstabe oder
    eine Ziffer direkt davor oder dahinter (eine Zusammensetzung), bleibt er
    ein Fund; ein Eintrag ohne solchen Wert ist ein Fund
    („Ausnahme ohne Listeneintrag, Nr. n“). Fehlt die Datei, gibt es keine
    Ausnahme; ist sie kaputt, Exit 2. Die Ausgabe nennt nur die Zahl. In Basis
    und Inhalt liest er sie nie. Alles andere gilt in jedem Repo. Die Art kommt
    nur von außerhalb des geprüften Baums: aus der Adresse des Remotes
    (-RemoteUrl, vom Hook) und der Liste <Inhalt>\pruefer\repos.toml
    (Abschnitt [repos], je Zeile "adresse" = "Basis"|"Projekt"|"Inhalt").
    Verglichen wird genau, nur Schema und Rechnername ohne Groß/Klein und ohne
    / und .git am Ende. Was im Baum steht, ändert die Art nie (sonst schaltete
    ein Verstoß wie projects\<slug> die Prüfung selbst ab). Geschlossen
    scheitern: ohne Adresse (Lauf von Hand, Hochladen.ps1), mit einer Adresse, die nicht in der Liste steht, oder
    ohne repos.toml (dann mit HINWEIS) wird als Basis-Repo geprüft; eine
    kaputte repos.toml bricht ab (Exit 2).

    Die Namenslisten liegen nur im Inhalts-Repo: <Inhalt>\pruefer\namen.toml
    mit sensibel, allgemein, tabu, benutzer, rechner, wlan, schluessel, mac,
    nur_basis (je eine Liste in eckigen Klammern), vereint mit jeder Datei
    <Inhalt>\pruefer\tokens\*.toml (je Quelle eine, nur sensibel = [...];
    LIESMICH.txt zählt nicht). Fehlt eine Datei oder der Ordner tokens, ist
    eine Datei kaputt, eine Datei in tokens leer oder ohne sensibel, oder
    sind sensibel (vereint), allgemein, tabu oder benutzer leer (im
    Basis-Repo auch nur_basis), bricht der Prüfer ab und nennt die Datei,
    nie ihren Inhalt (geschlossen scheitern). Ebenso bei jedem eigenen Fehler. Die Werte der
    Listen und der Text werden vor dem Vergleich gleich gefaltet: NFC
    (ein zerlegtes ü wie ein ganzes), Groß/Klein egal, ä/ö/ü/ß wie
    ae/oe/ue/ss, jeder Strich (auch U+2010, U+2013), Unterstrich, Punkt,
    Plus, /, \, · und Leerzeichen gleich (auch keins), auch über einen
    Zeilenumbruch (am Strich getrennt oder nicht). Mehr Falten gibt es nicht
    (keine Escapes, keine Doppelgänger, keine unsichtbaren Zeichen). Lange
    Zeilen werden ganz geprüft (in überlappenden Stücken). Eine Liste, die
    kein gültiges UTF-8 ist (oder U+FFFD enthält), eine Liste zweimal nennt
    oder einen TOML-Escape außer \\ enthält, ist kaputt (Exit 2).

    Cache (nur mit -MitCache, das Hochladen.ps1 für seine Vorprüfung setzt;
    ohne den Schalter und immer im Hook kalt): Ein Blob, dessen
    Inhaltsprüfung (Text, Kodierung, Binär, Metadaten, SVG) an einem Pfad
    nichts fand, wird am Ende eines Laufs ohne jeden Fund gemerkt, als Zeile
    „<sha><Tab><pfad>“ in <ort>\<regel-schlüssel>\sauber.txt (ort:
    DASHBOARD_PRUEFER_CACHE, sonst %LOCALAPPDATA%\Dashboard\cache\pruefer). Der
    Regel-Schlüssel ist SHA-256 über die Art, $GRENZE, die PowerShell-Fassung
    (Haupt und Neben), den Inhalt der Dateien des Prüfers (dieses Skript,
    Laufzeit.ps1) und der gelesenen Listen (namen.toml, tokens\*.toml,
    repos.toml, die benutzte Ausnahmedatei). Ein Treffer überspringt nur die
    Inhaltsprüfung genau dieses Blobs an genau diesem Pfad; Pfadregeln,
    Commits, Refs und Tags laufen immer. Eine Verknüpfung (Junction, Symlink)
    im Pfad des Caches schaltet ihn ab; ein kaputter Eintrag ist kein Treffer.
    Behalten werden die zwei zuletzt genutzten Regel-Ordner.

    Ausgabe: je Fund eine Zeile „ABGEWIESEN <Datei>, Zeile <n>: <Muster>“,
    ohne den gefundenen Wert (FEHLER- und HINWEIS-Zeilen nennen das
    Benutzerprofil nur als %USERPROFILE%); jede Zeile je Muster (bis 50 je Datei, dann die
    Zahl der weiteren). Exit 0: nichts gefunden; 1: abgewiesen;
    2: der Prüfer konnte nicht prüfen (auch dann geht nichts hinaus).
.PARAMETER Repo
    Das Repo (Standard: der Ordner über scripts).
.PARAMETER Inhalt
    Das Inhalts-Repo mit pruefer\namen.toml (Standard: DASHBOARD_INHALT, sonst
    der Nachbarordner dashboard-inhalt des Code-Repos, in dem dieser Prüfer
    liegt; nie der Nachbar des geprüften Repos, etwa eines Overlay-Klons).
.PARAMETER Hook
    Als pre-push-Hook: liest die Ref-Zeilen von der Standardeingabe.
.PARAMETER Baum
    Prüft den ganzen Baum dieses Stands (etwa refs/heads/main).
.PARAMETER Commits
    Prüft Autor, Committer und Nachricht dieser Commits (Bereich wie a..b oder ein Stand).
.PARAMETER AufMain
    Mit -Commits: die Regeln „nur Merge-Commits auf main“ und „eine Wurzel“ für diesen Bereich.
.PARAMETER Beitrag
    Die gelesene Kennung eines Beitrags aus einem Fork (Probelauf vor dem
    Merge): Jeder Commit in refs/heads/main..<kennung>, dessen Autor oder
    Committer den Namen oder die Adresse der Betreuer trägt (Groß/Klein egal),
    ist ein Fund. Echte Commits der Betreuer liegen dort nie.
.PARAMETER Pruefen
    Probelauf ohne Push: prüft die lokale Ref -Ref so, als ginge sie mit
    einem ersten Push nach refs/heads/main (Name, alle erreichbaren Commits,
    Baum, ganze Geschichte).
.PARAMETER Ref
    Mit -Pruefen: die lokale Ref (Standard refs/heads/main).
.PARAMETER Bundle
    Prüft ein Git-Bundle ganz (alle Refs, alle Objekte), in einem
    Wegwerf-Repo im Temp-Ordner.
.PARAMETER Nachricht
    Prüft nur diese Datei mit einer Commit-Nachricht (Hook commit-msg); ohne
    Namenslisten, ohne Remote. Exit 0 gut, 1 abgewiesen, 2 eigener Fehler.
.PARAMETER Art
    Nur eine Zusicherung: Basis, Projekt oder Inhalt. Weicht die Art aus
    -RemoteUrl und repos.toml davon ab, bricht der Prüfer ab (Exit 2). Sie
    wählt nie selbst die Art.
.PARAMETER RemoteName
    Name des Remotes (vom Hook, erstes Argument); nur zur Auskunft.
.PARAMETER RemoteUrl
    Adresse des Remotes (vom Hook, zweites Argument); bestimmt mit
    <Inhalt>\pruefer\repos.toml die Art des Repos.
.PARAMETER MitCache
    Nutzt den Cache sauberer Blobs (siehe oben). Nur für Vorprüfungen
    (Hochladen.ps1); mit -Hook wirkt er nie, die letzte Schranke prüft kalt.
#>
[CmdletBinding()]
param(
    [ValidateSet('Basis', 'Projekt', 'Inhalt')]
    [string]$Art,
    [string]$Repo,
    [string]$Inhalt,
    [switch]$Hook,
    [string]$Baum,
    [string]$Commits,
    [switch]$AufMain,
    [string]$Beitrag,
    [switch]$Pruefen,
    [string]$Ref = 'refs/heads/main',
    [string]$Bundle,
    [string]$Nachricht,
    [string]$RemoteName,
    [string]$RemoteUrl,
    [switch]$MitCache
)

$ErrorActionPreference = 'Stop'

# Kein Pfad der Ausgabe nennt das Benutzerprofil (auch nicht die FEHLER-Zeile
# mit dem absoluten Pfad der Namensliste): <Laufwerk>:\Users\<name>
# (auch mit / und „Benutzer“) und das eigene Profil werden zu %USERPROFILE%.
function OhneProfil([string]$t) {
    if ($null -eq $t) { return '' }
    if ($env:USERPROFILE) { $t = $t.Replace($env:USERPROFILE, '%USERPROFILE%') }
    return [regex]::Replace($t, '(?i)[a-z]:[\\/]+(?:users|benutzer)[\\/]+[^\\/"''<>|:*?\r\n]+', '%USERPROFILE%')
}

# Geschlossen scheitern: Jeder unerwartete Fehler beendet mit 2.
trap {
    Write-Host "FEHLER Der Prüfer ist selbst gescheitert ($(OhneProfil $_.Exception.Message)). Es geht nichts hinaus."
    if ($script:wegwerf) { Remove-Item -LiteralPath $script:wegwerf -Recurse -Force -ErrorAction SilentlyContinue }
    if ($uhr) { try { UhrZeile $uhr } catch { } }
    exit 2
}

# Die Dateien, die der Prüfer lädt: Ihr Inhalt geht in den Regel-Schlüssel des Caches.
$script:prueferDateien = @($PSCommandPath, (Join-Path $PSScriptRoot 'Laufzeit.ps1'))
$uhr = $null
function Ende([int]$code) { if ($uhr) { try { UhrZeile $uhr } catch { } }; exit $code }
# git über den vollen Pfad: Ein Start über den Namen (ProcessStartInfo, CreateProcess) sucht
# zuerst im aktuellen Ordner, und der Hook läuft in der Wurzel des geprüften Arbeitsbaums.
# Die Befehlssuche von PowerShell nimmt nur PATH, nie den aktuellen Ordner.
$gitExe = (Get-Command git -CommandType Application -ErrorAction Stop | Select-Object -First 1).Source

if (-not $Repo) { $Repo = Split-Path -Parent $PSScriptRoot }
$Repo = (Resolve-Path -LiteralPath $Repo).Path
if (-not $Inhalt) {
    if ($env:DASHBOARD_INHALT) { $Inhalt = $env:DASHBOARD_INHALT }
    # Neben dem Code-Repo des Prüfers: Neben einem Overlay-Klon kann ein anderer Ordner (alte Kopie, fremder Klon) liegen.
    else { $Inhalt = Join-Path (Split-Path -Parent (Split-Path -Parent $PSScriptRoot)) 'dashboard-inhalt' }
}
$utf8 = New-Object System.Text.UTF8Encoding($false)
# Text, Git-Ausgaben und Listen werden streng als UTF-8 gelesen (Utf8Text).
$strikt = New-Object System.Text.UTF8Encoding($false, $true)
try { [Console]::OutputEncoding = $utf8 } catch { }
# Die Zeile zur Laufzeit ändert nie das Ergebnis der Prüfung: Scheitert Laufzeit.ps1, fehlt nur sie.
try { . $script:prueferDateien[1]; $uhr = UhrStart } catch { Write-Host 'HINWEIS Laufzeit.ps1 ließ sich nicht laden; die Zeile zur Laufzeit fehlt.' }
# Ein „.“ oder relativer Eintrag im PATH holte git doch aus dem aktuellen Ordner oder dem geprüften Repo.
$relativ = @($env:PATH -split ';' | Where-Object { $_.Trim() -and -not [IO.Path]::IsPathFullyQualified($_.Trim().Trim('"')) })
if ($relativ) { throw "PATH enthält nicht voll qualifizierte Einträge ($($relativ -join ';')); PATH nur mit vollen Pfaden setzen" }
$gitOrt = Split-Path -Parent $gitExe
if ($gitOrt -eq (Get-Location).ProviderPath -or ($gitOrt + '\').StartsWith($Repo.TrimEnd('\') + '\', [StringComparison]::OrdinalIgnoreCase)) {
    throw "git liegt im geprüften Repo oder im aktuellen Ordner ($gitExe); PATH ohne . und ohne relative Einträge setzen"
}
# Ein Push überträgt die echten Objekte: Ersatz (git replace) und Grafts dürfen
# dem Prüfer nichts anderes zeigen (gilt für jeden git-Aufruf dieses Laufs).
$env:GIT_NO_REPLACE_OBJECTS = '1'
$env:GIT_GRAFT_FILE = Join-Path ([IO.Path]::GetTempPath()) "vor-push-ohne-grafts-$([guid]::NewGuid().ToString('N'))"
$script:funde = New-Object System.Collections.Generic.List[string]
# Was im geprüften Baum steht (Pfad und Blob), damit VerlaufPruefen es nicht doppelt meldet.
$script:imBaum = New-Object 'System.Collections.Generic.HashSet[string]'
$NULL_SHA = '0000000000000000000000000000000000000000'
$RANKE_NAME = 'Ranke'
# Die einzige Commit-Adresse. Im Text gilt sie genau als ganze
# Adresse, auch wenn ein Teil davon auf einer Liste steht; ein Teil allein
# woanders nicht.
$RANKE_MAIL = 'ranke2@proton.me'
# Lokale Zweige, die nach refs/heads/main hinausgehen dürfen. Ein Tag geht nur
# unter seinem eigenen Namen hinaus (TagPruefen).
$LOKAL_ERLAUBT = @('refs/heads/main')
# Teil 3: Tags nur v0.MINOR.PATCH (Versionsregel 0.x), ohne führende Null; [0-9], weil \d auch andere Ziffern nimmt.
$TAG_NAME = '^refs/tags/v0\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)$'
# Die Stände von main, auf deren First-Parent-Linie ein Tag liegen muss (füllt der Ablauf).
$script:mainStaende = @()
# Das Wegwerf-Repo von -Bundle (wird am Ende und im trap gelöscht).
$script:wegwerf = $null
# Geprüft wird als Basis-Repo, außer repos.toml nennt die Adresse des Remotes anders.
$script:istBasis = $true
$script:istProjekt = $false
# Der eine Slug eines Projekt-Repos: der erste Ordner unter projects/, den der Lauf sieht.
$script:projektSlug = $null
# Der ungekürzte Regex von sensibel, wenn die Ausnahmedatei Werte herausnimmt (nur zum Verbergen).
$script:maske = $null
# So viele Zeilen je Muster und Datei werden einzeln genannt, dann die Zahl der weiteren.
$GRENZE = 50

# Auch ein Pfad oder Ref-Name zitiert keinen Wert aus den Listen und
# keine Adresse, SID oder MAC: Treffer werden zu „…“, ein Wort
# mit U+FFFD (kein UTF-8) ganz.
function Maskiert([string]$wo) {
    if ($script:listenRx) { foreach ($r in $script:listenRx.Values) { $f = Falten $wo; if ($r.IsMatch($f)) { $wo = $r.Replace($f, '…') } } }
    # Ausgenommene Werte (Ausnahmedatei) bleiben in der Ausgabe verborgen: Sie ist nur im Projekt-Repo erlaubt.
    if ($script:maske -and $script:maske.IsMatch((Falten $wo))) { $wo = $script:maske.Replace((Falten $wo), '…') }
    if ($script:muster) { foreach ($r in $script:muster.Values) { $wo = $r.Replace($wo, '…') } }
    return [regex]::Replace($wo, '[^/\\\s]*\uFFFD[^/\\\s]*', '…')
}

function Fund([string]$wo, [int]$zeile, [string]$muster) {
    $wo = Maskiert $wo
    $z = if ($zeile -gt 0) { ", Zeile $zeile" } else { '' }
    $script:funde.Add("ABGEWIESEN $wo$($z): $muster")
}

# Die eine Regel der Kodierung: Git-Ausgaben und Textdateien sind
# gültiges UTF-8 (ein BOM fällt weg), sonst ein Fund „kein UTF-8“ mit der Stelle
# (bei Dateien die Zeile), nie dem Inhalt. Weiter geprüft wird dann die
# nachsichtige Lesart mit U+FFFD (Pfade und Commits bleiben lesbar); es gibt keine
# zweite Lesart wie Windows-1252 oder UTF-16.
function Utf8Text([byte[]]$b, [string]$wo, [bool]$mitZeile) {
    try { $t = $strikt.GetString($b) } catch {
        # Die Lage des ersten ungültigen Bytes (PowerShell 7 und 5.1 zählen leicht anders; für die Zeile reicht es).
        $i = [Math]::Min([Math]::Max(0, [int]$_.Exception.InnerException.Index), $b.Length)
        $zeile = if ($mitZeile) { ($utf8.GetString($b, 0, $i) -split "`n").Count } else { 0 }
        Fund $wo $zeile 'kein UTF-8 (ungültige Bytes, etwa Windows-1252 oder UTF-16)'
        $t = $utf8.GetString($b)
    }
    if ($t.Length -gt 0 -and $t[0] -eq [char]0xFEFF) { $t = $t.Substring(1) }
    return $t
}

# Für die Namenslisten: NFC (ein zerlegtes ü wie ein ganzes), klein, ä/ö/ü/ß als
# ae/oe/ue/ss. Trenner gelten in den Mustern gleich (ListenMuster). Mehr nicht
# (keine Doppelgänger, keine unsichtbaren Zeichen, keine Escapes).
function Falten([string]$t) {
    # shortcut: .NET Framework (5.1) wirft bei nicht vergebenen Codepunkten; dann bleibt die Zeile ohne NFC.
    try { $t = $t.Normalize([Text.NormalizationForm]::FormC) } catch { }
    return $t.ToLowerInvariant().Replace('ä', 'ae').Replace('ö', 'oe').Replace('ü', 'ue').Replace('ß', 'ss')
}

# Lange Zeilen (minifiziert, einzeilige JSON) für die Pfad- und Adressmuster in
# überlappenden Stücken: nichts hinter Zeichen 20000 bleibt ungeprüft, und kein
# Wert reißt an einer Stückgrenze (512 Zeichen Überlappung).
function Stuecke([string]$z) {
    if ($z.Length -le 20000) { return , $z }
    $v = New-Object System.Collections.Generic.List[string]
    for ($s = 0; $s -lt $z.Length; $s += 19488) {
        $v.Add($z.Substring($s, [Math]::Min(20000, $z.Length - $s)))
        if ($s + 20000 -ge $z.Length) { break }
    }
    return , $v.ToArray()
}

# Ein Muster für eine Liste: je Wert gefaltet, Trenner (jeder Strich \p{Pd} wie
# U+2010 und U+2013, _, ., +, /, \, ·, Leerzeichen) beliebig oft oder gar nicht.
$trenner = '[\p{Pd}_.+/\\\s·]'
function ListenMuster($namen) {
    (@($namen | ForEach-Object { ((Falten $_) -split "$trenner+" | Where-Object { $_ } | ForEach-Object { [regex]::Escape($_) }) -join "$trenner*" }) | Where-Object { $_ }) -join '|'
}

# ---------------------------------------------------------------------------
# git
# ---------------------------------------------------------------------------
function Git([string[]]$argumente) {
    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = $gitExe
    $psi.Arguments = (@('-C', "`"$Repo`"", '-c', 'core.quotepath=off') + ($argumente | ForEach-Object { if ($_ -match '[\s"]') { '"' + ($_ -replace '"', '\"') + '"' } else { $_ } })) -join ' '
    $psi.UseShellExecute = $false
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.CreateNoWindow = $true
    $p = [Diagnostics.Process]::Start($psi)
    $puffer = New-Object IO.MemoryStream
    $aus = $p.StandardOutput.BaseStream.CopyToAsync($puffer)
    $fehl = $p.StandardError.ReadToEnd()
    $p.WaitForExit()
    $aus.Wait()
    if ($p.ExitCode -ne 0) { throw "git $($argumente -join ' ') endete mit $($p.ExitCode): $($fehl.Trim())" }
    # Pfade, Baum-Einträge, Meldungen, Autor, Committer und Tag-Texte streng als UTF-8.
    return Utf8Text $puffer.ToArray() (OhneProfil "Git-Ausgabe (git $($argumente[0]) $($argumente[-1]))") $false
}

# Alle Blobs über einen Prozess (git cat-file --batch): Pfad -> Bytes.
function Blobs([string[]]$shas) {
    $erg = @{}
    if ($shas.Count -eq 0) { return $erg }
    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = $gitExe
    $psi.Arguments = "-C `"$Repo`" cat-file --batch"
    $psi.UseShellExecute = $false
    $psi.RedirectStandardInput = $true
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.CreateNoWindow = $true
    # Windows PowerShell 5.1 (.NET Framework) baut die Eingabe mit
    # Console.InputEncoding und schreibt dabei sofort deren BOM vor die erste SHA;
    # darum für den Start UTF-8 ohne BOM (gleiche Codepage), danach zurück.
    $altEin = $null
    try { if ([Console]::InputEncoding.GetPreamble().Length -gt 0) { $altEin = [Console]::InputEncoding; [Console]::InputEncoding = $utf8 } } catch { }
    try { $p = [Diagnostics.Process]::Start($psi) }
    finally { if ($altEin) { try { [Console]::InputEncoding = $altEin } catch { } } }
    $fehl = $p.StandardError.ReadToEndAsync()
    $ein = $p.StandardInput.BaseStream
    $strom = $p.StandardOutput.BaseStream
    foreach ($s in $shas) {
        # Je Objekt eine Zeile hinein, dann die Antwort ganz lesen (git gibt sie sofort aus).
        $zeile = [Text.Encoding]::ASCII.GetBytes("$s`n")
        $ein.Write($zeile, 0, $zeile.Length)
        $ein.Flush()
        $kopf = New-Object System.Collections.Generic.List[byte]
        while ($true) {
            $b = $strom.ReadByte()
            if ($b -lt 0) { throw 'git cat-file endete zu früh.' }
            if ($b -eq 10) { break }
            $kopf.Add([byte]$b)
        }
        $teile = $utf8.GetString($kopf.ToArray()).Split(' ')
        if ($teile.Count -lt 3 -or $teile[1] -ne 'blob') { throw "git cat-file: unerwartete Antwort für $s" }
        $groesse = [long]$teile[2]
        $daten = New-Object byte[] $groesse
        $gelesen = 0
        while ($gelesen -lt $groesse) {
            $n = $strom.Read($daten, $gelesen, [int][Math]::Min($groesse - $gelesen, 1048576))
            if ($n -le 0) { throw 'git cat-file endete zu früh.' }
            $gelesen += $n
        }
        [void]$strom.ReadByte()
        $erg[$s] = $daten
    }
    $ein.Close()
    $p.WaitForExit()
    if ($p.ExitCode -ne 0) { throw "git cat-file endete mit $($p.ExitCode): $($fehl.Result)" }
    return $erg
}

# ---------------------------------------------------------------------------
# Namenslisten aus dem Inhalts-Repo (nie im Code-Repo); geschlossen scheitern
# ---------------------------------------------------------------------------
# Genau die gelesenen Bytes jeder Liste (Ordner/Datei, etwa tokens/x.toml) für den Regel-Schlüssel des Caches.
$script:gelesen = New-Object 'System.Collections.Generic.SortedDictionary[string,byte[]]' ([StringComparer]::Ordinal)
function ListeGelesen([string]$datei, [byte[]]$roh) {
    $script:gelesen["$([IO.Path]::GetFileName([IO.Path]::GetDirectoryName($datei)))/$([IO.Path]::GetFileName($datei))"] = $roh
}
function ListenLaden([string]$datei, [string[]]$namen = @('sensibel', 'allgemein', 'tabu', 'benutzer', 'rechner', 'wlan', 'schluessel', 'mac', 'nur_basis'), [string[]]$pflichten = @('allgemein', 'tabu', 'benutzer')) {
    if (-not (Test-Path -LiteralPath $datei -PathType Leaf)) { throw "Die Namensliste $datei fehlt. Ohne sie prüft der Prüfer nicht, und es geht nichts hinaus." }
    # Streng UTF-8: eine Datei in Windows-1252 machte
    # jeden Umlaut-Namen still wirkungslos. Kaputt ist sie auch mit U+FFFD.
    $roh = [IO.File]::ReadAllBytes($datei)
    ListeGelesen $datei $roh
    try { $text = $strikt.GetString($roh) } catch { $text = $null }
    if ($null -eq $text -or $text.IndexOf([char]0xFFFD) -ge 0) { throw "Die Namensliste $datei ist kaputt (kein gültiges UTF-8, etwa Windows-1252). Ohne sie prüft der Prüfer nicht." }
    if ($text.Length -gt 0 -and $text[0] -eq [char]0xFEFF) { $text = $text.Substring(1) }
    $listen = @{}
    foreach ($k in $namen) { $listen[$k] = @() }
    # Eine Liste zweimal in einer Datei ist kaputt (der zweite Block überschrieb den ersten still).
    $schonDa = @{}
    # Nur Kommentare, Leerzeilen und `name = [ "…", … ]` (auch über mehrere Zeilen).
    $ohneKommentar = ($text -split "`n" | ForEach-Object { ($_ -replace '(^|\s)#.*$', '$1').TrimEnd("`r") }) -join "`n"
    $rest = [regex]::Replace($ohneKommentar, '(?ms)^\s*([a-z_]+)\s*=\s*\[(.*?)\]', {
            param($m)
            $name = $m.Groups[1].Value
            if (-not $listen.ContainsKey($name)) { throw "Die Namensliste $datei ist kaputt: unbekannte Liste $name." }
            if ($schonDa.ContainsKey($name)) { throw "Die Namensliste $datei ist kaputt: Die Liste $name steht zweimal da." }
            $schonDa[$name] = $true
            $innen = $m.Groups[2].Value
            # Ein TOML-Escape außer \\ (etwa \t oder \" oder ein \u mit Zahl) wirkte still nicht; kaputt.
            $werte = @([regex]::Matches($innen, '"((?:[^"\\]|\\.)*)"') | ForEach-Object {
                    if (($_.Groups[1].Value -replace '\\\\', '').Contains('\')) { throw "Die Namensliste $datei ist kaputt: In der Liste $name steht ein Escape außer \\ (das Zeichen bitte selbst schreiben)." }
                    $_.Groups[1].Value -replace '\\\\', '\' })
            $ohneWerte = [regex]::Replace($innen, '"((?:[^"\\]|\\.)*)"', '')
            if ($ohneWerte -match '[^\s,]') { throw "Die Namensliste $datei ist kaputt: In der Liste $name steht etwas außer Texten in Anführungszeichen." }
            $listen[$name] = @($werte | Where-Object { $_.Trim() -ne '' } | ForEach-Object { $_.Trim().ToLowerInvariant() })
            ''
        })
    if ($rest -match '\S') { throw "Die Namensliste $datei ist kaputt (eine Zeile ist keine Liste der Form name = [""…""])." }
    foreach ($pflicht in $pflichten) {
        if ($listen[$pflicht].Count -eq 0) { throw "Die Namensliste $datei ist unvollständig: Die Liste $pflicht ist leer. Ohne sie prüft der Prüfer nicht." }
    }
    return $listen
}

$opt = [Text.RegularExpressions.RegexOptions]'IgnoreCase, CultureInvariant'
function Rx([string]$m) { New-Object Text.RegularExpressions.Regex($m, $opt) }
# Namen von KI-Assistenten, Modellen und ihren Anbietern, auch mit Buchstaben
# dahinter (Name und Produkt in einem Wort). Je ein Buchstabe in [ ]: So steht kein
# Name wörtlich hier, und der Prüfer scheitert nicht an sich selbst. Zwischen den
# Wortteilen (hier und in den Anhängen unten) jeder Strich (\p{Pd}, etwa U+2010),
# Leerzeichen, _ oder nichts.
# shortcut: kein NFKC, keine unsichtbaren Zeichen, keine Doppelgänger (wie die Listen, Falten); aufrüsten, wenn absichtliche Umgehung droht.
$tr = '[\s\p{Pd}_]*'
$kiNamen = "c[l]aude|a[n]thropic|o[p]us|s[o]nnet|h[a]iku|c[h]at${tr}g[p]t|g[p]t|o[p]en${tr}ai(?!r)|g[e]mini|c[o]${tr}pilot|c[o]dex|l[l]ama|m[i]stral|f[a]ble"
# Keine Anhänge (Trailer) und keine KI-Namen in Commit-Nachrichten, in jeder Art.
# Läuft auf der gefalteten Zeile.
# Jeden anderen Trailer findet git selbst,
# den Fuß BREAKING CHANGE nicht (Leerzeichen im Schlüssel); Brüche nur mit ! (Teil 3).
$anhang = Rx "(?<![a-z])(?:co${tr}authored${tr}by|signed${tr}off${tr}by|generated${tr}(?:with|by)|breaking${tr}change\s*:|$kiNamen)"
# Erste Zeile nach Conventional Commits, für Hook und Push dieselbe Quelle (auch Merges).
# Hinter ': ' ein sichtbares Zeichen: kein Leerraum, kein Formatzeichen (\p{Cf}, etwa U+200B).
$ccKopf = '^(?:feat|fix|docs|style|refactor|perf|test|build|ci|chore|revert)(?:\([^()\s]+\))?!?: [^\s\p{Cf}]'

# Anhänge und KI-Namen in einer Nachricht (CommitsPruefen und -Nachricht).
function AnhangPruefen([string]$wo, [string]$meldung) {
    $zeilen = $meldung -split "`n"
    $jeZeile = $false
    for ($i = 0; $i -lt $zeilen.Count; $i++) {
        if ($anhang.IsMatch((Falten $zeilen[$i]))) { $jeZeile = $true; Fund $wo ($i + 1) 'Anhang (Trailer) oder KI-Name in der Nachricht' }
    }
    # Auch über einen Zeilenumbruch (Co-Authored-⏎By): das Muster erlaubt Leerraum zwischen den Teilen; ein am Strich getrennter Name (Clau-⏎de) zusammengesetzt.
    if (-not $jeZeile -and ($anhang.IsMatch((Falten $meldung)) -or $anhang.IsMatch((Falten ($meldung -replace '\p{Pd}\r?\n', ''))))) { Fund $wo 0 'Anhang (Trailer) oder KI-Name in der Nachricht (über einen Zeilenumbruch)' }
}

# Teil 3: Hook commit-msg. Ohne Namenslisten (ein Commit geht auch ohne
# Inhalts-Repo, etwa im Worktree); den Text prüft der Push mit den Listen.
if ($Nachricht) {
    # git gibt dem Hook die Nachricht vor dem Aufräumen, und bei -m und -F räumt es
    # Kommentare und Schere gar nicht weg: darum ohne sie zurückschreiben, dann prüft
    # der Hook genau, was git speichert (git liest die Datei nach dem Hook neu).
    # shortcut: nur core.commentChar '#', bei einem anderen Zeichen git config core.commentChar lesen.
    $datei = (Resolve-Path -LiteralPath $Nachricht).Path
    $zeilen = New-Object System.Collections.Generic.List[string]
    # git schreibt seine Kommentare meist hinter den Text; eine #-Zeile davor oder dazwischen kam meist von Hand (-m, -F), bei squash-Vorlagen auch von git (dann nur ein harmloser HINWEIS).
    $hinweis = $false; $textDa = $false; $kommentarNachText = $false
    foreach ($z in ((Utf8Text ([IO.File]::ReadAllBytes($datei)) 'Nachricht' $true) -split "`n")) {
        $z = $z.TrimEnd("`r")
        if ($z -cmatch '^# -{24} >8 -{24}$') { break }
        if ($z.StartsWith('#')) {
            if ($textDa) { $kommentarNachText = $true } else { $hinweis = $true }
            continue
        }
        if ($z.Trim()) { if ($kommentarNachText) { $hinweis = $true }; $textDa = $true }
        $zeilen.Add($z)
    }
    if ($hinweis) { Write-Host 'HINWEIS Kommentarzeilen (#) entfernt.' }
    $text = ($zeilen -join "`n").Trim()
    [IO.File]::WriteAllText($datei, $text + "`n", $utf8)
    $beispiel = 'So geht es: <typ>(<bereich>)!: <text>, typ einer von feat, fix, docs, style, refactor, perf, test, build, ci, chore, revert; Bereich und ! (Bruch) nach Bedarf, nach dem Doppelpunkt ein Leerzeichen. Beispiel: fix(web): Knopf reagiert wieder auf Enter'
    $kopf = ($text -split "`n")[0]
    if ($kopf -cnotmatch $ccKopf) { Fund 'Nachricht' 1 'erste Zeile nicht nach Conventional Commits' }
    AnhangPruefen 'Nachricht' $text
    if ((Git @('interpret-trailers', '--parse', $datei)).Trim()) { Fund 'Nachricht' 0 'Anhang (Trailer) in der Nachricht' }
    foreach ($f in $script:funde) { Write-Host $f }
    if ($script:funde.Count -gt 0) {
        Write-Host "$beispiel. Keine Fußzeilen und keine Anhänge (Trailer), keine Namen von KI-Modellen oder ihren Firmen."
        Ende 1
    }
    Ende 0
}

$listen = ListenLaden (Join-Path $Inhalt 'pruefer\namen.toml')
# Dazu jede Datei pruefer\tokens\*.toml (je Quelle eine, nur sensibel).
$tokensOrdner = Join-Path $Inhalt 'pruefer\tokens'
if (-not (Test-Path -LiteralPath $tokensOrdner -PathType Container)) { throw "Der Ordner $tokensOrdner fehlt. Ohne ihn prüft der Prüfer nicht." }
$tokenDateien = @(Get-ChildItem -LiteralPath $tokensOrdner -Filter '*.toml' -File -Force | Sort-Object Name)
if ($tokenDateien.Count -eq 0) { throw "Im Ordner $tokensOrdner liegt keine *.toml. Ohne sie prüft der Prüfer nicht." }
# Was sonst im Ordner liegt (Unterordner, *.toml.txt, *.bak), fehlte still.
foreach ($a in @(Get-ChildItem -LiteralPath $tokensOrdner -Force)) {
    if (-not ($a -is [IO.FileInfo] -and ($a.Extension -eq '.toml' -or $a.Name -ceq 'LIESMICH.txt'))) { throw "Im Ordner $tokensOrdner liegt $($a.Name), das nicht zählt (nur *.toml und LIESMICH.txt). Ohne Klarheit prüft der Prüfer nicht." }
}
foreach ($t in $tokenDateien) { $listen.sensibel += (ListenLaden $t.FullName @('sensibel') @('sensibel')).sensibel }

# ---------------------------------------------------------------------------
# Art des Repos: nur aus der Adresse des Remotes und <Inhalt>\pruefer\repos.toml
# ---------------------------------------------------------------------------
# Die Adresse zum Vergleich: Schema und Rechnername klein (https://Host/…,
# auch die scp-Form nutzer@host:pfad), am Ende kein / und kein .git. Sonst
# genau, auch Groß/Klein im Pfad.
function AdresseNormal([string]$adresse) {
    $a = $adresse.Trim()
    $m = [regex]::Match($a, '^([A-Za-z][A-Za-z0-9+.-]*://)([^/@]*@)?([^/]*)(.*)$')
    if ($m.Success) {
        $a = $m.Groups[1].Value.ToLowerInvariant() + $m.Groups[2].Value + $m.Groups[3].Value.ToLowerInvariant() + $m.Groups[4].Value
    } else {
        # scp-Form; ein Laufwerk wie C: ist kein Rechnername (mindestens zwei Zeichen).
        $m = [regex]::Match($a, '^([^@/:\\]+@)?([^/:\\]{2,}):(.*)$')
        if ($m.Success) { $a = $m.Groups[1].Value + $m.Groups[2].Value.ToLowerInvariant() + ':' + $m.Groups[3].Value }
    }
    while ($true) {
        $vorher = $a
        $a = $a.TrimEnd('/')
        if ($a.EndsWith('.git', [StringComparison]::Ordinal)) { $a = $a.Substring(0, $a.Length - 4) }
        if ($a -ceq $vorher) { break }
    }
    return $a
}

# Liest repos.toml: Kommentare, Leerzeilen, einmal [repos], darunter je Zeile
# "adresse" = "Basis"|"Projekt"|"Inhalt". Fehlt die Datei: $null. Alles
# andere (auch eine Adresse zweimal) ist kaputt und bricht ab (Exit 2).
function ReposLaden([string]$datei) {
    if (-not (Test-Path -LiteralPath $datei -PathType Leaf)) { return $null }
    $roh = [IO.File]::ReadAllBytes($datei)
    ListeGelesen $datei $roh
    $text = $utf8.GetString($roh)
    if ($text.Length -gt 0 -and $text[0] -eq [char]0xFEFF) { $text = $text.Substring(1) }
    # Groß/Klein zählt bei den Adressen (ein Hashtable von PowerShell unterschiede sie nicht).
    $tabelle = New-Object 'System.Collections.Generic.Dictionary[string,string]' ([StringComparer]::Ordinal)
    $abschnitt = $false
    $nr = 0
    foreach ($z in ($text -split "`n")) {
        $nr++
        $z = $z.TrimEnd("`r")
        if ($z -match '^\s*(#.*)?$') { continue }
        if ($z -cmatch '^\s*\[repos\]\s*(#.*)?$') {
            if ($abschnitt) { throw "Die Liste $datei ist kaputt (Zeile $nr): [repos] steht zweimal da." }
            $abschnitt = $true
            continue
        }
        $m = [regex]::Match($z, '^\s*"((?:[^"\\]|\\.)*)"\s*=\s*"(Basis|Projekt|Inhalt)"\s*(#.*)?$')
        if (-not $abschnitt -or -not $m.Success) {
            throw "Die Liste $datei ist kaputt (Zeile $nr): erwartet [repos] und darunter Zeilen der Form ""adresse"" = ""Basis"", ""Projekt"" oder ""Inhalt""."
        }
        $schluessel = AdresseNormal ($m.Groups[1].Value -replace '\\\\', '\')
        if (-not $schluessel) { throw "Die Liste $datei ist kaputt (Zeile $nr): die Adresse ist leer." }
        if ($tabelle.ContainsKey($schluessel)) { throw "Die Liste $datei ist kaputt (Zeile $nr): dieselbe Adresse steht schon weiter oben." }
        $tabelle[$schluessel] = $m.Groups[2].Value
    }
    return , $tabelle
}

# ---------------------------------------------------------------------------
# Muster
# ---------------------------------------------------------------------------
function Escape-Liste($namen) { ($namen | ForEach-Object { [regex]::Escape($_) }) -join '|' }

# Benutzerpfade in allen Schreibformen. Erlaubt sind nur Platzhalter: <…>,
# %…%, $…, Public/Default und erfundene Namen der Tests (ein Buchstabe,
# jemand, beispiel…, feind, name) und ein Platzhalter im Format von Rust
# wie {NAME} (ganz in geschweiften Klammern, kein Kontoname).
$platzhalter = '(?:<[^>]*>|%[a-z]+%|\$[a-z_:{}]+|\{[a-z_]+\}|public|default|all users|default user|jemand|beispiel\d*|feind|name|[a-z])(?![a-z0-9])'
$muster = [ordered]@{
    # C:\Users\<name>\…, auch die Admin-Freigabe \\rechner\c$\Users\<name>\… (c$ statt c:) und file:///c%3A/Users/<name>.
    # Trenner als eine Klasse: (?:\\\\|\\|/)+ zerlegt einen Lauf aus \ auf Fib(n) Arten (exponentiell).
    'Benutzerpfad'                       = Rx ('(?<![a-z0-9])[a-z](?::|\$|%3a)[\\/]+[u]sers[\\/]+(?!' + $platzhalter + ')[^\\/\s"''<>*|:]+')
    # /c/Users/<name>/… (Git Bash), auch /mnt/c/Users/<name>/… (WSL) und /cygdrive/c/Users/<name>/… (Cygwin).
    'Benutzerpfad (Git-Bash-Form)'       = Rx ('(?<![a-z0-9])(?:/mnt|/cygdrive)?/[a-z]/[u]sers/(?!' + $platzhalter + ')[^/\s"''<>*|:]+')
    # /Users/<name>/… (macOS) und /home/<name>/… (Linux), auch file:///Users/<name>, JSON \/home\/<name>,
    # PATH-Listen (…:/home/<name>), scp-Ziele (rechner:/home/<name>) und Schalter (-L/home/<name>).
    # Nicht nach Buchstabe oder Ziffer, auch nicht mit \ dazwischen (Domäne, auch in JSON, Wortteil,
    # /c/Users, /usr/home), außer einem Schalter wie -L oder -isystem. Users nicht nach einem
    # Laufwerk C:, c$ oder c%3A mit beliebigen Trennern (das meldet die Windows-Form oben, sonst
    # zählte derselbe Pfad doppelt); /home dort schon, die Windows-Form kennt nur Users.
    # Auch /var/home und /export/home und beliebige Trenner nach dem Ordner (/home//<name>, /home\<name>).
    # Das Literal steht vor dem Lookbehind über die Trenner: sonst lief er hinter jedem / zurück (quadratisch).
    'Benutzerpfad (macOS/Linux)'         = Rx ('(?:(?<![a-z0-9]\\?)|(?<=(?<![a-z0-9])-[a-z]+)|(?<=(?<![a-z0-9])\\?/(?:var|export)\\?)(?=/[h]ome))/(?:[u]sers(?<!(?<![a-z0-9])[a-z](?::|\$|%3a)[\\/]*/[u]sers)|[h]ome)(?>[\\/]+)(?!' + $platzhalter + ')[^\\/\s"''<>*|:]+')
    # WSL von Windows aus: \\wsl.localhost\<distro>\home\<name> und \\wsl$\<distro>\home\<name>, auch mit / und doppelten Trennern.
    'Benutzerpfad (WSL-Freigabe)'        = Rx ('[\\/]{2,}wsl(?:\.localhost|\$)[\\/]+[^\\/\s]+[\\/]+[h]ome[\\/]+(?!' + $platzhalter + ')[^\\/\s"''<>*|:]+')
    # Der Name ganz (bis zum nächsten %, Trenner oder Leerraum), damit Maskiert ihn ganz ersetzt.
    'Benutzerpfad (URL-kodiert)'         = Rx ('(?:users|(?:(?<![a-z0-9])|(?<=%2f|%5c|%252f|%255c))home)(?:%5c|%2f|%255c|%252f)+(?!' + $platzhalter + ')[^\\/\s"''<>*|:%]+')
    # Ausnahme: PROGRA~1 ist der Kurzname von Program Files, kein Konto.
    'Benutzerpfad (8.3-Kurzname)'        = Rx '(?:\\|/)(?!progra~)[a-z0-9]{1,6}~[0-9](?:\\|/|$)'
    'E-Mail-Adresse'                     = Rx '[a-z0-9._%+-]+@[a-z0-9-]+(?:\.[a-z0-9-]+)*\.[a-z]{2,}'
    # Erfundene SIDs der Tests (je Teil nur eine Ziffer wiederholt, etwa 1111111111) gelten nicht.
    'SID eines Kontos'                   = Rx 'S-1-5-21-(?!(\d)\1+-(\d)\2+-(\d)\3+(?!\d))\d{6,}-\d{6,}-\d{6,}'
    'MAC-Adresse'                        = Rx '(?<![0-9a-f:-])[0-9a-f]{2}([:-])[0-9a-f]{2}(?:\1[0-9a-f]{2}){4}(?![0-9a-f:-])'
}
$erlaubteMails = Rx ('^(?:' + [regex]::Escape($RANKE_MAIL) + '|[^@]+@(?:[a-z0-9-]+\.)*(?:invalid|example|test|localhost)|[^@]+@(?:[a-z0-9-]+\.)*example\.(?:org|com|net))$')
# Die Commit-Adresse fällt vor dem Vergleich mit den Listen weg, aber nur
# als ganze Adresse (nicht in xranke2@… oder …proton.me.example).
$adresseRanke = Rx ('(?<![a-z0-9._%+-])' + [regex]::Escape($RANKE_MAIL) + '(?![a-z0-9_-]|\.[a-z0-9])')
$listenMuster = [ordered]@{
    'sensibler Name'   = $listen.sensibel
    'Name eines Tabu-Ordners' = $listen.tabu
    'Benutzername'     = $listen.benutzer
    'Rechnername'      = $listen.rechner
    'WLAN-Name'        = $listen.wlan
    'Schlüssel-Name'   = $listen.schluessel
    'MAC-Adresse aus der Liste' = $listen.mac
}
$listenRx = [ordered]@{}
foreach ($k in $listenMuster.Keys) {
    $lm = ListenMuster $listenMuster[$k]
    if ($lm) { $listenRx[$k] = Rx ('(?:' + $lm + ')') }
}
# Im Inhalt einer Binärdatei sind gepackte Bytes Zufall: dort nur KI-Namen ab vier
# Buchstaben und mit Grenze auf beiden Seiten (sonst trifft das kurze in jedem Foto).
$kiBinaer = Rx ('(?<![a-z0-9])(?:' + (($kiNamen -split '\|' | Where-Object { $_ -cne 'g[p]t' }) -join '|') + ')(?![a-z0-9])')
# shortcut: Cargo.lock in der Wurzel nennt Pakete Dritter (etwa ein Paket für ein seltenes Betriebssystem), darum dort keine KI-Namen im Inhalt; aufrüsten, wenn die Sperrdatei von Hand bearbeitet wird.
$sperrdatei = Rx '^Cargo\.lock$'
$allgemeinPfad = Rx ('projects[\\/]+(?:' + (Escape-Liste $listen.allgemein) + ')(?![a-z0-9_-])')
$allgemeinKiste = Rx ('projekt[-_](?:' + ((($listen.allgemein | ForEach-Object { ([regex]::Escape($_)).Replace('-', '[-_]') })) -join '|') + ')(?![a-z0-9_-])')
$modulPfad = Rx 'path\s*=\s*"[^"]*projects[\\/]+(?!_vorlage[\\/"])'
# Ein privater Schlüssel (PEM, OpenSSH, PGP, PuTTY) ist in jedem Text ein Fund; ein öffentlicher Block (KONTAKT-PGP.asc) nicht.
# Geschrieben als -{5} und File[-], damit diese Datei sich nicht selbst meldet.
$privaterSchluessel = Rx '-{5}BEGIN [A-Z0-9 ]*PRIVATE KEY|PuTTY-User-Key-File[-]'
$geheimName = Rx '(?:zugangsdaten|passw|kennwort|\.pfx$|\.p12$|\.kdbx?$|\.pem$|\.key$|(?:^|/)\.env|sicherheitsschl|recovery.?code|settings\.dat$)'
# Ausnahme: genau der Abschnitt 7 der Anleitung handelt von Geheimnissen und ist keins.
$geheimAusnahme = Rx '^docs/anleitung/contributing-07-7-geheimnisse-passwoerter-nie-im-terminal\.toml$'
# Ausnahme: Die Prüfsumme (SRI, sha512-…, genau 88 Zeichen Base64) in
# web/vendor/<paket>/HERKUNFT.toml und web/package-lock.json ist Zufall; ein kurzer
# Listenwert kann darin stehen. Nur dort und nur in dieser Form fällt sie vor dem Listenvergleich weg.
$herkunft = Rx '^(?:web/vendor/[^/]+/HERKUNFT\.toml|web/package-lock\.json)$'
$sri = Rx '(?<![a-z0-9+/])sha512-[a-z0-9+/]{86}==(?![a-z0-9+/=])'
# Ausnahme: Das Feld "mappings" einer Quellkarte in
# web/vendor ist Base64-VLQ (nur Buchstaben, Ziffern, + / , ;); ein kurzer
# Listenwert steht darin zufällig. Nur dieses Feld fällt vor dem Listenvergleich weg.
$quellkarte = Rx '^web/vendor/.+\.map$'
$mappings = Rx '"mappings"\s*:\s*"[a-z0-9+/,;]*"'
$codeEndung = Rx '\.(?:kt|kts|java|rs|ts|tsx|js|jsx|mjs|svelte|go|py|sql|md|html|css|example|sample)$'
$chatName = Rx '(?:^|/)(?:conversations\.json|chat-export[^/]*|[^/]*-chat-[^/]*\.md)$'
# Lokale Einstellungen (*.local.*, etwa settings.local.json, und *.local) gehen nie hinaus.
$lokalName = Rx '(?:^|/)[^/]*\.local(?:\.[^/]+)?$'
$chatJsonRolle = Rx '"(?:role|author)"\s*:\s*(?:\{\s*"role"\s*:\s*)?"(?:user|assistant|system|human)"'
$chatJsonBehaelter = Rx '"(?:messages|mapping)"\s*:'
# Eine Zeile, die eine Rolle eröffnet: „## Du“, „**Human:**“, „User:“ und ähnlich.
$chatZeile = Rx ('^\s*(?:#{1,3}\s*(?<r>du|ich|user|assistant|human|ki|' + $kiNamen + ')\s*:?\s*$|\*\*(?<r>du|ich|user|assistant|human|ki|' + $kiNamen + ')\s*:?\s*\*\*\s*:?|(?<r>user|assistant|human|' + $kiNamen + ')\s*:)')

# Erlaubte Orte (Positivliste) und verbotene Orte.
$positiv = @(
    '^(?:src|crates|tests|scripts|vorlagen)/',
    '^web/(?!(?:build|builds|node_modules|\.svelte-kit)/)',
    '^docs/zielbild/',
    # Etappe 6b: die Anleitung als TOML-Abschnitte (/anleitung).
    '^docs/anleitung/',
    '^projects/_vorlage/',
    '^\.cargo/config\.toml$',
    '^(?:Cargo\.toml|Cargo\.lock|LIESMICH\.txt|CHANGELOG\.txt|CONTRIBUTING\.md|KONTAKT-PGP\.asc|LICENSE(?:\.txt)?|\.gitignore|\.gitattributes|rustfmt\.toml|\.git-blame-ignore-revs)$',
    # Nur die Issue-Formulare (Merge-Angebote), nicht ganz .github (keine Workflows, die auf GitHub Code ausführen).
    '^\.github/ISSUE_TEMPLATE/(?!config\.yml$)[^/]+\.yml$',
    # Lizenz der Dokumentation (CC0) und der übernommenen GPL-Dateien (src/herzschlag), Ort nach REUSE;
    # LICENSE in der Wurzel bleibt allein für GitHub.
    '^LICENSES/(?:CC0-1\.0|GPL-3\.0-or-later)\.txt$'
) | ForEach-Object { Rx $_ }
$verboten = [ordered]@{
    'projects/<slug> gehört in den Inhalt (dashboard-inhalt)' = Rx '^projects/(?!_vorlage/)'
    'data/ gehört in den Inhalt'                               = Rx '^data/'
    'docs/entscheidungen gehört in den Inhalt'                 = Rx '^docs/entscheidungen(?:/|$)'
    'docs/architektur-stand.md gehört in den Inhalt'           = Rx '^docs/architektur-stand\.md$'
    'projekte.generated.ts ist erzeugt und privat'             = Rx '^web/src/lib/projekte\.generated\.ts$'
    'erzeugte Typen eines Projekts (vertrag/projekte)'         = Rx '^web/src/lib/vertrag/projekte/'
    # Etappe 6d: kein Markdown mehr im Code-Repo; die Anleitung steht in docs/anleitung.
    # Ausnahme: CONTRIBUTING.md in der Wurzel, der Leitfaden für Mitwirkende, den GitHub dort erwartet.
    'Markdown-Datei (*.md) gibt es im Code-Repo nicht mehr'    = Rx '(?<!^CONTRIBUTING)\.md$'
}
$binaerErlaubt = @('^web/static/', '^tests/beispiel[^/]*/(?:.*/)?assets/') | ForEach-Object { Rx $_ }
$testBeispiel = Rx '^tests/beispiel[^/]*/'
$vendor = Rx '^web/vendor/'

# ---------------------------------------------------------------------------
# Prüfungen
# ---------------------------------------------------------------------------
function TextPruefen([string]$wo, [string]$text, [bool]$istPfad, [string]$pfad) {
    # $istPfad: der Text ist ein Pfad oder Ref-Name (ohne Prüfung auf Module und Chat-Exporte).
    # Teil der Regel „kein UTF-8“: Steuerzeichen außer Tab, LF, CR
    # (etwa NUL aus UTF-16 oder UTF-32 ohne BOM) in Text, Meldung oder Pfad.
    $s = [regex]::Match($text, '[\x00-\x08\x0B\x0C\x0E-\x1F\x7F-\x9F]')
    if ($s.Success) { Fund $wo ($text.Substring(0, $s.Index) -split "`n").Count 'kein UTF-8 (Steuerzeichen außer Tab, LF, CR, etwa NUL aus UTF-16)' }
    $zeilen = $text -split "`n"
    # Jede Zeile je Muster melden (bis $GRENZE je Muster
    # und Datei, dann die Zahl der weiteren), damit ein Lauf alle Stellen zeigt.
    $zahl = @{}
    $melden = {
        param([int]$zeile, [string]$k)
        $zahl[$k] = 1 + $(if ($zahl.ContainsKey($k)) { $zahl[$k] } else { 0 })
        if ($zahl[$k] -le $GRENZE) { Fund $wo $zeile $k }
    }
    # Für Werte über Zeilen hinweg: der gefaltete Gesamttext (Zeile am Strich
    # „-“ ohne Strich angehängt, sonst mit Leerzeichen) und wo jede Zeile beginnt.
    $gesamt = New-Object System.Text.StringBuilder
    $anfaenge = New-Object 'System.Collections.Generic.List[int]'
    $listenTreffer = New-Object 'System.Collections.Generic.HashSet[string]'
    for ($i = 0; $i -lt $zeilen.Count; $i++) {
        $z = $zeilen[$i]
        $gemeldet = @{}
        foreach ($stueck in (Stuecke $z)) {
            foreach ($k in $muster.Keys) {
                if ($gemeldet.ContainsKey($k)) { continue }
                foreach ($m in $muster[$k].Matches($stueck)) {
                    if ($k -eq 'E-Mail-Adresse' -and ($erlaubteMails.IsMatch($m.Value) -or ($pfad -and $vendor.IsMatch($pfad)))) { continue }
                    $gemeldet[$k] = $true; & $melden ($i + 1) $k; break
                }
            }
        }
        $gefaltet = $adresseRanke.Replace((Falten $z), '…')
        if ($pfad -and $herkunft.IsMatch($pfad)) { $gefaltet = $sri.Replace($gefaltet, '…') }
        if ($pfad -and $quellkarte.IsMatch($pfad)) { $gefaltet = $mappings.Replace($gefaltet, '"mappings":"…"') }
        foreach ($k in $listenRx.Keys) {
            if ($k -eq 'KI-Name' -and -not $istPfad -and $pfad -and $sperrdatei.IsMatch($pfad)) { continue }
            if ($listenRx[$k].IsMatch($gefaltet)) { [void]$listenTreffer.Add("$i`0$k"); & $melden ($i + 1) $k }
        }
        $anfaenge.Add($gesamt.Length)
        $g = $gefaltet.TrimEnd("`r")
        if ($g.EndsWith('-')) { [void]$gesamt.Append($g.Substring(0, $g.Length - 1)) } else { [void]$gesamt.Append($g).Append(' ') }
        # Ortsregeln des Code-Repos: nur im Basis-Repo.
        if ($script:istBasis -and -not ($pfad -and $testBeispiel.IsMatch($pfad)) -and $allgemeinPfad.IsMatch($z)) {
            & $melden ($i + 1) 'Projektname als Pfad projects/<slug>'
        }
        if ($script:istBasis -and $pfad -and -not $istPfad) {
            if (($pfad -match '(^|/)Cargo\.toml$' -and $modulPfad.IsMatch($z)) -or ($pfad -match '\.(rs|toml)$' -and $allgemeinKiste.IsMatch($z))) {
                & $melden ($i + 1) 'Projektmodul oder Projektname im Code-Repo'
            }
        }
    }
    # Ein Wert über einen Zeilenumbruch (Vorname⏎Nachname, geheim-⏎wort): nur
    # was über eine Zeilengrenze reicht, gemeldet an der Zeile seines Anfangs.
    if ($zeilen.Count -gt 1) {
        $alles = $gesamt.ToString()
        foreach ($k in $listenRx.Keys) {
            if ($k -eq 'KI-Name' -and -not $istPfad -and $pfad -and $sperrdatei.IsMatch($pfad)) { continue }
            foreach ($m in $listenRx[$k].Matches($alles)) {
                $von = $anfaenge.BinarySearch($m.Index); if ($von -lt 0) { $von = (-bnot $von) - 1 }
                $bis = $anfaenge.BinarySearch($m.Index + $m.Length - 1); if ($bis -lt 0) { $bis = (-bnot $bis) - 1 }
                # Auch innerhalb einer Zeile: ein Nachbar über den Umbruch (Super-⏎Wert) zählt; Treffer je Zeile stehen schon in listenTreffer.
                if ($listenTreffer.Add("$von`0$k")) { & $melden ($von + 1) $k }
            }
        }
    }
    foreach ($k in $zahl.Keys) {
        if ($zahl[$k] -gt $GRENZE) { Fund $wo 0 "$k (und $($zahl[$k] - $GRENZE) weitere Zeilen)" }
    }
    # Auch in Commit-Nachrichten, Tag-Texten und jedem Blob der Geschichte, nicht nur im Baum der Spitze.
    if (-not $istPfad) {
        $pk = $privaterSchluessel.Match($text)
        if ($pk.Success) { Fund $wo ($text.Substring(0, $pk.Index) -split "`n").Count 'privater Schlüssel (BEGIN … PRIVATE KEY oder PuTTY-Schlüsseldatei)' }
    }
    if ($pfad -and -not $istPfad) {
        # Chat-Exporte: JSON mit Rollen, Text mit mehreren Rollenwechseln.
        if ($pfad -match '\.json$' -and $chatJsonRolle.IsMatch($text) -and $chatJsonBehaelter.IsMatch($text)) { Fund $wo 0 'Chat-Export (JSON mit Rollen)' }
        if ($pfad -match '\.(md|txt|markdown)$') {
            $rollen = @()
            foreach ($z in $zeilen) {
                $m = $chatZeile.Match($z)
                if ($m.Success) {
                    $r = $m.Groups['r'].Value.ToLowerInvariant()
                    if ($rollen.Count -eq 0 -or $rollen[-1] -ne $r) { $rollen += $r }
                }
            }
            if ($rollen.Count -ge 3 -and (@($rollen | Sort-Object -Unique)).Count -ge 2) { Fund $wo 0 'Chat-Export (mehrere Rollenwechsel)' }
        }
    }
}

function PfadPruefen([string]$pfad) {
    $p = $pfad
    # Die Git-Ausgabe hat „kein UTF-8“ schon gemeldet; hier dazu der Pfad (Maskiert verschluckt das Wort mit U+FFFD).
    if ($p.Contains([char]0xFFFD)) { Fund $pfad 0 'kein UTF-8 (Pfad)' }
    $ok = $false
    foreach ($r in $positiv) { if ($r.IsMatch($p)) { $ok = $true; break } }
    $grund = $null
    foreach ($k in $verboten.Keys) { if ($verboten[$k].IsMatch($p)) { $grund = $k; break } }
    # Positivliste und Orte des Inhalts: nur im Basis-Repo.
    if ($script:istBasis) {
        if ($grund) { Fund $pfad 0 $grund }
        elseif (-not $ok) { Fund $pfad 0 'Ordner oder Datei nicht in der Positivliste' }
    } elseif ($script:istProjekt) {
        # Overlay: genau ein projects/<slug>/**, der erste Slug des Laufs; jeder weitere ist fremd.
        $m = [regex]::Match($p, '^projects/([^/]+)/.')
        if ($m.Success) {
            if (-not $script:projektSlug) { $script:projektSlug = $m.Groups[1].Value }
            if ($m.Groups[1].Value -cne $script:projektSlug) { Fund $pfad 0 'fremdes Projekt (ein zweiter Ordner unter projects/)' }
        } elseif ($p -cnotmatch '^(?:dashboard-version\.toml|\.gitignore|\.gitattributes|CONTRIBUTING\.md)$') { Fund $pfad 0 'Ordner oder Datei nicht in der Positivliste des Projekt-Repos' }
        if ($p -match '\.md$' -and $p -cne 'CONTRIBUTING.md') { Fund $pfad 0 'Markdown-Datei (*.md)' }
    }
    $name = Split-Path -Leaf $p
    if ($geheimName.IsMatch($p) -and -not $codeEndung.IsMatch($name) -and -not $geheimAusnahme.IsMatch($p)) { Fund $pfad 0 'Name einer Geheimnis-Datei' }
    if ($chatName.IsMatch($p)) { Fund $pfad 0 'Name eines Chat-Exports' }
    if ($lokalName.IsMatch($p)) { Fund $pfad 0 'Lokale Datei (*.local.*)' }
    TextPruefen "$pfad (Pfad)" $p $true $p
}

# Binärdatei ist nur ein Format, dessen Kopf MetadatenPruefen ansieht; alles andere (zip, woff, bin …) ist Text und
# muss gültiges UTF-8 ohne Steuerzeichen sein. Kein Raten am Inhalt (NUL-Bytes).
$binaerEndung = Rx '\.(?:png|jpe?g|gif|ico|webp|pdf)$'

# Die Stücke eines PNG von $start (gleich hinter der Signatur) bis $ende
# (ausschließlich): Meta ist der erste Typ mit Metadaten (tEXt, iTXt, zTXt,
# eXIf) oder $null; Kaputt ein Grund oder $null. Kaputt: ein Stück reicht über das Ende hinaus (abgeschnitten),
# IEND fehlt, IEND hat Daten oder nach IEND kommt noch etwas. Kaputt geht nie
# still durch: Was dahinter steht, sähe die Suche nach Metadaten nicht.
function PngLesen([byte[]]$b, [long]$start, [long]$ende) {
    $meta = $null
    $i = $start
    while ($true) {
        if ($i + 12 -gt $ende) { return @{ Meta = $meta; Kaputt = 'abgeschnitten, ohne IEND am Ende' } }
        $laenge = ([long]$b[$i] -shl 24) -bor ([long]$b[$i + 1] -shl 16) -bor ([long]$b[$i + 2] -shl 8) -bor [long]$b[$i + 3]
        if ($laenge -gt 0x7FFFFFFF -or $i + 12 + $laenge -gt $ende) { return @{ Meta = $meta; Kaputt = 'abgeschnittenes Stück' } }
        $typ = [Text.Encoding]::ASCII.GetString($b, $i + 4, 4)
        # Positivliste der Stücke: alles andere, auch iCCP
        # (Profilname) und private Stücke, zählt als Metadaten.
        if (-not $meta -and $typ -cnotin 'IHDR', 'PLTE', 'IDAT', 'IEND', 'tRNS', 'gAMA', 'cHRM', 'sRGB', 'pHYs') { $meta = $typ -replace '[^A-Za-z]', '?' }
        $i += 12 + $laenge
        if ($typ -eq 'IEND') {
            if ($laenge -ne 0 -or $i -ne $ende) { return @{ Meta = $meta; Kaputt = 'endet nicht sauber mit IEND' } }
            return @{ Meta = $meta; Kaputt = $null }
        }
    }
}

# Beginnt bei $ab die Signatur eines PNG?
function IstPng([byte[]]$b, [long]$ab) {
    $signatur = [byte[]](0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A)
    if ($ab + 8 -gt $b.Length) { return $false }
    for ($k = 0; $k -lt 8; $k++) { if ($b[$ab + $k] -ne $signatur[$k]) { return $false } }
    return $true
}

# Vier Bytes ab $i als Zahl (little endian).
function Le32([byte[]]$b, [long]$i) { [long]$b[$i] -bor ([long]$b[$i + 1] -shl 8) -bor ([long]$b[$i + 2] -shl 16) -bor ([long]$b[$i + 3] -shl 24) }

# Metadaten in Binärdateien der Positivliste, auch WebP (Stücke EXIF und XMP), ICO (darin eingebettete PNG) und XMP in
# jeder Datei (JPEG, PNG, PDF, WebP …: <x:xmpmeta oder der Namensraum xap/1.0).
# Auch eine kaputte Struktur von PNG, ICO und WebP ein Fund
# (abgeschnittene Stücke, kein sauberes Ende an IEND, RIFF-Länge passt nicht).
function MetadatenPruefen([string]$pfad, [byte[]]$b) {
    $latin = [Text.Encoding]::GetEncoding(28591).GetString($b)
    if ($pfad -match '(?i)\.png$') {
        if (-not (IstPng $b 0)) { Fund $pfad 0 'PNG kaputt (keine PNG-Signatur)' }
        else {
            $r = PngLesen $b 8 $b.Length
            if ($r.Meta) { Fund $pfad 0 "PNG mit Metadaten ($($r.Meta))" }
            if ($r.Kaputt) { Fund $pfad 0 "PNG kaputt ($($r.Kaputt))" }
        }
    } elseif ($pfad -match '(?i)\.jpe?g$') {
        if (-not $latin.StartsWith("$([char]0xFF)$([char]0xD8)", [StringComparison]::Ordinal)) { Fund $pfad 0 'JPEG kaputt (keine JPEG-Signatur)' }
        if ($latin.Contains("Exif`0")) { Fund $pfad 0 'JPEG mit Exif-Daten' }
        # Segmente FF xx mit Länge bis zum Bildbeginn (SOS, FF DA); COM (FF FE) ist ein Kommentar.
        $i = [long]2
        while ($i + 4 -le $b.Length -and $b[$i] -eq 0xFF -and $b[$i + 1] -ne 0xDA) {
            if ($b[$i + 1] -eq 0xFE) { Fund $pfad 0 'JPEG mit Kommentar (COM)'; break }
            $i += 2 + (([long]$b[$i + 2] -shl 8) -bor [long]$b[$i + 3])
        }
    } elseif ($pfad -match '(?i)\.gif$') {
        # Kopf (6), Bildschirm (7), Farbtabelle, dann Blöcke bis 3B: Bild (2C) oder
        # Erweiterung (21) mit Unterblöcken. Kommentar (FE), Klartext (01) und
        # Anwendungen außer NETSCAPE2.0 (Schleife) sind Metadaten.
        $i = [long]13
        if ($b.Length -ge 13 -and ($b[10] -band 0x80)) { $i += 3 * (2 -shl ($b[10] -band 7)) }
        $grund = $null
        if (-not $latin.StartsWith('GIF8')) { $grund = 'GIF kaputt (Kopf)' }
        while (-not $grund) {
            if ($i -ge $b.Length) { $grund = 'GIF kaputt (ohne Ende 3B)'; break }
            if ($b[$i] -eq 0x3B) { break }
            if ($b[$i] -eq 0x21 -and $i + 2 -lt $b.Length) {
                $label = $b[$i + 1]
                if ($label -in 0xFE, 0x01 -or ($label -eq 0xFF -and -not ($i + 14 -le $b.Length -and $latin.Substring($i + 3, 11) -ceq 'NETSCAPE2.0'))) { $grund = 'GIF mit Metadaten (Kommentar, Klartext oder Anwendung)'; break }
                $i += 2
            } elseif ($b[$i] -eq 0x2C -and $i + 10 -lt $b.Length) {
                if ($b[$i + 9] -band 0x80) { $i += 3 * (2 -shl ($b[$i + 9] -band 7)) }
                $i += 11
            } else { $grund = 'GIF kaputt (unbekannter Block)'; break }
            while ($i -lt $b.Length -and $b[$i] -ne 0) { $i += 1 + $b[$i] }
            $i++
        }
        if ($grund) { Fund $pfad 0 $grund }
    } elseif ($pfad -match '(?i)\.pdf$') {
        if (-not $latin.StartsWith('%PDF-', [StringComparison]::Ordinal)) { Fund $pfad 0 'PDF kaputt (keine PDF-Signatur)' }
        if ($latin -match '/(Author|Creator)\s*\(') { Fund $pfad 0 'PDF mit Author oder Creator' }
    } elseif ($pfad -match '(?i)\.webp$') {
        # RIFF: „RIFF“, Länge (ab Byte 8 bis zum Ende), „WEBP“, dann Stücke aus
        # Kennung (4), Länge (4, little endian), Daten (gerade aufgefüllt), bis genau ans Ende.
        if (-not ($b.Length -ge 12 -and $latin.StartsWith('RIFF') -and $latin.Substring(8, 4) -eq 'WEBP')) { Fund $pfad 0 'WebP kaputt (kein RIFF/WEBP-Kopf)' }
        elseif ((Le32 $b 4) + 8 -ne $b.Length) { Fund $pfad 0 'WebP kaputt (die Länge passt nicht zum RIFF-Ende)' }
        else {
            $i = [long]12
            $meta = $false
            while ($i -lt $b.Length) {
                if ($i + 8 -gt $b.Length) { Fund $pfad 0 'WebP kaputt (abgeschnittenes Stück)'; break }
                $kennung = $latin.Substring($i, 4)
                $laenge = Le32 $b ($i + 4)
                $weiter = $i + 8 + $laenge + ($laenge % 2)
                if ($weiter -gt $b.Length) { Fund $pfad 0 'WebP kaputt (abgeschnittenes Stück)'; break }
                if (-not $meta -and $kennung -in 'EXIF', 'XMP ') { Fund $pfad 0 "WebP mit Metadaten ($($kennung.Trim()))"; $meta = $true }
                $i = $weiter
            }
        }
    } elseif ($pfad -match '(?i)\.ico$') {
        # Ein ICO: Kopf (0, Typ 1 oder 2, Zahl der Bilder), je Bild 16 Bytes mit
        # Größe und Lage; die Bilder sind BMP oder ganze PNG. Jedes PNG wird wie oben geprüft.
        if ($b.Length -lt 6 -or $b[0] -ne 0 -or $b[1] -ne 0 -or $b[2] -notin 1, 2 -or $b[3] -ne 0) { Fund $pfad 0 'ICO kaputt (Kopf)' }
        else {
            $zahl = [long]$b[4] -bor ([long]$b[5] -shl 8)
            $verzeichnis = 6 + 16 * $zahl
            if ($zahl -eq 0 -or $verzeichnis -gt $b.Length) { Fund $pfad 0 'ICO kaputt (Verzeichnis abgeschnitten)' }
            else {
                $metaGemeldet = $false
                for ($k = 0; $k -lt $zahl; $k++) {
                    $eintrag = 6 + 16 * $k
                    $groesse = Le32 $b ($eintrag + 8)
                    $lage = Le32 $b ($eintrag + 12)
                    if ($groesse -eq 0 -or $lage -lt $verzeichnis -or $lage + $groesse -gt $b.Length) { Fund $pfad 0 'ICO kaputt (Bild außerhalb der Datei)'; break }
                    if (IstPng $b $lage) {
                        $r = PngLesen $b ($lage + 8) ($lage + $groesse)
                        if ($r.Meta -and -not $metaGemeldet) { Fund $pfad 0 "ICO mit Metadaten ($($r.Meta))"; $metaGemeldet = $true }
                        if ($r.Kaputt) { Fund $pfad 0 "ICO kaputt (eingebettetes PNG: $($r.Kaputt))"; break }
                    }
                }
            }
        }
    }
    if ($latin.Contains('<x:xmpmeta') -or $latin.Contains('http://ns.adobe.com/xap/1.0/')) { Fund $pfad 0 'XMP-Metadaten' }
    # Der ganze Inhalt gegen die Namenslisten: ein Name in einem unbekannten Stück
    # oder Kommentar, als Latin-1 und als UTF-8 (die verlangte Kodierung, sonst
    # fände man einen Umlaut-Namen in Klartext-UTF-8 nie); je Liste ein Fund.
    # Gepackte Daten (IDAT, zTXt, PDF-Streams) sieht das nicht.
    $lesarten = @((Falten $latin), (Falten $utf8.GetString($b)))
    foreach ($k in $listenRx.Keys) {
        $r = if ($k -eq 'KI-Name') { $kiBinaer } else { $listenRx[$k] }
        if ($lesarten | Where-Object { $r.IsMatch($_) }) { Fund $pfad 0 "$k (im Inhalt der Binärdatei)" }
    }
}

function SvgPruefen([string]$pfad, [string]$text) {
    if ($pfad -match '(?i)\.svg$' -and $text -match '(?i)<metadata|inkscape:|sodipodi:|<dc:|<rdf:') { Fund $pfad 0 'SVG mit Metadaten (metadata, inkscape, sodipodi, dc, rdf)' }
}

function BaumPruefen([string]$stand) {
    $ausgabe = Git @('ls-tree', '-r', '-z', '--full-tree', $stand)
    $eintraege = @()
    foreach ($e in ($ausgabe -split "`0")) {
        if (-not $e) { continue }
        $tab = $e.IndexOf("`t")
        $kopf = $e.Substring(0, $tab).Split(' ')
        $eintraege += [pscustomobject]@{ Modus = $kopf[0]; Art = $kopf[1]; Sha = $kopf[2]; Pfad = $e.Substring($tab + 1) }
    }
    if ($eintraege.Count -eq 0) { throw "Der Stand $stand hat keine Dateien." }
    foreach ($e in $eintraege) { [void]$script:imBaum.Add("$($e.Pfad)`0$($e.Sha)") }
    EintraegePruefen $eintraege
    return $eintraege.Count
}

# Was BaumPruefen und VerlaufPruefen je Eintrag (Modus, Art, Sha, Pfad) prüfen:
# Pfad, Art, Binärdatei (Positivliste, Metadaten, Struktur) oder Text.
function EintraegePruefen($eintraege) {
    foreach ($e in $eintraege) {
        PfadPruefen $e.Pfad
        if ($e.Art -ne 'blob') { Fund $e.Pfad 0 'kein normaler Dateieintrag (Submodul?)' }
        if ($e.Modus -eq '120000') { Fund $e.Pfad 0 'Verknüpfung (Symlink)' }
    }
    # Cache: Ein Treffer (genau dieser Blob an genau diesem Pfad) überspringt nur die Inhaltsprüfung; ohne -MitCache ist die Menge leer.
    $offen = [Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
    foreach ($e in $eintraege) { if ($e.Art -eq 'blob' -and -not $script:sauber.Contains("$($e.Sha)`t$($e.Pfad)")) { [void]$offen.Add($e.Sha) } }
    $blobs = Blobs @($offen)
    foreach ($e in $eintraege) {
        if ($e.Art -ne 'blob') { continue }
        $istBinaer = $binaerEndung.IsMatch($e.Pfad)
        if ($istBinaer) {
            $erlaubt = $false
            foreach ($r in $binaerErlaubt) { if ($r.IsMatch($e.Pfad)) { $erlaubt = $true; break } }
            # Die Positivliste der Binärdateien gilt nur im Basis-Repo, die Metadaten überall.
            if (-not $erlaubt -and $script:istBasis) { Fund $e.Pfad 0 'Binärdatei außerhalb der Positivliste'; continue }
        }
        $schluessel = "$($e.Sha)`t$($e.Pfad)"
        if ($script:sauber.Contains($schluessel)) { $script:ausCache++; continue }
        $vorher = $script:funde.Count
        $b = $blobs[$e.Sha]
        if ($istBinaer) { MetadatenPruefen $e.Pfad $b }
        else {
            $text = Utf8Text $b $e.Pfad $true
            SvgPruefen $e.Pfad $text
            TextPruefen $e.Pfad $text $false $e.Pfad
        }
        $script:inhaltGeprueft++
        # Gemerkt wird nur ein Blob ohne Fund; ein Pfad mit Zeilenumbruch passte nicht in eine Zeile der Cache-Datei.
        if ($script:cacheDatei -and $script:funde.Count -eq $vorher -and $e.Pfad.IndexOfAny([char[]]"`r`n") -lt 0) { [void]$script:neuSauber.Add($schluessel) }
    }
}

# Alle Einträge ALLER neuen Commits eines Bereichs alt..neu: Was in einem Zwischenstand stand und an der Spitze
# wieder weg ist, ginge sonst mit der Geschichte hinaus (auch ein Pfad allein,
# etwa projects/<slug>/x.md, das danach unverändert verschoben wurde). Geprüft
# wird jedes Paar (Pfad, Blob) mit seinem echten Modus, das ein neuer Commit
# hinzufügt oder ändert (git log --raw -m --root --no-renames, Merges gegen
# jeden Elternteil): Pfad, Verknüpfung (120000), Gitlink (160000), Binär, Text.
# Was genau so (Pfad und Blob) schon im geprüften Baum steht, nicht noch
# einmal. Ohne „..“ oder mit alt = 0000… (erster Push mit neuer Wurzel) gilt
# die ganze erreichbare Geschichte. Gibt die Zahl der geprüften Paare.
function VerlaufPruefen([string]$bereich) {
    $ziel = $bereich
    if ($bereich -match '\.\.') {
        $teile = $bereich -split '\.\.', 2
        if (-not $teile[0] -or $teile[0] -eq $NULL_SHA) { $ziel = $teile[1] }
    }
    $ausgabe = Git @('log', '--raw', '-m', '--root', '--no-renames', '--no-abbrev', '--format=', '-z', $ziel)
    $stuecke = $ausgabe -split "`0"
    $gesehen = New-Object 'System.Collections.Generic.HashSet[string]'
    $eintraege = New-Object System.Collections.Generic.List[object]
    for ($k = 0; $k -lt $stuecke.Count - 1; $k++) {
        # Je Eintrag „:altmodus neumodus altsha neusha status“, dann der Pfad.
        $kopf = $stuecke[$k].Trim()
        if (-not $kopf.StartsWith(':')) { continue }
        $pfad = $stuecke[$k + 1]
        $k++
        $f = $kopf.Substring(1).Split(' ')
        if ($f.Count -lt 5 -or $f[4].StartsWith('D')) { continue }
        $modus = $f[1]
        $sha = $f[3]
        if ($script:imBaum.Contains("$pfad`0$sha")) { continue }
        if (-not $gesehen.Add("$pfad`0$sha`0$modus")) { continue }
        $eintragsArt = if ($modus -eq '160000') { 'commit' } else { 'blob' }
        $eintraege.Add([pscustomobject]@{ Modus = $modus; Art = $eintragsArt; Sha = $sha; Pfad = $pfad })
    }
    if ($eintraege.Count -gt 0) { EintraegePruefen $eintraege.ToArray() }
    return $eintraege.Count
}

# Beiträge aus Forks (Commits außerhalb der First-Parent-Linie): erlaubte fremde
# Adressen, verankert, nur ASCII (sonst nähme Groß/Klein-egal etwa das Kelvin-Zeichen
# für k). GitHub-noreply mit Login nach GitHub-Regeln (Buchstaben, Ziffern, einzelne
# Striche innen, bis 39 Zeichen) oder eine erfundene Adresse auf .invalid oder .example.
$beitragsMail = Rx ('^(?:(?:[0-9]+\+)?[a-z0-9](?:[a-z0-9]|-(?=[a-z0-9])){0,38}@users\.noreply\.github\.com' +
    '|[a-z0-9._+-]+@(?:[a-z0-9](?:[a-z0-9-]*[a-z0-9])?\.)+(?:invalid|example))\z')
# Prüfpfade (M1): Ein fremder Commit ändert sie nie. Ganz scripts/ (Prüfer mit
# Positivlisten, Wurzeldateien und Listenverarbeitung, Hooks, Hochladen, Freigeben,
# Fassung-Festschreiben, dazu Skripte, die mit Rechten laufen) und die Gegentests
# des Prüfers; eine engere Liste vergäße still die nächste Datei, die er liest.
# Dazu .cargo/: runner, rustc-wrapper und Aliase führen beim nächsten Bau Code aus.
# Dazu .github/ (das Formular für Merge-Angebote und, falls die Positivliste je wächst,
# Workflows) und KONTAKT-PGP.asc: Ein Beitrag tauscht weder Vorlage noch Kontaktschlüssel.
# CONTRIBUTING.md bleibt frei: ein Textbeitrag; Regeln, die gelten, setzt dieser Prüfer durch.
# Gilt in jeder Art. Im Projekt-Repo liegen diese Pfade ohnehin außerhalb der Positivliste;
# dashboard-version.toml dort bleibt frei, sie nennt nur die Fassung (Abweichung ist ein Hinweis).
# Dazu jede .gitattributes, auch verschachtelt: Sie legt fest, wie scripts/ und tests/
# ausgecheckt werden (eol, filter), und mit -diff zeigte git diff eine Datei nur als
# „Binary files differ“, das Lesen vor dem Bau sähe nichts. Dazu jede .npmrc: web/.npmrc
# hält mit ignore-scripts=true die Paket-Skripte beim npm ci aus.
# Dazu jeder Pfad mit einem 8.3-Kurznamen (CARGO~1/…): Unter Windows landet er beim
# Auschecken in .cargo/ oder einer anderen Datei mit langem Namen.
# Groß/Klein egal (Rx): Unter Windows landet SCRIPTS/x.ps1 im selben Ordner wie scripts/.
# Cargo.toml und web/package.json sind bewusst keine Prüfpfade (sie ändern Beiträge
# regelmäßig); was dort im Bau läuft, deckt das Lesen des Diffs vor dem Bau (M2).
# Grenze: Fremd heißt hier nur „Angabe nicht genau Ranke“. Ein Fork-Commit mit gefälschter
# Ranke-Angabe ist für den Prüfer ein eigener; im Probelauf meldet ihn -Beitrag.
$pruefPfad = Rx '^(?:scripts/|\.cargo/|\.github/|tests/vor_push\.rs$|KONTAKT-PGP\.asc$)|(?:^|/)\.(?:gitattributes|npmrc)$|(?:^|/)[^/~]{1,6}~[0-9]+(?:\.[^/]{0,3})?(?:/|$)'

# Was ein Commit selbst ändert: ohne Eltern alles, sonst die Pfade, die sich
# gegen JEDEN Elternteil unterscheiden. Was ein Merge unverändert von einem
# Elternteil übernimmt, prüft der Commit, der es dort eingeführt hat (oder es
# steht schon auf dem fernen main); nur was in keinem Elternteil so stand, bringt
# der Merge selbst (gegen nur den ersten Elternteil zählte im Fork jede
# Änderung von main als fremd).
function EigenePfade([string]$c) {
    $eltern = @(((Git @('log', '-1', '--format=%P', $c)).Trim() -split ' ') | Where-Object { $_ })
    if ($eltern.Count -eq 0) { return @((Git @('diff-tree', '-r', '--root', '--no-renames', '--no-commit-id', '--name-only', '-z', $c)) -split "`0" | Where-Object { $_ }) }
    $menge = $null
    foreach ($p in $eltern) {
        $jetzt = @((Git @('diff-tree', '-r', '--no-renames', '--name-only', '-z', $p, $c)) -split "`0" | Where-Object { $_ })
        $menge = if ($null -eq $menge) { $jetzt } else { @($menge | Where-Object { $jetzt -ccontains $_ }) }
    }
    return $menge
}

function CommitsPruefen([string]$bereich, [bool]$nurMain) {
    $liste = @((Git @('rev-list', $bereich)) -split "`n" | Where-Object { $_ })
    # Die Linie: die First-Parent-Kette des Bereichs (die Merges des Betreuers auf
    # main; beim Hook und Hochladen fern..main, bei -Pruefen und je Ref eines Bundles
    # die ganze Kette dieser Ref). Alles andere kam über zweite Eltern (Seitenzweige, Forks).
    $linie = @((Git @('rev-list', '--first-parent', $bereich)) -split "`n" | Where-Object { $_ })
    $aufLinie = New-Object 'System.Collections.Generic.HashSet[string]'
    foreach ($c in $linie) { [void]$aufLinie.Add($c) }
    foreach ($c in $liste) {
        $info = Git @('log', '-1', '--format=%an%x00%ae%x00%cn%x00%ce', $c)
        $f = $info -split "`0", 4
        $f[3] = $f[3].TrimEnd("`n")
        $kurz = $c.Substring(0, 10)
        $fremd = $false
        $erlaubt = @{}
        foreach ($rolle in @(@('Autor', 'author', 0), @('Committer', 'committer', 2))) {
            $name = $f[$rolle[2]]; $mail = $f[$rolle[2] + 1]
            # Genau, Zeichen für Zeichen: auch ein breites ＠ (U+FF20) ist ein Fund.
            if ($name -ceq $RANKE_NAME -and $mail -ceq $RANKE_MAIL) { $erlaubt[$rolle[1]] = "$RANKE_NAME <$RANKE_MAIL>"; continue }
            if ($aufLinie.Contains($c)) { Fund "Commit $kurz" 0 "$($rolle[0]) ist nicht $RANKE_NAME <$RANKE_MAIL>"; continue }
            if ($mail -cnotmatch '^[\x21-\x7E]+$' -or -not $beitragsMail.IsMatch($mail)) {
                Fund "Commit $kurz" 0 "$($rolle[0]) ist nicht $RANKE_NAME <$RANKE_MAIL> und hat keine Beitragsadresse (GitHub-noreply oder auf .invalid oder .example)"
                continue
            }
            $fremd = $true
            $erlaubt[$rolle[1]] = "$name <$mail>"
            # Der Name wie jeder Text (Listen, KI-Namen im Basis-Repo, Steuerzeichen), dazu KI-Namen in jeder Art wie in Nachrichten.
            # Die ganze Adresse ohne die feste noreply-Endung (auch die Domäne einer .invalid-Adresse); das @ als Leerzeichen, sonst schlüge das Adressmuster an.
            TextPruefen "Commit $kurz ($($rolle[0]))" "$name`n$($mail -replace '@users\.noreply\.github\.com$', '' -replace '@', ' ')" $false $null
            if ($anhang.IsMatch((Falten $name))) { Fund "Commit $kurz ($($rolle[0]))" 0 'Anhang oder KI-Name im Namen' }
        }
        # M1: Änderungen am Prüfer kommen nie aus einem fremden Commit.
        if ($fremd) {
            foreach ($p in (EigenePfade $c)) {
                if ($pruefPfad.IsMatch($p)) { Fund "Commit $kurz" 0 "ein fremder Commit ändert den Prüfpfad $p; Änderungen am Prüfer übernimmt der Betreuer selbst, in einem eigenen Merge ohne fremde Commits" }
            }
        }
        # Das rohe Objekt: Die Nachricht ganz, auch hinter
        # einem NUL (an dem %B abbricht); Kopfzeilen bis zur ersten Leerzeile.
        $roh = Git @('cat-file', 'commit', $c)
        $trenn = $roh.IndexOf("`n`n")
        $meldung = if ($trenn -ge 0) { $roh.Substring($trenn + 2) } else { '' }
        $kopfteil = if ($trenn -ge 0) { $roh.Substring(0, $trenn) } else { $roh }
        TextPruefen "Commit $kurz (Nachricht)" $meldung $false $null
        # Auch hier, nicht nur im Hook: --no-verify, cherry-pick, rebase oder ein Klon ohne hooksPath umgehen ihn.
        if (($meldung -split "`n")[0] -cnotmatch $ccKopf) { Fund "Commit $kurz (Nachricht)" 1 'erste Zeile nicht nach Conventional Commits' }
        AnhangPruefen "Commit $kurz (Nachricht)" $meldung
        # Jeder Trailer, den git selbst erkennt (Reviewed-by, Change-Id, …).
        if ((Git @('log', '-1', '--format=%(trailers:only,unfold)', $c)).Trim()) { Fund "Commit $kurz (Nachricht)" 0 'Anhang (Trailer) in der Nachricht' }
        # Nur die Kopfzeilen tree, parent, author, committer (encoding nur UTF-8):
        # eine Signatur (gpgsig), ein mergetag, eine eigene Kopfzeile oder eine Fortsetzungszeile
        # (mit Leerzeichen am Anfang; erlaubt sind nur einzeilige Kopfzeilen) ginge sonst ungeprüft hinaus.
        foreach ($k in ($kopfteil -split "`n")) {
            if ($k -cnotmatch '^(?:tree|parent|author|committer) ' -and $k -notmatch '^encoding utf-?8$') { Fund "Commit $kurz" 0 'Kopfzeile außer tree, parent, author, committer (etwa Signatur)'; break }
            # Die Kopfzeile selbst (handgebaute Objekte): genau die oben erlaubte Angabe (Ranke oder der Beitrag), Zeit, Zone.
            if ($k -cmatch '^(author|committer) ') {
                $soll = $erlaubt[$Matches[1]]
                if (-not $soll) { $soll = "$RANKE_NAME <$RANKE_MAIL>" }
                if ($k -cnotmatch ('^(?:author|committer) ' + [regex]::Escape($soll) + ' \d+ [+-]\d{4}$')) { Fund "Commit $kurz" 0 "Kopfzeile author oder committer ist nicht genau $RANKE_NAME <$RANKE_MAIL> oder die geprüfte Beitragsangabe" }
            }
        }
    }
    # M1 auch auf der Linie: Holt ein Merge fremde Commits herein (irgendeiner in L^1..L
    # nicht genau Ranke), ändert er gegenüber seinem ersten Elternteil keinen Prüfpfad.
    # Sonst brächte ein fremder Merge mit einem alten main-Stand als Elternteil einen
    # alten Prüfer zurück, ohne dass ein einzelner Commit ihn gegen alle Eltern ändert.
    $ranke = "$RANKE_NAME`0$RANKE_MAIL`0$RANKE_NAME`0$RANKE_MAIL"
    foreach ($c in $linie) {
        $eltern = @(((Git @('log', '-1', '--format=%P', $c)).Trim() -split ' ') | Where-Object { $_ })
        if ($eltern.Count -lt 2) { continue }
        $wer = @((Git @('log', '--format=%an%x00%ae%x00%cn%x00%ce', "$($eltern[0])..$c")) -split "`n" | Where-Object { $_ })
        if (-not @($wer | Where-Object { $_ -cne $ranke }).Count) { continue }
        foreach ($p in @((Git @('diff-tree', '-r', '--no-renames', '--name-only', '-z', $eltern[0], $c)) -split "`0" | Where-Object { $_ })) {
            if ($pruefPfad.IsMatch($p)) { Fund "Commit $($c.Substring(0, 10))" 0 "der Merge übernimmt fremde Commits und ändert den Prüfpfad $p; Änderungen am Prüfer übernimmt der Betreuer selbst, in einem eigenen Merge ohne fremde Commits" }
        }
    }
    if ($nurMain) {
        # main setzt die bisherige Linie fort: Der älteste neue Commit der Linie hat den
        # alten Stand als ersten Elternteil (sonst etwa ein Merge falsch herum, und ein
        # fremder Commit, der schon draußen liegt, rückte auf die Linie).
        if ($bereich -match '^(.+)\.\.(.+)$' -and $linie.Count) {
            $alt = (Git @('rev-parse', "$($Matches[1])^{commit}")).Trim()
            $erster = (((Git @('log', '-1', '--format=%P', $linie[-1])).Trim() -split ' ') | Select-Object -First 1)
            if ($erster -ne $alt) { Fund "Commit $($linie[-1].Substring(0, 10))" 0 'main setzt die bisherige Linie nicht fort (erster Elternteil ist nicht der alte Stand von main, etwa ein Merge falsch herum)' }
        }
        foreach ($c in $linie) {
            $eltern = @(((Git @('log', '-1', '--format=%P', $c)).Trim() -split ' ') | Where-Object { $_ })
            if ($eltern.Count -eq 1) { Fund "Commit $($c.Substring(0, 10))" 0 'auf main nur Merge-Commits (kein Fast-Forward)' }
        }
        # main hat genau eine Wurzel: Nur beim ersten Push (Bereich ohne „..“, wie in RefPruefen) darf der älteste Commit der Linie elternlos sein.
        $eineWurzel = if ($bereich -notmatch '\.\.' -and $linie.Count) { $linie[-1] } else { '' }
        foreach ($c in @((Git @('rev-list', '--max-parents=0', $bereich)) -split "`n" | Where-Object { $_ })) {
            if ($c -ne $eineWurzel) { Fund "Commit $($c.Substring(0, 10))" 0 'Commit ohne Eltern: eine zweite Wurzel auf main ist nicht erlaubt; die Änderungen auf dem bestehenden main aufbauen (Zweig von main, dann Merge) statt eine fremde oder neue Geschichte einzumischen' }
        }
    }
    return $liste.Count
}

# Teil 3: ein Tag nur leicht, nur neu, lokal und fern derselbe Name, auf der
# First-Parent-Linie eines main aus $script:mainStaende. Sein Commit prüft der
# Push von main (oder es ist schon draußen).
function TagPruefen([string]$lokal, [string]$lokalSha, [string]$fern, [string]$fernSha) {
    if ($lokalSha -eq $NULL_SHA) { Fund "Ref $fern" 0 'Löschen eines Tags ist nicht erlaubt'; return }
    TextPruefen "Ref $lokal" $lokal $true $null
    if ($lokal -cne $fern) { TextPruefen "Ref $fern" $fern $true $null; Fund "Ref $lokal" 0 "Tag lokal unter anderem Namen als fern ($(Maskiert $fern))" }
    if ($fernSha -ne $NULL_SHA) { Fund "Ref $fern" 0 'Tag gibt es fern schon (kein Verschieben)' }
    if ((Git @('cat-file', '-t', $lokalSha)).Trim() -ne 'commit') {
        # Git liest dabei den Tag-Text streng als UTF-8.
        [void](Git @('cat-file', '-p', $lokalSha))
        Fund "Ref $lokal" 0 'Tag-Objekt (git tag -a/-s); nur leichte Tags, ohne Tagger, Text und Signatur'
        return
    }
    # Teil 4: Der getaggte Commit ist die Fassung selbst (Cargo.toml und CHANGELOG.txt im selben Commit gesetzt).
    $fassung = $fern.Substring('refs/tags/v'.Length)
    $da = @((Git @('ls-tree', '--name-only', $lokalSha, '--', 'Cargo.toml', 'CHANGELOG.txt')) -split '\r?\n')
    # Kein TOML-Parser: genau eine version-Zeile unter [package] zählt; eine zweite (etwa in einem
    # mehrzeiligen String) oder version.workspace ist ein Fund, [[bin]] usw. beendet [package].
    $versionen = @()
    if ($da -ccontains 'Cargo.toml') {
        $abschnitt = ''
        foreach ($z in ((Git @('show', "$($lokalSha):Cargo.toml")) -split '\r?\n')) {
            if ($z -match '^\s*\[\[?([^\]]*)\]\]?\s*(?:#.*)?$') { $abschnitt = $Matches[1].Trim(); continue }
            if ($abschnitt -ceq 'package' -and $z -match '^\s*"?version\b') { $versionen += $z }
        }
    }
    if ($versionen.Count -ne 1 -or $versionen[0] -cnotmatch ('^\s*version\s*=\s*"' + [regex]::Escape($fassung) + '"\s*(?:#.*)?$')) {
        Fund "Ref $lokal" 0 "Tag passt nicht zur Version: Cargo.toml im getaggten Commit braucht unter [package] genau eine Zeile version = `"$fassung`" (im Commit chore(release): v$fassung setzen)"
    }
    # Wie der Wächter: Überschrift ist eine Zeile mit Strichen darunter; die oberste nach Unreleased ist die Fassung.
    $oben = $null
    if ($da -ccontains 'CHANGELOG.txt') {
        $cl = @((Git @('show', "$($lokalSha):CHANGELOG.txt")) -split '\r?\n')
        for ($i = 0; $i + 1 -lt $cl.Count; $i++) {
            if ($cl[$i].Trim() -and $cl[$i].TrimEnd() -cne 'Unreleased' -and $cl[$i + 1] -cmatch '^-{3,}$') { $oben = $cl[$i].TrimEnd(); break }
        }
    }
    $tag = [datetime]::MinValue
    if (-not ($oben -cmatch ('^' + [regex]::Escape($fassung) + ' - ([0-9]{4}-[0-9]{2}-[0-9]{2})$') -and
            [datetime]::TryParseExact($Matches[1], 'yyyy-MM-dd', [cultureinfo]::InvariantCulture, [Globalization.DateTimeStyles]::None, [ref]$tag))) {
        Fund "Ref $lokal" 0 "CHANGELOG.txt im getaggten Commit ohne Abschnitt `"$fassung - JJJJ-MM-TT`" als oberste Fassung (gültiges Datum, darunter ----------; Unreleased im Commit chore(release): v$fassung umbenennen)"
    }
    if ($script:mainStaende.Count -eq 0) { Fund "Ref $lokal" 0 'Tag ohne main (weder im Push noch fern); geprüft wird gegen main'; return }
    foreach ($m in $script:mainStaende) {
        # --ignore-missing: ein fernes main, das lokal fehlt, ist ein Fund statt Exit 2.
        if (@((Git @('rev-list', '--ignore-missing', '--first-parent', $m)) -split "`n") -contains $lokalSha) { return }
    }
    Fund "Ref $lokal" 0 'Tag nicht auf der First-Parent-Linie von main (Merge-Commits von main oder der Wurzel-Commit)'
}

function RefPruefen([string]$lokal, [string]$lokalSha, [string]$fern, [string]$fernSha) {
    if ($fern -cmatch $TAG_NAME) { TagPruefen $lokal $lokalSha $fern $fernSha; return }
    if ($fern -ne 'refs/heads/main') { Fund "Ref $fern" 0 'hinaus darf nur refs/heads/main oder ein Tag refs/tags/v0.MINOR.PATCH (keine anderen Zweige, Tags nur v0.x)'; return }
    if ($lokal -cnotin $LOKAL_ERLAUBT) { Fund "Ref $lokal" 0 'nur der lokale Zweig main geht hinaus' }
    if ($lokalSha -eq $NULL_SHA) { Fund "Ref $fern" 0 'Löschen von main ist nicht erlaubt'; return }
    # Auch keine Ref auf ein Tag-Objekt. Git liest dabei den Tag-Text streng als UTF-8.
    if ((Git @('cat-file', '-t', $lokalSha)).Trim() -ne 'commit') {
        [void](Git @('cat-file', '-p', $lokalSha))
        Fund "Ref $lokal" 0 'zeigt auf kein Commit (etwa ein Tag-Objekt)'
    }
    TextPruefen "Ref $lokal" $lokal $true $null
    $bereich = if ($fernSha -eq $NULL_SHA) { $lokalSha } else { "$fernSha..$lokalSha" }
    [void](CommitsPruefen $bereich $true)
    [void](BaumPruefen $lokalSha)
    [void](VerlaufPruefen $bereich)
}

# ---------------------------------------------------------------------------
# Ablauf
# ---------------------------------------------------------------------------
$hookZeilen = @()
if ($Hook) {
    while ($null -ne ($zeile = [Console]::In.ReadLine())) { if ($zeile.Trim()) { $hookZeilen += $zeile.Trim() } }
    if ($hookZeilen.Count -eq 0) { throw 'Der Hook bekam keine Ref-Zeilen.' }
    foreach ($z in $hookZeilen) { if (@($z -split '\s+').Count -ne 4) { throw "Unerwartete Ref-Zeile vom Hook." } }
}
if (-not ($Hook -or $Commits -or $Baum -or $Pruefen -or $Bundle -or $Beitrag)) { throw 'Nichts zu prüfen: -Hook, -Baum, -Commits, -Pruefen, -Bundle oder -Beitrag angeben.' }

# Art des Repos: nur aus der Adresse des Remotes und repos.toml, nie aus dem
# geprüften Baum. Ohne Adresse, mit unbekannter Adresse oder ohne repos.toml:
# Basis (geschlossen scheitern). Die Adresse selbst steht nie in der Ausgabe.
$reposDatei = Join-Path $Inhalt 'pruefer\repos.toml'
$reposListe = ReposLaden $reposDatei
$artAusAdresse = 'Basis'
if ($null -eq $reposListe) {
    Write-Host "HINWEIS Die Liste $(OhneProfil $reposDatei) fehlt; geprüft wird als Basis-Repo."
} elseif ($RemoteUrl -and $RemoteUrl.Trim()) {
    $adresseNormal = AdresseNormal $RemoteUrl
    if ($reposListe.ContainsKey($adresseNormal)) { $artAusAdresse = $reposListe[$adresseNormal] }
    else { Write-Host 'HINWEIS Die Adresse des Remotes steht nicht in repos.toml; geprüft wird als Basis-Repo.' }
}
if ($Art -and $Art -ne $artAusAdresse) {
    Write-Host "FEHLER -Art $Art passt nicht zur Art aus der Adresse des Remotes und repos.toml ($artAusAdresse). Es geht nichts hinaus."
    Ende 2
}
$script:istBasis = ($artAusAdresse -eq 'Basis')
$script:istProjekt = ($artAusAdresse -eq 'Projekt')
# Ausnahmedatei: nur in der Art
# Projekt, nur für die Adresse aus repos.toml, je Eintrag einzeln abgenommen.
# Ein Eintrag nimmt nur einen gleich gefalteten Wert aus sensibel oder
# nur_basis heraus, nie einen Wortteil. Einer ohne Listeneintrag ist ein Fund:
# Ein Tippfehler wirkte sonst still nicht, und ein späterer Listeneintrag wäre still ausgenommen.
if ($script:istProjekt) {
    $repoName = (AdresseNormal $RemoteUrl) -replace '^.*[/\\:]', ''
    $ausnahmeDatei = Join-Path $Inhalt "pruefer\ausnahmen\$repoName.toml"
    if ($repoName -match '^[A-Za-z0-9][A-Za-z0-9._-]*$' -and (Test-Path -LiteralPath $ausnahmeDatei -PathType Leaf)) {
        # Kaputt: Abbruch wie bei den Listen; die Meldung nennt den Namen des Repos nicht.
        try { $erlaubt = @((ListenLaden $ausnahmeDatei @('erlaubt') @()).erlaubt) }
        catch { throw $_.Exception.Message.Replace($ausnahmeDatei, 'pruefer\ausnahmen\<repo>.toml') }
        $schluessel = { param($w) ((Falten $w) -split "$trenner+" | Where-Object { $_ }) -join ' ' }
        $bekannt = New-Object 'System.Collections.Generic.HashSet[string]'
        foreach ($w in @($listen.sensibel) + @($listen.nur_basis)) { [void]$bekannt.Add((& $schluessel $w)) }
        $weg = New-Object 'System.Collections.Generic.HashSet[string]'
        for ($i = 0; $i -lt $erlaubt.Count; $i++) {
            $s = & $schluessel $erlaubt[$i]
            if ($bekannt.Contains($s)) { [void]$weg.Add($s) } else { Fund 'Ausnahmedatei' 0 "Ausnahme ohne Listeneintrag, Nr. $($i + 1)" }
        }
        $script:maske = $listenRx['sensibler Name']
        # Das Muster trifft Wortteile; ein ausgenommener Wert bleibt darum
        # im Muster und ist nur alleinstehend frei. Mit Buchstabe oder Ziffer daneben (gefaltet)
        # bleibt die Zusammensetzung ein Fund, überall, wo listenRx gilt (Pfade, Text, Commits, Binär, Maskiert).
        # \p{M}: Marke, die NFC nicht zusammenzieht; \p{Cs}: Buchstabe oder Ziffer außerhalb der BMP (sperrt auch Emoji).
        $frei = ListenMuster @($listen.sensibel | Where-Object { $weg.Contains((& $schluessel $_)) })
        $lm = ListenMuster @($listen.sensibel | Where-Object { -not $weg.Contains((& $schluessel $_)) })
        if ($frei) { $lm = (@($lm) + "(?:$frei)(?=[\p{L}\p{M}\p{N}\p{Cs}])|(?<=[\p{L}\p{M}\p{N}\p{Cs}])(?:$frei)" | Where-Object { $_ }) -join '|' }
        if ($lm) { $listenRx['sensibler Name'] = Rx ('(?:' + $lm + ')') } else { $listenRx.Remove('sensibler Name') }
        Write-Host "Ausnahmen für dieses Repo: $($weg.Count) angewandt."
    }
}
$artText = switch ($artAusAdresse) { 'Basis' { 'Basis-Repo' } 'Projekt' { 'Projekt-Repo' } default { 'Inhalts-Repo' } }
if ($script:istBasis) {
    # Namen, die nur im Basis-Repo verboten sind (in Projekten und im Inhalt erlaubt).
    if ($listen.nur_basis.Count -eq 0) { throw "Die Liste nur_basis in der Namensliste ist leer oder fehlt. Im Basis-Repo prüft der Prüfer ohne sie nicht." }
    $listenRx['Name, der nur im Basis-Repo verboten ist'] = Rx ('(?:' + (ListenMuster $listen.nur_basis) + ')')
    # KI-Namen wie ein Listenwert (Pfade, Text, über Zeilen, Binär, Maskiert), nur hier: In Projekten
    # sind es oft Fachwörter. Davor kein Buchstabe, keine Ziffer, kein + (Base64-Inneres); ein / davor
    # trifft (Pfade), darum fallen Prüfsummen sha512-… vorher weg ($herkunft).
    $listenRx['KI-Name'] = Rx "(?<![a-z0-9+])(?:$kiNamen)"
}

# ---------------------------------------------------------------------------
# Cache sauberer Blobs: nur mit -MitCache (Vorprüfung in Hochladen.ps1), nie im Hook
# ---------------------------------------------------------------------------
# Die letzte Schranke prüft immer kalt: Ein vergifteter Eintrag täuscht höchstens eine
# Vorprüfung, nie den Push. Eine Datei je Regel-Schlüssel, als Menge geladen: Eine Datei
# mit einigen tausend Zeilen liest sich schneller als ebenso viele kleine Dateien.
$script:sauber = [Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
$script:neuSauber = [Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
$script:cacheDatei = $null
$script:ausCache = 0
$script:inhaltGeprueft = 0
$cacheZeile = [regex]::new('^[0-9a-f]{40}(?:[0-9a-f]{24})?\t[^\r\n]+$', [Text.RegularExpressions.RegexOptions]'Compiled, CultureInvariant')

# SHA-256 über alles, wovon die Inhaltsprüfung abhängt (der Slug eines Projekt-Repos nicht: Er gilt nur in PfadPruefen).
function RegelSchluessel {
    $strom = [IO.MemoryStream]::new()
    $teil = { param([string]$name, [byte[]]$b) $k = $utf8.GetBytes("$name`n$($b.Length)`n"); $strom.Write($k, 0, $k.Length); $strom.Write($b, 0, $b.Length) }
    & $teil 'regeln' $utf8.GetBytes("art=$artAusAdresse grenze=$GRENZE ps=$($PSVersionTable.PSVersion.Major).$($PSVersionTable.PSVersion.Minor) net=$([Environment]::Version)")
    foreach ($d in $script:prueferDateien) { & $teil "pruefer/$([IO.Path]::GetFileName($d))" ([IO.File]::ReadAllBytes($d)) }
    foreach ($k in $script:gelesen.Keys) { & $teil "liste/$k" $script:gelesen[$k] }
    return [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($strom.ToArray())).ToLowerInvariant()
}

# Liegt auf dem Weg zu $pfad (so weit er besteht) eine Verknüpfung (Junction, Symlink, Reparse-Point)?
function MitVerknuepfung([string]$pfad) {
    for ($p = [IO.Path]::GetFullPath($pfad); $p; $p = [IO.Path]::GetDirectoryName($p)) {
        if (([IO.File]::Exists($p) -or [IO.Directory]::Exists($p)) -and ([IO.File]::GetAttributes($p) -band [IO.FileAttributes]::ReparsePoint)) { return $true }
    }
    return $false
}

if ($MitCache -and $Hook) { Write-Host 'HINWEIS -MitCache gilt nicht im Hook; die letzte Schranke prüft kalt.' }
elseif ($MitCache) {
    $cacheOrt = if ($env:DASHBOARD_PRUEFER_CACHE) { $env:DASHBOARD_PRUEFER_CACHE } elseif ($env:LOCALAPPDATA) { Join-Path $env:LOCALAPPDATA 'Dashboard\cache\pruefer' } else { $null }
    try {
        if (-not $cacheOrt) { Write-Host 'HINWEIS Ohne LOCALAPPDATA gibt es keinen Prüfer-Cache; geprüft wird kalt.' }
        else {
            $regelName = RegelSchluessel
            $regelOrdner = Join-Path $cacheOrt $regelName
            $datei = Join-Path $regelOrdner 'sauber.txt'
            if (-not (MitVerknuepfung $datei)) { [void][IO.Directory]::CreateDirectory($regelOrdner) }
            if (MitVerknuepfung $datei) { Write-Host 'HINWEIS Im Pfad des Prüfer-Caches liegt eine Verknüpfung (Junction oder Symlink); geprüft wird kalt.' }
            else {
                $script:cacheDatei = $datei
                [IO.Directory]::SetLastWriteTimeUtc($regelOrdner, [DateTime]::UtcNow)
                # Unlesbar oder kein UTF-8: kein Treffer; eine Zeile anderer Form auch nicht.
                if ([IO.File]::Exists($datei)) {
                    try { foreach ($z in $strikt.GetString([IO.File]::ReadAllBytes($datei)).Split("`n")) { if ($cacheZeile.IsMatch($z)) { [void]$script:sauber.Add($z) } } }
                    catch { $script:sauber.Clear() }
                }
                # Zwei Regel-Ordner bleiben: dieser (nie gelöscht, auch wenn ein anderer jünger datiert ist) und der
                # zuletzt genutzte andere. Gelöscht wird nur, was wie ein Regel-Ordner aussieht und nur Cache-Dateien enthält.
                $alt = @([IO.DirectoryInfo]::new($cacheOrt).GetDirectories() |
                    Where-Object { $_.Name -cne $regelName -and $_.Name -cmatch '^[0-9a-f]{64}$' -and -not ($_.Attributes -band [IO.FileAttributes]::ReparsePoint) -and
                        @($_.GetFileSystemInfos() | Where-Object { $_ -isnot [IO.FileInfo] -or $_.Name -cnotmatch '^sauber\.txt(?:\.[0-9a-f]{32}\.tmp)?$' }).Count -eq 0 } |
                    Sort-Object LastWriteTimeUtc -Descending | Select-Object -Skip 1)
                foreach ($d in $alt) { try { $d.Delete($true) } catch { } }
            }
        }
    } catch {
        $script:cacheDatei = $null
        $script:sauber.Clear()
        Write-Host "HINWEIS Der Prüfer-Cache ist nicht nutzbar ($(OhneProfil $_.Exception.Message)); geprüft wird kalt."
    }
}

# Am Ende eines Laufs ohne jeden Fund: die neuen sauberen Blobs dazu, atomar (temporäre Datei, dann Move).
function CacheSchreiben {
    if (-not $script:cacheDatei -or $script:funde.Count -gt 0 -or $script:neuSauber.Count -eq 0) { return }
    $tmp = "$($script:cacheDatei).$([guid]::NewGuid().ToString('N')).tmp"
    try {
        if (MitVerknuepfung $script:cacheDatei) { throw 'Verknüpfung im Pfad' }
        $script:sauber.UnionWith($script:neuSauber)
        [IO.File]::WriteAllText($tmp, ([string]::Join("`n", [string[]]@($script:sauber)) + "`n"), $utf8)
        [IO.File]::Move($tmp, $script:cacheDatei, $true)
    } catch {
        try { [IO.File]::Delete($tmp) } catch { }
        Write-Host 'HINWEIS Der Prüfer-Cache ließ sich nicht schreiben; der nächste Lauf prüft diese Blobs wieder.'
    }
}

# Teil 3: main für die Tags, zuerst was dieser Push nach main schickt, dann was
# fern schon main ist; ohne main-Zeile fragt der Prüfer das Remote selbst. Nie die
# Remote-Tracking-Ref: die ist lokal frei setzbar (update-ref, fetch von woanders),
# und die Geschichte hinter dem Tag ginge ungeprüft hinaus.
$mainZeilen = @($hookZeilen | ForEach-Object { , ($_ -split '\s+') } | Where-Object { $_[2] -ceq 'refs/heads/main' })
foreach ($t in $mainZeilen) { if ($t[1] -ne $NULL_SHA) { $script:mainStaende += $t[1] } }
foreach ($t in $mainZeilen) { if ($t[3] -ne $NULL_SHA) { $script:mainStaende += $t[3] } }
if ($Hook -and $mainZeilen.Count -eq 0 -and $RemoteUrl) {
    foreach ($l in ((Git @('ls-remote', $RemoteUrl, 'refs/heads/main')) -split "`n")) {
        $t = $l.Trim() -split '\s+'
        if ($t.Count -eq 2 -and $t[1] -ceq 'refs/heads/main') { $script:mainStaende += $t[0] }
    }
}
if ($script:istProjekt) {
    # Hat das Remote schon main, steht der Slug fest: Sonst tauschte ein Merge, der den alten Ordner löscht, das Projekt still aus.
    $fernStaende = @($mainZeilen | Where-Object { $_[3] -ne $NULL_SHA } | ForEach-Object { $_[3] })
    if ($Commits -match '^([^.]+)\.\.[^.]') { $fernStaende += $Matches[1] }
    foreach ($s in $fernStaende) {
        # -z: ohne es setzt git einen Namen mit Sonderzeichen in Anführungszeichen (anderer Slug).
        $ordner = @((Git @('ls-tree', '-d', '--name-only', '-z', $s, 'projects/')) -split "`0" | Where-Object { $_ })
        if (-not $script:projektSlug -and $ordner.Count -gt 0) { $script:projektSlug = $ordner[0].Substring(9) }
    }
}
foreach ($z in $hookZeilen) {
    $t = $z -split '\s+'
    RefPruefen $t[0] $t[1] $t[2] $t[3]
}
if ($Pruefen) {
    # Probelauf ohne Push: die lokale Ref wie ein erster Push nach main.
    $voll = if ($Ref.StartsWith('refs/')) { $Ref } else { "refs/heads/$Ref" }
    $sha = (Git @('rev-parse', '--verify', "$voll^{commit}")).Trim()
    RefPruefen $voll $sha 'refs/heads/main' $NULL_SHA
    Write-Host "Geprüft: $(Maskiert $voll) wie ein erster Push nach refs/heads/main (Probelauf, kein Push)."
}
if ($Bundle) {
    # Das ganze Bundle: in ein Wegwerf-Repo holen, dann jede Ref
    # wie ein erster Push nach main, dazu Objekte, die keine Ref erreicht.
    $datei = (Resolve-Path -LiteralPath $Bundle).Path
    $script:wegwerf = Join-Path ([IO.Path]::GetTempPath()) "vor-push-bundle-$([guid]::NewGuid().ToString('N'))"
    [void](Git @('init', '-q', '--bare', $script:wegwerf))
    $Repo = $script:wegwerf
    # Ein Bundle mit Voraussetzungen (nicht ab der Wurzel) lässt git hier scheitern: Exit 2.
    [void](Git @('fetch', '-q', $datei, 'refs/*:refs/*'))
    $koepfe = @((Git @('bundle', 'list-heads', $datei)) -split "`n" | Where-Object { $_.Trim() } | ForEach-Object { , ($_.Trim() -split ' ', 2) })
    if ($koepfe.Count -eq 0) { throw 'Das Bundle hat keine Ref.' }
    # Tags des Bundles gegen seine Ref main.
    $script:mainStaende = @($koepfe | Where-Object { $_[1] -cin $LOKAL_ERLAUBT } | ForEach-Object { $_[0] })
    foreach ($k in $koepfe) {
        if ($k[1] -ceq 'HEAD') {
            if (-not @($koepfe | Where-Object { $_[1] -cne 'HEAD' -and $_[0] -eq $k[0] }).Count) { Fund 'Ref HEAD' 0 'HEAD im Bundle zeigt auf keinen Zweig des Bundles' }
            continue
        }
        if ($k[1].StartsWith('refs/tags/', [StringComparison]::Ordinal)) { RefPruefen $k[1] $k[0] $k[1] $NULL_SHA; continue }
        RefPruefen $k[1] $k[0] 'refs/heads/main' $NULL_SHA
    }
    $alle = @((Git @('cat-file', '--batch-all-objects', '--batch-check=%(objectname)')) -split "`n" | Where-Object { $_ }).Count
    $erreichbar = @((Git @('rev-list', '--objects', '--all')) -split "`n" | Where-Object { $_ }).Count
    if ($alle -gt $erreichbar) { Fund 'Bundle' 0 "$($alle - $erreichbar) Objekt(e), die keine Ref erreicht" }
    Write-Host "Geprüft: Bundle mit $($koepfe.Count) Ref(s) und $alle Objekten."
    Remove-Item -LiteralPath $script:wegwerf -Recurse -Force -ErrorAction SilentlyContinue
}
if ($Baum) { $n = BaumPruefen $Baum; Write-Host "Geprüft: $n Dateien in $Baum (als $artText)." }
if ($Commits) {
    [void](CommitsPruefen $Commits $AufMain.IsPresent)
    $v = VerlaufPruefen $Commits
    if ($v -gt 0) { Write-Host "Geprüft: $v weitere Blobs aus den Commits $Commits." }
}
if ($Beitrag) {
    # Ein Beitrag mit der Angabe der Betreuer gälte sonst als eigener (ohne Regel für Beiträge und ohne M1).
    $kennung = (Git @('rev-parse', '--verify', "$Beitrag^{commit}")).Trim()
    $zeilen = @((Git @('log', '--format=%H%x00%an%x00%ae%x00%cn%x00%ce', "refs/heads/main..$kennung")) -split "`n" | Where-Object { $_ })
    foreach ($z in $zeilen) {
        $f = $z -split "`0"
        foreach ($r in @(@('Autor', 1), @('Committer', 3))) {
            if ($f[$r[1]] -ieq $RANKE_NAME -or $f[$r[1] + 1] -ieq $RANKE_MAIL) { Fund "Commit $($f[0].Substring(0, 10))" 0 "$($r[0]) des Beitrags trägt die Angabe der Betreuer; so wird kein Beitrag übernommen" }
        }
    }
    Write-Host "Geprüft: $($zeilen.Count) Commit(s) des Beitrags auf die Angabe der Betreuer."
}

if ($script:cacheDatei) { Write-Host "Inhalt: $($script:ausCache) Blobs aus dem Cache, $($script:inhaltGeprueft) geprüft." }
else { Write-Host "Inhalt kalt: $($script:inhaltGeprueft) Blobs geprüft." }
Write-Host "Geprüft als $artText."
foreach ($f in $script:funde) { Write-Host $f }
if ($script:funde.Count -gt 0) {
    Write-Host "$($script:funde.Count) Fund(e). Es geht nichts hinaus; die Werte stehen hier absichtlich nicht."
    Ende 1
}
CacheSchreiben
Write-Host 'Nichts gefunden.'
Ende 0
