# Release verification — cadence 0.2.0

Target: Pop!_OS 24.04, KDE, X11, x86_64.

| Check | Expected coverage |
| --- | --- |
| Svelte / TypeScript | Compilation with no errors or warnings |
| Rust core | Standalone and session timing, pause exclusion, countdown expiry, recovery, migrations, history edits, effective dates, and backup validation |
| Calendar helpers | Grid date and display formatting |
| Browser interaction | Standalone CRUD, multi-stint sessions, session deletion and unlinking, manual assignment, neutral interface copy, responsive sizing, keyboard use, and reduced motion |
| Dependency audit | No known npm vulnerabilities at build time |
| Release packaging | Debian package and AppImage |
| Native integration | Real WebKitGTK IPC, tray, close/reopen, single instance, session and stint persistence, SQLite, CSV/JSON round trip, and countdown completion |

The browser scenarios use an IPC fixture; the native smoke test runs the release executable with an isolated XDG data directory. No smoke-test records enter the user's normal history.

The native script writes timer and history screenshots to `artifacts/`. It confirms that the idle timer fits the default 380 × 620 window. Long history and editor content may scroll. Notification submission is checked through the native countdown flow; whether a notification is visibly presented depends on desktop policy.

Timer and session suspend behavior is tested with a controlled clock. The native application also installs a systemd-logind delay inhibitor. Physical suspend is intentionally recorded separately because it interrupts the host session.

Wayland and Linux distributions outside the initial target are not part of this release validation.
