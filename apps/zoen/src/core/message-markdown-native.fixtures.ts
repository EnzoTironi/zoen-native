import { cpSync, existsSync, mkdirSync, readFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { join } from 'node:path';

/** Reconstruct the maintained native quote pipeline privately from installed dependencies. */
export function preparePackage(app: string, directory: string) {
  const pkg = join(directory, 'node_modules/react-native-enriched-markdown');
  const installed = join(app, 'node_modules/react-native-enriched-markdown');
  const patches = [
    '007+quote-source-map',
    '008+message-context-admission',
    '009+github-quote-pipeline',
  ].map((stage) => join(app, `patches/react-native-enriched-markdown+1.1.1+${stage}.patch`));
  const paths = new Set(
    patches.flatMap((patch) =>
      [
        ...readFileSync(patch, 'utf8').matchAll(
          /^diff --git a\/node_modules\/react-native-enriched-markdown\/(.+) b\//gm,
        ),
      ].map((match) => match[1]!),
    ),
  );
  for (const path of paths) {
    if (!existsSync(join(installed, path))) continue;
    mkdirSync(join(pkg, path, '..'), { recursive: true });
    cpSync(join(installed, path), join(pkg, path));
  }
  for (const path of ['cpp/parser', 'cpp/md4c', 'src', 'ios/parser', 'ios/segments'])
    cpSync(join(installed, path), join(pkg, path), { recursive: true });
  const markers = [
    () => existsSync(join(pkg, 'ios/utils/ENRMQuoteRanges.m')),
    () =>
      existsSync(join(pkg, 'ios/utils/ENRMMessageContextAdmission.h')) ||
      readFileSync(join(pkg, 'ios/EnrichedMarkdownText.mm'), 'utf8').includes(
        'zoenMessageContextBodyAtPoint:',
      ),
    () => existsSync(join(pkg, 'ios/utils/ENRMMessageContextAdmission.h')),
  ];
  for (const [index, patch] of patches.entries())
    if (!markers[index]!())
      execFileSync('patch', ['--batch', '--forward', '-p1', '-i', patch], { cwd: directory });
  return pkg;
}
