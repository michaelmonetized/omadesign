import { useEffect, useRef, useState } from "react";
import { sitePath } from "../site";
import "./cloud-waitlist.css";

const features = [
  ["Project files + assets", "Share project files and the assets that belong with them."],
  ["Team member access", "Control which team members can access each project."],
  ["Client review + annotations", "Collect comments and annotations on flat snapshots of your work."],
  ["User showcase", "Share finished public work in the Omadesign user showcase."],
  ["Competitions", "Submit public showcase work when a competition is open."],
];

export function CloudIntro() {
  const dialog = useRef<HTMLDialogElement>(null);
  const video = useRef<HTMLVideoElement>(null);
  const autoStart = useRef(true);
  const [open, setOpen] = useState(true);
  const [playing, setPlaying] = useState(false);
  const [playBlocked, setPlayBlocked] = useState(false);
  const [muted, setMuted] = useState(true);
  const [visibleCount, setVisibleCount] = useState(0);
  const [ready, setReady] = useState(false);

  function revealActions() { setVisibleCount(features.length); setReady(true); }

  useEffect(() => {
    const reopen = () => { if (window.location.hash === "#cloud") setOpen(true); };
    reopen();
    window.addEventListener("hashchange", reopen);
    return () => window.removeEventListener("hashchange", reopen);
  }, []);

  useEffect(() => {
    if (!open) return;
    const modal = dialog.current;
    const player = video.current;
    if (!modal || !player) return;
    const previousFocus = document.activeElement as HTMLElement | null;
    const overflow = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    modal.showModal();
    // Set the DOM properties before play(): Safari must see a muted, inline
    // player even when React hydrates an already-rendered video element.
    player.defaultMuted = true;
    player.muted = true;
    player.playsInline = true;
    player.setAttribute("muted", "");
    player.setAttribute("playsinline", "");
    setMuted(true);
    autoStart.current = true;
    const preference = window.matchMedia("(prefers-reduced-motion: reduce)");
    let active = true;
    let started = false;
    const markStarted = () => { started = true; setPlayBlocked(false); };
    const start = () => {
      if (!active || started || !autoStart.current || preference.matches) return;
      void player.play().catch(error => {
        if (!active || started || !autoStart.current || preference.matches) return;
        if (error.name === "NotAllowedError") setPlayBlocked(true);
        else if (error.name !== "AbortError") revealActions();
      });
    };
    const apply = () => {
      if (preference.matches) { player.pause(); revealActions(); }
      else { started = false; start(); }
    };
    player.addEventListener("playing", markStarted);
    player.addEventListener("canplay", start);
    apply();
    const frame = window.requestAnimationFrame(start);
    preference.addEventListener("change", apply);
    // Playback must never block access to the cloud workspace.
    const fallback = window.setTimeout(() => { if (!started) revealActions(); }, 8000);
    return () => {
      active = false;
      window.cancelAnimationFrame(frame);
      window.clearTimeout(fallback);
      player.removeEventListener("playing", markStarted);
      player.removeEventListener("canplay", start);
      preference.removeEventListener("change", apply);
      player.pause();
      modal.close();
      document.body.style.overflow = overflow;
      previousFocus?.focus();
    };
  }, [open]);

  function close() {
    setOpen(false);
    if (window.location.hash === "#cloud") history.replaceState(null, "", window.location.pathname + window.location.search);
  }

  return (
    <dialog ref={dialog} className="cloud-takeover" aria-labelledby="cloud-title" onCancel={close}>
      <div className="cloud-scene">
        <video ref={video} className="cloud-film" muted={muted} playsInline preload={open ? "auto" : "none"}
          poster={sitePath("media/cloud/reveal.webp")} width="1280" height="720"
          aria-label="Omadesign logo emerging from moonlit clouds"
          onPlaying={() => { setPlaying(true); setPlayBlocked(false); }} onPause={() => setPlaying(false)}
          onEnded={() => { setPlaying(false); revealActions(); }} onError={revealActions}
          onTimeUpdate={event => setVisibleCount(Math.min(features.length, Math.max(0, Math.floor((event.currentTarget.currentTime - 3) / 2.7) + 1)))}>
          <source src={sitePath("media/cloud/reveal.webm")} type="video/webm" />
          <source src={sitePath("media/cloud/reveal.mp4")} type="video/mp4" />
        </video>
        <div className="cloud-shade" />
        <header className="cloud-topline">
          <span>omadesign</span>
          <button type="button" onClick={close} autoFocus>Explore Omadesign <span aria-hidden="true">↗</span></button>
        </header>
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
                <a className="button" href={sitePath("cloud")}>Open cloud projects ↗</a>
                <a className="text-link" href={sitePath("docs/cloud")}>Read the cloud guide ↗</a>
              </div>
              <p className="cloud-consent">Private project access. Explicit uploads. No simultaneous canvas editing.</p>
            </section>
          </div>
        </div>
        <footer className="cloud-controls">
          <div>
            <button type="button" onClick={() => {
              const player = video.current;
              if (!player) return;
              autoStart.current = false;
              if (playing) player.pause();
              else { if (player.ended) player.currentTime = 0; void player.play().catch(revealActions); }
            }}>{playing ? "Pause" : playBlocked ? "Tap to play" : "Play"}</button>
            <button type="button" aria-pressed={!muted} onClick={() => {
              if (video.current) video.current.muted = !muted;
              setMuted(!muted);
            }}>{muted ? "Sound on" : "Sound off"}</button>
          </div>
          {!ready && <button type="button" onClick={() => { autoStart.current = false; video.current?.pause(); revealActions(); }}>Show cloud links ↓</button>}
          {ready && <span className="cloud-release-note">Included in Omadesign</span>}
        </footer>
      </div>
    </dialog>
  );
}
