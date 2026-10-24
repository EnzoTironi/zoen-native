import ExpoModulesCore
import Foundation
import UIKit
import PDFKit
#if canImport(FoundationModels)
import FoundationModels
#endif

extension ZoenSession {
  func pageInputSession(changed: @escaping @Sendable (String) -> Void) throws -> String {
    guard !closing else { throw ZoenPageInputLease.Failure.closed }
    _ = try requirePageInputSession(pageInputTicket)
    pageInputChanged = changed
    return pageInputTicket
  }

  private func requirePageInputSession(_ ticket: String) throws -> Client {
    guard ticket == pageInputTicket, !erasing, mode == "account", let core,
          let account = core.account(), account.unlocked, !account.setupPending else {
      throw ZoenPageInputLease.Failure.closed
    }
    return core
  }

  private func pageInputScope(_ ticket: String, itemId: String) throws -> FileEditDraftScope {
    let engine = try requirePageInputSession(ticket)
    // This accessor admits BinaryPage, not a Plan projection or historical document.
    let page = try engine.page(itemId: itemId)
    guard page.ready else { throw ZoenPageInputLease.Failure.unavailable }
    let owner = try fileScope(spaceId: page.spaceId, engine: engine)
    return FileEditDraftScope(expectedIdentityId: owner.identity, expectedDeviceId: owner.device,
      spaceId: page.spaceId, itemId: itemId)
  }

  private func currentPageInput(_ ticket: String, id: String) throws -> ZoenPageInputRecord {
    let engine = try requirePageInputSession(ticket)
    guard let record = pageInputs[id], record.request.session == ticket,
          record.engine === engine, record.generation == generation, record.lease.status().valid,
          let owner = try? fileScope(spaceId: record.scope.spaceId, engine: engine),
          owner.identity == record.scope.expectedIdentityId,
          owner.device == record.scope.expectedDeviceId else { throw ZoenPageInputLease.Failure.closed }
    return record
  }

  func loadPageInput(session: String, itemId: String) throws -> String {
    guard !closing else { throw ZoenPageInputLease.Failure.closed }
    let engine = try requirePageInputSession(session)
    let loaded = try engine.pageInputLoad(scope: pageInputScope(session, itemId: itemId))
    let mode = loaded.binding.map { pageInputModeName($0.mode) }
    return try ZoenMediaFiles.json(["revision": String(loaded.revision),
      "hasRetainedCapture": loaded.capture != nil,
      "blockId": loaded.binding.flatMap { pageInputBlockId($0.target) } as Any? ?? NSNull(),
      "target": loaded.binding.map { pageInputTargetName($0.target) } as Any? ?? NSNull(),
      "mode": mode as Any? ?? NSNull()])
  }

  func reservePageInput(session: String, itemId: String) throws -> String {
    guard !closing else { throw ZoenPageInputLease.Failure.closed }
    _ = try pageInputScope(session, itemId: itemId)
    if let existing = pageInputReservations.first(where: { $0.value == itemId }) { return existing.key }
    let id = UUID().uuidString.lowercased()
    pageInputReservations[id] = itemId
    return id
  }

