import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { chmod, cp, mkdir, mkdtemp, readFile, realpath, rm, writeFile } from 'node:fs/promises';
import { dirname, isAbsolute, join, relative, resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import { promisify } from 'node:util';
import { cargoTooling } from '../expo-cargo-artifacts.mjs';

const execute = promisify(execFile);
const source = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
let repositoryPaths;
async function repositoryPackagePaths() {
  if (!repositoryPaths) {
    const cargo = process.env.CARGO_TEST_BIN ?? join(process.env.HOME, '.cargo/bin/cargo');
    const { stdout } = await execute(
      cargo,
      ['metadata', '--locked', '--offline', '--no-deps', '--format-version', '1'],
      { cwd: source, maxBuffer: 8 * 1024 * 1024 },
    );
    const packages = JSON.parse(stdout).packages;
    repositoryPaths = Object.fromEntries(
      ['client-core', 'uniffi-bindgen'].map((name) => {
        const pkg = packages.find((item) => item.name === name);
        assert(pkg, `Missing actual repository package: ${name}`);
        const manifest = relative(source, pkg.manifest_path);
        assert(
          !manifest.startsWith('..') && !isAbsolute(manifest),
          'Fixture package must be inside the actual repository',
        );
        return [name, manifest];
      }),
    );
  }
  return repositoryPaths;
}
async function write(path, value) {
  await mkdir(dirname(path), { recursive: true });
  await writeFile(path, value);
}
async function executable(path, body) {
  await write(path, `#!${process.execPath}\n${body}\n`);
  await chmod(path, 0o700);
}

// These fakes exercise the real entrypoint's orchestration, inventories and
// receipt consumption. They are not Rust, Xcode or native compilation proof.
async function fixture(t, refuse = false) {
  const base = process.env.EXPO_CARGO_TEST_ROOT ?? tmpdir();
  await mkdir(base, { recursive: true });
  const root = await realpath(await mkdtemp(join(base, 'expo-ios-')));
  if (!process.env.EXPO_CARGO_TEST_ROOT) t.after(() => rm(root, { recursive: true, force: true }));
  for (const name of cargoTooling) {
    const from =
      name === 'build-expo-core.mjs' && process.env.EXPO_BUILDER_UNDER_TEST
        ? process.env.EXPO_BUILDER_UNDER_TEST
        : join(source, 'scripts', name);
    await mkdir(join(root, 'scripts'), { recursive: true });
    await cp(from, join(root, 'scripts', name));
  }
  await write(join(root, 'Cargo.toml'), '[workspace]\n');
  await write(join(root, 'Cargo.lock'), '# synthetic fixture\n');
  const packagePaths = await repositoryPackagePaths();
  await write(join(root, 'repository-package-paths.json'), JSON.stringify(packagePaths));
  for (const [name, manifest] of Object.entries(packagePaths))
    await write(join(root, manifest), `[package]\nname="${name}"\n`);
  const bin = join(root, 'home/.cargo/bin');
  await executable(
    join(bin, 'cargo'),
    `
    const p = require('node:path');
    if (process.argv[2] !== 'metadata') throw Error('unguarded Cargo compilation');
    const paths=${JSON.stringify(packagePaths)};
    console.log(JSON.stringify({packages: Object.entries(paths).map(([name,manifest]) => ({
      name,manifest_path:p.join(process.cwd(),manifest),dependencies:[]
    }))}));
  `,
  );
  await executable(join(bin, 'xcrun'), `console.log('/sdk/' + process.argv[3]);`);
  const guard = join(bin, 'fixture-guard');
  await executable(
    guard,
    `
    const fs=require('node:fs'), p=require('node:path'), crypto=require('node:crypto');
    const a=process.argv.slice(2), value=k=>a[a.indexOf(k)+1], root=process.cwd();
    if(${refuse}) process.exit(19);
    if(!a[0].endsWith('/scripts/cargo-source-guard.py') || a.includes('--adopt-idle-target')) throw Error('bad admission');
    const target=value('--target-dir'), receipt=value('--receipt'), pkg=value('-p');
    const profile=a.includes('--profile')?value('--profile'):'dev';
    const architecture=a.includes('--target')?value('--target'):null;
    const log=p.join(root,'commands.json');
    const prior=fs.existsSync(log)?JSON.parse(fs.readFileSync(log)):[];
    // Simulate a different cooperating build replacing earlier shared outputs.
    for(const old of prior) fs.writeFileSync(old.artifact,'foreign output');
    const name=pkg==='uniffi-bindgen'?'uniffi-bindgen':architecture?'libclient_core.a':'libclient_core.dylib';
    const artifact=p.join(target,...(architecture?[architecture]:[]),profile==='dev'?'debug':profile,name);
    fs.mkdirSync(p.dirname(artifact),{recursive:true});
    let body=architecture?'static:'+architecture:'host:'+profile;
    if(pkg==='uniffi-bindgen') body=${JSON.stringify(`#!${process.execPath}
      const fs=require('node:fs'),p=require('node:path');
      const a=process.argv.slice(2), v=k=>a[a.indexOf(k)+1];
      const host=fs.readFileSync(v('--library'),'utf8');
      if(host!=='host:release') throw Error('shared host library consumed');
      for(const name of ['client_core.swift','client_coreFFI.h','client_coreFFI.modulemap'])
        fs.writeFileSync(p.join(v('--out-dir'),name),host);
    `)};
    fs.writeFileSync(artifact,body);
    prior.push({args:a,artifact,profile,architecture,sdk:process.env.SDKROOT,deployment:process.env.IPHONEOS_DEPLOYMENT_TARGET,resource:process.env.ZOEN_DECKCRAFT_RESOURCE_MANIFEST_JSON,photo:process.env.ZOEN_PHOTOCRAFT_RESOURCE_MANIFEST_JSON,pdf:process.env.ZOEN_PDFCRAFT_RESOURCE_MANIFEST_JSON});
    fs.writeFileSync(log,JSON.stringify(prior));
    fs.writeFileSync(receipt,JSON.stringify({status:'PASS',target,
      before:{packages:{fixture:'stable'},context:{config:{fixture:'stable'},environment:Object.fromEntries(a.flatMap((v,i)=>v==='--input-env'?[[a[i+1],process.env[a[i+1]]===undefined?null:crypto.createHash('sha256').update(JSON.stringify(process.env[a[i+1]])).digest('hex')]]:[]))}},
      artifacts:[{manifest:p.join(root,${JSON.stringify(packagePaths)}[pkg]),path:artifact,
      sha256:crypto.createHash('sha256').update(body).digest('hex')}]}));
  `,
  );
  await executable(
    join(bin, 'xcodebuild'),
    `
    const fs=require('node:fs'),p=require('node:path'),a=process.argv.slice(2);
    const out=a[a.indexOf('-output')+1];fs.mkdirSync(out,{recursive:true});
    fs.writeFileSync(p.join(out,'Info.plist'),'fixture');
    for(let i=0;i<a.length;i++) if(a[i]==='-library') {
      const lib=a[i+1],headers=a[i+3],body=fs.readFileSync(lib,'utf8');
      if(!body.startsWith('static:aarch64-apple-ios')) throw Error('shared static library consumed');
      if(p.basename(lib)!=='libclient_core.a') throw Error('changed framework filename');
      const slice=body.endsWith('-sim')?'ios-arm64-simulator':'ios-arm64';
      const dir=p.join(out,slice);fs.mkdirSync(dir,{recursive:true});
      fs.copyFileSync(lib,p.join(dir,p.basename(lib)));
      fs.cpSync(headers,p.join(dir,'Headers'),{recursive:true});
    }
  `,
  );
  return {
    root,
    env: {
      ...process.env,
      HOME: join(root, 'home'),
      PATH: bin + ':' + process.env.PATH,
      CARGO_TARGET_DIR: join(root, 'shared'),
      ZOEN_CARGO_GUARD_PYTHON: guard,
      ZOEN_DECKCRAFT_RESOURCE_MANIFEST_JSON: '{"fixture":"manifest"}',
    },
  };
}

test('iOS release binds each SDK/profile, preserves output inventories and consumes only private copies', async (t) => {
  const { root, env } = await fixture(t);
  await execute(process.execPath, ['scripts/build-expo-core.mjs', 'ios', '--release'], {
    cwd: root,
    env,
  });
  const commands = JSON.parse(await readFile(join(root, 'commands.json'), 'utf8'));
  assert.equal(commands.length, 4);
  assert.deepEqual(
    commands.map((c) => [c.profile, c.architecture, c.sdk]),
    [
      ['release', null, '/sdk/macosx'],
      ['release', 'aarch64-apple-ios', '/sdk/iphoneos'],
      ['release', 'aarch64-apple-ios-sim', '/sdk/iphonesimulator'],
      ['dev', null, '/sdk/macosx'],
    ],
  );
  assert(commands.every((c) => c.resource === '{"fixture":"manifest"}'));
  assert(commands.filter((c) => c.architecture).every((c) => c.deployment === '26.0'));
  for (const command of commands) {
    assert(command.args.includes('SDKROOT'));
    assert(command.args.includes('ZOEN_DECKCRAFT_RESOURCE_MANIFEST_JSON'));
    assert(command.args.includes('ZOEN_PHOTOCRAFT_RESOURCE_MANIFEST_JSON'));
    assert(!command.args.includes('run'));
  }
  const stamp = JSON.parse(
    await readFile(join(root, 'apps/zoen/modules/zoen-core/generated/ios.json'), 'utf8'),
  );
  const qualification = JSON.parse(await readFile(stamp.cargoQualification, 'utf8'));
  assert.equal(qualification.builds.length, 4);
  const generatorReceipt = JSON.parse(await readFile(qualification.builds[3].receipt, 'utf8'));
  // The fixture uses actual repository metadata, rather than reproducing the
  // builder's assumptions about where a workspace package ought to live.
  assert.equal(
    generatorReceipt.artifacts[0].manifest,
    join(root, (await repositoryPackagePaths())['uniffi-bindgen']),
  );
  assert.equal(
    await readFile(
      join(root, 'apps/zoen/modules/zoen-core/ios/generated/ClientFFI.generated.swift'),
      'utf8',
    ),
    'host:release',
  );
  // Even an otherwise matching output stamp cannot bypass shared-cache admission.
  await execute(process.execPath, ['scripts/build-expo-core.mjs', 'ios', '--release'], {
    cwd: root,
    env,
  });
  const again = JSON.parse(await readFile(join(root, 'commands.json'), 'utf8'));
  assert.equal(again.length, 8);
});

test('initial iOS guard failure leaves the previous framework untouched and writes no platform stamp', async (t) => {
  const { root, env } = await fixture(t, true);
  const previous = join(root, 'apps/zoen/modules/zoen-core/ios/generated/keep');
  await write(previous, 'prior framework');
  await assert.rejects(
    execute(process.execPath, ['scripts/build-expo-core.mjs', 'ios'], { cwd: root, env }),
    /Guarded Cargo build failed/,
  );
  assert.equal(await readFile(previous, 'utf8'), 'prior framework');
  await assert.rejects(readFile(join(root, 'apps/zoen/modules/zoen-core/generated/ios.json')), {
    code: 'ENOENT',
  });
});

// Exercise the normal Node producer/cache/guard transport with explicit tool ports;
// this fixture never claims Cargo, Xcode, UniFFI or native runtime qualification.
for (const [name, field, variable] of [
  ['Photo', 'photo', 'ZOEN_PHOTOCRAFT_RESOURCE_MANIFEST_JSON'],
  ['PDF', 'pdf', 'ZOEN_PDFCRAFT_RESOURCE_MANIFEST_JSON'],
]) {
  test(`${name} resource-only changes reach every guarded build and invalidate the native stamp`, async (t) => {
    const { root, env } = await fixture(t);
    const build = (value) => execute(process.execPath, ['scripts/build-expo-core.mjs', 'ios', '--release', '--simulator'], {
      cwd: root,
      env: { ...env, [variable]: value },
    });
    const stamp = () => readFile(join(root, 'apps/zoen/modules/zoen-core/generated/ios.json'), 'utf8').then(JSON.parse);
    const first = JSON.stringify({ fixture: `${field}-A` });
    const last = JSON.stringify({ fixture: `${field}-B` });
    await build(first);
    const before = await stamp();
    await build(last);
    const after = await stamp();
    assert.notEqual(after.sha256, before.sha256, `${name}-only deployment changes must invalidate native reuse`);
    assert.notEqual(after.cargoQualification, before.cargoQualification);
    const qualification = JSON.parse(await readFile(after.cargoQualification, 'utf8'));
    assert.equal(qualification.builds.length, 3);
    const expected = (await import('node:crypto')).createHash('sha256').update(JSON.stringify(last)).digest('hex');
    for (const build of qualification.builds) {
      const guard = JSON.parse(await readFile(build.receipt, 'utf8'));
      assert.equal(guard.status, 'PASS');
      assert.equal(guard.before.context.environment[variable], expected);
    }
    const commands = JSON.parse(await readFile(join(root, 'commands.json'), 'utf8'));
    assert(commands.slice(-3).every((command) => command[field] === last));
  });
}
