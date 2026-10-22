import CoreGraphics
import CryptoKit
import Foundation
import PDFKit

extension PDFEditJournalTests {
  static func failedDiscardAndForeignCASNeverClearNewerEditor() throws {
    let f = try PDFJournalFixture(); defer { f.remove() }
    let file = try f.lease(PDFJournalFixture.pdf("own", shade: 0.4))
    let foreign = try PDFJournalFixture.pdf("foreign", shade: 0.8)
    let store = PDFJournalStore(canonical: f.base), j = try journal(f, store)
    try j.retain(file); store.failDiscardBeforePersistence()
    refused("Failed discard was silently acknowledged") { try j.discard() }
    expect(store.snapshot().pending.count == 1)
    try j.retain(file)
    store.raceForeignCaptureBeforeDiscard(foreign)
    refused("Discard raced a foreign edit without CAS") { try j.discard() }
    let foreignState = store.snapshot(), attempts = store.discards().count
    expect(foreignState.editorID == "foreign_editor" && foreignState.pending.count == 1)
    refused("Retired editor cleared foreign state on retry") { try j.discard() }
    expect(store.discards().count == attempts && store.snapshot().revision == foreignState.revision)
    let another = try f.lease(f.original)
    refused("Old editor staged over foreign captures") { try j.retain(another) }
    expect(store.snapshot().pending == foreignState.pending && store.versionCount() == 0)
    store.seed(.init(revision: "foreign_empty", editorID: "foreign_editor", origin: f.base, base: f.base, pending: []), assets: [:])
    refused("An old callback cleared a newer editor's empty journal") { try j.discard() }
    expect(store.discards().count == attempts && store.snapshot().revision == "foreign_empty")
  }

  static func successfulDiscardPermanentlyRetiresTheJournal() throws {
    let f = try PDFJournalFixture(); defer { f.remove() }
    let file = try f.lease(f.original), cap = capture("already_recovered", f.original)
    let store = PDFJournalStore(canonical: f.base)
    store.seed(.init(revision: "before_discard", editorID: "editor", origin: f.base, base: f.base, pending: [cap]), assets: [cap.id: f.original])
    let j = try journal(f, store)
    try j.registerRecovered(file, capture: cap)
    try j.discard(); try j.discard()
    refused("Discarded editor admitted a new capture") { try j.retain(file) }
    refused("Discarded editor attempted a save") { _ = try j.save(file) }
    refused("Discarded editor remapped recovery") { try j.registerRecovered(try f.lease(f.original), capture: cap) }
    refused("Discarded editor read prior private bytes") { _ = try j.recoveredBytes(cap) }
    expect(store.discards().count == 1 && store.stages().isEmpty && store.commits().isEmpty)
    expect(FileManager.default.fileExists(atPath: file.url.path), "Journal discarded a lease it does not own")
    let unwritten = PDFJournalStore(canonical: f.base), failed = try journal(f, unwritten)
    unwritten.failStageBeforePersistence()
    refused("Unwritten capture must remain retryable before discard") { try failed.retain(file) }
    try failed.discard()
    refused("Post-discard reconciliation restaged an old failed capture") { _ = try failed.save(file) }
    expect(unwritten.stages().count == 1 && unwritten.commits().isEmpty)
  }

  static func emptyJournalStartsFreshWhileRecoveryKeepsOriginAndBase() throws {
    let f = try PDFJournalFixture(); defer { f.remove() }
    let store = PDFJournalStore(canonical: f.base)
    let fresh = try journal(f, store), second = try journal(f, store)
    expect(fresh.origin == f.base && fresh.initialBase == f.base && fresh.editorID != second.editorID)
    let older = PDFBase(version: 3, sha256: String(repeating: "a", count: 64))
    store.seed(.init(revision: "old_completed", editorID: "old_editor", origin: older, base: f.base, pending: []), assets: [:])
    let next = try journal(f, store)
    expect(next.editorID != "old_editor" && next.origin == f.base && next.initialBase == f.base,
                 "An empty completed journal reused a stale editor/origin")
    try next.retain(f.lease(f.original))
    expect(store.snapshot().origin == f.base && store.snapshot().editorID == next.editorID)
  }

