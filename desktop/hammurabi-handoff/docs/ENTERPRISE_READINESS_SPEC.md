# Enterprise Readiness & End-to-End Specification

Full-system review, capability map, and target-state specification for
taking Hammurabi Handoff from its current shipped form — a working,
tested, single-machine hackathon submission — to an enterprise-grade,
end-to-end, production-operating-ready Agent Service Provider (ASP).

Written in the same spirit as `SUBMISSION_LEDGER.md`: every claim below is
checked against the code that exists today (`core/iedb-crypto`,
`tooling/iedb-oka`, `desktop/hammurabi-handoff`, commit `1b6642b` on
`main`), not asserted. Where a capability doesn't exist, this document says
so and specifies what would need to be built — it does not pretend
otherwise.

## 0. Method

Every source file in the three workspace crates was read directly:
`src/{app,commands,intent,lib,main,mesh,okx_bridge,server,settlement,
settlement_backend,vault,x402}.rs` (2,721 lines), `core/iedb-crypto/src/*`,
`tooling/iedb-oka/src/*`, the CI workflow, the ASP manifest/capabilities/
pricing JSON, and the existing docs (`ARCHITECTURE.md`,
`SUBMISSION_LEDGER.md`, `TRACK_ALIGNMENT.md`). Findings are organized as:
current state → gap → target requirement → why it matters → rough effort.

## 1. Executive summary

**Current maturity: working single-machine prototype (MVP), hackathon-
submission grade.** The core protocol logic — x402 metering, JIT ephemeral
credentials, canonical-JSON signing, chaos-tested minting, replay
prevention — is real, unit- and integration-tested (72 tests: 27 handoff ·
38 crypto · 7 oka, all green under `cargo test --workspace`), and
architecturally sound. It is **not** production/enterprise software today,
by the project's own design choices, most of them explicit and documented:

| Dimension | Today | Enterprise-grade bar |
|---|---|---|
| Deployment topology | One process, one machine, loopback-only (`127.0.0.1:3402`) | Multi-instance, network-reachable, load-balanced |
| State | 100% in-memory (`HashMap`, `Vec`, `RwLock`) — lost on restart | Durable, backed up, recoverable (RPO/RTO defined) |
| Payment rail | Mock by default; live facilitator seam exists but unverified against any real facilitator or testnet | Verified live settlement, reconciliation, dispute handling |
| On-chain settlement | Stubbed (`local://` endpoint, never dials out) | Deployed Stylus verifier, live 1Shot broadcast |
| AuthN/AuthZ | None — loopback binding *is* the trust boundary | Per-agent identity, scoped credentials, multi-tenant isolation |
| Observability | `tracing` calls exist; nothing exported (no metrics, no trace backend, no dashboards, no alerts) | Metrics, traces, logs shipped to a backend; SLOs with alerting |
| API contract | Ad hoc JSON shapes, no schema, no version header | Versioned, schema'd (OpenAPI), deprecation policy |
| Release engineering | CI = fmt/clippy/build/test on 3 crates; no security scanning, no SBOM, no signed releases | Supply-chain scanned, SBOM'd, signed, staged rollout |
| Ops readiness | No health/readiness endpoints, no runbooks, no on-call, no IaC | Health checks, runbooks, IaC, incident process |

