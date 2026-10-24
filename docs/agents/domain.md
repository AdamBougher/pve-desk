# Domain Docs

How the engineering skills should consume this repo's domain documentation
when exploring the codebase. **Layout: single-context** — one glossary and
one decision record, both at the repo root.

## Before exploring, read these

- **`DECISIONS.md`** at the repo root. This is the repo's ADR set: why the
  app is built this way, and **what was already tried and failed**. Read the
  sections that touch the area you're about to work in. Several approaches
  recorded there were built, measured and removed — check it before
  proposing something, so a dead end isn't walked twice.
- **`CLAUDE.md`** at the repo root. Its *Decisions already made* section is
  binding: language, UI shell, auth model, HTTP stack, target platform. Its
  *Hard rules* section is binding on anything touching the password, TLS,
  power actions, server-supplied text, or Tauri capabilities.
- **`GLOSSARY.md`** at the repo root. It exists, it is short, and it is
  binding on the words you write. Each entry gives the term to use and an
  `_Avoid_` line of words not to use for that concept.

## File structure

```
/
├── CLAUDE.md        ← binding decisions + hard rules
├── DECISIONS.md     ← the decision record (ADRs, as prose sections)
├── GLOSSARY.md      ← the agreed term for each concept
├── README.md        ← what the app does today
├── src-tauri/src/   ← Rust backend
└── ui/              ← frontend (plain HTML/CSS/JS)
```

This repo keeps its decisions as **prose sections in `DECISIONS.md`**, not as
numbered files under `docs/adr/`. When a skill says "read the ADRs", read
`DECISIONS.md`. When a skill says "write an ADR", add a section there with
the decision and its reasoning — and, if something was tried and rejected,
the measurement that killed it. Don't create `docs/adr/` alongside it; that
would split the record.

## Use the glossary's vocabulary

When your output names a domain concept (in an issue title, a refactor
proposal, a hypothesis, a test name), use the term as defined in
`GLOSSARY.md`. Don't drift to synonyms the glossary explicitly avoids.

Read `GLOSSARY.md` for the terms themselves; don't keep a second copy of
them here. Two places that list the same words will drift, which is the
problem the glossary exists to solve.

The traps that bite most often: **ticket** is the Proxmox credential and
never a unit of work (work items are **issues**, and a feature is an
**F-number**); **entry** is not a connection, and the word is already taken
by desktop entries and keyring entries; **kind** is only the Rust spelling of
**Type**, because `type` is a reserved word.

If the concept you need isn't in the glossary, that's a signal: either you're
inventing language the project doesn't use (reconsider) or there's a real gap
(note it for `/domain-modeling`).

## Flag conflicts with the record

If your output contradicts something in `DECISIONS.md` or `CLAUDE.md`,
surface it explicitly rather than silently overriding:

> _Contradicts "Liveness probe hits `/`, not the API" in DECISIONS.md, but
> worth reopening because…_

For anything in `CLAUDE.md`'s *Hard rules*, don't reopen it in passing —
stop and ask.
