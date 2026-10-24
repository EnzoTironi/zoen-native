import Foundation

/// Transient callback fence only; it neither owns a draft nor grants file access.
struct ZoenLocationLifetime {
  struct Ticket: Equatable, Sendable { let binding: String; let generation: UInt64 }
  private var generation: UInt64 = 0
  private var ticket: Ticket?
  static func validBinding(_ value: String) -> Bool {
    !value.isEmpty && value.utf8.count <= 128 && !ZoenStaticLocation.hasControls(value)
  }
  mutating func activate(_ binding: String) throws -> Ticket {
    retire()
    guard Self.validBinding(binding), generation < UInt64.max else { throw ZoenStaticLocation.Failure.invalid }
    let next = Ticket(binding: binding, generation: generation)
    ticket = next
    return next
  }
  mutating func retire() {
    ticket = nil
    if generation < UInt64.max { generation += 1 }
  }
  func current(_ expected: Ticket) -> Bool { ticket == expected }
  func capture() -> Ticket? { ticket }
}
