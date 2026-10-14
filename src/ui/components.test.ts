// @vitest-environment happy-dom
import { describe, expect, it, vi } from "vitest";
import { button, card, choice, field, link, pill, switchControl } from "./components";

describe("button", () => {
  it("defaults to the primary look and fires onClick", () => {
    const onClick = vi.fn();
    const b = button({ label: "Connect", onClick });
    expect(b.className).toBe("btn btn-primary");
    b.click();
    expect(onClick).toHaveBeenCalledOnce();
  });

  it("can't be pressed while loading, and says it is busy", () => {
    const onClick = vi.fn();
    const b = button({ label: "Checking…", loading: true, onClick });
    expect(b.disabled).toBe(true);
    expect(b.getAttribute("aria-busy")).toBe("true");
    expect(b.querySelector(".spin")).not.toBeNull();
    b.click();
    expect(onClick).not.toHaveBeenCalled();
  });

  it("applies variant, size and block", () => {
    const b = button({ label: "x", variant: "danger", small: true, block: true });
    expect(b.className).toBe("btn btn-danger btn-sm btn-block");
  });
});

describe("field", () => {
  it("links its label and shows a note", () => {
    const f = field({ id: "key", label: "Helper key", note: "Create a key." });
    expect(f.querySelector("label")?.htmlFor).toBe("key");
    expect(f.querySelector("#key-note")?.textContent).toBe("Create a key.");
    expect(f.querySelector("input")?.hasAttribute("aria-invalid")).toBe(false);
  });

  it("shows an error instead of the note and marks the field invalid", () => {
    const f = field({ id: "key", label: "k", note: "hint", error: "Bad key" });
    const note = f.querySelector("#key-note");
    expect(note?.textContent).toBe("Bad key");
    expect(note?.getAttribute("role")).toBe("alert");
    expect(f.querySelector("input")?.getAttribute("aria-invalid")).toBe("true");
  });

  it("reports typing", () => {
    const onInput = vi.fn();
    const input = field({ id: "k", label: "k", onInput }).querySelector("input")!;
    input.value = "abc";
    input.dispatchEvent(new Event("input"));
    expect(onInput).toHaveBeenCalledWith("abc");
  });
});

describe("switchControl", () => {
  it("toggles aria-checked and reports it", () => {
    const onChange = vi.fn();
    const s = switchControl({ id: "s", label: "Staging", onChange });
    const sw = s.querySelector<HTMLButtonElement>("button")!;
    expect(sw.getAttribute("aria-checked")).toBe("false");
    sw.click();
    expect(sw.getAttribute("aria-checked")).toBe("true");
    expect(onChange).toHaveBeenLastCalledWith(true);
    sw.click();
    expect(onChange).toHaveBeenLastCalledWith(false);
  });
});

describe("choice", () => {
  it("is a radio card with an optional second line", () => {
    const c = choice({ name: "v", label: "_classic_", sub: "D:\\WoW", checked: true });
    const input = c.querySelector("input")!;
    expect(input.type).toBe("radio");
    expect(input.checked).toBe(true);
    expect(c.querySelector(".sub")?.textContent).toBe("D:\\WoW");
  });
});

describe("link, pill and card", () => {
  it("keeps a link without href from navigating", () => {
    const onClick = vi.fn();
    const l = link({ label: "Open", onClick });
    const e = new Event("click", { cancelable: true });
    l.dispatchEvent(e);
    expect(e.defaultPrevented).toBe(true);
    expect(onClick).toHaveBeenCalledOnce();
  });

  it("builds a pill and a toned card", () => {
    expect(pill("ok", "Working").className).toBe("pill ok");
    const c = card({ tone: "warn", accolade: true }, ["hi"]);
    expect(c.className).toBe("card stack tone-warn accolade");
    expect(c.textContent).toBe("hi");
  });
});
