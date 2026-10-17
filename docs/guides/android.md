# How to run the Android app

**Goal:** build Zoen for Android with its Rust core and run it on an emulator or phone. **Requires:** JDK 17, Rust 1.99.0, Android SDK 36, build-tools 36.0.0, NDK 27.1.12297006 and `cargo-ndk`. Android 9+ (ARM64 phones, x86_64 emulators).

## 1. Prepare

```sh
cargo install cargo-ndk --locked
rustup target add aarch64-linux-android x86_64-linux-android
export ANDROID_HOME=/path/to/android/sdk
# macOS, if Java 17 is not the default:
export JAVA_HOME="$(/usr/libexec/java_home -v 17)"
```

## 2. Build and run

On a connected device or running emulator, from the repository root:

```sh
scripts/run-android.sh -PandroidAbis=arm64-v8a
```

That runs `./gradlew :app:assembleDebug`, installs the APK and launches `xyz.tironi.zoen/.MainActivity`. The Gradle build compiles the Rust library and generates matching Kotlin bindings first — never edit or commit the generated files.

Or from `android/`:

```sh
./gradlew :app:assembleDebug
./gradlew :app:testDebugUnitTest :app:lintDebug
./gradlew :app:connectedDebugAndroidTest
```

Android Studio works too: open `android/` and run the `app` configuration (Rust and `cargo-ndk` must be on its `PATH`).

## 3. Connect to a relay

- **Local:** run `scripts/dev-stack.sh`; the emulator reaches it at `http://10.0.2.2:8787` (choose it in the profile step's connection settings).
- **Default:** `https://relay.tryzoen.com` (staging). Current clients need protocol 4; check the [relay release notes](../dev/android-relay-release.md) before relying on it.

## Variations

| Want to… | Do this |
| --- | --- |
| Explore with simulated peers | Debug builds show **Explore the demo**, or `adb shell am start -n xyz.tironi.zoen/.MainActivity --ez demo true` |
| Build the core by hand | `scripts/build-android-core.sh --debug --abis=arm64-v8a,x86_64` |
| Release build | `./gradlew :app:assembleRelease -PcoreProfile=release` (bring your own signing) |
| Encrypted two-account journeys | Add `-Pandroid.testInstrumentationRunnerArguments.zoenRelay=http://10.0.2.2:8787` to `connectedDebugAndroidTest` |

Full platform details: [android/README.md](../../android/README.md).
