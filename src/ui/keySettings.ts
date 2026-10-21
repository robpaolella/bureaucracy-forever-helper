// Settings → Guild site (the staging switch) and Settings → Helper key (Replace and Remove).
// Markup follows the approved design's `settings` state (design/2-helper-window/index.html).
import { button, el, pill, switchControl } from "./components";

/** What the key and staging controls are doing now; only set while one of them runs or failed. */
export interface KeyPanel {
  /** A removal or a site switch is running, so nothing else starts. */
  busy: boolean;
  /** Why the last removal or switch didn't happen, in plain words. */
  error?: string;
  /** Which control the error belongs under. */
  source?: "staging" | "key";
}

export interface KeyActions {
  /** Opens the key step to paste a new key; the old one stays until it is accepted. */
  replace(): void;
  /** Deletes the stored key, no questions asked. */
  remove(): void;
  /** Opens the key step when this site has no key yet. */
  add(): void;
  setStaging(on: boolean): void;
}

/** The secure store's name for the officer's computer: Windows calls it Credential Manager. */
export const storeName = (userAgent = navigator.userAgent): string =>
  /Windows/i.test(userAgent) ? "Credential Manager" : "Keychain";

const failure = (panel: KeyPanel | undefined, source: "staging" | "key") => {
  if (!panel?.error || panel.source !== source) return [];
  const note = el("p", "note err", [panel.error]);
  note.setAttribute("role", "alert");
  return [note];
};

export function stagingSection(staging: boolean, panel: KeyPanel | undefined, actions: KeyActions): HTMLElement {
  return el("section", "stack", [
    el("h3", "th", ["Guild site"]),
    el("div", undefined, [
      switchControl({
        id: "staging",
        label: "Use the staging site (for testing)",
        checked: staging,
        disabled: panel?.busy,
        onChange: actions.setStaging,
      }),
      el("p", "caption", ["The staging site has its own key and raids. Practice awards always go to staging."]),
    ]),
    ...failure(panel, "staging"),
  ]);
}

/**
 * The key saved for the current site. `working` says the guild site has accepted it; with no
 * key saved for this site yet (staging's first time) it offers to add one.
 */
export function keySection(
  saved: boolean,
  working: boolean,
  panel: KeyPanel | undefined,
  actions: KeyActions,
  where = storeName(),
): HTMLElement {
  const busy = Boolean(panel?.busy);
  if (!saved) {
    return el("section", "stack", [
      el("h3", "th", ["Helper key"]),
      el("p", "small", ["No key saved for this site yet."]),
      el("div", "row", [button({ label: "Add a key", variant: "secondary", small: true, disabled: busy, onClick: actions.add })]),
      ...failure(panel, "key"),
    ]);
  }
  return el("section", "stack", [
    el("h3", "th", ["Helper key"]),
    el("div", "row", [
      el("span", "small", [`Saved in ${where}`]),
      el("span", "spacer"),
      working ? pill("ok", "Working") : pill("closed", "Not checked"),
    ]),
    el("div", "row", [
      button({ label: "Replace key", variant: "secondary", small: true, disabled: busy, onClick: actions.replace }),
      button({ label: "Remove key", variant: "danger", small: true, disabled: busy, onClick: actions.remove }),
    ]),
    ...failure(panel, "key"),
  ]);
}

/**
 * The switch for starting the helper at login, and under it the system's refusal if there was
 * one. No hint line, as designed. It shares Settings' unlabelled section with the add-on switch.
 */
export function loginControls(on: boolean, error: string | undefined, busy: boolean, onChange: (on: boolean) => void): Node[] {
  const sw = switchControl({ id: "login", label: "Start the helper when the computer starts", checked: on, disabled: busy, onChange });
  if (!error) return [sw];
  const note = el("p", "note err", [error]);
  note.setAttribute("role", "alert");
  return [sw, note];
}
