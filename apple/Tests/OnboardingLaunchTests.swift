@main struct OnboardingLaunchTests {
    static func main() {
        for arguments in [[], ["Zoen"], ["Zoen", "-AppleLanguages", "(en)"],
                          ["Zoen", "-NSDocumentRevisionsDebugMode", "YES"],
                          ["Zoen", "-DemoUnexpected", "YES"]] {
            precondition(OnboardingLaunch.shouldShow(onboarded: false, forced: false, arguments: arguments))
            precondition(!OnboardingLaunch.shouldShow(onboarded: true, forced: false, arguments: arguments))
        }
        for flag in ["-Demo", "-Story", "-FreshStart", "-Account", "-Open"] {
            let arguments = ["Zoen", flag, "YES"]
            #if DEBUG
            precondition(!OnboardingLaunch.shouldShow(onboarded: false, forced: false, arguments: arguments))
            #else
            precondition(OnboardingLaunch.shouldShow(onboarded: false, forced: false, arguments: arguments))
            #endif
            precondition(OnboardingLaunch.shouldShow(onboarded: true, forced: true, arguments: arguments))
        }
        print("Onboarding launch policy passed: explicit QA flags, unrelated/system arguments, completed onboarding and forced first launch.")
    }
}
