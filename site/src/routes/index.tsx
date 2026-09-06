import { createFileRoute } from "@tanstack/react-router";
import { faqs } from "../features";
import { media, sitePath } from "../site";
import { BrandKit } from "../components/brand-kit";
import { Studios } from "../components/studios";
import { ShortcutHud } from "../components/shortcut-hud";
import { FeatureExplorer } from "../components/feature-explorer";
import { Install } from "../components/install";
import { Arrow, Eyebrow, Shot } from "../components/studio-ui";

export const Route = createFileRoute("/")({ component: Home });

function Home() {
  return (
    <main id="main">
      <section className="hero shell">
        <div className="hero-heading">
          <div>
            <Eyebrow>
              <span className="status-dot" /> OMARCHY FIRST. NATIVE BY DESIGN.
            </Eyebrow>
            <h1>
              Your Linux.
              <br />
              <span>Your creative suite.</span>
            </h1>
          </div>
          <div className="hero-intro">
            <p>
              Draw, paint, retouch and animate in one native Rust app. Built for
              Omarchy. Your desktop’s theme. Your files. Your flow.
            </p>
            <div className="button-row">
              <a className="button" href="#install">
                Make yourself at home <Arrow />
              </a>
              <a className="text-link" href="#film">
                <span className="play-icon" aria-hidden="true">
                  ▷
                </span>{" "}
                Watch the film <span className="muted">1:37</span>
              </a>
            </div>
            <p className="hero-note">
              Free & open source · Linux ARM64 + x86_64 · Alpha
            </p>
          </div>
        </div>
        <figure className="hero-figure">
          <div className="frame-label">
            <span>
              <span className="status-dot" /> THE STUDIO, IN ITS NATURAL HABITAT
            </span>
            <span>REAL APP. REAL PIXELS.</span>
          </div>
          <div className="app-frame">
            <Shot
              name="brand-library.webp"
              alt="omadesign running natively on Linux: an editable Fieldwork layout beside a filterable project brand bank, with the shortcut HUD below the canvas."
              eager
            />
          </div>
          <figcaption>
            <span>One canvas. Four creative directions.</span>
            <a href="#studios">
              Design / Pixel / Photo / Motion <Arrow />
            </a>
          </figcaption>
        </figure>
      </section>

      <div className="btw-strip">
        <div className="shell">
          <span>
            Rust, <em>btw.</em>
          </span>
          <span>
            Arch, <em>btw.</em>
          </span>
          <span>
            ARM64 + x86_64, <em>btw.</em>
          </span>
          <span>
            FOSS, <em>always.</em>
          </span>
        </div>
      </div>

      <section className="section shell native-section" id="native">
        <div className="section-heading">
          <div>
            <Eyebrow>01 / RIGHT AT HOME</Eyebrow>
            <h2>
              A creative suite that
              <br />
              speaks Linux.
            </h2>
          </div>
          <p>
            Professional creative tools for the desktop you chose.
            <br />
            And the freedom to make it yours.
          </p>
        </div>
        <div className="principles">
          <article>
            <span className="principle-number">01.1</span>
            <div className="theme-swatch" aria-hidden="true">
              <i />
              <i />
              <i />
              <i />
              <i />
            </div>
            <h3>
              Your theme.
              <br />
              Already dressed for it.
            </h3>
            <p>
              omadesign reads your Omarchy colours and desktop font at launch.
              The panels feel like part of your setup, because they are.
            </p>
            <span className="small-label">OMARCHY COLOURS + FONTCONFIG</span>
          </article>
          <article>
            <span className="principle-number">01.2</span>
            <div className="native-symbol" aria-hidden="true">
              ↗
            </div>
            <h3>
              Native speed.
              <br />
              Room to stay in flow.
            </h3>
            <p>
              Rust at the core, with cached artwork and background previews,
              file scans and exports. A light interface that keeps the canvas
              close.
            </p>
            <span className="small-label">NATIVE LINUX · BUILT IN RUST</span>
          </article>
          <article>
            <span className="principle-number">01.3</span>
            <div className="file-symbol" aria-hidden="true">
              ~/
            </div>
            <h3>
              Ordinary files.
              <br />
              Extraordinary freedom.
            </h3>
            <p>
              Your work and brand kit live on your disk. Edit the JSON, share
              the folder, keep it in Git. Start creating without an app account.
            </p>
            <span className="small-label">LOCAL FIRST · MIT LICENSED</span>
          </article>
        </div>
        <p className="native-footnote">
          Omarchy first, Linux beyond it. Packages for ARM64 / aarch64 and
          x86_64, targeting glibc 2.35. Catppuccin is the fallback when an
          Omarchy theme isn’t available.
        </p>
      </section>

      <BrandKit />
      <Studios />
      <ShortcutHud />

      <section className="section shell templates-section" id="templates">
        <div className="templates-copy">
          <Eyebrow>05 / A YEAR OF GOOD STARTS</Eyebrow>
          <div className="template-number">
            52
            <span>
              templates.
              <br />
              Zero blank-page panic.
            </span>
          </div>
          <h2>
            Your next idea
            <br />
            has a head start.
          </h2>
          <p>
            Posters, identities, social posts, editorial layouts and more.
            Search nine categories, choose a size, and make every shape and
            every word your own.
          </p>
          <p className="muted">
            All 52 are in the app now. Portrait, square or landscape. Built-in
            sizes or a canvas of your own.
          </p>
          <a
            className="text-link"
            href={`${sitePath("docs/manual")}#templates`}
          >
            Meet the template library <Arrow />
          </a>
        </div>
        <figure className="template-visual">
          <Shot
            name="templates.webp"
            alt="The native library of 52 editable templates with category filters and document-size controls."
          />
          <figcaption>Made to remix. Ready to make yours.</figcaption>
        </figure>
      </section>

      <section className="section shell film-section" id="film">
        <div className="section-heading">
          <div>
            <Eyebrow>06 / SEE IT MOVE</Eyebrow>
            <h2>
              From a point
              <br />
              to a little personality.
            </h2>
          </div>
          <p>
            A quick trip through the current studio.
            <br />
            Precision, colour, texture and motion—in 97 seconds.
          </p>
        </div>
        <video
          controls
          playsInline
          preload="none"
          poster={media("omadesign-logo.webp")}
          width="1920"
          height="1080"
          aria-label="omadesign feature demonstration, 97 seconds"
        >
          <source src={media("film.mp4")} type="video/mp4" />
          <track
            kind="captions"
            src={media("film.vtt")}
            srcLang="en"
            label="English scene descriptions"
          />
          <p>
            <a href={media("film.mp4")}>Download the demonstration video</a>.
          </p>
        </video>
        <div className="film-caption">
          <span>
            Captured in the native studio · Music, no spoken narration
          </span>
          <a href={media("film.mp4")} download>
            Keep a copy ↓
          </a>
        </div>
      </section>

      <FeatureExplorer />

      <section className="section shell faq-section" id="questions">
        <div>
          <Eyebrow>08 / THE PRACTICAL BITS</Eyebrow>
          <h2>
            Good questions.
            <br />
            Straight answers.
          </h2>
          <p>
            Built in the open. That includes being clear about what works today.
          </p>
          <a className="text-link" href={sitePath("docs/roadmap")}>
            See where we’re headed <Arrow />
          </a>
        </div>
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
              Affinity’s supported desktop platforms are Windows and macOS.
              omadesign starts with native Linux, Omarchy theme integration and
              a brand kit made of portable dotfiles. It brings vector design,
              painting, photo adjustments and object animation into one app.
              It’s an independent alpha with its own workflow and scope.
            </p>
            <p>
              <a href="https://www.affinity.studio/get-affinity">
                Affinity’s current platform information ↗
              </a>{" "}
              ·{" "}
              <a href={sitePath("docs/manual")}>
                omadesign’s capabilities and format support →
              </a>
            </p>
          </details>
        </div>
      </section>
      <Install />
    </main>
  );
}
