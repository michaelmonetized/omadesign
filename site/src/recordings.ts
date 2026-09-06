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
    id: "design",
    name: "Design",
    duration: 42,
    chapters: [
      { name: "Layout", at: 0 },
      { name: "Appearance", at: 6 },
      { name: "Typography", at: 12 },
      { name: "Reshape", at: 18 },
      { name: "Effects", at: 24 },
      { name: "Trace", at: 30 },
      { name: "Layers", at: 36 },
    ],
  },
  {
    id: "pixel",
    name: "Pixel",
    duration: 30,
    chapters: [
      { name: "Brush", at: 0 },
      { name: "Retouch", at: 6 },
      { name: "Mask", at: 12 },
      { name: "Color", at: 18 },
      { name: "Layers", at: 24 },
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
      { name: "Library", at: 18 },
    ],
  },
  {
    id: "motion",
    name: "Motion",
    duration: 30,
    chapters: [
      { name: "Presets", at: 0 },
      { name: "Keyframes", at: 6 },
      { name: "Appearance", at: 12 },
      { name: "Reshape in Design", at: 18 },
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
