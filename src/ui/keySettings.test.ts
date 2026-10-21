// @vitest-environment happy-dom
import { describe, expect, it, vi } from "vitest";
import type { Bridge, ChangeOutcome, KeyOutcome, LoginOutcome, SyncStatus } from "../bridge";
import { keySection, stagingSection, storeName } from "./keySettings";
import { startWindow } from "./realWindow";

const accepted: KeyOutcome = {
  kind: "accepted",
  officer: "Aldric Vane",
  env: "production",
  formatVersions: [1],
  addonVersion: "0.1.0",
  addonUrl: "https://example.test/addon",
};
const done: ChangeOutcome = { kind: "done" };
const NEW_KEY = `bfh_${"A".repeat(43)}`;
const flush = () => new Promise((r) => setTimeout(r, 0));

/** A connected helper; each call can be replaced. */
function bridgeWith(over: Partial<Bridge> = {}): Bridge {
  const unused = () => Promise.reject(new Error("not used"));
  return {
    connect: vi.fn(unused),
    checkStored: vi.fn(() => Promise.resolve(accepted)),
    removeKey: vi.fn(() => Promise.resolve(done)),
    staging: () => Promise.resolve(false),
    setStaging: vi.fn(() => Promise.resolve(done)),
    syncStatus: unused,
    clearRefused: unused,
    onWindowVisible: unused, windowVisible: unused, startAtLogin: unused, setStartAtLogin: unused, finishLoginSetup: unused,
    searchFolder: unused,
    checkPickedFolder: unused,
    rememberVersion: unused,
    rememberedFolder: () =>
      Promise.resolve({ kind: "found", gameFolder: "/g", version: "_retail_", versionFolder: "/g/_retail_", addonFolder: "" }),
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
    openOfficerArea: vi.fn(() => Promise.resolve()),
    setTrayStatus: () => Promise.resolve(),
    notify: unused,
    ...over,
  };
}

/** Starts the window and opens Settings. */
async function settings(bridge: Bridge) {
  const root = document.createElement("div");
  document.body.append(root);
  await startWindow(root, bridge);
  root.querySelector<HTMLButtonElement>('button[aria-label="Settings"]')!.click();
  return root;
}
const press = (root: HTMLElement, label: string) =>
  [...root.querySelectorAll("button")].find((b) => b.textContent === label)!.click();
const staging = (root: HTMLElement) => root.querySelector<HTMLButtonElement>("#staging")!;
const band = (root: HTMLElement) => root.querySelector(".staging-band")?.textContent;
const back = (root: HTMLElement) => root.querySelector("header .back");

