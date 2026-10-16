/** Canonical retained live capability policy, shared by HTTP and Convex.
 * Args: encoded cells, never materialized artwork. Returns the oldest safe protocol or refusal.
 */
export const POLICY_VERSION = 3;
export const SERVER_PROTOCOL = 3;
export type Protocol = 1 | 2 | 3;
export class LiveProtocolError extends Error {}
const motionFields = new Set([
  "duration",
  "fps",
  "looped",
  "tracks",
  "expressions",
  "compositions",
  "scene3d",
]);
const legacyEase = new Set(["Linear", "EaseIn", "EaseOut", "EaseInOut"]);
const object = (v: unknown): v is Record<string, any> =>
  v !== null && typeof v === "object" && !Array.isArray(v);
const present = (v: unknown) =>
  v != null && (!Array.isArray(v) || v.length > 0);

export function offeredProtocol(offered: unknown): Protocol {
  if (offered === undefined) return 1;
  if (offered !== 1 && offered !== 2 && offered !== 3)
    throw new LiveProtocolError("Unsupported live protocol.");
  return offered;
}
export function requireProtocol(offered: unknown, required: number): Protocol {
  const supported = offeredProtocol(offered);
  if (![1, 2, 3].includes(required))
    throw new LiveProtocolError("Unsupported retained live feature.");
  if (supported < required)
    throw new LiveProtocolError(
      required === 2
        ? "This live document requires native-preservation protocol 2."
        : "This live document requires complete common+Motion preservation protocol 3.",
    );
  return supported;
}
function motionPath(path: string): string | undefined {
  return path === "/motion" || path.startsWith("/motion/")
    ? path.slice(7)
    : undefined;
}
function relativePath(path: string): string {
  for (;;) {
    const document = path.match(
      /^\/(?:@compositions\/[^/]+|motion\/compositions\/\d+)\/document(?=\/|$)/,
    );
    if (!document) return path;
    path = path.slice(document[0].length);
  }
}
const documentFields = new Set([
  "publishing",
  "color",
  "text_wrap_above_only",
  "name",
  "workspace",
  "width",
  "height",
  "dpi",
  "layers",
  "import_notes",
  "sources",
  "guides",
  "ruler",
  "grid",
  "transparent",
  "artboards",
  "artboardless",
  "show_bleed",
  "show_safe",
  "bleed",
  "motion",
  "cloud",
  "collaboration",
  "comments",
  "layout_tokens",
  "@entities",
  "@parent",
  "@position",
  "@compositions",
]);
const publishingMaps = new Set([
  "swatches",
  "master_stories",
  "master_layers",
  "master_assets",
  "page_sides",
  "page_masters",
  "links",
  "inherited_fingerprints",
  "inherited_pages",
]);
const publishingLists = new Set(["styles", "masters", "inherited_layers"]);
const nonzero = (value: unknown): boolean => {
  if (typeof value === "number" && Number.isFinite(value)) return value !== 0;
  if (Array.isArray(value)) {
    let found = false;
    for (const item of value) found = nonzero(item) || found;
    return found;
  }
  throw new LiveProtocolError("Unsupported retained numeric property.");
};
function commonProtocol(path: string, value: unknown): Protocol {
  const p = relativePath(path);
  refuseFutureLayout(p, value);
  const parts = p.split("/").slice(1),
    root = parts[0];
  if (root && !documentFields.has(root))
    throw new LiveProtocolError("Unsupported retained document property.");
  if (
    p === "" &&
    object(value) &&
    Object.keys(value).some((k) => !documentFields.has(k)) &&
    !("version" in value && "doc" in value)
  )
    throw new LiveProtocolError("Unsupported retained document property.");
  if (root === "sources")
    return parts.length > 1 || (Array.isArray(value) && value.length > 0)
      ? 3
      : 1;
  if (root === "publishing") {
    if (parts.length === 1) return 1;
    const field = parts[1];
    if (field === "facing_pages")
      return value === false || value == null ? 1 : 3;
    if (field === "bleed") return value == null ? 1 : nonzero(value) ? 3 : 1;
    if (publishingMaps.has(field))
      return parts.length > 2 ||
        (object(value) && Object.keys(value).length > 0)
        ? 3
        : 1;
    if (publishingLists.has(field))
      return parts.length > 2 || (Array.isArray(value) && value.length > 0)
        ? 3
        : 1;
    throw new LiveProtocolError("Unsupported retained publishing feature.");
  }
  const fillAddress = p.match(
    /(?:\/style\/fill|\/geom\/Text\/spans\/\d+\/fill)(.*)$/,
  );
  if (fillAddress) {
    const variants = [
      "Pattern",
      "Gradient",
      "None",
      "Solid",
      "Linear",
      "Radial",
    ];
    const tail = fillAddress[1];
    const names = tail.startsWith("/")
      ? [tail.split("/")[1]]
      : typeof value === "string"
        ? [value]
        : object(value)
          ? Object.keys(value)
          : [];
    if (names.some((name) => !variants.includes(name)))
      throw new LiveProtocolError("Unsupported retained fill variant.");
  }
  const layer =
    parts[0] === "@entities" || parts[0] === "layers" ? parts.slice(2) : [];
  if (
    ["svg_filter", "source_attachment", "psd_type", "psd_smart"].includes(
      layer[0],
    ) &&
    value != null
  )
    return 3;
  const text = p.lastIndexOf("/geom/Text/");
  if (text >= 0) {
    const tail = p.slice(text + 11).split("/");
    if (tail[0] === "font" && typeof value === "string" && value.length > 0)
      return 3;
    if (["transform", "svg_text"].includes(tail[0]) && value != null) return 3;
    if (
      (tail[0] === "spans" &&
        /^\d+$/.test(tail[1] ?? "") &&
        ["publishing_style", "font", "px", "fill"].includes(tail[2])) ||
      (tail[0] === "paragraphs" &&
        /^\d+$/.test(tail[1] ?? "") &&
        tail[2] === "publishing_style")
    ) {
      if (value != null) return 3;
    }
  }
  const fill =
    p.endsWith("/style/fill") || /\/geom\/Text\/spans\/\d+\/fill$/.test(p);
  if (fill && value === "Pattern") return 3;
  if (
    p.endsWith("/style/fill/Pattern") ||
    p.includes("/style/fill/Pattern/") ||
    /\/geom\/Text\/spans\/\d+\/fill\/Pattern(?:\/|$)/.test(p)
  )
    return 3;
  const gradient = p.match(
    /(?:\/style\/fill\/Gradient|\/style\/stroke\/gradient|\/geom\/Text\/spans\/\d+\/fill\/Gradient)(.*)$/,
  );
  if (gradient) {
    const field = gradient[1].split("/")[1];
    if (
      field &&
      ![
        "kind",
        "from",
        "to",
        "stops",
        "radial_axis",
        "spread",
        "focal",
        "inner_radius",
      ].includes(field)
    )
      throw new LiveProtocolError("Unsupported retained gradient feature.");
    if (field === "radial_axis" && value != null) return 3;
    if (field === "spread" && value != null) {
      if (!["Pad", "Repeat", "Reflect"].includes(value as string))
        throw new LiveProtocolError("Unsupported retained gradient spread.");
      return value === "Pad" ? 1 : 3;
    }
    if (
      ["focal", "inner_radius"].includes(field) &&
      value != null &&
      nonzero(value)
    )
      return 3;
  }
  return 1;
}

