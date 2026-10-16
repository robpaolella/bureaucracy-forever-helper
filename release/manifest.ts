// Builds latest.json, the updater's manifest, from the signed update files a release build
// made. The URLs point where GitHub serves the files once Robert publishes the draft.
// Usage: node release/manifest.ts <owner/repo> <version> <notes-file> <dir>  (writes <dir>/latest.json)
import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

export const PREFIX = "bureaucracy-forever-helper";

// The update file for each updater target, by the name the release build gives it. The
// updater looks for `{os}-{arch}-{installer}`, then `{os}-{arch}`; the bare Windows entry is
// the NSIS (setup.exe) one, the installer officers get by default.
export function updateFiles(version: string): Record<string, string> {
  const name = (suffix: string) => `${PREFIX}_${version}_${suffix}`;
  const appleSilicon = name("macos-apple-silicon.app.tar.gz");
  const intel = name("macos-intel.app.tar.gz");
  const nsis = name("windows-x64-setup.exe");
  return {
    "darwin-aarch64": appleSilicon,
    "darwin-aarch64-app": appleSilicon,
    "darwin-x86_64": intel,
    "darwin-x86_64-app": intel,
    "windows-x86_64": nsis,
    "windows-x86_64-nsis": nsis,
    "windows-x86_64-msi": name("windows-x64.msi"),
  };
}

// `files` maps a file name to its content; only the `.sig` files are read. Throws when an
// update file or its signature is missing, so a broken build never makes a draft.
export function manifest(input: {
  repo: string;
  version: string;
  notes: string;
  date: Date;
  files: Map<string, string>;
}): object {
  const platforms: Record<string, { signature: string; url: string }> = {};
  for (const [target, file] of Object.entries(updateFiles(input.version))) {
    if (!input.files.has(file)) throw new Error(`missing update file ${file}`);
    const signature = input.files.get(`${file}.sig`)?.trim();
    if (!signature) throw new Error(`missing signature ${file}.sig`);
    const url = `https://github.com/${input.repo}/releases/download/v${input.version}/${file}`;
    platforms[target] = { signature, url };
  }
  return { version: input.version, notes: input.notes, pub_date: input.date.toISOString(), platforms };
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? "").href) {
  const [repo, version, notesFile, dir] = process.argv.slice(2);
  if (!repo || !version || !notesFile || !dir) {
    console.error("Usage: node release/manifest.ts <owner/repo> <version> <notes-file> <dir>");
    process.exit(2);
  }
  const files = new Map<string, string>();
  for (const file of readdirSync(dir)) {
    files.set(file, file.endsWith(".sig") ? readFileSync(join(dir, file), "utf8") : "");
  }
  const notes = readFileSync(notesFile, "utf8");
  const json = manifest({ repo, version, notes, date: new Date(), files });
  writeFileSync(join(dir, "latest.json"), `${JSON.stringify(json, null, 2)}\n`);
  console.log(`Wrote latest.json for ${version}.`);
}