describe("Settings → Helper key", () => {
  it("shows where the key is kept, that it works, and Replace key and Remove key", async () => {
    const root = await settings(bridgeWith());
    expect(root.textContent).toContain("Saved in Keychain");
    expect(root.textContent).toContain("Working");
    expect(root.textContent).toContain("Replace key");
    expect(root.textContent).toContain("Remove key");
  });

  it("names Credential Manager on Windows", () => {
    expect(storeName("Mozilla/5.0 (Windows NT 10.0; Win64; x64)")).toBe("Credential Manager");
    expect(storeName("Mozilla/5.0 (Macintosh; Intel Mac OS X 14_0)")).toBe("Keychain");
    const section = keySection(true, true, undefined, { replace() {}, remove() {}, add() {}, setStaging() {} }, "Credential Manager");
    expect(section.textContent).toContain("Saved in Credential Manager");
  });

  it("Remove key asks nothing first, deletes the key and goes to the key step, touching nothing else", async () => {
    const bridge = bridgeWith();
    const root = await settings(bridge);
    press(root, "Remove key");
    await flush();
    expect(bridge.removeKey).toHaveBeenCalledTimes(1);
    expect(bridge.connect).not.toHaveBeenCalled();
    expect(bridge.setStaging).not.toHaveBeenCalled();
    expect(root.querySelector("h1")?.textContent).toBe("Connect the helper");
    expect(back(root)).toBeNull();
    expect(root.querySelector("#key")).not.toBeNull();
  });

  it("keeps the key and says so when it can't be removed", async () => {
    const root = await settings(
      bridgeWith({ removeKey: () => Promise.resolve<ChangeOutcome>({ kind: "storeFailed", message: "Keychain said no." }) }),
    );
    press(root, "Remove key");
    await flush();
    expect(root.querySelector("h1")?.textContent).toBe("Settings");
    expect(root.querySelector(".note.err")?.textContent).toBe("Keychain said no.");
    expect(root.textContent).toContain("Saved in Keychain");
  });

  it("Replace key opens the key step with a Back link and keeps the old key", async () => {
    const bridge = bridgeWith();
    const root = await settings(bridge);
    press(root, "Replace key");
    expect(root.querySelector("#key")).not.toBeNull();
    expect(back(root)?.textContent).toBe("Back");
    expect(bridge.removeKey).not.toHaveBeenCalled();
    press(root, "Back");
    expect(root.querySelector("h1")?.textContent).toBe("Settings");
    expect(root.textContent).toContain("Working");
    expect(bridge.checkStored).toHaveBeenCalledTimes(1);
    expect(bridge.connect).not.toHaveBeenCalled();
  });

  it("a bad new key leaves the old one in place", async () => {
    const bridge = bridgeWith({
      connect: vi.fn(() => Promise.resolve<KeyOutcome>({ kind: "refused", reason: "badKey", message: "No longer works." })),
    });
    const root = await settings(bridge);
    press(root, "Replace key");
    root.querySelector<HTMLInputElement>("#key")!.value = NEW_KEY;
    press(root, "Connect");
    await flush();
    expect(root.querySelector("#key-note")?.textContent).toBe("No longer works.");
    expect(back(root)).not.toBeNull();
    expect(bridge.removeKey).not.toHaveBeenCalled();
    press(root, "Back");
    expect(root.textContent).toContain("Saved in Keychain");
    expect(root.textContent).toContain("Working");
  });

  it("an accepted new key replaces the old and returns to Settings", async () => {
    const bridge = bridgeWith({ connect: vi.fn(() => Promise.resolve(accepted)) });
    const root = await settings(bridge);
    press(root, "Replace key");
    root.querySelector<HTMLInputElement>("#key")!.value = NEW_KEY;
    press(root, "Connect");
    await flush();
    expect(bridge.connect).toHaveBeenCalledWith(NEW_KEY);
    expect(root.querySelector("h1")?.textContent).toBe("Settings");
  });
});

describe("Settings → Guild site", () => {
  it("shows the switch off, with no band", async () => {
    const root = await settings(bridgeWith());
    expect(staging(root).getAttribute("aria-checked")).toBe("false");
    expect(band(root)).toBeUndefined();
  });

  it("starts with the band when staging is already on", async () => {
    const root = await settings(bridgeWith({ staging: () => Promise.resolve(true) }));
    expect(band(root)).toBe("Staging site");
    expect(staging(root).getAttribute("aria-checked")).toBe("true");
  });

  it("turning it on checks that site's key and shows the band; turning it off hides the band", async () => {
    const bridge = bridgeWith();
    const root = await settings(bridge);
    staging(root).click();
    await flush();
    expect(bridge.setStaging).toHaveBeenLastCalledWith(true);
    expect(bridge.checkStored).toHaveBeenCalledTimes(2);
    expect(band(root)).toBe("Staging site");
    press(root, "Done");
    expect(band(root)).toBe("Staging site");
    root.querySelector<HTMLButtonElement>('button[aria-label="Settings"]')!.click();
    staging(root).click();
    await flush();
    expect(bridge.setStaging).toHaveBeenLastCalledWith(false);
    expect(bridge.checkStored).toHaveBeenCalledTimes(3);
    expect(band(root)).toBeUndefined();
  });

  it("with no staging key, shows the key step with the band and a way back; production's key is untouched", async () => {
    const check = vi
      .fn<() => Promise<KeyOutcome>>()
      .mockResolvedValueOnce(accepted)
      .mockResolvedValueOnce({ kind: "noKey" })
      .mockResolvedValueOnce(accepted);
    const bridge = bridgeWith({ checkStored: check });
    const root = await settings(bridge);
    staging(root).click();
    await flush();
    expect(root.querySelector("h1")?.textContent).toBe("Connect the helper");
    expect(band(root)).toBe("Staging site");
    press(root, "Back");
    expect(root.querySelector("h1")?.textContent).toBe("Settings");
    expect(band(root)).toBe("Staging site");
    expect(root.textContent).toContain("No key saved for this site yet.");
    staging(root).click();
    await flush();
    expect(bridge.setStaging).toHaveBeenLastCalledWith(false);
    expect(bridge.removeKey).not.toHaveBeenCalled();
    expect(band(root)).toBeUndefined();
    expect(root.textContent).toContain("Working");
  });

  it("a failed staging check keeps the band, shows the error and doesn't turn the switch off", async () => {
    const check = vi
      .fn<() => Promise<KeyOutcome>>()
      .mockResolvedValueOnce(accepted)
      .mockResolvedValueOnce({ kind: "stagingNotOpen", message: "The staging site isn't open yet." });
    const bridge = bridgeWith({ checkStored: check });
    const root = await settings(bridge);
    staging(root).click();
    await flush();
    expect(band(root)).toBe("Staging site");
    expect(root.querySelector("#key-note")?.textContent).toBe("The staging site isn't open yet.");
    expect(bridge.setStaging).toHaveBeenCalledTimes(1);
    expect(bridge.setStaging).toHaveBeenCalledWith(true);
  });

  it("says so and shows no band when the setting can't be saved", async () => {
    const root = await settings(
      bridgeWith({
        setStaging: () => Promise.resolve<ChangeOutcome>({ kind: "settingsFailed", message: "Couldn’t save that." }),
      }),
    );
    staging(root).click();
    await flush();
    expect(band(root)).toBeUndefined();
    expect(root.querySelector(".note.err")?.textContent).toBe("Couldn’t save that.");
    expect(staging(root).getAttribute("aria-checked")).toBe("false");
  });

  it("can't be flipped again while a switch is running", () => {
    const section = stagingSection(false, { busy: true }, { replace() {}, remove() {}, add() {}, setStaging() {} });
    expect(section.querySelector<HTMLButtonElement>("#staging")?.disabled).toBe(true);
  });
});

