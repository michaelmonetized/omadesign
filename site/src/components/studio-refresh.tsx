import { useState, type CSSProperties } from "react";
import { sitePath } from "../site";
import "./studio-refresh.css";

const asset = (name: string) => sitePath(`media/refresh/${name}.webp`);
const highlights = [
  {
    id: "design-tools",
    title: "Color with more dimension.",
    tag: "DESIGN",
    text: "Linear, radial, shape and conic gradients. Editable stops, gradient strokes, opacity and blending—all in the same canvas.",
    detail: "Four gradient families. Fill and stroke.",
    chips: ["Linear", "Radial", "Shape", "Conic"],
  },
  {
    id: "pixel-effects",
    title: "Keep the subject. Change the scene.",
    tag: "PIXEL",
    text: "Sample a background color, refine the key and clean up spill. Explore 17 filters and 13 effects with a before-and-after preview and an undoable Apply.",
    detail: "Chroma key with live comparison.",
    chips: ["Sample", "Refine", "Compare", "Apply"],
  },
  {
    id: "layout",
    title: "One idea. Every screen.",
    tag: "LAYOUT",
    text: "Build with nested frames, Stack, Wrap and Grid. Reuse components, set breakpoint overrides and try interactions in Present. Export responsive HTML with embedded assets.",
    detail: "Responsive frames. Real prototype interactions.",
    chips: ["Desktop", "Tablet", "Phone", "Present"],
  },
];
export function StudioRefresh() {
  const [selected, setSelected] = useState(0);
  const [phone, setPhone] = useState(false);
  const feature = highlights[selected];
  const screenshot = selected === 2 && phone ? "layout-phone" : feature.id;
  return (
    <section className="section shell refresh-section" id="whats-new">
      <p className="refresh-eyebrow">
        MORE CONTROL, FROM FIRST SHAPE TO FINAL EXPORT
      </p>
      <h2>
        A bigger creative toolkit.
        <br />
        <span>A closer look.</span>
      </h2>
      <div className="refresh-tabs" aria-label="Explore new tools">
        {highlights.map((item, i) => (
          <button
            key={item.id}
            type="button"
            aria-pressed={selected === i}
            onClick={() => setSelected(i)}
          >
            {item.tag}
          </button>
        ))}
      </div>
      <div className="refresh-feature">
        <div className="refresh-copy">
          <p className="refresh-eyebrow">{feature.tag}</p>
          <h3>{feature.title}</h3>
          <p>{feature.text}</p>
          <div
            className={`feature-schematic schematic-${feature.id}`}
            aria-label={`${feature.tag} feature illustration`}
          >
            {feature.chips.map((chip, i) => (
              <div key={chip}>
                <i style={{ "--i": i } as CSSProperties} />
                <span>{chip}</span>
              </div>
            ))}
          </div>
          <small>Feature illustration · native workspace shown alongside</small>
          <a className="text-link" href={sitePath("updates/0.5.4")}>
            Explore the release ↗
          </a>
        </div>
        <figure>
          <a
            href={asset(screenshot)}
            aria-label={`Open full-size ${feature.tag} screenshot`}
          >
            <img
              src={asset(screenshot)}
              alt={`${feature.detail} Captured from the native Omadesign workspace.`}
              width="1600"
              height="900"
              loading="lazy"
            />
          </a>
          <figcaption>{feature.detail} · Native app capture</figcaption>
          {selected === 2 && (
            <div className="refresh-tabs" aria-label="Layout viewport">
              <button
                type="button"
                aria-pressed={!phone}
                onClick={() => setPhone(false)}
              >
                Desktop
              </button>
              <button
                type="button"
                aria-pressed={phone}
                onClick={() => setPhone(true)}
              >
                Phone
              </button>
            </div>
          )}
        </figure>
      </div>
      <div className="refresh-mini-grid">
        <article>
          <span>01 / PHOTO</span>
          <h3>A consistent look across the shoot.</h3>
          <p>
            Copy selected adjustment categories to selected photos or a whole
            folder. Background sidecar writes preserve the original pixels;
            progress, cancellation and Undo stay within reach.
          </p>
        </article>
        <article>
          <span>02 / WORKFLOW</span>
          <h3>Move work, keep it editable.</h3>
          <p>
            Paste images, text and SVG. Reorder and nest artwork directly in the
            layer tree. Carry palettes, typography and assets in a portable
            brand kit.
          </p>
        </article>
        <article>
          <span>03 / CLOUD</span>
          <h3>Bring people into the review.</h3>
          <p>
            Upload project versions and snapshots. Invite collaborators, collect
            comments and annotations, and publish selected work to the showcase.
          </p>
          <a className="text-link" href={sitePath("docs/cloud")}>
            Sharing and review ↗
          </a>
        </article>
      </div>
    </section>
  );
}

