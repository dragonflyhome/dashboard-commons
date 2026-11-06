# Api.ps1 - eine Anfrage an die API des Dashboards mit der Windows-Anmeldung
# des Kontos, das das Skript ausführt (ohne Passwort). Ersetzt
# die Aufrufe mit C:\Windows\System32\curl.exe --negotiate -u : (das curl 8.22
# von Windows kann kein NTLM mehr, siehe Anfrage.ps1).
#
#   Lesen:    pwsh -NoProfile -File %USERPROFILE%/dev/dashboard/scripts/Api.ps1 "api/vorgaenge?fuer=<slug>&offen=1"
#   Schicken: pwsh -NoProfile -File %USERPROFILE%/dev/dashboard/scripts/Api.ps1 api/meldungen -Json '{"projekt":"…"}'
#   Leerer Körper: -Post (etwa …/vorgaenge/<id>/uebernommen)
#
# Der Weg ohne führenden Schrägstrich (api/…): Git Bash schreibt Argumente,
# die mit / beginnen, in Windows-Pfade um. /api/… geht aus PowerShell auch.
#
# Mit -Json oder -Post geht ein POST mit Content-Type application/json raus,
# sonst ein GET. Ausgabe: der Körper der Antwort auf stdout. Exit-Code 0 bei
# 2xx, 1 ohne Verbindung, sonst 2 mit dem HTTP-Code auf stderr.

param(
  [Parameter(Mandatory, Position = 0)] [ValidatePattern('^/?api/')] [string] $Weg,
  [string] $Json = '',
  [switch] $Post,
  [ValidatePattern('^(127\.0\.0\.1|localhost)(:\d{1,5})?$')] [string] $Server = '127.0.0.1:8787',
  [int] $Sekunden = 30
)

[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
. (Join-Path $PSScriptRoot 'Anfrage.ps1')

$methode = if ($Json -or $Post) { 'POST' } else { 'GET' }
$antwort = WindowsAnfrage -Url "http://$Server/$($Weg.TrimStart('/'))" -Methode $methode -Json $Json -Sekunden $Sekunden

if ($antwort.Code -eq '000') {
  [Console]::Error.WriteLine("Api.ps1: keine Verbindung zu $Server (oder keine Antwort in $Sekunden s).")
  exit 1
}
$antwort.Text
if ($antwort.Code -notmatch '^2') {
  [Console]::Error.WriteLine("Api.ps1: Antwort $($antwort.Code).")
  exit 2
}
