# Cloud collaboration

The workspace is at **https://omadesign.app/cloud**. Cloud collaboration in the 0.5.2 nightly shares project files, assets and immutable flat exports. Clients review snapshots; designers continue authoring in the native Linux app.

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

The desktop transport is `/api/cloud`. `/project/:id` redirects to its authenticated workspace. The accepted 0.5.0 binary remains separate from nightly builds; see [rollout tracking](cloud-rollout.md).

## Cloud collaboration waitlist

The fullscreen homepage takeover at `/#cloud` lists planned 0.5.2 collaboration features and accepts
cloud access signups through `POST /api/waitlist`. This is a waitlist, not a
claim that collaborative editing is already released.

Signups are persisted in Convex `waitlistSignups`, indexed by list and normalized
email. Cloud and competition audiences are separate; duplicate submissions do
not create duplicate records or disclose whether an address has signed up.
No emails are sent by signup. Invitations are separate, explicit actions sent through Resend.

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

### 0.5.2 scope

Cloud collaboration means project file and asset sharing with team member access
control; client review, comments and annotations on flat snapshots; publishing
finished public work to the Omadesign user showcase; and entering showcased work
into competitions. The browser is for sharing and review, not a live online
Omadesign editor. Live multi-user editing, presence and browser authoring are
outside this scope.

The homepage takeover plays the supplied film muted when interactive, introduces
the five features from the bottom of the left stack upward, and reveals the
email-only waitlist in the right column when the film ends. Visitors can skip to
the signup or leave for the native app site. Reduced motion and playback failure
reveal signup immediately.
