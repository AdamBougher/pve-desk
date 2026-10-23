# pve-desk — Roadmap

**Outstanding work only, written as Gherkin.** A feature leaves this file the
moment its scenarios pass by hand: the decisions behind it go to
`DECISIONS.md`, what it does for a user goes to `README.md`, and the block
here is deleted. If something isn't in this file it is either done or was
cut — check `DECISIONS.md` before proposing it again, since several
approaches in there were built, tested and removed.

Scenarios are the spec. Implementing a feature means making its scenarios
true; "done" means someone ran them against the real node, not that they
compile. They are **documentation, not an executable suite** — there is no
`cucumber-rs` in the tree, and whether there should be is Q31.1 below rather
than a given.

`#` comments inside a feature carry its **open questions**. They are asked
one at a time and the answers land in `DECISIONS.md`, per *How to work on a
feature* in `CLAUDE.md`.

| Tag | Meaning |
| --- | --- |
| `@v1` `@v2` `@v3` `@later` | which pass it belongs to |
| `@todo` | not started |
| `@wip` | code exists, not all scenarios are true |
| `@unverified` | code exists and compiles; nobody has run the scenarios |
| `@blocked` | waiting on something outside the repo — hardware, a VM, a node |
| `@open` | the scenario is still a question, not a decision |
| `@perf` | its acceptance criteria are timings or counts |
| `@linux` `@windows` | platform-specific |

**The three passes.** v1 is a launcher, the embedded Proxmox panel, and a
local list with power actions — complete apart from packaging. v2 is
performance, and the local view earning the name *Home*. v3 is Home becoming
a replacement for Admin: a guest's logs, its shell, and its options.

---

## v1 — what's left

Two features, F12 and F20, seven scenarios between them:

- **Green (2).** The two `.desktop` files agree on the window class; the
  rasters are regenerated from the SVG.
- **Waiting on one look at a dock (2).** F12's "the taskbar shows the app's
  icon" and F20's "every part of the mark is visible". Same look settles
  both, and it needs only a local install — not the release. In progress:
  the dock entry integrated from the v1.0 AppImage was still showing the
  pre-fix icon on 2026-10-23, so its 256px raster was replaced by hand with
  the current one to make the look testable.
- **Waiting on the release (4).** The AppImage starting, `sha256sum -c`,
  `makepkg -si`, and the stale-entry question (Q12.2) all describe an
  artifact that does not exist yet.

So **v1 cannot be fully green before the release; the release is what makes
those three runnable.** Pushing `v1.0.1` is a step *in* F12, not something
that waits on it.

