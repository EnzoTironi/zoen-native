import Foundation

/// Feature flags for things that are designed but not shipped yet. Each reads a
/// UserDefaults key so a launch argument (`-RodaJamCalls YES`) can flip it in dev.
enum Flags {
    /// Voice and video calls arrive with Jam. Off: the call buttons explain that instead.
    static var jamCalls: Bool { UserDefaults.standard.bool(forKey: "RodaJamCalls") }
}

extension Flags {
    /// The build for a personal iPhone signed with a free Apple ID (scheme `Zoen-Device`).
    /// That signing can't carry push, iCloud, App Groups, associated domains (passkeys) or
    /// keychain sharing, so anything built on those checks this and quietly stays off.
    static var personalDevice: Bool {
        #if ZOEN_PERSONAL_DEVICE
        true
        #else
        false
        #endif
    }
}
