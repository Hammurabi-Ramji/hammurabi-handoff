# Video &amp; Screenshot Production Guide

**Capability note, up front:** I (the assistant) have no screen-recording
capability at all, and in this session my desktop-automation tool can only
attach to *installed* applications — it can't discover a raw `cargo run`
dev binary with no Start Menu entry. So both the screenshots and the demo
video have to be captured by a human at the keyboard. This doc exists to
make that a 10-minute mechanical task instead of a creative one: every click,
every line of expected on-screen text, and every timing beat below is copied
directly from the shipped source, not guessed.

The app is already running in the background from this session
(`hammurabi-handoff.exe`, loopback server on `127.0.0.1:3402`). If it's not,
relaunch it:

```bash
cargo run -p hammurabi-handoff --features tauri-app
```

A window titled **"Hammurabi Handoff — RAMesh #handoff-ops"** opens at
1100×760 (resizable, 800×520 minimum — don't shrink it below that for
recording, the four buttons wrap awkwardly).

---

## Part A — the 5 gallery screenshots (do this first, ~3 minutes)

`SUBMISSION_LEDGER.md` asks for 3–5 live shots of the shell. Use
**Win+Shift+S** (Snipping Tool region capture) after each click below, or
**Win+G** → camera icon if Game Bar is already open. Save each as
`assets/screenshots/0N-<name>.png` — that folder already exists (`.gitkeep`
only; drop the five PNGs in).

Record against the **shipped** shell (`ui/dist`, built from
`ui/src/App.svelte` — G-3 landed, no rebuild needed for this take). Button 3
enables only after the 402, matching this table and `DEMO_SCRIPT.md`.

| # | Filename | Action before capturing | What must be visible on screen |
|---|----------|--------------------------|----------------------------------|
| 1 | `01-idle.png` | Fresh launch, before any click | One grey `<system/handoff>` startup line (posted before the UI even connects — not an empty feed), header, all four buttons; buttons 2–4 greyed out (`disabled`), status reads `idle` |
| 2 | `02-announce.png` | Click **1 · Announce opportunity** | Green line: `<agent/scout-1> Opportunity found: ETH limit order at $3,100. Requesting execution.` Status bar shows `intent 01…`. Button 2 now enabled. |
| 3 | `03-restricted.png` | Click **2 · Request execution** | Red line: `<gate/x402> Action restricted. x402 payment of 0.50 USDC required.` Status: `402 — payment required`. Button 3 enabled. |
| 4 | `04-minted.png` | Click **3 · Settle x402 (0.50 USDC)**, then **4 · Execute with receipt** | Amber payment line, then purple line: `<vault/handshake> Payment confirmed. Ephemeral Handshake credential minted.`, then cyan settlement line ending `staged for gasless Stylus broadcast…`. Status: `settled — envelope 01… (TTL 60s)` |
| 5 | `05-replay-bounced.png` | Click **4 · Execute with receipt** again (same receipt) | A **second** red restriction line appears — the burned receipt bouncing. Status: `refused: 402`. This is the security proof shot; caption it as such wherever it's used. |

These five, in order, are also the whole story for a static OKX gallery
carousel if the portal wants stills instead of (or alongside) video.

---

## Part B — the 90-second demo video

### Software (pick one, no install needed for the first)

- **Xbox Game Bar** (built into Windows 11, `Win+G`) — zero setup, records
  the focused window with system + mic audio. Good enough for this; the app
  has no watermark or overlay to fight with.
- **OBS Studio** (already listed in your installed apps) — use if you want a
  window-capture source instead of full-screen, or need to mix a title-card
  scene in-app rather than in post.

Either way: **record the app window only**, not the full desktop — keeps
focus on the feed and avoids leaking anything else on screen.

### Pre-flight (do this before hitting record)

