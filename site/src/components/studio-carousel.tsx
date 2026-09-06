import { useEffect, useId, useRef, useState } from "react";
import type { KeyboardEvent } from "react";
import { sitePath } from "../site";
import "./studio-carousel.css";

export type StudioName = "Design" | "Pixel" | "Photo" | "Motion";

type StudioCarouselProps = {
  selected?: StudioName;
  onSelect?: (studio: StudioName) => void;
};

const works: { name: StudioName; image: string; alt: string }[] = [
  {
    name: "Design",
    image: "design.webp",
    alt: "Block Party: an original coral and citron poster made with editable vectors in omadesign.",
  },
  {
    name: "Pixel",
    image: "pixel.webp",
    alt: "Iris study: textured purple iris illustration prepared in Pixel studio.",
  },
  {
    name: "Photo",
    image: "photo.webp",
    alt: "Coast road: original landscape artwork color-graded in Photo studio.",
  },
  {
    name: "Motion",
    image: "motion.webp",
    alt: "After Hours: cream typography and concentric rings made as editable animated artwork in omadesign.",
  },
];
const count = works.length;
const wrap = (index: number) => ((index % count) + count) % count;
const nearestTurn = (index: number, position: number) =>
  index + Math.round((position - index) / count) * count;
const clamp = (value: number, min: number, max: number) =>
  Math.max(min, Math.min(max, value));

type OrbitController = {
  select: (index: number) => void;
  pause: (value: boolean) => void;
};
type Drag = {
  id: number;
  x: number;
  y: number;
  origin: number;
  last: number;
  time: number;
  moving: boolean;
};

