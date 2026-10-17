// Load the actual generated development WASM, not a mock Core implementation.
// Build it and run wasm-bindgen first; missing artifacts are a failure, not a skip.
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import init, { BrowserCore } from '../packages/zoen-ui/web/generated/zoen_web_core.js';

const bytes = await readFile(fileURLToPath(new URL('../packages/zoen-ui/web/generated/zoen_web_core_bg.wasm', import.meta.url)));
const wasm = await init({ module_or_path: bytes });
assert.ok(wasm.memory instanceof WebAssembly.Memory);
for (const method of [
  'set_living_space_enabled', 'living_space_command', 'living_space_mutate',
  'page_create', 'page', 'page_apply', 'page_download_manifest', 'page_blob_supply',
  'file_manifest', 'file_open', 'file_prepare', 'file_commit',
]) {
  assert.equal(typeof BrowserCore.prototype[method], 'function', `Missing WASM method: ${method}`);
}
console.log('PASS actual WASM initialization and all 12 living Space/page/file entry points. No account or live relay was used.');
