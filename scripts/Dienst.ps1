<#
.SYNOPSIS
    Zeigt, startet, stoppt oder erneuert den Dashboard-Dienst (ohne Administrator).
.DESCRIPTION
    -Status    Zustand des Dienstes, Prozess, Antwort von /api/betrieb, Aufgabe
               des Boten, Prozessrechte des Dienstes (sc qprivs; erwartet
               nur SeChangeNotifyPrivilege), Lese-Recht des Dienstkontos auf
               projects, data, web, docs\entscheidungen, docs\zielbild und
               docs\anleitung (die drei, wenn es sie gibt; seit dem Umzug des Inhalts die
               Ordner aus [inhalt] in dienst.toml statt projects, data und
               docs\entscheidungen im Repo), Stand des Datenspeichers, letzte 5 Zeilen
               des Protokolls (Standard ohne Schalter).
    -Start     Dienst und Boten starten und warten, bis /healthz antwortet
               (höchstens 30 s); dann muss der Datenspeicher offen sein.
    -Stopp     Boten und Dienst stoppen (wartet höchstens 20 s; Exit-Code 1,
               wenn Wächter oder Bote nach 5 s noch laufen).
    -Neustart  Neue Fassung einspielen: im Repo cargo build --release
               --locked, Bauordner %LOCALAPPDATA%\Dashboard\cache\dienst
               (CARGO_TARGET_DIR nur für diese Aufrufe; nach einer
               Bereinigung baut er kalt, etwa 2 bis 3 Minuten, ohne Frist).
               Bis die Programme in bin liegen, steht dort EINSPIELEN.lock
               (Bauordner-Bereinigen.ps1 lässt den Platz dann aus).
               Gibt es neben dem Inhalt (Ordner über der ersten
               Quelle aus [inhalt].projekte) die Zusammenbau-Kiste
               dashboard-lokal, prüft sie zuerst der Wächter aus dem Repo
               (<bauordner>\release\dashboard.exe pruefe-zusammenbau <ordner>
               --code <repo>; liest nur, baut nichts, vor jedem cargo-Aufruf
               dort). Nur wenn er grün ist, wird sie gebaut (cargo --config
               <repo>\.cargo\config.toml build --release --locked, mit
               demselben Bauordner; Folge in Fassung-Bauen.ps1) und
               ihr dashboard-lokal.exe als dashboard.exe eingespielt: Das ist
               die Fassung mit den Rechenteilen der Projekte. Fehlt sie, läuft
               das allgemeine dashboard.exe: Die Projektseiten gehen, die
               Rechenteile fehlen (ein Satz sagt das). Dann den Boten anhalten
               (er schreibt sonst dieselbe Bauliste, Kopien und stand.json;
               try/finally: bei jedem Abbruch im Skript bis zum Austausch
               (Ende mit Fehler, Ausnahme) startet er wieder, sofern keiner
               mehr läuft, und ebenso der Dienst, wenn er schon angehalten
               war; Strg+C und ein geschlossenes Fenster sind nicht
               abgedeckt; läuft er nach 5 s
               noch oder vor der Prüfung wieder, wird nichts eingespielt) und
               <bauordner>\release\dashboard.exe pruefe --bauliste --svelte
               (nach einer neuen Prüfer-Fassung etwa eine Minute, sonst
               Sekunden; dasselbe Urteil wie der Bote, bote::bau::bauliste_einspielen:
               je Projekt Scanner, Stil, ESLint, svelte-check, Vitest, dann die
               Gesamtprüfung; schreibt web\src\lib\projekte.generated.ts nur
               mit grünen Kopien: grün geprüft, sonst die letzte grüne Kopie,
               ohne Kopie fehlt es; lief ein Werkzeug nicht oder wurde nach
               der Prüfung gespeichert, ist es nicht geprüft, nie grün; rote
               werden genannt, der Neustart geht trotzdem weiter; bleibt die
               Gesamtprüfung rot, kommen alle letzten grünen Kopien in den Bau;
               scheitert die Prüfung selbst, ist die Liste leer) und in web\
               npm run build (bricht
               ein Projekt den Bau, einmal neu ohne Projektseiten;
               web\build erbt das Lese-Recht des Dienstkontos
               von web, es ist nichts neu zu setzen) und web\build als Kopie
               nach bin\web.neu (der Dienst liefert die Oberfläche aus bin\web,
               nie aus web\build: ein Bau im Repo ändert die laufende Seite
               nicht; dieselbe Kopie liegt als web neben den neuen Programmen
               im Bauordner, Dienst-Einrichten.ps1 nimmt sie von dort); dann den Dienst
               anhalten, web\builds\aktuell löschen (der Zeiger auf den letzten
               Bau über den Boten; danach gilt sicher die frische Fassung aus
               bin\web), die Programme (dashboard.exe, dashboard-bote.exe und
               dashboard-herzschlag.exe) nach bin (die vorige Fassung bleibt als *.alt.exe)
               und die Oberfläche nach bin\web (die vorige bleibt als bin\web.alt;
               scheitert ein Schritt, kommt beides zurück), Dienst und
               Boten starten und / und /api/daten (mit Windows-Anmeldung) binnen
               60 s prüfen. Antworten sie nicht, kommt die vorige Fassung
               (Programme und Oberfläche gemeinsam)
               zurück und startet wieder; auch dieser Rückweg startet Dienst
               und Boten in einem try/finally wieder, sofern sie nicht laufen
               (auch nach einem Abbruch im Skript oder einer Ausnahme; Strg+C
               nicht abgedeckt).
               Antworten sie, prüft das Skript den
               Datenspeicher (/api/betrieb, Feld speicher: offen und ohne
               fehler; bis etwa 10 s wiederholt). Stimmt er nicht oder ist
               sein Stand unklar, bleibt die neue Fassung laufen und das
               Skript meldet es (Exit-Code 1); zurückgerollt wird wegen der
               Daten nie, weil die neue sie schon umgestellt haben kann.
               Läuft die vorige nach dem Zurückrollen ohne Datenspeicher,
               weil die neue die Datei schon auf ein höheres Schema umgebaut
               hat, sagt das Skript genau das, nennt die Sicherung vor dem
               Umbau (sicherung\dashboard-JJJJ-MM-TT-vor-schema-N.redb) und wie
               man sie als Administrator zurückspielt (Exit-Code 1).
               Scheitert schon der Austausch der Dateien, kommt die vorige
               Fassung sofort zurück und Dienst und Bote starten wieder.
               Am Ende steht, wie lange das Dashboard weg war.

    Der Bote: Die Aufgabe startet bin\dashboard-bote.exe --waechter; der
    Wächter startet den Boten und nach einem Fehler neu (zwei Prozesse mit
    demselben Programm). Beim Anhalten wird erst die Aufgabe gestoppt, dann
    werden Wächter und Bote beendet. Beim Starten wird die Aufgabe gestartet;
    darf das Konto das nicht, startet das Skript den Wächter selbst (läuft
    so oder so unter deinem Konto).

    Adressen: Im Browser http://localhost:<port>. Die Lebenszeichen hier
    fragen 127.0.0.1 (schneller, ohne Umweg über ::1).

    Starten und Stoppen gehen ohne Administrator, weil Dienst-Einrichten.ps1
    deinem Konto genau diese Rechte am Dienst gegeben hat.

    Exit-Codes:
      0  gut (bei -Status: der Dienst läuft und antwortet)
      1  nicht eingerichtet, läuft nicht, oder ein Schritt ist gescheitert (Satz nennt ihn)
.PARAMETER Dienstname
    Name des Dienstes (Standard Dashboard).
#>
[CmdletBinding(DefaultParameterSetName = 'Status')]
param(
    [Parameter(ParameterSetName = 'Status')][switch]$Status,
    [Parameter(ParameterSetName = 'Start')][switch]$Start,
    [Parameter(ParameterSetName = 'Stopp')][switch]$Stopp,
    [Parameter(ParameterSetName = 'Neustart')][switch]$Neustart,
    [ValidatePattern('^[A-Za-z][A-Za-z0-9-]{0,39}$')][string]$Dienstname = 'Dashboard'
)

$ErrorActionPreference = 'Stop'

$sys32 = Join-Path $env:SystemRoot 'System32'
$scPfad = Join-Path $sys32 'sc.exe'
$curlPfad = Join-Path $sys32 'curl.exe'
$aufgabenOrdner = "\$Dienstname\"
$aufgabenName = "$Dienstname-Bote"
$programme = @('dashboard.exe', 'dashboard-bote.exe', 'dashboard-herzschlag.exe')

