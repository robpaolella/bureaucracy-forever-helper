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
function fakeBridge(check: () => Promise<KeyOutcome>): Bridge {
  const unused = () => Promise.reject(new Error("not used"));
  return { connect: unused, checkStored: vi.fn(check), removeKey: unused, staging: unused, setStaging: unused };
}

const siteRow = (root: HTMLElement) => root.querySelector(".chain li")?.textContent ?? "";

describe("the real window", () => {
  it("shows the chain, not the placeholder, and checks the stored key on start", async () => {
    const root = document.createElement("div");
    const bridge = fakeBridge(() => Promise.resolve({ kind: "noKey" }));
    await startWindow(root, bridge);
    expect(bridge.checkStored).toHaveBeenCalledTimes(1);
    expect(root.querySelectorAll(".chain li")).toHaveLength(5);
    expect(root.textContent).not.toContain("Bureaucracy Forever Helper");
    expect(siteRow(root)).toContain("Not connected");
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

  it.each<KeyOutcome>([
    { kind: "refused", reason: "badKey", message: "m" },
    { kind: "siteOff", message: "m" },
    { kind: "stagingNotOpen", message: "m" },
    { kind: "unreachable", message: "m" },
    { kind: "storeFailed", message: "m" },
    { kind: "noKey" },
  ])("shows the starting chain when the check ends as $kind", async (outcome) => {
    const root = document.createElement("div");
    await startWindow(root, fakeBridge(() => Promise.resolve(outcome)));
    expect(siteRow(root)).toContain("No helper key yet");
    expect(root.querySelectorAll(".chain li")).toHaveLength(5);
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
