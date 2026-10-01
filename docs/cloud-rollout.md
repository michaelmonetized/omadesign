# Cloud collaboration rollout and release gates

Tracking epic: https://github.com/michaelmonetized/omadesign/issues/62

- #63 identity, authorization and service setup
- #64 project membership and file storage
- #65 flat review, showcase and competitions
- #66 native desktop integration
- #67 nightly and release verification

Integration branch: `nightly`. Stack implementation PRs, merge verified slices into nightly, and promote to master only after complete release validation.

Cloud stores project packages, assets, immutable flat snapshots, review threads and public selections. It does not run the native editor in a browser. Clerk owns user identity; Convex owns project authorization, data and storage; Resend delivers explicit project invitations; Vercel hosts the web application.

The separate Layout work was integrated and reconciled for 0.5.4. The September 19 evidence below is retained as release history.

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

## Release gate reconciliation — 0.6.2

Desktop builds and tests always run locally. Use `cargo test --lib --locked` and `./scripts/release.sh`; the release script locally builds and packages both ARM64 and x86_64 against glibc 2.35, with archive checksums. The earlier hosted native workflow has been removed. GitHub billing is not a release requirement and must not be enabled or paid.

Hosted site checks and deployment target Blacksmith only. On September 30, the Blacksmith dashboard confirmed that personal GitHub accounts are unsupported, which explains the unassigned jobs for `michaelmonetized/omadesign`. The workflows now skip personal-account repositories; that skip is not a test pass. Run the site checks locally and publish the resulting prebuilt output with Vercel CLI until the repository has a supported Blacksmith organization. No GitHub-hosted compute, cache or artifact storage is used.

The local site sequence is `bun install --frozen-lockfile`, `bunx vitest run`, `vercel pull --cwd .. --yes --environment=production`, `vercel build --cwd .. --prod`, `bun run typecheck`, then `vercel deploy --cwd .. --prebuilt --prod --archive=tgz --yes`. Run these from `site/`; inspect the deployment and verify public pages and installer after publication.

Michael has confirmed human QA passed and authorized completing and shipping #62, #67 and #83. The human gate is satisfied; the accepted 0.5.0 hash above remains preserved. Layout reconciliation was completed for 0.5.4. The [0.6.1 archive and physical-machine receipts](qa/release-0.6.1/archives.json) record subsequent ARM64 and x86_64 validation. The 0.6.2 release repeats package/runtime checks for the new input and review changes. The five proposed [mode competition briefs](competition-ideas.md) remain unpublished; dates, prizes and final rules are unset. Motion requires animated media support before that round can open.
