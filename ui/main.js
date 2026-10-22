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
listen("session-lost", async (e) => {
  clearInterval(refreshTimer);
  refreshTimer = null;
  $("guests").replaceChildren();
  await showLauncher();
  status(`Session ended: ${e.payload}`);
});

let refreshTimer = null;

function show(section) {
  $("launcher").hidden = section !== "launcher";
  $("main").hidden = section !== "main";
}

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
  const result = await invoke("add_connection", { ...fields, trustFingerprint: trust ?? null });
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
  refreshTimer ??= setInterval(refresh, 30_000); // Q5.1: interval TBD
  invoke("open_admin_panel").catch((err) => {
    status(`Admin panel failed to open: ${err}`);
  });
}

$("logout").addEventListener("click", async () => {
  clearInterval(refreshTimer);
  refreshTimer = null;
  try {
    await invoke("logout");
    $("guests").replaceChildren();
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

async function refresh() {
  try {
    const guests = await invoke("list_guests");
    guests.sort((a, b) => a.vmid - b.vmid);
    const rows = guests.map((g) => {
      const tr = document.createElement("tr");
      const cpu = g.cpu != null ? `${(g.cpu * 100).toFixed(0)}%` : "–";
      tr.innerHTML = `
        <td>${g.vmid}</td>
        <td></td>
        <td>${g.kind === "qemu" ? "VM" : "LXC"}</td>
        <td></td>
        <td class="st-${g.status}">${g.status}</td>
        <td>${cpu}</td>
        <td>${fmtMem(g.mem)} / ${fmtMem(g.maxmem)}</td>`;
      // Names/nodes come from the server: set as text, not HTML.
      tr.children[1].textContent = g.name ?? "";
      tr.children[3].textContent = g.node;
      return tr;
    });
    $("guests").replaceChildren(...rows);
    status(`Updated ${new Date().toLocaleTimeString()}`);
  } catch (err) {
    status(`Refresh failed: ${err}`);
  }
}

$("refresh").addEventListener("click", refresh);
$("admin").addEventListener("click", async () => {
  try { await invoke("open_admin_panel"); }
  catch (err) { status(`${err}`); }
});

// ---- F16: always start on the launcher, never auto-connect ----
showLauncher();
