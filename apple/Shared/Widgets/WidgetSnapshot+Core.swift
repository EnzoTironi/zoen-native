import Foundation
import RodaCore

/// Snapshots a mini-app published itself (`zoen.widget.setSnapshot`), validated against the
/// same strict schema. They win over the core's default for this session only; the core's
/// snapshot comes back on relaunch.
@MainActor
enum WidgetOverrides {
    static var byItem: [String: String] = [:]
}

extension WidgetSnapshot {
    /// The live snapshot of a mini-app Item (`nil` = no widget).
    @MainActor static func from(_ item: ItemDetail) -> WidgetSnapshot? {
        guard let app = item.app else { return nil }
        if let o = WidgetOverrides.byItem[item.id], let s = decode(o) { return s }
        return decode(app.snapshotJson)
    }
}
