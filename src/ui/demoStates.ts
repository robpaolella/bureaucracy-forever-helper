// Sample window states for the test-only demo page, named as in the approved design
// (design/2-helper-window/index.html). Every name is invented.
import type { ReservesView } from "../bridge";
import type { ActivityEvent } from "./activity";
import type { AppState, KeyStep, View } from "./app";
import type { ChainRow, ChainState } from "./chain";
import type { FolderStep } from "./folder";
import { applyRaids } from "./realWindow";
import { addonFailedEvent, addonRow, addonUpdatedEvent, type AddonUpdate } from "./addonUpdate";
import { updateFailedEvent, type HelperUpdate } from "./update";

export interface DemoState {
  app: AppState;
  view?: View;
}

const MAC = "/Applications/World of Warcraft";
const WIN = "D:\\Games\\World of Warcraft";

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

/** The first-run key step before anything is connected. */
const keyStep = (key: KeyStep): DemoState => ({
  app: {
    status: { tone: "closed", word: "Not set up", title: "Not connected yet" },
    chain: {
      site: { tone: "closed", word: "Not connected", detail: "No helper key yet" },
      folder: ok("", ""),
      addon: ok("", ""),
      reserves: ok("", ""),
      awards: ok("", ""),
    },
    activity: [],
    sentTonight: 0,
    setup: { key },
  },
});
const KEY = "bfh_3kZ9q2Lw8VtR4mXc7Pn1";
const keyError = (error: string, action?: string) => keyStep({ value: KEY, checking: false, error, action });

/** Step 2 with the key already in, as in the design's `folder-*` states. */
const folderStep = (folder: FolderStep): DemoState => ({
  app: {
    status: { tone: "closed", word: "Not set up", title: "Not connected yet" },
    chain: {
      site: ok("Connected", "Connected as Aldric"),
      folder: { tone: "closed", word: "Step 2", detail: "Choose the game folder" },
      addon: ok("", ""),
      reserves: ok("", ""),
      awards: ok("", ""),
    },
    activity: [],
    sentTonight: 0,
    setup: { key: { value: "", checking: false } },
    folder,
  },
});

