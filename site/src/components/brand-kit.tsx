import { useState, type CSSProperties } from "react";
import { sitePath } from "../site";
import { Arrow } from "./studio-ui";
import "./brand-kit.css";

type KitFile = "colors" | "assets" | "type";
type MarkName = "Bloom" | "Orbit" | "Spark" | "Steps" | "Waves" | "Grid";

const colors = [
  { name: "Mauve", hex: "#CBA6F7" },
  { name: "Blue", hex: "#89B4FA" },
  { name: "Pink", hex: "#F5C2E7" },
  { name: "Peach", hex: "#FAB387" },
  { name: "Text", hex: "#CDD6F4" },
  { name: "Mist", hex: "#CBA6F780" },
];
const assets: { name: MarkName; folder: "Marks" | "Patterns" }[] = [
  { name: "Bloom", folder: "Marks" },
  { name: "Orbit", folder: "Marks" },
  { name: "Spark", folder: "Marks" },
  { name: "Steps", folder: "Patterns" },
  { name: "Waves", folder: "Patterns" },
  { name: "Grid", folder: "Patterns" },
];
const roles = [
  {
    name: "Heading",
    family: "Sans serif",
    css: "var(--font-sans)",
    file: "fonts/Heading.ttf",
    weight: 650,
  },
  {
    name: "Body",
    family: "Serif",
    css: "Georgia, serif",
    file: "fonts/Body.ttf",
    weight: 400,
  },
  {
    name: "Caption",
    family: "Monospace",
    css: "var(--font-mono)",
    file: "fonts/Caption.otf",
    weight: 400,
  },
];
const files: { id: KitFile; name: string; description: string }[] = [
  {
    id: "colors",
    name: ".omacolors",
    description: "Named palettes. Apply a color to a fill or stroke.",
  },
  {
    id: "assets",
    name: ".omabrand",
    description: "A folder of reusable artwork. Search by name or category.",
  },
  {
    id: "type",
    name: ".omatype",
    description:
      "Font roles with local TTF and OTF files. Apply a role to editable text.",
  },
];

function Mark({
  name,
  fill,
  stroke = "none",
}: {
  name: MarkName;
  fill: string;
  stroke?: string;
}) {
  return (
    <svg viewBox="0 0 240 240" aria-hidden="true" focusable="false">
      <g fill={fill} stroke={stroke} strokeWidth="5" strokeLinejoin="round">
        {name === "Bloom" && (
          <path d="M120 120C34 120 30 28 84 32C112 34 120 70 120 120C120 34 212 30 208 84C206 112 170 120 120 120C206 120 210 212 156 208C128 206 120 170 120 120C120 206 28 210 32 156C34 128 70 120 120 120Z" />
        )}
        {name === "Orbit" && (
          <>
            <circle cx="120" cy="120" r="40" />
            <g fill="none" stroke={fill} strokeWidth="16">
              <ellipse
                cx="120"
                cy="120"
                rx="92"
                ry="62"
                transform="rotate(-35 120 120)"
              />
              <ellipse
                cx="120"
                cy="120"
                rx="92"
                ry="62"
                transform="rotate(55 120 120)"
              />
            </g>
          </>
        )}
        {name === "Spark" && (
          <path d="M120 20L143 84L205 55L156 108L220 120L156 138L185 200L132 155L120 220L102 156L40 185L84 132L20 120L84 102L55 40L108 84Z" />
        )}
        {name === "Steps" && (
          <>
            {[0, 1, 2, 3].map((step) => (
              <path
                key={step}
                d={`M${30 + step * 45} 200V${35 + step * 35}h32v${165 - step * 35}Z`}
              />
            ))}
          </>
        )}
        {name === "Waves" && (
          <g fill="none" stroke={fill} strokeWidth="16" strokeLinecap="round">
            {[60, 100, 140, 180].map((y) => (
              <path key={y} d={`M30 ${y}Q75 ${y - 42} 120 ${y}T210 ${y}`} />
            ))}
          </g>
        )}
        {name === "Grid" && (
          <>
            {Array.from({ length: 9 }, (_, i) => (
              <rect
                key={i}
                x={30 + (i % 3) * 64}
                y={30 + Math.floor(i / 3) * 64}
                width="52"
                height="52"
                rx={i % 2 ? 26 : 5}
              />
            ))}
          </>
        )}
      </g>
    </svg>
  );
}

