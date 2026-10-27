import Foundation

/// Assemble the replacement in the existing journal; no new selection authority.
enum ZoenMediaDraftEditedContent {
  enum Failure: Error { case changed }

  static func replacingAsset(in original: [String: Any], assetId: String, mutationId: String,
                             replacementAssetId: String, replacementMutationId: String) throws -> [String: Any] {
    guard original["kind"] as? String == "attachments",
          var items = original["items"] as? [[String: Any]],
          let index = items.firstIndex(where: {
            $0["asset_id"] as? String == assetId && $0["mutation_id"] as? String == mutationId
          }),
          items.filter({ $0["mutation_id"] as? String == mutationId }).count == 1,
          replacementAssetId != assetId, replacementMutationId != mutationId,
          !items.contains(where: {
            $0["asset_id"] as? String == replacementAssetId || $0["mutation_id"] as? String == replacementMutationId
          }) else { throw Failure.changed }
    var content = original
    if let rawGallery = original["gallery"] {
      guard var gallery = rawGallery as? [String: Any],
            var selections = gallery["entries"] as? [[String: Any]],
            !selections.isEmpty, selections.count <= items.count else { throw Failure.changed }
      let first = items.count - selections.count
      // The journal owns this ordered suffix. Rebind only the selected item's
      // matching entry, never another photo that happens to share source bytes.
      for offset in selections.indices {
        guard selections[offset]["asset_id"] as? String == items[first + offset]["asset_id"] as? String else {
          throw Failure.changed
        }
      }
      if index >= first { selections[index - first]["asset_id"] = replacementAssetId }
      gallery["entries"] = selections
      content["gallery"] = gallery
    }
    items[index]["asset_id"] = replacementAssetId
    items[index]["mutation_id"] = replacementMutationId
    content["items"] = items
    return content
  }
}
