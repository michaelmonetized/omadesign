import { RELEASE_TAG } from "./release";

export type FeatureGroup = {
  id: string;
  title: string;
  intro: string;
  features: { name: string; description: string; since?: string }[];
};

export const featureGroups: FeatureGroup[] = [
  {
    id: "vector",
    title: "Vector drawing",
    intro: "Build logos, illustrations and layouts from editable vectors.",
    features: [
      {
        name: "Pen paths",
        description:
          "Click corners, drag smooth curves, continue open paths and join endpoints. Close a shape or finish an open line.",
      },
      {
        name: "Node editing",
        description:
          "Move points, segments and Bézier handles directly on rotated paths. Add or remove nodes, switch corners to curves and break handle symmetry.",
      },
      {
        name: "Geometric shapes",
        description:
          "Draw rectangles, ellipses, polygons, stars and lines. Adjust corner radius, side count and star proportions.",
      },
      {
        name: "Freehand drawing",
        description:
          "Sketch editable curves with the Pencil tool, then refine their points with Node.",
      },
      {
        name: "Free transform",
        description:
          "Move, scale and rotate with canvas handles. Right-click to flip vectors horizontally or vertically. Alt-drag makes a copy; text stays live until you explicitly outline it.",
      },
      {
        name: "Pathfinder and compound paths",
        description:
          "Unite, subtract, intersect, exclude or divide shapes on one layer. Combine and release contours, with holes and Undo preserved.",
      },
      {
        name: "Stroke outlines",
        description:
          "Turn a stroke into editable filled geometry, including caps, joins and dashes. Keep the original fill underneath.",
      },
      {
        name: "Image tracing",
        description:
          "Convert a pixel layer to vector paths with adjustable threshold, color count and smoothness.",
      },
    ],
  },
  {
    id: "precision",
    title: "Precision and guides",
    intro: "Position, align and reshape vector artwork.",
    features: [
      {
        name: "Rulers and guide lines",
        description:
          "Drag guides from the rulers, move the origin and switch between pixels, millimeters, centimeters, inches and points.",
      },
      {
        name: "Editable object guides",
        description:
          "Turn vector artwork into non-printing guides. Edit their contours, snap to their curves and release them back to artwork.",
      },
      {
        name: "Smart alignment and spacing",
        description:
          "Snap to objects, artboards, guides and the grid. Alignment lines and equal-spacing measurements appear as you move.",
      },
      {
        name: "Precision modifiers",
        description:
          "Hold Shift for horizontal, vertical or 45° placement. Hold Ctrl to reverse snapping during a drag, and combine Shift with Alt-cloning.",
      },
      {
        name: "Reshape cages",
        description:
          "Distort, skew, add perspective or use a nine-handle warp mesh. Editing converts live text and parameter shapes to paths; Undo restores them.",
      },
    ],
  },
  {
    id: "type-colour-effects",
    title: "Text, color and effects",
    intro: "Edit text, fills, strokes and effects.",
    features: [
      {
        name: "Type on the canvas",
        description:
          "Click to start typing, add line breaks and return to existing text with a double-click. Your source stays editable.",
      },
      {
        name: "Character controls",
        description:
          "Choose local fonts and adjust size, tracking and leading. Control kerning, ligatures, tabular figures and small caps where the font supports them.",
      },
      {
        name: "Fill, stroke and gradients",
        description:
          "Pick color with HSV or hex, sample artwork, reuse recent colors and drag a gradient across a selected shape.",
      },
      {
        name: "Select by appearance",
        description:
          "Find objects with matching fills, strokes or effects, or select those with or without a property. Hidden and locked artwork stays excluded.",
      },
      {
        name: "Object and layer effects",
        description:
          "Add blur, shadows, outlines through dilation, color adjustments, turbulence and displacement. Effects remain adjustable and carry into SVG.",
      },
      {
        name: "Layer blending",
        description:
          "Combine layers with opacity and 16 blend modes, including Multiply, Screen, Overlay, Difference, Hue and Luminosity.",
      },
    ],
  },
  {
    id: "pixel",
    title: "Painting and masks",
    intro: "Add texture, retouch images and control what stays visible.",
    features: [
      {
        name: "Brush painting",
        description:
          "Paint on pixel layers with adjustable size and hardness. Shift constrains a stroke from its last free point.",
      },
      {
        name: "Erase, fill and smudge",
        description:
          "Remove pixels, fill connected areas and blend nearby color with dedicated tools and controls for the current task.",
      },
      {
        name: "Clone and healing brushes",
        description:
          "Alt-click a source, then paint. Clone copies texture; healing blends sampled texture with the destination’s local color. Undo restores the stroke.",
      },
      {
        name: "Pixel selections",
        description:
          "Isolate an area with rectangular or elliptical marquees, a freehand lasso or a tolerance-based magic wand.",
      },
      {
        name: "Editable layer masks",
        description:
          "Reveal all, hide all or start from a selection. Paint black or white, invert or remove the mask, or apply it to pixels.",
      },
    ],
  },
  {
    id: "photo",
    title: "Photo development",
    intro:
      "Develop one photo or apply a consistent look across a whole shoot.",
    features: [
      {
        name: "Copy, paste and batch adjustments",
        since: "0.0.3-alpha",
        description:
          "Copy one photo’s look, select a range or the whole library, and apply it in one step. Choose which adjustments travel; crop and rotation stay separate by default. Undo restores the batch.",
      },
      {
        name: "Whole-folder development",
        since: "0.0.3-alpha",
        description:
          "Apply adjustments to a folder in the background without opening every RAW file. Track progress, cancel remaining work, and keep original images untouched.",
      },
      {
        name: "Reusable, shareable photo presets",
        since: "0.0.3-alpha",
        description:
          "Name and save a look, filter your preset library, and share it as an .omapreset JSON file. Apply it to another photo or a selected batch.",
      },
      {
        name: "Photo browsing and comparison",
        description:
          "Open a folder, drop images or try the samples. Inspect the histogram and use Before to compare with the original.",
      },
      {
        name: "Light and tone",
        description:
          "Adjust exposure, contrast, highlights, shadows, whites and blacks. Refine the tone curve or start with Auto light.",
      },
      {
        name: "Color and detail",
        description:
          "Set temperature, tint, vibrance and saturation. Explore the HSL mixer, shadow/highlight grading, clarity, dehaze, grain and vignette.",
      },
      {
        name: "Crop, export and place",
        description:
          "Crop and rotate, inspect at 100%, export a developed JPEG in the background or place the result as a pixel layer in Design.",
      },
      {
        name: "Camera RAW development",
        since: "0.0.2-alpha",
        description:
          "Open DNG, Canon CR2/CR3, Nikon NEF, Sony ARW, Fujifilm RAF and other supported camera files with the bundled decoder. Develop full-resolution 16-bit linear pixels; camera and compression coverage varies.",
      },
      {
        name: "Saved photo adjustments",
        since: "0.0.2-alpha",
        description:
          "Save exposure, color, crop and other settings beside the original in an .omaphoto file. Reopening restores them; Undo and Redo track each photo’s edits. The original stays untouched.",
      },
      {
        name: "Full-resolution detail and 16-bit export",
        since: "0.0.2-alpha",
        description:
          "Inspect source detail at 100% while work runs in the background. Export developed RAW photos to 16-bit PNG or TIFF, or 8-bit JPEG. Placement in Design becomes an 8-bit pixel layer.",
      },
    ],
  },
  {
    id: "motion",
    title: "Motion",
    intro:
      "Animate the artwork you already made, with editable keys and a preserved rest pose.",
    features: [
      {
        name: "Animation tracks",
        description:
          "Animate position, rotation, scale and opacity, plus stroke and fill reveals. The motion clip stays with the native document.",
      },
      {
        name: "Editable timeline",
        description:
          "Create keys, drag them to retime, change easing and remove them. Scrub, play and loop the result on the canvas.",
      },
      {
        name: "13 motion presets",
        description:
          "Try Draw stroke, Pop in, Slam, Shake, Fill up, four slide directions, Fly, Zoom, Buzz and Fade in.",
      },
      {
        name: "Timing and energy",
        description:
          "Set duration, delay, stagger, intensity and the starting playhead position. Presets create ordinary keys you can refine or undo.",
      },
      {
        name: "Animated SVG and Lottie",
        description:
          "Export animated SVG with masks and effects, or Lottie for supported vector animation. Lottie reports unsupported pixels, layer masks and effects.",
      },
    ],
  },
  {
    id: "templates-brand",
    title: "Templates and brand tools",
    intro: "Reuse templates, palettes, assets and project fonts.",
    features: [
      {
        name: "52 original templates",
        description:
          "Search nine categories of editable designs, all available locally. Artwork and copy adapt to portrait, square and landscape proportions.",
      },
      {
        name: "20 document presets",
        description:
          "Start with print, screen, social, identity or photo sizes, or enter your own dimensions and DPI. Templates use the size you choose.",
      },
      {
        name: "Personal and project palettes",
        description:
          "Create, name, duplicate and filter palettes. Collect colors from artwork, add transparent hex values and apply swatches to fill or stroke.",
      },
      {
        name: "Searchable brand banks",
        description:
          "Collect logos, images and native artwork in searchable folders. Background previews and refresh keep the bank in step with files on disk.",
      },
      {
        name: "Drag artwork into place",
        description:
          "Drag a brand tile onto an artboard or double-click to center it. Native assets retain editable artwork and motion, with one Undo per placement.",
      },
      {
        name: "Portable project fonts",
        description:
          "Add TTF or OTF files, name roles and apply them to live text. Font files travel with the project without a system installation.",
      },
      {
        name: "Portable brand files",
        description:
          "Share palettes in .omacolors, font roles in .omatype and artwork plus fonts in .omabrand/. Import or save copies from the sidebar.",
      },
    ],
  },
  {
    id: "workflow",
    title: "Workspace and shortcuts",
    intro: "Navigate documents, layers and tools.",
    features: [
      {
        name: "Four studios and document tabs",
        description:
          "Switch between Design, Pixel, Photo and Motion. Keep several documents open and move between them without closing your work.",
      },
      {
        name: "Layers and objects",
        description:
          "Expand vector layers to find individual objects. Select, name, hide, lock and reorder artwork from the layer stack.",
      },
      {
        name: "Multiple artboards",
        description:
          "Draw, name, move, resize, rotate or clone artboards. Wrap a selection in a board and show bleed or safe-area guides.",
      },
      {
        name: "Clipboard and style reuse",
        description:
          "Copy, cut, paste and duplicate objects. Copy a style separately and apply it to other artwork without replacing its geometry.",
      },
      {
        name: "Shortcuts in sight",
        description:
          "The Shortcut HUD follows your tool and held modifiers without moving the canvas. Toggle it with Ctrl+/ or open the full key list with F1.",
      },
      {
        name: "Desktop theme and navigation",
        description:
          "Omarchy colors and your desktop font shape the interface at launch. Pan with Space, pinch to zoom, or drag a zoom box.",
      },
      {
        name: "Recovery and protected drafts",
        description:
          "Changed documents recover in the background, including project fonts. Save prompts and library conflict notices help protect unsaved artwork and palette drafts.",
      },
    ],
  },
  {
    id: "files-resources",
    title: "Files and resources",
    intro:
      "Use familiar image formats, keep an editable native source and choose an export that suits the artwork.",
    features: [
      {
        name: "Native and common image imports",
        description:
          "Open .oma, SVG, PNG, JPEG, WebP, GIF, BMP and TIFF. SVG uses a supported subset; Lottie has a separate basic shape-animation importer.",
      },
      {
        name: "Layered document imports",
        since: "0.0.2-alpha",
        description:
          "Import supported PSD/PSB layers and masks, multi-page PDF and PDF-compatible AI artwork, SVG/SVGZ groups and OpenRaster layers. Photoshop text and smart objects use saved layer pixels. EPS/PS requires Ghostscript.",
      },
      {
        name: "Affinity document bridge",
        since: "0.0.2-alpha",
        description:
          "Import supported .afdesign, .afphoto, .afpub, template and package documents with an optional converter. Compatibility is partial; many effects and publishing features are lost. New unified .af support awaits verification with a real document.",
      },
      {
        name: "Layered PSD, PSB, PDF and OpenRaster export",
        since: "0.0.2-alpha",
        description:
          "Share supported layer structures and PDF pages, with conversion notes for substitutions or flattened appearances. Keep .oma as the editable working source. Native AI and Affinity writing are not available.",
      },
      {
        name: "PNG, JPEG and SVG export",
        description:
          "Export still artwork with selectable raster scale. SVG keeps vector geometry and outlines project-font text; .oma retains the editable source.",
      },
      {
        name: "Optional online resources",
        description:
          "Browse Google Fonts and icon libraries, or search Pixabay and Pexels with your API keys. Local templates and bundled Phosphor shapes work offline.",
      },
    ],
  },
];

