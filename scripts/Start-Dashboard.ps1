<#
.SYNOPSIS
    Startet das Dashboard und öffnet es im Browser.
.DESCRIPTION
    Baut die Oberfläche (web\, braucht Node.js) und das
    Programm mit cargo, startet es auf 127.0.0.1:8787 und öffnet die
    Startseite. Beenden mit Strg+C.
.PARAMETER Port
    Anderer Port statt 8787.
.PARAMETER NoBrowser
    Browser nicht automatisch öffnen.
.PARAMETER SkipWeb
    Oberfläche nicht neu bauen (schneller, wenn sich in web\ nichts geändert hat).
.NOTES
    Ist das Dashboard als Dienst eingerichtet (scripts\Dienst-Einrichten.ps1),
    startet dieses Skript auf Port 8787 kein zweites Dashboard, sondern sagt,
    wie man den Dienst neu startet. Mit anderem -Port (etwa 8788 für einen
    Testserver) geht es wie bisher.

    Exit-Codes:
      0  normal beendet, oder das Dashboard läuft als Dienst
      3  der Port ist schon belegt
#>
[CmdletBinding()]
param(
    [int]$Port = 8787,
    [switch]$NoBrowser,
    [switch]$SkipWeb
)

$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot

# Gesamtlaufzeit und mittlere Auslastung aller Kerne (Zähler des Systems, also
# auch npm und cargo) für die letzte Zeile jedes Laufs (Laufzeit).
$uhr = [Diagnostics.Stopwatch]::StartNew()
$kerneStart = Get-CimInstance Win32_PerfRawData_PerfOS_Processor -Filter "Name='_Total'" -ErrorAction SilentlyContinue
function Laufzeit {
    $jetzt = Get-CimInstance Win32_PerfRawData_PerfOS_Processor -Filter "Name='_Total'" -ErrorAction SilentlyContinue
    $last = 'unbekannt'
    if ($kerneStart -and $jetzt -and $jetzt.Timestamp_Sys100NS -gt $kerneStart.Timestamp_Sys100NS) {
        # Der Rohwert zählt die Leerlaufzeit (PERF_100NSEC_TIMER_INV).
        $last = '{0:P0}' -f (1 - ($jetzt.PercentProcessorTime - $kerneStart.PercentProcessorTime) / ($jetzt.Timestamp_Sys100NS - $kerneStart.Timestamp_Sys100NS))
    }
    Write-Host ('Gesamtlaufzeit {0:N1} s, Kernauslastung im Mittel {1} über {2} Kerne' -f $uhr.Elapsed.TotalSeconds, $last, [Environment]::ProcessorCount)
}

# Läuft das Dashboard als Dienst, nie ein zweites auf 8787 starten.
if ($Port -eq 8787 -and (Get-Service -Name 'Dashboard' -ErrorAction SilentlyContinue)) {
    Write-Host 'Das Dashboard läuft als Dienst. Neustart: scripts\Dienst.ps1 -Neustart, Status: scripts\Dienst.ps1 -Status'
    Laufzeit
    exit 0
}

# Ist der Port schon belegt, gleich aufhören statt erst zu bauen.
if (Get-Command Get-NetTCPConnection -ErrorAction SilentlyContinue) {
    $belegt = @(Get-NetTCPConnection -LocalPort $Port -State Listen -ErrorAction SilentlyContinue)
    if ($belegt.Count -gt 0) {
        $wer = $null
        try { $wer = (Get-Process -Id $belegt[0].OwningProcess -ErrorAction Stop).Path } catch { }
        if (-not $wer) { $wer = "Prozess $($belegt[0].OwningProcess)" }
        Write-Host "Port $Port ist schon belegt ($wer). Läuft das Dashboard schon in einem anderen Fenster? Sonst mit -Port einen anderen wählen."
        Laufzeit
        exit 3
    }
}

