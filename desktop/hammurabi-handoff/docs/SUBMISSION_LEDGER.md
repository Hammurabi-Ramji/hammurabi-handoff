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

## Resolved since the 2026-07-08 draft

- [x] **GitHub repo publication:** public — `Hammurabi-Ramji/hammurabi-handoff`,
      confirmed via `gh repo view` (2026-07-26). `asp/manifest.json`
      `author.url` already points at the live repo and resolves; the earlier
      `github.com/hammurabi/iedb` 404 caveat no longer applies.
- [x] **CI workflow:** `.github/workflows/handoff-ci.yml` landed via PR #1
      (`claude/hammurabi-handoff-inspection-ghelfx`, merged 2026-07-26) —
      verified green on the merge commit and on the follow-up LICENSE/icon
      commit. The README badge is live, not aspirational.
- [x] **LICENSE file:** added (MIT) — the README claimed this since the
      first commit but the file didn't exist until now.
- [x] **`icons/icon.ico`:** regenerated from `assets/logo.svg` at 16–256px
      (was a self-flagged 16×16 placeholder).
- [x] **G-4 seam (live x402 facilitator):** landed via the same PR —
      `settlement_backend.rs`, feature-gated (`x402-live`, off by default).
      Honesty boundary unchanged: the seam is real and tested, the
      facilitator client is unverified against any live facilitator. Still
      roadmap to actually point it at one.