function Starte([string]$exe, [string]$argumente) {
    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = $exe
    $psi.Arguments = $argumente
    $psi.UseShellExecute = $false
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.CreateNoWindow = $true
    try {
        $enc = [Text.Encoding]::GetEncoding([Globalization.CultureInfo]::CurrentCulture.TextInfo.OEMCodePage)
        $psi.StandardOutputEncoding = $enc
        $psi.StandardErrorEncoding = $enc
    } catch { }
    $p = [Diagnostics.Process]::Start($psi)
    $aus = $p.StandardOutput.ReadToEndAsync()
    $fehl = $p.StandardError.ReadToEnd()
    $p.WaitForExit()
    [pscustomobject]@{ Code = $p.ExitCode; Text = ($aus.Result + $fehl).Trim() }
}

function ScExe([string]$argumente) { Starte $scPfad $argumente }

function Ende([int]$code, [string]$satz) {
    Write-Host $satz
    exit $code
}

function DienstStatus {
    $s = Get-Service -Name $Dienstname -ErrorAction SilentlyContinue
    if ($s) { return [string]$s.Status }
    return $null
}

function WarteAufStatus([string]$ziel, [int]$sekunden) {
    $ende = (Get-Date).AddSeconds($sekunden)
    while ((Get-Date) -lt $ende) {
        if ((DienstStatus) -eq $ziel) { return $true }
        Start-Sleep -Milliseconds 500
    }
    return ((DienstStatus) -eq $ziel)
}

# Lebenszeichen über 127.0.0.1 (schneller als localhost; der Server hört auf
# beiden). WindowsAnfrage (Anfrage.ps1) meldet sich als das Konto an, das
# dieses Skript ausführt (der Inhaber): seit Etappe 7b verlangt /api/daten das. Das
# curl von Windows kann das nicht mehr (kein NTLM, siehe
# Anfrage.ps1). Die Adresse für den Browser ist http://localhost:<port>.
. (Join-Path $PSScriptRoot 'Anfrage.ps1')
# Bau der Fassung (Wächter vor jedem cargo-Aufruf in dashboard-lokal).
. (Join-Path $PSScriptRoot 'Fassung-Bauen.ps1')
function HttpCode([string]$weg) {
    (WindowsAnfrage -Url "http://127.0.0.1:$($script:port)$weg" -Sekunden 3).Code
}

# $null, wenn alle Wege binnen der Zeit 200 liefern, sonst eine Beschreibung.
function WarteAuf200([string[]]$wege, [int]$sekunden) {
    $ende = (Get-Date).AddSeconds($sekunden)
    $codes = @{}
    do {
        $alle = $true
        foreach ($w in $wege) {
            $c = HttpCode $w
            $codes[$w] = $c
            if ($c -ne '200') { $alle = $false }
        }
        if ($alle) { return $null }
        Start-Sleep -Seconds 1
    } while ((Get-Date) -lt $ende)
    return (($wege | ForEach-Object { "$_ -> $($codes[$_])" }) -join ', ')
}

# Eine Antwort nur dann als JSON nehmen, wenn der HTTP-Code 200 ist. Eine 401
# (Anmeldung) oder 503 mit JSON-Feld fehler ist keine Antwort über den
# Speicher. Rückgabe: Code (Text, etwa '200', '401', '000' ohne Verbindung)
# und Daten (Objekt aus ConvertFrom-Json oder $null). Ohne Netz, damit
# tests\skripte\Dienst-Speicher.ps1 es prüfen kann.
function JsonAusAntwort([string]$code, [string]$text) {
    $antwort = [pscustomobject]@{ Code = $code; Daten = $null }
    if ($code -ne '200' -or -not $text) { return $antwort }
    try { $antwort.Daten = ($text | ConvertFrom-Json) } catch { }
    return $antwort
}

# JSON von einem Weg (mit Windows-Anmeldung wie HttpCode; der Körper kommt
# als UTF-8, die Sätze haben Umlaute). Gibt JsonAusAntwort zurück.
function HoleJson([string]$weg) {
    $r = WindowsAnfrage -Url "http://127.0.0.1:$($script:port)$weg" -Sekunden 3
    return (JsonAusAntwort $r.Code $r.Text)
}

# Urteil über den Datenspeicher nach einem Start, aus den Antworten von
# /api/betrieb und /api/daten (beide als Objekt aus ConvertFrom-Json oder
# $null). Ohne Netz und ohne Dateien, damit tests\skripte\Dienst-Speicher.ps1
# es mit gestellten Antworten prüfen kann.
#
# Neue Fassungen (ab 5a) melden in /api/betrieb das Feld speicher { offen,
# schema, programm_schema, zu_neu, fehler }. Der Server öffnet den Speicher in
# jedem Modus (auch im Altweg ohne Einstellungsdatei); offen = false ist also
# immer ein Fehler. Ältere Fassungen (etwa die vorige nach dem Zurückrollen)
# kennen das Feld nicht; dann zählt das Feld fehler von /api/daten (ohne
# Speicher antwortet /api/daten trotzdem 200, mit dem Satz in fehler).
#
# Rückgabe: Gut (bool), Art ('gut', 'zu-neu', 'fehler', 'unbekannt'), Satz,
# Schema (der Datei, wenn bekannt) und Kennt (Schema der laufenden Fassung).
function SpeicherUrteil($betrieb, $daten) {
    $urteil = [pscustomobject]@{ Gut = $false; Art = 'unbekannt'; Satz = ''; Schema = $null; Kennt = $null }
    if ($betrieb -and ($betrieb.PSObject.Properties.Name -contains 'speicher') -and $betrieb.speicher) {
        $sp = $betrieb.speicher
        $urteil.Schema = $sp.schema
        $urteil.Kennt = $sp.programm_schema
        if ($sp.offen -eq $true -and -not $sp.fehler) {
            $urteil.Gut = $true; $urteil.Art = 'gut'; $urteil.Satz = 'Die Daten sind da.'
        } elseif ($sp.zu_neu -eq $true) {
            $urteil.Art = 'zu-neu'; $urteil.Satz = [string]$sp.fehler
        } else {
            $urteil.Art = 'fehler'
            $urteil.Satz = if ($sp.fehler) { [string]$sp.fehler } else { 'Der Datenspeicher ist nicht offen.' }
        }
        return $urteil
    }
    if (-not $daten) {
        $urteil.Satz = 'Ob die Daten da sind, ließ sich nicht feststellen.'
        return $urteil
    }
    if ($daten.fehler) {
        $satz = [string]$daten.fehler
        $urteil.Satz = $satz
        $m = [regex]::Match($satz, 'neueren Fassung.*?Schema ([0-9]+), dieses Programm kennt ([0-9]+)')
        if ($m.Success) {
            $urteil.Art = 'zu-neu'
            $urteil.Schema = [int]$m.Groups[1].Value
            $urteil.Kennt = [int]$m.Groups[2].Value
        } else {
            $urteil.Art = 'fehler'
        }
        return $urteil
    }
    $urteil.Gut = $true; $urteil.Art = 'gut'; $urteil.Schema = $daten.schema
    $urteil.Satz = 'Die Daten sind da.'
    return $urteil
}

# Fragt den laufenden Dienst und gibt SpeicherUrteil zurück. Ist das Urteil
# 'unbekannt' (keine Antwort mit Code 200, etwa eine kurze Störung), fragt es
# bis etwa $sekunden lang jede Sekunde neu; danach nennt der Satz die Codes.
function SpeicherPruefen([int]$sekunden = 10) {
    $ende = (Get-Date).AddSeconds($sekunden)
    while ($true) {
        $b = HoleJson '/api/betrieb'
        $d = HoleJson '/api/daten'
        $u = SpeicherUrteil $b.Daten $d.Daten
        if ($u.Art -ne 'unbekannt') { return $u }
        if ((Get-Date) -ge $ende) {
            $u.Satz = "Keine gültige Antwort (/api/betrieb: $($b.Code), /api/daten: $($d.Code))."
            return $u
        }
        Start-Sleep -Seconds 1
    }
}

