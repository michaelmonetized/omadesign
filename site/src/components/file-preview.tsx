import { sitePath } from "../site";
import { RELEASE_TAG } from "../release";
import { Arrow } from "./studio-ui";

export function FilePreview() {
  return (
    <section className="section shell file-preview" id="file-support">
      <div className="file-preview-heading">
        <div>
          <p className="preview-label">Included in {RELEASE_TAG}</p>
          <h2>
            Bring your layers.
            <br />
            Grade the whole shoot.
          </h2>
        </div>
        <p className="file-preview-intro">
          Copy a look across photos, save presets, and develop a folder in the
          background. {RELEASE_TAG} includes layered interchange and native
          Linux downloads for ARM64 and x86_64.
        </p>
      </div>
      <div className="file-preview-workflows">
        <article>
          <span className="file-preview-formats">
            PSD · PSB · XCF · PDF · AI · SVG · ORA
          </span>
          <h3>Pick up with your layers.</h3>
          <p>
            Open supported layers, groups, masks and artboards. Import Affinity
            documents through an optional bridge. Save an editable{" "}
            <code>.oma</code> working copy, with conversion notes that explain
            what changed.
          </p>
          <a className="text-link" href={sitePath("docs/formats/")}>
            Compare format support <Arrow />
          </a>
        </article>
        <article>
          <span className="file-preview-formats">
            DNG · CR2 / CR3 · NEF · ARW · RAF + more
          </span>
          <h3>One look. Every photo.</h3>
          <p>
            Develop a RAW photo, copy its adjustments, and apply them to
            selected photos or a whole folder. Save shareable{" "}
            <code>.omapreset</code> looks and keep each original’s edits in its
            own <code>.omaphoto</code> file.
          </p>
          <a className="text-link" href={`${sitePath("docs/manual/")}#photo`}>
            Explore Photo workflows <Arrow />
          </a>
        </article>
      </div>
      <figure className="file-preview-shot">
        <a
          href={sitePath("media/refresh/photo.webp")}
          aria-label="View the full-size Photo workspace screenshot"
        >
          <img
            src={sitePath("media/refresh/photo.webp")}
            width="1600"
            height="900"
            loading="lazy"
            decoding="async"
            alt="Native Omadesign Photo workspace showing a generated landscape and development controls."
          />
        </a>
        <figcaption>
          <span>Fresh native app capture · Photo development</span>
          <span>Generated example landscape</span>
        </figcaption>
      </figure>
      <div className="file-preview-footer">
        <p>
          Compatibility varies by document, camera and compression. Affinity
          import is partial; the new unified <code>.af</code> format is still
          awaiting verification with a real file. RAW placement in Design uses
          8-bit pixels.
        </p>
        <a className="button" href="#install">
          Get {RELEASE_TAG} <Arrow />
        </a>
      </div>
    </section>
  );
}
