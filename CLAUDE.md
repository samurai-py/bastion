# CLAUDE.md

## 🧠 Core Understanding

Bastion is a **framework for building and orchestrating AI agents** on top of OpenClaw.

Claude does NOT operate the system.
Claude is responsible for **maintaining and evolving the framework**.

---

## 🎯 Primary Objective

Improve:

* developer experience (DX)
* extensibility
* system consistency
* ease of installation

---

## 🧩 System Model

Bastion consists of:

* Persona creation system (framework-level)
* Skill system (core + optional + external)
* Integration with ClawHub
* Runtime abstraction over OpenClaw

---

## 🛠️ Skill System (CRITICAL)

Skills are the core extension mechanism.

### Types:

* Core skills (bundled)
* Optional skills
* External skills (ClawHub)

### Rules:

* Never duplicate existing skills
* Always check for existing functionality before creating new skills
* Skills must have a single responsibility
* Skills must be modular and isolated

### Design requirements:

* Easy to plug/unplug
* No hidden dependencies
* Clear interface

---

## 🧩 Persona System

Claude should NOT design personas.

Claude may:

* improve persona creation APIs
* improve validation
* improve usability

### Rules:

* Keep persona definition simple
* Avoid overengineering persona configuration
* Ensure personas are easy to create and modify

---

## 🔌 Plugin / External Integration (ClawHub)

* External skills must integrate cleanly
* Do not tightly couple external tools to core system
* Ensure compatibility and isolation

---

## ⚙️ Installation & Setup (HIGH PRIORITY)

This system must be:

* Easy to install
* Easy to configure
* Minimal setup required

### Rules:

* Reduce number of steps
* Avoid complex configuration
* Prefer sensible defaults
* Assume user only provides:

  * infrastructure
  * API keys

---

# 🏗️ Architecture Rules

* Do not introduce unnecessary abstractions
* Prefer simple and explicit designs
* Avoid premature generalization
* Keep system modular
* Design skills and modules to be loosely coupled and independently usable
* Respect clear boundaries between core logic, infrastructure, and interfaces

---

## 🔍 Code Navigation

* Use GitNexus to understand repository structure
* Use context-mode to retrieve relevant code segments efficiently
* Avoid reading full files unless necessary
* Prefer local code as the primary source of truth
* Use Context7 only when external documentation is required

---

## 🧠 Decision Model

* Claude is the decision-maker for code changes

* Tools must support decisions through:

  * structure analysis (GitNexus)
  * code retrieval (context-mode)
  * external validation (Context7)
  * behavioral validation (pytest, Playwright)
  * AI evaluation (DeepEval)
  * quality validation (Code Review, Architect)

* Do not rely on assumptions when tools can provide evidence

* Prefer evidence-based decisions over inferred behavior

* When possible, validate decisions against real system behavior

---

## 🧪 Validation & Testing

* Use pytest for validating internal logic and rules

* Use Playwright for end-to-end and integration testing when flows are affected

* Use DeepEval for evaluating AI behavior, decision-making, and response quality

* Select validation strategy based on task type:

  * logic → pytest
  * system flow → Playwright
  * AI behavior → DeepEval

* Critical changes must always be validated

---

## 📐 Structural Validation

* Use Architect to validate:

  * coupling and modularity
  * architectural boundaries
  * anti-patterns (God classes, spaghetti modules, shotgun surgery)

* Use Architect when:

  * modifying core systems (skills, personas, loaders)
  * introducing new modules or abstractions

* Treat Architect strictly as a validation tool

* Do not use Architect for trivial changes

---

## 📜 Contract & Protocol Rules

* When defining structured inputs/outputs, prefer declarative protocols (intent-compiler)

* Protocols must:

  * define schema as source of truth
  * enforce type-safe slots
  * validate outputs against schema

* Avoid implicit contracts between modules

* Prefer explicit, versioned, and validated interfaces

* When applicable:

  * generate mocks from schema
  * validate outputs in CI
  * ensure compatibility through versioning

---

## 🤖 Agent & Automation Principles

* Prefer deterministic workflows over implicit AI behavior

* Avoid uncontrolled agent autonomy

* When generating or using agents:

  * ensure clear responsibilities
  * enforce boundaries between roles
  * avoid overlapping responsibilities

* Treat automation (RPA / agents) as execution layers, not decision-makers

---

## 🧪 Refactoring Rules

* Preserve backward compatibility
* Avoid breaking changes unless necessary
* Refactor incrementally
* Validate behavior after refactoring using appropriate tools

---

## 🔐 Security Practices

* Use Security Guidance when:

  * designing authentication, authorization, or sensitive data handling
  * integrating external systems or plugins

* Security Review must validate all changes via CI

* Avoid overusing security tools for low-risk changes

---

## 🧪 Code Quality Review

* Use Code Review as the default validation for code quality

* Use Architect only for structural validation when needed

* Avoid redundant or excessive review passes

---

## 🚫 Anti-Patterns

* Overengineering systems
* Creating complex configuration layers
* Tight coupling between skills
* Hard dependencies between modules
* Complicating installation
* Using tools without clear purpose
* Relying on assumptions instead of validation
* Implicit or undocumented contracts between components

---

## 🧭 Preferred Workflow

1. Understand system structure (GitNexus)
2. Retrieve relevant code (context-mode)
3. Identify affected module (skills, persona API, setup)
4. Check existing implementations and patterns
5. Implement minimal and consistent change
6. Validate:

   * logic → pytest
   * flows → Playwright (if needed)
   * AI behavior → DeepEval (if applicable)
7. Validate structure (Architect if needed)
8. Review code quality (Code Review)
9. Ensure contracts are respected (intent-compiler if applicable)
10. Ensure simplicity and consistency

