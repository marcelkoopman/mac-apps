# Threat model (STRIDE) — Copycraft

![STRIDE threat model for Copycraft](stride-threat-model.png)

Vector version: [stride-threat-model.svg](stride-threat-model.svg).

The diagram is in Dutch. It is based on the v0.1.78 threat model and extended with what is new since v0.1.80: local AI actions, PDF handling and the in-memory history.

Notes:

- Items marked **[nieuw]** (AI actions, PDF and history limits) were estimated from the design and have **not** been checked against the code.
- Items marked **[onbevestigd]** are not verified as facts.
- History is kept in memory only; nothing is written to disk.
- Risk labels (laag/middel) are estimates for the existing items; new items have no estimate yet.

See also [SECURITY.md](../SECURITY.md).
