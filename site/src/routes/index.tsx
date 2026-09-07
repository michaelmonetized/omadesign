import { useState } from "react";
import { createFileRoute } from "@tanstack/react-router";
import { faqs } from "../features";
import { media, sitePath } from "../site";
import { BrandKit } from "../components/brand-kit";
import { StudioCarousel, type StudioName } from "../components/studio-carousel";
import { StudioRecordings } from "../components/native-recordings";
import { RevealText } from "../components/reveal-text";
import { ShortcutHud } from "../components/shortcut-hud";
import { FeatureExplorer } from "../components/feature-explorer";
import { Install } from "../components/install";
import { FilePreview } from "../components/file-preview";
import { Arrow, Shot } from "../components/studio-ui";
import { useScrollMotion } from "../components/scroll-motion";

export const Route = createFileRoute("/")({ component: Home });

function Home() {
  const motion = useScrollMotion();
  const [studio, setStudio] = useState<StudioName>("Design");
  return (
    <main id="main" ref={motion}>
      <section className="hero shell">
        <div className="hero-heading">
          <RevealText
            as="h1"
            hero
            lines={["Professional Grade", "Graphic Design Tool Suite"]}
          />
          <p className="hero-intro">
            <span>Developed for Omarchy BTW.</span>
            <span className="hero-modes">
              <span>Illustrate</span>
              <span>Paint</span>
              <span>Refine</span>
              <span>Animate</span>
            </span>
          </p>
        </div>
        <Install />
      </section>
      <section className="hero-stage">
        <StudioCarousel selected={studio} onSelect={setStudio} />
      </section>
      <StudioRecordings studio={studio} onSelect={setStudio} />
      <div className="btw-strip">
        <div className="shell">
          <span>
            Rust, <em>btw.</em>
          </span>
          <span>
            Arch, <em>btw.</em>
          </span>
          <span>ARM64 + x86_64</span>
          <span>FOSS. MIT.</span>
        </div>
      </div>
      <FilePreview />
      <section className="section shell native-section" id="native">
        <div className="section-heading">
          <RevealText text="Omarchy first." />
        </div>
        <div className="principles">
          <article>
            <div className="theme-swatch" aria-hidden="true">
              <i />
              <i />
              <i />
              <i />
              <i />
            </div>
            <RevealText as="h3" text="Your desktop theme" />
            <p>
              Reads your Omarchy colors and desktop font at launch. Uses
              Catppuccin when an Omarchy theme isn’t available.
            </p>
          </article>
          <article>
            <div className="native-symbol" aria-hidden="true">
              ↗
            </div>
            <RevealText as="h3" text="Native Rust" />
            <p>
              Cached rendering. Background previews, file scans, and exports.
              Built for ARM64 and x86_64 Linux.
            </p>
          </article>
          <article>
            <div className="file-symbol" aria-hidden="true">
              ~/
            </div>
            <RevealText as="h3" text="Local files" />
            <p>
              Editable documents and portable brand kits on your disk. No app
              account. Free and open source.
            </p>
          </article>
        </div>
      </section>
      <BrandKit />
      <ShortcutHud />
      <section className="section shell templates-section" id="templates">
        <div className="templates-copy">
          <RevealText iris lines={["52 editable", "templates."]} />
          <p>
            Posters, identity, social, and editorial. Nine categories. Preset or
            custom sizes. All included, all offline.
          </p>
          <a
            className="text-link"
            href={`${sitePath("docs/manual")}#templates`}
          >
            Template library <Arrow />
          </a>
        </div>
        <figure className="template-visual">
          <Shot
            name="templates.webp"
            alt="The native template library with editable designs, category filters, and size controls."
          />
        </figure>
      </section>
      <section className="section shell film-section" id="film">
        <div className="section-heading">
          <RevealText text="The studio in 97 seconds." />
          <a className="text-link" href={media("film.mp4")} download>
            Download video ↓
          </a>
        </div>
        <video
          controls
          playsInline
          preload="none"
          poster={media("omadesign-logo.webp")}
          width="1920"
          height="1080"
          aria-label="omadesign feature demo, 97 seconds"
        >
          <source src={media("film.mp4")} type="video/mp4" />
          <track
            kind="captions"
            src={media("film.vtt")}
            srcLang="en"
            label="English scene descriptions"
          />
          <p>
            <a href={media("film.mp4")}>Download the demo video</a>.
          </p>
        </video>
      </section>
      <FeatureExplorer />
      <section className="section shell faq-section" id="questions">
        <RevealText text="Details." />
        <div className="faq-list">
          {faqs.map(({ question, answer }) => (
            <details key={question}>
              <summary>
                {question}
                <span aria-hidden="true">+</span>
              </summary>
              <p>{answer}</p>
            </details>
          ))}
          <details>
            <summary>
              What sets it apart from Affinity?<span aria-hidden="true">+</span>
            </summary>
            <p>
              Native Linux support, Omarchy theme integration, portable brand
              dotfiles, and vector design, painting, photo editing, and
              animation in one app. omadesign is an independent alpha.
            </p>
            <p>
              <a href="https://www.affinity.studio/get-affinity">
                Affinity platform information ↗
              </a>{" "}
              · <a href={sitePath("docs/manual")}>omadesign manual →</a>
            </p>
          </details>
        </div>
      </section>
    </main>
  );
}
