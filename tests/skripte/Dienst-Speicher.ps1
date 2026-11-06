<#
.SYNOPSIS
    Prüft die Speicher-Prüfung von scripts\Dienst.ps1 mit gestellten Antworten.
.DESCRIPTION
    Dienst.ps1 verlangt nach jedem Start einen offenen Datenspeicher
    (SpeicherUrteil), nimmt nur Antworten mit HTTP 200 (JsonAusAntwort),
    entscheidet nach dem Start der neuen Fassung, ob es nur meldet statt
    zurückzurollen (NachStartEntscheidung), und sagt nach dem Zurückrollen
    ehrlich, wenn die vorige Fassung ohne Daten läuft (ZurueckSatz). Dieses
    Skript liest Dienst.ps1 als AST, holt nur diese Funktionen heraus (der Rest des
    Skripts läuft nicht, es fasst keinen Dienst an) und ruft sie mit
    gestellten Antworten von /api/betrieb und /api/daten auf: neue Fassung mit
    offenem Speicher, mit Fehler, mit zu neuer Datei; vorige Fassung (ohne Feld
    speicher) mit zu neuer Datei, mit anderem Fehler, mit Speicher; keine
    Antwort. ZurueckSatz bekommt einen Sicherungsordner im Temp-Ordner mit
    erfundenen Dateien, der danach wieder verschwindet.

    Ohne Pester, ohne Administrator, ohne Netz. Aufruf aus dem Repo, in beiden Shells:

        pwsh -NoProfile -File tests\skripte\Dienst-Speicher.ps1
        powershell.exe -NoProfile -ExecutionPolicy Bypass -File tests\skripte\Dienst-Speicher.ps1
.NOTES
    Exit-Codes: 0 alles gut, 1 mindestens eine Prüfung gescheitert.
