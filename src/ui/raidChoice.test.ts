import { describe, expect, it } from "vitest";
import type { RaidChoice } from "../bridge";
import { raidChoiceLine, type RaidChoiceInput } from "./raidChoice";
import { formatTime, zoneName } from "./activity";

const SAVED = 1_792_005_000;
const NOW = SAVED * 1000 + 60_000;
const RAIDS = [
  { id: "r1", name: "Molten Core" },
  { id: "r2", name: "Blackwing Lair" },
];
const TIME = `${formatTime(SAVED * 1000)} ${zoneName(SAVED * 1000)}`;

function choice(over: Partial<RaidChoice> = {}): RaidChoice {
  return {
    activeBy: "chosen",
    activeRaid: "r1",
    chosenBy: "Aldric",
    savedAt: SAVED,
    ...over,
  };
}
function line(over: Partial<RaidChoiceInput> = {}) {
  return raidChoiceLine({ choice: choice(), raids: RAIDS, now: NOW, ...over });
}

describe("raidChoiceLine", () => {
  it("chosen, before any award", () => {
    const l = line()!;
    expect(l).toMatchObject({
      state: "chosen",
      detail: "Going to Molten Core",
      pill: "None yet",
      tone: "normal",
    });
    expect(l.caption).toBe(
      `Chosen by Aldric, as of your last /reload, ${TIME}. Change it on the add-on’s Sync tab.`,
    );
    expect(l.trayLine).toBe("Awards going to Molten Core");
  });

  it("chosen with a name that is nil, empty or blank gives the raid alone", () => {
    for (const chosenBy of [null, "", "  "]) {
      const l = line({ choice: choice({ chosenBy }) })!;
      expect(l.state).toBe("chosen-no-name");
      expect(l.caption).toBe(
        `Picked in game, as of your last /reload, ${TIME}. Change it on the add-on’s Sync tab.`,
      );
      expect(l.detail).toBe("Going to Molten Core");
    }
  });

  it("chosen with no raid loaded is not-loaded, naming an officer when there is no name", () => {
    const l = line({ choice: choice({ activeRaid: null }) })!;
    expect(l).toMatchObject({
      state: "not-loaded",
      detail: "Raid not loaded",
      pill: "Not loaded",
      tone: "warn",
    });
    expect(l.caption).toBe(
      `Aldric chose a raid the add-on hasn’t loaded yet. Type /reload in game to load it. As of your last /reload, ${TIME}.`,
    );
    expect(l.trayLine).toBe("Raid not loaded in game");
    expect(
      line({ choice: choice({ activeRaid: null, chosenBy: null }) })!.caption,
    ).toMatch(/^An officer chose/);
  });

  it("none is not-chosen", () => {
    const l = line({
      choice: choice({ activeBy: "none", activeRaid: null, chosenBy: null }),
    })!;
    expect(l).toMatchObject({
      state: "not-chosen",
      detail: "Raid not chosen yet",
      pill: "Not chosen",
      tone: "warn",
    });
    expect(l.caption).toBe(
      `Awards wait in the add-on until an officer chooses the raid on its Sync tab. As of your last /reload, ${TIME}.`,
    );
    expect(l.trayLine).toBe("Raid not chosen yet");
  });

  it("off is not-guild-run", () => {
    const l = line({ choice: choice({ activeBy: "off" }) })!;
    expect(l).toMatchObject({
      state: "not-guild-run",
      detail: "Not a guild run",
      pill: "Off",
      tone: "off",
    });
    expect(l.caption).toBe(
      `The add-on records nothing for this group. As of your last /reload, ${TIME}.`,
    );
    expect(l.trayLine).toBe("Not a guild run");
  });

  it("gives no information for an unknown or missing activeBy, no data or no file time", () => {
    for (const activeBy of ["auto", "override", "", "CHOSEN", null]) {
      expect(line({ choice: choice({ activeBy }) })).toBeNull();
    }
    expect(line({ choice: null })).toBeNull();
    expect(line({ choice: choice({ savedAt: 0 }), now: 0 })).toBeNull();
    expect(line({ choice: choice({ savedAt: 0 }) })).toBeNull();
    expect(line({ choice: choice({ savedAt: Number.NaN }) })).toBeNull();
  });

  it("is chosen-live when the latest award went to the raid the saved data names", () => {
    const l = line({ awardsRaid: { id: "r1" } })!;
    expect(l).toMatchObject({
      state: "chosen-live",
      pill: "Sending",
      detail: "Going to Molten Core",
    });
    expect(l.caption).toBe(`Chosen by Aldric, as of your last /reload, ${TIME}. Change it on the add-on’s Sync tab.`);
    expect(l.trayLine).toBe("Awards going to Molten Core");
    expect(
      line({ awardsRaid: { id: "r1" }, choice: choice({ chosenBy: null }) })!
        .caption,
    ).toMatch(/^Picked in game, as of/);
  });

  it("keeps today's wording when the latest award went to a different raid", () => {
    expect(line({ awardsRaid: { id: "r2" } })).toBeNull();
    expect(
      line({ awardsRaid: { id: "r1" }, choice: choice({ activeBy: "off" }) }),
    ).toBeNull();
  });

  it("ignores saved data over 12 hours old and uses it at 11 hours 59 minutes", () => {
    const hours = (h: number, m = 0) => SAVED * 1000 + (h * 60 + m) * 60_000;
    expect(line({ now: hours(11, 59) })).not.toBeNull();
    expect(line({ now: hours(12) })).not.toBeNull();
    expect(line({ now: hours(12) + 1 })).toBeNull();
  });

  it("gives no information for a raid not in the list, with no crash", () => {
    expect(line({ choice: choice({ activeRaid: "gone" }) })).toBeNull();
    expect(line({ raids: [] })).toBeNull();
    expect(line({ raids: [], awardsRaid: { id: "r1" } })).toBeNull();
  });

  it("never puts a name in the tray line", () => {
    for (const activeBy of ["chosen", "none", "off"]) {
      for (const activeRaid of ["r1", null]) {
        const l = line({ choice: choice({ activeBy, activeRaid }) })!;
        expect(l.trayLine).not.toContain("Aldric");
      }
    }
  });
});
