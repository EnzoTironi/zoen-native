import { constants } from "node:fs";
import { access, stat } from "node:fs/promises";
import { posix, win32 } from "node:path";

async function isExecutableFile(path: string) {
  try {
    if (!(await stat(path)).isFile()) return false;
    await access(path, constants.X_OK);
    return true;
  } catch {
    return false;
  }
}

export async function findBrowserExecutable({
  env = process.env,
  platform = process.platform,
  canExecute = isExecutableFile,
}: {
  env?: Record<string, string | undefined>;
  platform?: string;
  canExecute?: (path: string) => Promise<boolean>;
} = {}): Promise<string | null> {
  const paths = platform === "win32" ? win32 : posix;
  const directories = (env.PATH ?? "").split(platform === "win32" ? ";" : ":").filter(Boolean);
  const fromPath = async (names: string[]) => {
    for (const name of names) {
      for (const directory of directories) {
        const candidate = paths.resolve(directory, name);
        if (await canExecute(candidate)) return candidate;
      }
    }
    return null;
  };
  const explicit = env.CHROME_BIN?.trim();
  const required = env.GITHUB_ACTIONS === "true" || Boolean(env.CI && !/^(false|0)$/i.test(env.CI));
  if (required && (!explicit || !paths.isAbsolute(explicit))) {
    throw new Error(
      "CI renderer tests require an absolute CHROME_BIN from the pinned browser setup.",
    );
  }
  if (explicit) {
    const candidate = /[/\\]/.test(explicit) ? paths.resolve(explicit) : await fromPath([explicit]);
    if (candidate && (await canExecute(candidate))) return candidate;
    throw new Error(`CHROME_BIN is not an executable browser file: ${explicit}`);
  }
  const known =
    platform === "darwin"
      ? [
          "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
          "/Applications/Chromium.app/Contents/MacOS/Chromium",
          ...(env.HOME
            ? [paths.join(env.HOME, "Applications/Google Chrome.app/Contents/MacOS/Google Chrome")]
            : []),
        ]
      : platform === "win32"
        ? [env.PROGRAMFILES, env["PROGRAMFILES(X86)"], env.LOCALAPPDATA]
            .filter((directory): directory is string => Boolean(directory))
            .map((directory) => paths.join(directory, "Google/Chrome/Application/chrome.exe"))
        : [
            "/usr/bin/google-chrome",
            "/usr/bin/google-chrome-stable",
            "/usr/bin/chromium",
            "/usr/bin/chromium-browser",
            "/opt/google/chrome/chrome",
          ];
  for (const candidate of known) if (await canExecute(candidate)) return candidate;
  const candidate = await fromPath(
    platform === "win32"
      ? ["chrome.exe", "chromium.exe"]
      : ["google-chrome", "google-chrome-stable", "chromium", "chromium-browser"],
  );
  if (candidate) return candidate;
  if (required) {
    throw new Error(
      "Chrome/Chromium is required for CI renderer tests. Install its normal sandbox runtime or set CHROME_BIN to an executable browser.",
    );
  }
  return null;
}
