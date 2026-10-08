import Foundation

/// Feature flags for things that are designed but not shipped yet. Each reads a
/// UserDefaults key so a launch argument (`-RodaJamCalls YES`) can flip it in dev.
enum Flags {
    /// Voice and video calls arrive with Jam. Off: the call buttons explain that instead.
    static var jamCalls: Bool { UserDefaults.standard.bool(forKey: "RodaJamCalls") }
}