```gherkin
@v1 @wip
Feature: Packaging and release (F12)
  As someone who does not build from source
  I want one file to download and run
  So that installing pve-desk is not a cargo exercise

  # AppImage plus an Arch PKGBUILD, released on GitHub as drafts; what is
  # already written, and the `workflow`-scope token needed to push it, are in
  # "Release format" and "Releases on GitHub" in DECISIONS.md.
  # No longer blocked: tauri-cli 2.12.4, makepkg and xprop are all installed
  # here, and release.yml has run green three times — once on the v1.0 tag,
  # twice by dispatch. v1.0 is published with its AppImage and .sha256.
  # Releases are tagged with three components from v1.0.1 on, so the
  # PKGBUILD's v$pkgver always resolves.
  # F20 is done and no longer gates this: the icon is fixed in the tree and
  # ships in whatever this builds.
  # ORDERING, and it is circular: three scenarios below — the AppImage
  # running, sha256sum -c, and makepkg — are about a release artifact that
  # does not exist yet. They cannot go green before the v1.0.1 tag is
  # pushed; the tag is what makes them runnable. The two window-class
  # scenarios are the only ones closable beforehand.

  Scenario: The AppImage runs on a machine that never had Rust
    Given a v* tag built by .github/workflows/release.yml on ubuntu-22.04
    When I chmod +x the AppImage and run it on a glibc 2.35 or newer system
    Then the launcher window appears
    And the release it came from was a draft until I wrote its notes
    # Blocked on the v1.0.1 tag, then on a second machine — this one has
    # Rust and glibc 2.44. The v1.0 asset was downloaded and extracted here
    # on 2026-10-23 but never seen to start: run from its AppDir it died on
    # a relative path to WebKitNetworkProcess, and run whole it produced no
    # X11 window to find. Inconclusive, not a failure.

  Scenario: sha256sum -c succeeds on the file as downloaded
    Given the .sha256 published beside the AppImage
    When I run sha256sum -c on it in the directory I downloaded to
    Then it reports OK
    # Reproduced against the published v1.0 asset on 2026-10-23:
    #   sha256sum: src-tauri/target/release/bundle/appimage/
    #   pve-desk_1.0.0_amd64.AppImage: No such file or directory
    # It was built before 97a1fcc and records the build path, so the command
    # README.md:114 gives cannot find the file. The hash itself matches
    # GitHub's digest — the binary is sound, the filename is not. 97a1fcc
    # fixes it but has never run on a tag; v1.0.1 is that run.

  Scenario: The desktop entry groups the window under its own icon
    Given packaging/pve-desk.desktop declares StartupWMClass=pve-desk
    When the app is running and I read xprop WM_CLASS on its window
    Then the two strings match
    And the taskbar shows the app's icon rather than a generic one
    # The strings match as of 2026-10-23: xprop -name pve-desk WM_CLASS
    # reports "pve-desk", "Pve-desk" (Q12.1 settled, see DECISIONS.md). The
    # taskbar half is still unrun — it needs an X11 taskbar, and this
    # machine has no X11 session installed, only XWayland.

  Scenario: The AppImage and the Arch package agree on the window class
    Given the AppImage ships a .desktop Tauri generates, not packaging/'s
    When I read StartupWMClass from each
    Then they name the same string
    # Passes as of 2026-10-23: both name pve-desk, which is the class the
    # app actually reports. The AppImage side was read by extracting the
    # published v1.0 asset, so it is the shipped entry, not a local guess;
    # it derives from productName, which has not changed.
    # Still unmeasured: what a taskbar on a Wayland desktop keys on is the
    # xdg-shell app_id, and StartupWMClass is only the X11 fallback — so
    # both files agreeing does not by itself prove the dock groups it.

  Scenario: Installing a new AppImage does not leave the old one in the dock
    Given a desktop entry integrated from a previous AppImage
    When I integrate the AppImage from the next release
    Then the dock shows one pve-desk entry, not one per version
    # Found 2026-10-23 and unsolved. AppImage desktop integration names both
    # files after the versioned filename — ~/.local/share/applications/
    # pve-desk_1.0.0_amd64.desktop and icons/.../pve-desk_1.0.0_amd64.png —
    # so v1.0.1 will add a second entry beside the first rather than replace
    # it, and the stale one keeps the old icon and an Exec pointing at an
    # AppImage that may be gone. The Arch package does not have this problem:
    # it installs unversioned pve-desk.desktop and pve-desk.png.
    # Q12.2 Is this ours to fix at all? It is appimaged/the integration
    #   prompt doing the naming, not our bundle. Options: document the
    #   stale-entry cleanup in README, or ship nothing and let AppImage
    #   users live with it.

  Scenario: The Arch package builds from the release tarball
    Given packaging/arch/PKGBUILD
    When I run makepkg -si
    Then the source tarball downloads
    And cargo build --release runs and the tests pass
    And the binary, the .desktop entry, three icon sizes and the licence install
    # Failed at the download under v1.0: pkgver=1.0.0 asked for
    # v1.0.0.tar.gz, a 404, because the only tag was v1.0 — which extracts to
    # pve-desk-1.0 while the PKGBUILD cds into pve-desk-1.0.0. pkgver is 1.0.1
    # now; it resolves as soon as the v1.0.1 tag is pushed, not before.
```

