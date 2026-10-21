// How the raid was chosen, as the Awards row and the tray's second line say it (#60, design
// revision 4). One pure function turns the newest saved reading into a state; `null` means
// "no information", and the row keeps today's wording. Part 2 (#91) draws the result.
import type { Raid, RaidChoice } from "../bridge";
import { formatTime, guildTime, zoneName } from "./activity";
import type { ChainRow } from "./chain";
import { el } from "./components";

/** Saved data older than this is ignored. */
export const CHOICE_MAX_AGE_MS = 12 * 60 * 60 * 1000;

export type ChoiceState =
  | "chosen"
  | "chosen-live"
  | "chosen-no-name"
  | "not-loaded"
  | "not-chosen"
  | "not-guild-run";

export interface RaidChoiceLine {
  state: ChoiceState;
  /** The Awards row's detail line. */
  detail: string;
  /** The Awards row's pill word. */
  pill: string;
  /** Only `not-loaded` and `not-chosen` turn the row amber; `not-guild-run` is grey. */
  tone: "normal" | "warn" | "off";
  caption: string;
  /** The tray's second line: the raid or state only, never a name. */
  trayLine: string;
  /** When the game saved the file, Unix seconds, for the caption's guild-time hover (#91). */
  savedAt: number;
}

export interface RaidChoiceInput {
  /** The newest reading, or null before the game has saved the file. */
  choice: RaidChoice | null;
  /** The raids list (#12), to name the chosen raid. */
  raids: readonly Pick<Raid, "id" | "name">[];
  /** The raid of the latest handled award (#30); it decides the raid shown. */
  awardsRaid?: { id: string };
  /** Now, in milliseconds. */
  now: number;
}

const CHANGE = "Change it on the add-on’s Sync tab.";

export function raidChoiceLine({
  choice,
  raids,
  awardsRaid,
  now,
}: RaidChoiceInput): RaidChoiceLine | null {
  // 0 is the reader's "no file time".
  if (!choice || !Number.isFinite(choice.savedAt) || choice.savedAt <= 0) return null;
  const savedMs = choice.savedAt * 1000;
  if (now - savedMs > CHOICE_MAX_AGE_MS) return null;
  const asOf = `as of your last /reload, ${formatTime(savedMs)} ${zoneName(savedMs)}`;
  const base = { savedAt: choice.savedAt };
  const who = choice.chosenBy?.trim() || null;

  // The latest award decides the raid shown; the saved data adds only who chose it.
  if (awardsRaid) {
    if (choice.activeBy !== "chosen" || choice.activeRaid !== awardsRaid.id)
      return null;
    const raid = raids.find((r) => r.id === awardsRaid.id);
    if (!raid) return null;
    return {
      ...base,
      state: "chosen-live",
      detail: `Going to ${raid.name}`,
      pill: "Sending",
      tone: "normal",
      caption: who
        ? `Chosen by ${who}, ${asOf}. ${CHANGE}`
        : `Picked in game, ${asOf}. ${CHANGE}`,
      trayLine: `Awards going to ${raid.name}`,
    };
  }

  switch (choice.activeBy) {
    case "chosen": {
      if (!choice.activeRaid) {
        return {
          ...base,
          state: "not-loaded",
          detail: "Raid not loaded",
          pill: "Not loaded",
          tone: "warn",
          caption: `${who ?? "An officer"} chose a raid the add-on hasn’t loaded yet. Type /reload in game to load it. ${capital(asOf)}.`,
          trayLine: "Raid not loaded in game",
        };
      }
      const raid = raids.find((r) => r.id === choice.activeRaid);
      if (!raid) return null;
      return {
        ...base,
        state: who ? "chosen" : "chosen-no-name",
        detail: `Going to ${raid.name}`,
        pill: "None yet",
        tone: "normal",
        caption: who
          ? `Chosen by ${who}, ${asOf}. ${CHANGE}`
          : `Picked in game, ${asOf}. ${CHANGE}`,
        trayLine: `Awards going to ${raid.name}`,
      };
    }
    case "none":
      return {
        ...base,
        state: "not-chosen",
        detail: "Raid not chosen yet",
        pill: "Not chosen",
        tone: "warn",
        caption: `Awards wait in the add-on until an officer chooses the raid on its Sync tab. ${capital(asOf)}.`,
        trayLine: "Raid not chosen yet",
      };
    case "off":
      return {
        ...base,
        state: "not-guild-run",
        detail: "Not a guild run",
        pill: "Off",
        tone: "off",
        caption: `The add-on records nothing for this group. ${capital(asOf)}.`,
        trayLine: "Not a guild run",
      };
    default:
      return null;
  }
}

function capital(text: string): string {
  return text.charAt(0).toUpperCase() + text.slice(1);
}

/**
 * The Awards row for `line`: its detail and pill, and the caption under it, with the file's
 * time in the officer's zone and the guild's time on hover and for screen readers. Never
 * `broken`, so the row never opens.
 */
export function choiceRow(line: RaidChoiceLine): ChainRow {
  const at = line.savedAt * 1000;
  const local = `${formatTime(at)} ${zoneName(at)}`.trim();
  const guild = `${guildTime(at)} guild time`;
  const caption = el("p", "caption");
  const [before, ...rest] = line.caption.split(local);
  if (rest.length === 0) {
    caption.append(line.caption);
  } else {
    const time = el("time", "tabular", [local.replace(/ (?=\S+$)/, "\u00a0")]);
    time.dateTime = new Date(at).toISOString();
    time.tabIndex = 0;
    time.title = guild;
    time.setAttribute("aria-label", `${local}, ${guild}`);
    caption.append(before ?? "", time, rest.join(local));
  }
  return {
    tone: line.state === "chosen-live" ? "ok" : line.tone === "warn" ? "warn" : "closed",
    word: line.pill,
    detail: line.detail,
    under: [caption],
  };
}
