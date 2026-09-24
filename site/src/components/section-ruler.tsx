import { useRouterState } from "@tanstack/react-router";
import { useEffect, useState } from "react";

type Mark = { id: string; label: string };

/**
 * A quiet section ruler for the right edge.
 * Desktop only. Ticks follow the page sections and show their names on hover.
 * @returns The ruler, or nothing when a page has fewer than two sections.
 */
export function SectionRuler() {
  const href = useRouterState({ select: state => state.location.href });
  const [marks, setMarks] = useState<Mark[]>([]);
  const [active, setActive] = useState("");

  useEffect(() => {
    const collect = () => {
      const main = document.getElementById("main");
      if (!main) {
        setMarks([]);
        return;
      }
      const sections = [...main.querySelectorAll<HTMLElement>("section[id]")];
      const headings = [...main.querySelectorAll<HTMLElement>("h2[id]")].filter(heading => !heading.closest("section[id]"));
      const next = [...sections, ...headings].flatMap(element => {
        const heading = element.matches("h1, h2, h3") ? element : element.querySelector("h1, h2, h3");
        const label = (heading?.textContent || element.getAttribute("aria-label") || element.id).replace(/\s+/g, " ").trim();
        return label ? [{ id: element.id, label: label.length > 48 ? `${label.slice(0, 47)}…` : label }] : [];
      });
      const seen = new Set<string>();
      const unique: Mark[] = [];
      for (const mark of next) {
        if (seen.has(mark.id)) continue;
        seen.add(mark.id);
        unique.push(mark);
      }
      setMarks(unique);
    };
    collect();
    const timer = window.setTimeout(collect, 300);
    return () => window.clearTimeout(timer);
  }, [href]);

  useEffect(() => {
    if (marks.length < 2) return;
    let frame = 0;
    const update = () => {
      frame = 0;
      const line = window.scrollY + window.innerHeight * 0.35;
      let current = marks[0].id;
      for (const mark of marks) {
        const element = document.getElementById(mark.id);
        if (!element) continue;
        if (element.getBoundingClientRect().top + window.scrollY <= line) current = mark.id;
      }
      setActive(current);
    };
    const onScroll = () => {
      if (!frame) frame = window.requestAnimationFrame(update);
    };
    update();
    window.addEventListener("scroll", onScroll, { passive: true });
    window.addEventListener("resize", onScroll);
    return () => {
      window.removeEventListener("scroll", onScroll);
      window.removeEventListener("resize", onScroll);
      if (frame) window.cancelAnimationFrame(frame);
    };
  }, [marks]);

  if (marks.length < 2) return null;
  return (
    <nav className="section-ruler" aria-label="On this page">
      {marks.map(mark => (
        <a
          key={mark.id}
          href={`#${mark.id}`}
          aria-current={mark.id === active ? "true" : undefined}
          onClick={event => {
            const element = document.getElementById(mark.id);
            if (!element) return;
            event.preventDefault();
            element.scrollIntoView({ behavior: "smooth", block: "start" });
            history.replaceState(null, "", `#${mark.id}`);
          }}
        >
          <span>{mark.label}</span>
        </a>
      ))}
    </nav>
  );
}
