// Die Leitung zum Server: TLS 1.3 mit dem in macOS eingebauten TLS, darauf HTTP.
//
// Die Leitung entsteht zuerst unbeglaubigt; dann wird geprüft:
//   - Der Helfer vergleicht den Fingerabdruck des Zertifikats (SHA-256) mit
//     dem, den er kennt. Passt er nicht, kommt keine Verbindung zustande.
//   - Für den Server signiert der Helfer den Bindungswert genau dieser
//     Verbindung (RFC 9266, tls-exporter). Eine mitgeschnittene Anmeldung
//     taugt so für keine andere Verbindung.
//
// HTTP: eine Anfrage, eine Antwort, auf einer Verbindung, die offen bleibt.
// Gesprochen wird nur mit dem eigenen Server; verstanden werden darum nur
// Antworten mit Content-Length.
import CryptoKit
import Foundation
import Network
import Security

enum LeitungFehler: Error, CustomStringConvertible {
    case verbindung(String)
    case bindung
    case antwort(String)

    var description: String {
        switch self {
        case .verbindung(let grund): return "Keine Verbindung zum Server (\(grund))."
        case .bindung: return "Die Verbindung nennt keinen Bindungswert."
        case .antwort(let grund): return "Die Antwort des Servers ist nicht lesbar (\(grund))."
        }
    }
}

extension Data {
    var base64url: String {
        base64EncodedString().replacingOccurrences(of: "+", with: "-").replacingOccurrences(of: "/", with: "_").replacingOccurrences(of: "=", with: "")
    }

    init?(base64url text: String) {
        var s = text.replacingOccurrences(of: "-", with: "+").replacingOccurrences(of: "_", with: "/")
        while s.count % 4 != 0 { s.append("=") }
        self.init(base64Encoded: s)
    }

    var hex: String { map { String(format: "%02x", $0) }.joined() }
}

/// Eine Verbindung. Wird nur von einer Stelle zugleich benutzt (`Dienst`).
final class Leitung: @unchecked Sendable {
    /// Marke und Länge des Bindungswerts, wie RFC 9266 sie festlegt.
    private static let bindungMarke = "EXPORTER-Channel-Binding"
    private static let bindungLaenge = 32
    /// Längste Antwort, die gelesen wird.
    private static let hoechstens = 1 << 20

    private let verbindung: NWConnection
    private let schlange = DispatchQueue(label: "dashboard-helfer.leitung")
    private var puffer = Data()
    /// Der Bindungswert dieser Verbindung; gesetzt, sobald sie steht.
    private(set) var bindung = Data()

    /// `fingerabdruck`: SHA-256 über das Zertifikat des Servers, in Hexziffern.
    init(host: String, port: UInt16, fingerabdruck: String) {
        let tls = NWProtocolTLS.Options()
        let sicherheit = tls.securityProtocolOptions
        sec_protocol_options_set_min_tls_protocol_version(sicherheit, .TLSv13)
        let erwartet = fingerabdruck.lowercased()
        // Ersetzt die übliche Prüfung gegen Zertifizierungsstellen: Es zählt
        // nur der Fingerabdruck. Dass die Gegenseite den Schlüssel zum
        // Zertifikat hat, prüft der Handschlag selbst.
        sec_protocol_options_set_verify_block(sicherheit, { _, vertrauen, fertig in
            let ref = sec_trust_copy_ref(vertrauen).takeRetainedValue()
            guard let kette = SecTrustCopyCertificateChain(ref) as? [SecCertificate], let blatt = kette.first else {
                fertig(false)
                return
            }
            fertig(Data(SHA256.hash(data: SecCertificateCopyData(blatt) as Data)).hex == erwartet)
        }, schlange)
        verbindung = NWConnection(host: NWEndpoint.Host(host), port: NWEndpoint.Port(rawValue: port)!, using: NWParameters(tls: tls))
    }

