// The helper's own update: the card under the chain, its Activity line and the Settings
// versions line. Wording is the approved design's (`helper-update`, `helper-installing`,
// `helper-update-failed` and `settings`); realWindow.ts runs the checks and installs.
import type { Bridge, CheckOutcome, InstallOutcome, RestartOutcome, UpdateFailure, UpdateProgress } from "../bridge";
import type { Store } from "../state";
import { addEvent, type ActivityEvent } from "./activity";
import type { AppState } from "./app";
import { button, card, el, link, type Child } from "./components";

/** The update the card shows. Nothing installs until the officer presses "Install and restart". */
export type HelperUpdate =
  | { phase: "available"; version: string }
  /** `percent` is null until the download's size is known. */
  | { phase: "installing"; version: string; percent: number | null }
  | { phase: "failed"; version: string; reason: UpdateFailure };

/** The Settings versions line: a check the officer asked for, and how it ended. */
export interface UpdateCheck {
  checking: boolean;
  note?: string;
}

export interface UpdateActions {
  install(): void;
  later(): void;
  notes(): void;
}

/** Why the install failed, as the failed card says it. */
const FAILED_WORDS: Record<UpdateFailure, string> = {
  offline: "Couldn’t download the update. Check your connection.",
  missing: "The update isn’t available right now. Try again later.",
  badSignature: "The update file didn’t pass its safety check, so nothing was installed.",
  interrupted: "The download stopped before it finished. Nothing was installed.",
  checkFailed: "Couldn’t check for the update. Try again later.",
  installFailed: "The update couldn’t be put in place, so nothing was changed.",
};

/** The same, shorter, for the Activity line. */
const ACTIVITY_WORDS: Record<UpdateFailure, string> = {
  offline: "Couldn’t download the update.",
  missing: "The update isn’t available right now.",
  badSignature: "The file didn’t pass its safety check.",
  interrupted: "The download stopped before it finished.",
  checkFailed: "Couldn’t check for the update.",
  installFailed: "The update couldn’t be put in place.",
};

/** The failures named under "Other reasons it can fail", in this order. */
const OTHER_REASONS: [UpdateFailure, string][] = [
  ["offline", "No internet"],
  ["missing", "File missing"],
  ["badSignature", "Safety check"],
  ["interrupted", "Interrupted"],
];

const stillOn = (current: string | undefined, lead: string) => (current ? ` ${lead} ${current}.` : "");

/** The Activity line for a failed install. */
export function updateFailedEvent(reason: UpdateFailure, current: string | undefined, at: number): ActivityEvent {
  return { at, tone: "warn", kind: "Update not installed", text: `${ACTIVITY_WORDS[reason]}${stillOn(current, "Still on")}` };
}

/** What a check the officer asked for says under the versions line. */
export function checkNote(outcome: CheckOutcome): string {
  switch (outcome.kind) {
    case "available":
      return `Helper ${outcome.version} is ready to install. Press Done to see it.`;
    case "upToDate":
      return "You have the latest version.";
    case "failed":
      if (outcome.reason === "offline") return "Couldn’t check for updates. Check your connection.";
      if (outcome.reason === "missing") return FAILED_WORDS.missing;
      return "Couldn’t check for updates. Try again later.";
  }
}

const title = (text: string) => el("p", "card-title", [text]);

/** The card under the chain. `current` is the installed version, when known. */
export function updateCard(update: HelperUpdate, current: string | undefined, actions: UpdateActions): HTMLElement {
  let node: HTMLElement;
  if (update.phase === "available") {
    node = card({ tone: "info" }, [
      title(`Helper ${update.version} is ready to install`),
      el("p", "small", ["It installs in a few seconds and the helper restarts."]),
      el("div", "row", [
        button({ label: "Install and restart", small: true, onClick: actions.install }),
        button({ label: "Later", variant: "ghost", small: true, onClick: actions.later }),
        el("span", "spacer"),
        link({ label: "What’s new", onClick: actions.notes }),
      ]),
    ]);
  } else if (update.phase === "installing") {
    const bar = el("div", "track-bar", [el("span")]);
    bar.setAttribute("role", "progressbar");
    bar.setAttribute("aria-label", "Progress");
    bar.setAttribute("aria-valuemin", "0");
    bar.setAttribute("aria-valuemax", "100");
    const body: Child[] = [
      title(`Downloading ${update.version}…`),
      el("p", "small", ["The helper restarts when it’s done. Awards keep sending until then."]),
      bar,
    ];
    if (update.percent !== null) {
      bar.setAttribute("aria-valuenow", String(update.percent));
      (bar.firstChild as HTMLElement).style.width = `${update.percent}%`;
      body.push(el("p", "caption tabular", [`${update.percent}%`]));
    }
    node = card({ tone: "info" }, body);
  } else {
    const reasons = el(
      "ul",
      undefined,
      OTHER_REASONS.filter(([reason]) => reason !== update.reason).map(([reason, label]) =>
        el("li", undefined, [`${label}: “${FAILED_WORDS[reason]}”`]),
      ),
    );
    node = card({ tone: "warn" }, [
      title("The update couldn’t be installed"),
      el("p", "small", [`${FAILED_WORDS[update.reason]}${stillOn(current, "You’re still on")}`]),
      el("div", "row", [button({ label: "Try again", variant: "secondary", small: true, onClick: actions.install })]),
      el("details", "caption", [el("summary", undefined, ["Other reasons it can fail"]), reasons]),
    ]);
  }
  node.setAttribute("aria-live", "polite");
  return node;
}

