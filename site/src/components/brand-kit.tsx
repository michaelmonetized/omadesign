import { useState } from "react";
import { sitePath } from "../site";
import { Arrow, Eyebrow, Shot } from "./studio-ui";

const kitFiles = [
  {
    name: ".omacolors",
    label: "Your colours",
    heading: "Good taste travels.",
    copy: "Name a palette. Collect colours from your artwork. Filter by name or hex, apply to fills or strokes, and share a readable JSON file.",
    code: '{\n  "version": 1,\n  "palettes": [{\n    "name": "Fieldwork",\n    "colors": [\n      "#173F35", "#F5EBDC",\n      "#D97C5B", "#DEB451"\n    ]\n  }]\n}',
    image: "palette-library.webp",
    alt: "Named Fieldwork project palettes and editable swatches beside the artwork.",
  },
  {
    name: ".omabrand/",
    label: "Your assets",
    heading: "Your brand. Within reach.",
    copy: "Logos, illustrations, photos and editable .oma artwork. A filterable thumbnail bank that follows your folders. Drag a tile straight onto an artboard.",
    code: "fieldwork/\n├── campaign.oma\n├── .omacolors\n├── .omatype\n└── .omabrand/\n    ├── brand.json\n    ├── marks/\n    │   └── symbol.svg\n    ├── photography/\n    └── fonts/",
    image: "brand-library.webp",
    alt: "A project brand bank with searchable logos, illustrations and patterns.",
  },
  {
    name: ".omatype",
    label: "Your type",
    heading: "The right type. Everywhere.",
    copy: "Name your Heading and Body roles. Keep TTF or OTF files inside the project, apply them to live text, and take the kit to another Linux machine.",
    code: '{\n  "version": 1,\n  "name": "Fieldwork type",\n  "roles": [{\n    "name": "Heading",\n    "font": "fonts/Display.ttf"\n  }, {\n    "name": "Body",\n    "font": "fonts/Reading.otf"\n  }]\n}',
    image: "typography.webp",
    alt: "The Brand Typography panel with named project font roles and editable Fieldwork type.",
  },
];

export function BrandKit() {
  const [selected, setSelected] = useState(0);
  const kit = kitFiles[selected];
  return (
    <section className="brand-section" id="brand">
      <div className="shell section">
        <div className="section-heading">
          <div>
            <Eyebrow>02 / A VERY LINUX KIND OF BRAND KIT</Eyebrow>
            <h2>
              Big brand energy.
              <br />
              <span className="accent">Tiny dotfiles.</span>
            </h2>
          </div>
          <p>
            Colours, assets and fonts, right beside your project.
            <br />
            Name it. Save it. Filter it. Take it with you.
          </p>
        </div>
        <div className="kit-layout">
          <div className="kit-controls">
            <div className="segment-control" aria-label="Explore project files">
              {kitFiles.map((file, i) => (
                <button
                  key={file.name}
                  type="button"
                  aria-pressed={selected === i}
                  onClick={() => setSelected(i)}
                >
                  {file.name}
                </button>
              ))}
            </div>
            <div className="kit-description">
              <span className="small-label">{kit.label}</span>
              <h3>{kit.heading}</h3>
              <p>{kit.copy}</p>
              <a
                className="text-link"
                href={`${sitePath("docs/manual")}#palettes-and-brand-libraries`}
              >
                Open the project kit guide <Arrow />
              </a>
            </div>
            <div className="code-window">
              <div>
                <span className="status-dot" />
                {kit.name}
                <span>PLAIN FILES. YOUR FILES.</span>
              </div>
              <pre>
                <code>{kit.code}</code>
              </pre>
            </div>
          </div>
          <figure className="kit-image">
            <Shot name={kit.image} alt={kit.alt} />
            <figcaption>
              Native panels. Local files. One portable project.
            </figcaption>
          </figure>
        </div>
      </div>
    </section>
  );
}
