# Privacy and anonymous usage

Open **omadesign → Config** to enable or disable **Send anonymous usage data**. It is off by default. The choice is saved locally in `~/.config/omadesign/preferences.json` (or your `XDG_CONFIG_HOME`). Turning it off clears queued counts and stops future transmissions; an already sent request cannot be recalled.

When enabled, the desktop sends aggregate counts of tools, modes and features, and fixed error/crash categories, through `https://omadesign.app/api/telemetry`. Each batch includes the app version, Linux CPU architecture and UTC calendar day/week/month. It contains no account, installation or device ID, document contents, names, paths, typed text, search terms, screenshots, raw error messages, stack traces or core dumps. We do not use cookies, IP hashes or fingerprints for usage counts.

Daily, weekly and monthly activity uses local period markers. The server adds one active-installation count for each period reported by the app. These are approximate counts of **participating installations**, not identifiable people or exact unique users. Offline use, failed delivery, multiple installations and reinstalling can affect totals. Counts are best effort; failed batches are not repeatedly resent.

The application backend stores only aggregate counters in Convex. It does not read, retain or forward client addresses or request metadata. Fixed crash/error categories are sent from our server to Sentry, with no client request context and an explicit non-user IP value. Native aborts and segmentation faults are classified by the process exit signal; the app never uploads the core dump. Diagnostics have no user content or stack trace, so they indicate the category and affected version rather than the exact failing code location. Sentry category notifications are limited to one per hour per category/version/architecture; Convex retains the actual aggregate count.

As with any HTTPS service, the network and hosting providers receive connection metadata to deliver the request. Omadesign does not use that metadata to identify users. The hosting provider's own infrastructure/security logs are separate from these application counters; this is not a claim that network transport hides your address from the service provider.

Download totals come from GitHub's existing public release-asset counters. They include repeat downloads and updates, not unique people. Reading `/api/stats` does not register a desktop installation or enable usage reporting.

**Check for updates automatically** is a separate preference. Update checks contact GitHub even with usage reporting disabled. An update only installs when you click its button. Before restart, Omadesign saves open documents, original photo pixels, unsaved photo adjustments and palette drafts in the local recovery directory (`~/.local/share/omadesign`, or `XDG_DATA_HOME/omadesign`). No recovery contents are uploaded by this process.

## For maintainers

Set the same server-only `TELEMETRY_SECRET` in the site and Convex deployments, plus the existing project's public `SENTRY_DSN` on the site. Never add these to a client-side `VITE_` variable. Keep platform request/body logging disabled for the intake route. The endpoint rejects unknown keys and non-allowlisted metrics, bounds bodies/counts and uses a global intake budget without visitor identifiers.

Authenticated operators can read aggregate usage with:

```sh
cd site
CONVEX_DEPLOYMENT=prod:healthy-buzzard-921 bunx convex run --prod telemetry:summary '{}'
```

The report groups each calendar period and metric across app versions and architectures. Detailed usage has no public read endpoint. For download totals, use `https://omadesign.app/api/stats` and label them **package downloads**, not users.
