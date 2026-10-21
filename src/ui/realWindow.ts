// The real helper window: the screens from app.ts drawn from one state, with the Guild site
// row filled in by the key engine through the bridge. Later issues add their own rows and
// Settings sections by changing the state through the same store.
import type { Bridge, ChatEvent, FolderOutcome, KeyOutcome, RaidsView, ReservesView, SyncStatus } from "../bridge";
import { createStore, type Store } from "../state";
import { mountApp, type AppState, type KeyStep } from "./app";
import { addonRow, addonSwitchSection, addonUpdates } from "./addonUpdate";
import { addEvent, chatEvent, nameList, raidStart, reservesEvent } from "./activity";
import type { ChainRow } from "./chain";
import { el } from "./components";
import { syncNews, withSync, type SyncActions, type Told } from "./sync";
import { trayStatusOf } from "./tray";
import { gameFolderSection, shortPath, stepFor, type FolderActions, type FolderStep } from "./folder";
import { helperUpdates, versionsSection } from "./update";

/** How often the helper looks for a newer version of itself while it runs. */
export const UPDATE_CHECK_EVERY = 6 * 60 * 60 * 1000;

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

const IN_GAME_WORD = "In the game";
const RELOAD_WORD = "/reload needed";

/**
 * The Reserves row for the raids list and what was written for it. The pill stays "Not yet"
 * until reserves are written into the game, then reads "In the game", and turns amber when a
 * change needs a /reload. Raids show in every state that still has a list (a refused key or a
 * site switched off keeps the last one); without one the row is as before the list was known.
 */
export function reservesRow(view: RaidsView, reserves?: ReservesView): ChainRow {
  if (view.raids.length > 0) {
    const state = reserves?.state;
    const row: ChainRow =
      state?.kind === "inGame"
        ? { tone: "ok", word: IN_GAME_WORD, detail: "Next 12 hours" }
        : state?.kind === "reload"
          ? {
              tone: "warn",
              word: RELOAD_WORD,
              detail: `${nameList(state.changed)} changed`,
              broken: true,
              message: { text: `Reserves for ${nameList(state.changed)} changed. Type /reload in game to load them.` },
            }
          : waiting("Next 12 hours");
    return {
      ...row,
      under: view.raids.map((raid) => {
        const when = raidStart(raid.startsAt);
        const time = el("time", "caption tabular", [when.local.replace(/ (?=\S+$)/, "\u00a0")]);
        time.dateTime = new Date(raid.startsAt * 1000).toISOString();
        time.tabIndex = 0;
        time.title = when.guild;
        time.setAttribute("aria-label", when.label);
        return el("div", "row", [el("span", "caption", [raid.name]), el("span", "spacer"), time]);
      }),
    };
  }
  if (view.state.kind === "empty" || view.state.kind === "fresh") {
    return { tone: "closed", word: NO_RAIDS_WORD, detail: "None in the next 12 hours" };
  }
  return initialState.chain.reserves;
}

const NO_RAIDS_WORD = "No raids soon";

/**
 * The connected headline, from the Reserves row: "Type /reload in game" while a reload is
 * pending, "Ready for tonight" once reserves are in the game, and the design's "No raids
 * coming up" while no raid is near.
 */
const connectedStatus = (chain: AppState["chain"]): AppState["status"] => {
  switch (chain.reserves.word) {
    case RELOAD_WORD:
      return { tone: "warn", word: "Needs you", title: "Type /reload in game" };
    case IN_GAME_WORD:
      return { tone: "ok", word: "Ready", title: "Ready for tonight" };
    case NO_RAIDS_WORD:
      return { tone: "ok", word: "Connected", title: "No raids coming up" };
    default:
      return { tone: "ok", word: "Connected", title: "Connected to the guild site" };
  }
};

/** The pill words of the connected headline, which follow the Reserves row. */
const HEADLINE_WORDS: ReadonlySet<string> = new Set(["Connected", "Ready", "Needs you"]);

/**
 * Shows `view` and what was written for it in the Reserves row, and in the headline when the
 * helper is connected.
 */
export function applyRaids(state: AppState, view: RaidsView, reserves?: ReservesView): AppState {
  const chain = { ...state.chain, reserves: reservesRow(view, reserves) };
  return { ...state, chain, status: HEADLINE_WORDS.has(state.status.word) ? connectedStatus(chain) : state.status };
}

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
const FAILED_INSTALL_MESSAGE = "The helper couldn’t install the add-on. Try again.";

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
      status: connectedStatus(state.chain),
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

