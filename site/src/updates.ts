export type UpdatePost = {
  slug: string;
  version: string;
  date: string;
  title: string;
  dek: string;
};

export const updates: UpdatePost[] = [
  {
    slug: "0.5.8", version: "0.5.8", date: "2026-09-22",
    title: "Make it your studio",
    dek: "Lua plugins with working examples, editable compound paths, shared corner-radius controls, guide locking and a steadier welcome screen with the transparent mark.",
  },
  {
    slug: "0.5.7",
    version: "0.5.7",
    date: "2026-09-21",
    title: "A new mark. A quicker look.",
    dek: "The new official Omadesign mark across the app and website, plus a fresh 32-second silent product film. The Cloud announcement stays in place.",
  },
  {
    slug: "0.5.6",
    version: "0.5.6",
    date: "2026-09-21",
    title: "Your work, ready when you are",
    dek: "A new welcome screen finds your documents and brand projects, previews them at their natural proportions, and connects your brief or question to your Omarchy agent. Discord, offline docs, and one stable Linux release.",
  },
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
