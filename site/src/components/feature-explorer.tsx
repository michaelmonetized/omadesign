import { useState } from "react";
import { featureGroups } from "../features";
import { sitePath } from "../site";
import { Arrow, Eyebrow } from "./studio-ui";

export function FeatureExplorer() {
  const [query, setQuery] = useState("");
  const [category, setCategory] = useState("all");
  const needle = query.trim().toLowerCase();
  const groups = featureGroups
    .filter((group) => category === "all" || group.id === category)
    .map((group) => ({
      ...group,
      features: group.features.filter((feature) =>
        `${feature.name} ${feature.description} ${group.title}`
          .toLowerCase()
          .includes(needle),
      ),
    }))
    .filter((group) => group.features.length > 0);
  const count = groups.reduce(
    (total, group) => total + group.features.length,
    0,
  );
  return (
    <section className="section shell features-section" id="features">
      <div className="section-heading">
        <div>
          <Eyebrow>07 / THE WHOLE TOOLBOX</Eyebrow>
          <h2>
            Small details.
            <br />
            Serious possibilities.
          </h2>
        </div>
        <p>
          From a single anchor point to a complete brand system. Find the tools
          for the work you want to do.
        </p>
      </div>
      <div className="feature-controls">
        <label className="search-box">
          <span aria-hidden="true">⌕</span>
          <span className="sr-only">Search features</span>
          <input
            type="search"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Find a tool, a format, a possibility…"
          />
        </label>
        <label className="category-label">
          <span className="sr-only">Feature category</span>
          <select
            value={category}
            onChange={(event) => setCategory(event.target.value)}
          >
            <option value="all">Every part of the studio</option>
            {featureGroups.map((group) => (
              <option key={group.id} value={group.id}>
                {group.title}
              </option>
            ))}
          </select>
        </label>
        <span className="feature-count" role="status">
          {count} features
        </span>
      </div>
      <div className="feature-groups">
        {groups.map((group, index) => (
          <details
            key={`${group.id}-${Boolean(needle)}-${category}`}
            open={needle.length > 0 || category !== "all" || index === 0}
          >
            <summary>
              <span className="group-number">
                {String(
                  featureGroups.findIndex((g) => g.id === group.id) + 1,
                ).padStart(2, "0")}
              </span>
              <span>
                {group.title}
                <small>{group.intro}</small>
              </span>
              <span className="group-count">{group.features.length}</span>
              <span className="disclosure-plus" aria-hidden="true">
                +
              </span>
            </summary>
            <div className="feature-grid">
              {group.features.map((feature) => (
                <article key={feature.name}>
                  <h3>{feature.name}</h3>
                  <p>{feature.description}</p>
                </article>
              ))}
            </div>
          </details>
        ))}
      </div>
      {count === 0 ? (
        <div className="empty-search">
          <h3>No matching tools.</h3>
          <p>Try “mask”, “fonts”, “guides” or “animation”.</p>
          <button
            className="text-link"
            type="button"
            onClick={() => {
              setQuery("");
              setCategory("all");
            }}
          >
            Show every feature <Arrow />
          </button>
        </div>
      ) : null}
      <p className="feature-note">
        This tour follows the current source. For formats, gestures and
        supported workflows,{" "}
        <a href={sitePath("docs/manual")}>read the full manual →</a>
      </p>
    </section>
  );
}
