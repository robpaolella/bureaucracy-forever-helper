// Prints the next version (or "none") and the release notes.
// Usage: node release/cli.ts <owner/repo> <head-sha>      (token in GITHUB_TOKEN; read-only)
// Output: "none", or the version, a blank line, then the notes in Markdown.
import { itemsSince, lastPublishedTag } from "./github.ts";
import { plan } from "./plan.ts";

const [repo, head] = process.argv.slice(2);
const token = process.env.GITHUB_TOKEN;
if (!repo || !head || !token) {
  console.error("Usage: GITHUB_TOKEN=<token> node release/cli.ts <owner/repo> <head-sha>");
  process.exit(2);
}

const tag = await lastPublishedTag(fetch, token, repo);
const items = await itemsSince(fetch, token, repo, tag, head);
const result = plan({ previous: tag, items });
console.log(result ? `${result.version}\n\n${result.notes}` : "none");
