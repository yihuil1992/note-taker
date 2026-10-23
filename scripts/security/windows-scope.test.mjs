import test from 'node:test';
import assert from 'node:assert/strict';
import path from 'node:path';
import { validateWindowsGraph, selectWindowsScope } from './windows-scope.mjs';
import { validateReport, updaterPin, updaterSource } from './scan.mjs';

const root = path.resolve('graph-fixture');
const registry = 'registry+https://github.com/rust-lang/crates.io-index';
function fixture() {
  const locked = [
    { name: 'app', version: '1.0.0' },
    ...['runtime', 'builder', 'devtool', 'linux-only'].map(name => ({ name, version: '1.0.0', source: registry })),
    { name: updaterPin.name, version: updaterPin.version, source: updaterSource },
  ];
  const packages = locked.filter(p => p.name !== 'linux-only').map(p => ({ ...p, source: p.source ?? null, id: p.name }));
  const kinds = [null, 'build', 'dev', null];
  const deps = packages.slice(1).map((p, i) => ({ name: p.name, pkg: p.id, dep_kinds: [{ kind: kinds[i], target: null }] }));
  const metadata = { version: 1, packages, workspace_members: ['app'], resolve: { root: 'app', nodes: packages.map(p => ({ id: p.id, features: [], dependencies: p.id === 'app' ? deps.map(d => d.pkg) : [], deps: p.id === 'app' ? deps : [] })) } };
  const tree = packages.map(p => `${p.name} v${p.version}`).join('\n');
  const manifest = { package: { name: 'app', version: '1.0.0' } };
  const expected = { 'pnpm-lock.yaml': [{ ecosystem: 'npm', name: 'frontend-devtool', version: '1.0.0' }],
    'src-tauri/Cargo.lock': locked.map(p => ({ ecosystem: 'crates.io', name: p.name, version: p.version })) };
  const report = { results: Object.entries(expected).map(([source, packages]) => ({ source: { path: path.join(root, source) }, packages: packages.map(p => ({ package: p })) })) };
  return { locked, metadata, tree, manifest, expected, report };
}
const graph = f => validateWindowsGraph(f.metadata, f.tree, f.locked, f.manifest);
const scope = f => selectWindowsScope(f.report, f.expected, graph(f), f.locked.filter(p => p.source?.startsWith('git+')));
const evaluate = s => validateReport(s.report, s.inventory, root, s.classification.some(p => p.blocking) ? 1 : 0);
function addFinding(f, name, id = 'GHSA-fixture') {
  f.report.results.flatMap(r => r.packages).find(p => p.package.name === name).vulnerabilities = [{ id, aliases: ['CVE-fixture'] }];
}

test('Windows graph includes normal, build and dev edges with exact source/version coverage', () => {
  const f = fixture(), g = graph(f);
  assert.equal(g.packages.length, 5);
  assert.deepEqual(g.edgeCounts, { normal: 2, build: 1, dev: 1 });
});
test('runtime, build, dev, npm tooling and pinned Git findings all block', () => {
  for (const name of ['runtime', 'builder', 'devtool', 'frontend-devtool', updaterPin.name]) {
    const f = fixture(); addFinding(f, name);
    const s = scope(f);
    assert.equal(evaluate(s).uniqueAdvisories, 1, name);
    assert.equal(s.classification[0].blocking, true);
  }
});
test('other-platform findings stay in the unchanged full report and classification', () => {
  const f = fixture(); addFinding(f, 'linux-only');
  const before = JSON.stringify(f.report), s = scope(f);
  assert.equal(evaluate(s).uniqueAdvisories, 0);
  assert.equal(s.classification[0].blocking, false);
  assert.match(s.classification[0].reason, /retained/);
  assert.equal(JSON.stringify(f.report), before);
  assert.equal(validateReport(f.report, f.expected, root, 1).uniqueAdvisories, 1);
});
test('metadata feature overapproximation remains blocking even when cargo tree omits it', () => {
  const f = fixture(); f.tree = f.tree.split('\n').filter(line => !line.startsWith('runtime ')).join('\n');
  addFinding(f, 'runtime');
  assert.deepEqual(graph(f).metadataOnlyPackages, [{ name: 'runtime', version: '1.0.0' }]);
  assert.equal(evaluate(scope(f)).uniqueAdvisories, 1);
});
test('pinned Git remains blocking even if absent from the Windows graph', () => {
  const f = fixture(), g = graph(f);
  g.packages = g.packages.filter(p => p.name !== updaterPin.name);
  addFinding(f, updaterPin.name);
  const s = selectWindowsScope(f.report, f.expected, g, f.locked.filter(p => p.source?.startsWith('git+')));
  assert.equal(evaluate(s).uniqueAdvisories, 1);
  assert.equal(s.classification[0].reason, 'pinned Git source');
});
test('mixed-platform aliases do not hide Windows exposure or double-count advisories', () => {
  const f = fixture(); addFinding(f, 'linux-only', 'RUSTSEC-fixture'); addFinding(f, 'builder');
  const s = scope(f);
  assert.equal(evaluate(s).uniqueAdvisories, 1);
  assert.equal(s.classification.filter(p => p.blocking).length, 1);
  assert.equal(validateReport(f.report, f.expected, root, 1).uniqueAdvisories, 1);
});
test('missing, duplicated, dangling and unreachable graph nodes fail closed', () => {
  const mutations = [
    f => f.metadata.resolve.nodes.pop(),
    f => f.metadata.resolve.nodes.push(f.metadata.resolve.nodes[0]),
    f => f.metadata.packages.push(f.metadata.packages[0]),
    f => f.metadata.resolve.nodes[0].deps[0].pkg = 'missing',
    f => { f.metadata.resolve.nodes[0].deps.pop(); f.metadata.resolve.nodes[0].dependencies.pop(); },
    f => f.metadata.resolve.nodes[0].deps[0].dep_kinds = [],
    f => f.metadata.resolve.nodes[0].deps[0].dep_kinds[0].kind = 'unknown',
  ];
  for (const mutate of mutations) { const f = fixture(); mutate(f); assert.throws(() => graph(f)); }
});
test('wrong source/version/root and metadata/tree disagreement fail closed', () => {
  for (const mutate of [
    f => f.metadata.packages[1].source = 'git+https://example.invalid',
    f => f.metadata.packages[1].version = '2.0.0',
    f => f.metadata.workspace_members.push('other'),
    f => f.manifest.package.name = 'wrong',
    f => f.tree = f.tree.split('\n').slice(1).join('\n'),
    f => f.tree += '\nmissing v1.0.0',
    f => f.tree += '\nwarning: truncated output',
  ]) { const f = fixture(); mutate(f); assert.throws(() => graph(f)); }
});
test('scope cannot pass with unverified target/features/edges or unscanned nodes', () => {
  for (const mutate of [
    g => g.coverageComplete = false, g => g.target = 'x86_64-unknown-linux-gnu',
    g => g.edgeKinds = ['normal'], g => g.featureSelection = 'unknown',
    g => g.packages.push({ name: 'missing', version: '1.0.0' }),
  ]) { const f = fixture(), g = graph(f); mutate(g); assert.throws(() => selectWindowsScope(f.report, f.expected, g, [])); }
});
