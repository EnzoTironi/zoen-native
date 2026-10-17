import ExpoModulesCore
import Foundation

public class ZoenCoreModule: Module {
  private let session = ZoenSession()

  public func definition() -> ModuleDefinition {
    Name("ZoenCore")
    Events("onChange")

    AsyncFunction("open") { [session, weak self] (mode: String) async throws -> String in
      try await session.open(mode: mode) { [weak self] event in
        self?.sendEvent("onChange", event)
      }
    }
    AsyncFunction("request") { (request: String) async throws -> String in
      try await self.session.request(request)
    }
    AsyncFunction("createAccount") { (name: String, handle: String, relay: String) async throws -> String in
      try await self.session.createAccount(name: name, handle: handle, relay: relay)
    }
    AsyncFunction("sendFile") { (spaceId: String, uri: String, name: String, mime: String) async throws -> String in
      try await self.session.sendFile(spaceId: spaceId, uri: uri, name: name, mime: mime)
    }
    AsyncFunction("sendVoice") { (spaceId: String, uri: String, mime: String, durationMs: Double, levels: [Double], replyTo: String?, thread: Bool) async throws -> String in
      try await self.session.sendVoice(spaceId: spaceId, uri: uri, mime: mime, durationMs: durationMs, levels: levels, replyTo: replyTo, thread: thread)
    }
    AsyncFunction("fileInfo") { (itemId: String, version: Int?) async throws -> String in
      try await self.session.fileInfo(itemId: itemId, version: version)
    }
    AsyncFunction("materializeFile") { (itemId: String, version: Int?) async throws -> String in
      try await self.session.materializeFile(itemId: itemId, version: version)
    }
    AsyncFunction("releaseLocalFile") { (uri: String) async in await self.session.releaseLocalFile(uri) }
    AsyncFunction("saveProfile") { (request: String) async throws in try await self.session.saveProfile(request) }
    AsyncFunction("materializeProfilePhoto") { (ownerId: String, deviceId: String, identityId: String, expectedSha256: String) async throws -> String in
      try await self.session.materializeProfilePhoto(ownerId: ownerId, deviceId: deviceId, identityId: identityId, expectedSha256: expectedSha256)
    }
    AsyncFunction("releaseProfilePhoto") { (uri: String) async in await self.session.releaseProfilePhoto(uri) }
    AsyncFunction("eraseDevice") { (identityId: String, deviceId: String) async throws in
      try await self.session.eraseDevice(expected: ZoenDeviceScope(identityId: identityId, deviceId: deviceId))
    }
    AsyncFunction("close") { () async in await self.session.close() }
    OnAppBecomesActive { [session] in Task { await session.resumePendingHistory() } }
    OnDestroy { Task { await self.session.close() } }
  }
}

