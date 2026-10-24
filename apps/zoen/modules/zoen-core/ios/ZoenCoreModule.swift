import ExpoModulesCore
import Foundation
import UIKit
import PDFKit
#if canImport(FoundationModels)
import FoundationModels
#endif

public class ZoenCoreModule: Module {
  private let sessionOwner = ZoenModuleSessionOwner(session: ZoenSession())
  private var session: ZoenSession { sessionOwner.current.session }

  public func definition() -> ModuleDefinition {
    Name("ZoenCore")
    Events("onChange", "onPageInputStatus", "onCallState", "onCallAudio")
    Constant("sitePreviewVersion") { 2 }
    Constant("pageInputVersion") { 2 }
    zoenSitePreviewDefinition()
    Constant("staticLocationVersion") { 1 }
    zoenLocationMapDefinition()
    zoenLocationSearchDefinition(sessionOwner)
    zoenLocationCancelDefinition()
    zoenLocationReadDefinition(sessionOwner)

    // Reuse the exact pinned Telegram mosaic already used by selected previews.
    // Pure, bounded geometry only: no media bytes, authority or persistent state.
    Function("attachmentMosaicLayout") { (width: Double, height: Double, scale: Double, sizes: [[Double]]) -> [String: Any]? in
      ZoenAttachmentMosaicBridge.layout(width: width, height: height, scale: scale, sizes: sizes)
    }

    AsyncFunction("measureComposer") { (tag: Int, expectedText: String, expectedLabel: String, expectedPlaceholder: String, width: Double, expectedFontSize: Double, nonce: Int) async -> [String: Double]? in
      await MainActor.run {
        guard let root = self.appContext?.findView(withTag: tag, ofType: UIView.self) else { return nil }
        return ZoenComposerSizing.measure(root: root, tag: tag, expectedText: expectedText, expectedLabel: expectedLabel, expectedPlaceholder: expectedPlaceholder, width: width, expectedFontSize: expectedFontSize, nonce: nonce)
      }
    }
    let emitChange: @Sendable ([String: Any]) -> Void = { [weak self] event in self?.sendEvent("onChange", event) }
    let emitCall: @Sendable (String) -> Void = { [weak self] json in self?.sendEvent("onCallState", ["json": json]) }
    let emitAudio: @Sendable (String) -> Void = { [weak self] json in self?.sendEvent("onCallAudio", ["json": json]) }
    AsyncFunction("open") { [sessionOwner, emitChange, emitCall, emitAudio] (mode: String, locale: String) async throws -> String in
      try await ZoenNativePushSessions.open(owner: sessionOwner, mode: mode, locale: locale,
        changed: emitChange, call: emitCall, audio: emitAudio)
    }
    AsyncFunction("startCall") { (spaceId: String, video: Bool) async throws in
      do { try await self.session.startCall(space: spaceId, video: video) }
      catch { throw Exception(name: "CallUnavailable", description: "The call could not start. Check permissions and try again.", code: "ERR_ZOEN_CALL_UNAVAILABLE") }
    }
    AsyncFunction("callAudioReply") { (ticket: String, success: Bool) async in
      await self.session.callAudioReply(ticket: ticket, success: success)
    }
    AsyncFunction("callPresentation") { () async -> String in await self.session.currentCallPresentation() }
    AsyncFunction("showCurrentCall") { (identityId: String, deviceId: String, callId: String) async throws in
      try await self.session.showCurrentCall(identity: identityId, device: deviceId, call: callId)
    }
    AsyncFunction("request") { (request: String) async throws -> String in
      do {
        return try await self.session.request(request)
      } catch CoreError.DraftRejected(let reason) {
        // Only this core variant proves this exact draft revision was never
        // accepted. Storage, transport, stale and session errors stay ambiguous.
        throw Exception(name: "MessageDraftRejected", description: reason, code: "ERR_ZOEN_DRAFT_REJECTED")
      } catch let error as CoreError {
        throw Exception(name: "CoreRequestFailed", description: ZoenCoreErrorMessage.message(error), code: "ERR_ZOEN_REQUEST_FAILED")
      }
    }
    AsyncFunction("pageInputSession") { [sessionOwner, weak self] () async throws -> String in
      try await sessionOwner.current.session.pageInputSession { [weak self] json in
        self?.sendEvent("onPageInputStatus", ["json": json])
      }
    }
    AsyncFunction("loadPageInput") { (ticket: String, itemId: String) async throws -> String in
      try await self.session.loadPageInput(session: ticket, itemId: itemId)
    }
    AsyncFunction("reservePageInput") { (ticket: String, itemId: String) async throws -> String in
      try await self.session.reservePageInput(session: ticket, itemId: itemId)
    }
    AsyncFunction("openPageInput") { (request: String) async throws -> String in
      try await self.session.openPageInput(request)
    }
    AsyncFunction("pageInputStatus") { (ticket: String, id: String) async throws -> String in
      try await self.session.pageInputStatus(session: ticket, id: id)
    }
    AsyncFunction("sealPageInput") { (ticket: String, id: String) async throws -> String in
      try await self.session.sealPageInput(session: ticket, id: id)
    }
    AsyncFunction("applyPageInput") { (ticket: String, id: String) async throws -> String in
      try await self.session.pageInputOperation(session: ticket, id: id, operation: .apply)
    }
    AsyncFunction("discardPageInput") { (ticket: String, id: String) async throws -> String in
      try await self.session.pageInputOperation(session: ticket, id: id, operation: .discard)
    }
    AsyncFunction("closePageInput") { (ticket: String, id: String) async throws in
      try await self.session.closePageInput(session: ticket, id: id)
    }
    View(ZoenPageTextInput.self) {
      Prop("token") { (view: ZoenPageTextInput, token: String) in view.token = token }
      OnViewDidUpdateProps { (view: ZoenPageTextInput) in view.bind() }
    }
    Constant("composerGlassVersion") { 1 }
    Constant("failedMediaRecoveryVersion") { 1 }
    Constant("nativePushPresentationVersion") { 1 }
    Constant("navigationContainerVersion") { 1 }
    Constant("navigationMetricsVersion") { 1 }
    zoenNavigationContainerDefinition()
    View(ZoenComposerGlassSurface.self) {
      ViewName("ComposerGlassSurface")
      Prop("isDark") { (view: ZoenComposerGlassSurface, value: Bool) in view.isDark = value }
      Prop("cornerRadius") { (view: ZoenComposerGlassSurface, value: Double) in view.cornerRadius = value }
      Prop("interactive") { (view: ZoenComposerGlassSurface, value: Bool) in view.interactive = value }
      Prop("preference") { (view: ZoenComposerGlassSurface, value: String) in view.preference = value }
      OnViewDidUpdateProps { (view: ZoenComposerGlassSurface) in view.apply() }
    }
    View(ZoenComposerGlassContainer.self) {
      ViewName("ComposerGlassContainer")
      Prop("isDark") { (view: ZoenComposerGlassContainer, value: Bool) in view.isDark = value }
      OnViewDidUpdateProps { (view: ZoenComposerGlassContainer) in view.apply() }
    }
    // Named presentation-only views; the existing first/default page-input view stays unchanged.
    View(ZoenKeyboardPanelHost.self) {
      ViewName("ZoenKeyboardPanelHost")
      Events("onAction", "onClosed")
      Prop("binding") { (view: ZoenKeyboardPanelHost, value: String) in view.binding = value }
      Prop("model") { (view: ZoenKeyboardPanelHost, value: String) in view.model = value }
      OnViewDidUpdateProps { (view: ZoenKeyboardPanelHost) in view.applyProps() }
    }
    View(ZoenAttachmentGalleryHost.self) {
      ViewName("ZoenAttachmentGalleryHost")
      Events("onAction", "onClosed")
      Prop("binding") { (view: ZoenAttachmentGalleryHost, value: String) in view.binding = value }
      Prop("model") { (view: ZoenAttachmentGalleryHost, value: String) in view.model = value }
      OnViewDidUpdateProps { (view: ZoenAttachmentGalleryHost) in view.applyProps() }
    }
    View(ZoenStandaloneAttachmentPanelHost.self) {
      ViewName("ZoenStandaloneAttachmentPanelHost")
      Events("onAction", "onClosed")
      Prop("binding") { (view: ZoenStandaloneAttachmentPanelHost, value: String) in view.binding = value }
      Prop("model") { (view: ZoenStandaloneAttachmentPanelHost, value: String) in view.model = value }
      OnViewDidUpdateProps { (view: ZoenStandaloneAttachmentPanelHost) in view.applyProps() }
    }
    View(ZoenEmojiKeyboardPanelHost.self) {
      ViewName("ZoenEmojiKeyboardPanelHost")
      Events("onAction", "onClosed")
      Prop("binding") { (view: ZoenEmojiKeyboardPanelHost, value: String) in view.binding = value }
      Prop("model") { (view: ZoenEmojiKeyboardPanelHost, value: String) in view.model = value }
      OnViewDidUpdateProps { (view: ZoenEmojiKeyboardPanelHost) in view.applyProps() }
    }
    View(ZoenMessageBubbleView.self) {
      ViewName("MessageBubble")
      Prop("ownerKey") { (view: ZoenMessageBubbleView, value: String) in view.ownerKey = value }
      Prop("incoming") { (view: ZoenMessageBubbleView, value: Bool) in view.incoming = value }
      Prop("tail") { (view: ZoenMessageBubbleView, value: Bool) in view.tail = value }
      Prop("topLeft") { (view: ZoenMessageBubbleView, value: Double) in view.topLeft = value }
      Prop("topRight") { (view: ZoenMessageBubbleView, value: Double) in view.topRight = value }
      Prop("bottomLeft") { (view: ZoenMessageBubbleView, value: Double) in view.bottomLeft = value }
      Prop("bottomRight") { (view: ZoenMessageBubbleView, value: Double) in view.bottomRight = value }
      Prop("fill") { (view: ZoenMessageBubbleView, value: String) in view.fill = value }
      Prop("fillColors") { (view: ZoenMessageBubbleView, value: [String]) in view.fillColors = value }
      Prop("animateChanges") { (view: ZoenMessageBubbleView, value: Bool) in view.animateChanges = value }
      OnViewDidUpdateProps { (view: ZoenMessageBubbleView) in view.apply() }
    }
    Constant("messageContextVersion") { 2 }
    View(ZoenMessageContextStage.self) {
      ViewName("MessageContextStage")
      Prop("active") { (view: ZoenMessageContextStage, value: Bool) in view.requestedActive = value }
      OnViewDidUpdateProps { (view: ZoenMessageContextStage) in view.bind(active: view.requestedActive) }
      AsyncFunction("dismissContexts") { (view: ZoenMessageContextStage) async in await view.dismissContexts() }
      AsyncFunction("retireContexts") { (view: ZoenMessageContextStage) async in await view.retireContexts() }
    }
    View(ZoenMessageContextSource.self) {
      ViewName("MessageContextSource")
      Events("onGesture")
      Prop("binding") { (view: ZoenMessageContextSource, value: String) in view.requestedBinding = value }
      Prop("active") { (view: ZoenMessageContextSource, value: Bool) in view.requestedActive = value }
      Prop("handlesTap") { (view: ZoenMessageContextSource, value: Bool) in view.requestedHandlesTap = value }
      Prop("reducedMotion") { (view: ZoenMessageContextSource, value: Bool) in view.requestedReducedMotion = value }
      OnViewDidUpdateProps { (view: ZoenMessageContextSource) in
        view.bind(view.requestedBinding, active: view.requestedActive, handlesTap: view.requestedHandlesTap, reducedMotion: view.requestedReducedMotion)
      }
      AsyncFunction("presentContext") { (view: ZoenMessageContextSource, requestId: String, binding: String, gestureId: String, configuration: String) async throws -> String? in
        try await view.present(requestId: requestId, expectedBinding: binding, gestureId: gestureId, configuration: configuration)
      }
      AsyncFunction("consumeGesture") { (view: ZoenMessageContextSource, binding: String, gestureId: String) async -> Bool in
        await view.consumeGesture(expectedBinding: binding, gestureId: gestureId)
      }
      AsyncFunction("dismissContext") { (view: ZoenMessageContextSource, requestId: String) async in await view.dismiss(requestId: requestId) }
    }
    View(ZoenMessageReceiptView.self) {
      ViewName("MessageReceipt")
      Prop("ownerKey") { (view: ZoenMessageReceiptView, value: String?) in view.ownerKey = value ?? "" }
      Prop("shape") { (view: ZoenMessageReceiptView, value: String) in view.shape = value }
      Prop("color") { (view: ZoenMessageReceiptView, value: String) in view.color = value }
      Prop("reducedMotion") { (view: ZoenMessageReceiptView, value: Bool) in view.reducedMotion = value }
      OnViewDidUpdateProps { (view: ZoenMessageReceiptView) in view.apply(propsDelivered: true) }
    }
    AsyncFunction("openAssistant") { (spaceId: String, locale: String) async throws -> String in
      try await self.session.openAssistant(spaceId: spaceId, locale: locale)
    }
    AsyncFunction("askAssistant") { (id: String, question: String) async throws -> String in
      try await self.session.askAssistant(id: id, question: question)
    }
    AsyncFunction("closeAssistant") { (id: String) async in await self.session.closeAssistant(id: id) }
    AsyncFunction("createAccount") { (name: String, handle: String, relay: String) async throws -> String in
      try await self.session.createAccount(name: name, handle: handle, relay: relay)
    }
    AsyncFunction("sendFile") { (request: String) async throws -> String in
      try await self.session.sendFile(request)
    }
    AsyncFunction("sendVoice") { (request: String) async throws -> String in
      try await self.session.sendVoice(request)
    }
    AsyncFunction("loadMediaDraft") { (request: String) async throws -> String in
      try await self.session.loadMediaDraft(request)
    }
    AsyncFunction("saveMediaDraft") { (request: String) async throws -> String in
      try await self.session.saveMediaDraft(request)
    }
    AsyncFunction("materializeMediaDraftAsset") { (request: String) async throws -> String in
      try await self.session.materializeMediaDraftAsset(request)
    }
    AsyncFunction("previewMediaDraftAsset") { (request: String, presentationId: String) async throws in
      let presenter = await MainActor.run { self.appContext?.utilities?.currentViewController() }
      try await self.session.previewMediaDraftAsset(request, presentationId: presentationId, presenter: presenter)
    }
    AsyncFunction("editMediaDraftAsset") { (request: String, presentationId: String) async throws -> String in
      let presenter = await MainActor.run { self.appContext?.utilities?.currentViewController() }
      return try await self.session.editMediaDraftAsset(request, presentationId: presentationId, presenter: presenter)
    }
    AsyncFunction("cancelMediaDraftPreview") { (presentationId: String) async throws in
      try await self.session.cancelMediaDraftPreview(presentationId)
    }
    AsyncFunction("sendMediaDraft") { (request: String) async throws -> String in
      do { return try await self.session.sendMediaDraft(request) }
      catch CoreError.DraftRejected(let reason) {
        throw Exception(name: "MediaDraftRejected", description: reason, code: "ERR_ZOEN_DRAFT_REJECTED")
      }
    }
    AsyncFunction("prepareVoice") { (uri: String) async throws -> String in try await self.session.prepareVoice(uri) }
    AsyncFunction("renderVoice") { (sourceId: String, ranges: String) async throws -> String in try await self.session.renderVoice(sourceId, ranges: ranges) }
    AsyncFunction("transcribeVoice") { (sourceId: String, locale: String) async throws -> String in try await self.session.transcribeVoice(sourceId, locale: locale) }
    AsyncFunction("releaseVoiceRender") { (sourceId: String, uri: String) async throws in try await self.session.releaseVoiceRender(sourceId, uri: uri) }
    AsyncFunction("cancelVoice") { (sourceId: String) async in await self.session.cancelVoice(sourceId) }
    AsyncFunction("releaseVoice") { (sourceId: String) async throws in try await self.session.releaseVoice(sourceId) }
    AsyncFunction("fileInfo") { (itemId: String, version: Int?) async throws -> String in
      try await self.session.fileInfo(itemId: itemId, version: version)
    }
    AsyncFunction("materializeFile") { (itemId: String, version: Int?) async throws -> String in
      try await self.session.materializeFile(itemId: itemId, version: version)
    }
    AsyncFunction("materializeRecentAttachment") { (request: String) async throws -> String in
      try await self.session.materializeRecentAttachment(request)
    }
    AsyncFunction("previewRecentAttachment") { (request: String, presentationId: String) async throws in
      let presenter = await MainActor.run { self.appContext?.utilities?.currentViewController() }
      try await self.session.previewRecentAttachment(request, presentationId: presentationId, presenter: presenter)
    }
    zoenContactReadDefinition(sessionOwner)
    AsyncFunction("previewLocalFile") { (uri: String) async throws in
      let presenter = await MainActor.run { self.appContext?.utilities?.currentViewController() }
      try await self.session.previewLocalFile(uri, presenter: presenter)
    }
    AsyncFunction("previewConversationMedia") { (request: String, presentationId: String) async throws in
      let presenter = await MainActor.run { self.appContext?.utilities?.currentViewController() }
      try await ZoenConversationGalleryCompletion.awaitDismissal {
        try await self.session.previewConversationMedia(request, presentationId: presentationId, presenter: presenter)
      }
    }
    AsyncFunction("cancelConversationMedia") { (presentationId: String) async throws in
      try await self.session.cancelMediaDraftPreview(presentationId)
    }
    AsyncFunction("releaseLocalFile") { (uri: String) async in await self.session.releaseLocalFile(uri) }
    AsyncFunction("releaseMediaLibraryExport") { (uri: String) async throws in
      try ZoenDeviceErasure.releaseMediaLibraryExport(uri)
    }
    AsyncFunction("saveProfile") { (request: String) async throws in try await self.session.saveProfile(request) }
    AsyncFunction("materializeProfilePhoto") { (ownerId: String, deviceId: String, identityId: String, expectedSha256: String) async throws -> String in
      try await self.session.materializeProfilePhoto(ownerId: ownerId, deviceId: deviceId, identityId: identityId, expectedSha256: expectedSha256)
    }
    AsyncFunction("releaseProfilePhoto") { (uri: String) async in await self.session.releaseProfilePhoto(uri) }
    AsyncFunction("notificationWake") { [sessionOwner] (identityId: String, deviceId: String) async in
      let owner = sessionOwner.current
      await ZoenNativePushSessions.wake(session: owner.session, lease: owner.lease, identityID: identityId, deviceID: deviceId)
    }
    AsyncFunction("eraseDevice") { [sessionOwner] (identityId: String, deviceId: String) async throws in
      let owner = sessionOwner.current
      try await ZoenNativePushSessions.gate.finishOwned(session: owner.session, lease: owner.lease) {
        try await owner.session.eraseDevice(expected: ZoenDeviceScope(identityId: identityId, deviceId: deviceId))
      }
    }
    AsyncFunction("close") { () async throws in
      let session = self.session
      try await ZoenNativePushSessions.gate.close(session: session) { try await session.close() }
    }
    AsyncFunction("closeForce") { () async throws in
      let session = self.session
      try await ZoenNativePushSessions.gate.close(session: session) { try await session.close(discardEdits: true) }
    }
    OnAppBecomesActive { [sessionOwner] in
      let session = sessionOwner.current.session
      session.activateDraftMediaEdits()
      Task { await session.setNotificationForeground(true); await session.resumePendingHistory() }
    }
    OnAppEntersBackground { [sessionOwner] in
      let session = sessionOwner.current.session
      session.retireDraftMediaEdits()
      Task { await session.setNotificationForeground(false) }
    }
    OnDestroy { [sessionOwner] in
      let session = sessionOwner.retire().session
      session.retireDraftMediaEdits()
      session.retireForModuleDestruction()
      Task {
        try? await ZoenNativePushSessions.gate.close(session: session) { try await session.close(discardEdits: true) }
      }
    }
  }
}
