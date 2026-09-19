import { useEffect, useRef, useState, type CSSProperties, type FormEvent } from "react";
import { SITE_ORIGIN, sitePath } from "../site";
import "./cloud-waitlist.css";

const features = [
  ["01", "One shared canvas", "Create together in the browser, with live edits, cursors and presence."],
  ["02", "Your team, invited", "Shared projects with invitations and owner, editor and viewer permissions."],
  ["03", "Feedback in place", "Pin comments to the work. Reply in threads, resolve decisions and reopen them."],
  ["04", "Room to explore", "Version history and restore points so a new direction doesn’t erase the old one."],
  ["05", "Desktop meets browser", "Move between native Omadesign and the web, with saved work and reconnect recovery."],
  ["06", "A shared visual language", "Keep team assets, palettes and typography together in shared libraries."],
  ["07", "Share on your terms", "Keep projects private, invite reviewers or deliberately publish to the showcase."],
  ["08", "Ready for handoff", "Review frames and export the assets your next step needs."],
];

export function CloudWaitlist() {
  const video = useRef<HTMLVideoElement>(null);
  const featureGrid = useRef<HTMLDivElement>(null);
  const [featuresEntered, setFeaturesEntered] = useState(false);
  const [playing, setPlaying] = useState(false);
  const [muted, setMuted] = useState(true);
  const [status, setStatus] = useState<"idle" | "sending" | "success" | "error">("idle");
  const [message, setMessage] = useState("");

  useEffect(() => {
    const preference = window.matchMedia("(prefers-reduced-motion: reduce)");
    const apply = () => {
      if (preference.matches) video.current?.pause();
      else void video.current?.play().catch(() => {});
    };
    apply();
    const observer = new IntersectionObserver(entries => {
      if (entries.some(entry => entry.isIntersecting)) {
        setFeaturesEntered(true);
        observer.disconnect();
      }
    }, { threshold: 0.1 });
    if (featureGrid.current) observer.observe(featureGrid.current);
    preference.addEventListener("change", apply);
    return () => { preference.removeEventListener("change", apply); observer.disconnect(); };
  }, []);

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
          email: data.get("email"), name: data.get("name"),
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
    <section className="cloud-section" id="cloud" aria-labelledby="cloud-title">
      <div className="shell">
        <div className="cloud-heading">
          <p className="cloud-eyebrow"><span /> Omadesign Cloud <span className="cloud-target">Planned for 0.5.2</span></p>
          <h1 id="cloud-title">Good things happen<br />when we <em>create together.</em></h1>
          <p>Your native creative studio. A shared space in the cloud.</p>
        </div>
        <div className="cloud-stage">
          <div className="cloud-film">
            <video ref={video} muted={muted} playsInline preload="metadata"
              poster={sitePath("media/cloud/reveal.webp")} width="1280" height="720"
              aria-label="Omadesign logo emerging from moonlit clouds"
              onPlay={() => setPlaying(true)} onPause={() => setPlaying(false)} onEnded={() => setPlaying(false)}>
              <source src={sitePath("media/cloud/reveal.mp4")} type="video/mp4" />
            </video>
            <div className="cloud-film-controls">
              <button type="button" onClick={() => {
                const player = video.current;
                if (!player) return;
                if (playing) player.pause();
                else { if (player.ended) player.currentTime = 0; void player.play().catch(() => {}); }
              }}>{playing ? "Pause film" : "Play film"}</button>
              <button type="button" aria-pressed={!muted} onClick={() => setMuted(!muted)}>{muted ? "Sound on" : "Sound off"}</button>
            </div>
          </div>
          <form className="cloud-signup" onSubmit={submit} aria-labelledby="cloud-signup-title" aria-busy={status === "sending"}>
            <span className="cloud-eyebrow">Be there from the beginning</span>
            <h2 id="cloud-signup-title">Make room<br />for your team.</h2>
            <p>Join the cloud collaboration waitlist for an invitation when it’s ready.</p>
            <label htmlFor="cloud-name">Name <span>(optional)</span></label>
            <input id="cloud-name" name="name" autoComplete="name" maxLength={100} placeholder="Your name" />
            <label htmlFor="cloud-email">Email</label>
            <input id="cloud-email" name="email" type="email" autoComplete="email" maxLength={254} placeholder="you@studio.com" required />
            <div className="cloud-honey" aria-hidden="true"><label>Website<input name="website" tabIndex={-1} autoComplete="off" /></label></div>
            <button className="button cloud-submit" type="submit" disabled={status === "sending" || status === "success"}>
              {status === "sending" ? "Joining…" : status === "success" ? "You’re on the list ✓" : "Join the waitlist"}<span aria-hidden="true">↗</span>
            </button>
            <p className="cloud-consent">We’ll use your email for cloud access updates. No newsletters or third-party marketing.</p>
            <p className={`cloud-status ${status === "error" ? "cloud-error" : ""}`} role="status" aria-live="polite">{message}</p>
          </form>
        </div>
        <div className="cloud-feature-heading"><h2>A place for the whole process.</h2><p>The planned cloud collaboration feature set.<br />In development. Join the waitlist for access.</p></div>
        <div className="cloud-features" ref={featureGrid} data-entered={featuresEntered}>
          {features.map(([number, title, description], index) => (
            <article key={number} style={{ "--cloud-order": index } as CSSProperties}>
              <span className="cloud-feature-number">{number}</span><h3>{title}</h3><p>{description}</p>
            </article>
          ))}
        </div>
        <div className="cloud-footnote"><p>Native by choice. Cloud when you need it.</p><a href={sitePath("#install")}>Get the Linux app today <span aria-hidden="true">↓</span></a></div>
      </div>
    </section>
  );
}
