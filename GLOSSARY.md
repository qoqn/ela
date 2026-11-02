# Ela

Ela is a desktop remote connection manager: it stores how to reach machines and opens live sessions to them over SSH, RDP, VNC and other protocols, in one tabbed window.

## Language

### Saved things

**Connection**:
A saved description of how to reach one remote endpoint: host, port, protocol, credential reference and protocol options. It is configuration, never a live link.
_Avoid_: Host, server, bookmark, profile, session (for the saved item)

**Folder**:
A named container in the connection tree that groups Connections and other Folders and can supply inherited defaults to everything beneath it.
_Avoid_: Group, directory, category

**Tag**:
A free-form label attached to Connections for cross-cutting filtering, independent of Folder placement.
_Avoid_: Label, category

**Inheritance**:
The rule by which a Connection takes a field's value from the nearest ancestor Folder that sets it, unless the Connection has an Override.
_Avoid_: Cascading, defaults

**Override**:
A field value set directly on a Connection or Folder that stops Inheritance for that field.

**Environment**:
The kind of place a Connection belongs to (development, staging, production, lab), shown as a colour on every mark of its identity. Set on a Folder and inherited, or set on a Connection.
_Avoid_: Stage, tier, colour tag

**Production flag**:
A marker on an Environment that turns on Ela's guardrails (hatched rail, Broadcast confirmation, multi-line paste preview). It is independent of the Environment's colour.

**Template**:
A reusable set of Connection fields used to create new Connections; unlike a Folder it has no ongoing link to the Connections made from it.
_Avoid_: Preset

### Secrets

**Vault**:
The store that holds Secrets, locked by default and unlocked by the user for a limited time.
_Avoid_: Keychain, keyring, password store (those are Secret Store backends)

**Secret**:
A password, private key, passphrase or token used to authenticate.
_Avoid_: Credential (when meaning the raw value)

**Credential Reference**:
An opaque identifier pointing to a Secret in the Vault. Connections and the user interface only ever hold Credential References, never Secrets.
_Avoid_: Credential ID, password field

**Secret Store**:
A backend that physically keeps Secrets: the built-in encrypted file, the OS keyring, KeePassXC, an external password manager.

**Known Host**:
A remote endpoint's identity (SSH host key or TLS certificate fingerprint) that the user has accepted and that future sessions are checked against.
_Avoid_: Fingerprint cache, trusted host

### Live things

**Session**:
One live, running link to a remote endpoint, opened from a Connection or from Quick Connect. A Connection can have zero, one or many Sessions at once.
_Avoid_: Connection (for the live link), tab, terminal

**Tab**:
The place in the window where one Session is shown. A Tab can be split into Panes or moved to its own window; it is presentation, not the Session itself.
_Avoid_: Session

**Pane**:
One region of a split Tab, showing exactly one Session.

**Quick Connect**:
Opening a Session from a typed address (`user@host`, `rdp://host`) without saving a Connection first.
_Avoid_: Ad-hoc connection

**Broadcast**:
A mode in which keyboard input typed into one Session is sent to a chosen set of other Sessions at the same time.
_Avoid_: Multi-exec, cluster mode, sync input

**Tunnel**:
A port forward (local, remote or dynamic) carried over an SSH Session, managed independently of any terminal.
_Avoid_: Port forward (acceptable in UI copy, but Tunnel is the model term)

### Extension points

**Protocol**:
A pluggable kind of remote access (SSH, RDP, VNC, SPICE…) that knows how to turn a Connection into a Session and which options it accepts.

**Renderer**:
The kind of view the user interface uses to display a Session: Terminal, Framebuffer, File Browser, Web or External. Many Protocols share one Renderer.
_Avoid_: Viewer, display

**Transport**:
The network path a Session travels over before reaching its Protocol: direct TCP, a Jump Host chain, a SOCKS proxy, a relay, a cloud bastion.
_Avoid_: Proxy (too narrow)

**Jump Host**:
An intermediate SSH endpoint a Transport passes through to reach the target.
_Avoid_: Bastion (reserve for cloud provider bastion services), proxy

**Launcher**:
A Protocol implemented by starting an external program (FreeRDP, remote-viewer, mosh) rather than by Ela itself.
_Avoid_: External client

**Importer**:
A component that reads another tool's saved data (`~/.ssh/config`, Remmina, MobaXterm, mRemoteNG) and produces Connections and Folders.

**Tool Panel**:
A side panel that augments the active Session with extra views, such as files, host metrics or Tunnels.
_Avoid_: Sidebar widget
