import { useEffect, useRef, useState, type PointerEvent } from "react";
import type { Doc } from "../../convex/_generated/dataModel";

export type Geometry = Pick<
  Doc<"cloudAnnotations">,
  | "shape"
  | "x"
  | "y"
  | "endX"
  | "endY"
  | "stamp"
  | "points"
  | "strokeWidth"
  | "color"
  | "opacity"
>;
export const stamps = ["check", "x", "heart", "question", "exclaim"] as const;
export const stampNames = {
  check: "Approve",
  x: "Problem",
  heart: "Like",
  question: "Question",
  exclaim: "Note",
};
type Tool = Geometry["shape"];
type Point = { x: number; y: number };
export function appendPoint(points: Point[], point: Point): Point[] {
  const last = points[points.length - 1];
  if (last && Math.hypot(last.x - point.x, last.y - point.y) < 0.0005)
    return points;
  // Keep both endpoints when reducing long strokes, so a held pointer never loses its tail.
  const reduced =
    points.length >= 511
      ? points.filter((_, i) => i === 0 || i % 2 === 1)
      : points;
  return [...reduced, point];
}
const paths = {
  stamp:
    "M12 20s-8-4.5-8-10a4.5 4.5 0 0 1 8-3 4.5 4.5 0 0 1 8 3c0 5.5-8 10-8 10Z",
  rectangle: "M4 5h16v14H4Z",
  highlight: "m5 15 9-11 6 5-9 11-6-5Zm2 2-3 4h8m0-14 6 5",
  brush: "m9 14 9-11 3 3-10 10M9 14c-4-1-4 4-6 6 6 1 9-2 8-4Z",
  pin: "M4 4h16v13H9l-5 4V4Zm4 5h8m-8 4h5",
};
function Icon({ tool }: { tool: Tool }) {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <path
        d={paths[tool]}
        fill="none"
        stroke="currentColor"
        strokeWidth="1.6"
        strokeLinejoin="round"
        strokeLinecap="round"
      />
    </svg>
  );
}
export function ReviewCanvas({
  url,
  name,
  width,
  height,
  annotations,
  selected,
  onSelect,
  draft,
  onDraft,
  onCreate,
  disabled = false,
}: {
  url: string;
  name: string;
  width: number;
  height: number;
  annotations: Doc<"cloudAnnotations">[];
  selected?: string;
  onSelect: (id: Doc<"cloudAnnotations">["_id"]) => void;
  draft?: Geometry;
  onDraft: (draft: Geometry | undefined) => void;
  onCreate: (geometry: Geometry) => void;
  disabled?: boolean;
}) {
  const [tool, setTool] = useState<Tool>("stamp");
  const [stamp, setStamp] = useState<NonNullable<Geometry["stamp"]>>("heart");
  const [hideResolved, setHideResolved] = useState(false);
  const [color, setColor] = useState("#facc15");
  const [brushColor, setBrushColor] = useState("#b784ff");
  const [thickness, setThickness] = useState(0.012);
  const drawing = useRef<{ id: number; geometry: Geometry } | null>(null);
  const canvas = useRef<HTMLDivElement>(null);
  const size = Math.min(width, height);
  const [displaySize, setDisplaySize] = useState(size);
  useEffect(() => {
    const element = canvas.current;
    if (!element) return;
    const observer = new ResizeObserver(([entry]) => setDisplaySize(Math.min(entry.contentRect.width, entry.contentRect.height)));
    observer.observe(element);
    return () => observer.disconnect();
  }, []);
  const stampSize = Math.max(size * 0.12, 42 * size / Math.max(1, displaySize));
  const pinRadius = Math.max(size * 0.016, 13 * size / Math.max(1, displaySize));
  function cancel() {
    drawing.current = null;
    onDraft(undefined);
  }
  function choose(next: Tool) {
    cancel();
    setTool(next);
  }
  function point(e: PointerEvent<HTMLDivElement>): Point {
    const r = e.currentTarget.getBoundingClientRect();
    return {
      x: Math.max(0, Math.min(1, (e.clientX - r.left) / r.width)),
      y: Math.max(0, Math.min(1, (e.clientY - r.top) / r.height)),
    };
  }
  function advance(e: PointerEvent<HTMLDivElement>) {
    const current = drawing.current;
    if (!current || current.id !== e.pointerId) return;
    const p = point(e),
      g = current.geometry;
    current.geometry =
      g.shape === "rectangle"
        ? { ...g, endX: p.x, endY: p.y }
        : g.points
          ? { ...g, points: appendPoint(g.points, p) }
          : g;
    onDraft(current.geometry);
  }
  function mark(
    g: Geometry,
    index: number,
    id?: Doc<"cloudAnnotations">["_id"],
    resolved = false,
  ) {
    const active = id === selected,
      x = g.x * width,
      y = g.y * height;
    const label = `${index + 1}: ${g.shape === "stamp" ? stampNames[g.stamp!] : g.shape} annotation`;
    return (
      <g
        key={id || "draft"}
        className={`review-mark ${active ? "selected" : ""}`}
        opacity={resolved ? 0.4 : 1}
        role={id ? "button" : undefined}
        tabIndex={id ? 0 : undefined}
        aria-label={id ? label : undefined}
        data-annotation={id}
        onClick={id ? () => onSelect(id) : undefined}
        onKeyDown={
          id
            ? (e) => {
                if (e.key === "Enter" || e.key === " ") {
                  e.preventDefault();
                  e.stopPropagation();
                  onSelect(id);
                }
              }
            : undefined
        }
        style={{ pointerEvents: id ? "auto" : "none" }}
      >
        {g.shape === "stamp" ? (
          <image
            href={`/media/review/${g.stamp}.svg`}
            x={x - stampSize * (0.028 / 0.075)}
            y={y - stampSize * (0.068 / 0.075)}
            width={stampSize}
            height={stampSize}
          />
        ) : g.points ? (
          <>
            <polyline
              points={g.points
                .map((p) => `${p.x * width},${p.y * height}`)
                .join(" ")}
              fill="none"
              stroke={g.color}
              strokeOpacity={g.opacity}
              strokeWidth={(g.strokeWidth || 0.004) * size}
              strokeLinecap="round"
              strokeLinejoin="round"
            />
            <polyline
              className="review-stroke-hit"
              points={g.points
                .map((p) => `${p.x * width},${p.y * height}`)
                .join(" ")}
              fill="none"
              stroke="transparent"
              strokeWidth={Math.max((g.strokeWidth || 0.004) * size, 16)}
            />
          </>
        ) : g.shape === "rectangle" ? (
          <rect
            x={Math.min(g.x, g.endX ?? g.x) * width}
            y={Math.min(g.y, g.endY ?? g.y) * height}
            width={Math.abs((g.endX ?? g.x) - g.x) * width}
            height={Math.abs((g.endY ?? g.y) - g.y) * height}
            fill="#d5ff7218"
            stroke="#d5ff72"
            strokeWidth="2"
            vectorEffect="non-scaling-stroke"
          />
        ) : null}
        {(g.shape === "pin" || g.shape === "rectangle" || active) && (
          <g transform={`translate(${x} ${y})`}>
            <circle
              r={pinRadius}
              fill="#d5ff72"
              stroke="#292134"
              strokeWidth={size * 0.002}
            />
            <text
              textAnchor="middle"
              dominantBaseline="central"
              fontSize={pinRadius * 1.1}
              fill="#171220"
            >
              {id ? index + 1 : "+"}
            </text>
          </g>
        )}
      </g>
    );
  }
  return (
    <section
      className="review-workspace"
      aria-label="Snapshot annotation tools"
      onKeyDown={(e) => {
        if ((e.target as HTMLElement).closest("input,textarea,select")) return;
        if (e.key === "Escape") {
          e.preventDefault();
          cancel();
        }
        if (tool === "stamp" && /^[1-5]$/.test(e.key)) {
          e.preventDefault();
          setStamp(stamps[Number(e.key) - 1]);
        }
      }}
    >
      <div className="review-rail" role="toolbar" aria-label="Review tools">
        <a href="/" className="review-brand" aria-label="Omadesign home">
          <span aria-hidden="true">a</span>
        </a>
        {(["stamp", "rectangle", "highlight", "brush", "pin"] as const).map(
          (t) => (
            <button
              key={t}
              type="button"
              title={
                t === "pin"
                  ? "Comment"
                  : t === "rectangle"
                    ? "Rectangle"
                    : t === "highlight"
                      ? "Highlighter"
                      : t === "brush"
                        ? "Paintbrush"
                        : "Stamps"
              }
              aria-label={
                t === "pin"
                  ? "Comment"
                  : t === "rectangle"
                    ? "Rectangle"
                    : t === "highlight"
                      ? "Highlighter"
                      : t === "brush"
                        ? "Paintbrush"
                        : "Stamps"
              }
              aria-pressed={tool === t}
              onClick={() => choose(t)}
            >
              {t === "stamp" ? <img className="review-stamp-tool" src={`/media/review/${stamp}.svg`} alt="" /> : <Icon tool={t} />}
            </button>
          ),
        )}
        {tool === "stamp" && (
          <div
            className="review-flyout"
            role="toolbar"
            aria-label="Stamp reactions"
          >
            {stamps.map((s, i) => (
              <button
                type="button"
                key={s}
                title={`${stampNames[s]} (${i + 1})`}
                aria-label={stampNames[s]}
                aria-pressed={stamp === s}
                onClick={() => {
                  cancel();
                  setStamp(s);
                }}
              >
                <img src={`/media/review/${s}.svg`} alt="" />
              </button>
            ))}
          </div>
        )}
        {(tool === "brush" || tool === "highlight") && (
          <div className="review-ink-options">
            <label>
              Color
              <input
                type="color"
                aria-label="Stroke color"
                value={tool === "brush" ? brushColor : color}
                onChange={(e) =>
                  tool === "brush"
                    ? setBrushColor(e.target.value)
                    : setColor(e.target.value)
                }
              />
            </label>
            <label>
              Width
              <input
                aria-label="Stroke width"
                type="range"
                min="0.002"
                max="0.04"
                step="0.002"
                value={thickness}
                onChange={(e) => setThickness(Number(e.target.value))}
              />
            </label>
          </div>
        )}
      </div>
      <div
        ref={canvas}
        className="cloud-review-image"
        tabIndex={0}
        aria-label={`${name}: ${tool} tool`}
        style={{ aspectRatio: `${width}/${height}` }}
        onPointerDown={(e) => {
          if (
            disabled ||
            !url ||
            e.button !== 0 ||
            drawing.current ||
            (e.target as Element).closest("[data-annotation]")
          )
            return;
          e.currentTarget.focus();
          e.currentTarget.setPointerCapture(e.pointerId);
          const p = point(e);
          const geometry: Geometry = {
            shape: tool,
            ...p,
            ...(tool === "stamp"
              ? { stamp }
              : tool === "rectangle"
                ? { endX: p.x, endY: p.y }
                : tool === "highlight" || tool === "brush"
                  ? {
                      points: [p],
                      strokeWidth: tool === "brush" ? Math.max(0.001, thickness / 3) : thickness,
                      color: tool === "brush" ? brushColor : color,
                      opacity: tool === "brush" ? 1 : 0.3,
                    }
                  : {}),
          };
          drawing.current = { id: e.pointerId, geometry };
          onDraft(geometry);
        }}
        onPointerMove={advance}
        onPointerUp={(e) => {
          advance(e);
          const g = drawing.current;
          if (!g || g.id !== e.pointerId) return;
          drawing.current = null;
          if (
            (g.geometry.points && g.geometry.points.length < 2) ||
            (g.geometry.shape === "rectangle" &&
              (g.geometry.x === g.geometry.endX ||
                g.geometry.y === g.geometry.endY))
          )
            onDraft(undefined);
          else if (g.geometry.shape === "stamp" || g.geometry.points)
            onCreate(g.geometry);
          e.currentTarget.releasePointerCapture(e.pointerId);
        }}
        onPointerCancel={cancel}
        onLostPointerCapture={() => {
          if (drawing.current) cancel();
        }}
      >
        {url ? (
          <img src={url} alt={`Review export: ${name}`} draggable={false} />
        ) : (
          <p>Loading export…</p>
        )}
        <svg
          className="review-overlay"
          viewBox={`0 0 ${width} ${height}`}
          aria-label="Annotations"
        >
          {annotations.map((a, i) =>
            hideResolved && a.resolved ? null : mark(a, i, a._id, a.resolved),
          )}
          {draft && mark(draft, annotations.length)}
        </svg>
      </div>
      <div className="review-canvas-options">
        <span>
          {tool === "stamp"
            ? `${stampNames[stamp]} · click to place`
            : tool === "pin"
              ? "Click to comment"
              : "Drag to mark"}{" "}
          · Esc cancels
        </span>
        <label>
          <input
            type="checkbox"
            checked={hideResolved}
            onChange={(e) => setHideResolved(e.target.checked)}
          />{" "}
          Hide resolved
        </label>
      </div>
    </section>
  );
}
