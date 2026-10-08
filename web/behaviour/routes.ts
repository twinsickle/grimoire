// Every route the UI serves, on the behaviour fixture: the fixture's own
// grimoire, `repo`, at its address /g/repo/ and at a page, the unplaced modules, the cauldron, one node page of
// each other kind, the grimoire and cauldron of `ledger`, the empty
// grimoire beside it in the fixture's library, for the empty states, and the
// Library, the machine's grimoires, at the daemon's own address. The behaviour specs visit them, the visual test
// screenshots them and the shots capture them, so a route added here
// is covered by all three.

/**
 * A route: a name for its screenshot, its path, a selector for the view's own content, which a screenshot waits for
 * before the tool calls settle, for a part of the page drawn only when asked, what to press to open it, and what the
 * visual test masks because it changes with the port the fixture is served on.
 */
export type Route = { name: string; path: string; content: string; press?: string; mask?: string[] };

/** Where the fixture's own grimoire, `repo`, is read: the daemon's own address is the Library. */
const repo = "/g/repo";
const node = (ref: string) => `${repo}/n/${encodeURIComponent(ref)}`;

export const ROUTES: Route[] = [
  { name: "grimoire", path: `${repo}/`, content: "#pages" },
  { name: "grimoire-page", path: node("page:readiness"), content: "#modules" },
  { name: "unplaced", path: `${repo}/unplaced`, content: "main h1" },
  { name: "cauldron", path: `${repo}/cauldron`, content: "section[id^='band-']" },
  // The header's pill opened: the project's checkouts, the finding counts and the way to every project.
  { name: "header-open", path: `${repo}/cauldron`, content: "section[id^='band-']", press: "header button[aria-label^='Where you are']" },
  { name: "node-requirement", path: node("req:ready-needs-commit"), content: "h1" },
  // A requirement the landing ticket repoints at a criterion the code lacks, so other work that may conflict is seen.
  { name: "node-requirement-conflict", path: node("req:ready-writes-nothing"), content: "main h2:has-text('Other work may conflict')" },
  { name: "node-ticket", path: node("ticket:0006"), content: "section[aria-label='Graph']" },
  // A closed ticket touching a requirement and relying on a page that no longer exist, so the mark for them is seen.
  { name: "node-ticket-closed", path: node("ticket:0004"), content: "section[aria-label='Graph']" },
  // A landing ticket: its decision written, its change yet to land.
  { name: "node-ticket-landing", path: node("ticket:0011"), content: "section[aria-label='Graph']" },
  { name: "node-module", path: node("module:src/ready"), content: "h1" },
  { name: "node-criterion", path: node("criterion:src/ready#needs-commit"), content: "h1" },
  { name: "node-file", path: node("file:src/ready.rs"), content: "[aria-label='Source of src/ready.rs'] table" },
  { name: "node-decl", path: node("decl:src/ready.rs#ready"), content: "h1" },
  { name: "node-test", path: node("test:src/ready.rs#ready-needs-a-commit"), content: "h1" },
  // A recipe that names a file the checkout lacks, so the mark for it is seen, and one of its ingredients.
  { name: "node-recipe", path: node("recipe:ready-check"), content: "main h2:has-text('Ingredients')" },
  { name: "node-ingredient", path: node("recipe:ready-check#answer"), content: "h1" },
  // The fixture library's empty grimoire: nothing yet to show, so its views are the empty states.
  { name: "empty-grimoire", path: "/g/ledger/", content: "[data-empty-state]" },
  { name: "empty-cauldron", path: "/g/ledger/cauldron", content: "[data-empty-state]" },
  // The Library: `repo`, a copy of the fixture's server, in one stack, and `almanac`, `bestiary` and `ledger`, stores on this machine, in another.
  { name: "library", path: "/", content: "[data-stack] [data-spine]", mask: ["[data-port]"] },
];
