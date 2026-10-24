import { buildBinaryResourceManifest } from "../../tools/miniapp-build/binary-resource-manifest.mjs";
await buildBinaryResourceManifest("photocraft", process.argv.slice(2));
