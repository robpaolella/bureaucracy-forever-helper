// The real helper window: the screens from app.ts drawn from one state, with the Guild site
// row filled in by the key engine through the bridge. Later issues add their own rows and
// Settings sections by changing the state through the same store.
import type { Bridge, FolderOutcome, KeyOutcome } from "../bridge";
import { createStore, type Store } from "../state";
import { mountApp, type AppState, type KeyStep } from "./app";
import type { ChainRow } from "./chain";
import { gameFolderSection, shortPath, stepFor, type FolderActions, type FolderStep } from "./folder";

const waiting = (detail: string): ChainRow => ({ tone: "closed", word: "Not yet", detail });

/** The window before anything is known: nothing connected yet. */
export const initialState: AppState = {
  status: { tone: "closed", word: "Not set up", title: "Not connected yet" },
  chain: {
    site: { tone: "closed", word: "Not connected", detail: "No helper key yet" },
    folder: waiting("Not set up yet"),
    addon: waiting("Not set up yet"),
    reserves: waiting("Waiting for the guild site"),
    awards: waiting("Waiting for the guild site"),
  },
  activity: [],
  sentTonight: 0,
};

const connected = (officer: string): ChainRow => ({
  tone: "ok",
  word: "Connected",
  detail: `Connected as ${officer}`,
});

const emptyKey = { value: "", checking: false };

/** Matches the engine's shape check (src-tauri/src/key.rs), so a bad key never leaves the window. */
const KEY_SHAPE = /^bfh_[A-Za-z0-9_-]{43}$/;
const EMPTY_KEY_MESSAGE = "Paste your helper key.";
const MALFORMED_KEY_MESSAGE = "That doesn’t look like a helper key. Copy it again from the officer area.";
const FAILED_CHECK_MESSAGE = "The helper couldn’t check your key. Try again.";
const FAILED_FOLDER_MESSAGE = "The helper couldn’t check that folder. Try again.";

/** Why `key` can't be a helper key, or nothing if it can. */
export function keyShapeError(key: string): string | undefined {
  if (!key) return EMPTY_KEY_MESSAGE;
  return KEY_SHAPE.test(key) ? undefined : MALFORMED_KEY_MESSAGE;
}

/** The answers where trying again can help, so the button says "Try again". */
const RETRYABLE: ReadonlySet<KeyOutcome["kind"]> = new Set(["siteOff", "stagingNotOpen", "unreachable", "storeFailed"]);

/**
 * What a key check changes in the window. An accepted key connects the Guild site row; any
 * other answer puts the setup screen's key step in its state, with the engine's own words.
 * `pasted` is what was typed, kept in the field; `stored` says whether a key is already saved.
 */
export function applyKeyOutcome(state: AppState, outcome: KeyOutcome, pasted = "", stored = false): AppState {
  if (outcome.kind === "accepted") {
    return {
      ...state,
      status: { tone: "ok", word: "Connected", title: "Connected to the guild site" },
      chain: { ...state.chain, site: connected(outcome.officer) },
      setup: { key: emptyKey },
    };
  }
  const key: KeyStep =
    outcome.kind === "noKey"
      ? emptyKey
      : {
          value: pasted,
          checking: false,
          error: outcome.message,
          action: RETRYABLE.has(outcome.kind) ? "Try again" : undefined,
          stored,
        };
  return {
    ...state,
    status: initialState.status,
    chain: { ...state.chain, site: initialState.chain.site },
    setup: { key },
    folder: undefined,
  };
}

/** The folder step is done: the chain row is fine and setup is over. */
export function applyFolderDone(state: AppState, versionFolder: string): AppState {
  return {
    ...state,
    setup: undefined,
    folder: undefined,
    gamePath: versionFolder,
    chain: { ...state.chain, folder: { tone: "ok", word: "Found", detail: shortPath(versionFolder) } },
  };
}

