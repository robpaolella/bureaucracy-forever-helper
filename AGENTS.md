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
- `lua5.1` (optional locally; CI requires it for the inbox test).
- On a Linux machine with no display: `xvfb` (for `xvfb-run`) and `dbus-run-session`. To test the window and tray, also `xdotool x11-apps imagemagick openbox wmctrl`.
```
npm install
npm run tauri dev       # opens the app; the front-end is served on http://localhost:1420
```
Without a display, run it as `dbus-run-session xvfb-run -a npm run tauri dev`. Without a D-Bus session the app starts but shows no window.

Native screenshots of the tray, its menu and a notification: the VM has no tray host or notification daemon, and needs no sudo for them.
- In a new empty temp folder outside any worktree, `apt-get download stalonetray dunst`, unpack each with `dpkg -x`, and run them from there. Nothing is installed. (`stalonetray` is the XEmbed tray libayatana-appindicator falls back to when no StatusNotifier watcher runs; `dunst` shows notifications.)
- Inside `dbus-run-session xvfb-run`, start openbox, then stalonetray and dunst, then the app. Capture the tray, the open menu and the notification (for example with `import -window root`).
- A notification needs a throwaway local trigger, never committed (#73 set a status and called `notify::raise` 35 seconds in). With dunst's defaults a left click only dismisses a notice; a middle click fires its action.
- Afterwards stop everything you started and delete the temp folder.

## Previews
Previews: keep until merge

Robert tests from his PC, connected remotely to the VM the agents run on, so he can't open `localhost` or the native app window. Serve every PR's preview on the VM's LAN address `192.168.50.10` (never `0.0.0.0`) and put the link in the PR's How to verify as `http://192.168.50.10:<port>/`.
- **App front-end:** `npm run dev -- --host 192.168.50.10 --port <port>`. Pick a port no other worktree holds (check `ss -ltn`); `strictPort` makes Vite exit if it's taken, so pick another and say which.
- **Design prototype:** `python3 -m http.server <port> --bind 192.168.50.10 --directory design/2-helper-window`, linked as `http://192.168.50.10:<port>/`.
- Check it: `curl -s -o /dev/null -w '%{http_code}' http://192.168.50.10:<port>/` returns 200, and `ss -ltn` shows the listener on `192.168.50.10:<port>`.
- The browser preview is the front-end only: Tauri-only calls fail in a browser. Show anything that needs the Rust side, and native window or tray behaviour, with screenshots taken under xvfb and attached to the PR. The PR says the real check on his Mac or Windows PC comes with a release build.
- If the preview dies, say so in the report; it doesn't block the PR.
- If the VM's address changes (it is DHCP), use the new one from `ip -4 addr` and update this section.

## Layout
- `src/` front-end (`main.ts` entry), with tests beside the code (`*.test.ts`).
- `src-tauri/src/lib.rs` app setup, tray and window behaviour; `main.rs` is the entry point.
- `src-tauri/tauri.conf.json` app, window and bundle config; `src-tauri/capabilities/` what the front-end may call.
- `design/` holds designs. The helper's look is `design/2-helper-window/index.html` (approved in #2): its window, tray menu, icons and every state, with screenshots in `screenshots/approved/real-size/`.
- `work/` multi-session work folders; `backlog.md` follow-up ideas.

## Releases
The helper ships as installers on GitHub Releases. Convention: /git/maestro/docs/git-workflow.md, Releases.
- **What is released:** the desktop app for macOS (Apple silicon and Intel) and Windows (x64), as `bureaucracy-forever-helper_<version>_<platform>` files, plus the signed update files and `latest.json` the app's updater reads.
- **Version rule:** Semantic Versioning from the PRs merged since the last published release: `fix` is a patch, `feat` a minor, `!` or a `BREAKING CHANGE:` footer a major, other types give no release. The first release is 1.0.0. Worked out by `release/plan.ts`.
- **Where the version is stamped:** at build time by `.github/workflows/release.yml`, passed to `tauri build --config`. It is never committed and never hand-edited; the version in `tauri.conf.json` and `package.json` is a placeholder.
- **Draft:** after each merge to `main` that calls for a release, the workflow builds the installers and creates or refreshes the one draft release (tag `v<version>`, targeting the built commit) on the repo's Releases page. Only Robert publishes it, after editing its notes if needed; no agent or workflow does. Publishing is also what makes the app offer the update to officers.
- **Release notes:** every PR body has a `Release note:` line, one plain-English sentence for officers, and a `Release:` line (the bump and the version it makes). `Release note: none` only for non-`feat`/`fix` types. A note starting with `Before you update:` (a breaking change, a restart or a manual step) goes under the "Before you update" heading in the notes with the prefix stripped; others go under "New" and "Fixed". `ship` and the reviewer check these.
- **Changelog:** the Releases page; there is no `CHANGELOG.md`. The README's Install section links to it.

## Deploy
The helper ships as GitHub Releases built by CI (#11, #16). Nothing is hosted and there are no servers, env vars or migrations.

## Needs Robert's own merge
Agents may merge reviewed PRs into a `package/*` branch or a Building branch below,
except these, which only Robert merges (docs/git-workflow.md, Merging). Robert also merges
every PR into a Live branch.
- Any change to `AGENTS.md` or `CLAUDE.md`. A `risk:high` PR is merged by an agent only
  after the `risky-review` role has passed it; in a package, the walkthrough marks steps
  Robert follows himself before he merges the package.
- Changes that reach beyond their branch: real data, live hosting or DNS settings, logins and
  secrets, outside accounts. On a Building branch, deploying this repo's own code from it
  is accepted.
- CI workflows, which build and publish the releases.
- The Tauri config and capabilities, which control what the app may do and will later hold
  the updater key setup.

Each branch's stage, read from this file on `main`. Building: its work reaches no one yet,
so agents merge into it. Live: people rely on it, so Robert merges into it. A branch not
listed counts as Live. One line per branch, `<branch>: Building` or `<branch>: Live`, with
an exact name (no patterns, no `package/` branches); lines starting with `#` are skipped.
A merge to `main` only refreshes the draft release Robert publishes, so `main` is Building.
```stages
main: Building
```

The guardrail refuses an agent's merge of a PR that changes a file matching a line
below, read from this file on `main`. Paths start at the repo root; `**` crosses
folders, `*` and `?` stay within one folder; end a folder with `/**`.
```paths
.github/workflows/**
src-tauri/tauri.conf.json
src-tauri/capabilities/**
```

## Watch out for
- Game facts must hold for WoW Forever, never for Classic or retail by default: character names, items, bosses, instances, levels, the add-on API and client limits. Check them against the Forever client, its beta results, or a Forever source (wago.tools data for a Forever build, Wowhead's Forever pages, foreverchanges.pro), and say so when a fact is unchecked for Forever. For example, a character name is a first and a last name separated by a space (the site's rule: 2–12 letters each), not Classic's single word.
- The window only appears under xvfb inside `dbus-run-session xvfb-run …`.
- Closing the window hides it; only the tray's Quit exits.
- The tray icon is the guild's column mark with a status dot (#73); the VM's light theme shows only the light-bar icon, so dark-bar, macOS and Windows rendering wait for a release build.

## Reference material
`ADDON-SPEC.md` in the site repo (robpaolella/bureaucracy-forever; tracked in its #271) is the contract for the data the add-on, helper and site exchange. Change the format there, not here.
