// Guards on the release workflow's safety rules, read from the files themselves: CI can't
// publish, only the draft job can write, and only the signed build step sees the signing key.
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const read = (path: string) => readFileSync(new URL(`../${path}`, import.meta.url), "utf8");
// Comments dropped: they describe the rules, so they would match the checks.
const code = (text: string) => text.replace(/^\s*#.*\n/gm, "");
const release = code(read(".github/workflows/release.yml"));
const ci = code(read(".github/workflows/ci.yml"));

// The text of each job in release.yml, by its id.
function jobs(text: string): Record<string, string> {
  const body = text.slice(text.indexOf("\njobs:\n"));
  const parts = body.split(/^ {2}([\w-]+):\n/m).slice(1);
  const out: Record<string, string> = {};
  for (let i = 0; i < parts.length; i += 2) out[parts[i]] = parts[i + 1];
  return out;
}

// The steps of a job, each as its text.
const steps = (job: string) => job.split(/^ {6}- /m).slice(1);

describe("release workflow", () => {
  it("can't publish a release or push a tag", () => {
    const all = [release, ci, read("release/draft.ts"), read("release/manifest.ts"), read("release/cli.ts")].join("\n");
    for (const banned of [/draft"?:\s*false/, /gh release/, /git push/, /git tag/, /PATCH/, /make_latest/, /prerelease/]) {
      expect(all).not.toMatch(banned);
    }
    expect(read("release/draft.ts")).toContain("draft: true");
  });

  it("runs only on pushes to main", () => {
    expect(release).toMatch(/^on:\n {2}push:\n {4}branches: \[main\]\n\n/m);
    expect(release).not.toMatch(/pull_request|workflow_dispatch|workflow_run/);
  });

  it("gives write permission to the draft job only", () => {
    expect(release).toMatch(/^permissions: \{\}$/m);
    const all = jobs(release);
    expect(Object.keys(all)).toEqual(["plan", "build", "draft"]);
    for (const [id, job] of Object.entries(all)) {
      expect(job.includes("contents: write"), id).toBe(id === "draft");
      expect(job).not.toMatch(/: write$(?<!contents: write)/m);
    }
  });

  it("passes the signing key to the signed build step only, and never prints it", () => {
    const withSecrets = Object.values(jobs(release))
      .flatMap(steps)
      .filter((step) => step.includes("secrets."));
    expect(withSecrets).toHaveLength(1);
    expect(withSecrets[0]).toMatch(/^name: Build signed installer and update files\n/);
    expect(withSecrets[0]).toContain("TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}");
    expect(withSecrets[0]).not.toMatch(/echo|printenv|set -x|::add-mask::/);
    expect(jobs(release).draft).not.toContain("TAURI_SIGNING");
  });

  it("makes update files in the release build only", () => {
    expect(steps(jobs(release).build).filter((step) => step.includes("createUpdaterArtifacts"))).toHaveLength(1);
    expect(ci).not.toMatch(/createUpdaterArtifacts|TAURI_SIGNING|secrets\./);
    expect(read("src-tauri/tauri.conf.json")).not.toContain("createUpdaterArtifacts");
  });

  it("never installs or builds anything in the draft job", () => {
    expect(jobs(release).draft).not.toMatch(/npm (ci|install)|tauri build|cargo/);
  });
});