# PowerShell 5.1 kennt frisch installierte Programme erst nach Neuladen des Pfads.
$env:Path = [Environment]::GetEnvironmentVariable('Path', 'Machine') + ';' +
            [Environment]::GetEnvironmentVariable('Path', 'User')

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    Write-Error "cargo wurde nicht gefunden. Ist Rust (rustup) installiert?"
}

# Oberfläche (SvelteKit) bauen; ohne sie zeigt der Server nur einen Hinweis.
$web = Join-Path $repo 'web'
if (-not $SkipWeb -and (Test-Path (Join-Path $web 'package.json'))) {
    if (Get-Command npm -ErrorAction SilentlyContinue) {
        Push-Location $web
        try {
            if (-not (Test-Path (Join-Path $web 'node_modules'))) {
                Write-Host "Installiere npm-Pakete für web\ (einmalig) …"
                npm ci --no-fund --no-audit
            }
            Write-Host "Baue die Oberfläche (web\build) …"
            # Schritt E2b: Was die Probe hinterlässt (oeffentlich.json.fund, oeffentlich.blind),
            # gilt nur für diesen Bau; ein alter Befund fiele sonst auf einen Bau, der vorher scheitert.
            $app = Join-Path $web 'build\_app'
            $fund = Join-Path $app 'oeffentlich.json.fund'
            $blind = Join-Path $app 'oeffentlich.blind'
            foreach ($p in $fund, $blind) {
                Remove-Item -LiteralPath $p -Force -ErrorAction SilentlyContinue
                if (Test-Path -LiteralPath $p) {
                    Laufzeit
                    throw "$p ließ sich nicht entfernen; gebaut wird nicht (der alte Befund der Probe gälte sonst für diesen Bau). Der Dienst startet nicht."
                }
            }
            npm run build
            $code = $LASTEXITCODE
            # Ein roter Bau (auch ein Fund oder Blindheit der Probe) startet keinen Server auf web\build.
            if ($code -ne 0) {
                Laufzeit
                # Nach Fund oder Blindheit hilft -SkipWeb nicht: Der Bau ist verschlossen oder ungeprüft.
                if (Test-Path -LiteralPath $fund) {
                    throw "Sicherheitsfund im Bau: Öffentliche Dateien der Oberfläche nennen Namen oder Texte eigener Projektseiten (Satz der Probe oben); der Dienst startet nicht. Kein -SkipWeb: Der Bau ist verschlossen oder darf nicht ausgeliefert werden. Erst die Oberfläche (web\src) richten, dann neu bauen."
                }
                if (Test-Path -LiteralPath $blind) {
                    throw "Die Probe kann den Bau nicht prüfen (Satz oben); kein Fehler eines Projekts. Der Dienst startet nicht. Kein -SkipWeb: Der Bau wäre ungeprüft. Erst die Probe oder die Bauliste richten (dashboard pruefe --bauliste --svelte), dann neu bauen."
                }
                throw "Der Bau der Oberfläche ist gescheitert (Rückgabe $code); der Dienst startet nicht. Ohne Neubau starten: -SkipWeb."
            }
        } finally {
            Pop-Location
        }
    } else {
        Write-Error "Node.js fehlt; ohne web\build gibt es keine Oberfläche. Installieren: winget install OpenJS.NodeJS.LTS"
    }
}

$env:DASHBOARD_PORT = "$Port"
# Browser-Adresse localhost: nur dort meldet Windows im Browser von selbst an (7b).
$url = "http://localhost:$Port/"

Write-Host "Dashboard startet unter $url (Beenden mit Strg+C)"

if (-not $NoBrowser) {
    Start-Job -ScriptBlock {
        param($u)
        for ($i = 0; $i -lt 60; $i++) {
            try {
                Invoke-WebRequest -Uri "$u/healthz" -UseBasicParsing -TimeoutSec 2 | Out-Null
                Start-Process $u
                return
            } catch { Start-Sleep -Seconds 2 }
        }
    } -ArgumentList $url.TrimEnd('/') | Out-Null
}

Push-Location $repo
try {
    cargo run --release
} finally {
    Pop-Location
    Laufzeit
}