```gherkin
@v1 @unverified
Feature: An app icon that survives a dark theme (F20)
  As someone with a dark panel and a dark tray
  I want the whole icon to be visible
  So that pve-desk is findable in a dock

  # Was: in src-tauri/icons/icon.svg the X carried fill:#e57000 while the top
  # and bottom chevrons carried no fill at all and defaulted to black, so on
  # a dark background only the orange X showed. Fixed — they are #f2f2f2 now
  # and the five tracked rasters were regenerated with `cargo tauri icon`.
  # Decided: an explicit light fill on the chevrons, not a backplate. One
  #   attribute, the mark keeps its shape and its transparent background,
  #   and a transparent glyph sits better in a tray than a plate does. The
  #   cost is accepted: on a light desktop the chevrons go faint and the
  #   orange X carries the mark. F12 (packaging) and F10 (tray) ship this file.

  Scenario: Every part of the mark is visible on a dark background
    Given src-tauri/icons/icon.svg
    When the icon is drawn on a dark panel or a dark tray
    Then both the orange chevrons and the pale ones read clearly
    And on a light desktop the orange carries the mark
    # Checked by compositing 128x128.png over #1c1c1c, #2e3436 and #ffffff:
    # both halves show on the two dark grounds, and #f2f2f2 goes faint on
    # white — the trade-off the fill was chosen with.
    # All that is left is one look at a real dock. That does NOT need the
    # release: copying packaging/pve-desk.desktop and the three icon files
    # into ~/.local/share is enough to see it, and it is the same look that
    # settles F12's "the taskbar shows the app's icon rather than a
    # generic one". A tray needs F10, which is @later — out of v1's scope.

  Scenario: The raster icons are regenerated from the source
    When icon.svg changes
    Then tauri icon is re-run
    And the PNGs and the .ico under src-tauri/icons are rebuilt from it
    # Passes: `cargo tauri icon` was re-run from the SVG on 2026-10-23 and
    # all five tracked rasters changed (commit 4dc4205). It was generated
    # into a temp dir and copied back selectively, so the .icns, the Windows
    # Store logos and the Android/iOS sets it also emits stayed out of a
    # tree that never carried them.
```

---

## v2 — performance, and Home earning its name

The local view stops being "the guest list" and becomes **Home**: the place
you land, that answers the routine questions without a trip to Admin. The
performance work is in the same pass because the list is what gets slower as
Home grows.

```gherkin
@v2 @todo
Feature: The local tab is Home (F21)
  As the person running pve-desk
  I want the local view to read as the app's home
  So that Admin is somewhere I go, not where I live

  # Rename only — no behaviour change. Touches the GTK tab label in
  # install_native_tabs, the "tab" event payload and its listener, the
  # #main section and its heading, and README/DECISIONS wording.
  # Q21.1 Does the window title follow the tab (pve-desk — Home), or stay
  #   the plain app name?

  Scenario: The tab strip says Home
    Given the app is running
    When I look at the native tab strip
    Then the local tab reads "Home"
    And nothing in the UI or the docs calls it "Guests"

  Scenario: Home is reachable before a session exists
    Given no connection is live
    When I press the Home tab
    Then the launcher is shown
    And the button does not look live while doing nothing visible
    # Today only Admin is greyed when disconnected, because Admin was the
    # one that could raise an error. Home's button stays live and inert.

  Scenario: The status line is visible wherever it is written to
    Given the admin panel is in front
    When the backend emits a certificate or session message
    Then I can see it without first switching tabs
    # Today the status strip lives inside the local webview, which is hidden
    # while the panel is up, so messages emitted during the panel's load are
    # only seen on the way back.
    # Q21.2 Move the strip into the GTK shell next to the tabs, or keep it
    #   in the page and surface panel-time messages some other way?
```

```gherkin
@v2 @todo @perf
Feature: The list redraws only what changed (F22)
  As someone watching 40 guests refresh every 30 seconds
  I want the table to stay still unless something actually moved
  So that Home is cheap to leave open all day

  # render() currently does replaceChildren(...shown.map(row)), so every
  # <tr>, every cell and every power button is rebuilt on each 30s tick and
  # on each keystroke in the search box, with fresh listeners each time.
  # matches() also re-lowercases every field of every row per keystroke.
  # Q22.1 Diff by vmid and patch changed cells, or keep the rebuild and
  #   only skip it when the payload is identical? The second is a few lines
  #   and gets most of the 30s case; the first is what makes typing cheap.
  # Q22.2 Precompute each guest's search haystack once at fetch time?

  Scenario: An unchanged refresh changes no DOM
    Given 40 guests are listed
    When the 30s refresh returns an identical payload
    Then no table row is replaced
    And no event listener is re-bound

  Scenario: One guest changing status touches one row
    Given 40 guests are listed and one is stopped
    When it reports "running" on the next refresh
    Then only that row's status cell and power buttons change

  Scenario: Typing in the search box does not rebuild the table
    Given 40 guests are listed
    When I type four characters into the search box
    Then the match test runs against precomputed text
    And rows are hidden or shown rather than recreated

  Scenario: A power action in flight survives a refresh
    Given I have pressed Shutdown on a guest
    When the 30s refresh lands before the request returns
    Then that row's buttons are still disabled
    # True today via the `busy` set; it must stay true under diff rendering.
```

