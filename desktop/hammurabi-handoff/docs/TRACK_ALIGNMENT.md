# Track Alignment Summary

> **Headline track: Revenue Rocket ($20,000).** The whole ASP is a working
> revenue instrument — every credential mint is a metered 0.50 USDC x402
> settlement, not a promise of future monetization. Lead every judge and
> every post with this.

## Revenue Rocket ($20,000)
**Why we qualify:**
- The monetization mechanism is the product, not a bolt-on: `POST /execute`
  returns HTTP **402** with terms until a **0.50 USDC** x402 `exact`-scheme
  settlement is presented; only then does the vault mint. Revenue is
  structurally inseparable from value delivery.
- Per-invocation metering is real and observable: the 1Shot settlement
  envelope is built and returned on every mint, live in the demo — the judge
  watches money change hands, then watches the credential the payment bought.
- Reusable revenue rail: any local agent that can POST JSON gets metered
  execution through the identical x402 gate (UI and OKX edge share one
  enforcement point via `okx_bridge` → same router). Pricing is declared in
  `asp/pricing.json` (500 000 micro-USDC), not hand-waved.
- Unit economics are legible: one action = one settlement = one single-use
  credential. No subscriptions to reconcile, no standing balance to secure.

**Emphasize:** the x402 gate refusing unpaid execution, the settlement, and
the 1Shot envelope — in that order, on-screen, in real time. On-chain
broadcast is **staged** (per `SUBMISSION_LEDGER.md`); the *metering and
settlement accounting* are live today.

## Best Product ($20,000)
**Why we qualify:**
- Working pipeline end-to-end today: announce → 402 → settle → mint →
  envelope, 19 automated tests including the full E2E transcript
- Documented: README, ARCHITECTURE, demo script, ASP manifest
- One-command run: `cargo run -p hammurabi-handoff --features tauri-app`
  (static UI ships in-repo — no npm step)

**Emphasize:** the judge can watch the whole security story happen in the
feed within ten seconds of launching; the replay-bounce is live, not staged.

## Creative Genius ($20,000)
**Why we qualify:**
- Compositional novelty: IRC-semantics feed + x402 metering + JIT
  payload-bound credentials + chaos-tested minting is not done elsewhere
- Architectural inversion: the chat channel IS the observability layer
- Per-mint self-attack ("chaos testing") turns library trust into a proof
  carried on every credential

**Emphasize:** "credentials that die after one use" + the vault attacking its
own signature before releasing it.

## Software Utility ($7,500)
**Why we qualify:**
- Drop-in credential layer: any local agent that can POST JSON to
  `127.0.0.1:3402` gets metered, credentialed, auditable execution
- The x402 gate is reusable Axum middleware; the vault is a two-function API
  (`mint`, `verify`) on a self-contained envelope format

**Emphasize:** integration is an HTTP call, not an SDK.

## Social Buzz ($10,000)
**Why we qualify:**
- Quotable concept, visual feed, clear narrative arc (problem → gate → mint
  → death of the credential)

**Strategy:** four posts over four days (copy in `SOCIAL_POSTS.md`), each
with a distinct hook; engage OKX.AI accounts; hashtags `#OKXAI
#AgentInfrastructure`.

## Honesty constraints (all tracks)
Do **not** claim: on-chain confirmation (settlement is stubbed until
G-1/G-5), live x402 settlement (mocked until G-4), `/kick` moderation
(roadmap), or multi-process agent discovery (roadmap). The demo script and
manifest already respect these; keep the posts consistent with them.
