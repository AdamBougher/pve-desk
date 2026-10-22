# pve-desk — completed and cut features

Moved out of `FEATURES.md` once resolved, so that file shows only
remaining work. Kept rather than deleted: these entries carry the
decisions, the measurements behind them, and several "don't try this
again" notes.

Status key: `[x]` done · `[-]` cut or superseded

---

### F1. First-run setup
`[x]` A setup screen shown when no saved login exists.

**Superseded by F16 (2026-10-21):** there is no separate first-run screen any
more. The same fields live in the launcher's "Add a connection" form, which
is open by default when no connections are saved. Everything below still
describes how that form behaves.

Built 2026-10-20: config schema below, input validation in
`HostConfig::validate`, realm placeholder in the form, 13 unit tests in
`config.rs`. Still `[~]` not `[x]`: `save_login` only persists after a
successful login, so the path end to end is unverified without a live node.

- Fields: host, port (default 8006), username, realm (`pam` / `pve`), password
- On save: test the login, then store the password in the system keyring
  and the rest in a config file (`~/.config/pve-desk/config.toml`)

Decisions (2026-10-20):
- Q1.1 **Multi-host schema, one host in it.** `config.toml` uses an
  `[[hosts]]` array from day one, with a single entry (`name = "main"`) and no
  host picker in the UI, so adding hosts later needs no format migration.
  App-level settings (`remote_viewer_path`) stay top-level, outside the host
  entries; only per-connection settings live in a host entry.
- Q1.2 **Cluster, no failover in v1.** One address is the entry point; any
  cluster member serves the whole `/cluster/resources` list and proxies
  consoles. Trying several node addresses at login is a later feature.
- Q1.3 **Both realms offered, neither preselected.** The realm `<select>`
  starts on a disabled placeholder and is `required`, so setup cannot be
  submitted without an explicit choice.

### F2. Saved login + session refresh
`[x]` Read the password from the keyring and call
`POST /api2/json/access/ticket` for a session ticket + CSRF token, then keep
that session alive for as long as it is used.

- Startup auto-login was **replaced by the launcher** (F16): the app always
  opens on the connection list and connects when you pick one.
- Re-login quietly before the ticket expires (tickets last ~2h; refreshed
  past ~90 min)
- If the keyring is locked or unavailable, fall back to a password prompt —
  **not built**; the keyring has always been available here (Q2.1), so this
  has never been exercised.

Built 2026-10-21. Decided: **refresh on demand, retry on 401** rather than a
background timer. Before an authenticated call, a session older than ~90 min
is renewed; if the node rejects a ticket anyway, the app logs in once more and
retries that call.

Why not a timer: age alone is not trustworthy. `Instant` uses
CLOCK_MONOTONIC, which does not advance while the machine is suspended, so a
laptop that slept through the ticket's lifetime still looks fresh. The 401
path is ground truth and is needed either way, which left the timer earning
very little. It also avoids a task whose lifetime has to track connect,
logout and switching connections.

Verified against the live node: with the threshold temporarily lowered to 10s,
two automatic re-logins fired during an 80-second run and the guest list kept
loading throughout, with no errors.

Not verified: the real ~90 min path (it would take 90 minutes), and recovery
when re-login genuinely fails — the `session-lost` path below has never been
triggered for real.

Open questions:
- Q2.1 **Answered by probing the machine (2026-10-20).** KDE Plasma on
  Wayland. `org.freedesktop.secrets` is owned by `gnome-keyring-daemon`
  (`kwalletd6` and `ksecretd` also run, but do not own that name). A
  throwaway secret was written and read back with no unlock prompt, so the
  `keyring` crate's Secret Service backend works as-is. A fallback password
  prompt is therefore not needed to get connected, but is still worth having
  for a locked keyring.
- Q2.2 **Answered 2026-10-21: return to the launcher with the reason.** The
  original framing (error+retry vs reopen setup) was overtaken by F16 — there
  is no setup screen any more, there is a launcher. When the node rejects the
  ticket *and* re-login fails, the backend drops the session and emits
  `session-lost`; the UI stops the 30s poll, clears the list and goes back to
  the launcher showing why. Decided rather than asked, since the options
  collapsed to one sensible answer once the launcher existed.

### F3. Certificate pinning
`[x]` Proxmox uses a self-signed cert by default. On first connect, show the
cert's SHA-256 fingerprint, ask you to confirm it, and store it. On later
connects, reject any cert that doesn't match.

Done for the **webview** (2026-10-21): the admin panel catches WebKitGTK's
TLS failure, compares the cert's SHA-256 against `cert_fingerprint` in the
config, and calls `allow_tls_certificate_for_host` for that one host. First
connect stores what it sees and reports it to the UI; a later mismatch is
refused and the error page stands. Verified: the stored fingerprint matches
the node's real leaf cert byte for byte.

Done for the **API client** (2026-10-21). `danger_accept_invalid_certs` is
gone from the codebase entirely. A custom `rustls::ServerCertVerifier` accepts
a certificate when it verifies against the OS trust store **or** matches the
pinned SHA-256, so a real certificate works and survives renewal while a
self-signed one is pinned. Decided under Q3.3: an unverified, unpinned
certificate stops the connection and the UI shows the fingerprint for
confirmation — the TLS handshake is refused before the request body is sent,
so the password never reaches an unverified server.

Verified against the live node: with the fingerprint pinned, connect succeeds
and the guest list loads; with the pin removed, the connection is refused and
the UI shows `F8:53:5C:…:51:40`, matching the node's real certificate.

New direct dependencies: `rustls`, `rustls-native-certs`, `sha2` — all were
already in the tree via reqwest, so nothing new is compiled.

