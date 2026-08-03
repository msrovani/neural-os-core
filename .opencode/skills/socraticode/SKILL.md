---
name: socraticode
description: Use when exploring this codebase via SocratiCode MCP tools — semantic codebase_search, codebase_graph_query, codebase_impact (blast radius), codebase_flow (entry points), codebase_symbol(s), codebase_context artifacts. Triggers on "where is X", "what breaks if I change", "who calls this", "circular dependencies", "how does this feature work", refactoring planning, and codebase index management. Teaches search-before-read: use SocratiCode tools before reading files directly.
---

# SocratiCode Codebase Search

This project is indexed with SocratiCode (MCP server). Always use its MCP tools to explore the codebase before reading any files directly.

## Workflow

1. **Start most explorations with `codebase_search`.** Hybrid semantic + keyword search (vector + BM25, RRF-fused) runs in a single call.
   - Broad, conceptual queries for orientation: "how is authentication handled", "boot sequence", "error handling patterns".
   - Precise queries for symbol lookups: exact function names, constants, type names.
   - Prefer search results to infer which files to read — do not speculatively open files.
   - **Use grep instead when** you already know the exact identifier, error string, or regex pattern — grep is faster and more precise there.
2. **Follow the graph before following imports.** Use `codebase_graph_query` to see what a file imports and what depends on it before diving into its contents. **Before modifying or deleting a file, check its dependents.** When planning a refactor, identify all affected files first.
3. **Use impact analysis BEFORE refactoring, renaming, or deleting code:**
   - `codebase_impact` = "what breaks if I change X?" (blast radius)
   - `codebase_flow` = "what does this code do?" (trace from entry point; no args → auto-detect entry points)
   - `codebase_symbol` = 360° view of one function (definition, callers, callees)
   - `codebase_symbols` = list symbols in a file / search by name
4. **Read files only after narrowing down via search.** Once results point to 1–3 files, read only relevant sections. Never read a file just to find out if it's relevant — search first.
5. **Use `codebase_graph_circular` when debugging unexpected behaviour.** Circular dependencies cause subtle runtime issues; check proactively, and on import errors / unexpected init order.
6. **Check `codebase_status` if search returns no results.** The project may not be indexed yet; run `codebase_index` and wait for completion.
7. **Leverage context artifacts for non-code knowledge.** Run `codebase_context` early; use `codebase_context_search` before asking about DB/API/infra structure; refresh with `codebase_context_index` if stale.

## When to use which tool

| Goal | Tool |
|---|---|
| Understand what the codebase does / where a feature lives | `codebase_search` (broad query) |
| Find a specific function, constant, or type | `codebase_search` (exact name) or grep |
| Find exact error messages, log strings, regex patterns | grep / ripgrep |
| See what a file imports or what depends on it | `codebase_graph_query` |
| Check blast radius before modifying/deleting a file | `codebase_impact` (symbol) / `codebase_graph_query` (file) |
| What breaks if I change function X? | `codebase_impact target=X` |
| What does this entry point actually do? | `codebase_flow entrypoint=X` |
| List entry points in this codebase | `codebase_flow` (no args) |
| Who calls this function and what does it call? | `codebase_symbol name=X` |
| What functions/classes exist in this file? | `codebase_symbols file=path` |
| Spot architectural problems | `codebase_graph_circular`, `codebase_graph_stats` |
| Visualise module structure | `codebase_graph_visualize` |
| Verify index is up to date | `codebase_status` |
| Discover schemas, specs, configs, docs | `codebase_context` / `codebase_context_search` |

## Index management

- First time on this project: call `codebase_index {}` (runs in background; first-time setup pulls Docker images ~5 min).
- **Keep the MCP connection alive during indexing** — some hosts drop idle connections and kill the background process. Call `codebase_status` roughly every 60 s after `codebase_index` until complete.
- Incremental updates are automatic (file watcher, debounced 2 s). Use `codebase_update` to force a sync pass.

## Notes

- Semantic search first: one call returns ranked, deduplicated snippets in ms at negligible token cost vs. speculative file opens.
- The dependency/call graph is static analysis without type inference — dynamic dispatch, macros, and framework magic are invisible. `unresolvedEdgePct` in `codebase_graph_status` signals quality.
