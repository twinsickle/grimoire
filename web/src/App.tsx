/**
 * # Module contract
 *
 * ```json
 * {"purpose":"The shell: routes to the grimoire, the cauldron and node pages beside the shelf rail, and the header: where the page is in its project, the section tabs, and search over find.","criteria":[{"id":"three-views","statement":"Under /g/<name>/ the routes are that grimoire's, every link staying there: the grimoire at /, at /n/page:<slug> for a page and at /unplaced; the cauldron at /cauldron; a node page at /n/<ref> for every other kind; the findings view at /findings; /map redirects to / and /board to /cauldron; nothing else; every view there is drawn beside the shelf rail, ShelfRail, down the left of the header and the view. At the daemon's own address, once status names the default grimoire, / is the Library, drawn without a project's header since it belongs to no one project, and every other path is replaced by the same path, query and fragment under the default grimoire's /g/<name>/; when status names none, a store served alone with no library, the same routes are served at the daemon's own address, with no shelf rail, since there is no Library to lead to. Which it is, status's last answer is remembered in the browser, so a page opened while the daemon is away goes where it went before; never told, it shows the views, which say the daemon is not answering."},{"id":"lit-tab","statement":"The header's first tab is Grimoire with a book and is the brand, the second Cauldron with a cauldron; exactly one tab is lit at all times: Grimoire on the grimoire view and the findings view, Cauldron on /cauldron, and on a node page the section the node was reached from, falling back to Cauldron for a ticket and Grimoire for anything else when the page was opened cold."},{"id":"search-is-find","statement":"The search box, at the header's right, calls the find tool and offers its results as links; Enter opens the first; only the answer to the latest text is offered, an earlier one arriving after it dropped; / puts the cursor in it unless the cursor is in a field already, where / is typed."},{"id":"visual-baselines","statement":"Every route this shell serves, from one list shared with the behaviour specs, is screenshotted by the visual test in web/visual against the behaviour fixture store and compared with a baseline committed in web/visual/baseline; a view that renders differently fails the test until the new screenshot is looked at and the baseline updated on purpose, and a change to the grimoire's own content never does."},{"id":"header-away","statement":"While useAway is true the header's pill says the daemon is not answering and that the page retries, its dot in the bad colour, and says nothing of it once the daemon answers."},{"id":"header-findings","statement":"The header's pill shows the lint answer's new findings as N new in the bad colour, only when there are any; its panel shows N new, in the bad colour when there are any, and the rest as N inherited, dimmed; inherited findings are never shown as a warning. Each count in the panel, may conflict's among them, is a link to the findings view, and following it closes the panel."},{"id":"header-advisories","statement":"The header's panel shows the count of the lint answer's advisories as N may conflict, beside the finding counts and never as a warning, its title saying they are other work's and do not fail the check."},{"id":"where-you-are","statement":"The header is three parts, the pill on the left, the tabs in the centre and the search on the right, and shows no node counts. The pill holds a dot, in the bad colour while serverAway gives a warning, status fails or useAway is true and else the ok colour; the grimoire status names, else the checkout's folder, and after it / and the checkout's folder when checkoutsOf gives more than one; the id of status's current ticket when it has one; and a chevron. Pressed, it opens a panel beneath it, lifted off the page by a shadow in the shade colour, closed by pressing it again, a click outside or Escape: the server's state in words, this machine for a store of its own; under Checkouts of this project, the checkouts checkoutsOf gives, those status lists of this grimoire alone, each its folder and its current ticket's id and title or no ticket, the one on screen marked and each other a link opening /g/<name>/?checkout=<checkout> as a new page; and the finding counts; it holds no link to other projects or the Library, which the shelf rail gives."}]}
 * ```
 */

