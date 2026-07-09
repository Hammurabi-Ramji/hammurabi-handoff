# 90-Second Demo Script (repo-accurate)

Every line below is real app behavior — record the actual shell, no mockups.
Launch: `cargo run -p hammurabi-handoff --features tauri-app`.

## Title card (5s)
**"Hammurabi Handoff — JIT Vault"** / *"Credentials that die after one use."*

## Scene 1 — The problem (15s)
Split screen: a dashboard of standing API keys vs. an agent-exploit headline.
> "Every AI agent today runs with a loaded gun pointed at your wallet.
> Standing API keys mean one exploit leaks everything."

## Scene 2 — The feed (20s)
Screen capture of the shell. Click **1 · Announce opportunity**. The line
appears live:
```
<agent/scout-1> Opportunity found: ETH limit order at $3,100. Requesting execution.
```
> "Agents negotiate in a feed you can actually read. Watch scout-1 request a
> limit order."

## Scene 3 — The gate, then the mint (25s)
Click **2 · Request execution** — the refusal lands in red:
```
<gate/x402> Action restricted. x402 payment of 0.50 USDC required.
```
Click **3 · Settle x402**, then **4 · Execute with receipt**:
```
<vault/handshake> Payment confirmed. Ephemeral Handshake credential minted.
```
> "Nothing executes until the x402 fee clears. Then the vault mints a
> credential bound to exactly this order — chaos-tested against tampering
> before it's released, dead in sixty seconds, one settlement ever."

## Scene 4 — Settlement + the kill test (20s)
Show the envelope line (`relay/1shot … staged for gasless Stylus broadcast`),
then click **4** again — the burned receipt is refused with a fresh 402 line.
> "The signed envelope is ready for gasless on-chain verification. And replay?
> The receipt burned on first use. Watch it bounce."

## Scene 5 — Close (5s)
Repo link + tagline.
> "Hammurabi Handoff. Legible autonomy. Ship it."

### Timing note
The credential TTL is 60 seconds — comfortably longer than scenes 3–4, so a
single unbroken take works. If a take runs long, re-announce (fresh intent,
fresh receipt) rather than splicing.
