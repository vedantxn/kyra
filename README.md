# Kyra V1

Kyra is a proactive personal operations interface. It turns commitments found in communications and calendars into evidence-backed open loops, separates what is on you from what is waiting on someone else, and helps protect time for the work that matters.

This repository is an independent, software-first V1 exploration based on public product material and the interface references documented in [`research/current-interface/README.md`](research/current-interface/README.md). It is not the official Kyra product.

## What works now

- A native Tauri 2 macOS shell with a global `Command+K` show/hide shortcut.
- A Rust-owned SQLite database with migrations and seeded evidence-backed open loops.
- A focused “Night” briefing, today timeline, open-loop groups, and an expandable three-day planner.
- `/task <what needs doing>` to persist a new local open loop.
- A direct Google connector using system-browser OAuth, PKCE, a random loopback callback, and no embedded client secret.
- Gmail read-only synchronization for up to 500 Inbox and Sent messages from the last 30 days.
- Google Calendar synchronization plus autonomous create, update, reschedule, attendee, cancellation, and deletion operations with event-version checks.
- Provider payload encryption before SQLite; refresh tokens and per-account data keys live in macOS Keychain.
- Startup, manual, and five-minute single-flight synchronization with bounded retry backoff.
- `/cal <title> <time>` creates a real one-hour Google Calendar event when connected, for example `/cal standup 9am`.
- Optimistic concurrency when completing an open loop.
- A browser preview that uses fixture data without exposing SQLite or privileged APIs to the webview.

Gmail is indexed for the later AI-engine milestone but does not create tasks yet. Message-shaped browser-preview evidence remains explicit fixture data; the V1 never claims live WhatsApp access.

## Google test-user setup

1. Create a Google Cloud project and enable the Gmail API and Google Calendar API.
2. Configure the OAuth consent screen in testing mode and add the Google accounts that may connect.
3. Create an OAuth client with application type **Desktop app**.
4. Copy `.env.example` to `.env.local` and add the desktop client ID:

```bash
KYRA_GOOGLE_CLIENT_ID=your-client-id.apps.googleusercontent.com
```

No client secret is used. `.env.local` is ignored by Git. Rebuild the native app after changing the client ID so a Finder-launched bundle can read the public desktop client identifier.

## Run it

Prerequisites: macOS, Rust, Node.js, and pnpm.

```bash
pnpm install
pnpm tauri dev
```

Browser-only UI preview:

```bash
pnpm dev
```

Verification:

```bash
pnpm build
pnpm test
cd src-tauri
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

## Architecture

The React webview is an unprivileged renderer. It cannot access SQLite, the filesystem, connector credentials, or future model credentials directly. Typed Tauri commands cross into the Rust core, and a window-scoped capability grants only the application commands used by this slice. Google refresh tokens and encryption keys never cross that boundary.

See [`research/TECHNICAL_ARCHITECTURE_V1.md`](research/TECHNICAL_ARCHITECTURE_V1.md) for the full V1 architecture and trust boundaries.
