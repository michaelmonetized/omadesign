# Q/A Testing Issue tracker

GitHub: [michaelmonetized/omadesign](https://github.com/michaelmonetized/omadesign/issues). Q/A 1–19 map to issues **#13–#31**.

1. [pass] [#13](https://github.com/michaelmonetized/omadesign/issues/13) pen tool has a regression you have to drag to set a point.
2. [pass] [#14](https://github.com/michaelmonetized/omadesign/issues/14) pen tool + node tool behaviors need full feature and behavior parity with affinity, adobe illustrator, inkscape.
3. [pass] [#15](https://github.com/michaelmonetized/omadesign/issues/15) once a raster object is placed on an artboard it can not be moved, selected, manipulated in anyway.
4. [retest] [#16](https://github.com/michaelmonetized/omadesign/issues/16) SVG export. Paper PNG no longer steals the thumbnail. Opaque color is hex. Place keeps fill/stroke per path. Re-export, do not Place the old logo-2.svg.
5. [retest] [#17](https://github.com/michaelmonetized/omadesign/issues/17) Ctrl+C / Ctrl+V. Copy was swallowed whenever a sidebar field had focus. Object copy/paste now fires unless you are editing type on the canvas. Alt-drag clone already worked.
6. [untested] [#18](https://github.com/michaelmonetized/omadesign/issues/18) i should be able to import, place and/or open any vector or raster format (svg, png, eps, jpeg, etc.) and/or including proprietary: (.afdesign, .ai, .pdf, .psd, etc.) files.
7. [retest] [#19](https://github.com/michaelmonetized/omadesign/issues/19) Discard. Quit+Discard used to CancelClose because dirty stayed true, so the dialog came back. Discard now clears dirty and actually quits.
8. [pass] [#20](https://github.com/michaelmonetized/omadesign/issues/20) while working if i go idle for more than 1s save a temporary file in ~/.local/share/omadesign/<UUID>.oma.swp like neovim does so i can recover files in the event of a crash, saving action should delete this *.oma.swp file from this directory.
9. [pass] [#21](https://github.com/michaelmonetized/omadesign/issues/21) opening splash should have a recents tab that shows me my .omas i've edited.
10. [pass] [#22](https://github.com/michaelmonetized/omadesign/issues/22) if there are any files matching ~/.local/share/omadesign/<UUID>.oma.swp i should see a recovered tab.
11. [pass] [#23](https://github.com/michaelmonetized/omadesign/issues/23) layer sidebar should expand to show objects on the layer
12. [pass] [#24](https://github.com/michaelmonetized/omadesign/issues/24) objects should be able to have their own fx not just layer-wide.
13. [retest] [#25](https://github.com/michaelmonetized/omadesign/issues/25) Rotate and resize artboards. Frame, handles, and hit-test now follow rotation. Rotate snaps to 0/90/180/270. Move/resize/rotate take the paper plate with the frame.
14. [retest] [#26](https://github.com/michaelmonetized/omadesign/issues/26) Artboard clone. Clone now duplicates objects whose center sits on the board, offset with the new board. Draw / wrap already worked.
15. [pass] [#27](https://github.com/michaelmonetized/omadesign/issues/27) I should be able to name artboards.
16. [pass] [#28](https://github.com/michaelmonetized/omadesign/issues/28) Like illustrator and affinity i should have corner rounding handles in both select mode (for all corners) and node mode (for selected nodes)
17. [pass] [#29](https://github.com/michaelmonetized/omadesign/issues/29) I should be able to shift+click to grab multiple nodes in node select mode and grab + drag to move lines and draw a box around a collection of nodes to select only those nodes for dragging around the canvas.
18. [pass] [#30](https://github.com/michaelmonetized/omadesign/issues/30) Font dialog should start at the currently selected font instead of at the top, have a search box, have a recents area pinned at the top and have a used area pinned at the top (used area shows fonts in this document) recents shows last 5 font's used regardless of when, which document, etc.
19. [retest] [#31](https://github.com/michaelmonetized/omadesign/issues/31) Document tabs. Sidebar is 150px. Full title, truncated with an ellipsis. Right-click still closes.
