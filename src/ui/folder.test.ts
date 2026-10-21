// @vitest-environment happy-dom
import { describe, expect, it, vi } from "vitest";
import type { Bridge, FolderOutcome, Install, KeyOutcome } from "../bridge";
import { shortPath } from "./folder";
import { startWindow } from "./realWindow";

const accepted: KeyOutcome = {
  kind: "accepted",
  officer: "Aldric Vane",
  env: "production",
  formatVersions: [1],
  addonVersion: "0.1.0",
  addonUrl: "https://example.test/addon",
};
const WOW = "D:\\Games\\World of Warcraft";
const install = (version: string): Install => ({
  gameFolder: WOW,
  version,
  versionFolder: `${WOW}\\${version}`,
  addonFolder: `${WOW}\\${version}\\Interface\\AddOns\\BureaucracyForever`,
});
const found = (version = "_classic_"): FolderOutcome => ({ kind: "found", ...install(version) });
const nothing: FolderOutcome = { kind: "nothingRemembered" };
const NOT_FOUND = "Couldn’t find World of Warcraft. Choose the folder it’s installed in.";
const NOT_GAME = "That folder isn’t a World of Warcraft folder. Choose the folder named World of Warcraft.";
const NO_ADDON = "World of Warcraft is here, but the Bureaucracy Forever add-on isn’t installed.";
const GONE = "The game folder you chose before isn’t there any more. Choose it again.";

type Folder = Partial<
  Pick<Bridge, "searchFolder" | "checkPickedFolder" | "rememberVersion" | "rememberedFolder" | "pickFolder">
>;

/** A connected helper whose folder engine answers as given. */
function fake(folder: Folder = {}): Bridge {
  const unused = () => Promise.reject(new Error("not used"));
  return {
    connect: unused,
    checkStored: () => Promise.resolve(accepted),
    removeKey: unused,
    staging: unused,
    setStaging: unused,
    syncStatus: unused,
    clearRefused: unused,
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
    searchFolder: vi.fn(() => Promise.resolve(nothing)),
    checkPickedFolder: unused,
    rememberVersion: vi.fn((f: string) => Promise.resolve({ kind: "found", ...install(f.split("\\").pop()!) } as FolderOutcome)),
    rememberedFolder: vi.fn(() => Promise.resolve(nothing)),
    pickFolder: unused,
    ...Object.fromEntries(Object.entries(folder).map(([name, run]) => [name, vi.fn(run)])),
  };
}

async function open(folder: Folder) {
  const root = document.createElement("div");
  document.body.append(root);
  const bridge = fake(folder);
  const store = await startWindow(root, bridge);
  return { root, bridge, store };
}
const body = (root: HTMLElement) => root.querySelector(".open-body")!;
const labels = (root: HTMLElement) => [...body(root).querySelectorAll("button")].map((b) => b.textContent);
const press = (root: HTMLElement, label: string) =>
  [...root.querySelectorAll("button")].find((b) => b.textContent === label)!.click();
const flush = () => new Promise((r) => setTimeout(r, 0));

