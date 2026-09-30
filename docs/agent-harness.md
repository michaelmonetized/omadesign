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

Send a brief in Create mode. Native tools are available in all five workspaces:
Design, Pixel, Photo, Layout and Motion. The current workspace changes the UI,
not the agent's capabilities. `get_document` reports mode, project directory,
revision, vector objects, raster layer transforms, masks and persistent IDs.
`get_editor_capabilities` reports native operation names and property defaults.

| Area | Tools |
| --- | --- |
| Disk assets | `list_files`, `read_file`, `import_file`, `set_image_fill` |
| Design | `get_objects`, `add_shape`, `update_shape`, `patch_object`, `remove_shapes`, `select_objects`, `list_fonts` |
| Layers/effects | `create_layer` (vector/raster/group), `update_layer` (including group parent), `transform_raster`, `set_effects`, `set_filter_stack` |
| Native commands | `editor_action` (undo/redo, duplicate/delete, ordering, paths, booleans, components, text paths, tracing) |
| Pixel | `set_pixel_selection`, `paint_stroke`, `set_mask` |
| Photo | `get_photos`, `select_photo`, `develop_photo` |
| Layout | `set_layout`, frame/image placement and native component actions |
| Motion | `get_motion`, `set_motion`, `set_keyframes`, `apply_motion_preset` |
| Workspace/output | `configure_canvas`, `set_mode`, `get_canvas_snapshot`, `save_document`, `get_documentation` |

User-requested disk assets can come from outside the project directory. Tool paths
are absolute or start with `~/`. Directory results are paginated; text and image
previews are bounded. Import decoding runs on the MCP worker, then the UI checks
revision, session, cancellation and live-edit permission again before placement.
The native desktop codecs preserve supported editable geometry/layers and return
conversion notes. A failed import does not change the canvas. Frame placement and
image fills retain native Layout structure. Photo import retains the original
and nondestructive adjustments. Source assets are not overwritten by placement.

Attachments are references until `import_file` or `set_image_fill` places them.
The harness permits file inspection/preparation through available provider tools;
it does not prohibit disk assets or require a vector approximation of a photo.
Pixel selections and masks support photographic cutouts while preserving pixels.
Raster transforms, compatible motion presets and keys use persistent layer IDs;
selection references use `{layer, id: 0}`. Draw stroke requires a vector outline.

Writes require the exact current canvas revision. Photo adjustment writes also
require `photo_revision`. A stale request asks the agent to inspect the changed
state. Hidden/locked artwork is protected; explicit layer metadata tools can
show/unlock a layer when the user's request calls for it. Every artwork edit uses
native command history, dirty tracking and rendering. Tools reject writes during
manual gestures. Snapshot supports Photo and explicit motion times.

Learn sessions and Create sessions with **Allow live canvas edits** disabled
remain read-only. ACP permission requests still show the agent's concrete options.
The ACP client delegates `fs/read_text_file` and `fs/write_text_file` during the
matching active session, with 1-based line ranges and bounded UTF-8 content.
Writes also require the live-edit gate. ACP terminal delegation is not advertised;
providers may use their own terminal tools. Native import never needs a terminal.
Local output is explicit through `save_document`, with an overwrite flag for
existing paths and native codec warnings for unsupported export features.

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
to 400 entries and 2 MiB of message text. The background writer also prunes older
entries to keep the complete serialized transcript, including attachment metadata,
below History's 4 MiB file limit. Provider thought chunks are used only
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
chip tooltip records its delivery method. A provider can inspect path fallbacks with `read_file` or ACP text-file
delegation and place them with `import_file`. The host does not claim to have
delivered visual content to a provider that lacks image support.

New clipboard data lives in `.omadesign/agent-attachments/<thread-id>/` under the
conversation's project folder. Copied existing files retain their absolute paths;
image previews are prepared separately. User transcript entries persist attachment
metadata, while bytes stay in files. Transcript and History show saved chips, and
missing source files are marked unavailable. Older conversations remain readable.

Limits are 20 attachments per prompt, 100 MiB per file, 20 MiB per encoded image,
5 MiB per inline audio/resource, and 25 MiB of inline base64 per turn. Larger
permitted files use paths. If an attached file cannot be decoded for a preview,
the original file remains attached by reference and valid sibling files are kept.
Clipboard reads retain the hard 256 MiB byte, 16 MiB
ordinary text, and 64-megapixel image caps. Oversized/refused data shows an inline
error. The 32 KiB brief cap counts typed text, excluding attachment tokens and
attachment contents.
