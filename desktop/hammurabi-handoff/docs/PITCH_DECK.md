# Pitch Deck — Slide-by-Slide

## Slide 1 — Title
**Hammurabi Handoff — JIT Vault**
*Credentials that die after one use. Agents you can actually read.*
(Logo: `assets/logo.svg`)

## Slide 2 — The problem
Every AI agent today runs with a loaded gun pointed at your wallet.
- Standing API keys → one exploit leaks everything
- Opaque A2A traffic → you can't see what agents are doing
- No transport-layer human-in-the-loop → you can't intervene mid-flow

## Slide 3 — The inversion
The chat channel IS the observability layer.
Screenshot of the live feed: five lines tell the whole security story —
intent, refusal, payment, mint, settlement staging.

## Slide 4 — The gate (x402)
Nothing executes unpaid. HTTP 402 + machine-readable terms; 0.50 USDC per
execution; receipts are single-use and intent-bound — they burn on
redemption. ~200 lines of readable Axum middleware.

## Slide 5 — The vault (Hammurabi Handshake)
Per action: a fresh Ed25519 keypair, a signature over the RFC 8785 canonical
payload + timestamp, a strict 60-second life, and the seed zeroized before
the mint returns. **There is no key to steal at rest.**

## Slide 6 — Chaos testing (the differentiator)
Before any credential is released, the vault attacks it: tampered payload,
shifted timestamp, 8 random signature bit-flips, wrong key. All must be
rejected or the credential is destroyed. Every credential carries its own
attack report.

## Slide 7 — Settlement (Stylus-ready)
The envelope is self-contained: intent + pubkey + signature + window +
idempotency key. A Stylus contract re-runs the exact verification the Rust
reference implements. Gasless via 1Shot relay. (Stubbed today — schema
frozen, adapter opt-in.)

## Slide 8 — Sovereignty
Loopback-only. Zero telemetry. Zero cloud persistence. One enforcement
point — the UI and headless agents share one router; there is no path
around the gate.

## Slide 9 — Live demo
The four buttons: Announce → Execute (bounces 402) → Settle → Execute
(mints). Then press Execute again and watch the burned receipt bounce.

## Slide 10 — Ask
Try it: `cargo run -p hammurabi-handoff --features tauri-app`.
Roadmap: Stylus verifier, WASM vault, live x402 facilitator — all opt-in,
core stays sovereign.
