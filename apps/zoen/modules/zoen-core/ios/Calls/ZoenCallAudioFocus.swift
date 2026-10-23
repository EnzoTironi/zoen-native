import Foundation

/// A bounded handoff to existing Expo media owners, not a second AVAudioSession.
/// The native call remains fenced if its JS media owners cannot acknowledge it.
actor ZoenCallAudioFocus {
  private var ticket: String?
  private var ready = false
  private var waiter: CheckedContinuation<Void, Error>?
  private var expiry: Task<Void, Never>?
  private var emit: (@Sendable (String) -> Void)?
  private var nativeOnly = false

  /// Only a process-gated cold session can use this: it has no JavaScript media
  /// owners. Foreground adoption must fence new JS owners before exposing UI.
  func acquireNative() throws {
    if ready { return }
    guard ticket == nil else { throw ZoenCallFailure.unavailable }
    ticket = UUID().uuidString; nativeOnly = true; ready = true
  }

  func adopt(emit: @escaping @Sendable (String) -> Void) async throws {
    guard nativeOnly, let ticket else { return }
    guard waiter == nil else { throw ZoenCallFailure.unavailable }
    nativeOnly = false; self.emit = emit
    try await wait(ticket: ticket, action: "adopt", emit: emit)
  }

  func acquire(emit: @escaping @Sendable (String) -> Void) async throws {
    if ready { return }
    guard ticket == nil else { throw ZoenCallFailure.unavailable }
    let ticket = UUID().uuidString
    self.ticket = ticket; self.emit = emit
    try await wait(ticket: ticket, action: "acquire", emit: emit)
  }
  private func wait(ticket: String, action: String, emit: @escaping @Sendable (String) -> Void) async throws {
    try await withCheckedThrowingContinuation { continuation in
      waiter = continuation
      expiry = Task { [weak self] in
        do { try await Task.sleep(for: .seconds(5)) } catch { return }
        await self?.answer(ticket: ticket, success: false)
      }
      emit(Self.event(ticket: ticket, action: action))
    }
  }
  func answer(ticket: String, success: Bool) {
    guard self.ticket == ticket, let waiter else { return }
    self.waiter = nil; expiry?.cancel(); expiry = nil
    if success { ready = true; waiter.resume() }
    else { waiter.resume(throwing: ZoenCallFailure.unavailable); release() }
  }
  func release() {
    expiry?.cancel(); expiry = nil
    if let waiter { self.waiter = nil; waiter.resume(throwing: ZoenCallFailure.retired) }
    if let ticket { emit?(Self.event(ticket: ticket, action: "release")) }
    ticket = nil; ready = false; nativeOnly = false; emit = nil
  }
  private static func event(ticket: String, action: String) -> String {
    // Only locally generated UUID/action strings enter this content-free event.
    "{\"ticket\":\"\(ticket)\",\"action\":\"\(action)\"}"
  }
}
