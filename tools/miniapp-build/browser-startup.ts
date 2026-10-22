import { constants } from "node:fs";
import { open, readlink } from "node:fs/promises";
import { basename, join } from "node:path";

type OwnedChild = { pid?: number; exitCode: number | null; signalCode: string | null };
type DiagnosticFiles = {
  platform: string;
  read(path: string, limit: number): Promise<string>;
  link(path: string): Promise<string>;
};

// Only the fresh test's profile and its directly spawned PID are inspected.
// Never follow a substituted profile file or collect environment/command lines.
export async function boundedDiagnosticFile(path: string, limit: number): Promise<string> {
  const file = await open(path, constants.O_RDONLY | constants.O_NOFOLLOW | constants.O_NONBLOCK);
  try {
    if (!(await file.stat()).isFile()) throw Error("Not a diagnostic file");
    const bytes = Buffer.alloc(limit + 1);
    const { bytesRead } = await file.read(bytes, 0, bytes.length, 0);
    if (bytesRead > limit) throw Error("Diagnostic file exceeds bound");
    return bytes.subarray(0, bytesRead).toString("utf8");
  } finally {
    await file.close();
  }
}

const files: DiagnosticFiles = {
  platform: process.platform,
  read: boundedDiagnosticFile,
  link: readlink,
};

/** Best-effort, failure-only observation; cannot extend admission or connect to a browser. */
export async function inspectOwnedBrowserStartup(
  child: OwnedChild,
  profile: string,
  system: DiagnosticFiles = files,
) {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const inspect = async () => {
    const activePort = await system.read(join(profile, "DevToolsActivePort"), 128).then(
      (value) => {
        const match = value.match(
          /^([1-9][0-9]{0,4})\n\/devtools\/browser\/[a-zA-Z0-9-]{1,80}\n?$/,
        );
        return match && Number(match[1]) <= 65_535 ? "valid" : "invalid";
      },
      (error: unknown) =>
        (error as NodeJS.ErrnoException).code === "ENOENT" ? "absent" : "unavailable",
    );
    if (system.platform !== "linux") return { activePort, process: "unsupported" };
    if (!child.pid || child.exitCode !== null || child.signalCode !== null)
      return { activePort, process: "exited" };
    const root = `/proc/${child.pid}`;
    const observed = await Promise.all([
      system.read(`${root}/stat`, 4096),
      system.read(`${root}/wchan`, 128),
      system.link(`${root}/exe`),
    ]).catch(() => null);
    if (!observed || child.exitCode !== null || child.signalCode !== null)
      return { activePort, process: "unavailable" };
    const [stat, wait, executable] = observed;
    // comm may contain spaces or parentheses. Only fixed stat fields are retained.
    if (!stat.startsWith(`${child.pid} (`) || stat.lastIndexOf(") ") < 0)
      return { activePort, process: "unavailable" };
    const fields = stat
      .slice(stat.lastIndexOf(") ") + 2)
      .trim()
      .split(/\s+/);
    const ticks = (value?: string) =>
      value && /^\d{1,15}$/.test(value) && Number.isSafeInteger(Number(value))
        ? Number(value)
        : null;
    const name = basename(executable);
    return {
      activePort,
      process: "observed",
      executable: /^(?:chrome|chromium|google-chrome(?:-stable)?|bash|sh|node)$/.test(name)
        ? name
        : "other",
      state: /^[RSDZTWtXxKWPIN]$/.test(fields[0] ?? "") ? fields[0] : null,
      waitChannel: /^[a-zA-Z0-9_]{1,80}$/.test(wait.trim()) ? wait.trim() : null,
      userCpuTicks: ticks(fields[11]),
      systemCpuTicks: ticks(fields[12]),
    };
  };
  try {
    return await Promise.race([
      inspect(),
      new Promise<{ process: string }>((resolve) => {
        timer = setTimeout(() => resolve({ process: "diagnostic-timeout" }), 200);
      }),
    ]);
  } catch {
    return { process: "unavailable" };
  } finally {
    clearTimeout(timer);
  }
}

export async function annotateOwnedBrowserFailure(
  error: unknown,
  child: OwnedChild,
  profile: string,
  system: DiagnosticFiles = files,
) {
  if (!(error instanceof Error)) return;
  const observed = await inspectOwnedBrowserStartup(child, profile, system);
  try {
    Object.assign(error, { ownedBrowserStartup: observed });
  } catch {
    // Diagnostics cannot replace the primary failure, even for a frozen Error.
  }
}
