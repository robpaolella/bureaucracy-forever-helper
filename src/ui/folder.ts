// The game folder screens: step 2 of the setup chain and the Settings "Game folder" section.
// Markup follows the approved design (design/2-helper-window/index.html, folderBody).
import type { FolderOutcome, Install } from "../bridge";
import { button, card, choice, el, type Child } from "./components";

/** Where the folder step stands. The words of each error come from the engine. */
export interface FolderStep {
  phase: "searching" | "found" | "several" | "notFound" | "notGame" | "noAddon" | "gone";
  message?: string;
  /** The folder the message is about. */
  path?: string;
  /** Shown above the buttons when the last try failed, such as a settings file that couldn't be saved. */
  error?: string;
  installs?: Install[];
  /** For `noAddon`: the game versions the add-on can be installed into. */
  versionFolders?: string[];
  /** The chosen card when there are several. */
  selected?: number;
  /** Set while a folder is being saved or checked, so buttons can't be pressed twice. */
  busy?: boolean;
}

export interface FolderActions {
  /** "Use this folder" on the one that was found. */
  use(): void;
  /** "Continue" with the chosen game version. */
  continueWith(): void;
  choose(): void;
  /** "Install the add-on" into the game version that has none. */
  installAddon(): void;
  select(index: number): void;
}

/** What a folder answer puts on screen, or nothing when it has no screen of its own. */
export function stepFor(outcome: FolderOutcome, picked?: string): FolderStep | undefined {
  switch (outcome.kind) {
    case "found":
      return { phase: "found", path: outcome.versionFolder };
    case "several":
      return { phase: "several", installs: outcome.installs, selected: 0 };
    case "notFound":
      return { phase: "notFound", message: outcome.message };
    case "notGame":
      return { phase: "notGame", message: outcome.message, path: picked };
    case "noAddon":
      return { phase: "noAddon", message: outcome.message, path: outcome.path, versionFolders: outcome.versionFolders };
    case "gone":
      return { phase: outcome.kind, message: outcome.message, path: outcome.path };
    default:
      return undefined;
  }
}

/** The last two folders of a path, for the chain row: `…/World of Warcraft/_classic_`. */
export function shortPath(path: string): string {
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts.length > 2 ? `…/${parts.slice(-2).join("/")}` : path;
}

const mono = (path?: string): Child[] => (path ? [el("p", "mono", [path])] : []);

/** A message card with the folder it is about under it. */
const notice = (tone: "warn" | "stop", message: string | undefined, path?: string) =>
  card({ tone }, [el("p", "small fg", [message ?? ""]), ...mono(path)]);

function alertCard(node: HTMLElement): HTMLElement {
  node.setAttribute("role", "alert");
  return node;
}

function severalChoices(step: FolderStep, actions: FolderActions): HTMLElement {
  const legend = el("legend", "body", ["The add-on is installed in more than one game version. Which one do you play?"]);
  return el("fieldset", "stack folder-choices", [
    legend,
    ...(step.installs ?? []).map((install, i) =>
      choice({
        name: "ver",
        label: install.version,
        sub: install.versionFolder,
        checked: i === (step.selected ?? 0),
        onChange: () => actions.select(i),
      }),
    ),
  ]);
}

/** The body of the open Game folder row (or Settings section) for `step`. */
export function folderBody(step: FolderStep, actions: FolderActions): Node[] {
  const busy = Boolean(step.busy);
  const pick = (label: string, primary = true) =>
    button({ label, variant: primary ? "primary" : "ghost", small: !primary, disabled: busy, onClick: actions.choose });
  const failure = step.error ? [alertCard(card({ tone: "stop" }, [el("p", "small fg", [step.error])]))] : [];
  switch (step.phase) {
    case "searching": {
      const spin = el("span", "spin");
      spin.setAttribute("aria-hidden", "true");
      spin.style.color = "var(--teal)";
      const row = el("div", "row", [spin, el("span", "body", ["Looking for World of Warcraft…"])]);
      row.setAttribute("role", "status");
      return [row];
    }
    case "found":
      return [
        card({}, [
          el("div", "row", [
            el("span", "found-title", ["Found the game"]),
            el("span", "spacer"),
            el("span", "pill ok", ["Add-on installed"]),
          ]),
          ...mono(step.path),
        ]),
        ...failure,
        el("div", "row", [
          button({ label: "Use this folder", loading: busy, onClick: actions.use }),
          pick("Choose a different folder", false),
        ]),
      ];
    case "several":
      return [
        severalChoices(step, actions),
        ...failure,
        el("div", "row", [button({ label: "Continue", loading: busy, onClick: actions.continueWith })]),
      ];
    case "notFound":
      return [alertCard(el("p", "body", [step.message ?? ""])), ...failure, el("div", "row", [pick("Choose folder…")])];
    case "notGame":
      return [alertCard(notice("stop", step.message, step.path)), ...failure, el("div", "row", [pick("Choose folder…")])];
    case "noAddon":
      // With several game versions it isn't clear which one the officer plays, so only a single one is offered.
      return [
        alertCard(notice("warn", step.message, step.path)),
        ...failure,
        el("div", "row", [
          ...(step.versionFolders?.length === 1
            ? [button({ label: "Install the add-on", loading: busy, onClick: actions.installAddon })]
            : []),
          pick("Choose a different folder", false),
        ]),
      ];
    case "gone":
      return [alertCard(notice("warn", step.message, step.path)), ...failure, el("div", "row", [pick("Choose folder…")])];
  }
}

/** Settings → Game folder: the path, "Change folder", and whatever the last change needs. */
export function gameFolderSection(
  path: string | undefined,
  step: FolderStep | undefined,
  actions: FolderActions,
  busy = false,
): HTMLElement {
  const section = el("section", "stack", [
    el("h3", "th", ["Game folder"]),
    ...mono(path),
    ...(step ? folderBody(step, actions) : []),
    ...(step?.phase === "notFound" || step?.phase === "notGame" || step?.phase === "gone" ? [] : [
      el("div", "row", [button({ label: "Change folder", variant: "secondary", small: true, disabled: busy, onClick: actions.choose })]),
    ]),
  ]);
  section.id = "settings-game-folder";
  return section;
}
