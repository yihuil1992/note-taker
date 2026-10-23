# Dependency security checks

`pnpm security:scan` scans the complete committed `pnpm-lock.yaml` and
`src-tauri/Cargo.lock` with OSV-Scanner 2.7.0. Development, build, optional and
other-platform dependencies are included. No severity threshold, advisory
ignore list or reachability filtering is applied. Informational RustSec
maintenance advisories also block this full-scan command.

The **Dependency Security / Windows dependencies (full report retained)** workflow runs
for pull requests, pushes to `main`, manual dispatch and Mondays at 08:19 UTC.
It has read-only repository permissions, uses immutable action revisions,
installs tooling with lifecycle scripts disabled, and retains full JSON,
inventory, input hashes, scanner version, diagnostics and exit status for 30
days even when findings or errors fail the job. It does not publish releases
or change repository security settings.

## Local use

Use Node 22 or newer and pnpm 9.15.9, then run:

```sh
pnpm install --frozen-lockfile
pnpm security:test
pnpm security:windows # Windows blocking policy; retains every finding
pnpm security:scan    # Optional full cross-platform blocking policy
```

The scanner supports Windows x64 and Linux x64. Its official release binary
is downloaded over HTTPS and checked against a SHA-256 digest pinned in
`scripts/security/scan.mjs`. `OSV_SCANNER_PATH` may select an existing binary;
the same digest and version verification still apply. Updating the scanner
requires reviewing the release and updating both platform digests.

Reports default to a new timestamped directory under ignored
`output/security/`. An optional argument specifies a different empty output
directory. Existing report directories cannot be reused, preventing stale
clean reports from masking a failed scan.

Exit codes are 0 when complete coverage has no in-scope findings, 1 for in-scope findings, and 2 for scanner
or validation errors. Any nonzero code must block the gate. Independent YAML
and TOML parsing enumerates all package/version identities; both source
reports must exactly match. Missing packages, malformed results, diagnostics,
unexpected scanner config, additional dependency files, changed scan inputs,
unsupported package sources and checksum failures fail closed. New ecosystems,
workspaces or source types need explicit coverage support before they pass.
The normal Windows CI also runs Cargo check/test with `--locked` so manifests
cannot silently resolve a different Rust graph.

## Remediation status as of 2026-10-23

Compatible updates retain React 18, Vite 6 and Tauri 2. The original full
baseline contained 44 unique advisory groups across 29 package versions
(13 npm groups and 31 Rust groups). After compatible updates there were eight
groups; the approved ZIP migration leaves five groups across five Rust
versions. The current complete inventory is 143 npm and 609 Rust entries.
No npm finding or finding in the Windows normal/build/dev graph remains.

### ZIP migration and upstream pin

The direct runtime archive dependency and the updater now share zip 9.0.5.
The removed zip 2.6.1 and 4.6.1 matched RUSTSEC-2026-0367,
RUSTSEC-2026-0368 and RUSTSEC-2026-0369 (aliases GHSA-vv5m-jxxh-4627,
GHSA-4v9h-p7m9-rh26 and GHSA-4684-h2q9-2vcp). The updater's former
ZipArchive::extract path was reachable on Windows; the other two advisories
concern Unix permission handling. None of these three remains in the scan.

