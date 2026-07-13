---
name: secure-code-guardian
description: Security review: OWASP, trust boundaries, auth.
version: 1.0.0
created_by: bundled
pinned: true
tags: [review, security]
---

Security review layer. Map first, then sweep, then rank.

## 1. Map trust boundaries
List every point where external data enters the changed code: CLI args, env,
network input, file contents, DB rows, tool/model output. Everything arriving
across a boundary is untrusted until validated.

## 2. Sweep — for each boundary, check the classes that apply
- Injection: SQL/command/path traversal; template injection. Parameterize;
  never build shell strings from untrusted input.
- AuthN/AuthZ: missing checks, confused-deputy paths, session fixation,
  privilege widening (does this change let a lower privilege reach more?).
- Secrets: keys/tokens in code, logs, error messages, or committed files.
- Sensitive data exposure: PII in logs, verbose errors leaking internals.
- SSRF / unsafe fetch: user-controlled URLs reaching internal surfaces.
- Deserialization / parsing of untrusted formats without limits.
- Dependency risk: new dependencies — why this one, what does it pull in?
- Regent-specific: anything widening the filesystem sandbox, the terminal
  jail, or gateway-artifact paths gets flagged regardless of intent.

## 3. Report
`file:line — [class] — attack path in one sentence — minimal fix`
Severity = exploitability × blast radius, worst first. If nothing is exploitable,
say so and name the two riskiest surfaces you checked anyway.

## Never
Never claim a vulnerability without the concrete attack path. Never propose a
rewrite when a one-line guard closes the hole.
