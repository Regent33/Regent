# Security Policy

Regent executes shell commands, stores API keys, and can be reached from
external chat platforms — security reports are taken seriously.

## Reporting a vulnerability

Please report vulnerabilities **privately** via GitHub → Security →
"Report a vulnerability" (private advisory) on this repository. Do not open a
public issue for anything exploitable. You'll get a response within a few
days; fixes ship in the next release with credit if you want it.

## Supported versions

Regent is in **alpha** (`v0.1.x`). Only the latest release is supported —
there are no backports. Expect sharp edges; the sandboxing layers
(approval gate, filesystem jail, isolated terminal backends) are documented
in [docs/QUICKSTART.md](docs/QUICKSTART.md#sandboxing-tool-execution).

## What's in scope

- Secret handling (`~/.regent/.env`, key redaction in logs)
- Webhook signature verification (all platform adapters)
- Filesystem jail escapes, approval-gate bypasses
- The installer/uninstaller scripts
