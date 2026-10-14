import { describe, expect, it } from "vitest";
import { FIRST_RELEASE_LINE, REWORD_MARK, nextVersion, plan } from "./plan.ts";
import * as pr from "./fixtures/prs.ts";

describe("nextVersion", () => {
  it("makes a fix a patch", () => expect(nextVersion("1.2.3", [pr.fixPr])).toBe("1.2.4"));
  it("makes a feat a minor, and the higher bump wins", () => {
    expect(nextVersion("1.2.3", [pr.featPr])).toBe("1.3.0");
    expect(nextVersion("1.2.3", [pr.fixPr, pr.featPr, pr.docsPr])).toBe("1.3.0");
  });
  it("makes a ! title or a BREAKING CHANGE footer a major", () => {
    expect(nextVersion("1.2.3", [pr.breakingBangPr])).toBe("2.0.0");
    expect(nextVersion("1.2.3", [pr.breakingFooterPr, pr.fixPr])).toBe("2.0.0");
  });
  it("accepts a v prefix on the last version", () => expect(nextVersion("v1.2.3", [pr.fixPr])).toBe("1.2.4"));
  it("gives no release for docs-only history", () => {
    expect(nextVersion("1.2.3", [pr.docsPr, pr.choreWithNotePr])).toBeUndefined();
  });
  it("gives no release when nothing is new", () => {
    expect(nextVersion("1.2.3", [])).toBeUndefined();
    expect(nextVersion(undefined, [])).toBeUndefined();
  });
  it("makes the first published release 1.0.0, whatever merged", () => {
    expect(nextVersion(undefined, [pr.fixPr])).toBe("1.0.0");
    expect(nextVersion(undefined, [pr.breakingBangPr])).toBe("1.0.0");
  });
  it("gives no first release when only non-release types merged", () => {
    expect(nextVersion(undefined, [pr.docsPr, pr.choreWithNotePr])).toBeUndefined();
  });
  it("counts a commit with no pull request by its subject", () => {
    expect(nextVersion("1.2.3", [pr.bareCommit])).toBe("1.2.4");
  });
  it("ignores a BREAKING CHANGE footer on a bare commit", () => {
    expect(nextVersion("1.2.3", [{ title: "chore: x", body: "BREAKING CHANGE: y" }])).toBeUndefined();
  });
  it("counts a repeated pull request once", () => {
    expect(plan({ previous: "1.0.0", items: [pr.fixPr, pr.fixPr] })?.notes).toBe(
      "## Fixed\n- The tray icon now stays after you close the window.\n",
    );
  });
  it("rejects a previous version that is not a version", () => {
    expect(() => nextVersion("latest", [pr.fixPr])).toThrow("not a version");
  });
});

describe("notes", () => {
  it("groups Release note lines under Before you update, New and Fixed", () => {
    const result = plan({
      previous: "1.0.0",
      items: [pr.fixPr, pr.featPr, pr.breakingBangPr, pr.manualStepPr, pr.docsPr],
    });
    expect(result).toEqual({
      version: "2.0.0",
      notes: [
        "## Before you update",
        "- Update the add-on before the helper.",
        "- quit the helper from the tray first.",
        "",
        "## New",
        "- Loot now uploads to the guild site by itself.",
        "",
        "## Fixed",
        "- The tray icon now stays after you close the window.",
        "",
      ].join("\n"),
      reword: [],
    });
  });
  it("lists a non-feat, non-fix PR that has a real note", () => {
    const result = plan({ previous: "1.0.0", items: [pr.fixPr, pr.choreWithNotePr] });
    expect(result?.notes).toContain("- Error messages are clearer.");
  });
  it("falls back to the title, marked for Robert, when the note is missing", () => {
    const result = plan({ previous: "1.0.0", items: [pr.featNoNotePr, pr.bareCommit] });
    expect(result?.notes).toBe(
      `## New\n- feat: show the version${REWORD_MARK}\n\n## Fixed\n- fix: typo straight on main${REWORD_MARK}\n`,
    );
    expect(result?.reword).toEqual(["feat: show the version", "fix: typo straight on main"]);
  });
  it("does not list docs PRs with no note", () => {
    expect(plan({ previous: "1.0.0", items: [pr.fixPr, pr.docsPr] })?.notes).not.toContain("docs");
  });
  it("opens the first release with the fresh-install line", () => {
    const result = plan({ items: [pr.featPr] });
    expect(result?.version).toBe("1.0.0");
    expect(result?.notes).toBe(
      `## Before you update\n${FIRST_RELEASE_LINE}\n\n## New\n- Loot now uploads to the guild site by itself.\n`,
    );
  });
  it("has no Before you update section on later releases without breaking changes", () => {
    expect(plan({ previous: "1.0.0", items: [pr.featPr] })?.notes).not.toContain("Before you update");
  });
  it("reports no release when none is due", () => {
    expect(plan({ previous: "1.0.0", items: [pr.docsPr] })).toBeUndefined();
  });
});