const sameDay = (a: number, b: number) => new Date(a).toDateString() === new Date(b).toDateString();

/**
 * Shows a line the chat log follower sent or ignored: an Activity line, and for a sent award
 * or replacement the Awards row, the raid it went to and the day's count. An undo shows
 * nothing.
 */
export function applyChatEvent(state: AppState, event: ChatEvent, at: number): AppState {
  const line = chatEvent(event, at);
  if (!line) return state;
  const next = { ...state, activity: addEvent(state.activity, line, at) };
  if (event.kind !== "sent") return next;
  return {
    ...next,
    sentTonight: state.sentTonight + 1,
    awardsRaid: { id: event.raid, name: event.raidName },
    awardsLine: `Awards going to ${event.raidName}`,
    chain: {
      ...state.chain,
      awards: {
        tone: "ok",
        word: "Sending",
        detail: `Going to ${event.raidName}`,
        under: [el("p", "caption", ["Picked in game. Change it on the add-on’s Sync tab."])],
      },
    },
  };
}

/** Draws the window into `root` and checks the stored key. Resolves once that check is done. */
export async function startWindow(
  root: HTMLElement,
  bridge: Bridge,
  store: Store<AppState> = createStore(initialState),
): Promise<Store<AppState>> {
  const addon = addonUpdates(store, bridge, (s) => addonRow(s.chain.addon, s.addon, s.addonVersion, addon));

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
    if (outcome.kind === "accepted") {
      // A new key ends a "key refused" status; ask for the status that follows it.
      bridge.syncStatus().then((status) => hearSync(status), () => {});
      await startFolder();
    }
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
  /** Marks a save or check as running, so nothing overlaps it and its result can't arrive out of order. */
  const working = (on: boolean) =>
    store.set((s) => ({ ...s, folderBusy: on, folder: s.folder ? { ...s.folder, busy: on } : s.folder }));
  const patch = (change: Partial<FolderStep>) =>
    store.set((s) => (s.folder ? { ...s, folder: { ...s.folder, ...change } } : s));
  /** Says a try failed, on the open step; with none open (Settings), on the folder that was to be saved. */
  const failed = (error: string, path?: string) =>
    store.set((s) => ({
      ...s,
      folder: s.folder ? { ...s.folder, error } : { phase: "found", path, error },
    }));

  /** Saves the chosen version folder; on success the folder step is done. */
  async function confirm(versionFolder: string): Promise<void> {
    if (store.get().folderBusy) return;
    working(true);
    patch({ error: undefined });
    try {
      const outcome = await ask(() => bridge.rememberVersion(versionFolder));
      if (!outcome) return failed(FAILED_FOLDER_MESSAGE, versionFolder);
      if (outcome.kind === "found") {
        store.set((s) => applyFolderDone(s, outcome.versionFolder));
        return void addon.check();
      }
      if (outcome.kind === "settingsFailed") return failed(outcome.message, versionFolder);
      show(stepFor(outcome));
    } finally {
      working(false);
    }
  }

  /** Shows what a check found. In Settings a single match is saved straight away. */
  async function settle(outcome: FolderOutcome | undefined, picked?: string): Promise<void> {
    if (!outcome) return failed(FAILED_FOLDER_MESSAGE);
    if (outcome.kind === "found" && !store.get().setup) return confirm(outcome.versionFolder);
    show(stepFor(outcome, picked));
  }

  /** Step 2: the remembered folder first, never replaced by a search; a search only if none. */
  async function startFolder(): Promise<void> {
    show({ phase: "searching" });
    const remembered = await ask(() => bridge.rememberedFolder());
    if (remembered?.kind === "found") {
      store.set((s) => applyFolderDone(s, remembered.versionFolder));
      return void addon.check();
    }
    if (remembered?.kind === "gone" || remembered?.kind === "noAddon") return show(stepFor(remembered));
    const searched = await ask(() => bridge.searchFolder());
    if (!searched) return show({ phase: "notFound", message: FAILED_FOLDER_MESSAGE });
    show(stepFor(searched));
  }

  /** "Install the add-on" on the folder step: installs into the one game version, then moves on. */
  async function installAddon(): Promise<void> {
    const { folder, folderBusy } = store.get();
    const target = folder?.versionFolders?.length === 1 ? folder.versionFolders[0] : undefined;
    if (!target || folderBusy) return;
    working(true);
    patch({ error: undefined });
    let installed = false;
    try {
      let message = FAILED_INSTALL_MESSAGE;
      try {
        const checked = await bridge.checkAddon(target);
        if (checked.kind === "failed") message = checked.message;
        else {
          const result = await bridge.installAddon(target);
          if (result.kind === "installed") installed = true;
          else if (result.kind === "notInstalled") message = result.message;
        }
      } catch {
        // The Rust side failed outright; the step says so and offers the button again.
      }
      if (!installed) return failed(message);
    } finally {
      working(false);
    }
    await confirm(target);
  }

  async function choose(): Promise<void> {
    if (store.get().folderBusy) return;
    let picked: string | null;
    try {
      picked = await bridge.pickFolder();
    } catch {
      return failed(FAILED_FOLDER_MESSAGE);
    }
    if (picked === null) return;
    working(true);
    // A new pick starts over: an earlier failed save must not be retried by "Use this folder".
    if (!store.get().setup) show(undefined);
    else patch({ error: undefined });
    let outcome: FolderOutcome | undefined;
    try {
      outcome = await ask(() => bridge.checkPickedFolder(picked));
    } finally {
      working(false);
    }
    await settle(outcome, picked);
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
    installAddon: () => void installAddon(),
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

  // The sync status sits on top of the state: the window and the tray show `shown()`.
  let sync: SyncStatus | undefined;
  const itemNames = new Map<number, string>();
  const syncActions: SyncActions = {
    // "Add a new key" opens the key step; a new key replaces the refused one.
    addKey: () => store.set((s) => applyKeyOutcome(s, { kind: "noKey" })),
    // The loot log on the page of the raid awards last went to, else the site's calendar.
    openLootLog: () => void bridge.openLootLog(store.get().awardsRaid?.id).catch(() => {}),
  };
  const shown = () => withSync(store.get(), sync, syncActions, itemNames);

  const updates = helperUpdates(store, bridge);
  /** The officer's own check, from Settings or the tray: its answer shows in Settings. */
  const checkAsked = () => {
    app.show("settings");
    void updates.check(true);
  };
  const app = mountApp(root, {
    getState: shown,
    settingsSections: () => {
      const s = store.get();
      return [
        gameFolderSection(s.gamePath, s.folder, actions, s.folderBusy),
        addonSwitchSection(),
        versionsSection(s.helperVersion, s.updateCheck, s.update?.phase === "installing", checkAsked, s.addonVersion),
      ];
    },
    onConnect: connect,
    folderActions: actions,
    updateActions: updates,
  });
  store.subscribe(app.refresh);
  // The tray shows the same status as the window; it is told only when that changes.
  let told = "";
  const tellTray = () => {
    const status = trayStatusOf(shown());
    const next = JSON.stringify(status);
    if (next === told) return;
    told = next;
    // A tray that can't be told keeps its last look and is told again at the next change.
    bridge.setTrayStatus(status).catch(() => {
      if (told === next) told = "";
    });
  };
  store.subscribe(tellTray);
  tellTray();
  // The raids list and what was written for it: the first answer, then every change. An
  // event that lands first is newer.
  let raidsView: RaidsView = { state: { kind: "loading" }, raids: [] };
  let reservesView: ReservesView | undefined;
  let announced = 0;
  let shownProblem: string | null = null;
  const redraw = () => store.set((s) => applyRaids(s, raidsView, reservesView));
  /** Each counted write adds one Activity line and, when a reload is needed, one notice. */
  function announce(view: ReservesView): void {
    if (view.problem && view.problem !== shownProblem) {
      const event = { at: Date.now(), tone: "warn" as const, kind: "Reserves not saved", text: view.problem };
      store.set((s) => ({ ...s, activity: addEvent(s.activity, event, event.at) }));
    }
    shownProblem = view.problem;
    if (view.changes <= announced) return;
    announced = view.changes;
    const event = reservesEvent(view, Date.now());
    if (event) store.set((s) => ({ ...s, activity: addEvent(s.activity, event, event.at) }));
    if (view.state.kind === "reload") {
      // A notice that can't be raised leaves the amber row, status and Activity line to say it.
      bridge
        .notify({ headline: "Type /reload in game", detail: `Reserves for ${nameList(view.state.changed)} are ready.` })
        .catch(() => {});
    }
  }
  const hear = (view: ReservesView) => {
    reservesView = view;
    redraw();
    announce(view);
  };
  // Lines the follower sent or ignored. Listening starts first, so none is missed while the
  // raids and reserves load. The count starts again each day, also with no new line.
  let countDay = Date.now();
  let lastAt = 0;
  const newDay = () => {
    const now = Date.now();
    if (sameDay(countDay, now)) return;
    countDay = now;
    store.set((s) => ({ ...s, sentTonight: 0 }));
  };
  setInterval(newDay, 60_000);
  try {
    await bridge.onChatLog((event) => {
      newDay();
      // Lines heard in the same millisecond still list newest first.
      lastAt = Math.max(Date.now(), lastAt + 1);
      const at = lastAt;
      if (event.kind === "sent" && event.item !== null && event.itemName) itemNames.set(event.item, event.itemName);
      store.set((s) => applyChatEvent(s, event, at));
    });
  } catch {
    // The chat log is unavailable; the Awards row and Activity keep their last look.
  }
  let sawEvent = false;
  try {
    await bridge.onRaids((view) => {
      sawEvent = true;
      raidsView = view;
      redraw();
    });
    const first = await bridge.raids();
    if (!sawEvent) {
      raidsView = first;
      redraw();
    }
  } catch {
    // The raids list is unavailable; the Reserves row keeps its last look.
  }
  let sawReserves = false;
  try {
    await bridge.onReserves((view) => {
      sawReserves = true;
      hear(view);
    });
    const first = await bridge.reserves();
    if (!sawReserves) hear(first);
  } catch {
    // Reserves are unavailable; the Reserves row keeps its last look.
  }
  // The helper's own version and update: progress and the tray's check, a check now, then
  // one every few hours. Each part that fails leaves the rest of the window as it is.
  bridge.helperVersion().then((helperVersion) => store.set((s) => ({ ...s, helperVersion })), () => {});
  bridge.onUpdateProgress(updates.progress).catch(() => {});
  bridge.onTrayCheck(checkAsked).catch(() => {});
  bridge.onAddonProgress(addon.progress).catch(() => {});
  setInterval(() => void addon.check(), UPDATE_CHECK_EVERY);
  void updates.check(false);
  setInterval(() => void updates.check(false), UPDATE_CHECK_EVERY);
  // The upload's status: each problem shows once in Activity and, where the officer must act,
  // as one system notice. A saved key that can't be checked at startup shows as offline.
  const heard: Told = { refused: new Set() };
  let startupOffline = false;
  const hearSync = (status: SyncStatus) => {
    sync = status;
    const news = syncNews(heard, status, itemNames, Date.now());
    store.set((s) => ({ ...s, activity: news.events.reduce((all, e) => addEvent(all, e, e.at), s.activity) }));
    // A notice that can't be raised leaves the headline, rows and Activity line to say it.
    if (news.notice) bridge.notify(news.notice).catch(() => {});
    if (startupOffline && status.kind !== "offline") {
      startupOffline = false;
      void verifyStored();
    }
  };
  async function verifyStored(): Promise<void> {
    try {
      const outcome = await bridge.checkStored();
      if (outcome.kind === "unreachable") {
        // A connected helper that can't reach the site yet: offline, not the key step.
        startupOffline = true;
        if (sync?.kind !== "offline") hearSync({ kind: "offline", waiting: 0 });
        return await startFolder();
      }
      store.set((s) => applyKeyOutcome(s, outcome, "", outcome.kind !== "noKey"));
      if (outcome.kind === "accepted") await startFolder();
    } catch {
      // The Rust side failed outright: the window stays as it is, never blank.
    }
  }
  let sawSync = false;
  try {
    await bridge.onSyncStatus((status) => {
      sawSync = true;
      hearSync(status);
    });
    const first = await bridge.syncStatus();
    if (!sawSync) hearSync(first);
  } catch {
    // The sync status is unavailable; the window shows no sync problem.
  }
  await verifyStored();
  return store;
}
