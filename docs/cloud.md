# Cloud, showcase, and omadesign.app

The public site is **https://omadesign.app**. GitHub Pages still exists so old
links work; the homepage sends that host to the real domain.

Cloud is opt-in. A document stays on disk until **File → Enable cloud sync**.

## Identity

Clerk signs the website when `VITE_CLERK_PUBLISHABLE_KEY` is set. Until then,
Account stores a name and email in the browser, and the desktop app stores the
same pair in `~/.config/omadesign/cloud-identity.json`. Match those emails when
you invite a collaborator.

## Desktop

- **File → Sign in…** writes the identity file.
- **File → Enable cloud sync** attaches a project id to the `.oma` file.
- **File → Invite collaborator…** adds an email to that document.
- **File → Publish to showcase…** is a second, explicit opt-in. Unpublished
  files never appear in the gallery.
- Layout inspector: write a note, **Pin on canvas**, resolve on the thread.
  Unresolved counts sit on the selected frame.

Set `OMADESIGN_CLOUD_URL` or the Sign in field to `https://omadesign.app/api/cloud`
when the Vercel API is live. Without a URL the desktop keeps a local store under
`~/.local/share/omadesign/cloud-store.json`.

## Website

| Path | What it is |
| --- | --- |
| `/showcase` | Public gallery. Seeded with the 0.5.0 Layout starters. |
| `/showcase/:id` | One published project. Missing or private ids stay dark. |
| `/compete` | Rules plus a waitlist. Entries are not required on day one. |
| `/account` | Sign in / identity. |
| `/project/:id` | Collaborator view: pin, reply, resolve. |
| `/api/cloud` | Sync, publish, waitlist. Last-write-wins. |

## Convex and Clerk

Schema lives in `site/convex/schema.ts`. Shared field names live in
`packages/schema/`. Deploy Convex and set:

```
VITE_CLERK_PUBLISHABLE_KEY=
CLERK_SECRET_KEY=
VITE_CONVEX_URL=
CONVEX_DEPLOYMENT=
```

Rate-limit invites. No anonymous public writes. Comments can contain names;
treat the gallery as public text.

## Domain

`omadesign.app` was purchased on Vercel Domains. Assign it to the site project
so TLS terminates there. `www.omadesign.app` redirects to the apex.

GitHub Pages (`https://michaelmonetized.github.io/omadesign/`) redirects in the
browser to `https://omadesign.app`. Keep publishing Pages so the old URL does
not 404 while DNS settles.

## Cloud collaboration waitlist

The homepage at `/#cloud` lists planned 0.5.2 collaboration features and accepts
cloud access signups through `POST /api/waitlist`. This is a waitlist, not a
claim that collaborative editing is already released.

Signups are persisted in Convex `waitlistSignups`, indexed by list and normalized
email. Cloud and competition audiences are separate; duplicate submissions do
not create duplicate records or disclose whether an address has signed up.
No emails are sent by signup. The legacy `/api/cloud` waitlist action uses the
same durable backend for the competition list.

The server requires `CONVEX_URL` and `WAITLIST_SECRET`; the same secret must be
set on the Convex deployment. These are server-only variables. An unavailable
backend returns an error and never claims to have saved a signup. Requests are
validated, origin-checked, and limited to five attempts per minute per hashed
visitor address. Raw addresses are not stored; expired counters are cleaned up.

For operations, use the authenticated Convex dashboard to review or export
`waitlistSignups`. There is no public signup-list or lookup endpoint.

To remove an address on request, an authenticated operator can run from `site/`:

```sh
bunx convex run waitlist:removeSignup '{"email":"address@example.com","list":"cloud"}' --prod
```

The removal function is internal and cannot be invoked through the public API.
