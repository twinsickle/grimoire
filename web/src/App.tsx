/**
 * # Module contract
 *
 * ```json
 * {"purpose":"The shell: routes to the grimoire, the cauldron and node pages, the header with the section tabs, status and the new and inherited finding counts, and search over find.","criteria":[{"id":"three-views","statement":"The routes are the grimoire at /, at /n/page:<slug> for a page and at /unplaced; the cauldron at /cauldron; a node page at /n/<ref> for every other kind; /map redirects to / and /board to /cauldron; nothing else; under /g/<name>/ the same routes are that grimoire's, every link staying there."},{"id":"lit-tab","statement":"The header's first tab is Grimoire with a book and is the brand, the second Cauldron with a cauldron; exactly one tab is lit at all times: Grimoire on the grimoire view, Cauldron on /cauldron, and on a node page the section the node was reached from, falling back to Cauldron for a ticket and Grimoire for anything else when the page was opened cold."},{"id":"search-is-find","statement":"The search box calls the find tool and offers its results as links; Enter opens the first; only the answer to the latest text is offered, an earlier one arriving after it dropped."},{"id":"visual-baselines","statement":"Every route this shell serves, from one list shared with the behaviour specs, is screenshotted by the visual test in web/visual against the behaviour fixture store and compared with a baseline committed in web/visual/baseline; a view that renders differently fails the test until the new screenshot is looked at and the baseline updated on purpose, and a change to the grimoire's own content never does."},{"id":"header-away","statement":"The header says the daemon is not answering and that the page retries while useAway is true, and says nothing of it once the daemon answers."},{"id":"header-findings","statement":"The header shows the lint answer's new findings as a warning and the rest as inherited, dimmed, and never shows inherited findings as a warning."},{"id":"header-advisories","statement":"The header shows the count of the lint answer's advisories as may conflict, apart from the finding counts and never as a warning, its title saying they are other work's and do not fail the check, and nothing of them when there are none."},{"id":"library-switcher","statement":"The header names the grimoire status says is on screen; when status lists more than one grimoire or checkout in its library it offers each, a grimoire served for several checkouts once for each, labelled with its checkout's folder, the one on screen chosen, grouped by where each comes from, this machine and then each team's server by its address, when they come from more than one place, and choosing another opens /g/<name>/?checkout=<checkout> as a new page; with none named it shows nothing."}]}
 * ```
 */

import { useEffect, useRef, useState } from "react";
import { BrowserRouter, Link, Navigate, Route, Routes, useLocation, useNavigate, useParams } from "react-router-dom";
import { api, base, useAway, useQuery } from "./api";
import { BookIcon, CauldronIcon, KindTag, mono } from "./components";
import { idOf, kindOf, libraryChoices, libraryGroups, libraryShown, nodePath, serverAway, type Summary } from "./types";
import { BoardView } from "./views/Board";
import { HomeView } from "./views/Home";
import { NodeView } from "./views/Node";

