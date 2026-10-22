// The helper window's screens: the main screen (status, chain list, recent activity),
// the full Activity list and the Settings frame. Markup and classes follow the approved
// design (design/2-helper-window/index.html); styles live in window.css.
import wordmark from "../assets/wordmark.png";
import { ACTIVITY_RECENT, formatTime, guildTime, showSeeAll, zoneName, type ActivityEvent } from "./activity";
import type { Raid, RaidChoice } from "../bridge";
import type { AddonUpdate } from "./addonUpdate";
import { ROW_NAMES, ROW_ORDER, firstBroken, type ChainRow, type ChainState, type RowId } from "./chain";
import { button, el, field, link, pill, type Child, type Tone } from "./components";
import type { KeyPanel } from "./keySettings";
import { folderBody, type FolderActions, type FolderStep } from "./folder";
import { updateCard, type HelperUpdate, type UpdateActions, type UpdateCheck } from "./update";

export interface AppState {
  /** The headline and its pill, such as "Everything’s working" and "Sending". */
  status: { tone: Tone; word: string; title: string };
  chain: ChainState;
  /** Today's events, newest first (see visibleActivity). */
  activity: ActivityEvent[];
  sentTonight: number;
  /** The tray's second line, finished ("Awards going to Molten Core"); #60 supplies it. */
  awardsLine?: string;
  /** The raid the latest handled award went to, for the Awards row's caption (#60). */
  awardsRaid?: { id: string; name: string };
  /** The add-on's newest saved reading of how the raid was chosen (#60); null before the game has saved one. */
  choice?: RaidChoice | null;
  /** The raids list (#12), to name the chosen raid. */
  raidList?: readonly Raid[];
  /** The staging site is in use: the band shows at the top of every screen. */
  staging?: boolean;
  /** A key is saved for the current site. */
  keyStored?: boolean;
  /** A key removal or site switch is running, or failed (Settings). */
  keyPanel?: KeyPanel;
  /** Set while the first-run setup is showing in place of the main screen. */
  setup?: { key: KeyStep };
  /** The game folder step: in the setup chain, or under Settings → Game folder. */
  folder?: FolderStep;
  /** The remembered game version folder, shown in Settings. */
  gamePath?: string;
  /** A folder is being checked or saved. */
  folderBusy?: boolean;
  /** The installed helper's version, such as "0.2.1". */
  helperVersion?: string;
  /** The add-on version in the game folder, such as "1.3.2". */
  addonVersion?: string;
  /** A newer add-on, or an install of one, shown in the Add-on row. */
  addon?: AddonUpdate;
  /** The helper starts when the computer starts, as the system has it. */
  startAtLogin?: boolean;
  /** A change to starting at login is under way, or the system refused it (shown under the switch). */
  login?: { busy: boolean; error?: string };
  /** A newer helper, shown in a card under the chain. */
  update?: HelperUpdate;
  /** A check for updates the officer asked for in Settings or the tray. */
  updateCheck?: UpdateCheck;
}

/** The key field and its button on the setup screen's first step. */
export interface KeyStep {
  value: string;
  checking: boolean;
  /** The plain-words error, shown under the field. */
  error?: string;
  /** The button's label when it isn't "Connect". */
  action?: string;
  /** A key is already saved, so an empty field with "Try again" checks it again. */
  stored?: boolean;
  /** The step was opened from Settings (or staging is on), so a "Back" link returns there and changes nothing. */
  back?: boolean;
  /** Replace key: the saved key stays and keeps the window connected until the new one is accepted. */
  replacing?: boolean;
}

export interface AppOptions {
  getState: () => AppState;
  /** The Settings sections, in order; each feature issue supplies its own. */
  settingsSections?: () => Node[];
  /** The screen to open on; the main screen by default. */
  view?: View;
  /** Called with the pasted key when the setup screen's button (or Enter) is pressed. */
  onConnect?: (value: string) => void;
  /** Called by the key step's "Back" link. */
  onBack?: () => void;
  /** Called by "Use the staging site" on the key step (staging off). */
  onUseStaging?: () => void;
  /** Called by "Open the officer area" on the key step. */
  onOpenOfficerArea?: () => void;
  /** What the buttons on the game folder step do. */
  folderActions?: FolderActions;
  /** What the update card's buttons and link do; without them no card is shown. */
  updateActions?: UpdateActions;
}

export type View = "main" | "settings" | "activity";

const GEAR =
  '<svg width="18" height="18" viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" aria-hidden="true"><path d="M3 5h12M3 13h12"/><circle cx="7" cy="5" r="1.8" fill="var(--ink-900)"/><circle cx="11" cy="13" r="1.8" fill="var(--ink-900)"/></svg>';
const CHEVRON =
  '<svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" aria-hidden="true"><path d="M9 2L4 7l5 5"/></svg>';

