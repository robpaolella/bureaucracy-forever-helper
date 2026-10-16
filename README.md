# Bureaucracy Forever Helper

Desktop helper for the Bureaucracy Forever loot add-on. Built with Tauri 2 (Rust) and a plain TypeScript front-end (Vite, Vitest). It sits in the tray or menu bar; closing the window hides it, and the tray's Quit item exits.

## Install

The helper runs on your Mac or Windows PC and sits in the menu bar (Mac) or system tray (Windows). Install it once; after that it keeps itself up to date when you say so.

1. Open the [Releases page](https://github.com/robpaolella/bureaucracy-forever-helper/releases) and find the newest release at the top. Under **Assets**, download the file for your computer (the number in the name is the version):
   - **Mac with Apple silicon** (M1 or newer): `bureaucracy-forever-helper_<version>_macos-apple-silicon.dmg`
   - **Mac with an Intel chip**: `bureaucracy-forever-helper_<version>_macos-intel.dmg`. Not sure which Mac you have? Choose Apple menu → About This Mac: "Chip" says Apple M1 (or later) or Intel.
   - **Windows PC**: `bureaucracy-forever-helper_<version>_windows-x64-setup.exe`. Ignore the other files in the list; they are for the helper's own updates.
2. Install it and approve it the first time (below). The helper isn't signed with a paid developer certificate yet, so your computer warns you the first time. That's expected, and you only do this once.

### Mac: opening it the first time

1. Open the `.dmg` and drag **Bureaucracy Forever Helper** into **Applications**.
2. Open it from Applications. macOS says it can't verify the app. Click **Done** (or **OK**).
3. Open **System Settings → Privacy & Security** and scroll down to the Security section. You'll see a note that "Bureaucracy Forever Helper" was blocked. Click **Open Anyway**.
4. Enter your Mac password or use Touch ID, then click **Open Anyway** once more.

From then on it opens normally.

### Windows: the SmartScreen warning

1. Double-click the downloaded `-setup.exe`. A blue window says **Windows protected your PC**.
2. Click **More info**, then **Run anyway**.
3. Follow the installer. The helper starts when it finishes.

### Updates

When a new version is ready, the helper tells you with a notice and a card on its main screen: "Helper <version> is ready to install". Press **Install and restart** to update; it takes a few seconds. **Later** puts it off, and it never installs on its own. It checks every few hours while it's running.

To see which version you have, open the helper's **Settings** and look under **Versions** ("Helper 1.2.0"). **Check for updates** there (or in the tray menu) looks right away.

**What's new**: each version's changes are on the [Releases page](https://github.com/robpaolella/bureaucracy-forever-helper/releases). The card's **What's new** link opens it.

## Develop

Needs Node, Rust, `lua5.1` (optional locally; CI requires it for the inbox test), and on Linux the Tauri libraries (see the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/)).

```
npm install
npm run tauri dev
npm run tauri build -- --debug
```

## Checks

```
npm run lint && npm run typecheck && npm test
cd src-tauri && cargo fmt --check && cargo clippy -- -D warnings && cargo test
```
