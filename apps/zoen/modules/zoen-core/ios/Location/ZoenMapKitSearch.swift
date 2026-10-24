import Foundation
import MapKit
#if canImport(UIKit)
import UIKit
#endif

@MainActor final class ZoenMapKitSearchOperation: ZoenLocationSearchOperation {
  private let search: MKLocalSearch
  init(query: String) {
    let request = MKLocalSearch.Request()
    request.naturalLanguageQuery = query
    request.resultTypes = [.address, .pointOfInterest]
    search = MKLocalSearch(request: request)
  }
  func start() async throws -> [ZoenStaticLocation.Point] {
    let response = try await search.start()
    return try response.mapItems.prefix(20).map { item in
      let coordinate = item.location.coordinate
      return try ZoenStaticLocation.Point(longitude: coordinate.longitude,
        latitude: coordinate.latitude, label: item.name).validated()
    }
  }
  func cancel() { search.cancel() }
}

@MainActor enum ZoenLocationServices {
  static let search = ZoenLocationSearch { ZoenMapKitSearchOperation(query: $0) }
  static func perform(_ request: String) async throws -> String {
    #if canImport(UIKit)
    observeBackground()
    guard UIApplication.shared.applicationState == .active else { throw ZoenStaticLocation.Failure.retired }
    #endif
    return try await search.search(request)
  }
  #if canImport(UIKit)
  private static var background: NSObjectProtocol?
  static func observeBackground() {
    guard background == nil else { return }
    background = NotificationCenter.default.addObserver(forName: UIApplication.didEnterBackgroundNotification,
      object: nil, queue: .main) { _ in
      MainActor.assumeIsolated { search.retire() }
    }
  }
  #endif
}
