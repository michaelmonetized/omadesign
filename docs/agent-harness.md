# Live design agents

Omadesign is an ACP client with its own native conversation panel and design-tool
host. It connects directly to local agent processes; it does not delegate its UI
to the Omarchy terminal launcher. The editor remains visible while native objects
arrive, and users can keep editing between agent operations.

## Connections

The Connection panel offers these editable presets:

| Agent | Executable and arguments |
| --- | --- |
| Codex | `npx --yes @agentclientprotocol/codex-acp@1.13.1` |
| Claude | `npx --yes @agentclientprotocol/claude-agent-acp@0.81.2` |
| Gemini | `gemini --acp` |
| OpenCode | `opencode acp` |
| Custom | Any installed ACP stdio executable and JSON argument array |

Commands are launched as argument arrays, never interpolated through a shell.
The project folder is explicit and must exist. The adapters use their agents'
existing authentication. ACP authentication methods, session configuration,
models, and modes are surfaced when advertised by the agent. Credentials are not
collected or stored by Omadesign. Adapter preset versions are pinned; changing an
adapter or model does not change the host's native tool contract.

## Working on the canvas

Send a brief in Create mode. The agent receives instructions to build with native
tools in visible stages and inspect its result. Agents can read context, see the
selected objects, and use these tools:

| Tool | Result |
| --- | --- |
| `get_document` | Canvas dimensions, layer and object summaries, selection, revision |
| `get_objects` | Paginated native geometry and styling |
| `add_shape` | Editable rectangle, ellipse, line, path, frame, or text |
| `update_shape` | Position, size, typography, color, gradient, stroke, rotation, opacity |
| `remove_shapes` | Undoable removal of validated objects and frame descendants |
| `create_layer`, `update_layer` | Native vector layer organization |
| `set_effects` | Native blur and drop shadow |
| `select_objects` | Highlight objects in the editor |
| `list_fonts` | Installed font names for editable text |
| `get_canvas_snapshot` | Rendered PNG for visual inspection, at most 960 pixels per side |
| `get_documentation` | Version-matched manual, Layout guide, or tool schemas |

The initial native tool set focuses on vector compositions, typography, and
frames. It does not expose arbitrary code execution or file replacement as an
editor tool. The standalone CLI creation skill remains a separate offline path.

Writes require the exact current canvas revision. A stale request returns a
recoverable error asking the agent to inspect the changed canvas. Locked or hidden
objects, guides, and locked parents cannot be edited. Every validated mutation
uses the editor's existing command history, dirty tracking, and renderer. At most
one queued tool call is processed per UI frame, so successive edits can paint
between calls. Tools reject edits during an active manual gesture.

Learn sessions are read-only. Create sessions also become read-only when **Allow
live canvas edits** is disabled. ACP permission requests are displayed with the
agent's concrete options; Omadesign does not automatically approve unrelated
terminal or filesystem requests. The ACP client advertises no terminal or file
read/write delegation capabilities.

## Cancellation and recovery

Stop immediately closes the gate for queued native calls, sends `session/cancel`,
and resolves pending permission requests as cancelled. An unresponsive adapter is
terminated after five seconds. Closing the application terminates the private
adapter process group and its helper processes. Completed changes remain undoable.

Connections are bound to the active document's identity. Switching or replacing
the document disconnects the agent before further native calls can apply. Saved
threads can reconnect only to their saved document; unsupported session loading
is reported as a new agent session, with the local conversation still readable.

Settings and transcripts live under
`$XDG_DATA_HOME/omadesign/agent` (normally
`~/.local/share/omadesign/agent`). Writes are atomic, ordered on a background
writer, and flushed on shutdown. Directories are private (0700) and files are
0600. History shows the 50 most recent saved threads; each transcript retains up
to 400 entries and 2 MiB of message text. Provider thought chunks are used only
as a transient status, not saved as conversation text.

## Protocol and implementation

ACP v1 initialization negotiates the connection before `session/new` or
`session/load`. The session supplies Omadesign's stdio MCP server, following the
[ACP session setup specification](https://agentclientprotocol.com/protocol/v1/session-setup).
Streaming messages, tool progress, configuration, plans, permissions, and stop
reasons follow the [ACP prompt lifecycle](https://agentclientprotocol.com/protocol/v1/prompt-turn)
and [permission contract](https://agentclientprotocol.com/protocol/v1/tool-calls).

The MCP helper is the same application binary invoked with `--agent-mcp`. It
connects to a private Unix socket with a random per-connection token. No public
network listener is opened. Requests enter a bounded queue and are checked again
on the UI thread against cancellation, document identity, revision, and editing
permissions before mutation. Message sizes and connection counts are bounded.
The tool surface follows [MCP tools](https://modelcontextprotocol.io/specification/2025-06-18/server/tools).

Source: `src/agent/{runtime,bridge,tools,workspace,config}.rs` and
`src/ui/agent.rs`. `src/bin/agent_qa.rs` drives an isolated native WGPU test using
a real local ACP adapter, and can reconnect with `--resume-from OUTPUT_DIRECTORY`.
