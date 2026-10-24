// Frontend for pve-desk. Plain JS, no build step.
// Every backend call goes through `invoke` (see src-tauri/src/commands.rs).

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const $ = (id) => document.getElementById(id);
const status = (msg) => { $("status").textContent = msg || ""; };

// The backend reports what it did with the node's TLS certificate (F3/F4).
listen("admin-cert", (e) => status(`Admin panel: ${e.payload}`));

// F2/Q2.2: the node stopped accepting the session and re-login failed too.
// Go back to the launcher rather than retrying every 30s against a dead one.
// The native Guests button switches in the backend; refresh what it reveals.
listen("tab", (e) => {
  if (e.payload === "guests") refresh();
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
  $("guests").replaceChildren();
  $("search").value = "";
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
  show("main");
  refresh();
  refreshTimer ??= setInterval(refresh, 30_000); // Q5.1: 30s, as cv4pve uses
  // Open on the admin panel: it is the view actually used (Q16.3).
  invoke("open_admin_panel").catch((err) => {
    status(`Admin panel failed to open: ${err}`);
  });
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

function kindLabel(kind) {
  return kind === "qemu" ? "VM" : "LXC";
}

// Q5.2: match over name, VMID, node and tags. Everything the server sent is
// lowercased once per row; the query is already lowercase.
function matches(g, query) {
  if (!query) return true;
  const haystack = [
    g.name ?? "",
    String(g.vmid),
    g.node,
    g.tags ?? "",
    kindLabel(g.kind),
  ].join(" ").toLowerCase();
  return haystack.includes(query);
}

// Server-supplied text (name, node, status, tags) goes in with textContent,
// never innerHTML — see the hard rules in CLAUDE.md.
function cell(tr, text, className) {
  const td = document.createElement("td");
  td.textContent = text;
  if (className) td.className = className;
  tr.append(td);
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

// Guests with a request in flight, keyed node/vmid. Held outside the DOM
// because render() rebuilds every row from scratch — including on the 30s
// refresh — which would otherwise re-enable a button mid-request and let a
// second POST through.
const busy = new Set();

const guestKey = (g) => `${g.node}/${g.vmid}`;
const guestLabel = (g) => (g.name ? `${g.name} (${g.vmid})` : `${g.vmid}`);

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
  render();
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
    render();
  }
  // Proxmox answers with a task id and works on it afterwards, so the status
  // in /cluster/resources lags the request. Read it now for the error case,
  // and once more when the task has had time to finish.
  refresh();
  setTimeout(refresh, 3000);
}

function powerCell(tr, g) {
  const td = document.createElement("td");
  td.className = "actions";
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
  tr.append(td);
}

function row(g) {
  const tr = document.createElement("tr");
  cell(tr, g.vmid);
  cell(tr, g.name ?? "");
  cell(tr, kindLabel(g.kind));
  cell(tr, g.node);
  // The class is derived from the status, so keep it to the values we style.
  const known = g.status === "running" || g.status === "stopped";
  cell(tr, g.status, known ? `st-${g.status}` : undefined);
  cell(tr, g.cpu != null ? `${(g.cpu * 100).toFixed(0)}%` : "–");
  cell(tr, `${fmtMem(g.mem)} / ${fmtMem(g.maxmem)}`);
  powerCell(tr, g);
  return tr;
}

function render() {
  const query = $("search").value.trim().toLowerCase();
  const shown = guests.filter((g) => matches(g, query));
  $("guests").replaceChildren(...shown.map(row));
  $("count").textContent = query
    ? `${shown.length} of ${guests.length}`
    : `${guests.length}`;
}

async function refresh() {
  try {
    const all = await invoke("list_guests");
    // Q5.3: templates are hidden outright, so drop them here rather than in
    // render() — they shouldn't be reachable through the search box either.
    guests = all.filter((g) => g.template !== 1);
    guests.sort((a, b) => a.vmid - b.vmid);
    render();
    status(`Updated ${new Date().toLocaleTimeString()}`);
  } catch (err) {
    status(`Refresh failed: ${err}`);
  }
}

$("search").addEventListener("input", render);
$("refresh").addEventListener("click", refresh);


// ---- F16: always start on the launcher, never auto-connect ----
showLauncher();
