import { useEffect, useId, useRef, useState } from "react";
import type { KeyboardEvent } from "react";
import { recordings, type Recording } from "../recordings";
import { sitePath } from "../site";
import { studioNames as studios, type StudioName } from "./studio-carousel";
import { RevealText } from "./reveal-text";
import "./native-recordings.css";

const file = (id: string, extension: string) =>
  sitePath(`media/recordings/${id}.${extension}`);
const clock = (seconds: number) =>
  `${Math.floor(seconds / 60)}:${String(Math.floor(seconds % 60)).padStart(2, "0")}`;

export function StudioRecordings({
  studio,
  onSelect,
}: {
  studio: StudioName;
  onSelect: (studio: StudioName) => void;
}) {
  const id = useId();
  const active = recordings.find((recording) => recording.name === studio);
  function navigate(event: KeyboardEvent<HTMLDivElement>) {
    const index = studios.indexOf(studio);
    const next =
      event.key === "ArrowRight"
        ? (index + 1) % studios.length
        : event.key === "ArrowLeft"
          ? (index + studios.length - 1) % studios.length
          : event.key === "Home"
            ? 0
            : event.key === "End"
              ? studios.length - 1
              : null;
    if (next === null) return;
    event.preventDefault();
    onSelect(studios[next]);
    event.currentTarget
      .querySelectorAll<HTMLButtonElement>("button")
      [next]?.focus();
  }
  return (
    <section className="section shell recordings-section" id="recordings">
      <div className="section-heading">
        <RevealText text="The studio." />
        <a href={sitePath("docs/manual")} className="text-link">
          Manual ↗
        </a>
      </div>
      <div
        className="recording-studio-tabs"
        role="tablist"
        aria-label="Studios"
        onKeyDown={navigate}
      >
        {studios.map((name) => (
          <button
            type="button"
            role="tab"
            key={name}
            id={`${id}-${name.toLowerCase()}`}
            aria-controls={`${id}-player`}
            aria-selected={name === studio}
            tabIndex={name === studio ? 0 : -1}
            onClick={() => onSelect(name)}
          >
            {name}
          </button>
        ))}
      </div>
      <div
        id={`${id}-player`}
        role="tabpanel"
        aria-labelledby={`${id}-${studio.toLowerCase()}`}
      >
        {active ? <RecordingPlayer key={active.id} recording={active} /> : <LayoutOverview />}
      </div>
      <noscript>
        <style>{`.recording-studio-tabs,.recording-chapters{display:none}`}</style>
        <p className="recording-fallback">
          All recordings:{" "}
          {recordings.map((item) => (
            <a href={file(item.id, "mp4")} key={item.id}>
              {item.name} ↗{" "}
            </a>
          ))}
        </p>
      </noscript>
    </section>
  );
}

function LayoutOverview() {
  return (
    <div className="layout-overview">
      <div className="layout-overview-copy">
        <h3>Frames. Stacks. Screens.</h3>
        <p>Build UI and web mockups in the same editable document as your artwork.</p>
        <dl>
          <div><dt>Nested frames</dt><dd>Press F to draw a frame. Nest frames and add type, shapes, and image placeholders.</dd></div>
          <div><dt>Auto-layout</dt><dd>Arrange children in horizontal or vertical stacks with gap, padding, and stretch.</dd></div>
          <div><dt>Constraints</dt><dd>Pin, center, stretch, or scale children when their parent frame resizes.</dd></div>
          <div><dt>Frame exports</dt><dd>Export a selected frame to PNG, SVG, or HTML.</dd></div>
        </dl>
        <a className="text-link" href={`${sitePath("docs/manual")}#layout`}>Explore Layout ↗</a>
      </div>
      <figure>
        <img src={sitePath("media/showcase/layout.webp")} width="1600" height="1000" loading="lazy" alt="Form: an editable studio dashboard exported from Omadesign Layout." />
        <figcaption>Form · Editable Layout example <a href={sitePath("media/showcase/layout.oma")} download>Download editable project ↗</a></figcaption>
      </figure>
    </div>
  );
}

export function RecordingPlayer({ recording }: { recording: Recording }) {
  const video = useRef<HTMLVideoElement>(null);
  const frame = useRef<HTMLDivElement>(null);
  const pending = useRef<number | null>(null);
  const [time, setTime] = useState(0);
  const [error, setError] = useState(false);
  const [notice, setNotice] = useState("");
  const chapter = recording.chapters.reduce(
    (active, item, index) => (time + 0.15 >= item.at ? index : active),
    0,
  );
  useEffect(() => {
    const player = video.current;
    const surface = frame.current;
    if (!player || !surface) return;
    const visible = new IntersectionObserver(
      ([entry]) => {
        if (!entry.isIntersecting) player.pause();
      },
      { threshold: 0.05 },
    );
    visible.observe(surface);
    return () => {
      visible.disconnect();
      player.pause();
    };
  }, []);
  function playChapter(at: number) {
    const player = video.current;
    if (!player) return;
    pending.current = at;
    setTime(at);
    setNotice("");
    if (player.readyState >= 1) {
      player.currentTime = at;
      pending.current = null;
    }
    void player
      .play()
      .catch(() => setNotice("Press play to watch this chapter."));
  }
  return (
    <div className="recording-player" data-reveal="surface">
      <div
        className="recording-chapters"
        aria-label={`${recording.name} recording chapters`}
      >
        {recording.chapters.map((item, index) => (
          <button
            type="button"
            key={item.name}
            aria-current={index === chapter ? "true" : undefined}
            onClick={() => playChapter(item.at)}
          >
            <span>{item.name}</span>
            <span>{clock(item.at)}</span>
          </button>
        ))}
      </div>
      <div className="recording-frame" ref={frame}>
        <video
          ref={video}
          controls
          playsInline
          muted
          preload="none"
          width="1600"
          height="900"
          poster={file(recording.id, "webp")}
          aria-label={`${recording.name} native screen recording`}
          onTimeUpdate={(event) => setTime(event.currentTarget.currentTime)}
          onError={() => setError(true)}
          onLoadedMetadata={(event) => {
            if (pending.current !== null) {
              event.currentTarget.currentTime = pending.current;
              pending.current = null;
            }
          }}
        >
          <source src={file(recording.id, "mp4")} type="video/mp4" />
          <track
            kind="captions"
            src={file(recording.id, "vtt")}
            srcLang="en"
            label="Panel descriptions"
          />
          <a href={file(recording.id, "mp4")}>
            Watch the {recording.name} recording
          </a>
        </video>
      </div>
      <div className="recording-meta">
        <span>Native screen recording · {clock(recording.duration)}</span>
        <a href={file(recording.id, "mp4")} download>
          Download ↗
        </a>
      </div>
      {error ? (
        <p role="alert" className="recording-notice">
          Video couldn’t load.{" "}
          <a href={file(recording.id, "mp4")}>Open the recording directly.</a>
        </p>
      ) : null}
      <p className="recording-notice" role="status">
        {notice}
      </p>
    </div>
  );
}
