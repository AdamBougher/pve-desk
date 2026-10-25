// Frontend for pve-desk. Plain JS, no build step.
// Every backend call goes through `invoke` (see src-tauri/src/commands.rs).

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const $ = (id) => document.getElementById(id);
// F21 Q21.2: mirrored to the native label beside the tab strip too, so a
// message is visible even while Admin covers this page. Fire-and-forget —
// a failed invoke must not break the caller, and nothing here awaits it.
const status = (msg) => {
  $("status").textContent = msg || "";
  invoke("set_status", { msg: msg || "" }).catch(() => {});
};

// The backend reports what it did with the node's TLS certificate (F3/F4).
listen("admin-cert", (e) => status(`Admin panel: ${e.payload}`));

// F23: the native tab strip and the window's own focus state (Linux and
// desktop generally, respectively) each drive one half of whether the poll
// should run — see syncPolling() below. Both default true: the app always
// opens on Home (F16/F21), and the window is assumed focused the moment the
// script runs, which is the normal case; a wrong guess self-corrects on the
// next real event either way.
let onHomeTab = true;
let windowFocused = true;

// The native Home button switches in the backend; refresh what it reveals.
listen("tab", (e) => {
  onHomeTab = e.payload === "home";
  syncPolling();
});

listen("window-focus", (e) => {
  windowFocused = e.payload;
  syncPolling();
});

listen("session-lost", async (e) => {
  clearInterval(refreshTimer);
  refreshTimer = null;
  // Don't leave a dead panel on screen covering the launcher.
  await invoke("hide_admin_panel").catch(() => {});
  clearGuests();
  await showLauncher();
  status(`Session ended: ${e.payload}`);
});

let refreshTimer = null;

// F5: last fetch, templates already dropped. Typing in the search box filters
// this rather than hitting the API again (Q5.2).
let guests = [];

// Wipe the table and the cached list together (logout, session loss).
function clearGuests() {
  guests = [];
  busy.clear();
  rowsByKey.clear();
  $("guests").replaceChildren();
  $("search").value = "";
  // F26: the filter is a view of a list that no longer exists, so it goes
  // back to "any" with the list. The sort does not — it is remembered on
  // purpose (Q26.2) and survives a log out as it survives a restart.
  $("status-filter").value = "";
  $("count").textContent = "";
  $("main").hidden = true;
}

function show(section) {
  $("launcher").hidden = section !== "launcher";
  $("main").hidden = section !== "main";
}

// ---- F14 ----
//
// The tab bar is a native GTK strip above this webview (DECISIONS.md), so
// switching is driven from the backend. This page only keeps its own view in
// step: the guest list stays rendered whether or not it is on screen, because
// the webview is hidden outright while the admin panel is up and must be
// correct the moment it is shown again.

// ---- F16: launcher ----

// Status cell per connection id, so probe results can land as they arrive.
const statusCells = new Map();

function connectionRow(c) {
  const li = document.createElement("li");
  li.className = "connection";

  const label = document.createElement("div");
  const addr = document.createElement("strong");
  addr.textContent = `${c.host}:${c.port}`;
  const who = document.createElement("span");
  who.textContent = ` ${c.username}@${c.realm}`;
  label.append(addr, who);

  const state = document.createElement("div");
  state.className = "state";
  state.textContent = c.connected ? "Connected" : "Checking…";
  statusCells.set(c.name, state);

  const go = document.createElement("button");
  go.textContent = c.connected ? "Open" : "Connect";
  go.disabled = !c.saved_password;
  go.title = c.saved_password ? "" : "No password in the keyring for this entry";
  go.addEventListener("click", () => connect(c));

  // F19: deletion is a launcher job, and only for an entry that isn't the
  // live session — the backend refuses that too, this is just the label.
  const remove = document.createElement("button");
  remove.className = "danger";
  remove.textContent = "Remove";
  remove.disabled = c.connected;
  remove.title = c.connected
    ? "Log out before removing this connection"
    : "Delete this connection and its keyring password";
  remove.addEventListener("click", () => removeConnection(c));

  li.append(label, state, go, remove);
  if (c.last_used) li.classList.add("last-used");
  return li;
}

async function showLauncher() {
  show("launcher");
  statusCells.clear();
  let conns = [];
  try {
    conns = await invoke("list_connections");
  } catch (err) {
    status(`Could not read saved connections: ${err}`);
    return;
  }
  $("connections").replaceChildren(...conns.map(connectionRow));
  $("no-connections").hidden = conns.length > 0;
  $("add-connection").open = conns.length === 0;
  conns.forEach(probe);
}

