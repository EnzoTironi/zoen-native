import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { chmod, cp, mkdir, mkdtemp, readFile, realpath, rm, writeFile } from 'node:fs/promises';
import { dirname, join, relative, resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import { promisify } from 'node:util';
import { androidCargoTooling } from '../expo-android-cargo-proxy.mjs';
import { cargoTooling } from '../expo-cargo-artifacts.mjs';

const execute = promisify(execFile);
const source = resolve(dirname(fileURLToPath(import.meta.url)), '../..');

async function write(path, value) {
  await mkdir(dirname(path), { recursive: true });
  await writeFile(path, value);
}
async function executable(path, body) {
  await write(path, `#!${process.execPath}\n${body}\n`);
  await chmod(path, 0o700);
}

// Port fakes exercise the actual shell/Node orchestration and private-copy
// consumer. They do not qualify Rust, cargo-ndk, Kotlin/JNI or Android execution.
async function fixture(t, failure = '') {
  const base = process.env.EXPO_CARGO_TEST_ROOT ?? tmpdir();
  await mkdir(base, { recursive: true });
  const root = await realpath(await mkdtemp(join(base, 'android-ports-')));
  if (!process.env.EXPO_CARGO_TEST_ROOT) t.after(() => rm(root, { recursive: true, force: true }));
  for (const name of new Set([...cargoTooling, ...androidCargoTooling])) {
    await mkdir(join(root, 'scripts'), { recursive: true });
    await cp(join(source, 'scripts', name), join(root, 'scripts', name));
  }
  const cargo = process.env.CARGO_TEST_BIN ?? join(process.env.HOME, '.cargo/bin/cargo');
  const metadata = JSON.parse((await execute(cargo, [
    'metadata', '--locked', '--offline', '--no-deps', '--format-version', '1',
  ], { cwd: source, maxBuffer: 8 * 1024 * 1024 })).stdout);
  const manifests = Object.fromEntries(['client-core', 'uniffi-bindgen'].map((name) => {
    const pkg = metadata.packages.find((item) => item.name === name);
    assert(pkg, 'Expected an actual repository package');
    const manifest = relative(source, pkg.manifest_path);
    assert(!manifest.startsWith('..'), 'Actual package must be in this repository');
    return [name, manifest];
  }));
  await write(join(root, 'Cargo.toml'), '[workspace]\n');
  await write(join(root, 'Cargo.lock'), '# synthetic orchestration fixture\n');
  for (const [name, manifest] of Object.entries(manifests))
    await write(join(root, manifest), `[package]\nname="${name}"\n`);
  const bin = join(root, 'bin');
  const generator = `#!${process.execPath}
    const fs=require('node:fs'),p=require('node:path'),a=process.argv.slice(2),v=k=>a[a.indexOf(k)+1];
    if(fs.readFileSync(v('--library'),'utf8')!=='private-host') throw Error('shared host consumed');
    fs.mkdirSync(p.join(v('--out-dir'),'xyz/tironi/zoen/core'),{recursive:true});
    fs.writeFileSync(p.join(v('--out-dir'),'xyz/tironi/zoen/core/client_core.kt'),'private-kotlin');
  `;
  const ports = `
    const fs=require('node:fs'),p=require('node:path'),crypto=require('node:crypto');
    const a=process.argv.slice(2),v=k=>a[a.indexOf(k)+1],root=process.cwd();
    const hash=x=>crypto.createHash('sha256').update(x).digest('hex');
    const put=(path,value)=>{fs.mkdirSync(p.dirname(path),{recursive:true});fs.writeFileSync(path,value)};
    const paths=${JSON.stringify(manifests)}, failure=${JSON.stringify(failure)};
    const log=p.join(root,'commands.json'),history=fs.existsSync(log)?JSON.parse(fs.readFileSync(log)):[];
    history.push({program:p.basename(process.argv[1]),args:a});put(log,JSON.stringify(history));
    if(a[0]==='metadata') {
      console.log(JSON.stringify({workspace_root:root,packages:Object.entries(paths).map(([name,manifest])=>({name,manifest_path:p.join(root,manifest)}))}));
    } else if(a[0]?.endsWith('/android-core-contract.py')) {
      const out=v('--output');
      if(a[1]==='capture') put(p.join(out,'core-inputs.json'),JSON.stringify({input_sha256:'0'.repeat(64),library:{name:'client_core',package:'client-core',manifest:paths['client-core']}}));
      else if(a[1]==='finish') {
        if(fs.readFileSync(v('--host-library'),'utf8')!=='private-host') throw Error('finish consumed shared host');
        if(fs.readFileSync(p.join(out,'kotlin/xyz/tironi/zoen/core/client_core.kt'),'utf8')!=='private-kotlin') throw Error('changed Kotlin');
        for(const abi of v('--abis').split(',')) if(fs.readFileSync(p.join(out,'jniLibs',abi,'libclient_core.so'),'utf8')!=='private-'+abi) throw Error('shared JNI consumed');
        const proof=v('--cargo-qualification');
        put(p.join(out,'core-contract.json'),JSON.stringify({cargo_qualification:{path:proof,sha256:hash(fs.readFileSync(proof))}}));
      } else throw Error('Unexpected contract operation');
    } else if(a[0]?.endsWith('/cargo-source-guard.py')) {
      if(a.includes('--adopt-idle-target')) throw Error('automatic adoption');
      if(failure==='guard') process.exit(19);
      if(v('--cargo')!==p.join(root,'bin/cargo')) throw Error('missing real Cargo');
      const target=v('--target-dir'),receipt=v('--receipt'),pkg=v('-p');
      const name=pkg==='uniffi-bindgen'?'uniffi-bindgen':process.platform==='darwin'?'libclient_core.dylib':'libclient_core.so';
      const artifact=p.join(target,'debug',name),body=pkg==='uniffi-bindgen'?${JSON.stringify(generator)}:'private-host';
      // Invalidate the earlier shared generator before it is consumed.
      if(pkg==='client-core') put(p.join(target,'debug/uniffi-bindgen'),'foreign-generator');
      put(artifact,body);
      put(receipt,JSON.stringify({status:'PASS',target,before:{packages:{fixture:'same'},context:{config:{fixture:'same'}}},artifacts:[{manifest:p.join(root,paths[pkg]),path:artifact,sha256:hash(body)}]}));
    } else if(a[0]==='ndk' && a[1]==='--version') console.log('cargo-ndk 4.1.2');
    else if(a[0]==='ndk') {
      if(a.includes('-o')||process.env.CARGO_NDK_OUTPUT_PATH!==undefined) throw Error('NDK shared copy');
      const c=JSON.parse(fs.readFileSync(process.env.ZOEN_ANDROID_CARGO_CONTEXT));
      const artifact=p.join(c.directory,'private/libclient_core.so'),body='private-'+c.abi;
      put(artifact,body);
      const receipt=p.join(c.directory,'receipt.json'),qualification=p.join(c.directory,'qualification.json');
      put(receipt,JSON.stringify({status:'PASS',target:c.target,scope:{profile:c.profile,targets:[c.triple]}}));
      put(qualification,JSON.stringify({sourceSha256:c.sourceSha256,builds:[{receipt,artifacts:[{path:artifact,sha256:hash(body)}]}]}));
      put(c.report,JSON.stringify({schema:1,abi:c.abi,triple:c.triple,profile:c.profile,manifest:c.manifest,library:artifact,qualification,ndk:'/fixture/ndk'}));
      // Shared paths are unusable after their guard leases. Only private copies work.
      for(const name of ['libclient_core.so','libclient_core.dylib','uniffi-bindgen']) put(p.join(c.target,'debug',name),'foreign');
      if(failure==='raced') put(artifact,'changed-private-library');
    } else if(a[0]?.endsWith('/fix-android-uniffi-callbacks.py')) {}
    else if(a[0]==='target' && a[1]==='add') {}
    else throw Error('Unexpected port invocation');
  `;
  for (const name of ['cargo', 'cargo-ndk', 'python3', 'rustup'])
    await executable(join(bin, name), ports);
  await mkdir(join(root, 'sdk/ndk'), { recursive: true });
  const output = join(root, 'apps/zoen/modules/zoen-core/generated/android');
  await write(join(output, 'kotlin/xyz/tironi/zoen/core/client_core.kt'), 'previous-output');
  return { root, output, env: { ...process.env, PATH: bin + ':' + process.env.PATH,
    ANDROID_HOME: join(root, 'sdk'), CARGO_TARGET_DIR: join(root, 'shared'),
    ZOEN_CARGO_GUARD_PYTHON: join(bin, 'python3') } };
}

test('shared Android shell entrypoint stages only qualified private Kotlin/JNI and keeps actual generator manifest', async (t) => {
  const { root, output, env } = await fixture(t);
  await execute('bash', ['scripts/build-android-core.sh', '--release'], { cwd: root, env });
  assert.equal(await readFile(join(output, 'kotlin/xyz/tironi/zoen/core/client_core.kt'), 'utf8'), 'private-kotlin');
  for (const abi of ['arm64-v8a', 'x86_64'])
    assert.equal(await readFile(join(output, 'jniLibs', abi, 'libclient_core.so'), 'utf8'), 'private-' + abi);
  const contract = JSON.parse(await readFile(join(output, 'core-contract.json'), 'utf8'));
  const qualification = JSON.parse(await readFile(contract.cargo_qualification.path, 'utf8'));
  assert.equal(qualification.abis.length, 2);
  const commands = JSON.parse(await readFile(join(root, 'commands.json'), 'utf8'));
  const guarded = commands.filter((c) => c.args[0]?.endsWith('/cargo-source-guard.py'));
  assert.equal(guarded.length, 2);
  assert(guarded.every((c) => c.args.includes('--cargo') && !c.args.includes('--adopt-idle-target')));
  assert.equal(commands.filter((c) => c.args[0] === 'ndk' && c.args[1] === '-t').length, 2);
});

for (const failure of ['guard', 'raced'])
  test('Android ' + failure + ' refusal leaves previous generated output untouched', async (t) => {
    const { root, output, env } = await fixture(t, failure);
    await assert.rejects(execute('bash', ['scripts/build-android-core.sh'], { cwd: root, env }));
    assert.equal(await readFile(join(output, 'kotlin/xyz/tironi/zoen/core/client_core.kt'), 'utf8'), 'previous-output');
    await assert.rejects(readFile(join(output, 'core-contract.json')), { code: 'ENOENT' });
  });
