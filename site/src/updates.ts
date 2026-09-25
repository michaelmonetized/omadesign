export type UpdatePost = {
  slug: string;
  version: string;
  date: string;
  title: string;
  dek: string;
};

export const updates: UpdatePost[] = [
  {
    slug: "0.6.0", version: "0.6.0", date: "2026-09-25",
    title: "Key the design you already made",
    dek: "Motion keys position, width, height, rotation, opacity, gradient angle, fill, stroke width, dash, gap, and dash length. Blend stays a design edit. The brush draws its edge. Clone has size, edge, opacity, flow, and an aligned source. A selection can move, resize, feather, and distort. Apple glass displaces through turbulence. Simplify drops extra nodes. Libraries show thumbnails. Pickers and zoom sit at the edges. Color pickers sample the screen and copy hex.",
  },
  {
    slug: "0.5.9", version: "0.5.9", date: "2026-09-24",
    title: "Click the pixel you mean",
    dek: "The eyedropper samples any pixel on the screen. Alt subtracts from a selection. Dilate and erode hold a hard cutout. Welcome links come forward. Cloud sign-in tells you you're in. Midnight Duotone bakes a vector page. The pen drops its forward handle.",
  },
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
