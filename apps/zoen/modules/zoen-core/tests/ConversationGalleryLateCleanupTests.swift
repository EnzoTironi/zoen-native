import Foundation
import Darwin

@MainActor private final class LateCleanupProbe: ZoenFilePreviewPresentation {
  var ended: (@MainActor (Error?) -> Void)?
  var dismissal: (@MainActor () -> Void)?
  func show(file: URL, title: String, ended: @escaping @MainActor (Error?) -> Void) throws { self.ended = ended }
  func dismiss(completion: @escaping @MainActor () -> Void) { dismissal = completion }
}
private struct LateCleanupFailure: Error, CustomStringConvertible { let description: String }

@MainActor private final class LateCleanupFixture {
  let root = URL.temporaryDirectory.appendingPathComponent("zoen-gallery-late-cleanup-\(UUID().uuidString)")
  private var directories: [URL] = []
  init() throws { try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true) }
  func file() throws -> ZoenPreviewFile {
    let directory = root.appendingPathComponent(UUID().uuidString)
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    directories.append(directory)
    let url = directory.appendingPathComponent("synthetic.bin")
    try Data("Synthetic owned page bytes".utf8).write(to: url)
    return .init(url: url, directory: directory)
  }
  func permission(_ file: ZoenPreviewFile, _ value: Int) throws {
    try FileManager.default.setAttributes([.posixPermissions: value], ofItemAtPath: file.directory.path)
  }
  func exists(_ file: ZoenPreviewFile) -> Bool { FileManager.default.fileExists(atPath: file.url.path) }
  func cleanup() {
    for directory in directories { try? FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: directory.path) }
    try? FileManager.default.removeItem(at: root)
  }
}

