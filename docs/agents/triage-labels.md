# Triage Labels

The skills speak in terms of five canonical triage roles. This file maps those roles to the actual label strings used in this repo's issue tracker.

| Label in mattpocock/skills | Label in our tracker | Meaning                                  |
| -------------------------- | -------------------- | ---------------------------------------- |
| `needs-triage`             | `needs-triage`       | Maintainer needs to evaluate this issue  |
| `needs-info`               | `needs-info`         | Waiting on reporter for more information |
| `ready-for-agent`          | `ready-for-agent`    | Fully specified, ready for an AFK agent  |
| `ready-for-human`          | `ready-for-human`    | Requires human implementation            |
| `wontfix`                  | `wontfix`            | Will not be actioned                     |

When a skill mentions a role (e.g. "apply the AFK-ready triage label"), use the corresponding label string from this table.

Edit the "Label in our tracker" column to match whatever vocabulary you actually use.

## Roadmap tag labels

The Gherkin blocks in the old `ROADMAP.md` carried tags that are now labels.
Apply them on every feature issue: exactly one pass, exactly one state, and
any qualifier that applies.

| Label | Was | Meaning |
| --- | --- | --- |
| `v1` | `@v1` | pass 1 — launcher, embedded panel, local list with power actions |
| `v2` | `@v2` | pass 2 — performance, and the local view earning the name *Home* |
| `v3` | `@v3` | pass 3 — Home becoming a replacement for Admin |
| `later` | `@later` | beyond the three passes |
| `todo` | `@todo` | not started |
| `wip` | `@wip` | code exists, not all scenarios are true |
| `unverified` | `@unverified` | code exists and compiles; nobody has run the scenarios |
| `blocked` | `@blocked` | waiting on something outside the repo — hardware, a VM, a node |
| `open` | `@open` | the scenario is still a question, not a decision |
| `perf` | `@perf` | its acceptance criteria are timings or counts |
| `linux` | `@linux` | Linux-specific |
| `windows` | `@windows` | Windows-specific |

`unverified` is the one that matters most here: it means *written but never
run*. Per `CLAUDE.md`, a feature does not lose it until someone has executed
its scenarios against a live Proxmox node. Don't clear it because the code
compiles.

The `@done` tag on an individual scenario stays inside the issue body — it
marks one scenario green while the feature as a whole is not.
