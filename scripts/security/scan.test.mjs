import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { inventory, validateReport, checkScope, verifyBinary, sha256, updaterPin, updaterSource, queryCommit, addGitFindings } from './scan.mjs';

const root = path.resolve('security-fixture');
const expected = {
  'pnpm-lock.yaml': [{ ecosystem: 'npm', name: 'example', version: '1.0.0' }],
  'src-tauri/Cargo.lock': [{ ecosystem: 'crates.io', name: 'example', version: '1.0.0' }],
};
const clean = () => ({ results: Object.entries(expected).map(([source, packages]) => ({ source: { path: path.join(root, source) }, packages: packages.map(p => ({ package: { ...p } })) })) });
const evaluate = (report, code = 0, log = '') => validateReport(report, expected, root, code, log);
test('complete clean report passes', () => assert.equal(evaluate(clean()).uniqueAdvisories, 0));
test('all severity findings block, deduplicating aliases across sources', () => {
  const r = clean();
  r.results[0].packages[0].vulnerabilities = [{ id: 'GHSA-example', aliases: ['CVE-example'] }];
  r.results[1].packages[0].vulnerabilities = [{ id: 'RUSTSEC-example', aliases: ['GHSA-example'] }];
  assert.deepEqual([evaluate(r, 1).uniqueAdvisories, evaluate(r, 1).affectedPackageVersions], [1, 2]);
  assert.throws(() => evaluate(r, 0), /disagree/);
});
test('scanner error, signal-like null status and inconsistent exit fail', () => {
  for (const code of [2, 3, 127, null, 1]) assert.throws(() => evaluate(clean(), code));
});
test('partial results cannot pass even when scanner exits zero', () => {
  const r = clean(); r.results[0].packages = [];
  assert.throws(() => evaluate(r), /Incomplete/);
  r.results.pop(); assert.throws(() => evaluate(r), /sources/);
});
test('duplicate sources and package identities fail', () => {
  const r = clean(); r.results[1] = structuredClone(r.results[0]);
  assert.throws(() => evaluate(r), /duplicate source/);
  const p = clean(); p.results[0].packages.push(p.results[0].packages[0]);
  assert.throws(() => evaluate(p), /Duplicate report/);
});
test('wrong package version, source and ecosystem fail', () => {
  for (const field of ['version', 'ecosystem']) {
    const r = clean(); r.results[0].packages[0].package[field] = 'wrong';
    assert.throws(() => evaluate(r), /Unexpected package/);
  }
  const r = clean(); r.results[0].source.path = path.resolve('different/pnpm-lock.yaml');
  assert.throws(() => evaluate(r), /source/);
});
test('malformed report and incomplete finding details fail', () => {
  assert.throws(() => evaluate({}));
  const r = clean(); r.results[0].packages[0].vulnerabilities = [{}];
  assert.throws(() => evaluate(r, 1), /advisory ID/);
  const g = clean(); g.results[0].packages[0].groups = [{ ids: ['GHSA-example'] }];
  assert.throws(() => evaluate(g), /without full/);
});
test('scanner diagnostics fail closed', () => {
  for (const log of ['ERROR: API unavailable', 'Warning: one extractor failed', 'panic: decode']) assert.throws(() => evaluate(clean(), 0, log), /diagnostics/);
});
const files = ['package.json', 'pnpm-lock.yaml', 'src-tauri/Cargo.toml', 'src-tauri/Cargo.lock'];
test('new dependency files, missing lockfiles and ignore configurations fail', () => {
  checkScope(files);
  for (const file of ['other/Cargo.lock', 'requirements-dev.txt', 'sub/package.json', 'osv-scanner.toml', 'src-tauri/osv-scanner.json']) assert.throws(() => checkScope([...files,file]));
  assert.throws(() => checkScope(files.slice(1)), /Missing/);
});
test('binary checksum mismatch blocks execution', () => {
  const binary = Buffer.from('test');
  verifyBinary(binary, sha256(binary));
  assert.throws(() => verifyBinary(Buffer.from('tampered'), sha256(binary)), /SHA256/);
});

