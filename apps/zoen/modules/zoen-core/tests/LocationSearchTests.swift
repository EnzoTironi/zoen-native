import Foundation

@MainActor final class Work: ZoenLocationSearchOperation {
  private var pending: CheckedContinuation<[ZoenStaticLocation.Point], Error>?
  private var waiting: CheckedContinuation<Void, Never>?
  var cancelled = 0
  func start() async throws -> [ZoenStaticLocation.Point] {
    try await withCheckedThrowingContinuation { continuation in
      pending = continuation
      waiting?.resume(); waiting = nil
    }
  }
  func started() async {
    if pending == nil { await withCheckedContinuation { waiting = $0 } }
  }
  func finish(_ points: [ZoenStaticLocation.Point]) { pending?.resume(returning: points); pending = nil }
  func fail() { pending?.resume(throwing: ZoenStaticLocation.Failure.invalid); pending = nil }
  func cancel() { cancelled += 1 }
}
@main struct LocationSearchTests {
  @MainActor static func main() async throws {
    var life = ZoenLocationLifetime()
    let first = try life.activate("same")
    precondition(life.current(first))
    life.retire()
    precondition(!life.current(first))
    let next = try life.activate("same")
    precondition(next != first && !life.current(first) && life.current(next))
    for invalid in ["", "x\n", String(repeating: "x", count: 129)] {
      do { _ = try life.activate(invalid); preconditionFailure("invalid binding admitted") } catch {}
    }
    let mapConfig = try ZoenLocationMapConfiguration.decode(#"{"binding":"map","editable":false,"point":{"longitude":180,"latitude":-90}}"#)
    precondition(!mapConfig.editable && mapConfig.point?.longitude == 180)
    for invalid in [#"{"binding":"","editable":true}"#, #"{"binding":"x","editable":true,"point":{"longitude":181,"latitude":0}}"#] {
      do { _ = try ZoenLocationMapConfiguration.decode(invalid); preconditionFailure("invalid native map configuration") } catch {}
    }
    let old = Work(), fresh = Work(), third = Work(), fourth = Work()
    var queue = [old, fresh, third, fourth], made = 0
    let service = ZoenLocationSearch { _ in made += 1; return queue.removeFirst() }
    let point = ZoenStaticLocation.Point(longitude: 1, latitude: 2, label: "Actual result")
    let request = #"{"binding":"route","query":"Explicit place"}"#
    let a = Task { try await service.search(request) }
    await old.started()
    let b = Task { try await service.search(request) }
    await fresh.started()
    precondition(old.cancelled == 1)
    old.fail()
    do { _ = try await a.value; preconditionFailure("old failure reported success") } catch {}
    fresh.finish([point])
    let result = try await b.value
    let decoded = try JSONDecoder().decode([ZoenStaticLocation.Point].self, from: Data(result.utf8))
    precondition(decoded == [point], "late old error/finally cannot retire the newer same-binding operation")
    let c = Task { try await service.search(request) }
    await third.started()
    service.cancel(binding: "other-route")
    precondition(third.cancelled == 0)
    service.cancel(binding: "route")
    precondition(third.cancelled == 1)
    third.finish([point])
    do { _ = try await c.value; preconditionFailure("cancelled result escaped") } catch {}
    let d = Task { try await service.search(request) }
    await fourth.started()
    service.retire()
    precondition(fourth.cancelled == 1)
    fourth.finish([point])
    do { _ = try await d.value; preconditionFailure("background result escaped") } catch {}
    for invalid in [#"{"binding":"x","query":" "}"#, #"{"binding":"","query":"x"}"#, String(repeating: " ", count: 2049)] {
      do { _ = try await service.search(invalid); preconditionFailure("invalid search started") } catch {}
    }
    precondition(made == 4, "no search is created until an explicit bounded query is admitted")
    print("Location search PASS: actual production coordinator/lease rejects stale same-binding completion, cancellation, foreign cancel, background and malformed queries; MapKit transport not invoked.")
  }
}
