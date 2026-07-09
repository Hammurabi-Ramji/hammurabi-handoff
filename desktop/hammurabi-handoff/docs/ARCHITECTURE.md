# Architecture Overview

## System topology (as built)

```
┌────────────────────────────────────────────────────────────────┐
│                User's machine — one Tauri process              │
│                                                                │
│  ┌──────────────┐   ramesh://message    ┌───────────────────┐ │
│  │   WebView    │◄──────────────────────│   RAMesh MeshBus  │ │
│  │  (ui/dist)   │   invoke() commands   │   #handoff-ops    │ │
│  └──────┬───────┘                       └─────────▲─────────┘ │
│         │ tower::oneshot (in-process)             │ posts     │
│         ▼                                         │           │
│  ┌─────────────────────────────────────────────────────────┐  │
│  │              ONE Axum router — ONE gate                 │  │
│  │                                                         │  │
│  │  GET  /mesh/feed ────────────── transcript              │  │
│  │  POST /agent/announce ───────── Phase 1: intent         │  │
│  │  POST /x402/settle ──────────── Phase 2: mock payment   │  │
│  │  POST /execute ──► x402 guard ──► Handshake vault       │  │
│  │                    (402 or       (JIT mint, chaos test, │  │
│  │                     burn receipt) 60s TTL)               │  │
│  │                                   │                      │  │
│  │                                   ▼                      │  │
│  │                            1Shot envelope                │  │
│  │                            (local:// stub)               │  │
│  └─────────────────────────────────────────────────────────┘  │
│         ▲                                                      │
│         │ same router, same state                              │
│  127.0.0.1:3402  ◄── headless local agents (loopback ONLY)     │
└────────────────────────────────────────────────────────────────┘
                     │ (future, G-1/G-5: opt-in)
                     ▼
        1Shot relay ──► Arbitrum Stylus `verify_handoff`
```

Key structural property: the WebView and headless agents reach the vault
through the **same router instance** — the Tauri commands dispatch via
in-process `tower::oneshot`, so the x402 guard cannot be bypassed from any
surface.

## Data flow: credential minting

1. Agent posts intent → the announcement line lands in `#handoff-ops`.
2. `POST /execute` without a receipt → the x402 middleware refuses with
   HTTP 402 + machine-readable terms, and narrates the refusal to the feed.
3. `POST /x402/settle` (mock in this build) mints a **single-use,
   intent-bound receipt**.
4. Retried `/execute` with `x-402-receipt`: the guard **burns** the receipt
   and injects it into the request — holding a `PaymentReceipt` is the
   type-level precondition for minting; only the gate constructs one.
5. The vault canonicalizes the intent (RFC 8785; floats refused), digests it
   (BLAKE3), generates an **ephemeral Ed25519 keypair**, and signs
   `canonical(intent) ‖ 0x00 ‖ issued_at`.
6. **Chaos testing** before release: honest verify must pass; tampered
   payload, ±1 s timestamp shift, 8 random signature bit-flips, and a wrong
   verifying key must all be rejected — otherwise the credential is
   destroyed, never returned.
7. The signing key drops (seed zeroized) before the mint function returns.
   The credential carries public material only: pubkey, signature, digest,
   `issued_at`, `expires_at = issued_at + 60` (strict).
8. The settlement envelope packages intent + credential + receipt id under
   the credential id as idempotency key. It is **self-contained**: a
   verifier needs nothing but the envelope.
9. Replay: the burned receipt yields a fresh 402. Expiry: verification
   rejects at `now ≥ expires_at` and refuses pre-dated use at
   `now < issued_at`; a stretched `expires_at` is rejected structurally.

### x402 sequence (in-process, one gate)

This is the *actual* flow — no network, no IRC message-passing. The caller is
any of the three surfaces (Tauri UI command, loopback HTTP agent, or an OKX
`OkxInvocation` via `okx_bridge`); all three drive the **same** Axum router by
`tower::oneshot`, so the x402 gate is the single choke point for every one.

```
 Caller                    Axum router / x402 gate            Vault + settlement
 (UI | loopback | OKX)     (server.rs · x402.rs)              (vault.rs · settlement.rs)
   │                                │                                   │
   │  POST /execute (no receipt)    │                                   │
   │───────────────────────────────►│                                   │
   │                                │  no x-402-receipt →               │
   │        402 + payment terms     │  narrate "Action restricted" to   │
   │◄───────────────────────────────│  #handoff-ops feed                │
   │                                │                                   │
   │  POST /x402/settle {intent_id} │                                   │
   │───────────────────────────────►│  mock_settle → single-use,        │
   │        200 { receipt_id }       │  intent-bound PaymentReceipt      │
   │◄───────────────────────────────│                                   │
   │                                │                                   │
   │  POST /execute                 │                                   │
   │  x-intent-id + x-402-receipt   │                                   │
   │───────────────────────────────►│  redeem → BURN receipt;           │
   │                                │  inject PaymentReceipt ───────────►│  mint:
   │                                │  (only the gate constructs one)   │  canonical‖0x00‖issued_at,
   │                                │                                   │  ephemeral Ed25519, 60s TTL,
   │                                │                                   │  chaos-test, zeroize seed
   │        200 OneShotEnvelope      │◄──────────────────────────────────│  → self-contained envelope
   │◄───────────────────────────────│                                   │
   │                                │                                   │
   │  POST /execute (same receipt)  │  receipt already burned →         │
   │───────────────────────────────►│  402 (replay bounces)             │
   │◄───────────────────────────────│                                   │
```

## Key design decisions

**Why IRC semantics without an IRC server?** Human-readable by default is the
point — channels, handles, a transcript. A network ircd adds an attack
surface and a dependency for zero legibility gain on a single machine. The
`MeshBus` broadcast stream is bridgeable to a local ircd later, opt-in.

**Why Tauri?** Local-first sovereignty. No cloud dependency; the operator
owns the mesh, the vault, and the feed.

**Zero-JS / Zero-Node.js (clarification).** The backend and the desktop shell
contain no JavaScript dependencies and no Node.js runtime — the vault, gate,
mesh, OKX bridge, and settlement are pure Rust. The frontend is a hand-written
static bundle under `ui/dist` embedded at compile time by
`tauri::generate_context!`; there is no npm install, no bundler, and no Node
process at build, run, or deploy time. (`ui/src/App.svelte` is provided as the
source for a future Svelte 5 scaffold, but nothing in the shipped path executes
it or requires a toolchain.) This satisfies the spirit of the constraint: a
fully self-contained, local-first application with no JS runtime.

**Why JIT credentials?** Zero standing permissions. The blast radius of a
compromise is exactly one action for at most 60 seconds — and the secret key
never exists outside one stack frame, so there is nothing to steal at rest.

**Why x402?** HTTP 402 is the forgotten status code: agents pay per action,
no pre-funded accounts, no subscriptions. The gate is ordinary Axum
middleware — the monetization layer is ~200 lines you can read.

**Why caller-supplied time on crypto paths?** Deterministic tests, no ambient
clock authority, and the same discipline the wider IEDB workspace enforces in
`iedb-crypto::jcs`.

**Why RFC 8785 canonical JSON?** Signature-over-JSON is only sound if both
sides serialize identically; canonicalization (with float rejection) makes
the on-chain re-verification in Stylus (roadmap G-1) byte-exact.
