import Foundation

@MainActor protocol ZoenLocationSearchOperation: AnyObject {
  func start() async throws -> [ZoenStaticLocation.Point]
  func cancel()
}

/// One explicit search at a time. Late completion never clears/replaces a new
/// operation; cancellation has no effect on a different route's binding.
@MainActor final class ZoenLocationSearch {
  struct Request: Decodable {
    let binding: String
    let query: String
    static func decode(_ json: String) throws -> Self {
      guard json.utf8.count <= 2048 else { throw ZoenStaticLocation.Failure.invalid }
      let value = try JSONDecoder().decode(Self.self, from: Data(json.utf8))
      guard ZoenLocationLifetime.validBinding(value.binding),
            !value.query.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
            value.query.utf8.count <= 256,
            !ZoenStaticLocation.hasControls(value.query) else { throw ZoenStaticLocation.Failure.invalid }
      return value
    }
  }
  private var lifetime = ZoenLocationLifetime()
  private var operation: (ticket: ZoenLocationLifetime.Ticket, work: any ZoenLocationSearchOperation)?
  private let make: (String) -> any ZoenLocationSearchOperation
  init(make: @escaping (String) -> any ZoenLocationSearchOperation) { self.make = make }

  func search(_ json: String) async throws -> String {
    let request = try Request.decode(json)
    operation?.work.cancel()
    let ticket = try lifetime.activate(request.binding)
    let work = make(request.query)
    operation = (ticket, work)
    defer {
      if operation?.ticket == ticket { operation = nil }
    }
    let points = try await withTaskCancellationHandler {
      try await work.start()
    } onCancel: {
      Task { @MainActor [weak self] in self?.cancel(ticket: ticket) }
    }
    guard !Task.isCancelled, lifetime.current(ticket), operation?.ticket == ticket else { throw ZoenStaticLocation.Failure.retired }
    let bounded = try points.prefix(20).map { try $0.validated() }
    return String(decoding: try JSONEncoder().encode(bounded), as: UTF8.self)
  }
  func cancel(binding: String) {
    guard let ticket = operation?.ticket, ticket.binding == binding else { return }
    cancel(ticket: ticket)
  }
  private func cancel(ticket: ZoenLocationLifetime.Ticket) {
    guard operation?.ticket == ticket else { return }
    lifetime.retire()
    operation?.work.cancel()
    operation = nil
  }
  func retire() {
    lifetime.retire()
    operation?.work.cancel()
    operation = nil
  }
}