```gherkin
@v2 @todo @perf
Feature: Polling stops when nothing is watching (F23)
  As someone who leaves the app open on the Admin tab
  I want it to stop asking the node for a list nobody is reading
  So that the node is not answering /cluster/resources every 30s for nothing

  # The 30s timer runs whenever a session exists, including while the local
  # webview is hidden behind the admin panel, and while the window is
  # minimised. The backend already emits "tab" on a switch, so the signal
  # exists.
  # Q23.1 Pause on tab switch only, or also on window focus loss or
  #   minimise? Focus is a tauri window event; tab is already wired.
  # Q23.2 On resume, refresh immediately or wait for the next tick? An
  #   immediate read makes the first glance correct; it also means a burst
  #   if the tab is being flipped.

  Scenario: Switching to Admin stops the poll
    Given a session is live and Home is in front
    When I switch to the Admin tab
    Then no further guest list requests are made

  Scenario: Switching back to Home is immediately current
    Given the poll is paused and the list is 10 minutes stale
    When I switch to Home
    Then a refresh is issued at once
    And the 30s cadence resumes from there

  Scenario: A paused poll is not a dropped session
    Given the poll has been paused for an hour
    When I switch back to Home
    Then the session is refreshed on demand as usual
    And no session-lost is emitted merely because time passed
```

```gherkin
@v2 @todo @perf @open
Feature: Cold start and connect are measured, then faster (F24)
  As someone opening the app to check one VM
  I want it usable in about a second
  So that it competes with a browser tab I already have open

  # Nothing here is measured yet, and the first job is a number, not a fix.
  # Candidates, in the order they are worth checking:
  #   - time from exec to the launcher being interactive (webview start
  #     dominates, probably)
  #   - time from Connect to the admin panel being usable: login, the
  #     cookie, the panel's own load of the Proxmox UI
  #   - the per-row reachability probe, already parallel per row
  #   - release profile: lto, codegen-units, strip
  # Q24.1 What is the target? "Launcher in under a second, panel in under
  #   three" is a guess until the baseline exists.
  # Q24.2 Is a splash or a skeleton acceptable, or must the first paint be
  #   the real launcher?

  Scenario: The launcher paints before the node is reached
    Given two saved connections, one of them unreachable
    When I start the app
    Then the rows appear with their fields filled in
    And each row's status resolves on its own without blocking the others
```

```gherkin
@v2 @todo
Feature: A guest's detail without leaving Home (F25)
  As someone diagnosing one guest
  I want its numbers in Home
  So that "how much disk has it got" is not an Admin trip

  # /cluster/resources already carries disk, maxdisk, uptime and netin/
  # netout for each guest; the table shows none of them. Per-guest reads
  # (config, agent-reported IPs) are extra calls and should stay on demand.
  # Q25.1 Expand the row in place, or a side panel? A row that grows keeps
  #   one view; a panel has room for charts later.
  # Q25.2 Is a qemu-guest-agent IP lookup in scope, given it fails on every
  #   guest without the agent and needs a per-guest call?
  # Q25.3 Per-guest history (a CPU sparkline) needs /rrddata — worth it, or
  #   is that the point where Admin is simply better?

  Scenario: Opening a guest shows what the cluster listing already knows
    Given Home is listing a running VM
    When I open that guest's detail
    Then uptime, disk and maxdisk, and network totals are shown
    And no extra API call was needed to show them

  Scenario: Closing the detail leaves the list as it was
    Given a guest's detail is open and the search box has a query in it
    When I close the detail
    Then the filtered list is unchanged
    And the 30s refresh has not been reset

  Scenario: Detail for a guest that disappears
    Given a guest's detail is open
    When the next refresh no longer lists that guest
    Then the detail closes with a message saying it is gone
    And the list stays usable
```

