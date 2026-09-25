import type { PublicMetrics } from "../server/public-metrics";

/**
 * The four public count shields.
 * Same two-tone chips as the README: name on the left, number on the right.
 * @param metrics The public counts, or null when downloads could not be read.
 * @returns The shield row, or nothing.
 */
export function MetricChips({ metrics }: { metrics: PublicMetrics | null }) {
  const chips = metrics?.chips.filter(chip => chip.display) ?? [];
  if (!chips.length) return null;
  return (
    <nav className="metric-chips" aria-label="omadesign counts">
      {chips.map(chip => (
        <a key={chip.key} className="shield" href={chip.href} title={chip.title}>
          <span>{chip.label}</span>
          <span style={{ background: `#${chip.color}` }}>{chip.display}</span>
        </a>
      ))}
    </nav>
  );
}
