// Every route the UI serves, on the behaviour fixture: the grimoire at the
// root and at a page, the unplaced modules, the cauldron, and one node page of
// each other kind. The behaviour specs visit them, the visual test
// screenshots them and the shots capture them, so a route added here
// is covered by all three.

/** A route: a name for its screenshot, its path, and a selector for the view's own content, which a screenshot waits for before the tool calls settle. */
export type Route = { name: string; path: string; content: string };

const node = (ref: string) => `/n/${encodeURIComponent(ref)}`;

export const ROUTES: Route[] = [
  { name: "grimoire", path: "/", content: "#pages" },
  { name: "grimoire-page", path: node("page:readiness"), content: "#modules" },
  { name: "unplaced", path: "/unplaced", content: "main h1" },
  { name: "cauldron", path: "/cauldron", content: "section[id^='band-']" },
  { name: "node-requirement", path: node("req:ready-needs-commit"), content: "h1" },
  { name: "node-ticket", path: node("ticket:0006"), content: "section[aria-label='What this ticket rests on']" },
  // A closed ticket touching a requirement and relying on a page that no longer exist, so the mark for them is seen.
  { name: "node-ticket-closed", path: node("ticket:0004"), content: "section[aria-label='What this ticket rests on']" },
  // A landing ticket: its decision written, its change yet to land.
  { name: "node-ticket-landing", path: node("ticket:0011"), content: "section[aria-label='What this ticket rests on']" },
  { name: "node-module", path: node("module:src/ready"), content: "h1" },
  { name: "node-criterion", path: node("criterion:src/ready#needs-commit"), content: "h1" },
  { name: "node-file", path: node("file:src/ready.rs"), content: "[aria-label='Source of src/ready.rs'] table" },
  { name: "node-decl", path: node("decl:src/ready.rs#ready"), content: "h1" },
  { name: "node-test", path: node("test:src/ready.rs#ready-needs-a-commit"), content: "h1" },
];
