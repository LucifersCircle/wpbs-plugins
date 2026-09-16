# Extension Status Plugin

This local WPBS plugin discovers Inkdex extensions from the hosted registry,
pings their source sites on a schedule, posts a summary embed to Discord, and
serves per-extension live checks through `/status extension:<name>`.

The plugin intentionally keeps the report channel shape generic:

```yaml
settings:
  registry_base_url: https://inkdex.github.io/extensions/0.9/stable
  discovery_cron: "0 0 */6 * * * *"
  ping_cron: "0 */3 * * * * *"
  request_timeout_ms: 10000
  progress:
    discord: false
    every: 10
  channels:
    ping: "DISCORD_CHANNEL_ID"
    tests: "DISCORD_CHANNEL_ID"
```

`channels.tests` is reserved for a later GitHub test-runner report. It is not
used by the current ping scanner.

On plugin load, the plugin immediately rewrites the existing Discord report
messages from the saved report, preserving their message IDs after downtime. If
no saved report exists, it captures the Inkdex catalog and generates the first
ping report before `/status` needs it.

`/status` uses the latest saved report only to find the extension and its
captured website URL. It then pings that URL live and returns the fresh result.
Captured GraphQL endpoints are probed with a JSON POST instead of GET, so APIs
like AniList are not marked failed just because they return 404 to a browser
style GET.

Failed sites keep a `Down for` duration across reports while the same extension
and captured URL continue failing. The duration resets after the site recovers
or the captured URL changes.

Discord availability embeds deliberately omit raw HTTP status codes. Scheduled
reports use a compact summary/key followed by one embed for every repository
discovered in the registry. Each repository heading links only its name and
shows its source count as plain `• (x)` text. Each source is one compact row with
its status indicator on the left and its linked name beside it. Only failed rows
add a `Down for` duration, so healthy sources do not create empty columns or
placeholder values. Repository headings have their own line, and each
repository heading is separated from its source list by a blank line. The
summary shows only a relative `Next ping` countdown and
an explicit relative `Last ping` time recorded after all probes complete. A
stale repost keeps the previous `Last ping` value, while a genuine scan resets
it. The summary is labeled `Only reports site reachability`. Newly discovered
repositories are included automatically; if the report grows beyond Discord's
per-message limits, the embeds continue in a follow-up message. Newly discovered
sources receive an inline `New` indicator in their actual repository. The live
`/status` response presents a user-facing availability result while keeping the
underlying response code only in plugin state for classification.

The first report publish creates the required Discord messages and stores their
IDs in plugin state. Later scheduled and manual reports edit those same messages
in place. If a stored message was deleted, the plugin creates its replacement;
obsolete overflow messages are removed when the report becomes smaller.

Templated extension URLs retain their static origin during discovery. For
example, MangaUpdates' `https://api.mangaupdates.com${...}` request template is
captured as `https://api.mangaupdates.com` instead of being discarded.

Report channels can be set from Discord:

```text
/set-report-channel report: ping channel: #extension-pings
/set-report-channel report: tests channel: #extension-tests
```

These command-set channels are stored in plugin state and override the static
config values on later restarts. The plugin only accepts this command from
members with Manage Server permission. Setting the ping report channel also
posts the latest saved ping report to that channel when one is available.

The latest saved report can also be posted manually:

```text
/post-report report: ping
```

For the ping report, this command performs a fresh check of every captured site,
edits the deferred Discord response with progress as the scan runs, saves the new
report, and posts it to the configured channel. `/post-report report: tests` is
registered as a placeholder for the future test runner and returns a
not-yet-available message for now.

During report UI development, the temporary command below re-renders and posts
the latest saved ping report without refreshing the catalog or pinging any site:

```text
/post-stale-report
```

It requires Manage Server and intentionally bypasses the saved report classifier
freshness check. Remove this command after the report UI is finalized.

Report intervals can be changed from Discord by members with Manage Server:

```text
/set-report-interval schedule: Extension list refresh interval: Every 6 hours
/set-report-interval schedule: Ping report interval: Every 12 hours
```

The available choices are every 3 minutes, every hour, every 6 hours, every 12
hours, and daily at 09:00 local time. The temporary default ping interval is
every 3 minutes for report-refresh testing. Extension-list and ping schedule
changes take effect immediately and persist across restarts. A tests interval
can be saved now for the future test runner, but it does not schedule a tests job
yet.

The report's `Cloudflare` count follows the Inkdex Paperback interceptor signal:
only `cf-mitigated: challenge` is treated as Cloudflare protection. A normal
2xx/3xx response served through Cloudflare's CDN is counted as working, and a
4xx/5xx response without `cf-mitigated: challenge` is counted as failed.

Required WPBS services and permissions:

The host must expose the `EditMessage` Discord request from
[wpbs-rs/wpbs#44](https://github.com/wpbs-rs/wpbs/pull/44) or a later compatible
WPBS release.

```yaml
services:
  job_scheduler:
    enabled: true
  discord:
    enabled: true

plugins:
  google_ping:
    plugin: local/google-ping:0.1.0
    permissions:
      services:
        job_scheduler:
          - ScheduledJobs
        discord:
          requests:
            - InteractionCallback
            - UpdateInteractionOriginal
            - CreateMessage
            - EditMessage
            - DeleteMessage
          events:
            - InteractionCreate
          interactions:
            - ApplicationCommands
```

The Discord app also needs server/channel access outside WPBS. Invite it with
both `bot` and `applications.commands` scopes, and grant the bot role at least
View Channel, Send Messages, and Embed Links in the configured report channel.
The minimum invite permission integer for those three permissions is `19456`.
