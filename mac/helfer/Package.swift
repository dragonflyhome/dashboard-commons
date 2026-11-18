// swift-tools-version:5.9
// Das Mac-Hilfsprogramm des Dashboards. Ohne fremde Pakete: TLS kommt aus
// macOS (Network.framework), die Kryptografie aus CryptoKit.
import PackageDescription

let package = Package(
    name: "dashboard-helfer",
    platforms: [.macOS(.v14)],
    targets: [
        .executableTarget(name: "dashboard-helfer"),
    ]
)
