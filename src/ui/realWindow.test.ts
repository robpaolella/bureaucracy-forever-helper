// @vitest-environment happy-dom
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { Bridge, ChatEvent, KeyOutcome, Raid, RaidsView, ReservesView, SyncStatus } from "../bridge";
import { createStore } from "../state";
import { mountApp } from "./app";
import { trayStatusOf } from "./tray";
import { applyRaids, initialState, reservesRow, startWindow } from "./realWindow";

const accepted: KeyOutcome = {
  kind: "accepted",
  officer: "Aldric Vane",
  env: "production",
  formatVersions: [1],
  addonVersion: "0.1.0",
  addonUrl: "https://example.test/addon",
};

/** A bridge whose check gives `check`; the other calls are never expected. */
function fakeBridge(check: () => Promise<KeyOutcome>, connect?: (key: string) => Promise<KeyOutcome>): Bridge {
  const unused = () => Promise.reject(new Error("not used"));
  return {
    connect: vi.fn(connect ?? unused),
    checkStored: vi.fn(check),
    removeKey: vi.fn(unused),
    staging: unused,
    setStaging: unused,
    syncStatus: unused,
    clearRefused: unused,
    searchFolder: unused,
    checkPickedFolder: unused,
    rememberVersion: unused,
    rememberedFolder: unused,
    pickFolder: unused,
    onSyncStatus: unused,
    raids: unused,
    onRaids: unused,
    raidChoice: unused,
    onRaidChoice: unused,
    reserves: unused,
    onReserves: unused,
    onChatLog: unused,
    checkForUpdate: unused,
    installUpdate: unused,
    restartAfterUpdate: unused,
    onUpdateProgress: unused,
    openUpdateNotes: unused,
    onTrayCheck: unused,
    helperVersion: unused,
    checkAddon: unused,
    installAddon: unused,
    onAddonProgress: unused,
    openAddonNotes: unused,
    openLootLog: unused,
    openOfficerArea: unused,
    setTrayStatus: () => Promise.resolve(),
    notify: unused,
  };
}

const KEY = `bfh_${"3kZ9q2Lw8VtR4mXc7Pn1".padEnd(43, "x")}`;
const noKey = () => Promise.resolve<KeyOutcome>({ kind: "noKey" });
const flush = () => new Promise((r) => setTimeout(r, 0));

/** Opens the key step, pastes `pasted` and presses the button. */
async function paste(bridge: Bridge, pasted: string) {
  const root = document.createElement("div");
  document.body.append(root);
  const store = await startWindow(root, bridge);
  root.querySelector<HTMLInputElement>("#key")!.value = pasted;
  root.querySelector<HTMLButtonElement>(".open-body button")!.click();
  return { root, store };
}
const note = (root: HTMLElement) => root.querySelector("#key-note");
const button = (root: HTMLElement) => root.querySelector(".open-body button")!;

const siteRow = (root: HTMLElement) => root.querySelector(".chain li")?.textContent ?? "";

describe("the real window", () => {
  it("shows the chain, not the placeholder, and checks the stored key on start", async () => {
    const root = document.createElement("div");
    const bridge = fakeBridge(() => Promise.resolve({ kind: "noKey" }));
    await startWindow(root, bridge);
    expect(bridge.checkStored).toHaveBeenCalledTimes(1);
    expect(root.querySelectorAll(".chain li")).toHaveLength(5);
    expect(root.textContent).not.toContain("Bureaucracy Forever Helper");
    expect(siteRow(root)).toContain("Paste your helper key");
  });

  it("draws the chain before the check answers", () => {
    const root = document.createElement("div");
    void startWindow(root, fakeBridge(() => new Promise(() => {})));
    expect(root.querySelectorAll(".chain li")).toHaveLength(5);
  });

  it("reads 'Connected as <Discord name>' for a good stored key", async () => {
    const root = document.createElement("div");
    await startWindow(root, fakeBridge(() => Promise.resolve(accepted)));
    expect(siteRow(root)).toContain("Connected as Aldric Vane");
  });

  it("opens the key step, with focus in the field, when no key is stored", async () => {
    const root = document.createElement("div");
    document.body.append(root);
    await startWindow(root, fakeBridge(() => Promise.resolve({ kind: "noKey" })));
    expect(root.querySelector("h1")?.textContent).toBe("Connect the helper");
    expect(root.textContent).toContain("Two steps, once. After that the helper runs by itself.");
    expect(root.querySelectorAll(".chain li")).toHaveLength(5);
    const rows = [...root.querySelectorAll(".chain li")].map((li) => li.textContent);
    expect(rows[0]).toContain("Step 1");
    expect(rows[0]).toContain("Paste your helper key");
    expect(rows[1]).toContain("Found after the keyStep 2");
    expect(rows[2]).toContain("Checked with the folderNext");
    expect(rows[3]).toContain("Ready once you’re connectedNext");
    expect(rows[4]).toContain("Sent during the raidNext");
    expect(document.activeElement).toBe(root.querySelector("#key"));
    expect(root.querySelector(".note")?.textContent).toBe(
      "Create a key in the officer area of the guild site, then paste it here.",
    );
    root.remove();
  });

  it("draws the key step too when the stored key fails to check", async () => {
    const root = document.createElement("div");
    await startWindow(root, fakeBridge(() => Promise.resolve({ kind: "refused", reason: "badKey", message: "gone" })));
    expect(root.querySelector("#key-note")?.textContent).toBe("gone");
  });

  it("still opens the window when the Rust side fails outright", async () => {
    const root = document.createElement("div");
    await startWindow(root, fakeBridge(() => Promise.reject(new Error("boom"))));
    expect(root.querySelectorAll(".chain li")).toHaveLength(5);
  });
  it("redraws when the state changes", () => {
    const root = document.createElement("div");
    const store = createStore(initialState);
    const app = mountApp(root, { getState: store.get });
    store.subscribe(app.refresh);
    expect(siteRow(root)).toContain("No helper key yet");
    store.set((s) => ({ ...s, chain: { ...s.chain, site: { tone: "ok", word: "Connected", detail: "Connected as Thessaly" } } }));
    expect(siteRow(root)).toContain("Connected as Thessaly");
  });
});

