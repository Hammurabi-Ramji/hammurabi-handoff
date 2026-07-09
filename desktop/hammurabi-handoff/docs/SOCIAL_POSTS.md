# X Content Plan (repo-accurate copy)

Numbers corrected from the draft: 60-second TTL (not 30), 0.50 USDC x402
fee, "staged for gasless Stylus broadcast" (no on-chain confirmation claims
while settlement is stubbed). Media slots reference `assets/`.

## Post 1 — Introduction (Day 1)
```
AI agents are trusted with everything and can be trusted with nothing.
So I built Hammurabi Handoff.
A code of cryptographic law for autonomous systems.
Credentials that die after one use — 60 seconds, one action, key zeroized.
Agents you can actually read.
Thread ↓ #OKXAI #AgentInfrastructure
```
Media: problem banner + live feed screenshot.

## Post 2 — Demo video (Day 2)
```
The 90-second demo.
Watch an agent get told no (HTTP 402).
Watch it pay 0.50 USDC. Watch a credential mint — and die.
Watch the replay bounce off the gate.
This is what "legible autonomy" looks like. ↓ 🎥
```
Media: demo MP4 + thumbnail.

## Post 3 — Technical deep-dive (Day 3)
```
Under the hood of Hammurabi Handoff:
1️⃣ Agent announces intent in a readable feed
2️⃣ x402 gate refuses unpaid execution (402 + terms)
3️⃣ Payment mints a single-use, intent-bound receipt
4️⃣ Vault mints an ephemeral Ed25519 credential — payload-bound, 60s TTL
5️⃣ The vault ATTACKS its own signature before releasing it
6️⃣ Credential dies; receipt is already ash
No standing keys. No blast radius. Nothing to steal at rest.
```
Media: credential envelope graphic + architecture diagram (`assets/architecture.svg`).

## Post 4 — Why we built this (Day 4)
```
Every agent framework treats human oversight as an afterthought.
You get logs. Dashboards. Post-hoc analysis.
What you don't get: sitting in the room while your agents work.
Hammurabi Handoff makes the channel itself the observability layer —
five chat lines are the entire security story.
The feed IS the audit.
```
Media: feed screenshot + shell screenshot.
