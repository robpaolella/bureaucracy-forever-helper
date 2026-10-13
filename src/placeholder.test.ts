import { describe, expect, it } from "vitest";
import { PLACEHOLDER_TEXT } from "./placeholder";

describe("placeholder", () => {
  it("names the app", () => {
    expect(PLACEHOLDER_TEXT).toBe("Bureaucracy Forever Helper");
  });
});
