// swift-tools-version: 6.2
// The swift-tools-version declares the minimum version of Swift required to build this package.

import PackageDescription

let package = Package(
    name: "SharedApp",
    platforms: [.iOS(.v13)],
    products: [
        .library(name: "shared_app", targets: ["shared_app"])
    ],
    targets: [
        .binaryTarget(
            name: "shared_appFFI",
            path: "./shared_appFFI.xcframework",
        ),
        .target(
            name: "shared_app",
            dependencies: ["shared_appFFI"],
            path: "Sources/shared_app"
        ),
    ]
)