import { useEffect, useRef, useState } from "react";
import { BrowserRouter, Link, Navigate, Route, Routes, useLocation, useNavigate, useParams } from "react-router-dom";
import { api, base, useAway, useQuery } from "./api";
import { BookIcon, CauldronIcon, KindTag, mono } from "./components";
import { checkoutsOf, folderOf, idOf, kindOf, nodePath, serverState, type Summary } from "./types";
import { BoardView } from "./views/Board";
import { HomeView } from "./views/Home";
import { LibraryView, ShelfRail } from "./views/Library";
import { NodeView } from "./views/Node";
import { FindingsView } from "./views/Findings";

export function App() {
  // Under /g/<name>/ every route is that grimoire's, so every link stays there.
  if (base) return <ProjectAt basename={base} />;
  return <DaemonRoot />;
}

/**
 * The daemon's own address. With a library, / is the Library, the machine's grimoires, where projects are
 * switched, and any other path is the default grimoire's at its own /g/<name>/, the address its views live at.
 * A store served alone, with no library and so no /g/<name>/, keeps its views at the bare routes.
 */
function DaemonRoot() {
  const status = useQuery(() => api.status(), []);
  // While the daemon is away, what status last said of the default grimoire, so the page still knows where to go.
  const named = status.data ? status.data.grimoire : status.error ? remembered() : undefined;
  useEffect(() => {
    if (status.data) remember(status.data.grimoire);
  }, [status.data]);
  const { pathname, search, hash } = window.location;
  const moving = !!named && pathname !== "/";
  useEffect(() => {
    if (moving) window.location.replace(`/g/${named}${pathname}${search}${hash}`);
  }, [moving, named, pathname, search, hash]);
  // Not known yet: nothing until status answers, or the views, which say the daemon is not answering, if it does not.
  if (named === undefined) return status.error ? <ProjectAt /> : null;
  if (named === null) return <ProjectAt />;
  return pathname === "/" ? <LibraryView /> : null;
}

const DEFAULT_KEY = "grimoire.default";

/** The default grimoire's name as status last gave it, null for a store served alone, undefined when never told. */
function remembered(): string | null | undefined {
  try {
    const v = localStorage.getItem(DEFAULT_KEY);
    return v === null ? undefined : v || null;
  } catch {
    return undefined;
  }
}

function remember(grimoire: string | null) {
  try {
    localStorage.setItem(DEFAULT_KEY, grimoire ?? "");
  } catch {
    // Storage refused: the next page asks status again.
  }
}

function ProjectAt({ basename }: { basename?: string }) {
  return (
    <BrowserRouter basename={basename}>
      {/* A grimoire at its /g/<name>/ is one of a library, so the rail leads to the rest; a store served alone has none. */}
      <Project shelf={!!basename} />
    </BrowserRouter>
  );
}

/** One project's views, under its header, beside the shelf rail when the project is one of a library. */
function Project({ shelf }: { shelf: boolean }) {
  return (
    <div className={`grid grid-rows-[auto_1fr] h-full ${shelf ? "grid-cols-[auto_1fr]" : ""}`}>
      {shelf && <ShelfRail />}
      <Top />
      <main className="min-w-0 overflow-auto px-5 pt-4 pb-10">
        <Routes>
          <Route path="/" element={<HomeView at={{ root: true }} />} />
          <Route path="/unplaced" element={<HomeView at={{ unplaced: true }} />} />
          <Route path="/map" element={<Navigate to="/" replace />} />
          <Route path="/cauldron" element={<BoardView />} />
          <Route path="/board" element={<Navigate to="/cauldron" replace />} />
          <Route path="/n/:ref" element={<NodeRoute />} />
          <Route path="/findings" element={<FindingsView />} />
        </Routes>
      </main>
    </div>
  );
}

/** A page is shown by the grimoire view, which is its node page; every other kind by the node page. */
function NodeRoute() {
  const { ref: raw } = useParams();
  const ref = decodeURIComponent(raw ?? "");
  return kindOf(ref) === "page" ? <HomeView at={{ page: ref }} /> : <NodeView />;
}

type Section = "grimoire" | "board";

