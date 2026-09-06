import { useState } from "react";
import { Arrow, Eyebrow, Shot } from "./studio-ui";

const studios = [
  {
    name: "Design",
    number: "01",
    title: "From first point to final form.",
    description:
      "Precise vectors. Expressive type. Layouts with room to breathe. Draw your idea, refine every curve, and give it a home on an artboard.",
    image: "logo-design.webp",
    alt: "The corrected deSiGN wordmark, drawn as editable vectors over the Omarchy maze in the native Design studio.",
    features: [
      "Pen & Bézier editing",
      "Smart spacing & guides",
      "Vector warp & perspective",
      "Live type & OpenType",
    ],
  },
  {
    name: "Pixel",
    number: "02",
    title: "A little texture. A human touch.",
    description:
      "Paint, erase, clone and heal. Make a precise selection or reach for a soft brush. Editable masks let you keep experimenting.",
    image: "healing.webp",
    alt: "The native Pixel studio showing a texture before and after healing, with brush and sampling controls.",
    features: [
      "Brush, fill & smudge",
      "Clone & healing brushes",
      "Marquee, lasso & wand",
      "Editable layer masks",
    ],
  },
  {
    name: "Photo",
    number: "03",
    title: "Find the feeling in the frame.",
    description:
      "Shape the light, tune the colour, bring out the detail. Compare your edit with the original, then place it straight into your design.",
    image: "photo.webp",
    alt: "Photo studio with a sample landscape, histogram and grouped light, colour and detail adjustments.",
    features: [
      "Curves & colour mixer",
      "Colour grading & detail",
      "Before & auto light",
      "Crop & background JPEG export",
    ],
  },
  {
    name: "Motion",
    number: "04",
    title: "Good design has good moves.",
    description:
      "Make a mark draw itself. Let a headline land. Animate the artwork you already made, with presets that become keys you can make your own.",
    image: "motion-presets.webp",
    alt: "Motion studio with Draw stroke, Fill up and Pop in examples and editable keys on the timeline.",
    features: [
      "13 editable motion presets",
      "Delay, stagger & easing",
      "Stroke & fill reveals",
      "Animated SVG & Lottie",
    ],
  },
];

export function Studios() {
  const [selected, setSelected] = useState(0);
  const studio = studios[selected];
  return (
    <section className="section shell studios-section" id="studios">
      <div className="section-heading">
        <div>
          <Eyebrow>03 / FOUR WAYS TO MAKE IT YOURS</Eyebrow>
          <h2>
            Stay with the idea.
            <br />
            Switch the studio.
          </h2>
        </div>
        <p>
          Move from vectors to texture to motion.
          <br />
          Bring developed photos into your design.
          <br />
          Keep your creative momentum.
        </p>
      </div>
      <div className="studio-switcher" aria-label="Explore the four studios">
        {studios.map((item, i) => (
          <button
            key={item.name}
            type="button"
            aria-pressed={i === selected}
            onClick={() => setSelected(i)}
          >
            <span>{item.number}</span>
            {item.name}
            <Arrow />
          </button>
        ))}
      </div>
      <div className="studio-stage">
        <figure>
          <Shot name={studio.image} alt={studio.alt} />
        </figure>
        <div className="studio-copy">
          <span className="small-label">THE {studio.name} STUDIO</span>
          <h3>{studio.title}</h3>
          <p>{studio.description}</p>
          <ul>
            {studio.features.map((item) => (
              <li key={item}>
                <span aria-hidden="true">↗</span>
                {item}
              </li>
            ))}
          </ul>
          <a className="text-link" href="#features">
            Explore the tools <Arrow />
          </a>
        </div>
      </div>
    </section>
  );
}
