// swift-tools-version:5.3
// The iOS side of the app's own plugin `hashi` (src/lib.rs), like HashiPlugin.kt on Android.
// build.rs builds it and links it into the app's library, the way tauri-plugin does it for the
// iOS side of a plugin crate. Tauri's Swift API is the copy build.rs makes in ../.tauri.

import PackageDescription

let package = Package(
  name: "hashi-plugin",
  platforms: [
    .macOS(.v10_13),
    .iOS(.v13),
  ],
  products: [
    .library(name: "hashi-plugin", type: .static, targets: ["hashi-plugin"])
  ],
  dependencies: [
    .package(name: "Tauri", path: "../.tauri/tauri-api")
  ],
  targets: [
    .target(name: "hashi-plugin", dependencies: [.byName(name: "Tauri")], path: "Sources")
  ]
)
