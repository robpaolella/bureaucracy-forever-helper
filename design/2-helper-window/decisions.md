# Design decisions: the helper's window

- Design issue: #44 (revision 3; revision 2 was #33, revision 1 was #2); covered build issues: #3, #4, #6, #7, #8, #12, #13, #14, #24, #26, #27, #28, #29, #30, #31
- Revision: 3; replaces: revision 2 (#33)
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

## Experiment
None.

## Open items
None affecting the approved scope. Answer 5 (no raid choice in the helper) changes other repos'
issues; the helper conductor was asked to carry it (#2 comment 6156569134).

## Build handoff
Merged folder/revision is the feature authority; DESIGN.md/live site govern site-wide
look. Reconcile issues and paused branches in prep before marking ready. Without a
conductor, the design session owns that follow-up. Later revisions use new issues/PRs.
