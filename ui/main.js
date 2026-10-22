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

  li.append(label, state, go);
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

// F3 Q3.3: ask before trusting a certificate nothing vouches for. Resolves
// true if the user accepts. The password has not been sent at this point —
// the TLS handshake was refused before any request body went out.
function askTrust(fingerprint) {
  const panel = $("cert-confirm");
  // Group into pairs so it can be compared against openssl output by eye.
  $("cert-fingerprint").textContent =
    fingerprint.toUpperCase().match(/../g).join(":");
  panel.hidden = false;
  return new Promise((resolve) => {
    const done = (answer) => {
      panel.hidden = true;
      $("cert-trust").removeEventListener("click", yes);
      $("cert-cancel").removeEventListener("click", no);
      resolve(answer);
    };
    const yes = () => done(true);
    const no = () => done(false);
    $("cert-trust").addEventListener("click", yes);
    $("cert-cancel").addEventListener("click", no);
  });
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
    status("Logged out. Your saved login is still stored.");
    showLauncher();
  } catch (err) {
    status(`Log out failed: ${err}`);
  }
});

// ---- F5: guest list (read-only; power controls are the admin panel's job) ----

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
