import type { PublicMetrics } from "../server/public-metrics";

/**
 * The four public count chips.
 * Renders `[ N | label ]` for each count that was actually read.
 * @param metrics The public counts, or null when downloads could not be read.
 * @returns The chip row, or nothing.
 */
export function MetricChips({ metrics }: { metrics: PublicMetrics | null }) {
  const chips = metrics?.chips.filter(chip => chip.display) ?? [];
  if (!chips.length) return null;
  return (
    <nav className="metric-chips" aria-label="omadesign counts">
      {chips.map(chip => (
        <a key={chip.key} href={chip.href} title={chip.title}>
          [ <strong>{chip.display}</strong> | {chip.label} ]
        </a>
      ))}
    </nav>
  );
}
