import Foundation

// Exercise the same assembly helper called by native Save, without UIKit/FFI ports.
@main struct MediaDraftEditedContentTests {
  static func main() throws {
    let data = FileHandle.standardInput.readDataToEndOfFile()
    let content = try JSONSerialization.jsonObject(with: data) as! [String: Any]
    let args = CommandLine.arguments
    do {
      let result = try ZoenMediaDraftEditedContent.replacingAsset(in: content,
        assetId: args[1], mutationId: args[2], replacementAssetId: args[3], replacementMutationId: args[4])
      let output = try JSONSerialization.data(withJSONObject: result, options: [.sortedKeys])
      FileHandle.standardOutput.write(output)
    } catch {
      FileHandle.standardOutput.write(Data("null".utf8))
    }
  }
}
