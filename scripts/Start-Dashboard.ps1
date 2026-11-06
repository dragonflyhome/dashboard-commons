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

# Läuft das Dashboard als Dienst, nie ein zweites auf 8787 starten.
if ($Port -eq 8787 -and (Get-Service -Name 'Dashboard' -ErrorAction SilentlyContinue)) {
    Write-Host 'Das Dashboard läuft als Dienst. Neustart: scripts\Dienst.ps1 -Neustart, Status: scripts\Dienst.ps1 -Status'
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
            npm run build
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
}