  func openPageInput(_ json: String) async throws -> String {
    let request = try ZoenPageInputOpenRequest.decode(json)
    let engine = try requirePageInputSession(request.session)
    guard !closing, pageInputReservations[request.id] == request.itemId else { throw ZoenPageInputLease.Failure.closed }
    let record: ZoenPageInputRecord
    if let existing = pageInputs[request.id] {
      guard existing.request == request, existing.closeTask == nil else { throw ZoenPageInputLease.Failure.changed }
      record = try currentPageInput(request.session, id: request.id)
    } else {
      guard !pageInputs.values.contains(where: { $0.scope.itemId == request.itemId }) else { throw ZoenPageInputLease.Failure.changed }
      let page = try engine.page(itemId: request.itemId)
      guard page.ready, page.canEdit else { throw ZoenPageInputLease.Failure.unsupported }
      let scope = try pageInputScope(request.session, itemId: request.itemId)
      let loaded = try engine.pageInputLoad(scope: scope)
      guard let revision = UInt64(request.revision), loaded.revision == revision else { throw ZoenPageInputLease.Failure.changed }
      let target: PageInputTarget
      let context: String
      let inputMode: PageInputMode
      if request.resume {
        guard loaded.capture != nil, let binding = loaded.binding,
              pageInputModeName(binding.mode) == request.mode,
              pageInputTargetName(binding.target) == request.target.rawValue,
              pageInputBlockId(binding.target) == request.blockId else { throw ZoenPageInputLease.Failure.changed }
        target = binding.target; context = binding.baseEditContext; inputMode = binding.mode
      } else {
        guard loaded.capture == nil else { throw ZoenPageInputLease.Failure.changed }
        switch request.target {
        case .document: target = .wholeDocument
        case .append: target = .appendNew
        case .block:
          guard let blockId = request.blockId else { throw ZoenPageInputLease.Failure.unsupported }
          target = .block(blockId: blockId)
        }
        context = request.editContext
        switch request.mode {
        case "rich": inputMode = .rawRich
        case "markdown": inputMode = .markdown
        case "lossless-text": inputMode = .losslessText
        default: throw ZoenPageInputLease.Failure.unsupported
        }
      }
      let opened = try engine.pageInputOpen(scope: scope, expectedRevision: revision,
        writerId: UUID().uuidString, target: target, mode: inputMode, observedEditContext: context)
      guard let binding = opened.binding, binding.target == target, binding.mode == inputMode else {
        throw ZoenPageInputLease.Failure.changed
      }
      let block: [String: Any]?
      if request.target == .document { block = nil }
      else { block = try ZoenPageInputOpenRequest.seedBlock(binding.baseBlockJson) }
      if request.mode == "rich", let block {
        guard let kind = block["kind"] as? String, ["paragraph", "heading", "bullet", "numbered"].contains(kind),
              let spans = block["spans"] as? [[String: Any]],
              spans.allSatisfy({ ["b", "i", "s", "a"].contains($0["key"] as? String ?? "") }) else {
          throw ZoenPageInputLease.Failure.unsupported
        }
      }
      if request.mode == "lossless-text", let block {
        guard let spans = block["spans"] as? [[String: Any]],
              spans.allSatisfy({ ["b", "i", "s", "c", "a", "hb"].contains($0["key"] as? String ?? "") }) else {
          throw ZoenPageInputLease.Failure.unsupported
        }
      }
      let lease = try ZoenPageInputLease(id: request.id, engine: engine, generation: generation,
        scope: scope, opened: opened) { [weak self] id in
          Task { await self?.pageInputDidChange(session: request.session, id: id) }
        }
      try pageInputLifetime.bind(lease)
      record = ZoenPageInputRecord(request: request, engine: engine, generation: generation,
        scope: scope, lease: lease, binding: binding)
      pageInputs[request.id] = record
    }
    if record.token != nil { return try pageInputJSON(record, includeSeed: true) }
    if let task = record.openTask { return try await task.value }
    let task = Task { try await self.completePageInputOpen(record) }
    record.openTask = task
    defer { record.openTask = nil }
    return try await task.value
  }

  private func completePageInputOpen(_ record: ZoenPageInputRecord) async throws -> String {
    _ = try currentPageInput(record.request.session, id: record.request.id)
    let block: [String: Any]?
    if record.request.target == .document {
      guard record.request.mode == "markdown", case .wholeDocument = record.binding.target,
            let markdown = record.binding.baseDocumentMarkdown else {
        throw ZoenPageInputLease.Failure.unsupported
      }
      // The exact Core binding owns this seed at its base edit context. Never
      // substitute the current document or text from the JavaScript request.
      record.initialMarkdown = markdown
      block = nil
    } else {
      block = try ZoenPageInputOpenRequest.seedBlock(record.binding.baseBlockJson)
    }
    if record.request.mode != "lossless-text", let block {
      let request = try ZoenMediaFiles.json(["op": "page_block_markdown", "block": block])
      let encoded = try await record.engine.clientRequest(request: request)
      _ = try currentPageInput(record.request.session, id: record.request.id)
      guard let markdown = try JSONSerialization.jsonObject(with: Data(encoded.utf8), options: [.fragmentsAllowed]) as? String else {
        throw ZoenPageInputLease.Failure.unsupported
      }
      record.initialMarkdown = markdown
    }
    let mode = record.request.mode
    let seed = mode == "lossless-text" ? block?["text"] as? String ?? "" : record.initialMarkdown
    let spans = block?["spans"] as? [[String: Any]] ?? []
    let label = mode == "markdown" ? "Page Markdown" : "Page text"
    let producer = await ZoenPageInputProducer(lease: record.lease, mode: mode, initialText: seed, initialSpans: spans, label: label)
    record.producer = producer
    let token = try await producer.register()
    do {
      // Close may already be awaiting this open. Complete registration so close
      // owns its token; never make open and close wait on each other's rejection.
      _ = try currentPageInput(record.request.session, id: record.request.id)
      record.token = token
      return try pageInputJSON(record, includeSeed: true)
    } catch {
      record.lease.invalidate()
      await producer.revoke()
      throw error
    }
  }

