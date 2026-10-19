// swift-tools-version: 6.2
// ClientCore: o núcleo Rust (XCFramework estático) + bindings Swift gerados pelo UniFFI.
import PackageDescription

let package = Package(
    name: "ClientCore",
    platforms: [.iOS(.v26), .macOS(.v26)],
    products: [
        .library(name: "ClientCore", targets: ["ClientCore"]),
    ],
    targets: [
        .binaryTarget(name: "ClientFFI", path: "ClientFFI.xcframework"),
        .target(
            name: "ClientCore",
            dependencies: ["ClientFFI"],
            path: "Sources/ClientCore",
            swiftSettings: [.swiftLanguageMode(.v5)]
        ),
    ]
)
