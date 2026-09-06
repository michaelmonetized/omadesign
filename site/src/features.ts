export type FeatureGroup = {
  id: string;
  title: string;
  intro: string;
  features: { name: string; description: string }[];
};

export const featureGroups: FeatureGroup[] = [
  {
    id: "vector",
    title: "Draw the idea",
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
          "Move points, segments and Bézier handles. Add or remove nodes, switch corners to curves and break handle symmetry.",
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
          "Move, scale and rotate with canvas handles while keeping live text and shape parameters editable. Alt-drag makes a copy.",
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
          "Turn a pixel layer into vector paths. Tune the threshold, colour count and smoothness before making the result your own.",
      },
    ],
  },
  {
    id: "precision",
    title: "Make every point count",
    intro:
      "Guides, snapping and shape controls help a rough sketch become a deliberate mark.",
    features: [
      {
        name: "Rulers and guide lines",
        description:
          "Drag guides from the rulers, move the origin and switch between pixels, millimetres, centimetres, inches and points.",
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
    title: "Give it character",
    intro: "Work with live type, expressive colour and adjustable effects.",
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
          "Pick colour with HSV or hex, sample artwork, reuse recent colours and drag a gradient across a selected shape.",
      },
      {
        name: "Select by appearance",
        description:
          "Find objects with matching fills, strokes or effects, or select those with or without a property. Hidden and locked artwork stays excluded.",
      },
      {
        name: "Object and layer effects",
        description:
          "Add blur, shadows, outlines through dilation, colour adjustments, turbulence and displacement. Effects remain adjustable and carry into SVG.",
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
    title: "Paint and repair",
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
          "Remove pixels, fill connected areas and blend nearby colour with dedicated tools and controls for the current task.",
      },
      {
        name: "Clone and healing brushes",
        description:
          "Alt-click a source, then paint. Clone copies texture; healing blends sampled texture with the destination’s local colour. Undo restores the stroke.",
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
    title: "Find the right light",
    intro:
      "Adjust photographs in a dedicated studio, then bring the result into your design.",
    features: [
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
        name: "Colour and detail",
        description:
          "Set temperature, tint, vibrance and saturation. Explore the HSL mixer, shadow/highlight grading, clarity, dehaze, grain and vignette.",
      },
      {
        name: "Crop, export and place",
        description:
          "Crop and rotate, inspect at 100%, export a developed JPEG in the background or place the result as a pixel layer in Design.",
      },
    ],
  },
  {
    id: "motion",
    title: "Put your mark in motion",
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
        name: "Thirteen motion presets",
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
    title: "Start with a little momentum",
    intro:
      "Keep useful beginnings, brand colours, fonts and reusable artwork close to the canvas.",
    features: [
      {
        name: "Fifty-two original templates",
        description:
          "Search nine categories of editable designs, all available locally. Artwork and copy adapt to portrait, square and landscape proportions.",
      },
      {
        name: "Twenty document presets",
        description:
          "Start with print, screen, social, identity or photo sizes, or enter your own dimensions and DPI. Templates use the size you choose.",
      },
      {
        name: "Personal and project palettes",
        description:
          "Create, name, duplicate and filter palettes. Collect colours from artwork, add transparent hex values and apply swatches to fill or stroke.",
      },
      {
        name: "Searchable brand banks",
        description:
          "Collect logos, images and native artwork in searchable folders. Background previews and refresh keep the bank in step with files on disk.",
      },
      {
        name: "Drag artwork into place",
        description:
          "Drag a brand tile onto an artboard or double-click to centre it. Native assets retain editable artwork and motion, with one Undo per placement.",
      },
      {
        name: "Portable project fonts",
        description:
          "Add TTF or OTF files, name roles and apply them to live text. Font files travel with the project without a system installation.",
      },
      {
        name: "A brand kit made of files",
        description:
          "Share palettes in .omacolors, font roles in .omatype and artwork plus fonts in .omabrand/. Import or save copies from the sidebar.",
      },
    ],
  },
  {
    id: "workflow",
    title: "Stay with the work",
    intro:
      "A familiar desktop, helpful shortcuts and a workspace that keeps the canvas in reach.",
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
        name: "A desktop that feels familiar",
        description:
          "Omarchy colours and your desktop font shape the interface at launch. Pan with Space, pinch to zoom, or drag a zoom box.",
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
    title: "Bring it in. Send it out.",
    intro:
      "Use familiar image formats, keep an editable native source and choose an export that suits the artwork.",
    features: [
      {
        name: "Native and common image imports",
        description:
          "Open .oma, SVG, PNG, JPEG, WebP, GIF, BMP and TIFF. SVG uses a supported subset; Lottie has a separate basic shape-animation importer.",
      },
      {
        name: "Converted document imports",
        description:
          "Import PDF, PDF-based AI, EPS and PSD with installed conversion tools. PDF handling starts with the first page; PSD imports are raster images.",
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
      "omadesign is an alpha Linux app. Release downloads are available for x86_64 and ARM64 Linux, with a glibc 2.35 baseline. ARM64 includes Linux on Apple Silicon through Asahi; it is not a macOS build. Omarchy integration is built in, with fallback colours and fonts for other desktops.",
  },
  {
    question: "Does the published download include everything shown here?",
    answer:
      "Not yet. The latest published packages are v0.0.1-alpha.rc from September 2, 2026. This site also shows newer work from the current source, including the expanded brand tools and templates. Build the current source for those features, or check the release notes before downloading. The app remains an alpha.",
  },
  {
    question: "Can I open Affinity files?",
    answer:
      "Affinity .afdesign, .afphoto and .afpub files cannot be opened directly. Export SVG or PDF from Affinity first. SVG import supports a subset of the format; PDF conversion requires tools such as Poppler or Inkscape and may not preserve every editable detail.",
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
      "RAW development, advanced publishing and text layout, reusable symbols and collaboration are not available. Photo adjustments are session-based; export or place the result to keep it. Motion exports to animated SVG or supported Lottie, not MP4 or GIF. Lottie cannot preserve pixel layers, layer masks or effects, and there is no PDF/X or CMYK print workflow.",
  },
];