/** The window with raids listed and `reserves` written, drawn by the real window code. */
const written = (reserves: ReservesView, list: ActivityEvent[], minutes: number[]): DemoState => {
  const at = (h: number, m: number) => new Date().setHours(h, m, 0, 0);
  const startsAt = (h: number, m: number) => Math.floor(new Date().setHours(h, m, 0, 0) / 1000);
  const raids = ["Molten Core", "Blackwing Lair"].map((name, i) => ({
    id: `r${i}`, code: "c", name, startsAt: startsAt(21 + i * 2, i * 30), durationMin: 180, instance: null, hasLootTable: true,
  }));
  const state = applyRaids(
    {
      status: { tone: "ok", word: "Connected", title: "Connected to the guild site" },
      chain: { ...working, awards: { tone: "closed", word: "None yet", detail: "They send as they’re handed out" } },
      activity: list.map((e, i) => ({ ...e, at: at(20, minutes[i] ?? 0) })),
      sentTonight: 0,
    },
    { state: { kind: "fresh" }, raids },
    reserves,
  );
  return { app: state };
};
const both = ["Molten Core", "Blackwing Lair"];
const ready = (): DemoState => written({ state: { kind: "inGame", raids: both }, changes: 1, first: true, problem: null }, [wrote, joined], [12, 11]);
/** The ready window with the helper's update card, as in the design's `helper-*` states. */
const helperUpdate = (update: HelperUpdate, extra: ActivityEvent[] = []): DemoState => {
  const { app } = ready();
  const activity = [...extra.map((e) => ({ ...e, at: new Date().setHours(20, 20, 0, 0) })), ...app.activity];
  return { app: { ...app, update, helperVersion: "0.2.1", activity } };
};
/** The ready window with the Add-on row in an update's state, as in the design's `addon-*` states. */
const addonUpdate = (update: AddonUpdate, installed: string, extra?: ActivityEvent): DemoState => {
  const { app } = ready();
  const inert = { install() {}, later() {}, notes() {} };
  const chain = { ...app.chain, addon: addonRow(app.chain.addon, update, installed, inert) };
  const activity = extra ? [{ ...extra, at: new Date().setHours(20, 26, 0, 0) }, ...app.activity] : app.activity;
  return { app: { ...app, chain, activity } };
};
const wrote: ActivityEvent = { at: 0, tone: "ok", kind: "Reserves written", text: "for Molten Core and Blackwing Lair. Type /reload in game." };
const joined: ActivityEvent = { at: 0, tone: "ok", kind: "Connected", text: "to the guild site." };

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
  ready: ready(),
  "helper-update": helperUpdate({ phase: "available", version: "0.3.0" }),
  "helper-installing": helperUpdate({ phase: "installing", version: "0.3.0", percent: 45 }),
  "helper-update-failed": helperUpdate({ phase: "failed", version: "0.3.0", reason: "badSignature" }, [
    updateFailedEvent("badSignature", "0.2.1", 0),
  ]),
  "addon-update": addonUpdate({ phase: "available", version: "1.4.0", note: null }, "1.3.2"),
  "addon-installing": addonUpdate({ phase: "installing", version: "1.4.0", percent: 70 }, "1.3.2"),
  "addon-updated": addonUpdate({ phase: "updated", version: "1.4.0", note: "It loads the next time you type /reload or start the game." }, "1.4.0", addonUpdatedEvent("1.4.0", 0)),
  "addon-rolled-back": addonUpdate({ phase: "failed", version: "1.4.0", kept: "1.3.2" }, "1.3.2", addonFailedEvent("1.3.2", 0)),
  reload: written({ state: { kind: "reload", raids: both, changed: ["Molten Core"] }, changes: 2, first: false, problem: null }, [
    { at: 0, tone: "warn", kind: "Reserves changed", text: "for Molten Core. Type /reload in game." },
    wrote,
    joined,
  ], [47, 12, 11]),
  settings: {
    view: "settings",
    app: {
      status: { tone: "ok", word: "Sending", title: "Everything’s working" },
      chain: working,
      activity: [],
      sentTonight: 0,
      gamePath: `${MAC}/_classic_`,
      helperVersion: "0.2.1",
      addonVersion: "1.3.2",
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
  "folder-searching": folderStep({ phase: "searching" }),
  "folder-found": folderStep({ phase: "found", path: `${MAC}/_classic_` }),
  "folder-several": folderStep({
    phase: "several",
    selected: 0,
    installs: ["_classic_", "_classic_era_"].map((version) => ({
      gameFolder: WIN,
      version,
      versionFolder: `${WIN}\\${version}`,
      addonFolder: "",
    })),
  }),
  "folder-not-found": folderStep({
    phase: "notFound",
    message: "Couldn’t find World of Warcraft. Choose the folder it’s installed in.",
  }),
  "folder-not-game": folderStep({
    phase: "notGame",
    message: "That folder isn’t a World of Warcraft folder. Choose the folder named World of Warcraft.",
    path: "C:\\Users\\Élise Marchand\\Downloads",
  }),
  "folder-no-addon": folderStep({
    phase: "noAddon",
    message: "World of Warcraft is here, but the Bureaucracy Forever add-on isn’t installed.",
    path: `${WIN}\\_classic_`,
    versionFolders: [`${WIN}\\_classic_`],
  }),
  "folder-gone": folderStep({
    phase: "gone",
    message: "The game folder you chose before isn’t there any more. Choose it again.",
    path: WIN,
  }),
  "key-empty": keyStep({ value: "", checking: false }),
  "key-checking": keyStep({ value: KEY, checking: true }),
  "key-malformed": keyStep({
    value: "my key from the site",
    checking: false,
    error: "That doesn’t look like a helper key. Copy it again from the officer area.",
  }),
  "key-revoked": keyError("This key has been revoked or no longer works. Create a new key in the officer area."),
  "key-site-off": keyError("The guild site hasn’t switched the helper on yet. Try again later.", "Try again"),
  "key-unreachable": keyError("Couldn’t reach the guild site. Check your internet connection and try again.", "Try again"),
  "key-store-failed": keyError(
    "The helper couldn’t save your key in Keychain. Allow access when your Mac asks, then try again.",
    "Try again",
  ),
};