describe("staging with no staging key", () => {
  it("at start-up shows the key step with the band and a Back link to Settings, where the switch turns staging off", async () => {
    const check = vi.fn<() => Promise<KeyOutcome>>().mockResolvedValueOnce({ kind: "noKey" }).mockResolvedValueOnce(accepted);
    const bridge = bridgeWith({ staging: () => Promise.resolve(true), checkStored: check });
    const root = document.createElement("div");
    document.body.append(root);
    await startWindow(root, bridge);
    expect(root.querySelector("h1")?.textContent).toBe("Connect the helper");
    expect(band(root)).toBe("Staging site");
    press(root, "Back");
    expect(root.querySelector("h1")?.textContent).toBe("Settings");
    staging(root).click();
    await flush();
    expect(bridge.setStaging).toHaveBeenCalledWith(false);
    expect(band(root)).toBeUndefined();
    expect(root.textContent).toContain("Working");
  });

  it("after Remove key on staging, still leaves a way back to Settings", async () => {
    const bridge = bridgeWith({ staging: () => Promise.resolve(true) });
    const root = await settings(bridge);
    press(root, "Remove key");
    await flush();
    expect(root.querySelector("h1")?.textContent).toBe("Connect the helper");
    expect(back(root)).not.toBeNull();
  });
});

describe("the key's state in Settings", () => {
  it("doesn't say Working once the site has refused the key", async () => {
    let push: (s: SyncStatus) => void = () => {};
    const bridge = bridgeWith({
      onSyncStatus: (l) => ((push = l), Promise.resolve(() => {})),
      syncStatus: () => Promise.resolve<SyncStatus>({ kind: "idle", otherSite: 0 }),
    });
    const root = await settings(bridge);
    expect(root.textContent).toContain("Working");
    push({ kind: "keyStopped" });
    root.querySelector<HTMLButtonElement>(".back")!.click();
    root.querySelector<HTMLButtonElement>('button[aria-label="Settings"]')!.click();
    expect(root.textContent).not.toContain("Working");
  });
});

describe("Open the officer area", () => {
  it("is on the key step and asks the Rust side to open the current site's key page", async () => {
    const bridge = bridgeWith({ checkStored: () => Promise.resolve<KeyOutcome>({ kind: "noKey" }) });
    const root = document.createElement("div");
    await startWindow(root, bridge);
    const link = [...root.querySelectorAll("a")].find((a) => a.textContent === "Open the officer area")!;
    link.click();
    expect(bridge.openOfficerArea).toHaveBeenCalledTimes(1);
  });
});

