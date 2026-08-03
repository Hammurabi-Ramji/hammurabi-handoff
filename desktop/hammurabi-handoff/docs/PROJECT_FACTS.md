# Project Facts — Single Source of Truth

This doc exists because the submission package (README, ARCHITECTURE,
SUBMISSION_LEDGER, PITCH_DECK, DEMO_SCRIPT, TRACK_ALIGNMENT,
OKX_LISTING_COPY, SOCIAL_POSTS, VIDEO_PREP) each restate the same core
numbers and roadmap labels for their own audience — a judge, a form field, a
tweet. That repetition is intentional (each doc is paste-ready for a
different destination) and should **not** be collapsed into one file. What
*should* live in exactly one place is the set of facts those documents all
have to agree on, so a future edit only has to happen once and every other
doc is checked against it instead of silently drifting.

If a number below ever needs to change, change it here first, then grep the
rest of the docs for the old value.

## Core numbers

| Fact | Value | Authority |
|---|---|---|
| Credential TTL | **60 seconds**, strict, structural + verify-time | `vault::CREDENTIAL_TTL_SECS` |
| Execution fee | **0.50 USDC** (500,000 micro-USDC), x402 `exact` scheme | `asp/pricing.json` |
| Demo narrative price | **$3,100** (ETH limit-order line — do not reword) | `DEMO_SCRIPT.md` |
| Payload hash | **BLAKE3** over **RFC 8785 canonical** JSON | `iedb-crypto::jcs` |
| Signing scheme | Ephemeral **Ed25519** per mint, seed zeroized on return | `vault.rs` |
| Replay prevention | Single-use burned x402 receipt + timestamp-bound signature + idempotency-keyed settlement (no explicit nonce field) | `x402.rs`, `settlement.rs` |
| Binary | One binary (`hammurabi-handoff`), headless surface on `127.0.0.1:3402` | `src/server.rs` |
| Test counts (default features) | 27 handoff · 38 crypto · 7 oka | `cargo test --workspace` |

## Roadmap gate legend

The docs refer to roadmap items as `G-1` through `G-6` with no single legend
in one place — collected here from README.md, ARCHITECTURE.md,
SUBMISSION_LEDGER.md, and the `asp/*.json` manifests:

| Gate | What it is | Status as of this doc |
|---|---|---|
| G-1 | Stylus on-chain deployment of `verify_handoff` | Roadmap. Reference verifier is implemented and tested (`settlement::verify_handoff`); not deployed on-chain. |
| G-2 | WASM vault surface (`wasm/iedb-wasm`) | Roadmap, not started. |
| G-3 | Svelte 5 scaffold for the frontend | Roadmap; `ui/src/App.svelte` exists as source only, not built or shipped. |
| G-4 | Live x402 facilitator | Landed, opt-in, **off by default** (`--features x402-live` + `HANDOFF_X402_*` env vars). Wire format not yet verified against a real facilitator or testnet. |
| G-5 | Live 1Shot broadcast (real gasless relay) | Roadmap. `settlement.rs`'s `local://` endpoint is unchanged. |
| G-6 | OKX.AI ASP manifest submission | Roadmap — human/external step, not producible in-repo. |

## Honesty constraints (do not claim)

Carried forward verbatim in intent from `TRACK_ALIGNMENT.md`, and enforced
across `OKX_LISTING_COPY.md`, `SOCIAL_POSTS.md`, and `DEMO_SCRIPT.md`:

- **No** on-chain confirmation — settlement is staged/stubbed until G-1/G-5.
- **No** unconditional "live x402 settlement" claim — mocked by default until
  a live facilitator is actually wired in and testnet-verified (G-4 exists
  but is off by default).
- **No** `/kick` or other moderation claims — roadmap, not built.
- **No** multi-process / cross-machine agent discovery claims — `mesh_discovery`
  is not implemented and is intentionally omitted from `asp/manifest.json`.

## Where each fact is restated (by design, not by accident)

