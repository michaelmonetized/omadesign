import type { CSSProperties, ElementType } from "react";
import "./motion.css";

export function RevealText({
  as: Tag = "h2",
  text,
  lines,
  className = "",
  hero = false,
  iris = false,
}: {
  as?: ElementType;
  text?: string;
  lines?: string[];
  className?: string;
  hero?: boolean;
  iris?: boolean;
}) {
  const content = lines ?? [text ?? ""];
  return (
    <Tag
      className={`reveal-text ${hero ? "reveal-hero" : ""} ${iris ? "reveal-iris" : ""} ${className}`}
      data-reveal="text"
      aria-label={content.join(" ")}
    >
      {content.map((line, index) => (
        <span
          className="reveal-line"
          key={`${line}-${index}`}
          aria-hidden="true"
          style={{ "--line-index": index } as CSSProperties}
        >
          <span className="reveal-line-content">{line}</span>
          <span className="reveal-line-cover" />
        </span>
      ))}
    </Tag>
  );
}
