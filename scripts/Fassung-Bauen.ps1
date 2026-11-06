# Der Bau der Fassung für den Dienst (Dienst.ps1 -Neustart), in einer eigenen
# Datei, damit genau dieselbe Folge sich ohne Dienst an einer Wegwerf-Kopie
# prüfen lässt (Nachbesserung zum Umzug des Inhalts, Punkt 4). Eingebunden mit
# `. $PSScriptRoot\Fassung-Bauen.ps1`. Ändert nichts am Dienst; es baut nur und
# wählt die Programme.
#
# Folge:
#   1. Code-Repo bauen: cargo build --release --locked (Bauordner $bauordner als
#      CARGO_TARGET_DIR nur für diese Aufrufe; Dienst.ps1 gibt
#      %LOCALAPPDATA%\Dashboard\cache\dienst).
#   2. Gibt es die Zusammenbau-Kiste dashboard-lokal: zuerst der Wächter aus
#      dem Code-Repo (dashboard.exe pruefe-zusammenbau, liest nur, baut nichts).
#      Ist er rot oder fehlt dashboard.exe: Schluss, in dashboard-lokal lief
#      kein cargo, der alte Dienst läuft weiter.
#   3. Nur bei grünem Wächter: cargo --config <code-repo>\.cargo\config.toml
#      build --release --locked in dashboard-lokal (dlltool aus dem Code-Repo;
#      im Inhalt gibt es keine .cargo-Einstellung), dann dashboard-lokal.exe
#      als dashboard.exe nach <bauordner>\release\zusammenbau.
# Rückgabe: Code (0 gut), Satz, Quelle (Ordner mit dashboard.exe und
# dashboard-bote.exe) und Zusammenbau (ob die Fassung mit Rechenteilen ist).

function Fassung-Bauen([string]$repo, [string]$lokal, [string]$bauordner) {
    if (-not $bauordner) { $bauordner = Join-Path $repo 'target' }
    $altZiel = $env:CARGO_TARGET_DIR
    $altFehler = $ErrorActionPreference
    $env:CARGO_TARGET_DIR = $bauordner
    try {
        $ErrorActionPreference = 'Continue'
        Write-Host "Baue das Programm in $repo (cargo build --release --locked) ..."
        Push-Location $repo
        try { cargo build --release --locked 2>&1 | ForEach-Object { Write-Host "  $_" }; $code = $LASTEXITCODE } finally { Pop-Location }
        if ($code -ne 0) { return [pscustomobject]@{ Code = 1; Satz = 'Bauen mit cargo ist gescheitert. Der Dienst läuft unverändert weiter.'; Quelle = $null; Zusammenbau = $false } }
        $release = Join-Path $bauordner 'release'
        if (-not $lokal) {
            return [pscustomobject]@{ Code = 0; Satz = 'Hinweis: Es gibt keine Zusammenbau-Kiste dashboard-lokal neben dem Inhalt. Es läuft das allgemeine dashboard.exe: Die Projektseiten gehen, die Rechenteile der Projekte fehlen.'; Quelle = $release; Zusammenbau = $false }
        }
        Write-Host "Prüfe die Zusammenbau-Kiste in $lokal (Wächter aus dem Repo, vor jedem cargo-Aufruf dort) ..."
        $waechter = Join-Path $release 'dashboard.exe'
        # Ohne Wächter kein Bau: Fehlt dashboard.exe, liefe der Aufruf ins Leere
        # und $LASTEXITCODE stammte noch vom cargo-Aufruf davor (0).
        if (-not (Test-Path -LiteralPath $waechter -PathType Leaf)) {
            $global:LASTEXITCODE = 1
            return [pscustomobject]@{ Code = 1; Satz = "Der Wächter fehlt: $waechter gibt es nicht (hat cargo build es nicht geschrieben?). Ohne Wächter wird in $lokal nichts gebaut. Der Dienst läuft unverändert weiter."; Quelle = $null; Zusammenbau = $false }
        }
        & $waechter pruefe-zusammenbau $lokal --code $repo 2>&1 | ForEach-Object { Write-Host "  $_" }
        if ($LASTEXITCODE -ne 0) {
            return [pscustomobject]@{ Code = 1; Satz = "Der Wächter der Zusammenbau-Kiste ist rot (siehe oben; Liste geprüfter Kisten in $lokal\kisten.toml). In $lokal wurde nichts gebaut. Der Dienst läuft unverändert weiter."; Quelle = $null; Zusammenbau = $false }
        }
        Write-Host 'Baue die Zusammenbau-Kiste (cargo build --release --locked, dlltool aus dem Code-Repo) ...'
        Push-Location $lokal
        try { cargo --config (Join-Path $repo '.cargo\config.toml') build --release --locked 2>&1 | ForEach-Object { Write-Host "  $_" }; $code = $LASTEXITCODE } finally { Pop-Location }
        if ($code -ne 0) { return [pscustomobject]@{ Code = 1; Satz = 'Der Bau von dashboard-lokal ist gescheitert (der Wächter war grün). Der Dienst läuft unverändert weiter.'; Quelle = $null; Zusammenbau = $false } }
        $zusammen = Join-Path $release 'zusammenbau'
        New-Item -ItemType Directory -Force -Path $zusammen | Out-Null
        Copy-Item -LiteralPath (Join-Path $release 'dashboard-lokal.exe') -Destination (Join-Path $zusammen 'dashboard.exe') -Force
        Copy-Item -LiteralPath (Join-Path $release 'dashboard-bote.exe') -Destination (Join-Path $zusammen 'dashboard-bote.exe') -Force
        return [pscustomobject]@{ Code = 0; Satz = "Fassung mit Rechenteilen (Zusammenbau dashboard-lokal) gebaut: $zusammen"; Quelle = $zusammen; Zusammenbau = $true }
    } finally {
        $env:CARGO_TARGET_DIR = $altZiel
        $ErrorActionPreference = $altFehler
    }
}
