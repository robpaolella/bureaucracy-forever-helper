// Recorded pull request data (titles and bodies as GitHub returns them), shaped like the
// items the command-line wrapper builds. Tests use this, never the live API.
import type { Item } from "../plan.ts";

export const fixPr: Item = {
  number: 21,
  title: "fix: keep the tray icon after the window closes",
  body: "## What\nStops the tray vanishing.\n\nRelease note: The tray icon now stays after you close the window.\nRelease: patch, 1.0.0 to 1.0.1 when Robert publishes",
};
export const featPr: Item = {
  number: 22,
  title: "feat(sync): upload loot to the guild site",
  body: "Release note: Loot now uploads to the guild site by itself.\r\nRelease: minor",
};
export const breakingBangPr: Item = {
  number: 23,
  title: "feat(sync)!: new loot file format",
  body: "Release note: Update the add-on before the helper.\n\nBREAKING CHANGE: the file format changed.",
};
export const breakingFooterPr: Item = {
  number: 24,
  title: "refactor: rename the settings file",
  body: "Release note: Your settings are reset once.\n\nBREAKING CHANGE: settings file renamed.",
};
export const manualStepPr: Item = {
  number: 25,
  title: "feat: start with the computer",
  body: "Release note: Before you update: quit the helper from the tray first.",
};
export const docsPr: Item = {
  number: 26,
  title: "docs: explain the install",
  body: "Release note: none",
};
export const choreWithNotePr: Item = {
  number: 27,
  title: "chore: better error text",
  body: "Release note: Error messages are clearer.",
};
export const featNoNotePr: Item = {
  number: 28,
  title: "feat: show the version",
  body: "Release note:\nRelease: minor",
};
export const bareCommit: Item = { title: "fix: typo straight on main" };
