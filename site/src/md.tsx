import ReactMarkdown, { defaultUrlTransform } from "react-markdown";
import remarkGfm from "remark-gfm";
import { PREVIEW_COMMIT } from "./release";

const REPOSITORY = "https://github.com/michaelmonetized/omadesign";
const DOC_ROUTES: Record<string, string> = {
  "/docs": "docs/",
  "/docs/manual": "docs/manual/",
  "/docs/manual.md": "docs/manual/",
  "/docs/formats": "docs/formats/",
  "/docs/format-support.md": "docs/formats/",
  "/docs/affinity": "docs/affinity/",
  "/docs/affinity-import.md": "docs/affinity/",
  "/docs/contributing": "docs/contributing/",
  "/docs/contributing.md": "docs/contributing/",
  "/docs/roadmap": "docs/roadmap/",
  "/docs/roadmap.md": "docs/roadmap/",
};

/** Keep source-relative Markdown links useful in a GitHub Pages subdirectory. */
export function documentationUrl(
  url: string,
  sourcePath: string,
  base = import.meta.env.BASE_URL,
): string | undefined {
  const safe = defaultUrlTransform(url);
  if (!safe) return undefined;
  if (safe.startsWith("#") || /^(?:[a-z][\w+.-]*:|\/\/)/i.test(safe))
    return safe;

  const resolved = new URL(safe, `https://docs.invalid/${sourcePath}`);
  const prefix = `${base.replace(/\/$/, "")}/`;
  const pathname = resolved.pathname.startsWith(prefix)
    ? `/${resolved.pathname.slice(prefix.length)}`
    : resolved.pathname;
  const route = DOC_ROUTES[pathname.replace(/\/$/, "").toLowerCase()];
  const suffix = resolved.search + resolved.hash;
  if (route) return prefix + route + suffix;
  if (pathname === "/") return prefix + suffix;

  // Examples and planning documents live in the repository, not on site routes.
  const kind = /\.[^/]+$/.test(pathname) ? "blob" : "tree";
  return `${REPOSITORY}/${kind}/${PREVIEW_COMMIT}${pathname}${suffix}`;
}

type MarkdownNode = {
  type: string;
  tagName?: string;
  value?: string;
  properties?: Record<string, unknown>;
  children?: MarkdownNode[];
};

function headingAnchors() {
  return (tree: MarkdownNode) => {
    const used = new Set<string>();
    const text = (node: MarkdownNode): string =>
      node.value ?? node.children?.map(text).join("") ?? "";
    const visit = (node: MarkdownNode) => {
      if (/^h[1-6]$/.test(node.tagName ?? "")) {
        const slug =
          text(node)
            .toLowerCase()
            .trim()
            .replace(/[^\p{L}\p{N}\s_-]/gu, "")
            .replace(/\s/g, "-") || "section";
        let id = slug;
        for (let index = 1; used.has(id); index++) id = `${slug}-${index}`;
        used.add(id);
        node.properties = { ...node.properties, id };
      }
      node.children?.forEach(visit);
    };
    visit(tree);
  };
}

const remarkPlugins = [remarkGfm];
const rehypePlugins = [headingAnchors];

export function Markdown({
  source,
  sourcePath,
}: {
  source: string;
  sourcePath: string;
}) {
  return (
    <ReactMarkdown
      skipHtml
      remarkPlugins={remarkPlugins}
      rehypePlugins={rehypePlugins}
      urlTransform={(url) => documentationUrl(url, sourcePath)}
    >
      {source}
    </ReactMarkdown>
  );
}
