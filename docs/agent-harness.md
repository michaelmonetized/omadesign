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

## Prompt attachments

Paste into the focused prompt with Ctrl+V or Shift+Insert. Short text stays in the
brief. Text over 2,000 characters or 40 lines becomes a file attachment. Images,
SVG markup, copied Omadesign objects, file-manager copies (including PDF, video,
audio and arbitrary files), and other offered clipboard MIME types become chips
with matching `[📎 name · id]` references at the cursor. Drop files over the agent
panel to attach them. Paste outside the prompt retains the canvas behavior.

The chip's × removes its inline reference. Backspace or Delete at or inside a
reference removes the whole reference and attachment. Select and delete a reference
to remove it as well. The prompt can remain editable while a turn is running;
attachment reads and encoding use workers and show an “Attaching…” indicator.

Provider delivery follows the agent's advertised ACP `promptCapabilities`:

| Capability | Delivery |
| --- | --- |
| `image` | PNG image block, reduced to a maximum 2048-pixel edge; copied objects also include a rendered preview when possible |
| `audio` | Audio block for audio attachments within the inline limit |
| `embeddedContext` | Text/SVG or small file resource, with UTF-8 text or base64 blob |
| Unsupported media, video, or an inline limit reached | File resource link plus readable path, MIME, size, image dimensions when known, and an explicit delivery note |

The harness instruction remains the first text block. Inline references remain in
the brief and each attachment includes a text line resolving its reference. The
chip tooltip records its delivery method. A provider must independently have file
access to inspect a path fallback; the host does not claim to have delivered visual
content to a provider that lacks image support.

New clipboard data lives in `.omadesign/agent-attachments/<thread-id>/` under the
conversation's project folder. Copied existing files retain their absolute paths;
image previews are prepared separately. User transcript entries persist attachment
metadata, while bytes stay in files. Transcript and History show saved chips, and
missing source files are marked unavailable. Older conversations remain readable.

Limits are 20 attachments per prompt, 100 MiB per file, 20 MiB per encoded image,
5 MiB per inline audio/resource, and 25 MiB of inline base64 per turn. Larger
permitted files use paths. Clipboard reads retain the hard 256 MiB byte, 16 MiB
ordinary text, and 64-megapixel image caps. Oversized/refused data shows an inline
error. The 32 KiB brief cap counts typed text, excluding attachment tokens and
attachment contents.
