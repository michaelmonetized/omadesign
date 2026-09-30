use super::*;
pub fn extended() -> Vec<Value> {
    let n = json!({"type":"number"});
    let i = json!({"type":"integer","minimum":0});
    let s = json!({"type":"string"});
    let b = json!({"type":"boolean"});
    let obj = json!({"type":"object"});
    let point = json!({"type":"array","items":n,"minItems":2,"maxItems":2});
    let points = json!({"type":"array","items":point,"minItems":1,"maxItems":4096});
    let targets = json!({"type":"array","minItems":1,"maxItems":100,"items":{"type":"object","properties":{"layer":i,"id":i},"required":["layer","id"],"additionalProperties":false}});
    let specs = vec![
        (
            "configure_canvas",
            "Set document name, dimensions, DPI or transparency without replacing artwork. Optionally replace native artboards, guides or layout_tokens with validated arrays using get_document data/schema; replacing artboards does not move their artwork.",
            json!({"name":s,"width":n,"height":n,"dpi":n,"transparent":b,"artboards":{"type":"array","items":obj,"maxItems":256},"guides":{"type":"array","items":obj,"maxItems":1000},"layout_tokens":{"type":"array","items":obj,"maxItems":1000}}),
            vec![],
            true,
        ),
        (
            "patch_object",
            "Edit advanced native vector geometry/style/text properties while preserving ID, masks and layout. Supply changed native fields from get_objects: geom (including Bezier paths/rich text), style (full native stroke/gradient), corners, text_wrap, opacity, fill_opacity, blend_interior, blend, name, visible, locked or guide. Native rotation here is radians; update_shape uses degrees. Use dedicated layout/effect/image tools for those properties. One undo step.",
            json!({"layer":i,"id":i,"changes":obj}),
            vec!["layer", "id", "changes"],
            true,
        ),
        (
            "list_files",
            "Browse a user-requested local directory, including assets outside the project. Absolute path or ~/. Optional filename substring and pagination. No shell needed.",
            json!({"path":s,"query":s,"offset":i}),
            vec!["path"],
            false,
        ),
        (
            "read_file",
            "Inspect a local asset. Photos return a bounded image preview; UTF-8 files return text with optional 1-based line/limit. Reading a photo does not place it: use import_file.",
            json!({"path":s,"as":{"enum":["auto","image","text"]},"line":i,"limit":i}),
            vec!["path"],
            false,
        ),
        (
            "import_file",
            "Bring a file from disk into this live session, in any workspace. Use absolute path from attachments or list_files. Canvas preserves editable OMA/SVG/PDF/PSD/ORA/XCF layers and places photos as pixel layers. Frame places inside the specified native frame. Photo loads the original into Photo's nondestructive filmstrip. Canvas/frame bounds are document coordinates; omitted dimensions preserve aspect ratio. Existing artwork is preserved. Returns selection IDs and import notes.",
            json!({"path":s,"destination":{"enum":["canvas","frame","photo"]},"layer":i,"id":i,"x":n,"y":n,"width":n,"height":n}),
            vec!["path"],
            true,
        ),
        (
            "set_image_fill",
            "Embed a disk photo as an editable image fill on a vector object/frame. Supports cover, contain, stretch and normalized focal point. Preserves the source file and existing shape geometry.",
            json!({"path":s,"layer":i,"id":i,"fit":{"enum":["cover","contain","stretch"]},"focal_x":n,"focal_y":n}),
            vec!["path", "layer", "id"],
            true,
        ),
        (
            "set_mode",
            "Show Design, Pixel, Photo, Layout or Motion. Tools remain available in every workspace; mode only changes the visible editor.",
            json!({"mode":{"enum":["design","pixel","photo","layout","motion"]}}),
            vec!["mode"],
            true,
        ),
        (
            "transform_raster",
            "Set the display bounds, rotation in degrees, shear or opacity of an imported/pasted pixel layer. Original pixels and masks remain editable. Use layer index; its selection ID is 0, motion ID is the persistent layer ID.",
            json!({"layer":i,"x":n,"y":n,"width":n,"height":n,"rotation":n,"shear":n,"opacity":n}),
            vec!["layer"],
            true,
        ),
        (
            "set_layout",
            "Patch native frame/child layout properties. Use get_objects and get_documentation(layout) for schema: stack, sizing, constraints, clip, breakpoints, text_size, parent, etc. Parent may be null to detach. Image bytes and component bindings are managed by their dedicated tools/actions.",
            json!({"layer":i,"id":i,"changes":obj}),
            vec!["layer", "id", "changes"],
            true,
        ),
        (
            "set_filter_stack",
            "Replace editable effects on any layer, or on a vector object when id is supplied. Each effect has name from get_editor_capabilities.effects and optional parameters overriding its native defaults. Empty effects removes the stack. Works on raster layers.",
            json!({"layer":i,"id":i,"effects":{"type":"array","maxItems":32,"items":{"type":"object","properties":{"name":s,"parameters":obj},"required":["name"],"additionalProperties":false}}}),
            vec!["layer", "effects"],
            true,
        ),
        (
            "get_editor_capabilities",
            "Discover native effect defaults, motion properties/presets, layout example and photo adjustment schema. Use this to find available operations before claiming a capability is missing.",
            json!({}),
            vec![],
            false,
        ),
        (
            "editor_action",
            "Run a native undoable editor command on explicit targets, or current selection if targets omitted. Raster selection ID is 0. Layer actions use layer. Undo/redo history is selected by history (canvas/photo), default current mode. Targets are validated before changing selection.",
            json!({"action":{"enum":editing::ACTIONS},"targets":targets,"layer":i,"dx":n,"dy":n,"history":{"enum":["canvas","photo"]}}),
            vec!["action"],
            true,
        ),
        (
            "set_pixel_selection",
            "Create a pixel selection in source-pixel coordinates on a raster layer: rect, ellipse, polygon, wand, all, none, invert. Optional combine=add/subtract/intersect, feather radius. The selection can drive masks and painting without deleting the original photo.",
            json!({"layer":i,"kind":{"enum":["rect","ellipse","polygon","wand","all","none","invert"]},"x":n,"y":n,"width":n,"height":n,"points":points,"tolerance":n,"combine":{"enum":["replace","add","subtract","intersect"]},"feather":i}),
            vec!["layer", "kind"],
            true,
        ),
        (
            "paint_stroke",
            "Paint, erase, smudge, clone or heal a stroke on a raster layer using source-pixel coordinates. Respects current pixel selection; one undo step. Optional mask=true paints its mask. Points are [x,y]. Source is required for clone/heal.",
            json!({"layer":i,"operation":{"enum":["paint","erase","smudge","clone","heal","fill"]},"points":points,"source":point,"color":s,"size":n,"hardness":n,"opacity":n,"flow":n,"spacing":n,"tolerance":n,"mask":b}),
            vec!["layer", "points"],
            true,
        ),
        (
            "set_mask",
            "Create or edit a nondestructive layer/object mask. Operations: selection, reveal, hide, invert, remove; layer masks also support apply. id targets a vector object; omit for raster/layer mask. Use a pixel selection for a photographic cutout and retain its source pixels.",
            json!({"layer":i,"id":i,"operation":{"enum":["selection","reveal","hide","invert","remove","apply"]}}),
            vec!["layer", "operation"],
            true,
        ),
        (
            "get_motion",
            "Inspect duration, FPS, loop, playhead and paginated tracks with persistent target IDs.",
            json!({"offset":i}),
            vec![],
            false,
        ),
        (
            "set_motion",
            "Set duration, FPS, loop and playhead. Playback can be started/stopped. Works from every workspace; use set_mode to show Motion.",
            json!({"duration":n,"fps":n,"looped":b,"time":n,"playing":b}),
            vec![],
            true,
        ),
        (
            "set_keyframes",
            "Add/update or remove keys on vector objects or raster images. id=0 selects raster layer. property uses names from get_editor_capabilities. Rotation is degrees; x/y are offsets, scale/width/height are multipliers, opacity/reveal 0–1. Key ease is Linear, EaseIn, EaseOut or EaseInOut. Fill keys use hex color. One undo step.",
            json!({"layer":i,"id":i,"property":s,"remove":b,"keys":{"type":"array","minItems":1,"maxItems":1000,"items":{"type":"object","properties":{"time":n,"value":n,"color":s,"ease":{"enum":["Linear","EaseIn","EaseOut","EaseInOut"]}},"required":["time"],"additionalProperties":false}}}),
            vec!["layer", "id", "property", "keys"],
            true,
        ),
        (
            "apply_motion_preset",
            "Apply a native motion preset to explicit targets (raster id=0) or selection. Recipes remain ordinary editable keys. Preset names come from get_editor_capabilities; only Draw stroke requires a vector stroke.",
            json!({"preset":s,"targets":targets,"time":n,"duration":n,"delay":n,"stagger":n,"intensity":n}),
            vec!["preset"],
            true,
        ),
        (
            "get_photos",
            "Inspect Photo filmstrip, selected photo, original dimensions, source paths and nondestructive adjustments. Includes photo_revision; use it for Photo writes to detect concurrent edits.",
            json!({"offset":i}),
            vec![],
            false,
        ),
        (
            "develop_photo",
            "Patch nondestructive Photo adjustments, including crop/rotate, tone, color, curves, HSL, grain and vignette. Numeric ranges match native UI/schema. Does not alter the source file. Optional auto_tone/reset. Photo revision from get_photos is required.",
            json!({"photo":i,"photo_revision":i,"changes":obj,"auto_tone":b,"reset":b}),
            vec!["photo", "photo_revision"],
            true,
        ),
        (
            "select_photo",
            "Select a Photo filmstrip image for the user and snapshots.",
            json!({"photo":i}),
            vec!["photo"],
            true,
        ),
        (
            "save_document",
            "Save the live editable document or export through native codecs to an absolute path (.oma, png, jpg, svg, pdf, psd/psb, ora). format=animated_svg/lottie/dotlottie exports motion. Source=photo exports selected developed image or .omaphoto settings. Existing files require overwrite:true. Saving a copy does not replace the current tab.",
            json!({"path":s,"overwrite":b,"format":{"enum":["auto","animated_svg","lottie","dotlottie"]},"source":{"enum":["canvas","photo"]},"photo":i,"photo_revision":i}),
            vec!["path"],
            true,
        ),
    ];
    specs.into_iter().map(|(name,description,mut properties,mut required,mutation)|{
        if mutation {properties["revision"]=i.clone();required.push("revision");}
        json!({"name":name,"description":description,"inputSchema":{"type":"object","properties":properties,"required":required,"additionalProperties":false},"annotations":{"readOnlyHint":!mutation,"openWorldHint":matches!(name,"list_files"|"read_file"|"import_file"|"set_image_fill"|"save_document")}})
    }).collect()
}
