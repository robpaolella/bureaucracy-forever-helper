import { describe, expect, it, vi } from "vitest";
import { check, create, githubApi, parsePlan, type Api, type Draft } from "./draft.ts";
import type { Fetch } from "./github.ts";

// The scripts narrate for the workflow log; keep the test output quiet.
vi.spyOn(console, "log").mockImplementation(() => {});

const HEAD = "a".repeat(40);
const OLDER = "b".repeat(40);
const NEWER = "c".repeat(40);
const files = [
  { name: "x.dmg", data: new Uint8Array([1]) },
  { name: "latest.json", data: new Uint8Array([2]) },
];

// A fake GitHub: `ancestors` holds "older>newer" pairs; every call is recorded.
function fake(state: {
  drafts?: Draft[];
  tags?: string[];
  ancestors?: string[];
  published?: number[];
  gone?: number[];
  publishedOnDelete?: number[];
  failUpload?: boolean;
}) {
  const calls: string[] = [];
  const tags = new Set(state.tags ?? []);
  const api: Api = {
    drafts: async () => state.drafts ?? [],
    tagExists: async (tag) => tags.has(tag),
    isAncestor: async (older, newer) => (state.ancestors ?? []).includes(`${older}>${newer}`),
    releaseState: async (id) =>
      state.gone?.includes(id) ? "gone" : state.published?.includes(id) ? "published" : "draft",
    createDraft: async (tag, target) => {
      calls.push(`create ${tag} ${target}`);
      return 99;
    },
    upload: async (id, name) => {
      if (state.failUpload) throw new Error("upload failed");
      calls.push(`upload ${id} ${name}`);
    },
    remove: async (id) => {
      calls.push(`remove ${id}`);
      const draft = state.drafts?.find((d) => d.id === id);
      if (draft && state.publishedOnDelete?.includes(id)) tags.add(draft.tag);
    },
  };
  return { api, calls };
}

const input = { version: "1.2.0", notes: "## New\n- A\n", head: HEAD, files };

describe("parsePlan", () => {
  it("reads none as no release", () => expect(parsePlan("none\n")).toBeUndefined());
  it("reads the version and the notes", () =>
    expect(parsePlan("1.2.0\n\n## New\n- A\n")).toEqual({ version: "1.2.0", notes: "## New\n- A\n" }));
  it("refuses anything that isn't a plain version", () => expect(() => parsePlan("1.2\n\nx")).toThrow("not a version"));
});

describe("check", () => {
  it("builds nothing when no release is due", async () => {
    expect(await check(fake({}).api, undefined)).toBe(false);
  });
  it("builds when a release is due and its tag is free", async () => {
    expect(await check(fake({}).api, input)).toBe(true);
  });
  it("fails when the tag is already taken", async () => {
    await expect(check(fake({ tags: ["v1.2.0"] }).api, input)).rejects.toThrow("v1.2.0 is already taken");
  });
});

describe("create", () => {
  it("makes a draft on the built commit holding every file", async () => {
    const { api, calls } = fake({});
    expect(await create(api, input)).toBe("created");
    expect(calls).toEqual(["create v1.2.0 " + HEAD, "upload 99 x.dmg", "upload 99 latest.json"]);
  });

  it("refreshes a re-run for the same version, leaving one draft", async () => {
    const { api, calls } = fake({ drafts: [{ id: 1, tag: "v1.2.0", target: HEAD }] });
    expect(await create(api, input)).toBe("created");
    expect(calls.filter((c) => c.startsWith("create"))).toHaveLength(1);
    expect(calls.at(-1)).toBe("remove 1");
  });

  it("replaces an older draft, including one aimed at a branch name", async () => {
    const drafts = [
      { id: 1, tag: "v1.1.1", target: OLDER },
      { id: 2, tag: "v1.2.0", target: "main" },
    ];
    const { api, calls } = fake({ drafts, ancestors: [`${OLDER}>${HEAD}`] });
    expect(await create(api, input)).toBe("created");
    expect(calls.slice(-2)).toEqual(["remove 1", "remove 2"]);
  });

  it("leaves a draft from a newer merge alone", async () => {
    const { api, calls } = fake({ drafts: [{ id: 1, tag: "v1.2.0", target: NEWER }], ancestors: [`${HEAD}>${NEWER}`] });
    expect(await create(api, input)).toBe("newer");
    expect(calls).toEqual([]);
  });

  it("fails without a draft when the tag is taken", async () => {
    const { api, calls } = fake({ tags: ["v1.2.0"] });
    await expect(create(api, input)).rejects.toThrow("already taken");
    expect(calls).toEqual([]);
  });

  it("keeps the old draft when an upload fails", async () => {
    const { api, calls } = fake({ drafts: [{ id: 1, tag: "v1.2.0", target: OLDER }], failUpload: true });
    await expect(create(api, input)).rejects.toThrow("upload failed");
    expect(calls).toEqual(["create v1.2.0 " + HEAD, "remove 99"]);
  });

  it("backs out when Robert published the old draft during the run", async () => {
    const { api, calls } = fake({ drafts: [{ id: 1, tag: "v1.2.0", target: OLDER }], published: [1] });
    expect(await create(api, input)).toBe("published");
    expect(calls).not.toContain("remove 1");
    expect(calls.at(-1)).toBe("remove 99");
  });

  it("skips an old draft deleted by hand during the run", async () => {
    const { api, calls } = fake({ drafts: [{ id: 1, tag: "v1.2.0", target: OLDER }], gone: [1] });
    expect(await create(api, input)).toBe("created");
    expect(calls).not.toContain("remove 1");
  });

  it("fails loudly when a publish lands just before the delete", async () => {
    const { api, calls } = fake({ drafts: [{ id: 1, tag: "v1.1.1", target: OLDER }], publishedOnDelete: [1] });
    await expect(create(api, input)).rejects.toThrow("v1.1.1 was published just as this run replaced its draft");
    expect(calls.slice(-2)).toEqual(["remove 1", "remove 99"]);
  });
});

