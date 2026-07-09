# OKX.AI Submission Ledger

Reconciles the submission-package draft (received via chat 2026-07-07,
authored by an external text-only model) against the actual implementation.
Rule of precedence: the code and the standing sovereignty directives win;
every correction is logged here.

## Corrections applied to the draft

| Draft claim | Reality (shipped) |
|---|---|
| TTL "30 seconds" | **60 seconds**, strict, structural + verify-time (`vault::CREDENTIAL_TTL_SECS`) |
| Mint fee "0.0001 ETH equivalent" | **0.50 USDC** (500 000 micro), x402 `exact` scheme |
| "ETH/USDC @ 3200" | **$3,100** — the exact Legible Autonomy narrative string |
| IRC server on `localhost:6667`, mIRC clients, `/kick` | **RAMesh**: in-process IRC-*semantics* feed + Tauri events. No network ircd (attack surface for zero benefit); operator intervention beyond watching is roadmap |
| Separate `hammurabi-mesh` / `hammurabi-vault` binaries | **One binary** (`hammurabi-handoff`, `--features tauri-app`); headless surface on `127.0.0.1:3402` |
| "Nonce" field in credential | No explicit nonce: replay is prevented by **single-use burned receipts** + timestamp-bound signatures + idempotency-keyed settlement |
| "Agent-bound" credential | **Payload-bound** (the payload contains `agent_handle`, so agent binding follows) |
| `mesh_discovery` capability | Not implemented — omitted from `asp/manifest.json` rather than advertised falsely |
| "Transaction confirmed on Arbitrum Stylus" demo line | Settlement is **stubbed** (`local://` 1Shot endpoint); demo says "staged for gasless Stylus broadcast" — do not claim on-chain confirmation |
| SHA-256 payload hash | **BLAKE3** digest over **RFC 8785 canonical** JSON (workspace standard) |

## Produced artifacts (repo-accurate)

- [x] `README.md` — full project documentation
- [x] `docs/ARCHITECTURE.md` — as-built topology, minting data flow, design decisions
- [x] `docs/DEMO_SCRIPT.md` — 90-second script using only real app behavior
- [x] `docs/PITCH_DECK.md` — slide-by-slide
- [x] `docs/TRACK_ALIGNMENT.md` — four tracks + honesty constraints
- [x] `docs/SOCIAL_POSTS.md` — corrected X copy (60s TTL, no on-chain claims)
- [x] `asp/manifest.json` + `asp/capabilities.json` + `asp/pricing.json` — 1:1 with real endpoints; mock/stub status declared honestly
- [x] `assets/logo.svg` — stele logo per draft Image-6 spec (512×512; export PNG for upload)
- [x] `assets/architecture.svg` — corrected system diagram per draft Image-1 spec (1200×800)
- [x] `icons/icon.ico` — minimal placeholder (gold 16×16); regenerate from logo.svg for release
- [x] Tests green (`cargo test -p hammurabi-handoff`, **22/22**); workspace release build green
- [x] Desktop shell compiles: `cargo check -p hammurabi-handoff --features tauri-app` exit 0 — static `ui/dist` embeds cleanly, no npm step
- [x] **OKX edge wired (roadmap B1):** `src/okx_bridge.rs` dispatches `iedb-oka` `OkxInvocation`s through the same in-process x402-gated router as the UI. 3 tests: full OKX loop (announce→402→settle→200 envelope→feed), burned-receipt replay bounce from the OKX edge, unknown-capability error. Dependency direction handoff→oka keeps OKX schema churn contained.
- [x] **Stylus reference verifier (roadmap B2a):** `settlement::verify_handoff` — the byte-level, `no_std`-portable logic the on-chain contract would run (structural TTL + validity window + Ed25519 over `canonical ‖ 0x00 ‖ issued_at`). `OneShotEnvelope::verify` proves the envelope self-verifies off its wire form; failure-mode test covers tamper/expiry/TTL-stretch/bad-sig. On-chain deployment remains roadmap G-1 — the reference is not claimed as deployed. `cargo test -p hammurabi-handoff` = 23/23.

## Still owed (human / external — cannot be produced in-repo)

- [ ] **`author.url` caveat (accepted 2026-07-08):** `asp/manifest.json`
      `author.url` is `https://github.com/hammurabi/iedb`, which 404s until the
      repo is made public. Owner ships as-is; repo visibility is to be toggled
      to public **concurrently with** the OKX submission so the link resolves
      before any judge clicks it. Must be verified live at submission time.
- [ ] OKX.AI ASP account + gallery form + Google form
- [ ] X account; schedule the four posts from `SOCIAL_POSTS.md`
- [ ] Demo video recording (script ready) + thumbnail
- [x] PNG exports: `assets/logo.png` (512×512, transparent) + `assets/banner.png`
      (1200×630, `#0b0e14`). **Banner spec (ratified 2026-07-08):** purpose-built
      hero (not an architecture crop), two-line tagline **"Hammurabi Handoff" /
      "Zero-Telemetry · 1Shot Settlement"** with sign-off *"Keep the faith, the
      good exists."* This two-line form supersedes the earlier single-line
      literal ("… : Zero-Telemetry. 1Shot Settlement.") — ratified as
      semantically identical and more legible. Source: `assets/banner.svg`.
- [ ] Live screenshots of the running shell (3–5, per checklist)
- [ ] GitHub repo publication decision (this crate lives inside the private IEDB workspace)
- [ ] Live ASP deployment (requires G-4/G-5/G-6 adapters — see `docs/specs/sovereign-stack-phase0-alignment.md` §3 at the workspace root)

## Review-finding deferrals (adversarial review, 2026-07-07)

An in-session multi-agent review raised 9 findings; verification agents were
cut off by a session limit, so these were triaged manually. Two were fixed
immediately (broadcast-lag killing the feed forwarder; sender-string HTML
injection + boot race in `ui/dist`). The rest are real-but-mock-scope,
deferred until the corresponding adapter goes live:

1. **Receipt ids visible on the ungated feed** — on loopback with a mock
   settler this grants nothing (anyone local can mint receipts anyway); must
   be redacted the moment G-4 (live x402) lands.
2. **No receipt expiry/eviction** — unbounded map + indefinitely-valid
   receipts; add TTL eviction with G-4.
3. **Unbounded mesh log** — cap/ring-buffer before any long-running deployment.
4. **Caller-controlled `agent_handle` as feed sender** — audit-trail
   impersonation; bind handles to authenticated agent identities when A2A
   goes multi-process.
5. **`Option<Json<AgentIntent>>` treats malformed JSON as "no body"** —
   acceptable for the demo announce endpoint; tighten to a 400 when real
   agents integrate.
6. **Announce always narrates the fixed mock line** — intentional for the
   Legible Autonomy demo string; derive the line from the intent when custom
   intents matter.