describe("the key step", () => {
  it.each([
    ["", "Paste your helper key."],
    ["   ", "Paste your helper key."],
    ["my key from the site", "That doesn’t look like a helper key. Copy it again from the officer area."],
    [KEY + "A", "That doesn’t look like a helper key. Copy it again from the officer area."],
  ])("never calls the bridge for %j", async (pasted, message) => {
    const bridge = fakeBridge(noKey);
    const { root } = await paste(bridge, pasted);
    expect(bridge.connect).not.toHaveBeenCalled();
    expect(note(root)?.textContent).toBe(message);
    expect(note(root)?.getAttribute("role")).toBe("alert");
    const input = root.querySelector("#key")!;
    expect(input.getAttribute("aria-invalid")).toBe("true");
    expect(input.getAttribute("aria-describedby")).toBe("key-note");
    expect(button(root).textContent).toBe("Connect");
    root.remove();
  });

  it("shows the checking state, then connects with the name the engine returned", async () => {
    let answer!: (o: KeyOutcome) => void;
    const bridge = fakeBridge(noKey, () => new Promise((r) => (answer = r)));
    const { root } = await paste(bridge, `  ${KEY}\n`);
    expect(bridge.connect).toHaveBeenCalledWith(KEY);
    expect(button(root).textContent).toBe("Checking…");
    expect((button(root) as HTMLButtonElement).disabled).toBe(true);
    expect(button(root).getAttribute("aria-busy")).toBe("true");
    expect(note(root)?.textContent).toBe("Checking your key with the guild site…");
    expect(root.querySelector<HTMLInputElement>("#key")!.readOnly).toBe(true);
    answer(accepted);
    await flush();
    const rows = [...root.querySelectorAll(".chain li")].map((li) => li.textContent);
    expect(rows[0]).toContain("Connected as Aldric Vane");
    expect(rows[1]).toContain("Choose the game folderStep 2");
    expect(root.querySelector("#key")).toBeNull();
    root.remove();
  });

  it("submits on Enter", async () => {
    const bridge = fakeBridge(noKey, () => Promise.resolve(accepted));
    const root = document.createElement("div");
    await startWindow(root, bridge);
    const input = root.querySelector<HTMLInputElement>("#key")!;
    input.value = KEY;
    input.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter" }));
    expect(bridge.connect).toHaveBeenCalledTimes(1);
  });

  it.each<[string, KeyOutcome, string, string]>([
    [
      "revoked",
      { kind: "refused", reason: "badKey", message: "This key has been revoked or no longer works. Create a new key in the officer area." },
      "This key has been revoked or no longer works. Create a new key in the officer area.",
      "Connect",
    ],
    [
      "no longer an officer",
      { kind: "refused", reason: "notOfficer", message: "no officer" },
      "no officer",
      "Connect",
    ],
    [
      "site off",
      { kind: "siteOff", message: "The guild site hasn’t switched the helper on yet. Try again later." },
      "The guild site hasn’t switched the helper on yet. Try again later.",
      "Try again",
    ],
    [
      "unreachable",
      { kind: "unreachable", message: "Couldn’t reach the guild site. Check your internet connection and try again." },
      "Couldn’t reach the guild site. Check your internet connection and try again.",
      "Try again",
    ],
    [
      "store failed",
      { kind: "storeFailed", message: "The helper couldn’t save your key in Keychain. Allow access when your Mac asks, then try again." },
      "The helper couldn’t save your key in Keychain. Allow access when your Mac asks, then try again.",
      "Try again",
    ],
    [
      "staging not open",
      { kind: "stagingNotOpen", message: "The staging site isn't open yet." },
      "The staging site isn't open yet.",
      "Try again",
    ],
  ])("shows %s as the design words it, tied to the field and announced", async (_name, outcome, text, label) => {
    const { root } = await paste(fakeBridge(noKey, () => Promise.resolve(outcome)), KEY);
    await flush();
    expect(note(root)?.textContent).toBe(text);
    expect(note(root)?.className).toBe("note err");
    expect(note(root)?.getAttribute("role")).toBe("alert");
    expect(root.querySelector("#key")?.getAttribute("aria-describedby")).toBe("key-note");
    expect(root.querySelector("#key")?.getAttribute("aria-invalid")).toBe("true");
    expect(root.querySelector<HTMLInputElement>("#key")!.value).toBe(KEY);
    expect(button(root).textContent).toBe(label);
    expect(root.querySelector(".chain li")?.textContent).toContain("Step 1");
    expect(document.activeElement).toBe(root.querySelector("#key"));
    root.remove();
  });

  it("shows the engine's own malformed answer in the same way", async () => {
    const message = "That doesn’t look like a helper key. Copy it again from the officer area.";
    const outcome: KeyOutcome = { kind: "malformed", reason: "malformed", message };
    const { root } = await paste(fakeBridge(noKey, () => Promise.resolve(outcome)), KEY);
    await flush();
    expect(note(root)?.textContent).toBe(message);
    expect(note(root)?.getAttribute("role")).toBe("alert");
    expect(root.querySelector("#key")?.getAttribute("aria-describedby")).toBe("key-note");
    expect(button(root).textContent).toBe("Connect");
    root.remove();
  });

  it("keeps a saved key when staging isn't open, and checks it again on 'Try again'", async () => {
    const answers: KeyOutcome[] = [{ kind: "stagingNotOpen", message: "The staging site isn't open yet." }, accepted];
    const bridge = fakeBridge(() => Promise.resolve(answers.shift()!));
    const root = document.createElement("div");
    await startWindow(root, bridge);
    expect(note(root)?.textContent).toBe("The staging site isn't open yet.");
    expect(note(root)?.className).toBe("note err");
    expect(button(root).textContent).toBe("Try again");
    button(root).dispatchEvent(new MouseEvent("click"));
    await flush();
    expect(bridge.removeKey).not.toHaveBeenCalled();
    expect(bridge.connect).not.toHaveBeenCalled();
    expect(root.querySelector(".chain li")?.textContent).toContain("Connected as Aldric Vane");
  });

  it("shows a failure of the Rust side in the field, not endless checking", async () => {
    const { root } = await paste(fakeBridge(noKey, () => Promise.reject(new Error("boom"))), KEY);
    await flush();
    expect(note(root)?.textContent).toBe("The helper couldn’t check your key. Try again.");
    expect(button(root).textContent).toBe("Try again");
    root.remove();
  });

  it("shows a connected helper's unreachable site as offline, not the key step, and checks again once sending resumes", async () => {
    const answers: KeyOutcome[] = [{ kind: "unreachable", message: "offline" }, accepted];
    let push: (s: SyncStatus) => void = () => {};
    const bridge = {
      ...fakeBridge(() => Promise.resolve(answers.shift()!)),
      syncStatus: () => Promise.resolve<SyncStatus>({ kind: "idle", otherSite: 0 }),
      onSyncStatus: (l: (s: SyncStatus) => void) => ((push = l), Promise.resolve(() => {})),
      rememberedFolder: () =>
        Promise.resolve({ kind: "found", gameFolder: "/g", version: "_retail_", versionFolder: "/g/_retail_", addonFolder: "" }),
    } as Bridge;
    const root = document.createElement("div");
    await startWindow(root, bridge);
    expect(root.querySelector("#key")).toBeNull();
    expect(siteRow(root)).toContain("Can’t reach the guild site");
    expect(root.querySelector(".status")?.textContent).toContain("Offline");
    push({ kind: "idle", otherSite: 0 });
    await flush();
    expect(bridge.checkStored).toHaveBeenCalledTimes(2);
    expect(siteRow(root)).toContain("Connected as Aldric Vane");
  });
});

