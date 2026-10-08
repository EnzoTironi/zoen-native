// swift-tools-version: 6.2
// RodaCore: o núcleo Rust (XCFramework estático) + bindings Swift gerados pelo UniFFI.
import PackageDescription

let package = Package(
    name: "RodaCore",
    platforms: [.iOS(.v26), .macOS(.v26)],
    products: [
        .library(name: "RodaCore", targets: ["RodaCore"]),
    ],
    targets: [
        .binaryTarget(name: "RodaFFI", path: "RodaFFI.xcframework"),
        .target(
            name: "RodaCore",
            dependencies: ["RodaFFI"],
            path: "Sources/RodaCore",
            swiftSettings: [.swiftLanguageMode(.v5)]
        ),
    ]
)
