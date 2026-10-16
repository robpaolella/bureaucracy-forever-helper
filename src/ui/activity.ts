// The activity log: today's events, newest first, at most the last 50 (design: "Today,
// newest first. The last 50 are kept."). Feature issues add events; the window lists them.
import { GUILD_TIMEZONE } from "../guild";
import type { Tone } from "./components";

export const ACTIVITY_KEEP = 50;
/** How many events the main screen shows before "See all". */
export const ACTIVITY_RECENT = 3;

export interface ActivityEvent {
  /** When it happened, in milliseconds since 1970 (Date.now()). */
  at: number;
  tone: Exclude<Tone, "closed">;
  /** The coloured lead-in, such as "Sent" or "Offline". */
  kind: string;
  /** Bold text after the lead-in, such as an item name. */
  strong?: string;
  text?: string;
  /** Muted trailing text, such as the boss. */
  meta?: string;
}

function sameDay(a: number, b: number): boolean {
  const x = new Date(a);
  const y = new Date(b);
  return (
    x.getFullYear() === y.getFullYear() && x.getMonth() === y.getMonth() && x.getDate() === y.getDate()
  );
}

/** Today's events and the last 50, newest first. */
export function visibleActivity(events: readonly ActivityEvent[], now: number): ActivityEvent[] {
  return events
    .filter((e) => sameDay(e.at, now))
    .sort((a, b) => b.at - a.at)
    .slice(0, ACTIVITY_KEEP);
}

export interface ActivityLog {
  add(event: ActivityEvent): void;
  list(): ActivityEvent[];
}

export function createActivityLog(now: () => number = Date.now): ActivityLog {
  let events: ActivityEvent[] = [];
  return {
    add(event) {
      events = visibleActivity([...events, event], now());
    },
    list() {
      events = visibleActivity(events, now());
      return events;
    },
  };
}

/** Whether the main screen offers "See all": only when there is more than it shows. */
export function showSeeAll(count: number): boolean {
  return count > ACTIVITY_RECENT;
}

/** A time such as "11:58 PM", in the officer's own zone unless `timeZone` is given. */
export function formatTime(at: number, timeZone?: string): string {
  return new Date(at).toLocaleTimeString("en-US", { hour: "numeric", minute: "2-digit", timeZone });
}

/** The time in the guild's zone, such as "7:00 PM". */
export function guildTime(at: number): string {
  return formatTime(at, GUILD_TIMEZONE);
}

/** The officer's time zone, such as "CDT". */
export function zoneName(at: number): string {
  const part = new Intl.DateTimeFormat("en-US", { timeZoneName: "short" })
    .formatToParts(new Date(at))
    .find((p) => p.type === "timeZoneName");
  return part?.value ?? "";
}

/** When a raid starts: the officer's weekday, time and zone, with the guild's time for hover and screen readers. */
export interface RaidStart {
  /** Such as "Tue 9:00 PM CDT". */
  local: string;
  /** Such as "7:00 PM guild time". */
  guild: string;
  /** What a screen reader says: both, in one line. */
  label: string;
}

/** `startsAt` is Unix seconds, as the raids list carries it. */
export function raidStart(startsAt: number): RaidStart {
  const at = startsAt * 1000;
  const day = new Date(at).toLocaleDateString("en-US", { weekday: "short" });
  const local = `${day} ${formatTime(at)} ${zoneName(at)}`.trim();
  const guild = `${guildTime(at)} guild time`;
  return { local, guild, label: `${local}, ${guild}` };
}
