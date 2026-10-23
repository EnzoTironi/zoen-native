import ExpoModulesCore
import Foundation
import UIKit
import PDFKit
#if canImport(FoundationModels)
import FoundationModels
#endif

actor ZoenSession {
  // A close returns its exact native UI epoch; a stale open cannot resume a newer retirement.
  private var keyboardAdmissionTicket: UUID?
  private var galleryAdmissionTicket: UUID?
  nonisolated let pickupLifetime = ZoenModulePickupLifetime()
  private var pickupBinding: UUID?
  var core: Client?
  var mode = "account"
  var sessionLocale = "en"
  private let vault = ZoenVault()
  var listener: ZoenListener?
  private var syncStarted = false
  private var historyTask: Task<Void, Never>?
  private var historyRearmRequested = false
  var generation: UInt64 = 0
  private var openingAttempt: UUID?
  var closing = false
  private var forceClosing = false
  private var closeTask: Task<Void, Error>?
  private let erasure = ZoenDeviceErasure()
  var erasing = false
  private var erasureTask: Task<Void, Error>?
  private var erasureScope: ZoenDeviceScope?
  private var completedErasure: ZoenDeviceScope?
  var requests = 0
  private var requestDrain: [CheckedContinuation<Void, Never>] = []
  let mediaFiles = ZoenMediaFiles()
  let filePreview = ZoenFilePreview()
  var previewBindings: [String: PreviewBinding] = [:]
  struct RecentSelectionBinding {
    let engine: Client
    let generation: UInt64
    let scope: ZoenMediaDraftBridge.Scope
    let kind: RecentAttachmentKind
    let reference: RecentAttachmentReference
  }
  var recentSelections: [String: RecentSelectionBinding] = [:]
  nonisolated let draftMediaEditLifetime = ZoenMediaEditLifetime()
  var draftMediaEdits: [UUID: DraftMediaEditBinding] = [:]
  var draftPreviewTasks: [UUID: Task<Void, Error>] = [:]
  // A cancellation may cross the bridge before its presentation is admitted.
  // These bounded, content-free tokens are not a second draft store.
  var cancelledDraftPreviews: [UUID] = []
  var conversationGallerySnapshots: [UUID: (ConversationGalleryScope, [ZoenConversationMediaGallery.Item])] = [:]
  var galleryValidation: Task<Void, Never>?
  var galleryValidationAgain = false
  let profilePhotos = ZoenProfilePhotos()
  let voiceEditor = ZoenVoiceEditor()
  var voiceSources: [String: UInt64] = [:]
  var voiceRenders: [String: String] = [:]
  var voiceRenderAttempts: [String: UUID] = [:]
  var assistants: [String: ZoenAssistantLease] = [:]
  var assistantDrains: [String: Task<Void, Never>] = [:]

  var pageInputTicket = UUID().uuidString
  var pageInputReservations: [String: String] = [:]
  var pageInputs: [String: ZoenPageInputRecord] = [:]
  var pageInputForcedDrain: Task<Void, Never>?
  var pageInputChanged: (@Sendable (String) -> Void)?
  nonisolated let pageInputLifetime = ZoenPageInputLifetime()

  private func releasePickupBinding() {
    if let pickupBinding { pickupLifetime.clear(pickupBinding) }
    pickupBinding = nil
  }

  struct FileScope: Equatable, Sendable {
    let identity: String
    let device: String
  }
  struct PreviewBinding: Sendable {
    let engine: Client
    let generation: UInt64
    let mode: String
    let scope: FileScope?
    let itemId: String
    let spaceId: String
    let version: UInt32
    let sha256: String
    let name: String
    let mime: String
  }

  func fileScope(spaceId: String, engine: Client) throws -> FileScope {
    guard !erasing, self.core === engine else { throw SessionError.closed }
    let space = try engine.space(spaceId: spaceId)
    guard let member = space.members.first(where: { $0.isMe }) else { throw SessionError.closed }
    if let account = engine.account() {
      guard account.unlocked, !account.setupPending, account.identityId == member.id else { throw SessionError.closed }
      return FileScope(identity: account.identityId, device: account.deviceId)
    }
    guard mode == "demo" else { throw SessionError.closed }
    return FileScope(identity: member.id, device: "demo")
  }

  func bindingIsCurrent(_ binding: PreviewBinding) -> Bool {
    guard !erasing, core === binding.engine, generation == binding.generation,
          mode == binding.mode, let expected = binding.scope,
          let current = try? fileScope(spaceId: binding.spaceId, engine: binding.engine) else { return false }
    return current == expected
  }


  func open(mode: String, locale: String, changed: @escaping @Sendable ([String: Any]) -> Void) async throws -> String {
    guard ["account", "demo"].contains(mode) else { throw SessionError.invalidMode }
    guard ["en", "pt-BR"].contains(locale) else { throw SessionError.invalidLocale }
    let attempt = UUID()
    openingAttempt = attempt
    if erasureTask != nil { throw ZoenDeviceErasure.Failure.incomplete }
    sessionLocale = locale
    if let pending = try erasure.pending() {
      self.mode = "account"
      try await eraseDevice(expected: pending, preservingOpening: attempt)
    }
    guard !erasing else { throw ZoenDeviceErasure.Failure.incomplete }
    try await close(cancelOpening: false)
    guard openingAttempt == attempt, !erasing else { throw SessionError.closed }
    guard let keyboardTicket = keyboardAdmissionTicket,
          let galleryTicket = galleryAdmissionTicket else { throw SessionError.closed }
    let openingGeneration = generation
    let root = mode == "account" ? ZoenDeviceErasure.accountRoot
      : URL.applicationSupportDirectory.appending(path: "demo", directoryHint: .isDirectory)
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    var excluded = URLResourceValues()
    excluded.isExcludedFromBackup = true
    var directory = root
    try directory.setResourceValues(excluded)
    let engine = try Client.open(path: root.appending(path: "client.sqlite").path, locale: sessionLocale)
    let bridge = ZoenListener(changed, sourceChanged: { [weak self] spaces in
      Task { await self?.validateConversationGalleryChanges(spaces) }
    }) { [weak self] in
      Task { await self?.resumePendingHistory() }
    }
    do {
      var started = false
      if mode == "account" { _ = try engine.resumeProviderSetup(vault: vault) }
      if mode == "demo" { _ = try engine.seedDemoIfEmpty() }
      else if engine.account() != nil {
        guard try engine.unlock(vault: vault) else { throw SessionError.locked }
        if engine.account()?.setupPending != true {
          try engine.startSync(listener: bridge)
          started = true
        }
      } else if vault.hasAccountMaterial() { throw SessionError.orphanedKeys }
      let snapshot = try await engine.clientRequest(request: #"{"op":"snapshot"}"#)
      guard generation == openingGeneration, openingAttempt == attempt else { throw SessionError.closed }
      let resumed = await MainActor.run {
        ZoenKeyboardPanelHost.resumeAdmissions(expected: keyboardTicket)
          && ZoenAttachmentGalleryHost.resumeAdmissions(expected: galleryTicket)
      }
      guard resumed else { throw SessionError.closed }
      guard generation == openingGeneration, openingAttempt == attempt, !erasing, !closing else { throw SessionError.closed }
      // Publish only the fully validated session. A rejected unlock must leave
      // no callable engine behind; an older async open cannot replace a newer one.
      guard let binding = pickupLifetime.bind(retire: { engine.retireLinkPickup() }) else {
        throw SessionError.closed
      }
      self.pickupBinding = binding
      self.core = engine
      self.listener = bridge
      self.mode = mode
      syncStarted = started
      scheduleInitialHistory()
      return snapshot
    } catch {
      engine.stopSync()
      throw error
    }
  }

  func request(_ request: String) async throws -> String {
    guard !erasing, !closing, let core else { throw SessionError.closed }
    requests += 1
    defer { retireRequest() }
    let result = mode == "demo"
      ? try await core.clientRequest(request: request)
      : try await core.clientVaultRequest(request: request, vault: vault)
    guard !erasing, !closing, self.core === core else { throw SessionError.closed }
    // A linking/recovery command can install the first account. Its secrets stay
    // inside this actor/vault boundary and sync starts only after unlock succeeds.
    if !syncStarted, core.account()?.unlocked == true, core.account()?.setupPending != true, let listener {
      try core.startSync(listener: listener)
      syncStarted = true
      scheduleInitialHistory()
    }
    return result
  }

  func createAccount(name: String, handle: String, relay: String) async throws -> String {
    guard !erasing, !closing, mode == "account", let core, let listener else { throw SessionError.closed }
    guard core.account() == nil else { throw SessionError.accountExists }
    guard !vault.hasAccountMaterial() else { throw SessionError.orphanedKeys }
    _ = try core.createAccount(name: name, handle: handle, relayUrl: relay, vault: vault)
    try core.startSync(listener: listener)
    syncStarted = true
    return try await core.clientRequest(request: #"{"op":"snapshot"}"#)
  }


  func resumePendingHistory() {
    if historyTask != nil {
      // Coalesce an external wake that raced the last attempt/defer. It must
      // not disappear merely because the previous bounded pump is finishing.
      historyRearmRequested = true
    } else {
      scheduleInitialHistory()
    }
  }

  private func scheduleInitialHistory() {
    guard !erasing, !closing, mode == "account", syncStarted, historyTask == nil, let engine = core,
          let bridge = listener, (try? engine.historyStatus())?.initialPending == true else { return }
    let expectedGeneration = generation
    historyRearmRequested = false
    let progress = ZoenHistoryListener(bridge)
    historyTask = Task { [weak self] in
      guard let self else { return }
      await self.resumeInitialHistory(engine: engine, expectedGeneration: expectedGeneration, progress: progress)
    }
  }

  private func resumeInitialHistory(engine: Client, expectedGeneration: UInt64, progress: ZoenHistoryListener) async {
    defer {
      if generation == expectedGeneration, core === engine {
        historyTask = nil
        if historyRearmRequested {
          historyRearmRequested = false
          scheduleInitialHistory()
        }
      }
    }
    // A bounded foreground resume. The canonical marker survives all failures;
    // devices UI offers explicit retry after these attempts or a later cold launch.
    for delay in [0, 2, 5, 15] {
      guard !Task.isCancelled, generation == expectedGeneration, core === engine else { return }
      if delay > 0 {
        do { try await Task.sleep(for: .seconds(delay)) } catch { return }
      }
      guard !Task.isCancelled, generation == expectedGeneration, core === engine else { return }
      do {
        let status = try engine.historyStatus()
        if !status.initialPending { return }
        if status.receiveActive { continue }
        _ = try await engine.receiveHistory(timeoutMs: 15_000, listener: progress)
        guard !Task.isCancelled, generation == expectedGeneration, core === engine else { return }
        listener?.onChange(spaceIds: [])
        if (try engine.historyStatus()).initialPending == false { return }
      } catch {
        guard !Task.isCancelled, generation == expectedGeneration, core === engine else { return }
        // Keep pending visible without leaking native transport diagnostics or
        // replacing the account with an empty/failed transfer projection.
        listener?.onChange(spaceIds: [])
      }
    }
  }

  func retireRequest() {
    requests -= 1
    if requests == 0 {
      let waiting = requestDrain; requestDrain.removeAll()
      for waiter in waiting { waiter.resume() }
    }
  }

  func eraseDevice(expected: ZoenDeviceScope, preservingOpening: UUID? = nil) async throws {
    if let erasureTask {
      guard erasureScope == expected else { throw ZoenDeviceErasure.Failure.changed }
      if let preservingOpening {
        guard openingAttempt == preservingOpening else { throw SessionError.closed }
      } else { openingAttempt = nil }
      try await erasureTask.value
      return
    }
    guard mode == "account" else { throw ZoenDeviceErasure.Failure.changed }
    let pending = try erasure.pending()
    if core == nil, pending != expected, completedErasure != expected { throw ZoenDeviceErasure.Failure.changed }
    let engine = try core ?? Client.open(path: ZoenDeviceErasure.accountRoot.appending(path: "client.sqlite").path, locale: sessionLocale)
    if let account = engine.account() {
      guard account.identityId == expected.identityId, account.deviceId == expected.deviceId else { throw ZoenDeviceErasure.Failure.changed }
    } else {
      guard pending == expected || completedErasure == expected else { throw ZoenDeviceErasure.Failure.changed }
    }
    // A confirmed erase supersedes any older suspended open. Only open's own
    // pending-erasure recovery may keep its exact ticket across this boundary.
    if let preservingOpening {
      guard openingAttempt == preservingOpening else { throw SessionError.closed }
    } else { openingAttempt = nil }
    try erasure.begin(expected)
    erasing = true
    let pageDrain = invalidatePageInputs()
    engine.retireLinkPickup()
    voiceSources.removeAll()
    voiceRenders.removeAll()
    voiceRenderAttempts.removeAll()
    erasureScope = expected
    generation &+= 1
    let clearingGeneration = generation
    filePreview.invalidate(before: clearingGeneration)
    let history = historyTask
    history?.cancel()
    historyTask = nil
    historyRearmRequested = false
    engine.cancelHistoryReceive()
    engine.stopSync()
    syncStarted = false
    let task = Task {
      await ZoenKeyboardPanelHost.retireAll()
      await ZoenAttachmentGalleryHost.retireAll()
      await pageDrain.value
      try await filePreview.clear(before: clearingGeneration)
      await closeAssistants()
      await engine.drainLinkPickup()
      if requests > 0 { await withCheckedContinuation { requestDrain.append($0) } }
      await history?.value
      // An older suspended provider command may have changed the canonical scope.
      if let account = engine.account() {
        guard account.identityId == expected.identityId, account.deviceId == expected.deviceId else { throw ZoenDeviceErasure.Failure.changed }
      }
      try await voiceEditor.releaseAll()
      try engine.eraseDevice(vault: vault)
      try vault.eraseAllVerified()
      mediaFiles.removeAll()
      previewBindings.removeAll()
      recentSelections.removeAll()
      profilePhotos.removeAll()
      try ZoenDeviceErasure.erasePreviewCache()
      try erasure.complete()
      completedErasure = expected
      releasePickupBinding()
      core = nil
      listener = nil
      erasing = false
    }
    erasureTask = task
    defer { erasureTask = nil }
    try await task.value
  }

  func close(discardEdits: Bool = false, cancelOpening: Bool = true) async throws {
    if cancelOpening { openingAttempt = nil }
    if let closeTask {
      if discardEdits {
        forceClosing = true
        let pageDrain = invalidatePageInputs()
        // Escalate a graceful close that is already waiting on the editor.
        // Its admission fence is in place before this suspension.
        generation &+= 1
        filePreview.invalidate(before: generation)
        try await filePreview.clear(before: generation)
        await pageDrain.value
      }
      try await closeTask.value
      return
    }
    // Fence new work and publish the coalesced task BEFORE its first await,
    // including forced teardown; a new preview cannot enter while clear drains.
    closing = true
    forceClosing = discardEdits
    let forcedPageDrain = discardEdits ? invalidatePageInputs() : nil
    let retiringCore = core
    retiringCore?.retireLinkPickup()
    let closeGeneration = generation
    let task = Task {
      defer { self.closing = false; self.forceClosing = false; self.closeTask = nil }
      let keyboardTicket = await ZoenKeyboardPanelHost.retireAll()
      self.keyboardAdmissionTicket = keyboardTicket
      let galleryTicket = await ZoenAttachmentGalleryHost.retireAll()
      self.galleryAdmissionTicket = galleryTicket
      do {
        if let forcedPageDrain { await forcedPageDrain.value }
        if discardEdits {
          // Module destruction drains accepted workers and discards temporary edits.
          self.generation &+= 1
          self.filePreview.invalidate(before: self.generation)
          try await self.filePreview.clear(before: self.generation)
        }
        if let erasureTask = self.erasureTask { try await erasureTask.value }
        await self.closeAssistants()
        // Keep the exact engine/account live for already admitted edited copies.
        // New bridge operations are fenced by `closing`; a save failure stays
        // recoverable in native UI instead of turning account change into data loss.
        do { try await self.preparePageInputsForClose() }
        catch { if !self.forceClosing { throw error } }
        if let pageDrain = self.pageInputForcedDrain { await pageDrain.value }
        try await self.filePreview.prepareForClose(before: self.generation &+ 1)
        if let retiringCore { await retiringCore.drainLinkPickup() }
        if self.requests > 0 {
          await withCheckedContinuation { self.requestDrain.append($0) }
        }
        guard !self.erasing else { throw SessionError.closed }
        try await self.finishClose()
      } catch {
        // A failed graceful close may keep the exact old session available for recovery.
        // Never reopen UI admissions across erasure, forced teardown or a newer native epoch.
        if !self.forceClosing, !self.erasing, self.generation == closeGeneration,
           let retiringCore, self.core === retiringCore {
          await MainActor.run {
            if ZoenKeyboardPanelHost.resumeAdmissions(expected: keyboardTicket) {
              ZoenAttachmentGalleryHost.resumeAdmissions(expected: galleryTicket)
            }
          }
        }
        throw error
      }
    }
    closeTask = task
    try await task.value
  }

  private func finishClose() async throws {
    pageInputTicket = UUID().uuidString
    pageInputReservations.removeAll()
    pageInputChanged = nil
    voiceSources.removeAll()
    voiceRenders.removeAll()
    voiceRenderAttempts.removeAll()
    generation &+= 1
    let closingGeneration = generation
    filePreview.invalidate(before: closingGeneration)
    historyTask?.cancel()
    historyTask = nil
    historyRearmRequested = false
    core?.cancelHistoryReceive()
    core?.stopSync()
    releasePickupBinding()
    core = nil
    listener = nil
    syncStarted = false
    mediaFiles.removeAll()
    previewBindings.removeAll()
    recentSelections.removeAll()
    profilePhotos.removeAll()
    await closeAssistants()
    try? await voiceEditor.releaseAll()
    try await filePreview.clear(before: closingGeneration)
  }
}
