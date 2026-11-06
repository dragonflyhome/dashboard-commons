# Anfrage.ps1 - HTTP an das Dashboard mit der Windows-Anmeldung des Kontos,
# das das Skript ausführt (ohne Passwort). Zum Einbinden mit Punkt:
#   . (Join-Path $PSScriptRoot 'Anfrage.ps1')
#
# Früher lief das über C:\Windows\System32\curl.exe --negotiate -u :.
# Das curl 8.22, das Windows mit KB5127936 eingespielt hat, kann
# kein NTLM mehr; Negotiate bricht dann für ein lokales Konto mit
# SEC_E_NO_CREDENTIALS ab und der Server antwortet 401. .NET (HttpClient mit
# UseDefaultCredentials) handelt Negotiate samt NTLM selbst aus, in pwsh 7 wie
# in Windows PowerShell 5.1. Wie curl ohne -L folgt es keiner Umleitung (die
# Anmeldung geht nie an eine andere Adresse) und nimmt keinen Proxy.
#
# WindowsAnfrage gibt ein Objekt { Code; Text } zurück: Code ist der HTTP-Code
# als Text ('200', '401', …) oder '000' ohne Verbindung oder nach Ablauf der
# Zeit; Text ist der Körper als UTF-8 (leer bei '000'). Jeder andere Fehler
# (etwa ein fehlendes System.Net.Http) bricht ab, statt als '000' endlos
# erneut versucht zu werden.

if (-not ('System.Net.Http.HttpClient' -as [type])) {
  Add-Type -AssemblyName System.Net.Http
}

function WindowsAnfrage {
  param(
    [Parameter(Mandatory)] [string] $Url,
    [string] $Methode = 'GET',
    [int] $Sekunden = 10,
    # Nur bei POST: JSON-Körper; der Server verlangt Content-Type application/json auch bei leerem Körper.
    [string] $Json = '',
    # Ohne Nachweis fragen (für Tests: so sieht eine Anfrage ohne Anmeldung aus).
    [switch] $OhneAnmeldung
  )
  $handler = New-Object System.Net.Http.HttpClientHandler
  $handler.UseDefaultCredentials = -not $OhneAnmeldung
  $handler.AllowAutoRedirect = $false
  $handler.UseProxy = $false
  $client = New-Object System.Net.Http.HttpClient($handler)
  $client.Timeout = [TimeSpan]::FromSeconds($Sekunden)
  try {
    $anfrage = New-Object System.Net.Http.HttpRequestMessage((New-Object System.Net.Http.HttpMethod($Methode)), $Url)
    if ($Methode -ne 'GET') {
      $anfrage.Content = New-Object System.Net.Http.StringContent($Json, (New-Object System.Text.UTF8Encoding($false)), 'application/json')
    }
    $antwort = $client.SendAsync($anfrage).GetAwaiter().GetResult()
    $bytes = $antwort.Content.ReadAsByteArrayAsync().GetAwaiter().GetResult()
    return [pscustomobject]@{
      Code = ([int]$antwort.StatusCode).ToString()
      Text = (New-Object System.Text.UTF8Encoding($false)).GetString($bytes)
    }
  } catch [System.Net.Http.HttpRequestException], [System.Threading.Tasks.TaskCanceledException] {
    # Keine Verbindung (Server startet neu) oder Zeit abgelaufen.
    return [pscustomobject]@{ Code = '000'; Text = '' }
  } finally {
    $client.Dispose()
  }
}