export function BrandKit() {
  const [selected, setSelected] = useState<KitFile>("colors");
  const [target, setTarget] = useState<"Fill" | "Stroke">("Fill");
  const [fill, setFill] = useState(colors[0].hex);
  const [stroke, setStroke] = useState(colors[1].hex);
  const [assetQuery, setAssetQuery] = useState("");
  const [assetFolder, setAssetFolder] = useState("All");
  const [selectedAsset, setSelectedAsset] = useState<MarkName>("Bloom");
  const [roleQuery, setRoleQuery] = useState("");
  const [appliedRole, setAppliedRole] = useState(roles[0]);
  const [previewText, setPreviewText] = useState(
    "The quick brown fox\njumps over the lazy dog.",
  );
  const file = files.find((item) => item.id === selected)!;
  const visibleAssets = assets.filter(
    (asset) =>
      (assetFolder === "All" || asset.folder === assetFolder) &&
      `${asset.name} ${asset.folder} svg`
        .toLowerCase()
        .includes(assetQuery.trim().toLowerCase()),
  );
  const visibleRoles = roles.filter((role) =>
    `${role.name} ${role.family}`
      .toLowerCase()
      .includes(roleQuery.trim().toLowerCase()),
  );
  const example =
    selected === "colors"
      ? {
          version: 1,
          palettes: [
            {
              name: "Catppuccin Mocha",
              colors: colors.map((color) => color.hex),
            },
          ],
        }
      : selected === "assets"
        ? { version: 1, name: "Studio" }
        : {
            version: 1,
            name: "Studio typography",
            roles: roles.map(({ name, file }) => ({ name, font: file })),
          };

  return (
    <section className="brand-section brand-kit-block" id="brand">
      <div className="shell section">
        <div className="section-heading" data-motion>
          <h2>Your project’s brand kit</h2>
          <p>Keep colors, artwork and fonts beside your document.</p>
        </div>
        <div
          className="brandkit-tabs"
          aria-label="Explore project files"
          data-motion
        >
          {files.map((item) => (
            <button
              key={item.id}
              type="button"
              aria-pressed={selected === item.id}
              onClick={() => setSelected(item.id)}
            >
              {item.name}
            </button>
          ))}
        </div>
        <div className="brandkit-workspace" data-motion>
          <div className="brandkit-toolbar">
            <p>{file.description}</p>
            <span>Interactive preview</span>
          </div>
          {selected === "colors" && (
            <div className="brandkit-panel">
              <div className="brandkit-sidebar">
                <div className="brandkit-control-row">
                  <h3>Catppuccin Mocha</h3>
                  <div className="brandkit-toggle" aria-label="Color target">
                    {(["Fill", "Stroke"] as const).map((value) => (
                      <button
                        key={value}
                        type="button"
                        aria-pressed={target === value}
                        onClick={() => setTarget(value)}
                      >
                        {value}
                      </button>
                    ))}
                  </div>
                </div>
                <div className="brandkit-swatches">
                  {colors.map((color) => (
                    <button
                      key={color.hex}
                      type="button"
                      className="brandkit-swatch"
                      aria-label={`Apply ${color.name} ${color.hex} to ${target.toLowerCase()}`}
                      aria-pressed={
                        (target === "Fill" ? fill : stroke) === color.hex
                      }
                      onClick={() =>
                        target === "Fill"
                          ? setFill(color.hex)
                          : setStroke(color.hex)
                      }
                    >
                      <span
                        className="brandkit-swatch-chip"
                        style={{ "--swatch": color.hex } as CSSProperties}
                      />
                      <span>
                        {color.name}
                        <code>{color.hex}</code>
                      </span>
                    </button>
                  ))}
                </div>
              </div>
              <div
                className="brandkit-preview"
                role="img"
                aria-label={`Bloom mark with fill ${fill} and stroke ${stroke}`}
              >
                <div className="brandkit-artboard">
                  <Mark name="Bloom" fill={fill} stroke={stroke} />
                </div>
                <p className="brandkit-preview-status" aria-live="polite">
                  Fill <code>{fill}</code>
                  <span>·</span>Stroke <code>{stroke}</code>
                </p>
              </div>
            </div>
          )}
          {selected === "assets" && (
            <div className="brandkit-panel">
              <div className="brandkit-sidebar">
                <label className="brandkit-search">
                  <span>Search assets</span>
                  <input
                    type="search"
                    value={assetQuery}
                    onChange={(event) => setAssetQuery(event.target.value)}
                    placeholder="Name or folder"
                  />
                </label>
                <div className="brandkit-filter" aria-label="Asset category">
                  {["All", "Marks", "Patterns"].map((folder) => (
                    <button
                      key={folder}
                      type="button"
                      aria-pressed={assetFolder === folder}
                      onClick={() => setAssetFolder(folder)}
                    >
                      {folder}
                    </button>
                  ))}
                </div>
                <div className="brandkit-assets">
                  {visibleAssets.map((asset) => (
                    <button
                      key={asset.name}
                      type="button"
                      aria-label={`Preview ${asset.name}`}
                      aria-pressed={selectedAsset === asset.name}
                      onClick={() => setSelectedAsset(asset.name)}
                    >
                      <span>
                        <Mark name={asset.name} fill="currentColor" />
                      </span>
                      <strong>{asset.name}</strong>
                      <small>
                        {asset.folder.toLowerCase()}/{asset.name.toLowerCase()}
                        .svg
                      </small>
                    </button>
                  ))}
                </div>
                {visibleAssets.length === 0 && (
                  <div className="brandkit-empty">
                    <p>No matching assets.</p>
                    <button
                      type="button"
                      onClick={() => {
                        setAssetQuery("");
                        setAssetFolder("All");
                      }}
                    >
                      Clear filters
                    </button>
                  </div>
                )}
                <p className="brandkit-result-count" role="status">
                  {visibleAssets.length}{" "}
                  {visibleAssets.length === 1 ? "asset" : "assets"}
                </p>
              </div>
              <div
                className="brandkit-preview"
                role="img"
                aria-label={`${selectedAsset} asset preview`}
              >
                <div className="brandkit-artboard">
                  <Mark name={selectedAsset} fill="#CBA6F7" />
                </div>
                <p className="brandkit-preview-status" aria-live="polite">
                  {selectedAsset}
                  <span>·</span>Editable SVG
                </p>
              </div>
            </div>
          )}
          {selected === "type" && (
            <div className="brandkit-panel">
              <div className="brandkit-sidebar">
                <label className="brandkit-search">
                  <span>Search font roles</span>
                  <input
                    type="search"
                    value={roleQuery}
                    onChange={(event) => setRoleQuery(event.target.value)}
                    placeholder="Role or font style"
                  />
                </label>
                <div className="brandkit-roles">
                  {visibleRoles.map((role) => (
                    <div
                      key={role.name}
                      className="brandkit-role"
                      data-selected={appliedRole.name === role.name}
                    >
                      <span
                        className="brandkit-role-sample"
                        style={{ fontFamily: role.css }}
                      >
                        Aa
                      </span>
                      <span>
                        <strong>{role.name}</strong>
                        <small>{role.family}</small>
                      </span>
                      <button
                        type="button"
                        aria-label={`Apply ${role.name} role`}
                        aria-pressed={appliedRole.name === role.name}
                        onClick={() => setAppliedRole(role)}
                      >
                        Apply
                      </button>
                    </div>
                  ))}
                </div>
                {visibleRoles.length === 0 && (
                  <div className="brandkit-empty">
                    <p>No matching roles.</p>
                    <button type="button" onClick={() => setRoleQuery("")}>
                      Clear search
                    </button>
                  </div>
                )}
              </div>
              <div className="brandkit-type-preview">
                <label htmlFor="brandkit-text-preview">Text preview</label>
                <textarea
                  id="brandkit-text-preview"
                  value={previewText}
                  maxLength={200}
                  onChange={(event) => setPreviewText(event.target.value)}
                  spellCheck={false}
                  style={{
                    fontFamily: appliedRole.css,
                    fontWeight: appliedRole.weight,
                  }}
                />
                <p className="brandkit-preview-status" aria-live="polite">
                  {appliedRole.name}
                  <span>·</span>
                  {appliedRole.family}
                </p>
              </div>
            </div>
          )}
          <details className="brandkit-file-example" key={selected}>
            <summary>
              View {selected === "assets" ? ".omabrand/brand.json" : file.name}{" "}
              example
            </summary>
            <pre>
              <code>{JSON.stringify(example, null, 2)}</code>
            </pre>
          </details>
        </div>
        <a
          className="brandkit-guide text-link"
          href={`${sitePath("docs/manual")}#palettes-and-brand-libraries`}
        >
          Read the brand kit guide <Arrow />
        </a>
      </div>
    </section>
  );
}
