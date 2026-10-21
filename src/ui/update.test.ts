// @vitest-environment happy-dom
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Bridge, CheckOutcome, InstallOutcome, RestartOutcome, UpdateFailure, UpdateProgress } from "../bridge";
import { startWindow } from "./realWindow";

const found: CheckOutcome = { kind: "available", version: "0.3.0", notesUrl: "https://example.test/v0.3.0" };
const flush = () => new Promise((r) => setTimeout(r, 0));

/** A connected helper on 0.2.1 whose update engine answers as given. */
function fakeEngine(engine: { check?: () => Promise<CheckOutcome>; install?: () => Promise<InstallOutcome>; restart?: () => Promise<RestartOutcome> } = {}) {
  const unused = () => Promise.reject(new Error("not used"));
  const heard: { progress?: (p: UpdateProgress) => void; tray?: () => void } = {};
  const bridge: Bridge = {
    connect: unused,
    checkStored: () =>
      Promise.resolve({ kind: "accepted", officer: "Aldric", env: "production", formatVersions: [1], addonVersion: "1.3.2", addonUrl: "" }),
    removeKey: unused,
    staging: unused,
    setStaging: unused,
    syncStatus: unused,
    onSyncStatus: unused,
    raids: unused,
    onRaids: unused,
    raidChoice: unused,
    onRaidChoice: unused,
    reserves: unused,
    onReserves: unused,
    onChatLog: unused,
    searchFolder: unused,
    checkPickedFolder: unused,
    rememberVersion: unused,
    rememberedFolder: () =>
      Promise.resolve({ kind: "found", gameFolder: "/wow", version: "_classic_", versionFolder: "/wow/_classic_", addonFolder: "" }),
    pickFolder: unused,
    checkForUpdate: vi.fn(engine.check ?? (() => Promise.resolve(found))),
    installUpdate: vi.fn(engine.install ?? (() => new Promise<InstallOutcome>(() => {}))),
    restartAfterUpdate: vi.fn(engine.restart ?? (() => new Promise<RestartOutcome>(() => {}))),
    onUpdateProgress: (listener) => {
      heard.progress = listener;
      return Promise.resolve(() => {});
    },
    openUpdateNotes: vi.fn(() => Promise.resolve()),
    onTrayCheck: (listener) => {
      heard.tray = listener;
      return Promise.resolve(() => {});
    },
    helperVersion: () => Promise.resolve("0.2.1"),
    checkAddon: unused,
    installAddon: unused,
    onAddonProgress: unused,
    openAddonNotes: unused,
    openLootLog: unused,
    setTrayStatus: () => Promise.resolve(),
    notify: vi.fn(() => Promise.resolve()),
  };
  return { bridge, heard };
}

async function open(bridge: Bridge) {
  const root = document.createElement("div");
  await startWindow(root, bridge);
  await flush();
  return root;
}

const card = (root: HTMLElement) => root.querySelector<HTMLElement>(".card");
const press = (root: HTMLElement, label: string) =>
  [...root.querySelectorAll<HTMLElement>("button, a")].find((b) => b.textContent === label)!.click();
const activity = (root: HTMLElement) => root.querySelector(".acts li")?.textContent ?? "";

beforeEach(() => localStorage.clear());

