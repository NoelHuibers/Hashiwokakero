# libSQL dependency advisory triage — 2026-10-20

Reviewed application baseline: `232ecf2` (main after PR #78). This is a
source/configuration assessment, not a scan of production or its certificates.

## Decision

Apply only compatible lockfile patches: `rustls-webpki 0.103.15 → 0.103.16` and
`webpki-roots 1.0.9 → 1.0.10`. A fresh audit identified additional emSign distrust
advisories published October 19–20 that these updates address on the modern TLS
chain. Keep the manifest and runtime configuration unchanged. All five current
Dependabot alerts remain unresolved; this report neither suppresses them nor
claims that lack of an identified input path patches vulnerable code. No published,
compatible dependency update fixes the legacy TLS chain or libSQL parser today.
Replacing the driver, adapting between Hyper generations, changing the TLS
backend, or maintaining a crypto fork needs separate review and regression tests.

The CRL-specific failures have no identified application path with the existing
configuration. Certificate name constraints are evaluated on the real remote DB
connection, so the two name-constraint alerts retain conditional exposure. The
parser's invalid-byte prerequisite has no identified path through the application's
safe libSQL API and bound values.

## Reproduced dependency graph

`cargo tree --locked --features ssr -i <package>@<version>` gives:

```text
hashi 2.0.0 [ssr]
├── libsql 0.10.0 [default-features=false; core, remote, tls]
│   ├── hyper-rustls 0.25.0
│   │   ├── rustls 0.22.4
│   │   │   └── rustls-webpki 0.102.8       ← alerts #41–44
│   │   └── tokio-rustls 0.25.0
│   │       └── rustls 0.22.4 (same copy)
│   └── libsql-sqlite3-parser 0.13.0       ← alert #40
├── reqwest 0.12.28
│   └── hyper-rustls 0.27.10
│       └── rustls 0.23.46
│           └── rustls-webpki 0.103.16    ← updated; baseline was 0.103.15
└── lettre 0.11.23
    └── rustls 0.23.46 (same copy)
```

This diagram shows the relevant runtime paths, not every dependency. Build-time
`icu_provider_source → ureq` also uses the patched Rustls copy.
`remote → hrana → parser` enables the parser: removing only the explicit `parser`
feature cannot remove it. `core` remains necessary for local development/tests.

## Alerts and prerequisites

| Alert | Primary advisory / published fix | Reachability in this application |
| --- | --- | --- |
| #44, high | [GHSA-82j2-j2ch-gfr8](https://github.com/rustls/webpki/security/advisories/GHSA-82j2-j2ch-gfr8), [RUSTSEC-2026-0104](https://rustsec.org/advisories/RUSTSEC-2026-0104.html); WebPKI `0.103.13` | Parsing attacker-influenced CRL bytes can panic before checking the CRL signature. No CRLs are loaded/configured here. A peer's certificate or CRL Distribution Point URL alone does not cause this client to fetch/parse a CRL. |
| #43, low | [GHSA-xgp8-3hg3-c2mh](https://github.com/rustls/webpki/security/advisories/GHSA-xgp8-3hg3-c2mh), [RUSTSEC-2026-0099](https://rustsec.org/advisories/RUSTSEC-2026-0099.html); `0.103.12` | Wildcard DNS names can pass overly narrow permitted-subtree constraints. Conditional exposure on DB HTTPS: requires a properly signed misissued certificate chain under a trusted authority and ability to present it for the configured DB hostname. Normal signature, validity and hostname checks still apply. |
| #42, low | [GHSA-965h-392x-2mh5](https://github.com/rustls/webpki/security/advisories/GHSA-965h-392x-2mh5), [RUSTSEC-2026-0098](https://rustsec.org/advisories/RUSTSEC-2026-0098.html); `0.103.12` | URI name constraints are ignored instead of rejected. Also requires a properly signed misissued chain; selecting a DNS hostname does not remove certificate-chain constraint processing. Conditional exposure remains. |
| #41, medium | [GHSA-pwjx-qhcg-rvj4](https://github.com/rustls/webpki/security/advisories/GHSA-pwjx-qhcg-rvj4), [RUSTSEC-2026-0049](https://rustsec.org/advisories/RUSTSEC-2026-0049.html); `0.103.10` | Multiple certificate distribution points can prevent consulting the relevant CRL. Unsafe acceptance additionally involves allowing unknown revocation status; the default denies it. No configured CRLs here, so no affected revocation decision was identified. |
| #40, low | [GHSA-8m95-fffc-h4c5 / CVE-2025-47736](https://github.com/advisories/GHSA-8m95-fffc-h4c5); no listed fixed libSQL parser version | The parser accepts arbitrary bytes and performs unchecked UTF-8 conversion. The libSQL path used here accepts SQL as `&str` and calls `Parser::new(s.as_bytes())`. User values are bound separately. No path supplying malformed UTF-8 as SQL was identified; the vulnerable dependency remains. |

### Trust and input boundaries checked

`src/server/db.rs::init` uses an operator-provided `TURSO_DATABASE_URL` and token
with `Builder::new_remote(...).build()`. HTTP requests do not choose the DB
endpoint, trust store or verifier. LibSQL maps `libsql://` to `https://` (and adds
HTTPS to a schemeless URL); its default connector also accepts explicit HTTP.
This review did not read environment secrets or verify the deployment's actual
URL, CA store or certificate chain. Production must continue using a trusted
HTTPS/libSQL endpoint; explicit plaintext HTTP provides no certificate protection.

Although libSQL enables hyper-rustls's `webpki-roots` feature, its actual
`database::connector` calls `with_native_roots()`, **not** `with_webpki_roots()`.
It uses platform root certificates, default verification, no client authentication,
HTTP/1, and TCP_NODELAY. Rustls's server verifier starts with an empty CRL vector;
when empty it passes no revocation options. Neither this application nor the
default connector calls `with_crls` or installs a custom verifier. Native trust
store contents are an operational boundary, not proof that name constraints can
never matter. Inbound application HTTP is served by Axum; it does not use this
outbound libSQL TLS connector to verify client certificates.

For SQL, inspected the libSQL connection and Hrana statement preparation paths,
application queries and migrations. Migration batches are application constants;
account/contact/upload values use parameters. Dynamic query fragments such as
stamp query composition and numeric ID lists remain valid Rust strings; they
do not introduce the advisory's raw invalid-byte input. Binary uploads and bound
BLOB values are data, not SQL text. This assessment covers the reported malformed
UTF-8 prerequisite, not every possible parser defect or every possible SQL bug.

## Why a dependency-only fix is blocked

The crates.io version catalogs on this date show latest compatible releases:
`libsql 0.10.0`, `libsql-sqlite3-parser 0.13.0`, `hyper-rustls 0.25.0`,
`rustls 0.22.4`, and `rustls-webpki 0.102.8`. No patched `0.102.x` backport is
published. A newer `0.103.x` copy cannot satisfy Rustls's `^0.102.1` requirement.
Verified without modifying the lockfile:

```text
cargo update --locked -p rustls-webpki@0.102.8 --precise 0.103.13
error: failed to select a version for the requirement rustls-webpki = "^0.102.1"
```

LibSQL's current upstream main is the same commit as the published crate,
`d8bc862d20bedc72945a1da27c8b7045414c0261`; it still selects hyper-rustls `0.25`
and parser `0.13`. Switching to git main therefore does not repair either chain.
The parser crate's packaged VCS commit is
`97652c12fe8bb215fe51b5370b46f27c0bc9d6e1` and still contains the unchecked
conversion. [libSQL issue #2052](https://github.com/tursodatabase/libsql/issues/2052)
is open. Upstream lemon-rs fixed its separate `sqlite3-parser` package in
[`14f422a` / PR #87](https://github.com/gwenn/lemon-rs/pull/87), released as
`0.15.0` according to [issue #86](https://github.com/gwenn/lemon-rs/issues/86).
That does not publish a fixed `libsql-sqlite3-parser` or establish grammar/API
compatibility with libSQL's fork.

`Builder<Remote>::connector` is a supported extension point, but expects an
HTTP `0.2::Uri` service and a Hyper `0.14::Connection`/Tokio socket. Hyper-rustls
`0.27.10` expects HTTP `1` and Hyper `1` I/O traits. It cannot be substituted
directly. Hyper-rustls `0.26.0` already uses Hyper `1`, but still Rustls `0.22`;
it would not fix these alerts either. Disabling `libsql/tls` without supplying a
working custom connector makes the default connector panic. Adding patched
WebPKI as a direct dependency merely leaves both versions installed.

Primary pinned-source references:

- [libSQL manifest](https://github.com/tursodatabase/libsql/blob/d8bc862d20bedc72945a1da27c8b7045414c0261/libsql/Cargo.toml), [default connector](https://github.com/tursodatabase/libsql/blob/d8bc862d20bedc72945a1da27c8b7045414c0261/libsql/src/database.rs), [custom connector API](https://github.com/tursodatabase/libsql/blob/d8bc862d20bedc72945a1da27c8b7045414c0261/libsql/src/database/builder.rs).
- [libSQL parser wrapper](https://github.com/tursodatabase/libsql/blob/d8bc862d20bedc72945a1da27c8b7045414c0261/libsql/src/parser.rs), [pinned parser conversion](https://github.com/tursodatabase/libsql/blob/97652c12fe8bb215fe51b5370b46f27c0bc9d6e1/vendored/sqlite3-parser/src/dialect/mod.rs).
- [Rustls 0.22.4 manifest](https://docs.rs/crate/rustls/0.22.4/source/Cargo.toml), [server verifier](https://docs.rs/crate/rustls/0.22.4/source/src/webpki/server_verifier.rs).
- [hyper-rustls 0.25.0 configuration](https://docs.rs/crate/hyper-rustls/0.25.0/source/src/config.rs), [0.26.0 manifest](https://docs.rs/crate/hyper-rustls/0.26.0/source/Cargo.toml), [0.27.10 connector](https://docs.rs/crate/hyper-rustls/0.27.10/source/src/connector.rs).
- Published version catalogs: [libSQL](https://crates.io/api/v1/crates/libsql), [parser](https://crates.io/api/v1/crates/libsql-sqlite3-parser), [WebPKI](https://crates.io/api/v1/crates/rustls-webpki), [Rustls](https://crates.io/api/v1/crates/rustls), [hyper-rustls](https://crates.io/api/v1/crates/hyper-rustls).

## Additional fresh-audit findings and the chosen patch

`cargo-audit 0.22.2` against RustSec database commit
`e6f94f3760d5e6c54c1b05f204002263bc025524` (1,348 advisories) found **16 package/
advisory matches before, 14 after** the two lockfile updates. Both scans report
vulnerabilities, not a clean audit. The GitHub parser advisory is not among this
RustSec scan's findings; it remains independently unresolved.

- [RUSTSEC-2026-0382](https://github.com/RustSec/advisory-db/blob/e6f94f3760d5e6c54c1b05f204002263bc025524/crates/rustls-webpki/RUSTSEC-2026-0382.md)
  flags both WebPKI versions for accepting paths with distrusted emSign
  intermediate keys whose private keys are public. [Upstream PR #546](https://github.com/rustls/webpki/pull/546)
  adds explicit public-key distrust checks and regression coverage. Updating to
  `0.103.16` fixes the modern TLS copy; `0.102.8` remains vulnerable when its native
  trust anchors permit such a chain. Unlike the CRL alerts, this does not require
  opting into CRLs. A reviewed deployment trust-store update removing affected
  trust paths is a potential interim mitigation for the legacy client, not an
  application fix performed or verified here. Account for alternate/cross-signed
  paths before declaring the trust-store mitigation complete.
- [RUSTSEC-2026-0381](https://github.com/RustSec/advisory-db/blob/e6f94f3760d5e6c54c1b05f204002263bc025524/crates/webpki-roots/RUSTSEC-2026-0381.md)
  is addressed in bundled roots `1.0.10`, which removes emSign ECC Root CA - G3
  ([upstream PR #133](https://github.com/rustls/webpki-roots/pull/133)). This protects
  the clients actually using that bundled list. Audit still flags `0.26.11` by
  version, but its entire root-list implementation is
  `pub use parent::TLS_SERVER_ROOTS`, with `parent = webpki-roots 1`. The lock now
  resolves that parent to `1.0.10`; it contains no independent stale root list.
  No advisory suppression was added. LibSQL uses native roots regardless, so this
  root-list update does not itself protect its legacy certificate validation.
- Legacy Rustls `0.22.4` also matches [RUSTSEC-2026-0349](https://rustsec.org/advisories/RUSTSEC-2026-0349.html)
  (key-share prediction downgrade) and [RUSTSEC-2026-0346](https://rustsec.org/advisories/RUSTSEC-2026-0346.html)
  (debug output of custom session-store values exposes secrets). The default
  Ring provider here offers only classical groups, and the application installs
  no custom session store or session-value logging. Those reported prerequisites
  were not identified; the package-level findings remain. Modern Rustls `0.23.46`
  is already the patched release for both.
- [RUSTSEC-2026-0258](https://rustsec.org/advisories/RUSTSEC-2026-0258.html) flags
  `h2 0.3.27`. LibSQL's default connector enables only HTTP/1, limiting that path;
  updating h2 to `0.4.16` also requires the Hyper generation migration.
- Outside this TLS/parser change, the lockfile scan also flags Wasmtime `49.0.3`
  ([0378](https://rustsec.org/advisories/RUSTSEC-2026-0378.html),
  [0379](https://rustsec.org/advisories/RUSTSEC-2026-0379.html); patched `49.0.4`)
  and build-time `icu_provider_source → zip 2.6.1`
  ([0367](https://rustsec.org/advisories/RUSTSEC-2026-0367.html),
  [0368](https://rustsec.org/advisories/RUSTSEC-2026-0368.html),
  [0369](https://rustsec.org/advisories/RUSTSEC-2026-0369.html); patched `9.0.0`).
  Wasmtime is optional in the Dojo worker, not selected by the default SSR graph;
  parent integration should prioritize its separate patch and worker regressions,
  including the exact `wiggle = "=49.0.3"` pin in `dojo/Cargo.toml` that prevents
  updating the Wasmtime/WASI family together through a lockfile-only update.
  These findings were not remediated or fully assessed here. Audit additionally
  reports unmaintained `paste 1.0.15`, `rustls-pemfile 2.2.0`, and
  `unsafe-libyaml 0.2.11`; the dependency status warnings remain.

Only the two supported TLS patches were applied; other pinned versions, DB APIs
and TLS configuration remain unchanged. Certificates using distrusted emSign
paths are intentionally rejected on the updated chain; affected service endpoints
would need replacement certificates. Reproduction:

```sh
cargo update -p rustls-webpki@0.103.15 --precise 0.103.16
cargo update -p webpki-roots@1.0.9 --precise 1.0.10
cargo audit --file Cargo.lock --json
```

## Remediation plan and acceptance criteria

The steps below address the unresolved legacy chain/parser. The compatible modern
TLS patches above do not depend on that larger work.

1. Prefer a maintained upstream libSQL release/backport selecting patched WebPKI
   and a repaired parser. Recheck the actual resolved graph and packaged source,
   including the libSQL fork rather than only the original lemon-rs project.
   A WebPKI-compatible backport would address TLS independently of the parser.
2. If upstream cannot provide that, separately review a transport-only adaptation
   through `Builder::connector` with `libsql/tls` disabled. A Rustls solution needs
   an HTTP/Hyper compatibility adapter or upstream Hyper migration. An existing
   Hyper 0.14 native-TLS connector is another option, but changes TLS backend,
   platform dependencies and trust behavior and also needs review. Preserve
   hostname verification, platform roots, HTTP/1 and connection behavior. Do not
   add a permissive verifier or a locally maintained cryptography fork.
3. Require local TLS integration regressions before accepting a connector change:
   trusted certificate success; unknown issuer, wrong hostname, expired cert and
   the relevant signed name-constraint fixtures rejected; no authentication token
   delivered to a rejected peer. Exercise actual remote Hrana queries, parameter
   binding, batches and transaction commit/rollback against a disposable service,
   plus local migrations/account/session tests. Preserve PR #78's move-log writes.
   Keep any explicit HTTP behavior change a separate reviewed deployment choice.
4. For the parser, accept a maintained fork release carrying the upstream fix and
   verify its malformed-byte regression in that package. Do not invoke the known
   unsound raw-byte API from application tests or rename an unrelated parser
   merely to clear the advisory. A full DB-driver migration is a larger option
   requiring separate API, transaction, error and deployment validation.
5. Re-run the locked dependency graph and advisory scans; confirm every vulnerable
   copy is removed before claiming remediation. Keep unresolved alerts visible.
   All six required CI checks still apply before any code remediation merges.

Existing exposure limits to preserve: operator-controlled secure DB endpoint and
CA store, normal certificate verification, no CRL ingestion, safe UTF-8 SQL APIs,
and bound user values. They limit attack prerequisites; they do not close alerts.

## Validation

- Re-fetched the five open repository Dependabot alerts through read-only GitHub
  API access and read their primary advisories and upstream issue/fix discussions.
- Reproduced both WebPKI reverse graphs and the parser reverse graph with
  `cargo tree --locked --features ssr`; inspected the actual Cargo registry source.
- Checked published compatible versions and current libSQL upstream source.
- The forced legacy WebPKI update failed with the expected version requirement
  error; `git diff --exit-code Cargo.toml Cargo.lock` passed immediately afterward,
  before applying the two supported modern TLS updates.
- `cargo test --locked --features ssr --lib server::db::tests`: **9 passed**.
- `cargo clippy --locked --features ssr --lib --tests -- -D warnings`: **passed**.
- Native root-list regression: compiled baseline `webpki-roots 1.0.9` and linked
  both current `1.0.10` and compatibility `0.26.11` artifacts from this worktree.
  Located the baseline emSign ECC Root CA - G3 by subject, then asserted that
  **neither current list contains its public key**: **1 passed**. This verifies
  the compatibility package's effective root list despite its remaining audit
  match. The temporary probe/artifacts are under
  `/tmp/hashi-security-root-regression`; no production endpoints were contacted.
- Final `cargo audit --no-fetch --file Cargo.lock`: **exit 1**, 14 remaining
  package/advisory matches plus the three maintenance warnings listed above.
- Upstream WebPKI `0.103.16` source at its packaged VCS commit
  `8ecdcc76f56b68fda67a6adf7976f5eed5006a4c`, extracted under `/tmp` without source
  changes: `cargo test --locked --manifest-path <upstream>/Cargo.toml
  --no-default-features --features std,ring --lib distrusted_spki`: **1 passed**.
  This exercises distrust at root/intermediate/leaf positions, unused issuers and
  cross-signed paths. These are upstream tests, not new application tests.
- `cargo fmt --all -- --check` passed. No browser/site, release or coverage build
  was run. All Cargo builds/tests used `CARGO_BUILD_JOBS=2` and this isolated
  worktree or temporary upstream directories, never the parent checkout.
