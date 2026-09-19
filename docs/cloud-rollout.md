# 0.5.2 cloud collaboration rollout

Tracking epic: https://github.com/michaelmonetized/omadesign/issues/62

- #63 identity, authorization and service setup
- #64 project membership and file storage
- #65 flat review, showcase and competitions
- #66 native desktop integration
- #67 nightly and release verification

Integration branch: `nightly`. Stack implementation PRs, merge verified slices into nightly, and promote to master only after complete release validation.

Cloud stores project packages, assets, immutable flat snapshots, review threads and public selections. It does not run the native editor in a browser. Clerk owns user identity; Convex owns project authorization, data and storage; Resend delivers explicit project invitations; Vercel hosts the web application.

The original working tree contains separate uncommitted Layout work. Keep it intact; reconcile those changes before the final 0.5.2 native release.

Accepted 0.5.0 human QA remains recorded: Michael confirmed “Qa passed” on 2026-09-12 for SHA-256 `69c35f6cc3f4397a883798172a0d654cb05289e4715d0678db7822c823e6791d`.
