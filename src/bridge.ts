// The only place the front-end calls Tauri. Everything else takes a `Bridge`, so tests
// can hand it a fake. The result shapes mirror `KeyOutcome` and `ChangeOutcome` in
// src-tauri/src/commands.rs, `FolderOutcome` in src-tauri/src/game_folder.rs, `SyncStatus` in src-tauri/src/upload.rs,
// and the update outcomes in src-tauri/src/updater.rs.
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
  | { kind: "noAddon"; message: string; path: string }
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
  /** Asks GitHub for a newer helper. Downloads and installs nothing. */
  checkForUpdate(): Promise<CheckOutcome>;
  /** Downloads and installs the update the last check found. Call only once the officer
   * confirms. On Windows it downloads and checks it; the restart runs the installer. */
  installUpdate(): Promise<InstallOutcome>;
  /** Restarts into the update `installUpdate` made ready; does nothing unless one is. */
  restartAfterUpdate(): Promise<RestartOutcome>;
  /** Calls `listener` with download progress; resolves to a function that stops it. */
  onUpdateProgress(listener: (progress: UpdateProgress) => void): Promise<() => void>;
}

export const tauriBridge: Bridge = {
  connect: (key) => invoke("connect_key", { key }),
  checkStored: () => invoke("check_stored_key"),
  removeKey: () => invoke("remove_stored_key"),
  staging: () => invoke("get_staging"),
  setStaging: (on) => invoke("set_staging_site", { on }),
  syncStatus: () => invoke("get_sync_status"),
  raids: () => invoke("get_raids"),
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
  checkForUpdate: () => invoke("check_for_update"),
  installUpdate: () => invoke("install_update"),
  restartAfterUpdate: () => invoke("restart_after_update"),
  onUpdateProgress: (listener) =>
    listen<UpdateProgress>(UPDATE_PROGRESS_EVENT, (event) => listener(event.payload)),
};
