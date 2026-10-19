#!/usr/bin/env node
// Build current shared Rust for the private Worker; no native FDB fixture is needed.
import { spawn } from 'node:child_process';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const child = spawn(process.execPath, [
  join(root, 'scripts/verify-cloudflare-admission.mjs'),
  '--out-dir', join(root, 'infra/cloudflare/relay-state/generated'),
], {
  cwd: root, stdio: 'inherit',
  env: { ...process.env, CARGO_BUILD_JOBS: process.env.CARGO_BUILD_JOBS ?? '2' },
});
child.once('error', error => { console.error(error.message); process.exitCode = 1; });
child.once('close', (code, signal) => {
  if (signal) console.error(`Admission build stopped by ${signal}`);
  process.exitCode = code ?? 1;
});
