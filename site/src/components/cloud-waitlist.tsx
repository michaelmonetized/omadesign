import { useCallback, useEffect, useRef, useState } from "react";
import { sitePath } from "../site";
import { CloudReveal, type CloudRevealHandle } from "./cloud-reveal";
import "./cloud-waitlist.css";

const features = [
  ["Project files + assets", "Share project files and the assets that belong with them."],
  ["Team member access", "Control which team members can access each project."],
  ["Client review + annotations", "Collect comments and annotations on flat snapshots of your work."],
  ["User showcase", "Share finished public work in the Omadesign user showcase."],
  ["Competitions", "Submit public showcase work when a competition is open."],
];

/** A living part of the homepage. Native document scrolling always stays available. */
export function CloudIntro({ siteBase }: { siteBase?: string } = {}) {
  const section = useRef<HTMLElement>(null);
  const scene = useRef<HTMLDivElement>(null);
  const reveal = useRef<CloudRevealHandle>(null);
  const [settled, setSettled] = useState(false);
  const [visibleCount, setVisibleCount] = useState(0);
  const [ready, setReady] = useState(false);
  const revealContent = useCallback((count: number, complete: boolean) => { setVisibleCount(count); setReady(complete); }, []);
  const markSettled = useCallback(() => setSettled(true), []);
  const destination = (path: string) => siteBase ? `${siteBase}/${path}` : sitePath(path);

  useEffect(() => {
    const element = section.current;
    const viewport = scene.current;
    if (!element || !viewport) return;
    const header = document.querySelector<HTMLElement>(".site-header");
    let frame = 0;
    let headerHeight = 0;
    let contentHeight = 0;
    let previousProgress = -1;

    const update = () => {
      frame = 0;
      const nextHeaderHeight = header?.getBoundingClientRect().height ?? 0;
      if (nextHeaderHeight !== headerHeight) {
        headerHeight = nextHeaderHeight;
        element.style.setProperty("--cloud-nav-height", `${headerHeight}px`);
      }
      if (contentHeight !== viewport.clientHeight) {
        contentHeight = viewport.clientHeight;
        element.style.setProperty("--cloud-content-height", `${contentHeight}px`);
      }
      const bounds = element.getBoundingClientRect();
      const travel = Math.max(1, bounds.height - viewport.clientHeight);
      const overflow = Math.max(0, contentHeight - (window.innerHeight - headerHeight));
      const progress = Math.max(0, Math.min(1, (headerHeight - bounds.top - overflow) / travel));
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

  return (
    <section ref={section} id="cloud" className="cloud-announcement" data-cloud-experience=""
      data-settled={settled} aria-labelledby="cloud-title">
      <div ref={scene} className="cloud-scene">
        <div className="cloud-stage" aria-hidden="true">
          <CloudReveal ref={reveal} onSettled={markSettled} onContentProgress={revealContent} />
        </div>
        <div className="cloud-shade" aria-hidden="true" />
        <div className="cloud-topline" aria-hidden="true" />
        <div className="cloud-columns">
          <div className="cloud-copy">
            <h1 id="cloud-title">cloud collab</h1>
            <p className="cloud-kicker">project sharing and snapshot review</p>
            <ul className="cloud-feature-stack" aria-label="Cloud collaboration features">
              {features.map(([title, description], index) => (
                <li key={title} data-visible={ready || visibleCount >= features.length - index}>
                  <span className="cloud-feature-mark" aria-hidden="true">↗</span>
                  <div><h2>{title}</h2><p>{description}</p></div>
                </li>
              ))}
            </ul>
          </div>
          <div className="cloud-invitation" data-ready={ready} inert={!ready} aria-hidden={!ready}>
            <section className="cloud-signup" aria-labelledby="cloud-start-title">
              <h2 id="cloud-start-title">share the work</h2>
              <p className="cloud-summary">Sign in to share files, invite reviewers and publish a finished export. Keep designing in the native app.</p>
              <div className="cloud-actions">
                <a className="button" href={destination("cloud")}>Open cloud projects ↗</a>
                <a className="text-link" href={destination("docs/cloud")}>Read the cloud guide ↗</a>
              </div>
              <p className="cloud-consent">Private project access. Explicit uploads. No simultaneous canvas editing.</p>
            </section>
          </div>
        </div>
        <footer className="cloud-controls">
          {ready && <span className="cloud-release-note">Included in Omadesign</span>}
        </footer>
      </div>
    </section>
  );
}
