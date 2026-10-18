import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';

const root = new URL('../../', import.meta.url);
const harness = String.raw`
import Foundation

@main struct NativeCoreErrorMessageProbe {
  static func main() throws {
    guard CommandLine.arguments.count == 2,
          let bundle = Bundle(path: CommandLine.arguments[1]) else {
      fatalError("Missing synthetic localization bundle")
    }
    let invalid = CoreError.Invalid(reason: "Imagem inválida: café, ação e 日本語 🎨\nEscolha outra imagem.")
    let forbidden = CoreError.Forbidden(reason: "Você não pode editar este desenho.")
    let stale = CoreError.Stale(reason: "O desenho mudou. Abra a versão atual.")
    let rejected = CoreError.DraftRejected(reason: "Este destino foi removido. Seu rascunho foi mantido.")
    let storage = CoreError.Storage(message: "sqlite synthetic-private-path/store.sqlite: secret detail")
    let missing = CoreError.NotFound(what: "synthetic-private-item-id")
    let result = [
      "invalid": ZoenCoreErrorMessage.message(invalid, bundle: bundle),
      "forbidden": ZoenCoreErrorMessage.message(forbidden, bundle: bundle),
      "stale": ZoenCoreErrorMessage.message(stale, bundle: bundle),
      "rejected": ZoenCoreErrorMessage.message(rejected, bundle: bundle),
      "storage": ZoenCoreErrorMessage.message(storage, bundle: bundle),
      "missing": ZoenCoreErrorMessage.message(missing, bundle: bundle),
      "generatedInvalid": invalid.localizedDescription,
    ]
    print(String(decoding: try JSONSerialization.data(withJSONObject: result), as: UTF8.self))
  }
}
`;

test(
  'native CoreError presentation preserves real reasons and localizes safe fallbacks without reflection',
  { skip: process.platform !== 'darwin', timeout: 60_000 },
  () => {
    // Generated Apple artifacts do not exist on the Linux client CI job. Read
    // them only after the Darwin-only test has actually been admitted.
    const generated = readFileSync(
      new URL('modules/zoen-core/ios/generated/RodaFFI.generated.swift', root),
      'utf8',
    );
    // Compile the actual generated enum, including its reflective LocalizedError
    // implementation. No FFI converters or Expo SDK replacements are needed.
    const start = generated.indexOf('enum CoreError: Swift.Error,');
    const end = generated.indexOf('\n#if compiler(>=6)', start);
    assert.ok(start >= 0 && end > start, 'The installed generated CoreError must be exercised');
    const declaration = generated.slice(start, end);
    const directory = mkdtempSync(join(tmpdir(), 'zoen-native-core-error-'));
    try {
      const source = join(directory, 'Harness.swift');
      const binary = join(directory, 'probe');
      writeFileSync(source, `import Foundation\n${declaration}\n${harness}`);
      const sdk = execFileSync('xcrun', ['--sdk', 'macosx', '--show-sdk-path'], {
        encoding: 'utf8',
        timeout: 10_000,
      }).trim();
      execFileSync(
        'xcrun',
        [
          'swiftc',
          '-swift-version',
          '6',
          '-strict-concurrency=complete',
          '-warnings-as-errors',
          '-module-cache-path',
          join(directory, 'module-cache'),
          fileURLToPath(new URL('modules/zoen-core/ios/ZoenCoreErrorMessage.swift', root)),
          source,
          '-o',
          binary,
        ],
        { env: { ...process.env, SDKROOT: sdk }, timeout: 45_000 },
      );

      for (const locale of ['en', 'pt-BR']) {
        const configured = JSON.parse(
          readFileSync(new URL(`config/locales/${locale}.json`, root), 'utf8'),
        ).ios['Localizable.strings'] as Record<string, string>;
        const bundle = join(directory, `${locale}.bundle`);
        mkdirSync(bundle);
        writeFileSync(
          join(bundle, 'Localizable.strings'),
          Object.entries(configured)
            .map(([key, value]) => `${JSON.stringify(key)} = ${JSON.stringify(value)};`)
            .join('\n'),
        );
        const result = JSON.parse(
          execFileSync(binary, [bundle], { encoding: 'utf8', timeout: 10_000 }),
        ) as Record<string, string>;

        assert.match(result.generatedInvalid, /CoreError\.Invalid\(reason:/);
        assert.notEqual(result.invalid, result.generatedInvalid);
        assert.equal(
          result.invalid,
          'Imagem inválida: café, ação e 日本語 🎨\nEscolha outra imagem.',
        );
        assert.equal(result.forbidden, 'Você não pode editar este desenho.');
        assert.equal(result.stale, 'O desenho mudou. Abra a versão atual.');
        assert.equal(result.rejected, 'Este destino foi removido. Seu rascunho foi mantido.');
        assert.equal(
          result.storage,
          locale === 'pt-BR'
            ? 'Algo deu errado. Tente novamente.'
            : 'Something went wrong. Try again.',
        );
        assert.equal(
          result.missing,
          locale === 'pt-BR'
            ? 'Este conteúdo não está mais disponível. Atualize e tente novamente.'
            : 'This content is no longer available. Refresh and try again.',
        );
        for (const key of ['invalid', 'forbidden', 'stale', 'rejected', 'storage', 'missing']) {
          assert.doesNotMatch(result[key], /CoreError|reason:|sqlite|secret|synthetic-private/);
        }
      }
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  },
);
