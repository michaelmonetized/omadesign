import { useEffect, useId, useRef, useState } from "react";
import type { CSSProperties, KeyboardEvent, PointerEvent } from "react";
import { sitePath } from "../site";
import "./studio-carousel.css";

type StudioName = "Design" | "Pixel" | "Photo" | "Motion";
type Value = string | number | boolean;
type Settings = Record<string, Value>;

const sharedPanels = ["Palettes", "Brand"];
const studios: {
  name: StudioName;
  image: string;
  alt: string;
  panels: string[];
}[] = [
  {
    name: "Design",
    image: "design.webp",
    alt: "Block Party: a coral, citron, and ink geometric poster created with editable vectors in omadesign.",
    panels: [
      "Appearance",
      "Layout",
      "Typography",
      "Reshape",
      "Effects",
      "Trace",
      "Layers",
      ...sharedPanels,
    ],
  },
  {
    name: "Pixel",
    image: "pixel.webp",
    alt: "Iris study: textured purple iris illustration prepared in Pixel studio.",
    panels: ["Brush", "Layer mask", "Color", "Layers", ...sharedPanels],
  },
  {
    name: "Photo",
    image: "photo.webp",
    alt: "Coast road: original landscape artwork color-graded in Photo studio.",
    panels: ["Light", "Color", "Detail", "Library", ...sharedPanels],
  },
  {
    name: "Motion",
    image: "motion.webp",
    alt: "After Hours: cream typography and mauve and gold concentric rings, created as editable animated artwork in omadesign.",
    panels: [
      "Presets",
      "Keyframes",
      "Appearance",
      "Reshape",
      "Layers",
      ...sharedPanels,
    ],
  },
];

const defaultSettings: Settings = {
  fill: "#cba6f7",
  stroke: "#f5e0dc",
  strokeWidth: 2,
  opacity: 100,
  x: 0,
  y: 0,
  scale: 100,
  rotation: 0,
  text: "Aa\n0123",
  font: "Sans",
  typeSize: 52,
  tracking: -2,
  leading: 1.06,
  smallCaps: false,
  reshape: "Distort",
  amount: 18,
  blur: 0,
  shadow: 12,
  traceColors: 3,
  smoothness: 2,
  brush: "Brush",
  size: 34,
  edge: 70,
  flow: 85,
  tolerance: 40,
  mask: true,
  reveal: 72,
  paintOn: "Mask",
  markVisible: true,
  textVisible: true,
  paperVisible: true,
  palette: "Project",
  target: "Fill",
  query: "",
  asset: "Mark",
  assetType: "All",
  brandFont: "Sans",
  exposure: 0.2,
  contrast: 110,
  highlights: -12,
  shadows: 15,
  curve: 50,
  temperature: 0,
  saturation: 105,
  hue: 0,
  grading: 0,
  clarity: 10,
  grain: 0,
  vignette: 10,
  orientation: "0°",
  photo: "Landscape",
  before: false,
  preset: "Draw stroke",
  duration: 1.2,
  delay: 0,
  intensity: 1,
  progress: 72,
  ease: "Ease out",
};

const swatches = [
  "#cba6f7",
  "#89b4fa",
  "#94e2d5",
  "#a6e3a1",
  "#f9e2af",
  "#fab387",
  "#f38ba8",
  "#f5e0dc",
];
const presets = [
  "Draw stroke",
  "Pop in",
  "Slam",
  "Shake",
  "Fill up",
  "Slide up",
  "Slide down",
  "Slide left",
  "Slide right",
  "Fly",
  "Zoom",
  "Buzz",
  "Fade in",
];
const artwork = (image: string) => sitePath(`media/showcase/${image}`);
const number = (settings: Settings, key: string) => Number(settings[key]);
const wrap = (index: number) => (index + studios.length) % studios.length;