describe("the helper update card", () => {
  it("shows a found update with Install and restart, Later and What’s new, under the chain", async () => {
    const { bridge } = fakeEngine();
    const root = await open(bridge);
    expect(card(root)?.textContent).toContain("Helper 0.3.0 is ready to install");
    expect(card(root)?.textContent).toContain("It installs in a few seconds and the helper restarts.");
    const labels = [...card(root)!.querySelectorAll("button, a")].map((b) => b.textContent);
    expect(labels).toEqual(["Install and restart", "Later", "What’s new"]);
    expect(card(root)?.previousElementSibling?.classList.contains("chain")).toBe(true);
  });

  it("installs nothing until the officer presses Install and restart", async () => {
    const { bridge } = fakeEngine();
    const root = await open(bridge);
    expect(bridge.installUpdate).not.toHaveBeenCalled();
    expect(bridge.restartAfterUpdate).not.toHaveBeenCalled();
    press(root, "Install and restart");
    expect(bridge.installUpdate).toHaveBeenCalledTimes(1);
  });

  it("shows the download with its progress while installing, and restarts once it’s in place", async () => {
    let finish: (o: InstallOutcome) => void = () => {};
    const { bridge, heard } = fakeEngine({ install: () => new Promise((r) => (finish = r)) });
    const root = await open(bridge);
    press(root, "Install and restart");
    expect(card(root)?.textContent).toContain("Downloading 0.3.0…");
    expect(card(root)?.textContent).toContain("The helper restarts when it’s done. Awards keep sending until then.");
    heard.progress?.({ downloaded: 45, total: 100 });
    const bar = root.querySelector(".track-bar")!;
    expect(bar.getAttribute("aria-valuenow")).toBe("45");
    expect(card(root)?.textContent).toContain("45%");
    heard.progress?.({ downloaded: 50, total: null });
    expect(card(root)?.textContent).not.toContain("%");
    expect(bridge.restartAfterUpdate).not.toHaveBeenCalled();
    finish({ kind: "ready", version: "0.3.0" });
    await flush();
    expect(bridge.restartAfterUpdate).toHaveBeenCalledTimes(1);
  });

  it.each<[UpdateFailure, string, string]>([
    ["offline", "Couldn’t download the update. Check your connection.", "Couldn’t download the update. Still on 0.2.1."],
    ["missing", "The update isn’t available right now. Try again later.", "The update isn’t available right now. Still on 0.2.1."],
    [
      "badSignature",
      "The update file didn’t pass its safety check, so nothing was installed.",
      "The file didn’t pass its safety check. Still on 0.2.1.",
    ],
    [
      "interrupted",
      "The download stopped before it finished. Nothing was installed.",
      "The download stopped before it finished. Still on 0.2.1.",
    ],
  ])("says a %s failure in plain words, in the card and in Activity", async (reason, words, line) => {
    const { bridge } = fakeEngine({ install: () => Promise.resolve({ kind: "failed", reason, message: "engine words" }) });
    const root = await open(bridge);
    press(root, "Install and restart");
    await flush();
    const text = card(root)?.textContent ?? "";
    expect(card(root)?.classList.contains("tone-warn")).toBe(true);
    expect(text).toContain("The update couldn’t be installed");
    expect(text).toContain(`${words} You’re still on 0.2.1.`);
    expect(text).toContain("Try again");
    expect(text).toContain("Other reasons it can fail");
    expect(root.querySelectorAll(".card details li")).toHaveLength(3);
    expect(root.querySelector(".card details")?.textContent).not.toContain(words);
    expect(activity(root)).toContain(`Update not installed: ${line}`);
    expect(bridge.restartAfterUpdate).not.toHaveBeenCalled();
  });

  it("tries the install again from the failed card", async () => {
    const { bridge } = fakeEngine({ install: () => Promise.resolve({ kind: "failed", reason: "offline", message: "" }) });
    const root = await open(bridge);
    press(root, "Install and restart");
    await flush();
    press(root, "Try again");
    expect(bridge.installUpdate).toHaveBeenCalledTimes(2);
  });

  it("shows a restart that didn’t happen as a failure, with the installed version still working", async () => {
    const { bridge } = fakeEngine({
      install: () => Promise.resolve({ kind: "ready", version: "0.3.0" }),
      restart: () => Promise.resolve({ kind: "failed", reason: "installFailed", message: "" }),
    });
    const root = await open(bridge);
    press(root, "Install and restart");
    await flush();
    expect(card(root)?.textContent).toContain("The update couldn’t be put in place, so nothing was changed. You’re still on 0.2.1.");
    expect(activity(root)).toContain("Update not installed");
  });

  it("puts the update off with Later, and the helper’s own checks don’t bring it back", async () => {
    vi.useFakeTimers({ toFake: ["setInterval"] });
    try {
      const { bridge } = fakeEngine();
      const root = await open(bridge);
      press(root, "Later");
      expect(card(root)).toBeNull();
      vi.advanceTimersByTime(6 * 60 * 60 * 1000);
      await flush();
      expect(bridge.checkForUpdate).toHaveBeenCalledTimes(2);
      expect(card(root)).toBeNull();
    } finally {
      vi.useRealTimers();
    }
  });

  it("opens What’s new for the offered version", async () => {
    const { bridge } = fakeEngine();
    const root = await open(bridge);
    press(root, "What’s new");
    expect(bridge.openUpdateNotes).toHaveBeenCalledTimes(1);
  });

  it("raises one notification per found version, and none when nothing is found", async () => {
    const { bridge, heard } = fakeEngine();
    await open(bridge);
    heard.tray?.();
    await flush();
    expect(bridge.checkForUpdate).toHaveBeenCalledTimes(2);
    expect(bridge.notify).toHaveBeenCalledTimes(1);
    expect(bridge.notify).toHaveBeenCalledWith({ headline: "Helper 0.3.0 is ready to install", detail: "Open the helper to install it." });
    const quiet = fakeEngine({ check: () => Promise.resolve({ kind: "upToDate" }) });
    localStorage.clear();
    await open(quiet.bridge);
    expect(quiet.bridge.notify).not.toHaveBeenCalled();
  });

  it("stays quiet when the helper’s own check fails", async () => {
    const { bridge } = fakeEngine({ check: () => Promise.resolve({ kind: "failed", reason: "missing", message: "" }) });
    const root = await open(bridge);
    expect(card(root)).toBeNull();
    expect(activity(root)).toBe("");
    expect(bridge.notify).not.toHaveBeenCalled();
  });
});