// Q16.2: reachable (no credentials sent) plus, if we hold a ticket, whether
// the node still accepts it.
async function probe(c) {
  const cell = statusCells.get(c.name);
  if (!cell) return;
  try {
    const s = await invoke("probe_connection", { name: c.name });
    if (s.ticket_valid) cell.textContent = "Connected · session valid";
    else if (s.reachable) cell.textContent = c.connected ? "Session expired" : "Reachable";
    else cell.textContent = "Unreachable";
    cell.classList.toggle("ok", s.reachable);
    cell.classList.toggle("bad", !s.reachable);
  } catch (err) {
    cell.textContent = `Check failed: ${err}`;
  }
}

// Show a one-shot confirmation panel; resolves true if the user accepts.
// Shared by the certificate prompt (F3) and the remove prompt (F19). The
// listeners are removed with the answer, so reopening the panel doesn't
// accumulate them.
function askConfirm(panelId, okId, cancelId) {
  const panel = $(panelId);
  panel.hidden = false;
  return new Promise((resolve) => {
    const done = (answer) => {
      panel.hidden = true;
      $(okId).removeEventListener("click", yes);
      $(cancelId).removeEventListener("click", no);
      resolve(answer);
    };
    const yes = () => done(true);
    const no = () => done(false);
    $(okId).addEventListener("click", yes);
    $(cancelId).addEventListener("click", no);
  });
}

// F3 Q3.3: ask before trusting a certificate nothing vouches for. The
// password has not been sent at this point — the TLS handshake was refused
// before any request body went out.
function askTrust(fingerprint) {
  // Group into pairs so it can be compared against openssl output by eye.
  $("cert-fingerprint").textContent =
    fingerprint.toUpperCase().match(/../g).join(":");
  return askConfirm("cert-confirm", "cert-trust", "cert-cancel");
}

// F19: removing a connection deletes its keyring password as well, and
// nothing in the app can undo it, so it is confirmed first.
async function removeConnection(c) {
  $("remove-target").textContent =
    `${c.username}@${c.realm} on ${c.host}:${c.port}`;
  if (!(await askConfirm("remove-confirm", "remove-ok", "remove-cancel"))) {
    status("Cancelled — nothing was removed.");
    return;
  }
  try {
    await invoke("remove_connection", { name: c.name });
    status(`Removed ${c.username}@${c.realm} on ${c.host}, keyring password included.`);
  } catch (err) {
    status(`Remove failed: ${err}`);
  }
  // Either way the list is re-read, so a partial failure shows as it is.
  showLauncher();
}

async function connect(c, trust) {
  status(`Connecting to ${c.host}…`);
  try {
    const result = await invoke("connect", {
      name: c.name,
      trustFingerprint: trust ?? null,
    });
    if (result.status === "certificate_unverified") {
      status("");
      if (!(await askTrust(result.fingerprint))) {
        status("Cancelled — nothing was sent to the node.");
        return;
      }
      return connect(c, result.fingerprint);
    }
    status("");
    startSession();
  } catch (err) {
    status(`Connect failed: ${err}`);
  }
}

$("recheck").addEventListener("click", showLauncher);

async function addConnection(fields, trust) {
  const result = await invoke("add_connection", { fields, trustFingerprint: trust ?? null });
  if (result.status === "certificate_unverified") {
    status("");
    if (!(await askTrust(result.fingerprint))) {
      status("Cancelled — nothing was saved.");
      return false;
    }
    return addConnection(fields, result.fingerprint);
  }
  return true;
}

$("setup-form").addEventListener("submit", async (e) => {
  e.preventDefault();
  const f = new FormData(e.target);
  const fields = {
    host: f.get("host"),
    port: Number(f.get("port")),
    username: f.get("username"),
    realm: f.get("realm"),
    password: f.get("password"),
  };
  status("Connecting…");
  try {
    if (await addConnection(fields)) {
      e.target.reset();
      status("");
      startSession();
    }
  } catch (err) {
    status(`Login failed: ${err}`);
  }
});

// ---- session ----

function startSession() {
  // F21: Home is where you land, not Admin — Q16.3's auto-open is reversed.
  // Admin builds lazily on its own tab's first click, same as it always did.
  show("main");
  onHomeTab = true;
  syncPolling();
}