/** Refuse stored Layout-28 data before a protocol-3 reader can discard it. */
function refuseFutureLayout(path: string, value: unknown): void {
  const marker = path.lastIndexOf("/layout");
  if (marker < 0 || !/^\/(?:@entities|layers)\//.test(path)) return;
  const tail = path.slice(marker + 7);
  if (tail && !tail.startsWith("/")) return;
  const fields = tail.split("/").slice(1), field = fields[0];
  let unsupported = false;
  if (field === "grid" && fields.length > 1) {
    unsupported = ["row", "column"].includes(fields[1])
      ? value != null
      : ["row_span", "column_span"].includes(fields[1])
        ? value !== 1
        : true;
  } else if (field === "stack" || field === "breakpoints") {
    const property = fields[field === "breakpoints" ? 2 : 1];
    if (["column_tracks", "row_tracks"].includes(property))
      unsupported = fields.length > 2 || !Array.isArray(value) || value.length !== 0;
    else if (value != null) {
      const variants: Record<string, string[]> = {
        align: ["Start", "Center", "End", "Stretch"],
        flow: ["Stack", "Wrap", "Grid"],
        direction: ["Vertical", "Horizontal"],
        justify: ["Start", "Center", "End", "SpaceBetween", "SpaceAround", "SpaceEvenly"],
      };
      if (property in variants) unsupported = !variants[property].includes(value as string);
    }
  } else if (field === "component") {
    if (fields.length === 1 && value != null)
      unsupported = typeof value === "string"
        ? !["Main", "Instance"].includes(value)
        : !object(value) || Object.keys(value).some(k => !["Main", "Instance"].includes(k));
    else if (fields[1] === "Main" && fields[2] === "properties")
      unsupported = fields.length > 3 || !Array.isArray(value) || value.length !== 0;
    else if (fields.length > 1) unsupported = !["Main", "Instance"].includes(fields[1]);
  }
  if (unsupported)
    throw new LiveProtocolError("Unsupported retained Layout feature or variant (feature 28 is outside live protocol 3).");
}
function valueProtocol(path: string, value: unknown, depth = 0): Protocol {
  if (depth > 128)
    throw new LiveProtocolError(
      "Live property nesting exceeds the policy limit.",
    );
  let required: Protocol = commonProtocol(path, value);
  if (relativePath(path) !== path) required = 3;
  const motion = motionPath(relativePath(path));
  if (motion !== undefined) {
    const field = motion.split("/")[1];
    if (field && !motionFields.has(field))
      throw new LiveProtocolError("Unsupported retained Motion feature.");
    if (
      motion === "" &&
      object(value) &&
      Object.keys(value).some((k) => !motionFields.has(k))
    )
      throw new LiveProtocolError("Unsupported retained Motion feature.");
    if (
      (field === "expressions" || field === "compositions") &&
      (motion.split("/").length > 2 || present(value))
    )
      required = 3;
    if (field === "scene3d" && value != null) required = 3;
    if (motion.endsWith("/ease") && value != null) {
      if (typeof value === "string" && legacyEase.has(value)) {
        /* existing ease */
      } else if (
        value === "Hold" ||
        value === "CubicBezier" ||
        (object(value) &&
          Object.keys(value).length === 1 &&
          "CubicBezier" in value)
      )
        required = 3;
      else throw new LiveProtocolError("Unsupported retained Motion easing.");
    }
    if (
      motion.includes("/ease/") &&
      !/\/ease\/CubicBezier\/(x1|y1|x2|y2)$/.test(motion) &&
      !motion.endsWith("/ease/CubicBezier")
    )
      throw new LiveProtocolError("Unsupported retained Motion easing.");
    if (motion.includes("/ease/CubicBezier")) required = 3;
  }
  if (/^\/(?:@compositions\/[^/]+\/document(?:\/|$))/.test(path)) required = 3;
  if (
    (path === "/color" || path.endsWith("/document/color")) &&
    value != null
  ) {
    if (
      !object(value) ||
      value.model !== "Rgb" ||
      value.depth !== "U8" ||
      value.intent !== 0 ||
      !Array.isArray(value.icc_profile) ||
      value.icc_profile.length
    )
      required = Math.max(required, 2) as Protocol;
    return required;
  }
  if (path.startsWith("/color/") && value != null)
    required = Math.max(required, 2) as Protocol;
  if (
    object(value) &&
    "doc" in value &&
    "version" in value &&
    (!Number.isSafeInteger(value.version) ||
      value.version < 1 ||
      value.version > 27)
  )
    throw new LiveProtocolError("Unsupported retained document version.");
  if (relativePath(path) === "/sources") return required;
  if (object(value) || Array.isArray(value)) {
    if ("path" in value && typeof value.path === "string" && "value" in value)
      return Math.max(required, encodedProtocol(value.path, value.value, depth + 1)) as Protocol;
    for (const [key, child] of Object.entries(value))
      required = Math.max(
        required,
        key.startsWith("/") ? encodedProtocol(key, child, depth + 1) : valueProtocol(
          `${path}/${key.replaceAll("~", "~0").replaceAll("/", "~1")}`,
          child,
          depth + 1,
        ),
      ) as Protocol;
  }
  return required;
}
export function cellProtocol(path: string, cell: unknown): Protocol {
  return encodedProtocol(path, cell, 0);
}
function encodedProtocol(path: string, cell: unknown, depth: number): Protocol {
  if (cell == null) return 1;
  if (cell === "Object" || cell === "List") {
    return valueProtocol(path, cell === "List" ? [] : {}, depth);
  }
  if (!object(cell) || Object.keys(cell).length !== 1)
    throw new LiveProtocolError("Unsupported live cell.");
  if ("Pixels" in cell) {
    const required = valueProtocol(path, null, depth);
    return Math.max(required, cell.Pixels?.native != null ? 2 : 1) as Protocol;
  }
  if ("Value" in cell) return valueProtocol(path, cell.Value, depth);
  if ("Variant" in cell) return valueProtocol(path, cell.Variant, depth);
  throw new LiveProtocolError("Unsupported live cell.");
}
export function seedProtocol(seed: unknown): Protocol {
  if (
    typeof seed !== "string" ||
    new TextEncoder().encode(seed).length > 500_000
  )
    throw new LiveProtocolError("Invalid or oversized live document seed.");
  let base: unknown;
  try {
    base = JSON.parse(seed);
  } catch {
    throw new LiveProtocolError("Invalid live document seed.");
  }
  if (!object(base) || base[""] !== "Object")
    throw new LiveProtocolError("Invalid live document seed.");
  return Object.entries(base).reduce<Protocol>(
    (n, [path, cell]) => Math.max(n, cellProtocol(path, cell)) as Protocol,
    1,
  );
}
export function changesProtocol(changes: readonly any[]): Protocol {
  return changes.reduce<Protocol>((required, change) => {
    if (change?.edit?.type === "toggle") return required;
    if (change?.edit?.type !== "write" || !Array.isArray(change.edit.writes))
      throw new LiveProtocolError("Unsupported live operation.");
    return change.edit.writes.reduce((n: Protocol, write: any) => {
      if (typeof write.path !== "string")
        throw new LiveProtocolError("Invalid live property address.");
      return Math.max(n, cellProtocol(write.path, write.value)) as Protocol;
    }, required);
  }, 1);
}
/** Read-only create preflight; must precede quota initialization as well as authoritative insertion. */
export function preflightCreate(seed: unknown, offered: unknown): Protocol {
  const required = seedProtocol(seed);
  requireProtocol(offered, required);
  return required;
}
