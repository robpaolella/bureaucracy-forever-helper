// @vitest-environment happy-dom
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AddonCheck, AddonInstall, Bridge, FolderOutcome, KeyOutcome } from "../bridge";
import { createStore } from "../state";
import { addonRow, addonSwitchSection, addonUpdates, autoUpdateOn } from "./addonUpdate";
import { mountApp } from "./app";
import { initialState, startWindow } from "./realWindow";
import { versionsSection } from "./update";

const flush = () => new Promise((r) => setTimeout(r, 0));
const checked = (latest: string, installed: string | null, note: string | null = null): AddonCheck => ({
  kind: "checked",
  latest,
  note,
  installed,
  upToDate: installed === latest,
});
const failure = (kept: string | null): AddonInstall => ({
  kind: "notInstalled",
  reason: "installFailed",
  message: "The add-on couldn’t be put in place.",
  kept,
});

/** A bridge whose add-on engine answers as given; everything else is unused. */
function fakeBridge(engine: { check?: () => Promise<AddonCheck>; install?: () => Promise<AddonInstall> } = {}) {
  const unused = () => Promise.reject(new Error("not used"));
  return {
    connect: unused, checkStored: unused, removeKey: unused, staging: unused, setStaging: unused, syncStatus: unused,
    onSyncStatus: unused, raids: unused, onRaids: unused, raidChoice: unused, onRaidChoice: unused, reserves: unused, onReserves: unused, onChatLog: unused,
    searchFolder: unused, checkPickedFolder: unused, rememberVersion: unused, rememberedFolder: unused, pickFolder: unused,
    checkForUpdate: unused, installUpdate: unused, restartAfterUpdate: unused, onUpdateProgress: unused,
    openUpdateNotes: unused, onTrayCheck: unused, helperVersion: unused,
    checkAddon: vi.fn(engine.check ?? (() => Promise.resolve(checked("1.4.0", "1.3.2")))),
    installAddon: vi.fn(engine.install ?? (() => Promise.resolve<AddonInstall>({ kind: "installed", version: "1.4.0" }))),
    openAddonNotes: vi.fn(() => Promise.resolve()),
    openLootLog: vi.fn(() => Promise.resolve()),
    onAddonProgress: unused,
    setTrayStatus: () => Promise.resolve(),
    notify: vi.fn(() => Promise.resolve()),
  } satisfies Bridge;
}

/** The add-on flow over a window that has found its game folder, drawn into `root`. */
function open(bridge: Bridge) {
  const store = createStore({ ...initialState, gamePath: "/wow/_classic_" });
  const root = document.createElement("div");
  const updates = addonUpdates(store, bridge, (s) => addonRow(s.chain.addon, s.addon, s.addonVersion, updates));
  const app = mountApp(root, { getState: store.get });
  store.subscribe(app.refresh);
  return { store, root, updates };
}

const row = (root: HTMLElement) => root.querySelector<HTMLElement>(".chain li:nth-child(3)")!;
const press = (root: HTMLElement, label: string) =>
  [...root.querySelectorAll<HTMLElement>("button, a")].find((b) => b.textContent === label)!.click();
const activity = (root: HTMLElement) => [...root.querySelectorAll(".acts li")].map((li) => li.textContent ?? "");

beforeEach(() => localStorage.clear());

describe("the automatic update switch", () => {
  it("is on by default and its value is kept between runs", () => {
    expect(autoUpdateOn()).toBe(true);
    const section = addonSwitchSection();
    const sw = section.querySelector<HTMLElement>("[role=switch]")!;
    expect(section.textContent).toContain("Update the add-on automatically");
    expect(sw.getAttribute("aria-checked")).toBe("true");
    sw.click();
    expect(autoUpdateOn()).toBe(false);
    expect(addonSwitchSection().querySelector("[role=switch]")?.getAttribute("aria-checked")).toBe("false");
    addonSwitchSection().querySelector<HTMLElement>("[role=switch]")!.click();
    expect(autoUpdateOn()).toBe(true);
  });
});

