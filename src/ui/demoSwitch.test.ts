import { build } from "vite";
import type { Rollup } from "vite";
import { afterEach, describe, expect, it, vi } from "vitest";
import { demoRequested } from "./demoSwitch";

describe("demoRequested", () => {
  it("is true only for ?demo", () => {
    expect(demoRequested("?demo")).toBe(true);
    expect(demoRequested("")).toBe(false);
    expect(demoRequested("?other=1")).toBe(false);
  });
});

describe("release build", () => {
  afterEach(() => vi.unstubAllEnvs());

  it("carries no demo page", async () => {
    // Vitest sets NODE_ENV=test, which Vite would treat as a dev build.
    vi.stubEnv("NODE_ENV", "production");
    const result = (await build({
      logLevel: "silent",
      mode: "production",
      build: { write: false, minify: false },
    })) as Rollup.RollupOutput;
    const code = result.output
      .map((o) => (o.type === "chunk" ? o.code : ""))
      .join("\n");
    expect(code).toContain("Bureaucracy Forever Helper");
    expect(code).not.toContain("Shared pieces");
    expect(result.output.some((o) => o.fileName.includes("demo"))).toBe(false);
  });
});