    /// Baut die Verbindung auf und holt ihren Bindungswert.
    func verbinden() async throws {
        try await withCheckedThrowingContinuation { (weiter: CheckedContinuation<Void, Error>) in
            var gemeldet = false
            let einmal: (Result<Void, Error>) -> Void = { ergebnis in
                guard !gemeldet else { return }
                gemeldet = true
                weiter.resume(with: ergebnis)
            }
            verbindung.stateUpdateHandler = { [verbindung] stand in
                switch stand {
                case .ready: einmal(.success(()))
                case .failed(let fehler), .waiting(let fehler):
                    verbindung.cancel()
                    einmal(.failure(LeitungFehler.verbindung("\(fehler)")))
                case .cancelled: einmal(.failure(LeitungFehler.verbindung("geschlossen")))
                default: break
                }
            }
            verbindung.start(queue: schlange)
        }
        guard let angaben = verbindung.metadata(definition: NWProtocolTLS.definition) as? NWProtocolTLS.Metadata else { throw LeitungFehler.bindung }
        let marke = Self.bindungMarke
        guard let wert = marke.withCString({ sec_protocol_metadata_create_secret(angaben.securityProtocolMetadata, marke.utf8.count, $0, Self.bindungLaenge) }) else {
            throw LeitungFehler.bindung
        }
        bindung = Data(wert as DispatchData)
        guard bindung.count == Self.bindungLaenge else { throw LeitungFehler.bindung }
    }

    func schliessen() {
        verbindung.cancel()
    }

    private func senden(_ daten: Data) async throws {
        try await withCheckedThrowingContinuation { (weiter: CheckedContinuation<Void, Error>) in
            verbindung.send(content: daten, completion: .contentProcessed { fehler in
                if let fehler { weiter.resume(throwing: LeitungFehler.verbindung("\(fehler)")) } else { weiter.resume() }
            })
        }
    }

    private func lesen() async throws -> Data {
        try await withCheckedThrowingContinuation { (weiter: CheckedContinuation<Data, Error>) in
            verbindung.receive(minimumIncompleteLength: 1, maximumLength: 65536) { daten, _, zuEnde, fehler in
                if let daten, !daten.isEmpty {
                    weiter.resume(returning: daten)
                } else if let fehler {
                    weiter.resume(throwing: LeitungFehler.verbindung("\(fehler)"))
                } else {
                    weiter.resume(throwing: LeitungFehler.verbindung(zuEnde ? "vom Server geschlossen" : "nichts gelesen"))
                }
            }
        }
    }

    /// Eine Anfrage und ihre Antwort: Status und Körper.
    func anfrage(_ methode: String, _ pfad: String, schein: String?, koerper: Data?) async throws -> (Int, Data) {
        var kopf = "\(methode) \(pfad) HTTP/1.1\r\nHost: localhost\r\n"
        if let schein { kopf += "Authorization: Bearer \(schein)\r\n" }
        if koerper != nil { kopf += "Content-Type: application/json\r\n" }
        kopf += "Content-Length: \(koerper?.count ?? 0)\r\n\r\n"
        try await senden(Data(kopf.utf8) + (koerper ?? Data()))
        let trenner = Data("\r\n\r\n".utf8)
        while true {
            if let ende = puffer.range(of: trenner) {
                let zeilen = String(decoding: puffer[puffer.startIndex..<ende.lowerBound], as: UTF8.self).components(separatedBy: "\r\n")
                let teile = (zeilen.first ?? "").split(separator: " ")
                guard teile.count >= 2, let status = Int(teile[1]) else { throw LeitungFehler.antwort("Statuszeile") }
                var laenge = 0
                for zeile in zeilen.dropFirst() {
                    let klein = zeile.lowercased()
                    if klein.hasPrefix("transfer-encoding:") { throw LeitungFehler.antwort("ohne Content-Length") }
                    if klein.hasPrefix("content-length:") {
                        guard let n = Int(klein.dropFirst("content-length:".count).trimmingCharacters(in: .whitespaces)), n >= 0, n <= Self.hoechstens else {
                            throw LeitungFehler.antwort("Content-Length")
                        }
                        laenge = n
                    }
                }
                let beginn = ende.upperBound
                if puffer.distance(from: beginn, to: puffer.endIndex) >= laenge {
                    let schluss = puffer.index(beginn, offsetBy: laenge)
                    let inhalt = Data(puffer[beginn..<schluss])
                    puffer = Data(puffer[schluss...])
                    return (status, inhalt)
                }
            }
            guard puffer.count <= Self.hoechstens else { throw LeitungFehler.antwort("zu lang") }
            puffer.append(try await lesen())
        }
    }
}
