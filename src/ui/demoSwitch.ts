/**
 * Whether the address asks for the demo page (`?demo`). main.ts also requires
 * `import.meta.env.DEV` right beside the import, so a release build drops the demo code
 * (checked in demoSwitch.test.ts).
 */
export function demoRequested(search: string): boolean {
  return new URLSearchParams(search).has("demo");
}

/** Which demo state the address asks for (`?demo=live`); empty for the shared pieces. */
export function demoName(search: string): string {
  return new URLSearchParams(search).get("demo") ?? "";
}
