/**
 * # Module contract
 *
 * ```json
 * {"purpose":"The Library: the machine's grimoires as books' spines in stacks on a shelf, one stack for each place they come from, where projects are switched.","criteria":[{"id":"stacks","statement":"The Library draws libraryStacks of status: a stack for this machine's stores first, then one for each team's server in address order, standing side by side on a shelf, each labelled beneath it with where it comes from, this machine or the server's address, its port set apart; in each stack one spine per grimoire, in name order from the top, named by its grimoire, offset sideways from the spine beneath it and given a band colour by its name alone, so a stack looks the same on every visit, and marked with its count of checkouts when it has more than one; with no library, the one grimoire the daemon serves stands alone; it has a header of its own holding Grimoire's book and name and no project's pill, tabs or search."},{"id":"spine-opens","statement":"Each spine is a link opening its book's href as a page of its own, /g/<name>/ for a grimoire of the library and / for the one grimoire of a daemon serving none; on hover or keyboard focus it slides out of its stack, only where motion is allowed, and keyboard focus is ringed in the accent colour."},{"id":"jingle","statement":"Beneath the stacks the Library shows Grimoire's jingle with Jingle, the component the empty states draw it with, and beside it the empty state's glint, which twinkles only where motion is allowed; the tab reads Library · Grimoire."}]}
 * ```
 */

import { useQuery, api } from "../api";
import { BookIcon, errorBox, Jingle, mono, Sparkle } from "../components";
import { useTabTitle } from "../title";
import { libraryStacks, type Book, type Stack } from "../types";

/**
 * A spine's looks, chosen from its name: its cloth, a tone of the palette laid thin over the surface, and its bands
 * in that tone at full strength; its length, thickness and offset. Written out so Tailwind sees every class it must generate.
 */
const CLOTHS = [
  { body: "bg-syn-keyword/22 border-syn-keyword/40", band: "bg-syn-keyword" },
  { body: "bg-syn-string/20 border-syn-string/40", band: "bg-syn-string" },
  { body: "bg-syn-number/20 border-syn-number/40", band: "bg-syn-number" },
  { body: "bg-syn-type/20 border-syn-type/40", band: "bg-syn-type" },
  { body: "bg-syn-meta/20 border-syn-meta/40", band: "bg-syn-meta" },
];
const WIDTHS = ["w-[224px]", "w-[240px]", "w-[256px]", "w-[272px]", "w-[288px]"];
const HEIGHTS = ["h-[40px]", "h-[46px]", "h-[52px]"];
const OFFSETS = ["left-[-20px]", "left-[-12px]", "left-[-5px]", "left-[4px]", "left-[11px]", "left-[19px]"];

/** A small, steady hash of a name, so a book looks the same on every visit and on every machine. */
function hashOf(name: string): number {
  let h = 2166136261;
  for (const c of name) h = Math.imul(h ^ c.charCodeAt(0), 16777619);
  return h >>> 0;
}

function pick<T>(from: T[], h: number, salt: number): T {
  return from[(h >>> salt) % from.length];
}

export function LibraryView() {
  useTabTitle("Library");
  const status = useQuery(() => api.status(), []);
  const stacks = status.data ? libraryStacks(status.data) : [];
  return (
    <div className="grid grid-rows-[auto_1fr] h-full">
      {/* The Library is the machine's, so its header holds Grimoire alone, no project's pill, tabs or search. */}
      <header className="flex items-center gap-2 px-4 h-[49px] bg-bg-1 border-b border-line text-accent">
        <BookIcon />
        <span className="font-bold tracking-wide">Grimoire</span>
      </header>
      <main className="overflow-auto px-4 pt-[9vh] pb-16">
        <section aria-label="Library" data-library className="flex flex-col items-center max-w-[1040px] mx-auto">
          <h1 className="m-0 text-[30px] font-semibold tracking-[-0.01em] text-fg">Library</h1>
          <p className="mt-2 mb-0 text-fg-dim">Every grimoire on this machine. Open one to read it.</p>
          {status.error && <p className={`${errorBox} mt-6`}>{status.error.message}</p>}
          <div className="mt-16 flex flex-wrap items-end justify-center gap-x-20 gap-y-16">
            {stacks.map((stack) => (
              <StackOf key={stack.key} stack={stack} />
            ))}
          </div>
          {status.data && (
            <div className="relative mt-20">
              <Jingle />
              <Sparkle size={11} className="absolute -top-2 -right-4 text-accent motion-safe:animate-twinkle" />
            </div>
          )}
        </section>
      </main>
    </div>
  );
}