```gherkin
@v2 @todo @open
Feature: Sorting, and filters worth having (F26)
  As someone with more guests than rows on screen
  I want to order and narrow the list
  So that finding a guest does not depend on remembering its name

  # Q5.2 settled v1 as "a search box only, no status or tag filters" — the
  # list existed to find a guest. Home changes that premise, so this
  # revisits it deliberately rather than by accident.
  # Q26.1 Which columns sort? vmid, name, node, status have obvious orders;
  #   CPU and RAM sort by current value, which moves every 30 seconds.
  # Q26.2 Is the sort remembered across restarts? That means a key in
  #   config.toml, i.e. a schema change.
  # Q26.3 Status and tag filters, or does search over tags already cover
  #   it? Templates stay hidden either way (Q5.3).

  Scenario: Sorting by a column holds across a refresh
    Given Home is sorted by name
    When the 30s refresh returns
    Then the order is still by name
    And the row I was pointing at has not moved under the cursor

  Scenario: A filter and the search box compose
    Given a status filter of "running"
    When I also type "web" in the search box
    Then only running guests matching "web" are listed
    And the count reads "N of M"
```

```gherkin
@v2 @todo
Feature: Favourites pinned to the top (F11)
  As someone with three guests I touch and twenty I don't
  I want those three at the top
  So that Home is useful without scrolling or searching

  # Q11.1 Where does the pin live? config.toml means a schema change and a
  #   decision about whether pins are per connection (they should be — a
  #   vmid is only unique within a cluster).
  # Q11.2 Pinned rows: a separate section above the table, or the same
  #   table re-ordered? Interacts with F26's sorting.
  # Q11.3 Does F10's tray read this list? That was the tray's whole
  #   premise, so probably yes, which makes the storage decision shared.

  Scenario: A pinned guest leads the list
    Given 20 guests and one of them pinned
    When Home renders
    Then the pinned guest is above the others
    And it is still subject to the search box

  Scenario: Pins survive a restart
    Given a pinned guest
    When I quit and reopen the app and connect to the same entry
    Then it is still pinned

  Scenario: Pins belong to their connection
    Given a guest pinned on connection A
    When I connect to connection B which has a guest with the same vmid
    Then that guest is not pinned
```

---

## v3 — Home instead of Admin, first pass

```gherkin
@v3 @todo
Feature: Home replaces the embedded panel (F15)
  As someone who only opens Admin for three things
  I want those three things in Home
  So that the embedded Proxmox UI becomes optional rather than the point

  # The umbrella for v3. It is not "re-implement the Proxmox web UI" — that
  # UI is good and this app embeds it. It is: find the handful of jobs that
  # send me to Admin daily and bring them over. First pass is F27 logs, F28
  # shell, F29 options; the panel stays as the fallback for everything else.
  # Q15.1 What is the actual exit criterion — a week of daily use without
  #   opening Admin? Without one, v3 has no end.
  # Q15.2 Does Admin stay a tab forever, or does it become something you
  #   enable? Keep it: it is the escape hatch, and it is already proven.

  Scenario: The three v3 jobs are done from Home
    Given a connected session
    When I read a guest's recent task log, open its shell and read its options
    Then I did not switch to the Admin tab

  Scenario: Admin is still there when Home cannot help
    Given I want storage, backups or cluster configuration
    When I switch to Admin
    Then the full Proxmox UI works exactly as it does today
```