describe("with the switch on", () => {
  it("installs a newer release with no press, then shows Updated, an Activity line and one notice", async () => {
    const bridge = fakeBridge();
    const { store, root, updates } = open(bridge);
    await updates.check();
    expect(bridge.installAddon).toHaveBeenCalledWith("/wow/_classic_");
    expect(row(root).textContent).toContain("Updated");
    expect(row(root).textContent).toContain("Version 1.4.0");
    expect(store.get().addonVersion).toBe("1.4.0");
    expect(activity(root)[0]).toContain("Add-on updated:");
    expect(activity(root)[0]).toContain("to 1.4.0.");
    expect(bridge.notify).toHaveBeenCalledTimes(1);
    expect(bridge.notify).toHaveBeenCalledWith({
      headline: "Add-on updated to 1.4.0",
      detail: "Loads on your next /reload or game start.",
    });
    await updates.check();
    expect(bridge.installAddon).toHaveBeenCalledTimes(1);
    expect(bridge.notify).toHaveBeenCalledTimes(1);
  });

  it("shows the release note under Updated only when the release has one", async () => {
    const note = "Type /reload in the game after updating.";
    const withNote = open(fakeBridge({ check: () => Promise.resolve(checked("1.4.0", "1.3.2", note)) }));
    await withNote.updates.check();
    expect(row(withNote.root).textContent).toContain(note);
    const without = open(fakeBridge());
    await without.updates.check();
    expect(row(without.root).querySelector(".under")).toBeNull();
  });

  it("shows Installing with a progress bar while it runs", async () => {
    let finish: (o: AddonInstall) => void = () => {};
    const { root, updates } = open(fakeBridge({ install: () => new Promise((r) => (finish = r)) }));
    const done = updates.check();
    await flush();
    expect(row(root).textContent).toContain("Installing 1.4.0");
    expect(row(root).textContent).toContain("Your reserves file is kept.");
    updates.progress({ step: "downloading", version: "1.4.0", downloaded: 70, total: 100 });
    expect(row(root).querySelector(".track-bar")?.getAttribute("aria-valuenow")).toBe("70");
    expect(row(root).textContent).toContain("70%");
    updates.progress({ step: "downloading", version: "1.4.0", downloaded: 7, total: null });
    expect(row(root).textContent).not.toContain("%");
    finish({ kind: "installed", version: "1.4.0" });
    await done;
    expect(row(root).textContent).toContain("Updated");
  });

  it("on a failure keeps the old version, says so once, and doesn't try again by itself", async () => {
    const bridge = fakeBridge({ install: () => Promise.resolve(failure("1.3.2")) });
    const { root, updates } = open(bridge);
    await updates.check();
    expect(row(root).textContent).toContain("Not updated");
    expect(row(root).textContent).toContain("Still on 1.3.2");
    expect(row(root).textContent).toContain("Add-on 1.3.2 is still in place, with your reserves file. Nothing was lost.");
    expect([...row(root).querySelectorAll("button")].map((b) => b.textContent)).toEqual(["Try again"]);
    expect(activity(root)).toHaveLength(1);
    expect(activity(root)[0]).toContain("Add-on update not installed:");
    expect(activity(root)[0]).toContain("1.3.2 is still in place.");
    expect(bridge.notify).toHaveBeenCalledTimes(1);
    expect(bridge.notify).toHaveBeenCalledWith({
      headline: "The add-on update didn’t install",
      detail: "Add-on 1.3.2 is still in place.",
    });
    await updates.check();
    await updates.check();
    expect(bridge.installAddon).toHaveBeenCalledTimes(1);
    expect(bridge.notify).toHaveBeenCalledTimes(1);
    expect(row(root).textContent).toContain("Not updated");
  });

  it("doesn't try a version again after a restart either", async () => {
    const first = fakeBridge({ install: () => Promise.resolve(failure("1.3.2")) });
    await open(first).updates.check();
    const second = fakeBridge();
    const { root, updates } = open(second);
    await updates.check();
    expect(second.installAddon).not.toHaveBeenCalled();
    expect(row(root).textContent).toContain("Update ready");
  });

  it("treats a call that fails outright as a failed install", async () => {
    const { root, updates } = open(fakeBridge({ install: () => Promise.reject(new Error("boom")) }));
    await updates.check();
    expect(row(root).textContent).toContain("Not updated");
  });

  it("stays quiet when the check fails or the add-on is current", async () => {
    const failed = fakeBridge({
      check: () => Promise.resolve({ kind: "failed", reason: "offline", message: "No internet", installed: "1.3.2" }),
    });
    const a = open(failed);
    await a.updates.check();
    expect(failed.installAddon).not.toHaveBeenCalled();
    expect(row(a.root).textContent).toContain("Installed");
    expect(row(a.root).textContent).toContain("Version 1.3.2");
    const current = fakeBridge({ check: () => Promise.resolve(checked("1.3.2", "1.3.2")) });
    const b = open(current);
    await b.updates.check();
    expect(current.installAddon).not.toHaveBeenCalled();
    expect(current.notify).not.toHaveBeenCalled();
    expect(row(b.root).textContent).toContain("Installed");
  });
});

