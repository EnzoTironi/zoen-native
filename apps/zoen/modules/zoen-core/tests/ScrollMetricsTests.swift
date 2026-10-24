import Foundation

@main struct ScrollMetricsTests {
  static func main() {
    var state = ZoenScrollMetricsState()
    let original = [223.66666666666666, 1783, 402, 0, 275.33333587646484]
    let first = state.packet(binding: "owner-a", values: original, onlyChanged: true)!
    precondition(first["sequence"] as? String == "1")
    precondition(state.packet(binding: "owner-a", values: original, onlyChanged: true) == nil)
    // An explicit unchanged-offset read still returns current facts, once.
    precondition(state.packet(binding: "owner-a", values: original, onlyChanged: false)?["sequence"] as? String == "2")
    let epoch = state.epoch
    state.retire()
    precondition(state.epoch != epoch, "queued observations must lose their lease before rebinding")
    precondition(state.packet(binding: "owner-b", values: original, onlyChanged: true)?["sequence"] as? String == "3")
    for invalid in [[Double.nan, 1, 1, 0, 0], [0, 0, 402, 0, 0], [0, 1, 0, 0, 0],
                    [0, 1, 402, -1, 0], [0, 1, 402, 0, -1], [0, 1, 402, 0, Double.infinity], []] {
      precondition(state.packet(binding: "owner-b", values: invalid, onlyChanged: false) == nil)
    }
    precondition(state.packet(binding: "", values: original, onlyChanged: false) == nil)
    var boundary = ZoenScrollMetricsState(sequence: UInt64.max - 1)
    precondition(boundary.packet(binding: "owner", values: original, onlyChanged: false)?["sequence"] as? String == String(UInt64.max))
    precondition(boundary.packet(binding: "owner", values: original, onlyChanged: false) == nil)
    boundary.retire()
    precondition(boundary.packet(binding: "owner2", values: original, onlyChanged: false) == nil)
    print("PASS native scroll metrics ordering, no-op reads, coalescing, retirement and overflow")
  }
}
