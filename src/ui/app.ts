// The helper window's screens: the main screen (status, chain list, recent activity),
// the full Activity list and the Settings frame. Markup and classes follow the approved
// design (design/2-helper-window/index.html); styles live in window.css.
import wordmark from "../assets/wordmark.png";
import { ACTIVITY_RECENT, formatTime, guildTime, showSeeAll, zoneName, type ActivityEvent } from "./activity";
import { ROW_NAMES, ROW_ORDER, firstBroken, type ChainRow, type ChainState, type RowId } from "./chain";
import { button, el, field, pill, type Child, type Tone } from "./components";

export interface AppState {
  /** The headline and its pill, such as "Everything’s working" and "Sending". */
  status: { tone: Tone; word: string; title: string };
  chain: ChainState;
  /** Today's events, newest first (see visibleActivity). */
  activity: ActivityEvent[];
  sentTonight: number;
  staging?: boolean;
  /** Set while the first-run setup is showing in place of the main screen. */
  setup?: { key: KeyStep };
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
}

export interface AppOptions {
  getState: () => AppState;
  /** The Settings sections, in order; each feature issue supplies its own. */
  settingsSections?: () => Node[];
  /** The screen to open on; the main screen by default. */
  view?: View;
  /** Called with the pasted key when the setup screen's button (or Enter) is pressed. */
  onConnect?: (value: string) => void;
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

export function mountApp(root: HTMLElement, options: AppOptions): { refresh(): void } {
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
    const pieces: Node[] = [];
    if (s.staging) {
      const band = el("div", "staging-band", ["Staging site"]);
      band.setAttribute("role", "status");
      pieces.push(band);
    }
    pieces.push(
      header(gear(() => go("settings"))),
      el("main", "win-body", [
        el("div", "row status", [el("h1", "title-sm", [s.status.title]), pill(s.status.tone, s.status.word)]),
        chainList(s.chain),
        el("section", "stack tight", [head, activityList(recent)]),
      ]),
    );
    return pieces;
  }

  function connect(): void {
    options.onConnect?.(root.querySelector<HTMLInputElement>("#key")?.value ?? "");
  }

  /** The Guild site row's open body: the key field and its button. */
  function keyBody(key: KeyStep): Node[] {
    const note = key.checking ? "Checking your key with the guild site…" : "Create a key in the officer area of the guild site, then paste it here.";
    const input = field({
      id: "key",
      label: "Helper key",
      value: key.value,
      placeholder: "Paste your key",
      mono: true,
      readOnly: key.checking,
      error: key.error,
      note,
    });
    input.querySelector("input")?.addEventListener("keydown", (e) => {
      if (e.key === "Enter" && !key.checking) connect();
    });
    const go = key.checking
      ? button({ label: "Checking…", loading: true })
      : button({ label: key.action ?? "Connect", onClick: connect });
    return [input, el("div", "row", [go])];
  }

  /** The first-run chain, in the design's setup wording. */
  function setupChain(s: AppState, key: KeyStep): ChainState {
    const keyOpen = s.chain.site.tone !== "ok";
    const next = (detail: string): ChainRow => ({ tone: "closed", word: "Next", detail });
    return {
      site: keyOpen
        ? { tone: "closed", word: "Step 1", detail: "Paste your helper key", broken: true, body: keyBody(key) }
        : s.chain.site,
      folder: { tone: "closed", word: "Step 2", detail: keyOpen ? "Found after the key" : "Choose the game folder" },
      addon: next("Checked with the folder"),
      reserves: next("Ready once you’re connected"),
      awards: next("Sent during the raid"),
    };
  }

  function setupScreen(s: AppState, key: KeyStep): Node[] {
    return [
      header(el("span")),
      el("main", "win-body", [
        el("h1", "title-sm", ["Connect the helper"]),
        el("p", "small lede", ["Two steps, once. After that the helper runs by itself."]),
        chainList(setupChain(s, key)),
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

  function render(): void {
    const s = options.getState();
    const setup = s.setup;
    const screen = setup
      ? setupScreen(s, setup.key)
      : view === "settings"
        ? settingsScreen()
        : view === "activity"
          ? activityScreen(s)
          : mainScreen(s);
    root.replaceChildren(...screen);
    if (setup && s.chain.site.tone !== "ok" && !setup.key.checking) root.querySelector<HTMLInputElement>("#key")?.focus();
  }

  render();
  return { refresh: render };
}