describe("step 2: the game folder", () => {
  it("shows the search while it runs", async () => {
    const root = document.createElement("div");
    void startWindow(root, fake({ searchFolder: () => new Promise(() => {}) }));
    await flush();
    expect(body(root).textContent).toBe("Looking for World of Warcraft…");
    expect(body(root).querySelector("[role=status]")).not.toBeNull();
    expect(root.querySelectorAll(".chain li")[1]?.textContent).toContain("Looking for the game");
  });

  it("offers one match with its path and uses it on request", async () => {
    const { root, bridge, store } = await open({ searchFolder: () => Promise.resolve(found()) });
    expect(body(root).textContent).toContain("Found the game");
    expect(body(root).textContent).toContain("Add-on installed");
    expect(body(root).querySelector(".mono")?.textContent).toBe(`${WOW}\\_classic_`);
    expect(labels(root)).toEqual(["Use this folder", "Choose a different folder"]);
    press(root, "Use this folder");
    await flush();
    expect(bridge.rememberVersion).toHaveBeenCalledWith(`${WOW}\\_classic_`);
    expect(store.get().setup).toBeUndefined();
    expect(root.querySelector(".chain")?.textContent).toContain("…/World of Warcraft/_classic_Found");
  });

  it("lets the officer pick one of several versions and continue", async () => {
    const { root, bridge } = await open({
      searchFolder: () => Promise.resolve({ kind: "several", installs: [install("_classic_"), install("_classic_era_")] }),
    });
    expect(body(root).textContent).toContain(
      "The add-on is installed in more than one game version. Which one do you play?",
    );
    const radios = body(root).querySelectorAll<HTMLInputElement>("input[type=radio]");
    expect(radios).toHaveLength(2);
    expect(radios[0]?.checked).toBe(true);
    expect(body(root).querySelectorAll(".sub")[1]?.textContent).toBe(`${WOW}\\_classic_era_`);
    radios[1]!.checked = true;
    radios[1]!.dispatchEvent(new Event("change"));
    press(root, "Continue");
    await flush();
    expect(bridge.rememberVersion).toHaveBeenCalledWith(`${WOW}\\_classic_era_`);
  });

  it.each<[string, FolderOutcome, string, string[]]>([
    ["not found", { kind: "notFound", message: NOT_FOUND }, NOT_FOUND, ["Choose folder…"]],
    ["no add-on", { kind: "noAddon", message: NO_ADDON, path: WOW, versionFolders: [] }, NO_ADDON, ["Choose a different folder"]],
  ])("shows %s with the engine's words", async (_name, outcome, message, buttons) => {
    const { root } = await open({ searchFolder: () => Promise.resolve(outcome) });
    expect(body(root).textContent).toContain(message);
    expect(body(root).querySelector("[role=alert]")).not.toBeNull();
    expect(labels(root)).toEqual(buttons);
  });

  it("offers Install the add-on when the game has one version", async () => {
    const outcome: FolderOutcome = { kind: "noAddon", message: NO_ADDON, path: WOW, versionFolders: [`${WOW}\\_classic_`] };
    const { root } = await open({ searchFolder: () => Promise.resolve(outcome) });
    expect(labels(root)).toEqual(["Install the add-on", "Choose a different folder"]);
  });

  it("shows the remembered folder's absence with its path, never replacing it by a search", async () => {
    const { root, bridge } = await open({
      rememberedFolder: () => Promise.resolve({ kind: "gone", message: GONE, path: WOW }),
    });
    expect(body(root).textContent).toContain(GONE);
    expect(body(root).querySelector(".mono")?.textContent).toBe(WOW);
    expect(labels(root)).toEqual(["Choose folder…"]);
    expect(bridge.searchFolder).not.toHaveBeenCalled();
  });

  it("finishes at once when the remembered folder is still good", async () => {
    const { root, bridge, store } = await open({ rememberedFolder: () => Promise.resolve(found()) });
    expect(bridge.searchFolder).not.toHaveBeenCalled();
    expect(store.get().setup).toBeUndefined();
    expect(root.querySelector(".chain")?.textContent).toContain("Found");
  });

  it("checks a picked folder and shows what it is", async () => {
    const picked = "C:\\Users\\Élise Marchand\\Downloads";
    const { root, bridge } = await open({
      searchFolder: () => Promise.resolve({ kind: "notFound", message: NOT_FOUND }),
      pickFolder: () => Promise.resolve(picked),
      checkPickedFolder: () => Promise.resolve({ kind: "notGame", message: NOT_GAME }),
    });
    press(root, "Choose folder…");
    await flush();
    expect(bridge.checkPickedFolder).toHaveBeenCalledWith(picked);
    expect(body(root).textContent).toContain(NOT_GAME);
    expect(body(root).querySelector(".mono")?.textContent).toBe(picked);
  });

  it("changes nothing when the picker is cancelled", async () => {
    const { root } = await open({
      searchFolder: () => Promise.resolve(found()),
      pickFolder: () => Promise.resolve(null),
    });
    press(root, "Choose a different folder");
    await flush();
    expect(labels(root)).toEqual(["Use this folder", "Choose a different folder"]);
  });

  it("says so, and keeps the buttons, when saving the folder fails", async () => {
    const message = "The helper couldn’t save that folder. Try again.";
    const { root } = await open({
      searchFolder: () => Promise.resolve(found()),
      rememberVersion: () => Promise.resolve({ kind: "settingsFailed", message }),
    });
    press(root, "Use this folder");
    await flush();
    expect(body(root).textContent).toContain(message);
    expect(labels(root)).toEqual(["Use this folder", "Choose a different folder"]);
  });

  it("shows a plain error when the Rust side fails outright", async () => {
    const { root } = await open({ searchFolder: () => Promise.reject(new Error("boom")) });
    expect(body(root).textContent).toContain("The helper couldn’t check that folder. Try again.");
  });
});

