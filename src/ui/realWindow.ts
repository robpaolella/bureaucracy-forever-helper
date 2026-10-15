// The real helper window: the screens from app.ts drawn from one state, with the Guild site
// row filled in by the key engine through the bridge. Later issues add their own rows and
// Settings sections by changing the state through the same store.
import type { Bridge, KeyOutcome } from "../bridge";
import { createStore, type Store } from "../state";
import { mountApp, type AppState } from "./app";
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

/**
 * What a key check changes in the window. Only an accepted key connects the Guild site row;
 * any other answer leaves it as at the start, until the setup screen (#56) handles it.
 */
export function applyKeyOutcome(state: AppState, outcome: KeyOutcome): AppState {
  if (outcome.kind !== "accepted") {
    return { ...state, status: initialState.status, chain: { ...state.chain, site: initialState.chain.site } };
  }
  return {
    ...state,
    status: { tone: "ok", word: "Connected", title: "Connected to the guild site" },
    chain: { ...state.chain, site: connected(outcome.officer) },
  };
}

/** Draws the window into `root` and checks the stored key. Resolves once that check is done. */
export async function startWindow(
  root: HTMLElement,
  bridge: Bridge,
  store: Store<AppState> = createStore(initialState),
): Promise<Store<AppState>> {
  const app = mountApp(root, { getState: store.get, settingsSections: () => [] });
  store.subscribe(app.refresh);
  try {
    const outcome = await bridge.checkStored();
    store.set((s) => applyKeyOutcome(s, outcome));
  } catch {
    // The Rust side failed outright: the window stays as it is, never blank.
  }
  return store;
}
