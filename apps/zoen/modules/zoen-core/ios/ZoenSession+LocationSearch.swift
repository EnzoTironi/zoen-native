import Foundation

extension ZoenSession {
  struct LocationSearchScope: Equatable, Sendable {
    let generation: UInt64
    let identity: String
    let device: String
  }
  /// Public place search is transient, but its result still belongs to the
  /// account/device/native epoch that explicitly requested it, including ABA.
  func locationSearchScope() throws -> LocationSearchScope {
    guard !closing, !erasing, mode == "account", let account = core?.account(),
          account.unlocked, !account.setupPending else { throw SessionError.closed }
    return LocationSearchScope(generation: generation, identity: account.identityId, device: account.deviceId)
  }
  func requireLocationSearchScope(_ expected: LocationSearchScope) throws {
    guard try locationSearchScope() == expected else { throw SessionError.closed }
  }
}
