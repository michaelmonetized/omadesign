import { useEffect, useRef } from "react";

const surfaces = [
  "[data-reveal]",
  "[data-motion]",
  ".hero-install",
  ".principles > article",
  ".template-visual",
  ".templates-copy > p",
  ".film-section video",
  ".feature-controls",
  ".feature-groups > details",
  ".feature-grid > article",
  ".faq-list > details",
  ".file-cards > article",
].join(",");

/** Timed reveals follow the viewport; scroll only drives the scene exit. */
export function useScrollMotion() {
  const root = useRef<HTMLElement>(null);
  useEffect(() => {
    const page = root.current;
    if (!page) return;
    const preference = matchMedia("(prefers-reduced-motion: reduce)");
    const known = new Set<HTMLElement>();
    const scenes = new Set<HTMLElement>();
    const active = new Set<HTMLElement>();
    const pressed = new Set<HTMLElement>();
    let frame = 0;
    let scanFrame = 0;
    let releaseFrame = 0;
    const clamp = (value: number) => Math.max(0, Math.min(1, value));
    function render() {
      frame = 0;
      if (preference.matches) return;
      const height = innerHeight;
      const states = [...active].map((element) => {
        let documentTop = 0;
        for (
          let parent: HTMLElement | null = element;
          parent;
          parent = parent.offsetParent as HTMLElement | null
        )
          documentTop += parent.offsetTop;
        const top = documentTop - scrollY;
        const bottom = top + element.offsetHeight;
        const exit = clamp((height * 0.25 - bottom) / (height * 0.25));
        return {
          element,
          exit,
          progress: clamp((height - top) / (height + element.offsetHeight)),
        };
      });
      for (const { element, exit, progress } of states) {
        element.style.setProperty("--scene-exit", exit.toFixed(3));
        element.style.setProperty("--scene-progress", progress.toFixed(3));
      }
    }
    function schedule() {
      if (!frame && !preference.matches) frame = requestAnimationFrame(render);
    }
    const entrances = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          const element = entry.target as HTMLElement;
          if (entry.isIntersecting) element.dataset.revealState = "entered";
          else if (entry.boundingClientRect.bottom <= 0)
            element.dataset.revealState = "past";
          else if (entry.boundingClientRect.top >= innerHeight)
            element.dataset.revealState = "waiting";
        }
      },
      { rootMargin: "0px 0px -16% 0px" },
    );
    const viewport = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          const element = entry.target as HTMLElement;
          if (entry.isIntersecting) {
            active.add(element);
            element.dataset.sceneActive = "true";
          } else {
            active.delete(element);
            delete element.dataset.sceneActive;
          }
        }
        schedule();
      },
      { rootMargin: "100px 0px" },
    );
    function scan() {
      scanFrame = 0;
      for (const element of page!.querySelectorAll<HTMLElement>(surfaces)) {
        if (
          known.has(element) ||
          element.closest(".studio-carousel") ||
          element.matches(".section-heading,.hero-heading")
        )
          continue;
        known.add(element);
        if (!element.dataset.reveal) element.dataset.reveal = "surface";
        element.dataset.revealState = "waiting";
        if (
          element.parentElement?.matches(
            ".principles,.file-cards,.feature-grid",
          )
        ) {
          const index = [...element.parentElement.children].indexOf(element);
          element.style.setProperty(
            "--reveal-delay",
            `${Math.min(index, 3) * 90}ms`,
          );
        }
        entrances.observe(element);
      }
      for (const element of page!.querySelectorAll<HTMLElement>(
        ":scope > section, :scope > .btw-strip",
      )) {
        if (scenes.has(element)) continue;
        scenes.add(element);
        element.dataset.motionScene = "";
        viewport.observe(element);
      }
      for (const element of known)
        if (!page!.contains(element)) {
          entrances.unobserve(element);
          known.delete(element);
        }
      for (const element of scenes)
        if (!page!.contains(element)) {
          viewport.unobserve(element);
          active.delete(element);
          scenes.delete(element);
        }
      schedule();
    }
    function rescan() {
      if (!scanFrame) scanFrame = requestAnimationFrame(scan);
    }
    function changePreference() {
      page!.classList.toggle("motion-ready", !preference.matches);
      if (preference.matches) {
        cancelAnimationFrame(frame);
        frame = 0;
      } else schedule();
    }
    function focus(event: FocusEvent) {
      const element = event.target as HTMLElement;
      if (!element.matches(":focus-visible")) return;
      for (const surface of known)
        if (surface.contains(element) && !pressed.has(surface))
          surface.dataset.revealState = "settled";
    }
    function press(event: PointerEvent) {
      for (const surface of known) {
        if (
          surface.dataset.reveal === "surface" &&
          surface.dataset.revealState === "entered" &&
          surface.contains(event.target as Node)
        ) {
          surface.style.animationPlayState = "paused";
          pressed.add(surface);
        }
      }
    }
    function release() {
      if (!pressed.size || releaseFrame) return;
      // Keep the pressed control in place through mouseup and click dispatch.
      releaseFrame = requestAnimationFrame(() => {
        releaseFrame = 0;
        for (const surface of pressed) {
          surface.dataset.revealState = "settled";
          surface.style.removeProperty("animation-play-state");
        }
        pressed.clear();
      });
    }
    function settled(event: AnimationEvent) {
      if (event.animationName !== "surface-arrive") return;
      const element = event.target as HTMLElement;
      if (known.has(element)) element.dataset.revealState = "settled";
    }
    const mutations = new MutationObserver(rescan);
    mutations.observe(page, { childList: true, subtree: true });
    const resize = new ResizeObserver(schedule);
    resize.observe(page);
    scan();
    changePreference();
    window.addEventListener("scroll", schedule, { passive: true });
    window.addEventListener("resize", schedule, { passive: true });
    page.addEventListener("focusin", focus);
    page.addEventListener("pointerdown", press, true);
    window.addEventListener("pointerup", release);
    window.addEventListener("pointercancel", release);
    window.addEventListener("blur", release);
    page.addEventListener("animationend", settled);
    preference.addEventListener("change", changePreference);
    return () => {
      cancelAnimationFrame(frame);
      cancelAnimationFrame(scanFrame);
      cancelAnimationFrame(releaseFrame);
      entrances.disconnect();
      viewport.disconnect();
      mutations.disconnect();
      resize.disconnect();
      window.removeEventListener("scroll", schedule);
      window.removeEventListener("resize", schedule);
      page.removeEventListener("focusin", focus);
      page.removeEventListener("pointerdown", press, true);
      window.removeEventListener("pointerup", release);
      window.removeEventListener("pointercancel", release);
      window.removeEventListener("blur", release);
      page.removeEventListener("animationend", settled);
      preference.removeEventListener("change", changePreference);
      page.classList.remove("motion-ready");
      for (const surface of pressed)
        surface.style.removeProperty("animation-play-state");
      for (const element of known) delete element.dataset.revealState;
      for (const element of scenes) {
        delete element.dataset.motionScene;
        delete element.dataset.sceneActive;
      }
    };
  }, []);
  return root;
}