```gherkin
@v3 @todo @open
Feature: A guest's logs in Home (F27)
  As someone whose VM just did something odd
  I want to read what Proxmox recorded about it
  So that I can tell what happened without the panel's task tree

  # What "logs from a guest" means has to be settled before any code: the
  # API exposes Proxmox's *task* history for a guest (per the API viewer:
  # /nodes/{node}/tasks with a vmid filter, then a task's own log by UPID),
  # not the guest OS's journal. The guest's own logs need either the shell
  # (F28) or a qemu-guest-agent exec, which is a different feature with
  # different permissions. To confirm against
  # https://pve.proxmox.com/pve-docs/api-viewer/ rather than assumed.
  # Q27.1 Task history only for the first pass, or task history plus a
  #   node-journal view filtered to that guest?
  # Q27.2 Live tail, or read on open? A tail means polling or a websocket,
  #   and the refresh model so far is deliberately pull-on-demand.
  # Q27.3 Which permission does this need? VM.Audit lists tasks; reading a
  #   task's log may want Sys.Audit on the node. Must be checked, because
  #   the account is root@pam here and that proves nothing about a normal
  #   account.

  Scenario: The recent tasks for one guest
    Given a guest that was started, stopped and backed up this week
    When I open its log view in Home
    Then I see those tasks newest first, with who ran them and the outcome
    And a failed task is distinguishable from a successful one at a glance

  Scenario: Reading one task's output
    Given a failed backup task in the list
    When I open it
    Then its log lines are shown as the node recorded them
    And the text is inserted with textContent, never as markup

  Scenario: A guest with no task history
    Given a guest nothing has happened to
    When I open its log view
    Then it says so plainly rather than showing an empty frame

  Scenario: The node refuses the read
    Given an account without the permission the log read needs
    When I open a guest's log view
    Then the error from the node is shown
    And no part of the UI is left spinning
```

```gherkin
@v3 @todo @open
Feature: A guest's shell in Home (F28)
  As someone who uses the container shell daily
  I want it in Home
  So that the most-used thing in Admin is no longer a reason to open Admin

  # This is the biggest piece of v3 and it is mostly an architecture
  # question, not a Proxmox one. Per the API viewer, the node offers
  # termproxy plus vncwebsocket for lxc and for qemu serial consoles; the
  # transport is a websocket carrying terminal bytes. We would then need a
  # terminal emulator in a frontend that deliberately has no bundler, no
  # Node and no framework (CLAUDE.md), and xterm.js is an npm package.
  # Q28.1 Vendor a terminal emulator into ui/ as plain JS, write a minimal
  #   VT handler, or keep the shell as a webview pointed at the panel's own
  #   console (which is Admin again, honestly labelled)? This one is an
  #   architecture decision and must be asked before any code.
  # Q28.2 Who holds the websocket — the frontend with a ticket, or Rust,
  #   proxying bytes over Tauri events? Rust keeps the ticket out of the
  #   page, which is the direction every other decision here went.
  # Q28.3 LXC only for a first pass? A VM shell needs a serial port
  #   configured on the guest; containers get one free.
  # Q28.4 Does this need a new capability, and can it be kept off the admin
  #   webview? No remote origin may reach app commands (hard rule).

  Scenario Outline: Opening a shell on a guest that has one
    Given a running <kind> guest with <prerequisite>
    When I open its shell from Home
    Then I get an interactive prompt
    And typing is as responsive as the panel's console on the same guest

    Examples:
      | kind | prerequisite            |
      | lxc  | nothing extra           |
      | qemu | a serial0 port configured |

  Scenario: A guest that cannot offer a shell
    Given a running VM with no serial port
    When I look at it in Home
    Then no shell action is offered
    And the reason is discoverable rather than a dead button

  Scenario: The ticket does not reach the page
    Given a shell session is open
    When I inspect what the frontend was given
    Then it never held the session ticket
    # Only if Q28.2 goes the Rust-proxy way; if the frontend holds the
    # socket, this scenario is replaced by an explicit decision to let it.

  Scenario: Closing the shell releases the node side
    Given a shell session is open
    When I close it
    Then the websocket is closed and no process is left behind on the node
```

