const ALT =
  "omadesign - Native Linux studio for design, paint, photo & motion | Product Hunt";

type Badge = {
  href: string;
  darkSrc: string;
  lightSrc: string;
  width: number;
  height: number;
};

const BADGES: Badge[] = [
  {
    href: "https://www.producthunt.com/products/omadesign/reviews?utm_source=badge-product_rating&utm_medium=badge&utm_source=badge-omadesign",
    darkSrc:
      "https://api.producthunt.com/widgets/embed-image/v1/product_rating.svg?product_id=1313169&theme=dark",
    lightSrc:
      "https://api.producthunt.com/widgets/embed-image/v1/product_rating.svg?product_id=1313169&theme=light",
    width: 242,
    height: 108,
  },
  {
    href: "https://www.producthunt.com/products/omadesign/reviews/new?utm_source=badge-product_review&utm_medium=badge&utm_source=badge-omadesign",
    darkSrc:
      "https://api.producthunt.com/widgets/embed-image/v1/product_review.svg?product_id=1313169&theme=dark",
    lightSrc:
      "https://api.producthunt.com/widgets/embed-image/v1/product_review.svg?product_id=1313169&theme=light",
    width: 250,
    height: 54,
  },
  {
    href: "https://www.producthunt.com/products/omadesign?utm_source=badge-follow&utm_medium=badge&utm_source=badge-omadesign",
    darkSrc:
      "https://api.producthunt.com/widgets/embed-image/v1/follow.svg?product_id=1313169&theme=dark",
    lightSrc:
      "https://api.producthunt.com/widgets/embed-image/v1/follow.svg?product_id=1313169&theme=light",
    width: 250,
    height: 54,
  },
];

export function ProductHuntEmbed() {
  return (
    <aside className="ph-embed" aria-label="On Product Hunt">
      <p className="ph-embed-label">On Product Hunt</p>
      <div className="ph-embed-badges">
        {BADGES.map((badge) => (
          <a
            key={badge.href}
            href={badge.href}
            target="_blank"
            rel="noopener noreferrer"
          >
            <img
              className="ph-embed-img ph-embed-img-dark"
              src={badge.darkSrc}
              alt={ALT}
              width={badge.width}
              height={badge.height}
            />
            <img
              className="ph-embed-img ph-embed-img-light"
              src={badge.lightSrc}
              alt={ALT}
              width={badge.width}
              height={badge.height}
            />
          </a>
        ))}
      </div>
    </aside>
  );
}