# Was nach dem Start der NEUEN Fassung geschieht, wenn sie schon antwortet
# (/ und /api/daten mit 200). Ohne Netz, für tests\skripte\Dienst-Speicher.ps1.
# Weg 'fertig': alles gut. Weg 'melden': die neue Fassung läuft weiter, das
# Skript sagt, was nicht stimmt, und endet mit Exit-Code 1. Wegen der Daten
# wird nie zurückgerollt: Die neue Fassung hat die Datendatei womöglich schon
# umgestellt (bei gleichem Schema sicher), und die vorige liefe dann sicher
# ohne Daten. Zurückgerollt wird nur, wenn die neue Fassung gar nicht
# antwortet (das prüft WarteAuf200 vorher).
function NachStartEntscheidung($urteil, [int]$dauer) {
    if ($urteil.Gut) {
        return [pscustomobject]@{ Weg = 'fertig'; Satz = "Neustart fertig. Das Dashboard war $dauer s nicht erreichbar." }
    }
    $kopf = "Die neue Fassung läuft (Unterbrechung $dauer s), aber"
    switch ($urteil.Art) {
        'unbekannt' { $mitte = "es ließ sich nicht feststellen, ob ihre Daten da sind. $($urteil.Satz)" }
        'zu-neu' { $mitte = 'ihre Daten stammen von einer noch neueren Fassung, sie kann sie nicht lesen. Aufgaben, Vorgänge und Warten gehen gerade nicht.' }
        default {
            $mitte = "mit ihren Daten stimmt etwas nicht: $($urteil.Satz) Aufgaben, Vorgänge und Warten gehen gerade nicht."
            if (($null -ne $urteil.Schema) -and ($urteil.Schema -eq $urteil.Kennt)) { $mitte += ' Die Daten sind schon auf die neue Fassung umgestellt.' }
        }
    }
    $weiter = 'Die neue Fassung läuft weiter; zurückgerollt wird nicht, denn die vorige Fassung könnte die Daten danach vielleicht nicht mehr lesen.'
    $nachsehen = 'Bitte scripts\Dienst.ps1 -Status ansehen und den Chat des Dashboards fragen.'
    return [pscustomobject]@{ Weg = 'melden'; Satz = "$kopf $mitte $weiter $nachsehen" }
}

# Der ehrliche Satz, wenn die vorige Fassung nach dem Zurückrollen ohne ihre
# Daten läuft, weil die neue die Datendatei schon umgestellt hat. Nennt die
# Sicherung von vor der Umstellung (dashboard-JJJJ-MM-TT-vor-schema-<Kennt>.redb,
# die neueste im Ordner sicherung) und wie man sie als Administrator
# zurückspielt. Ohne Administrator geht das nicht: Der Inhaber darf sicherung nur
# lesen und daten gar nicht (Entscheidung autostart-dienst).
# In einfacher Sprache, ohne Fachwörter.
function ZurueckSatz($urteil, [string]$sicherungsOrdner, [string]$datenDatei, [int]$dauer) {
    $muster = if ($urteil.Kennt) { "dashboard-*-vor-schema-$($urteil.Kennt).redb" } else { 'dashboard-*-vor-schema-*.redb' }
    $kopie = Get-ChildItem -LiteralPath $sicherungsOrdner -Filter $muster -File -ErrorAction SilentlyContinue | Sort-Object Name | Select-Object -Last 1
    $wo = if ($kopie) { $kopie.FullName } else { "(keine gefunden im Ordner $sicherungsOrdner)" }
    @(
        "Die neue Fassung startete nicht. Die vorige Fassung läuft wieder (Unterbrechung $dauer s), aber ohne ihre Daten:",
        'die neue Fassung hatte die Datendatei beim Start schon umgestellt, und die vorige kann sie so nicht lesen.',
        'Die Projektseiten gehen; Aufgaben, Vorgänge und Warten gehen gerade nicht.',
        'Zwei Wege:',
        '  1. Besser: den Fehler der neuen Fassung beheben lassen und scripts\Dienst.ps1 -Neustart noch einmal; dann ist alles wieder da.',
        "  2. Die Sicherung von vor der Umstellung zurückspielen: $wo",
        "     Das geht nur als Administrator: Dienst anhalten (scripts\Dienst.ps1 -Stopp), $datenDatei umbenennen (nicht löschen),",
        '     die Sicherung an ihre Stelle kopieren, Dienst starten (scripts\Dienst.ps1 -Start). Schritt für Schritt: LIESMICH.txt im Dienst-Ordner, Abschnitt „Zurück zur vorigen Fassung“.',
        '     Was seit der Umstellung dazukam (Aufgaben, Vorgänge), fehlt dann; es bleibt in der umbenannten Datei erhalten.'
    ) -join [Environment]::NewLine
}

function TomlWert([string]$text, [string]$schluessel) {
    $m = [regex]::Match($text, '(?m)^\s*' + $schluessel + '\s*=\s*(?:"([^"]*)"|(\d+))')
    if (-not $m.Success) { return $null }
    if ($m.Groups[1].Success) { return $m.Groups[1].Value }
    return $m.Groups[2].Value
}