function svgButton(svg: string, className: string, label: string, onClick: () => void): HTMLButtonElement {
  const node = el("button", className);
  node.type = "button";
  node.innerHTML = svg;
  if (label) node.append(label);
  node.addEventListener("click", onClick);
  return node;
}

function header(right: Node): HTMLElement {
  const logo = el("img");
  logo.src = wordmark;
  logo.alt = "Bureaucracy";
  return el("header", "win-head", [el("span", "brand", [logo, el("span", "caption helper", ["Helper"])]), right]);
}

function gear(onClick: () => void): HTMLButtonElement {
  const node = svgButton(GEAR, "btn btn-ghost btn-icon", "", onClick);
  node.setAttribute("aria-label", "Settings");
  return node;
}

function back(label: string, onClick: () => void): HTMLButtonElement {
  return svgButton(CHEVRON, "btn btn-ghost btn-sm back", label, onClick);
}

function chainRow(id: RowId, row: ChainRow, open: boolean): HTMLElement {
  const line = el("div", "row line", [
    el("span", "name", [ROW_NAMES[id]]),
    el("span", "caption detail", [row.detail]),
    pill(row.tone, row.word),
  ]);
  const li = el("li", open ? `link-open tone-${row.tone}` : "", [line]);
  if (row.under?.length) li.append(el("div", "under", row.under));
  if (open && row.message) {
    const body: Child[] = [el("p", "small msg", [row.message.text])];
    const a = row.message.action;
    if (a) body.push(el("div", "row", [button({ label: a.label, variant: a.variant, small: true, onClick: a.onClick })]));
    li.append(el("div", "stack open-body", body));
  } else if (open && row.body) {
    li.append(el("div", "stack open-body", row.body));
  }
  return li;
}

function chainList(chain: ChainState): HTMLElement {
  const open = firstBroken(chain);
  return el(
    "ol",
    "chain",
    ROW_ORDER.map((id) => chainRow(id, chain[id], id === open)),
  );
}

function activityList(events: ActivityEvent[]): HTMLElement {
  return el(
    "ul",
    "acts",
    events.map((e) => {
      const time = el("time", "tabular", [formatTime(e.at)]);
      time.dateTime = new Date(e.at).toISOString();
      time.tabIndex = 0;
      time.title = `${guildTime(e.at)} guild time`;
      time.setAttribute("aria-label", `${formatTime(e.at)} ${zoneName(e.at)}, ${guildTime(e.at)} guild time`);
      const what = el("span", "what", [el("span", `k k-${e.tone}`, [`${e.kind}:`])]);
      if (e.strong) what.append(" ", el("b", undefined, [e.strong]));
      if (e.text) what.append(" ", e.text);
      if (e.meta) what.append(" ", el("span", "meta", [`· ${e.meta}`]));
      return el("li", undefined, [time, what]);
    }),
  );
}

/** The permanent band that says the staging site is in use. */
function stagingBand(): HTMLElement {
  const band = el("div", "staging-band", ["Staging site"]);
  band.setAttribute("role", "status");
  return band;
}