describe("the bridge", () => {
  it("is the only file that calls Tauri", () => {
    const files = (dir: string): string[] =>
      readdirSync(dir, { withFileTypes: true }).flatMap((e) =>
        e.isDirectory() ? files(join(dir, e.name)) : /\.ts$/.test(e.name) ? [join(dir, e.name)] : [],
      );
    const src = join(import.meta.dirname, "..");
    const callers = files(src)
      .filter((f) => !f.endsWith(".test.ts") && readFileSync(f, "utf8").includes("@tauri-apps"))
      .map((f) => f.slice(src.length + 1));
    expect(callers).toEqual(["bridge.ts"]);
  });
});

describe("the tray", () => {
  it("is told the status at start and again only when it changes", async () => {
    const bridge = fakeBridge(noKey);
    const told = vi.fn(() => Promise.resolve());
    bridge.setTrayStatus = told;
    const store = await startWindow(document.createElement("div"), bridge);
    expect(told).toHaveBeenCalledTimes(1);
    expect(told).toHaveBeenLastCalledWith(expect.objectContaining({ tone: "closed", word: "Not set up" }));
    store.set((s) => ({ ...s, activity: [] }));
    expect(told).toHaveBeenCalledTimes(1);
    store.set((s) => ({ ...s, sentTonight: 3, awardsLine: "Not a guild run" }));
    expect(told).toHaveBeenCalledTimes(2);
    expect(told).toHaveBeenLastCalledWith(expect.objectContaining({ sentTonight: 3, secondLine: "Not a guild run" }));
  });

  it("leaves the window working when the tray can't be told, and tells it again at the next change", async () => {
    const bridge = fakeBridge(noKey);
    const told = vi.fn(() => Promise.reject(new Error("no tray")));
    bridge.setTrayStatus = told;
    const store = await startWindow(document.createElement("div"), bridge);
    await flush();
    const before = told.mock.calls.length;
    store.set((s) => ({ ...s, activity: [] }));
    expect(told).toHaveBeenCalledTimes(before + 1);
  });
});

