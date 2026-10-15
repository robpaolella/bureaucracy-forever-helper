# Design: the helper's window

- Design issue: #62 (revision 3 was #44, revision 2 was #33, revision 1 was #2)
- Revision: 4
- Replaces: revision 3 (#44, PR #46)
- Source plan: robpaolella/bureaucracy-forever-addon#3 (plan v3, task 30); covered build issues below

## Job and audience
Guild officers who run the Bureaucracy Forever loot add-on, on their own Mac or Windows PC.
The helper connects the add-on to the guild site: it writes the site's reserves into the
add-on's inbox file and sends the add-on's award lines to the site, live from the chat log and
in full at logout or `/reload`. Only the add-on's award lines leave the computer.

Officers set it up once and then leave it in the tray / menu bar while the game fills the
screen (Robert, answer 2). They open the window only when something needs them: the `/reload`
prompt, an error, an update. The window is for a **quick check**, not for watching. Success:
an officer glances at it (or just the tray icon) and knows the chain is working; when it
isn't, one plain sentence says what's wrong and what to do.

The officer never chooses a raid in the helper (Robert, answers 5a/5b). The helper hands the
add-on every site raid (calendar event) starting within 12 hours or under way; the add-on
picks the one for the dungeon the officer is in, and the officer overrides it in game. The
helper only shows which raids are ready and which raid awards are going to.

## Covers
| Build issue | Part of this feature | Out of scope |
| --- | --- | --- |
| #3 | First-run key step and its errors (malformed, revoked, site switched off, unreachable, secure store failed); key in Settings (replace, remove); the staging setting and its band | Folder (#13); storage code |
| #13 | First-run game folder step: searching, found, several game versions (officer picks), not found (pick by hand), not a game folder, add-on not installed, remembered folder gone; folder in Settings | Reading game files |
| #12 | Reduced by Robert's answer 5: no raid picker. "Reserves ready" list of the raids handed over (none, one, several) and the raid awards are going to | The add-on's in-game pick and override (robpaolella/bureaucracy-forever-addon#9) |
| #4 | Reserves written; the `/reload` prompt (window, tray, notification); last good file kept when a fetch fails | Serialization |
| #6 | Live award lines in status and activity ("sent", including lines the helper carried for the loot master) | Log parsing |
| #14 | Sync state: waiting, sending, sent, offline with awards waiting, a record refused, key revoked or site off mid-upload, damaged queue | Queue code |
| #7 | Helper update: available, downloading/installing, failed (no network, file missing, signature refused, interrupted), never without confirmation | Release workflow (#16), docs (#15) |
| #8 | Add-on update: available (with the game-closed or `/reload` note), installing, installed, rolled back on failure; the Settings switch “Update the add-on automatically” (on by default): with it on, a release installs by itself, the add-on row shows `addon-updated`, and a notification says so; a failed automatic install shows `addon-rolled-back` with its own notification | — |
| #24 | Settings switch “Start the helper when the computer starts”, on by default (Robert, answer 8) | Other settings |
| #60 | How the raid was chosen: the Awards row's raid line and caption and the tray's second line (Revision 4 below) | Reading the file (#5); the live "Going to" (#30) |
| #1 (done) | Tray / menu bar icon and menu: restyled words, order and status | — |

## Boundaries and visual authority
- **Look:** the guild site's `DESIGN.md` and `PRODUCT.md` (`/git/bureaucracy-forever`, commit
  `d3534e4`), reused so officers see the same brand (Robert, 2026-10-13 on #2). Dark only; one
  theme. Tokens from `app/tokens.css`. Fonts: Newsreader (titles), Cinzel (eyebrows), Archivo
  (everything else), inlined from the site's approved design `design/245-unlisted-drop/index.html`.
- **Site components reused** (markup and classes re-expressed in plain CSS, since the helper
  has no React/Tailwind): `Button` (primary sand / secondary / ghost / danger; 44px floor;
  loading spinner), `Field` + `Input` (46px, label-field labels, error replaces hint),
  `Choice` card, `StatusPill`, `Card`, `EmptyState` (with the column mark), `Toggle`,
  `TextLink`, toast style for the notification preview. Focus: 2px teal outline.
- **Accessibility (PRODUCT.md):** 44px targets, 4.5:1 text contrast, status never colour alone,
  real buttons/links/labelled inputs, visible focus, labelled times (officer's local time with
  zone; guild time on hover/focus and in the accessible name).
- **Voice (PRODUCT.md):** plain, confident, no exclamation marks or emoji, familiar wording for
  errors.
- **Icons (Robert, answer 2):** app icon is the site's `public/brand/tile.png`; tray / menu bar
  icon is the column mark `public/brand/mark.png` with a status dot at the top right (green all
  good, amber needs you, red stopped), drawn in a light and a dark version for the Mac menu bar.
- **The tray menu and title bar are the system's own.** The design sets the menu's words,
  order and greyed-out items, not its look.
- **New kinds of element Robert is asked to approve:** system notifications (approved in
  answer 3, only when the officer must act); the window's status header; the staging band.
- Not in scope: the add-on's window (robpaolella/bureaucracy-forever-addon#9), the site's
  screens (robpaolella/bureaucracy-forever#272).

## Window
Fixed 420×640 content area, not resizable (Robert, answer 1). Same on Mac and Windows; the
system draws the title bar ("Bureaucracy Forever Helper"). Closing hides it; Quit is in the
tray menu. Screenshots of every state are at 420×640 (Robert, 2026-10-13 on #2). Open Design
and the standard helper also capture 390×844 and 1440×900: at 1440 the prototype shows the
window at its real size on a plain backdrop with the state bar beside it; at 390 it is scaled
down to fit.

## States, themes and content
One theme (`default`, dark). Sample data only, invented: officer Discord name "Aldric" (the Guild site row reads "Connected as <Discord name>"); raids
"Molten Core, Tue 7:00 PM PDT" and "Blackwing Lair, Tue 9:30 PM PDT"; characters in the
game's "First Last" form (Aldric Vane, Thessaly Ward, Corvin Ashby, Mirela Stone, …); real
Classic item names; Mac and Windows paths (`/Applications/World of Warcraft`,
`D:\Games\World of Warcraft`, a path with spaces and an accented name).

| State slug | Situation | Content and wording (draft) | Actions / feedback |
| --- | --- | --- | --- |
| key-empty | First run | "Connect the helper"; label "Helper key"; hint "Create a key in the officer area of the guild site, then paste it here." | Connect; link "Open the officer area" (browser) |
| key-checking | Checking with the site | Note "Checking your key with the guild site…" | Button shows its spinner, "Checking…" |
| key-malformed | Empty or wrong shape; no site call | "That doesn't look like a helper key. Copy it again from the officer area." | — |
| key-revoked | Site refuses the key (revoked, or owner no longer an officer) | "This key has been revoked or no longer works. Create a new key in the officer area." | — |
| key-site-off | Site has the helper switched off | "The guild site hasn't switched the helper on yet. Try again later." | Try again |
| key-unreachable | Can't reach the site | "Couldn't reach the guild site. Check your internet connection and try again." (a stored key is kept) | Try again |
| key-store-failed | Keychain / Credential Manager refused | Mac: "The helper couldn't save your key in Keychain. Allow access when your Mac asks, then try again." Windows names Credential Manager. Nothing saved elsewhere. | Try again |
| folder-searching | After the key | "Looking for World of Warcraft…" | — |
| folder-found | One match | "Found the game" + path + "Add-on installed" | Use this folder; Choose a different folder |
| folder-several | More than one version has the add-on | "The add-on is installed in more than one game version. Which one do you play?" Choice cards `_classic_` / `_classic_era_` with paths | Continue |
| folder-not-found | Nothing in the usual places | "Couldn't find World of Warcraft. Choose the folder it's installed in." | Choose folder… |
| folder-not-game | Picked folder isn't the game | "That folder isn't a World of Warcraft folder. Choose the folder named World of Warcraft." | Choose folder… |
| folder-no-addon | Game found, no add-on | "World of Warcraft is here, but the Bureaucracy Forever add-on isn't installed." | Install the add-on (Robert, answer 7; uses the #8 installer); Choose a different folder |
| folder-gone | Remembered folder has gone | "The game folder you chose before isn't there any more: D:\Games\World of Warcraft. Choose it again." Never silently replaced | Choose folder… |
| ready | Connected, reserves written, nothing to do | "Ready" ; reserves ready for 2 raids; 0 awards waiting | — |
| reload | Inbox changed | "Type /reload in game" ; "Reserves for Molten Core changed. Type /reload in game to load them." Tray amber; notification | — |
| live | Raid under way, typical | "Sending awards"; "Awards are going to Molten Core, Tue 7:00 PM PDT"; 14 sent; activity list | — |
| live-full | Maximum content | 50 activity lines (a full night of ~40 awards plus events); long names and items | Activity scrolls inside the window |
| no-raids | Nothing within 12 hours | "No raids in the next 12 hours. Reserves will be ready here once a raid is that close." | — |
| offline | Offline with awards waiting | "Offline: 4 awards waiting"; "They'll send when you're back online. Nothing is lost." Last good reserves kept | Tray red; notification once |
| refused | Site refused one record | "The site refused 1 award: Bindings of the Windseeker to Thessaly Ward. Check it on the loot log." The rest still send | Open the loot log |
| revoked | Key stops working mid-use | "Your helper key no longer works. Awards are kept and will send once you add a new key." | Add a new key |
| site-off | Site switched the helper off mid-use | "The guild site has the helper switched off. Awards are kept and will send when it's back on." | — |
| queue-damaged | Damaged queue file | "Sending has stopped" / "The list of awards waiting to send is damaged, so the helper stopped sending. Nothing was deleted. The add-on still has every award, and they send again at your next /reload or logout." (each helper uploads the add-on's full list at logout or /reload, and repeats are harmless) | No action that could empty the list |
| staging | Staging setting on | Permanent band "Staging site"; tray hover text says it | — |
| settings | Settings view | Site toggle "Use the staging site (for testing)"; Helper key "Saved in Keychain" (Replace, Remove); Game folder (Change); "Start the helper when the computer starts" (on); "Update the add-on automatically" (on); versions "Helper 0.2.1 · Add-on 1.3.2" (Check for updates) | — |
| helper-update | Helper update found | "Helper 0.3.0 is ready to install." | Install and restart; Later; What's new |
| helper-installing | Downloading / installing | "Downloading 0.3.0… 45%" | — |
| helper-update-failed | Failed, old version kept | "The update couldn't be installed, so you're still on 0.2.1." + reason: no network / file missing / safety check failed (nothing installed) / download interrupted | Try again |
| addon-update | Add-on update found, with “Update the add-on automatically” off | "Add-on 1.4.0 is ready. Close the game or type /reload after it installs." (from the release notes) | Install; Later |
| addon-installing | Installing | "Installing add-on 1.4.0…" | — |
| addon-updated | Add-on installed by itself (switch on) | Add-on row green, “Updated”, Version 1.4.0; note "It loads the next time you type /reload or start the game." (from the release's “Before you update” notes); Activity line "Add-on updated to 1.4.0" | — |
| addon-rolled-back | Install failed (by hand or automatic) | "The add-on update didn't install, so 1.3.2 is still in place. Your reserves file was kept." | Try again |
| chosen, chosen-live, chosen-no-name, not-loaded, not-chosen, not-guild-run | How the raid was chosen (revision 4) | See Revision 4 below | — |
| tray | Tray icon and menu | Icon in the three statuses, light and dark menu bars; menu: status line (greyed) "Ready · 14 awards sent", "Staging site" when on, Open Bureaucracy Forever Helper, Check for updates, Quit | — |
| notification | System notification preview | "Bureaucracy Forever Helper" / "Reserves for Molten Core are ready. Type /reload in game."; also "Add-on updated to 1.4.0" / "Loads on your next /reload or game start." and "The add-on update didn't install" / "Add-on 1.3.2 is still in place." | Clicking opens the window |

Not applicable: **themes** (dark only); **roles** (only officers have keys; a non-officer's
key is the revoked state); **no permission** (same); **phone/desktop widths** (fixed window).
**Loading** is covered by key-checking, folder-searching and the update/installing states.
Keyboard: every control is a real button, link, input or radio; first-run focus starts in
the key field; errors are announced (`role="alert"`) and tied to their field.

## Host snapshot and offline proof
**Exception: there is no host page.** The helper's window doesn't exist yet (the skeleton
shows placeholder text) and the guild site has no page it lives in. So there is no running
page to snapshot. The versions are built directly from the site's components and tokens
(listed above), with the site's fonts and brand images inlined as `data:` URIs, so each file
works with no server and no network. `host.html` held the shared window shell, fonts and
component styles that each version started from; it was removed at approval (commit f7d8bf7
keeps it), once `index.html` passed its own offline check.

## Confirmed decisions
Robert's answers, 2026-10-13, recorded on #2 (https://github.com/robpaolella/bureaucracy-forever-helper/issues/2#issuecomment-6156569134 and https://github.com/robpaolella/bureaucracy-forever-helper/issues/2#issuecomment-6156645882):
1. Fixed 420×640 window. 2. Status lives in the tray icon and menu, in words; guild logo
icons with a status dot. 3. Notifications only when the officer must act. 4. Activity: one
line per event, newest first, today's and the last 50. 5a/5b. No raid choice in the helper;
the add-on picks by dungeon, override in game. 6. Staging switch in Settings, with a
permanent band. 7. "Install the add-on" when it's missing. 8. Start with the computer, on by
default (new build issue #24).

## Real-site experiment
None.

## Versions
Three structures, each implementing every state (the decisions they differ on are asked one
by one at review):
- **A, Headline:** one status headline (pill, Newsreader title, sentence, action), then
  "Reserves in the game", then the activity list, all on one scrolling screen. Setup goes one
  step at a time ("Step 1 of 2"). Updates appear as a card at the top.
- **B, Chain:** a short title and pill over five links (Guild site, Game folder, Add-on,
  Reserves, Awards), each with its state; the first broken link opens to say what's wrong and
  what to do. Setup fills the same chain in. Activity shows the last three with "See all".
  Add-on updates open the Add-on link; helper updates sit under the chain.
- **C, Tabs:** Status / Activity / Settings tabs. Status leads with the sentence and three
  counts (Sent, Waiting, Refused), then the raids. Setup is one page with both parts.
  Updates are a strip pinned to the bottom.

Independent decisions: (1) main layout; (2) setup flow; (3) where activity lives; (4) where
updates appear.

## Captures
- Real size: `node shoot.mjs <folder> <out>` (kept outside the design folder in
  `/tmp/d2-build/`; same isolated chrome-devtools-axi session handling as design.ts) opens
  each version with `?state=<s>&theme=default&bare=1`, network emulated Offline, viewport
  420×640, waits for the state marker and fonts, and screenshots. Its `network.txt` shows
  only `file:` and `data:` requests.
- Standard: `node /git/maestro/scripts/design.ts shots <folder> --out <dir>` at 390×844 and
  1440×900, for Open Design review and the critique.

## Open decisions
None. Wording of every message is draft for Robert's review in Open Design.

## Revision 4 (issue #62): how the raid was chosen
- Design issue: #62; covers #60. Replaces revision 3 (#44, PR #46) in place, as revisions 2 and 3 did (Robert, 2026-10-15 on #60: "a design revision of `design/2-helper-window`").
- Source: the add-on's saved data (ADDON-SPEC 2.1, site branch `package/addon-3-loot-addon`): `activeRaid`, `activeBy` (`none` / `chosen` / `off`), `chosenBy`. The helper reads it only when the game saves it, at `/reload` or logout (#5), so the line says when it was read.
- Spec facts the wording relies on: **not chosen yet** (`none`) keeps awards in the add-on, never sent, until an officer chooses the raid on its Sync tab; **not a guild run** (`off`) records nothing; **chosen, not loaded** (`chosen`, `activeRaid` nil) means the add-on hasn't the raid's reserves yet and applies the choice when they load. With `activeRaid` nil the helper doesn't know which raid was chosen.

### Covers
| Build issue | Part of this feature | Out of scope |
| --- | --- | --- |
| #60 | The Awards row's raid line and caption, and the tray's second line, for each `activeBy` case | Reading the file (#5); the live "Going to" from the latest award (#30); a raid picker |

### Rules (Robert, 2026-10-15, in this design tab)
1. **The latest award decides the raid shown.** Once awards flow, "Going to <raid>" comes from the latest handled award (#30). "Chosen by <name>" shows only when the saved data names that same raid; otherwise the caption is today's "Picked in game. Change it on the add-on's Sync tab." Before the night's first award the saved data is the only source, so the row shows its raid or state.
2. **Freshness.** The line says when it was read ("As of your last /reload, 8:47 PM", the file's save time in the officer's local time). Saved data older than 12 hours is ignored and the row falls back to today's wording.
3. **Tray line:** raid or state only, no name: "Awards going to Molten Core", "Raid not chosen yet", "Not a guild run" (and the not-loaded wording).
- Unknown or missing `activeBy` (including the old `auto` / `override`): treated as no information, today's wording, no new state. `chosenBy` missing or empty: the raid without a name.

### New states (wording approved: Robert, 2026-10-15, answer 4, "Whatever you recommend.")
| State slug | Situation | Awards row (detail · pill · caption) | Tray second line |
| --- | --- | --- | --- |
| chosen | `chosen`, raid loaded, before any award | "Going to Molten Core" · None yet · "Chosen by Aldric. As of your last /reload, 8:47 PM." | Awards going to Molten Core |
| chosen-live | as `live`, saved data names the same raid | "Going to Molten Core" · Sending · "Chosen by Aldric. Change it on the add-on's Sync tab." | Awards going to Molten Core |
| chosen-no-name | `chosen`, no `chosenBy` | "Going to Molten Core" · None yet · "As of your last /reload, 8:47 PM." | Awards going to Molten Core |
| not-loaded | `chosen`, `activeRaid` nil | "Raid not loaded" · amber · "Aldric chose a raid the add-on hasn't loaded yet. Type /reload in game to load it." | Raid not loaded in game |
| not-chosen | `none` | "Raid not chosen yet" · amber · "Awards wait in the add-on until an officer chooses the raid on its Sync tab." | Raid not chosen yet |
| not-guild-run | `off` | "Not a guild run" · grey · "The add-on records nothing for this group." | Not a guild run |
Each caption also carries the "As of your last /reload" time. Existing `live` stays as the mismatch / stale fallback. Phone/desktop: as the rest of this design (fixed 420×640 window; 390 scaled, 1440 on a backdrop). One theme.

Tone (Robert, 2026-10-15, answer 5, "Whatever you recommend."): not-chosen and not-loaded turn only the Awards row amber; the header, tray dot and notifications are unchanged (an officer may have chosen "Decide later" on purpose, and the add-on shows it in game).

Wording as built (small changes for reading order, same meaning): the time follows the name it qualifies, and the Sync tab line stays on every chosen state. Chosen: "Chosen by Aldric, as of your last /reload, 8:47 PM CDT. Change it on the add-on's Sync tab." Without a name: "Picked in game, as of your last /reload, 8:47 PM CDT. Change it on the add-on's Sync tab." (today's phrase). The other three end with "As of your last /reload, 8:47 PM CDT." The new Awards states never open the row (decision 5), so the header's sentence doesn't show under them.

### Versions (built by `/tmp/d62-build/build.py` from `index.html`, shared states unchanged)
- **A, Awards row:** every case sits in the Awards row's caption.
- **B, On the raid:** a chosen raid is marked in the Reserves list ("Awards go here." and who chose it, under that raid); not loaded, not chosen and not a guild run sit in the Awards row.
- **C, Headline:** the window's headline names the raid or the case ("Ready for Molten Core", "Raid not chosen yet", "Not a guild run"), with the chosen-by line under it; the Awards row keeps only its short words and pill.
Independent decision: (6) where the line sits. Tray line: the same in every version.

Chosen (Robert, 2026-10-15): **A**, the Awards row. B and C were dropped (decisions.md, Revision 4).

### Captures
- 390 and 1440: `node /git/maestro/scripts/design.ts shots <folder> --out <dir>`.
- 420×640: `/tmp/d62-build/shoot.sh <folder> <out> <state>…` (kept outside the design folder): an isolated chrome-devtools-axi session opens `index.html?state=<s>&theme=default&bare=1`, resizes to 420×640, waits for the state marker and fonts, and screenshots; then an offline pass (network emulated Offline) writes `network.txt`, which lists only `file:` and `data:` requests.

### Open decisions
None.
