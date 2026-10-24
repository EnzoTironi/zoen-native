import { isAbsolute } from "node:path";
import { buildManagedBinaryBootstrap } from "../../tools/miniapp-build/managed-binary-bootstrap.mjs";
const directory = process.argv[2];
if (!directory || !isAbsolute(directory))
  throw Error("Usage: build-bootstrap.mjs ABS_BUILD_DIRECTORY [--check]");
await buildManagedBinaryBootstrap("gridcraft", directory, process.argv.includes("--check"));
