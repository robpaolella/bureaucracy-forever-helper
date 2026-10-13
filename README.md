# Bureaucracy Forever Helper

Desktop helper for the Bureaucracy Forever loot add-on. Built with Tauri 2 (Rust) and a plain TypeScript front-end (Vite, Vitest). It sits in the tray or menu bar; closing the window hides it, and the tray's Quit item exits.

## Develop

Needs Node, Rust, and on Linux the Tauri libraries (see the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/)).

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
