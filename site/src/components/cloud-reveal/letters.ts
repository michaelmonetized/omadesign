import { cloudBrand } from './brand';

/**
 * Exact source contours, grouped into the nine letters of “omadesign”.
 * Keep every original coordinate: animation reveals these filled paths without
 * replacing the custom lettering with a font or a fitted centerline.
 * The I has a tiny second contour at its lower-right serif (indices 6 and 7).
 * Every letter uses cloudBrand.wordmark.viewBox and the original evenodd fill.
 */
const contours = cloudBrand.wordmark.paths[0].d.match(/M[^M]+/g) ?? [];

/** Exact polygon bounds in the shared wordmark coordinate system. */
export const cloudLetterBounds = [
  { x: 64.186, y: 895.384, width: 176.314, height: 176.313 },
  { x: 276.110, y: 894.500, width: 175.195, height: 177.608 },
  { x: 486.430, y: 895.384, width: 175.844, height: 176.451 },
  { x: 698.242, y: 900.497, width: 177.109, height: 173.003 },
  { x: 911.319, y: 896.341, width: 178.084, height: 175.850 },
  { x: 1123.217, y: 896.347, width: 176.250, height: 175.861 },
  { x: 1335.304, y: 896.347, width: 175.626, height: 175.861 },
  { x: 1545.767, y: 896.358, width: 178.084, height: 175.850 },
  { x: 1758.046, y: 894.500, width: 177.768, height: 177.608 },
];

export const cloudLetters: Array<{
  d: string;
  fillRule: 'evenodd';
  bounds: { x: number; y: number; width: number; height: number };
}> = [
  [0], [1], [2], [3], [4], [5], [6, 7], [8], [9],
].map((indices, letterIndex) => ({
  d: indices.map((index) => contours[index]).join(''),
  fillRule: 'evenodd',
  bounds: cloudLetterBounds[letterIndex],
}));
