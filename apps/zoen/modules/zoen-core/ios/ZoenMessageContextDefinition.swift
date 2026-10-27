import ExpoModulesCore

func zoenMessageContextSourceDefinition() -> ViewDefinition<ZoenMessageContextSource> {
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
      AsyncFunction("finishTextSelection") { (view: ZoenMessageContextSource, requestId: String, resultJSON: String?) async throws in
        try await view.finishTextSelection(requestId: requestId, resultJSON: resultJSON)
      }
    }
}
