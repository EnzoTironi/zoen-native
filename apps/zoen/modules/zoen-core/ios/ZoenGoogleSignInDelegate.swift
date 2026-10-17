import ExpoModulesCore
import GoogleSignIn
import UIKit

/// Expo SDK 58 forwards both warm and cold scene URL events to these subscribers.
/// Returning false preserves Expo Router and every other registered link handler.
public final class ZoenGoogleSignInDelegate: ExpoAppDelegateSubscriber {
  public func application(
    _ application: UIApplication,
    open url: URL,
    options: [UIApplication.OpenURLOptionsKey: Any] = [:]
  ) -> Bool {
    GIDSignIn.sharedInstance.handle(url)
  }
}
