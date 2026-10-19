/// Only explicit development entry points may bypass first-launch onboarding.
enum OnboardingLaunch {
    static func shouldShow(onboarded: Bool, forced: Bool, arguments: [String]) -> Bool {
        if forced { return true }
        #if DEBUG
        let qaFlags: Set<String> = [
            "-Demo", "-ResetDemo", "-Showcase", "-Story", "-FreshStart", "-Account",
            "-Open", "-Tab", "-Settings", "-MascotGallery", "-IconSheet",
            "-TopBarWidths", "-IconExplore", "-BackgroundEditor",
        ]
        if !qaFlags.isDisjoint(with: arguments) { return false }
        #endif
        return !onboarded
    }
}
