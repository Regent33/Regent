# ADR-007: Crate-internal clean architecture; learning loop as whitelisted sub-agent

**Status:** Accepted (user mandate, 2026-06-12)

**Context:** The user mandated feature-based clean architecture for the whole codebase — the
layering contract (`presentation → domain ← data`, interfaces in domain, implementations in
infra), not just the canonical folder tree. Orchustr's own crates already use this internal
convention.

**Decision:** Every Regent crate uses the `domain/` + `application/` + `infra/` internal layout
(kernel uses the canonical `types/` + `contracts/`). Domain = pure entities, contracts, rules
(zero I/O); application = use cases/orchestrators against contracts; infra = filesystem/network/
DB/process implementations. Public APIs were kept stable through lib.rs re-exports — the
migration changed no behavior (65 tests green before/after). Mapping table:
`docs/architecture-mapping.md`.

The M3 learning loop follows the same discipline: `regent-skills` is domain/application/infra
from birth (SkillRepository contract in domain, FsSkillRepository in infra, library + curator +
review prompt as use cases); the background review is a **whitelisted sub-agent** (memory + skill
tools only, bounded iterations, own session tagged `review`, no recursion) spawned after
successful turns — the main transcript and prompt cache are never touched.

**Consequences:** New code has one obvious home; swapping infra (e.g. a remote skill store) means
a new contract impl, not surgery. The curator can never delete (archive max) and never touches
pinned or non-agent skills.