/** Draws the window into `root` and checks the stored key. Resolves once that check is done. */
export async function startWindow(
  root: HTMLElement,
  bridge: Bridge,
  store: Store<AppState> = createStore(initialState),
): Promise<Store<AppState>> {
  /** Runs one key check and shows its result. `pasted` stays in the field if it fails. */
  async function check(run: () => Promise<KeyOutcome>, pasted: string, stored: boolean): Promise<void> {
    let outcome: KeyOutcome;
    try {
      outcome = await run();
    } catch {
      // The Rust side failed outright; say so in the field rather than leave it checking.
      outcome = { kind: "unreachable", message: FAILED_CHECK_MESSAGE };
    }
    store.set((s) => applyKeyOutcome(s, outcome, pasted, stored));
    if (outcome.kind === "accepted") await startFolder();
  }

  /** Runs a folder call; undefined if the Rust side failed outright. */
  async function ask(run: () => Promise<FolderOutcome>): Promise<FolderOutcome | undefined> {
    try {
      return await run();
    } catch {
      return undefined;
    }
  }

  const show = (step: FolderStep | undefined) => store.set((s) => ({ ...s, folder: step }));
  const patch = (change: Partial<FolderStep>) =>
    store.set((s) => (s.folder ? { ...s, folder: { ...s.folder, ...change } } : s));
  const failed = () =>
    store.set((s) => ({
      ...s,
      folder: s.folder
        ? { ...s.folder, busy: false, error: FAILED_FOLDER_MESSAGE }
        : { phase: "notFound", message: FAILED_FOLDER_MESSAGE },
    }));

  /** Saves the chosen version folder; on success the folder step is done. */
  async function confirm(versionFolder: string): Promise<void> {
    patch({ busy: true, error: undefined });
    const outcome = await ask(() => bridge.rememberVersion(versionFolder));
    if (!outcome) return failed();
    if (outcome.kind === "found") return store.set((s) => applyFolderDone(s, outcome.versionFolder));
    if (outcome.kind === "settingsFailed") return patch({ busy: false, error: outcome.message });
    show(stepFor(outcome));
  }

  /** Shows what a check found. In Settings a single match is saved straight away. */
  function settle(outcome: FolderOutcome | undefined, picked?: string): void {
    if (!outcome) return failed();
    if (outcome.kind === "found" && !store.get().setup) void confirm(outcome.versionFolder);
    else show(stepFor(outcome, picked));
  }

  /** Step 2: the remembered folder first, never replaced by a search; a search only if none. */
  async function startFolder(): Promise<void> {
    show({ phase: "searching" });
    const remembered = await ask(() => bridge.rememberedFolder());
    if (remembered?.kind === "found") return store.set((s) => applyFolderDone(s, remembered.versionFolder));
    if (remembered?.kind === "gone" || remembered?.kind === "noAddon") return show(stepFor(remembered));
    const searched = await ask(() => bridge.searchFolder());
    if (!searched) return show({ phase: "notFound", message: FAILED_FOLDER_MESSAGE });
    show(stepFor(searched));
  }

  async function choose(): Promise<void> {
    let picked: string | null;
    try {
      picked = await bridge.pickFolder();
    } catch {
      return failed();
    }
    if (picked === null) return;
    patch({ busy: true, error: undefined });
    settle(await ask(() => bridge.checkPickedFolder(picked)), picked);
  }

  const actions: FolderActions = {
    use: () => {
      const path = store.get().folder?.path;
      if (path) void confirm(path);
    },
    continueWith: () => {
      const { installs, selected } = store.get().folder ?? {};
      const install = installs?.[selected ?? 0];
      if (install) void confirm(install.versionFolder);
    },
    choose: () => void choose(),
    select: (selected) => patch({ selected }),
  };

  function connect(raw: string): void {
    const key = raw.trim();
    const current = store.get().setup?.key;
    const stored = current?.stored ?? false;
    const checking = (value: string) =>
      store.set((s) => ({ ...s, setup: { key: { value, checking: true, stored } } }));
    if (!key && stored) {
      // "Try again" on a saved key whose check failed: ask again, don't make them paste it.
      checking("");
      void check(() => bridge.checkStored(), "", true);
      return;
    }
    const error = keyShapeError(key);
    if (error) {
      store.set((s) => ({ ...s, setup: { key: { value: raw, checking: false, error, stored } } }));
      return;
    }
    checking(key);
    void check(() => bridge.connect(key), key, stored);
  }

  const app = mountApp(root, { getState: store.get, settingsSections: () => [gameFolderSection(store.get().gamePath, store.get().folder, actions)],
    onConnect: connect,
    folderActions: actions,
  });
  store.subscribe(app.refresh);
  try {
    const outcome = await bridge.checkStored();
    store.set((s) => applyKeyOutcome(s, outcome, "", outcome.kind !== "noKey"));
    if (outcome.kind === "accepted") await startFolder();
  } catch {
    // The Rust side failed outright: the window stays as it is, never blank.
  }
  return store;
}
