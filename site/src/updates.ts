export type UpdatePost = {
  slug: string;
  version: string;
  date: string;
  title: string;
  dek: string;
};

export const updates: UpdatePost[] = [
  {
    slug: "0.5.4",
    version: "0.5.4",
    date: "2026-09-20",
    title: "More control on the canvas",
    dek: "Chroma key, 30 raster treatments, four gradient types for fills and strokes, a clearer color picker, and responsive Layout components and prototypes. Built on the 0.5.3 clipboard and cloud work.",
  },
  {
    slug: "0.5.3",
    version: "0.5.3",
    date: "2026-09-20",
    title: "Paste it onto the canvas",
    dek: "Screenshots, copied images, text, and SVG now paste into the visible canvas center. Objects copied inside Omadesign keep their positions.",
  },
  {
    slug: "0.5.1",
    version: "0.5.1",
    date: "2026-09-18",
    title: "You can see the selection now",
    dek: "Pixel mode had a well of tools. Most of them did nothing you could see. 0.5.1 makes the rest of the studio real.",
  },
  {
    slug: "0.5.0",
    version: "0.5.0",
    date: "2026-09-11",
    title: "Layout, then show it",
    dek: "A fifth studio. Nested frames, stacks, constraints, and a site with the door closed until you publish.",
  },
];

export const latestUpdate = updates[0];

export function updateBySlug(slug: string) {
  return updates.find((post) => post.slug === slug);
}