const workflows = [
  {
    label: "Create designs",
    prompt:
      "Create an editable event campaign: a poster, a square social graphic and a wide banner. Keep the colors and typography consistent. Save SVG sources, editable Omadesign projects and PNG previews.",
    title: "From a brief to an editable campaign.",
    text: "An AI coding agent writes the SVG source, then Omadesign imports supported shapes and text into editable layers. Review the native render, refine the source, and open the .oma file to finish by hand.",
    code: "omadesign --convert poster.svg --output poster.oma\nomadesign --inspect poster.oma\nomadesign --convert poster.oma --output poster.png",
  },
  {
    label: "Alter graphics",
    prompt:
      "Make a second version of this SVG campaign with a violet background and a peach accent. Preserve the original sources, update the three sizes consistently, and render previews for review.",
    title: "Change the direction without rebuilding the idea.",
    text: "Let the agent revise source colors, copy and geometry across related SVGs. Convert each revision into a separate .oma document and inspect the rendered output before choosing the final version.",
    code: "omadesign --convert poster-violet.svg --output poster-violet.oma\nomadesign --inspect poster-violet.oma\nomadesign --convert poster-violet.oma --output poster-violet.png",
  },
  {
    label: "Batch process",
    prompt:
      "Export every .oma file in this campaign folder to PNG. Use a separate output folder, keep the editable originals, and report any file that fails instead of silently skipping it.",
    title: "Repeat the work. Keep the originals.",
    text: "An agent can orchestrate the CLI in a shell loop: inspect inputs, export each document and collect failures. The same native import and export engine powers the desktop and headless conversion.",
    code: 'mkdir -p exports\nfailed=0\nfor file in ./*.oma; do\n  [ -f "$file" ] || continue\n  omadesign --convert "$file" \\\n    --output "exports/$(basename "$file" .oma).png" || failed=1\ndone\nexit "$failed"',
  },
];
export function AiWorkflow() {
  const [selected, setSelected] = useState(0);
  const [copied, setCopied] = useState("");
  const flow = workflows[selected];
  async function copy() {
    try {
      await navigator.clipboard.writeText(flow.prompt);
      setCopied("Prompt copied");
    } catch {
      setCopied("Copy unavailable. Select the prompt text to copy it.");
    }
  }
  return (
    <section className="section ai-section" id="ai">
      <div className="shell">
        <p className="refresh-eyebrow">
          YOUR AGENT. YOUR FILES. YOUR CREATIVE DIRECTION.
        </p>
        <div className="ai-heading">
          <h2>
            Give AI the busywork.
            <br />
            <span>Keep the art direction.</span>
          </h2>
          <p>
            Create a campaign. Revise a family of graphics. Export a folder in
            one pass. Connect an external AI coding agent to Omadesign’s
            existing file and command-line workflows. Start from “Create with agent” on the welcome screen; “Learn with AI” opens a question prompt with the app’s documentation.
          </p>
        </div>
        <div className="ai-workbench">
          <div className="ai-brief">
            <div className="refresh-tabs" aria-label="AI workflow examples">
              {workflows.map((item, i) => (
                <button
                  type="button"
                  key={item.label}
                  aria-pressed={selected === i}
                  onClick={() => {
                    setSelected(i);
                    setCopied("");
                  }}
                >
                  {item.label}
                </button>
              ))}
            </div>
            <p className="refresh-eyebrow">EXAMPLE BRIEF FOR YOUR AGENT</p>
            <blockquote>{flow.prompt}</blockquote>
            <button className="ai-copy" type="button" onClick={copy}>
              Copy this brief ↗
            </button>
            <p className="ai-copy-status" role="status">
              {copied}
            </p>
            <h3>{flow.title}</h3>
            <p>{flow.text}</p>
            <details>
              <summary>See the CLI workflow</summary>
              <pre>
                <code>{flow.code}</code>
              </pre>
            </details>
          </div>
          <figure className="ai-output">
            <div className="ai-output-label">
              <span>BRIEF → SVG → .OMA → PNG</span>
              <span>Native-rendered outputs</span>
            </div>
            <div className="ai-campaign">
              {["poster", "square", "wide"].map((name) => (
                <img
                  key={name}
                  className={`campaign-${name}`}
                  width={
                    name === "poster" ? 900 : name === "square" ? 1080 : 1600
                  }
                  height={
                    name === "poster" ? 1125 : name === "square" ? 1080 : 900
                  }
                  src={sitePath(
                    `media/ai/${name}${selected === 1 ? "-violet" : ""}.png`,
                  )}
                  alt={`AI-authored ${name} campaign design, imported and rendered through Omadesign${selected === 1 ? ", violet variation" : ""}.`}
                  loading="lazy"
                />
              ))}
            </div>
            <figcaption>
              One visual identity. Three editable formats.
              <br />
              Example artwork made for this page with an AI agent and the native
              converter.
            </figcaption>
            <div className="ai-downloads">
              <a
                href={sitePath(
                  `media/ai/poster${selected === 1 ? "-violet" : ""}.oma`,
                )}
                download
              >
                Editable poster ↓
              </a>
              <a
                href={sitePath(
                  `media/ai/poster${selected === 1 ? "-violet" : ""}.svg`,
                )}
                download
              >
                SVG source ↓
              </a>
              <a href={sitePath("media/ai/workflow.zip")} download>
                Full example kit ↓
              </a>
            </div>
          </figure>
        </div>
        <div className="ai-footnotes">
          <p>
            <strong>Efficient by design.</strong> Use structured files and
            headless conversion for repetitive work. Render previews at
            milestones, check import notes, and return to the canvas for visual
            decisions.
          </p>
          <p>
            <strong>Bring your own agent.</strong> These workflows use an
            external agent with local file and shell access. Omadesign does not
            include an AI chat panel or a general-purpose prompt-to-edit API.
            Native painting, filters and Layout controls remain desktop
            workflows.
          </p>
        </div>
        <a className="text-link" href={sitePath("skills/omadesign-create/SKILL.md")} download>
          Get the Omadesign creation skill ↗
        </a>
        {" · "}
        <a className="text-link" href={sitePath("llms.txt")}>
          Markdown documentation ↗
        </a>
      </div>
    </section>
  );
}
