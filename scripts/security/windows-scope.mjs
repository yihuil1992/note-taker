import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';

export const windowsTarget = 'x86_64-pc-windows-msvc';
const key = p => `crates.io:${p.name}@${p.version}`;
const assert = (ok, message) => { if (!ok) throw new Error(message); };
const equalSets = (a, b) => a.size === b.size && [...a].every(x => b.has(x));

export function validateWindowsGraph(metadata, tree, locked, manifest) {
  assert(metadata.version === 1 && Array.isArray(metadata.packages) && metadata.packages.length > 0, 'Missing Cargo metadata packages');
  assert(metadata.resolve?.root && Array.isArray(metadata.resolve.nodes), 'Missing Cargo resolve graph');
  assert(metadata.workspace_members?.length === 1 && metadata.workspace_members[0] === metadata.resolve.root, 'Unexpected Cargo workspace/root');
  const packages = new Map(), identities = new Set();
  for (const p of metadata.packages) {
    assert(typeof p.id === 'string' && !packages.has(p.id) && !identities.has(key(p)), 'Duplicate Cargo graph identity');
    const matches = locked.filter(l => l.name === p.name && l.version === p.version && (l.source ?? null) === p.source);
    assert(matches.length === 1, `Graph package missing from exact lock/source: ${p.name}@${p.version}`);
    packages.set(p.id, p); identities.add(key(p));
  }
  const root = packages.get(metadata.resolve.root);
  assert(root && root.source === null && root.name === manifest.package.name && root.version === manifest.package.version, 'Wrong Cargo graph root');
  const nodes = new Map();
  const edgeCounts = { normal: 0, build: 0, dev: 0 };
  for (const node of metadata.resolve.nodes) {
    assert(packages.has(node.id) && !nodes.has(node.id), 'Missing or duplicate Cargo graph node');
    assert(Array.isArray(node.deps) && Array.isArray(node.dependencies) && Array.isArray(node.features), 'Incomplete Cargo graph node');
    assert(node.features.every(f => typeof f === 'string'), 'Invalid Cargo feature selection');
    const deps = new Set();
    for (const dep of node.deps) {
      assert(packages.has(dep.pkg) && !deps.has(dep.pkg), 'Dangling or duplicate Cargo dependency edge');
      assert(Array.isArray(dep.dep_kinds) && dep.dep_kinds.length > 0, 'Missing Cargo edge kinds');
      for (const edge of dep.dep_kinds) {
        assert([null, 'build', 'dev'].includes(edge.kind) && (edge.target === null || typeof edge.target === 'string'), 'Unsupported Cargo edge kind/target');
        edgeCounts[edge.kind ?? 'normal']++;
      }
      deps.add(dep.pkg);
    }
    assert(new Set(node.dependencies).size === node.dependencies.length && equalSets(deps, new Set(node.dependencies)), 'Cargo edge inventories disagree');
    nodes.set(node.id, node);
  }
  assert(equalSets(new Set(nodes.keys()), new Set(packages.keys())), 'Incomplete Cargo resolve nodes');
  const visited = new Set(), pending = [root.id];
  while (pending.length) {
    const id = pending.pop();
    if (visited.has(id)) continue;
    assert(nodes.has(id), 'Unresolved Cargo graph reference');
    visited.add(id); pending.push(...nodes.get(id).dependencies);
  }
  assert(visited.size === packages.size, 'Unreachable nodes in target-filtered Cargo metadata');
  const treeKeys = new Set();
  for (const line of tree.split(/\r?\n/).filter(Boolean)) {
    const match = /^([\w-]+) v([^ ]+)(?: \([^()]*\))*$/.exec(line);
    assert(match, `Unexpected cargo tree output: ${line}`);
    const identity = key({ name: match[1], version: match[2] });
    assert(identities.has(identity), `Tree package outside metadata: ${identity}`);
    treeKeys.add(identity);
  }
  assert(treeKeys.has(key(root)), 'Incomplete Windows tree: root missing');
  // metadata can unify more features than resolver-2 cargo tree. Keep that
  // conservative superset blocking; never drop a metadata-only package.
  const metadataOnlyPackages = [...packages.values()].filter(p => !treeKeys.has(key(p))).map(p => ({ name: p.name, version: p.version }));
  return { coverageComplete: true, target: windowsTarget, edgeKinds: ['normal', 'build', 'dev'],
    featureSelection: 'all workspace features', edgeCounts, cargoTreePackageCount: treeKeys.size, metadataOnlyPackages,
    packages: [...packages.values()].map(p => ({ ecosystem: 'crates.io', name: p.name, version: p.version, source: p.source, features: nodes.get(p.id).features })) };
}

