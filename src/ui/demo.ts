// Test-only demo page: each shared piece in its states, to compare with the approved
// design. main.ts loads this only in a dev build (see demoEnabled), so a release build
// carries none of it. `?demo` shows the shared pieces; `?demo=<state>` shows a window screen.
import { mountApp } from "./app";
import { gameFolderSection } from "./folder";
import { versionsSection } from "./update";
import { demoStates } from "./demoStates";
import { button, card, choice, field, link, pill, switchControl, tag } from "./components";

function section(title: string, ...children: Node[]): HTMLElement {
  const wrap = document.createElement("section");
  wrap.className = "stack";
  const h = document.createElement("h3");
  h.className = "th";
  h.style.margin = "0";
  h.textContent = title;
  wrap.append(h, ...children);
  return wrap;
}

function row(...children: Node[]): HTMLElement {
  const r = document.createElement("div");
  r.className = "row-wrap";
  r.append(...children);
  return r;
}

function text(className: string, value: string): HTMLElement {
  const p = document.createElement("p");
  p.className = className;
  p.textContent = value;
  return p;
}

const inert = { use() {}, continueWith() {}, choose() {}, select() {} };

export function renderDemo(root: HTMLElement, name = ""): void {
  const state = demoStates[name];
  if (state) {
    mountApp(root, {
      getState: () => state.app,
      view: state.view,
      settingsSections: () => [gameFolderSection(state.app.gamePath, undefined, inert), versionsSection(state.app.helperVersion, undefined, false, () => {})],
      folderActions: inert,
      updateActions: { install() {}, later() {}, notes() {} },
    });
    return;
  }
  root.className = "win-body";
  const title = document.createElement("h1");
  title.className = "title";
  title.textContent = "Shared pieces";
  const eyebrow = text("eyebrow", "Demo page");
  root.replaceChildren(
    eyebrow,
    title,
    section(
      "Buttons",
      row(
        button({ label: "Connect" }),
        button({ label: "Replace key", variant: "secondary", small: true }),
        button({ label: "Remove key", variant: "danger", small: true }),
        button({ label: "Later", variant: "ghost", small: true }),
      ),
      row(button({ label: "Checking…", loading: true }), button({ label: "Connect", disabled: true })),
      button({ label: "Use this folder", block: true }),
    ),
    section(
      "Field",
      field({ id: "key-empty", label: "Helper key", placeholder: "Paste your key", mono: true, note: "Create a key in the officer area." }),
      field({
        id: "key-bad",
        label: "Helper key",
        value: "my key from the site",
        mono: true,
        error: "That doesn’t look like a helper key. Copy it again from the officer area.",
      }),
    ),
    section("Pills and tags", row(pill("ok", "Working"), pill("warn", "Needs you"), pill("stop", "Stopped"), pill("closed", "Closed"), tag("Staging"))),
    section(
      "Cards",
      card({}, [text("small", "A plain card.")]),
      card({ tone: "warn" }, [text("small", "A warning card.")]),
      card({ tone: "stop" }, [text("small", "A stop card.")]),
      card({ tone: "info" }, [text("small", "An info card.")]),
      card({ accolade: true }, [text("small", "An accolade card.")]),
    ),
    section(
      "Choice cards",
      choice({ name: "ver", label: "_classic_", sub: "D:\\Games\\World of Warcraft\\_classic_", checked: true }),
      choice({ name: "ver", label: "_classic_era_", sub: "D:\\Games\\World of Warcraft\\_classic_era_" }),
    ),
    section(
      "Switches",
      switchControl({ id: "sw-on", label: "Start the helper when the computer starts", checked: true }),
      switchControl({ id: "sw-off", label: "Use the staging site (for testing)" }),
    ),
    section("Link", link({ label: "Open the officer area" })),
  );
}
