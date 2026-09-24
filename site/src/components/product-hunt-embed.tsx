/** Official Product Hunt product embed from https://www.producthunt.com/products/omadesign/embed */
const PRODUCT_CARD =
  "https://cards.producthunt.com/cards/products/omadesign";
const PRODUCT_PAGE =
  "https://www.producthunt.com/products/omadesign?utm_source=badge-product&utm_medium=badge&utm_source=badge-omadesign";

export function ProductHuntEmbed() {
  return (
    <aside className="ph-embed" aria-label="Omadesign on Product Hunt">
      <iframe
        title="Omadesign on Product Hunt"
        src={PRODUCT_CARD}
        width={500}
        height={405}
        style={{ border: "none" }}
        loading="lazy"
        allowFullScreen
      />
      <a
        className="ph-embed-fallback text-link"
        href={PRODUCT_PAGE}
        target="_blank"
        rel="noopener noreferrer"
      >
        Find omadesign on Product Hunt ↗
      </a>
    </aside>
  );
}
