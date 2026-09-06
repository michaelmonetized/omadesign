import { createFileRoute } from "@tanstack/react-router";
import { faqs } from "../features";
import { media, sitePath } from "../site";
import { BrandKit } from "../components/brand-kit";
import { StudioCarousel } from "../components/studio-carousel";
import { ShortcutHud } from "../components/shortcut-hud";
import { FeatureExplorer } from "../components/feature-explorer";
import { Install } from "../components/install";
import { Arrow, Shot } from "../components/studio-ui";
import { useScrollMotion } from "../components/scroll-motion";

export const Route = createFileRoute("/")({ component: Home });

function Home() {
  const motion = useScrollMotion();
  return (
    <main id="main" ref={motion}>
      <section className="hero shell">
        <div className="hero-heading" data-motion>
          <h1>
            Native Linux.
            <br />
            <span>Full creative control.</span>
          </h1>
          <p>
            Design, paint, edit photos, and animate.
            <br />
            Built in Rust. Built for Omarchy.
          </p>
        </div>
        <Install />
        <StudioCarousel />
      </section>
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
      <section className="section shell native-section" id="native">
        <div className="section-heading">
          <h2>Omarchy first.</h2>
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
            <h3>Your desktop theme</h3>
            <p>
              Reads your Omarchy colors and desktop font at launch. Uses
              Catppuccin when an Omarchy theme isn’t available.
            </p>
          </article>
          <article>
            <div className="native-symbol" aria-hidden="true">
              ↗
            </div>
            <h3>Native Rust</h3>
            <p>
              Cached rendering. Background previews, file scans, and exports.
              Built for ARM64 and x86_64 Linux.
            </p>
          </article>
          <article>
            <div className="file-symbol" aria-hidden="true">
              ~/
            </div>
            <h3>Local files</h3>
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
          <h2>
            <span className="accent">52</span> editable templates.
          </h2>
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
          <h2>The studio in 97 seconds.</h2>
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
        <h2>Details.</h2>
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