```gherkin
@v3 @todo @open
Feature: A guest's options in Home (F29)
  As someone checking what a VM is actually configured with
  I want to read its options in Home
  So that "how much RAM does it have" needs no panel

  # Per the API viewer, /nodes/{node}/{kind}/{vmid}/config reads a guest's
  # configuration, and the same path with PUT writes it. Read first: the
  # write path needs VM.Config.* permissions, careful validation and a
  # confirmation story, and it is the first thing in this app that would
  # change a guest's definition rather than its power state.
  # Q29.1 Read-only for v3, with editing as its own feature later? That is
  #   the recommendation; "maybe see guest options" was how this was asked
  #   for, so read is the ask and write is the temptation.
  # Q29.2 Which fields? The raw config is long and includes device lines
  #   that only mean something to Proxmox. A chosen subset (cores, memory,
  #   disks, network, boot order, agent, display) reads better and hides
  #   less than it seems.
  # Q29.3 If editing ever happens: does it go through an allow-list of
  #   editable keys, the way power does? Hard rule says the frontend must
  #   not drive arbitrary API calls, and a config PUT is exactly that
  #   shape.

  Scenario: Reading a VM's configuration
    Given a running VM
    When I open its options in Home
    Then cores, memory, disks, network and display are shown
    And every value came from the node rather than being inferred

  Scenario: Reading a container's configuration
    Given a running LXC container
    When I open its options
    Then the equivalent fields are shown for a container
    And fields that do not apply are absent rather than blank

  Scenario: Options are read-only in this pass
    When I look at a guest's options
    Then nothing in the view can change them
    And it is obvious that changing them is the panel's job for now
```

---

## Later — not scheduled

```gherkin
@later @wip @blocked
Feature: SPICE console (F7)
  As someone with a graphical desktop VM
  I want it to open in remote-viewer
  So that I get a real desktop rather than noVNC in a browser

  # The .vv handling is written and hardened; the launch path has never run.
  # Blocked twice: no remote-viewer on this desktop, and no graphical desktop
  # guest to point it at. Prerequisites, the console-vs-display trap and
  # Q7.1/Q7.2 are in "SPICE" (two sections) in DECISIONS.md.
  # Nothing in ui/ calls open_spice — the button went when F6 was cut and did
  # not return with it, though powerCell is now somewhere to put one.
  # Q7.3 How does a row know it qualifies? /cluster/resources does not report
  #   a guest's display type, so it needs a per-guest config read — which F29
  #   would already be doing.

  Scenario: Opening a SPICE console
    Given a running VM whose display is SPICE (qxl)
    And virt-viewer installed locally
    When I open its SPICE console from Home
    Then remote-viewer opens connected to that guest

  Scenario: The .vv file is private and brief
    When a SPICE console is launched
    Then the .vv file is created with mode 0600
    And it is deleted shortly after the viewer has read it

  Scenario: The deletion window is long enough
    Given remote-viewer needs to read the .vv file after being spawned
    When the file is deleted after the configured wait
    Then the viewer had already read it
    # The 3s wait is a guess and the symptom of it being wrong is a launch
    # failing with a missing-file error.

  Scenario: Two consoles at once
    Given a SPICE console is already open
    When I open a second one for a different guest
    Then each had its own .vv file
    And neither deletion affected the other
```

```gherkin
@later @todo
Feature: System tray (F10)
  As someone who keeps pve-desk running
  I want a tray icon with the guests I care about
  So that starting a VM does not need the window

  # Q10.1 Which guests does the menu list? Favourites (F11) was the
  #   premise, which makes F11 a prerequisite and the pin storage shared.
  # Q4.2 Should closing the window quit, or keep running in the tray? Open
  #   since F4 and only answerable once a tray exists.
  # Q10.2 Which actions in the menu — start and stop only, or the same
  #   three as Home? A tray with a force-stop and no confirmation is a
  #   different risk from a button in a table (Q6.2).
  # Needs trayIcon in tauri.conf.json (v2 renamed it from systemTray) and
  # ships src-tauri/icons/icon.svg, so F20 lands first.

  Scenario: Starting a guest from the tray
    Given a session is live and a guest is pinned
    When I pick Start for it in the tray menu
    Then it starts, and the result is visible without opening the window
```

