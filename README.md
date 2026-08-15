# Kyra V1

Kyra is a proactive personal operations interface. It turns commitments found in communications and calendars into evidence-backed open loops, separates what is on you from what is waiting on someone else, and helps protect time for the work that matters.

This repository is an independent, software-first V1 exploration based on public product material and the interface references documented in [`research/current-interface/README.md`](research/current-interface/README.md). It is not the official Kyra product.

## What works now

- A native Tauri 2 macOS shell with a global `Command+K` show/hide shortcut.
- A Rust-owned SQLite database with migrations and seeded evidence-backed open loops.
- A focused “Night” briefing, today timeline, open-loop groups, and an expandable three-day planner.
- `/task <what needs doing>` to persist a new local open loop.
- `/cal <title> <time>` to persist a one-hour execution block, for example `/cal standup 9am`.
- Optimistic concurrency when completing an open loop.
- A browser preview that uses fixture data without exposing SQLite or privileged APIs to the webview.

Gmail and Google Calendar are not connected yet. Message-shaped evidence is explicitly fixture data; the V1 never claims live WhatsApp access.

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

The React webview is an unprivileged renderer. It cannot access SQLite, the filesystem, connectors, or future model credentials directly. Typed Tauri commands cross into the Rust core, and a window-scoped capability grants only the five commands needed by this slice.

See [`research/TECHNICAL_ARCHITECTURE_V1.md`](research/TECHNICAL_ARCHITECTURE_V1.md) for the full V1 architecture and trust boundaries.
