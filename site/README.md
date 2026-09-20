# omadesign website

The homepage and documentation share one TanStack Start source in `src/`. Markdown
pages read the repository's manuals directly. GitHub Pages serves the generated
site at <https://omadesign.app/>. GitHub Pages still publishes so the old URL
can redirect.

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

## Production deployment

Production is **https://omadesign.app**. The Vercel project's root directory is
`site`, with access to outside-root documentation sources enabled. Link and run
Vercel CLI from the repository root; the website itself builds inside `site/`.
CLI state and production environment files under `.vercel/` stay ignored.

For a local validated prebuilt deployment, run from the repository root:

```sh
vercel pull --yes --environment=production --scope hustle-launch
mise exec bun@1.4.2 -- vercel build --prod --scope hustle-launch
mise exec bun@1.4.2 -- bun run --cwd site typecheck
vercel deploy --prebuilt --prod --archive=tgz --yes --scope hustle-launch
```

The website-only Blacksmith workflow is available through:

```sh
bun scripts/ship.mts
```

That pushes the current branch and runs `.github/workflows/ship.yml` on
`blacksmith-4vcpu-ubuntu-2404`: frozen dependency install, Vercel production
build, TypeScript and output checks, then prebuilt deployment. It uses
root-linked CLI state and `.vercel/output`. Desktop Cargo builds and packages
always run locally. GitHub Pages remains the legacy URL entry point.