// F23: starts or stops the 30s poll to match whether anyone can see its
// result — a session is live AND Home is the visible tab AND the window is
// focused. Called from all three signals changing, so any one of them
// re-checks the whole thing; idempotent either way, since it only acts when
// the timer's current state doesn't already match what's wanted.
//
// The session-liveness check matters on its own: onHomeTab and windowFocused
// both default true, and "window-focus" is a global listener that fires
// before any session exists — found by tracing through the launcher-screen
// case, not by a failing test. Without it, focusing the window while still
// on the plain launcher would start the poll against no session at all,
// failing every 30s and cycling a status message for nothing. $("main")'s
// hidden state is the same signal startSession()/clearGuests() already use
// for "a session is live, showing Home's data" — not new state, just read.
function syncPolling() {
  if (!$("main").hidden && onHomeTab && windowFocused) {
    if (!refreshTimer) {
      refresh(); // Q23.2: catch up at once, matching the stale-tab scenario.
      refreshTimer = setInterval(refresh, 30_000); // Q5.1: 30s, as cv4pve uses
    }
  } else if (refreshTimer) {
    clearInterval(refreshTimer);
    refreshTimer = null;
  }
}

$("logout").addEventListener("click", async () => {
  clearInterval(refreshTimer);
  refreshTimer = null;
  try {
    await invoke("logout");
    clearGuests();
    status("Logged out. Your connection is still saved.");
    showLauncher();
  } catch (err) {
    status(`Log out failed: ${err}`);
  }
});

// ---- F5: guest list, with F6 power buttons per row ----

function fmtMem(bytes) {
  if (!bytes) return "–";
  return `${(bytes / 1024 ** 3).toFixed(1)} GiB`;
}

// F25: the detail row's numbers run from megabytes to terabytes, a range
// fmtMem has no scale for — it renders GiB to one decimal, which is right
// for a RAM column and useless for a 44 MB network total. fmtMem is left
// exactly as it is on purpose: F32 (#18) owns the RAM column's units and is
// still open, so changing it here would be doing that feature's work and
// pre-empting its decision.
function fmtBytes(bytes) {
  if (bytes == null) return "–";
  if (bytes === 0) return "0 B";
  const units = ["B", "KiB", "MiB", "GiB", "TiB"];
  let n = bytes;
  let i = 0;
  while (n >= 1024 && i < units.length - 1) {
    n /= 1024;
    i++;
  }
  return `${i > 0 && n < 10 ? n.toFixed(1) : Math.round(n)} ${units[i]}`;
}

// F25: uptime arrives in seconds. A stopped guest reports 0, which is not a
// duration, so it reads as unknown rather than "0m".
function fmtUptime(secs) {
  if (!secs) return "–";
  const days = Math.floor(secs / 86400);
  const hours = Math.floor((secs % 86400) / 3600);
  const mins = Math.floor((secs % 3600) / 60);
  if (days) return `${days}d ${hours}h`;
  if (hours) return `${hours}h ${mins}m`;
  return `${mins}m`;
}

// F25: a QEMU guest's `disk` is 0 because the host cannot see inside its
// filesystem — Proxmox reports zero rather than a figure. Printing "0 B"
// would be a wrong answer where this is a missing one, so it reads as
// unknown and maxdisk is still shown. A container reports real usage.
function diskText(g) {
  const used = g.kind === "qemu" && !g.disk ? "–" : fmtBytes(g.disk);
  return `${used} / ${fmtBytes(g.maxdisk)}`;
}

function netText(g) {
  return `↑ ${fmtBytes(g.netout)}   ↓ ${fmtBytes(g.netin)}`;
}

function kindLabel(kind) {
  return kind === "qemu" ? "VM" : "LXC";
}

// Q5.2/F22 Q22.2: match over name, VMID, node and tags. Built once per row,
// not once per keystroke — applyFilter() tests the query against this
// cached string rather than rebuilding it from the guest object each time.
function haystack(g) {
  return [
    g.name ?? "",
    String(g.vmid),
    g.node,
    g.tags ?? "",
    kindLabel(g.kind),
  ].join(" ").toLowerCase();
}

// ---- F26: sorting ----
//
// Mirrors `config::SORT_COLUMNS` in Rust, which is the real guard — a column
// this list does not hold is refused by `set_sort` before it can reach the
// config file. This copy only keeps the UI from offering one.
const SORT_COLUMNS = ["vmid", "name", "node", "status"];

