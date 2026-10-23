// Exercise private extraction and upstream signature regressions in a disposable
// copy of the exact pinned plugin. Production dependency sources stay untouched.
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { parse, stringify } from 'smol-toml';
import { updaterPin, updaterSource, sha256 } from './scan.mjs';

assert.equal(process.platform, 'win32', 'Updater archive regressions require Windows');
const root = path.resolve(fileURLToPath(new URL('../../', import.meta.url)));
const base = path.join(root, 'output/updater-regressions');
fs.mkdirSync(base, { recursive: true });
const out = fs.mkdtempSync(path.join(base, 'run-'));
const fixture = path.join(out, 'plugin');
const temp = path.join(out, 'temp');
fs.mkdirSync(temp);
const evidence = { startedAt: new Date().toISOString(), pin: updaterPin, status: 'error' };
const run = (args, name, cwd = root) => {
  const result = spawnSync('cargo', args, { cwd, encoding: 'utf8', timeout: 1200000,
    maxBuffer: 64 * 1024 * 1024,
    env: { ...process.env, TMP: temp, TEMP: temp, CARGO_TARGET_DIR: path.join(root, 'src-tauri/target') } });
  fs.writeFileSync(path.join(out, `${name}.stdout.log`), result.stdout ?? '');
  fs.writeFileSync(path.join(out, `${name}.stderr.log`), result.stderr ?? '');
  assert(!result.error && result.status === 0, `${name} failed (${result.status}): ${result.error?.message ?? result.stderr?.slice(-3000)}. Reports: ${out}`);
  return result.stdout;
};
console.log(`Updater fixture reports: ${out}`);
try {
  const metadata = JSON.parse(run(['metadata', '--locked', '--format-version', '1', '--manifest-path', 'src-tauri/Cargo.toml'], 'application-metadata'));
  const pkg = metadata.packages.find(p => p.name === updaterPin.name && p.source === updaterSource);
  assert(pkg && pkg.version === updaterPin.version, 'Expected official updater pin');
  const source = path.dirname(pkg.manifest_path), workspace = path.resolve(source, '../..');
  const original = fs.readFileSync(path.join(source, 'src/updater.rs'), 'utf8');
  const git = spawnSync('git', ['show', `${updaterPin.rev}:plugins/updater/src/updater.rs`], { cwd: workspace, encoding: 'utf8', maxBuffer: 4 * 1024 * 1024 });
  assert(git.status === 0 && original.replaceAll('\r\n', '\n') === git.stdout.replaceAll('\r\n', '\n'), 'Updater source differs from pinned Git revision');
  evidence.updaterSourceSha256 = sha256(original);
  fs.cpSync(source, fixture, { recursive: true });
  const manifest = parse(fs.readFileSync(pkg.manifest_path, 'utf8'));
  const shared = parse(fs.readFileSync(path.join(workspace, 'Cargo.toml'), 'utf8')).workspace;
  for (const [key, value] of Object.entries(manifest.package)) {
    if (value?.workspace) { assert(shared.package[key] !== undefined); manifest.package[key] = shared.package[key]; }
  }
  for (const table of [manifest, ...Object.values(manifest.target ?? {})]) {
    for (const kind of ['dependencies', 'dev-dependencies', 'build-dependencies']) {
      for (const [name, value] of Object.entries(table[kind] ?? {})) {
        if (!value.workspace) continue;
        let inherited = shared.dependencies[name]; assert(inherited, `Missing workspace dependency ${name}`);
        if (typeof inherited === 'string') inherited = { version: inherited };
        const merged = { ...inherited, ...value, features: [...new Set([...(inherited.features ?? []), ...(value.features ?? [])])] };
        delete merged.workspace; table[kind][name] = merged;
      }
    }
  }
  // Match the app's unified ZIP default features, including Deflate.
  manifest['dev-dependencies'].zip = '=9.0.5';
  // A local root's lock resolves every declared feature. The application uses
  // only the defaults; omit unused TLS alternatives from this fixture so it
  // cannot resolve new vendored OpenSSL packages outside the application lock.
  assert.deepEqual(manifest.features.default, ['rustls-tls', 'system-proxy', 'zip']);
  delete manifest.features['native-tls'];
  delete manifest.features['native-tls-vendored'];
  manifest.workspace = { resolver: '3' };
  fs.writeFileSync(path.join(fixture, 'Cargo.toml'), stringify(manifest));
  fs.copyFileSync(path.join(root, 'src-tauri/Cargo.lock'), path.join(fixture, 'Cargo.lock'));
  const marker = 'mod offline_tests {';
  assert.equal(original.split(marker).length, 2, 'Upstream test module changed');
  let instrumented = original.replace(marker, `${marker}\n    include!("note_taker_security_regressions.rs");`);
  // Fail if a signature regression ever reaches installation. Only the fixture
  // is instrumented; extraction and cryptographic verification remain original.
  const install = 'fn install_inner(&self, bytes: &[u8]) -> Result<()> {';
  assert(instrumented.includes(install), 'Installation boundary changed');
  instrumented = instrumented.replaceAll(install, `${install}\n        assert!(std::hint::black_box(false), "installer execution forbidden in regression fixture");`);
  fs.writeFileSync(path.join(fixture, 'src/updater.rs'), instrumented);
  fs.copyFileSync(path.join(root, 'scripts/security/updater-regressions.rs'), path.join(fixture, 'src/note_taker_security_regressions.rs'));
  const fixtureMetadata = JSON.parse(run(['metadata', '--offline', '--format-version', '1'], 'fixture-metadata', fixture));
  const appIds = new Set(metadata.packages.map(p => `${p.name}@${p.version}:${p.source}`));
  evidence.fixtureOnlyPackages = fixtureMetadata.packages.filter(p => p.source && !appIds.has(`${p.name}@${p.version}:${p.source}`)).map(p => ({ name: p.name, version: p.version, source: p.source }));
  assert.equal(evidence.fixtureOnlyPackages.length, 0, 'Fixture resolved packages outside the application lock; review before testing');
  evidence.fixtureLockSha256 = sha256(fs.readFileSync(path.join(fixture, 'Cargo.lock')));
  run(['test', '--locked', '--offline', '--lib', 'offline_tests', '--', '--test-threads=1'], 'tests', fixture);
  evidence.status = 'passed';
  console.log(`Updater archive/signature regressions passed. Reports: ${out}`);
} catch (error) {
  evidence.error = error.message;
  console.error(error.message); process.exitCode = 1;
} finally {
  evidence.completedAt = new Date().toISOString();
  fs.writeFileSync(path.join(out, 'evidence.json'), JSON.stringify(evidence, null, 2));
}
