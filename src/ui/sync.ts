// What the sync status (#31) shows: the window's headline, the Guild site and Awards rows,
// the Activity lines and the notices. Wording follows the approved design's `offline`,
// `refused`, `revoked`, `site-off`, `queue-damaged` and `notification` states.
import type { Notice, RefusedRecord, SyncStatus } from "../bridge";
import type { ActivityEvent } from "./activity";
import type { AppState } from "./app";
import type { ChainRow } from "./chain";

/** What the buttons a sync problem offers do. */
export interface SyncActions {
  addKey(): void;
  openLootLog(): void;
}

/** Item names heard from the chat log, by game item id, for a refused record's wording. */
export type ItemNames = ReadonlyMap<number, string>;

const awardsWord = (n: number) => (n === 1 ? "1 award" : `${n} awards`);
const itemOf = (record: RefusedRecord, names: ItemNames) =>
  (record.item !== null ? names.get(record.item) : undefined) ?? undefined;

const KEPT_AWARDS: ChainRow = { tone: "stop", word: "Waiting", detail: "Kept until they can send" };

/** The headline, the rows and the problem's own message for one sync status, or nothing to add. */
function problem(
  sync: SyncStatus,
  actions: SyncActions,
  names: ItemNames,
): { status: AppState["status"]; site?: ChainRow; awards?: ChainRow } | undefined {
  switch (sync.kind) {
    case "offline":
      return {
        status: {
          tone: "stop",
          word: "Offline",
          title: sync.waiting > 0 ? `Offline: ${awardsWord(sync.waiting)} waiting` : "Can’t reach the guild site",
        },
        site: {
          tone: "stop",
          word: "Offline",
          detail: "Can’t reach the guild site",
          broken: true,
          message: {
            text: "They’ll send when you’re back online. Nothing is lost. The game still has its reserves.",
          },
        },
        awards: sync.waiting > 0 ? KEPT_AWARDS : undefined,
      };
    case "refused": {
      const n = sync.records.length;
      const [only] = sync.records;
      const text =
        n === 1 && only
          ? `${[itemOf(only, names) ?? "An award", only.character ? `to ${only.character}` : ""].filter(Boolean).join(" ")} wasn’t recorded. Check it on the loot log. Other awards are still sending.`
          : `${n} awards weren’t recorded. Check them on the loot log. Other awards are still sending.`;
      return {
        status: { tone: "warn", word: "Needs you", title: `The site refused ${awardsWord(n)}` },
        awards: {
          tone: "warn",
          word: `${n} refused`,
          detail: "Others still sending",
          broken: true,
          message: { text, action: { label: "Open the loot log", variant: "secondary", onClick: actions.openLootLog } },
        },
      };
    }
    case "keyStopped":
      return {
        status: { tone: "stop", word: "Stopped", title: "Your helper key no longer works" },
        site: {
          tone: "stop",
          word: "Key refused",
          detail: "The site refused this key",
          broken: true,
          message: {
            text: "It was revoked, or it belongs to someone who’s no longer an officer. Awards are kept and will send once you add a new key.",
            action: { label: "Add a new key", variant: "primary", onClick: actions.addKey },
          },
        },
        awards: KEPT_AWARDS,
      };
    case "siteOff":
      return {
        status: { tone: "stop", word: "Stopped", title: "The site has the helper switched off" },
        site: {
          tone: "stop",
          word: "Switched off",
          detail: "The site isn’t accepting the helper",
          broken: true,
          message: {
            text: "Awards are kept and will send when it’s back on. Officers can still record loot on the site’s loot log.",
          },
        },
        awards: KEPT_AWARDS,
      };
    case "queueDamaged":
      // No button: nothing here may change or empty the queue file.
      return {
        status: { tone: "stop", word: "Stopped", title: "Sending has stopped" },
        awards: {
          tone: "stop",
          word: "Stopped",
          detail: "Waiting list damaged",
          broken: true,
          message: {
            text: "The list of awards waiting to send is damaged, so the helper stopped sending. Nothing was deleted. The add-on still has every award, and they send again at your next /reload or logout.",
          },
        },
      };
    default:
      return undefined;
  }
}

/**
 * `state` as the window shows it with `sync` on top: a problem takes the headline, and its row
 * shows only the problem (no "who chose the raid" caption). Setup, and any status that is not
 * a problem, leave the state as it is.
 */
export function withSync(state: AppState, sync: SyncStatus | undefined, actions: SyncActions, names: ItemNames): AppState {
  if (!sync || state.setup) return state;
  const shown = problem(sync, actions, names);
  if (!shown) return state;
  return {
    ...state,
    status: shown.status,
    chain: {
      ...state.chain,
      ...(shown.site && { site: shown.site }),
      ...(shown.awards && { awards: shown.awards }),
    },
  };
}

/** Which records the officer has already been told about, and which kind of problem was last. */
export interface Told {
  kind?: SyncStatus["kind"];
  refused: Set<string>;
}

/**
 * The Activity lines and the notice for `sync`, each once: a kind of problem is told when it
 * begins (a retry that fails the same way is silent), a refused record when it is first seen.
 */
export function syncNews(
  told: Told,
  sync: SyncStatus,
  names: ItemNames,
  at: number,
): { events: ActivityEvent[]; notice?: Notice } {
  const begun = told.kind !== sync.kind;
  told.kind = sync.kind;
  switch (sync.kind) {
    case "offline":
      if (!begun) break;
      return {
        events: [
          {
            at,
            tone: "stop",
            kind: "Offline",
            text: sync.waiting > 0 ? `${awardsWord(sync.waiting)} ${sync.waiting === 1 ? "is" : "are"} waiting to send.` : "Can’t reach the guild site.",
          },
        ],
        notice:
          sync.waiting > 0
            ? { headline: `Offline: ${awardsWord(sync.waiting)} waiting`, detail: "They’ll send when you’re back online." }
            : undefined,
      };
    case "keyStopped":
      if (!begun) break;
      return {
        events: [{ at, tone: "stop", kind: "Key refused", text: "by the guild site. Awards are kept." }],
        notice: { headline: "Your helper key no longer works", detail: "Add a new key to keep sending awards." },
      };
    case "siteOff":
      if (!begun) break;
      return {
        events: [{ at, tone: "stop", kind: "Switched off", text: "The guild site isn’t accepting the helper. Awards are kept." }],
      };
    case "queueDamaged":
      if (!begun) break;
      return {
        events: [{ at, tone: "stop", kind: "Stopped", text: "The waiting list is damaged. Nothing was deleted." }],
      };
    case "refused": {
      const fresh = sync.records.filter((r) => !told.refused.has(r.id));
      fresh.forEach((r) => told.refused.add(r.id));
      if (fresh.length === 0) break;
      const events = fresh.map<ActivityEvent>((r) => ({
        at,
        tone: "stop",
        kind: "Refused",
        strong: itemOf(r, names) ?? "An award",
        text: `${r.character ? `to ${r.character}. ` : ""}Check it on the loot log.`,
      }));
      const [only] = fresh;
      const item = only && fresh.length === 1 ? itemOf(only, names) : undefined;
      return {
        events,
        notice: {
          headline: `The site refused ${awardsWord(fresh.length)}`,
          detail: fresh.length > 1 ? "Check them on the loot log." : item ? `Check ${item} on the loot log.` : "Check the loot log.",
        },
      };
    }
  }
  return { events: [] };
}
