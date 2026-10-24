# pve-desk

A Linux desktop client for a Proxmox VE node on a private network. It holds
saved logins, shows the guests on the node, and powers them on and off.

The terms below are the ones this project is opinionated about. Where several
words are in use for one concept, the heading is the one to write and the
`_Avoid_` line is what not to write.

## Language

### Connecting to a node

**Connection**:
A saved login to one node: its name, address, port, username, realm, and the
certificate fingerprint pinned on first connect. It is a stored record, not a
live link.
_Avoid_: saved login, entry, saved host

**Host**:
The address of a node — what you type into the Add a connection form. A Host
is one field of a Connection and never means the Connection itself.
_Avoid_: using "host" for the saved record

**Node**:
A Proxmox VE machine. For a cluster, the node a Connection points at is the
entry point: any member serves the whole guest list and proxies consoles.

**Default Connection**:
The Connection the launcher preselects at startup, because it was used most
recently. It only preselects: the app never connects on its own.
_Avoid_: active connection, current connection

**Session**:
The one live, authenticated link to a Node, held in memory and keyed by the
Connection it belongs to. A Connection outlives its Session; a Session is
lost when the app closes.

**Ticket**:
The Proxmox authentication credential a Session holds, sent as a cookie and
valid for about two hours. Never a unit of work — work items are **issues**,
identified by an F-number.
_Avoid_: token, cookie, work item

### Guests

**Guest**:
A virtual machine or a container on a Node. The union term; use it whenever
the distinction does not matter.
_Avoid_: VM/container, instance, machine

**VM**:
A Guest that is a virtual machine. Narrower than Guest — never use it for a
container or for the union.

**Container**:
A Guest that is a container. Narrower than Guest.
_Avoid_: LXC (except when naming the Proxmox value itself)

**Type**:
Whether a Guest is a VM or a container — `qemu` or `lxc`, the two values
Proxmox uses. The Rust field spells it `kind` only because `type` is a
reserved word; that is a spelling, not a second term.
_Avoid_: kind (in prose, in UI labels, and in issue titles)

**Template**:
A Guest marked as a template, which exists to be cloned and cannot be
started. A Template is a Guest; Home hides it from the list.

### Views

**Home**:
The local, native view of a Node: the place you land, that answers the
routine questions without a trip to Admin. The tab strip reads "Home" (F21).
_Avoid_: the guest list, the local tab, Guests (as the tab's name)

**Admin**:
The embedded Proxmox web interface, signed in and ready. The thing Home is
being built to replace.
_Avoid_: the panel, the web UI, the admin panel