describe("the Reserves row", () => {
  // Tue 20 Oct 2026, 9:00 PM in Chicago (CDT) and in Los Angeles (PDT) it is 7:00 PM.
  const raid = (name: string, startsAt: number): Raid => ({
    id: name, code: "mc", name, startsAt, durationMin: 180, instance: null, hasLootTable: true,
  });
  const TUE_9PM_CDT = Date.UTC(2026, 9, 21, 2, 0) / 1000;
  const view = (kind: RaidsView["state"]["kind"], raids: Raid[] = []): RaidsView =>
    ({ state: { kind }, raids }) as RaidsView;
  const rowOf = (v: RaidsView) => {
    const root = document.createElement("div");
    const store = createStore(applyRaids(initialState, v));
    mountApp(root, { getState: store.get });
    return root.querySelectorAll(".chain li")[3] as HTMLElement;
  };

  it("shows the waiting pill, 'Next 12 hours' and each raid's time, never 'In the game'", () => {
    const row = rowOf(view("fresh", [raid("Molten Core", TUE_9PM_CDT), raid("Blackwing Lair", TUE_9PM_CDT + 9000)]));
    expect(row.querySelector(".pill")?.textContent).toBe("Not yet");
    expect(row.querySelector(".pill")?.className).toBe("pill closed");
    expect(row.querySelector(".detail")?.textContent).toBe("Next 12 hours");
    expect(row.querySelectorAll(".under .row")).toHaveLength(2);
    expect(row.querySelector(".under .row span")?.textContent).toBe("Molten Core");
    expect(row.textContent).not.toContain("In the game");
    expect(row.textContent).not.toContain("/reload");
  });

  it("shows the start in the officer's zone, with the guild time on hover and in the name", () => {
    const time = rowOf(view("fresh", [raid("Molten Core", TUE_9PM_CDT)])).querySelector("time")!;
    expect(time.textContent).toMatch(/^\w{3} \d{1,2}:\d{2}\s[AP]M\u00a0\S+$/);
    expect(time.title).toBe("7:00 PM guild time");
    expect(time.getAttribute("aria-label")).toBe(`${time.textContent!.replace("\u00a0", " ")}, 7:00 PM guild time`);
    expect(time.tabIndex).toBe(0);
  });

  it("says 'No raids soon' for an empty list, and the headline follows", () => {
    const state = applyRaids({ ...initialState, status: { tone: "ok", word: "Connected", title: "Connected to the guild site" } }, view("empty"));
    expect(state.chain.reserves).toMatchObject({ tone: "closed", word: "No raids soon", detail: "None in the next 12 hours" });
    expect(state.status.title).toBe("No raids coming up");
    const back = applyRaids(state, view("fresh", [raid("Molten Core", TUE_9PM_CDT)]));
    expect(back.status.title).toBe("Connected to the guild site");
  });

  it.each(["unreachable", "keyRefused", "siteOff", "stagingNotOpen"] as const)("keeps the last list when %s", (kind) => {
    const row = reservesRow(view(kind, [raid("Molten Core", TUE_9PM_CDT)]));
    expect(row.detail).toBe("Next 12 hours");
    expect(row.under).toHaveLength(1);
  });

  it.each(["loading", "noKey", "unreachable", "keyRefused"] as const)("stays waiting with no list when %s", (kind) => {
    expect(reservesRow(view(kind))).toEqual(initialState.chain.reserves);
  });

  it("is fed from the bridge at start and on every later event", async () => {
    let push: (v: RaidsView) => void = () => {};
    const bridge = {
      ...fakeBridge(noKey),
      raids: () => Promise.resolve(view("fresh", [raid("Molten Core", TUE_9PM_CDT)])),
      onRaids: (l: (v: RaidsView) => void) => {
        push = l;
        return Promise.resolve(() => {});
      },
    } as Bridge;
    const root = document.createElement("div");
    const store = await startWindow(root, bridge);
    expect(store.get().chain.reserves.detail).toBe("Next 12 hours");
    push(view("empty"));
    expect(store.get().chain.reserves.word).toBe("No raids soon");
  });
});

