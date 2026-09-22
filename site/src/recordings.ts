export type Chapter = { name: string; at: number };
export type Recording = {
  id: string;
  name: string;
  duration: number;
  chapters: Chapter[];
};

// Chapter offsets are checked against the native capture manifest before release.
export const recordings: Recording[] = [
  {
    id: "layout",
    name: "Layout",
    duration: 15,
    chapters: [
      { name: "Present", at: 0 },
      { name: "Draw a frame", at: 7 },
      { name: "Undo", at: 12 },
    ],
  },
  {
    id: "graphics",
    name: "Design",
    duration: 17,
    chapters: [
      { name: "Gradient placement", at: 0 },
      { name: "Gradient direction", at: 7 },
      { name: "Group and undo", at: 11 },
    ],
  },
  {
    id: "chroma",
    name: "Pixel",
    duration: 15,
    chapters: [
      { name: "Compare and sample", at: 0 },
      { name: "Apply and undo", at: 6 },
      { name: "Reopen preview", at: 11 },
    ],
  },
  {
    id: "photo",
    name: "Photo",
    duration: 24,
    chapters: [
      { name: "Light", at: 0 },
      { name: "Color", at: 6 },
      { name: "Detail", at: 12 },
      { name: "Compare", at: 18 },
    ],
  },
  {
    id: "motion",
    name: "Motion",
    duration: 30,
    chapters: [
      { name: "Presets", at: 0 },
      { name: "Keyframes", at: 6 },
      { name: "Pop in", at: 12 },
      { name: "Slide up", at: 18 },
      { name: "Layers", at: 24 },
    ],
  },
];
export const brandRecording: Recording = {
  id: "brand-kit",
  name: "Brand kit",
  duration: 30,
  chapters: [
    { name: ".omacolors", at: 0 },
    { name: ".omabrand", at: 10 },
    { name: ".omatype", at: 20 },
  ],
};
