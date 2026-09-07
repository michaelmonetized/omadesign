import { media, sitePath } from "../site";
import { RELEASE_TAG } from "../release";
import { Arrow } from "./studio-ui";

export function FilePreview() {
  return (
    <section className="section shell file-preview" id="file-support">
      <div className="file-preview-heading">
        <div>
          <p className="preview-label">Development preview</p>
          <h2>Bring your layers.<br />Develop your originals.</h2>
        </div>
        <p className="file-preview-intro">
          More of your work, in one native studio. Layered interchange and camera
          RAW development are ready to try from source. Published downloads remain{" "}
          {RELEASE_TAG}.
        </p>
      </div>
      <div className="file-preview-workflows">
        <article>
          <span className="file-preview-formats">PSD · PSB · PDF · AI · SVG · ORA</span>
          <h3>Pick up with your layers.</h3>
          <p>
            Open supported layers, groups, masks and artboards. Import Affinity
            documents through an optional bridge. Save an editable <code>.oma</code>{" "}
            working copy, with conversion notes that explain what changed.
          </p>
          <a className="text-link" href={sitePath("docs/formats/")}>
            Compare format support <Arrow />
          </a>
        </article>
        <article>
          <span className="file-preview-formats">DNG · CR2 / CR3 · NEF · ARW · RAF + more</span>
          <h3>Start with the sensor.</h3>
          <p>
            Develop full-resolution RAW photos with 16-bit linear pixels, inspect
            real detail at 100%, and export 16-bit PNG or TIFF. Save adjustments
            beside the original in an <code>.omaphoto</code> file.
          </p>
          <a className="text-link" href={`${sitePath("docs/formats/")}#camera-raw`}>
            Explore RAW development <Arrow />
          </a>
        </article>
      </div>
      <figure className="file-preview-shot">
        <a href={media("raw-preview.webp")} aria-label="View the full-size RAW Photo workspace screenshot">
          <img
            src={media("raw-preview.webp")}
            width="1600"
            height="1000"
            loading="lazy"
            decoding="async"
            alt="Omadesign Photo workspace developing a Fujifilm RAW landscape, with camera metadata, histogram, and light controls."
          />
        </a>
        <figcaption>
          <span>From the native app · Fujifilm X-T30 II · 6246 × 4170 RAW</span>
          <a href="https://raw.pixls.us/">Photo: raw.pixls.us · CC0</a>
        </figcaption>
      </figure>
      <div className="file-preview-footer">
        <p>
          Compatibility varies by document, camera and compression. Affinity
          import is partial; the new unified <code>.af</code> format is still
          awaiting verification with a real file. RAW placement in Design uses 8-bit pixels.
        </p>
        <a className="button" href={`${sitePath("docs/")}#development-preview`}>
          Try the preview <Arrow />
        </a>
      </div>
    </section>
  );
}
