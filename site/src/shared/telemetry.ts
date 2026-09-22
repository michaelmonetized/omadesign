/** Closed schema: adding a metric requires an explicit code change and review. */
const tools = "select node pen pencil rect ellipse polygon star line text gradient eyedropper trace brush eraser fill clone heal smudge crop marquee ellipse_marquee lasso wand hand zoom artboard frame".split(" ");
export const METRICS = new Set([
  "active.day", "active.week", "active.month",
  ..."design pixel photo motion layout".split(" ").map(x => `mode.${x}`),
  ...tools.map(x => `tool.${x}`),
  ..."create edit group mask duplicate align import save export undo redo update".split(" ").map(x => `feature.${x}`),
  ..."import save export update recovery panic".split(" ").map(x => `error.${x}`),
  ..."segv abort bus ill panic".split(" ").map(x => `crash.${x}`),
]);
export type Count = { period: string; metric: string; count: number };
export type Batch = { schema: 1; release: string; platform: "linux-x86_64" | "linux-aarch64"; counts: Count[] };
function exact(value: unknown, keys: string[]): value is Record<string, unknown> {
  return !!value && typeof value === "object" && !Array.isArray(value) && Object.keys(value).length === keys.length && keys.every(k => Object.hasOwn(value, k));
}
export function parseBatch(value: unknown, now = new Date()): Batch | null {
  if (!exact(value, ["schema", "release", "platform", "counts"]) || value.schema !== 1 ||
    typeof value.release !== "string" || !/^\d{1,4}\.\d{1,4}\.\d{1,4}(?:-(?:nightly|alpha|beta|rc)\.\d{1,6})?$/.test(value.release) ||
    !["linux-x86_64", "linux-aarch64"].includes(value.platform as string) || !Array.isArray(value.counts) || value.counts.length < 1 || value.counts.length > 128) return null;
  const seen = new Set<string>();
  for (const item of value.counts) {
    if (!exact(item, ["period", "metric", "count"]) || typeof item.metric !== "string" || !METRICS.has(item.metric) ||
      !Number.isSafeInteger(item.count) || (item.count as number) < 1 || (item.count as number) > 10000 ||
      typeof item.period !== "string") return null;
    const month = item.metric === "active.month";
    if (!(month ? /^\d{4}-\d{2}$/ : /^\d{4}-\d{2}-\d{2}$/).test(item.period)) return null;
    const date = new Date(`${item.period}${month ? "-01" : ""}T00:00:00Z`);
    if (!Number.isFinite(date.getTime()) || date.toISOString().slice(0, month ? 7 : 10) !== item.period ||
      date.getTime() > now.getTime() + 86400000 || date.getTime() < now.getTime() - 40 * 86400000 ||
      (item.metric === "active.week" && date.getUTCDay() !== 1) ||
      (item.metric.startsWith("active.") && item.count !== 1)) return null;
    const key = `${item.period}:${item.metric}`;
    if (seen.has(key)) return null;
    seen.add(key);
  }
  return value as Batch;
}
