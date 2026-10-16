import { describe, expect, it } from "vitest";
import {
  ACTIVITY_KEEP,
  createActivityLog,
  guildTime,
  raidStart,
  showSeeAll,
  visibleActivity,
  type ActivityEvent,
} from "./activity";
import { GUILD_TIMEZONE } from "../guild";

const NOW = new Date(2026, 9, 14, 20, 0).getTime();
const MIN = 60_000;
const event = (at: number, kind = "Sent"): ActivityEvent => ({ at, tone: "ok", kind });

describe("visibleActivity", () => {
  it("keeps today's events and drops yesterday's", () => {
    const yesterday = new Date(2026, 9, 13, 23, 59).getTime();
    const out = visibleActivity([event(yesterday), event(NOW - MIN)], NOW);
    expect(out.map((e) => e.at)).toEqual([NOW - MIN]);
  });

  it("lists newest first whatever order they were added in", () => {
    const out = visibleActivity([event(NOW - 3 * MIN), event(NOW - MIN), event(NOW - 2 * MIN)], NOW);
    expect(out.map((e) => e.at)).toEqual([NOW - MIN, NOW - 2 * MIN, NOW - 3 * MIN]);
  });

  it("keeps only the last 50", () => {
    const many = Array.from({ length: 60 }, (_, i) => event(NOW - i * MIN, `E${i}`));
    const out = visibleActivity(many, NOW);
    expect(out).toHaveLength(ACTIVITY_KEEP);
    expect(out[0]?.kind).toBe("E0");
    expect(out[49]?.kind).toBe("E49");
  });
});

describe("createActivityLog", () => {
  it("adds events and lists them newest first, capped at 50", () => {
    const log = createActivityLog(() => NOW);
    for (let i = 0; i < 55; i++) log.add(event(NOW - (55 - i) * MIN, `E${i}`));
    const out = log.list();
    expect(out).toHaveLength(50);
    expect(out[0]?.kind).toBe("E54");
  });

  it("drops events once the day has rolled over", () => {
    let now = NOW;
    const log = createActivityLog(() => now);
    log.add(event(NOW));
    now = new Date(2026, 9, 15, 0, 5).getTime();
    expect(log.list()).toEqual([]);
  });
});

describe("showSeeAll", () => {
  it("shows only above three events", () => {
    expect([0, 3, 4].map(showSeeAll)).toEqual([false, false, true]);
  });
});

describe("guildTime", () => {
  it("is Pacific time in summer and winter, whatever the officer's zone", () => {
    expect(guildTime(Date.UTC(2026, 6, 14, 2, 0))).toBe("7:00 PM");
    expect(guildTime(Date.UTC(2026, 11, 14, 3, 0))).toBe("7:00 PM");
  });

  it("uses the one guild zone constant", () => {
    expect(GUILD_TIMEZONE).toBe("America/Los_Angeles");
  });
});

describe("raidStart", () => {
  it("gives the officer's weekday, time and zone, and the guild time", () => {
    const start = raidStart(Date.UTC(2026, 9, 21, 2, 0) / 1000);
    expect(start.local).toMatch(/^\w{3} \d{1,2}:\d{2} [AP]M \S+$/);
    expect(start.guild).toBe("7:00 PM guild time");
    expect(start.label).toBe(`${start.local}, 7:00 PM guild time`);
  });
});