| Document | Audience | Why it repeats these numbers |
|---|---|---|
| `README.md` | GitHub visitor / judge skimming the repo | Project overview |
| `ARCHITECTURE.md` | Technical reviewer | System topology and data flow |
| `SUBMISSION_LEDGER.md` | Whoever is fact-checking the submission | Corrections log + audit trail (see below) |
| `PITCH_DECK.md` | Live/slide audience | Slide-by-slide talking points |
| `DEMO_SCRIPT.md` | Whoever records the demo video | Word-for-word narration |
| `TRACK_ALIGNMENT.md` | Hackathon judges per track | Track-specific framing |
| `OKX_LISTING_COPY.md` | OKX ASP portal form | Paste-ready field values |
| `SOCIAL_POSTS.md` | X/Twitter audience | Launch thread copy |
| `VIDEO_PREP.md` | Human operator recording video/screenshots | Shot list + click sequence |

## Contamination report

```
Input analyzed: 9 docs (README + 8 files under docs/), 1,021 lines total
Redundancy detected: the same ~8 numeric/roadmap facts restated across all
  9 documents — expected, since each is a paste-ready artifact for a
  different audience (form field, slide, tweet, video script). Not
  collapsed: doing so would break each doc's fitness for its destination.
Contradictions found: 0 current. SUBMISSION_LEDGER.md's own "Corrections
  applied to the draft" table documents historical drift (e.g. an earlier
  external draft claimed a 30s TTL and 0.0001 ETH fee; both were corrected
  to the shipped 60s / 0.50 USDC values before publication).
Real defect found and fixed: README.md's Roadmap section pointed at
  `C:\IEDB\docs\specs\sovereign-stack-phase0-alignment.md` — a Windows
  absolute local path that cannot resolve for any reader of the public
  repo. Replaced with an inline description of G-1/G-2/G-3/G-6 (this doc's
  roadmap gate legend), since the referenced file is outside this repo.
Action: added this single reference doc for the 8 core facts + the G-1..G-6
  legend (previously scattered with no legend anywhere); left all 9
  existing documents' prose untouched, since none of it was actually
  redundant clutter — it was 9 different deliverables that happen to share
  a few numbers.
```

<!--METADATA
doc_id: hammurabi_handoff_facts_2026-08-03
source_files:
  - README.md
  - desktop/hammurabi-handoff/README.md
  - desktop/hammurabi-handoff/docs/ARCHITECTURE.md
  - desktop/hammurabi-handoff/docs/SUBMISSION_LEDGER.md
  - desktop/hammurabi-handoff/docs/PITCH_DECK.md
  - desktop/hammurabi-handoff/docs/DEMO_SCRIPT.md
  - desktop/hammurabi-handoff/docs/TRACK_ALIGNMENT.md
  - desktop/hammurabi-handoff/docs/OKX_LISTING_COPY.md
  - desktop/hammurabi-handoff/docs/SOCIAL_POSTS.md
  - desktop/hammurabi-handoff/docs/VIDEO_PREP.md
extracted_entities:
  - {type: fact, id: FACT-001, topic: credential_ttl, value: 60_seconds, confidence: high}
  - {type: fact, id: FACT-002, topic: execution_fee, value: 0.50_usdc, confidence: high}
  - {type: fact, id: FACT-003, topic: test_counts, value: "27_handoff_38_crypto_7_oka", confidence: high}
  - {type: roadmap_gate, id: GATE-001, gate: G-1, status: roadmap, topic: stylus_onchain_deploy}
  - {type: roadmap_gate, id: GATE-002, gate: G-2, status: roadmap, topic: wasm_vault}
  - {type: roadmap_gate, id: GATE-003, gate: G-3, status: roadmap, topic: svelte_scaffold}
  - {type: roadmap_gate, id: GATE-004, gate: G-4, status: landed_opt_in_off_by_default, topic: live_x402_facilitator}
  - {type: roadmap_gate, id: GATE-005, gate: G-5, status: roadmap, topic: live_1shot_broadcast}
  - {type: roadmap_gate, id: GATE-006, gate: G-6, status: roadmap_external, topic: okx_manifest_submission}
  - {type: defect_fixed, id: DEF-001, location: "desktop/hammurabi-handoff/README.md", issue: leaked_local_absolute_path}
relationships:
  - GATE-004 depends_on GATE-005
  - FACT-001 referenced_by [README.md, ARCHITECTURE.md, SUBMISSION_LEDGER.md, PITCH_DECK.md, DEMO_SCRIPT.md, OKX_LISTING_COPY.md, SOCIAL_POSTS.md, VIDEO_PREP.md]
confidence_score: 0.97
redundancy_removed: 0_docs_merged
contradictions_resolved: 0
defects_fixed: 1
-->
