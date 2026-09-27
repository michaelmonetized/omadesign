import { useCallback, useEffect, useRef, useState } from "react";
import { sitePath } from "../site";
import { CloudReveal, type CloudRevealHandle } from "./cloud-reveal";
import "./cloud-waitlist.css";

/** A living part of the homepage. Native document scrolling always stays available. */
export function CloudIntro({ siteBase }: { siteBase?: string } = {}) {
  const section = useRef<HTMLElement>(null);
  const scene = useRef<HTMLDivElement>(null);
  const reveal = useRef<CloudRevealHandle>(null);
  const [settled, setSettled] = useState(false);
  const [motionEnabled, setMotionEnabled] = useState(true);
  const markSettled = useCallback(() => setSettled(true), []);
  const destination = (path: string) => siteBase ? `${siteBase}/${path}` : sitePath(path);

  const finishEntrance = useCallback(() => {
    reveal.current?.finishEntrance();
    setSettled(true);
  }, []);

  useEffect(() => {
    const element = section.current;
    const viewport = scene.current;
    if (!element || !viewport) return;
    const header = document.querySelector<HTMLElement>(".site-header");
    let frame = 0;
    let headerHeight = 0;
    let previousProgress = -1;

    const update = () => {
      frame = 0;
      const nextHeaderHeight = header?.getBoundingClientRect().height ?? 0;
      if (nextHeaderHeight !== headerHeight) {
        headerHeight = nextHeaderHeight;
        element.style.setProperty("--cloud-nav-height", `${headerHeight}px`);
      }
      const bounds = element.getBoundingClientRect();
      const travel = Math.max(1, bounds.height - viewport.clientHeight);
      const progress = Math.max(0, Math.min(1, (headerHeight - bounds.top) / travel));
      if (progress !== previousProgress) {
        element.style.setProperty("--cloud-scroll", String(progress));
        element.dataset.departed = String(progress > 0.56);
        reveal.current?.setScroll(progress);
        previousProgress = progress;
      }
    };
    const schedule = () => { if (!frame) frame = requestAnimationFrame(update); };
    const resize = new ResizeObserver(schedule);
    resize.observe(element);
    resize.observe(viewport);
    if (header) resize.observe(header);
    window.addEventListener("scroll", schedule, { passive: true });
    window.addEventListener("resize", schedule, { passive: true });
    update();
    return () => {
      cancelAnimationFrame(frame);
      resize.disconnect();
      window.removeEventListener("scroll", schedule);
      window.removeEventListener("resize", schedule);
    };
  }, []);

  function toggleMotion() {
    const next = !motionEnabled;
    reveal.current?.setMotion(next);
    setMotionEnabled(next);
    if (!next) finishEntrance();
  }

  return (
    <section ref={section} id="cloud" className="cloud-announcement" data-cloud-experience=""
      data-settled={settled} data-motion={motionEnabled} aria-labelledby="cloud-title">
      <div ref={scene} className="cloud-scene">
        <div className="cloud-stage" aria-hidden="true">
          <CloudReveal ref={reveal} onSettled={markSettled} onMotionChange={setMotionEnabled} />
        </div>
        <div className="cloud-copy-shade" aria-hidden="true" />
        <div className="cloud-topline">
          <a className="cloud-explore" href="#omadesign-product">Explore Omadesign <span aria-hidden="true">↗</span></a>
          <div className="cloud-preferences">
            {!settled && <button type="button" onClick={finishEntrance}>Skip entrance</button>}
            <button type="button" aria-pressed={motionEnabled} aria-label="Animate the dreamscape" onClick={toggleMotion}>
              <span className="cloud-motion-dot" aria-hidden="true" />
              Motion {motionEnabled ? "on" : "off"}
            </button>
          </div>
        </div>
        <div className="cloud-content" onFocusCapture={() => { if (!settled) finishEntrance(); }}>
          <p className="cloud-kicker" id="cloud-title">cloud collab</p>
          <h2>Your creative work.<br className="cloud-mobile-break" /> Connected.</h2>
          <p className="cloud-summary">Share project files, invite your team and gather feedback on snapshots of your work.</p>
          <div className="cloud-actions">
            <a className="cloud-primary" href={destination("cloud")}>Open cloud projects <span aria-hidden="true">↗</span></a>
            <a className="cloud-guide" href={destination("docs/cloud")}>Cloud guide <span aria-hidden="true">↗</span></a>
          </div>
          <ul className="cloud-capabilities" aria-label="Cloud collaboration features">
            <li>Project sharing</li><li>Team access</li><li>Snapshot review</li>
          </ul>
        </div>
        <a className="cloud-scroll-cue" href="#omadesign-product">
          <span>Keep exploring</span><span className="cloud-scroll-arrow" aria-hidden="true">↓</span>
        </a>
      </div>
    </section>
  );
}