export function mountApp(root: HTMLElement, options: AppOptions): { refresh(): void; show(view: View): void } {
  let view: View = options.view ?? "main";
  const go = (next: View) => {
    view = next;
    render();
  };

  function mainScreen(s: AppState): Node[] {
    const recent = s.activity.slice(0, ACTIVITY_RECENT);
    const head = el("div", "sec-head", [
      el("h2", "th", ["Recent activity"]),
      el("span", "caption", [zoneName(Date.now())]),
      el("span", "spacer"),
    ]);
    if (showSeeAll(s.activity.length)) {
      const more = el("button", "link link-row small", [`See all ${s.activity.length}`]);
      more.type = "button";
      more.addEventListener("click", () => go("activity"));
      head.append(more);
    }
    return [
      header(gear(() => go("settings"))),
      el("main", "win-body", [
        el("div", "row status", [el("h1", "title-sm", [s.status.title]), pill(s.status.tone, s.status.word)]),
        chainList(s.chain),
        ...helperUpdateCard(s),
        el("section", "stack tight", [head, activityList(recent)]),
      ]),
    ];
  }

  /** The helper's update card, under the chain; setup shows it too, so an update can't wait on a key. */
  function helperUpdateCard(s: AppState): Node[] {
    return s.update && options.updateActions ? [updateCard(s.update, s.helperVersion, options.updateActions)] : [];
  }

  function connect(): void {
    options.onConnect?.(root.querySelector<HTMLInputElement>("#key")?.value ?? "");
  }

  /** The Guild site row's open body: the key field and its button. */
  function keyBody(s: AppState, key: KeyStep): Node[] {
    // A failed staging switch from this screen shows under the field, where the officer is looking.
    const error = key.error ?? (s.keyPanel?.source === "staging" ? s.keyPanel.error : undefined);
    const stagingLink =
      s.staging || key.replacing
        ? []
        : [link({ label: "Use the staging site", disabled: key.checking || Boolean(s.keyPanel?.busy), onClick: () => options.onUseStaging?.() })];
    const note = key.checking ? "Checking your key with the guild site…" : "Create a key in the officer area of the guild site, then paste it here.";
    const input = field({
      id: "key",
      label: "Helper key",
      value: key.value,
      placeholder: "Paste your key",
      mono: true,
      readOnly: key.checking,
      error,
      note,
    });
    input.querySelector("input")?.addEventListener("keydown", (e) => {
      if (e.key === "Enter" && !key.checking) connect();
    });
    const go = key.checking
      ? button({ label: "Checking…", loading: true })
      : button({ label: key.action ?? "Connect", onClick: connect });
    const officerArea = link({ label: "Open the officer area", onClick: () => options.onOpenOfficerArea?.() });
    return [input, el("div", "row", [go, el("span", "spacer"), officerArea]), ...stagingLink];
  }

  /** Step 2: opens with the folder screens once the key is in. */
  function folderRow(s: AppState, keyOpen: boolean): ChainRow {
    if (keyOpen) return { tone: "closed", word: "Step 2", detail: "Found after the key" };
    const searching = s.folder?.phase === "searching";
    const row: ChainRow = { tone: "closed", word: "Step 2", detail: searching ? "Looking for the game" : "Choose the game folder" };
    if (s.folder && options.folderActions) return { ...row, broken: true, body: folderBody(s.folder, options.folderActions) };
    return row;
  }

  /** The first-run chain, in the design's setup wording. */
  function setupChain(s: AppState, key: KeyStep): ChainState {
    const keyOpen = s.chain.site.tone !== "ok" || Boolean(key.replacing);
    const next = (detail: string): ChainRow => ({ tone: "closed", word: "Next", detail });
    return {
      site: keyOpen
        ? { tone: "closed", word: "Step 1", detail: "Paste your helper key", broken: true, body: keyBody(s, key) }
        : s.chain.site,
      folder: folderRow(s, keyOpen),
      addon: next("Checked with the folder"),
      reserves: next("Ready once you’re connected"),
      awards: next("Sent during the raid"),
    };
  }

  function setupScreen(s: AppState, key: KeyStep): Node[] {
    let left: Node = el("span");
    if (key.back || s.staging) {
      const goBack = back("Back", () => options.onBack?.());
      goBack.disabled = key.checking;
      left = goBack;
    }
    return [
      header(left),
      el("main", "win-body", [
        el("h1", "title-sm", ["Connect the helper"]),
        el("p", "small lede", ["Two steps, once. After that the helper runs by itself."]),
        chainList(setupChain(s, key)),
        ...helperUpdateCard(s),
      ]),
    ];
  }

  function activityScreen(s: AppState): Node[] {
    return [
      header(back("Status", () => go("main"))),
      el("main", "win-body tight", [
        el("div", "sec-head", [
          el("h1", "title-sm", ["Activity"]),
          el("span", "caption tabular", [`${s.sentTonight} sent tonight`]),
        ]),
        el("p", "caption", [`Today, newest first, ${zoneName(Date.now())}. The last 50 are kept.`]),
        activityList(s.activity),
      ]),
    ];
  }

  function settingsScreen(): Node[] {
    const body: Child[] = [el("h1", "title-sm", ["Settings"])];
    (options.settingsSections?.() ?? []).forEach((section, i) => {
      if (i > 0) body.push(el("hr", "divider"));
      body.push(section);
    });
    return [header(back("Done", () => go("main"))), el("main", "win-body roomy", body)];
  }

  /** Which screen is drawn now; a re-draw of the same one keeps its scroll position. */
  let drawn: string | undefined;
  /** Where Settings was scrolled to when the key step took its place, for when it comes back. */
  let settingsTop = 0;

  function render(): void {
    const s = options.getState();
    const setup = s.setup;
    const key = setup ? "setup" : view;
    const top = root.querySelector<HTMLElement>("main.win-body")?.scrollTop ?? 0;
    if (drawn === "settings" && key === "setup") settingsTop = top;
    const scrollTop = drawn === key ? top : drawn === "setup" && key === "settings" ? settingsTop : 0;
    if (key !== "setup") settingsTop = 0;
    const screen = setup
      ? setupScreen(s, setup.key)
      : view === "settings"
        ? settingsScreen()
        : view === "activity"
          ? activityScreen(s)
          : mainScreen(s);
    root.replaceChildren(...(s.staging ? [stagingBand()] : []), ...screen);
    drawn = key;
    const body = root.querySelector<HTMLElement>("main.win-body");
    if (body && scrollTop) body.scrollTop = scrollTop;
    if (setup && (s.chain.site.tone !== "ok" || setup.key.replacing) && !setup.key.checking) root.querySelector<HTMLInputElement>("#key")?.focus();
  }

  render();
  return { refresh: render, show: go };
}
