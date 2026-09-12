export type ShowcaseItem = {
  id: string;
  title: string;
  author: string;
  tags: string[];
  summary: string;
  published: boolean;
};

export const showcaseSeed: ShowcaseItem[] = [
  {
    id: "seed-mobile",
    title: "Night ferry boarding",
    author: "omadesign",
    tags: ["layout", "mobile"],
    summary: "A nested phone frame with an auto-stacked boarding pass.",
    published: true,
  },
  {
    id: "seed-hero",
    title: "Harbor landing",
    author: "omadesign",
    tags: ["layout", "web"],
    summary: "A landing hero with a stretching copy column and a pinned call to action.",
    published: true,
  },
  {
    id: "seed-dash",
    title: "Studio dashboard",
    author: "omadesign",
    tags: ["layout", "dashboard"],
    summary: "Sidebar navigation with a stretching content pane.",
    published: true,
  },
];

export const publishedShowcase = (items: ShowcaseItem[]) =>
  items.filter((item) => item.published);
