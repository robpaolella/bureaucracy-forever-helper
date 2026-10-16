// Creates or refreshes the one draft release after a merge to main. It never publishes: every
// release it makes is a draft, and only Robert's Publish click on GitHub creates the tag.
// Usage (token in GITHUB_TOKEN):
//   node release/draft.ts check <owner/repo> <plan-file>
//     Read-only. <plan-file> is release/cli.ts's output. Appends build, version and notes
//     to $GITHUB_OUTPUT.
//   node release/draft.ts create <owner/repo> <head-sha> <version> <notes-file> <dir>
//     Needs write access. Makes a draft for <version> on <head-sha> holding every file in
//     <dir>, then deletes any older draft.
import { randomUUID } from "node:crypto";
import { appendFileSync, readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import type { Fetch } from "./github.ts";

export interface Draft {
  id: number;
  tag: string;
  target: string;
  notes: string;
}

export interface File {
  name: string;
  data: Uint8Array<ArrayBuffer>;
}

export interface Api {
  drafts(): Promise<Draft[]>;
  tagExists(tag: string): Promise<boolean>;
  isAncestor(older: string, newer: string): Promise<boolean>;
  releaseState(id: number): Promise<"draft" | "published" | "gone">;
  files(id: number): Promise<File[]>;
  createDraft(tag: string, target: string, notes: string): Promise<number>;
  upload(id: number, name: string, data: Uint8Array<ArrayBuffer>): Promise<void>;
  remove(id: number): Promise<void>;
}

export interface Planned {
  version: string;
  notes: string;
}

// release/cli.ts's output: "none", or the version, a blank line, then the notes.
export function parsePlan(text: string): Planned | undefined {
  if (text.trim() === "none") return undefined;
  const [version, ...rest] = text.split("\n");
  if (!/^\d+\.\d+\.\d+$/.test(version)) throw new Error(`not a version: ${version}`);
  return { version, notes: rest.join("\n").trim() + "\n" };
}

const isSha = (text: string) => /^[0-9a-f]{40}$/.test(text);

function takenError(tag: string): Error {
  return new Error(`${tag} is already taken. Tags are never reused: publish a newer release instead.`);
}

// Read-only, before the builds: is a draft due? A taken tag fails the run.
export async function check(api: Api, planned: Planned | undefined): Promise<boolean> {
  if (!planned) {
    console.log("No release due: any draft is left as it is.");
    return false;
  }
  if (await api.tagExists(`v${planned.version}`)) throw takenError(`v${planned.version}`);
  console.log(`Next version: ${planned.version}\n\n${planned.notes}`);
  return true;
}

export type Outcome = "created" | "newer" | "published";

// Makes the new draft first, so a failed upload leaves the old draft in place, then deletes
// every older draft so there is only ever one.
export async function create(
  api: Api,
  input: Planned & { head: string; files: File[] },
): Promise<Outcome> {
  const tag = `v${input.version}`;
  if (await api.tagExists(tag)) throw takenError(tag);
  const old = await api.drafts();
  for (const draft of old) {
    // A re-run of an older merge must not move the draft back.
    if (isSha(draft.target) && draft.target !== input.head && (await api.isAncestor(input.head, draft.target))) {
      console.log("The draft is from a newer merge: left as it is.");
      return "newer";
    }
  }

  const id = await draftWith(api, tag, input.head, input.notes, input.files);

  for (const draft of old) {
    // Robert may have published it since the list: look again right before deleting.
    const state = await api.releaseState(draft.id);
    if (state === "gone") continue;
    if (state === "published") {
      await api.remove(id);
      console.log(`${draft.tag} was published during this run: the next merge drafts the release after it.`);
      return "published";
    }
    // GitHub can't delete only if still a draft, so Robert could publish it in the moment
    // before the delete. Publishing makes the tag and deleting the release keeps it, so that
    // shows up afterwards; its files and notes, saved first, put it back as a draft.
    const saved = await api.files(draft.id);
    await api.remove(draft.id);
    if (await api.tagExists(draft.tag)) {
      await api.remove(id);
      try {
        await draftWith(api, draft.tag, draft.target, draft.notes, saved);
      } catch (error) {
        throw new Error(
          `${draft.tag} was published just as this run replaced its draft, so the release was deleted, ` +
            `and putting it back as a draft failed (${error}). Its tag is kept: draft a release on the tag ` +
            `${draft.tag} on GitHub with the files from the workflow run that built it, and publish it.`,
          { cause: error },
        );
      }
      throw new Error(
        `${draft.tag} was published just as this run replaced its draft. It is back as a draft with ` +
          "the same files and notes: publish it again on GitHub.",
      );
    }
  }
  console.log(`Draft ${tag} is ready for Robert to publish.`);
  return "created";
}

// A new draft holding `files`. If an upload fails it is deleted, so no half-filled draft stays.
async function draftWith(api: Api, tag: string, target: string, notes: string, files: File[]): Promise<number> {
  const id = await api.createDraft(tag, target, notes);
  try {
    for (const file of files) await api.upload(id, file.name, file.data);
  } catch (error) {
    await api.remove(id);
    throw error;
  }
  return id;
}

// The GitHub REST API. It never edits a release, so it can't publish one: it only creates
// drafts, uploads to them, reads them and deletes them.
export function githubApi(fetcher: Fetch, token: string, repo: string): Api {
  const headers = {
    Accept: "application/vnd.github+json",
    Authorization: `Bearer ${token}`,
    "X-GitHub-Api-Version": "2022-11-28",
  };
  async function call(method: string, url: string, init: { body?: BodyInit; type?: string; missing?: boolean } = {}) {
    const response = await fetcher(url.startsWith("https:") ? url : `https://api.github.com${url}`, {
      method,
      headers: init.type ? { ...headers, "Content-Type": init.type } : headers,
      body: init.body,
    });
    if (response.status === 404 && init.missing) return undefined;
    if (!response.ok) throw new Error(`GitHub answered ${response.status} for ${method} ${url.split("?")[0]}`);
    return response;
  }
  const json = async <T>(response: Response | undefined) => (response ? ((await response.json()) as T) : undefined);
  const base = `/repos/${repo}`;

  return {
    async drafts() {
      const out: Draft[] = [];
      for (let page = 1; ; page++) {
        type Release = { id: number; tag_name: string; target_commitish: string; body: string | null; draft: boolean };
        const batch = (await json<Release[]>(await call("GET", `${base}/releases?per_page=100&page=${page}`))) ?? [];
        for (const r of batch) {
          if (r.draft) out.push({ id: r.id, tag: r.tag_name, target: r.target_commitish, notes: r.body ?? "" });
        }
        if (batch.length < 100) return out;
      }
    },
    async tagExists(tag) {
      return (await call("GET", `${base}/git/ref/tags/${encodeURIComponent(tag)}`, { missing: true })) !== undefined;
    },
    async isAncestor(older, newer) {
      const body = await json<{ status: string }>(await call("GET", `${base}/compare/${older}...${newer}`));
      return body?.status === "ahead" || body?.status === "identical";
    },
    async releaseState(id) {
      const body = await json<{ draft: boolean }>(await call("GET", `${base}/releases/${id}`, { missing: true }));
      return body === undefined ? "gone" : body.draft ? "draft" : "published";
    },
    async files(id) {
      const out: File[] = [];
      for (let page = 1; ; page++) {
        const path = `${base}/releases/${id}/assets?per_page=100&page=${page}`;
        const batch = (await json<{ id: number; name: string }[]>(await call("GET", path))) ?? [];
        for (const asset of batch) {
          // GitHub answers with a redirect to the file; fetch drops the token when following it.
          const response = await fetcher(`https://api.github.com${base}/releases/assets/${asset.id}`, {
            headers: { ...headers, Accept: "application/octet-stream" },
          });
          if (!response.ok) throw new Error(`GitHub answered ${response.status} for asset ${asset.name}`);
          out.push({ name: asset.name, data: new Uint8Array(await response.arrayBuffer()) });
        }
        if (batch.length < 100) return out;
      }
    },
    async createDraft(tag, target, notes) {
      const payload = { tag_name: tag, target_commitish: target, name: tag, body: notes, draft: true };
      const body = await json<{ id: number; draft: boolean }>(
        await call("POST", `${base}/releases`, { body: JSON.stringify(payload), type: "application/json" }),
      );
      if (!body?.draft) throw new Error(`GitHub did not make ${tag} a draft`);
      return body.id;
    },
    async upload(id, name, data) {
      const url = `https://uploads.github.com${base}/releases/${id}/assets?name=${encodeURIComponent(name)}`;
      await call("POST", url, { body: data, type: "application/octet-stream" });
    },
    async remove(id) {
      await call("DELETE", `${base}/releases/${id}`);
    },
  };
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? "").href) {
  const [mode, repo, ...args] = process.argv.slice(2);
  const token = process.env.GITHUB_TOKEN;
  const output = process.env.GITHUB_OUTPUT;
  if (!token || !repo || !((mode === "check" && args.length === 1 && output) || (mode === "create" && args.length === 4))) {
    console.error("Usage: see the top of release/draft.ts");
    process.exit(2);
  }
  const api = githubApi(fetch, token, repo);
  if (mode === "check") {
    const planned = parsePlan(readFileSync(args[0], "utf8"));
    const build = await check(api, planned);
    const end = `EOF_${randomUUID()}`;
    const lines = [`build=${build}`, `version=${planned?.version ?? ""}`, `notes<<${end}`, planned?.notes ?? "", end];
    appendFileSync(output!, `${lines.join("\n")}\n`);
  } else {
    const [head, version, notesFile, dir] = args;
    const notes = readFileSync(notesFile, "utf8");
    const files = readdirSync(dir).map((name) => ({ name, data: readFileSync(join(dir, name)) }));
    await create(api, { version, notes, head, files });
  }
}