describe("reserves in the window", () => {
  const raid = (name: string): Raid => ({
    id: name, code: "mc", name, startsAt: 1_792_000_000, durationMin: 180, instance: null, hasLootTable: true,
  });
  const raidsView = (names: string[]): RaidsView => ({ state: { kind: "fresh" }, raids: names.map(raid) });
  const waitingView: ReservesView = { state: { kind: "waiting" }, changes: 0, first: false, problem: null };
  const inGame = (...raids: string[]): ReservesView => ({
    state: { kind: "inGame", raids }, changes: 1, first: true, problem: null,
  });
  const reload = (changed: string[], changes = 2): ReservesView => ({
    state: { kind: "reload", raids: ["Molten Core", "Blackwing Lair"], changed }, changes, first: false, problem: null,
  });
  const row = (root: HTMLElement) => root.querySelectorAll(".chain li")[3] as HTMLElement;
  const headline = (root: HTMLElement) => root.querySelector(".status")!;

  /** A connected window whose raids and reserves a test can push; `notify` is the spy. */
  async function connectedWindow(first: ReservesView = waitingView) {
    let pushRaids: (v: RaidsView) => void = () => {};
    let pushReserves: (v: ReservesView) => void = () => {};
    const notify = vi.fn(() => Promise.resolve());
    const bridge = {
      ...fakeBridge(() => Promise.resolve(accepted)),
      raids: () => Promise.resolve(raidsView(["Molten Core", "Blackwing Lair"])),
      onRaids: (l: (v: RaidsView) => void) => ((pushRaids = l), Promise.resolve(() => {})),
      reserves: () => Promise.resolve(first),
      onReserves: (l: (v: ReservesView) => void) => ((pushReserves = l), Promise.resolve(() => {})),
      rememberedFolder: () =>
        Promise.resolve({ kind: "found", gameFolder: "/g", version: "_retail_", versionFolder: "/g/_retail_", addonFolder: "" }),
      notify,
    } as unknown as Bridge;
    const root = document.createElement("div");
    const store = await startWindow(root, bridge);
    return { root, store, notify, pushRaids, pushReserves, bridge };
  }

  it("keeps 'Not yet' and the connected headline while raids are listed and nothing is written", async () => {
    const { root, store } = await connectedWindow();
    expect(row(root).querySelector(".pill")?.textContent).toBe("Not yet");
    expect(store.get().status.title).toBe("Connected to the guild site");
  });

  it("reads 'In the game' and 'Ready for tonight' after a write, with the Activity line and no notice", async () => {
    const { root, store, notify, pushReserves } = await connectedWindow();
    pushReserves(inGame("Molten Core", "Blackwing Lair"));
    expect(row(root).querySelector(".pill")?.textContent).toBe("In the game");
    expect(row(root).querySelector(".pill")?.className).toBe("pill ok");
    expect(row(root).querySelector(".detail")?.textContent).toBe("Next 12 hours");
    expect(row(root).querySelectorAll(".under .row")).toHaveLength(2);
    expect(store.get().status).toEqual({ tone: "ok", word: "Ready", title: "Ready for tonight" });
    expect(headline(root).textContent).toContain("Ready for tonight");
    expect(store.get().activity[0]).toMatchObject({
      tone: "ok", kind: "Reserves written", text: "for Molten Core and Blackwing Lair. Type /reload in game.",
    });
    expect(notify).not.toHaveBeenCalled();
  });

  it("turns amber and asks for a /reload after a real change, with one notice", async () => {
    const { root, store, notify, pushReserves } = await connectedWindow();
    pushReserves(inGame("Molten Core", "Blackwing Lair"));
    pushReserves(reload(["Molten Core"]));
    expect(row(root).querySelector(".pill")?.textContent).toBe("/reload needed");
    expect(row(root).querySelector(".pill")?.className).toBe("pill warn");
    expect(row(root).querySelector(".detail")?.textContent).toBe("Molten Core changed");
    expect(row(root).className).toContain("link-open");
    expect(row(root).querySelector(".msg")?.textContent).toBe(
      "Reserves for Molten Core changed. Type /reload in game to load them.",
    );
    expect(store.get().status).toEqual({ tone: "warn", word: "Needs you", title: "Type /reload in game" });
    expect(store.get().activity[0]).toMatchObject({
      tone: "warn", kind: "Reserves changed", text: "for Molten Core. Type /reload in game.",
    });
    expect(notify).toHaveBeenCalledTimes(1);
    expect(notify).toHaveBeenCalledWith({ headline: "Type /reload in game", detail: "Reserves for Molten Core are ready." });
  });

  it("tells the tray amber through the window status, and green again once the game reloaded", async () => {
    const { store, pushReserves, bridge } = await connectedWindow();
    const tray = vi.spyOn(bridge, "setTrayStatus");
    pushReserves(reload(["Molten Core"]));
    expect(tray).toHaveBeenLastCalledWith(expect.objectContaining({ tone: "warn" }));
    pushReserves({ ...reload([]), state: { kind: "inGame", raids: ["Molten Core"] } });
    expect(store.get().status.title).toBe("Ready for tonight");
    expect(tray).toHaveBeenLastCalledWith(expect.objectContaining({ tone: "ok" }));
  });

  it("raises one notice per change, none for the same view again or for a reload ending", async () => {
    const { notify, pushReserves, store } = await connectedWindow();
    pushReserves(reload(["Molten Core"], 2));
    pushReserves(reload(["Molten Core"], 2));
    pushReserves({ ...reload([], 2), state: { kind: "inGame", raids: ["Molten Core"] } });
    expect(notify).toHaveBeenCalledTimes(1);
    expect(store.get().activity.filter((e) => e.kind === "Reserves changed")).toHaveLength(1);
    pushReserves(reload(["Blackwing Lair"], 3));
    expect(notify).toHaveBeenCalledTimes(2);
  });

  it("announces a change that was already pending when the window loaded", async () => {
    const { notify, store } = await connectedWindow(reload(["Molten Core"]));
    expect(notify).toHaveBeenCalledTimes(1);
    expect(store.get().chain.reserves.word).toBe("/reload needed");
  });

  it("still shows the change when the notice can't be raised", async () => {
    const { store, pushReserves, bridge } = await connectedWindow();
    vi.spyOn(bridge, "notify").mockRejectedValue(new Error("no notifications"));
    pushReserves(reload(["Molten Core"]));
    await flush();
    expect(store.get().status.title).toBe("Type /reload in game");
  });

  it("shows a refused write's words in Activity once, and leaves the row as it was", async () => {
    const { store, pushReserves } = await connectedWindow();
    pushReserves(inGame("Molten Core", "Blackwing Lair"));
    const refused = { ...inGame("Molten Core", "Blackwing Lair"), problem: "The reserves for Blackwing Lair are too large to save (250,000 bytes; the limit is 200,000)." };
    pushReserves(refused);
    pushReserves(refused);
    const lines = store.get().activity.filter((e) => e.kind === "Reserves not saved");
    expect(lines).toHaveLength(1);
    expect(lines[0]!.text).toBe(refused.problem);
    expect(store.get().chain.reserves.word).toBe("In the game");
  });

  it("goes back to waiting when the reserves are cleared, and ignores them with no raids listed", async () => {
    const { store, pushReserves, pushRaids } = await connectedWindow();
    pushReserves(inGame("Molten Core"));
    pushReserves({ ...waitingView, changes: 1 });
    expect(store.get().chain.reserves.word).toBe("Not yet");
    pushReserves(reload(["Molten Core"]));
    pushRaids({ state: { kind: "empty" }, raids: [] });
    expect(store.get().chain.reserves.word).toBe("No raids soon");
    expect(store.get().status.title).toBe("No raids coming up");
  });
});