/** Four DOM planes travel through actual X/Y/Z space; frames never rerender React. */
export function StudioCarousel({
  selected,
  onSelect,
}: StudioCarouselProps = {}) {
  const [localSelection, setLocalSelection] = useState<StudioName>("Design");
  const [paused, setPaused] = useState(false);
  const name = selected ?? localSelection;
  const selectedIndex = works.findIndex((work) => work.name === name);
  const id = useId();
  const stage = useRef<HTMLDivElement>(null);
  const world = useRef<HTMLDivElement>(null);
  const cards = useRef<(HTMLButtonElement | null)[]>([]);
  const tabs = useRef<(HTMLButtonElement | null)[]>([]);
  const controls = useRef<OrbitController | null>(null);
  const active = useRef(selectedIndex);
  active.current = selectedIndex;
  const selectionHandler = useRef((index: number) => {});
  selectionHandler.current = (index) => {
    const next = wrap(index);
    if (next === active.current) return;
    if (selected === undefined) setLocalSelection(works[next].name);
    onSelect?.(works[next].name);
  };

  useEffect(() => {
    const surface = stage.current;
    const scene = world.current;
    if (!surface || !scene) return;
    const preference = matchMedia("(prefers-reduced-motion: reduce)");
    let reduced = preference.matches;
    let visible = false;
    let stopped = false;
    let width = surface.clientWidth;
    let height = surface.clientHeight;
    let position = active.current;
    let target = position;
    let velocity = 0;
    let phase = 0;
    let frame = 0;
    let lastTime = 0;
    let drag: Drag | null = null;
    let suppressClick = false;
    let pointerX = 0;
    let pointerY = 0;
    let cameraX = 0;
    let cameraY = 0;

    function paint() {
      const narrow = width < 650;
      const drift = reduced ? 0 : Math.sin(phase * 0.28) * 0.038;
      scene!.style.transform = `rotateX(${cameraY * -5}deg) rotateY(${cameraX * 8}deg) translate3d(${cameraX * -22}px,${cameraY * -12}px,0)`;
      cards.current.forEach((card, index) => {
        if (!card) return;
        const angle = ((index - position - drift) * Math.PI) / 2;
        const cosine = Math.cos(angle);
        const sine = Math.sin(angle);
        const depth = (1 - cosine) * 0.5;
        const float = reduced
          ? 0
          : Math.sin(phase * 0.65 + index * 1.7) * (narrow ? 8 : 17);
        const x = sine * width * (narrow ? 0.88 : 0.74);
        const y =
          -(1 - cosine) * height * (narrow ? 0.34 : 0.48) +
          sine * height * 0.15 +
          float;
        const z = (cosine - 1) * width * (narrow ? 0.8 : 0.76);
        const yaw = -sine * 34;
        const pitch = cosine * -5;
        const roll = sine * 11 + [-4, 5, -3, 4][index];
        card.style.transform = `translate(-50%,-50%) translate3d(${x.toFixed(2)}px,${y.toFixed(2)}px,${z.toFixed(2)}px) rotateY(${yaw.toFixed(2)}deg) rotateX(${pitch.toFixed(2)}deg) rotateZ(${roll.toFixed(2)}deg)`;
        card.style.opacity = String(1 - depth * 0.34);
      });
      surface!.dataset.orbit = position.toFixed(3);
    }

    function tick(now: number) {
      frame = 0;
      if (!visible || document.hidden || reduced) return;
      const dt = Math.min(lastTime ? (now - lastTime) / 1000 : 1 / 60, 0.035);
      lastTime = now;
      if (!stopped) phase += dt;
      if (!drag?.moving) {
        velocity += ((target - position) * 58 - velocity * 14) * dt;
        position += velocity * dt;
        if (
          Math.abs(target - position) < 0.0002 &&
          Math.abs(velocity) < 0.002
        ) {
          position = target;
          velocity = 0;
        }
      }
      const response = 1 - Math.exp(-dt * 5);
      cameraX += (pointerX - cameraX) * response;
      cameraY += (pointerY - cameraY) * response;
      paint();
      if (
        !stopped ||
        drag?.moving ||
        Math.abs(target - position) > 0.0002 ||
        Math.abs(velocity) > 0.002 ||
        Math.abs(pointerX - cameraX) + Math.abs(pointerY - cameraY) > 0.001
      )
        schedule();
    }

    function schedule() {
      if (!frame && visible && !document.hidden && !reduced)
        frame = requestAnimationFrame(tick);
    }

    function select(index: number) {
      drag = null;
      surface!.dataset.dragging = "false";
      target = nearestTurn(wrap(index), position);
      if (reduced || !visible) {
        position = target;
        velocity = 0;
        paint();
      } else schedule();
    }

    function down(event: PointerEvent) {
      if (!event.isPrimary || event.button !== 0 || reduced) return;
      suppressClick = false;
      drag = {
        id: event.pointerId,
        x: event.clientX,
        y: event.clientY,
        origin: position,
        last: position,
        time: event.timeStamp,
        moving: false,
      };
    }

    function move(event: PointerEvent) {
      if (reduced) return;
      if (event.pointerType === "mouse" && !stopped) {
        const rect = surface!.getBoundingClientRect();
        pointerX = clamp(
          ((event.clientX - rect.left) / rect.width) * 2 - 1,
          -1,
          1,
        );
        pointerY = clamp(
          ((event.clientY - rect.top) / rect.height) * 2 - 1,
          -1,
          1,
        );
      }
      if (drag?.id === event.pointerId) {
        const dx = event.clientX - drag.x;
        const dy = event.clientY - drag.y;
        if (
          !drag.moving &&
          Math.abs(dy) > 10 &&
          Math.abs(dy) > Math.abs(dx) * 1.2
        ) {
          drag = null;
          return;
        }
        if (
          !drag.moving &&
          Math.abs(dx) > 6 &&
          Math.abs(dx) > Math.abs(dy) * 1.15
        ) {
          drag.moving = true;
          velocity = 0;
          surface!.setPointerCapture(event.pointerId);
          surface!.dataset.dragging = "true";
        }
        if (drag.moving) {
          event.preventDefault();
          position = drag.origin - dx / Math.max(180, width * 0.38);
          const dt = Math.max(8, event.timeStamp - drag.time) / 1000;
          const sample = clamp((position - drag.last) / dt, -6, 6);
          velocity = velocity * 0.55 + sample * 0.45;
          drag.last = position;
          drag.time = event.timeStamp;
          target = position;
        }
      }
      schedule();
    }

    function release(event: PointerEvent) {
      if (!drag || drag.id !== event.pointerId) return;
      const moving = drag.moving;
      drag = null;
      surface!.dataset.dragging = "false";
      if (!moving) return;
      suppressClick = true;
      if (event.type === "pointercancel") velocity = 0;
      target = Math.round(position + velocity * 0.17);
      selectionHandler.current(wrap(target));
      schedule();
    }

    function leave() {
      pointerX = 0;
      pointerY = 0;
      schedule();
    }

    function click(event: MouseEvent) {
      if (!suppressClick) return;
      suppressClick = false;
      event.preventDefault();
      event.stopPropagation();
    }

    function refreshPreference() {
      reduced = preference.matches;
      if (drag && surface!.hasPointerCapture(drag.id))
        surface!.releasePointerCapture(drag.id);
      drag = null;
      surface!.dataset.dragging = "false";
      velocity = 0;
      position = target = nearestTurn(active.current, position);
      pointerX = pointerY = cameraX = cameraY = 0;
      cancelAnimationFrame(frame);
      frame = 0;
      lastTime = 0;
      paint();
      schedule();
    }

    function visibility() {
      if (document.hidden) {
        cancelAnimationFrame(frame);
        frame = 0;
        drag = null;
        velocity = 0;
        position = target = nearestTurn(active.current, position);
        paint();
        surface!.dataset.dragging = "false";
      } else {
        lastTime = 0;
        schedule();
      }
    }

    controls.current = {
      select,
      pause(value) {
        stopped = value;
        pointerX = pointerY = 0;
        paint();
        schedule();
      },
    };
    const resize = new ResizeObserver(() => {
      width = surface.clientWidth;
      height = surface.clientHeight;
      paint();
      schedule();
    });
    resize.observe(surface);
    const intersection = new IntersectionObserver(([entry]) => {
      visible = entry.isIntersecting;
      if (visible) {
        lastTime = 0;
        schedule();
      } else {
        cancelAnimationFrame(frame);
        frame = 0;
        if (!drag?.moving) {
          position = target;
          velocity = 0;
          paint();
        }
      }
    });
    intersection.observe(surface);
    surface.addEventListener("pointerdown", down);
    surface.addEventListener("pointermove", move, { passive: false });
    surface.addEventListener("pointerup", release);
    surface.addEventListener("pointercancel", release);
    surface.addEventListener("pointerleave", leave);
    surface.addEventListener("click", click, true);
    preference.addEventListener("change", refreshPreference);
    document.addEventListener("visibilitychange", visibility);
    paint();
    return () => {
      cancelAnimationFrame(frame);
      resize.disconnect();
      intersection.disconnect();
      surface.removeEventListener("pointerdown", down);
      surface.removeEventListener("pointermove", move);
      surface.removeEventListener("pointerup", release);
      surface.removeEventListener("pointercancel", release);
      surface.removeEventListener("pointerleave", leave);
      surface.removeEventListener("click", click, true);
      preference.removeEventListener("change", refreshPreference);
      document.removeEventListener("visibilitychange", visibility);
      controls.current = null;
    };
  }, []);

  useEffect(() => {
    controls.current?.select(selectedIndex);
  }, [selectedIndex]);
  useEffect(() => {
    controls.current?.pause(paused);
  }, [paused]);

  function choose(index: number) {
    const next = wrap(index);
    controls.current?.select(next);
    selectionHandler.current(next);
  }

  function keydown(event: KeyboardEvent<HTMLElement>, focusTab = false) {
    const next =
      event.key === "ArrowRight"
        ? wrap(selectedIndex + 1)
        : event.key === "ArrowLeft"
          ? wrap(selectedIndex - 1)
          : event.key === "Home"
            ? 0
            : event.key === "End"
              ? count - 1
              : null;
    if (next === null) return;
    event.preventDefault();
    choose(next);
    if (focusTab) tabs.current[next]?.focus();
  }

  return (
    <section
      className="studio-carousel"
      id="studios"
      aria-label="Explore the studios"
    >
      <div
        className="orb-stage"
        ref={stage}
        tabIndex={0}
        role="region"
        aria-roledescription="carousel"
        aria-label="Three-dimensional artwork gallery"
        aria-describedby={`${id}-hint`}
        onKeyDown={(event) => keydown(event)}
      >
        <div className="orb-world" ref={world}>
          {works.map((work, index) => (
            <button
              key={work.name}
              ref={(element) => {
                cards.current[index] = element;
              }}
              className="orb-artwork"
              type="button"
              tabIndex={-1}
              data-work={index}
              aria-label={`Select ${work.name}`}
              aria-pressed={name === work.name}
              onClick={() => choose(index)}
            >
              <img
                src={sitePath(`media/showcase/${work.image}`)}
                alt={work.alt}
                width="1600"
                height="1000"
                decoding="async"
                loading={index === 0 ? "eager" : "lazy"}
                draggable={false}
              />
              <span className="orb-artwork-name">{work.name}</span>
            </button>
          ))}
        </div>
        <span className="orb-drag-hint" id={`${id}-hint`}>
          Drag to explore · arrow keys to navigate
        </span>
      </div>
      <div className="orb-controls">
        <div className="orb-caption">
          <button
            type="button"
            className="orb-arrow"
            aria-label="Previous studio"
            onClick={() => choose(selectedIndex - 1)}
          >
            ←
          </button>
          <h2 aria-live="polite" aria-atomic="true">
            {name}
          </h2>
          <button
            type="button"
            className="orb-arrow"
            aria-label="Next studio"
            onClick={() => choose(selectedIndex + 1)}
          >
            →
          </button>
        </div>
        <div
          className="orb-nav"
          aria-label="Studios"
          onKeyDown={(event) => keydown(event, true)}
        >
          {works.map((work, index) => (
            <button
              key={work.name}
              ref={(element) => {
                tabs.current[index] = element;
              }}
              type="button"
              aria-pressed={name === work.name}
              onClick={() => choose(index)}
            >
              {work.name}
            </button>
          ))}
        </div>
        <button
          type="button"
          className="orb-motion-toggle"
          aria-pressed={paused}
          onClick={() => setPaused(!paused)}
        >
          <span aria-hidden="true">{paused ? "▷" : "Ⅱ"}</span>
          {paused ? "Resume motion" : "Pause motion"}
        </button>
      </div>
      <noscript>
        <style>{`.studio-carousel .orb-stage{height:auto;min-height:0;max-height:none;overflow:visible;perspective:none;padding:24px}.studio-carousel .orb-world{position:static;display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:24px;transform:none}.studio-carousel .orb-artwork{position:static;width:100%;transform:none;opacity:1}.studio-carousel .orb-artwork-name{display:block}.studio-carousel .orb-controls,.studio-carousel .orb-drag-hint{display:none}@media(max-width:600px){.studio-carousel .orb-world{grid-template-columns:1fr}}`}</style>
      </noscript>
    </section>
  );
}
