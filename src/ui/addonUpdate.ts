// The add-on's updates: the Add-on row's states, the "Update the add-on automatically" switch,
// the Activity lines and the two notices. Wording is the approved design's (`addon-update`,
// `addon-installing`, `addon-updated`, `addon-rolled-back`, `settings` and `notification`);
// realWindow.ts hands in the game folder and calls `check`.
import type { AddonInstall, AddonProgress, Bridge, Notice } from "../bridge";
import type { Store } from "../state";
import { addEvent, type ActivityEvent } from "./activity";
import type { AppState } from "./app";
import type { ChainRow } from "./chain";
import { button, el, link, switchControl } from "./components";

/** The add-on release the Add-on row shows. Nothing installs by itself unless the switch is on. */
export type AddonUpdate =
  /** A release is waiting for the officer's "Install". `note` is its "Before you update" section. */
  | { phase: "available"; version: string; note: string | null }
  /** `percent` is null until the download's size is known. */
  | { phase: "installing"; version: string; percent: number | null }
  | { phase: "updated"; version: string; note: string | null }
  /** `kept` is the version still in place. */
  | { phase: "failed"; version: string; kept: string | null };

export interface AddonActions {
  install(): void;
  later(): void;
  notes(): void;
}

/** The switch's saved value, kept between runs. On unless the officer turned it off. */
const AUTO_KEY = "addon-auto-update";

export function autoUpdateOn(): boolean {
  try {
    return localStorage.getItem(AUTO_KEY) !== "off";
  } catch {
    return true;
  }
}

function saveAutoUpdate(on: boolean): void {
  try {
    localStorage.setItem(AUTO_KEY, on ? "on" : "off");
  } catch {
    // Without storage the switch holds until the helper restarts.
  }
}

/** Settings: the switch, built like the other switches. */
export function addonSwitchSection(): HTMLElement {
  return el("section", "stack", [
    switchControl({
      id: "auto-addon",
      label: "Update the add-on automatically",
      checked: autoUpdateOn(),
      onChange: saveAutoUpdate,
    }),
  ]);
}

/** The version whose automatic install failed, so no later check tries it again by itself. */
const FAILED_KEY = "addon-auto-failed";

const failedBefore = (version: string): boolean => {
  try {
    return localStorage.getItem(FAILED_KEY) === version;
  } catch {
    return false;
  }
};

function rememberFailed(version: string): void {
  try {
    localStorage.setItem(FAILED_KEY, version);
  } catch {
    // The version is still skipped until the helper restarts.
  }
}

const RELOAD_LINE = "After it installs, type /reload in game, or start the game if it’s closed.";

const note = (text: string) => el("p", "caption", [text]);

/**
 * The Add-on row for an update's state; `installed` is the version in the game folder. Without
 * either, `row` is left as it was.
 */
export function addonRow(row: ChainRow, update: AddonUpdate | undefined, installed: string | undefined, actions: AddonActions): ChainRow {
  if (!update) return installed ? { tone: "ok", word: "Installed", detail: `Version ${installed}` } : row;
  switch (update.phase) {
    case "available": {
      const buttons = el("div", "row", [
        button({ label: "Install", small: true, onClick: actions.install }),
        button({ label: "Later", variant: "ghost", small: true, onClick: actions.later }),
        el("span", "spacer"),
        link({ label: "What’s new", onClick: actions.notes }),
      ]);
      const body = [el("p", "small", [RELOAD_LINE]), ...(update.note ? [note(update.note)] : []), buttons];
      return { tone: "warn", word: "Update ready", detail: `Version ${update.version} is ready`, broken: true, body };
    }
    case "installing": {
      const bar = el("div", "track-bar", [el("span")]);
      bar.setAttribute("role", "progressbar");
      bar.setAttribute("aria-label", `Installing add-on ${update.version}…`);
      bar.setAttribute("aria-valuemin", "0");
      bar.setAttribute("aria-valuemax", "100");
      const body: Node[] = [el("p", "small", ["Your reserves file is kept."]), bar];
      if (update.percent !== null) {
        bar.setAttribute("aria-valuenow", String(update.percent));
        (bar.firstChild as HTMLElement).style.width = `${update.percent}%`;
        body.push(el("p", "caption tabular", [`${update.percent}%`]));
      }
      return { tone: "closed", word: "Installing", detail: `Installing ${update.version}`, broken: true, body };
    }
    case "updated":
      return {
        tone: "ok",
        word: "Updated",
        detail: `Version ${update.version}`,
        under: update.note ? [note(update.note)] : undefined,
      };
    case "failed": {
      const text = update.kept
        ? `Add-on ${update.kept} is still in place, with your reserves file. Nothing was lost.`
        : "The add-on couldn’t be installed. Nothing was changed.";
      const retry = el("div", "row", [button({ label: "Try again", variant: "secondary", small: true, onClick: actions.install })]);
      return {
        tone: "warn",
        word: "Not updated",
        detail: update.kept ? `Still on ${update.kept}` : "Not installed",
        broken: true,
        body: [el("p", "small", [text]), retry],
      };
    }
  }
}

/** The Activity line for an install that finished. */
export const addonUpdatedEvent = (version: string, at: number): ActivityEvent => ({
  at,
  tone: "ok",
  kind: "Add-on updated",
  text: `to ${version}.`,
});

