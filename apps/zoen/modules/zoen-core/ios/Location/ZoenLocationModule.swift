import ExpoModulesCore
import Foundation

func zoenLocationSearchDefinition(_ owner: ZoenModuleSessionOwner<ZoenSession>) -> AnyDefinition {
  AsyncFunction("searchLocations") { (request: String) async throws -> String in
    let original = owner.current
    guard original.lease.isLive else { throw SessionError.closed }
    let scope = try await original.session.locationSearchScope()
    let result = try await ZoenLocationServices.perform(request)
    guard original.lease.isLive, owner.current.lease === original.lease else { throw SessionError.closed }
    try await original.session.requireLocationSearchScope(scope)
    return result
  }
}
func zoenLocationCancelDefinition() -> AnyDefinition {
  AsyncFunction("cancelLocationSearch") { (binding: String) async in
    await ZoenLocationServices.search.cancel(binding: binding)
  }
}
func zoenLocationReadDefinition(_ owner: ZoenModuleSessionOwner<ZoenSession>) -> AnyDefinition {
  AsyncFunction("readLocationFile") { (request: String) async throws -> String in
    try await owner.current.session.readLocationFile(request)
  }
}
func zoenContactReadDefinition(_ owner: ZoenModuleSessionOwner<ZoenSession>) -> AnyDefinition {
  AsyncFunction("readContactFile") { (request: String) async throws -> String in
    try await owner.current.session.readContactFile(request)
  }
}
