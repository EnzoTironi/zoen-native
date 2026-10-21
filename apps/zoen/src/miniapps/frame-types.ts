import type { RuntimePackage } from './runtime-package.ts';
export type MiniappFrameHandle = { send: (message: unknown) => void; dispose: () => void };
export type MiniappFrameProps = {
  /** Resource HTML with the host-validated app CSP already applied. */
  html: string;
  onMessage: (message: string) => void;
  onError: (message: string) => void;
  height: number;
  title: string;
  blocked: boolean;
  fill?: boolean;
  /** Public executable assets pinned by the verified resource; no document/network authority. */
  runtimePackage?: RuntimePackage;
};