describe("live award lines", () => {
  const sent = (over: Partial<ChatEvent & { kind: "sent" }> = {}): ChatEvent => ({
    kind: "sent", line: "award", item: 1, itemName: "Earthshaker", character: "Aldric", boss: "Magmadar",
    raid: "r1", raidName: "Molten Core", at: 1, ...over,
  });

  async function liveWindow() {
    let push: (e: ChatEvent) => void = () => {};
    const bridge = {
      ...fakeBridge(() => Promise.resolve(accepted)),
      rememberedFolder: () =>
        Promise.resolve({ kind: "found", gameFolder: "/g", version: "_retail_", versionFolder: "/g/_retail_", addonFolder: "" }),
      onChatLog: (l: (e: ChatEvent) => void) => ((push = l), Promise.resolve(() => {})),
    } as unknown as Bridge;
    const root = document.createElement("div");
    const store = await startWindow(root, bridge);
    return { root, store, push: (e: ChatEvent) => push(e) };
  }

  it("shows a sent award as an Activity line and counts it", async () => {
    const { store, push } = await liveWindow();
    push(sent());
    push(sent({ itemName: "Striker’s Mark", character: "Thessaly" }));
    const [newest, older] = store.get().activity;
    expect(newest).toMatchObject({ tone: "ok", kind: "Sent", strong: "Striker’s Mark", text: "to Thessaly", meta: "Magmadar" });
    expect(older?.strong).toBe("Earthshaker");
    expect(store.get().sentTonight).toBe(2);
  });

  it("counts replacements, and keeps only the last 50 lines", async () => {
    const { store, push } = await liveWindow();
    for (let i = 0; i < 55; i++) push(sent({ line: i % 2 ? "replacement" : "award" }));
    expect(store.get().activity).toHaveLength(50);
    expect(store.get().sentTonight).toBe(55);
  });

  it("shows ignored lines in amber, in Robert's words, and the next good line still shows", async () => {
    const { store, push } = await liveWindow();
    push({ kind: "ignored", reason: "malformed" });
    push({ kind: "ignored", reason: "newerVersion" });
    push(sent());
    const [good, newer, bad] = store.get().activity;
    expect(good?.kind).toBe("Sent");
    expect(newer).toMatchObject({ tone: "warn", kind: "Ignored", text: "a loot line from a newer version. Update the helper to read it." });
    expect(bad).toMatchObject({ tone: "warn", text: "a loot line the helper couldn’t read. Nothing was recorded from it." });
    expect(store.get().sentTonight).toBe(1);
  });

  it("adds no line and no count for an undo", async () => {
    const { store, push } = await liveWindow();
    push(sent({ line: "undo", character: null, boss: null }));
    expect(store.get().activity).toEqual([]);
    expect(store.get().sentTonight).toBe(0);
    expect(store.get().chain.awards.word).toBe("Not yet");
  });

  it("reads Sending, Going to <raid> and the caption, keeps the raid for #60, and tells the tray", async () => {
    const { root, store, push } = await liveWindow();
    push(sent());
    const awards = root.querySelectorAll(".chain li")[4]!;
    expect(awards.querySelector(".pill")?.textContent).toBe("Sending");
    expect(awards.textContent).toContain("Going to Molten Core");
    expect(awards.textContent).toContain("Picked in game. Change it on the add-on’s Sync tab.");
    expect(store.get().awardsRaid).toEqual({ id: "r1", name: "Molten Core" });
    expect(trayStatusOf(store.get())).toMatchObject({ sentTonight: 1, secondLine: "Awards going to Molten Core" });
  });

  it("lists lines heard in the same millisecond newest first", async () => {
    const { store, push } = await liveWindow();
    vi.spyOn(Date, "now").mockReturnValue(Date.now());
    push(sent({ itemName: "First" }));
    push(sent({ itemName: "Second" }));
    vi.restoreAllMocks();
    expect(store.get().activity.map((e) => e.strong)).toEqual(["Second", "First"]);
  });

  it("starts listening before the raids and reserves load", async () => {
    let listening = false;
    let raidsAskedAfterListening = false;
    const bridge = {
      ...fakeBridge(() => Promise.resolve({ kind: "noKey" })),
      onChatLog: () => ((listening = true), Promise.resolve(() => {})),
      onRaids: () => ((raidsAskedAfterListening = listening), Promise.resolve(() => {})),
      raids: () => new Promise(() => {}),
    } as unknown as Bridge;
    void startWindow(document.createElement("div"), bridge);
    await flush();
    expect(raidsAskedAfterListening).toBe(true);
  });

  it("starts the count again at midnight even when no new line comes", async () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date(2026, 9, 14, 23, 59));
    const { store, push } = await liveWindow();
    push(sent());
    expect(store.get().sentTonight).toBe(1);
    vi.setSystemTime(new Date(2026, 9, 15, 0, 5));
    vi.advanceTimersByTime(60_000);
    expect(store.get().sentTonight).toBe(0);
    vi.useRealTimers();
  });
});

