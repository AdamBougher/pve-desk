#!/usr/bin/env node
// Drive pve-desk's frontend in WebKitGTK — the same engine the Tauri webview
// uses on Linux — with `window.__TAURI__` stubbed, so no Proxmox node, no
// keyring and no Rust build are needed.
//
//   node .claude/skills/run-pve-desk/driver.mjs smoke        # scripted flow + asserts
//   node .claude/skills/run-pve-desk/driver.mjs repl         # stdin REPL
//
// Needs a display plus, on Arch, two packages: webkitgtk-6.0 for
// /usr/bin/WebKitWebDriver (4.1 ships no driver binary) and webkit2gtk-4.1
// for the MiniBrowser this points at. MiniBrowser rejects --headless
// ("Cannot parse arguments: Unknown option --headless"), so a headless box
// would need xvfb-run around it — untested, there is no xvfb here.

import { spawn } from "node:child_process";
import { mkdtempSync, writeFileSync, readFileSync, cpSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { createInterface } from "node:readline";
import { createRequire } from "node:module";

const UI = resolve(import.meta.dirname, "../../../ui");
const SHOTS = resolve(import.meta.dirname, "shots");
const WINDOW = "pve-desk-driver";

// An ephemeral port, not a fixed one. A WebKitWebDriver whose session died of
// a page crash keeps running and keeps its port, so a hardcoded port means the
// next run fails to bind and reports "not ready" — which looks like a broken
// driver rather than a leftover one.
async function freePort() {
  const { createServer } = await import("node:net");
  return new Promise((ok, no) => {
    const srv = createServer();
    srv.once("error", no);
    srv.listen(0, "127.0.0.1", () => {
      const { port } = srv.address();
      srv.close(() => ok(port));
    });
  });
}
let PORT = Number(process.env.WKD_PORT || 0);

// ---------------------------------------------------------------- fixtures
// Shapes copied from the Rust types, not invented: Connection and
// ConnectionStatus in src-tauri/src/commands.rs, Guest in src-tauri/src/api.rs.
// ConnectOutcome is #[serde(tag="status", rename_all="snake_case")].
const FIXTURES = {
  list_connections: [
    {
      name: "home", host: "192.168.1.2", port: 8006, username: "root",
      realm: "pam", saved_password: true, connected: false, last_used: true,
    },
    {
      name: "spare", host: "192.168.1.3", port: 8006, username: "adam",
      realm: "pve", saved_password: true, connected: false, last_used: false,
    },
  ],
  probe_connection: { reachable: true, ticket_valid: false },
  connect: { status: "connected" },
  add_connection: { status: "connected" },
  remove_connection: null,
  power_action: null,
  logout: null,
  hide_admin_panel: null,
  open_admin_panel: null,
  open_spice: null,
  list_guests: [
    {
      vmid: 100, name: "truenas", kind: "qemu", node: "pve", status: "running",
      cpu: 0.04, maxcpu: 4, mem: 8589934592, maxmem: 17179869184,
      template: 0, tags: "storage;prod",
    },
    {
      vmid: 101, name: "docker", kind: "lxc", node: "pve", status: "stopped",
      cpu: 0, maxcpu: 2, mem: 0, maxmem: 2147483648, template: 0, tags: "",
    },
    // Neither running nor stopped: must get no power buttons at all.
    {
      vmid: 102, name: "paused-vm", kind: "qemu", node: "pve2",
      status: "paused", cpu: 0, maxcpu: 2, mem: 0, maxmem: 1073741824,
      template: 0, tags: "",
    },
    // A hostile name. The hard rule in CLAUDE.md says server text goes in
    // with textContent; if that ever regresses this row injects an <img>.
    {
      vmid: 103, name: '<img src=x onerror="window.__XSS=1">', kind: "lxc",
      node: "pve2", status: "stopped", cpu: 0, maxcpu: 1, mem: 0,
      maxmem: 536870912, template: 0, tags: "",
    },
  ],
};

const STUB = `
// Injected by driver.mjs. Stands in for the Tauri IPC bridge.
window.__CALLS = [];
window.__RESPONSES = ${JSON.stringify(FIXTURES)};
window.__FAIL = {};
window.__LISTENERS = {};
window.__TAURI__ = {
  core: {
    invoke(cmd, args) {
      window.__CALLS.push({ cmd, args });
      if (cmd in window.__FAIL) return Promise.reject(window.__FAIL[cmd]);
      const r = window.__RESPONSES[cmd];
      return Promise.resolve(r === undefined ? null : r);
    },
  },
  event: {
    listen(name, cb) {
      (window.__LISTENERS[name] = window.__LISTENERS[name] || []).push(cb);
      return Promise.resolve(() => {});
    },
  },
};
window.__emit = (name, payload) =>
  (window.__LISTENERS[name] || []).forEach((cb) => cb({ payload }));
`;

// ---------------------------------------------------------------- webdriver
let session = null;

async function wd(method, path, body) {
  const url = `http://127.0.0.1:${PORT}${path}`;
  const res = await fetch(url, {
    method,
    headers: { "Content-Type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const json = await res.json().catch(() => ({}));
  if (!res.ok) {
    const v = json.value || {};
    throw new Error(`${method} ${path} -> ${res.status} ${v.error || ""}: ${v.message || ""}`);
  }
  return json.value;
}

const s = (method, path, body) => wd(method, `/session/${session}${path}`, body);

async function waitForDriver() {
  for (let i = 0; i < 60; i++) {
    try {
      const v = await fetch(`http://127.0.0.1:${PORT}/status`).then((r) => r.json());
      if (v.value && v.value.ready) return;
    } catch {}
    await new Promise((r) => setTimeout(r, 250));
  }
  throw new Error(`WebKitWebDriver not ready on :${PORT}`);
}

// The stub has to be in place before main.js runs, so mirror ui/ into a temp
// dir and insert the stub's <script> ahead of main.js's. Nothing is written
// back into the repo.
function buildPage() {
  const dir = mkdtempSync(join(tmpdir(), "pve-desk-ui-"));
  cpSync(UI, dir, { recursive: true });
  writeFileSync(join(dir, "__stub.js"), STUB);
  const html = readFileSync(join(UI, "index.html"), "utf8");
  if (!html.includes('<script src="main.js">')) {
    throw new Error("ui/index.html no longer loads main.js the expected way");
  }
  writeFileSync(
    join(dir, "index.html"),
    html
      .replace('<script src="main.js">', '<script src="__stub.js"></script>\n  <script src="main.js">')
      // The real app titles its window "pve-desk" too, and screenshots are
      // taken by matching the window name — so rename this one.
      .replace("<title>pve-desk</title>", `<title>${WINDOW}</title>`),
  );
  return join(dir, "index.html");
}

// Module-level so the cleanup path can reach it even when start() throws
// partway through — an earlier version lost the handle that way and leaked a
// driver process holding the port.
let driverProc = null;
// MiniBrowser is a grandchild — WebKitWebDriver spawns it — so killing the
// driver does not always take it with it. A run killed by a timeout leaves
// one behind, and the orphan keeps an X window with the same title that
// screenshots then try (and fail) to capture. Snapshot the pids that existed
// before this run so stop() can kill only the ones this run created.
function browserPids() {
  const { execSync } = createRequire(import.meta.url)("node:child_process");
  try {
    return execSync("pgrep -f MiniBrowser", { encoding: "utf8", stdio: ["pipe", "pipe", "ignore"] })
      .trim().split(/\s+/).filter(Boolean);
  } catch {
    return [];
  }
}
let preexistingBrowsers = [];

async function start() {
  if (!PORT) PORT = await freePort();
  preexistingBrowsers = browserPids();
  // GDK_BACKEND=x11 is load-bearing, not tidying. On a Wayland session
  // MiniBrowser dies the moment it paints real content —
  //   Gdk-Message: Error 71 (Protocol error) dispatching to Wayland display
  // and WebDriver reports "session deleted because of page crash or hang".
  // about:blank survives, so the session looks healthy until the first real
  // navigation. XWayland is fine. Same trick DECISIONS.md uses to read
  // WM_CLASS off the app.
  const proc = spawn("WebKitWebDriver", [`--port=${PORT}`], {
    stdio: ["ignore", "pipe", "pipe"],
    detached: false,
    env: { ...process.env, GDK_BACKEND: "x11" },
  });
  driverProc = proc;
  let stderr = "";
  proc.stderr.on("data", (d) => { stderr += d; });
  proc.on("exit", (code) => {
    if (code) console.error(`WebKitWebDriver exited ${code}: ${stderr.trim()}`);
  });
  await waitForDriver();
  session = (
    await wd("POST", "/session", {
      capabilities: {
        alwaysMatch: {
          browserName: "MiniBrowser",
          "webkitgtk:browserOptions": {
            binary: "/usr/lib/webkit2gtk-4.1/MiniBrowser",
            args: ["--automation"],
          },
        },
      },
    })
  ).sessionId;
  await s("POST", "/url", { url: `file://${buildPage()}` });
  // main.js boots asynchronously (showLauncher awaits list_connections).
  await waitUntil(() => count("#connections li").then((n) => n > 0), "launcher rows");
}

async function stop() {
  try { if (session) await wd("DELETE", `/session/${session}`); } catch {}
  try { if (driverProc) driverProc.kill("SIGTERM"); } catch {}
  for (const pid of browserPids()) {
    if (!preexistingBrowsers.includes(pid)) {
      try { process.kill(Number(pid), "SIGTERM"); } catch {}
    }
  }
}

// ---------------------------------------------------------------- dom helpers
const exec = (script, args = []) => s("POST", "/execute/sync", { script, args });
const count = (sel) => exec(`return document.querySelectorAll(arguments[0]).length`, [sel]);
const text = (sel) =>
  exec(`var e=document.querySelector(arguments[0]);return e?e.textContent.trim():null`, [sel]);
const visible = (sel) =>
  exec(`var e=document.querySelector(arguments[0]);return !!e && !e.hidden && e.offsetParent!==null`, [sel]);

async function click(sel) {
  // element/click refuses anything it thinks is not interactable, and a few
  // of these live inside a just-unhidden section. A DOM click is what the
  // app's own handlers listen for and it never flakes on visibility.
  const ok = await exec(
    `var e=document.querySelector(arguments[0]); if(!e) return false; e.click(); return true`,
    [sel],
  );
  if (!ok) throw new Error(`no element for ${sel}`);
  await new Promise((r) => setTimeout(r, 120));
}

async function type(sel, value) {
  await exec(
    `var e=document.querySelector(arguments[0]); e.value=arguments[1];
     e.dispatchEvent(new Event("input",{bubbles:true})); return true`,
    [sel, value],
  );
  await new Promise((r) => setTimeout(r, 120));
}

async function waitUntil(fn, what, ms = 5000) {
  const until = Date.now() + ms;
  for (;;) {
    if (await fn()) return;
    if (Date.now() > until) throw new Error(`timed out waiting for ${what}`);
    await new Promise((r) => setTimeout(r, 100));
  }
}

// WebDriver's own GET /screenshot HANGS on this machine — it never answers
// and the run dies on whatever timeout wraps it. The driver log fills with
//   Failed to create GBM buffer of size 1024x732: Invalid argument
// and with WEBKIT_DMABUF_RENDERER_DISABLE_GBM=1 it becomes "Failed to import
// DMABuf" instead. WebKit cannot render into an offscreen buffer on this
// NVIDIA setup, which is the same family of trouble DECISIONS.md records for
// the app's own rendering. Reading the X pixmap sidesteps it completely:
// the window is on screen and X already has the pixels.
// MiniBrowser launched with --automation prefixes its window title, so the
// X name is "[Automation] pve-desk-driver", not "pve-desk-driver". `import
// -window <name>` does not match that and blocks until it is killed, which
// reads as a hang. Resolving the id out of _NET_CLIENT_LIST first avoids
// name-matching altogether.
function windowIds() {
  const { execSync } = createRequire(import.meta.url)("node:child_process");
  const list = execSync("xprop -root _NET_CLIENT_LIST", { encoding: "utf8" });
  const ids = (list.split("# ")[1] || "").trim().split(/,\s*/);
  const hits = [];
  for (const id of ids) {
    try {
      const nm = execSync(`xprop -id ${id} WM_NAME`, { encoding: "utf8", stdio: ["pipe", "pipe", "ignore"] });
      if (nm.includes(WINDOW)) hits.push(id);
    } catch {}
  }
  // Newest last: an orphaned MiniBrowser from a killed run keeps its window
  // listed and its title, and capturing that one fails. Try the newest first
  // and fall through the rest.
  return hits.reverse();
}

async function shot(name) {
  const { mkdirSync } = await import("node:fs");
  const { execFileSync } = await import("node:child_process");
  mkdirSync(SHOTS, { recursive: true });
  const file = join(SHOTS, `${name}.png`);
  try {
    const ids = windowIds();
    if (!ids.length) return `(no screenshot: no X window named ${WINDOW})`;
    let captured = false;
    for (const id of ids) {
      try {
        execFileSync("import", ["-window", id, file], { stdio: "pipe", timeout: 20000 });
        captured = true;
        break;
      } catch {}
    }
    if (!captured) return `(no screenshot: none of ${ids.length} window(s) could be captured)`;
    // A capture with one or two unique colours is a window that never
    // painted. That happens on this machine (see the header note), and a
    // blank PNG handed back without comment is worse than none — an agent
    // will read it as "the UI is empty" rather than "the GPU lost".
    // Count colours BELOW MiniBrowser's chrome. The toolbar and URL bar are
    // always colourful (519 unique colours in a measured blank capture), so
    // the whole image never looks empty even when the page painted nothing.
    const colours = Number(
      execFileSync(
        "magick",
        [file, "-crop", "100%x100%+0+80", "+repage", "-format", "%k", "info:"],
        { encoding: "utf8" },
      ).trim(),
    );
    if (colours <= 2) {
      return `${file} (BLANK — ${colours} colour(s); the window did not paint. ` +
        `The DOM checks above are still valid. For a real picture of the UI, ` +
        `screenshot the app itself: see app-shot.sh)`;
    }
    return file;
  } catch (e) {
    return `(no screenshot: ${String(e.message).split("\n")[0]})`;
  }
}

const calls = () => exec("return window.__CALLS");
const setResponse = (cmd, value) =>
  exec(`window.__RESPONSES[arguments[0]] = arguments[1]; return true`, [cmd, value]);
const emit = (name, payload) =>
  exec(`window.__emit(arguments[0], arguments[1]); return true`, [name, payload]);

// ---------------------------------------------------------------- smoke
let failures = 0;
function check(name, ok, detail = "") {
  console.log(`${ok ? "  ok  " : "  FAIL"} ${name}${ok ? "" : ` — ${detail}`}`);
  if (!ok) failures++;
}

async function smoke() {
  console.log("launcher");
  check("two connection rows", (await count("#connections li")) === 2);
  check("launcher visible", await visible("#launcher"));
  check("guest view hidden", !(await visible("#main")));
  await waitUntil(async () => (await text("#connections li")).includes("Reachable"), "probe");
  check("probe reported reachable", (await text("#connections li")).includes("Reachable"));

  console.log("certificate refusal");
  // api.rs:93 builds the fingerprint as bare lowercase hex with no
  // separators; askTrust() in main.js is what groups it into pairs for
  // comparing against openssl by eye. A pre-grouped fixture gets re-grouped
  // into nonsense, so the fixture has to be the wire format.
  const FP = "f8535c1d9e4a7b2c8f0e6d3a5b9c7e1f4a8d2b6c0e9f3a7d5b1c8e4f2a6d0b9c";
  const FP_SHOWN =
    "F8:53:5C:1D:9E:4A:7B:2C:8F:0E:6D:3A:5B:9C:7E:1F:4A:8D:2B:6C:0E:9F:3A:7D:5B:1C:8E:4F:2A:6D:0B:9C";
  await setResponse("connect", { status: "certificate_unverified", fingerprint: FP });
  await click("#connections li button");
  await waitUntil(() => visible("#cert-confirm"), "cert panel");
  const shown = await text("#cert-fingerprint");
  check("fingerprint shown grouped in pairs", shown === FP_SHOWN, shown);
  check(
    "password was not sent while the cert is unconfirmed",
    !JSON.stringify(await calls()).includes("password"),
  );
  await click("#cert-cancel");
  check("cancel says nothing was sent", (await text("#status")).includes("nothing was sent"));

  console.log("connect");
  await setResponse("connect", { status: "connected" });
  await click("#connections li button");
  await waitUntil(() => visible("#main"), "guest view");
  check("guest view shown", await visible("#main"));

  console.log("guest list");
  await waitUntil(async () => (await count("#guests tr")) > 0, "rows");
  const rows = await count("#guests tr");
  check("four guest rows", rows === 4, `got ${rows}`);
  check(
    "running guest has Shutdown and Stop",
    (await exec(
      `return [...document.querySelectorAll("#guests tr")]
         .find(r=>r.textContent.includes("truenas"))
         .querySelectorAll("button").length`,
    )) === 2,
  );
  check(
    "stopped guest has Start only",
    (await exec(
      `var r=[...document.querySelectorAll("#guests tr")].find(r=>r.textContent.includes("docker"));
       var b=r.querySelectorAll("button");
       return b.length===1 && b[0].textContent.trim()==="Start"`,
    )),
  );
  check(
    "paused guest has no power buttons",
    (await exec(
      `return [...document.querySelectorAll("#guests tr")]
         .find(r=>r.textContent.includes("paused-vm"))
         .querySelectorAll("button").length`,
    )) === 0,
  );

  console.log("hard rule: server text is not HTML");
  check("no injected element", (await exec(`return !!window.__XSS`)) === false);
  check(
    "hostile name is literal text",
    await exec(
      `return [...document.querySelectorAll("#guests tr")]
         .some(r=>r.textContent.includes("onerror"))`,
    ),
  );
  check("no img smuggled into the table", (await count("#guests img")) === 0);

  console.log("search");
  await type("#search", "docker");
  check("filtered to one row", (await count("#guests tr")) === 1);
  await type("#search", "");
  check("filter cleared", (await count("#guests tr")) === 4);

  console.log("force stop asks first");
  // F21: status() now also invokes set_status (mirrored to the native GTK
  // label), so a raw call-count comparison no longer holds across a status
  // message — check the actual invariant instead: no power_action sent.
  const noPowerAction = async () => !(await calls()).some((c) => c.cmd === "power_action");
  await click(
    `#guests tr:nth-child(1) button:nth-of-type(2)`,
  );
  await waitUntil(() => visible("#power-confirm"), "confirm panel");
  check("confirm names the guest", (await text("#power-target")) === "truenas (100)");
  check("nothing sent yet", await noPowerAction());
  await click("#power-cancel");
  check("cancel left it alone", (await text("#status")).includes("left alone"));
  check("still nothing sent", await noPowerAction());

  console.log("force stop confirmed");
  await click(`#guests tr:nth-child(1) button:nth-of-type(2)`);
  await waitUntil(() => visible("#power-confirm"), "confirm panel");
  await click("#power-ok");
  await waitUntil(async () => (await calls()).some((c) => c.cmd === "power_action"), "power_action");
  const pa = (await calls()).filter((c) => c.cmd === "power_action").pop();
  check(
    "power_action args are the four named fields",
    JSON.stringify(pa.args) === JSON.stringify({ node: "pve", kind: "qemu", vmid: 100, action: "stop" }),
    JSON.stringify(pa.args),
  );
  check(
    "frontend sent no API path",
    !JSON.stringify(pa.args).includes("/api2/"),
  );

  console.log("backend errors surface");
  await exec(`window.__FAIL.list_guests = "node said 596 no route to host"; return true`);
  await click("#refresh");
  await waitUntil(async () => (await text("#status")).includes("596"), "error in status");
  check("fetch error shown to the user", (await text("#status")).includes("596"));
  await exec(`delete window.__FAIL.list_guests; return true`);

  console.log("session-lost returns to the launcher");
  await emit("session-lost", "ticket rejected and re-login failed");
  await waitUntil(() => visible("#launcher"), "launcher back");
  check("back at launcher", await visible("#launcher"));
  check("guest view hidden again", !(await visible("#main")));
  check("reason shown", (await text("#status")).includes("ticket rejected"));

  const file = await shot("smoke");
  console.log(`\nscreenshot: ${file}`);
  return failures;
}

// ---------------------------------------------------------------- repl
async function repl() {
  const help = `commands:
  click <css>          DOM-click a selector
  type <css> <value>   set a value and fire input
  text <css>           textContent of the first match
  count <css>          how many match
  vis <css>            is it visible
  eval <js>            run JS in the page, print the result
  resp <cmd> <json>    override a stubbed invoke response
  fail <cmd> <msg>     make an invoke reject
  emit <name> <payload>  fire a Tauri event (tab, session-lost, admin-cert)
  calls                print every invoke the frontend made
  ss <name>            screenshot to shots/<name>.png
  quit`;
  console.log(help);
  const rl = createInterface({ input: process.stdin, terminal: false });
  for await (const line of rl) {
    const [cmd, ...rest] = line.trim().split(/\s+/);
    const arg = rest.join(" ");
    try {
      if (!cmd) continue;
      else if (cmd === "quit") break;
      else if (cmd === "help") console.log(help);
      else if (cmd === "click") { await click(arg); console.log("ok"); }
      else if (cmd === "type") { await type(rest[0], rest.slice(1).join(" ")); console.log("ok"); }
      else if (cmd === "text") console.log(await text(arg));
      else if (cmd === "count") console.log(await count(arg));
      else if (cmd === "vis") console.log(await visible(arg));
      else if (cmd === "eval") console.log(JSON.stringify(await exec(`return (${arg})`)));
      else if (cmd === "resp") { await setResponse(rest[0], JSON.parse(rest.slice(1).join(" "))); console.log("ok"); }
      else if (cmd === "fail") { await exec(`window.__FAIL[arguments[0]]=arguments[1];return true`, [rest[0], rest.slice(1).join(" ")]); console.log("ok"); }
      else if (cmd === "emit") { await emit(rest[0], rest.slice(1).join(" ")); console.log("ok"); }
      else if (cmd === "calls") console.log(JSON.stringify(await calls(), null, 1));
      else if (cmd === "ss") console.log(await shot(arg || "repl"));
      else console.log(`? ${cmd}`);
    } catch (e) {
      console.log(`error: ${e.message}`);
    }
  }
}

// ---------------------------------------------------------------- main
const mode = process.argv[2] || "smoke";
try {
  await start();
  if (mode === "smoke") {
    const n = await smoke();
    console.log(n === 0 ? "\nall checks passed" : `\n${n} check(s) FAILED`);
    await stop();
    process.exit(n === 0 ? 0 : 1);
  } else if (mode === "repl") {
    await repl();
    await stop();
  } else {
    console.error(`unknown mode: ${mode} (smoke|repl)`);
    await stop();
    process.exit(2);
  }
} catch (e) {
  console.error(`driver failed: ${e.message}`);
  await stop();
  process.exit(3);
}
