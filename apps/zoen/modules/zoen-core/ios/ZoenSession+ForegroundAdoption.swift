import Foundation

extension ZoenSession {
  func retainsNotificationCall() -> Bool {
    notificationOnly && !closing && !erasing && callCoordinator != nil
      && callsModuleLease?.isLive == true && core?.account()?.unlocked == true
  }

  /// The process gate transfers a reference, not an account/database. Attach UI
  /// sinks to the existing listener without closing Core or the live media port.
  func adoptNotificationCall(locale: String, lease: ZoenModuleSessionLease,
    changed: @escaping @Sendable ([String: Any]) -> Void,
    call: @escaping @Sendable (String) -> Void,
    audio: @escaping @Sendable (String) -> Void) async throws -> String {
    guard ["en", "pt-BR"].contains(locale), notificationOnly, lease.isLive,
          !closing, !erasing, mode == "account", let core, let listener,
          core.account()?.unlocked == true, callsModuleLease === lease,
          let keyboard = keyboardAdmissionTicket, let gallery = galleryAdmissionTicket else { throw SessionError.closed }
    let expected = generation
    requests += 1; defer { retireRequest() }
    do { try await callAudioFocus.adopt(emit: audio) }
    catch {
      // A missing foreground media fence must not expose players alongside a
      // live microphone. If the call ended during adoption, normal open may proceed.
      if callCoordinator != nil { await stopCallsForClose(); throw error }
    }
    guard self.core === core, generation == expected, lease.isLive, !closing, !erasing else { throw SessionError.closed }
    let resumed = await MainActor.run {
      ZoenKeyboardPanelHost.resumeAdmissions(expected: keyboard)
        && ZoenAttachmentGalleryHost.resumeAdmissions(expected: gallery)
    }
    guard resumed, self.core === core, generation == expected, lease.isLive, !closing, !erasing else { throw SessionError.closed }
    sessionLocale = locale
    listener.rebind(changed, sourceChanged: { [weak self] spaces in
      Task { await self?.adoptedSourceChanged(spaces, engine: core, expected: expected) }
    }, resumeHistory: { [weak self] in
      Task { await self?.adoptedHistoryReady(engine: core, expected: expected) }
    })
    notificationOnly = false; notificationActive = true
    await bindCalls(lease: lease, changed: call, audio: audio)
    guard self.core === core, generation == expected, lease.isLive, !closing, !erasing else { throw SessionError.closed }
    scheduleInitialHistory()
    return try await core.clientRequest(request: #"{"op":"snapshot"}"#)
  }

  private func adoptedSourceChanged(_ spaces: [String], engine: Client, expected: UInt64) async {
    guard core === engine, generation == expected, !closing, !erasing else { return }
    await validateConversationGalleryChanges(spaces)
    await callCanonicalChanges(spaces)
  }
  private func adoptedHistoryReady(engine: Client, expected: UInt64) async {
    guard core === engine, generation == expected, !closing, !erasing else { return }
    await resumePendingHistory()
  }
}
