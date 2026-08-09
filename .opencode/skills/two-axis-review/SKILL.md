---
name: two-axis-review
description: Two-axis review of the diff since a fixed point — Standards (repo coding standards + Fowler smell baseline) and Spec (does it faithfully implement the originating ADR/idea/ticket), run as parallel sub-agents so neither pollutes the other, reported separately. Use when the user wants to review a branch, a PR, work-in-progress changes, or asks to "review since X".
---

# Two-Axis-Review

Review of the diff between `HEAD` and a fixed point the user supplies, along two independent axes:

- **Standards** — does the code conform to this repo's documented coding standards?
- **Spec** — does the code faithfully implement the originating ADR / idea / ticket?

Both axes run as **parallel sub-agents** so they don't pollute each other's context, then you aggregate their findings.

## 1. Pin the fixed point

Whatever the user said — a commit SHA, branch name, tag, `main`, `HEAD~5`, etc. If they didn't specify one, ask.

Capture the diff command once: `git diff <fixed-point>...HEAD` (three-dot, so the comparison is against the merge-base). Also capture `git log <fixed-point>..HEAD --oneline`.

Before going further, confirm the fixed point resolves (`git rev-parse <fixed-point>`) and the diff is non-empty. A bad ref or empty diff should fail here — not inside two parallel sub-agents.

## 2. Identify the spec source

In this order:

1. ADR references in the commit messages (`ADR-0041`, `#123`, etc.) — fetch the ADR from `docs/architecture/`.
2. A path the user passed as an argument.
3. An entry in `docs/memory/IDEA_BANK.md` or `docs/memory/SESSION_*.md` matching the change.
4. If nothing is found, ask the user where the spec is. If there isn't one, the **Spec** sub-agent skips and reports "no spec available".

## 3. Identify the standards sources

Anything in the repo that documents how code should be written: `AGENTS.md`, `docs/GOVERNANCE.md`, `.cursor/rules/*.mdc`, `CODING_STANDARDS.md` if present.

On top of whatever the repo documents, the Standards axis always carries the **smell baseline** below — a fixed set of Fowler code smells that applies even when the repo documents nothing. Two rules bind it:

- **The repo overrides.** A documented repo standard always wins; where it endorses something the baseline would flag, suppress the smell.
- **Always a judgement call.** Each smell is a labelled heuristic, never a hard violation — and skip anything tooling already enforces.

The baseline (read *what it is* → *how to fix*; match against the diff):

- **Mysterious Name** — a function/type whose name doesn't reveal what it does. → rename; if no honest name comes, the design's murky.
- **Duplicated Code** — the same logic shape in more than one hunk/file. → extract the shared shape.
- **Feature Envy** — a method reaches into another object's data more than its own. → move the method onto the data.
- **Data Clumps** — the same few fields/params travel together (a type wanting to be born). → bundle into one type.
- **Primitive Obsession** — a primitive/string standing in for a domain concept. → give the concept its own type.
- **Repeated Switches** — the same switch/if-cascade on the same type recurs. → polymorphism or one shared map.
- **Shotgun Surgery** — one logical change forces scattered edits across many files. → gather into one module.
- **Divergent Change** — one file edited for several unrelated reasons. → split so each module changes for one reason.
- **Speculative Generality** — abstraction/parameters/hooks added for needs the spec doesn't have. → delete; inline back until a real need shows.
- **Message Chains** — long `a.b().c().d()` navigation. → hide the walk behind one method.
- **Middle Man** — a class/function that mostly delegates onward. → cut it, call the target direct.
- **Refused Bequest** — a subclass/implementer that ignores most of what it inherits. → drop inheritance, use composition.

## 4. Spawn both sub-agents in parallel

Dispatch two `task` calls (oracle type; general if the review is small) in the same turn. Include in each:

**Standards sub-agent prompt**
- The full diff command and commit list.
- The list of standards-source files found in step 3, **plus the smell baseline from step 3 pasted in full** — the sub-agent has no other access to it.
- Brief: "Report — per file/hunk where relevant — (a) every place the diff violates a documented standard: cite the standard (file + rule); and (b) any baseline smell you spot: name it and quote the hunk. Distinguish hard violations from judgement calls — documented-standard breaches can be hard, but baseline smells are always judgement calls, and a documented repo standard overrides the baseline. Skip anything tooling enforces. Under 400 words."

**Spec sub-agent prompt**
- The diff command and commit list.
- The path or fetched contents of the spec (ADR/IDEA/SESSION).
- Brief: "Report: (a) requirements the spec asked for that are missing or partial; (b) behaviour in the diff that wasn't asked for (scope creep); (c) requirements that look implemented but where the implementation looks wrong. Quote the spec line for each finding. Under 400 words."

If the spec is missing, skip the Spec sub-agent and note this in the final report.

## 5. Aggregate

Present the two reports under `## Standards` and `## Spec` headings, verbatim or lightly cleaned. **Do not merge or rerank findings** — the two axes are deliberately separate.

End with a one-line summary: total findings per axis, and the worst issue *within each axis* (if any). Don't pick a single winner across axes.

## Why two axes

A change can pass one axis and fail the other:

- Code that follows every standard but implements the wrong thing → **Standards pass, Spec fail.**
- Code that does exactly what the ADR asked but breaks project conventions → **Spec pass, Standards fail.**

Reporting them separately stops one axis from masking the other. For an ADR-driven repo, the Spec axis is the one that catches "implemented something, but not the ADR" — the most expensive failure mode here.
