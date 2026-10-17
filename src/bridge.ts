// The only place the front-end calls Tauri. Everything else takes a `Bridge`, so tests
// can hand it a fake. The result shapes mirror `KeyOutcome` and `ChangeOutcome` in
// src-tauri/src/commands.rs, `FolderOutcome` in src-tauri/src/game_folder.rs, `SyncStatus` in src-tauri/src/upload.rs, `ReservesView` in src-tauri/src/reserves.rs,
// the update outcomes in src-tauri/src/updater.rs, the add-on update outcomes in
// src-tauri/src/addon_update.rs, `TrayStatus` and `Notice` in tray.rs and notify.rs, and
// `ChatEvent` in src-tauri/src/chatlog/follower.rs.
import { getVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";

export type KeyOutcome =
  | {
      kind: "accepted";
      /** The key owner's Discord name, for "Connected as <name>". */
      officer: string;
      env: string;
      formatVersions: number[];
      addonVersion: string;
      addonUrl: string;
    }
  | { kind: "refused"; reason: "badKey" | "notOfficer"; message: string }
  | { kind: "siteOff"; message: string }
  | { kind: "stagingNotOpen"; message: string }
  | { kind: "unreachable"; message: string }
  | { kind: "storeFailed"; message: string }
  | { kind: "malformed"; reason: "empty" | "malformed"; message: string }
  /** Checking the stored key, but none is stored for the current site. */
  | { kind: "noKey" };

export type ChangeOutcome =
  | { kind: "done" }
  | { kind: "storeFailed"; message: string }
  | { kind: "settingsFailed"; message: string };

/** One game version with the add-on installed. */
export interface Install {
  gameFolder: string;
  /** The subfolder for the version played, such as `_retail_`. */
  version: string;
  /** The game folder and version together: the folder that is remembered. */
  versionFolder: string;
  addonFolder: string;
}

/** What looking at a game folder found; the error wording is in `message`. */
export type FolderOutcome =
  | ({ kind: "found" } & Install)
  | { kind: "several"; installs: Install[] }
  | { kind: "notFound"; message: string }
  | { kind: "notGame"; message: string }
  /** `versionFolders` are the game versions in `path` that the add-on can be installed into. */
  | { kind: "noAddon"; message: string; path: string; versionFolders: string[] }
  | { kind: "gone"; message: string; path: string }
  | { kind: "nothingRemembered" }
  | { kind: "settingsFailed"; message: string };

/** A record kept out of the upload. */
export interface RefusedRecord {
  id: string;
  /** The game item id and character, when the record is an award or a replacement. */
  item: number | null;
  character: string | null;
  /** The site's `rejected` code, or `too-large`. */
  code: string;
}

/** What the window and tray show about the upload. */
export type SyncStatus =
  /** Nothing waiting that can be sent now; `otherSite` records wait for the other site. */
  | { kind: "idle"; otherSite: number }
  | { kind: "sending"; raid: string }
  /** The site can't be reached; this many wait for it. */
  | { kind: "offline"; waiting: number }
  | { kind: "refused"; records: RefusedRecord[]; otherSite: number }
  /** The site refused the key; nothing is sent until the key or site changes. */
  | { kind: "keyStopped" }
  | { kind: "siteOff" }
  | { kind: "queueDamaged"; problem: string }
  /** No key is stored for the current site. */
  | { kind: "noKey" }
  /** The secure store couldn't be read. */
  | { kind: "keyUnreadable"; message: string };

/** The event that carries each new sync status (`SYNC_STATUS_EVENT` in upload/runner.rs). */
const SYNC_STATUS_EVENT = "sync-status";

/** One upcoming site raid (`Raid` in raids.rs). `startsAt` is Unix seconds. */
export interface Raid {
  id: string;
  code: string;
  name: string;
  startsAt: number;
  durationMin: number;
  /** The game instance number; null when the raid's template has none. */
  instance: number | null;
  hasLootTable: boolean;
}

/** How the raids list stands. Raids are the last good list in every state but `loading`,
 * `noKey` and `keyUnreadable`, which have none. */
export type RaidsState =
  | { kind: "loading" }
  | { kind: "fresh" }
  /** The site returned no raids, which is a good answer. */
  | { kind: "empty" }
  /** The last fetch failed; the last good list is kept and the next tick tries again. */
  | { kind: "unreachable" }
  | { kind: "noKey" }
  /** The site refused the key; nothing is fetched until the key or site changes. */
  | { kind: "keyRefused" }
  | { kind: "siteOff" }
  | { kind: "stagingNotOpen" }
  | { kind: "keyUnreadable"; message: string };

export interface RaidsView {
  state: RaidsState;
  raids: Raid[];
}

/** The event that carries each new raids view (`RAIDS_EVENT` in raids.rs). */
const RAIDS_EVENT = "raids";

/** How the reserves stand in the game (`ReservesState` in reserves.rs). */
export type ReservesState =
  /** Nothing written yet, or no raids to prepare. */
  | { kind: "waiting" }
  /** These raids' reserves are in the add-on's inbox and nothing has changed since. */
  | { kind: "inGame"; raids: string[] }
  /** The inbox changed and the game hasn't read it: `changed` are the raids that did. */
  | { kind: "reload"; raids: string[]; changed: string[] };

export interface ReservesView {
  state: ReservesState;
  /** Goes up with every write the officer should hear about, so each is announced once. */
  changes: number;
  /** Whether the latest counted write was the first. */
  first: boolean;
  /** Why the latest write was refused, in plain words; the old file is as it was. */
  problem: string | null;
}

/** The event that carries each new reserves view (`RESERVES_EVENT` in reserves.rs). */
const RESERVES_EVENT = "reserves";

/** Why an update check or install failed; the wording is in `message`. Any failure leaves the
 * installed version working. */
export type UpdateFailure =
  | "offline"
  | "missing"
  | "badSignature"
  | "interrupted"
  | "checkFailed"
  | "installFailed";

export type CheckOutcome =
  /** `notesUrl` is the release page with what's new. */
  | { kind: "available"; version: string; notesUrl: string }
  | { kind: "upToDate" }
  | { kind: "failed"; reason: UpdateFailure; message: string };

export type InstallOutcome =
  /** Installed (on Windows, downloaded and checked); `restartAfterUpdate` finishes it. */
  | { kind: "ready"; version: string }
  /** No earlier check found an update. */
  | { kind: "nothingFound" }
  /** An install is already under way. */
  | { kind: "busy" }
  | { kind: "failed"; reason: UpdateFailure; message: string };

/** A restart that happens doesn't answer, so these are the only answers the window gets. */
export type RestartOutcome =
  /** No update is ready to restart into. */
  | { kind: "notReady" }
  /** Windows wouldn't start the installer; the helper carries on as it was. */
  | { kind: "failed"; reason: UpdateFailure; message: string };

/** Download progress in bytes; `total` is null when the size isn't known. */
export interface UpdateProgress {
  downloaded: number;
  total: number | null;
}

/** The event that carries download progress (`PROGRESS_EVENT` in updater.rs). */
const UPDATE_PROGRESS_EVENT = "update-progress";

/** The event the tray's "Check for updates" sends (`CHECK_EVENT` in tray.rs). */
const TRAY_CHECK_EVENT = "tray-check-for-updates";

/** Why an add-on check or install failed; the wording is in `message`. The old add-on and its
 * inbox are always left as they were. */
export type AddonFailure =
  | "site"
  | "noRelease"
  | "offline"
  | "badDownload"
  | "notGameFolder"
  | "installFailed"
  /** The old add-on couldn't be moved back yet; the next check or install moves it back. */
  | "notRestored";

/** The add-on's latest version against the installed one; `installed` is null when there is none. */
export type AddonCheck =
  /** `note` is the release's "Before you update" section, null when it has none. */
  | { kind: "checked"; latest: string; note: string | null; installed: string | null; upToDate: boolean }
  | { kind: "failed"; reason: AddonFailure; message: string; installed: string | null };

export type AddonInstall =
  | { kind: "installed"; version: string }
  /** No earlier check found a release. */
  | { kind: "nothingFound" }
  /** An install is already under way. */
  | { kind: "busy" }
  /** `kept` is the version still in place, null when there was none. Nothing tries again by itself. */
  | { kind: "notInstalled"; reason: AddonFailure; message: string; kept: string | null };

/** Where an add-on install is; `total` is null when the download's size isn't known. */
export type AddonProgress =
  | { step: "downloading"; version: string; downloaded: number; total: number | null }
  | { step: "installing"; version: string };

/** The event that carries add-on install progress (`PROGRESS_EVENT` in addon_update.rs). */
const ADDON_PROGRESS_EVENT = "addon-progress";

/** A chat log line whose operation the site acknowledged (`SentLine` in chatlog/follower.rs). */
export interface SentLine {
  line: "award" | "replacement" | "undo";
  /** The game item id and name from the line's item link. */
  item: number | null;
  itemName: string | null;
  /** The winner; null for an undo or a disenchant. */
  character: string | null;
  /** The boss's name; null for an undo. */
  boss: string | null;
  raid: string;
  raidName: string;
  /** When the line was posted, Unix seconds. */
  at: number;
}

/** What the chat log follower reports: a sent line, or an add-on line it couldn't use. */
export type ChatEvent =
  | ({ kind: "sent" } & SentLine)
  | { kind: "ignored"; reason: "malformed" | "newerVersion" };

/** The event that carries each chat log event (`CHAT_LOG_EVENT` in chatlog/follower.rs). */
const CHAT_LOG_EVENT = "chat-log";

/** What the window tells the tray (`TrayStatus` in src-tauri/src/tray.rs). */
export interface TrayStatus {
  /** The status pill's tone; `closed` ("Not set up") shows as amber. */
  tone: "ok" | "warn" | "stop" | "closed";
  word: string;
  sentTonight: number;
  staging: boolean;
  /** The finished second menu line; null leaves it out. */
  secondLine: string | null;
}

/** A system notification: a bold headline and one line of detail (`Notice` in notify.rs). */
export interface Notice {
  headline: string;
  detail: string;
}

/** Each call rejects only if the Rust side itself failed; every engine result is data. */
export interface Bridge {
  connect(key: string): Promise<KeyOutcome>;
  checkStored(): Promise<KeyOutcome>;
  removeKey(): Promise<ChangeOutcome>;
  staging(): Promise<boolean>;
  setStaging(on: boolean): Promise<ChangeOutcome>;
  syncStatus(): Promise<SyncStatus>;
  /** The upcoming raids and how the list stands now. */
  raids(): Promise<RaidsView>;
  /** How the reserves stand in the game now. */
  reserves(): Promise<ReservesView>;
  /** Looks in the usual places for the game. */
  searchFolder(): Promise<FolderOutcome>;
  /** Checks a folder the officer picked. */
  checkPickedFolder(folder: string): Promise<FolderOutcome>;
  /** Remembers a game version folder, once it checks out. */
  rememberVersion(versionFolder: string): Promise<FolderOutcome>;
  rememberedFolder(): Promise<FolderOutcome>;
  /** Opens the native folder picker; resolves to the chosen folder, or null if cancelled. */
  pickFolder(): Promise<string | null>;
  /** Calls `listener` with every later sync status; resolves to a function that stops it. */
  onSyncStatus(listener: (status: SyncStatus) => void): Promise<() => void>;
  /** Calls `listener` with every later raids view; resolves to a function that stops it. */
  onRaids(listener: (view: RaidsView) => void): Promise<() => void>;
  /** Calls `listener` with every later reserves view; resolves to a function that stops it. */
  onReserves(listener: (view: ReservesView) => void): Promise<() => void>;
  /** Calls `listener` with each line the chat log follower sent or ignored; resolves to a function that stops it. */
  onChatLog(listener: (event: ChatEvent) => void): Promise<() => void>;
  /** Asks GitHub for a newer helper. Downloads and installs nothing. */
  checkForUpdate(): Promise<CheckOutcome>;
  /** Downloads and installs the update the last check found. Call only once the officer
   * confirms. On Windows it downloads and checks it; the restart runs the installer. */
  installUpdate(): Promise<InstallOutcome>;
  /** Restarts into the update `installUpdate` made ready; does nothing unless one is. */
  restartAfterUpdate(): Promise<RestartOutcome>;
  /** Calls `listener` with download progress; resolves to a function that stops it. */
  onUpdateProgress(listener: (progress: UpdateProgress) => void): Promise<() => void>;
  /** Opens what's new in the update the last check found, in the browser. */
  openUpdateNotes(): Promise<void>;
  /** Calls `listener` each time the tray's "Check for updates" is chosen; resolves to a function that stops it. */
  onTrayCheck(listener: () => void): Promise<() => void>;
  /** The installed helper's version, such as "0.2.1". */
  helperVersion(): Promise<string>;
  /** Compares the add-on in a game version folder with the latest version the site names.
   * Downloads nothing. */
  checkAddon(versionFolder: string): Promise<AddonCheck>;
  /** Installs the add-on the last check found into a game version folder, with or without an
   * add-on already there. */
  installAddon(versionFolder: string): Promise<AddonInstall>;
  /** Opens what's new in an add-on version, in the browser. */
  openAddonNotes(version: string): Promise<void>;
  /** Calls `listener` with add-on install progress; resolves to a function that stops it. */
  onAddonProgress(listener: (progress: AddonProgress) => void): Promise<() => void>;
  /** Shows the tray its new status: icon dot, menu and hover text. */
  setTrayStatus(status: TrayStatus): Promise<void>;
  /** Raises a system notification. Only when the officer must act, never for routine news. */
  notify(notice: Notice): Promise<void>;
}

export const tauriBridge: Bridge = {
  connect: (key) => invoke("connect_key", { key }),
  checkStored: () => invoke("check_stored_key"),
  removeKey: () => invoke("remove_stored_key"),
  staging: () => invoke("get_staging"),
  setStaging: (on) => invoke("set_staging_site", { on }),
  syncStatus: () => invoke("get_sync_status"),
  raids: () => invoke("get_raids"),
  reserves: () => invoke("get_reserves"),
  searchFolder: () => invoke("search_game_folder"),
  checkPickedFolder: (folder) => invoke("check_picked_game_folder", { folder }),
  rememberVersion: (versionFolder) => invoke("remember_game_version", { folder: versionFolder }),
  rememberedFolder: () => invoke("get_remembered_game_folder"),
  pickFolder: async () => {
    const picked = await open({ directory: true, multiple: false });
    return typeof picked === "string" ? picked : null;
  },
  onSyncStatus: (listener) => listen<SyncStatus>(SYNC_STATUS_EVENT, (event) => listener(event.payload)),
  onRaids: (listener) => listen<RaidsView>(RAIDS_EVENT, (event) => listener(event.payload)),
  onReserves: (listener) => listen<ReservesView>(RESERVES_EVENT, (event) => listener(event.payload)),
  onChatLog: (listener) => listen<ChatEvent>(CHAT_LOG_EVENT, (event) => listener(event.payload)),
  checkForUpdate: () => invoke("check_for_update"),
  installUpdate: () => invoke("install_update"),
  restartAfterUpdate: () => invoke("restart_after_update"),
  onUpdateProgress: (listener) =>
    listen<UpdateProgress>(UPDATE_PROGRESS_EVENT, (event) => listener(event.payload)),
  openUpdateNotes: () => invoke("open_update_notes"),
  onTrayCheck: (listener) => listen(TRAY_CHECK_EVENT, () => listener()),
  helperVersion: () => getVersion(),
  checkAddon: (versionFolder) => invoke("check_addon_update", { folder: versionFolder }),
  installAddon: (versionFolder) => invoke("install_addon", { folder: versionFolder }),
  openAddonNotes: (version) => invoke("open_addon_notes", { version }),
  onAddonProgress: (listener) =>
    listen<AddonProgress>(ADDON_PROGRESS_EVENT, (event) => listener(event.payload)),
  setTrayStatus: (status) => invoke("set_tray_status", { status }),
  notify: (notice) => invoke("raise_notification", { notice }),
};
