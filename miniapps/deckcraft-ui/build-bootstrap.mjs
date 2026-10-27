import { buildManagedBinaryBootstrap } from "../../tools/miniapp-build/managed-binary-bootstrap.mjs";
await buildManagedBinaryBootstrap("deckcraft", undefined, process.argv.includes("--check"));