export const faqs: { question: string; answer: string }[] = [
  {
    question: "Which computers can run omadesign?",
    answer:
      "omadesign is an alpha Linux app. Release downloads are available for x86_64 and ARM64 Linux, with a glibc 2.35 baseline. ARM64 includes Linux on Apple Silicon through Asahi; it is not a macOS build. Omarchy integration is built in, with fallback colors and fonts for other desktops.",
  },
  {
    question: "Does the published download include everything shown here?",
    answer: `Yes. ${RELEASE_TAG} includes layered interchange, camera RAW development, batch photo adjustments, shareable presets and 16-bit photo export alongside the existing studios, templates and brand tools. Affinity import requires a separate optional bridge setup. The app remains an alpha; the format guide explains compatibility limits.`,
  },
  {
    question: "Can I open Affinity files?",
    answer:
      "Import supported .afdesign, .afphoto and .afpub artwork through the optional Affinity bridge. This is partial compatibility: most adjustments, live effects and publishing structures are not retained. The bridge includes new unified .af support, but a real .af document has not yet been verified. Keep your original; native Affinity export is not available.",
  },
  {
    question: "Do I need an account or an internet connection?",
    answer:
      "Core editing, local brand libraries and all 52 templates need no omadesign account or internet connection. Downloading fonts or icons and searching online photos requires a connection. Pixabay and Pexels search also require your own provider API keys.",
  },
  {
    question: "How do I move a project and its brand kit?",
    answer:
      "Copy the .oma document together with .omacolors, .omatype and the complete .omabrand folder. Project fonts stay inside .omabrand/fonts so text remains editable on another machine. Palette and typography edits have their own save controls; save those drafts before sharing the folder.",
  },
  {
    question: "What is outside the current scope?",
    answer:
      "Advanced publishing and text layout, reusable symbols and collaboration are not available. There is no complete Affinity or Illustrator round trip, PDF/X or CMYK print workflow. RAW camera support varies, and proprietary camera looks and automatic lens corrections are not reproduced. Motion exports to animated SVG or supported Lottie, not MP4 or GIF; Lottie cannot preserve pixel layers, layer masks or effects.",
  },
];
