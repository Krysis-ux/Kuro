// swift-tools-version:5.10
import PackageDescription

// The native macOS interface for Kuro.
//
//   KuroKit     API types, the HTTP client and pure logic (no UI).
//   Kuro        The SwiftUI app. Starts the bundled server and talks to it.
//   KuroChecks  Assertion-based checks for KuroKit: `swift run KuroChecks`.
let package = Package(
    name: "Kuro",
    platforms: [.macOS(.v14)],
    targets: [
        .target(name: "KuroKit"),
        .executableTarget(name: "Kuro", dependencies: ["KuroKit"]),
        .executableTarget(name: "KuroChecks", dependencies: ["KuroKit"]),
    ]
)
