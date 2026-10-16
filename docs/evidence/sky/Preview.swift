import SwiftUI

@main
struct SkyPreviewApp: App {
    var body: some Scene {
        WindowGroup {
            ZStack {
                LivingSky()
                VStack(alignment: .leading, spacing: 20) {
                    Text("Zoen · sky study").font(.largeTitle.bold())
                    Text("Local time · \(UserDefaults.standard.double(forKey: "ZoenSkyHour"), specifier: "%.0f"):00")
                    Spacer()
                    Text("A quiet place to think together.").font(.title2)
                    Text("Standalone component preview — not the full Zoen app.").font(.footnote)
                }.padding(28)
            }
        }
    }
}