---

## 💰 Token Efficiency

* Prefer indexed search over full file reads
* Avoid redundant context
* Reuse retrieved information
* Avoid unnecessary tool usage

<!-- gitnexus:start -->
# GitNexus — Code Intelligence

This project is indexed by GitNexus as **bastion** (981 symbols, 1850 relationships, 36 execution flows). Use the GitNexus MCP tools to understand code, assess impact, and navigate safely.

> If any GitNexus tool warns the index is stale, run `npx gitnexus analyze` in terminal first.

## Always Do

- **MUST run impact analysis before editing any symbol.** Before modifying a function, class, or method, run `gitnexus_impact({target: "symbolName", direction: "upstream"})` and report the blast radius (direct callers, affected processes, risk level) to the user.
- **MUST run `gitnexus_detect_changes()` before committing** to verify your changes only affect expected symbols and execution flows.
- **MUST warn the user** if impact analysis returns HIGH or CRITICAL risk before proceeding with edits.
- When exploring unfamiliar code, use `gitnexus_query({query: "concept"})` to find execution flows instead of grepping. It returns process-grouped results ranked by relevance.
- When you need full context on a specific symbol — callers, callees, which execution flows it participates in — use `gitnexus_context({name: "symbolName"})`.

## When Debugging

1. `gitnexus_query({query: "<error or symptom>"})` — find execution flows related to the issue
2. `gitnexus_context({name: "<suspect function>"})` — see all callers, callees, and process participation
3. `READ gitnexus://repo/bastion/process/{processName}` — trace the full execution flow step by step
4. For regressions: `gitnexus_detect_changes({scope: "compare", base_ref: "main"})` — see what your branch changed

## When Refactoring

- **Renaming**: MUST use `gitnexus_rename({symbol_name: "old", new_name: "new", dry_run: true})` first. Review the preview — graph edits are safe, text_search edits need manual review. Then run with `dry_run: false`.
- **Extracting/Splitting**: MUST run `gitnexus_context({name: "target"})` to see all incoming/outgoing refs, then `gitnexus_impact({target: "target", direction: "upstream"})` to find all external callers before moving code.
- After any refactor: run `gitnexus_detect_changes({scope: "all"})` to verify only expected files changed.

## Never Do

- NEVER edit a function, class, or method without first running `gitnexus_impact` on it.
- NEVER ignore HIGH or CRITICAL risk warnings from impact analysis.
- NEVER rename symbols with find-and-replace — use `gitnexus_rename` which understands the call graph.
- NEVER commit changes without running `gitnexus_detect_changes()` to check affected scope.

## Tools Quick Reference

| Tool | When to use | Command |
|------|-------------|---------|
| `query` | Find code by concept | `gitnexus_query({query: "auth validation"})` |
| `context` | 360-degree view of one symbol | `gitnexus_context({name: "validateUser"})` |
| `impact` | Blast radius before editing | `gitnexus_impact({target: "X", direction: "upstream"})` |
| `detect_changes` | Pre-commit scope check | `gitnexus_detect_changes({scope: "staged"})` |
| `rename` | Safe multi-file rename | `gitnexus_rename({symbol_name: "old", new_name: "new", dry_run: true})` |
| `cypher` | Custom graph queries | `gitnexus_cypher({query: "MATCH ..."})` |

## Impact Risk Levels

| Depth | Meaning | Action |
|-------|---------|--------|
| d=1 | WILL BREAK — direct callers/importers | MUST update these |
| d=2 | LIKELY AFFECTED — indirect deps | Should test |
| d=3 | MAY NEED TESTING — transitive | Test if critical path |

## Resources

| Resource | Use for |
|----------|---------|
| `gitnexus://repo/bastion/context` | Codebase overview, check index freshness |
| `gitnexus://repo/bastion/clusters` | All functional areas |
| `gitnexus://repo/bastion/processes` | All execution flows |
| `gitnexus://repo/bastion/process/{name}` | Step-by-step execution trace |

## Self-Check Before Finishing

Before completing any code modification task, verify:
1. `gitnexus_impact` was run for all modified symbols
2. No HIGH/CRITICAL risk warnings were ignored
3. `gitnexus_detect_changes()` confirms changes match expected scope
4. All d=1 (WILL BREAK) dependents were updated

## Keeping the Index Fresh

After committing code changes, the GitNexus index becomes stale. Re-run analyze to update it:

```bash
npx gitnexus analyze
```

If the index previously included embeddings, preserve them by adding `--embeddings`:

```bash
npx gitnexus analyze --embeddings
```

To check whether embeddings exist, inspect `.gitnexus/meta.json` — the `stats.embeddings` field shows the count (0 means no embeddings). **Running analyze without `--embeddings` will delete any previously generated embeddings.**

> Claude Code users: A PostToolUse hook handles this automatically after `git commit` and `git merge`.

## CLI

| Task | Read this skill file |
|------|---------------------|
| Understand architecture / "How does X work?" | `.claude/skills/gitnexus/gitnexus-exploring/SKILL.md` |
| Blast radius / "What breaks if I change X?" | `.claude/skills/gitnexus/gitnexus-impact-analysis/SKILL.md` |
| Trace bugs / "Why is X failing?" | `.claude/skills/gitnexus/gitnexus-debugging/SKILL.md` |
| Rename / extract / split / refactor | `.claude/skills/gitnexus/gitnexus-refactoring/SKILL.md` |
| Tools, resources, schema reference | `.claude/skills/gitnexus/gitnexus-guide/SKILL.md` |
| Index, status, clean, wiki CLI commands | `.claude/skills/gitnexus/gitnexus-cli/SKILL.md` |

<!-- gitnexus:end -->
