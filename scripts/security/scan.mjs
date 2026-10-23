import fs from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { parse as parseYaml } from 'yaml';
import { parse as parseToml } from 'smol-toml';
import { collectWindowsGraph, selectWindowsScope } from './windows-scope.mjs';

export const scannerVersion = '2.7.0';
export const scannerPins = {
  'win32-x64': ['osv-scanner_windows_amd64.exe', '9964ed64ec9a387391eb4cc006ad5c531154821aef29b793e02c448de4686438'],
  'linux-x64': ['osv-scanner_linux_amd64', '160c6253e2922e9e0a6a9771c5c4c40a4b3c6090b34a3cdd5e80c96cc2e4218b'],
};
export const lockfiles = ['pnpm-lock.yaml', 'src-tauri/Cargo.lock'];
export const updaterPin = {
  name: 'tauri-plugin-updater', version: '2.14.0',
  git: 'https://github.com/tauri-apps/plugins-workspace',
  rev: 'ba98bfadc4c868c9a7e1a226c83af528e1f11b89',
};
export const updaterSource = `git+${updaterPin.git}?rev=${updaterPin.rev}#${updaterPin.rev}`;
const manifests = ['package.json', 'src-tauri/Cargo.toml'];
const read = file => fs.readFileSync(file, 'utf8').replace(/^\uFEFF/, '');
export const sha256 = value => createHash('sha256').update(value).digest('hex');
const key = p => `${p.ecosystem}:${p.name}@${p.version}`;
const semver = /^\d+\.\d+\.\d+(?:-[\w.-]+)?(?:\+[\w.-]+)?$/;
const assert = (ok, message) => { if (!ok) throw new Error(message); };

function npmPackage(value) {
  const id = value.split('(')[0];
  const at = id.lastIndexOf('@');
  const name = id.slice(0, at), version = id.slice(at + 1);
  assert(at > 0 && semver.test(version), `Unsupported pnpm package identity: ${value}`);
  return { ecosystem: 'npm', name, version };
}

