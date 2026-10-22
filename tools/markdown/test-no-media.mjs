import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const request = createRequire(join(root, "apps/zoen/package.json"));
const supplied = process.argv[2];
const pkg = supplied
  ? resolve(supplied)
  : dirname(request.resolve("react-native-enriched-markdown/package.json"));
if (JSON.parse(readFileSync(join(pkg, "package.json"), "utf8")).version !== "1.1.1")
  throw new Error("Expected pinned Markdown 1.1.1 source.");
const temp = mkdtempSync(join(tmpdir(), "zoen-markdown-no-media-"));
function run(command, args) {
  const result = spawnSync(command, args, { encoding: "utf8", timeout: 45_000 });
  if (result.error) throw result.error;
  if (result.status !== 0)
    throw new Error(`${command} exited ${result.status}: ${result.stdout}${result.stderr}`);
  return result.stdout;
}
try {
  run("cc", [
    "-std=c99",
    "-O0",
    "-g0",
    "-c",
    join(pkg, "cpp/md4c/md4c.c"),
    "-o",
    join(temp, "md4c.o"),
  ]);
  run("c++", [
    "-std=c++17",
    "-O0",
    "-g0",
    `-I${join(pkg, "cpp/parser")}`,
    "-c",
    join(pkg, "cpp/parser/MD4CParser.cpp"),
    "-o",
    join(temp, "parser.o"),
  ]);
  run("c++", [
    "-std=c++17",
    "-O0",
    "-g0",
    `-I${join(pkg, "cpp/parser")}`,
    join(root, "tools/markdown/no-media-test.cpp"),
    join(temp, "parser.o"),
    join(temp, "md4c.o"),
    "-o",
    join(temp, "no-media"),
  ]);
  process.stdout.write(
    run(
      join(temp, "no-media"),
      process.argv.includes("--defaults-only") ? ["--defaults-only"] : [],
    ),
  );
  if (process.argv.includes("--ios-bridge")) {
    if (process.platform !== "darwin")
      throw new Error("The Objective-C Foundation bridge check requires macOS.");
    const objcStubs = join(temp, "objc-stubs");
    const cppStubs = join(temp, "cpp-stubs");
    mkdirSync(join(objcStubs, "React"), { recursive: true });
    mkdirSync(join(cppStubs, "react/renderer/graphics"), { recursive: true });
    writeFileSync(
      join(objcStubs, "React/RCTLog.h"),
      "#pragma once\n#define RCTLogError(...) ((void)0)\n",
    );
    writeFileSync(join(objcStubs, "React/RCTUtils.h"), "#pragma once\n");
    writeFileSync(
      join(cppStubs, "react/renderer/graphics/Float.h"),
      "#pragma once\nnamespace facebook::react { using Float = double; }\n",
    );
    run("clang", [
      "-fobjc-arc",
      "-O0",
      "-g0",
      `-I${join(pkg, "ios/parser")}`,
      "-c",
      join(pkg, "ios/parser/MarkdownASTNode.m"),
      "-o",
      join(temp, "ast.o"),
    ]);
    run("clang++", [
      "-std=c++17",
      "-fobjc-arc",
      "-O0",
      "-g0",
      "-framework",
      "Foundation",
      `-I${join(pkg, "ios/parser")}`,
      `-I${join(pkg, "cpp/parser")}`,
      `-I${objcStubs}`,
      join(pkg, "ios/parser/ENRMMarkdownParser.mm"),
      join(pkg, "ios/parser/MarkdownParserBridge.mm"),
      join(root, "tools/markdown/no-media-bridge-test.mm"),
      join(temp, "parser.o"),
      join(temp, "md4c.o"),
      join(temp, "ast.o"),
      "-o",
      join(temp, "bridge"),
    ]);
    process.stdout.write(run(join(temp, "bridge"), []));
    run("clang++", [
      "-std=c++20",
      "-O0",
      "-g0",
      `-I${cppStubs}`,
      `-I${objcStubs}`,
      `-I${join(pkg, "ios/internals")}`,
      join(root, "tools/markdown/no-media-cache-test.cpp"),
      "-o",
      join(temp, "cache"),
    ]);
    process.stdout.write(run(join(temp, "cache"), []));
  }
} finally {
  rmSync(temp, { recursive: true, force: true });
}
