import { useEffect, useRef, useState, type FormEvent } from "react";
import { SITE_ORIGIN, sitePath } from "../site";
import "./cloud-waitlist.css";

const features = [
  ["Project files + assets", "Share project files and the assets that belong with them."],
  ["Team member access", "Control which team members can access each project."],
  ["Client review + annotations", "Collect comments and annotations on flat snapshots of your work."],
  ["User showcase", "Share finished public work in the Omadesign user showcase."],
  ["Competitions", "Enter your showcased work into competitions."],
];

export function CloudWaitlist() {
  const dialog = useRef<HTMLDialogElement>(null);
  const video = useRef<HTMLVideoElement>(null);
  const [open, setOpen] = useState(true);
  const [playing, setPlaying] = useState(false);
  const [muted, setMuted] = useState(true);
  const [visibleCount, setVisibleCount] = useState(0);
  const [ready, setReady] = useState(false);
  const [status, setStatus] = useState<"idle" | "sending" | "success" | "error">("idle");
  const [message, setMessage] = useState("");

  function revealSignup() { setVisibleCount(features.length); setReady(true); }

  useEffect(() => {
    const reopen = () => { if (window.location.hash === "#cloud") setOpen(true); };
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
    const preference = window.matchMedia("(prefers-reduced-motion: reduce)");
    const apply = () => {
      if (preference.matches) { player.pause(); revealSignup(); }
      else void player.play().catch(revealSignup);
    };
    apply();
    preference.addEventListener("change", apply);
    // A stalled download must never block access to the waitlist.
    const fallback = window.setTimeout(() => { if (player.readyState < 2) revealSignup(); }, 8000);
    return () => {
      window.clearTimeout(fallback);
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
  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (status === "sending") return;
    const form = event.currentTarget;
    const data = new FormData(form);
    setStatus("sending");
    setMessage("");
    try {
      const response = await fetch(`${import.meta.env.BASE_URL === "/" ? "" : SITE_ORIGIN}/api/waitlist`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          email: data.get("email"), name: "",
          website: data.get("website"), list: "cloud",
        }),
        signal: AbortSignal.timeout(15000),
      });
      const result = await response.json();
      if (!response.ok || result.ok !== true) throw new Error(result.error || "We couldn’t save your signup. Please try again.");
      setStatus("success");
      setMessage("You’re on the list. We’ll email you when cloud collaboration is ready.");
      form.reset();
    } catch (error) {
      setStatus("error");
      setMessage(error instanceof Error && error.name !== "TimeoutError" && error.name !== "TypeError"
        ? error.message : "We couldn’t reach the waitlist. Please try again in a moment.");
    }
  }

  return (
    <dialog ref={dialog} className="cloud-takeover" aria-labelledby="cloud-title" onCancel={close}>
      <div className="cloud-scene">
        <video ref={video} className="cloud-film" muted={muted} playsInline preload="auto"
          poster={sitePath("media/cloud/reveal.webp")} width="1280" height="720"
          aria-label="Omadesign logo emerging from moonlit clouds"
          onPlay={() => setPlaying(true)} onPause={() => setPlaying(false)}
          onEnded={() => { setPlaying(false); revealSignup(); }} onError={revealSignup}
          onTimeUpdate={event => setVisibleCount(Math.min(features.length, Math.max(0, Math.floor((event.currentTarget.currentTime - 3) / 2.7) + 1)))}>
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
            <p className="cloud-kicker">coming to omadesign 0.5.2</p>
            <ul className="cloud-feature-stack" aria-label="Planned cloud collaboration features">
              {features.map(([title, description], index) => (
                <li key={title} data-visible={ready || visibleCount >= features.length - index}>
                  <span className="cloud-feature-mark" aria-hidden="true">↗</span>
                  <div><h2>{title}</h2><p>{description}</p></div>
                </li>
              ))}
            </ul>
          </div>
          <div className="cloud-invitation" data-ready={ready} inert={!ready} aria-hidden={!ready}>
            <form className="cloud-signup" onSubmit={submit} aria-labelledby="cloud-signup-title" aria-busy={status === "sending"}>
              <h2 id="cloud-signup-title">join the waitlist</h2>
              <label className="cloud-sr-only" htmlFor="cloud-email">Email address</label>
              <div className="cloud-email-row">
                <input id="cloud-email" name="email" type="email" autoComplete="email" maxLength={254} placeholder="your@email.com" required />
                <button type="submit" disabled={status === "sending" || status === "success"} aria-label="Join the waitlist">
                  {status === "sending" ? "Joining…" : status === "success" ? "Joined ✓" : "Join ↗"}
                </button>
              </div>
              <div className="cloud-honey" aria-hidden="true"><label>Website<input name="website" tabIndex={-1} autoComplete="off" /></label></div>
              <p className="cloud-consent">An email when it’s ready. No newsletters.</p>
              <p className="cloud-status" role="status" aria-live="polite">{message}</p>
            </form>
          </div>
        </div>
        <footer className="cloud-controls">
          <div>
            <button type="button" onClick={() => {
              const player = video.current;
              if (!player) return;
              if (playing) player.pause();
              else { if (player.ended) player.currentTime = 0; void player.play().catch(revealSignup); }
            }}>{playing ? "Pause" : "Play"}</button>
            <button type="button" aria-pressed={!muted} onClick={() => setMuted(!muted)}>{muted ? "Sound on" : "Sound off"}</button>
          </div>
          {!ready && <button type="button" onClick={() => { video.current?.pause(); revealSignup(); }}>Skip to waitlist ↓</button>}
          {ready && <span className="cloud-release-note">Planned for 0.5.2</span>}
        </footer>
      </div>
    </dialog>
  );
}
