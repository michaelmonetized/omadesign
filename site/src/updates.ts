export type UpdatePost = {
  slug: string;
  version: string;
  date: string;
  title: string;
  dek: string;
};

export const updates: UpdatePost[] = [
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