describe("the sync states", () => {
  const site = (root: HTMLElement) => root.querySelectorAll<HTMLElement>(".chain li")[0]!;
  const awards = (root: HTMLElement) => root.querySelectorAll<HTMLElement>(".chain li")[4]!;
  const refused = (n: number): SyncStatus => ({
    kind: "refused",
    otherSite: 0,
    records: Array.from({ length: n }, (_, i) => ({ id: `r${i}`, item: 19019, character: "Thessaly Ward", code: "x" })),
  });

  async function connected() {
    let push: (s: SyncStatus) => void = () => {};
    const notify = vi.fn(() => Promise.resolve());
    const bridge = {
      ...fakeBridge(() => Promise.resolve(accepted)),
      syncStatus: () => Promise.resolve<SyncStatus>({ kind: "idle", otherSite: 0 }),
      onSyncStatus: (l: (s: SyncStatus) => void) => ((push = l), Promise.resolve(() => {})),
      rememberedFolder: () =>
        Promise.resolve({ kind: "found", gameFolder: "/g", version: "_retail_", versionFolder: "/g/_retail_", addonFolder: "" }),
      notify,
    } as Bridge;
    const root = document.createElement("div");
    const store = await startWindow(root, bridge);
    return { root, store, push, notify, bridge };
  }

  it("offline: the title, the opened Guild site row and one notice, not repeated on retries", async () => {
    const { root, push, notify } = await connected();
    push({ kind: "offline", waiting: 4 });
    expect(root.querySelector(".status")?.textContent).toContain("Offline: 4 awards waiting");
    expect(site(root).className).toContain("link-open");
    expect(site(root).querySelector(".msg")?.textContent).toBe(
      "They’ll send when you’re back online. Nothing is lost. The game still has its reserves.",
    );
    expect(awards(root).textContent).toContain("Waiting");
    push({ kind: "offline", waiting: 5 });
    push({ kind: "offline", waiting: 5 });
    expect(notify).toHaveBeenCalledTimes(1);
    expect(notify).toHaveBeenCalledWith({
      headline: "Offline: 4 awards waiting",
      detail: "They’ll send when you’re back online.",
    });
    push({ kind: "idle", otherSite: 0 });
    expect(root.querySelector(".status")?.textContent).not.toContain("Offline");
    push({ kind: "offline", waiting: 1 });
    expect(notify).toHaveBeenCalledTimes(2);
  });

  it("refused: the Awards row, the button, an Activity line and one notice per record", async () => {
    const { root, store, push, notify } = await connected();
    push(refused(1));
    expect(awards(root).querySelector(".pill")?.textContent).toBe("1 refused");
    expect(awards(root).querySelector(".msg")?.textContent).toBe(
      "An award to Thessaly Ward wasn’t recorded. Check it on the loot log. Other awards are still sending.",
    );
    expect(awards(root).querySelector("button")?.textContent).toBe("Open the loot log");
    expect(awards(root).querySelector(".under")).toBeNull();
    expect(root.querySelector(".status")?.textContent).toContain("The site refused 1 award");
    expect(store.get().activity[0]).toMatchObject({ tone: "stop", kind: "Refused" });
    push(refused(1));
    expect(notify).toHaveBeenCalledTimes(1);
    expect(notify).toHaveBeenCalledWith({ headline: "The site refused 1 award", detail: "Check the loot log." });
    push(refused(2));
    expect(notify).toHaveBeenCalledTimes(2);
    expect(awards(root).querySelector(".pill")?.textContent).toBe("2 refused");
  });

  it("refused: names the item once the chat log has shown it", async () => {
    let chat: (e: ChatEvent) => void = () => {};
    let push: (s: SyncStatus) => void = () => {};
    const bridge = {
      ...fakeBridge(() => Promise.resolve(accepted)),
      onChatLog: (l: (e: ChatEvent) => void) => ((chat = l), Promise.resolve(() => {})),
      syncStatus: () => Promise.resolve<SyncStatus>({ kind: "idle", otherSite: 0 }),
      onSyncStatus: (l: (s: SyncStatus) => void) => ((push = l), Promise.resolve(() => {})),
      rememberedFolder: () =>
        Promise.resolve({ kind: "found", gameFolder: "/g", version: "_retail_", versionFolder: "/g/_retail_", addonFolder: "" }),
      notify: vi.fn(() => Promise.resolve()),
      openLootLog: vi.fn(() => Promise.resolve()),
    } as Bridge;
    const root = document.createElement("div");
    await startWindow(root, bridge);
    chat({
      kind: "sent", line: "award", item: 19019, itemName: "Thunderfury", character: "Thessaly Ward",
      boss: "Garr", raid: "r1", raidName: "Molten Core", at: 1,
    });
    push(refused(1));
    expect(awards(root).querySelector(".msg")?.textContent).toBe(
      "Thunderfury to Thessaly Ward wasn’t recorded. Check it on the loot log. Other awards are still sending.",
    );
    expect(bridge.notify).toHaveBeenCalledWith({
      headline: "The site refused 1 award",
      detail: "Check Thunderfury on the loot log.",
    });
    awards(root).querySelector("button")!.click();
    expect(bridge.openLootLog).toHaveBeenCalledWith("r1");
  });

  describe("clearing a refusal once the window has shown it", () => {
    const visibility = (state: "visible" | "hidden") => {
      Object.defineProperty(document, "visibilityState", { value: state, configurable: true });
      document.dispatchEvent(new Event("visibilitychange"));
    };
    async function shownWindow() {
      const made = await connected();
      const clearRefused = vi.fn(() => Promise.resolve());
      Object.assign(made.bridge, { clearRefused, openLootLog: vi.fn(() => Promise.resolve()) });
      return { ...made, clearRefused };
    }
    afterEach(() => visibility("visible"));

    it("clicking Open the loot log clears the records it showed; the Activity line stays", async () => {
      const { root, store, push, clearRefused } = await shownWindow();
      push(refused(2));
      awards(root).querySelector("button")!.click();
      expect(clearRefused).toHaveBeenCalledTimes(1);
      expect(clearRefused).toHaveBeenCalledWith(["r0", "r1"]);
      push({ kind: "idle", otherSite: 0 });
      expect(awards(root).querySelector(".pill")?.textContent).not.toContain("refused");
      expect(store.get().activity.filter((a) => a.kind === "Refused")).toHaveLength(2);
    });

    it("closing the window after it displayed a refusal clears it, once", async () => {
      const { push, clearRefused } = await shownWindow();
      push(refused(1));
      visibility("hidden");
      expect(clearRefused).toHaveBeenCalledWith(["r0"]);
      push({ kind: "idle", otherSite: 0 });
      visibility("visible");
      visibility("hidden");
      expect(clearRefused).toHaveBeenCalledTimes(1);
    });

    it("a clear that fails is tried again on the next click or close", async () => {
      const { root, push, clearRefused } = await shownWindow();
      clearRefused.mockRejectedValueOnce(new Error("no"));
      push(refused(1));
      awards(root).querySelector("button")!.click();
      await Promise.resolve();
      visibility("hidden");
      expect(clearRefused).toHaveBeenCalledTimes(2);
      expect(clearRefused).toHaveBeenLastCalledWith(["r0"]);
    });

    it("a refusal that arrives while the window is hidden is cleared only after it was shown", async () => {
      const { root, push, clearRefused } = await shownWindow();
      visibility("hidden");
      push(refused(1));
      expect(clearRefused).not.toHaveBeenCalled();
      visibility("visible");
      expect(awards(root).querySelector(".pill")?.textContent).toBe("1 refused");
      visibility("hidden");
      expect(clearRefused).toHaveBeenCalledWith(["r0"]);
    });

    it("a refusal after the clearing shows again with its own notice", async () => {
      const { push, notify, clearRefused } = await shownWindow();
      push(refused(1));
      visibility("hidden");
      push({ kind: "idle", otherSite: 0 });
      visibility("visible");
      push({ kind: "refused", otherSite: 0, records: [{ id: "r9", item: null, character: null, code: "x" }] });
      expect(notify).toHaveBeenCalledTimes(2);
      visibility("hidden");
      expect(clearRefused).toHaveBeenLastCalledWith(["r9"]);
    });
  });

  it("a revoked key: stopped, 'Add a new key' opens the key step, one notice", async () => {
    const { root, push, notify } = await connected();
    push({ kind: "keyStopped" });
    push({ kind: "keyStopped" });
    expect(root.querySelector(".status")?.textContent).toContain("Your helper key no longer works");
    expect(notify).toHaveBeenCalledTimes(1);
    expect(notify).toHaveBeenCalledWith({
      headline: "Your helper key no longer works",
      detail: "Add a new key to keep sending awards.",
    });
    const add = site(root).querySelector("button")!;
    expect(add.textContent).toBe("Add a new key");
    add.click();
    expect(root.querySelector("#key")).not.toBeNull();
  });

  it("the site switched off: stopped, no button, no notice", async () => {
    const { root, push, notify } = await connected();
    push({ kind: "siteOff" });
    expect(root.querySelector(".status")?.textContent).toContain("The site has the helper switched off");
    expect(site(root).querySelector("button")).toBeNull();
    expect(notify).not.toHaveBeenCalled();
  });

  it("a damaged queue: 'Sending has stopped', and nothing on screen to act on", async () => {
    const { root, push } = await connected();
    push({ kind: "queueDamaged", problem: "x" });
    expect(root.querySelector(".status")?.textContent).toContain("Sending has stopped");
    expect(awards(root).querySelector(".msg")?.textContent).toContain("Nothing was deleted.");
    expect(root.querySelectorAll(".chain button")).toHaveLength(0);
  });

  it("the tray follows the problem's headline", async () => {
    const { push, bridge } = await connected();
    const tray = vi.fn(() => Promise.resolve());
    bridge.setTrayStatus = tray;
    push({ kind: "offline", waiting: 2 });
    expect(tray).toHaveBeenLastCalledWith(expect.objectContaining({ tone: "stop", word: "Offline" }));
  });
});
