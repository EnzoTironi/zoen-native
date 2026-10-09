import SwiftUI
#if os(iOS)
import UIKit
#elseif os(macOS)
import AppKit
#endif

/// Hápticos do menu radial. No iPhone usa os geradores do UIKit (preparados antes
/// do gesto para não atrasar o primeiro toque). No Mac, o Force Touch do trackpad
/// via NSHapticFeedbackManager — só sente quem está com o dedo no trackpad.
@MainActor
enum Haptics {
    #if os(iOS)
    private static let soft = UIImpactFeedbackGenerator(style: .soft)
    private static let light = UIImpactFeedbackGenerator(style: .light)
    private static let rigid = UIImpactFeedbackGenerator(style: .rigid)
    private static let medium = UIImpactFeedbackGenerator(style: .medium)
    private static let tick = UISelectionFeedbackGenerator()
    private static let notify = UINotificationFeedbackGenerator()
    #endif

    static func prepare() {
        #if os(iOS)
        soft.prepare(); tick.prepare(); medium.prepare(); notify.prepare()
        #endif
    }

    /// A tile lifts under the finger (edit mode).
    static func pickUp() {
        #if os(iOS)
        rigid.impactOccurred(intensity: 0.75)
        #elseif os(macOS)
        NSHapticFeedbackManager.defaultPerformer.perform(.generic, performanceTime: .now)
        #endif
    }

    /// A tile settles into its new place.
    static func drop() {
        #if os(iOS)
        soft.impactOccurred(intensity: 0.9)
        #elseif os(macOS)
        NSHapticFeedbackManager.defaultPerformer.perform(.alignment, performanceTime: .now)
        #endif
    }

    /// Something was removed (after you confirmed it).
    static func remove() {
        #if os(iOS)
        medium.impactOccurred(intensity: 0.85)
        #elseif os(macOS)
        NSHapticFeedbackManager.defaultPerformer.perform(.levelChange, performanceTime: .now)
        #endif
    }

    /// Menu opened.
    static func open() {
        #if os(iOS)
        medium.impactOccurred()
        #elseif os(macOS)
        NSHapticFeedbackManager.defaultPerformer.perform(.levelChange, performanceTime: .now)
        #endif
    }

    /// O dedo cruzou para outro item.
    static func selectionTick() {
        #if os(iOS)
        tick.selectionChanged()
        tick.prepare()
        #elseif os(macOS)
        NSHapticFeedbackManager.defaultPerformer.perform(.alignment, performanceTime: .now)
        #endif
    }

    /// Soltou em cima de um item.
    static func commit() {
        #if os(iOS)
        notify.notificationOccurred(.success)
        #elseif os(macOS)
        NSHapticFeedbackManager.defaultPerformer.perform(.levelChange, performanceTime: .now)
        #endif
    }

    /// Algo precisa da sua confirmação.
    static func warning() {
        #if os(iOS)
        notify.notificationOccurred(.warning)
        #elseif os(macOS)
        NSHapticFeedbackManager.defaultPerformer.perform(.generic, performanceTime: .now)
        #endif
    }

    /// Toque leve de ação (cada toque num mini-app).
    static func action() {
        #if os(iOS)
        rigid.impactOccurred(intensity: 0.7)
        #elseif os(macOS)
        NSHapticFeedbackManager.defaultPerformer.perform(.alignment, performanceTime: .now)
        #endif
    }

    /// Closed without choosing.
    static func dismiss() {
        #if os(iOS)
        soft.impactOccurred(intensity: 0.7)
        #endif
    }

    /// Light tap (the Search button).
    static func tap() {
        #if os(iOS)
        light.impactOccurred(intensity: 0.8)
        #endif
    }

    /// Sending a message (light impact — never on scroll).
    static func send() {
        #if os(iOS)
        light.impactOccurred(intensity: 0.55)
        #endif
    }

    /// Voice-record lock (rigid).
    static func recordLock() {
        #if os(iOS)
        rigid.impactOccurred(intensity: 1.0)
        #endif
    }

    /// Soft tick for pull-to-refresh or a gentle settle.
    static func settle() {
        #if os(iOS)
        soft.impactOccurred(intensity: 0.5)
        #endif
    }
}
