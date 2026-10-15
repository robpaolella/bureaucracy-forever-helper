# Design decisions: the helper's window

- Design issue: #62 (revision 4; revision 3 was #44, revision 2 was #33, revision 1 was #2); covered build issues: as in `design.json`, now including #60
- Revision: 4; replaces: revision 3 (#44)
- Maker: claude-bridge/claude-opus-5-5 (Opus 5.5), thinking medium

## Choice and approval
Robert chose **B (Chain)** for all four decisions. His confirmation, 2026-10-13: "I agree,
I like option B across the board. Let's go with it. Approved."

The versions differ on four independent decisions (brief.md, Versions), each asked as its
own numbered question:

| # | Decision | Options (version) | Robert's pick and why |
| --- | --- | --- | --- |
| 1 | Main layout | A: status headline over reserves and activity; B: chain of five links, broken link opened; C: tabs with counts first | B: shows at a glance which link broke, the main reason to open the helper |
| 2 | Setup flow | A: one step at a time; B: the chain fills in; C: one page with both parts | B: setup and everyday use look the same |
| 3 | Where activity lives | A: on the main screen; B: last three plus "See all"; C: its own tab | B: enough to confirm things flow without crowding the status |
| 4 | Where updates appear | A: card at the top; B: inside the Add-on link (helper update under the chain); C: strip at the bottom | B: the update sits beside what it changes |

Approved file: `index.html` (version B, with the two additions below). Evidence:
`screenshots/approved/` (390 and 1440) and `screenshots/approved/real-size/` (420×640), every
state, default theme, captured offline (`network.txt` shows only `file:` and `data:`).

Design PR: use the linked PR on the design issue once created. Only Robert merging that PR
approves it for building.

## Revision 2 (2026-10-14, issue #33)
#26–#31 were filed after revision 1 merged, so the design gate could not clear for them.
Robert, 2026-10-14, in the conductor chat: "Agreed to all. Proceed." His answers, recorded
on [#28](https://github.com/robpaolella/bureaucracy-forever-helper/issues/28#issuecomment-6161620997)
and [#13](https://github.com/robpaolella/bureaucracy-forever-helper/issues/13#issuecomment-6161621986):
- Design gate (Q14): "design revision 2 adds #26–#31 to the covered list." `design.json` now
  covers them and names #33 as its `design_issue`, so the gate checks this revision's PR.
- Key name (Q15): "show "Connected as <Discord name>" instead of the design's `Key "Rob's PC"`;
  revision 2 records the wording." The Guild site row in the connected state now reads
  "Connected as Aldric" (sample name); the site answers the key owner's Discord name, not a
  key label. The only change to `index.html`, so only states showing that row had screenshots
  retaken (both sets, 390, 1440 and real-size 420×640, and the `b-*` copies in `all/`).
- Remove key (Q16): "no confirmation. It returns to the key step and keeps awards waiting
  to send."
- Install button (Q20): "ship without the "Install the add-on" button; #8 adds it. Until #8
  lands the `folder-no-addon` state differs from the design only by that button." Builds of
  #13 follow this; it is not a design change.
No other design change. Screenshots were retaken with a newer browser, so unchanged pixels
inside retaken images may differ slightly from revision 1.

## Revision 3 (2026-10-14, issue #44): update the add-on automatically
Robert, on #8, choosing option B ([comment](https://github.com/robpaolella/bureaucracy-forever-helper/issues/8#issuecomment-6174002730)): "B, agreed." Option B: an "Update the add-on automatically" switch in Settings, on by default; new versions install by themselves and the officer gets a notice to /reload or restart. Add-on updates then work like CurseForge's.
- **Settings:** a second switch, "Update the add-on automatically", on by default, the same switch piece and layout as "Start the helper when the computer starts".
- **Switch on:** a release installs by itself (`addon-installing` shows while it runs), then the new state `addon-updated`: Add-on row green with the new version, a one-line note, an Activity line "Add-on updated to 1.4.0", and one system notification.
- **Switch off:** `addon-update` is unchanged (Install, Later, What's new).
- **Failed automatic install:** `addon-rolled-back`, plus a system notification ("The add-on update didn't install. Add-on 1.3.2 is still in place.").
- **Wording rule:** WoW loads add-on files only at login or /reload, so installing while the game runs is safe; the officer chooses when to reload. The notice never tells them to reload now; it says the update loads the next time they type /reload or start the game. The note's text comes from the release's "Before you update" notes. All wording is draft for Robert's review in Open Design.
- **Notification set:** `notification` now shows seven notices. To fit them in 420×640 the prototype tightens the list's spacing and the explanatory caption sits at the bottom and scrolls. Existing notice wording is unchanged. The caption now says which add-on notices are sent.
- **No new kind of element:** the switch, row, notification and Activity line already existed.
- **Screenshots:** retaken for `settings`, `addon-updated` and `notification` (390, 1440 and real-size 420×640, in `approved/` and the `b-*` copies in `all/`), with new manifests; no other state changed. The 420×640 pictures were taken with viewport 420×640 and `?state=<s>&theme=default&bare=1`. `network.txt` is a separate offline pass over the same three states (the screenshot tool fails while network emulation is Offline); it lists only `file:` and `data:` requests.

## Alternatives
A and C were left out. Their final files are in commit f7d8bf7 (`versions/a.html`,
`versions/c.html`); screenshots of every state are in `screenshots/all/` (390 and 1440) and
`screenshots/all/real-size/` (420×640), named `a-*` and `c-*`.

What only A or C had, and what happened to it (asked as points 5 to 8, 2026-10-13; Robert:
"Yes to the recommendation."):
- Raid start times (A, C): **kept**, folded into B as a line per raid under the Reserves row.
- "Change it on the add-on's Sync tab" (A): **kept**, under the Awards row whenever awards
  have a raid.
- Tonight's sent/waiting/refused counts on the main screen (A, C): **dropped**. B flags
  waiting and refused in the Awards row; the sent count shows on the Activity screen.
- A's large headline and full activity list, C's tabs, count tiles and bottom update strip:
  **dropped**, as the options Robert chose against.

## Independent critique
Route: `node /git/maestro/routing/route.ts role checker --maker claude-bridge/claude-opus-5-5`
gave GPT-5.6 Terra (`--model openai/gpt-5.6-terra --thinking medium`).

- Command: `node /git/maestro/scripts/read-only-run.ts "$PWD" review /git/maestro/skills/design-feature/reviewer-prompt.md "Design: …/design/2-helper-window. Images and manifest: /tmp/design-review-m82241/versions (also real-size 420x640 captures in …/real-size). Maker: claude-bridge/claude-opus-5-5." -- --model openai/gpt-5.6-terra --thinking medium`
- Verdict (2026-10-13): **PASS**, no fixes. All 204 reference-width and 102 real-size
  (420×640) images opened; 34 states × 3 versions × default covered; workspace unchanged.
- PR review (2026-10-13), `node /git/maestro/routing/route.ts role risky-review` gave GPT-6
  Astra (`--model openai/gpt-6-astra --thinking low`): two Blocking items (caption contrast
  under 4.5:1; three links under 44px), fixed as recorded under Comments and changes.
- Stated limits: read-only image and HTML review; keyboard use, screen readers, real
  tray behaviour, file picker and updates are proven only in the builds.

## Comments and changes
No Open Design comments (checked 2026-10-13 before approval).

Changes after the critique, both from Robert's points 5 and 6 (content A already had, so no
new critique): B's Reserves row now reads "Next 12 hours" with each raid and its start time
beneath, and the Awards row adds "Picked in game. Change it on the add-on's Sync tab." The
chain list was set not to shrink, so the longer rows never clip the Awards row. Prototype
fix: real-size (`bare=1`) captures no longer apply the narrow-screen scaling, which had
shifted the window 12px.

Fixes from the PR review (GPT-6 Astra, low; see Independent critique), applied to `index.html`
only: caption, hint, placeholder, label and time text moved from `fg-3` to the site's existing
`fg-muted` (#8D97A8; 5.4:1 or better on every helper surface, where `fg-3` fell to 3.77:1;
the site's tokens.css limits `fg-3` to the darkest ground). "Open the officer area", "What's
new", "See all" and "Other reasons it can fail" now have 44px-tall click areas, measured in
the browser (147×44, 81×44, 80×44, 354×44). Approved screenshots retaken.

## Covers #39 and #40
After revision 2 merged, the conductor split two issues for size: #39 (chain list, Settings,
Activity) is part 2 of #26, and #40 (sync states) is split from #31. They show the same
screens as #26 and #31 did, so no screen changes and no new decision. `design.json` now also
lists 39 and 40 in `covers`; the revision stays 2.

## Covers #49, #52, #53 and #56
After revision 3 merged, the conductor split four issues for size: #49 (add-on update
screens) is part 2 of #8; #53 (game folder step and Settings section) is part 2 of #13; and
#28 was split twice: #56 (first-run key step) is part 2 of #28 and #52 (Settings key section
and staging) is part 3, while #28 itself now only wires the window. They show the same
screens as #8, #13 and #28 did, so no screen changes and no new decision. `design.json` now
also lists 49, 52, 53 and 56 in `covers`; the revision stays 3.

## Experiment
None.

## Open items
None affecting the approved scope. Answer 5 (no raid choice in the helper) changes other repos'
issues; the helper conductor was asked to carry it (#2 comment 6156569134).

## Build handoff
Merged folder/revision is the feature authority; DESIGN.md/live site govern site-wide
look. Reconcile issues and paused branches in prep before marking ready. Without a
conductor, the design session owns that follow-up. Later revisions use new issues/PRs.

## Revision 4 (2026-10-15, issue #62): how the raid was chosen
- Maker: claude-bridge/claude-opus-5-5 (Opus 5.5).
- Robert's answers in this design tab, 2026-10-15 (questions in brief.md, Revision 4): 1 "Whatever you recommend." (the latest award decides the raid; the name only when the saved data names the same raid); 2 "Whatever you recommend." (show the read time; ignore saved data older than 12 hours); 3 "Agreed." (tray line without a name); 4 "Whatever you recommend." (wording table); 5 "Whatever you recommend." (amber on the Awards row only).
- Independent critique: `node /git/maestro/routing/route.ts role checker --maker claude-bridge/claude-opus-5-5` gave GPT-5.6 Terra (`--model openai/gpt-5.6-terra --thinking medium`). Command: `node /git/maestro/scripts/read-only-run.ts "$PWD" review /git/maestro/skills/design-feature/reviewer-prompt.md "Design: …/design/2-helper-window (revision 4 …). Images and manifest: /tmp/design-review-NxOiH8/versions. Maker: claude-bridge/claude-opus-5-5." -- --model openai/gpt-5.6-terra --thinking medium`. Verdict (2026-10-15): one fix: (1) B's chosen-raid note used a 2px green left border, against the craft floor. Fixed: indent only, no border. Re-check of fix 1: **PASS**. All 246 images decoded; workspace unchanged both runs.
- Decision 6 (where the line sits): Robert, 2026-10-15: "A is fine." (A, the Awards row; recommended).
- Open Design comments: none (checked 2026-10-15).
- Dropping B and C (point 7, asked 2026-10-15, what only each had: B, the "Awards go here" note under the chosen raid in the Reserves list; C, the headline naming the raid or the case). Robert: "Drop them." Their screenshots for every revision-4 state are in `screenshots/all/revision-4/` (`b-*`, `c-*`, 390 and 1440, with `manifest.json`); their files are in commit cf0354f (`versions/b.html`, `versions/c.html`, on branch `docs/62-design-raid-chosen`). Every other state in B and C is identical to revision 3, since all three were built from the approved `index.html`; so `revision-4/` keeps only the states that differ (the six new states and `tray`). Revision 1's `a-*`/`c-*` files in `all/` are untouched.
- **Approval:** Robert, 2026-10-15: "Approved." (point 8). Chosen: version A, which `index.html` now is (its prototype note names the Awards row placement). Only Robert's merge of the design PR approves it for building.
- **Screenshots:** new or changed states only (`chosen`, `chosen-live`, `chosen-no-name`, `not-loaded`, `not-chosen`, `not-guild-run`, `tray`'s caption) were copied into `approved/` (390, 1440) and `approved/real-size/` (420×640), with new manifests over every state. Other states keep their earlier screenshots: recaptured, their window content is identical, and only the newer browser's text rendering differs. `real-size/network.txt` is this revision's offline pass, listing only `file:` and `data:` requests.
- **Rules for the build (#60):** the six states, the wording and the rules in brief.md, Revision 4: the latest award decides the raid, "Chosen by" only when the saved data names that raid; the read time; saved data older than 12 hours ignored; unknown or missing `activeBy` falls back to today's wording; the new Awards states never open the row and leave the header, tray dot and notifications alone.
- Open items: none affecting the approved scope.