let sortColumn = "vmid";
let sortDesc = false;

// Q26.1: the four columns whose order is stable. Ties break on vmid so the
// comparator is total — without that, two guests sharing a status could swap
// places between refreshes, which is exactly the row-moving-under-the-cursor
// the first scenario forbids. The tie-break is deliberately outside the
// direction flip: reversing it too would reintroduce the swap.
function compareGuests(a, b) {
  const dir = sortDesc ? -1 : 1;
  let r = 0;
  if (sortColumn === "name") {
    r = (a.name ?? "").localeCompare(b.name ?? "", undefined, {
      numeric: true,
      sensitivity: "base",
    });
  } else if (sortColumn === "node") {
    r = a.node.localeCompare(b.node);
  } else if (sortColumn === "status") {
    r = a.status.localeCompare(b.status);
  } else {
    r = a.vmid - b.vmid;
  }
  return r * dir || a.vmid - b.vmid;
}

// The arrow and aria-sort are set from the same state, so what a screen
// reader is told and what the header shows cannot disagree.
function markSortedHeader() {
  for (const th of document.querySelectorAll("th[data-sort]")) {
    const active = th.dataset.sort === sortColumn;
    th.setAttribute("aria-sort", active ? (sortDesc ? "descending" : "ascending") : "none");
    th.firstElementChild.dataset.dir = active ? (sortDesc ? "desc" : "asc") : "";
  }
}

function sortBy(column) {
  if (!SORT_COLUMNS.includes(column)) return;
  // Clicking the column already sorted reverses it; a different column
  // starts ascending.
  if (column === sortColumn) {
    sortDesc = !sortDesc;
  } else {
    sortColumn = column;
    sortDesc = false;
  }
  markSortedHeader();
  guests.sort(compareGuests);
  syncRows();
  // Q26.2: remembered across restarts. Deliberately not awaited — a config
  // file that cannot be written must not stop the list reordering now, and
  // there is nothing useful to tell the user about it.
  invoke("set_sort", { column: sortColumn, desc: sortDesc }).catch(() => {});
}

// Read the remembered sort once at startup. A missing or unreadable config
// is not worth a message: the default order still works.
async function loadSort() {
  try {
    const saved = await invoke("get_sort");
    if (saved && SORT_COLUMNS.includes(saved.column)) {
      sortColumn = saved.column;
      sortDesc = !!saved.desc;
    }
  } catch {
    // Keep the defaults.
  }
  markSortedHeader();
}

// ---- F6: power actions ----
//
// Q6.1: start, graceful shutdown, forced stop. The same three are the
// allow-list in api.rs — anything else is refused there, so this list is a
// label for the user, not the guard.
//
// Which buttons a row gets is decided by its status. A status that is neither
// running nor stopped (paused, suspended, a node that is down reporting
// "unknown") gets none: guessing wrong here means firing the wrong action at
// a real VM, and the admin panel handles those cases.
const POWER_BUTTONS = {
  running: [
    { action: "shutdown", label: "Shutdown", danger: false },
    { action: "stop", label: "Stop", danger: true },
  ],
  stopped: [{ action: "start", label: "Start", danger: false }],
};

// Guests with a request in flight, keyed node/vmid. Held outside the DOM —
// syncRows() reads it fresh for every patch, so a row mid-request stays
// disabled across a refresh without this living in the guest payload itself.
const busy = new Set();

const guestKey = (g) => `${g.node}/${g.vmid}`;
const guestLabel = (g) => (g.name ? `${g.name} (${g.vmid})` : `${g.vmid}`);

// F22: one entry per row, kept across refreshes so syncRows() can patch
// instead of rebuild. Keyed by vmid alone, not guestKey()'s node/vmid — a
// live-migrated guest keeps its vmid (cluster-unique in Proxmox), so this
// treats a migration as "the node cell changed" rather than delete+recreate.
// busy deliberately keeps its own node/vmid key; see DECISIONS.md.
const rowsByKey = new Map();

