// Waiting for a view to be drawn, shared by the visual test and the shots.
import type { Page } from "@playwright/test";
import type { Route } from "../behaviour/routes";

/**
 * Open `route` and wait until it is drawn: the header's status line, which
 * proves the daemon answered, the route's content, and then no tool call in
 * flight for 300 ms, since a view's sections query separately and a heading
 * arrives before them. The event stream holds a connection open forever, so
 * the network never goes idle; tool calls are what the view waits on.
 */
export async function open(page: Page, route: Route): Promise<void> {
  let inFlight = 0;
  let last = Date.now();
  const isTool = (url: string) => new URL(url).pathname.startsWith("/api/") && !url.endsWith("/api/events");
  page.on("request", (r) => {
    if (isTool(r.url())) inFlight++;
  });
  const done = (r: { url(): string }) => {
    if (isTool(r.url())) {
      inFlight--;
      last = Date.now();
    }
  };
  page.on("requestfinished", done);
  page.on("requestfailed", done);
  await page.goto(route.path);
  await page.locator("header").getByText(/\d+ pages ·/).waitFor();
  await page.locator(route.content).first().waitFor();
  await expectQuiet(() => inFlight === 0 && Date.now() - last >= 300);
  // Fonts and layout settle.
  await page.waitForTimeout(100);
}

async function expectQuiet(quiet: () => boolean, timeout = 10_000): Promise<void> {
  const until = Date.now() + timeout;
  while (!quiet()) {
    if (Date.now() > until) throw new Error("tool calls were still in flight");
    await new Promise((r) => setTimeout(r, 50));
  }
}