describe("githubApi", () => {
  function recorded(answer: (method: string, url: string) => { status?: number; body?: unknown }) {
    const calls: { method: string; url: string; body?: unknown; type?: string }[] = [];
    const fetcher: Fetch = async (url, init) => {
      const headers = init?.headers as Record<string, string>;
      const method = init?.method ?? "GET";
      calls.push({ method, url: String(url), body: init?.body, type: headers["Content-Type"] });
      const { status = 200, body = {} } = answer(method, String(url));
      return new Response(status === 204 ? null : JSON.stringify(body), { status });
    };
    return { api: githubApi(fetcher, "t", "o/r"), calls };
  }

  it("creates releases only as drafts on the given commit", async () => {
    const { api, calls } = recorded(() => ({ status: 201, body: { id: 7, draft: true } }));
    expect(await api.createDraft("v1.2.0", HEAD, "notes")).toBe(7);
    expect(calls[0].method).toBe("POST");
    expect(calls[0].url).toBe("https://api.github.com/repos/o/r/releases");
    expect(JSON.parse(String(calls[0].body))).toEqual({
      tag_name: "v1.2.0",
      target_commitish: HEAD,
      name: "v1.2.0",
      body: "notes",
      draft: true,
    });
  });

  it("fails if GitHub didn't make a draft", async () => {
    const { api } = recorded(() => ({ status: 201, body: { id: 7, draft: false } }));
    await expect(api.createDraft("v1.2.0", HEAD, "notes")).rejects.toThrow("did not make v1.2.0 a draft");
  });

  it("uploads files to the release", async () => {
    const { api, calls } = recorded(() => ({ status: 201 }));
    await api.upload(7, "a b.dmg", new Uint8Array([1]));
    expect(calls[0]).toMatchObject({
      method: "POST",
      url: "https://uploads.github.com/repos/o/r/releases/7/assets?name=a%20b.dmg",
      type: "application/octet-stream",
    });
  });

  it("lists only drafts", async () => {
    const { api } = recorded(() => ({
      body: [
        { id: 1, tag_name: "v1.2.0", target_commitish: HEAD, draft: true },
        { id: 2, tag_name: "v1.1.0", target_commitish: "main", draft: false },
      ],
    }));
    expect(await api.drafts()).toEqual([{ id: 1, tag: "v1.2.0", target: HEAD }]);
  });

  it("reads tags, ancestry and release state", async () => {
    const { api } = recorded((_, url) => {
      if (url.endsWith("/git/ref/tags/v1.0.0")) return { body: {} };
      if (url.includes("/git/ref/tags/")) return { status: 404 };
      if (url.includes("/compare/")) return { body: { status: url.includes(`${OLDER}...`) ? "ahead" : "behind" } };
      if (url.endsWith("/releases/1")) return { body: { draft: true } };
      if (url.endsWith("/releases/2")) return { body: { draft: false } };
      return { status: 404 };
    });
    expect(await api.tagExists("v1.0.0")).toBe(true);
    expect(await api.tagExists("v1.2.0")).toBe(false);
    expect(await api.isAncestor(OLDER, HEAD)).toBe(true);
    expect(await api.isAncestor(NEWER, HEAD)).toBe(false);
    expect(await api.releaseState(1)).toBe("draft");
    expect(await api.releaseState(2)).toBe("published");
    expect(await api.releaseState(3)).toBe("gone");
  });

  it("fails on any other error", async () => {
    const { api } = recorded(() => ({ status: 500 }));
    await expect(api.remove(1)).rejects.toThrow("GitHub answered 500 for DELETE /repos/o/r/releases/1");
  });
});