  func pageInputStatus(session: String, id: String) throws -> String {
    try pageInputJSON(currentPageInput(session, id: id))
  }

  private func pageInputJSON(_ record: ZoenPageInputRecord, includeSeed: Bool = false) throws -> String {
    let status = record.lease.status()
    var value: [String: Any] = [
      "session": record.request.session, "id": record.request.id, "token": record.token as Any? ?? NSNull(),
      "itemId": record.scope.itemId, "blockId": pageInputBlockId(record.binding.target) as Any? ?? NSNull(),
      "target": pageInputTargetName(record.binding.target),
      "resuming": record.request.resume, "mode": record.request.mode,
      "revision": status.revision, "statusVersion": status.statusVersion, "sequence": status.sequence as Any? ?? NSNull(),
      "hasRetainedCapture": status.retained, "retentionPending": status.unretained,
      "failed": status.failed, "sealed": status.sealed, "completed": status.completed,
      "operation": status.operation?.rawValue as Any? ?? NSNull()
    ]
    // Legacy rich/block seeds cross JS once. Whole-document seed and all live
    // capture/selection/IME bytes remain in the native producer/journal.
    if includeSeed, record.request.target != .document { value["initialMarkdown"] = record.initialMarkdown }
    return try ZoenMediaFiles.json(value)
  }

  private func pageInputDidChange(session: String, id: String) {
    guard let record = try? currentPageInput(session, id: id), let json = try? pageInputJSON(record) else { return }
    pageInputChanged?(json)
  }

  func sealPageInput(session: String, id: String) async throws -> String {
    let record = try currentPageInput(session, id: id)
    guard !closing, record.closeTask == nil, record.operationTask == nil,
          let producer = record.producer else { throw ZoenPageInputLease.Failure.closed }
    try await producer.seal()
    _ = try currentPageInput(session, id: id)
    return try pageInputJSON(record)
  }

  func pageInputOperation(session: String, id: String, operation: ZoenPageInputLease.Operation) async throws -> String {
    let record = try currentPageInput(session, id: id)
    guard !closing, record.closeTask == nil else { throw ZoenPageInputLease.Failure.closed }
    return try await runPageInputOperation(record, operation: operation)
  }

  private func runPageInputOperation(_ record: ZoenPageInputRecord, operation: ZoenPageInputLease.Operation) async throws -> String {
    if let completed = record.completedResult {
      guard record.completedOperation == operation else { throw ZoenPageInputLease.Failure.wrongOperation }
      return completed
    }
    if let task = record.operationTask {
      guard record.runningOperation == operation else { throw ZoenPageInputLease.Failure.wrongOperation }
      return try await task.value
    }
    if let pending = record.lease.status().operation, pending != operation { throw ZoenPageInputLease.Failure.wrongOperation }
    let task = Task { try await self.performPageInputOperation(record, operation: operation) }
    record.runningOperation = operation
    record.operationTask = task
    defer { record.operationTask = nil; record.runningOperation = nil }
    return try await task.value
  }

  private func performPageInputOperation(_ record: ZoenPageInputRecord, operation: ZoenPageInputLease.Operation) async throws -> String {
    _ = try currentPageInput(record.request.session, id: record.request.id)
    guard let producer = record.producer else { throw ZoenPageInputLease.Failure.unavailable }
    if record.lease.status().operation == nil {
      if operation == .apply { try await producer.seal() }
      else { try await producer.freezeForDiscard() }
      _ = try currentPageInput(record.request.session, id: record.request.id)
      try record.lease.prepare(operation)
    }
    if operation == .apply { _ = try record.lease.apply() }
    else { _ = try record.lease.discard() }
    let result = try pageInputJSON(record)
    record.completedOperation = operation
    record.completedResult = result
    listener?.onChange(spaceIds: [record.scope.spaceId])
    pageInputChanged?(result)
    return result
  }