function Gleicher-Pfad([string]$a, [string]$b) {
    if (-not $a -or -not $b) { return $false }
    (($a -replace '/', '\').TrimEnd('\')) -ieq (($b -replace '/', '\').TrimEnd('\'))
}

# Ein Abschnitt der TOML-Datei ([name] bis zum nächsten [..]); sonst ''.
function TomlAbschnitt([string]$text, [string]$name) {
    $m = [regex]::Match($text, '(?ms)^\s*\[' + [regex]::Escape($name) + '\]\s*$(.*?)(?=^\s*\[|\z)')
    if ($m.Success) { return $m.Groups[1].Value }
    return ''
}

# Der private Inhalt aus dienst.toml ([inhalt], Umzug des Inhalts): Quellen,
# Daten, Entscheidungen und die Zusammenbau-Kiste dashboard-lokal (neben dem
# Inhalt, wenn es sie gibt). Ohne [inhalt] wie vorher im Repo.
function Inhalt([string]$text, [string]$repo) {
    $abschnitt = TomlAbschnitt $text 'inhalt'
    $quellen = @()
    $m = [regex]::Match($abschnitt, '(?ms)^\s*projekte\s*=\s*\[(.*?)\]')
    if ($m.Success) { $quellen = @([regex]::Matches($m.Groups[1].Value, '"([^"]*)"') | ForEach-Object { $_.Groups[1].Value -replace '/', '\' }) }
    if ($quellen.Count -eq 0) {
        $alt = TomlWert (TomlAbschnitt $text 'pfade') 'projekte'
        $quellen = @($(if ($alt) { $alt -replace '/', '\' } else { Join-Path $repo 'projects' }))
    }
    $daten = TomlWert $abschnitt 'daten'
    $entsch = TomlWert $abschnitt 'entscheidungen'
    $lokal = $null
    $wurzel = Split-Path -Parent $quellen[0]
    if ($wurzel -and -not (Gleicher-Pfad $wurzel $repo)) {
        $k = Join-Path $wurzel 'dashboard-lokal'
        if (Test-Path -LiteralPath (Join-Path $k 'Cargo.toml') -PathType Leaf) { $lokal = $k }
    }
    [pscustomobject]@{
        Projekte       = $quellen
        Daten          = $(if ($daten) { $daten -replace '/', '\' } else { Join-Path $repo 'data' })
        Entscheidungen = $(if ($entsch) { $entsch -replace '/', '\' } else { Join-Path $repo 'docs\entscheidungen' })
        Lokal          = $lokal
        Eigen          = [bool]$m.Success
    }
}

# -ohneEnde (im finally des Rückwegs): auch bei Code 5 nur den Satz zurück, denn
# ein exit dort verdeckte den ursprünglichen Abbruch (Ende oder Ausnahme).
function DienstStarten([switch]$ohneEnde) {
    $r = ScExe "start $Dienstname"
    # 1056: läuft schon.
    if ($r.Code -eq 5) {
        $satz = 'Keine Berechtigung, den Dienst zu starten. Dienst-Einrichten.ps1 noch einmal als Administrator laufen lassen (es setzt die Rechte neu).'
        if ($ohneEnde) { return $satz }
        Ende 1 $satz
    }
    if ($r.Code -ne 0 -and $r.Code -ne 1056) { return "sc start meldet: $($r.Text)" }
    return $null
}

function DienstStoppen {
    if ((DienstStatus) -eq 'Stopped') { return $null }
    $r = ScExe "stop $Dienstname"
    if ($r.Code -eq 5) { Ende 1 "Keine Berechtigung, den Dienst zu stoppen. Dienst-Einrichten.ps1 noch einmal als Administrator laufen lassen (es setzt die Rechte neu)." }
    if (-not (WarteAufStatus 'Stopped' 20)) { return "Der Dienst hielt nicht binnen 20 s an ($($r.Text); Zustand $(DienstStatus))." }
    return $null
}

# Alle Prozesse mit dem Programm bin\dashboard-bote.exe: der Wächter
# (--waechter, von der Aufgabe gestartet) und der Bote, den er startet und
# nach einem Fehler neu startet. Der Wächter steht vorn, damit er beim
# Beenden zuerst geht und den Boten nicht gleich wieder startet.
function BoteProzesse {
    $exe = Join-Path $script:bin 'dashboard-bote.exe'
    @(Get-CimInstance Win32_Process -Filter "Name='dashboard-bote.exe'" -ErrorAction SilentlyContinue |
        Where-Object { $_.ExecutablePath -and (Gleicher-Pfad $_.ExecutablePath $exe) } |
        Sort-Object @{ Expression = { if (IstWaechter $_) { 0 } else { 1 } } })
}

function IstWaechter($prozess) { [string]$prozess.CommandLine -match '(^|\s)--waechter(\s|$)' }

# Erst die Aufgabe anhalten, dann Wächter und Boten direkt beenden (auch wenn
# Stop-ScheduledTask ohne Administrator nicht darf). Beide laufen unter dem
# Konto des Inhabers, das darf er also beenden. Gibt $null zurück, wenn keiner
# mehr läuft, sonst einen Satz.
function BoteStoppen {
    try {
        $null = Stop-ScheduledTask -TaskPath $aufgabenOrdner -TaskName $aufgabenName -ErrorAction Stop
    } catch {
        Write-Host "Hinweis: Die Aufgabe des Boten ließ sich nicht anhalten ($($_.Exception.Message)); beende Wächter und Boten direkt."
    }
    $ende = (Get-Date).AddSeconds(5)
    do {
        $laufend = BoteProzesse
        foreach ($pr in $laufend) { try { Stop-Process -Id $pr.ProcessId -Force -ErrorAction Stop } catch { } }
        if ($laufend.Count -eq 0) { return $null }
        Start-Sleep -Milliseconds 300
    } while ((Get-Date) -lt $ende)
    if ((BoteProzesse).Count -gt 0) { return 'Wächter oder Bote laufen nach 5 s noch' }
    return $null
}

# Erst über die Aufgabe (wie bei der Anmeldung); sie startet den Wächter, der
# den Boten startet. Scheitert Start-ScheduledTask (etwa ohne Administrator)
# oder läuft danach binnen 5 s kein Wächter, wird der Wächter direkt
# gestartet; er läuft so oder so unter dem Konto des Inhabers.
function BoteStarten {
    $ueberAufgabe = $false
    try {
        Start-ScheduledTask -TaskPath $aufgabenOrdner -TaskName $aufgabenName -ErrorAction Stop
        $ueberAufgabe = $true
    } catch {
        Write-Host "Hinweis: Die Aufgabe des Boten ließ sich nicht starten ($($_.Exception.Message))."
    }
    if ($ueberAufgabe) {
        $ende = (Get-Date).AddSeconds(5)
        while (@(BoteProzesse | Where-Object { IstWaechter $_ }).Count -eq 0 -and (Get-Date) -lt $ende) { Start-Sleep -Milliseconds 300 }
        if (@(BoteProzesse | Where-Object { IstWaechter $_ }).Count -gt 0) { Write-Host 'Bote gestartet (Aufgabe, mit Wächter)'; return }
        Write-Host 'Hinweis: Die Aufgabe lief an, aber der Wächter nicht.'
    }
    try {
        Start-Process -FilePath (Join-Path $script:bin 'dashboard-bote.exe') -ArgumentList @('--waechter', '--einstellungen', "`"$($script:toml)`"") -WorkingDirectory $script:bin -WindowStyle Hidden -ErrorAction Stop
        Write-Host 'Bote gestartet (Wächter direkt)'
    } catch {
        Write-Host "Hinweis: Der Bote startete nicht ($($_.Exception.Message)); er startet bei der nächsten Anmeldung."
    }
}

# Die Oberfläche des Dienstes ist eine Kopie in bin\web, neben den Programmen
# (der Dienst liest sie dort, app::WebOrt); ein npm run build im Repo ändert
# die laufende Seite also nicht. Legt web\build als bin\web.neu bereit (ein
# liegen gebliebenes web.neu fällt vorher weg). Mit $ablage liegt dieselbe Kopie
# auch neben den neuen Programmen im Bauordner (<quelle>\web): Dienst-Einrichten.ps1
# spielt sie von dort ein, nicht aus web\build, das danach neuer sein kann.
# Gibt $null zurück, sonst einen Satz.
function OberflaecheBereitlegen([string]$build, [string]$ablage) {
    $neu = Join-Path $script:bin 'web.neu'
    try {
        if (Test-Path -LiteralPath $neu) { Remove-Item -LiteralPath $neu -Recurse -Force }
        if (-not (Test-Path -LiteralPath (Join-Path $build 'index.html') -PathType Leaf)) { return "in $build fehlt index.html" }
        if ($ablage) {
            if (Test-Path -LiteralPath $ablage) { Remove-Item -LiteralPath $ablage -Recurse -Force }
            Copy-Item -LiteralPath $build -Destination $ablage -Recurse
        }
        Copy-Item -LiteralPath $build -Destination $neu -Recurse
        if (-not (Test-Path -LiteralPath (Join-Path $neu 'index.html') -PathType Leaf)) { return "in $neu fehlt nach dem Kopieren index.html" }
        return $null
    } catch {
        return $_.Exception.Message
    }
}

# Neue Programme nach bin, die vorigen als *.alt.exe; mit $oberflaeche (bin\web.neu)
# auch die Oberfläche: bin\web wird bin\web.alt, web.neu wird web. Scheitert ein
# Schritt, kommen die schon verschobenen Fassungen zurück (Programme und
# Oberfläche); gibt dann einen Satz zurück (sonst $null). So bleibt bin nie ohne
# dashboard.exe und nie halb eingespielt. $script:oberflaecheGetauscht sagt dem
# Rückweg, ob bin\web.alt zu dieser Fassung gehört, $script:webVorher, ob es
# vor diesem Lauf bin\web gab.
function TauscheProgramme([string]$quelle, [string]$oberflaeche) {
    $verschoben = @()
    $web = Join-Path $script:bin 'web'
    $webAlt = Join-Path $script:bin 'web.alt'
    $webVerschoben = $false
    $script:oberflaecheGetauscht = $false
    $script:webVorher = Test-Path -LiteralPath $web
    try {
        foreach ($n in $programme) {
            $z = Join-Path $script:bin $n
            $alt = Join-Path $script:bin ($n -replace '\.exe$', '.alt.exe')
            if (Test-Path -LiteralPath $z) {
                Move-Item -LiteralPath $z -Destination $alt -Force
                $verschoben += $n
            }
            Copy-Item -LiteralPath (Join-Path $quelle $n) -Destination $z -Force
        }
        if ($oberflaeche) {
            if (Test-Path -LiteralPath $webAlt) { Remove-Item -LiteralPath $webAlt -Recurse -Force }
            if (Test-Path -LiteralPath $web) {
                Move-Item -LiteralPath $web -Destination $webAlt
                $webVerschoben = $true
            }
            Move-Item -LiteralPath $oberflaeche -Destination $web
            $script:oberflaecheGetauscht = $true
        }
        return $null
    } catch {
        $grund = $_.Exception.Message
        if ($webVerschoben) {
            try { Move-Item -LiteralPath $webAlt -Destination $web -ErrorAction Stop }
            catch { Write-Host "Achtung: $webAlt ließ sich nicht nach web zurückholen ($($_.Exception.Message))." }
        }
        foreach ($n in $verschoben) {
            $z = Join-Path $script:bin $n
            $alt = Join-Path $script:bin ($n -replace '\.exe$', '.alt.exe')
            try { Move-Item -LiteralPath $alt -Destination $z -Force -ErrorAction Stop }
            catch { Write-Host "Achtung: $alt ließ sich nicht nach $n zurückholen ($($_.Exception.Message))." }
        }
        return "Der Austausch der Programme ist gescheitert ($grund); die vorige Fassung liegt wieder in $($script:bin).$(if ($webVerschoben -and -not (Test-Path -LiteralPath (Join-Path $web 'index.html'))) { " Nur $web fehlt: Bis zum nächsten -Neustart kommt die Oberfläche aus web\build im Repo." })"
    }
}

# Rückweg der Oberfläche: bin\web.alt wird wieder bin\web (die neue geht über
# bin\web.verworfen und wird danach gelöscht). Scheitert das Zurückholen, bleibt
# die neue in bin\web. Gab es vor dem Lauf kein bin\web ($script:webVorher), fällt
# die neue weg; das vorige Programm liest dann wie bisher web\build. Gibt $null
# zurück, sonst einen Satz.
function OberflaecheZurueck {
    $web = Join-Path $script:bin 'web'
    $webAlt = Join-Path $script:bin 'web.alt'
    $weg = Join-Path $script:bin 'web.verworfen'
    if ($script:webVorher -eq $false) {
        try {
            # bin\web stammt aus diesem Lauf (kopiert, ohne Verknüpfungen darin); nur der Ordner selbst könnte inzwischen eine sein.
            if (((Get-Item -LiteralPath $web -Force -ErrorAction Stop).Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw "$web ist eine Verknüpfung" }
            Remove-Item -LiteralPath $web -Recurse -Force -ErrorAction Stop
            return $null
        } catch {
            return "$web ließ sich nicht entfernen ($($_.Exception.Message)); die Oberfläche bleibt die neue."
        }
    }
    if (-not (Test-Path -LiteralPath $webAlt)) { return "$webAlt fehlt; die Oberfläche bleibt die neue." }
    try {
        if (Test-Path -LiteralPath $weg) { Remove-Item -LiteralPath $weg -Recurse -Force }
        if (Test-Path -LiteralPath $web) { Move-Item -LiteralPath $web -Destination $weg }
        try { Move-Item -LiteralPath $webAlt -Destination $web }
        catch {
            if (Test-Path -LiteralPath $weg) { Move-Item -LiteralPath $weg -Destination $web }
            throw
        }
    } catch {
        return "$webAlt ließ sich nicht zurückholen ($($_.Exception.Message)); $(if (Test-Path -LiteralPath (Join-Path $web 'index.html')) { 'die Oberfläche bleibt die neue.' } else { "$web fehlt: Bis zum nächsten -Neustart kommt die Oberfläche aus web\build im Repo." })"
    }
    Remove-Item -LiteralPath $weg -Recurse -Force -ErrorAction SilentlyContinue
    return $null
}

# Hat das Dienstkonto sein Lese-Recht (vererbt) auf projects, data, web,
# docs\entscheidungen, docs\zielbild und docs\anleitung (seit 6b)?
# Das Recht liegt auf web (nicht auf web\build), damit jeder Neubau der
# Oberfläche es erbt; nach npm run build ist also nichts neu zu setzen.
# Gibt $null zurück, wenn alles da ist, sonst einen Satz.
function Leserecht {
    if (-not $script:dienstSid) { return "In $($script:toml) steht keine dienst_sid; Dienst-Einrichten.ps1 als Administrator noch einmal laufen lassen." }
    $rx = [Security.AccessControl.FileSystemRights]::ReadAndExecute
    $ci = [Security.AccessControl.InheritanceFlags]::ContainerInherit
    $oi = [Security.AccessControl.InheritanceFlags]::ObjectInherit
    $fehlt = @()
    # Seit dem Umzug des Inhalts: die Quellen, der Daten-Ordner und die
    # Entscheidungen aus [inhalt] (sonst wie vorher im Repo); web und
    # docs\zielbild und docs\anleitung (seit 6b) im Repo. Die Entscheidungen,
    # docs\zielbild (seit 4d) und docs\anleitung nur, wenn es sie gibt.
    $ordner = @($script:inhalt.Projekte) + @($script:inhalt.Daten, (Join-Path $repo 'web'), $script:inhalt.Entscheidungen, (Join-Path $repo 'docs\zielbild'), (Join-Path $repo 'docs\anleitung'))
    $wennDa = @($script:inhalt.Entscheidungen, (Join-Path $repo 'docs\zielbild'), (Join-Path $repo 'docs\anleitung'))
    foreach ($p in $ordner) {
        if (-not (Test-Path -LiteralPath $p -PathType Container)) {
            if (-not ($wennDa -contains $p)) { $fehlt += "$p (Ordner fehlt)" }
            continue
        }
        $da = @((Get-Acl -LiteralPath $p).GetAccessRules($true, $false, [Security.Principal.SecurityIdentifier]) | Where-Object {
                $_.IdentityReference.Value -eq $script:dienstSid -and $_.AccessControlType -eq 'Allow' -and
                (($_.FileSystemRights -band $rx) -eq $rx) -and (($_.InheritanceFlags -band $ci) -ne 0) -and (($_.InheritanceFlags -band $oi) -ne 0) })
        if ($da.Count -eq 0) { $fehlt += $p }
    }
    if ($fehlt.Count -eq 0) { return $null }
    return "NT SERVICE\$Dienstname darf nicht lesen: $($fehlt -join ', ')."
}

# ---------------------------------------------------------------------------
# Wo liegt der Dienst?
# ---------------------------------------------------------------------------
$w = Get-CimInstance Win32_Service -Filter "Name='$Dienstname'" -ErrorAction SilentlyContinue
if (-not $w) {
    Ende 1 "Dienst $Dienstname nicht eingerichtet. Einrichten: scripts\Dienst-Einrichten.ps1 als Administrator (siehe /anleitung, Das Dashboard als Dienst). Ohne Dienst startet das Dashboard mit scripts\Start-Dashboard.ps1."
}
$m = [regex]::Match([string]$w.PathName, '^\s*"?([^"]+?\.exe)"?')
$exePfad = if ($m.Success) { $m.Groups[1].Value } else { $null }
$script:bin = if ($exePfad) { Split-Path -Parent $exePfad } else { Join-Path $env:ProgramData "$Dienstname\bin" }
$wurzel = Split-Path -Parent $script:bin
$m = [regex]::Match([string]$w.PathName, '--einstellungen\s+"?([^"]+?)"?(\s|$)')
$script:toml = if ($m.Success) { $m.Groups[1].Value } else { Join-Path $wurzel 'dienst.toml' }

$script:port = 8787
$script:dienstSid = $null
$repo = Split-Path -Parent $PSScriptRoot
$protokoll = Join-Path $wurzel 'protokoll'
$script:datenDatei = Join-Path $wurzel 'daten\dashboard.redb'
$script:sicherungsOrdner = Join-Path $wurzel 'sicherung'
$t = ''
try {
    $t = [IO.File]::ReadAllText($script:toml)
    $pf = TomlAbschnitt $t 'pfade'
    if (TomlWert $t 'port') { $script:port = [int](TomlWert $t 'port') }
    if (TomlWert $pf 'daten') { $script:datenDatei = (TomlWert $pf 'daten') -replace '/', '\' }
    if (TomlWert $pf 'sicherung') { $script:sicherungsOrdner = (TomlWert $pf 'sicherung') -replace '/', '\' }
    $script:dienstSid = TomlWert $t 'dienst_sid'
    if (TomlWert $t 'repo') { $repo = (TomlWert $t 'repo') -replace '/', '\' }
    if (TomlWert $pf 'protokoll') { $protokoll = (TomlWert $pf 'protokoll') -replace '/', '\' }
} catch {
    Write-Host "Hinweis: $($script:toml) ist nicht lesbar; nehme Port $($script:port) und das Repo $repo."
}
$script:inhalt = Inhalt $t $repo

# ---------------------------------------------------------------------------
# -Status
# ---------------------------------------------------------------------------
if ($PSCmdlet.ParameterSetName -eq 'Status') {
    $zustand = @{ Running = 'läuft'; Stopped = 'gestoppt'; 'Start Pending' = 'startet'; 'Stop Pending' = 'hält an'; Paused = 'angehalten' }
    $z = if ($zustand.ContainsKey([string]$w.State)) { $zustand[[string]$w.State] } else { [string]$w.State }
    $startArt = switch ([string]$w.StartMode) { 'Auto' { 'beim Hochfahren' } 'Manual' { 'von Hand' } 'Disabled' { 'abgeschaltet' } default { [string]$w.StartMode } }
    Write-Host ("Dienst:     {0}, {1}{2}; Start {3}; Konto {4}" -f $Dienstname, $z, $(if ($w.ProcessId) { " (Prozess $($w.ProcessId))" } else { '' }), $startArt, $w.StartName)
    if ($w.State -eq 'Stopped' -and $w.ExitCode -ne 0) {
        Write-Host ("            letztes Ende mit Fehler: Exit-Code {0}, Dienst-Code {1} (2 Einstellungen fehlerhaft, 3 Port belegt, 9 sonst)" -f $w.ExitCode, $w.ServiceSpecificExitCode)
    }
    $gut = $false
    $r = Starte $curlPfad "-s --max-time 3 http://127.0.0.1:$($script:port)/api/betrieb"
    if ($r.Code -eq 0 -and $r.Text) {
        try {
            $j = $r.Text | ConvertFrom-Json
            Write-Host ("Antwort:    http://localhost:{0}/ ; Modus {1}, gestartet {2}" -f $script:port, $j.modus, $j.gestartet)
            $gut = ($w.State -eq 'Running')
            $u = SpeicherPruefen 3
            Write-Host ("Speicher:   {0}" -f $u.Satz)
            if (-not $u.Gut) { $gut = $false }
        } catch {
            Write-Host "Antwort:    /api/betrieb lieferte kein JSON: $($r.Text)"
        }
    } else {
        Write-Host "Antwort:    keine auf Port $($script:port)"
    }
    $a = Get-ScheduledTask -TaskPath $aufgabenOrdner -TaskName $aufgabenName -ErrorAction SilentlyContinue
    $bp = BoteProzesse
    $waechter = @($bp | Where-Object { IstWaechter $_ }).Count
    $boten = $bp.Count - $waechter
    $aufgabe = if ($a) { "Aufgabe $($a.State)" } else { 'Aufgabe nicht gefunden (oder nicht lesbar)' }
    $prozesse = if ($bp.Count -eq 0) { 'kein Prozess (läuft nur, wenn du angemeldet bist)' }
                else { "Wächter $(if ($waechter) { 'läuft' } else { 'fehlt' }), Bote $(if ($boten) { 'läuft' } else { 'fehlt' })" }
    Write-Host ("Bote:       {0}; {1}" -f $aufgabe, $prozesse)
    # Prozessrechte (sc qprivs): erwartet ist nur SeChangeNotifyPrivilege. Ohne
    # Liste hätte der Dienst alle Rechte seines Kontos, auch SeImpersonatePrivilege.
    $q = ScExe "qprivs $Dienstname"
    $privs = @([regex]::Matches($q.Text, 'Se[A-Za-z]+Privilege') | ForEach-Object { $_.Value } | Sort-Object -Unique)
    if ($q.Code -ne 0) {
        Write-Host "Rechte:     sc qprivs nicht lesbar ($($q.Text))"
    } elseif (($privs -join '/') -eq 'SeChangeNotifyPrivilege') {
        Write-Host 'Rechte:     nur SeChangeNotifyPrivilege (kein SeImpersonatePrivilege)'
    } else {
        Write-Host ("Rechte:     {0}; erwartet nur SeChangeNotifyPrivilege. Neu setzen: Dienst-Einrichten.ps1 als Administrator noch einmal laufen lassen." -f $(if ($privs) { $privs -join ', ' } else { 'keine Liste gesetzt (alle Rechte des Kontos, auch SeImpersonatePrivilege)' }))
        $gut = $false
    }
    $l = Leserecht
    Write-Host ("Leserecht:  {0}" -f $(if ($l) { "$l Neu setzen: einmal als Administrator Dienst-Einrichten.ps1 -NurLeserecht (siehe /anleitung, Das Dashboard als Dienst)." } elseif ($script:inhalt.Eigen) { 'Inhalt (Quellen, Daten, Entscheidungen), web, docs\zielbild und docs\anleitung (wenn da): ok' } else { 'projects, data, web, docs\entscheidungen, docs\zielbild und docs\anleitung (wenn da): ok' }))
    Write-Host ("Fassung:    {0}" -f $(if ($script:inhalt.Lokal) { "Zusammenbau dashboard-lokal in $($script:inhalt.Lokal) (mit Rechenteilen; gebaut bei -Neustart)" } else { 'allgemeines dashboard.exe: Die Projektseiten gehen, die Rechenteile der Projekte fehlen (keine Zusammenbau-Kiste dashboard-lokal neben dem Inhalt).' }))
    if ($l) { $gut = $false }
    $f = Get-ChildItem -LiteralPath $protokoll -Filter 'dienst-*.log' -ErrorAction SilentlyContinue | Sort-Object LastWriteTime | Select-Object -Last 1
    if ($f) {
        Write-Host "Protokoll:  $($f.FullName) (letzte 5 Zeilen)"
        foreach ($zeile in @(Get-Content -LiteralPath $f.FullName -Tail 5 -Encoding UTF8)) { Write-Host "  | $zeile" }
    } else {
        Write-Host "Protokoll:  noch keins in $protokoll"
    }
    $fb = Get-ChildItem -LiteralPath (Join-Path $protokoll 'bote') -Filter 'bote-*.log' -ErrorAction SilentlyContinue | Sort-Object LastWriteTime | Select-Object -Last 1
    if ($fb) { Write-Host "Bote-Log:   $($fb.FullName)" }
    if ($gut) { exit 0 }
    exit 1
}

# ---------------------------------------------------------------------------
# -Start, -Stopp
# ---------------------------------------------------------------------------
if ($Start) {
    $fehler = DienstStarten
    if ($fehler) { Ende 1 $fehler }
    BoteStarten
    $f = WarteAuf200 @('/healthz') 30
    if ($f) { Ende 1 "Der Dienst ist gestartet, antwortet aber nicht binnen 30 s ($f). Mehr mit: scripts\Dienst.ps1 -Status" }
    $u = SpeicherPruefen
    if (-not $u.Gut) { Ende 1 "Dienst $Dienstname läuft (http://localhost:$($script:port)/), aber ohne seine Daten: $($u.Satz) Mehr mit: scripts\Dienst.ps1 -Status" }
    Ende 0 "Dienst $Dienstname läuft: http://localhost:$($script:port)/"
}
if ($Stopp) {
    $nochDa = BoteStoppen
    $fehler = DienstStoppen
    if ($fehler) { Ende 1 $fehler }
    if ($nochDa) { Ende 1 "Dienst $Dienstname ist gestoppt, der Bote nicht: $nochDa. Mehr mit: scripts\Dienst.ps1 -Status" }
    Ende 0 "Dienst $Dienstname und Bote sind gestoppt. Wieder an: scripts\Dienst.ps1 -Start (beim nächsten Hochfahren startet der Dienst von selbst, der Bote bei der nächsten Anmeldung)."
}

# ---------------------------------------------------------------------------
# -Neustart
# ---------------------------------------------------------------------------
$env:Path = [Environment]::GetEnvironmentVariable('Path', 'Machine') + ';' +
            [Environment]::GetEnvironmentVariable('Path', 'User') + ';' +
            (Join-Path $env:ProgramFiles 'nodejs')
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) { Ende 1 'cargo wurde nicht gefunden. Ist Rust (rustup) installiert? Der Dienst läuft unverändert weiter.' }

# Bau der Fassung (Fassung-Bauen.ps1, dieselbe Folge wie in der Probe an einer
# Wegwerf-Kopie): Code-Repo bauen (--locked); gibt es dashboard-lokal neben dem
# Inhalt, zuerst der Wächter aus dem Repo (dashboard.exe pruefe-zusammenbau:
# liest nur, baut nichts; so laufen weder Bauskripte noch proc-macros, bevor er
# grün ist), erst dann cargo --config <repo>\.cargo\config.toml build
# --release --locked im selben Bauordner. Ist etwas rot, läuft der alte
# Dienst weiter. Das Programm der Kiste kommt als dashboard.exe nach bin; der
# Bote bleibt der aus dem Repo.
# Bauordner des Dienstes im Cache, getrennt von den Plätzen der Arbeiter (nie target
# des Repos). Die Sperrdatei hält die tägliche Bereinigung (Bauordner-Bereinigen.ps1)
# fern, bis die Programme in bin liegen; ihr finally entfernt sie wieder.
$bauordner = Join-Path $env:LOCALAPPDATA 'Dashboard\cache\dienst'
$sperre = Join-Path $bauordner 'EINSPIELEN.lock'
New-Item -ItemType Directory -Force -Path $bauordner | Out-Null
# cargo schreibt CACHEDIR.TAG nur in einen Ordner, den es selbst anlegt; ohne das Tag lässt die Bereinigung den Platz aus.
$tag = Join-Path $bauordner 'CACHEDIR.TAG'
if (-not (Test-Path -LiteralPath $tag)) { Set-Content -LiteralPath $tag -Value 'Signature: 8a477f597d28d172789f06886806bc55' -Encoding ascii }
Set-Content -LiteralPath $sperre -Value "$(Get-Date -Format o) PID $PID" -Encoding UTF8
try {
    $fassung = Fassung-Bauen $repo $script:inhalt.Lokal $bauordner
    if ($fassung.Code -ne 0) { Ende 1 $fassung.Satz }
    Write-Host $fassung.Satz
    $quelle = $fassung.Quelle
    # Die Bauliste und die Prüfung sehen alle Inhaltsquellen (wie der Dienst;
    # ohne [inhalt] die aus [pfade] oder projects im Repo).
    $env:DASHBOARD_PROJECTS_DIR = ($script:inhalt.Projekte -join ';')

    # Ab hier bis zum Austausch steht der Bote: Er schriebe sonst dieselbe
    # Bauliste, dieselben grünen Kopien und denselben stand.json (sein EIN_BAU
    # sperrt nur im eigenen Prozess), und vite läse womöglich seine vorläufige
    # Liste. Das finally startet ihn wieder, auf jedem Weg aus dem Abschnitt (Ende,
    # Ausnahme; Strg+C ist nicht zugesagt: Cmdlets im finally können danach selbst
    # abbrechen), aber nur, wenn keiner mehr läuft (sonst liefen zwei Wächter mit
    # zwei Boten). Ebenso den Dienst, solange $dienstAus gilt (von DienstStoppen
    # bis vor das DienstStarten am Ende): danach startet ihn der Abschnitt selbst,
    # und antwortet die neue Fassung nicht, übernimmt der Rückweg (nie doppelt).
    # shortcut: was in dieser Zeit gespeichert wird, baut erst die nächste Änderung (der Beobachter baut nach dem Neustart nicht); aufrüsten, wenn das stört (nach dem Start einmal bauen).
    Write-Host 'Halte den Boten an ...'
    $dienstAus = $false
    try {
        $nochDa = BoteStoppen
        if ($nochDa) { Ende 1 "Der Bote ließ sich nicht anhalten, nichts eingespielt ($nochDa). Der Dienst läuft unverändert weiter." }

        $web = Join-Path $repo 'web'
        $oberflaeche = $null
        if (Test-Path -LiteralPath (Join-Path $web 'package.json')) {
            if (-not (Get-Command npm -ErrorAction SilentlyContinue)) { Ende 1 'npm wurde nicht gefunden (Node.js unter C:\Program Files\nodejs?). Der Dienst läuft unverändert weiter.' }
            # Bauliste: dasselbe Urteil wie im Boten (bote::bau::bauliste_einspielen,
            # dieselben grünen Kopien und stand.json). Gebaut wird nur aus grünen
            # Kopien (was nach der Prüfung gespeichert wird, kommt so nicht in den
            # Bau); rot oder nicht geprüft: die letzte grüne Kopie, ohne fehlt es. Rote
            # werden genannt; abgebrochen wird nicht (sie fehlen nur auf ihrer
            # Projektseite). Bleibt die Gesamtprüfung rot, kommen alle letzten
            # grünen Kopien (Rückgabe 0). Scheitert die Prüfung selbst, ist die Liste leer
            # (Rückgabe 1, das Programm schreibt sie); nie bleibt die alte Datei stehen.
            $dashExe = Join-Path $bauordner 'release\dashboard.exe'
            $baulisteDatei = Join-Path $web 'src\lib\projekte.generated.ts'
            # Die leere Bauliste, genau wie src/werkzeug/scanner.rs sie schreibt.
            $leereBauliste = @(
                '// Erzeugt von dashboard pruefe --bauliste. Nicht von Hand ändern.',
                '',
                "type Laden = () => Promise<{ default: import('svelte').Component }>;",
                '',
                'export const projektSeiten: Record<string, Laden> = {};',
                '',
                'export const projektUnterseiten: Record<string, Laden> = {};',
                ''
            ) -join "`n"
            Write-Host 'Schreibe die Bauliste (dashboard.exe pruefe --bauliste --svelte, dieselbe Prüfung wie im Boten; nach einer neuen Prüfer-Fassung prüft sie alle Seiten, etwa eine Minute, sonst Sekunden) ...'
            # Die Aufgabe könnte den Boten inzwischen neu gestartet haben.
            if ((BoteProzesse).Count -gt 0) { Ende 1 'Der Bote läuft wieder (die Aufgabe hat ihn gestartet?), nichts eingespielt. Der Dienst läuft unverändert weiter.' }
            $altKodierung = [Console]::OutputEncoding
            Push-Location $repo
            try {
                [Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
                $ErrorActionPreference = 'Continue'
                $bauliste = @(& $dashExe pruefe --bauliste --svelte 2>&1 | ForEach-Object { "$_" })
                $code = $LASTEXITCODE
                $ErrorActionPreference = 'Stop'
            } finally {
                Pop-Location
                [Console]::OutputEncoding = $altKodierung
            }
            foreach ($zeile in $bauliste) { Write-Host "  $zeile" }
            $ohneSeiten = $false
            if ($code -eq 1) {
                $ohneSeiten = $true
                Write-Host 'Hinweis: Die Prüfung für die Bauliste ist gescheitert; die Bauliste ist leer, gebaut wird ohne Projektseiten.'
            } elseif ($code -ne 0) {
                try {
                    [IO.File]::WriteAllText($baulisteDatei, $leereBauliste, [Text.UTF8Encoding]::new($false))
                } catch {
                    Ende 1 "pruefe --bauliste endete mit Code $code, und die leere Bauliste ließ sich nicht schreiben ($($_.Exception.Message)). Gebaut wird nicht mit der alten Liste. Der Dienst läuft unverändert weiter."
                }
                $ohneSeiten = $true
                Write-Host "Hinweis: pruefe --bauliste endete mit Code $code; die Bauliste ist jetzt leer, gebaut wird ohne Projektseiten."
            }
            $fehlen = @($bauliste | ForEach-Object { if ($_ -match '^FEHLT (.+)$') { $Matches[1] } })
            if ($fehlen.Count -gt 0) { Write-Host "Ohne Seite in diesem Bau (rot oder nicht geprüft, ohne grüne Kopie): $($fehlen -join ', ')" }
            $gebaut = @($bauliste | ForEach-Object { if ($_ -match '^GEBAUT (.+)$' -and $Matches[1] -ne 'keins') { $Matches[1] } })
            Write-Host 'Baue die Oberfläche (web\build) ...'
            Push-Location $web
            try {
                $ErrorActionPreference = 'Continue'
                npm run build
                $code = $LASTEXITCODE
                $ErrorActionPreference = 'Stop'
            } finally { Pop-Location }
            if ($code -ne 0 -and -not $ohneSeiten -and $gebaut.Count -gt 0) {
                # Ein Projekt im Bau bricht ihn (etwas, das svelte-check nicht
                # sieht): einmal ohne Projektseiten, damit das Dashboard selbst neu kommt.
                Write-Host "npm run build ist mit den Projekten im Bau ($($gebaut -join ', ')) gescheitert. Baue einmal ohne Projektseiten ..."
                try {
                    [IO.File]::WriteAllText($baulisteDatei, $leereBauliste, [Text.UTF8Encoding]::new($false))
                } catch {
                    Ende 1 "Die leere Bauliste ließ sich nicht schreiben ($($_.Exception.Message)). Der Dienst läuft unverändert weiter."
                }
                Push-Location $web
                try {
                    $ErrorActionPreference = 'Continue'
                    npm run build
                    $code = $LASTEXITCODE
                    $ErrorActionPreference = 'Stop'
                } finally { Pop-Location }
                if ($code -eq 0) { Write-Host 'Hinweis: Gebaut ohne Projektseiten. Erst wenn das Projekt repariert ist, kommen die Seiten mit dem nächsten Bau zurück.' }
            }
            if ($code -ne 0) { Ende 1 'npm run build ist gescheitert. Der Dienst läuft unverändert weiter.' }
            # Die Kopie für den Dienst, noch bei laufendem Dienst (er liest bin\web, nicht web.neu),
            # und dieselbe neben den Programmen im Bauordner für Dienst-Einrichten.ps1.
            $f = OberflaecheBereitlegen (Join-Path $web 'build') (Join-Path $quelle 'web')
            if ($f) { Ende 1 "Die Oberfläche ließ sich nicht nach $($script:bin)\web.neu kopieren ($f). Der Dienst läuft unverändert weiter." }
            $oberflaeche = Join-Path $script:bin 'web.neu'
        }

        # $quelle: <bauordner>\release oder (mit Zusammenbau) <bauordner>\release\zusammenbau.
        foreach ($n in $programme) {
            if (-not (Test-Path -LiteralPath (Join-Path $quelle $n))) { Ende 1 "$n fehlt in $quelle. Der Dienst läuft unverändert weiter." }
        }

        $beginn = Get-Date
        Write-Host 'Halte den Dienst an ...'
        $dienstAus = $true
        $fehler = DienstStoppen
        if ($fehler) { Ende 1 "$fehler Es wurde nichts ausgetauscht." }

        # Den Zeiger auf den letzten Bau über den Boten entfernen: nach dem Neustart
        # liefert der Dienst sicher die eben gebaute Fassung aus bin\web aus.
        $zeiger = Join-Path $repo 'web\builds\aktuell'
        if (Test-Path -LiteralPath $zeiger -PathType Leaf) {
            try {
                Remove-Item -LiteralPath $zeiger -Force -ErrorAction Stop
                Write-Host 'web\builds\aktuell entfernt: die Oberfläche kommt jetzt aus bin\web.'
            } catch {
                Write-Host "Hinweis: web\builds\aktuell ließ sich nicht entfernen ($($_.Exception.Message)); der Dienst nimmt dann weiter den letzten Bau des Boten."
            }
        }

        $fehler = TauscheProgramme $quelle $oberflaeche
        if ($fehler) { Ende 1 "$fehler Dienst und Bote laufen mit der vorigen Fassung weiter (sofern sie starten; scripts\Dienst.ps1 -Status zeigt es)." }
        Write-Host "Neue Programme liegen in $($script:bin) (vorige als *.alt.exe)$(if ($script:oberflaecheGetauscht) { ', die Oberfläche in bin\web (vorige in bin\web.alt)' })"

        $dienstAus = $false
        $fehler = DienstStarten
    } finally {
        if ($dienstAus) {
            # Abbruch mitten in DienstStoppen: sc start nähme StopPending nicht an (1061).
            if ((DienstStatus) -eq 'StopPending') { $null = WarteAufStatus 'Stopped' 20 }
            if ((DienstStatus) -ne 'Running') {
                # Welche Fassung im bin liegt, hängt davon ab, wie weit das try kam.
                $startFehler = DienstStarten -ohneEnde
                if ($startFehler) { Write-Host "Der Dienst startet nicht: $startFehler" }
            }
        }
        if ((BoteProzesse).Count -eq 0) { BoteStarten } else { Write-Host 'Bote läuft noch, nicht neu gestartet.' }
    }
} finally {
    # Auf jedem Weg aus dem Abschnitt (Ende, Ausnahme): bis hier liegen die Programme in bin, oder es wurde nichts eingespielt.
    Remove-Item -LiteralPath $sperre -Force -ErrorAction SilentlyContinue
}
if (-not $fehler) { $fehler = WarteAuf200 @('/', '/api/daten') 60 }
if (-not $fehler) {
    # Die neue Fassung antwortet. /api/daten antwortet auch ohne Datenspeicher
    # 200; darum eigens prüfen (mit Wiederholung). Wegen der Daten wird nicht
    # zurückgerollt (NachStartEntscheidung): dann nur ehrlich melden.
    $u = SpeicherPruefen
    $dauer = [int]((Get-Date) - $beginn).TotalSeconds
    $e = NachStartEntscheidung $u $dauer
    if ($e.Weg -eq 'fertig') { Ende 0 "$($e.Satz) http://localhost:$($script:port)/" }
    Ende 1 "$($e.Satz) Protokoll: $protokoll"
}

# Zurück zur vorigen Fassung. Move-Item -Force ersetzt die neue Datei in
# einem Schritt; scheitert es, bleibt die neue liegen (nie ohne dashboard.exe).
# Anders als oben (dort nur, solange $dienstAus gilt) startet das finally hier
# immer, was nicht läuft (Dienst und Bote), auf jedem Weg aus dem Abschnitt
# (Ende, Ausnahme; Strg+C nicht zugesagt), und nur dort (nie doppelt).
Write-Host "Die neue Fassung antwortet nicht ($fehler). Hole die vorige Fassung zurück ..."
$neuGeblieben = @()
try {
    $nochDa = BoteStoppen
    if ($nochDa) { Write-Host "Hinweis: $nochDa; die Datei wird trotzdem umbenannt und ersetzt." }
    $null = DienstStoppen
    foreach ($n in $programme) {
        $z = Join-Path $script:bin $n
        $alt = Join-Path $script:bin ($n -replace '\.exe$', '.alt.exe')
        if (Test-Path -LiteralPath $alt) {
            try { Move-Item -LiteralPath $alt -Destination $z -Force -ErrorAction Stop }
            catch {
                Write-Host "Achtung: $alt ließ sich nicht zurückholen ($($_.Exception.Message)); $n bleibt die neue Fassung."
                $neuGeblieben += $n
            }
        }
    }
    # Die Oberfläche gehört zur Fassung: nur zurück, wenn dieser Lauf sie getauscht
    # hat und dashboard.exe wieder die vorige ist (sonst passten sie nicht zusammen).
    if ($script:oberflaecheGetauscht -and -not ($neuGeblieben -contains 'dashboard.exe')) {
        $f = OberflaecheZurueck
        if ($f) { Write-Host "Achtung: $f" }
        elseif ($script:webVorher) { Write-Host 'Die vorige Oberfläche liegt wieder in bin\web.' }
        else { Write-Host 'Die neue Oberfläche in bin\web ist wieder entfernt; die vorige Fassung liefert wie bisher aus web\build.' }
    }
} finally {
    $fehler2 = $null
    # Abbruch mitten in DienstStoppen: sc start nähme StopPending nicht an (1061).
    if ((DienstStatus) -eq 'StopPending') { $null = WarteAufStatus 'Stopped' 20 }
    if ((DienstStatus) -ne 'Running') {
        $fehler2 = DienstStarten -ohneEnde
        # Welche Fassung im bin liegt, hängt davon ab, wie weit das try kam.
        if ($fehler2) { Write-Host "Der Dienst startet nicht: $fehler2" }
    }
    if ((BoteProzesse).Count -eq 0) { BoteStarten } else { Write-Host 'Bote läuft noch, nicht neu gestartet.' }
}
if (-not $fehler2) { $fehler2 = WarteAuf200 @('/', '/api/daten') 60 }
$dauer = [int]((Get-Date) - $beginn).TotalSeconds
if ($fehler2) {
    # Der Satz nennt die Ursache: hängender Stopp, nicht zurückgeholte Datei oder die vorige Fassung selbst.
    # Antworten muss dashboard.exe; ein nicht zurückgeholter Bote wird nur mitgenannt.
    $warum = if ((DienstStatus) -eq 'StopPending') { 'Der Dienst hängt beim Anhalten (nach über 40 s noch StopPending), darum startete keine Fassung' }
             elseif ($neuGeblieben -contains 'dashboard.exe') { "Die vorige Fassung ließ sich nicht zurückholen ($($neuGeblieben -join ', ') blieb die neue), und das Dashboard antwortet nicht" }
             elseif ($neuGeblieben.Count -gt 0) { "Auch die vorige Fassung antwortet nicht (nur $($neuGeblieben -join ', ') blieb die neue)" }
             else { 'Auch die vorige Fassung antwortet nicht' }
    Ende 1 "$warum ($fehler2). Das Dashboard ist seit $dauer s weg. Bitte scripts\Dienst.ps1 -Status ansehen und das Protokoll in $protokoll."
}
# Die vorige Fassung antwortet; hat sie auch ihren Datenspeicher? Hat die neue
# die Datei schon umgebaut (höheres Schema), verweigert die vorige sie sauber
# und läuft ohne. Das wird hier ehrlich gesagt, samt Weg zurück.
$u = SpeicherPruefen
if ($u.Art -eq 'zu-neu') {
    Ende 1 ((ZurueckSatz $u $script:sicherungsOrdner $script:datenDatei $dauer) + [Environment]::NewLine + "Grund für das Scheitern der neuen Fassung im Protokoll: $protokoll")
}
if (-not $u.Gut) {
    Ende 1 "Die neue Fassung startete nicht. Die vorige läuft wieder (Unterbrechung $dauer s), aber ohne ihre Daten: $($u.Satz) Aufgaben, Vorgänge und Warten gehen gerade nicht. Bitte scripts\Dienst.ps1 -Status ansehen und das Protokoll in $protokoll."
}
Ende 1 "Die neue Fassung startete nicht; die vorige läuft wieder, mit ihren Daten (Unterbrechung $dauer s). Grund im Protokoll: $protokoll"