describe("with the switch off", () => {
  beforeEach(() => localStorage.setItem("addon-auto-update", "off"));

  it("shows Update ready and installs nothing until the officer presses Install", async () => {
    const bridge = fakeBridge();
    const { root, updates } = open(bridge);
    await updates.check();
    expect(bridge.installAddon).not.toHaveBeenCalled();
    expect(row(root).textContent).toContain("Update ready");
    expect(row(root).textContent).toContain("Version 1.4.0 is ready");
    expect(row(root).textContent).toContain("After it installs, type /reload in game, or start the game if it’s closed.");
    expect([...row(root).querySelectorAll("button, a")].map((b) => b.textContent)).toEqual(["Install", "Later", "What’s new"]);
    expect(row(root).classList.contains("link-open")).toBe(true);
    press(root, "Install");
    await flush();
    expect(bridge.installAddon).toHaveBeenCalledTimes(1);
    expect(row(root).textContent).toContain("Updated");
    // A press is the officer's own doing: no notice, but the Activity line.
    expect(bridge.notify).not.toHaveBeenCalled();
    expect(activity(root)[0]).toContain("Add-on updated:");
  });

  it("shows the release note on the card when there is one", async () => {
    const { root, updates } = open(fakeBridge({ check: () => Promise.resolve(checked("1.4.0", "1.3.2", "Type /reload after.")) }));
    await updates.check();
    expect(row(root).textContent).toContain("Type /reload after.");
  });

  it("puts the update off with Later, and doesn't show it again at the next check", async () => {
    const { root, updates } = open(fakeBridge());
    await updates.check();
    press(root, "Later");
    expect(row(root).textContent).toContain("Version 1.3.2");
    await updates.check();
    expect(row(root).textContent).toContain("Installed");
  });

  it("opens what's new for the version shown", async () => {
    const bridge = fakeBridge();
    const { root, updates } = open(bridge);
    await updates.check();
    press(root, "What’s new");
    expect(bridge.openAddonNotes).toHaveBeenCalledWith("1.4.0");
  });

  it("after a failed press, Try again installs again, with no notice", async () => {
    let answer: AddonInstall = failure("1.3.2");
    const bridge = fakeBridge({ install: () => Promise.resolve(answer) });
    const { root, updates } = open(bridge);
    await updates.check();
    press(root, "Install");
    await flush();
    expect(row(root).textContent).toContain("Not updated");
    expect(activity(root)[0]).toContain("Add-on update not installed:");
    answer = { kind: "installed", version: "1.4.0" };
    press(root, "Try again");
    await flush();
    expect(bridge.installAddon).toHaveBeenCalledTimes(2);
    expect(row(root).textContent).toContain("Updated");
    expect(bridge.notify).not.toHaveBeenCalled();
  });
});