- [x] **G-4 wiring (2026-07-26, follow-up to the above):** the seam existed
      but nothing constructed `SettlementBackend::Facilitator` anywhere —
      `HandoffState::new()` always hardcoded the mock, feature flag or not.
      Added `HandoffState::with_backend`,
      `settlement_backend::from_env_or_default()`, and wired the composition
      root (`app.rs`) to call it. Now `--features tauri-app,x402-live` +
      every `HANDOFF_X402_*` env var actually arms the live backend at
      startup — previously it was a no-op even fully configured. Covered by
      `settlement_backend::env_wiring_tests` (only compiled/run under
      `--features x402-live`, so it doesn't touch the default CI gate).
- [x] **PROJECT_FACTS.md + SSOT enforcement (2026-08-03):** added
      `docs/PROJECT_FACTS.md` consolidating the numbers/roadmap labels
      restated across all 9 submission docs, and
      `tests/project_facts_ssot.rs`, which reads that file and fails the
      build if its Credential TTL / Execution fee rows stop matching
      `vault::CREDENTIAL_TTL_SECS` / `x402::EXECUTION_FEE_MICRO_USDC` — the
      doc is checked against the code, not just trusted. This raised the
      handoff test count from 27 to 29 (30 under `--features x402-live`);
      every "27 handoff" reference across README/SUBMISSION_LEDGER/
      OKX_LISTING_COPY/TRACK_ALIGNMENT was updated to match, so this change
      doesn't itself introduce the exact drift it exists to prevent.
- [x] **G-3 landed (2026-08-03): real Svelte 5 frontend.** `ui/` is now a
      Vite + Svelte 5 project (`package.json`, `vite.config.js`,
      `ui/src/main.js`); `ui/src/App.svelte` — previously unused reference
      source — is the real frontend, built by `npm run build` into the
      committed `ui/dist` that `tauri.conf.json`'s `frontendDist` embeds.
      `cargo build`/`run`/`check --features tauri-app` still take zero npm
      steps; npm is only needed to regenerate `ui/dist` after editing
      `ui/src`. Fixed in the same pass: `App.svelte` previously enabled
      button 3 (Settle) right after Announce instead of after the 402
      restriction — a known divergence from the shipped hand-written dist
      that `VIDEO_PREP.md`/`docs/amg/capture-map.json` explicitly warned
      recorders to avoid. Now that `App.svelte` *is* what's shipped, that
      divergence is fixed at the source and the warning is removed.
      Restyled to full visual parity with the previous hand-written
      `ui/dist/index.html` (same CSS variables, colors per message kind,
      header/footer chrome, button states) rather than shipping Svelte's
      unstyled default. **Not verified in this session:** `cargo check -p
      hammurabi-handoff --features tauri-app` — this sandbox lacks the
      system GTK/GDK dev packages Tauri's Linux build needs
      (`gdk-3.0.pc` not found via pkg-config), which predates this change
      (confirmed by stashing it and reproducing the same failure on the
      prior commit) and is exactly what `handoff-ci.yml`'s GitHub Actions
      runner provides. What *was* verified here: `npm run build` succeeds
      and emits a valid `index.html` + hashed JS/CSS bundle, and the
      default-feature test suite (29 handoff / 38 crypto / 7 oka) stays
      green since `tauri-app` is off by default and untouched by this
      change otherwise. Confirm the CI badge on this PR before treating the
      Tauri build itself as re-verified.

## Still owed (human / external — cannot be produced in-repo)

- [ ] OKX.AI ASP account + gallery form + Google form
- [ ] X account; schedule the four posts from `SOCIAL_POSTS.md`
- [x] **Demo video + thumbnail (2026-07-27):** `assets/video/demo.mp4` —
      90.0s, 1116×800, h264, narrated (AAC mono, real speech confirmed via
      `ffmpeg -af volumedetect`: mean −20.7dB / peak −0.5dB, not silence or
      clipping). Video track verified byte-identical, frame-for-frame at six
      sampled timestamps, to an earlier silent take of the same recording —
      confirms the narrated cut is dubbed audio over the same clean capture,
      not a re-shoot with new visual risk. Content spot-checked at those six
      timestamps: correct narrative strings, correct button-enable states,
      full six-line sequence visible by the close. **Not independently
      verified: the spoken narration's exact wording** — no transcription
      tooling was available in-session to check it against `DEMO_SCRIPT.md`'s
      quoted (**"do not reword"**) lines; give it one human listen before
      calling this final. `assets/video/thumbnail.png` is `banner.png` at the
      correct 1200×630 — kept from the earlier pass, still valid.
- [x] PNG exports: `assets/logo.png` (512×512, transparent) + `assets/banner.png`
      (1200×630, `#0b0e14`). **Banner spec (ratified 2026-07-08):** purpose-built
      hero (not an architecture crop), two-line tagline **"Hammurabi Handoff" /
      "Zero-Telemetry · 1Shot Settlement"** with sign-off *"Keep the faith, the
      good exists."* This two-line form supersedes the earlier single-line
      literal ("… : Zero-Telemetry. 1Shot Settlement.") — ratified as
      semantically identical and more legible. Source: `assets/banner.svg`.
- [x] **Live screenshots of the running shell (2026-07-26):** all 5 landed
      in `assets/screenshots/`, captured by driving the real Tauri window
      via Chrome DevTools Protocol rather than an OS-level screen grab —
      see the commit history for why that distinction mattered (the first
      capture attempt had real problems: OS overlay contamination and a
      genuine app-level event-name bug that was silently swallowing the live
      feed). Feed-line counts verified against the known-correct backend
      sequence at every step before capturing.
- [ ] Live ASP deployment (requires G-5/G-6 — the 1Shot broadcast adapter
      and OKX manifest submission; G-4's facilitator seam is landed but
      still needs a real facilitator wired in and testnet-verified)

## Full adversarial review (2026-07-26)

Every source file in `hammurabi-handoff` and `iedb-oka` was read directly
(not sampled), plus `iedb-crypto::jcs` (the canonicalization/signing core)
and spot checks of the rest of `iedb-crypto` against its own test suite.
Grepped all of `src/` for `.unwrap()` / `.expect()` / `panic!` /
`unreachable!`: every non-test occurrence is on a value that's structurally
float-free by the type system (`AgentIntent`/`OneShotEnvelope` contain no
`f64` anywhere in their field graph), so the two `.expect("float-free ...
always serializes")` calls cannot actually panic — confirmed by
construction, not by assumption.

Findings: three real gaps fixed (receipt eviction, mesh log cap, request
body limit — see the numbered list below), one real gap closed at the
design level (G-4's dead wiring — see "Resolved since" above), one nuance
found and documented rather than fixed (`Option<Json>` swallowing a 413,
item 5 below). Nothing found that changes the honesty posture already
documented elsewhere in this file — no claim in README/TRACK_ALIGNMENT/
OKX_LISTING_COPY turned out to be false, only the G-4 README wording that
undersold how *un*-wired the seam actually was until tonight.

Verified green after every change, across the full feature matrix, not just
the default one:

| Build | Tests | fmt | clippy -D warnings |
|---|---|---|---|
| default | 29 handoff / 38 crypto / 7 oka | clean | clean |
| `--features tauri-app` | (check only, no test target) | — | clean |
| `--features x402-live` | 30 handoff (incl. `env_wiring_tests`) | clean | clean |
| `--features tauri-app,x402-live` | check only | — | (not re-run; default+each singly covers the surface) |

## Review-finding deferrals (adversarial review, 2026-07-07)

An in-session multi-agent review raised 9 findings; verification agents were
cut off by a session limit, so these were triaged manually. Two were fixed
immediately (broadcast-lag killing the feed forwarder; sender-string HTML
injection + boot race in `ui/dist`). The rest are real-but-mock-scope,
deferred until the corresponding adapter goes live:

1. **Receipt ids visible on the ungated feed** — on loopback with a mock
   settler this grants nothing (anyone local can mint receipts anyway); must
   be redacted the moment G-4 (live x402) lands. *Still deferred* — G-4 is
   wired but off by default; revisit when a build actually ships with it on.
2. ~~**No receipt expiry/eviction**~~ — **fixed 2026-07-26.**
   `x402::RECEIPT_STALE_AFTER_SECS` (600s) is swept opportunistically inside
   `settle`/`mock_settle` before each insert, so the treasury is bounded by
   `settle-rate × 600s` regardless of how long the process runs or how many
   requests never come back to redeem. Tests:
   `stale_unredeemed_receipts_are_evicted_on_next_settle`,
   `receipts_inside_the_stale_window_survive_a_sweep`.
3. ~~**Unbounded mesh log**~~ — **fixed 2026-07-26.** `mesh::MAX_LOG_LEN`
   (2,000) caps the retained log; `post()` drains the oldest entries past the
   cap. `seq` numbering stays monotonic and gap-aware through the trim, so a
   client can tell it missed lines rather than mistake a trimmed feed for a
   fresh one. Test: `log_is_capped_and_seq_stays_monotonic_across_the_trim`.
4. **Caller-controlled `agent_handle` as feed sender** — audit-trail
   impersonation; bind handles to authenticated agent identities when A2A
   goes multi-process. *Still deferred* — correctly scoped as a
   multi-process concern, not a same-machine one.
5. **`Option<Json<AgentIntent>>` treats malformed JSON as "no body"** —
   *still deferred, and sharper than first described.* While adding a
   request-body size limit (below) it turned out this same swallow also
   absorbs a too-large-body rejection on `/agent/announce` specifically: the
   body-limit layer still stops the stream at the cap (no unbounded
   buffering), but the caller sees 200 + the mock intent instead of a 413.
   `/execute` and `/x402/settle` use a plain (non-`Option`) `Json` extractor
   and do surface the 413 correctly — verified by
   `server::tests::oversized_body_is_rejected_before_it_reaches_a_handler`.
   Tighten `/agent/announce` to a real 400/413 when real agents integrate,
   same as originally noted.
6. **Announce always narrates the fixed mock line** — intentional for the
   Legible Autonomy demo string; derive the line from the intent when custom
   intents matter. *Still deferred, unchanged.*
7. **New, found and fixed 2026-07-26: unbounded request body.** Axum applies
   no body-size limit unless one is set explicitly; nothing did. Added
   `DefaultBodyLimit::max(1 MiB)` (`server::MAX_REQUEST_BODY_BYTES`) to the
   router. 1 MiB is generous headroom for payloads that are actually a few
   hundred bytes, not a tight fit.
