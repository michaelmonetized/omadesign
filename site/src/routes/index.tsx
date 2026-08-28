import { createFileRoute } from "@tanstack/react-router";
import { CURL } from "./__root";

export const Route = createFileRoute("/")({
  component: Home,
});

const shots = [
  { src: "/media/design.jpg", cap: "Design — marks, type, boolean" },
  { src: "/media/paint.jpg", cap: "Pixel — a brush that lives on a layer" },
  { src: "/media/photo.jpg", cap: "Photo — develop, then Place in Design" },
  { src: "/media/type.jpg", cap: "Type — caret, Character, OpenType" },
];

function Home() {
  return (
    <main>
      <section className="mx-auto max-w-6xl px-6 pb-8 pt-16 md:pt-24">
        <p className="mb-4 text-sm uppercase tracking-[0.2em] text-ctp-overlay1">
          Updated with 1.0.4
        </p>
        <h1 className="max-w-4xl text-5xl font-semibold leading-[1.05] tracking-tight text-ctp-text md:text-7xl">
          Your Linux, for making things.
        </h1>
        <p className="mt-6 max-w-2xl text-lg text-ctp-subtext0 md:text-xl">
          A native studio. Design, paint, photograph. One document, one layer
          stack. Theme from ~/.config. Type you can type into.
        </p>
        <ul className="mt-8 flex flex-wrap gap-x-6 gap-y-2 text-sm text-ctp-subtext1">
          <li>Free, MIT</li>
          <li>aarch64 Asahi + x86_64</li>
          <li>glibc 2.35</li>
          <li>No Electron</li>
        </ul>
        <div className="mt-10 flex flex-col gap-3 md:flex-row md:items-center">
          <code className="flex-1 overflow-x-auto rounded-xl border border-ctp-surface1 bg-ctp-mantle px-4 py-3 font-mono text-sm text-ctp-green">
            {CURL}
          </code>
          <a
            href="https://github.com/michaelmonetized/omadesign/releases"
            className="rounded-xl bg-ctp-lavender px-5 py-3 text-center text-sm font-medium text-ctp-base"
          >
            Releases
          </a>
        </div>
      </section>

      <section className="mx-auto max-w-6xl px-6 pb-20">
        <div className="overflow-hidden rounded-3xl border border-ctp-surface0 bg-ctp-mantle shadow-2xl shadow-ctp-crust/40">
          <video
            className="aspect-video w-full bg-ctp-crust object-cover"
            autoPlay
            muted
            loop
            playsInline
            poster="/media/design.jpg"
          >
            <source src="/media/hero.mp4" type="video/mp4" />
          </video>
        </div>
        <p className="mt-3 text-center text-xs text-ctp-overlay1">
          Drag a box to zoom. Click T and type. The well is Phosphor Light.
        </p>
      </section>

      <section className="border-y border-ctp-surface0 bg-ctp-mantle/40 py-16">
        <div className="mx-auto max-w-6xl px-6">
          <h2 className="text-3xl font-semibold tracking-tight md:text-4xl">
            Three rooms, one house.
          </h2>
          <p className="mt-3 max-w-2xl text-ctp-subtext0">
            Switch persona from the top bar. Keys stay where they already live
            in your fingers.
          </p>
          <div className="mt-10 grid gap-6 md:grid-cols-3">
            {[
              ["Design", "V · P · R · T", "Marks, posters, boolean, live type."],
              ["Pixel", "B · E · J · W", "Brush, clone, wand. On a raster layer."],
              ["Photo", "C · develop", "Grade, crop, Place in Design."],
            ].map(([name, keys, blurb]) => (
              <div
                key={name}
                className="rounded-2xl border border-ctp-surface0 bg-ctp-base p-6"
              >
                <h3 className="text-xl font-medium text-ctp-lavender">{name}</h3>
                <p className="mt-1 font-mono text-xs text-ctp-overlay1">{keys}</p>
                <p className="mt-4 text-ctp-subtext0">{blurb}</p>
              </div>
            ))}
          </div>
        </div>
      </section>

      <section className="mx-auto max-w-6xl px-6 py-20">
        <h2 className="text-3xl font-semibold tracking-tight md:text-4xl">
          Just some of the highlights
        </h2>
        <div className="mt-10 grid gap-10 md:grid-cols-2">
          {shots.map((s) => (
            <figure key={s.src}>
              <img
                src={s.src}
                alt={s.cap}
                className="aspect-video w-full rounded-2xl border border-ctp-surface0 object-cover"
              />
              <figcaption className="mt-3 text-sm text-ctp-subtext1">{s.cap}</figcaption>
            </figure>
          ))}
        </div>
      </section>

      <section className="border-y border-ctp-surface0 py-16">
        <div className="mx-auto max-w-6xl px-6">
          <h2 className="text-3xl font-semibold tracking-tight">
            Built like desktop software.
          </h2>
          <div className="mt-8 grid gap-6 md:grid-cols-2">
            {[
              [
                "Your chrome, not ours",
                "Colours from the Omarchy theme on disk. Font from fontconfig. No baked orange.",
              ],
              [
                "Asahi is first class",
                "aarch64 tarball, zig-linked to glibc 2.35. Same release as x86_64.",
              ],
              [
                "Type is a string",
                "Caret, Enter for a line, Character studio, OpenType kern/liga/tnum/smcp.",
              ],
              [
                "No runners",
                "Binaries are built on this machine and uploaded. Microsoft does not get a penny.",
              ],
            ].map(([t, d]) => (
              <div key={t}>
                <h3 className="font-medium text-ctp-text">{t}</h3>
                <p className="mt-2 text-ctp-subtext0">{d}</p>
              </div>
            ))}
          </div>
        </div>
      </section>

      <section className="mx-auto max-w-6xl px-6 py-20">
        <h2 className="text-3xl font-semibold tracking-tight">Take this with you</h2>
        <p className="mt-3 text-ctp-subtext0">
          Manual, contributing, roadmap. The site is Catppuccin mocha until you
          ask for light.
        </p>
        <div className="mt-8 flex flex-wrap gap-4">
          <a className="rounded-xl border border-ctp-surface1 px-4 py-2 text-sm" href="/docs/manual">
            User manual
          </a>
          <a
            className="rounded-xl border border-ctp-surface1 px-4 py-2 text-sm"
            href="/docs/contributing"
          >
            Contributing
          </a>
          <a className="rounded-xl border border-ctp-surface1 px-4 py-2 text-sm" href="/docs/roadmap">
            Roadmap
          </a>
        </div>
        <img
          src="/media/mark.png"
          alt="omadesign mark"
          className="mt-12 max-h-64 rounded-2xl border border-ctp-surface0"
        />
      </section>
    </main>
  );
}