```gherkin
@later @unverified @windows
Feature: Windows and macOS (F17)
  As someone with a Windows laptop
  I want pve-desk to run there too
  So that the saved logins and the signed-in panel are not Linux-only

  # Windows builds in CI and has never been run; macOS is after it (Q17.1).
  # What was verified by reading wry, keyring and reqwest, and Q17.2's answer
  # (native roots plus a CA import, not per-platform pinning), are in
  # "Cross-platform findings" in DECISIONS.md.
  # Q17.3 Does the admin panel actually open already signed in under
  #   WebView2? set_cookie exists there, but cookie domain and secure
  #   handling may differ. Needs the laptop.

  Scenario: The panel opens signed in on Windows
    Given a saved connection and a Proxmox node with a trusted certificate
    When I connect on Windows
    Then the admin panel loads the Proxmox UI with no login form

  Scenario: Windows has no tab strip, by decision
    When I connect on Windows
    Then the admin panel is its own window
    And nothing pretends there is a tab bar
    # The strip is a GTK widget; see "Linux first" in DECISIONS.md.

  Scenario: The API client trusts what the OS trusts
    Given the Proxmox cluster CA imported into the Windows trust store
    When pve-desk connects
    Then both the webview and the API client accept the node
    # Requires the rustls-tls-native-roots switch; webpki-roots would make
    # the webview trust it and the API client refuse.
```

```gherkin
@later @unverified @linux
Feature: Containerised dev environment (F18)
  As someone who wants a pinned toolchain
  I want builds and tests to run in a container
  So that a green run means something reproducible

  # Tier 1 (build and test, no GUI) works and is verified; Tier 2 is written
  # but commented out in devcontainer.json. Why it was demoted the same day
  # it was built is in "Dev container" in DECISIONS.md.
  # Q18.5 With the mounts enabled, does the GUI actually work from inside —
  #   a window, rendering, and the keyring over the mounted session bus?

  Scenario: A window appears from inside the container
    Given the display, /dev/dri and session D-Bus mounts are enabled
    When I run the app inside the container
    Then a window appears and renders

  Scenario: The keyring resolves over the mounted session bus
    Given the same mounts
    When the app stores and reads a password
    Then org.freedesktop.secrets answers inside the container
    # Without the session bus there is no keyring, so no saved logins.
```

```gherkin
@later @todo
Feature: A password prompt when the keyring will not answer (F30)
  As someone whose keyring is locked
  I want to be asked for my password
  So that a locked keyring is an inconvenience rather than a dead app

  # Never built. The keyring has always been available on this machine
  # (Q2.1), so this path has never been exercised and has no urgency — but
  # "saved logins don't stick" in the README's troubleshooting is this gap.
  # Q30.1 Prompt per connect, or offer to unlock and retry?
  # Q30.2 Does a password typed into the prompt get offered to the keyring
  #   for saving, or used once and dropped? Once-and-dropped is the safe
  #   default and keeps the hard rule trivially true.

  Scenario: Connecting with a locked keyring
    Given a saved connection whose password is in a locked keyring
    When I press Connect
    Then I am asked for the password
    And on success the session behaves like any other
    And the password is never written anywhere but the login request
```

---

## Shipped but unproven

Code that exists and compiles, whose scenarios nobody has run. Not features —
there is nothing to build — but they are the honest boundary of what works,
and each one is a thing to check when the opportunity appears.

```gherkin
@unverified
Feature: Paths that have never been exercised

  Scenario: The real ticket refresh fires
    Given a session older than 90 minutes
    When the next authenticated call is made
    Then it is re-logged-in first and the call succeeds
    # Only ever tested with the threshold lowered to 10s, where two
    # re-logins fired during an 80-second run with no errors.

  Scenario: A session the node will not renew
    Given a ticket the node rejects and a re-login that also fails
    When a refresh is attempted
    Then session-lost is emitted and the launcher returns with the reason
    # The code path exists; it has never triggered for real.

  Scenario: Two saved connections
    Given two entries in config.toml
    When I connect to one, log out, and connect to the other
    Then each gets its own session, keyring entry and pinned fingerprint
    # One entry is all that has ever been in the config.

  Scenario: A spawned viewer is reaped
    Given a SPICE console was launched
    When I close the viewer
    Then no zombie process is left until pve-desk exits
    # launch() drops the Child without waiting. Harmless at the scale of a
    # few consoles, but it is a leak.

  Scenario: The second read after a power action is late enough
    Given a power action on a slow node or a large VM
    When the list is re-read immediately and again after 3 seconds
    Then the row shows the new status
    # The 3s is a guess. A row still showing the old status is the symptom;
    # the fix is a longer second read, not a faster one.
```