const SECTION_PATH: Record<Section, string> = { grimoire: "/", board: "/cauldron" };

function sectionOfPath(pathname: string): Section | null {
  if (pathname === "/" || pathname === "/unplaced" || pathname === "/findings" || pathname.startsWith("/n/page%3A") || pathname.startsWith("/n/page:")) return "grimoire";
  if (pathname.startsWith("/cauldron") || pathname.startsWith("/board")) return "board";
  return null;
}

/**
 * Which tab is lit. On a section route, that section. On a node page, the
 * section the node was reached from, remembered as the route changes; when
 * the page was opened cold, the node's kind decides.
 */
function useLitSection(): Section {
  const { pathname } = useLocation();
  const last = useRef<Section | null>(null);
  const here = sectionOfPath(pathname);
  if (here) last.current = here;
  if (here) return here;
  if (last.current) return last.current;
  const m = pathname.match(/^\/n\/(.+)$/);
  const ref = m ? decodeURIComponent(m[1]) : "";
  return ref && kindOf(ref) === "ticket" ? "board" : "grimoire";
}

function Tab({ section, lit, children }: { section: Section; lit: Section; children: React.ReactNode }) {
  const on = section === lit;
  return (
    <Link
      to={SECTION_PATH[section]}
      aria-current={on ? "page" : undefined}
      className={`inline-flex items-center gap-1.5 px-2.5 py-1.5 -mb-[9px] border-b-2 hover:no-underline ${
        on ? "text-accent border-accent" : "text-fg-dim border-transparent hover:text-fg"
      }`}
    >
      {children}
    </Link>
  );
}

function Top() {
  const lit = useLitSection();
  return (
    // Three parts: where the page is on the left, the two views in the centre, search on the right.
    <header className="min-w-0 grid grid-cols-[1fr_auto_1fr] items-center gap-4 px-4 py-2 bg-bg-1 border-b border-line">
      <div className="justify-self-start min-w-0">
        <Where />
      </div>
      <nav className="flex gap-1 items-center">
        <Tab section="grimoire" lit={lit}>
          <BookIcon />
          <span className="font-bold tracking-wide">Grimoire</span>
        </Tab>
        <Tab section="board" lit={lit}>
          <CauldronIcon />
          Cauldron
        </Tab>
      </nav>
      <div className="justify-self-end">
        <Search />
      </div>
    </header>
  );
}

function Dot({ bad }: { bad: boolean }) {
  return <span aria-hidden className={`inline-block size-2 shrink-0 rounded-full ${bad ? "bg-bad" : "bg-ok"}`} />;
}

/**
 * Where the page is in its project, as one pill: the server's dot, the grimoire
 * and the checkout, its current ticket and its new findings. Opened, the
 * project's checkouts and the finding counts; the way to every project is the shelf rail.
 */