describe("Settings → Game folder", () => {
  async function settings(folder: Folder) {
    const opened = await open({ rememberedFolder: () => Promise.resolve(found("_retail_")), ...folder });
    opened.root.querySelector<HTMLButtonElement>("[aria-label=Settings]")!.click();
    return opened;
  }
  const section = (root: HTMLElement) =>
    [...root.querySelectorAll("section.stack")].find((s) => s.textContent?.startsWith("Game folder"))!;

  it("shows the remembered path with a Change folder button", async () => {
    const { root } = await settings({});
    expect(section(root).textContent).toContain("Game folder");
    expect(section(root).querySelector(".mono")?.textContent).toBe(`${WOW}\\_retail_`);
    expect([...section(root).querySelectorAll("button")].map((b) => b.textContent)).toEqual(["Change folder"]);
  });

  it("opens the picker, checks the picked folder and remembers it", async () => {
    const { root, bridge } = await settings({
      pickFolder: () => Promise.resolve(WOW),
      checkPickedFolder: () => Promise.resolve(found("_classic_")),
    });
    press(root, "Change folder");
    await flush();
    expect(bridge.checkPickedFolder).toHaveBeenCalledWith(WOW);
    expect(bridge.rememberVersion).toHaveBeenCalledWith(`${WOW}\\_classic_`);
    expect(section(root).querySelector(".mono")?.textContent).toBe(`${WOW}\\_classic_`);
  });

  it("explains a folder that isn't right and keeps the old one", async () => {
    const { root, bridge } = await settings({
      pickFolder: () => Promise.resolve("C:\\Temp"),
      checkPickedFolder: () => Promise.resolve({ kind: "notGame", message: NOT_GAME }),
    });
    press(root, "Change folder");
    await flush();
    expect(section(root).textContent).toContain(NOT_GAME);
    expect(bridge.rememberVersion).not.toHaveBeenCalled();
    expect(section(root).querySelectorAll(".mono")[0]?.textContent).toBe(`${WOW}\\_retail_`);
    expect(section(root).querySelectorAll(".mono")[1]?.textContent).toBe("C:\\Temp");
  });
});

describe("Settings → Game folder, when things go wrong", () => {
  const section = (root: HTMLElement) =>
    [...root.querySelectorAll("section.stack")].find((s) => s.textContent?.startsWith("Game folder"))!;
  async function settings(folder: Folder) {
    const opened = await open({ rememberedFolder: () => Promise.resolve(found("_retail_")), ...folder });
    opened.root.querySelector<HTMLButtonElement>("[aria-label=Settings]")!.click();
    return opened;
  }

  it("shows the engine's words when the new folder can't be saved, and keeps the old path", async () => {
    const message = "The helper couldn’t save that folder. Try again.";
    const { root } = await settings({
      pickFolder: () => Promise.resolve(WOW),
      checkPickedFolder: () => Promise.resolve(found("_classic_")),
      rememberVersion: () => Promise.resolve({ kind: "settingsFailed", message }),
    });
    press(root, "Change folder");
    await flush();
    expect(section(root).textContent).toContain(message);
    expect(section(root).querySelectorAll(".mono")[0]?.textContent).toBe(`${WOW}\\_retail_`);
    expect(section(root).textContent).toContain("Use this folder");
  });

  it("retries the folder last chosen, not an earlier one that failed", async () => {
    const picks = [`${WOW}\\a`, `${WOW}\\b`];
    const { root, bridge } = await settings({
      pickFolder: () => Promise.resolve(picks.shift() ?? null),
      checkPickedFolder: (f) => Promise.resolve(found(f.endsWith("a") ? "_classic_" : "_classic_era_")),
      rememberVersion: () => Promise.resolve({ kind: "settingsFailed", message: "Try again." }),
    });
    press(root, "Change folder");
    await flush();
    press(root, "Change folder");
    await flush();
    press(root, "Use this folder");
    await flush();
    expect(bridge.rememberVersion).toHaveBeenLastCalledWith(`${WOW}\\_classic_era_`);
  });

  it("ignores a second change while one is being checked, so the latest choice stays", async () => {
    let answer!: (o: FolderOutcome) => void;
    const check = vi.fn(() => new Promise<FolderOutcome>((r) => (answer = r)));
    const { root, bridge } = await settings({ pickFolder: () => Promise.resolve(WOW), checkPickedFolder: check });
    press(root, "Change folder");
    await flush();
    const again = [...root.querySelectorAll("button")].find((b) => b.textContent === "Change folder")!;
    expect(again.disabled).toBe(true);
    again.click();
    await flush();
    expect(check).toHaveBeenCalledTimes(1);
    answer(found("_classic_"));
    await flush();
    expect(bridge.rememberVersion).toHaveBeenCalledTimes(1);
    expect(section(root).querySelector(".mono")?.textContent).toBe(`${WOW}\\_classic_`);
  });
});

describe("shortPath", () => {
  it.each([
    ["/Applications/World of Warcraft/_classic_", "…/World of Warcraft/_classic_"],
    ["D:\\Games\\World of Warcraft\\_retail_", "…/World of Warcraft/_retail_"],
    ["/wow", "/wow"],
  ])("%s", (path, short) => expect(shortPath(path)).toBe(short));
});
