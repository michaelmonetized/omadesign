# omadesign.site

TanStack Start + Catppuccin Tailwind (mocha default, latte if you pick it, never system).

```sh
cd site
bun install
bun run dev
```

Vercel: set the project root to `site/`. Build command `bun run build` (Nitro `vercel` preset writes `.vercel/output`).

```sh
cd site
bunx vercel --prod
```

`/install` is the curl installer.
