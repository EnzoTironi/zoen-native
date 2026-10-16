# How to build the core and run the Apple apps

**Goal:** run Zoen on the iOS simulator and the Mac from source. **Requires:** a Mac with Xcode (iOS 26 / macOS 26 SDKs), [XcodeGen](https://github.com/yonaskolb/XcodeGen) and Rust 1.99.0.

## 1. Install the Rust targets

```sh
rustup toolchain install 1.99.0 --profile minimal --component rustfmt,clippy
export RUSTUP_TOOLCHAIN=1.99.0
rustup target add aarch64-apple-ios aarch64-apple-ios-sim aarch64-apple-darwin
```

## 2. Start a local relay (optional but recommended)

```sh
scripts/dev-stack.sh    # separate terminal; debug builds connect to 127.0.0.1:8787
```

## 3. Build and run

```sh
scripts/run.sh            # builds the core if missing, generates the Xcode project, opens iOS + Mac
scripts/run.sh ios        # simulator only
scripts/run.sh mac        # Mac only
SIM="iPhone 17 Pro" scripts/run.sh ios
```

After changing any shared-core API, rebuild the XCFramework and its Swift bindings together:

```sh
scripts/build-core.sh            # always
scripts/build-core.sh --if-stale # only when a core source is newer than the package
```

To work in Xcode directly: `cd apple && xcodegen generate`, then open the generated project and pick an iOS simulator or the Mac scheme.

## Variations

| Want to… | Do this |
| --- | --- |
| See edits on screen within seconds | `nohup scripts/watch.sh > build/watch.log 2>&1 &` (stop with `scripts/watch.sh stop`) |
| Install on your own iPhone with a free Apple ID | `scripts/device.sh`, then `ZOEN_TEAM=ABCDE12345 scripts/device.sh install` — see [installing on iPhone](../dev/instalar-no-iphone.md) |
| Run the iPhone app in a window on an Apple Silicon Mac | `scripts/mac-iphone.sh` — see [iPhone on Mac](../dev/iphone-no-mac.md) |
| Point a device build at your Mac's relay | `ZOEN_RELAY=http://YOUR-MAC.local:8787 scripts/device.sh install` |
| Run the recorded simulator journey (relay + CLI person + XCUITest) | `scripts/journey-sim.sh` |
| Check glass styling like CI | `scripts/check-apple-glass.sh` |

## Notes

- Use a dedicated simulator, database and Keychain service for recovery tests.
- Demo UI recordings use explicit development flags; they don't prove real-account encryption or transport.
- XCUITest needs a supported simulator or a physical iPhone.
