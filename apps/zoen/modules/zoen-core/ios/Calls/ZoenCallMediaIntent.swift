/// User intent belongs to one transient call, even before RTC or OS permissions.
/// Permission/capture completion can narrow this intent, never turn it back on.
struct ZoenCallMediaIntent: Sendable {
  var muted = false
  var speaker = false
  var cameraEnabled = true
  func microphoneLive(allowed: Bool, activated: Bool, live: Bool) -> Bool {
    live && allowed && activated && !muted
  }
  func cameraLive(allowed: Bool, started: Bool, live: Bool) -> Bool {
    live && allowed && started && cameraEnabled
  }
}