async function power(g, action, label) {
  // Q6.2: only the forced stop asks, because it is the one that can lose
  // data. Start and shutdown are reversible and fire on one click.
  if (action === "stop") {
    $("power-target").textContent = guestLabel(g);
    if (!(await askConfirm("power-confirm", "power-ok", "power-cancel"))) {
      status("Cancelled — the guest was left alone.");
      return;
    }
  }

  busy.add(guestKey(g));
  syncRows();
  status(`${label} ${guestLabel(g)}…`);
  try {
    await invoke("power_action", {
      node: g.node,
      kind: g.kind,
      vmid: g.vmid,
      action,
    });
    status(`${label} requested for ${guestLabel(g)}.`);
  } catch (err) {
    status(`${label} failed for ${guestLabel(g)}: ${err}`);
  } finally {
    busy.delete(guestKey(g));
    // Re-enable the row here rather than leaving it to refresh(), which
    // returns early if the fetch fails.
    syncRows();
  }
  // Proxmox answers with a task id and works on it afterwards, so the status
  // in /cluster/resources lags the request. Read it now for the error case,
  // and once more when the task has had time to finish.
  refresh();
  setTimeout(refresh, 3000);
}

// Fills an existing power-actions <td> with this guest's buttons. Shared by
// row creation and by patchRow(), which only calls this when status or busy
// state actually changed — the one case where listeners must be rebound.
function fillPowerCell(td, g) {
  td.replaceChildren();
  const working = busy.has(guestKey(g));
  // `hasOwn`, not a bare lookup: g.status comes from the server, and
  // "constructor" would otherwise resolve to something that isn't a list.
  const buttons = Object.hasOwn(POWER_BUTTONS, g.status)
    ? POWER_BUTTONS[g.status]
    : [];
  for (const b of buttons) {
    const button = document.createElement("button");
    button.textContent = b.label;
    if (b.danger) button.className = "danger";
    button.disabled = working;
    button.addEventListener("click", () => power(g, b.action, b.label));
    td.append(button);
  }
}

// The formatted strings diffed against on every patch, so a guest whose raw
// cpu/mem jitters but rounds to the same text doesn't touch the DOM either.
function cpuText(g) {
  return g.cpu != null ? `${(g.cpu * 100).toFixed(0)}%` : "–";
}
function memText(g) {
  return `${fmtMem(g.mem)} / ${fmtMem(g.maxmem)}`;
}

// One row, built once. Cell references are kept on the entry so later
// patches can write straight to them instead of re-creating anything.
// Server-supplied text (name, node, status, tags) goes in with textContent,
// never innerHTML, here and in patchRow() below — see the hard rules in
// CLAUDE.md.
function createRowEntry(g) {
  const tr = document.createElement("tr");
  const idCell = document.createElement("td");
  const nameCell = document.createElement("td");
  const typeCell = document.createElement("td");
  const nodeCell = document.createElement("td");
  const statusCell = document.createElement("td");
  const cpuCell = document.createElement("td");
  const memCell = document.createElement("td");
  const powerTd = document.createElement("td");
  powerTd.className = "actions";

  // F25 Q25.1: the disclosure lives in the ID cell. Every guest has a vmid,
  // the column is narrow and its content never changes, and it keeps clear
  // of the Name and Status cells that F34 (#20) and F31 (#17) will rework.
  // A button, not a click handler on the row: focusable, announced, and it
  // cannot swallow a click meant for a power button.
  const toggle = document.createElement("button");
  toggle.type = "button";
  toggle.className = "disclose";
  toggle.textContent = g.vmid;
  toggle.setAttribute("aria-expanded", "false");
  toggle.addEventListener("click", () => toggleDetail(g.vmid));
  idCell.append(toggle);
  nameCell.textContent = g.name ?? "";
  typeCell.textContent = kindLabel(g.kind);
  nodeCell.textContent = g.node;
  const known = g.status === "running" || g.status === "stopped";
  statusCell.textContent = g.status;
  if (known) statusCell.className = `st-${g.status}`;
  cpuCell.textContent = cpuText(g);
  memCell.textContent = memText(g);
  fillPowerCell(powerTd, g);

  tr.append(idCell, nameCell, typeCell, nodeCell, statusCell, cpuCell, memCell, powerTd);
  return {
    tr,
    nameCell,
    typeCell,
    nodeCell,
    statusCell,
    cpuCell,
    memCell,
    powerTd,
    // Cached only for diffing and the search haystack — never read by
    // anything that needs live server state.
    name: g.name ?? "",
    kind: g.kind,
    node: g.node,
    tags: g.tags ?? "",
    status: g.status,
    cpuText: cpuText(g),
    memText: memText(g),
    working: busy.has(guestKey(g)),
    haystack: haystack(g),
    // F25: the detail strip, built on first open and kept afterwards so
    // reopening costs nothing. `open` is its own flag rather than being read
    // off the DOM, because `hidden` on the row means something else — the
    // search filter uses it.
    toggle,
    open: false,
    detailTr: null,
    detailCells: null,
    detailText: null,
  };
}

