import { useEffect, useRef, useState } from "react";
import { sitePath } from "../site";

const film = (extension: string) => sitePath(`media/studio/film-0.5.7.${extension}`);

export function ProductFilm() {
  const player = useRef<HTMLVideoElement>(null);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    const video = player.current;
    if (!video) return;
    // Playback is always a deliberate user action, including with reduced motion.
    const observer = new IntersectionObserver(([entry]) => {
      if (!entry.isIntersecting) video.pause();
    }, { threshold: 0.05 });
    observer.observe(video);
    const pauseWhenHidden = () => { if (document.hidden) video.pause(); };
    document.addEventListener("visibilitychange", pauseWhenHidden);
    return () => {
      observer.disconnect();
      document.removeEventListener("visibilitychange", pauseWhenHidden);
      video.pause();
    };
  }, []);

  return (
    <section className="section shell film-section" id="film" aria-labelledby="product-film-title">
      <div className="section-heading">
        <div>
          <p className="gallery-kicker">OMADESIGN 0.5.7 · THE FILM</p>
          <h2 id="product-film-title">See the studio move.</h2>
        </div>
        <p id="product-film-description">
          A quick look at the native studio, captured in Omadesign 0.5.7.
          Thirty-two seconds. No audio.
        </p>
      </div>
      <video
        ref={player}
        controls
        playsInline
        muted
        preload="none"
        width="1920"
        height="1080"
        poster={film("webp")}
        aria-label="Omadesign 0.5.7 native product film, 32 seconds, no audio"
        aria-describedby="product-film-description"
        onError={() => setFailed(true)}
        onLoadedMetadata={() => setFailed(false)}
      >
        <source src={film("webm")} type="video/webm" />
        <source src={film("mp4")} type="video/mp4" />
        <track kind="captions" src={film("vtt")} srcLang="en" label="English scene descriptions" />
        <a href={film("mp4")}>Watch the product film.</a>
      </video>
      <div className="film-caption">
        <span>Native app footage · 0:32 · Silent</span>
        <nav aria-label="Product film downloads">
          <a href={film("vtt")}>Scene descriptions</a>
          <a href={film("mp4")} download>Keep a copy ↓</a>
        </nav>
      </div>
      {failed ? (
        <p className="film-error" role="alert">
          The film could not load. <a href={film("mp4")}>Open the video directly.</a>
        </p>
      ) : null}
    </section>
  );
}