/** Settings → Versions: the helper's version and "Check for updates". */
export function versionsSection(current: string | undefined, check: UpdateCheck | undefined, busy: boolean, onCheck: () => void): HTMLElement {
  const checking = Boolean(check?.checking);
  const body: Child[] = [
    el("h3", "th", ["Versions"]),
    el("div", "row", [
      el("span", "small tabular", [current ? `Helper ${current}` : "Helper"]),
      el("span", "spacer"),
      button({
        label: checking ? "Checking…" : "Check for updates",
        variant: "secondary",
        small: true,
        loading: checking,
        disabled: busy,
        onClick: onCheck,
      }),
    ]),
  ];
  if (check?.note && !checking) {
    const note = el("p", "caption", [check.note]);
    note.setAttribute("role", "status");
    body.push(note);
  }
  return el("section", "stack", body);
}

/** The notice a found update raises, once per version (design `notification`). */
export const updateNotice = (version: string) => ({
  headline: `Helper ${version} is ready to install`,
  detail: "Open the helper to install it.",
});

/** Where the version last announced is kept, so a restart doesn't announce it again. */
const NOTIFIED_KEY = "helper-update-notified";

function notifiedBefore(version: string): boolean {
  try {
    return localStorage.getItem(NOTIFIED_KEY) === version;
  } catch {
    return false;
  }
}

function rememberNotified(version: string): void {
  try {
    localStorage.setItem(NOTIFIED_KEY, version);
  } catch {
    // Without storage the notice may be raised again after a restart; nothing else changes.
  }
}

export interface HelperUpdates extends UpdateActions {
  /** Asks for a newer helper. `asked` is the officer's own check, which answers in Settings;
   * the helper's own checks stay quiet unless they find one. */
  check(asked: boolean): Promise<void>;
  /** Shows download progress while installing. */
  progress(progress: UpdateProgress): void;
}

/** The update flow over the window's state: checks, the card's buttons and their results. */
export function helperUpdates(store: Store<AppState>, bridge: Bridge): HelperUpdates {
  /** A version the officer put off with "Later": the helper's own checks don't show it again. */
  let putOff: string | undefined;
  const set = (change: (s: AppState) => Partial<AppState>) => store.set((s) => ({ ...s, ...change(s) }));
  const installing = () => store.get().update?.phase === "installing";

  function fail(version: string, reason: UpdateFailure): void {
    const at = Date.now();
    set((s) => ({
      update: { phase: "failed", version, reason },
      activity: addEvent(s.activity, updateFailedEvent(reason, s.helperVersion, at), at),
    }));
  }

  async function check(asked: boolean): Promise<void> {
    if (installing() || store.get().updateCheck?.checking) return;
    if (asked) set(() => ({ updateCheck: { checking: true } }));
    let outcome: CheckOutcome;
    try {
      outcome = await bridge.checkForUpdate();
    } catch {
      outcome = { kind: "failed", reason: "checkFailed", message: "" };
    }
    if (asked) set(() => ({ updateCheck: { checking: false, note: checkNote(outcome) } }));
    if (installing()) return;
    if (outcome.kind === "upToDate") {
      if (store.get().update?.phase === "available") set(() => ({ update: undefined }));
      return;
    }
    if (outcome.kind !== "available") return;
    const { version } = outcome;
    const shown = store.get().update;
    // The helper's own checks leave a failed card for the same version, and a put-off one, as they are.
    if (asked || (version !== putOff && (shown?.phase !== "failed" || shown.version !== version))) {
      set(() => ({ update: { phase: "available", version } }));
    }
    if (!notifiedBefore(version)) {
      rememberNotified(version);
      // A notice that can't be raised leaves the card to say it.
      bridge.notify(updateNotice(version)).catch(() => {});
    }
  }

  async function install(): Promise<void> {
    const shown = store.get().update;
    if (!shown || shown.phase === "installing") return;
    const { version } = shown;
    set(() => ({ update: { phase: "installing", version, percent: 0 } }));
    let outcome: InstallOutcome;
    try {
      outcome = await bridge.installUpdate();
    } catch {
      outcome = { kind: "failed", reason: "installFailed", message: "" };
    }
    if (outcome.kind === "busy") return;
    if (outcome.kind === "nothingFound") return fail(version, "missing");
    if (outcome.kind === "failed") return fail(version, outcome.reason);
    // A restart that happens never answers; one that answers didn't happen.
    let restart: RestartOutcome;
    try {
      restart = await bridge.restartAfterUpdate();
    } catch {
      restart = { kind: "failed", reason: "installFailed", message: "" };
    }
    fail(version, restart.kind === "failed" ? restart.reason : "installFailed");
  }

  return {
    check,
    install: () => void install(),
    later: () => {
      putOff = store.get().update?.version;
      set(() => ({ update: undefined }));
    },
    notes: () => void bridge.openUpdateNotes().catch(() => {}),
    progress: ({ downloaded, total }) => {
      const shown = store.get().update;
      if (shown?.phase !== "installing") return;
      const percent = total ? Math.min(100, Math.floor((downloaded * 100) / total)) : null;
      set(() => ({ update: { ...shown, percent } }));
    },
  };
}
