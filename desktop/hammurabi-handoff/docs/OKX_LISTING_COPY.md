# OKX.AI ASP Listing Copy — paste-ready

Grounded in the shipped code. Every claim here is backed by `README.md`,
`asp/manifest.json`, or a green test. Honesty constraints (see
`SUBMISSION_LEDGER.md`) are respected: settlement broadcast is **staged**, not
on-chain-confirmed; RAMesh is an in-process feed, not a network IRC server.

---

## Field: Project name
Hammurabi Handoff

## Field: Tagline / one-liner (≤ 80 chars)
Credentials that die after one use. Agents you can actually read.

## Field: Category / type
Agent Service Provider (ASP) — infrastructure / credential & payment layer

## Field: Headline track
**Revenue Rocket ($20K)** — the ASP *is* a revenue instrument: every credential
mint is a metered 0.50 USDC x402 settlement, inseparable from value delivery.

## Field: Short description (≤ 280 chars)
A local-first, zero-telemetry credential vault for AI agents. Every privileged
action is metered by an x402 payment gate (0.50 USDC), then granted a
single-use, payload-bound Ed25519 credential with a strict 60-second life — all
narrated in a human-readable audit feed you can watch in real time.

## Field: Full description

**The problem.** Every AI agent today runs with a loaded gun pointed at your
wallet. Standing API keys mean one exploit leaks everything. Agent-to-agent
traffic is opaque. There's no human-in-the-loop at the transport layer, and
logging is post-hoc — you learn about failures after the damage is done.

**The solution: Legible Autonomy.** Named for the Code of Hammurabi — the first
written legal code — Hammurabi Handoff brings cryptographic law to autonomous
agents in two fused layers:

1. **RAMesh (Readable Agent Mesh).** Every actor in the pipeline posts a
   human-readable line to an append-only feed rendered live in the desktop
   shell. If it isn't in the feed, it didn't happen. The chat channel *is* the
   observability layer.

2. **Hammurabi Handshake (JIT Vault).** Nothing executes until an x402 payment
   clears. Then the vault mints an ephemeral Ed25519 credential that is
   **payload-bound** (Ed25519 over RFC 8785 canonical JSON — change one field
   and it's void), **time-limited** (strict 60-second TTL, enforced structurally
   *and* at verification), **single-use** (the x402 receipt burns on
   redemption), and **ephemeral** (a fresh keypair per mint, seed zeroized
   before the function returns — the secret never exists outside one stack
   frame). Before release, every credential survives its own attack suite:
   tampered payloads, shifted timestamps, bit-flipped signatures, and wrong keys
   must all be rejected, or the credential is destroyed.

**Why it's a revenue engine.** Monetization isn't bolted on — it's the gate.
`POST /execute` returns HTTP 402 with terms until a 0.50 USDC x402 `exact`-scheme
settlement is presented; only then does the vault mint. One action = one
settlement = one credential. The 1Shot settlement envelope is built and returned
on every mint. (On-chain broadcast is staged for gasless Arbitrum Stylus; the
metering and settlement accounting are live today.)

**Sovereignty posture.** Binds to `127.0.0.1` only. No telemetry. No cloud
persistence. No standing keys. No network I/O in core logic — live adapters are
opt-in features. Cryptographic paths take caller-supplied time.

## Field: Key features (bullets)
- x402 payment gate — HTTP 402 + terms until 0.50 USDC clears
- Single-use, payload-bound Ed25519 credentials, strict 60-second TTL
- Per-mint chaos test: the vault attacks its own signature before releasing it
- RAMesh live audit feed — every gate/vault/relay action as a readable line
- One enforcement point: UI, loopback HTTP agents, and OKX marketplace
  invocations all route through the same Axum router — no bypass path
- Zero-telemetry, loopback-only, no standing secrets at rest

## Field: How it works (numbered)
1. Agent announces intent to the RAMesh feed (`POST /agent/announce`)
2. `POST /execute` without payment → **HTTP 402** + settlement terms
3. `POST /x402/settle` resolves the 0.50 USDC payment → single-use receipt
4. `POST /execute` with the receipt → vault mints the chaos-tested credential and
   returns the 1Shot settlement envelope
5. Replay the burned receipt → it bounces with a fresh 402

## Field: Pricing
x402, `exact` scheme, USDC. Credential mint: **0.50 USDC** (500,000 micro-USDC).
Announce / settle / feed / verify: free. Verification is a pure function — the
envelope is self-contained.

## Field: Tech stack
Rust · Tauri 2 (local-first desktop shell) · Axum (x402 middleware) ·
Ed25519 + BLAKE3 + RFC 8785 JCS (`iedb-crypto`) · 1Shot/Arbitrum Stylus envelope
formatting (broadcast staged) · `iedb-oka` OKX marketplace edge

## Field: Repository
https://github.com/Hammurabi-Ramji/hammurabi-handoff

## Field: Run it (one command, no npm step)
```
cargo run -p hammurabi-handoff --features tauri-app
```
Tests: `cargo test` → hammurabi-handoff 29/29 · iedb-crypto 38/38 · iedb-oka 7/7

## Field: Team
Hammurabi Coding Company, LLC

## Field: Tracks applied for
- **Revenue Rocket ($20K)** — headline; x402 metering as the product engine
- **Best Product ($20K)** — working E2E pipeline, 29 tests, one-command run
- **Creative Genius ($20K)** — compositional novelty + per-mint self-attack
- **Software Utility ($7.5K)** — drop-in credential layer; integration is an
  HTTP call, not an SDK
- **Social Buzz ($10K)** — four-post launch thread (see `SOCIAL_POSTS.md`)

## Honesty note for judges (verbatim, on the record)
Live x402 settlement and on-chain broadcast are **mocked/staged** in this build
(local facilitator + `local://` 1Shot endpoint). What is production-ready and
locally green-tested: the ASP core, the x402 gate, JIT vault minting, the
chaos-test suite, and the OKX edge. We document what's real vs. staged rather
than blur the line — that transparency is the point.
