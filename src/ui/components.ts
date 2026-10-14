// The shared pieces every screen reuses. Class names and markup follow the approved
// design (design/2-helper-window/index.html); styles live in components.css.

type Child = Node | string;

function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  className?: string,
  children: Child[] = [],
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (className) node.className = className;
  node.append(...children);
  return node;
}

export type ButtonVariant = "primary" | "secondary" | "ghost" | "danger";

export interface ButtonOptions {
  label: string;
  variant?: ButtonVariant;
  /** The compact size the design uses beside other controls. */
  small?: boolean;
  block?: boolean;
  /** Shows a spinner and the label; the button can't be pressed meanwhile. */
  loading?: boolean;
  disabled?: boolean;
  onClick?: () => void;
}

export function button(o: ButtonOptions): HTMLButtonElement {
  const classes = ["btn", `btn-${o.variant ?? "primary"}`];
  if (o.small) classes.push("btn-sm");
  if (o.block) classes.push("btn-block");
  if (o.loading) classes.push("loading");
  const node = el("button", classes.join(" "));
  node.type = "button";
  if (o.loading) {
    const spin = el("span", "spin");
    spin.setAttribute("aria-hidden", "true");
    node.append(spin);
    node.setAttribute("aria-busy", "true");
  }
  node.append(o.label);
  node.disabled = Boolean(o.disabled || o.loading);
  if (o.onClick) node.addEventListener("click", o.onClick);
  return node;
}

export interface LinkOptions {
  label: string;
  href?: string;
  onClick?: () => void;
}

/** A text link with a 44px-tall hit area. */
export function link(o: LinkOptions): HTMLAnchorElement {
  const node = el("a", "link link-row small", [o.label]);
  node.href = o.href ?? "#";
  node.addEventListener("click", (e) => {
    if (!o.href) e.preventDefault();
    o.onClick?.();
  });
  return node;
}

export interface FieldOptions {
  id: string;
  label: string;
  value?: string;
  placeholder?: string;
  /** Shown in place of `note`, in the error colour, and marks the field invalid. */
  error?: string;
  note?: string;
  /** The monospace look the design uses for keys. */
  mono?: boolean;
  onInput?: (value: string) => void;
}

export function field(o: FieldOptions): HTMLElement {
  const label = el("label", "label-field", [o.label]);
  label.htmlFor = o.id;
  const input = el("input", o.mono ? "input mono" : "input");
  input.id = o.id;
  input.value = o.value ?? "";
  input.placeholder = o.placeholder ?? "";
  input.autocomplete = "off";
  input.spellcheck = false;
  if (o.onInput) input.addEventListener("input", () => o.onInput?.(input.value));
  const wrap = el("div", "field", [label, input]);
  const text = o.error ?? o.note;
  if (text) {
    const note = el("span", o.error ? "note err" : "note", [text]);
    note.id = `${o.id}-note`;
    input.setAttribute("aria-describedby", note.id);
    if (o.error) {
      input.setAttribute("aria-invalid", "true");
      note.setAttribute("role", "alert");
    }
    wrap.append(note);
  }
  return wrap;
}

export type Tone = "ok" | "warn" | "stop" | "closed";

export function pill(tone: Tone, text: string): HTMLSpanElement {
  return el("span", `pill ${tone}`, [text]);
}

export function tag(text: string): HTMLSpanElement {
  return el("span", "tag", [text]);
}

export type CardTone = "warn" | "stop" | "info";

export interface CardOptions {
  tone?: CardTone;
  /** The sand border the design uses to mark the one thing to look at. */
  accolade?: boolean;
}

export function card(o: CardOptions, children: Child[]): HTMLElement {
  const classes = ["card", "stack"];
  if (o.tone) classes.push(`tone-${o.tone}`);
  if (o.accolade) classes.push("accolade");
  return el("div", classes.join(" "), children);
}

export interface ChoiceOptions {
  name: string;
  label: string;
  /** A second, smaller line, such as a folder path. */
  sub?: string;
  checked?: boolean;
  onChange?: () => void;
}

/** A radio option drawn as a card. */
export function choice(o: ChoiceOptions): HTMLLabelElement {
  const input = el("input");
  input.type = "radio";
  input.name = o.name;
  input.checked = Boolean(o.checked);
  if (o.onChange) input.addEventListener("change", o.onChange);
  const text = el("span", undefined, [o.label]);
  if (o.sub) text.append(el("span", "sub", [o.sub]));
  return el("label", "choice", [input, text]);
}

export interface SwitchOptions {
  id: string;
  label: string;
  checked?: boolean;
  onChange?: (checked: boolean) => void;
}

export function switchControl(o: SwitchOptions): HTMLElement {
  const label = el("label", undefined, [o.label]);
  label.htmlFor = o.id;
  const sw = el("button", "switch", [el("span", "track", [el("span", "knob")])]);
  sw.id = o.id;
  sw.type = "button";
  sw.setAttribute("role", "switch");
  sw.setAttribute("aria-checked", String(Boolean(o.checked)));
  sw.addEventListener("click", () => {
    const next = sw.getAttribute("aria-checked") !== "true";
    sw.setAttribute("aria-checked", String(next));
    o.onChange?.(next);
  });
  return el("div", "toggle", [label, sw]);
}
