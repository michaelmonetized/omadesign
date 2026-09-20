# Layout · 0.5.4

Layout is the native workspace for responsive screens, reusable components and interactive prototypes. Documents remain editable `.oma` files. See the [release notes](blog/0.5.4-more-control-on-the-canvas.md) for what changed in 0.5.4.

## Start and navigate

Choose the **Layout** tab. With nothing selected, the inspector offers **Open Fieldwork starter** and Phone, Tablet and Desktop frames. Fieldwork is an editable example with responsive layout, component variants and linked screens; use a copy for experiments.

Draw frames with **F**, add rectangles with **R** and live text with **T**. Pen, node, pencil, ellipse, polygon, star, line, gradient and eyedropper tools are available directly in Layout. New objects inside nested frames attach to the deepest editable frame under the pointer. The Layers panel expands the hierarchy; Shift-click selects multiple objects. Drag sidebar rows between objects to sort, or onto the center of a frame/group to nest. Its context menu also provides **Nest selection here**, Duplicate and Delete. The inspector's **Earlier**, **Later** and **Detach from frame** controls manage an object's position in the hierarchy.

| Shortcut on Linux | Action |
| --- | --- |
| V / F / T / R / O | Select / frame / text / rectangle / ellipse |
| Shift+A | Add auto layout; wrap selected objects when necessary |
| Arrow / Shift+Arrow | Nudge 1 / 10 document units |
| Ctrl+D | Duplicate selection, preserving nested contents |
| Ctrl+G / Ctrl+Shift+G | Group into a layer group / ungroup |
| Ctrl+8 / Ctrl+Shift+8 | Make compound shape / release compound |
| Ctrl+C / Ctrl+X / Ctrl+V | Copy / cut / paste |
| Ctrl+Alt+C / Ctrl+Alt+V | Copy / paste appearance |
| Ctrl+Z / Ctrl+Shift+Z | Undo / redo |
| Ctrl+S / Ctrl+Shift+S | Save / Save As |
| Ctrl+0 / Ctrl+1 | Fit document / actual size |
| F1 | Shortcut help |
| Esc in Present | Close the prototype preview |

## Responsive layout

Select a frame and enable **Arrange children automatically**, or select objects and press **Shift+A**. Choose Stack, Wrap or Grid. Stack and Wrap support horizontal and vertical flow; Grid uses equal-width columns. Set alignment, distribution, gaps and padding. Base padding has independent top, right, bottom and left values.

Each object has independent width and height sizing:

- **Fixed** retains its current size unless constraints or stretching control that axis.
- **Hug** measures its contents. For paragraphs, use Hug height so wrapping makes room for the next object.
- **Fill** uses available parent space; siblings share remaining space, subject to min/max limits.

Use **Absolute position** for an overlay that should not consume stack space. Its horizontal and vertical constraints control resizing: Min, Max, Stretch, Center or Scale. **Clip content** clips descendants at a frame boundary. **Lock aspect ratio** makes a frame's height follow its resolved width.

Under **Breakpoint overrides**, add a Phone or Tablet rule and set its maximum viewport width. Frame controls expose layout mode, row/column direction, gap, uniform padding and grid columns. Text rules expose font size. Rules use the outermost frame's width; narrower rules inherit applicable wider rules before overriding them. Authored base settings remain available when returning to desktop width.

The inspector's Phone/Tablet/Desktop buttons resize the actual selected frame. **Present** has separate viewport controls for testing widths without editing the source document.

## Components and overrides

Select a frame and choose **Components → Create component**. **Insert instance** creates a linked copy; **Insert from this document…** also lets you place another component inside a selected frame.

Edit the main component to update its instances. Instance edits such as changed text or appearance become local overrides; unrelated main-component edits still propagate. Save and reopen to verify both links and overrides survive.

On a main component, enter a name and choose **Add variant**, then edit the new definition. On an instance, use the variant picker to switch within its family. **Reset overrides** returns the instance to its definition. **Detach** retains editable objects while removing the component link.

## Images and design variables

Shapes, icons, photos and brand assets use the selected frame, or the nearest frame containing the selected object. The destination is retained while an asset loads. File → Place keeps Layout active; click or drag to position the file in the selected frame. With no frame selected, the frame under the placement pointer becomes the destination. Multi-part SVGs remain one asset in auto layout, with editable objects inside.

Drag existing objects into a frame to nest them, including objects on another layer. Drag them outside frames to detach them, or use **Nest selection here** from the destination frame's Layers menu. Moving a frame carries its children; placed raster images become embedded image objects when nested. Their opacity, blend and effects are retained. An existing layer mask is baked into image alpha; Undo restores the original raster layer and editable mask. Placement and nesting each undo as one action.

