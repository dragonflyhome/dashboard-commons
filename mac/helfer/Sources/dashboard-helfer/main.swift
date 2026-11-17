// dashboard-helfer: das Mac-Hilfsprogramm des Dashboards.
//
// Drei Aufgaben, alle über eine TLS-1.3-Leitung zum Server auf diesem Rechner:
//   1. Sperre und Entsperren des Rechners melden.
//   2. Alle paar Sekunden ein Lebenszeichen schicken. Bleibt es aus, sperrt
//      sich das Dashboard.
//   3. Dem Server helfen, seine Schlüssel zu öffnen: Der Sicherheitschip
//      rechnet mit einem verblendeten Punkt. Der Schlüssel selbst geht nie
//      über die Leitung, und dieser Helfer lernt ihn nie.
//
// Der Helfer kann nur sperren, sein Dasein melden und beim Öffnen helfen.
// Eine Anmeldung am Dashboard ersetzt er nie.
import CryptoKit
import Foundation
import LocalAuthentication

let hilfe = """
dashboard-helfer --server <host:port> (--server-ordner <ordner> | --fingerabdruck <hex>)
                 --ordner <ordner> [--schluessel <pfad>=<name>=<etikett> ...]
                 [--kennung [--name <name>]] [--mit-bestaetigung] [--ohne-chip] [--einmal]

  --server            Adresse der Helfer-Leitung des Servers, etwa 127.0.0.1:8788
  --server-ordner     Schlüsselordner des Servers; dort liegt die Datei helfer-fingerabdruck
  --fingerabdruck     stattdessen der Fingerabdruck der Leitung direkt (SHA-256, Hexziffern)
  --ordner            Ordner dieses Helfers für die Verweise auf seine Chip-Schlüssel (wird 0700 angelegt)
  --schluessel        eine Nutzlast, bei der dieser Helfer hilft: ihr Pfad, ein Name, ein Etikett
  --kennung           nur die Zeile für die Liste helfer-zugelassen des Servers ausgeben, dann enden
  --name              der Name in dieser Zeile (sonst: Mac-Helfer)
  --mit-bestaetigung  beim Start einmal nach Fingerabdruck oder Kennwort des Macs fragen
  --ohne-chip         nur zum Proben: Schlüssel in Software, offen im Ordner
  --einmal            ein Lebenszeichen, dabei Schlüssel anbieten und öffnen, dann enden
"""

func ende(_ satz: String, code: Int32 = 2) -> Never {
    FileHandle.standardError.write(Data((satz + "\n").utf8))
    exit(code)
}

func melden(_ satz: String) {
    print(satz)
    fflush(stdout)
}

/// Eine Nutzlast, bei der dieser Helfer hilft.
struct Beschreibung {
    let pfad: String
    let name: String
    let etikett: String
}

// ----- Aufruf lesen -----

var server: String?
var serverOrdner: String?
var fingerabdruck: String?
var ordnerPfad: String?
var beschreibungen: [Beschreibung] = []
var nurKennung = false
var name = "Mac-Helfer"
var mitBestaetigung = false
var ohneChip = false
var einmal = false

var args = CommandLine.arguments.dropFirst().makeIterator()
while let a = args.next() {
    switch a {
    case "--server": server = args.next()
    case "--server-ordner": serverOrdner = args.next()
    case "--fingerabdruck": fingerabdruck = args.next()
    case "--ordner": ordnerPfad = args.next()
    case "--schluessel":
        let teile = (args.next() ?? "").split(separator: "=", maxSplits: 2, omittingEmptySubsequences: false).map(String.init)
        guard teile.count == 3, teile.allSatisfy({ !$0.isEmpty }) else { ende("Nach --schluessel fehlt <pfad>=<name>=<etikett>.\n\n\(hilfe)") }
        beschreibungen.append(Beschreibung(pfad: teile[0], name: teile[1], etikett: teile[2]))
    case "--kennung": nurKennung = true
    case "--name": name = args.next() ?? name
    case "--mit-bestaetigung": mitBestaetigung = true
    case "--ohne-chip": ohneChip = true
    case "--einmal": einmal = true
    case "--hilfe", "-h", "--help": print(hilfe); exit(0)
    default: ende("Das Argument „\(a)“ kennt dashboard-helfer nicht.\n\n\(hilfe)")
    }
}
guard let ordnerPfad else { ende("Es fehlt --ordner.\n\n\(hilfe)") }
let ordner = URL(fileURLWithPath: ordnerPfad)

// ----- Ordner und Schlüssel -----