  static func invalidDTOsAndAcknowledgmentsFailClosed() throws {
    let f = try PDFJournalFixture(); defer { f.remove() }
    let store = PDFJournalStore(canonical: f.base), real = store.backend(), cap = capture("valid", f.original)
    let valid = PDFJournal.State(revision: "valid_revision", editorID: "editor", origin: f.base, base: f.base, pending: [cap])
    func state(revision: String = "valid_revision", editor: String? = "editor", origin: PDFBase? = nil,
               base: PDFBase? = nil, pending: [PDFJournal.Capture]? = nil) -> PDFJournal.State {
      .init(revision: revision, editorID: editor, origin: origin ?? f.base, base: base ?? f.base, pending: pending ?? [cap])
    }
    let badStates: [PDFJournal.State] = [
      state(revision: ""), state(revision: String(repeating: "r", count: 129)), state(revision: "bad/revision"),
      state(revision: "révision"), state(revision: "0"), state(editor: "bad editor"),
      .init(revision: "partial", editorID: nil, origin: f.base, base: f.base, pending: []),
      .init(revision: "partial", editorID: "editor", origin: nil, base: f.base, pending: []),
      .init(revision: "partial", editorID: "editor", origin: f.base, base: nil, pending: []),
      .init(revision: "unbound", editorID: nil, origin: nil, base: nil, pending: [cap]),
      state(origin: .init(version: 0, sha256: f.base.sha256)),
      state(base: .init(version: 6, sha256: f.base.sha256)),
      state(base: .init(version: 7, sha256: String(repeating: "b", count: 64))),
      state(origin: .init(version: 7, sha256: String(repeating: "A", count: 64))),
      state(pending: [cap, cap]),
      state(pending: [.init(id: "bad/id", sha256: cap.sha256, size: cap.size)]),
      state(pending: [.init(id: "short", sha256: "bad", size: cap.size)]),
      state(pending: [.init(id: "small", sha256: cap.sha256, size: 4)]),
      state(pending: [.init(id: "huge", sha256: cap.sha256, size: journalMaximum + 1)]),
      state(pending: (0..<17).map { .init(id: "capture_\($0)", sha256: cap.sha256, size: cap.size) }),
      state(pending: (0..<3).map { .init(id: "large_\($0)", sha256: cap.sha256, size: journalMaximum) }),
    ]
    for invalid in badStates {
      let backend = PDFJournal.Backend(load: { invalid }, stage: real.stage, bytes: real.bytes, commit: real.commit, discard: real.discard)
      refused("Malformed DTO admitted a private PDF editor") { _ = try journal(f, store, backend: backend) }
    }
    expect(valid.pending == [cap] && store.stages().isEmpty && store.commits().isEmpty)
    for base in [PDFBase(version: 0, sha256: cap.sha256), .init(version: 1, sha256: "bad")] {
      refused("Invalid observed canonical base admitted") { _ = try PDFJournal(observedBase: base, maximumBytes: journalMaximum, backend: real) }
    }
    for maximum in [UInt64(4), journalMaximum + 1] {
      refused("Invalid configured bounds admitted") { _ = try PDFJournal(observedBase: f.base, maximumBytes: maximum, backend: real) }
    }
    for mode in 0..<3 {
      let model = PDFJournalStore(canonical: f.base), backend = model.backend()
      let wrongDraftACK = PDFJournal.Backend(load: backend.load, stage: { request in
        let actual = try backend.stage(request)
        let captured = actual.draft.pending[0]
        let pending: [PDFJournal.Capture] = mode == 1 ? [] : mode == 2
          ? [.init(id: captured.id, sha256: String(repeating: "a", count: 64), size: captured.size)]
          : actual.draft.pending
        let draft = PDFJournal.State(revision: actual.draft.revision,
          editorID: mode == 0 ? "foreign_editor" : actual.draft.editorID,
          origin: actual.draft.origin, base: actual.draft.base, pending: pending)
        return .init(acceptedRevision: actual.acceptedRevision, draft: draft)
      }, bytes: backend.bytes, commit: backend.commit, discard: backend.discard)
      let j = try journal(f, model, backend: wrongDraftACK), file = try f.lease(f.original)
      refused("Wrong editor/capture ACK admitted publication") { _ = try j.save(file) }
      expect(model.snapshot().pending.count == 1 && model.commits().isEmpty)
    }
    for accepted in ["", "0", "bad/revision", String(repeating: "a", count: 129)] {
      let model = PDFJournalStore(canonical: f.base), backend = model.backend()
      let invalidACK = PDFJournal.Backend(load: backend.load, stage: { request in
        let actual = try backend.stage(request)
        return .init(acceptedRevision: accepted, draft: actual.draft)
      }, bytes: backend.bytes, commit: backend.commit, discard: backend.discard)
      let j = try journal(f, model, backend: invalidACK), file = try f.lease(f.original)
      refused("Malformed stage ACK admitted publication") { try j.retain(file) }
      refused("A save bypassed the malformed stage ACK") { _ = try j.save(file) }
      expect(model.snapshot().pending.count == 1 && model.commits().isEmpty,
                   "Bad ACK rejection must preserve actual persisted bytes, not publish/discard them")
    }
  }
}
