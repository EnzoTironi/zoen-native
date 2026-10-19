import { createHash } from "node:crypto";
import { readFile, readdir } from "node:fs/promises";
import { join } from "node:path";

const fontModuleSha = "5d42242a4c5c7d7b835d154c291b838e2f0fcdccd649c8b2fcbe05b3a7c2b346";
const hash = (bytes) => createHash("sha256").update(bytes).digest("hex");
function replaceOnce(text, from, to) {
  if (text.split(from).length !== 2)
    throw new Error(`Pinned Excalidraw asset policy changed: ${from.slice(0, 80)}`);
  return text.replace(from, to);
}
export async function bundledLibraryAssets(root) {
  const base = join(root, "node_modules/@excalidraw/excalidraw/dist/prod");
  const assets = new Map();
  const hashes = {};
  for (const family of await readdir(join(base, "fonts"))) {
    if (family === "Xiaolai") continue;
    for (const name of await readdir(join(base, "fonts", family))) {
      if (!name.endsWith(".woff2")) continue;
      const bytes = await readFile(join(base, "fonts", family, name));
      const path = `./fonts/${family}/${name}`;
      hashes[path] = hash(bytes);
      assets.set(path, `data:font/woff2;base64,${bytes.toString("base64")}`);
    }
  }
  return {
    hashes,
    plugin: {
      name: "pinned-excalidraw-private-assets",
      setup(build) {
        build.onResolve({ filter: /^@excalidraw\/mermaid-to-excalidraw$/ }, () => ({
          path: "disabled-provider-converter",
          namespace: "zoen-disabled-converter",
        }));
        build.onLoad({ filter: /.*/, namespace: "zoen-disabled-converter" }, () => ({
          contents:
            'export async function parseMermaidToExcalidraw(){throw new Error("Provider conversion is not enabled in this private editor.")}',
          loader: "js",
        }));
        build.onLoad(
          { filter: /@excalidraw\/excalidraw\/dist\/prod\/index\.js$/ },
          async ({ path }) => {
            let text = await readFile(path, "utf8");
            if (hash(text) !== "6ed19fd70ef6ecbdafc47b66a405910c9a6e5982c72f2f4d3d94f37da8b6ad77")
              throw new Error("Pinned Excalidraw editor module changed");
            const menu =
              'Me(Ce.Item,{onSelect:()=>t.setOpenDialog({name:"ttd",tab:"mermaid"}),icon:Jl,"data-testid":"toolbar-embeddable",children:g("toolBar.mermaidToExcalidraw")})';
            text = replaceOnce(text, menu, `t.props.aiEnabled!==!1&&${menu}`);
            text = replaceOnce(
              text,
              "if(m.text&&lT(m.text))",
              "if(this.props.aiEnabled!==!1&&m.text&&lT(m.text))",
            );
            return { contents: text, loader: "js" };
          },
        );
        build.onLoad(
          { filter: /@excalidraw\/excalidraw\/dist\/prod\/chunk-FX7ZIABN\.js$/ },
          async ({ path }) => {
            let text = await readFile(path, "utf8");
            if (hash(text) !== fontModuleSha)
              throw new Error("Pinned Excalidraw font module changed");
            text = replaceOnce(text, 'Mn="Xiaolai"', 'Mn="system-ui"');
            text = replaceOnce(
              text,
              "[Un.Xiaolai]:{metrics:{unitsPerEm:1e3,ascender:880,descender:-144,lineHeight:1.15},fallback:!0}",
              "[Un[Mn]]:{metrics:{unitsPerEm:1e3,ascender:880,descender:-144,lineHeight:1.15},local:!0,fallback:!0}",
            );
            text = replaceOnce(text, "n(Mn,...b1)", "n(Mn,{uri:Wn})");
            // All standard font bytes are build inputs. No runtime CDN fallback.
            text = replaceOnce(text, "r.push(new URL(n,jn.ASSETS_FALLBACK_URL)),r", "r");
            text = replaceOnce(
              text,
              'let n=await fetch(t,{cache:"force-cache",headers:{Accept:"font/woff2"}})',
              'if(typeof t==="string"&&t.startsWith("data:font/woff2;base64,")){const raw=atob(t.split(",")[1]);return Uint8Array.from(raw,c=>c.charCodeAt(0)).buffer}let n=await fetch(t,{cache:"force-cache",headers:{Accept:"font/woff2"}})',
            );
            for (const [uri, data] of assets)
              text = text.split(JSON.stringify(uri)).join(JSON.stringify(data));
            return { contents: text, loader: "js" };
          },
        );
      },
    },
  };
}
