// The real helper window: the screens from app.ts drawn from one state, with the Guild site
// row filled in by the key engine through the bridge. Later issues add their own rows and
// Settings sections by changing the state through the same store.
import type { Bridge, KeyOutcome } from "../bridge";
import { createStore, type Store } from "../state";
import { mountApp, type AppState, type KeyStep } from "./app";
import type { ChainRow } from "./chain";

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
  }

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

  const app = mountApp(root, { getState: store.get, settingsSections: () => [], onConnect: connect });
  store.subscribe(app.refresh);
  try {
    const outcome = await bridge.checkStored();
    store.set((s) => applyKeyOutcome(s, outcome, "", outcome.kind !== "noKey"));
  } catch {
    // The Rust side failed outright: the window stays as it is, never blank.
  }
  return store;
}
