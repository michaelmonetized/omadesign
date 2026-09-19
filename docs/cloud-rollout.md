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

## Verified 2026-09-19

The four implementation PRs are merged into nightly: #68 foundation, #69 web workspace, #70 native client, #71 verification and access boundaries. Stable master and the accepted 0.5.0 installation remain separate.

- Production web deployment: `dpl_5yMKZhDMeUxrUbtzPeAVmQaJsZFp`, live at https://omadesign.app/cloud.
- Clerk production issuer and domain verified. Production Clerk ticket sign-in issues a real Convex JWT with verified-email claims.
- Production round trip: flat upload, rectangle annotation, reply, resolution, authenticated byte-matching download, anonymous denial, browser-approved desktop RPC, device revocation and archive all passed with a disposable QA account.
- Resend domain verified. A probe to Resend's own delivery sink reached `delivered`, confirmed through the Convex webhook status.
- Ten Convex tests pass, covering identity isolation, roles/revocation, storage intent ownership/expiry, annotation boundaries, invitations, publication removal, device expiry and archives.
- Native suite: 391 passed, 5 ignored. ARM64 optimized binary builds and reports `0.5.2-nightly.1`.
- Native real-service push/pull, flat export, annotation/reply and review-window capture pass. Preview decoding is bounded and runs on a worker.
- ARM64 binary SHA-256: `62c04ba9e2af80f6c184c15d94b7d376e5e1d6859cddb79ae8d84eb2378ec9f8`.
- Local command `omadesign-nightly` launches the separate nightly binary with separate app config/data directories. Installed stable SHA-256 remains `69c35f6cc3f4397a883798172a0d654cb05289e4715d0678db7822c823e6791d`.
- Browser automation recovered after a session reset. Development review at 390x844 posted both a pin and a rectangular annotation. Production browser sign-in, archive restore, source upload and pin posting passed. The 1600x1000 review image loaded; portrait 390x844 and landscape 844x390 had no document-width overflow.
- [Native review screenshot](qa/cloud-review-nightly.png) records the optimized nightly app showing its real cloud snapshot and thread.

Disposable dev/prod QA accounts and their projects/files were removed after verification. The temporary operator cleanup function was also removed from both deployments.

## Release gates still open

GitHub Actions run https://github.com/michaelmonetized/omadesign/actions/runs/35452482385 was rejected before any job steps: account locked due to a billing issue. Do not describe x86_64/ARM64 CI artifacts as produced. After billing is restored, rerun the nightly workflow. Its daily schedule becomes active when the workflow is promoted to the default branch; until then use pushes to nightly or manual dispatch.

Final human acceptance of the new cloud/native release, x86_64 artifact validation, and reconciliation of the separate Layout working tree remain part of #67. This rollout does not change the accepted stable 0.5.0 QA status. The five proposed [mode competition briefs](competition-ideas.md) remain unpublished; dates, prizes and final rules are unset. Motion requires animated media support before that round can open.