/** The Activity line for an install that didn't. */
export const addonFailedEvent = (kept: string | null, at: number): ActivityEvent => ({
  at,
  tone: "warn",
  kind: "Add-on update not installed",
  text: kept ? `${kept} is still in place.` : "Nothing was changed.",
});

/** The notices an automatic install raises, once each: never for one the officer pressed. */
export const addonUpdatedNotice = (version: string): Notice => ({
  headline: `Add-on updated to ${version}`,
  detail: "Loads on your next /reload or game start.",
});

export const addonFailedNotice = (kept: string | null): Notice => ({
  headline: "The add-on update didn’t install",
  detail: kept ? `Add-on ${kept} is still in place.` : "Nothing was changed.",
});

export interface AddonUpdates extends AddonActions {
  /** Looks at the add-on in the remembered game folder and, with the switch on, installs a
   * newer release. Does nothing while an install runs. */
  check(): Promise<void>;
  /** Shows install progress while installing. */
  progress(progress: AddonProgress): void;
}

/** The add-on update flow over the window's state: checks, installs and what they show. */
export function addonUpdates(store: Store<AppState>, bridge: Bridge, rowFor: (s: AppState) => ChainRow): AddonUpdates {
  /** A version put off with "Later": checks don't show it again. */
  let putOff: string | undefined;
  /** Versions installed by themselves this run, so none is tried twice. */
  const tried = new Set<string>();
  const set = (change: (s: AppState) => Partial<AppState>) =>
    store.set((s) => {
      const next = { ...s, ...change(s) };
      return { ...next, chain: { ...next.chain, addon: rowFor(next) } };
    });
  const phase = () => store.get().addon?.phase;
  const version = () => store.get().addon?.version;

  /** What an install did. `auto` installs also raise a notice. */
  function finish(outcome: AddonInstall, wanted: string, note: string | null, auto: boolean): void {
    const at = Date.now();
    const notify = (notice: Notice) => bridge.notify(notice).catch(() => {});
    if (outcome.kind === "busy") return;
    if (outcome.kind === "installed") {
      set((s) => ({
        addon: { phase: "updated", version: outcome.version, note },
        addonVersion: outcome.version,
        activity: addEvent(s.activity, addonUpdatedEvent(outcome.version, at), at),
      }));
      if (auto) notify(addonUpdatedNotice(outcome.version));
      return;
    }
    const kept = outcome.kind === "notInstalled" ? outcome.kept : (store.get().addonVersion ?? null);
    if (auto) rememberFailed(wanted);
    set((s) => ({
      addon: { phase: "failed", version: wanted, kept },
      addonVersion: kept ?? undefined,
      activity: addEvent(s.activity, addonFailedEvent(kept, at), at),
    }));
    if (auto) notify(addonFailedNotice(kept));
  }

  async function install(wanted: string, note: string | null, auto: boolean): Promise<void> {
    const folder = store.get().gamePath;
    if (!folder || phase() === "installing") return;
    set(() => ({ addon: { phase: "installing", version: wanted, percent: 0 } }));
    let outcome: AddonInstall;
    try {
      outcome = await bridge.installAddon(folder);
    } catch {
      outcome = { kind: "notInstalled", reason: "installFailed", message: "", kept: store.get().addonVersion ?? null };
    }
    finish(outcome, wanted, note, auto);
  }

  async function check(): Promise<void> {
    const folder = store.get().gamePath;
    if (!folder || phase() === "installing") return;
    let outcome;
    try {
      outcome = await bridge.checkAddon(folder);
    } catch {
      return;
    }
    if (phase() === "installing") return;
    // A check that fails stays quiet: the row keeps its last look, and the next check tries again.
    if (outcome.kind === "failed") {
      if (outcome.installed) set(() => ({ addonVersion: outcome.installed ?? undefined }));
      return;
    }
    if (outcome.installed === null) return;
    set(() => ({ addonVersion: outcome.installed ?? undefined }));
    const shown = store.get().addon;
    if (outcome.upToDate) {
      if (shown?.phase === "available") set(() => ({ addon: undefined }));
      return;
    }
    const { latest, note } = outcome;
    if (autoUpdateOn() && !tried.has(latest) && !failedBefore(latest)) {
      tried.add(latest);
      return install(latest, note, true);
    }
    // A failed card for the same version, and a put-off one, stay as they are.
    if (latest === putOff || (shown?.phase === "failed" && shown.version === latest)) return;
    set(() => ({ addon: { phase: "available", version: latest, note } }));
  }

  return {
    check,
    install: () => {
      const shown = store.get().addon;
      if (shown?.phase !== "available" && shown?.phase !== "failed") return;
      // A press always installs what the last check found, which is the version shown.
      void install(shown.version, shown.phase === "available" ? shown.note : null, false);
    },
    later: () => {
      putOff = version();
      set(() => ({ addon: undefined }));
    },
    notes: () => {
      const shown = version();
      if (shown) void bridge.openAddonNotes(shown).catch(() => {});
    },
    progress: (progress) => {
      const shown = store.get().addon;
      if (shown?.phase !== "installing") return;
      const percent =
        progress.step === "installing" ? 100 : progress.total ? Math.min(100, Math.floor((progress.downloaded * 100) / progress.total)) : null;
      set(() => ({ addon: { ...shown, percent } }));
    },
  };
}