describe("Check for updates", () => {
  const versions = (root: HTMLElement) => [...root.querySelectorAll("section")].find((s) => s.textContent?.startsWith("Versions"));

  it.each<[CheckOutcome, string]>([
    [{ kind: "upToDate" }, "You have the latest version."],
    [found, "Helper 0.3.0 is ready to install. Press Done to see it."],
    [{ kind: "failed", reason: "offline", message: "" }, "Couldn’t check for updates. Check your connection."],
    [{ kind: "failed", reason: "missing", message: "" }, "The update isn’t available right now. Try again later."],
    [{ kind: "failed", reason: "checkFailed", message: "" }, "Couldn’t check for updates. Try again later."],
  ])("in Settings runs the check and says how it went: %j", async (outcome, note) => {
    const { bridge } = fakeEngine({ check: () => Promise.resolve(outcome) });
    const root = await open(bridge);
    root.querySelector<HTMLButtonElement>("[aria-label=Settings]")!.click();
    expect(versions(root)?.textContent).toContain("Helper 0.2.1");
    press(root, "Check for updates");
    expect(versions(root)?.textContent).toContain("Checking…");
    await flush();
    expect(bridge.checkForUpdate).toHaveBeenCalledTimes(2);
    expect(versions(root)?.querySelector("[role=status]")?.textContent).toBe(note);
  });

  it("in the tray opens Settings and runs the same check", async () => {
    const { bridge, heard } = fakeEngine({ check: () => Promise.resolve({ kind: "upToDate" }) });
    const root = await open(bridge);
    heard.tray?.();
    await flush();
    expect(bridge.checkForUpdate).toHaveBeenCalledTimes(2);
    expect(root.querySelector("h1")?.textContent).toBe("Settings");
    expect(versions(root)?.textContent).toContain("You have the latest version.");
  });
});

describe("checks that overlap", () => {
  it("share one answer, so a later one can’t undo what an earlier one found", async () => {
    const answers: ((o: CheckOutcome) => void)[] = [];
    const { bridge } = fakeEngine({ check: () => new Promise((r) => answers.push(r)) });
    const root = await open(bridge);
    root.querySelector<HTMLButtonElement>("[aria-label=Settings]")!.click();
    press(root, "Check for updates");
    expect(bridge.checkForUpdate).toHaveBeenCalledTimes(1);
    answers[0]?.(found);
    await flush();
    root.querySelector<HTMLElement>(".back")!.click();
    expect(card(root)?.textContent).toContain("Helper 0.3.0 is ready to install");
    expect(bridge.notify).toHaveBeenCalledTimes(1);
  });
});

describe("during first-run setup", () => {
  it("shows the update card under the setup chain", async () => {
    const { bridge } = fakeEngine();
    bridge.checkStored = () => Promise.resolve({ kind: "noKey" });
    const root = await open(bridge);
    expect(root.querySelector("h1")?.textContent).toBe("Connect the helper");
    expect(card(root)?.textContent).toContain("Helper 0.3.0 is ready to install");
    press(root, "Install and restart");
    expect(bridge.installUpdate).toHaveBeenCalledTimes(1);
  });
});
