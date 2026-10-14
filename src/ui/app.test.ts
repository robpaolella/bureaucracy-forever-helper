// @vitest-environment happy-dom
import { beforeEach, describe, expect, it } from "vitest";
import { mountApp, type AppState } from "./app";
import { ROW_ORDER, firstBroken, type ChainRow, type ChainState } from "./chain";

const fine = (word = "Fine"): ChainRow => ({ tone: "ok", word, detail: "All good" });
const broken = (word: string, text: string): ChainRow => ({
  tone: "stop",
  word,
  detail: "Wrong",
  broken: true,
  message: { text, action: { label: "Fix it", variant: "secondary" } },
});
const chain = (over: Partial<ChainState> = {}): ChainState => ({
  site: fine(),
  folder: fine(),
  addon: fine(),
  reserves: fine(),
  awards: fine(),
  ...over,
});

const events = (n: number) =>
  Array.from({ length: n }, (_, i) => ({ at: Date.now() - i * 60_000, tone: "ok" as const, kind: `Event ${i}` }));
const state = (over: Partial<AppState> = {}): AppState => ({
  status: { tone: "ok", word: "Ready", title: "Ready for tonight" },
  chain: chain(),
  activity: [],
  sentTonight: 0,
  ...over,
});

describe("firstBroken", () => {
  it("is the first broken row, in the design's order", () => {
    const c = chain({ reserves: broken("Late", "r"), site: broken("Off", "s") });
    expect(firstBroken(c)).toBe("site");
  });

  it("is none when nothing is broken", () => {
    expect(firstBroken(chain())).toBeUndefined();
  });
});

describe("main screen", () => {
  let root: HTMLElement;
  beforeEach(() => {
    root = document.createElement("div");
  });

  it("lists the five rows in order", () => {
    mountApp(root, { getState: () => state() });
    const names = [...root.querySelectorAll(".chain .name")].map((n) => n.textContent);
    expect(names).toEqual(["Guild site", "Game folder", "Add-on", "Reserves", "Awards"]);
    expect(ROW_ORDER).toHaveLength(5);
  });

  it("opens only the first broken row, with its message and action", () => {
    const c = chain({ folder: broken("Gone", "Folder message"), awards: broken("Stuck", "Awards message") });
    mountApp(root, { getState: () => state({ chain: c }) });
    const open = root.querySelectorAll(".chain li.link-open");
    expect(open).toHaveLength(1);
    expect(open[0]?.textContent).toContain("Folder message");
    expect(open[0]?.querySelector("button")?.textContent).toBe("Fix it");
    expect(root.textContent).not.toContain("Awards message");
  });

  it("opens no row when nothing is broken", () => {
    mountApp(root, { getState: () => state() });
    expect(root.querySelector(".chain li.link-open")).toBeNull();
  });

  it("shows the last three events, and See all only above three", () => {
    mountApp(root, { getState: () => state({ activity: events(3) }) });
    expect(root.querySelector(".link-row")).toBeNull();
    mountApp(root, { getState: () => state({ activity: events(5) }) });
    expect(root.querySelectorAll(".acts li")).toHaveLength(3);
    expect(root.querySelector(".link-row")?.textContent).toBe("See all 5");
  });

  it("opens the full Activity screen from See all and returns", () => {
    mountApp(root, { getState: () => state({ activity: events(5), sentTonight: 2 }) });
    (root.querySelector(".link-row") as HTMLElement).click();
    expect(root.querySelector("h1")?.textContent).toBe("Activity");
    expect(root.querySelectorAll(".acts li")).toHaveLength(5);
    expect(root.textContent).toContain("2 sent tonight");
    (root.querySelector(".back") as HTMLElement).click();
    expect(root.querySelector("h1")?.textContent).toBe("Ready for tonight");
  });

  it("shows the staging band only when staging", () => {
    mountApp(root, { getState: () => state() });
    expect(root.querySelector(".staging-band")).toBeNull();
    mountApp(root, { getState: () => state({ staging: true }) });
    expect(root.querySelector(".staging-band")?.textContent).toBe("Staging site");
  });
});

describe("settings", () => {
  it("opens from the gear, shows the sections, and Done returns", () => {
    const root = document.createElement("div");
    const section = (t: string) => Object.assign(document.createElement("section"), { textContent: t });
    mountApp(root, { getState: () => state(), settingsSections: () => [section("A"), section("B")] });
    (root.querySelector('[aria-label="Settings"]') as HTMLElement).click();
    expect(root.querySelector("h1")?.textContent).toBe("Settings");
    expect(root.querySelectorAll("section")).toHaveLength(2);
    expect(root.querySelectorAll(".divider")).toHaveLength(1);
    (root.querySelector(".back") as HTMLElement).click();
    expect(root.querySelector("h1")?.textContent).toBe("Ready for tonight");
  });
});