/** One place's grimoires: books stacked on a plank, the place named beneath it. */
function StackOf({ stack }: { stack: Stack }) {
  // A server's port changes with where it runs; it is set apart so the visual test can mask it.
  const port = stack.server ? stack.label.match(/:\d+$/)?.[0] : undefined;
  const host = port ? stack.label.slice(0, -port.length) : stack.label;
  return (
    <figure aria-label={stack.server ? `From ${stack.label}` : "On this machine"} data-stack className="m-0 flex flex-col items-center">
      <ul className="m-0 p-0 list-none flex flex-col items-center">
        {stack.books.map((book) => (
          <li key={book.name}>
            <Spine book={book} />
          </li>
        ))}
      </ul>
      {/* The plank the books stand on, casting its shade on the page. */}
      <div aria-hidden className="w-[360px] h-3 rounded-[2px] bg-bg-3 border-t border-line shadow-[0_16px_24px_-8px_var(--color-shade)]" />
      <figcaption title={stack.server ?? undefined} className={`mt-3 text-[12.5px] text-fg-dim ${stack.server ? mono : ""}`}>
        {host}
        {port && (
          // As wide as any port, so the address stands in the same place whatever port is masked.
          <span data-port className="inline-block w-[6ch] text-left text-fg-faint">
            {port}
          </span>
        )}
      </figcaption>
    </figure>
  );
}

/** A grimoire as a book's spine: its name between two bands of colour, a link that slides the book out. */
function Spine({ book }: { book: Book }) {
  const h = hashOf(book.name);
  const cloth = pick(CLOTHS, h, 23);
  const several = book.checkouts > 1;
  return (
    <a
      href={book.href}
      data-spine
      // The name in full where a narrow spine cuts it short.
      title={book.name}
      className={`relative flex items-center justify-center gap-2.5 px-12 ${pick(WIDTHS, h, 4)} ${pick(HEIGHTS, h, 9)} ${pick(OFFSETS, h, 14)} rounded-[3px] border ${cloth.body} text-fg shadow-[inset_0_1px_0_var(--color-bg-3),0_2px_0_var(--color-shade)] hover:no-underline hover:text-accent focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent motion-safe:transition-transform motion-safe:duration-200 motion-safe:ease-out motion-safe:hover:translate-x-4 motion-safe:focus-visible:translate-x-4`}
    >
      {/* The bands a binding gilds near each end of the spine. */}
      <span aria-hidden className={`absolute inset-y-0 left-3.5 w-1 ${cloth.band}`} />
      <span aria-hidden className={`absolute inset-y-0 left-[22px] w-px ${cloth.band}`} />
      <span aria-hidden className={`absolute inset-y-0 right-[22px] w-px ${cloth.band}`} />
      <span aria-hidden className={`absolute inset-y-0 right-3.5 w-1 ${cloth.band}`} />
      <span className="font-serif text-[17px] font-semibold tracking-[0.08em] [font-variant-caps:small-caps] truncate">{book.name}</span>
      {several && (
        <>
          {" "}
          <span className="shrink-0 text-[11px] text-fg-dim">{book.checkouts} checkouts</span>
        </>
      )}
    </a>
  );
}
