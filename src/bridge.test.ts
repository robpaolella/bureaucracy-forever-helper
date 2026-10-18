import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AddonProgress, ChatEvent, RaidChoice, RaidsView, ReservesView, SyncStatus, UpdateProgress } from "./bridge";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
const listen = vi.fn();
vi.mock("@tauri-apps/api/event", () => ({ listen }));

const getVersion = vi.fn();
vi.mock("@tauri-apps/api/app", () => ({ getVersion }));

const open = vi.fn();
vi.mock("@tauri-apps/plugin-dialog", () => ({ open }));

const { tauriBridge } = await import("./bridge");

describe("tauriBridge", () => {
  beforeEach(() => invoke.mockReset());

  it("calls each command by name with its arguments and returns the result as is", async () => {
    invoke.mockResolvedValue({ kind: "noKey" });
    expect(await tauriBridge.connect("bfh_x")).toEqual({ kind: "noKey" });
    expect(invoke).toHaveBeenLastCalledWith("connect_key", { key: "bfh_x" });
    await tauriBridge.checkStored();
    expect(invoke).toHaveBeenLastCalledWith("check_stored_key");
    await tauriBridge.removeKey();
    expect(invoke).toHaveBeenLastCalledWith("remove_stored_key");
    await tauriBridge.staging();
    expect(invoke).toHaveBeenLastCalledWith("get_staging");
    await tauriBridge.setStaging(true);
    expect(invoke).toHaveBeenLastCalledWith("set_staging_site", { on: true });
    await tauriBridge.syncStatus();
    expect(invoke).toHaveBeenLastCalledWith("get_sync_status");
    await tauriBridge.raids();
    expect(invoke).toHaveBeenLastCalledWith("get_raids");
    await tauriBridge.reserves();
    expect(invoke).toHaveBeenLastCalledWith("get_reserves");
    await tauriBridge.raidChoice();
    expect(invoke).toHaveBeenLastCalledWith("get_raid_choice");
    await tauriBridge.searchFolder();
    expect(invoke).toHaveBeenLastCalledWith("search_game_folder");
    await tauriBridge.checkPickedFolder("/games");
    expect(invoke).toHaveBeenLastCalledWith("check_picked_game_folder", { folder: "/games" });
    await tauriBridge.rememberVersion("/games/_retail_");
    expect(invoke).toHaveBeenLastCalledWith("remember_game_version", { folder: "/games/_retail_" });
    await tauriBridge.setTrayStatus({ tone: "ok", word: "Sending", sentTonight: 2, staging: false, secondLine: null });
    expect(invoke).toHaveBeenLastCalledWith("set_tray_status", {
      status: { tone: "ok", word: "Sending", sentTonight: 2, staging: false, secondLine: null },
    });
    await tauriBridge.notify({ headline: "Type /reload in game", detail: "Reserves are ready." });
    expect(invoke).toHaveBeenLastCalledWith("raise_notification", {
      notice: { headline: "Type /reload in game", detail: "Reserves are ready." },
    });
    await tauriBridge.rememberedFolder();
    expect(invoke).toHaveBeenLastCalledWith("get_remembered_game_folder");
    await tauriBridge.checkForUpdate();
    expect(invoke).toHaveBeenLastCalledWith("check_for_update");
    await tauriBridge.installUpdate();
    expect(invoke).toHaveBeenLastCalledWith("install_update");
    await tauriBridge.restartAfterUpdate();
    expect(invoke).toHaveBeenLastCalledWith("restart_after_update");
    await tauriBridge.openUpdateNotes();
    expect(invoke).toHaveBeenLastCalledWith("open_update_notes");
    await tauriBridge.checkAddon("/games/_retail_");
    expect(invoke).toHaveBeenLastCalledWith("check_addon_update", { folder: "/games/_retail_" });
    await tauriBridge.installAddon("/games/_retail_");
    expect(invoke).toHaveBeenLastCalledWith("install_addon", { folder: "/games/_retail_" });
    await tauriBridge.openAddonNotes("1.4.0");
    expect(invoke).toHaveBeenLastCalledWith("open_addon_notes", { version: "1.4.0" });
    await tauriBridge.openLootLog("r1");
    expect(invoke).toHaveBeenLastCalledWith("open_loot_log", { raid: "r1" });
    await tauriBridge.openLootLog();
    expect(invoke).toHaveBeenLastCalledWith("open_loot_log", { raid: null });
  });

  it("opens the native picker for one folder and gives back its path, or null when cancelled", async () => {
    open.mockResolvedValueOnce("/games/World of Warcraft");
    expect(await tauriBridge.pickFolder()).toBe("/games/World of Warcraft");
    expect(open).toHaveBeenLastCalledWith({ directory: true, multiple: false });
    open.mockResolvedValueOnce(null);
    expect(await tauriBridge.pickFolder()).toBeNull();
  });

  it("passes on each sync status from the sync-status event until stopped", async () => {
    const stop = vi.fn();
    let emit: (event: { payload: unknown }) => void = () => {};
    listen.mockImplementation((_name, handler: typeof emit) => {
      emit = handler;
      return Promise.resolve(stop);
    });
    const heard: SyncStatus[] = [];
    const unlisten = await tauriBridge.onSyncStatus((status) => heard.push(status));
    expect(listen).toHaveBeenCalledWith("sync-status", expect.any(Function));
    emit({ payload: { kind: "noKey" } });
    emit({ payload: { kind: "idle", otherSite: 0 } });
    expect(heard).toEqual([{ kind: "noKey" }, { kind: "idle", otherSite: 0 }]);
    unlisten();
    expect(stop).toHaveBeenCalled();
  });

  it("passes on each raids view from the raids event until stopped", async () => {
    const stop = vi.fn();
    let emit: (event: { payload: unknown }) => void = () => {};
    listen.mockImplementation((_name, handler: typeof emit) => {
      emit = handler;
      return Promise.resolve(stop);
    });
    const heard: RaidsView[] = [];
    const unlisten = await tauriBridge.onRaids((view) => heard.push(view));
    expect(listen).toHaveBeenCalledWith("raids", expect.any(Function));
    emit({ payload: { state: { kind: "loading" }, raids: [] } });
    expect(heard).toEqual([{ state: { kind: "loading" }, raids: [] }]);
    unlisten();
    expect(stop).toHaveBeenCalled();
  });

  it("passes on each raid choice from the raid-choice event until stopped", async () => {
    const stop = vi.fn();
    let emit: (event: { payload: unknown }) => void = () => {};
    listen.mockImplementation((_name, handler: typeof emit) => {
      emit = handler;
      return Promise.resolve(stop);
    });
    const heard: RaidChoice[] = [];
    const unlisten = await tauriBridge.onRaidChoice((choice) => heard.push(choice));
    expect(listen).toHaveBeenCalledWith("raid-choice", expect.any(Function));
    const choice = { activeBy: "chosen", activeRaid: "r1", chosenBy: "Aldric", savedAt: 1_792_005_000 };
    emit({ payload: choice });
    expect(heard).toEqual([choice]);
    unlisten();
    expect(stop).toHaveBeenCalled();
  });

  it("passes on each reserves view from the reserves event until stopped", async () => {
    const stop = vi.fn();
    let emit: (event: { payload: unknown }) => void = () => {};
    listen.mockImplementation((_name, handler: typeof emit) => {
      emit = handler;
      return Promise.resolve(stop);
    });
    const heard: ReservesView[] = [];
    const unlisten = await tauriBridge.onReserves((view) => heard.push(view));
    expect(listen).toHaveBeenCalledWith("reserves", expect.any(Function));
    const view = { state: { kind: "waiting" }, changes: 0, first: false, problem: null };
    emit({ payload: view });
    expect(heard).toEqual([view]);
    unlisten();
    expect(stop).toHaveBeenCalled();
  });

  it("passes on each chat log event until stopped", async () => {
    const stop = vi.fn();
    let emit: (event: { payload: unknown }) => void = () => {};
    listen.mockImplementation((_name, handler: typeof emit) => {
      emit = handler;
      return Promise.resolve(stop);
    });
    const heard: ChatEvent[] = [];
    const unlisten = await tauriBridge.onChatLog((event) => heard.push(event));
    expect(listen).toHaveBeenCalledWith("chat-log", expect.any(Function));
    const sent = {
      kind: "sent", line: "award", item: 16805, itemName: "Felheart Gloves", character: "Zoé Brûlé",
      boss: "Lucifron", raid: "cm9xr2k4a0001", raidName: "Molten Core", at: 1791885711,
    };
    emit({ payload: sent });
    emit({ payload: { kind: "ignored", reason: "newerVersion" } });
    expect(heard).toEqual([sent, { kind: "ignored", reason: "newerVersion" }]);
    unlisten();
    expect(stop).toHaveBeenCalled();
  });

  it("passes on download progress from the update-progress event until stopped", async () => {
    const stop = vi.fn();
    let emit: (event: { payload: unknown }) => void = () => {};
    listen.mockImplementation((_name, handler: typeof emit) => {
      emit = handler;
      return Promise.resolve(stop);
    });
    const heard: UpdateProgress[] = [];
    const unlisten = await tauriBridge.onUpdateProgress((progress) => heard.push(progress));
    expect(listen).toHaveBeenCalledWith("update-progress", expect.any(Function));
    emit({ payload: { downloaded: 10, total: 20 } });
    emit({ payload: { downloaded: 20, total: null } });
    expect(heard).toEqual([
      { downloaded: 10, total: 20 },
      { downloaded: 20, total: null },
    ]);
    unlisten();
    expect(stop).toHaveBeenCalled();
  });

  it("reads the helper's own version", async () => {
    getVersion.mockResolvedValueOnce("0.2.1");
    expect(await tauriBridge.helperVersion()).toBe("0.2.1");
  });

  it("calls back each time the tray's Check for updates is chosen, until stopped", async () => {
    const stop = vi.fn();
    let emit: (event: { payload: unknown }) => void = () => {};
    listen.mockImplementation((_name, handler: typeof emit) => {
      emit = handler;
      return Promise.resolve(stop);
    });
    const heard = vi.fn();
    const unlisten = await tauriBridge.onTrayCheck(heard);
    expect(listen).toHaveBeenCalledWith("tray-check-for-updates", expect.any(Function));
    emit({ payload: null });
    emit({ payload: null });
    expect(heard).toHaveBeenCalledTimes(2);
    unlisten();
    expect(stop).toHaveBeenCalled();
  });

  it("passes on add-on install progress from the addon-progress event until stopped", async () => {
    const stop = vi.fn();
    let emit: (event: { payload: unknown }) => void = () => {};
    listen.mockImplementation((_name, handler: typeof emit) => {
      emit = handler;
      return Promise.resolve(stop);
    });
    const heard: AddonProgress[] = [];
    const unlisten = await tauriBridge.onAddonProgress((progress) => heard.push(progress));
    expect(listen).toHaveBeenCalledWith("addon-progress", expect.any(Function));
    emit({ payload: { step: "downloading", version: "1.4.0", downloaded: 10, total: 20 } });
    emit({ payload: { step: "installing", version: "1.4.0" } });
    expect(heard).toEqual([
      { step: "downloading", version: "1.4.0", downloaded: 10, total: 20 },
      { step: "installing", version: "1.4.0" },
    ]);
    unlisten();
    expect(stop).toHaveBeenCalled();
  });
});
