import Foundation
#if os(iOS) && DEBUG
import QuartzCore
import UIKit

/// Debug-only frame-pacing probe (launch with `-RodaFramePacing YES`). Between `begin` and
/// `end` it records every display-link interval and every main-thread busy stretch (run
/// loop wake → sleep), and logs one line, e.g.
/// `ZPACE card-drag frames=212 target=8.3ms mean=8.4ms p95=9.1ms max=16.9ms hitches=1
///  busy p95=2.1ms max=6.0ms over8=0 over16=0`.
/// A hitch is a frame that took longer than 1.5x the display's nominal interval. A headless
/// simulator paces its display slowly, so there the busy stretches are the honest number:
/// a commit that fits in 8.3 ms makes a 120 Hz frame on a device.
@MainActor
final class FramePacingProbe: NSObject {
    static let shared = FramePacingProbe()
    static var enabled: Bool { UserDefaults.standard.bool(forKey: "RodaFramePacing") }

    private var link: CADisplayLink?
    private var last: CFTimeInterval = 0
    private var intervals: [Double] = []
    private var target: Double = 1.0 / 60
    private var label = ""
    private var observer: CFRunLoopObserver?
    private var wokeAt: CFTimeInterval = 0
    private var busy: [Double] = []

    func begin(_ label: String) {
        guard Self.enabled, link == nil else { return }
        self.label = label
        intervals.removeAll(keepingCapacity: true)
        last = 0
        let l = CADisplayLink(target: self, selector: #selector(tick(_:)))
        l.preferredFrameRateRange = CAFrameRateRange(minimum: 60, maximum: 120, preferred: 120)
        l.add(to: .main, forMode: .common)
        link = l
        busy.removeAll(keepingCapacity: true)
        let obs = CFRunLoopObserverCreateWithHandler(nil, CFRunLoopActivity.afterWaiting.rawValue | CFRunLoopActivity.beforeWaiting.rawValue, true, 0) { [weak self] _, act in
            MainActor.assumeIsolated {
                guard let self else { return }
                let now = CACurrentMediaTime()
                if act == .afterWaiting { self.wokeAt = now }
                else if self.wokeAt > 0 { self.busy.append(now - self.wokeAt); self.wokeAt = 0 }
            }
        }
        CFRunLoopAddObserver(CFRunLoopGetMain(), obs, .commonModes)
        observer = obs
    }

    @objc private func tick(_ l: CADisplayLink) {
        target = l.duration > 0 ? l.duration : target
        if last > 0 { intervals.append(l.timestamp - last) }
        last = l.timestamp
    }

    func end() {
        guard let l = link else { return }
        l.invalidate(); link = nil
        if let o = observer { CFRunLoopRemoveObserver(CFRunLoopGetMain(), o, .commonModes); observer = nil }
        guard intervals.count > 4 else { return }
        let b = busy.map { $0 * 1000 }.sorted()
        let bp95 = b.isEmpty ? 0 : b[min(b.count - 1, Int(Double(b.count) * 0.95))]
        let ms = intervals.map { $0 * 1000 }.sorted()
        let mean = ms.reduce(0, +) / Double(ms.count)
        let p95 = ms[min(ms.count - 1, Int(Double(ms.count) * 0.95))]
        let hitches = ms.filter { $0 > target * 1000 * 1.5 }.count
        let line = String(format: "ZPACE %@ frames=%d target=%.1fms mean=%.1fms p95=%.1fms max=%.1fms hitches=%d busy n=%d p95=%.1fms max=%.1fms over8=%d over16=%d",
                          label, ms.count, target * 1000, mean, p95, ms.last ?? 0, hitches,
                          b.count, bp95, b.last ?? 0, b.filter { $0 > 8.3 }.count, b.filter { $0 > 16.7 }.count)
        NSLog("%@", line)
        // Also kept on disk so a UI test can read it back without log streaming.
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("zpace.log")
        if let h = try? FileHandle(forWritingTo: url) { h.seekToEndOfFile(); h.write(Data((line + "\n").utf8)); try? h.close() }
        else { try? Data((line + "\n").utf8).write(to: url) }
    }
}
#endif
