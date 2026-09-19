# Convex backend

The backend now lives in `site/convex/`, beside the website’s package and lockfile.
The original cloud schema is preserved there, with durable waitlist storage.

From `site/`, run `bunx convex dev --once` for development and `bunx convex deploy`
for production. Set `WAITLIST_SECRET` on Convex and the same secret plus
`CONVEX_URL` on Vercel. Deploy backend changes before deploying the site.
See [cloud operations](../docs/cloud.md#cloud-collaboration-waitlist).