function Where() {
  const status = useQuery(() => api.status(), []);
  const lint = useQuery(() => api.lint(), []);
  const daemonAway = useAway();
  const [open, setOpen] = useState(false);
  const box = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const onDoc = (e: MouseEvent) => {
      if (!box.current?.contains(e.target as Node)) setOpen(false);
    };
    // Escape closes it, and so does "/", which takes the cursor to the search.
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" || e.key === "/") setOpen(false);
    };
    document.addEventListener("mousedown", onDoc);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDoc);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);
  const fresh = lint.data?.new ?? 0;
  const inherited = (lint.data?.findings.length ?? 0) - fresh;
  // Other work's, never this checkout's to fix: shown apart, not as a warning (req:check-own-work).
  const conflicts = lint.data?.advisories.length ?? 0;
  const s = status.data;
  const server = daemonAway ? { words: "daemon not answering · retrying", bad: true } : serverState(s?.server);
  const folder = s ? folderOf(s.repository) : null;
  const here = s ? checkoutsOf(s) : [];
  return (
    <div className="relative" ref={box}>
      <button
        type="button"
        // Its name says what it shows, so a screen reader hears where the page is, not only what the button is.
        aria-label={`Where you are: ${[s?.grimoire ?? folder, here.length > 1 && s?.grimoire ? folder : null, s?.current?.id, fresh > 0 ? `${fresh} new` : null, daemonAway ? "daemon not answering" : server.bad ? server.words : null].filter(Boolean).join(", ")}`}
        aria-expanded={open}
        title={server.words}
        onClick={() => setOpen((o) => !o)}
        className="inline-flex items-center gap-2 max-w-full rounded-full bg-bg-2 border border-accent-dim hover:border-accent px-3 py-1 text-xs text-fg"
      >
        <Dot bad={server.bad || !!status.error} />
        {s && (
          <span className="inline-flex items-baseline gap-1.5 min-w-0 truncate">
            <span className={`${mono} font-bold`}>{s.grimoire ?? folder}</span>
            {/* The checkout is named only when the project has more than one to tell apart. */}
            {s.grimoire && here.length > 1 && (
              <>
                {" "}
                <span className="text-fg-faint">/</span> <span className={mono}>{folder}</span>
              </>
            )}
            {s.current && (
              <>
                {" "}
                <span className={`${mono} text-ticket`}>{s.current.id}</span>
              </>
            )}
          </span>
        )}
        {fresh > 0 && <span className="text-bad font-semibold whitespace-nowrap">{fresh} new</span>}
        {daemonAway ? (
          <span className="text-bad whitespace-nowrap">daemon not answering · retrying</span>
        ) : (
          status.error && <span className="text-bad truncate">{status.error.message}</span>
        )}
        <svg aria-hidden width="10" height="10" viewBox="0 0 10 10" className={`shrink-0 text-fg-dim transition-transform ${open ? "rotate-180" : ""}`}>
          <path d="M2 3.5 5 6.5 8 3.5" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" />
        </svg>
      </button>
      {open && (
        <div role="dialog" aria-label="Checkouts" className="absolute left-0 top-full z-20 mt-2 w-[380px] max-w-[calc(100vw-2rem)] bg-bg-2 border border-line rounded-md shadow-[0_20px_50px_-4px_var(--color-shade),0_6px_16px_-2px_var(--color-shade)] p-3 text-xs">
          <div title={s?.server?.address} className={`flex items-center gap-2 ${server.bad ? "text-bad" : "text-fg-dim"}`}>
            <Dot bad={server.bad} />
            {server.words}
          </div>
          <h2 className="mt-3 mb-1.5 text-[10.5px] font-semibold uppercase tracking-[0.08em] text-fg-faint">Checkouts of this project</h2>
          <ul className="m-0 p-0 list-none">
            {here.map((c) => {
              const line = (
                <>
                  <span className={`${mono} ${c.here ? "text-accent font-semibold" : ""}`}>{c.folder}</span>{" "}
                  {c.current ? (
                    <span className="min-w-0 truncate">
                      <span className={`${mono} text-ticket`}>{c.current.id}</span> <span className={c.here ? "text-fg" : "text-fg-dim"}>{c.current.title}</span>
                    </span>
                  ) : (
                    <span className="text-fg-faint">no ticket</span>
                  )}
                </>
              );
              return (
                <li key={c.key}>
                  {c.here ? (
                    <span aria-current="true" className="flex items-baseline gap-2 px-2 py-1 -mx-2 rounded bg-bg-3">
                      {line}
                    </span>
                  ) : (
                    <a href={c.href} title={c.key} className="flex items-baseline gap-2 px-2 py-1 -mx-2 rounded text-fg hover:bg-bg-3 hover:no-underline">
                      {line}
                    </a>
                  )}
                </li>
              );
            })}
          </ul>
          <hr className="my-2.5 border-0 border-t border-line" />
          {/* Each count opens the one view of them all. */}
          <div className="text-fg-dim">
            <Link to="/findings" onClick={() => setOpen(false)} className={fresh > 0 ? "text-bad font-semibold" : "text-fg-dim"}>
              {fresh} new
            </Link>
            {" · "}
            <Link to="/findings" onClick={() => setOpen(false)} className="text-fg-dim">
              {inherited} inherited
            </Link>
            {" · "}
            <Link
              to="/findings"
              onClick={() => setOpen(false)}
              className="text-fg-dim"
              title="Other work, on the main line since this branch's base or being worked elsewhere, that may conflict with this checkout. It does not fail the check."
            >
              {conflicts} may conflict
            </Link>
          </div>
        </div>
      )}
    </div>
  );
}

