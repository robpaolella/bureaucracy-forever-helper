// Sample window states for the test-only demo page, named as in the approved design
// (design/2-helper-window/index.html). Every name is invented.
import type { ActivityEvent } from "./activity";
import type { AppState, View } from "./app";
import type { ChainRow, ChainState } from "./chain";

export interface DemoState {
  app: AppState;
  view?: View;
}

const MIN = 60_000;
const PEOPLE = ["Aldric Vane", "Thessaly Ward", "Corvin Ashby", "Mirela Stone", "Bram Hollow", "Isolde Fenn"];
const LOOT: [string, string][] = [
  ["Aged Core Leather Gloves", "Lucifron"],
  ["Gauntlets of Might", "Lucifron"],
  ["Striker’s Mark", "Magmadar"],
  ["Earthshaker", "Magmadar"],
  ["Sabatons of Might", "Gehennas"],
];

function times(...parts: [string, string][]): Node[] {
  return parts.map(([name, when]) => {
    const row = document.createElement("div");
    row.className = "row";
    const a = document.createElement("span");
    a.className = "caption";
    a.textContent = name;
    const spacer = document.createElement("span");
    spacer.className = "spacer";
    const b = document.createElement("span");
    b.className = "caption tabular";
    b.textContent = when;
    row.append(a, spacer, b);
    return row;
  });
}

function note(value: string): Node {
  const p = document.createElement("p");
  p.className = "caption";
  p.textContent = value;
  return p;
}

const ok = (word: string, detail: string, under?: Node[]): ChainRow => ({ tone: "ok", word, detail, under });

/** `n` events, newest first, counting back from 11:58 PM today in 4–5 minute steps. */
function activity(n: number, extra: ActivityEvent[] = []): ActivityEvent[] {
  const end = new Date();
  end.setHours(23, 58, 0, 0);
  const list = [...extra];
  for (let i = 0; list.length < n - 2; i++) {
    const [strong, meta] = LOOT[i % LOOT.length] as [string, string];
    list.push({ at: 0, tone: "ok", kind: "Sent", strong, text: `to ${PEOPLE[i % PEOPLE.length]}`, meta });
  }
  list.push({ at: 0, tone: "ok", kind: "Reserves written", text: "for Molten Core and Blackwing Lair. Type /reload in game." });
  list.push({ at: 0, tone: "ok", kind: "Connected", text: "to the guild site." });
  return list.slice(0, n).map((e, j) => ({ ...e, at: end.getTime() - (j * 4 + (j % 3)) * MIN }));
}

const reserves = () =>
  ok("In the game", "Next 12 hours", times(["Molten Core", "Tue 9:00 PM CDT"], ["Blackwing Lair", "Tue 11:30 PM CDT"]));
const awards = (word: string, tone: ChainRow["tone"], detail: string): ChainRow => ({
  tone,
  word,
  detail,
  under: [note("Picked in game. Change it on the add-on’s Sync tab.")],
});

const working: ChainState = {
  site: ok("Connected", "Connected as Aldric"),
  folder: ok("Found", ".../World of Warcraft/_classic_"),
  addon: ok("Installed", "Version 1.3.2"),
  reserves: reserves(),
  awards: awards("Sending", "ok", "Going to Molten Core"),
};

export const demoStates: Record<string, DemoState> = {
  live: {
    app: {
      status: { tone: "ok", word: "Sending", title: "Everything’s working" },
      chain: working,
      activity: activity(16),
      sentTonight: 14,
    },
  },
  "live-full": {
    view: "activity",
    app: {
      status: { tone: "ok", word: "Sending", title: "Everything’s working" },
      chain: working,
      activity: activity(50),
      sentTonight: 48,
    },
  },
  settings: {
    view: "settings",
    app: {
      status: { tone: "ok", word: "Sending", title: "Everything’s working" },
      chain: working,
      activity: [],
      sentTonight: 0,
    },
  },
  offline: {
    app: {
      status: { tone: "stop", word: "Offline", title: "Offline: 4 awards waiting" },
      chain: {
        ...working,
        site: {
          tone: "stop",
          word: "Offline",
          detail: "Can’t reach the guild site",
          broken: true,
          message: {
            text: "They’ll send when you’re back online. Nothing is lost. The game still has its reserves.",
          },
        },
        awards: awards("Waiting", "stop", "Kept until they can send"),
      },
      activity: activity(14, [{ at: 0, tone: "stop", kind: "Offline", text: "4 awards are waiting to send." }]),
      sentTonight: 9,
    },
  },
};
