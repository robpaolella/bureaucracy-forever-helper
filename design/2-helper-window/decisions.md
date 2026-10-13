# Design decisions: the helper's window

- Design issue: #2; covered build issues: #3, #4, #6, #7, #8, #12, #13, #14, #24
- Revision: 1; replaces: none
- Maker: claude-bridge/claude-opus-5-5 (Opus 5.5), thinking medium

## Choice and approval
Pending: Robert hasn't seen the versions yet. The versions differ on four independent
decisions (brief.md, Versions), each to be asked as its own numbered question:

| # | Decision | Options (version) | Robert's pick and why |
| --- | --- | --- | --- |
| 1 | Main layout | A: status headline over reserves and activity; B: chain of five links, broken link opened; C: tabs with counts first | pending |
| 2 | Setup flow | A: one step at a time; B: the chain fills in; C: one page with both parts | pending |
| 3 | Where activity lives | A: on the main screen; B: last three plus "See all"; C: its own tab | pending |
| 4 | Where updates appear | A: card at the top; B: inside the Add-on link (helper update under the chain); C: strip at the bottom | pending |

Design PR: use the linked PR on the design issue once created. Only Robert merging that PR
approves it for building.

## Alternatives
Pending Robert's choice.

## Independent critique
Route: `node /git/maestro/routing/route.ts role checker --maker claude-bridge/claude-opus-5-5`
gave GPT-5.6 Terra (`--model openai/gpt-5.6-terra --thinking medium`).

- Command: `node /git/maestro/scripts/read-only-run.ts "$PWD" review /git/maestro/skills/design-feature/reviewer-prompt.md "Design: …/design/2-helper-window. Images and manifest: /tmp/design-review-m82241/versions (also real-size 420x640 captures in …/real-size). Maker: claude-bridge/claude-opus-5-5." -- --model openai/gpt-5.6-terra --thinking medium`
- Verdict (2026-10-13): **PASS**, no fixes. All 204 reference-width and 102 real-size
  (420×640) images opened; 34 states × 3 versions × default covered; workspace unchanged.
- Stated limits: read-only image and HTML review; keyboard use, screen readers, real
  tray behaviour, file picker and updates are proven only in the builds.

## Comments and changes
None yet.

## Experiment
None.

## Open items
None affecting the scope. Answer 5 (no raid choice in the helper) changes other repos'
issues; the helper conductor was asked to carry it (#2 comment 6156569134).

## Build handoff
Merged folder/revision is the feature authority; DESIGN.md/live site govern site-wide
look. Reconcile issues and paused branches in prep before marking ready. Without a
conductor, the design session owns that follow-up. Later revisions use new issues/PRs.