Open questions:
- Q3.3 **Answered: ask.** An unverified certificate blocks the connection
  until the user confirms the fingerprint. The admin panel's webview still
  trusts on first use, but by then the API client has already pinned the
  fingerprint, so in practice it validates against it.

- Q3.1 **Answered by inspection:** self-signed, issued by the cluster's own
  `PVE Cluster Manager CA`, `CN=home.opah-tuna.ts.net`, valid to 2027-09-08.
- Q3.2 **Answered by inspection:** reached over the LAN at `192.168.1.2`.
  The cert's SAN also covers `home`, `localhost` and the Tailscale name
  `home.opah-tuna.ts.net`, so a VPN route would work without a new cert.

### F4. Embedded admin panel
`[x]` A window showing the full Proxmox web UI, already logged in, by handing
the session ticket to the webview as the `PVEAuthCookie` cookie.

Default view since 2026-10-21: the panel opens automatically as soon as a
session starts (auto-login or first-run setup), centered and focused. The
guest list stays open in the `pve-desk` window behind it; its "Admin panel"
button refocuses the panel if you close or lose it.

Verified 2026-10-21 against the live node: panel opens on the datacenter view
with both nodes listed, signed in as `root@pam` with no login form, so
`WebviewWindow::set_cookie` does survive the navigation. Not yet checked:
whether the Console button works, since Proxmox opens consoles with
`window.open` and Tauri does not handle popups by default.

Decisions (2026-10-21):
- Q4.1 **Separate window**, label `admin`. Asked but not answered directly;
  the stated goal was "do everything from the webview without opening my
  browser", which either layout satisfies. Chose the separate window because
  no capability names that label, so the hard rule (remote UI must not reach
  app commands) is visible at a glance rather than argued from origin rules.
  Proxmox sends no `X-Frame-Options` or CSP, so a tab/iframe stays viable if
  this proves annoying.
- Q4.2 Should closing the main window quit the app, or keep it in the
  system tray? **Still open.**

Cert handling: see F3. The CA-trust route was abandoned when it turned out
the node's sshd refuses password auth for root, so the CA file could not be
copied off without manual steps.

### F6. Power controls
`[-]` **Cut from v1 (2026-10-21).** The embedded Proxmox UI already does
start / shutdown / reboot, and that is where the work happens in practice, so
our own buttons duplicated it without ever being exercised.

The backend is harmless and stays: `Session::power` and its allow-list are
still there and still registered, just not driven by any UI. Delete or revive
when F15 (native UI) is decided. Q6.1 and Q6.2 are moot until then.

### F13. Log out
`[x]` Button in the main header. Drops the in-memory session and closes the
admin panel window, then returns to the setup screen.

Confirmed working by the user 2026-10-21.

Decided 2026-10-21: **non-destructive**. The config file and the keyring
entry are left in place, so the next launch auto-logs in again. Open
question: should there also be a "Forget this login" action that deletes the
keyring entry and config (i.e. a real sign-out rather than a session drop)?

### F16. Login launcher with saved connections
`[x]` The app always opens on a login screen listing saved connections
(host, username, realm), rather than auto-connecting to one. Pick one to
connect. Each entry shows whether it is usable.

Asked for 2026-10-21: "always open to login, add a list of saved ip,
username, password combos, maybe show if an active login is still good."

**Reverses F2's auto-login on startup.** Quiet re-login before ticket expiry
still matters *within* a session — that is a separate concern and still
needed (the ~90 min refresh is still a TODO).

Already in place, no migration needed:
- `config.toml` is already an `[[hosts]]` array (Q1.1), today holding one
  entry. Adding entries needs no format change.
- `secrets.rs` already keys the keyring on `userid@host`, so several
  passwords coexist.

Hard rule unchanged: passwords live in the keyring only. The list shows host,
username and realm — never the password, and nothing is written to
`config.toml` but the non-secret fields.

Decisions (2026-10-21):
- Q16.1 **One live session, map-shaped.** `AppState` holds
  `HashMap<String, Session>` keyed by config entry id, with `set_only`
  enforcing a single entry. Allowing several later is a local change.
- Q16.2 **Reachable + ticket valid, on demand.** Opening the launcher (and
  the Re-check button) probes each entry; no timers, no background tasks.
  Reachability sends no credentials.
- Q16.3 **Connecting opens the admin panel immediately**, as before — it is
  the view actually used. Decided rather than asked.
- Q16.4 **`default` becomes "last used"**, written on every  successful connect
  and used only to preselect a row. Never auto-connects. Decided rather than
  asked.

Entry ids: new entries are named `user@realm@host` (`HostConfig::id`), so the
same account on two nodes never collides. The pre-existing `main` entry keeps
working untouched — ids are opaque and never shown; the UI displays the
fields.

Built and verified 2026-10-21: the launcher lists the saved entry, detects
the keyring password, reports Reachable, and connecting works end to end
(exercised repeatedly while testing F3 and F2 — it is how the app is used).

Not verified: adding a *second* connection, and switching between two.

Still open:
- Q16.5 Should entries be removable from the launcher? Deleting one means
  deleting its keyring password, which is destructive and needs a decision.

### F8. VNC console (non-SPICE VMs)
`[-]` **Dropped (2026-10-21): superseded by F4.** The embedded Proxmox UI
already provides the noVNC console, and it works. Only worth revisiting as
part of F15.

### F9. LXC terminal
`[-]` **Dropped (2026-10-21): superseded by F4.** The embedded Proxmox UI
already provides the container shell, and it works. Only worth revisiting as
part of F15.
