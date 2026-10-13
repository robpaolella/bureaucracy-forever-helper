// Reads the last published release and the pull requests merged since it. GET requests only:
// this script has no write access of its own, and the token is passed in by the caller.
import type { Item } from "./plan.ts";

const API = "https://api.github.com";

export type Fetch = typeof fetch;

async function get(fetcher: Fetch, token: string, path: string): Promise<Response> {
  const response = await fetcher(`${API}${path}`, {
    method: "GET",
    headers: {
      Accept: "application/vnd.github+json",
      Authorization: `Bearer ${token}`,
      "X-GitHub-Api-Version": "2022-11-28",
    },
  });
  if (!response.ok && response.status !== 404) {
    throw new Error(`GitHub answered ${response.status} for GET ${path}`);
  }
  return response;
}

// The tag of the latest published (non-draft, non-prerelease) release, or undefined if none.
export async function lastPublishedTag(fetcher: Fetch, token: string, repo: string): Promise<string | undefined> {
  const response = await get(fetcher, token, `/repos/${repo}/releases/latest`);
  if (response.status === 404) return undefined;
  return ((await response.json()) as { tag_name: string }).tag_name;
}

interface CommitInfo {
  sha: string;
  commit: { message: string };
  parents: unknown[];
}
interface PullInfo {
  number: number;
  title: string;
  body: string | null;
  merged_at: string | null;
}

async function commitsSince(fetcher: Fetch, token: string, repo: string, tag: string | undefined, head: string) {
  const commits: CommitInfo[] = [];
  for (let page = 1; ; page++) {
    const path = tag
      ? `/repos/${repo}/compare/${encodeURIComponent(tag)}...${head}?per_page=100&page=${page}`
      : `/repos/${repo}/commits?sha=${head}&per_page=100&page=${page}`;
    const body = (await (await get(fetcher, token, path)).json()) as CommitInfo[] | { commits: CommitInfo[] };
    const batch = Array.isArray(body) ? body : body.commits;
    commits.push(...batch);
    if (batch.length < 100) return commits;
  }
}

// The non-merge commits since `tag` (all of them when undefined) up to `head`, each as its
// merged pull request, or as a bare title when it has none. Repeats are fine: plan() dedupes.
export async function itemsSince(
  fetcher: Fetch,
  token: string,
  repo: string,
  tag: string | undefined,
  head: string,
): Promise<Item[]> {
  const items: Item[] = [];
  for (const commit of await commitsSince(fetcher, token, repo, tag, head)) {
    if (commit.parents.length > 1) continue;
    const pulls = (await (await get(fetcher, token, `/repos/${repo}/commits/${commit.sha}/pulls`)).json()) as PullInfo[];
    const merged = pulls.filter((pull) => pull.merged_at);
    if (merged.length === 0) items.push({ title: commit.commit.message.split("\n")[0] });
    for (const pull of merged) items.push({ number: pull.number, title: pull.title, body: pull.body ?? "" });
  }
  return items;
}
