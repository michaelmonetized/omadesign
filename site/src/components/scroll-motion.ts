import { useEffect, useRef } from "react";

const targets = [
  ":scope > section",
  ":scope > .btw-strip",
  "[data-motion]",
  ".section-heading",
  ".principles > article",
  ".templates-copy",
  ".template-visual",
  ".film-section video",
  ".feature-controls",
  ".feature-groups > details",
  ".feature-grid > article",
  ".faq-list > details",
].join(",");

/** One scroll listener, one frame, and only visible surfaces participate. */
export function useScrollMotion() {
  const root = useRef<HTMLElement>(null);
  useEffect(() => {
    const page = root.current;
    if (!page) return;
    const preference = matchMedia("(prefers-reduced-motion: reduce)");
    const active = new Set<HTMLElement>();
    const known = new Set<HTMLElement>();
    let frame = 0;
    let scanFrame = 0;
    const clamp = (value: number) => Math.max(0, Math.min(1, value));

    function render() {
      frame = 0;
      if (preference.matches) return;
      const height = window.innerHeight;
      // Read first, then write: panel interaction does not cause layout thrashing.
      const states = [...active].map((element) => {
        // Layout offsets exclude our own and ancestor animations. Reading the
        // transformed rectangle would feed the previous frame into the next.
        let documentTop = 0;
        for (
          let parent: HTMLElement | null = element;
          parent;
          parent = parent.offsetParent as HTMLElement | null
        ) {
          documentTop += parent.offsetTop;
        }
        const top = documentTop - window.scrollY;
        const size = element.offsetHeight;
        const bottom = top + size;
        const entrance = clamp((height - top) / Math.min(180, size * 0.6 + 40));
        const exit = clamp(bottom / Math.min(140, size * 0.5 + 30));
        const progress = clamp((height - top) / (height + size));
        const focused = element.matches(":focus-within");
        const visibility = focused ? 1 : Math.min(entrance, exit);
        const drift = (progress - 0.5) * -8;
        return {
          element,
          visibility,
          scale: 0.987 + Math.min(entrance, exit) * 0.013,
          y: (1 - entrance) * 30 - (1 - exit) * 20 + drift,
          progress,
          state: top >= height ? "before" : bottom <= 0 ? "after" : "inside",
        };
      });
      for (const { element, visibility, scale, y, progress, state } of states) {
        element.style.setProperty(
          "--motion-opacity",
          String(0.08 + visibility * 0.92),
        );
        element.style.setProperty("--motion-y", `${y.toFixed(2)}px`);
        element.style.setProperty("--motion-scale", String(scale));
        element.style.setProperty("--motion-progress", progress.toFixed(3));
        element.dataset.motionState = state;
      }
      page?.classList.add("motion-ready");
    }
    function schedule() {
      if (!frame && !preference.matches) frame = requestAnimationFrame(render);
    }
    const observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          const element = entry.target as HTMLElement;
          if (entry.isIntersecting) {
            active.add(element);
            element.dataset.motionActive = "true";
          } else {
            active.delete(element);
            delete element.dataset.motionActive;
            element.dataset.motionState =
              entry.boundingClientRect.top > 0 ? "before" : "after";
          }
        }
        schedule();
      },
      { rootMargin: "100px 0px" },
    );
    function scan() {
      scanFrame = 0;
      for (const element of page!.querySelectorAll<HTMLElement>(targets)) {
        if (!known.has(element)) {
          known.add(element);
          element.dataset.scrollMotion = "";
          observer.observe(element);
        }
      }
      for (const element of known) {
        if (!page!.contains(element)) {
          observer.unobserve(element);
          known.delete(element);
          active.delete(element);
        }
      }
      schedule();
    }
    function scheduleScan() {
      if (!scanFrame) scanFrame = requestAnimationFrame(scan);
    }
    function changePreference() {
      page!.classList.toggle("motion-ready", !preference.matches);
      if (preference.matches) {
        cancelAnimationFrame(frame);
        frame = 0;
      } else schedule();
    }
    const mutations = new MutationObserver(scheduleScan);
    mutations.observe(page, { childList: true, subtree: true });
    const resize = new ResizeObserver(schedule);
    resize.observe(page);
    scan();
    window.addEventListener("scroll", schedule, { passive: true });
    window.addEventListener("resize", schedule, { passive: true });
    page.addEventListener("focusin", schedule);
    page.addEventListener("focusout", schedule);
    preference.addEventListener("change", changePreference);
    return () => {
      cancelAnimationFrame(frame);
      cancelAnimationFrame(scanFrame);
      observer.disconnect();
      mutations.disconnect();
      resize.disconnect();
      window.removeEventListener("scroll", schedule);
      window.removeEventListener("resize", schedule);
      page.removeEventListener("focusin", schedule);
      page.removeEventListener("focusout", schedule);
      preference.removeEventListener("change", changePreference);
      page.classList.remove("motion-ready");
      for (const element of known) {
        delete element.dataset.scrollMotion;
        delete element.dataset.motionActive;
        delete element.dataset.motionState;
      }
    };
  }, []);
  return root;
}
