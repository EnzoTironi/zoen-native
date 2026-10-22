import type { ChildProcess } from 'node:child_process';
import { open } from 'node:fs/promises';
import { join } from 'node:path';

export type ChromeStartup = {
  portFile: 'missing' | 'incomplete' | 'ready' | 'invalid' | 'unreadable';
};

/** Chrome publishes this endpoint in its fresh profile, independently of logging.
 * https://chromedevtools.github.io/devtools-protocol/#how-do-i-access-the-browser-target
 * The caller owns process termination and the profile's lifetime. */
export function chromeEndpoint(
  child: ChildProcess,
  profile: string,
  startup: ChromeStartup,
  failure: (message: string, cause?: Error) => Error,
): Promise<string> {
  return new Promise((resolve, reject) => {
    let done = false;
    let poll: ReturnType<typeof setTimeout> | undefined;
    const finish = (error: Error | null, endpoint?: string) => {
      if (done) return;
      done = true;
      clearTimeout(deadline);
      clearTimeout(poll);
      child.removeListener('error', onError);
      child.removeListener('exit', onExit);
      if (error) reject(error);
      else resolve(endpoint!);
    };
    const onError = (error: Error) => finish(failure('Owned Chrome could not spawn', error));
    const onExit = () => finish(failure('Owned Chrome exited before connection'));
    const deadline = setTimeout(() => finish(failure('Owned Chrome did not start')), 10_000);
    child.once('error', onError);
    child.once('exit', onExit);
    const inspect = async () => {
      try {
        const file = await open(join(profile, 'DevToolsActivePort'), 'r');
        let text: string;
        try {
          const buffer = Buffer.alloc(1025);
          const { bytesRead } = await file.read(buffer, 0, buffer.length, 0);
          text = bytesRead > 1024 ? '' : buffer.subarray(0, bytesRead).toString('utf8');
          startup.portFile = bytesRead > 1024 ? 'invalid' : 'incomplete';
        } finally {
          await file.close();
        }
        if (done) return;
        if (child.exitCode !== null || child.signalCode !== null) return onExit();
        const match = text.match(
          /^(\d{1,5})\n(\/devtools\/browser\/[\da-f]{8}-[\da-f]{4}-[\da-f]{4}-[\da-f]{4}-[\da-f]{12})\n?$/i,
        );
        if (match && Number(match[1]) > 0 && Number(match[1]) <= 65535) {
          startup.portFile = 'ready';
          finish(null, `ws://127.0.0.1:${match[1]}${match[2]}`);
          return;
        }
      } catch (error) {
        if (done) return;
        const code = (error as NodeJS.ErrnoException).code;
        startup.portFile = code === 'ENOENT' ? 'missing' : 'unreadable';
        if (code !== 'ENOENT') {
          finish(failure('Owned Chrome endpoint file could not be read', error as Error));
          return;
        }
      }
      if (!done) poll = setTimeout(() => void inspect(), 50);
    };
    if (child.exitCode !== null || child.signalCode !== null) onExit();
    else void inspect();
  });
}