#>
[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$skript = Join-Path (Join-Path $repo 'scripts') 'Dienst.ps1'

$token = $null; $parseFehler = $null
$ast = [System.Management.Automation.Language.Parser]::ParseFile($skript, [ref]$token, [ref]$parseFehler)
if ($parseFehler.Count -gt 0) {
    foreach ($f in $parseFehler) { Write-Host ("FEHLER Dienst.ps1:{0}: {1}" -f $f.Extent.StartLineNumber, $f.Message) }
    exit 1
}
$gesucht = @('JsonAusAntwort', 'SpeicherUrteil', 'NachStartEntscheidung', 'ZurueckSatz')
$funktionen = $ast.FindAll({ param($n) $n -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $gesucht -contains $n.Name }, $true)
foreach ($name in $gesucht) {
    $f = @($funktionen | Where-Object { $_.Name -eq $name })
    if ($f.Count -ne 1) { Write-Host "FEHLER Dienst.ps1: Funktion $name fehlt (oder steht doppelt)"; exit 1 }
    . ([ScriptBlock]::Create($f[0].Extent.Text))
}

$script:fehler = 0
function Pruefe([string]$was, [bool]$ok, [string]$sonst = '') {
    if ($ok) { Write-Host "ok     $was" }
    else { Write-Host "FEHLER $was $sonst"; $script:fehler++ }
}

function Json([string]$text) { $text | ConvertFrom-Json }

# Sätze wie der Server (SpeicherFehler::ZuNeu, AppState::speicher_satz).
$zuNeu43 = 'Die Datenbank wurde von einer neueren Fassung des Dashboards geschrieben (Schema 4, dieses Programm kennt 3). Bitte das neuere Programm starten; die Datei bleibt unverändert.'
$altZuNeu = 'Der Datenspeicher lässt sich nicht öffnen. Die letzten Sicherungen liegen in C:\ProgramData\Dashboard\sicherung. Die Datenbank wurde von einer neueren Fassung des Dashboards geschrieben (Schema 3, dieses Programm kennt 2). Bitte das neuere Programm starten; die Datei bleibt unverändert.'

# 1. Neue Fassung, Speicher offen.
$u = SpeicherUrteil (Json '{"modus":"dienst","speicher":{"offen":true,"schema":3,"programm_schema":3,"zu_neu":false,"fehler":null}}') $null
Pruefe 'neu, offen: gut' ($u.Gut -and $u.Art -eq 'gut' -and $u.Schema -eq 3) ($u | Out-String)

# 2. Neue Fassung, Speicher offen, aber nicht lesbar: kein Gut.
$u = SpeicherUrteil (Json '{"speicher":{"offen":true,"schema":null,"programm_schema":3,"zu_neu":false,"fehler":"Der Datenspeicher lässt sich nicht lesen oder schreiben."}}') $null
Pruefe 'neu, offen mit fehler: nicht gut' (-not $u.Gut -and $u.Art -eq 'fehler' -and $u.Satz -like '*nicht lesen*') ($u | Out-String)

# 3. Neue Fassung, Datei beschädigt.
$u = SpeicherUrteil (Json '{"speicher":{"offen":false,"schema":null,"programm_schema":3,"zu_neu":false,"fehler":"Ein Eintrag im Datenspeicher ist beschädigt."}}') (Json '{"fehler":"x","schema":0}')
Pruefe 'neu, nicht offen: fehler (betrieb zählt vor daten)' (-not $u.Gut -and $u.Art -eq 'fehler' -and $u.Satz -eq 'Ein Eintrag im Datenspeicher ist beschädigt.') ($u | Out-String)

# 4. Neue Fassung, Datei zu neu.
$u = SpeicherUrteil (Json ('{"speicher":{"offen":false,"schema":4,"programm_schema":3,"zu_neu":true,"fehler":' + (ConvertTo-Json $zuNeu43) + '}}')) $null
Pruefe 'neu, zu neu: zu-neu mit Schema 4, kennt 3' (-not $u.Gut -and $u.Art -eq 'zu-neu' -and $u.Schema -eq 4 -and $u.Kennt -eq 3) ($u | Out-String)

# 5. Vorige Fassung (kein Feld speicher), Datei schon Schema 3: /api/daten 200 mit fehler.
$daten = Json ('{"schema":0,"tabellen":[],"fehler":' + (ConvertTo-Json $altZuNeu) + '}')
$u = SpeicherUrteil (Json '{"modus":"dienst","bote":"verbunden"}') $daten
Pruefe 'vorige, zu neu: zu-neu aus /api/daten, Schema 3, kennt 2' (-not $u.Gut -and $u.Art -eq 'zu-neu' -and $u.Schema -eq 3 -and $u.Kennt -eq 2) ($u | Out-String)

# 6. Vorige Fassung, anderer Fehler.
$u = SpeicherUrteil (Json '{"modus":"dienst"}') (Json '{"schema":0,"fehler":"Der Datenspeicher lässt sich nicht öffnen. Die Datenbank ist schon geöffnet."}')
Pruefe 'vorige, anderer Fehler: fehler' (-not $u.Gut -and $u.Art -eq 'fehler') ($u | Out-String)

# 7. Vorige Fassung mit Speicher.
$u = SpeicherUrteil (Json '{"modus":"dienst"}') (Json '{"schema":2,"fehler":null}')
Pruefe 'vorige, offen: gut' ($u.Gut -and $u.Schema -eq 2) ($u | Out-String)

# 8. Keine Antworten: nie gut.
$u = SpeicherUrteil $null $null
Pruefe 'ohne Antworten: unbekannt, nicht gut' (-not $u.Gut -and $u.Art -eq 'unbekannt') ($u | Out-String)

# 9. ZurueckSatz: nennt die neueste passende Sicherung, den Weg als Administrator und den Verlust.
$ordner = Join-Path ([IO.Path]::GetTempPath()) ("dashboard-dienst-speicher-{0}" -f [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $ordner | Out-Null
try {
    foreach ($n in @('dashboard-2026-10-20-vor-schema-2.redb', 'dashboard-2026-10-28-vor-schema-2.redb', 'dashboard-2026-10-29-vor-schema-1.redb', 'dashboard-2026-10-29.redb')) {
        [IO.File]::WriteAllText((Join-Path $ordner $n), 'erfunden')
    }
    $u = [pscustomobject]@{ Gut = $false; Art = 'zu-neu'; Satz = $altZuNeu; Schema = 3; Kennt = 2 }
    $satz = ZurueckSatz $u $ordner 'C:\ProgramData\Dashboard\daten\dashboard.redb' 42
    Pruefe 'ZurueckSatz nennt die neueste vor-schema-2' ($satz -like "*$(Join-Path $ordner 'dashboard-2026-10-28-vor-schema-2.redb')*") $satz
    Pruefe 'ZurueckSatz sagt einfach: ohne ihre Daten; Aufgaben, Vorgänge und Warten gehen gerade nicht' ($satz -like '*ohne ihre Daten*' -and $satz -like '*Aufgaben, Vorgänge und Warten gehen gerade nicht*') $satz
    Pruefe 'ZurueckSatz ohne Fachwörter (kein Schema, 503, /daten)' (-not ($satz -match 'Schema [0-9]|503|/daten|Datenspeicher')) $satz
    Pruefe 'ZurueckSatz: als Administrator, stoppen, ersetzen, starten' ($satz -like '*Administrator*' -and $satz -like '*-Stopp*' -and $satz -like '*-Start*' -and $satz -like '*dashboard.redb umbenennen*') $satz
    Pruefe 'ZurueckSatz: fehlt dann, bleibt in der umbenannten Datei' ($satz -like '*fehlt dann; es bleibt in der umbenannten Datei erhalten*' -and $satz -notlike '*verloren*') $satz
    Pruefe 'ZurueckSatz: Unterbrechung und LIESMICH' ($satz -like '*42 s*' -and $satz -like '*LIESMICH*') $satz
    $u.Kennt = 5
    $satz = ZurueckSatz $u $ordner 'x.redb' 1
    Pruefe 'ZurueckSatz ohne passende Sicherung: sagt es' ($satz -like '*keine gefunden im Ordner*') $satz
} finally {
    Remove-Item -LiteralPath $ordner -Recurse -Force -ErrorAction SilentlyContinue
}

# 10. Nur HTTP 200 zählt als Antwort; eine 401 oder 503 mit Feld fehler nicht.
$a = JsonAusAntwort '401' '{"fehler":"Bitte mit dem Windows-Konto anmelden.","art":"anmeldung"}'
Pruefe 'JsonAusAntwort 401 mit fehler: keine Daten' (($null -eq $a.Daten) -and $a.Code -eq '401') ($a | Out-String)
$a = JsonAusAntwort '503' '{"fehler":"x","art":"speicher-fehlt"}'
Pruefe 'JsonAusAntwort 503: keine Daten' ($null -eq $a.Daten)
$a = JsonAusAntwort '000' $null
Pruefe 'JsonAusAntwort ohne Verbindung: keine Daten' (($null -eq $a.Daten) -and $a.Code -eq '000')
$a = JsonAusAntwort '200' 'kein json'
Pruefe 'JsonAusAntwort 200 ohne JSON: keine Daten' ($null -eq $a.Daten)
$a = JsonAusAntwort '200' '{"schema":2,"fehler":null}'
Pruefe 'JsonAusAntwort 200: Daten' ($a.Daten.schema -eq 2)
$b = JsonAusAntwort '200' '{"modus":"dienst"}'
$d = JsonAusAntwort '401' '{"fehler":"Bitte mit dem Windows-Konto anmelden.","art":"anmeldung"}'
$u = SpeicherUrteil $b.Daten $d.Daten
Pruefe 'vorige mit 401 auf /api/daten: unbekannt, nicht fehler' ($u.Art -eq 'unbekannt') ($u | Out-String)

# 11. Nach dem Start der neuen Fassung: nur bei gut fertig, sonst melden; nie zurückrollen.
$gut = [pscustomobject]@{ Gut = $true; Art = 'gut'; Satz = 'Die Daten sind da.'; Schema = 3; Kennt = 3 }
$e = NachStartEntscheidung $gut 12
Pruefe 'Entscheidung gut: fertig' ($e.Weg -eq 'fertig' -and $e.Satz -like '*12 s*') ($e | Out-String)
$unbekannt = [pscustomobject]@{ Gut = $false; Art = 'unbekannt'; Satz = 'Ob die Daten da sind, ließ sich nicht feststellen (keine gültige Antwort: /api/betrieb 000, /api/daten 000).'; Schema = $null; Kennt = $null }
$e = NachStartEntscheidung $unbekannt 12
Pruefe 'Entscheidung unbekannt: melden, nicht zurückrollen' ($e.Weg -eq 'melden' -and $e.Satz -like '*nicht feststellen*' -and $e.Satz -like '*läuft weiter*') ($e | Out-String)
$gleich = [pscustomobject]@{ Gut = $false; Art = 'fehler'; Satz = 'Der Datenspeicher lässt sich nicht lesen oder schreiben.'; Schema = 3; Kennt = 3 }
$e = NachStartEntscheidung $gleich 12
Pruefe 'Entscheidung Fehler bei gleichem Schema: melden (umgestellt)' ($e.Weg -eq 'melden' -and $e.Satz -like '*schon auf die neue Fassung umgestellt*' -and $e.Satz -like '*Aufgaben, Vorgänge und Warten gehen gerade nicht*') ($e | Out-String)
$zuNeu = [pscustomobject]@{ Gut = $false; Art = 'zu-neu'; Satz = $zuNeu43; Schema = 4; Kennt = 3 }
$e = NachStartEntscheidung $zuNeu 12
Pruefe 'Entscheidung zu-neu: melden' ($e.Weg -eq 'melden' -and $e.Satz -like '*noch neueren Fassung*') ($e | Out-String)
$anders = [pscustomobject]@{ Gut = $false; Art = 'fehler'; Satz = 'Die Datenbank ist schon geöffnet.'; Schema = $null; Kennt = 3 }
$e = NachStartEntscheidung $anders 12
Pruefe 'Entscheidung anderer Fehler: melden, ohne „umgestellt“' ($e.Weg -eq 'melden' -and $e.Satz -notlike '*umgestellt.*') ($e | Out-String)

# 12. Dienst.ps1 prüft den Speicher nach dem Start und nach dem Zurückrollen.
$text = [IO.File]::ReadAllText($skript)
$nachZurueck = $text.Substring([Math]::Max(0, $text.IndexOf('Hole die vorige Fassung zur')))
Pruefe 'Dienst.ps1: nach dem Zurückrollen SpeicherPruefen und ZurueckSatz' ($nachZurueck -match 'SpeicherPruefen' -and $nachZurueck -match 'ZurueckSatz')
$vorZurueck = $text.Substring(0, [Math]::Max(0, $text.IndexOf('Hole die vorige Fassung zur')))
Pruefe 'Dienst.ps1: nach dem neuen Start SpeicherPruefen' ($vorZurueck.LastIndexOf('SpeicherPruefen') -gt $vorZurueck.LastIndexOf("WarteAuf200 @('/', '/api/daten') 60"))
$nachPruefung = $vorZurueck.Substring($vorZurueck.LastIndexOf('SpeicherPruefen'))
Pruefe 'Dienst.ps1: nach der Speicherprüfung endet es (kein Zurückrollen wegen der Daten)' ($nachPruefung -match 'NachStartEntscheidung' -and $nachPruefung -match 'Ende 0 ' -and $nachPruefung -match 'Ende 1 ')

Write-Host ("PowerShell {0}" -f $PSVersionTable.PSVersion)
if ($script:fehler) { exit 1 }
exit 0
