import { describe, expect, it } from "vitest";
import { manifest, updateFiles } from "./manifest.ts";

const v = "1.2.0";
const name = (suffix: string) => `bureaucracy-forever-helper_${v}_${suffix}`;
const built = () =>
  new Map<string, string>(
    ["macos-apple-silicon.app.tar.gz", "macos-intel.app.tar.gz", "windows-x64-setup.exe", "windows-x64.msi"].flatMap(
      (suffix) => [
        [name(suffix), ""],
        [`${name(suffix)}.sig`, `sig-${suffix}\n`],
      ],
    ),
  );
const date = new Date("2026-10-16T12:00:00Z");

describe("manifest", () => {
  it("lists every updater target with its signature and its download URL once published", () => {
    const json = manifest({ repo: "o/r", version: v, notes: "## New\n- A\n", date, files: built() }) as {
      platforms: Record<string, { signature: string; url: string }>;
    };
    expect(json).toMatchObject({ version: v, notes: "## New\n- A\n", pub_date: "2026-10-16T12:00:00.000Z" });
    expect(Object.keys(json.platforms).sort()).toEqual([
      "darwin-aarch64",
      "darwin-aarch64-app",
      "darwin-x86_64",
      "darwin-x86_64-app",
      "windows-x86_64",
      "windows-x86_64-msi",
      "windows-x86_64-nsis",
    ]);
    expect(json.platforms["darwin-aarch64"]).toEqual({
      signature: "sig-macos-apple-silicon.app.tar.gz",
      url: `https://github.com/o/r/releases/download/v${v}/${name("macos-apple-silicon.app.tar.gz")}`,
    });
    expect(json.platforms["windows-x86_64"]).toEqual(json.platforms["windows-x86_64-nsis"]);
    expect(json.platforms["windows-x86_64-msi"].signature).toBe("sig-windows-x64.msi");
  });

  it("names every update file with the version", () => {
    for (const file of Object.values(updateFiles(v))) expect(file).toContain(`_${v}_`);
  });

  it("fails when an update file is missing", () => {
    const files = built();
    files.delete(name("macos-intel.app.tar.gz"));
    expect(() => manifest({ repo: "o/r", version: v, notes: "", date, files })).toThrow("missing update file");
  });

  it("fails when a signature is missing or empty", () => {
    const files = built();
    files.set(`${name("windows-x64.msi")}.sig`, " \n");
    expect(() => manifest({ repo: "o/r", version: v, notes: "", date, files })).toThrow("missing signature");
  });
});
