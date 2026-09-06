import { media } from "../site";

export function Arrow({ diagonal = false }: { diagonal?: boolean }) {
  return <span aria-hidden="true">{diagonal ? "↗" : "→"}</span>;
}
export function Shot({ name, alt }: { name: string; alt: string }) {
  return (
    <img
      src={media(name)}
      alt={alt}
      width="1600"
      height="880"
      loading="lazy"
      decoding="async"
    />
  );
}
