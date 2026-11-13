<#
.SYNOPSIS
    Probe: übersetzt die Bibliothek für macOS (aarch64-apple-darwin), ohne Abbruch.
.DESCRIPTION
    cargo check --lib --profile test --target aarch64-apple-darwin. Das Profil
    test, weil die Bibliothek außerhalb von Windows ohne cfg(test) absichtlich
    nicht baut (Verschlüsselung nur mit DPAPI).

    Bei --target gelten die rustflags aus .cargo/config.toml nicht für die
    Abhängigkeiten des Hosts; windows-sys (raw-dylib) ruft dann das dlltool
    des self-contained-Ordners, dem der Assembler fehlt. Darum liegt
    llvm-dlltool.exe als dlltool.exe in einem eigenen Ordner im Anwendungscache
    und steht nur für diesen Lauf vorn im PATH.

    Eine Probe: Das Ergebnis steht da, der Exit-Code ist immer 0. Fehlt das Ziel
    oder llvm-dlltool, sagt sie es in einem Satz (rustup target add
    aarch64-apple-darwin; rustup component add llvm-tools).
.PARAMETER Ziel
    Das Ziel (Standard aarch64-apple-darwin).
#>
[CmdletBinding()]
param([string]$Ziel = 'aarch64-apple-darwin')

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'Laufzeit.ps1')
$uhr = UhrStart
$repo = Split-Path -Parent $PSScriptRoot

function Fertig([string]$satz) { Write-Host $satz; UhrZeile $uhr; exit 0 }

$ziele = @(& rustup target list --installed 2>$null)
if ($ziele -notcontains $Ziel) { Fertig "Mac-Probe übersprungen: Das Ziel $Ziel ist nicht installiert (rustup target add $Ziel)." }
$quelle = Join-Path $env:USERPROFILE '.cargo\bin\llvm-dlltool.exe'
if (-not (Test-Path -LiteralPath $quelle)) { Fertig "Mac-Probe übersprungen: llvm-dlltool.exe fehlt in $env:USERPROFILE\.cargo\bin (rustup component add llvm-tools)." }

$ordner = Join-Path $env:LOCALAPPDATA 'Dashboard\cache\mac-dlltool'
$dlltool = Join-Path $ordner 'dlltool.exe'
if (-not (Test-Path -LiteralPath $dlltool) -or (Get-Item -LiteralPath $quelle).Length -ne (Get-Item -LiteralPath $dlltool).Length) {
    [void][IO.Directory]::CreateDirectory($ordner)
    Copy-Item -LiteralPath $quelle -Destination $dlltool -Force
}
$env:PATH = "$ordner;$env:PATH"

$zeilen = New-Object Collections.Generic.List[string]
Push-Location -LiteralPath $repo
try {
    $ErrorActionPreference = 'Continue'
    & cargo check --lib --profile test --locked --target $Ziel 2>&1 | ForEach-Object { $zeilen.Add("$_") }
    $code = $LASTEXITCODE
} finally { Pop-Location; $ErrorActionPreference = 'Stop' }

$warnungen = @($zeilen | Where-Object { $_ -match '^warning: ' -and $_ -notmatch 'generated \d+ warning' }).Count
$fehler = @($zeilen | Where-Object { $_ -match '^error(\[E\d+\])?: ' }).Count
if ($code -eq 0) {
    Fertig "Mac-Probe grün: cargo check für $Ziel ohne Fehler, $warnungen Warnungen."
}
$zeilen | Select-Object -Last 40 | ForEach-Object { Write-Host "  $_" }
Fertig "Mac-Probe rot (nur Probe, kein Abbruch): cargo check für $Ziel endete mit $code, $fehler Fehler, $warnungen Warnungen."
