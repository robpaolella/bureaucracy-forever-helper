import { beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

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
  });
});