// F25: one detail strip for a guest, spanning the table under its row.
function buildDetail(entry, g) {
  const tr = document.createElement("tr");
  tr.className = "detail";
  const td = document.createElement("td");
  td.colSpan = 8;
  const dl = document.createElement("dl");
  const cells = {};
  for (const [key, label] of [
    ["uptime", "Uptime"],
    ["disk", "Disk"],
    ["net", "Network"],
  ]) {
    const dt = document.createElement("dt");
    dt.textContent = label;
    const dd = document.createElement("dd");
    cells[key] = dd;
    dl.append(dt, dd);
  }
  td.append(dl);
  tr.append(td);
  entry.detailTr = tr;
  entry.detailCells = cells;
  entry.detailText = {};
  fillDetail(entry, g);
}

// Diffed the same way patchRow() diffs its cells (F22): an open detail whose
// values round to the same text writes nothing on a refresh.
function fillDetail(entry, g) {
  const values = {
    uptime: fmtUptime(g.uptime),
    disk: diskText(g),
    net: netText(g),
  };
  for (const key of Object.keys(values)) {
    if (entry.detailText[key] !== values[key]) {
      entry.detailCells[key].textContent = values[key];
      entry.detailText[key] = values[key];
    }
  }
}

// F25: open or close one guest's detail.
//
// Synchronous and unable to fail, because every value shown comes from the
// listing the table was already drawn from — Q25.2 and Q25.3 both went
// against a per-guest call, which is what makes opening a detail free.
// Placement is left to syncRows(), so row order has exactly one owner.
function toggleDetail(vmid) {
  const entry = rowsByKey.get(vmid);
  if (!entry) return;
  if (!entry.open) {
    const g = guests.find((x) => x.vmid === vmid);
    // Clicked between a refresh dropping the guest and the row going: there
    // is nothing to show, so leave it closed.
    if (!g) return;
    if (entry.detailTr) fillDetail(entry, g);
    else buildDetail(entry, g);
  }
  entry.open = !entry.open;
  entry.toggle.setAttribute("aria-expanded", String(entry.open));
  syncRows();
}

// Writes only the cells whose value actually differs from the last patch.
// An identical refresh — same data, same busy state — writes nothing at all
// and rebinds no listener, which is the first scenario in the issue.
function patchRow(entry, g) {
  // Diffed against the cache BEFORE any of it is overwritten below — compare
  // first, mutate after, or every comparison here would hold its own cached
  // value up against itself and never fire.
  const name = g.name ?? "";
  const tags = g.tags ?? "";
  const identityChanged = name !== entry.name || g.kind !== entry.kind ||
    g.node !== entry.node || tags !== entry.tags;

  if (name !== entry.name) {
    entry.nameCell.textContent = name;
    entry.name = name;
  }
  if (g.kind !== entry.kind) {
    entry.typeCell.textContent = kindLabel(g.kind);
    entry.kind = g.kind;
  }
  if (g.node !== entry.node) {
    entry.nodeCell.textContent = g.node;
    entry.node = g.node;
  }
  entry.tags = tags;
  if (identityChanged) entry.haystack = haystack(g);

  if (g.status !== entry.status) {
    entry.statusCell.textContent = g.status;
    const known = g.status === "running" || g.status === "stopped";
    entry.statusCell.className = known ? `st-${g.status}` : "";
  }

  const cpu = cpuText(g);
  if (cpu !== entry.cpuText) {
    entry.cpuCell.textContent = cpu;
    entry.cpuText = cpu;
  }
  const mem = memText(g);
  if (mem !== entry.memText) {
    entry.memCell.textContent = mem;
    entry.memText = mem;
  }

  const working = busy.has(guestKey(g));
  if (g.status !== entry.status || working !== entry.working) {
    fillPowerCell(entry.powerTd, g);
  }
  entry.status = g.status;
  entry.working = working;

  // F25: an open detail tracks the 30s refresh. A closed one is not touched,
  // so a detail nobody opened costs nothing per refresh.
  if (entry.open && entry.detailTr) fillDetail(entry, g);
}

