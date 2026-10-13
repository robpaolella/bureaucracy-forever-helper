import { describe, expect, it } from "vitest";
import { itemsSince, lastPublishedTag, type Fetch } from "./github.ts";

// Recorded GitHub responses, keyed by request path.
function recorded(responses: Record<string, { status?: number; body: unknown }>) {
  const calls: { method?: string; path: string; auth?: string }[] = [];
  const fetcher: Fetch = async (url, init) => {
    const path = String(url).replace("https://api.github.com", "");
    const headers = init?.headers as Record<string, string>;
    calls.push({ method: init?.method, path, auth: headers.Authorization });
    const hit = responses[path];
    if (!hit) throw new Error(`unrecorded request ${path}`);
    return new Response(JSON.stringify(hit.body), { status: hit.status ?? 200 });
  };
  return { fetcher, calls };
}

const commit = (sha: string, message: string, parents = 1) => ({
  sha,
  commit: { message },
  parents: Array.from({ length: parents }, () => ({})),
});

describe("lastPublishedTag", () => {
  it("returns the latest release's tag", async () => {
    const { fetcher } = recorded({ "/repos/o/r/releases/latest": { body: { tag_name: "v1.2.3" } } });
    expect(await lastPublishedTag(fetcher, "t", "o/r")).toBe("v1.2.3");
  });
  it("returns undefined when nothing is published", async () => {
    const { fetcher } = recorded({ "/repos/o/r/releases/latest": { status: 404, body: {} } });
    expect(await lastPublishedTag(fetcher, "t", "o/r")).toBeUndefined();
  });
  it("fails on any other error", async () => {
    const { fetcher } = recorded({ "/repos/o/r/releases/latest": { status: 403, body: {} } });
    await expect(lastPublishedTag(fetcher, "t", "o/r")).rejects.toThrow("403");
  });
});

describe("itemsSince", () => {
  it("turns non-merge commits into merged PRs or bare titles, using only GET with the token", async () => {
    const { fetcher, calls } = recorded({
      "/repos/o/r/compare/v1.0.0...abc?per_page=100&page=1": {
        body: { commits: [commit("c1", "feat: a (#5)\n\nbody"), commit("c2", "Merge pull request #6", 2), commit("c3", "fix: b")] },
      },
      "/repos/o/r/commits/c1/pulls": {
        body: [
          { number: 5, title: "feat: a", body: "Release note: A.", merged_at: "2026-10-01T00:00:00Z" },
          { number: 9, title: "wip", body: null, merged_at: null },
        ],
      },
      "/repos/o/r/commits/c3/pulls": { body: [] },
    });
    expect(await itemsSince(fetcher, "secret", "o/r", "v1.0.0", "abc")).toEqual([
      { number: 5, title: "feat: a", body: "Release note: A." },
      { title: "fix: b" },
    ]);
    expect(calls.every((call) => call.method === "GET" && call.auth === "Bearer secret")).toBe(true);
  });
  it("lists the whole history when nothing is published", async () => {
    const { fetcher } = recorded({
      "/repos/o/r/commits?sha=abc&per_page=100&page=1": { body: [commit("c1", "feat: a")] },
      "/repos/o/r/commits/c1/pulls": { body: [] },
    });
    expect(await itemsSince(fetcher, "t", "o/r", undefined, "abc")).toEqual([{ title: "feat: a" }]);
  });
});
