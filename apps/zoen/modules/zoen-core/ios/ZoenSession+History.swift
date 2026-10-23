import Foundation

extension ZoenSession {
  func resumePendingHistory() {
    if historyTask != nil {
      // Coalesce an external wake that raced the last attempt/defer. It must
      // not disappear merely because the previous bounded pump is finishing.
      historyRearmRequested = true
    } else {
      scheduleInitialHistory()
    }
  }

  func scheduleInitialHistory() {
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

  func resumeInitialHistory(engine: Client, expectedGeneration: UInt64, progress: ZoenHistoryListener) async {
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

}
