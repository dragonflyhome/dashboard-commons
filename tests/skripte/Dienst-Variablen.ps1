<#
.SYNOPSIS
    Prüft die Dienst-Skripte statisch: Sie parsen fehlerfrei, und keine
    Zuweisung trifft eine Variable, die so heißt wie ein Parameter des Skripts.
.DESCRIPTION
    PowerShell unterscheidet bei Variablen nicht zwischen Groß- und
    Kleinschreibung. `$start = 'beim Hochfahren'` in einem Skript mit
    `[switch]$Start` will den Text in den Switch-Parameter wandeln und bricht
    ab. Dieses Skript liest jedes Skript als AST
    (System.Management.Automation.Language.Parser), sammelt die Parameter des
    Skripts und meldet jede Zuweisung (auch `+=`, `[typ]$x = …`, `$script:x = …`)
    und jede foreach-Variable mit einem dieser Namen, auch in Funktionen.

    Ohne Pester, ohne Administrator, ändert nichts. Aufruf aus dem Repo, in
    beiden Shells:

        pwsh -NoProfile -File tests\skripte\Dienst-Variablen.ps1
        powershell.exe -NoProfile -ExecutionPolicy Bypass -File tests\skripte\Dienst-Variablen.ps1

    Ohne -Datei prüft es scripts\Dienst.ps1, Dienst-Einrichten.ps1 und
    Dienst-Entfernen.ps1.
.PARAMETER Datei
    Andere Skripte prüfen (Pfade).
.NOTES
    Exit-Codes: 0 alles gut, 1 Parse-Fehler oder Kollision gefunden.
#>
[CmdletBinding()]
param(
    [string[]]$Datei
)

$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
if (-not $Datei) {
    $Datei = @('Dienst.ps1', 'Dienst-Einrichten.ps1', 'Dienst-Entfernen.ps1') | ForEach-Object { Join-Path (Join-Path $repo 'scripts') $_ }
}

# Name ohne Bereich ($script:x -> x); $env:… und andere Laufwerke zählen nicht.
function Name-Von([System.Management.Automation.Language.VariableExpressionAst]$v) {
    $p = $v.VariablePath
    if ($p.IsUnqualified) { return $p.UserPath }
    if ($p.IsScript -or $p.IsLocal -or $p.IsPrivate -or $p.IsGlobal) { return ($p.UserPath -split ':', 2)[1] }
    return $null
}

# Die Variable links einer Zuweisung, auch hinter Typumwandlungen ([int]$x = …).
function Ziel-Von($links) {
    while ($links -is [System.Management.Automation.Language.ConvertExpressionAst] -or
           $links -is [System.Management.Automation.Language.AttributedExpressionAst]) {
        $links = $links.Child
    }
    if ($links -is [System.Management.Automation.Language.VariableExpressionAst]) { return $links }
    return $null
}

$fehler = 0
foreach ($d in $Datei) {
    $token = $null; $parseFehler = $null
    $ast = [System.Management.Automation.Language.Parser]::ParseFile($d, [ref]$token, [ref]$parseFehler)
    $kurz = Split-Path -Leaf $d
    if ($parseFehler.Count -gt 0) {
        foreach ($f in $parseFehler) { Write-Host ("FEHLER {0}:{1}: {2}" -f $kurz, $f.Extent.StartLineNumber, $f.Message) }
        $fehler++
        continue
    }
    $namen = @()
    if ($ast.ParamBlock) { $namen = @($ast.ParamBlock.Parameters | ForEach-Object { $_.Name.VariablePath.UserPath }) }
    $treffer = @()
    $zuweisungen = $ast.FindAll({ param($n) $n -is [System.Management.Automation.Language.AssignmentStatementAst] }, $true)
    foreach ($z in $zuweisungen) {
        $v = Ziel-Von $z.Left
        if (-not $v) { continue }
        $n = Name-Von $v
        if ($n -and ($namen -contains $n)) { $treffer += , @($z.Extent.StartLineNumber, $n, $z.Extent.Text) }
    }
    $schleifen = $ast.FindAll({ param($n) $n -is [System.Management.Automation.Language.ForEachStatementAst] }, $true)
    foreach ($s in $schleifen) {
        $n = Name-Von $s.Variable
        if ($n -and ($namen -contains $n)) { $treffer += , @($s.Extent.StartLineNumber, $n, "foreach (`$$n in …)") }
    }
    if ($treffer.Count -gt 0) {
        foreach ($t in $treffer) {
            $text = ($t[2] -split "`r?`n")[0]
            if ($text.Length -gt 100) { $text = $text.Substring(0, 100) + ' …' }
            Write-Host ("FEHLER {0}:{1}: Zuweisung an `${2}, so heißt ein Parameter des Skripts: {3}" -f $kurz, $t[0], $t[1], $text)
        }
        $fehler++
    } else {
        Write-Host ("ok     {0}: parst, {1} Parameter ({2}), {3} Zuweisungen ohne Kollision" -f $kurz, $namen.Count, ($namen -join ', '), $zuweisungen.Count)
    }
}
Write-Host ("PowerShell {0}" -f $PSVersionTable.PSVersion)
if ($fehler) { exit 1 }
exit 0