export function inventory(root) {
  const pnpm = parseYaml(read(path.join(root, lockfiles[0])), { maxAliasCount: 0, uniqueKeys: true });
  assert(String(pnpm.lockfileVersion) === '9.0', 'Expected pnpm lockfile version 9.0');
  assert(pnpm.importers && Object.keys(pnpm.importers).join() === '.', 'Unexpected pnpm workspace: extend scan coverage first');
  assert(pnpm.packages && pnpm.snapshots, 'Missing pnpm package inventory');
  const packages = Object.keys(pnpm.packages).map(npmPackage);
  assert(packages.length > 0, 'Empty npm inventory');
  const npmKeys = new Set(packages.map(key));
  const snapshotKeys = new Set(Object.keys(pnpm.snapshots).map(id => key(npmPackage(id))));
  assert(npmKeys.size === packages.length, 'Duplicate npm package identity');
  assert(npmKeys.size === snapshotKeys.size && [...npmKeys].every(k => snapshotKeys.has(k)), 'pnpm packages/snapshots coverage mismatch');
  for (const [id, info] of Object.entries(pnpm.packages)) {
    assert(typeof info.resolution?.integrity === 'string', `Unpinned or unsupported npm source: ${id}`);
  }
  const snapshotIds = new Set(Object.keys(pnpm.snapshots));
  for (const [id, snapshot] of Object.entries(pnpm.snapshots)) {
    for (const kind of ['dependencies', 'optionalDependencies']) {
      for (const [name, version] of Object.entries(snapshot[kind] ?? {})) {
        assert(typeof version === 'string' && snapshotIds.has(`${name}@${version}`), `Unresolved pnpm dependency: ${id} -> ${name}@${version}`);
      }
    }
  }
  const manifest = JSON.parse(read(path.join(root, 'package.json')));
  const importer = pnpm.importers['.'];
  for (const kind of ['dependencies', 'devDependencies', 'optionalDependencies']) {
    const declared = manifest[kind] ?? {}, locked = importer[kind] ?? {};
    assert(Object.keys(declared).length === Object.keys(locked).length, `Stale pnpm ${kind}`);
    for (const [name, spec] of Object.entries(declared)) {
      assert(locked[name]?.specifier === spec, `Stale pnpm specifier: ${name}`);
      assert(npmKeys.has(key(npmPackage(`${name}@${locked[name].version}`))), `Missing imported package: ${name}`);
      assert(snapshotIds.has(`${name}@${locked[name].version}`), `Missing imported snapshot: ${name}`);
    }
  }
  const cargo = parseToml(read(path.join(root, lockfiles[1])));
  assert(cargo.version === 4 && Array.isArray(cargo.package) && cargo.package.length > 0, 'Expected nonempty Cargo v4 lockfile');
  const cargoManifest = parseToml(read(path.join(root, manifests[1])));
  const declaredGit = [cargoManifest, ...Object.values(cargoManifest.target ?? {})]
    .flatMap(table => ['dependencies', 'dev-dependencies', 'build-dependencies']
      .flatMap(kind => Object.entries(table[kind] ?? {}).filter(([, dep]) => dep.git)));
  for (const [name, dep] of declaredGit) {
    assert(name === updaterPin.name && dep.git === updaterPin.git && dep.rev === updaterPin.rev && dep.version === updaterPin.version,
      `Unsupported or unpinned Cargo Git declaration: ${name}`);
    assert(cargo.package.some(p => p.name === name && p.version === updaterPin.version && p.source === updaterSource), 'Cargo Git pin missing from lockfile');
  }
  const crates = cargo.package.map(p => {
    assert(typeof p.name === 'string' && semver.test(p.version), 'Invalid Cargo package identity');
    if (p.source?.startsWith('git+')) {
      assert(p.name === updaterPin.name && p.version === updaterPin.version && p.source === updaterSource && declaredGit.length === 1,
        `Unsupported or unpinned Cargo source: ${p.name}`);
    } else if (p.source) {
      assert(p.source === 'registry+https://github.com/rust-lang/crates.io-index' && /^[a-f0-9]{64}$/.test(p.checksum), `Unsupported or unpinned Cargo source: ${p.name}`);
    } else {
      assert(p.name === cargoManifest.package.name && p.version === cargoManifest.package.version, `Unexpected local Cargo package: ${p.name}`);
    }
    return { ecosystem: 'crates.io', name: p.name, version: p.version };
  });
  assert(new Set(crates.map(key)).size === crates.length, 'Duplicate Cargo package identity');
  assert(cargo.package.some(p => !p.source && p.name === cargoManifest.package.name), 'Missing root Cargo package');
  for (const p of cargo.package) {
    assert(p.dependencies === undefined || Array.isArray(p.dependencies), `Malformed Cargo dependencies: ${p.name}`);
    for (const dependency of p.dependencies ?? []) {
      assert(typeof dependency === 'string', 'Malformed Cargo dependency identity');
      const match = /^([\w-]+)(?: ([^ ]+))?(?: \(([^)]+)\))?$/.exec(dependency);
      assert(match, `Unsupported Cargo dependency identity: ${dependency}`);
      const [, name, version, source] = match;
      const matches = cargo.package.filter(item => item.name === name && (!version || item.version === version) && (!source || item.source === source));
      assert(matches.length === 1, `Unresolved or ambiguous Cargo dependency: ${p.name} -> ${dependency}`);
    }
  }
  return { [lockfiles[0]]: packages, [lockfiles[1]]: crates };
}

// Cargo's OSV extractor identifies Git crates by package version. Query the
// immutable commit as well; neither lookup replaces or filters the other.
export async function queryCommit(commit, fetchImpl = fetch, onPage = () => {}) {
  assert(/^[a-f0-9]{40}$/.test(commit), 'Invalid Git commit');
  const vulnerabilities = [], seen = new Set();
  let token;
  for (let page = 0; page < 20; page++) {
    const response = await fetchImpl('https://api.osv.dev/v1/query', {
      method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ commit, ...(token ? { page_token: token } : {}) }),
      signal: AbortSignal.timeout(60000),
    });
    const raw = await response.text();
    onPage(page, raw);
    assert(response.ok, `Git commit query failed: HTTP ${response.status}`);
    const body = JSON.parse(raw);
    assert(body && typeof body === 'object' && !Array.isArray(body) && !body.error && !body.code &&
      Object.keys(body).every(k => ['vulns', 'next_page_token'].includes(k)), 'Malformed Git query response');
    assert(body.vulns === undefined || Array.isArray(body.vulns), 'Malformed Git query findings');
    for (const v of body.vulns ?? []) {
      assert(typeof v.id === 'string' && v.id && Array.isArray(v.affected), 'Incomplete Git advisory');
      vulnerabilities.push(v);
    }
    token = body.next_page_token;
    if (token === undefined || token === '') return vulnerabilities;
    assert(typeof token === 'string' && !seen.has(token), 'Invalid or repeated Git query pagination');
    seen.add(token);
  }
  throw new Error('Incomplete Git query pagination');
}

