import { readFile, writeFile, mkdir } from "node:fs/promises";
import { createHash } from "node:crypto";
import { fileURLToPath } from "node:url";
import { resolve } from "node:path";
import { isAbsolute } from "node:path";
import { buildManagedBinaryBootstrap } from "../../tools/miniapp-build/managed-binary-bootstrap.mjs";
const directory = process.argv[2];
if (!directory || !isAbsolute(directory))
  throw Error("Usage: build-bootstrap.mjs ABS_BUILD_DIRECTORY [--check]");
await buildManagedBinaryBootstrap("gridcraft", directory, process.argv.includes("--check"));

if (process.argv.includes("--install-resource")) {
  const root = fileURLToPath(new URL("../../", import.meta.url));
  const html = await readFile(resolve(directory, "gridcraft.html"));
  const notices = await readFile(resolve(directory, "BOOTSTRAP-LICENSES.txt"));
  const manifest =
    JSON.stringify(
      {
        id: "gridcraft",
        name: "GridCraft",
        resourceUri: "ui://zoen/gridcraft/index.html",
        mimeType: "text/html;profile=mcp-app",
        capabilities: [],
        allowedDomains: [],
        linkDomains: [],
        networkPurpose: "",
        permissions: {},
        csp: { connectDomains: [], resourceDomains: [], frameDomains: [], baseUriDomains: [] },
        sha256: createHash("sha256").update(html).digest("hex"),
        bytes: html.length,
      },
      null,
      2,
    ) + "\n";
  const generated = resolve(root, "crates/miniapps/resources/generated");
  if (process.argv.includes("--check")) {
    if (
      !(await readFile(resolve(generated, "gridcraft.html"))).equals(html) ||
      (await readFile(
        resolve(root, "crates/miniapps/resources/gridcraft.manifest.json"),
        "utf8",
      )) !== manifest
    )
      throw Error("Stale generated Grid resource");
    if (!(await readFile(resolve(generated, "gridcraft-bootstrap-licenses.txt"))).equals(notices))
      throw Error("Stale generated Grid licenses");
  } else {
    await mkdir(generated, { recursive: true });
    await writeFile(resolve(generated, "gridcraft.html"), html);
    await writeFile(resolve(generated, "gridcraft-bootstrap-licenses.txt"), notices);
    await writeFile(resolve(root, "crates/miniapps/resources/gridcraft.manifest.json"), manifest);
  }
}
