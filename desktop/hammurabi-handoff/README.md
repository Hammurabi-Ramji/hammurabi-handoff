# Hammurabi Handoff — JIT Vault ASP

[![CI](https://github.com/Hammurabi-Ramji/hammurabi-handoff/actions/workflows/handoff-ci.yml/badge.svg)](https://github.com/Hammurabi-Ramji/hammurabi-handoff/actions/workflows/handoff-ci.yml)

**Credentials that die after one use. Agents you can actually read.**

A local-first, zero-telemetry Agent Service Provider (ASP): AI agents announce
intents in a human-readable feed, an x402 gate meters every privileged action,
and a JIT vault mints ephemeral, payload-bound Ed25519 credentials with a
strict 60-second TTL — then formats the signed intent for gasless settlement.

## The problem

Every AI agent today runs with a loaded gun pointed at your wallet:

- **Standing API keys** — one exploit leaks everything
- **Opaque agent-to-agent traffic** — you can't see what your agents are doing
- **No human-in-the-loop at the transport layer** — you can't intervene mid-flow
- **Post-hoc logging** — you learn about failures after the damage

## The solution: Legible Autonomy

Named for the Code of Hammurabi — the first written legal code — this project
brings cryptographic law to autonomous agents, in two fused layers:

### 1. RAMesh (Readable Agent Mesh)

Every pipeline actor posts human-readable lines to `#handoff-ops` — an
IRC-style, append-only feed rendered live in the Tauri shell. If it isn't in
the feed, it didn't happen:

```
[   0] <agent/scout-1>   Opportunity found: ETH limit order at $3,100. Requesting execution.
[   1] <gate/x402>       Action restricted. x402 payment of 0.50 USDC required.
[   2] <gate/x402>       x402 payment of 0.50 USDC settled for intent 01JZ… — receipt 01JZ….
[   3] <vault/handshake> Payment confirmed. Ephemeral Handshake credential minted.
[   4] <relay/1shot>     Settlement envelope 01JZ… staged for gasless Stylus broadcast…
```

### 2. Hammurabi Handshake (JIT Vault)

Each credential is:

- **Payload-bound** — Ed25519 over `canonical(intent) ‖ 0x00 ‖ issued_at`
  (RFC 8785 canonical JSON); change one field and it's void
- **Time-limited** — strict 60-second TTL, enforced structurally *and* at
  verification
- **One-use** — x402 receipts burn on redemption; the credential id doubles
  as the settlement idempotency key
- **Ephemeral** — a fresh keypair per mint, seed zeroized before the mint
  function returns; the secret never exists outside one stack frame
- **Chaos-tested** — before release, every credential survives its own attack
  suite: tampered payloads, shifted timestamps, bit-flipped signatures, and
  wrong keys must all be rejected, or the credential is destroyed

## Architecture

| Layer | Component | Where |
|-------|-----------|-------|
| Substrate | Tauri 2 (local-first, loopback only) | `src/app.rs`, `src/commands.rs` |
| Mesh | RAMesh IRC-style feed | `src/mesh.rs` |
| Intent | Structured agent payloads | `src/intent.rs` |
| Monetization | x402 middleware (Axum) | `src/x402.rs` |
| Vault | JIT Ed25519 credentials | `src/vault.rs` (on `core/iedb-crypto`) |
| Settlement | 1Shot/Stylus envelope formatting | `src/settlement.rs` |
| Backend | One Axum router — one enforcement point | `src/server.rs` |
| OKX edge | Marketplace invocation → vault bridge | `src/okx_bridge.rs` (on `tooling/iedb-oka`) |

**Single enforcement point:** the Tauri UI, loopback HTTP agents, **and OKX
marketplace invocations** all drive their requests through the *same* Axum
router (in-process `tower::oneshot`). An `OkxInvocation` from the storefront is
translated by `okx_bridge` and dispatched through the identical x402 gate — so
there is no code path around it from any surface. The dependency direction
(`hammurabi-handoff` → `iedb-oka`) keeps OKX schema churn contained to the
bridge; the vault never imports anything OKX-specific.

**Sovereignty posture:** no cloud persistence, no telemetry, no network I/O in
core logic. The 1Shot endpoint is a `local://` stub; live adapters only ever
enter as opt-in features. Cryptographic paths take caller-supplied time.

## Quick start

```bash
# From the IEDB workspace root
cargo test -p hammurabi-handoff        # 27 tests: full pipeline, TTL, replay, chaos, OKX edge, eviction
cargo build -p hammurabi-handoff --release

# Desktop shell (static ui/dist ships in-repo — no npm step)
cargo run -p hammurabi-handoff --features tauri-app
```

The shell walks the four-step loop: **Announce → Request execution (402) →
Settle x402 → Execute with receipt**, with the feed streaming live. Headless
agents get the same surface on `http://127.0.0.1:3402`:

```
GET  /mesh/feed         the transcript
POST /agent/announce    Phase 1 — post the mock intent
POST /x402/settle       Phase 2 — mock payment resolution  {"intent_id": …}
POST /execute           Phases 3+4 — x-intent-id + x-402-receipt headers
```

## HTTP flow example

```bash
INTENT=$(curl -s -X POST http://127.0.0.1:3402/agent/announce)
ID=$(echo "$INTENT" | jq -r .intent_id)

# Unpaid → 402 + terms
curl -s -X POST http://127.0.0.1:3402/execute \
  -H "x-intent-id: $ID" -H "content-type: application/json" -d "$INTENT"

# Pay, then execute
RECEIPT=$(curl -s -X POST http://127.0.0.1:3402/x402/settle \
  -H "content-type: application/json" -d "{\"intent_id\":\"$ID\"}" | jq -r .receipt_id)
curl -s -X POST http://127.0.0.1:3402/execute \
  -H "x-intent-id: $ID" -H "x-402-receipt: $RECEIPT" \
  -H "content-type: application/json" -d "$INTENT"
# → 1Shot envelope: verifiable signature, 60s validity window, idempotency key
```

## Screenshots

<!-- Drop-in slot: save the 5 captures from docs/VIDEO_PREP.md Part A into
     assets/screenshots/ using the filenames below — nothing else to edit,
     these embeds pick them up automatically. -->

| Idle | Announce | 402 restricted |
|------|----------|-----------------|
| ![Idle shell](assets/screenshots/01-idle.png) | ![Intent announced](assets/screenshots/02-announce.png) | ![x402 restriction](assets/screenshots/03-restricted.png) |

| Credential minted | Replay bounced |
|---------------------|------------------|
| ![Credential minted](assets/screenshots/04-minted.png) | ![Burned receipt replay bounced](assets/screenshots/05-replay-bounced.png) |

## Demo video

<!-- Drop-in slot: save the recording + thumbnail from docs/VIDEO_PREP.md
     Part B as assets/video/demo.mp4 and assets/video/thumbnail.png. -->

[![Watch the 90-second demo](assets/video/thumbnail.png)](assets/video/demo.mp4)

*90 seconds: announce → 402 refusal → x402 settle → credential mint → replay
bounce. Script: [`docs/DEMO_SCRIPT.md`](docs/DEMO_SCRIPT.md).*

## Continuous integration

`.github/workflows/handoff-ci.yml` (badge above, live on `main`) gates this
crate + `iedb-oka` on four checks: `cargo fmt --check`, `cargo clippy -- -D
warnings`, `cargo build`, `cargo test` (27/27 handoff, 7/7 oka, 38/38
iedb-crypto). It is **scoped to the submission crates on purpose** — the
wider IEDB workspace has legacy crates that don't pass `-D warnings`, so a
workspace-wide gate would be red and misleading.

## Roadmap

See `C:\IEDB\docs\specs\sovereign-stack-phase0-alignment.md` §3: Stylus
verifier contract (G-1), WASM vault surface via `wasm/iedb-wasm` (G-2),
Svelte 5 scaffold (G-3), OKX.AI ASP manifest submission (G-6).

**G-4 (live x402 facilitator) — wired, opt-in, off by default:**
`settlement_backend.rs` adds a `SettlementBackend` behind the `x402-live`
feature flag (off by default; pulls in `reqwest` only when enabled). The
composition root (`app.rs`) arms it automatically at startup via
`settlement_backend::from_env_or_default()` — build with
`--features tauri-app,x402-live` and set every `HANDOFF_X402_*` env var and
the live facilitator is what answers `/x402/settle`; leave any of them unset
and it falls back to the mock rather than half-configuring. The gate, receipt
ledger, and vault are unchanged — `/x402/settle` now accepts an optional
client `PaymentPayload` that the mock backend ignores and the live backend
requires. **Not yet verified against a live facilitator or a testnet** —
confirm the wire format against your facilitator's actual API and round-trip
on testnet USDC before pointing this at real money.

**G-5 (live 1Shot broadcast)** remains fully roadmap — `settlement.rs`'s
`local://` endpoint is unchanged.

## License

MIT
