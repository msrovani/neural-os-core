---
name: grill-frontier
description: Interview the user in frontier rounds over a design tree before starting any significant plan, design, or decision. Use when requirements are fuzzy or the user wants to stress-test a plan ("grill me", "stress test this", "o que devo perguntar", "valide meu plano", "me entreviste"). Never act on the plan until the frontier is empty and the user confirms shared understanding.
---

# Grill-Frontier

Interview the user relentlessly until you reach a shared understanding. Map the plan as a **design tree**: every decision branches into the decisions that hang off it. Use this BEFORE starting a significant plan, design, or decision — never after writing code.

## The loop

1. **Compute the frontier.** The frontier is every decision whose prerequisites are already settled — the questions you can ask *now* without guessing at answers you haven't heard yet.
2. **Ask the whole frontier in one round.** Number each question and give your recommended answer for each. Format:
   ```
   ❓ Q1 - <título>: <corpo, pode ter múltiplos parágrafos e opções>

   ➡️ <sua resposta recomendada>
   ```
   Use the `question` tool for a single decisive question; for multi-question rounds, present numbered questions as text and wait.
3. **Wait for the user's answers before the next round.** Each round reshapes the tree — settled decisions push the frontier outward and unblock questions that depended on them. Recompute and ask again. A question whose answer depends on another question still open in *this* round belongs to a later round, not this one.

## Facts are your job, never the user's

When a frontier question needs a fact from the environment (filesystem, codebase, docs, library behavior), find it yourself or dispatch a sub-agent (explorer for codebase, librarian for external docs) — don't ask the user for anything you could look up. A running exploration is an unsettled prerequisite: only the questions downstream of it wait for the report — ask the rest of the frontier now.

## Done

The session is done when the frontier is empty: every branch of the design tree visited, nothing left silently assumed. **Do not act on the plan until the user confirms you reached shared understanding.**

## Governance hooks

- Surface new vocabulary the user uses during grilling and propose it for `CONTEXT.md` (shared-language doc) — the terms you settle here are the names files, functions and tests should use.
- Decisions worth keeping land as IDEA → ADR → SESSION per `docs/GOVERNANCE.md`, not as conversation memory.
- Do not grill trivia the user already settled or routine details you can default (`question` tool guidance: reasonable assumptions for minor details, stated briefly).
