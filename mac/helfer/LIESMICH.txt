# dashboard-helfer

Das Mac-Hilfsprogramm des Dashboards. Es läuft getrennt vom Server und hat drei Aufgaben:

1. Es meldet, wenn der Rechner gesperrt und wieder entsperrt wird.
2. Es schickt alle fünf Sekunden ein Lebenszeichen. Bleibt es aus, sperrt sich das Dashboard.
3. Es hilft dem Server, seine Schlüssel zu öffnen. Der Sicherheitschip des Macs rechnet dabei
   mit einem verblendeten Punkt; der Schlüssel selbst geht nie über die Leitung, und der Helfer
   lernt ihn nie.

Der Helfer kann nur sperren, sein Dasein melden und beim Öffnen helfen. Eine Anmeldung am
Dashboard ersetzt er nie.

## Bauen

```
swift build
```

Ohne fremde Pakete: TLS kommt aus macOS (Network.framework), die Kryptografie aus CryptoKit.
Das Programm liegt danach in `.build/debug/dashboard-helfer`.

## Einrichten

Der Server braucht drei Einstellungen (vorläufig aus der Umgebung):

```
DASHBOARD_HELFER_LEITUNG=127.0.0.1:8788
DASHBOARD_SPERRE_MIT_RECHNER=1
DASHBOARD_SCHLUESSEL_VOM_HELFER=1
```

Beim Start legt er in seinem Schlüsselordner die Datei `helfer-fingerabdruck` ab. Der Helfer
muss in der Liste `helfer-zugelassen` im selben Ordner stehen; seine Zeile gibt er selbst aus:

```
dashboard-helfer --ordner <ordner-des-helfers> --kennung --name Schreibtisch >> <schlüsselordner>/helfer-zugelassen
```

Wer die Zeile wieder streicht, sperrt den Helfer sofort aus.

## Starten

```
dashboard-helfer --server 127.0.0.1:8788 --server-ordner <schlüsselordner> \
    --ordner <ordner-des-helfers> \
    --schluessel speicher/aufgaben/links=links=Aufgaben-Links
```

Je `--schluessel` eine Nutzlast, bei der dieser Helfer hilft: ihr Pfad, ein Name, ein Etikett.
Nicht jeder Helfer muss bei jeder Nutzlast helfen.

## Wie die beiden Seiten einander prüfen

Die Leitung ist TLS 1.3 und entsteht zuerst unbeglaubigt. Dann:

- Der Helfer vergleicht den Fingerabdruck des Zertifikats mit dem aus `helfer-fingerabdruck`.
  Passt er nicht, kommt keine Verbindung zustande, und kein Chip wird gefragt.
- Der Server verlangt eine Signatur über den Bindungswert genau dieser Verbindung (RFC 9266,
  `tls-exporter`) von einem zugelassenen Schlüssel. Eine mitgeschnittene Anmeldung taugt so
  für keine andere Verbindung.
- Danach bekommt der Helfer einen Schein: ein signiertes Token nur für diese Aufgabe, zwölf
  Stunden gültig, nie der Ausweis einer Person.

## Grenzen

- `--ohne-chip` ist nur zum Proben: Die Schlüssel des Helfers liegen dann offen in seinem Ordner.
- `--mit-bestaetigung` fragt einmal beim Start nach Fingerabdruck oder Kennwort des Macs. Die
  Schlüssel im Chip sind daran nicht gebunden.
- Ein anderes Programm desselben Kontos kann die Verweise im Ordner des Helfers lesen und den
  Chip ebenso fragen. Der Chip schützt vor Kopien auf ein anderes Gerät, nicht vor dem eigenen Konto.
- HTTP spricht der Helfer selbst, in kleinstem Umfang: eine Anfrage, eine Antwort mit
  Content-Length, nur zum eigenen Server.