describe("Settings → Start the helper when the computer starts", () => {
  const REFUSAL =
    "Your computer didn’t allow that change, so the helper won’t start by itself. Check your computer’s startup settings and try again.";
  const login = (root: HTMLElement) => root.querySelector<HTMLButtonElement>("#login")!;
  const refused = (on: boolean): LoginOutcome => ({ kind: "refused", message: REFUSAL, on });

  it("shows the switch with the designed label and the system's state, without a hint line", async () => {
    const root = await settings(bridgeWith({ startAtLogin: () => Promise.resolve(true) }));
    expect(root.querySelector('label[for="login"]')?.textContent).toBe("Start the helper when the computer starts");
    expect(login(root).getAttribute("aria-checked")).toBe("true");
    expect(login(root).closest("section")?.querySelector(".caption")).toBeNull();
    expect(login(root).closest("section")?.querySelector("#auto-addon")).not.toBeNull();
  });

  it("is off when the system has it off", async () => {
    const root = await settings(bridgeWith({ startAtLogin: () => Promise.resolve(false) }));
    expect(login(root).getAttribute("aria-checked")).toBe("false");
  });

  it("turning it off and on calls the change, and shows the state the system answers with", async () => {
    const setStartAtLogin = vi.fn((on: boolean) => Promise.resolve<LoginOutcome>({ kind: "done", on }));
    const root = await settings(bridgeWith({ startAtLogin: () => Promise.resolve(true), setStartAtLogin }));
    login(root).click();
    await flush();
    expect(setStartAtLogin).toHaveBeenLastCalledWith(false);
    expect(login(root).getAttribute("aria-checked")).toBe("false");
    login(root).click();
    await flush();
    expect(setStartAtLogin).toHaveBeenLastCalledWith(true);
    expect(login(root).getAttribute("aria-checked")).toBe("true");
    expect(root.textContent).not.toContain(REFUSAL);
  });

  it("a refused change shows the message under the switch and the switch keeps its real state", async () => {
    const root = await settings(
      bridgeWith({ startAtLogin: () => Promise.resolve(false), setStartAtLogin: () => Promise.resolve(refused(false)) }),
    );
    login(root).click();
    await flush();
    expect(login(root).getAttribute("aria-checked")).toBe("false");
    const note = login(root).closest(".toggle")!.nextElementSibling!;
    expect(note.textContent).toBe(REFUSAL);
    expect(note.getAttribute("role")).toBe("alert");
  });

  it("the message goes when a later change works", async () => {
    const setStartAtLogin = vi
      .fn<(on: boolean) => Promise<LoginOutcome>>()
      .mockResolvedValueOnce(refused(false))
      .mockResolvedValueOnce({ kind: "done", on: true });
    const root = await settings(bridgeWith({ startAtLogin: () => Promise.resolve(false), setStartAtLogin }));
    login(root).click();
    await flush();
    login(root).click();
    await flush();
    expect(root.textContent).not.toContain(REFUSAL);
    expect(login(root).getAttribute("aria-checked")).toBe("true");
  });

  it("a call that fails outright gives the same message and the real state", async () => {
    const root = await settings(
      bridgeWith({ startAtLogin: () => Promise.resolve(false), setStartAtLogin: () => Promise.reject(new Error("boom")) }),
    );
    login(root).click();
    await flush();
    expect(root.textContent).toContain(REFUSAL);
    expect(login(root).getAttribute("aria-checked")).toBe("false");
  });

  it("finishing setup asks the helper once the key and game folder are both done, and shows its answer", async () => {
    const finishLoginSetup = vi.fn(() => Promise.resolve<LoginOutcome>({ kind: "done", on: true }));
    const root = await settings(bridgeWith({ startAtLogin: () => Promise.resolve(false), finishLoginSetup }));
    await flush();
    expect(finishLoginSetup).toHaveBeenCalledTimes(1);
    expect(login(root).getAttribute("aria-checked")).toBe("true");
  });

  it("does not ask while the key is not accepted", async () => {
    const finishLoginSetup = vi.fn(() => Promise.resolve<LoginOutcome>({ kind: "done", on: true }));
    await startWindow(
      document.createElement("div"),
      bridgeWith({ checkStored: () => Promise.resolve<KeyOutcome>({ kind: "noKey" }), finishLoginSetup }),
    );
    await flush();
    expect(finishLoginSetup).not.toHaveBeenCalled();
  });
});
