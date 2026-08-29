import { createFileRoute } from "@tanstack/react-router";
import { CURL } from "./__root";

export const Route = createFileRoute("/")({
  component: Home,
});

const shots = [
  { src: "/media/design.jpg", cap: "Design — resize from any tool, compound, boolean" },
  { src: "/media/paint.jpg", cap: "Pixel — brush, clone, wand on a raster layer" },
  { src: "/media/photo.jpg", cap: "Photo — develop, crop, Place in Design" },
  { src: "/media/type.jpg", cap: "Type — caret + Google Fonts on-demand + max font default + palettes" },
  { src: "/media/shapes.jpg", cap: "Shapes — Phosphor / LineIcons / Heroicons / Feather live SVG browser" },
  { src: "/media/assets.jpg", cap: "Assets — Pixabay / Pexels / Vecteezy / Picsum free browser" },
];

function Home() {
  return (
    <main>
      <section className="mx-auto max-w-6xl px-6 pb-8 pt-16 md:pt-24">
        <p className="mb-4 text-sm uppercase tracking-[0.2em] text-ctp-overlay1">
          Updated with v0.0.0.0alpha-rc — 7 stacked PRs merged
        </p>
        <h1 className="max-w-4xl text-5xl font-semibold leading-[1.05] tracking-tight text-ctp-text md:text-7xl">
          Your Linux, for making things.
        </h1>
        <p className="mt-6 max-w-2xl text-lg text-ctp-subtext0 md:text-xl">
          A native studio. Design, paint, photograph. One document, one layer
          stack. Theme from ~/.config. Type you can type into. Now with shape &
          asset browsers and a big welcome.
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
          New in alpha-rc — drag any handle to resize (Shift uniform, Alt from centre), Google Fonts on tap, palettes, compound, shape & asset browsers, and the big welcome with tabs. Remotion 4.0.518 hero (45 s, 1920×1080, 2.5 MB, 1350f) with in-app pane mocks.
        </p>
      </section>

      <section className="mx-auto max-w-6xl px-6 pb-16">
        <div className="rounded-3xl border border-ctp-surface0 bg-ctp-mantle p-8">
          <h2 className="text-2xl font-semibold tracking-tight">What&apos;s new in v0.0.0.0alpha-rc</h2>
          <p className="mt-2 text-sm text-ctp-subtext0">7 commits, 6 stacked PRs → fast-forward to master, built locally with zig cc (glibc 2.35).</p>
          <div className="mt-6 grid gap-4 md:grid-cols-3">
            {[
              ["Resize anywhere", "Handles now work from Rect/Ellipse/Star/Line. Group bbox + precise X/Y/W/H/Rotate/Flip in Transform."],
              ["Fonts, full", "fontconfig + ttf_parser, 2000 cap. Google Fonts browser (30 bundled, ureq → ~/.local/share/fonts/omadesign/google)."],
              ["Max font", "Scans ~/Projects next/font/google, most frequent wins (Inter). Shown as default."],
              ["Palettes", "Custom palettes at ~/.config/omadesign/palettes.json — New/Rename/Delete, +Fill/+Stroke, Import/Export."],
              ["Compound", "Combine (even-odd) / Release + multi-boolean Union/Subtract/Intersect/Xor."],
              ["Browsers", "◇ Shapes (Phosphor/LineIcons/Heroicons/Feather) + ⬙ Assets (Pixabay/Pexels/Picsum fallback)."],
              ["Welcome 2.0", "Big 720 px square, tabs All/Web/Print/Social/Photo/Identity, 210×112 cards, transparent/bleed/safe/artboards ×1–16."],
              ["Canvas", "Artboard frames, bleed red + crop marks, safe green inset. Checker when transparent."],
              ["Lander", "Regenerated 6 screenshots via gen_media + Remotion 4.0.518 hero.mp4 (45 s, mock panes for shape/asset/fonts)."],
            ].map(([t, d]) => (
              <div key={t} className="rounded-2xl border border-ctp-surface0 bg-ctp-base p-4">
                <h3 className="text-sm font-semibold text-ctp-text">{t}</h3>
                <p className="mt-1 text-sm leading-snug text-ctp-subtext0">{d}</p>
              </div>
            ))}
          </div>
        </div>
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
        <p className="mt-2 text-sm text-ctp-subtext0">Screenshots regenerated headless via <code className="rounded bg-ctp-surface0 px-1 py-0.5 font-mono text-xs">cargo run --bin gen_media</code> + <code className="rounded bg-ctp-surface0 px-1 py-0.5 font-mono text-xs">compositor::export_png</code> → JPEG.</p>
        <div className="mt-10 grid gap-6 md:grid-cols-3">
          {shots.map((s) => (
            <figure key={s.src}>
              <img
                src={s.src}
                alt={s.cap}
                className="aspect-[16/9] w-full rounded-2xl border border-ctp-surface0 object-cover"
              />
              <figcaption className="mt-3 text-sm leading-snug text-ctp-subtext1">{s.cap}</figcaption>
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
