import { describe, expect, it } from "vitest";
import { initialState } from "./realWindow";
import { trayStatusOf } from "./tray";

describe("trayStatusOf", () => {
  it("passes the status, count and staging on, with no second line by default", () => {
    const status = trayStatusOf({
      ...initialState,
      status: { tone: "ok", word: "Sending", title: "Everything’s working" },
      sentTonight: 14,
      staging: true,
    });
    expect(status).toEqual({ tone: "ok", word: "Sending", sentTonight: 14, staging: true, secondLine: null });
  });

  it("gives the second line as it was given, and leaves a blank one out", () => {
    expect(trayStatusOf({ ...initialState, awardsLine: "Raid not chosen yet" }).secondLine).toBe("Raid not chosen yet");
    expect(trayStatusOf({ ...initialState, awardsLine: "  " }).secondLine).toBeNull();
  });

  it("starts as not set up with staging off", () => {
    expect(trayStatusOf(initialState)).toMatchObject({ tone: "closed", word: "Not set up", staging: false });
  });
});
