# pve-desk — Roadmap

**This file is a map, not the spec.** Outstanding work lives in
[GitHub issues](https://github.com/AdamBougher/pve-desk/issues), one issue per
feature, and the issue body is the spec — written as Gherkin, with its open
questions as `#` comments and its acceptance criteria as `Scenario:` blocks.
This file only says what the passes are and which issues are in them.

Conventions for reading and writing those issues are in
[`docs/agents/issue-tracker.md`](docs/agents/issue-tracker.md); the labels are
in [`docs/agents/triage-labels.md`](docs/agents/triage-labels.md).

Scenarios are documentation, not an executable suite: there is no
`cucumber-rs` in the tree, because most scenarios need a live node or a GUI
click, which is exactly what cannot be automated here. A scenario is true
because someone ran it and said so — which is why so many carry a `#` note
about what was actually measured. **"Done" means someone ran them against the
real node, not that it compiled.**

A finished feature is closed, not edited into silence: its decisions and its
dead ends go to `DECISIONS.md`, what it does for a user goes to `README.md`,
and anything still unrun moves to
[#30 Shipped but unproven](https://github.com/AdamBougher/pve-desk/issues/30).
If something is in neither this file nor an open issue it is either done or
was cut — check `DECISIONS.md` before proposing it again, since several
approaches in there were built, tested and removed.

---

## The three passes

v1 is a launcher, Admin, and a local list with power actions, and it is done.
v2 is performance, and the local view earning the name *Home*. v3 is Home
becoming a replacement for Admin: a guest's logs, its shell, and its options.

### v1 — complete

Nothing outstanding. F12 closed on 2026-10-24: `makepkg -si` installs the
Arch package and its `.desktop` entry, and the published AppImage carries a
checksum that verifies. One thing was never run and is not a feature — the
AppImage has never been *seen to start* on a machine without Rust, which this
one cannot be. That scenario lives in
[#30](https://github.com/AdamBougher/pve-desk/issues/30).

### v2 — performance, and Home earning its name

The local view stops being "the guest list" and becomes **Home**: the place
you land, that answers the routine questions without a trip to Admin. The
performance work is in the same pass because the list is what gets slower as
Home grows.

F31 to F34 are the same pass seen from the other side: how a row *reads*.
A status light instead of a word, RAM in a unit that suits the number, a
type you recognise without reading it, and Log out in the bar that is always
on screen rather than in a header the admin panel hides.

| Issue | Feature | Tags |
| --- | --- | --- |
| [#10](https://github.com/AdamBougher/pve-desk/issues/10) | F21 — The local tab is Home | `todo` |
| [#11](https://github.com/AdamBougher/pve-desk/issues/11) | F22 — The list redraws only what changed | `todo` `perf` |
| [#12](https://github.com/AdamBougher/pve-desk/issues/12) | F23 — Polling stops when nothing is watching | `todo` `perf` |
| [#13](https://github.com/AdamBougher/pve-desk/issues/13) | F24 — Cold start and connect are measured, then faster | `todo` `perf` `open` |
| [#14](https://github.com/AdamBougher/pve-desk/issues/14) | F25 — A guest's detail without leaving Home | `todo` |
| [#15](https://github.com/AdamBougher/pve-desk/issues/15) | F26 — Sorting, and filters worth having | `todo` `open` |
| [#16](https://github.com/AdamBougher/pve-desk/issues/16) | F11 — Favourites pinned to the top | `todo` |
| [#17](https://github.com/AdamBougher/pve-desk/issues/17) | F31 — Status is a light, not a word | `todo` `open` |
| [#18](https://github.com/AdamBougher/pve-desk/issues/18) | F32 — RAM reads in the unit it deserves | `todo` |
| [#19](https://github.com/AdamBougher/pve-desk/issues/19) | F33 — Log out moves to the top bar | `todo` `open` `linux` |
| [#20](https://github.com/AdamBougher/pve-desk/issues/20) | F34 — A guest's type is an icon beside its name | `todo` `open` |

### v3 — Home instead of Admin, first pass

| Issue | Feature | Tags |
| --- | --- | --- |
| [#21](https://github.com/AdamBougher/pve-desk/issues/21) | F15 — Home replaces the embedded panel | `todo` |
| [#22](https://github.com/AdamBougher/pve-desk/issues/22) | F27 — A guest's logs in Home | `todo` `open` |
| [#23](https://github.com/AdamBougher/pve-desk/issues/23) | F28 — A guest's shell in Home | `todo` `open` |
| [#24](https://github.com/AdamBougher/pve-desk/issues/24) | F29 — A guest's options in Home | `todo` `open` |

### Later — not scheduled

| Issue | Feature | Tags |
| --- | --- | --- |
| [#25](https://github.com/AdamBougher/pve-desk/issues/25) | F7 — SPICE console | `wip` `blocked` |
| [#26](https://github.com/AdamBougher/pve-desk/issues/26) | F10 — System tray | `todo` |
| [#27](https://github.com/AdamBougher/pve-desk/issues/27) | F17 — Windows and macOS | `unverified` `windows` |
| [#28](https://github.com/AdamBougher/pve-desk/issues/28) | F18 — Containerised dev environment | `unverified` `linux` |
| [#29](https://github.com/AdamBougher/pve-desk/issues/29) | F30 — A password prompt when the keyring will not answer | `todo` |
| [#31](https://github.com/AdamBougher/pve-desk/issues/31) | F36 — Proxmox Datacenter Manager as a second remote type | `todo` |

---

## Shipped but unproven

[#30](https://github.com/AdamBougher/pve-desk/issues/30) holds the code that
exists and compiles whose scenarios nobody has run. Not features — there is
nothing to build — but the honest boundary of what works, and each one is a
thing to check when the opportunity appears.

---

## F-numbers

The `F<n>` in an issue title is permanent and is how features refer to each
other. It is **not** the issue number and the two do not match: cite the
F-number in prose, the `#number` only in links. Highest assigned: F36 — F35
is deliberately retired, see `DECISIONS.md`.
