# omadesign website

The homepage and documentation share one TanStack Start source in `src/`. Markdown
pages read the repository's manuals directly. GitHub Pages serves the generated
site at <https://michaelmonetized.github.io/omadesign/>.

## Develop

Use Node 22.12+ and a current Bun release that supports the checked-in v2 lockfile.

```sh
cd site
bun install --frozen-lockfile
bun run dev
```

## Build, review, publish

```sh
# From site/: build every page and validate local links and assets.
bun run build:pages

# Review the prerendered site, including direct documentation URLs.
GITHUB_PAGES=1 bun run preview
```

Open the preview server's `/omadesign/` URL. The Pages build uses that base path;
plain anchors and public assets should use `sitePath()` from `src/site.ts`.
TanStack router links use the configured router base automatically.

The reviewed files live in `.pages/`; `.pages-build/` contains temporary client and
server build files. Both are ignored. No server or build service is needed on
GitHub Pages. `/omadesign/install` serves the installer as a static file.

Commit website and documentation changes, build and review the resulting revision,
then publish that exact artifact:

```sh
bun run publish:pages
```

Publishing requires GitHub CLI authentication and write access to the Git remote.
It pushes generated files to `gh-pages` from an isolated temporary checkout and
sets the existing Pages site to use that branch. It preserves deployment history
and leaves your source checkout untouched. It refuses stale artifacts, uncommitted
website changes, missing routes/assets, and files over GitHub's size limit. It does
not create a GitHub Actions workflow. Rebuild after committing a new revision.

The same commands can be run from the repository root with a supported Node:

```sh
node site/scripts/publish-pages.mjs
node site/scripts/publish-pages.mjs --publish
```

## Optional server deployment

The default `bun run build` retains Nitro's Vercel preset and root-relative URLs.
No Vercel project is currently linked in this checkout; link the intended project
explicitly before using Vercel deployment commands. GitHub Pages remains the
current production host.