do {
    try FileManager.default.createDirectory(at: ordner, withIntermediateDirectories: true, attributes: [.posixPermissions: 0o700])
} catch {
    ende("Der Ordner \(ordner.path) lässt sich nicht anlegen: \(error.localizedDescription)", code: 9)
}

if ohneChip { FileHandle.standardError.write(Data("ACHTUNG: --ohne-chip. Die Schlüssel des Helfers liegen offen im Ordner. Nur zum Proben.\n".utf8)) }
let brett = Schluesselbrett(ordner: ordner, ohneChip: ohneChip)
let anmelden: Unterzeichner
var chips: [String: Vereinbarer] = [:]
do {
    anmelden = try brett.unterzeichner()
    for b in beschreibungen { chips[b.pfad] = try brett.vereinbarer(b.name) }
} catch {
    ende("Die Schlüssel des Helfers lassen sich nicht nutzen: \(error)", code: 9)
}

if nurKennung {
    // Diese Zeile gehört in die Datei helfer-zugelassen im Schlüsselordner des Servers.
    print("\(anmelden.oeffentlich.base64url) \(name)")
    exit(0)
}

guard let server, let trenner = server.lastIndex(of: ":"), let port = UInt16(server[server.index(after: trenner)...]) else {
    ende("Es fehlt --server <host:port>.\n\n\(hilfe)")
}
let host = String(server[..<trenner])
if fingerabdruck == nil, let serverOrdner {
    fingerabdruck = (try? String(contentsOfFile: URL(fileURLWithPath: serverOrdner).appendingPathComponent("helfer-fingerabdruck").path, encoding: .utf8))?
        .trimmingCharacters(in: .whitespacesAndNewlines)
}
guard let fingerabdruck, fingerabdruck.count == 64 else {
    ende("Der Fingerabdruck der Leitung fehlt: --fingerabdruck <hex> oder --server-ordner <ordner> mit der Datei helfer-fingerabdruck.", code: 9)
}

if mitBestaetigung {
    // Der Dialog von macOS selbst: Fingerabdruck, sonst das Kennwort des Macs.
    let bestaetigt = (try? await LAContext().evaluatePolicy(.deviceOwnerAuthentication, localizedReason: "Das Dashboard-Hilfsprogramm starten")) ?? false
    guard bestaetigt else { ende("Nicht bestätigt. Der Helfer startet nicht.", code: 9) }
}

// ----- Gespräch mit dem Server -----

struct AnmeldeAntwort: Decodable { let schein: String }
struct Anfrage: Decodable { let pfad: String; let chip: String; let punkt: String }
struct Lebenszeichen: Decodable { let neu: [String]; let offen: [Anfrage] }
struct Ergebnis: Decodable { let pfad: String; let stand: String }

enum DienstFehler: Error, CustomStringConvertible {
    case status(String, Int)

    var description: String {
        switch self {
        case .status(let weg, 403): return "Der Server lässt diesen Helfer nicht zu (\(weg)). Steht seine Kennung in helfer-zugelassen?"
        case .status(let weg, let status): return "Der Server antwortet auf \(weg) mit \(status)."
        }
    }
}

/// Was vor dem Bindungswert steht, wenn der Helfer seine Anmeldung signiert.
let anmeldeMarke = Data("dashboard-helfer/anmeldung/v1".utf8) + Data([0])

/// Hält Verbindung und Schein und stellt beides bei Bedarf neu her. Wird
/// nur von der einen Schleife unten benutzt, nie von zwei Stellen zugleich.
@MainActor
final class Dienst {
    private let host: String
    private let port: UInt16
    private let fingerabdruck: String
    private let anmelden: Unterzeichner
    private var leitung: Leitung?
    private var schein: String?

    init(host: String, port: UInt16, fingerabdruck: String, anmelden: Unterzeichner) {
        self.host = host
        self.port = port
        self.fingerabdruck = fingerabdruck
        self.anmelden = anmelden
    }

    private func verbunden() async throws -> Leitung {
        if let leitung { return leitung }
        let neu = Leitung(host: host, port: port, fingerabdruck: fingerabdruck)
        try await neu.verbinden()
        leitung = neu
        schein = nil
        return neu
    }

    private func trennen() {
        leitung?.schliessen()
        leitung = nil
        schein = nil
    }

    private func angemeldet(_ l: Leitung) async throws -> String {
        if let schein { return schein }
        let signatur = try anmelden.signieren(anmeldeMarke + l.bindung)
        let koerper = try JSONSerialization.data(withJSONObject: ["schluessel": anmelden.oeffentlich.base64url, "signatur": signatur.base64url])
        let (status, antwort) = try await l.anfrage("POST", "/anmeldung", schein: nil, koerper: koerper)
        guard status == 200 else { throw DienstFehler.status("/anmeldung", status) }
        let neu = try JSONDecoder().decode(AnmeldeAntwort.self, from: antwort).schein
        schein = neu
        return neu
    }