function Search() {
  const [q, setQ] = useState("");
  const [hits, setHits] = useState<{ q: string; nodes: Summary[] }>({ q: "", nodes: [] });
  const [open, setOpen] = useState(false);
  const nav = useNavigate();
  const box = useRef<HTMLDivElement>(null);
  const input = useRef<HTMLInputElement>(null);
  // Each find is numbered; only the latest one's answer is kept, so a slower
  // answer to earlier text can neither replace the hits nor be opened by Enter.
  // The hits carry the text they answer and show only for that text, so none
  // are offered while the latest text is still being answered.
  const latest = useRef(0);
  useEffect(() => {
    const n = ++latest.current;
    if (!q.trim()) return;
    const keep = (nodes: Summary[]) => {
      if (n === latest.current) setHits({ q, nodes });
    };
    const t = setTimeout(() => api.find(q, undefined, 12).then((r) => keep(r.nodes)).catch(() => keep([])), 120);
    return () => clearTimeout(t);
  }, [q]);
  // An empty box shows nothing, derived rather than cleared in the effect.
  const shown = q.trim() && hits.q === q ? hits.nodes : [];
  useEffect(() => {
    const onDoc = (e: MouseEvent) => {
      if (!box.current?.contains(e.target as Node)) setOpen(false);
    };
    // "/" finds from anywhere but a field, where it is typed as usual.
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "/" || e.ctrlKey || e.metaKey || e.altKey || inField(e.target)) return;
      e.preventDefault();
      input.current?.focus();
    };
    document.addEventListener("mousedown", onDoc);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDoc);
      document.removeEventListener("keydown", onKey);
    };
  }, []);
  return (
    <div className="relative" ref={box}>
      <input
        ref={input}
        aria-label="Find"
        className="w-[340px] max-w-full"
        placeholder="Find a page, requirement, module, ticket…"
        value={q}
        onChange={(e) => {
          setQ(e.target.value);
          setOpen(true);
        }}
        onFocus={() => setOpen(true)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && shown[0]) {
            nav(nodePath(shown[0].ref));
            setOpen(false);
          }
          if (e.key === "Escape") setOpen(false);
        }}
      />
      {!q && (
        <kbd aria-hidden className="pointer-events-none absolute right-2 top-1/2 -translate-y-1/2 px-1.5 leading-4 rounded border border-line text-fg-faint text-[10.5px] font-mono">
          /
        </kbd>
      )}
      {open && shown.length > 0 && (
        <div className="absolute top-full left-0 right-0 z-10 mt-1 bg-bg-2 border border-line rounded-md max-h-[60vh] overflow-auto shadow-[0_20px_50px_-4px_var(--color-shade),0_6px_16px_-2px_var(--color-shade)]">
          {shown.map((h) => (
            <Link key={h.ref} to={nodePath(h.ref)} onClick={() => setOpen(false)} className="block px-2.5 py-1.5 text-fg hover:bg-bg-3 hover:no-underline">
              <KindTag ref={h.ref} /> {h.title} <span className={`text-fg-faint ${mono}`}>{idOf(h.ref)}</span>
            </Link>
          ))}
        </div>
      )}
    </div>
  );
}

/** Whether a key pressed there is typed: an input, a text area, a select or editable content. */
function inField(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  return target.isContentEditable || ["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName);
}