None of this is a knock on the current code — it is exactly what a
sovereign, single-operator, hackathon-scoped ASP should look like, and the
project is unusually honest about the boundary (see
`SUBMISSION_LEDGER.md`'s own "still owed" and "deferred" sections). This
document specifies what changes, in what order, to cross that boundary
deliberately, without regressing the sovereignty/legibility properties
that are the product's actual differentiator.

## 2. As-built system map

```
┌────────────────────────────────────────────────────────────────┐
│                User's machine — one Tauri process              │
│  WebView ── invoke() ──► tower::oneshot ──► ONE Axum router     │
│                                              (server.rs)         │
│  127.0.0.1:3402 ◄── headless agents (loopback ONLY, same router)│
│  OKX storefront ──► okx_bridge ──► same router (same gate)      │
│                                                                  │
│  Router:                                                        │
│   GET  /mesh/feed         → mesh.rs   (in-memory log, cap 2000) │
│   POST /agent/announce    → intent.rs (no auth, caller-set      │
│                              agent_handle)                       │
│   POST /x402/settle       → x402.rs → settlement_backend.rs     │
│                              (Mock | Facilitator behind          │
│                              x402-live, off by default)          │
│   POST /execute            → x402 guard → vault.rs (JIT mint,   │
│                               chaos test, 60s TTL) → settlement.rs│
│                               (envelope, local:// stub, no I/O)  │
└────────────────────────────────────────────────────────────────┘
```

### 2.1 Component inventory (current status, verified against code)

| Component | File | Status | Notes |
|---|---|---|---|
| RAMesh feed | `src/mesh.rs` | **Working, bounded** | In-memory `Vec` + `broadcast` channel, capped at `MAX_LOG_LEN=2000` (mesh.rs:35), monotonic `seq` survives trim. No persistence, no auth on who can post (`sender` is caller-supplied). |
| Intent model | `src/intent.rs` | **Working** | `AgentIntent` fixed to one mock ETH-limit-order shape for the demo path; `agent_handle` is caller-controlled (documented risk, `SUBMISSION_LEDGER.md` item 4). |
| x402 gate | `src/x402.rs` | **Working, bounded** | Receipts in `RwLock<HashMap>`, swept opportunistically (`RECEIPT_STALE_AFTER_SECS=600`, x402.rs:115). Intent-bound, single-redemption, replay-proof by test. All in-memory — a restart drops the treasury and any receipt in flight. |
| Settlement backend | `src/settlement_backend.rs` | **Mock default; live seam unverified** | `SettlementBackend::Mock` is the default (401 lines total). `Facilitator` variant exists behind `x402-live`, wired at startup via `from_env_or_default()`, but explicitly documented as "unverified against any specific facilitator" (settlement_backend.rs:18-26) — never round-tripped against testnet USDC. |
| JIT vault | `src/vault.rs` | **Working, well-tested** | Ephemeral Ed25519 per mint, seed zeroized, RFC 8785 canonical signing, strict 60s TTL, 4-vector chaos test before every credential is released. This is the most mature subsystem in the repo. |
| Settlement envelope | `src/settlement.rs` | **Working, stubbed transport** | `OneShotEnvelope` is self-verifying and carries a portable `verify_handoff` reference function mirroring the intended Stylus contract logic — but `endpoint` is hardcoded `local://…`, never dials out (settlement.rs:27). No deployed contract exists. |
| OKX bridge | `src/okx_bridge.rs` | **Working (in-process only)** | Routes `OkxInvocation`s through the same router via `tower::oneshot`. Tested for the full loop + replay bounce. No live OKX network path is exercised — this is the loopback-only in-process bridge, not a deployed marketplace integration. |
| Desktop shell | `src/app.rs`, `src/commands.rs` | **Compiles, functional** | Tauri 2, static `ui/dist` embedded at compile time, no Node/npm. Feature-gated (`tauri-app`), off in default CI build. |
| CI | `.github/workflows/handoff-ci.yml` | **Working, narrow** | `fmt --check`, `clippy -D warnings`, `build`, `test` on default features, scoped to the 3 submission crates. No `cargo audit`/`cargo deny`, no coverage, no perf/load gate, no release job. |

### 2.2 Dependency posture (from `Cargo.toml` workspace manifest)

Notable: `sqlx` (sqlite/postgres-capable), `zeroize`, `argon2`,
`chacha20poly1305`, `curve25519-dalek`, and `crypto_box` are declared as
**workspace dependencies but are not used by any crate in this workspace
today** — they appear to be inherited from a larger IEDB workspace
convention. This matters directly for §4.3 (durable storage): the
persistence layer this spec proposes doesn't need a new dependency
decision, `sqlx` is already a pinned workspace version.

## 3. Gap analysis

Each gap: current state → risk if it ships to production as-is → target
requirement. Ordered by blast radius, not by file location.

### 3.1 Durability — all state is in-memory

**Current**: `MeshBus.log: RwLock<Vec<MeshMessage>>`, `X402Gate.receipts:
RwLock<HashMap<...>>` — both process-local, both gone on restart, crash,
or redeploy. There is no database, no write-ahead log, no snapshot.

**Risk**: A settled-but-unredeemed receipt (customer paid, hasn't yet
called `/execute`) is destroyed by a process restart — the customer paid
and got nothing, with no record to reconcile against. The audit feed
(RAMesh) — the product's core "if it isn't in the feed, it didn't happen"
promise — is itself not durable past a restart or the 2,000-line cap.

**Target**: A durable append-only audit log (the feed) and a durable
receipt ledger, both surviving process restart. `sqlx` (sqlite for
single-node, postgres for multi-node) is already a workspace dependency.
Two concrete additions:
- `mesh::MeshBus` gains an optional durable sink: every `post()` also
  appends to a `mesh_log` table (seq, channel, sender, kind, body, at) —
  the in-memory ring stays for the live broadcast/UI hot path, the table
  becomes the source of truth for feed replay across restarts.
- `x402::X402Gate` receipts move from `HashMap` to a `receipts` table with
  a `redeemed_at` column instead of remove-on-redeem, so a redeemed
  receipt remains a permanent audit record instead of disappearing —
  today `redeem()` removes the entry entirely (x402.rs:178-189), which
  means there is no record a specific receipt was ever used once it's
  gone. This is also required for §3.6 (financial reconciliation).

**Effort**: Medium. `sqlx` migrations + swap the two in-memory stores for
repository structs behind the same method signatures (`post`, `feed`,
`redeem`, `settle`) — the call sites in `server.rs`/`x402.rs` don't need
to change shape, only what backs them.

### 3.2 AuthN/AuthZ — the trust boundary is a bind address

**Current**: There is no authentication anywhere in the request path. The
entire security model is "only loopback processes can reach
`127.0.0.1:3402`" (server.rs:88-95, explicitly called the "sovereignty
invariant"). `agent_handle` in every intent is a caller-supplied string
with no verification it corresponds to a real, distinct agent
(`SUBMISSION_LEDGER.md` item 4, explicitly deferred as "correctly scoped
as a multi-process concern, not a same-machine one").

**Risk**: This is fine — by design — for a single-operator desktop app.
It is not fine the moment the router is reachable from anywhere but
loopback: any process on the network could announce intents, impersonate
any `agent_handle`, and drive the x402/mint pipeline. The moment this ASP
serves more than one operator's own agents (multi-tenant SaaS, a hosted
ASP, or even multiple untrusted agents on one shared machine), the audit
trail (`sender` field) becomes unverifiable and spoofable.

**Target**:
- Per-agent identity: each agent registers/holds a keypair; `agent_handle`
  is derived from (or checked against) a signature over the intent, not
  taken at face value. This composes naturally with the existing
  `iedb-crypto::Identity`/Ed25519 machinery already used for credentials.
- A service-level API key or mTLS client cert for any caller reaching the
  router over a network interface (as opposed to loopback), independent
  of per-agent identity.
- Scoped authorization: which agents/callers may invoke which
  capabilities (today all four capabilities are open to anyone who can
  reach the port).

**Effort**: Medium-large. This is the single biggest architectural change
in this document, because it's the one gap that's invisible at the current
(loopback-only, single-operator) scope and mandatory at any larger one.

### 3.3 Payment rail — mock by default, live path unverified

**Current**: `SettlementBackend::Mock` is the default and the only path
exercised by CI. The `Facilitator` backend (settlement_backend.rs:151-350)
is real HTTP plumbing against the documented x402 facilitator protocol
shape, but the module's own doc comment states it is "unverified against
any specific facilitator" and must be tested "on a testnet... before
mainnet" (settlement_backend.rs:24-26). No round-trip against a real
facilitator has happened.

**Risk**: Shipping `x402-live` today means trusting untested wire-format
assumptions (field names like `isValid`, `maxAmountRequired`, response
shapes) against real money. `SettlementError::Transport`/`Rejected` are
handled, but there's no retry/backoff, no idempotency key sent to the
facilitator (a retried `/settle` after a timeout could double-charge if
the facilitator itself isn't idempotent on its side), and no reconciliation
between "facilitator says settled" and "vault actually minted."

**Target**:
1. Testnet round-trip (e.g. `base-sepolia`) against one real facilitator
   before the `x402-live` feature is considered anything but experimental.
2. Idempotency key on the facilitator request (the intent id is a natural
   one) so a retried settle after a network timeout can't double-charge.
3. A reconciliation job: periodically diff "receipts marked settled" (now
   durable, per §3.1) against the facilitator's own settlement records,
   alerting on drift.
4. Retry/backoff with circuit-breaking on `SettlementError::Transport`
   (today a transport failure is a bare error, once, no retry).

**Effort**: Medium, mostly external (finding and integrating with a real
facilitator) rather than internal code complexity.

### 3.4 On-chain settlement — fully stubbed

**Current**: `settlement::ONESHOT_ENDPOINT_STUB` is a `local://` URI that
is architecturally guaranteed non-routable (settlement.rs:26-27, by
design, "so that nothing can mistake it for a live relay target"). No
Stylus contract is deployed; `STYLUS_VERIFIER_STUB_ADDRESS` is the zero-ish
placeholder `0x0…5710`. `settlement::verify_handoff` is a correct,
`no_std`-portable reference implementation of what the on-chain verifier
*would* run — proven by test against tamper/expiry/TTL-stretch/bad-sig —
but it has never executed inside an actual EVM/Stylus environment.

**Target** (this is the existing roadmap's G-1/G-5, restated with
acceptance criteria):
- G-1: Port `verify_handoff` to an actual Arbitrum Stylus contract
  (the function is already written to be mechanically portable — no
  `serde`, no JSON, no host types in the core logic). Acceptance: a
  deployed testnet contract that accepts the exact byte layout
  `canonical_intent ‖ 0x00 ‖ issued_at_le` and rejects the same four
  attack classes the Rust chaos test already covers.
- G-5: Replace the `local://` endpoint with a real 1Shot API integration,
  gated the same way `x402-live` is (off by default, explicit opt-in,
  explicit env-var-driven arming — reuse the `from_env_or_default` pattern
  from `settlement_backend.rs`).
- Add gas-sponsorship SLA handling: what happens when the sponsor account
  is out of funds, or the relay is down — today there is no fallback path
  because there is no live path at all.

**Effort**: Large (Stylus contract development + audit + deployment is
its own project). Sequenced last on purpose — it's the one gap that
depends on external infrastructure decisions (which chain, which relay)
outside this repo's control.

### 3.5 Observability — instrumented but not observable

**Current**: `tracing::info!` calls exist at a handful of points
(`server.rs`, `settlement_backend.rs`) but there is no exporter, no
metrics (request counts, latencies, 402 rate, mint failure rate, chaos
failure rate), no trace propagation across the mesh→gate→vault→settlement
pipeline, and no dashboards or alerting of any kind. `tracing-subscriber`
is only pulled in under the `tauri-app` feature (for local console
output), not in the headless/server path.

**Risk**: Zero visibility into production behavior. No way to answer "what
is the 402 rate right now," "how many credentials failed chaos testing
today (which would itself be a crypto-library-integrity alarm)," or "is
the receipt treasury growing unexpectedly." The 600-second stale-receipt
eviction (`x402.rs:108-115`) and the 2,000-line mesh cap
(`mesh.rs:28-35`) are both silent, unmonitored bounds — if either is being
hit constantly under real load, nobody would know.

**Target**:
- Structured logging to a real sink (not just stdout) in the headless
  path, always on (not feature-gated behind the desktop shell).
- Metrics: request rate/latency per route, 402 rate, settle success/
  failure rate, mint success/failure rate (specifically chaos-test
  failures — this metric should page someone, since a chaos failure means
  the crypto path may be misbehaving), receipt-treasury size, mesh-log
  size, eviction-sweep counts.
- Traces spanning the four-phase pipeline (announce → 402 → settle →
  execute) correlated by `intent_id`, so a slow or failed mint can be
  traced end-to-end.
- SLOs (see §5) with alerting wired to them, not just dashboards nobody
  watches.

**Effort**: Medium. `tracing` is already the logging facade in use; this
is primarily wiring an exporter (OpenTelemetry/Prometheus) and defining
the metric set, not re-instrumenting the codebase.

### 3.6 API contract — no schema, no versioning

**Current**: Four HTTP routes with ad hoc JSON shapes defined only by the
Rust types (`AgentIntent`, `PaymentReceipt`, `OneShotEnvelope`, error
`json!({...})` literals). No OpenAPI/JSON-Schema artifact. No API version
in the URL or headers — `asp/manifest.json` declares `"manifest_version":
"1.0"` but that versions the manifest, not the wire protocol. Error
response shapes are inconsistent: some are `{"error": ..., "detail":
...}`, the 402 body is `{"error": ..., "accepts": [...]}`.

**Risk**: Any consumer (OKX marketplace, a future SDK, a partner
integration) is coupled to whatever the Rust structs happen to serialize
as today, with no contract to review changes against and no way to
evolve the API without breaking every existing caller silently.

**Target**:
- Generate an OpenAPI 3 spec from the route set (a workspace crate like
  `utoipa` integrates directly with Axum handlers with minimal
  intrusion).
- A versioned API surface (`/v1/execute`, etc., or an `Accept`/custom
  version header) with a documented deprecation policy before any breaking
  change ships.
- A single consistent error envelope shape across all routes and status
  codes.
- Publish the schema alongside `asp/manifest.json` / `asp/capabilities.json`
  so those hand-maintained JSON files (currently manually kept in sync
  per their own `$comment` fields) can be validated against — or
  generated from — the real contract instead of drifting.

**Effort**: Small-medium. Mechanical once the error-shape decision is
made; the bulk of the work is picking and integrating the schema tool.

### 3.7 Scalability & concurrency — single process, single machine

**Current**: One Axum router, one process, `Arc<MeshBus>`/`Arc<X402Gate>`
shared via in-process `RwLock`. `serve_loopback` binds exactly one
address (`server.rs:90-95`). There is no notion of multiple instances,
load balancing, or partitioning by tenant/agent.

**Risk**: Not a risk at current (single-operator desktop) scope — it's
the correct architecture for that scope. It becomes the ceiling the
moment this needs to serve concurrent load from many agents/tenants
beyond what one process's in-memory locks can serialize, or needs
availability beyond "however long this one process stays up."

**Target**: Once §3.1 (durable storage) lands, horizontal scaling is a
matter of pointing N stateless router instances at the same database and
putting a load balancer in front — the router itself (`handoff_router`)
has no in-process state that can't move to the DB. This is deliberately
sequenced *after* durability, not before: scaling an in-memory `RwLock`
architecture horizontally is not meaningful (each instance would have its
own receipt treasury).

**Effort**: Small once §3.1 is done; large if attempted before it (would
require inventing a distributed-lock story for no reason).

### 3.8 Reliability & operational readiness

**Current**: No `/health` or `/ready` endpoint. No documented graceful
shutdown behavior (what happens to an in-flight `/execute` on SIGTERM?).
No runbooks, no incident response process, no defined on-call, no
infra-as-code, no containerization (`Dockerfile` does not exist in the
repo).

**Target**:
- `GET /health` (process liveness) and `GET /ready` (dependency checks —
  once §3.1 adds a DB, readiness should check it's reachable).
- Documented graceful shutdown: drain in-flight `/execute` calls (the
  chaos-test + mint path should never be killed mid-signature) before
  exiting on SIGTERM.
- A `Dockerfile` + minimal IaC (even a docker-compose for local
  multi-service testing once a DB is added) so "how do I run this
  somewhere that isn't my laptop" has an answer.
- Runbooks for the specific failure modes this codebase already knows
  about: facilitator transport failure, chaos-test failure (should this
  ever fire in production, it needs an explicit response — it means an
  Ed25519 verification anomaly), receipt-treasury growth anomalies.

**Effort**: Medium, mostly additive (new endpoints, new deployment
artifacts) rather than architectural.

### 3.9 Security hardening & supply chain

**Current**: CI runs `cargo fmt`, `cargo clippy -D warnings`, `cargo
build`, `cargo test` — no dependency vulnerability scanning (`cargo
audit`/`cargo deny`), no SBOM generation, no release signing, no
fuzzing, no formal penetration test. `.expect()`/`.unwrap()` calls were
manually audited in the last adversarial review and confirmed
structurally unreachable given the type system (`SUBMISSION_LEDGER.md`
§"Full adversarial review"), which is good practice — but it was a
one-time manual pass, not a CI gate that re-checks on every future
change.

**Target**:
- `cargo audit` (or `cargo deny`) as a CI gate, not a one-off manual
  check — catches newly disclosed CVEs in dependencies automatically.
- SBOM generation (`cargo cyclonedx` or equivalent) published per release.
- Fuzzing the JCS canonicalization and vault mint/verify paths
  (`cargo-fuzz`) — these are exactly the kind of parser/crypto-boundary
  code fuzzing is built for, and the existing chaos test already proves
  the team thinks adversarially about this code; fuzzing extends that
  discipline continuously instead of as a single hand-written probe set.
- A real third-party penetration test / security review before any build
  handles real money at scale (the `x402-live` path specifically).
- Signed release artifacts (the desktop binary, at minimum) once this
  ships to anyone who isn't building from source.

**Effort**: Small for the CI-gate items (audit/deny/SBOM are near
drop-in); larger for fuzzing harness setup and external pen testing.

### 3.10 Compliance & governance

**Current**: No terms of service, no data handling policy document (even
though the actual posture — zero telemetry, no cloud persistence, no
network I/O in core logic — is a genuinely strong privacy story, it isn't
written down as a policy anyone could rely on contractually). No defined
data retention policy for the audit feed once it's durable (§3.1) — how
long does a financial/audit record need to be kept once real money moves
through it?

**Target**: A short, honest privacy/data-handling policy document
matching what the code actually does (continuing this project's existing
practice of ledger-verified claims), a retention policy for the durable
audit log once §3.1 lands, and — specifically because this handles
payments — a review of what jurisdiction-specific requirements (money
transmission, KYC/AML) apply once `x402-live` settlement is real and not
mocked. This is the one area of this document that is more legal/policy
than engineering, and should be scoped with counsel before `x402-live`
goes to production, not after.

**Effort**: Small (documentation) plus external (legal review) before any
live-money launch.

## 4. Target architecture (aggregate)

```
                         ┌─────────────────────────────┐
                         │   Load balancer / gateway    │
                         │  (authN: API key / mTLS)     │
                         └──────────────┬───────────────┘
                    ┌───────────────────┼───────────────────┐
                    ▼                   ▼                   ▼
             ┌────────────┐      ┌────────────┐      ┌────────────┐
             │ Router      │      │ Router      │      │ Router      │
             │ instance N  │ ...  │ instance N  │      │ instance N  │
             │ (stateless) │      │ (stateless) │      │ (stateless) │
             └──────┬──────┘      └──────┬──────┘      └──────┬──────┘
                    └───────────────────┼───────────────────┘
                                         ▼
                     ┌──────────────────────────────────┐
                     │ Durable store (sqlx: postgres)    │
                     │  - mesh_log (audit feed)          │
                     │  - receipts (redeemed retained)   │
                     │  - reconciliation records         │
                     └──────────────────┬─────────────────┘
                                         │
        ┌────────────────────────────────┼────────────────────────────────┐
        ▼                                ▼                                ▼
┌───────────────┐              ┌──────────────────┐              ┌───────────────┐
│ x402 facilitator│             │ On-chain Stylus   │              │ Observability │
│ (verified, live) │            │ verifier (deployed)│             │ stack (metrics│
│ + reconciliation │            │ + 1Shot broadcast  │             │ traces, logs, │
│ job               │            │ (live)             │             │ alerts)       │
└───────────────┘              └──────────────────┘              └───────────────┘
```

The vault (`vault.rs`) is deliberately **unchanged** in this target
architecture: ephemeral-keypair-per-mint, zeroize-on-drop, chaos-tested
signing is already the right design and scales horizontally with zero
modification — it holds no state between calls. Everything in this
section is additive around it, not a rewrite of it.

## 5. Non-functional requirements (proposed — none exist today)

No SLOs, capacity targets, or recovery objectives are currently defined
anywhere in the repo. Proposed starting targets, to be revised once real
traffic data exists:

| Property | Target | Rationale |
|---|---|---|
| Availability | 99.9% for the router tier | Standard SaaS-tier target; revisit once §3.7 (multi-instance) lands — single-instance today cannot meaningfully commit to this. |
| `/execute` P99 latency | < 300ms (excluding facilitator/chain round-trip) | The mint path itself (canonicalize, sign, chaos-test) is CPU-bound and fast; this budget should be dominated by the settlement backend call, which is the part to actually optimize/cache. |
| RPO (audit log) | 0 (synchronous durable write before `post()` returns) | "If it isn't in the feed, it didn't happen" is a stated product invariant — it must survive a crash, not just a clean shutdown. |
| RPO (receipt ledger) | 0 | A lost receipt record is an unreconcilable payment. |
| RTO | < 15 min | Restore from the durable store behind a fresh router instance. |
| Chaos-test failure rate | 0, alerting on any nonzero occurrence | A single chaos-test failure in production is a crypto-integrity incident, not routine noise — see §3.5. |

## 6. Phased roadmap

Merges the existing roadmap items (`README.md` G-1 through G-6) with the
enterprise-track items from this document (E-1 through E-8), sequenced by
dependency, not by section order above.

| Phase | Item | Depends on | Existing ref |
|---|---|---|---|
| 1 | **E-1** Durable storage (mesh log + receipt ledger) | — | §3.1 |
| 1 | **E-2** `cargo audit`/`deny` + SBOM in CI | — | §3.9 |
| 1 | **E-3** Health/readiness endpoints, graceful shutdown, `Dockerfile` | — | §3.8 |
| 2 | **E-4** Observability (metrics/traces/logs export, SLO alerting) | E-1 (metrics need something to measure against) | §3.5 |
| 2 | **E-5** OpenAPI spec + versioned API + unified error envelope | — | §3.6 |
| 2 | **G-4 completion**: live facilitator testnet verification + reconciliation job | E-1 (reconciliation needs durable receipts) | Already landed as opt-in seam; this closes the "unverified" gap |
| 3 | **E-6** AuthN/AuthZ (per-agent identity, API keys/mTLS for network exposure) | E-1 | §3.2 |
| 3 | **E-7** Horizontal scaling (stateless router instances behind LB) | E-1, E-6 | §3.7 |
| 4 | **G-1** Stylus verifier contract deployed (testnet, then mainnet) | — (independently deployable; the reference implementation already exists) | §3.4 |
| 4 | **G-5** Live 1Shot broadcast | G-1 | §3.4 |
| 4 | **E-8** Fuzzing harness, third-party pen test, legal/compliance review | E-6 (auth should exist before external pen test) | §3.9, §3.10 |
| — | G-2 (WASM vault surface), G-3 (Svelte 5 scaffold), G-6 (OKX manifest submission) | — | Unchanged from existing roadmap; orthogonal to this document |

Phase 1 items are chosen specifically because they're prerequisites with
no architectural regret risk — durable storage, CI security scanning, and
health checks don't need to be revisited once built, whereas building
auth or horizontal scaling before durable storage would.

## 7. Traceability matrix

| Requirement | Current status | Component | Target phase |
|---|---|---|---|
| Audit feed survives process restart | ❌ Missing (in-memory `Vec`, capped) | `mesh.rs` | 1 (E-1) |
| Receipt record survives redemption | ❌ Missing (removed on redeem) | `x402.rs` | 1 (E-1) |
| Dependency CVE scanning | ❌ Missing | CI | 1 (E-2) |
| Health/readiness endpoints | ❌ Missing | `server.rs` | 1 (E-3) |
| Containerized deployment | ❌ Missing | repo root | 1 (E-3) |
| Metrics export | ❌ Missing (tracing calls only) | all | 2 (E-4) |
| Versioned/schema'd API | ❌ Missing | `server.rs` routes | 2 (E-5) |
| Live facilitator verified on testnet | ⚠️ Seam exists, unverified | `settlement_backend.rs` | 2 |
| Per-agent authentication | ❌ Missing | new | 3 (E-6) |
| Multi-instance horizontal scaling | ❌ Missing (single process) | `server.rs` | 3 (E-7) |
| On-chain Stylus verifier deployed | ⚠️ Reference implementation only | `settlement.rs` | 4 (G-1) |
| Live 1Shot broadcast | ❌ Stubbed (`local://`) | `settlement.rs` | 4 (G-5) |
| Fuzzing (JCS, vault) | ❌ Missing | new | 4 (E-8) |
| Third-party pen test | ❌ Missing | — | 4 (E-8) |
| Payment jurisdiction/compliance review | ❌ Missing | — | 4 (E-8) |
| JIT credential minting, chaos-tested | ✅ Done | `vault.rs` | — |
| Replay prevention (burned receipts) | ✅ Done | `x402.rs` | — |
| Payload-bound, TTL'd signatures | ✅ Done | `vault.rs` | — |
| Single enforcement point (UI/loopback/OKX share one gate) | ✅ Done | `server.rs`, `okx_bridge.rs` | — |
| Request body size limit | ✅ Done | `server.rs` | — |
| Stale-receipt eviction | ✅ Done | `x402.rs` | — |
| Mesh log bounded growth | ✅ Done | `mesh.rs` | — |

## 8. What this document deliberately does not change

- **The vault's core design** (§4) — ephemeral-per-mint, zeroized,
  chaos-tested, payload-bound, strict TTL. It's correct and it scales.
- **The single-enforcement-point property** — every proposed addition
  (auth, durability, observability) wraps around the existing
  `handoff_router`; none of them introduce a second path to `/execute`
  that could bypass the x402 gate.
- **The zero-telemetry/local-first sovereignty posture as an available
  mode** — the target architecture in §4 is what a *hosted, multi-tenant*
  deployment needs; the current single-operator desktop mode remains a
  valid, smaller deployment topology of the same codebase (durable
  storage can be sqlite-local, auth can be a no-op for a single trusted
  operator) rather than something this spec obsoletes.