export function collectWindowsGraph(root, out, locked, manifest) {
  assert(process.platform === 'win32' && process.arch === 'x64', 'Windows gate requires a Windows x64 host to resolve build/proc-macro dependencies');
  const run = (command, args, name) => {
    const result = spawnSync(command, args, { cwd: root, encoding: 'utf8', timeout: 600000, maxBuffer: 64 * 1024 * 1024 });
    fs.writeFileSync(path.join(out, `${name}.stdout.log`), result.stdout ?? '');
    fs.writeFileSync(path.join(out, `${name}.stderr.log`), result.stderr ?? '');
    assert(!result.error && !result.signal && result.status === 0, `${name} did not complete successfully: ${result.status}`);
    assert(!/\b(error|warning|failed|panic|incomplete)\b/i.test(result.stderr), `${name} diagnostics require investigation`);
    return result.stdout;
  };
  const rust = run('rustc', ['-vV'], 'rust-version');
  assert(rust.split(/\r?\n/).includes(`host: ${windowsTarget}`), 'Rust host does not match Windows gate target');
  const common = ['--locked', '--all-features', '--manifest-path', 'src-tauri/Cargo.toml'];
  const metadataArgs = ['metadata', ...common, '--format-version', '1', '--filter-platform', windowsTarget];
  const treeArgs = ['tree', ...common, '--target', windowsTarget, '--edges', 'normal,build,dev', '--prefix', 'none', '--format', '{p}'];
  fs.writeFileSync(path.join(out, 'windows-graph-commands.json'), JSON.stringify({ metadataArgs, treeArgs }, null, 2));
  const raw = run('cargo', metadataArgs, 'windows-metadata');
  fs.writeFileSync(path.join(out, 'windows-metadata.json'), raw);
  const graph = validateWindowsGraph(JSON.parse(raw), run('cargo', treeArgs, 'windows-tree'), locked, manifest);
  fs.writeFileSync(path.join(out, 'windows-graph.json'), JSON.stringify(graph, null, 2));
  return graph;
}

export function selectWindowsScope(report, expected, graph, gitPackages) {
  assert(graph.coverageComplete && graph.target === windowsTarget &&
    graph.edgeKinds?.join() === 'normal,build,dev' && graph.featureSelection === 'all workspace features', 'Unverified Windows graph scope');
  const rust = new Set(graph.packages.map(key));
  const git = new Set(gitPackages.map(key));
  const expectedRust = new Set(expected['src-tauri/Cargo.lock'].map(key));
  assert(rust.size > 0 && [...rust, ...git].every(k => expectedRust.has(k)), 'Graph/Git packages missing from scan inventory');
  const reason = p => p.ecosystem === 'npm' ? 'npm (including development/build)' : git.has(key(p)) ? 'pinned Git source' : rust.has(key(p)) ? 'Windows normal/build/dev graph' : 'outside Windows graph (retained in full report)';
  const blocks = p => p.ecosystem === 'npm' || git.has(key(p)) || rust.has(key(p));
  const scoped = structuredClone(report);
  const classification = report.results.flatMap(r => r.packages.filter(p => p.vulnerabilities?.length).map(p => ({ ...p.package, blocking: blocks(p.package), reason: reason(p.package), ids: p.vulnerabilities.map(v => v.id) })));
  for (const result of scoped.results) result.packages = result.packages.filter(p => blocks(p.package));
  const inventory = Object.fromEntries(Object.entries(expected).map(([source, packages]) => [source, packages.filter(blocks)]));
  return { report: scoped, inventory, classification };
}