private actor ZoenSession {
  private var core: RodaEngine?
  private var mode = "account"
  private let vault = ZoenVault()
  private var listener: ZoenListener?
  private var syncStarted = false
  private var historyTask: Task<Void, Never>?
  private var historyRearmRequested = false
  private var generation: UInt64 = 0
  private let erasure = ZoenDeviceErasure()
  private var erasing = false
  private var erasureTask: Task<Void, Error>?
  private var erasureScope: ZoenDeviceScope?
  private var completedErasure: ZoenDeviceScope?
  private var requests = 0
  private var requestDrain: [CheckedContinuation<Void, Never>] = []
  private let mediaFiles = ZoenMediaFiles()
  private let profilePhotos = ZoenProfilePhotos()

  func open(mode: String, changed: @escaping @Sendable ([String: Any]) -> Void) async throws -> String {
    guard ["account", "demo"].contains(mode) else { throw SessionError.invalidMode }
    if erasureTask != nil { throw ZoenDeviceErasure.Failure.incomplete }
    if let pending = try erasure.pending() {
      self.mode = "account"
      try await eraseDevice(expected: pending)
    }
    guard !erasing else { throw ZoenDeviceErasure.Failure.incomplete }
    close()
    let openingGeneration = generation
    // The shared app keeps the engine's existing primary store, rather than creating
    // a second account database beside the same Keychain identity.
    let root = URL.applicationSupportDirectory.appending(path: mode == "account" ? "Roda" : "Zoen/demo", directoryHint: .isDirectory)
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    var excluded = URLResourceValues()
    excluded.isExcludedFromBackup = true
    var directory = root
    try directory.setResourceValues(excluded)
    let engine = try RodaEngine.open(path: root.appending(path: mode == "account" ? "roda.sqlite" : "zoen.sqlite").path, locale: Locale.current.identifier)
    let bridge = ZoenListener(changed) { [weak self] in
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
      guard generation == openingGeneration else { throw SessionError.closed }
      // Publish only the fully validated session. A rejected unlock must leave
      // no callable engine behind; an older async open cannot replace a newer one.
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
    guard !erasing, let core else { throw SessionError.closed }
    requests += 1
    defer { retireRequest() }
    let result = mode == "demo"
      ? try await core.clientRequest(request: request)
      : try await core.clientVaultRequest(request: request, vault: vault)
    guard !erasing, self.core === core else { throw SessionError.closed }
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
    guard !erasing, mode == "account", let core, let listener else { throw SessionError.closed }
    guard core.account() == nil else { throw SessionError.accountExists }
    guard !vault.hasAccountMaterial() else { throw SessionError.orphanedKeys }
    _ = try core.createAccount(name: name, handle: handle, relayUrl: relay, vault: vault)
    try core.startSync(listener: listener)
    syncStarted = true
    return try await core.clientRequest(request: #"{"op":"snapshot"}"#)
  }

  func sendFile(spaceId: String, uri: String, name: String, mime: String) throws -> String {
    guard !erasing, let core else { throw SessionError.closed }
    let selection = try mediaFiles.readSelection(uri: uri, name: name, mime: mime)
    let thumbnail = try? ZoenMediaFiles.thumbnail(at: selection.url)
    let item = try core.fileAdd(spaceId: spaceId, path: selection.name, name: selection.name, mime: selection.mime, bytes: selection.data, thumbnail: thumbnail)
    return try ZoenMediaFiles.json(["itemId": item.id])
  }

  func sendVoice(spaceId: String, uri: String, mime: String, durationMs: Double, levels: [Double], replyTo: String?, thread: Bool) throws -> String {
    guard !erasing, let core else { throw SessionError.closed }
    guard durationMs.isFinite, durationMs >= 250, durationMs <= 3_600_000,
          levels.count <= 1800, levels.allSatisfy({ $0.isFinite && $0 >= 0 && $0 <= 1 }),
          mime.hasPrefix("audio/") else { throw ZoenMediaFiles.Failure.invalidRecording }
    let selection = try mediaFiles.readSelection(uri: uri, name: "voice.m4a", mime: mime)
    let entry = try core.sendVoiceNote(spaceId: spaceId, bytes: selection.data, mime: selection.mime,
      ms: UInt64(durationMs.rounded()), levels: levels.map(Float.init), transcript: "", replyTo: replyTo, thread: thread)
    guard let voice = entry.voice else { throw ZoenMediaFiles.Failure.invalidRecording }
    return try ZoenMediaFiles.json(["messageId": entry.id, "itemId": voice.itemId])
  }

  func fileInfo(itemId: String, version: Int?) throws -> String {
    guard !erasing, let core else { throw SessionError.closed }
    return try ZoenMediaFiles.json(ZoenMediaFiles.info(try core.fileInfoAt(itemId: itemId, version: ZoenMediaFiles.version(version))))
  }

  func materializeFile(itemId: String, version: Int?) throws -> String {
    guard !erasing, let core else { throw SessionError.closed }
    // Select the version once. An incoming edit cannot swap the description and
    // payload between these reads, even while the sync engine runs concurrently.
    let pinned = try ZoenMediaFiles.version(version) ?? core.item(itemId: itemId).version
    let info = try core.fileInfoAt(itemId: itemId, version: pinned)
    guard let bytes = try core.fileBytesBounded(itemId: itemId, version: pinned, maxBytes: ZoenMediaFiles.maximumBytes) else { return "null" }
    let uri = try mediaFiles.stage(bytes, name: info.name)
    return try ZoenMediaFiles.json(["uri": uri, "itemId": itemId, "version": pinned, "info": ZoenMediaFiles.info(info)])
  }

  func releaseLocalFile(_ uri: String) { mediaFiles.release(uri) }

  func saveProfile(_ request: String) throws {
    guard !erasing, request.utf8.count <= 8192, let core else { throw SessionError.closed }
    let save = try JSONDecoder().decode(ZoenProfilePhotos.Save.self, from: Data(request.utf8))
    guard let account = core.account(), account.unlocked, !account.setupPending, account.identityId == save.ownerId, account.deviceId == save.deviceId else { throw ZoenProfilePhotos.Failure.changedSession }
    _ = try core.updateMyProfile(name: save.name, bio: save.bio, photo: profilePhotos.change(save))
    listener?.onChange(spaceIds: [])
  }

  func materializeProfilePhoto(ownerId: String, deviceId: String, identityId: String, expectedSha256: String) throws -> String {
    guard !erasing, let core, let account = core.account(), account.unlocked, !account.setupPending, account.identityId == ownerId, account.deviceId == deviceId else { throw ZoenProfilePhotos.Failure.changedSession }
    guard let photo = try core.profilePhoto(identityId: identityId, expectedSha256: expectedSha256) else { return "null" }
    return try profilePhotos.stage(photo)
  }

  func releaseProfilePhoto(_ uri: String) { profilePhotos.release(uri) }

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
    guard !erasing, mode == "account", syncStarted, historyTask == nil, let engine = core,
          let bridge = listener, (try? engine.historyStatus())?.initialPending == true else { return }
    let expectedGeneration = generation
    historyRearmRequested = false
    let progress = ZoenHistoryListener(bridge)
    historyTask = Task { [weak self] in
      guard let self else { return }
      await self.resumeInitialHistory(engine: engine, expectedGeneration: expectedGeneration, progress: progress)
    }
  }

  private func resumeInitialHistory(engine: RodaEngine, expectedGeneration: UInt64, progress: ZoenHistoryListener) async {
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

  private func retireRequest() {
    requests -= 1
    if requests == 0 {
      let waiting = requestDrain; requestDrain.removeAll()
      for waiter in waiting { waiter.resume() }
    }
  }

  func eraseDevice(expected: ZoenDeviceScope) async throws {
    if let erasureTask {
      guard erasureScope == expected else { throw ZoenDeviceErasure.Failure.changed }
      try await erasureTask.value
      return
    }
    guard mode == "account" else { throw ZoenDeviceErasure.Failure.changed }
    let pending = try erasure.pending()
    if core == nil, pending != expected, completedErasure != expected { throw ZoenDeviceErasure.Failure.changed }
    let engine = try core ?? RodaEngine.open(path: ZoenDeviceErasure.accountRoot.appending(path: "roda.sqlite").path, locale: Locale.current.identifier)
    if let account = engine.account() {
      guard account.identityId == expected.identityId, account.deviceId == expected.deviceId else { throw ZoenDeviceErasure.Failure.changed }
    } else {
      guard pending == expected || completedErasure == expected else { throw ZoenDeviceErasure.Failure.changed }
    }
    try erasure.begin(expected)
    erasing = true
    erasureScope = expected
    generation &+= 1
    let history = historyTask
    history?.cancel()
    historyTask = nil
    historyRearmRequested = false
    engine.cancelHistoryReceive()
    engine.stopSync()
    syncStarted = false
    let task = Task {
      if requests > 0 { await withCheckedContinuation { requestDrain.append($0) } }
      await history?.value
      // An older suspended provider command may have changed the canonical scope.
      if let account = engine.account() {
        guard account.identityId == expected.identityId, account.deviceId == expected.deviceId else { throw ZoenDeviceErasure.Failure.changed }
      }
      try engine.eraseDevice(vault: vault)
      try vault.eraseAllVerified()
      mediaFiles.removeAll()
      profilePhotos.removeAll()
      try ZoenDeviceErasure.erasePreviewCache()
      try erasure.complete()
      completedErasure = expected
      core = nil
      listener = nil
      erasing = false
    }
    erasureTask = task
    defer { erasureTask = nil }
    try await task.value
  }

  func close() {
    generation &+= 1
    historyTask?.cancel()
    historyTask = nil
    historyRearmRequested = false
    core?.cancelHistoryReceive()
    core?.stopSync()
    core = nil
    listener = nil
    syncStarted = false
    mediaFiles.removeAll()
    profilePhotos.removeAll()
  }
}

private enum SessionError: Error, LocalizedError {
  case closed, invalidMode, accountExists, locked, orphanedKeys
  var errorDescription: String? {
    switch self {
    case .closed: "Esta sessão foi encerrada. Abra o app novamente."
    case .invalidMode: "Não foi possível abrir esta sessão."
    case .accountExists: "Este aparelho já tem uma conta."
    case .locked: "Não foi possível desbloquear esta conta com as chaves deste aparelho. Use a recuperação da conta."
    case .orphanedKeys: "Há chaves de uma conta neste aparelho, mas o banco de dados não está disponível. Recupere a conta para preservar essas chaves."
    }
  }
}

private final class ZoenListener: CoreListener, @unchecked Sendable {
  private let changed: @Sendable ([String: Any]) -> Void
  private let resumeHistory: @Sendable () -> Void
  private let connectionLock = NSLock()
  private var wasReady = false
  init(_ changed: @escaping @Sendable ([String: Any]) -> Void, resumeHistory: @escaping @Sendable () -> Void = {}) {
    self.changed = changed
    self.resumeHistory = resumeHistory
  }
  func onChange(spaceIds: [String]) { changed(["kind": "change", "spaceIds": spaceIds]) }
  func onEphemeral(spaceId: String, fromId: String, kind: String, detail: String) {
    changed(["kind": "ephemeral", "spaceId": spaceId, "fromId": fromId, "type": kind, "detail": detail])
    if kind == "history_ready", spaceId.isEmpty, detail.isEmpty { resumeHistory() }
  }
  func onPresence(identityId: String, online: Bool) { changed(["kind": "presence", "identityId": identityId, "online": online]) }
  func onConnection(status: ConnectionDto) {
    let ready = status.state == "online" && status.synced
    connectionLock.lock()
    let becameReady = ready && !wasReady
    wasReady = ready
    connectionLock.unlock()
    changed(["kind": "connection"])
    if becameReady { resumeHistory() }
  }
  func onError(message: String) { changed(["kind": "error", "message": message]) }
  func onProfileChanged(identityId: String) { changed(["kind": "profile", "identityId": identityId]) }
}

private final class ZoenHistoryListener: TransferListener, @unchecked Sendable {
  private let listener: ZoenListener
  init(_ listener: ZoenListener) { self.listener = listener }
  func onProgress(done: UInt32, total: UInt32) { listener.onChange(spaceIds: []) }
}
