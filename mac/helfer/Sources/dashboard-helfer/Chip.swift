// Die Schlüssel dieses Helfers. Beide Arten liegen im Sicherheitschip des
// Macs (Secure Enclave) und verlassen ihn nie; im Ordner des Helfers liegt
// je Schlüssel nur ein Verweis, der nur auf diesem Gerät gilt.
//
//   - Der Schlüssel zum Anmelden signiert den Bindungswert der Verbindung.
//   - Je benanntem Schlüssel für Nutzlasten gibt es einen zum Vereinbaren:
//     Der Server verschließt den Schlüssel einer Nutzlast an dessen
//     öffentlichen Teil und legt später einen verblendeten Punkt vor. Der
//     Chip rechnet damit; heraus kommt nur eine x-Koordinate, aus der allein
//     der Server den Schlüssel gewinnt. Dieser Helfer lernt ihn nie.
import CryptoKit
import Foundation

protocol Unterzeichner {
    /// Der öffentliche Teil: P-256, 65 Bytes.
    var oeffentlich: Data { get }
    /// ECDSA mit SHA-256 über `nachricht`: r und s zu je 32 Bytes.
    func signieren(_ nachricht: Data) throws -> Data
}

protocol Vereinbarer {
    /// Der öffentliche Teil: P-256, 65 Bytes.
    var oeffentlich: Data { get }
    /// Die Schlüsselvereinbarung mit `punkt` (65 Bytes): die x-Koordinate, 32 Bytes.
    func vereinbaren(mit punkt: Data) throws -> Data
}

enum ChipFehler: Error, CustomStringConvertible {
    case keinChip

    var description: String { "Dieser Mac hat keinen erreichbaren Sicherheitschip." }
}

extension SecureEnclave.P256.Signing.PrivateKey: Unterzeichner {
    var oeffentlich: Data { publicKey.x963Representation }
    func signieren(_ nachricht: Data) throws -> Data { try signature(for: nachricht).rawRepresentation }
}

extension P256.Signing.PrivateKey: Unterzeichner {
    var oeffentlich: Data { publicKey.x963Representation }
    func signieren(_ nachricht: Data) throws -> Data { try signature(for: nachricht).rawRepresentation }
}

extension SecureEnclave.P256.KeyAgreement.PrivateKey: Vereinbarer {
    var oeffentlich: Data { publicKey.x963Representation }
    func vereinbaren(mit punkt: Data) throws -> Data {
        try sharedSecretFromKeyAgreement(with: P256.KeyAgreement.PublicKey(x963Representation: punkt)).withUnsafeBytes { Data($0) }
    }
}

extension P256.KeyAgreement.PrivateKey: Vereinbarer {
    var oeffentlich: Data { publicKey.x963Representation }
    func vereinbaren(mit punkt: Data) throws -> Data {
        try sharedSecretFromKeyAgreement(with: P256.KeyAgreement.PublicKey(x963Representation: punkt)).withUnsafeBytes { Data($0) }
    }
}

/// Lädt und legt die Schlüssel dieses Helfers an.
struct Schluesselbrett {
    let ordner: URL
    /// Nur zum Proben: Schlüssel in Software, offen in Dateien. Schützt nichts.
    let ohneChip: Bool

    private func zugriff() throws -> SecAccessControl {
        var fehler: Unmanaged<CFError>?
        guard let z = SecAccessControlCreateWithFlags(nil, kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly, [.privateKeyUsage], &fehler) else { throw ChipFehler.keinChip }
        return z
    }

    /// Liest die Datei `name`, oder legt sie mit `neu()` an (0600).
    private func datei(_ name: String, neu: () throws -> Data) throws -> Data {
        let ort = ordner.appendingPathComponent(name)
        if let vorhanden = try? Data(contentsOf: ort) { return vorhanden }
        let inhalt = try neu()
        try inhalt.write(to: ort, options: [.atomic])
        try FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: ort.path)
        return inhalt
    }

    func unterzeichner() throws -> Unterzeichner {
        if ohneChip {
            return try P256.Signing.PrivateKey(rawRepresentation: datei("anmelden.offen") { P256.Signing.PrivateKey().rawRepresentation })
        }
        guard SecureEnclave.isAvailable else { throw ChipFehler.keinChip }
        let verweis = try datei("anmelden.chip") { try SecureEnclave.P256.Signing.PrivateKey(accessControl: zugriff()).dataRepresentation }
        return try SecureEnclave.P256.Signing.PrivateKey(dataRepresentation: verweis)
    }

    /// Der Schlüssel zum Vereinbaren mit dem Namen `name`. Der Dateiname
    /// zeigt den Namen nicht.
    func vereinbarer(_ name: String) throws -> Vereinbarer {
        let kurz = Data(SHA256.hash(data: Data(name.utf8))).prefix(8).hex
        if ohneChip {
            return try P256.KeyAgreement.PrivateKey(rawRepresentation: datei("nutzlast-\(kurz).offen") { P256.KeyAgreement.PrivateKey().rawRepresentation })
        }
        guard SecureEnclave.isAvailable else { throw ChipFehler.keinChip }
        let verweis = try datei("nutzlast-\(kurz).chip") { try SecureEnclave.P256.KeyAgreement.PrivateKey(accessControl: zugriff()).dataRepresentation }
        return try SecureEnclave.P256.KeyAgreement.PrivateKey(dataRepresentation: verweis)
    }
}