Select an object, open **Image fill**, and choose an image. The loader embeds the image in the document. **Fill** (cover), **Fit** (contain) and **Stretch** control fitting; the two Focus values set the crop's horizontal and vertical focal point. Replace image and Remove image are available in the same section.

Under **Design variables**, enter a name and add a Color or Number. Bind colors to Fill or Stroke, and numbers to Gap, Padding or Corners where supported. Changing a variable updates linked objects. Choose **Local value** to unlink one property. Deleting a variable preserves the current appearance and detaches its references. Gap variables control both the main and cross-axis gaps; Padding variables apply uniformly to all four edges.

## Prototype and export

Select an object and add an **Interaction**. Triggers are Click, Hover and Press. Actions are Navigate, Back, Open overlay, Close overlay and Change variant. Variant actions require a component instance. Hover variant changes restore the prior appearance when the pointer leaves.

Transitions are Instant, Dissolve, Slide left and Slide right, with configurable duration. Choose **Present** to interact with the design, switch between viewport presets or enter a custom width. Use its Back control and scroll the preview as needed; Esc closes it.

Select a frame and use **Export frame → PNG, SVG or HTML**. PNG and SVG export that frame's subtree. HTML produces a standalone responsive document with embedded assets and a runtime for the supported prototype actions, including referenced screens and variants. Open the file in a browser and check the behavior at multiple widths. Export does not publish or host a website.

## Human QA checklist

Use a saved copy of Fieldwork and a small design of your own. Record the build identifier, viewport, exact action, expected result and observed result for each failure.

- [ ] Create and nest frames; select, duplicate, copy/paste and delete a complete subtree. Undo and redo restore its hierarchy and appearance.
- [ ] Resize a nested Stack, Wrap and Grid. Exercise Fixed/Hug/Fill, mixed min/max limits, absolute children and clipping. No child drifts, overlaps unexpectedly or loses editability.
- [ ] Test desktop → phone → desktop, including an intermediate width. Typography, padding, flow and aspect ratios recover correctly. Present leaves the source unchanged.
- [ ] Create a component, two instances and a variant. Override one instance's text; edit the main's typography/color. The text override survives while shared changes propagate. Check reset, detach and undo.
- [ ] Place an image, test each fitting mode and focal point, then save/reopen after moving the original image file. The embedded image remains visible.
- [ ] Select a frame and then a child of that frame; insert a Phosphor icon, photo and brand asset in each case. Drag an existing icon from another layer into the frame. Place an SVG, delete it, place another, then save/reopen and place again. Verify the frame hierarchy and undo/redo after each operation.
- [ ] Link multiple objects to color and spacing variables. Change, unlink, delete, undo and reopen them; appearance and links remain consistent.
- [ ] Test Click, Hover and Press; navigation/back; an overlay and its close action; variant switching; and all transition styles in Present and exported HTML.
- [ ] Compare native, PNG, SVG and browser output at desktop, tablet and phone widths. Check text wrapping, images, rounded clips, opacity and unrelated overlapping artwork.
- [ ] Use a realistically large design and sustained editing. Check interaction responsiveness, memory use, save/reopen time and recovery from an invalid export destination.
- [ ] Complete a real design task without needing an unsupported feature, then explicitly accept or reject the tested build.

## Remaining gaps against full Figma / Framer

This implementation provides a substantial local design and prototype workflow, with these boundaries:

- Components and variables belong to one document; there are no remote shared libraries, variable aliases/modes or a full reusable typography/effects-style system.
- Grid uses uniform columns without spanning or custom track sizing. Layout lacks baseline alignment, negative gaps and the full set of advanced auto-layout controls. Text is edited as a single styled run rather than a rich mixture of inline styles.
- Breakpoints provide width-based layout and type-size overrides, not arbitrary property overrides or a complete responsive editing model for every visual property.
- Prototypes have the actions and transitions listed above. They do not provide Smart Animate, spring physics, drag/scroll-driven animation, conditional logic, forms or application data.
- HTML export is a responsive design/prototype artifact, not a Framer-style CMS, production application framework, deployment service or SEO management workflow. Native and browser output still require fidelity testing.
- Repeated embedded image fills share runtime storage, but their encoded data is repeated in saved component snapshots; photo-heavy component libraries can produce large `.oma` files.
- Native Figma `.fig` import/export and Framer project round-tripping are not implemented. Cloud project sharing and web snapshot review are separate workflows, described in the [cloud guide](cloud.md). There is no simultaneous multiplayer canvas editing.

See the [release validation record](releases/0.5.4.md) for build and workflow checks.