    /// Eine Anfrage mit Schein. Bricht die Verbindung ab oder gilt der
    /// Schein nicht mehr (etwa nach einem Neustart des Servers), wird einmal
    /// neu verbunden und neu angemeldet.
    func rufen(_ methode: String, _ pfad: String, _ koerper: [String: String]? = nil) async throws -> Data {
        let daten = try koerper.map { try JSONSerialization.data(withJSONObject: $0) }
        for versuch in 0..<2 {
            do {
                let l = try await verbunden()
                let (status, antwort) = try await l.anfrage(methode, pfad, schein: try await angemeldet(l), koerper: daten)
                if status == 401, versuch == 0 {
                    schein = nil
                    continue
                }
                guard (200..<300).contains(status) else { throw DienstFehler.status(pfad, status) }
                return antwort
            } catch let fehler as LeitungFehler {
                trennen()
                if versuch == 1 { throw fehler }
            }
        }
        throw DienstFehler.status(pfad, 401)
    }
}

let dienst = Dienst(host: host, port: port, fingerabdruck: fingerabdruck, anmelden: anmelden)

/// Ein Takt: Lebenszeichen; dabei für neue Pfade einen Chip-Schlüssel
/// anbieten und offene Anfragen mit dem Chip beantworten.
@MainActor @discardableResult
func takt() async -> Bool {
    do {
        let zeichen = try JSONDecoder().decode(Lebenszeichen.self, from: try await dienst.rufen("POST", "/lebenszeichen"))
        for b in beschreibungen where zeichen.neu.contains(b.pfad) {
            guard let chip = chips[b.pfad] else { continue }
            let antwort = try await dienst.rufen("POST", "/schluessel/angebot", ["pfad": b.pfad, "name": b.name, "etikett": b.etikett, "chip": chip.oeffentlich.base64url])
            melden("Angebot für \(b.pfad): \(try JSONDecoder().decode(Ergebnis.self, from: antwort).stand)")
        }
        for a in zeichen.offen {
            // Nur Nutzlasten, bei denen dieser Helfer hilft, und nur, wenn sie an seinen Chip-Schlüssel verschlossen sind.
            guard let chip = chips[a.pfad], chip.oeffentlich.base64url == a.chip, let punkt = Data(base64url: a.punkt) else { continue }
            let x = try chip.vereinbaren(mit: punkt)
            let antwort = try await dienst.rufen("POST", "/schluessel/antwort", ["pfad": a.pfad, "antwort": x.base64url])
            melden("Öffnen für \(a.pfad): \(try JSONDecoder().decode(Ergebnis.self, from: antwort).stand)")
        }
        return true
    } catch {
        melden("Lebenszeichen gescheitert: \(error)")
        return false
    }
}

if einmal {
    let gut = await takt()
    exit(gut ? 0 : 9)
}

// ----- Sperre und Entsperren hören, Lebenszeichen alle fünf Sekunden -----
// Alles läuft durch eine Schlange von Ereignissen: So spricht immer nur eine
// Stelle über die Leitung.

enum Ereignis { case takt, sperre, entsperrt }
let (ereignisse, eingang) = AsyncStream<Ereignis>.makeStream()

let zentrale = DistributedNotificationCenter.default()
zentrale.addObserver(forName: Notification.Name("com.apple.screenIsLocked"), object: nil, queue: .main) { _ in eingang.yield(.sperre) }
zentrale.addObserver(forName: Notification.Name("com.apple.screenIsUnlocked"), object: nil, queue: .main) { _ in eingang.yield(.entsperrt) }
Task {
    while true {
        eingang.yield(.takt)
        try? await Task.sleep(for: .seconds(5))
    }
}

melden("dashboard-helfer läuft: Server \(host):\(port), \(beschreibungen.count) Nutzlasten.")
for await ereignis in ereignisse {
    switch ereignis {
    case .takt: await takt()
    case .sperre:
        melden("Der Rechner ist gesperrt.")
        do { _ = try await dienst.rufen("POST", "/sperre") } catch { melden("Sperre nicht gemeldet: \(error)") }
    case .entsperrt:
        melden("Der Rechner ist entsperrt.")
        do { _ = try await dienst.rufen("POST", "/entsperrt") } catch { melden("Entsperren nicht gemeldet: \(error)") }
    }
}
