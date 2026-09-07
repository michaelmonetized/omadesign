import { RevealText } from "./reveal-text";
import { useState } from "react";
import { featureGroups } from "../features";
import { sitePath } from "../site";
import { Arrow } from "./studio-ui";

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
      <div className="section-heading" data-motion>
        <RevealText text="Features" />
      </div>
      <div className="feature-controls">
        <label className="search-box">
          <span aria-hidden="true">⌕</span>
          <span className="sr-only">Search features</span>
          <input
            type="search"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Search tools and formats"
          />
        </label>
        <label className="category-label">
          <span className="sr-only">Feature category</span>
          <select
            value={category}
            onChange={(event) => setCategory(event.target.value)}
          >
            <option value="all">All categories</option>
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
            data-motion
            key={`${group.id}-${Boolean(needle)}-${category}`}
            open={needle.length > 0 || category !== "all" || index === 0}
          >
            <summary>
              <span className="feature-group-title">{group.title}</span>
              <span className="group-count">{group.features.length}</span>
              <span className="disclosure-plus" aria-hidden="true">
                +
              </span>
            </summary>
            <div className="feature-grid">
              {group.features.map((feature) => (
                <article key={feature.name}>
                  <h3>{feature.name}</h3>
                  {feature.preview ? <span className="feature-preview-label">Development preview</span> : null}
                  <p>{feature.description}</p>
                </article>
              ))}
            </div>
          </details>
        ))}
      </div>
      {count === 0 ? (
        <div className="empty-search">
          <h3>No matching features</h3>
          <p>Try “mask”, “fonts”, “guides” or “animation”.</p>
          <button
            className="text-link"
            type="button"
            onClick={() => {
              setQuery("");
              setCategory("all");
            }}
          >
            Clear filters <Arrow />
          </button>
        </div>
      ) : null}
      <p className="feature-note">
        Preview features require a{" "}
        <a href={`${sitePath("docs/")}#development-preview`}>preview source build</a>.
        {" "}See the <a href={sitePath("docs/formats/")}>format guide</a> for
        import, export and compatibility limits, or the{" "}
        <a href={sitePath("docs/manual/")}>manual</a> for editing workflows.
      </p>
    </section>
  );
}
