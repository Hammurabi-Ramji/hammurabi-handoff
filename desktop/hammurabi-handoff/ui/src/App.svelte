<!--
  RAMesh feed — Svelte source for the Hammurabi Handoff shell.

  NOTE: `ui/dist/index.html` is a committed, hand-written static build so
  the tauri-app feature compiles with zero npm steps. When you scaffold the
  Svelte project (`npm create vite@latest ui -- --template svelte`), this
  component replaces it 1:1 — same four-step Legible Autonomy loop, same
  Tauri commands.
-->
<script>
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";

  let feed = [];
  let intent = null;
  let receiptId = null;
  let status = "idle";

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
    status = r.status === 402 ? "402 — payment required" : `unexpected ${r.status}`;
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
    <span>RAMesh · #handoff-ops · local-first · zero-telemetry</span>
  </header>

  <div class="feed">
    {#each feed as m (m.seq)}
      <div class="msg {m.kind}">
        <span class="seq">[{m.seq}]</span>
        <span class="sender">&lt;{m.sender}&gt;</span>
        <span class="body">{m.body}</span>
      </div>
    {/each}
  </div>

  <footer>
    <button on:click={announce}>1 · Announce opportunity</button>
    <button on:click={execute} disabled={!intent}>2 · Request execution</button>
    <button on:click={settle} disabled={!intent}>3 · Settle x402 (0.50 USDC)</button>
    <button on:click={retry} disabled={!receiptId}>4 · Execute with receipt</button>
    <span class="status">{status}</span>
  </footer>
</main>

<style>
  main { display: flex; flex-direction: column; height: 100vh; font-family: monospace; }
  .feed { flex: 1; overflow-y: auto; }
  .msg { display: flex; gap: 0.5rem; }
  .restriction .body { color: #f7768e; }
  .credential .body { color: #bb9af7; }
  .status { margin-left: auto; opacity: 0.6; }
</style>
