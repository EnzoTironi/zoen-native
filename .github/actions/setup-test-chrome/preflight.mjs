import { spawn } from "node:child_process";
import { mkdtemp } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import { findBrowserExecutable } from "../../../tools/miniapp-build/browser-executable.ts";
import { annotateOwnedBrowserFailure } from "../../../tools/miniapp-build/browser-startup.ts";
import { chromeEndpoint } from "../../../tools/miniapp-build/browser-endpoint.ts";
import {
  observeOwnedChild,
  withBrowserProfile,
  withCleanup,
} from "../../../miniapps/excalidraw-ui/test/browser-lifecycle.mjs";

class PreflightFailure extends Error {
  constructor(phase) {
    super("Pinned browser preflight failed");
    this.phase = phase;
  }
}

export function commandVersionMatches(output, version) {
  return (
    /^\d+\.\d+\.\d+\.\d+$/.test(version) && output.trim() === `Google Chrome for Testing ${version}`
  );
}

export function protocolVersionMatches(product, version) {
  return product === `Chrome/${version}` || product === `HeadlessChrome/${version}`;
}

async function verifyCommandVersion(binary, version) {
  const child = spawn(binary, ["--version"], {
    stdio: ["ignore", "pipe", "pipe"],
    detached: process.platform !== "win32",
  });
  const owned = observeOwnedChild(child, { processGroup: process.platform !== "win32" });
  let output = "";
  let deadline;
  child.stdout.on("data", (data) => {
    output = (output + data.toString()).slice(0, 4097);
  });
  child.stderr.resume();
  return withCleanup(
    async () => {
      try {
        await Promise.race([
          owned.closed.then(() => {
            if (
              owned.state.exitCode !== 0 ||
              owned.state.spawnError ||
              output.length > 4096 ||
              !commandVersionMatches(output, version)
            ) {
              throw new PreflightFailure("version");
            }
          }),
          new Promise((_, reject) => {
            deadline = setTimeout(() => reject(new PreflightFailure("version")), 2000);
          }),
        ]);
      } finally {
        clearTimeout(deadline);
      }
    },
    async () => {
      try {
        await owned.stop();
      } catch {
        throw new PreflightFailure("cleanup");
      }
    },
  );
}

export async function preflightChrome(binary, version) {
  if (!/^\d+\.\d+\.\d+\.\d+$/.test(version ?? "")) throw new PreflightFailure("configuration");
  await verifyCommandVersion(binary, version);
  const folder = await mkdtemp(join(tmpdir(), "zoen-cft-preflight-"));
  return withBrowserProfile(folder, async () => {
    const profile = join(folder, "profile");
    const child = spawn(
      binary,
      [
        "--headless",
        "--disable-gpu",
        "--no-first-run",
        "--no-default-browser-check",
        "--disable-background-networking",
        "--disable-component-update",
        "--disable-sync",
        "--disable-default-apps",
        "--host-resolver-rules=MAP * ~NOTFOUND, EXCLUDE localhost",
        "--remote-debugging-address=127.0.0.1",
        "--remote-debugging-port=0",
        `--user-data-dir=${profile}`,
        "about:blank",
      ],
      { stdio: ["ignore", "pipe", "pipe"], detached: process.platform !== "win32" },
    );
    const owned = observeOwnedChild(child, { processGroup: process.platform !== "win32" });
    child.stdout.resume();
    child.stderr.resume();
    const startup = { portFile: "unobserved", checksStarted: 0, checksCompleted: 0 };
    let socket, deadline;
    let phase = "startup";
    return withCleanup(
      async () => {
        try {
          await Promise.race([
            (async () => {
              const endpoint = await chromeEndpoint(
                child,
                profile,
                startup,
                () => new PreflightFailure("startup"),
              );
              phase = "protocol";
              socket = new WebSocket(endpoint);
              await new Promise((resolve, reject) => {
                socket.addEventListener("open", resolve, { once: true });
                socket.addEventListener("error", () => reject(new PreflightFailure("protocol")), {
                  once: true,
                });
              });
              const product = await new Promise((resolve, reject) => {
                socket.addEventListener("message", (event) => {
                  try {
                    const message = JSON.parse(String(event.data));
                    if (message.id !== 1) return;
                    if (
                      message.error ||
                      !protocolVersionMatches(message.result?.product, version)
                    ) {
                      reject(new PreflightFailure("protocol"));
                    } else resolve(message.result.product);
                  } catch {
                    reject(new PreflightFailure("protocol"));
                  }
                });
                socket.addEventListener("error", () => reject(new PreflightFailure("protocol")), {
                  once: true,
                });
                socket.send(JSON.stringify({ id: 1, method: "Browser.getVersion" }));
              });
              if (!protocolVersionMatches(product, version)) throw new PreflightFailure("protocol");
            })(),
            new Promise((_, reject) => {
              deadline = setTimeout(() => reject(new PreflightFailure(phase)), 10_000);
            }),
          ]);
          return {
            event: "pinnedChromePreflight",
            status: "ready",
            version,
            protocolVersionMatched: true,
          };
        } catch (error) {
          await annotateOwnedBrowserFailure(error, child, profile);
          error.startup = startup;
          throw error;
        }
      },
      async () => {
        clearTimeout(deadline);
        try {
          await owned.stop(() => {
            if (socket?.readyState === WebSocket.OPEN) {
              socket.send(JSON.stringify({ id: 2, method: "Browser.close" }));
            }
          });
        } catch {
          throw new PreflightFailure("cleanup");
        } finally {
          socket?.close();
        }
      },
    );
  });
}

export async function main() {
  try {
    const binary = await findBrowserExecutable({ env: { ...process.env, CI: "true" } });
    console.log(
      JSON.stringify(await preflightChrome(binary, process.env.ZOEN_PINNED_CHROME_VERSION)),
    );
  } catch (error) {
    const phases = new Set(["configuration", "version", "startup", "protocol", "cleanup"]);
    const primary = error instanceof AggregateError ? error.cause : error;
    console.error(
      JSON.stringify({
        event: "pinnedChromePreflight",
        status: "failed",
        phase:
          error instanceof AggregateError
            ? "cleanup"
            : phases.has(error.phase)
              ? error.phase
              : "configuration",
        ...(error instanceof AggregateError && phases.has(primary?.phase)
          ? { primaryPhase: primary.phase }
          : {}),
        ...(primary?.startup ? { startup: primary.startup } : {}),
        ...(primary?.ownedBrowserStartup
          ? { ownedBrowserStartup: primary.ownedBrowserStartup }
          : {}),
      }),
    );
    process.exitCode = 1;
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) await main();