  func closePageInput(session: String, id: String) async throws {
    _ = try requirePageInputSession(session)
    guard let record = pageInputs[id] else {
      pageInputReservations.removeValue(forKey: id)
      return
    }
    _ = try currentPageInput(session, id: id)
    if let task = record.closeTask { try await task.value; return }
    let task = Task { try await self.performPageInputClose(record) }
    record.closeTask = task
    defer { record.closeTask = nil }
    try await task.value
  }

  private func performPageInputClose(_ record: ZoenPageInputRecord) async throws {
    if let opening = record.openTask { _ = await opening.result }
    _ = try currentPageInput(record.request.session, id: record.request.id)
    if let operation = record.operationTask { _ = try await operation.value }
    _ = try currentPageInput(record.request.session, id: record.request.id)
    if let pending = record.lease.status().operation, !record.lease.status().completed {
      if record.producer == nil { try record.lease.releaseEmpty() }
      else { _ = try await runPageInputOperation(record, operation: pending) }
    }
    if let producer = record.producer { try await producer.sealForClose() }
    else if !record.lease.status().retained { try record.lease.releaseEmpty() }
    _ = try currentPageInput(record.request.session, id: record.request.id)
    record.lease.invalidate()
    if let producer = record.producer { await producer.revoke() }
    pageInputLifetime.remove(record.request.id)
    pageInputs.removeValue(forKey: record.request.id)
    pageInputReservations.removeValue(forKey: record.request.id)
    listener?.onChange(spaceIds: [record.scope.spaceId])
  }

  func preparePageInputsForClose() async throws {
    for record in Array(pageInputs.values) { try await closePageInput(session: record.request.session, id: record.request.id) }
    pageInputReservations.removeAll()
    pageInputTicket = UUID().uuidString
  }

  func invalidatePageInputs() -> Task<Void, Never> {
    for record in pageInputs.values { record.lease.invalidate() }
    pageInputTicket = UUID().uuidString
    pageInputReservations.removeAll()
    if let drain = pageInputForcedDrain { return drain }
    let records = Array(pageInputs.values)
    let task = Task {
      defer { self.pageInputForcedDrain = nil }
      for record in records { if let producer = record.producer { await producer.revoke() } }
      for record in records {
        _ = await record.openTask?.result
        _ = await record.operationTask?.result
        _ = await record.closeTask?.result
        // An opening task may have installed a producer after the first revoke.
        if let producer = record.producer { await producer.revoke() }
        self.pageInputLifetime.remove(record.request.id)
        self.pageInputs.removeValue(forKey: record.request.id)
      }
    }
    pageInputForcedDrain = task
    return task
  }

  nonisolated func retireForModuleDestruction() {
    pickupLifetime.retire()
    pageInputLifetime.retire()
  }

}

final class ZoenPageInputRecord {
  let request: ZoenPageInputOpenRequest
  let engine: Client
  let generation: UInt64
  let scope: FileEditDraftScope
  let lease: ZoenPageInputLease
  let binding: PageInputBindingDto
  var producer: ZoenPageInputProducer?
  var initialMarkdown = ""
  var token: String?
  var openTask: Task<String, Error>?
  var operationTask: Task<String, Error>?
  var runningOperation: ZoenPageInputLease.Operation?
  var completedOperation: ZoenPageInputLease.Operation?
  var completedResult: String?
  var closeTask: Task<Void, Error>?
  init(request: ZoenPageInputOpenRequest, engine: Client, generation: UInt64,
       scope: FileEditDraftScope, lease: ZoenPageInputLease, binding: PageInputBindingDto) {
    self.request = request; self.engine = engine; self.generation = generation
    self.scope = scope; self.lease = lease; self.binding = binding
  }
}

private func pageInputModeName(_ mode: PageInputMode) -> String {
  switch mode {
  case .rawRich: "rich"
  case .markdown: "markdown"
  case .losslessText: "lossless-text"
  }
}

private func pageInputBlockId(_ target: PageInputTarget) -> String? {
  switch target {
  case .block(let blockId): blockId
  case .appendNew, .wholeDocument: nil
  }
}

private func pageInputTargetName(_ target: PageInputTarget) -> String {
  switch target {
  case .block: "block"
  case .appendNew: "append"
  case .wholeDocument: "document"
  }
}
