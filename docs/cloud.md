# Cloud collaboration

The workspace is at **https://omadesign.app/cloud**. Cloud collaboration, included in 0.5.4, shares project files, assets and immutable flat exports. Clients review snapshots; designers continue authoring in the native Linux app.

## Sign in and connect a desktop

Use a verified email address to sign in with Clerk. The native app's **File → Sign in…** opens your browser with a device code. Approve only the code displayed by the desktop. Desktop credentials expire after 30 days; revoke them immediately from `/account` or disconnect in the app. Your local identity file is written with owner-only permissions. A name or email alone never grants cloud access.

## Share a project

Choose **File → Push project + review export** to upload a versioned `.oma` source and a PNG of the current document. Project font assets are included, and raster pixels remain embedded in the design file. **Upload project asset…** adds another file. **Cloud projects… → Pull & open** opens the latest source in a separate document and downloads shared assets into a new folder under the app's cloud-downloads directory.

Transfers are explicit. Local edits do not silently upload or overwrite another designer's version. Every push adds files; cloud comments remain attached to the snapshot on which they were written. Save the local `.oma` after its first push to keep the cloud link with the document.

The web project accepts source, asset and snapshot uploads. Source and asset files are limited to 100 MB each; flat PNG/JPEG/WebP exports to 20 MB; each project supports 200 files. The initial service limit is 100 projects per account and 100 members per project. Archive a project from the web workspace to hide it from collaborators and unpublish its public work; its owner can restore it.

## Team and client review

Owners invite verified email addresses as editors or reviewers. Invitations arrive through Resend and expire after seven days. Sign in with the invited email and accept from the workspace. Owners can change a member's role, remove access or cancel an invitation.

| Role | Access |
| --- | --- |
| Owner | Project files, uploads, review, team management, archive and publishing |
| Editor | Source and asset downloads, uploads, comments, replies and resolving threads |
| Reviewer | Flat exports, pins, rectangular annotations, comments and replies; can resolve their own threads |

Select an export version in the web project. Click to pin a comment or drag a rectangular annotation, then post the thread. Coordinates scale with the image, including on mobile. Review threads, replies and resolved status persist in Convex. The desktop's **Review annotations…** window loads the same versioned exports and threads, with replies and resolve/reopen controls.

Private downloads recheck membership on every request. Revoking a member or desktop blocks future requests. Previously downloaded files are still held by the recipient.

## Public showcase and competitions

Publication is a separate owner action. Select a finished flat export, add its title and description, and choose **Publish selected export** in the desktop or web workspace. Source files, assets and private review threads are excluded. `/showcase` lists public works; `/showcase/:id` displays the complete flat image. Unpublishing removes it from the gallery, competition displays and image endpoint. Copies already saved by viewers cannot be recalled.

Use an owned public showcase work to enter an open competition. Entries are persisted, duplicate submissions are rejected, and closing dates are enforced by the server. `/compete` lists competitions, public entries and your withdrawal controls. Competitions remain unpublished until the operator supplies a real brief and opening/closing dates.

## Service operations

Vercel hosts the web app. Clerk owns identity, Convex owns authorization, data and storage, and the Convex Resend component queues invitations with retries and delivery webhooks. There is no browser-local substitute for these services.

Production uses the Clerk issuer `https://clerk.omadesign.app`, Convex deployment `healthy-buzzard-921`, and Resend sender `collab@mail.omadesign.app`. Development uses separate Clerk and Convex instances. Secrets belong in service environment configuration and ignored local env files, never Git.

See `site/.env.example` for web variable names. Convex requires `CLERK_JWT_ISSUER_DOMAIN`, `RESEND_API_KEY`, `RESEND_WEBHOOK_SECRET` and the existing `WAITLIST_SECRET`. Clerk's `convex` JWT template must include audience `convex`, email, verified email and name claims. The production Resend webhook is `/resend-webhook` on the Convex HTTP domain.

From `site/`, use `bunx convex dev --once` for development and `bunx convex deploy --yes` for production. Use `vercel pull --environment=production`, `vercel build --prod`, and `vercel deploy --prebuilt --prod` for the site. Production and development keys must not be mixed.

Operators create or update competition briefs through the internal `showcase:configureCompetition` function, with `title`, `description`, millisecond `opens`/`closes` timestamps, and `active`. No public administrator endpoint exists. `mail:deliveryProbe` sends only to Resend's delivery test sink; `mail:deliveryStatus` inspects the returned queue id. Expired upload intents, device requests and rate counters are cleaned automatically. Invitations are capped at 20 per account per hour.

The desktop transport is `/api/cloud`. `/project/:id` redirects to its authenticated workspace. Historical cloud rollout evidence remains in [rollout tracking](cloud-rollout.md). The accepted 0.5.0 binary and its human QA record remain preserved separately from newer builds.

## Website entry and historical waitlist

Open `/cloud` to sign in and use project sharing and snapshot review. The optional
homepage film at `/#cloud` introduces these features and links to the workspace
and this guide. It does not open automatically on every visit, and it no longer
asks people to wait for functionality already included in the release.

The earlier cloud waitlist records remain in Convex `waitlistSignups`, indexed
by list and normalized email. Cloud and competition audiences stay separate.
There is no public signup-list or lookup endpoint. Signup never sent emails;
member invitations remain separate, explicit actions through Resend.

An authenticated operator can review or export retained records in the Convex
dashboard, or remove an address on request from `site/`:

```sh
bunx convex run waitlist:removeSignup '{"email":"address@example.com","list":"cloud"}' --prod
```

The removal function is internal and cannot be invoked through the public API.

## Scope

Cloud collaboration means project file and asset sharing with member access
control; client review, comments and annotations on flat snapshots; publishing
finished public work to the Omadesign showcase; and entering that work into open
competitions. The browser is for sharing and review. Live multi-user canvas
editing, presence and browser authoring are outside this scope.