export function App() {
  return (
    // Under /g/<name>/ every route is that grimoire's, so every link stays there.
    <BrowserRouter basename={base || undefined}>
      <div className="grid grid-rows-[auto_1fr] h-full">
        <Top />
        <main className="overflow-auto px-5 pt-4 pb-10">
          <Routes>
            <Route path="/" element={<HomeView at={{ root: true }} />} />
            <Route path="/unplaced" element={<HomeView at={{ unplaced: true }} />} />
            <Route path="/map" element={<Navigate to="/" replace />} />
            <Route path="/cauldron" element={<BoardView />} />
            <Route path="/board" element={<Navigate to="/cauldron" replace />} />
            <Route path="/n/:ref" element={<NodeRoute />} />
          </Routes>
        </main>
      </div>
    </BrowserRouter>
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
  if (pathname === "/" || pathname === "/unplaced" || pathname.startsWith("/n/page%3A") || pathname.startsWith("/n/page:")) return "grimoire";
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
  const status = useQuery(() => api.status(), []);
  const lint = useQuery(() => api.lint(), []);
  const lit = useLitSection();
  const fresh = lint.data?.new ?? 0;
  const inherited = (lint.data?.findings.length ?? 0) - fresh;
  // Other work's, never this checkout's to fix: shown apart, not as a warning (req:check-own-work).
  const conflicts = lint.data?.advisories.length ?? 0;
  const repo = status.data?.repository.split("/").pop();
  const away = serverAway(status.data?.server);
  const daemonAway = useAway();
  return (
    <header className="flex items-center gap-4 px-4 py-2 bg-bg-1 border-b border-line">
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
      <Search />
      <span className="flex-1" />
      <Library grimoire={status.data?.grimoire ?? null} checkout={status.data?.repository ?? null} library={status.data?.library ?? []} />
      <span className="text-fg-faint text-xs">
        {daemonAway ? <span className="text-bad">daemon not answering · retrying</span> : status.error && <span className="text-bad">{status.error.message}</span>}
        {status.data && (
          <>
            {away && <span className="text-bad font-semibold">{away} · </span>}
            {/* The switcher names the grimoire; the folder is said only when nothing else names it. */}
            {!status.data.grimoire && `${repo} · `}
            {status.data.nodes.page} pages · {status.data.nodes.req} reqs · {status.data.nodes.module} modules · {status.data.nodes.ticket} tickets ·{" "}
            {fresh > 0 && <span className="text-bad font-semibold">{fresh} new</span>}
            {fresh > 0 && inherited > 0 && " · "}
            {inherited > 0 && <span>{inherited} inherited</span>}
            {fresh === 0 && inherited === 0 && <span>no findings</span>}
            {conflicts > 0 && (
              <span title="Other work, landed since this branch's base or being worked elsewhere, that may conflict with this checkout. It does not fail the check.">
                {" · "}
                {conflicts} may conflict
              </span>
            )}
          </>
        )}
      </span>
    </header>
  );
}

/**
 * The grimoire on screen and, when the daemon's library holds others, a list
 * of them. Choosing one opens it at its own address as a page of its own, so
 * every query and the event stream are that grimoire's from the start.
 */
function Library({ grimoire, checkout, library }: { grimoire: string | null; checkout: string | null; library: { name: string; repository: string; server: string | null }[] }) {
  const shown = libraryShown(grimoire, library);
  const choices = libraryChoices(grimoire, checkout, library);
  if (shown === "none") return null;
  if (shown === "name") return <span className={`text-fg-dim text-xs ${mono}`} aria-label="Grimoire">{grimoire}</span>;
  return (
    <label className="text-xs text-fg-dim inline-flex items-center gap-1.5">
      <span className="sr-only">Grimoire</span>
      <select
        aria-label="Grimoire"
        className={`bg-bg-2 border border-line rounded px-1.5 py-0.5 text-fg ${mono}`}
        value={choices.find((c) => c.chosen)?.key}
        onChange={(e) => {
          const to = choices.find((c) => c.key === e.target.value);
          if (to) window.location.assign(to.href);
        }}
      >
        {libraryGroups(choices).map((group) => {
          const options = group.choices.map((c) => (
            <option key={c.key} value={c.key} title={c.key.split("\n")[1]}>
              {c.label}
            </option>
          ));
          // Grouped by where each comes from, each server and this machine, only when there is more than one.
          return group.label === null ? (
            options
          ) : (
            <optgroup key={group.key} label={group.label}>
              {options}
            </optgroup>
          );
        })}
      </select>
    </label>
  );
}

function Search() {
  const [q, setQ] = useState("");
  const [hits, setHits] = useState<{ q: string; nodes: Summary[] }>({ q: "", nodes: [] });
  const [open, setOpen] = useState(false);
  const nav = useNavigate();
  const box = useRef<HTMLDivElement>(null);
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
    document.addEventListener("mousedown", onDoc);
    return () => document.removeEventListener("mousedown", onDoc);
  }, []);
  return (
    <div className="relative" ref={box}>
      <input
        className="w-80"
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
      {open && shown.length > 0 && (
        <div className="absolute top-full left-0 right-0 z-10 mt-1 bg-bg-2 border border-line rounded-md max-h-[60vh] overflow-auto shadow-xl shadow-bg">
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
