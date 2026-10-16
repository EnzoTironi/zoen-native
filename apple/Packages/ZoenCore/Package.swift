// swift-tools-version: 6.2
// ZoenCore: o núcleo Rust (XCFramework estático) + bindings Swift gerados pelo UniFFI.
import PackageDescription

let package = Package(
    name: "ZoenCore",
    platforms: [.iOS(.v26), .macOS(.v26)],
    products: [
        .library(name: "ZoenCore", targets: ["ZoenCore"]),
    ],
    targets: [
        .binaryTarget(name: "ZoenFFI", path: "ZoenFFI.xcframework"),
        .target(
            name: "ZoenCore",
            dependencies: ["ZoenFFI"],
            path: "Sources/ZoenCore",
            swiftSettings: [.swiftLanguageMode(.v5)]
        ),
    ]
)