export function addGitFindings(report, findings) {
  const merged = structuredClone(report);
  const matches = merged.results.flatMap(r => r.packages).filter(p => p.package.ecosystem === 'crates.io' && p.package.name === updaterPin.name && p.package.version === updaterPin.version);
  assert(matches.length === 1, 'Missing or ambiguous Git package report');
  const entry = matches[0];
  entry.vulnerabilities = [...new Map([...(entry.vulnerabilities ?? []), ...findings].map(v => [v.id, v])).values()];
  return merged;
}

export function checkScope(files) {
  const covered = new Set([...lockfiles, ...manifests]);
  const dependencyFile = /(?:^|\/)(?:package\.json|(?:pnpm-lock\.yaml|package-lock\.json|npm-shrinkwrap\.json|yarn\.lock|bun\.lockb?|Cargo\.(?:lock|toml)|go\.(?:mod|sum)|requirements[^/]*\.txt|pyproject\.toml|poetry\.lock|uv\.lock|Pipfile(?:\.lock)?|Gemfile(?:\.lock)?|composer\.(?:json|lock)|pom\.xml|build\.gradle(?:\.kts)?|gradle\.lockfile|packages\.lock\.json|[^/]+\.csproj))$/;
  for (const file of files) {
    assert(!/(?:^|\/)osv-scanner\.(?:toml|json)$/i.test(file), `Unexpected OSV configuration: ${file}`);
    assert(!dependencyFile.test(file) || covered.has(file), `Dependency file outside declared scan coverage: ${file}`);
  }
  for (const file of covered) assert(files.includes(file), `Missing dependency file: ${file}`);
}

export function validateReport(report, expected, root, exitCode, log = '') {
  assert(exitCode === 0 || exitCode === 1, `Scanner error exit: ${exitCode}`);
  assert(!/\b(error|warning|failed|panic|incomplete)\b/i.test(log), 'Scanner diagnostics require investigation');
  assert(Array.isArray(report.results) && report.results.length === lockfiles.length, 'Missing or unexpected scan sources');
  assert(!report.errors?.length, 'Scanner reported errors');
  const seen = new Set(), vulnerable = [], parent = new Map();
  const find = id => { if (!parent.has(id)) parent.set(id, id); if (parent.get(id) !== id) parent.set(id, find(parent.get(id))); return parent.get(id); };
  for (const result of report.results) {
    assert(typeof result.source?.path === 'string', 'Missing report source path');
    const source = path.relative(root, path.resolve(root, result.source.path)).replaceAll('\\', '/');
    assert(expected[source] && !seen.has(source), `Unexpected or duplicate source: ${source}`);
    seen.add(source);
    assert(Array.isArray(result.packages), `Missing packages: ${source}`);
    const wanted = new Set(expected[source].map(key)), actual = new Set();
    for (const entry of result.packages) {
      assert(entry.package && wanted.has(key(entry.package)), `Unexpected package in ${source}: ${JSON.stringify(entry.package)}`);
      assert(!actual.has(key(entry.package)), `Duplicate report package: ${key(entry.package)}`);
      actual.add(key(entry.package));
      assert(entry.vulnerabilities === undefined || Array.isArray(entry.vulnerabilities), 'Malformed vulnerability list');
      if (entry.vulnerabilities?.length) {
        for (const vuln of entry.vulnerabilities) {
          assert(typeof vuln.id === 'string' && vuln.id.length > 0, 'Missing advisory ID');
          assert(vuln.aliases === undefined || Array.isArray(vuln.aliases), 'Malformed advisory aliases');
          const ids = [vuln.id, ...(vuln.aliases ?? [])];
          for (const id of ids) { assert(typeof id === 'string', 'Invalid advisory alias'); parent.set(find(id), find(ids[0])); }
        }
        vulnerable.push({ source, ...entry.package, ids: entry.vulnerabilities.map(v => v.id) });
      }
      assert(!entry.groups?.length || entry.vulnerabilities?.length, 'Advisory group without full vulnerability data');
    }
    assert(actual.size === wanted.size, `Incomplete package coverage in ${source}: ${actual.size}/${wanted.size}`);
  }
  const uniqueAdvisories = new Set(vulnerable.flatMap(p => p.ids.map(find))).size;
  assert(exitCode === (uniqueAdvisories ? 1 : 0), 'Scanner exit and report findings disagree');
  return { coverageComplete: true, uniqueAdvisories, affectedPackageVersions: vulnerable.length, packages: Object.fromEntries(Object.entries(expected).map(([s,p]) => [s,p.length])), vulnerable };
}

