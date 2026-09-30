# Agent panel and file workflows

Omadesign 0.6.1 includes a built-in Agent harness: an in-app Agent panel with
Create / Learn modes, History threads, attachments, and native canvas tools over
ACP. `agent-panel.{mp4,webm,webp,vtt}` is a native screen recording excerpt from
the 2026-09-29 promo pickups (`marquee-agent-061.mp4`, seconds 55–87).

The campaign kit below remains a separate headless CLI path. Those examples were
authored by an AI coding agent on 2026-09-20, then imported, inspected and
rendered with Omadesign 0.5.4.

## Create and inspect

Ask your agent to create SVG artwork using supported text and shape primitives. Keep the SVG source alongside the editable native document.

```sh
omadesign --convert poster.svg --output poster.oma
omadesign --inspect poster.oma
omadesign --convert poster.oma --output poster.png
```

Inspect emits JSON with dimensions, layer information and import notes. Review both the notes and rendered image: unsupported SVG constructs may not remain editable or match exactly. These examples use Nimbus Sans; install the font for matching typography.

## Revise graphics

Edit a copy of the SVG: change colors, text or geometry, then convert into a new .oma and PNG. The violet variants demonstrate this process. Native `.oma` documents use versioned structured JSON, including packed payloads for raster data. For most authored vector work, the safest path is to create SVG and import it with the CLI. For native frames or animation, follow the [Omadesign creation skill](https://omadesign.app/skills/omadesign-create/SKILL.md), inspect an existing document and preserve its version, IDs, hierarchy and packed data. Do not invent a file schema.

## Batch export

Run this POSIX shell script in a folder containing .oma documents. It retains sources and returns a failure status if any conversion fails. Use a fresh output folder when preserving previous exports.

```sh
mkdir -p exports
failed=0
for file in ./*.oma; do
  [ -f "$file" ] || continue
  omadesign --convert "$file" --output "exports/$(basename "$file" .oma).png" || failed=1
done
exit "$failed"
```

Conversion uses the native engine. Document output formats include OMA, SVG, PNG, JPEG, PSD, PSB, OpenRaster and PDF; support varies by format. See https://omadesign.app/docs/formats for limitations. File workflows still need local file and shell access for the CLI. The desktop
Agent panel connects to your chosen ACP agent (Codex, Claude, Gemini, OpenCode,
or custom) and hosts the conversation beside the canvas. The [Markdown docs index](https://omadesign.app/llms.txt), `omadesign --agent-docs manual` and `omadesign --agent-skill` supply the documentation and creation instructions. Native photo folder adjustment batching, raster treatments and Layout interactions are desktop workflows.

## Efficient agent brief

Create an editable event campaign in poster, square and wide formats. Preserve SVG sources, import to Omadesign, inspect dimensions and import notes, render previews, and report failures. Ask for visual review before final delivery. Make a violet variation as separate files.

The kit includes three green designs and three violet variations, each as SVG, editable OMA and native-rendered PNG. No customer files or third-party artwork are included.
