# Welcome screen agreement

Reference: `media/welcome-mock.jpg`. Decisions recorded with Michael on
2026-09-21. This specifies the replacement welcome screen; it is not a record
of completed implementation or human QA.

## Structure and appearance

Three columns: Your Work, central creation/help actions, Projects. Follow the
mock's composition and use the app's existing Omarchy theme palette and font.
There is no independent light/dark setting. Show the installed version rather
than hard-coding the mock's 0.6.0 label. Retain startup preferences.

Both file browsers use masonry thumbnails at their natural aspect ratio, with
at most three columns per browser. Names appear on hover. Clicking opens an
item. Shift-clicking the first item starts multi-selection. Subsequent selection
must not unexpectedly open a document. Selected files need an explicit Open
action and a way to clear selection.

Project cards show a folder outline around a pile of asset thumbnails, as in
the Synchro reference requested by Michael. Project navigation has depth.

## Your Work

Find every `.oma` file recursively beneath `~/`, sorted by modification time,
newest first. This is a file browser, not the application's recent-path list.
Recovered is a separate tab for recovery snapshots.

Skip hidden directories, Trash and symlinks. Check visible directories for the
hidden `.omabrand` project marker without walking hidden directories for files.
Do not impose a traversal-depth cap. Scanning and preview generation must keep
the UI responsive and show incomplete/error states honestly.

The funnel opens a mode select list: Vector, Raster, Layout, Photo, Motion.
Provide a way to clear the selection to show all files.

## Projects

A project is any directory beneath `~/`, at any depth, containing `.omabrand`.
The recent tab shows projects on the machine. Opening a project initially shows
all descendant `.oma` files, sorted newest modified first. Subprojects appear
above document thumbnails and can be entered. Provide navigation back out.

Team appears only while the desktop is signed into cloud and shared team
projects exist. The cloud account's own projects alone do not justify the tab.

## Creation

| Target | Action |
| --- | --- |
| Vector main + button | Template chooser |
| Vector right file icon | Blank-document size chooser |
| Raster | Size chooser; no raster-specific template library |
| Layout main + button | Template chooser |
| Layout right file icon | Blank-document size chooser |
| Photo main + button | Enter Photo workspace |
| Photo folder icon | Choose a folder |
| Photo image icon | Choose an image |
| Project | Open the brand editor |

Motion is deliberately absent from creation actions because it needs existing
canvas elements. It remains an editing mode and a file-browser filter option.

## Agent actions

Learn with AI opens a free-form prompt box. Pass the question to Omarchy's
default agent with instructions describing where to curl Omadesign's Markdown
docs/index. Supply version-matched bundled docs as an offline fallback.

Create with agent opens a free-form brief box. Point the configured agent to
the bundled `omadesign-create` skill. Use the browsed project directory as its
working directory when available. The agent should produce editable `.oma`
work, inspect native rendering, and deliver requested exports.

Both actions use Omarchy's agent choice. If no installed default is configured,
explain how to choose one. Do not silently choose or install an agent.

## Links and availability

- Join the conversation: `https://discord.gg/ejkZS2RBx` (currently expires
  2026-10-21; replace with a non-expiring invite when supplied).
- Cloud: registration when working, waitlist otherwise. On 2026-09-21 the live
  `/cloud` page opened the Clerk registration form and `/api/cloud` returned its
  production endpoint configuration. This check did not re-test collaboration
  between two users. Prior rollout evidence is in `docs/cloud-rollout.md`.
- What's new: installed-version release notes.
- Docs, bugs and contribution links use the existing official destinations.
- Update Available should use the existing updater and show only when a newer
  compatible release is known. Opening its details preserves the current
  explicit install-and-restart action.

## Still to validate during implementation

Mode filtering needs trustworthy saved metadata or a documented legacy fallback.
Do not infer that every `.oma` is a vector document. Verify nested projects,
empty projects, recovery, hidden/symlink exclusions, incremental refresh, and
multi-selection with real files. Validate small windows and both light and dark
Omarchy palettes without adding an app theme toggle.