function fixture(t) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'note-taker-security-test-'));
  t.after(() => {
    assert.equal(path.dirname(path.resolve(dir)), path.resolve(os.tmpdir()));
    assert.ok(path.basename(dir).startsWith('note-taker-security-test-'));
    fs.rmSync(dir, { recursive: true, force: true });
  });
  fs.mkdirSync(path.join(dir,'src-tauri'));
  fs.writeFileSync(path.join(dir,'package.json'), JSON.stringify({ dependencies: { example: '^1.0.0' } }));
  fs.writeFileSync(path.join(dir,'pnpm-lock.yaml'), "lockfileVersion: '9.0'\nimporters:\n  .:\n    dependencies:\n      example:\n        specifier: ^1.0.0\n        version: 1.0.0\npackages:\n  example@1.0.0:\n    resolution: {integrity: sha512-example}\nsnapshots:\n  example@1.0.0: {}\n");
  fs.writeFileSync(path.join(dir,'src-tauri/Cargo.toml'), '[package]\nname = "example"\nversion = "1.0.0"\n');
  fs.writeFileSync(path.join(dir,'src-tauri/Cargo.lock'), 'version = 4\n[[package]]\nname = "example"\nversion = "1.0.0"\n');
  return dir;
}
test('independent parsers produce exact package identities', t => assert.deepEqual(inventory(fixture(t)), expected));
test('stale importer and missing snapshot fail', t => {
  const dir = fixture(t), file = path.join(dir,'pnpm-lock.yaml'), content = fs.readFileSync(file,'utf8');
  fs.writeFileSync(file,content.replace('specifier: ^1.0.0','specifier: ^2.0.0'));
  assert.throws(() => inventory(dir), /Stale/);
  fs.writeFileSync(file,content.replace('snapshots:\n  example@1.0.0: {}','snapshots: {}'));
  assert.throws(() => inventory(dir), /coverage mismatch/);
});
test('duplicate YAML keys and malformed TOML fail', t => {
  const dir = fixture(t);
  fs.appendFileSync(path.join(dir,'pnpm-lock.yaml'), 'snapshots: {}\n');
  assert.throws(() => inventory(dir));
  const other = fixture(t); fs.appendFileSync(path.join(other,'src-tauri/Cargo.lock'), 'invalid = [');
  assert.throws(() => inventory(other));
});
test('unscannable dependency sources fail', t => {
  const dir = fixture(t);
  fs.appendFileSync(path.join(dir,'src-tauri/Cargo.lock'), 'source = "git+https://example.invalid/crate"\n');
  assert.throws(() => inventory(dir), /Unsupported or unpinned/);
});
test('missing transitive nodes fail even when inventory counts would match', t => {
  const dir = fixture(t), file = path.join(dir,'pnpm-lock.yaml');
  fs.writeFileSync(file, fs.readFileSync(file,'utf8').replace('example@1.0.0: {}', 'example@1.0.0:\n    dependencies:\n      missing: 2.0.0'));
  assert.throws(() => inventory(dir), /Unresolved pnpm dependency/);
  const other = fixture(t);
  fs.appendFileSync(path.join(other,'src-tauri/Cargo.lock'), 'dependencies = ["missing 2.0.0"]\n');
  assert.throws(() => inventory(other), /Unresolved or ambiguous Cargo/);
});

test('only the reviewed official Git revision is accepted, matching both manifest and lock', t => {
  const dir = fixture(t), lock = path.join(dir, 'src-tauri/Cargo.lock'), manifest = path.join(dir, 'src-tauri/Cargo.toml');
  fs.appendFileSync(manifest, `\n[dependencies]\n${updaterPin.name} = { version = "${updaterPin.version}", git = "${updaterPin.git}", rev = "${updaterPin.rev}" }\n`);
  assert.throws(() => inventory(dir), /missing from lockfile/);
  fs.appendFileSync(lock, `\n[[package]]\nname = "${updaterPin.name}"\nversion = "${updaterPin.version}"\nsource = "${updaterSource}"\n`);
  assert.equal(inventory(dir)['src-tauri/Cargo.lock'].length, 2);
  fs.writeFileSync(lock, fs.readFileSync(lock, 'utf8').replaceAll(updaterPin.rev, 'a'.repeat(40)));
  assert.throws(() => inventory(dir), /missing from lockfile/);
});

const response = body => ({ ok: true, text: async () => JSON.stringify(body) });
test('commit query retains all pages and validates the immutable query', async () => {
  const pages = [], requests = [];
  const findings = await queryCommit(updaterPin.rev, async (_url, options) => {
    requests.push(JSON.parse(options.body));
    return response(requests.length === 1 ? { vulns: [{ id: 'GHSA-git', affected: [] }], next_page_token: 'page2' } : {});
  }, (page, raw) => pages.push([page, JSON.parse(raw)]));
  assert.equal(findings.length, 1); assert.equal(pages.length, 2);
  assert.deepEqual(requests, [{ commit: updaterPin.rev }, { commit: updaterPin.rev, page_token: 'page2' }]);
});
test('commit query fails closed on network, HTTP, malformed or incomplete results', async () => {
  for (const body of [null, [], { error: 'unavailable' }, { vulns: {} }, { vulns: [{}] }, { unexpected: true }]) {
    await assert.rejects(queryCommit(updaterPin.rev, async () => response(body)));
  }
  await assert.rejects(queryCommit(updaterPin.rev, async () => { throw new Error('offline'); }), /offline/);
  await assert.rejects(queryCommit(updaterPin.rev, async () => ({ ...response({}), ok: false, status: 503 })), /HTTP 503/);
  await assert.rejects(queryCommit(updaterPin.rev, async () => response({ next_page_token: 'loop' })), /repeated/);
});
test('Git-only findings block the gate and aliases do not double count', () => {
  const p = { ecosystem: 'crates.io', name: updaterPin.name, version: updaterPin.version };
  const e = structuredClone(expected); e['src-tauri/Cargo.lock'] = [p];
  const r = clean(); r.results[1].packages = [{ package: p }];
  const merged = addGitFindings(r, [{ id: 'GHSA-git', aliases: ['CVE-git'], affected: [] }]);
  assert.throws(() => validateReport(merged, e, root, 0), /disagree/);
  assert.equal(validateReport(merged, e, root, 1).uniqueAdvisories, 1);
  assert.equal(r.results[1].packages[0].vulnerabilities, undefined);
});