/// Real OS permission failures against the production Scope/FilePreview owner;
/// the presentation probe controls only the actual-dismissal completion barrier.
@main @MainActor struct ConversationGalleryLateCleanupTests {
  private static func expect(_ value: Bool, _ message: String) throws {
    if !value { throw LateCleanupFailure(description: message) }
  }
  private static func denial(_ error: Error) -> Bool {
    guard let error = error as? CocoaError else { return false }
    return error.code == .fileReadNoPermission || error.code == .fileWriteNoPermission
  }
  private static func until(_ condition: () -> Bool) async throws {
    for _ in 0..<10000 { if condition() { return }; await Task.yield() }
    throw LateCleanupFailure(description: "Production owner did not settle")
  }
  private static func retired(_ owner: ZoenFilePreview, fixture: LateCleanupFixture) async throws -> ZoenFilePreview.ReadOnlyRegistration {
    let file = try fixture.file(), copies = ZoenFilePreview.ReadOnlyCopies(), probe = LateCleanupProbe()
    let ticket = try owner.registerReadOnly(file: file, generation: 1, owner: copies.registration)
    let work = Task { try await owner.open(id: ticket, title: "QA", edit: nil, readOnlyCopies: copies) { _ in probe } }
    try await until { probe.ended != nil }
    probe.ended?(nil); try await work.value
    try expect(!fixture.exists(file), "Initial page survived a successful close")
    return copies.registration
  }
  private static func failedLateCopy(_ owner: ZoenFilePreview, registration: ZoenFilePreview.ReadOnlyRegistration,
                                    fixture: LateCleanupFixture) throws -> ZoenPreviewFile {
    let late = try fixture.file(); try fixture.permission(late, 0o000)
    do {
      _ = try owner.registerReadOnly(file: late, generation: 1, owner: registration)
      throw LateCleanupFailure(description: "A retired producer was admitted")
    } catch {
      guard denial(error) else { throw error }
    }
    // stat of the directory remains available even while lookup inside it is
    // denied. fileExists(payload) is not evidence of deletion under mode000.
    _ = try FileManager.default.attributesOfItem(atPath: late.directory.path)
    return late
  }
  private static func nextOpenAndRetry() async throws {
    let fixture = try LateCleanupFixture(); defer { fixture.cleanup() }
    let owner = ZoenFilePreview(), registration = try await retired(owner, fixture: fixture)
    let late = try failedLateCopy(owner, registration: registration, fixture: fixture)
    let fresh = try fixture.file(), probe = LateCleanupProbe(), ticket = owner.register(file: fresh, generation: 1)
    var result: Result<Void, Error>?
    let opening = Task {
      do { try await owner.open(id: ticket, title: "QA next") { probe }; result = .success(()) }
      catch { result = .failure(error) }
    }
    try await until { result != nil || probe.ended != nil }
    if probe.ended != nil {
      probe.ended?(nil); await opening.value
      throw LateCleanupFailure(description: "Next open bypassed failed late-copy cleanup")
    }
    await opening.value
    guard case .failure(let failure)? = result, case ZoenFilePreview.Failure.busy = failure else {
      throw LateCleanupFailure(description: "Failed late copy did not reach existing retained/busy guard")
    }
    try expect(!fixture.exists(fresh), "Rejected next viewer leaked its own input copy")
    do { try await owner.clear(before: 1); throw LateCleanupFailure(description: "Clear ignored denied late cleanup") }
    catch { guard denial(error) else { throw error } }
    try fixture.permission(late, 0o700)
    try expect(fixture.exists(late), "Denied copy was falsely reported deleted")
    try await owner.clear(before: 1)
    try expect(!fixture.exists(late), "Retry failed to remove late copy")
    let recovered = try fixture.file(), recoveredProbe = LateCleanupProbe()
    let recoveredTicket = owner.register(file: recovered, generation: 1)
    let retry = Task { try await owner.open(id: recoveredTicket, title: "QA recovered") { recoveredProbe } }
    try await until { recoveredProbe.ended != nil }; recoveredProbe.ended?(nil); try await retry.value
    try expect(!fixture.exists(recovered), "Recovered preview did not release its copy")
  }
  private static func clearWithoutNextOpen() async throws {
    let fixture = try LateCleanupFixture(); defer { fixture.cleanup() }
    let owner = ZoenFilePreview(), registration = try await retired(owner, fixture: fixture)
    let late = try failedLateCopy(owner, registration: registration, fixture: fixture)
    do { try await owner.clear(before: 1); throw LateCleanupFailure(description: "Same-generation clear ignored retired-owner cleanup") }
    catch { guard denial(error) else { throw error } }
    try fixture.permission(late, 0o700); try await owner.clear(before: 1)
    try expect(!fixture.exists(late), "Explicit cleanup retry left owned bytes")
  }
  private static func failureArrivingDuringDismissal() async throws {
    let fixture = try LateCleanupFixture(); defer { fixture.cleanup() }
    let owner = ZoenFilePreview(), registration = try await retired(owner, fixture: fixture)
    let current = try fixture.file(), probe = LateCleanupProbe()
    let ticket = owner.register(file: current, generation: 1)
    let opening = Task { try await owner.open(id: ticket, title: "QA current") { probe } }
    try await until { probe.ended != nil }
    let clear = Task { () -> Result<Void, Error> in
      do { try await owner.clear(before: 2); return .success(()) } catch { return .failure(error) }
    }
    try await until { probe.dismissal != nil }
    let late = try failedLateCopy(owner, registration: registration, fixture: fixture)
    probe.dismissal?()
    do { try await opening.value; throw LateCleanupFailure(description: "Revoked current viewer succeeded") }
    catch { guard case ZoenFilePreview.Failure.changed = error else { throw error } }
    guard case .failure(let error) = await clear.value, denial(error) else {
      throw LateCleanupFailure(description: "Clear ACK skipped late failure arriving while hierarchy dismissed")
    }
    try fixture.permission(late, 0o700); try await owner.clear(before: 2)
    try expect(!fixture.exists(late) && !fixture.exists(current), "Clear retry retained late/current bytes")
  }
  static func main() async {
    var failures = 0
    for (name, operation) in [
      ("next-open retained guard and retry", nextOpenAndRetry),
      ("explicit clear without next open", clearWithoutNextOpen),
      ("late failure during native dismissal", failureArrivingDuringDismissal),
    ] {
      do { try await operation(); print("PASS: \(name)") }
      catch { failures += 1; print("FAIL: \(name): \(error)") }
    }
    print("Late-copy cleanup: \(3 - failures) passed, \(failures) failed")
    if failures > 0 { exit(1) }
  }
}
