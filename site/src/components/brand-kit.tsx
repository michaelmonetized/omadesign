import { brandRecording } from "../recordings";
import { sitePath } from "../site";
import { RecordingPlayer } from "./native-recordings";
import { RevealText } from "./reveal-text";

export function BrandKit() {
  return (
    <section className="brand-recordings" id="brand">
      <div className="section shell">
        <div className="section-heading">
          <RevealText lines={["Your brand.", "Your files."]} />
          <p>
            Named palettes, reusable artwork, and font roles. Stored beside your
            project.
          </p>
        </div>
        <RecordingPlayer recording={brandRecording} />
        <a
          className="text-link"
          href={`${sitePath("docs/manual")}#palettes-and-brand-libraries`}
        >
          Brand file guide ↗
        </a>
      </div>
    </section>
  );
}
