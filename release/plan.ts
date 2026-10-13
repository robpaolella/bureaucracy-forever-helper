// Release logic: works out the next version and the officers' release notes from the pull
// requests merged since the last published release. Plain data in, plain data out, no network.
// A port of the add-on's Release.plan (bureaucracy-forever-addon, release/Release.lua).

export interface Item {
  // A pull request has a number and a body; a commit with no pull request has only a title.
  number?: number;
  title: string;
  body?: string;
}

export interface Plan {
  version: string;
  notes: string;
  reword: string[];
}

export const REWORD_MARK = " (needs a release note: Robert to reword)";
export const FIRST_RELEASE_LINE = "This is the first release: a fresh install, nothing to update.";
// A `Release note:` starting with this puts the note under "Before you update" (a manual step).
export const BEFORE_PREFIX = "Before you update:";

const NONE = 0;
const FIX = 1;
const FEATURE = 2;
const BREAKING = 3;

interface Entry {
  title: string;
  kind: string;
  rank: number;
  breaking: boolean;
  note?: string;
}

// "feat(ui)!: add a thing" -> feat, breaking. Not a Conventional Commit -> other.
function parseTitle(title: string): { kind: string; bang: boolean } {
  const match = /^\s*([A-Za-z]+)(?:\([^)]*\))?(!?):/.exec(title);
  return match ? { kind: match[1].toLowerCase(), bang: match[2] === "!" } : { kind: "other", bang: false };
}

const hasBreakingFooter = (body: string) => body.split(/\r?\n/).some((line) => /^BREAKING[ -]CHANGE:/.test(line));

// The PR's `Release note:` line, or undefined when missing, empty or "none".
function releaseNote(body: string): string | undefined {
  for (const line of body.split(/\r?\n/)) {
    const match = /^\s*release note:\s*(.*?)\s*$/i.exec(line);
    if (match) {
      const text = match[1];
      return text === "" || text.replace(/\.$/, "").toLowerCase() === "none" ? undefined : text;
    }
  }
  return undefined;
}

// One entry per distinct pull request (by number), in first-seen order. An item with no
// number is a commit with no pull request: it counts by its subject, with no note.
function classify(items: Item[]): Entry[] {
  const seen = new Set<number>();
  const out: Entry[] = [];
  for (const item of items) {
    if (item.number !== undefined) {
      if (seen.has(item.number)) continue;
      seen.add(item.number);
    }
    const hasPr = item.number !== undefined;
    const body = item.body ?? "";
    const { kind, bang } = parseTitle(item.title);
    const breaking = bang || (hasPr && hasBreakingFooter(body));
    let rank = NONE;
    if (breaking) rank = BREAKING;
    else if (kind === "feat") rank = FEATURE;
    else if (kind === "fix") rank = FIX;
    out.push({ title: item.title.trim(), kind, rank, breaking, note: hasPr ? releaseNote(body) : undefined });
  }
  return out;
}

function parseVersion(version: string): [number, number, number] {
  const match = /^v?(\d+)\.(\d+)\.(\d+)$/.exec(version);
  if (!match) throw new Error(`not a version: ${version}`);
  return [Number(match[1]), Number(match[2]), Number(match[3])];
}

// Next version after `previous` ("1.2.3" or "v1.2.3"; undefined when nothing is published
// yet), or undefined when nothing merged calls for a release.
export function nextVersion(previous: string | undefined, items: Item[]): string | undefined {
  const entries = classify(items);
  const top = Math.max(NONE, ...entries.map((entry) => entry.rank));
  if (previous === undefined) return top > NONE ? "1.0.0" : undefined;
  const [major, minor, patch] = parseVersion(previous);
  if (top === BREAKING) return `${major + 1}.0.0`;
  if (top === FEATURE) return `${major}.${minor + 1}.0`;
  if (top === FIX) return `${major}.${minor}.${patch + 1}`;
  return undefined;
}

// The notes in Markdown, plus the entries that fell back to their title.
export function notes(previous: string | undefined, items: Item[]): { notes: string; reword: string[] } {
  const before: string[] = [];
  const added: string[] = [];
  const fixed: string[] = [];
  const reword: string[] = [];

  if (previous === undefined) before.push(FIRST_RELEASE_LINE);

  for (const entry of classify(items)) {
    if (entry.rank === NONE && entry.note === undefined) continue;
    let text = entry.note;
    let manualStep = false;
    if (text !== undefined && text.toLowerCase().startsWith(BEFORE_PREFIX.toLowerCase())) {
      text = text.slice(BEFORE_PREFIX.length).trim();
      manualStep = true;
    }
    if (text === undefined || text === "") {
      text = entry.title + REWORD_MARK;
      reword.push(entry.title);
    }
    const target = entry.breaking || manualStep ? before : entry.kind === "fix" ? fixed : added;
    target.push(`- ${text}`);
  }

  const sections = [
    ["Before you update", before],
    ["New", added],
    ["Fixed", fixed],
  ] as const;
  const text = sections
    .filter(([, lines]) => lines.length > 0)
    .map(([heading, lines]) => `## ${heading}\n${lines.join("\n")}`)
    .join("\n\n");
  return { notes: text === "" ? "" : `${text}\n`, reword };
}

// `previous`: the last published version, or undefined when none. Returns undefined when no
// release is due.
export function plan(input: { previous?: string; items: Item[] }): Plan | undefined {
  const version = nextVersion(input.previous, input.items);
  if (version === undefined) return undefined;
  return { version, ...notes(input.previous, input.items) };
}
