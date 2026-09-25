import { CloudIntro } from "../cloud-waitlist";
import { SITE_ORIGIN } from "../../site";
import "./preview.css";

/** The same scrollable announcement used on the homepage, with a real page below it. */
export function CloudDreamscapePreview() {
  return (
    <main className="cloud-dreamscape-preview">
      <CloudIntro siteBase={SITE_ORIGIN} />
      <section className="dreamscape-product" id="omadesign-product" aria-labelledby="dreamscape-product-title">
        <p className="dreamscape-eyebrow">Made for your desktop</p>
        <h1 id="dreamscape-product-title">Every idea.<br />One native creative studio.</h1>
        <p className="dreamscape-product-intro">Design, layout, paint, develop and animate. Built for Linux.</p>
        <div className="dreamscape-product-links">
          <a href={`${SITE_ORIGIN}/`}>Explore Omadesign <span aria-hidden="true">↗</span></a>
          <a href={`${SITE_ORIGIN}/docs/cloud`}>How cloud collaboration works <span aria-hidden="true">↗</span></a>
        </div>
        <div className="dreamscape-product-details">
          <article><span>01</span><h2>Create locally.</h2><p>Keep the tools and your creative work on your desktop, with a native studio built for Linux.</p></article>
          <article><span>02</span><h2>Share deliberately.</h2><p>Upload project files and assets, then give the right people access to your project.</p></article>
          <article><span>03</span><h2>Bring feedback home.</h2><p>Collect comments and annotations on shared snapshots while you keep designing in the native app.</p></article>
        </div>
        <a className="dreamscape-return" href="#cloud">Return to the clouds <span aria-hidden="true">↑</span></a>
      </section>
    </main>
  );
}