1. Close other notification-generating apps (Slack/Discord/Outlook toasts
   mid-take are the #1 reason for a second take).
2. Relaunch fresh so the feed is empty: kill any existing
   `hammurabi-handoff.exe` process, then `cargo run -p hammurabi-handoff
   --features tauri-app` again. A stale feed from earlier testing will make
   scene 2 confusing (sequence numbers won't start at `[0]`).
3. Confirm the window is the default 1100×760 and not maximized —
   `DEMO_SCRIPT.md`'s framing assumes the compact terminal-style window, not
   fullscreen.
4. Increase your terminal/system font scaling if 13px feed text will be hard
   to read once downscaled for X/portal upload — nothing in the app itself
   is configurable here, so do this at the OS display-scale level and verify
   legibility in a test recording before the real take.

### Shot-by-shot (script is `DEMO_SCRIPT.md` verbatim; this adds the exact clicks/timing)

The credential TTL is a strict 60 seconds, and the whole scene 3→4 sequence
below takes well under that — so it's safe to do in one continuous take as
the script notes. If a take runs long, don't splice: close and relaunch,
re-announce, and go again from a clean feed.

| Time | Beat | Do | On-screen proof |
|------|------|----|-------------------|
| 0:00–0:05 | Title card | Show `assets/banner.png` full-frame (edit in post, or hold on it if you build a title scene in OBS) | "Hammurabi Handoff — JIT Vault" / "Credentials that die after one use." |
| 0:05–0:20 | The problem | Voiceover only, no app on screen yet (or dim/blur the idle window behind it) | — |
| 0:20–0:40 | The feed | Click **1** | Green announce line lands live, as in screenshot 2 above |
| 0:40–0:50 | The gate | Click **2** | Red 402 refusal line, as in screenshot 3 |
| 0:50–1:05 | The mint | Click **3 · Settle x402 (0.50 USDC)**, then **4** | Payment line → purple mint line → cyan settlement line, as in screenshot 4 |
| 1:05–1:20 | The kill test | Click **4** again (same receipt) | Second red refusal — the replay bounce; status `refused: 402`, as in screenshot 5 |
| 1:20–1:25 | Close | Cut to `assets/logo.svg`/banner + repo link | "Hammurabi Handoff. Legible autonomy. Ship it." |

Narration is `DEMO_SCRIPT.md` word-for-word — it was written against this
exact click sequence, so don't paraphrase the quoted lines; they're the
project's own "Legible Autonomy narrative strings" (the source code has
`// do not reword` comments on these exact sentences for a reason — keep the
video consistent with the code).

### Post-production

- **Trim** dead air around clicks; keep the on-screen wait after each click
  long enough that a viewer can actually read the new feed line (~1.5–2s
  hold minimum) before you narrate over it or cut.
- **Optional captions** for judges unfamiliar with the domain — a small
  lower-third the first time each term appears is enough, don't caption
  every line:
  - On the 402 line: *"HTTP 402 Payment Required — the forgotten status code"*
  - On the mint line: *"Fresh keypair per action. Seed destroyed on return."*
  - On the replay bounce: *"Receipt already burned — this is not a retry, it's a rejection"*
- **Thumbnail**: crop `assets/banner.png` (1200×630, already the right
  aspect for most platforms) rather than a mid-video frame — the feed text
  is too small to read as a thumbnail at scale.
- **Export**: H.264 MP4, 1080p, keep it near the 90s script length — most
  hackathon portals cap upload length/size; **check the actual OKX ASP
  submission page for its specific limits before exporting**, since that
  spec lives outside this repo and I don't have it in front of me to quote
  a number I'd just be guessing at.

### Where everything goes — this part is a pure drop-in, no doc edits needed

The drop folders (`assets/screenshots/`, `assets/video/`) and every README /
social / OKX reference already exist in the repo, wired to these exact paths.
Machine-readable beat → source → asset provenance lives in
`docs/amg/capture-map.json`. Save under these names and you're done:

```
desktop/hammurabi-handoff/assets/screenshots/
  01-idle.png
  02-announce.png
  03-restricted.png
  04-minted.png
  05-replay-bounced.png

desktop/hammurabi-handoff/assets/video/
  demo.mp4
  thumbnail.png
```

- `README.md` — the "Screenshots" and "Demo video" sections already embed
  these exact paths; the images/thumbnail render automatically once the
  files land, nothing else to edit.
- `SOCIAL_POSTS.md` — each post's `Media:` line already names the specific
  file(s) it needs from the lists above.
- OKX gallery upload — pull straight from `assets/screenshots/` and
  `assets/video/` when filling out the portal form from
  `OKX_LISTING_COPY.md`.

If Cursor's capture tool wants to name things differently, either rename its
output to match the list above, or update the handful of paths in `README.md`
and `SOCIAL_POSTS.md` to match — but matching the convention here means a
literal file copy is the entire remaining step.

---

## Quick checklist

- [ ] 5 screenshots captured per Part A, saved to `assets/screenshots/`
- [ ] Fresh app relaunch (empty feed) immediately before recording
- [ ] Video recorded per Part B shot table
- [ ] Captions added (optional but recommended for judge legibility)
- [ ] Thumbnail cropped from `banner.png`
- [ ] Exported against the actual OKX portal's length/size spec
- [ ] Files linked into `SOCIAL_POSTS.md` media slots and the OKX gallery upload
