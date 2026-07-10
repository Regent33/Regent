# ADR-034 — Desktop web layer: Vite + React Router replaces Next.js

**Status:** accepted (2026-07-10)

**Context:** The desktop app used Next.js 16 purely as a static-export SPA
builder — no SSR, no API routes, no next/image or next/font. Tauri consumes
`out/` and a port-3000 dev server; Next was overhead (slower builds, a
framework's worth of semantics for six client-rendered routes).

**Decision:** Vite 8 + react-router-dom 7, keeping Next's external contract:
dev on port 3000 (`strictPort`), build into `out/`, alias `@/*` → package
root. Feature code stays router-agnostic behind a Next-shaped shim
(`shared/infrastructure/router/adapter.ts`); only import specifiers changed.

**Consequences:** Tauri config and Rust side untouched. New router features
must extend the shim rather than import react-router in features. SPA is a
single entry — deep-route hard reloads in release rely on Tauri's index.html
fallback; escape hatch is swapping BrowserRouter for HashRouter in one place.
