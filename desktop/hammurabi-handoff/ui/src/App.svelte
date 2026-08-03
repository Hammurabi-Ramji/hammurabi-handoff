<!--
  RAMesh feed — Svelte source for the Hammurabi Handoff shell.

  This is the real, shipped frontend (G-3, landed): `npm run build` in
  `ui/` produces `ui/dist`, which `tauri.conf.json`'s `frontendDist` embeds
  at Rust compile time. `ui/dist` is committed, so `cargo build`/`cargo run`
  still need zero npm steps — npm is only needed when this source changes
  and `ui/dist` has to be regenerated.
-->
<script>
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";

  let feed = [];
  let intent = null;
  let receiptId = null;
  let restricted = false; // true once execute() has been refused with a 402
  let status = "idle";

  function pad(seq) {
    return "[" + String(seq).padStart(4, " ") + "]";
  }

  onMount(async () => {
    // Subscribe first, snapshot second — the seq dedupe absorbs overlap,
    // so no message posted during boot is lost.
    const seen = new Set();
    const add = (m) => {
      if (seen.has(m.seq)) return;
      seen.add(m.seq);
      feed = [...feed, m];
    };
    await listen("ramesh/message", (e) => add(e.payload));
    (await invoke("mesh_feed")).forEach(add);
  });

  async function announce() {
    const r = await invoke("agent_announce");
    intent = r.body;
    status = `intent ${intent.intent_id}`;
  }

  async function execute() {
    const r = await invoke("request_execution", { intent, receiptId: null });
    if (r.status === 402) {
      status = "402 — payment required";
      restricted = true;
    } else {
      status = `unexpected ${r.status}`;
    }
  }

  async function settle() {
    const r = await invoke("x402_settle", { intentId: intent.intent_id });
    receiptId = r.body.receipt_id;
    status = `receipt ${receiptId}`;
  }

  async function retry() {
    const r = await invoke("request_execution", { intent, receiptId });
    status =
      r.status === 200
        ? `settled — envelope ${r.body.idempotency_key} (TTL 60s)`
        : `refused: ${r.status}`;
  }
</script>

<main>
  <header>
    <h1>Hammurabi Handoff</h1>
    <span class="channel">RAMesh · #handoff-ops · local-first · zero-telemetry</span>
  </header>

  <div class="feed">
    {#each feed as m (m.seq)}
      <div class="msg k-{m.kind}">
        <span class="seq">{pad(m.seq)}</span>
        <span class="sender">&lt;{m.sender}&gt;</span>
        <span class="body">{m.body}</span>
      </div>
    {/each}
  </div>

  <footer>
    <button on:click={announce}>1 · Announce opportunity</button>
    <button on:click={execute} disabled={!intent}>2 · Request execution</button>
    <button on:click={settle} disabled={!restricted}>3 · Settle x402 (0.50 USDC)</button>
    <button on:click={retry} disabled={!receiptId}>4 · Execute with receipt</button>
    <span class="status">{status}</span>
  </footer>
</main>

<style>
  :global(:root) {
    --bg: #0b0e14;
    --panel: #11151f;
    --line: #1f2633;
    --fg: #c9d4e3;
    --dim: #5c6b82;
    --accent: #7aa2f7;
    --intent: #9ece6a;
    --restriction: #f7768e;
    --payment: #e0af68;
    --credential: #bb9af7;
    --settlement: #7dcfff;
    --info: #5c6b82;
  }

  :global(html),
  :global(body) {
    margin: 0;
    height: 100%;
    background: var(--bg);
    color: var(--fg);
    font: 13px/1.55 "Cascadia Mono", "JetBrains Mono", Consolas, monospace;
  }

  main {
    display: flex;
    flex-direction: column;
    height: 100vh;
  }

  header {
    padding: 10px 16px;
    background: var(--panel);
    border-bottom: 1px solid var(--line);
    display: flex;
    gap: 12px;
    align-items: baseline;
  }
  header h1 {
    font-size: 14px;
    margin: 0;
    color: var(--accent);
    font-weight: 600;
  }
  .channel {
    color: var(--dim);
  }

  .feed {
    flex: 1;
    overflow-y: auto;
    padding: 12px 16px;
    white-space: pre-wrap;
  }
  .msg {
    display: flex;
    gap: 8px;
    padding: 1px 0;
  }
  .msg .seq {
    color: var(--dim);
    min-width: 46px;
    text-align: right;
  }
  .msg .sender {
    min-width: 150px;
  }
  .k-intent .sender {
    color: var(--intent);
  }
  .k-restriction .sender {
    color: var(--restriction);
  }
  .k-payment .sender {
    color: var(--payment);
  }
  .k-credential .sender {
    color: var(--credential);
  }
  .k-settlement .sender {
    color: var(--settlement);
  }
  .k-info .sender {
    color: var(--info);
  }
  .k-restriction .body {
    color: var(--restriction);
  }
  .k-credential .body {
    color: var(--credential);
  }

  footer {
    padding: 10px 16px;
    background: var(--panel);
    border-top: 1px solid var(--line);
    display: flex;
    gap: 8px;
  }
  button {
    background: transparent;
    color: var(--accent);
    border: 1px solid var(--accent);
    border-radius: 3px;
    padding: 6px 12px;
    font: inherit;
    cursor: pointer;
  }
  button:hover {
    background: var(--accent);
    color: var(--bg);
  }
  button:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .status {
    margin-left: auto;
    color: var(--dim);
    align-self: center;
  }
</style>
