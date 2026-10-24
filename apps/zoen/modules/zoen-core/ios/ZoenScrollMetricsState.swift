import Foundation

/// Per-view, in-memory ordering only. UIKit supplies every geometric value.
struct ZoenScrollMetricsState {
  private(set) var epoch: UInt64 = 0
  private var sequence: UInt64
  private var usable = true
  private var last: [Double]?

  init(sequence: UInt64 = 0) { self.sequence = sequence }

  mutating func retire() {
    let (next, overflow) = epoch.addingReportingOverflow(1)
    usable = usable && !overflow
    if !overflow { epoch = next }
    last = nil
  }

  mutating func packet(binding: String, values: [Double], onlyChanged: Bool) -> [String: Any]? {
    guard usable, !binding.isEmpty, values.count == 5,
          values.allSatisfy({ $0.isFinite }), values[1] > 0, values[2] > 0,
          values[3] >= 0, values[4] >= 0, sequence < UInt64.max,
          !onlyChanged || values != last else { return nil }
    sequence += 1
    last = values
    return ["binding": binding, "sequence": String(sequence),
            "geometry": ["contentOffset": ["y": values[0]],
                         "contentSize": ["height": values[1]],
                         "layoutMeasurement": ["height": values[2]],
                         "contentInset": ["top": values[3], "bottom": values[4]]]]
  }
}