describe("the first install, from the folder step", () => {
  const WOW = "/wow";
  const noAddon = (versionFolders: string[]): FolderOutcome => ({ kind: "noAddon", message: "No add-on here.", path: WOW, versionFolders });
  const accepted: KeyOutcome = { kind: "accepted", officer: "Aldric", env: "production", formatVersions: [1], addonVersion: "1.4.0", addonUrl: "" };

  async function setup(folder: FolderOutcome, engine: Parameters<typeof fakeBridge>[0] = {}) {
    const base = fakeBridge({ check: () => Promise.resolve(checked("1.4.0", null)), ...engine });
    const bridge = {
      ...base,
      checkStored: () => Promise.resolve(accepted),
      rememberedFolder: () => Promise.resolve(folder),
      rememberVersion: vi.fn(() =>
        Promise.resolve<FolderOutcome>({ kind: "found", gameFolder: WOW, version: "_classic_", versionFolder: `${WOW}/_classic_`, addonFolder: "" }),
      ),
    } satisfies Bridge;
    const root = document.createElement("div");
    const store = await startWindow(root, bridge);
    await flush();
    return { bridge, root, store };
  }

  it("installs into the one game version and moves on", async () => {
    const { bridge, root, store } = await setup(noAddon([`${WOW}/_classic_`]));
    expect(store.get().setup).toBeDefined();
    press(root, "Install the add-on");
    await flush();
    expect(bridge.checkAddon).toHaveBeenCalledWith(`${WOW}/_classic_`);
    expect(bridge.installAddon).toHaveBeenCalledWith(`${WOW}/_classic_`);
    expect(bridge.rememberVersion).toHaveBeenCalledWith(`${WOW}/_classic_`);
    expect(store.get().setup).toBeUndefined();
    expect(store.get().gamePath).toBe(`${WOW}/_classic_`);
  });

  it("stays on the step with the reason when the install fails", async () => {
    const { bridge, root, store } = await setup(noAddon([`${WOW}/_classic_`]), { install: () => Promise.resolve(failure(null)) });
    press(root, "Install the add-on");
    await flush();
    expect(store.get().setup).toBeDefined();
    expect(store.get().folder?.error).toBe("The add-on couldn’t be put in place.");
    expect(bridge.rememberVersion).not.toHaveBeenCalled();
    expect(root.textContent).toContain("The add-on couldn’t be put in place.");
    expect([...root.querySelectorAll("button")].map((b) => b.textContent)).toContain("Install the add-on");
  });

  it("stays on the step when the check can't find a release", async () => {
    const { bridge, root, store } = await setup(noAddon([`${WOW}/_classic_`]), {
      check: () => Promise.resolve({ kind: "failed", reason: "site", message: "No latest version.", installed: null }),
    });
    press(root, "Install the add-on");
    await flush();
    expect(bridge.installAddon).not.toHaveBeenCalled();
    expect(store.get().folder?.error).toBe("No latest version.");
  });

  it("offers no button when it isn't clear which game version to use", async () => {
    const { root } = await setup(noAddon([`${WOW}/_classic_`, `${WOW}/_retail_`]));
    expect([...root.querySelectorAll("button")].map((b) => b.textContent)).not.toContain("Install the add-on");
  });
});

describe("Settings versions line", () => {
  it("names the add-on version beside the helper's, once it is known", () => {
    const line = (addon?: string) => versionsSection("0.2.1", undefined, false, () => {}, addon).querySelector(".small")?.textContent;
    expect(line()).toBe("Helper 0.2.1");
    expect(line("1.3.2")).toBe("Helper 0.2.1 · Add-on 1.3.2");
  });
});
