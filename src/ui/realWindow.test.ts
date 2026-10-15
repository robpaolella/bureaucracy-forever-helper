// @vitest-environment happy-dom
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it, vi } from "vitest";
import type { Bridge, KeyOutcome } from "../bridge";
import { createStore } from "../state";
import { mountApp } from "./app";
import { initialState, startWindow } from "./realWindow";

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
    searchFolder: unused,
    checkPickedFolder: unused,
    rememberVersion: unused,
    rememberedFolder: unused,
    pickFolder: unused,
    onSyncStatus: unused,
    raids: unused,
    onRaids: unused,
    checkForUpdate: unused,
    installUpdate: unused,
    restartAfterUpdate: unused,
    onUpdateProgress: unused,
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

  it("asks the site again, without a new paste, when a saved key couldn't be checked", async () => {
    const answers: KeyOutcome[] = [{ kind: "unreachable", message: "offline" }, accepted];
    const bridge = fakeBridge(() => Promise.resolve(answers.shift()!));
    const root = document.createElement("div");
    await startWindow(root, bridge);
    expect(note(root)?.textContent).toBe("offline");
    button(root).dispatchEvent(new MouseEvent("click"));
    await flush();
    expect(bridge.checkStored).toHaveBeenCalledTimes(2);
    expect(bridge.connect).not.toHaveBeenCalled();
    expect(root.querySelector(".chain li")?.textContent).toContain("Connected as Aldric Vane");
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