function Arrow({ previous = false }: { previous?: boolean }) {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      aria-hidden="true"
      style={previous ? { transform: "rotate(180deg)" } : undefined}
    >
      <path
        d="M4 12h15m-6-6 6 6-6 6"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

export function StudioCarousel() {
  const id = useId();
  const [selected, setSelected] = useState(0);
  const [panels, setPanels] = useState<Record<StudioName, string>>({
    Design: "Appearance",
    Pixel: "Brush",
    Photo: "Light",
    Motion: "Presets",
  });
  const [settings, setSettings] = useState<Record<StudioName, Settings>>(
    () => ({
      Design: { ...defaultSettings },
      Pixel: { ...defaultSettings },
      Photo: { ...defaultSettings },
      Motion: { ...defaultSettings },
    }),
  );
  const pointer = useRef<{ x: number; y: number; id: number } | null>(null);
  const swiped = useRef(false);
  const buttons = useRef<(HTMLButtonElement | null)[]>([]);
  const studio = studios[selected];
  const panel = panels[studio.name];
  const values = settings[studio.name];
  const change = (key: string, value: Value) =>
    setSettings((current) => ({
      ...current,
      [studio.name]: { ...current[studio.name], [key]: value },
    }));

  function navigate(event: KeyboardEvent<HTMLElement>, focusTab = false) {
    const next =
      event.key === "ArrowRight"
        ? wrap(selected + 1)
        : event.key === "ArrowLeft"
          ? wrap(selected - 1)
          : event.key === "Home"
            ? 0
            : event.key === "End"
              ? studios.length - 1
              : null;
    if (next === null) return;
    event.preventDefault();
    setSelected(next);
    if (focusTab) buttons.current[next]?.focus();
  }

  function startSwipe(event: PointerEvent<HTMLDivElement>) {
    if (!event.isPrimary || event.button !== 0) return;
    swiped.current = false;
    pointer.current = {
      x: event.clientX,
      y: event.clientY,
      id: event.pointerId,
    };
    (event.target as Element)
      .closest("button")
      ?.setPointerCapture(event.pointerId);
  }

  function finishSwipe(event: PointerEvent<HTMLDivElement>) {
    const start = pointer.current;
    pointer.current = null;
    if (!start || start.id !== event.pointerId) return;
    const dx = event.clientX - start.x;
    const dy = event.clientY - start.y;
    if (Math.abs(dx) > 40 && Math.abs(dx) > Math.abs(dy) * 1.3) {
      swiped.current = true;
      setSelected((current) => wrap(current + (dx < 0 ? 1 : -1)));
    }
  }

  return (
    <section
      className="studio-carousel"
      id="studios"
      aria-label="Explore the studios"
    >
      <div
        data-motion
        className="sc-studio-tabs"
        aria-label="Studios"
        onKeyDown={(event) => navigate(event, true)}
      >
        {studios.map((item, index) => (
          <button
            key={item.name}
            ref={(element) => {
              buttons.current[index] = element;
            }}
            type="button"
            aria-pressed={index === selected}
            aria-controls={`${id}-stage`}
            onClick={() => setSelected(index)}
          >
            {item.name}
          </button>
        ))}
      </div>
      <div
        data-motion
        className="sc-carousel"
        id={`${id}-stage`}
        role="region"
        aria-roledescription="carousel"
        aria-label="Native studio artwork"
        tabIndex={0}
        onKeyDown={(event) => navigate(event)}
        onPointerDown={startSwipe}
        onPointerUp={finishSwipe}
        onPointerCancel={() => {
          pointer.current = null;
        }}
      >
        {studios.map((item, index) => {
          let offset = wrap(index - selected);
          if (offset > 2) offset -= 4;
          return (
            <button
              key={item.name}
              type="button"
              className="sc-slide"
              data-offset={offset}
              aria-hidden={offset === 2 ? true : undefined}
              aria-label={`Show ${item.name} studio`}
              aria-pressed={offset === 0}
              tabIndex={-1}
              onClick={() => {
                if (!swiped.current) setSelected(index);
              }}
            >
              <span className="sc-window-bar">
                <span aria-hidden="true" className="sc-window-dots">
                  <i />
                  <i />
                  <i />
                </span>
                <span>{item.name}</span>
                <span aria-hidden="true">omadesign</span>
              </span>
              <img
                src={artwork(item.image)}
                alt={item.alt}
                width="1600"
                height="1000"
                draggable={false}
                decoding="async"
                loading={index === 0 ? "eager" : "lazy"}
              />
            </button>
          );
        })}
      </div>
      <div className="sc-caption" data-motion>
        <button
          type="button"
          className="sc-arrow"
          aria-label="Previous studio"
          onClick={() => setSelected(wrap(selected - 1))}
        >
          <Arrow previous />
        </button>
        <h2 aria-live="polite" aria-atomic="true">
          {studio.name}
        </h2>
        <button
          type="button"
          className="sc-arrow"
          aria-label="Next studio"
          onClick={() => setSelected(wrap(selected + 1))}
        >
          <Arrow />
        </button>
      </div>
      <div className="sc-panel-explorer">
        <div className="sc-panel-heading" data-motion>
          <h3>Explore the panels</h3>
          <span>Interactive previews</span>
        </div>
        <div
          data-motion
          className="sc-panel-tabs"
          aria-label={`${studio.name} panels`}
        >
          {studio.panels.map((name) => (
            <button
              key={name}
              type="button"
              aria-pressed={panel === name}
              aria-controls={`${id}-panel`}
              onClick={() =>
                setPanels((current) => ({ ...current, [studio.name]: name }))
              }
            >
              {name}
            </button>
          ))}
        </div>
        <div data-motion className="sc-preview-workspace" id={`${id}-panel`}>
          <div className="sc-preview-canvas">
            <Preview studio={studio.name} panel={panel} settings={values} />
            <span className="sc-preview-note">
              Simplified preview · changes stay here
            </span>
          </div>
          <div className="sc-inspector">
            <div className="sc-inspector-title">
              <h4>{panel}</h4>
              <button
                type="button"
                onClick={() =>
                  setSettings((current) => ({
                    ...current,
                    [studio.name]: { ...defaultSettings },
                  }))
                }
              >
                Reset preview
              </button>
            </div>
            {studio.name === "Photo" && !sharedPanels.includes(panel) && (
              <Toggle
                label="Show original"
                value={Boolean(values.before)}
                onChange={(value) => change("before", value)}
              />
            )}
            <PanelControls
              studio={studio.name}
              panel={panel}
              settings={values}
              change={change}
            />
          </div>
        </div>
      </div>
      <noscript>
        <style>{`.studio-carousel .sc-carousel,.studio-carousel .sc-studio-tabs,.studio-carousel .sc-caption,.studio-carousel .sc-panel-explorer{display:none}`}</style>
        <div className="sc-static">
          {studios.map((item) => (
            <article key={item.name}>
              <h2>{item.name}</h2>
              <img
                src={artwork(item.image)}
                alt={item.alt}
                width="1600"
                height="1000"
                decoding="async"
                loading="lazy"
              />
              <p>{item.panels.join(" · ")}</p>
            </article>
          ))}
        </div>
      </noscript>
    </section>
  );
}

type ControlsProps = {
  studio: StudioName;
  panel: string;
  settings: Settings;
  change: (key: string, value: Value) => void;
};

function Range({
  label,
  value,
  min,
  max,
  step = 1,
  unit = "",
  onChange,
}: {
  label: string;
  value: number;
  min: number;
  max: number;
  step?: number;
  unit?: string;
  onChange: (value: number) => void;
}) {
  return (
    <label className="sc-range">
      <span>
        {label}
        <output>
          {Number(value.toFixed(2))}
          {unit}
        </output>
      </span>
      <input
        type="range"
        aria-label={label}
        min={min}
        max={max}
        step={step}
        value={value}
        onChange={(event) => onChange(event.currentTarget.valueAsNumber)}
      />
    </label>
  );
}

function Choice({
  label,
  value,
  options,
  onChange,
}: {
  label: string;
  value: string;
  options: string[];
  onChange: (value: string) => void;
}) {
  return (
    <label className="sc-choice">
      <span>{label}</span>
      <select
        value={value}
        onChange={(event) => onChange(event.currentTarget.value)}
      >
        {options.map((option) => (
          <option key={option}>{option}</option>
        ))}
      </select>
    </label>
  );
}

function Toggle({
  label,
  value,
  onChange,
}: {
  label: string;
  value: boolean;
  onChange: (value: boolean) => void;
}) {
  return (
    <label className="sc-toggle">
      <input
        type="checkbox"
        checked={value}
        onChange={(event) => onChange(event.currentTarget.checked)}
      />
      <span>{label}</span>
    </label>
  );
}

function Swatches({
  value,
  onChange,
  filter = "",
}: {
  value: string;
  onChange: (value: string) => void;
  filter?: string;
}) {
  const colors = swatches.filter((color) =>
    color.toLowerCase().includes(filter.toLowerCase().replace(/^#?$/, "")),
  );
  return (
    <div className="sc-swatches">
      {colors.map((color) => (
        <button
          key={color}
          type="button"
          aria-label={`Use ${color}`}
          aria-pressed={value === color}
          style={{ backgroundColor: color }}
          onClick={() => onChange(color)}
        >
          {value === color && <span aria-hidden="true">✓</span>}
        </button>
      ))}
      {colors.length === 0 && <p className="sc-empty">No matching colors.</p>}
    </div>
  );
}

function PanelControls({ studio, panel, settings: s, change }: ControlsProps) {
  const range = (
    key: string,
    label: string,
    min: number,
    max: number,
    unit = "",
    step = 1,
  ) => (
    <Range
      key={key}
      label={label}
      value={number(s, key)}
      min={min}
      max={max}
      step={step}
      unit={unit}
      onChange={(value) => change(key, value)}
    />
  );
  const choice = (key: string, label: string, options: string[]) => (
    <Choice
      label={label}
      value={String(s[key])}
      options={options}
      onChange={(value) => change(key, value)}
    />
  );
  const toggle = (key: string, label: string) => (
    <Toggle
      label={label}
      value={Boolean(s[key])}
      onChange={(value) => change(key, value)}
    />
  );
  const colors = (
    <>
      <Swatches
        value={String(s.fill)}
        onChange={(color) => change("fill", color)}
      />
      <label className="sc-color-input">
        Fill
        <input
          type="color"
          value={String(s.fill)}
          onChange={(event) => change("fill", event.currentTarget.value)}
        />
        <code>{String(s.fill)}</code>
      </label>
    </>
  );

  if (panel === "Appearance" || (panel === "Color" && studio !== "Photo"))
    return (
      <>
        {colors}
        {range("opacity", "Opacity", 0, 100, "%")}
        {panel === "Appearance" && (
          <>
            <label className="sc-color-input">
              Stroke
              <input
                type="color"
                value={String(s.stroke)}
                onChange={(event) =>
                  change("stroke", event.currentTarget.value)
                }
              />
              <code>{String(s.stroke)}</code>
            </label>
            {range("strokeWidth", "Width", 0, 12, " px")}
          </>
        )}
      </>
    );
  if (panel === "Layout")
    return (
      <>
        <div className="sc-two-fields">
          {range("x", "X", -70, 70, " px")}
          {range("y", "Y", -50, 50, " px")}
        </div>
        {range("scale", "Scale", 60, 130, "%")}
        {range("rotation", "Rotation", -45, 45, "°")}
      </>
    );
  if (panel === "Typography")
    return (
      <>
        <label className="sc-text-input">
          Text
          <textarea
            rows={2}
            value={String(s.text)}
            maxLength={36}
            onChange={(event) => change("text", event.currentTarget.value)}
          />
        </label>
        {choice("font", "Font", ["Sans", "Serif", "Mono"])}
        {range("typeSize", "Size", 28, 64, " px")}
        {range("tracking", "Letter spacing", -3, 6, " px", 0.5)}
        {range("leading", "Line height", 0.9, 1.5, "×", 0.05)}
        {toggle("smallCaps", "Small caps")}
      </>
    );
  if (panel === "Reshape")
    return (
      <>
        {choice("reshape", "Mode", [
          "Distort",
          "Skew",
          "Perspective",
          "Warp mesh",
        ])}
        {range("amount", "Handle position", -35, 35)}
        <div className="sc-control-note">
          Move a handle in the app. Try its effect here.
        </div>
      </>
    );
  if (panel === "Effects")
    return (
      <>
        {range("blur", "Gaussian blur", 0, 8, " px", 0.2)}
        {range("shadow", "Drop shadow", 0, 30, " px")}
        {range("opacity", "Opacity", 0, 100, "%")}
      </>
    );
  if (panel === "Trace")
    return (
      <>
        {range("traceColors", "Colors", 1, 8)}
        {range("smoothness", "Smoothness", 0, 8, "", 0.5)}
        <div className="sc-control-note">
          See the sample resolve into simpler paths.
        </div>
      </>
    );
  if (panel === "Brush")
    return (
      <>
        {choice("brush", "Tool", [
          "Brush",
          "Eraser",
          "Clone",
          "Heal",
          "Smudge",
          "Fill",
          "Wand",
        ])}
        {["Fill", "Wand"].includes(String(s.brush)) ? (
          range("tolerance", "Tolerance", 0, 180)
        ) : (
          <>
            {range("size", "Size", 4, 100, " px")}
            {range("edge", "Edge", 0, 100, "%")}
            {range("opacity", "Opacity", 5, 100, "%")}
            {range(
              "flow",
              s.brush === "Smudge" ? "Strength" : "Flow",
              5,
              100,
              "%",
            )}
          </>
        )}
        {["Clone", "Heal"].includes(String(s.brush)) && (
          <p className="sc-control-note">
            In the app: Alt-click a source, then paint.
          </p>
        )}
      </>
    );
  if (panel === "Layer mask")
    return (
      <>
        {toggle("mask", "Enable layer mask")}
        {choice("paintOn", "Paint on", ["Mask", "Pixels"])}
        {range("reveal", "Reveal", 0, 100, "%")}
        <div
          className="sc-mask-meter"
          style={{
            background: `linear-gradient(90deg,#fff ${number(s, "reveal")}%,#181825 ${number(s, "reveal")}%)`,
          }}
          aria-label={`Mask reveals ${s.reveal}%`}
        />
        <p className="sc-control-note">
          White reveals. Black hides. Original pixels stay intact.
        </p>
      </>
    );
  if (panel === "Layers")
    return (
      <>
        {range("opacity", "Layer opacity", 0, 100, "%")}
        <div className="sc-layers">
          {studio !== "Pixel" && toggle("textVisible", "Headline")}
          {toggle("markVisible", studio === "Pixel" ? "Brushwork" : "Artwork")}
          {toggle("paperVisible", "Paper")}
        </div>
      </>
    );
  if (panel === "Palettes")
    return (
      <>
        {choice("palette", "Collection", ["Project", "Personal"])}
        {choice("target", "Apply to", ["Fill", "Stroke"])}
        <label className="sc-text-input">
          Filter by hex
          <input
            type="search"
            value={String(s.query)}
            placeholder="#cba6f7"
            onChange={(event) => change("query", event.currentTarget.value)}
          />
        </label>
        <span className="sc-collection-name">
          {s.palette === "Project" ? "Studio colors" : "My favorites"}
        </span>
        <Swatches
          value={String(s[s.target === "Fill" ? "fill" : "stroke"])}
          filter={String(s.query)}
          onChange={(color) =>
            change(s.target === "Fill" ? "fill" : "stroke", color)
          }
        />
        <p className="sc-control-note">
          Click a swatch to change the {String(s.target).toLowerCase()}.
        </p>
      </>
    );
  if (panel === "Brand")
    return (
      <>
        {choice("assetType", "Filter assets", ["All", "Vectors", "Images"])}
        <div className="sc-brand-assets">
          {["Mark", "Poster", "Photo"]
            .filter(
              (asset) =>
                s.assetType === "All" ||
                (s.assetType === "Images"
                  ? asset === "Photo"
                  : asset !== "Photo"),
            )
            .map((asset) => (
              <button
                key={asset}
                type="button"
                aria-pressed={s.asset === asset}
                onClick={() => change("asset", asset)}
              >
                <span aria-hidden="true">
                  {asset === "Mark" ? "✳" : asset === "Poster" ? "Aa" : "▧"}
                </span>
                {asset}
              </button>
            ))}
        </div>
        {s.asset !== "Photo" &&
          choice("brandFont", "Typography role", ["Sans", "Serif", "Mono"])}
        <p className="sc-control-note">
          Pick an asset or font role to preview it.
        </p>
      </>
    );
  if (panel === "Light")
    return (
      <>
        <Histogram value={number(s, "exposure")} />

        {range("exposure", "Exposure", -1.5, 1.5, " EV", 0.1)}
        {range("contrast", "Contrast", 60, 160, "%")}
        {range("highlights", "Highlights", -50, 50)}
        {range("shadows", "Shadows", -50, 50)}
        <details className="sc-disclosure">
          <summary>Tone curve</summary>
          {range("curve", "Midtones", 0, 100)}
          <svg viewBox="0 0 240 90" aria-label="Tone curve">
            <path
              d={`M0 90 Q70 ${130 - number(s, "curve")} 120 ${90 - number(s, "curve")} T240 0`}
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
            />
          </svg>
        </details>
      </>
    );
  if (panel === "Color")
    return (
      <>
        {range("temperature", "Temperature", -100, 100)}
        {range("saturation", "Saturation", 0, 170, "%")}
        <details className="sc-disclosure" open>
          <summary>Color mixer</summary>
          {range("hue", "Hue", -40, 40, "°")}
        </details>
        <details className="sc-disclosure">
          <summary>Color grading</summary>
          {range("grading", "Shadow warmth", -40, 40)}
        </details>
      </>
    );
  if (panel === "Detail")
    return (
      <>
        {range("clarity", "Clarity", 0, 70)}
        {range("grain", "Grain", 0, 60)}
        {range("vignette", "Vignette", 0, 70)}
        {choice("orientation", "Orientation", ["0°", "90°", "180°", "270°"])}
      </>
    );
  if (panel === "Library")
    return (
      <div className="sc-photo-library">
        {["Landscape", "Detail", "Wide crop"].map((name, index) => (
          <button
            key={name}
            type="button"
            aria-pressed={s.photo === name}
            onClick={() => change("photo", name)}
          >
            <img
              src={artwork("photo.webp")}
              alt=""
              style={{ objectPosition: `${30 + index * 25}% center` }}
              width="100"
              height="72"
              decoding="async"
            />
            <span>{name}</span>
          </button>
        ))}
      </div>
    );
  if (panel === "Presets")
    return (
      <>
        <div className="sc-preset-grid">
          {presets.map((preset) => (
            <button
              key={preset}
              type="button"
              aria-pressed={s.preset === preset}
              onClick={() => change("preset", preset)}
            >
              {preset}
            </button>
          ))}
        </div>
        {range("duration", "Duration", 0.4, 3, " s", 0.1)}
        <details className="sc-disclosure">
          <summary>Timing & energy</summary>
          {range("delay", "Delay", 0, 1, " s", 0.1)}
          {range("intensity", "Intensity", 0.3, 2, "×", 0.1)}
        </details>
        <Playback settings={s} change={change} />
      </>
    );
  if (panel === "Keyframes")
    return (
      <>
        {choice("ease", "Easing", [
          "Linear",
          "Ease in",
          "Ease out",
          "Ease in-out",
        ])}
        {range("x", "X offset", -70, 70, " px")}
        {range("rotation", "Rotation", -90, 90, "°")}
        {range("scale", "Scale", 60, 130, "%")}
        <Playback settings={s} change={change} />
      </>
    );
  return null;
}

function Playback({
  settings,
  change,
}: {
  settings: Settings;
  change: ControlsProps["change"];
}) {
  const [playing, setPlaying] = useState(false);
  const changeRef = useRef(change);
  changeRef.current = change;
  useEffect(() => {
    if (!playing) return;
    const preference = window.matchMedia("(prefers-reduced-motion: reduce)");
    if (preference.matches) {
      changeRef.current("progress", 100);
      setPlaying(false);
      return;
    }
    let frame = 0;
    const start = performance.now() + number(settings, "delay") * 1000;
    const tick = (now: number) => {
      const progress = Math.max(
        0,
        Math.min(100, (now - start) / (number(settings, "duration") * 10)),
      );
      changeRef.current("progress", progress);
      if (progress < 100) frame = requestAnimationFrame(tick);
      else setPlaying(false);
    };
    const reduceMotion = () => {
      if (!preference.matches) return;
      cancelAnimationFrame(frame);
      changeRef.current("progress", 100);
      setPlaying(false);
    };
    preference.addEventListener("change", reduceMotion);
    frame = requestAnimationFrame(tick);
    return () => {
      cancelAnimationFrame(frame);
      preference.removeEventListener("change", reduceMotion);
    };
  }, [playing, settings.duration, settings.delay]);
  return (
    <div className="sc-playback">
      <Range
        label="Playhead"
        value={number(settings, "progress")}
        min={0}
        max={100}
        unit="%"
        onChange={(value) => {
          setPlaying(false);
          change("progress", value);
        }}
      />
      <button
        type="button"
        className="sc-play-button"
        onClick={() => {
          if (!playing) change("progress", 0);
          setPlaying(!playing);
        }}
      >
        {playing ? "Pause" : "Play preview"}
        <span aria-hidden="true">{playing ? "Ⅱ" : "▷"}</span>
      </button>
    </div>
  );
}

function Histogram({ value }: { value: number }) {
  return (
    <svg
      className="sc-histogram"
      viewBox="0 0 260 52"
      role="img"
      aria-label="Illustrative exposure histogram"
    >
      {["#89b4fa", "#a6e3a1", "#f38ba8"].map((color, channel) => (
        <path
          key={color}
          d={`M0 52 ${Array.from({ length: 53 }, (_, i) => {
            const x = i * 5;
            const height =
              Math.exp(-(((x - 85 - channel * 25 - value * 20) / 38) ** 2)) *
                (34 - channel * 6) +
              Math.exp(-(((x - 190) / 28) ** 2)) * 22;
            return `L${x} ${52 - height}`;
          }).join(" ")} L260 52Z`}
          fill={color}
          opacity="0.4"
        />
      ))}
    </svg>
  );
}

function Preview({
  studio,
  panel,
  settings: s,
}: {
  studio: StudioName;
  panel: string;
  settings: Settings;
}) {
  const id = useId().replaceAll(":", "");
  const photo =
    (studio === "Photo" && !["Palettes", "Brand"].includes(panel)) ||
    (panel === "Brand" && s.asset === "Photo");
  if (photo) {
    const original = Boolean(s.before);
    const brightness = original
      ? 1
      : 2 ** (number(s, "exposure") * 0.4) +
        (number(s, "shadows") + number(s, "highlights")) / 400 +
        (number(s, "curve") - 50) / 180;
    const style: CSSProperties = {
      filter: original
        ? "none"
        : `brightness(${brightness}) contrast(${(number(s, "contrast") + number(s, "clarity") / 4) / 100}) saturate(${number(s, "saturation") / 100}) sepia(${Math.abs(number(s, "temperature")) / 300}) hue-rotate(${number(s, "hue") + number(s, "temperature") / 4 + number(s, "grading") / 3}deg)`,
      transform: `rotate(${parseInt(String(s.orientation), 10)}deg) scale(${s.photo === "Detail" ? 1.5 : s.photo === "Wide crop" ? 1.1 : 1})`,
    };
    return (
      <div className="sc-photo-preview">
        <img
          src={artwork("photo.webp")}
          alt="Landscape artwork responding to the preview adjustments"
          style={style}
          width="1600"
          height="1000"
          decoding="async"
        />
        <div
          className="sc-vignette"
          style={{ opacity: original ? 0 : number(s, "vignette") / 90 }}
        />
        <svg
          className="sc-grain"
          style={{ opacity: original ? 0 : number(s, "grain") / 150 }}
          aria-hidden="true"
        >
          <filter id={`${id}-grain`}>
            <feTurbulence
              type="fractalNoise"
              baseFrequency="0.85"
              numOctaves="3"
              stitchTiles="stitch"
            />
          </filter>
          <rect width="100%" height="100%" filter={`url(#${id}-grain)`} />
        </svg>
        {original && <span className="sc-before-label">Original</span>}
      </div>
    );
  }

  const pixel = studio === "Pixel" && !["Palettes", "Brand"].includes(panel);
  const motion =
    studio === "Motion" && ["Presets", "Keyframes"].includes(panel);
  let progress = number(s, "progress") / 100;
  if (s.ease === "Ease in") progress **= 2;
  else if (s.ease === "Ease out") progress = 1 - (1 - progress) ** 3;
  else if (s.ease === "Ease in-out")
    progress = progress * progress * (3 - 2 * progress);
  const remaining = motion ? 1 - progress : 0;
  const energy = number(s, "intensity");
  const preset = String(s.preset);
  let dx = number(s, "x"),
    dy = number(s, "y"),
    rotation = number(s, "rotation"),
    scale = number(s, "scale") / 100,
    alpha = number(s, "opacity") / 100;
  if (motion) {
    if (["Slide left", "Slide right", "Fly"].includes(preset))
      dx += remaining * 160 * energy * (preset === "Slide right" ? -1 : 1);
    if (["Slide up", "Slide down", "Slam", "Fly"].includes(preset))
      dy += remaining * 120 * energy * (preset === "Slide up" ? 1 : -1);
    if (["Pop in", "Zoom"].includes(preset))
      scale *=
        preset === "Zoom"
          ? 1 + remaining * energy
          : 1 - remaining + Math.sin(progress * Math.PI) * 0.2 * energy;
    if (["Shake", "Buzz"].includes(preset))
      dx +=
        Math.sin(progress * Math.PI * (preset === "Buzz" ? 18 : 8)) *
        remaining *
        25 *
        energy;
    if (preset === "Fly") rotation -= remaining * 25;
    if (["Fade in", "Slam", "Fly", "Pop in"].includes(preset))
      alpha *= Math.min(1, progress * 2);
  }
  const warped = panel === "Reshape";
  const amount = warped ? number(s, "amount") : 0;
  const markPath =
    warped && s.reshape === "Warp mesh"
      ? `M-78 -30 Q${amount} -135 78 -30 Q${100 + amount} 80 0 86 Q${-100 + amount} 80 -78 -30Z`
      : "M0-88 27-36 84-28 44 14 53 74 0 46-53 74-44 14-84-28-27-36Z";
  const font = String(panel === "Brand" ? s.brandFont : s.font);
  const family =
    font === "Serif"
      ? "Georgia, serif"
      : font === "Mono"
        ? "var(--font-mono)"
        : "var(--font-sans)";
  const text = String(s.text).split("\n").slice(0, 2);
  const skew = warped && s.reshape === "Skew" ? amount : 0;
  const trace = panel === "Trace";

  return (
    <svg
      className="sc-art-preview"
      viewBox="0 0 600 420"
      role="img"
      aria-label={`${studio} sample artwork; ${panel} controls change this preview`}
    >
      <defs>
        <pattern
          id={`${id}-grid`}
          width="24"
          height="24"
          patternUnits="userSpaceOnUse"
        >
          <circle cx="1" cy="1" r="0.7" fill="currentColor" opacity="0.14" />
        </pattern>
        <filter
          id={`${id}-effects`}
          x="-60%"
          y="-60%"
          width="220%"
          height="220%"
        >
          <feGaussianBlur
            stdDeviation={panel === "Effects" ? number(s, "blur") : 0}
          />
          <feDropShadow
            dx="0"
            dy={panel === "Effects" ? number(s, "shadow") : 0}
            stdDeviation={panel === "Effects" ? number(s, "shadow") / 2 : 0}
            floodColor="#11111b"
            floodOpacity="0.3"
          />
        </filter>
        <filter id={`${id}-brush`} x="-30%" y="-80%" width="160%" height="260%">
          <feGaussianBlur stdDeviation={(100 - number(s, "edge")) / 20} />
        </filter>
        <clipPath id={`${id}-mask`}>
          <rect
            x="0"
            y="0"
            width={
              s.mask && panel === "Layer mask"
                ? (600 * number(s, "reveal")) / 100
                : 600
            }
            height="420"
          />
        </clipPath>
        <clipPath id={`${id}-fill`}>
          <rect x="-100" y={100 - progress * 200} width="200" height="200" />
        </clipPath>
      </defs>
      <rect
        width="600"
        height="420"
        fill={s.paperVisible ? "var(--panel-deep)" : "transparent"}
      />
      <rect width="600" height="420" fill={`url(#${id}-grid)`} />
      <g clipPath={`url(#${id}-mask)`} opacity={alpha}>
        {pixel ? (
          <g visibility={s.markVisible ? "visible" : "hidden"}>
            <path
              d="M60 292C126 58 213 105 245 220S346 346 393 186 518 95 548 138"
              fill="none"
              stroke={String(s.fill)}
              strokeWidth={number(s, "size")}
              strokeLinecap="round"
              opacity={number(s, "flow") / 100}
              filter={`url(#${id}-brush)`}
            />
            <path
              d="M81 304C156 358 219 320 279 340S410 350 515 263"
              fill="none"
              stroke={String(s.stroke)}
              strokeWidth={number(s, "size") * 0.65}
              strokeLinecap="round"
              opacity="0.45"
            />
            {s.brush === "Eraser" && (
              <circle
                cx="299"
                cy="251"
                r={number(s, "size")}
                fill="var(--panel-deep)"
              />
            )}
            {s.brush === "Clone" && (
              <path
                d="M144 313C190 190 233 178 264 231"
                fill="none"
                stroke={String(s.fill)}
                strokeWidth={number(s, "size")}
                strokeLinecap="round"
                opacity="0.55"
              />
            )}
            {s.brush === "Heal" && (
              <circle
                cx="245"
                cy="220"
                r={number(s, "size") * 0.6}
                fill={String(s.fill)}
                opacity={number(s, "flow") / 100}
                filter={`url(#${id}-brush)`}
              />
            )}
            {s.brush === "Smudge" && (
              <path
                d={`M232 210 Q${300 + number(s, "flow")} 94 425 180`}
                fill="none"
                stroke={String(s.fill)}
                strokeWidth={number(s, "size")}
                strokeLinecap="round"
                filter={`url(#${id}-brush)`}
              />
            )}
            {["Fill", "Wand"].includes(String(s.brush)) && (
              <rect
                x="92"
                y="100"
                width={100 + number(s, "tolerance")}
                height="200"
                rx="40"
                fill={s.brush === "Fill" ? String(s.fill) : "none"}
                opacity="0.5"
                stroke={String(s.stroke)}
                strokeWidth="2"
                strokeDasharray={s.brush === "Wand" ? "5 5" : undefined}
              />
            )}
            <circle
              cx="420"
              cy="210"
              r={number(s, "size") / 2 + 4}
              fill="none"
              stroke={
                panel === "Layer mask" && s.paintOn === "Pixels"
                  ? String(s.fill)
                  : "var(--ink)"
              }
              strokeWidth="1"
              strokeDasharray="3 3"
            />
          </g>
        ) : (
          <g
            transform={`translate(${300 + dx} ${205 + dy}) rotate(${rotation}) scale(${scale}) skewX(${skew})`}
            filter={`url(#${id}-effects)`}
          >
            {s.markVisible && (
              <g
                transform={`translate(132 -45) ${warped && s.reshape === "Perspective" ? `scale(${1 + amount / 80} 1) rotate(${amount / 4})` : warped && s.reshape === "Distort" ? `skewY(${amount})` : ""}`}
              >
                {trace ? (
                  Array.from(
                    { length: number(s, "traceColors") },
                    (_, index) => (
                      <path
                        key={index}
                        d={markPath}
                        transform={`rotate(${index * 15}) scale(${1 - index * 0.09})`}
                        fill={swatches[index]}
                        stroke={String(s.stroke)}
                        strokeWidth={Math.max(
                          0.5,
                          4 - number(s, "smoothness") / 2,
                        )}
                      />
                    ),
                  )
                ) : (
                  <path
                    d={
                      panel === "Brand" && s.asset === "Poster"
                        ? "M-68-86H68V86H-68Z"
                        : markPath
                    }
                    fill={
                      motion && preset === "Draw stroke"
                        ? "none"
                        : String(s.fill)
                    }
                    stroke={String(s.stroke)}
                    strokeWidth={
                      motion && preset === "Draw stroke"
                        ? 4
                        : number(s, "strokeWidth")
                    }
                    strokeLinejoin="round"
                    pathLength="100"
                    strokeDasharray={
                      motion && preset === "Draw stroke" ? "100" : undefined
                    }
                    strokeDashoffset={
                      motion && preset === "Draw stroke"
                        ? (1 - progress) * 100
                        : undefined
                    }
                    clipPath={
                      motion && preset === "Fill up"
                        ? `url(#${id}-fill)`
                        : undefined
                    }
                  />
                )}
                {warped && (
                  <g stroke="var(--accent)" strokeWidth="1" fill="var(--paper)">
                    <path
                      d="M-100-100H100V100H-100Z M0-100V100 M-100 0H100"
                      fill="none"
                      strokeDasharray="4 4"
                    />
                    {[
                      [-100, -100],
                      [100, -100],
                      [-100, 100],
                      [100, 100],
                    ].map(([x, y]) => (
                      <circle key={`${x}-${y}`} cx={x} cy={y} r="5" />
                    ))}
                  </g>
                )}
              </g>
            )}
            {s.textVisible && (
              <text
                x="-230"
                y="30"
                fill="var(--ink)"
                fontFamily={family}
                fontSize={number(s, "typeSize")}
                fontWeight="500"
                letterSpacing={number(s, "tracking")}
              >
                {text.map((line, index) => (
                  <tspan
                    key={index}
                    x="-230"
                    dy={
                      index === 0
                        ? 0
                        : number(s, "typeSize") * number(s, "leading")
                    }
                  >
                    {s.smallCaps ? line.toUpperCase() : line}
                  </tspan>
                ))}
              </text>
            )}
          </g>
        )}
      </g>
      {panel === "Layout" && (
        <g
          stroke="var(--accent)"
          strokeWidth="1"
          strokeDasharray="4 6"
          opacity="0.7"
        >
          <path d={`M${300 + dx} 0V420M0 ${205 + dy}H600`} />
        </g>
      )}
      {motion && (
        <g>
          <path d="M60 377H540" stroke="var(--line)" strokeWidth="2" />
          {[60, 180, 300, 420, 540].map((x) => (
            <path key={x} d={`M${x} 371l6 6-6 6-6-6Z`} fill="var(--muted)" />
          ))}
          <circle
            cx={60 + number(s, "progress") * 4.8}
            cy="377"
            r="7"
            fill="var(--accent)"
          />
        </g>
      )}
    </svg>
  );
}
