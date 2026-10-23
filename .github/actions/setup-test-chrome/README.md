# Pinned renderer-test browser

The Expo `client` and `miniapp-resources` jobs install the same official Chrome for
Testing Linux x64 artifact. Version, revision, URL, exact archive length and SHA-256
are checked in `browser.json`; setup never resolves `stable` or selects the runner's
preinstalled browser. There is no additional npm installer or browser dependency.

The installer needs the existing Ubuntu runner's Python, curl, unzip, sudo and
AppArmor tools. It verifies the archive before extracting, installs it in a fresh
root-owned `/usr/lib/zoen-cft` directory, and refuses an existing installation/policy.
The `/`, `/usr` and `/usr/lib` ancestors must already satisfy the same root-owner,
non-symlink and no group/other-write checks; setup never repairs them. The first
Linux candidate rejected `/opt` at that boundary before browser startup.
When AppArmor is enabled, the policy allows user namespaces only for that exact
protected executable. It neither changes the global namespace restriction nor adds
`--no-sandbox` to a browser invocation.

After installation, a separate owned preflight requires the exact command version,
a real DevTools connection, and the same version from `Browser.getVersion`. It uses
a fresh temporary profile, a 10-second startup/protocol budget, and the existing
two-second owned process-group cleanup. Its output contains closed phase/status
fields and bounded process observations, never the endpoint or browser output.
An installation/preflight failure fails the job before the application oracle.
The Linux wrapper also enforces a 20-second wall with two-second signal escalation
so a stuck teardown cannot hold the job indefinitely. A wall/cleanup failure fails
admission; it does not claim that a kernel-blocked process was successfully reaped.

`CHROME_BIN` is exported as an absolute executable path. Both renderer-test launchers
use the shared selector, which refuses missing/relative pins in CI. Local macOS,
Windows and Linux browser discovery is retained. Every real application test still
starts its own fresh profile and retains its existing flags, assertions, startup
and execution deadlines, and cleanup.

This changes acquisition and admission of a reproducible CI browser. The preflight
and version check can warm executable pages, so a subsequent green run does not
establish the cause of the earlier runner I/O wait. Linux CI execution remains the
acceptance boundary; Darwin renderer success is a separate result. Other standalone
web E2E jobs are outside this focused action rollout. Refs #194 and #247.

To update the pin, select a complete release from the official version metadata,
download its version-specific Linux archive, record its exact bytes and SHA-256,
then qualify the setup and real renderer tests on Linux. Keep the application
oracles independent from the preflight; do not fall back after setup failure.

Primary references:

- [Official download guide](https://developer.chrome.com/docs/automation-and-testing/download-test-binaries)
- [Official availability API](https://github.com/GoogleChromeLabs/chrome-for-testing#json-api-endpoints)
- [Chromium's scoped AppArmor namespace policy](https://chromium.googlesource.com/chromium/src/+/main/docs/security/apparmor-userns-restrictions.md)