Released tauri-plugin-updater 2.14.0 still requires zip 4. The application
therefore pins the official v2 commit
ba98bfadc4c868c9a7e1a226c83af528e1f11b89 from
[upstream PR 3698](https://github.com/tauri-apps/plugins-workspace/pull/3698).
Compared with the 2.14.0 release, the updater package changes only its zip
requirement from 4 to 9. There is no fork, incompatible transitive override,
Tauri 3 migration or change to updater source/signature verification.
Default ZIP, Rustls and proxy features remain enabled. Return to a released
2.x updater when a release includes this fix; review that release, update the
manifest/lock and scanner's explicit Git pin policy, then repeat these tests.
The pinned Git source makes builds depend on that official repository's
availability until then. It is an unreleased upstream fix, not a new stable
updater release.

The application's runtime extraction adapts zip 9's fallible name API and
rejects symlinks, absolute roots, drive prefixes and alternate data streams.
Its pinned Whisper archive/model SHA-256 checks remain unchanged. Runtime
ZIP fixtures cover Deflate, nested files, traversal, absolute paths, symlinks
and truncation. Existing meeting archival/restoration and Markdown/JSON
exports are tested with an isolated database. The product has no meeting
ZIP import/export feature; no such feature was added.

On Windows, run node scripts/security/test-updater.mjs after installing
frontend dependencies and building the locked Rust app. Windows CI runs it
automatically. The harness copies the exact upstream package to ignored
output/updater-regressions/, verifies source against its Git revision,
resolves only packages already in the app lock, and tests the real private
extraction functions with inert ZIP/EXE/MSI payloads. It also runs upstream
signature/version regressions. A fixture-only installation guard fails if
execution reaches installation. Production sources are untouched. Unused
native-TLS feature alternatives are omitted only from the fixture manifest;
the app's production feature set is unchanged. Temporary extraction and
fixture evidence remain under that ignored output directory.

The security gate scans every locked package version, including the Git
updater's version, and separately queries OSV for its exact Git commit.
Pagination, raw responses and merged findings are retained. Missing commit
coverage, API errors or a different/unreviewed Git source fail closed. An
empty advisory result means OSV knows no advisory for that query, not a
proof that a commit is vulnerability-free.

### Remaining advisory groups

All five remain visible and keep the optional full-scan command red; they do not block the Windows gate. None appears in the
verified x86_64-pc-windows-msvc normal/build/dev dependency graph.
There are zero remaining Windows runtime or Windows build-only findings.

| Advisory | Dependency path | Classification |
| --- | --- | --- |
| RUSTSEC-2024-0429 / GHSA-wrw7-89jp-8q8g | App -> tauri 2.12.5 -> gtk 0.18.2 -> glib 0.18.5 | Other-platform vulnerability in VariantStrIter; patched glib line starts at 0.20, outside the GTK 0.18 chain. |
| RUSTSEC-2024-0370 | App -> Tauri/GTK -> glib-macros 0.18.5 -> proc-macro-error 1.0.4 | Other-platform compile-time maintenance notice. |
| RUSTSEC-2026-0371 | App -> tauri-plugin-notification 2.7.0 -> zbus 5.19.0 -> zbus_names 4.3.5 | Other-platform maintenance notice; standalone package merged into zbus 6. |
| RUSTSEC-2026-0370 | Same notification/D-Bus path -> zvariant 5.15.1 | Other-platform maintenance notice; standalone package merged into zbus 6. |
| RUSTSEC-2026-0372 | Same D-Bus path -> zvariant -> zvariant_derive 5.15.1 | Other-platform compile-time maintenance notice. |

### Windows blocking policy

The hosted security job runs pnpm security:windows on Windows x64. It first
completes the full lockfile scan and the pinned Git commit query, preserving
their raw findings. It then resolves Cargo metadata for
x86_64-pc-windows-msvc with --locked --all-features and traverses every normal,
build and development edge. All npm packages (including development/build
and optional packages) and pinned Git sources always remain blocking.

The graph validates package versions and sources against Cargo.lock, the
workspace root, complete resolve nodes, every edge reference and edge kind.
An independent cargo tree invocation includes normal/build/dev edges and
must match the root and resolve every tree package in that metadata graph.
Cargo metadata can unify more dependency features than cargo tree; all such
extra packages remain conservatively blocking and are listed separately in
windows-graph.json. They are never discarded to obtain a clean result.
See [Cargo feature resolution](https://doc.rust-lang.org/cargo/reference/resolver.html).
The currently checked graph contains 383 Rust entries, including all 357
unique cargo tree entries and 26 additional metadata entries.

The runner verifies its Windows Rust host. Missing graph nodes, wrong source
or version, unsupported edge kinds, inconsistent inventories, tree packages
outside metadata, command/diagnostic failures, missing scan coverage and
changed input files fail closed with exit 2. Every in-scope finding blocks
with exit 1, regardless of severity or maintenance classification.

The full summary.json, osv.json, combined-osv.json and Git responses stay
unaltered. windows-scope.json classifies every affected package and records
blocking reasons; windows-osv.json is an additional scoped view. The job
summary explicitly displays nonblocking findings, and all reports/graphs
are retained for 30 days on success or failure. A green Windows job does not
claim that the cross-platform report is clean. No advisory ignore list or
continue-on-error is used.

The standalone pnpm security:scan still exits 1 for the five remaining groups.
Full cross-platform cleanup requires upstream GTK/glib and D-Bus migrations,
which are outside this task. Draft PR publication and hosted checks are
authorized; merge, release, deployment and repository policy changes are not.

Lockfile scanning does not analyze application logic, the separately downloaded
Whisper executable/model, the user's Codex CLI, the system WebView2 runtime or
Windows itself. No real signed update installation, live recording/provider
call or hosted CI run is part of this validation. The published v0.2.10 feed
still points to its existing signed NSIS executable and was not modified.
