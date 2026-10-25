import { buildBinaryResourceManifest } from "../../tools/miniapp-build/binary-resource-manifest.mjs";
await buildBinaryResourceManifest("pdfcraft", process.argv.slice(2));
