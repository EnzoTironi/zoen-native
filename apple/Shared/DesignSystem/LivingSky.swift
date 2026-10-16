import SwiftUI

/// Local civil time, deliberately independent of location/network permissions.
enum SkyClock {
    static let hours: [Double] = [0, 6, 9, 12, 17, 20, 24]
    static let colors: [[Double]] = [
        [0.20, 0.23, 0.46], [0.84, 0.48, 0.53], [0.38, 0.67, 0.87],
        [0.27, 0.62, 0.90], [0.94, 0.61, 0.30], [0.51, 0.34, 0.63], [0.20, 0.23, 0.46]
    ]
    static func sample(_ hour: Double) -> (rgb: [Double], stars: Double) {
        let h = hour.isFinite ? (hour.truncatingRemainder(dividingBy: 24) + 24).truncatingRemainder(dividingBy: 24) : 12
        let i = (0..<hours.count - 1).first { h < hours[$0 + 1] } ?? 0
        let t = (h - hours[i]) / (hours[i + 1] - hours[i])
        let s = t * t * (3 - 2 * t)
        let night = [1.0, 0, 0, 0, 0, 0.4, 1]
        return ((0..<3).map { colors[i][$0] + (colors[i + 1][$0] - colors[i][$0]) * s }, night[i] + (night[i + 1] - night[i]) * s)
    }
}

/// A quiet ink-and-wash sky. No display-link, textures, location requests or glass changes.
struct LivingSky: View {
    @Environment(\.colorScheme) private var scheme
    @Environment(\.scenePhase) private var phase
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    var previewHour: Double? = nil

    private var override: Double? {
        #if DEBUG
        if let previewHour { return previewHour }
        guard let value = UserDefaults.standard.object(forKey: "ZoenSkyHour") else { return nil }
        if let number = value as? NSNumber { return number.doubleValue }
        return (value as? String).flatMap(Double.init)
        #else
        return nil
        #endif
    }

    var body: some View {
        if phase == .active {
        TimelineView(.periodic(from: .now, by: reduceMotion ? 60 : 1)) { timeline in
            let date = phase == .active ? timeline.date : Date.distantPast
            let parts = Calendar.autoupdatingCurrent.dateComponents([.hour, .minute, .second], from: date)
            let hour = override ?? Double(parts.hour ?? 12) + Double(parts.minute ?? 0) / 60 + Double(parts.second ?? 0) / 3600
            let sky = SkyClock.sample(hour)
            let dark = scheme == .dark
            let base = dark ? [0.035, 0.05, 0.08] : [0.96, 0.97, 0.985]
            let wash = 0.08
            let rgb = (0..<3).map { base[$0] * (1 - wash) + sky.rgb[$0] * wash }
            let ink = Color(red: rgb[0], green: rgb[1], blue: rgb[2])
            ZStack {
                LinearGradient(colors: [ink, Color(red: base[0], green: base[1], blue: base[2])], startPoint: .top, endPoint: .bottom)
                Canvas { context, size in
                    let time = reduceMotion || override != nil ? 0 : date.timeIntervalSince1970
                    for i in 0..<5 {
                        let drift = sin(time / 90 + Double(i)) * 9
                        let x = size.width * CGFloat(i) / 4 + drift
                        let y = size.height * (0.10 + Double(i % 3) * 0.12)
                        var cloud = Path()
                        cloud.move(to: CGPoint(x: x - 65, y: y))
                        cloud.addCurve(to: CGPoint(x: x + 70, y: y + 3), control1: CGPoint(x: x - 30, y: y - 24), control2: CGPoint(x: x + 20, y: y + 16))
                        context.stroke(cloud, with: .color(.white.opacity(dark ? 0.025 : 0.22)), style: StrokeStyle(lineWidth: 13, lineCap: .round))
                    }
                    for i in 0..<36 {
                        let x = size.width * Double((i * 73 + 19) % 997) / 997
                        let y = size.height * Double((i * 137 + 31) % 631) / 1000
                        var star = Path()
                        star.move(to: CGPoint(x: x - 2, y: y)); star.addLine(to: CGPoint(x: x + 2, y: y + 0.5))
                        star.move(to: CGPoint(x: x, y: y - 2)); star.addLine(to: CGPoint(x: x + 0.5, y: y + 2))
                        context.stroke(star, with: .color((dark ? Color.white : Color(red: 0.32, green: 0.37, blue: 0.5)).opacity(sky.stars * (dark ? 0.08 : 0.025))), lineWidth: 0.8)
                    }
                }
            }
        }
        .allowsHitTesting(false)
        .accessibilityHidden(true)
        .ignoresSafeArea()
        } else {
            Color.clear.accessibilityHidden(true)
        }
    }
}