// Toggles visibility against each row's precomputed haystack rather than
// filtering the guest list and rebuilding — the search-box scenario.
function applyFilter() {
  const query = $("search").value.trim().toLowerCase();
  // F26 Q26.3: composes with the search box rather than replacing it. The
  // cached status is the one patchRow() keeps current, so this needs no
  // lookup into `guests`.
  const wantStatus = $("status-filter").value;
  let shown = 0;
  for (const entry of rowsByKey.values()) {
    const match = (!query || entry.haystack.includes(query)) &&
      (!wantStatus || entry.status === wantStatus);
    if (entry.tr.hidden !== !match) entry.tr.hidden = !match;
    // F25: a detail is part of its guest's row, so it hides with it. Without
    // this it would sit under whichever row happened to follow.
    if (entry.detailTr && entry.detailTr.hidden !== !match) {
      entry.detailTr.hidden = !match;
    }
    if (match) shown++;
  }
  const narrowed = query || wantStatus;
  $("count").textContent = narrowed
    ? `${shown} of ${rowsByKey.size}`
    : `${rowsByKey.size}`;
}

// Patches existing rows, creates rows for new guests, drops rows for gone
// ones, and reorders only a row whose position actually moved. `guests` is
// already sorted by vmid (refresh()), so this walk is the intended order.
// Returns a note for the caller to show, or null. The note is handed back
// rather than written here because refresh() writes its own "Updated" line
// straight afterwards, which overwrites anything this function says.
function syncRows() {
  const tbody = $("guests");
  const seen = new Set();
  let note = null;
  let prevTr = null;
  for (const g of guests) {
    seen.add(g.vmid);
    let entry = rowsByKey.get(g.vmid);
    if (entry) {
      patchRow(entry, g);
    } else {
      entry = createRowEntry(g);
      rowsByKey.set(g.vmid, entry);
    }
    const wantAfter = prevTr ? prevTr.nextSibling : tbody.firstChild;
    if (wantAfter !== entry.tr) tbody.insertBefore(entry.tr, wantAfter);
    prevTr = entry.tr;
    // F25: an open detail is part of its guest's position — placed directly
    // after that row and then used as the anchor for the next guest. That is
    // what keeps it under its own row when F26's sort reorders the list, and
    // it is why placement lives here rather than in toggleDetail().
    if (entry.open && entry.detailTr) {
      const afterRow = prevTr.nextSibling;
      if (afterRow !== entry.detailTr) tbody.insertBefore(entry.detailTr, afterRow);
      prevTr = entry.detailTr;
    } else if (entry.detailTr?.parentNode) {
      entry.detailTr.remove();
    }
  }
  for (const [key, entry] of rowsByKey) {
    if (!seen.has(key)) {
      // F25: a detail open on a guest that has left the listing says so.
      // Vanishing without a word would read as the app losing the row.
      if (entry.open) {
        note = `${entry.name || key} is no longer listed — its detail closed.`;
      }
      entry.tr.remove();
      entry.detailTr?.remove();
      rowsByKey.delete(key);
    }
  }
  applyFilter();
  return note;
}

async function refresh() {
  try {
    const all = await invoke("list_guests");
    // Q5.3: templates are hidden outright, so drop them here rather than in
    // syncRows() — they shouldn't be reachable through the search box either.
    guests = all.filter((g) => g.template !== 1);
    // F26: was a fixed vmid order. syncRows() reorders rows from this array,
    // so the sort is the whole mechanism.
    guests.sort(compareGuests);
    // A note from syncRows() wins: it is about something that just happened
    // to a row, which matters more than the timestamp.
    const note = syncRows();
    status(note ?? `Updated ${new Date().toLocaleTimeString()}`);
  } catch (err) {
    status(`Refresh failed: ${err}`);
  }
}

$("search").addEventListener("input", applyFilter);
$("status-filter").addEventListener("change", applyFilter);
$("refresh").addEventListener("click", refresh);

// F26: each sortable header carries its column in data-sort; the button
// inside it is what takes the click.
for (const th of document.querySelectorAll("th[data-sort]")) {
  th.firstElementChild.addEventListener("click", () => sortBy(th.dataset.sort));
}


// ---- F16: always start on the launcher, never auto-connect ----
// F26: the remembered sort is read once here, before any list exists, so the
// first refresh is already in the right order.
loadSort();
showLauncher();
