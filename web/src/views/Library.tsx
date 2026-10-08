/**
 * # Module contract
 *
 * ```json
 * {"purpose":"The Library: the machine's grimoires as books' spines in stacks on a shelf, one stack for each place they come from, where projects are switched, and the shelf rail that carries the same books small beside every page of a grimoire.","criteria":[{"id":"stacks","statement":"The Library draws libraryStacks of status: a stack for this machine's stores first, then one for each team's server in address order, standing side by side on a shelf, each labelled beneath it with where it comes from, this machine or the server's address, its port set apart; in each stack one spine per grimoire, in name order from the top, named by its grimoire, offset sideways from the spine beneath it and given a band colour by its name alone with clothOf, so a stack looks the same on every visit, and marked with its count of checkouts when it has more than one; it has a header of its own holding Grimoire's book and name and no project's pill, tabs or search."},{"id":"spine-opens","statement":"Each spine is a link opening its grimoire at /g/<name>/ as a page of its own; on hover or keyboard focus it slides out of its stack, only where motion is allowed, and keyboard focus is ringed in the accent colour."},{"id":"jingle","statement":"Beneath the stacks the Library shows Grimoire's jingle with Jingle, the component the empty states draw it with, and beside it the empty state's glint, which twinkles only where motion is allowed; the tab reads Library · Grimoire."},{"id":"shelf-rail","statement":"ShelfRail holds at its top the books mark, three flat books in the band colours, a link to the Library at /, and beneath it libraryStacks of status as shelves, this machine first, each a stack of small flat books on a plank, titled with its place, this machine or its address, and named for a screen reader as the Library names its stacks; each book is one grimoire, in name order from the top, shows its initial, is titled with its name, is drawn in the cloth clothOf gives the Library's spine of that name and offset sideways by the same hash, and is a link opening /g/<name>/; the book of the grimoire status names is ringed in the accent colour and marked as the current page; a book slides out on hover or keyboard focus only where motion is allowed, and keyboard focus is outlined in the accent colour."}]}
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

/** The rail's books: the same lengths and offsets as the Library's spines, drawn small. */
const RAIL_WIDTHS = ["w-[28px]", "w-[30px]", "w-[32px]", "w-[34px]", "w-[36px]"];
const RAIL_OFFSETS = ["left-[-4px]", "left-[-3px]", "left-[-1px]", "left-[1px]", "left-[3px]", "left-[4px]"];

/** A small, steady hash of a name, so a book looks the same on every visit and on every machine. */
function hashOf(name: string): number {
  let h = 2166136261;
  for (const c of name) h = Math.imul(h ^ c.charCodeAt(0), 16777619);
  return h >>> 0;
}

function pick<T>(from: T[], h: number, salt: number): T {
  return from[(h >>> salt) % from.length];
}

/** A grimoire's cloth, from its name alone: the same on the Library's shelf and on the rail, on every visit and machine. */
function clothOf(name: string): (typeof CLOTHS)[number] {
  return pick(CLOTHS, hashOf(name), 23);
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
  const cloth = clothOf(book.name);
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

/** The Library's mark: three flat books stacked, each in a cloth of the shelf with its bands, as the Library draws them. */
export function BooksMark() {
  const book = (cloth: (typeof CLOTHS)[number], size: string) => (
    <span className={`relative block ${size} h-[7px] rounded-[1px] border ${cloth.body}`}>
      <span className={`absolute inset-y-0 left-[2px] w-[2px] ${cloth.band}`} />
      <span className={`absolute inset-y-0 right-[2px] w-[2px] ${cloth.band}`} />
    </span>
  );
  return (
    <span aria-hidden className="flex flex-col items-center gap-px">
      {book(CLOTHS[0], "w-[20px] left-[-3px]")}
      {book(CLOTHS[3], "w-[26px] left-[2px]")}
      {book(CLOTHS[1], "w-[23px] left-[-1px]")}
    </span>
  );
}

/**
 * The shelf rail, down the left of every page of a grimoire: the Library's mark, the way back to it, and the machine's
 * grimoires as small books shelf by shelf, as the Library stacks them, so any project is one press away.
 */
export function ShelfRail() {
  const status = useQuery(() => api.status(), []);
  const stacks = status.data ? libraryStacks(status.data) : [];
  const here = status.data?.grimoire;
  return (
    // Two rows of the page's grid, so the mark sits level with the header and the shelves beside the view.
    <nav aria-label="Projects" data-shelf-rail className="row-span-2 grid grid-rows-subgrid w-[52px] bg-bg-1 border-r border-line">
      <a
        href="/"
        title="Library"
        aria-label="Library"
        className="flex items-center justify-center border-b border-line hover:bg-bg-2 focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-accent"
      >
        <BooksMark />
      </a>
      <div className="min-h-0 overflow-y-auto overflow-x-hidden flex flex-col items-center gap-5 pt-4 pb-6">
        {stacks.map((stack) => (
          <div key={stack.key} data-shelf title={stack.label} className="flex flex-col items-center">
            <ul aria-label={stack.server ? `From ${stack.label}` : "On this machine"} className="m-0 p-0 list-none flex flex-col items-center gap-[6px]">
              {stack.books.map((book) => (
                <li key={book.name}>
                  <RailBook book={book} here={book.name === here} />
                </li>
              ))}
            </ul>
            {/* The plank the shelf's books stand on. */}
            <div aria-hidden className="mt-[6px] w-[42px] h-[4px] rounded-[1px] bg-bg-3 border-t border-line shadow-[0_6px_8px_-3px_var(--color-shade)]" />
          </div>
        ))}
      </div>
    </nav>
  );
}

/** A grimoire on the rail: a small flat book in its Library cloth, its initial on the spine. */
function RailBook({ book, here }: { book: Book; here: boolean }) {
  const h = hashOf(book.name);
  const cloth = clothOf(book.name);
  return (
    <a
      href={book.href}
      data-rail-book
      title={book.name}
      aria-label={book.name}
      aria-current={here ? "page" : undefined}
      className={`relative flex items-center justify-center ${pick(RAIL_WIDTHS, h, 4)} h-[18px] ${pick(RAIL_OFFSETS, h, 14)} rounded-[2px] border ${cloth.body} text-fg hover:no-underline hover:text-accent focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent ${here ? "ring-2 ring-accent ring-offset-1 ring-offset-bg-1" : ""} motion-safe:transition-transform motion-safe:duration-200 motion-safe:ease-out motion-safe:hover:translate-x-1 motion-safe:focus-visible:translate-x-1`}
    >
      <span aria-hidden className={`absolute inset-y-0 left-[3px] w-[2px] ${cloth.band}`} />
      <span aria-hidden className={`absolute inset-y-0 right-[3px] w-[2px] ${cloth.band}`} />
      <span aria-hidden className="font-serif text-[12px] font-semibold leading-none">
        {book.name.charAt(0).toUpperCase()}
      </span>
    </a>
  );
}
