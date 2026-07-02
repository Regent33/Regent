# regent-orchustr-core — vendored Orchustr crates

`or-core` and `or-mcp` are vendored copies of the Orchustr crates
(`../Orchustr/orchustr/crates/`), so Regent builds standalone â€” no sibling
checkout required.

- Source of truth for new development stays in the Orchustr repo.
- To update: copy the crate's `src/` + `tests/` over the vendored copy and
  keep these Cargo.tomls (they are self-contained; the originals inherit
  Orchustr's workspace keys).
- Keep `schemars` in lockstep with or-mcp's `McpTool.input_schema` type
  (see the note in the root Cargo.toml).
