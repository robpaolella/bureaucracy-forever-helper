# Bureaucracy Forever Helper

Desktop helper for the guild's officers: it connects the in-game loot add-on (bureaucracy-forever-addon) to the guild site. Runs on officers' Macs and Windows PCs, in the tray or menu bar.

## Stack
Tauri 2 (Rust, in `src-tauri/`) with a plain TypeScript front-end (Vite, Vitest, ESLint), npm. Shipped as installable apps; nothing is hosted.

## Checks
Run in this order before any PR. The `ship` skill reads this section.
```
npm run lint && npm run typecheck && npm test
cd src-tauri && cargo fmt --check && cargo clippy -- -D warnings && cargo test
```
Neither half needs a display. There is no `build` step in Checks; `npm run build` only builds the front-end and `npm run tauri build -- --debug` builds the full app.

## Run locally
System prerequisites:
- Node and npm.
- Rust via rustup, with the `clippy` and `rustfmt` components.
- On Linux, the Tauri libraries: https://v2.tauri.app/start/prerequisites/.
- On a Linux machine with no display: `xvfb` (for `xvfb-run`) and `dbus-run-session`. To test the window and tray, also `xdotool x11-apps imagemagick openbox wmctrl`.
```
npm install
npm run tauri dev       # opens the app; the front-end is served on http://localhost:1420
```
Without a display, run it as `dbus-run-session xvfb-run -a npm run tauri dev`. Without a D-Bus session the app starts but shows no window.

## Layout
- `src/` front-end (`main.ts` entry), with tests beside the code (`*.test.ts`).
- `src-tauri/src/lib.rs` app setup, tray and window behaviour; `main.rs` is the entry point.
- `src-tauri/tauri.conf.json` app, window and bundle config; `src-tauri/capabilities/` what the front-end may call.
- `design/` holds designs. The helper's look comes from the design made in #2; this file names the design file once that design exists.
- `work/` multi-session work folders; `backlog.md` follow-up ideas.

## Deploy
The helper ships as GitHub Releases built by CI (#11, #16). Nothing is hosted and there are no servers, env vars or migrations.

## Needs Robert's own merge
Agents may merge reviewed PRs into a `package/*` branch, except these, which only Robert
merges (docs/git-workflow.md, Merging):
- Any `risk:high` PR, and any change to `AGENTS.md` or `CLAUDE.md`.
- Changes that reach beyond the package: real data, live hosting settings, logins and
  secrets, outside accounts.
- CI workflows, which build and publish the releases.
- The Tauri config and capabilities, which control what the app may do and will later hold the updater key setup.

The guardrail refuses an agent's merge of a PR that changes a file matching a line
below, read from this file on `main`. Paths start at the repo root; `**` crosses
folders, `*` and `?` stay within one folder; end a folder with `/**`.
```paths
.github/workflows/**
src-tauri/tauri.conf.json
src-tauri/capabilities/**
```

## Watch out for
- The window only appears under xvfb inside `dbus-run-session xvfb-run …`.
- Closing the window hides it; only the tray's Quit exits.
- The tray icon is a placeholder blue square until the design in #2.

## Reference material
`ADDON-SPEC.md` in the site repo (robpaolella/bureaucracy-forever; tracked in its #271) is the contract for the data the add-on, helper and site exchange. Change the format there, not here.