export function verifyBinary(bytes, expected) {
  assert(sha256(bytes) === expected, 'OSV scanner SHA256 mismatch');
}

async function scan() {
  const root = path.resolve(fileURLToPath(new URL('../../', import.meta.url)));
  const args = process.argv.slice(2);
  const windows = args[0] === '--windows';
  if (windows) args.shift();
  assert(args.length <= 1 && !args[0]?.startsWith('--'), 'Usage: scan.mjs [--windows] [empty-report-directory]');
  const out = path.resolve(args[0] ?? path.join(root, 'output/security', new Date().toISOString().replaceAll(':', '-')));
  // A fresh report directory prevents stale clean reports from surviving a failed run.
  fs.mkdirSync(out, { recursive: true });
  assert(fs.readdirSync(out).length === 0, `Report directory must be empty: ${out}`);
  const write = (name, value) => fs.writeFileSync(path.join(out, name), value);
  const run = { startedAt: new Date().toISOString(), scannerVersion, gateScope: windows ? 'Windows normal/build/dev + all npm + pinned Git sources' : 'all locked dependencies', status: 'error' };
  try {
    const listing = spawnSync('git', ['ls-files', '-co', '--exclude-standard', '-z'], { cwd: root, encoding: 'utf8' });
    assert(listing.status === 0, 'Cannot enumerate repository dependency files');
    checkScope([...new Set(listing.stdout.split('\0').filter(Boolean))]);
    const before = Object.fromEntries([...lockfiles, ...manifests].map(p => [p, sha256(fs.readFileSync(path.join(root, p)))]));
    run.inputHashes = before;
    const expected = inventory(root);
    write('inventory.json', JSON.stringify(expected, null, 2));
    const pin = scannerPins[`${process.platform}-${process.arch}`];
    assert(pin, `Unsupported scanner platform: ${process.platform}-${process.arch}`);
    const [asset, digest] = pin;
    let binary = process.env.OSV_SCANNER_PATH;
    if (!binary) {
      binary = path.join(out, asset);
      const response = await fetch(`https://github.com/google/osv-scanner/releases/download/v${scannerVersion}/${asset}`, { signal: AbortSignal.timeout(120000) });
      assert(response.ok, `Scanner download failed: HTTP ${response.status}`);
      const bytes = Buffer.from(await response.arrayBuffer());
      verifyBinary(bytes, digest);
      fs.writeFileSync(binary, bytes, { mode: 0o755 });
    }
    binary = path.resolve(binary);
    verifyBinary(fs.readFileSync(binary), digest);
    run.scannerSha256 = digest;
    const version = spawnSync(binary, ['--version'], { encoding: 'utf8', timeout: 30000 });
    write('scanner-version.txt', `${version.stdout ?? ''}${version.stderr ?? ''}`);
    assert(version.status === 0 && version.stdout.includes(`osv-scanner version: ${scannerVersion}`), 'Cannot verify OSV scanner version');
    const config = path.join(out, 'empty-config.toml');
    write('empty-config.toml', '# Deliberately empty: no advisory ignores or severity filters.\n');
    const args = ['scan', 'source', ...lockfiles.flatMap(p => ['--lockfile', p]), '--config', config, '--all-packages', '--all-vulns', '--no-call-analysis', 'rust', '--no-call-analysis', 'go', '--format', 'json', '--output-file', path.join(out, 'osv.json')];
    run.args = args;
    const result = spawnSync(binary, args, { cwd: root, encoding: 'utf8', timeout: 600000, maxBuffer: 64 * 1024 * 1024 });
    run.scannerExitCode = result.status;
    write('stdout.log', result.stdout ?? '');
    write('stderr.log', result.stderr ?? '');
    assert(!result.error && !result.signal, `Scanner did not complete: ${result.error?.message ?? result.signal}`);
    for (const [p, hash] of Object.entries(before)) assert(sha256(fs.readFileSync(path.join(root,p))) === hash, `Dependency input changed during scan: ${p}`);
    let report = JSON.parse(read(path.join(out, 'osv.json')));
    let summary = validateReport(report, expected, root, result.status, `${result.stdout}\n${result.stderr}`);
    const gitPackages = parseToml(read(path.join(root, lockfiles[1]))).package.filter(p => p.source?.startsWith('git+'));
    const gitQueries = [];
    for (const p of gitPackages) {
      const findings = await queryCommit(updaterPin.rev, fetch, (page, raw) => write(`git-${p.name}-${page}.json`, raw));
      report = addGitFindings(report, findings);
      const hasFindings = report.results.some(r => r.packages.some(entry => entry.vulnerabilities?.length));
      summary = validateReport(report, expected, root, hasFindings ? 1 : 0);
      gitQueries.push({ name: p.name, source: p.source, commit: updaterPin.rev, complete: true, advisoryIds: findings.map(v => v.id) });
    }
    summary.gitQueries = gitQueries;
    write('combined-osv.json', JSON.stringify(report, null, 2));
    write('summary.json', JSON.stringify(summary, null, 2));
    run.fullScanStatus = summary.uniqueAdvisories ? 'findings' : 'clean';
    run.fullUniqueAdvisories = summary.uniqueAdvisories;
    let blocking = summary;
    if (windows) {
      const cargo = parseToml(read(path.join(root, lockfiles[1])));
      const graph = collectWindowsGraph(root, out, cargo.package, parseToml(read(path.join(root, manifests[1]))));
      const scope = selectWindowsScope(report, expected, graph, gitPackages);
      const hasFindings = scope.report.results.some(r => r.packages.some(p => p.vulnerabilities?.length));
      blocking = validateReport(scope.report, scope.inventory, root, hasFindings ? 1 : 0);
      write('windows-scope.json', JSON.stringify({ ...blocking, target: graph.target, featureSelection: graph.featureSelection, edgeKinds: graph.edgeKinds, classification: scope.classification }, null, 2));
      write('windows-osv.json', JSON.stringify(scope.report, null, 2));
      const details = ['# Windows dependency gate', '', `Blocking advisory groups: ${blocking.uniqueAdvisories}. Full cross-platform advisory groups retained: ${summary.uniqueAdvisories}.`, '',
        'Scope: x86_64-pc-windows-msvc normal/build/dev graph (all workspace features), all npm packages and pinned Git sources.', '',
        ...scope.classification.map(p => `- ${p.name}@${p.version}: ${p.blocking ? 'BLOCKING' : 'NONBLOCKING'}; ${p.reason}; ${p.ids.join(', ')}`), '', 'Full raw reports and graph evidence are retained as workflow artifacts.', ''].join('\n');
      write('windows-summary.md', details);
      if (process.env.GITHUB_STEP_SUMMARY) fs.appendFileSync(process.env.GITHUB_STEP_SUMMARY, details);
    }
    for (const [p, hash] of Object.entries(before)) assert(sha256(fs.readFileSync(path.join(root,p))) === hash, `Dependency input changed during scan: ${p}`);
    run.blockingUniqueAdvisories = blocking.uniqueAdvisories;
    run.status = blocking.uniqueAdvisories ? 'findings' : summary.uniqueAdvisories ? 'windows-clean-with-other-platform-findings' : 'clean';
    console.log(`${blocking.uniqueAdvisories} blocking groups (${run.gateScope}); ${summary.uniqueAdvisories} full-report groups across ${summary.affectedPackageVersions} affected versions retained. Coverage verified. Reports: ${out}`);
    process.exitCode = blocking.uniqueAdvisories ? 1 : 0;
  } catch (error) {
    run.error = error.message;
    if (process.env.GITHUB_STEP_SUMMARY) fs.appendFileSync(process.env.GITHUB_STEP_SUMMARY, `\nSecurity gate failed closed: ${error.message}\n`);
    console.error(`Security scan failed closed: ${error.message}. Reports: ${out}`);
    process.exitCode = 2;
  } finally {
    run.completedAt = new Date().toISOString();
    write('run.json', JSON.stringify(run, null, 2));
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) await scan();
