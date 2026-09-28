/**
 * # Module contract
 *
 * ```json
 * {"purpose":"The shell: routes to the summary, the map, the board and node pages, the header with the section tabs, status and finding count, and search over find.","criteria":[{"id":"four-views","statement":"The routes are the summary at /, the map at /map, the board at /board and a node page at /n/<ref>; nothing else."},{"id":"lit-tab","statement":"The header's first tab is Grimoire and is the brand; exactly one tab is lit at all times: the current section on /, /map and /board, and on a node page the section the node was reached from, falling back to Board for a ticket and Grimoire for anything else when the page was opened cold."},{"id":"search-is-find","statement":"The search box calls the find tool and offers its results as links; Enter opens the first."},{"id":"visual-baselines","statement":"Every route this shell serves is screenshotted by the visual test in web/visual against a running Grimoire and compared with a baseline committed in web/visual/baseline; a view that renders differently fails the test until the new screenshot is looked at and the baseline updated on purpose."}]}
 * ```
 */

import { useEffect, useRef, useState } from "react";
import { BrowserRouter, Link, Route, Routes, useLocation, useNavigate } from "react-router-dom";
import { api, useQuery } from "./api";
import { KindTag, mono, nodePath } from "./components";
import { idOf, kindOf, type Summary } from "./types";
import { BoardView } from "./views/Board";
import { HomeView } from "./views/Home";
import { MapView } from "./views/Map";
import { NodeView } from "./views/Node";

export function App() {
  return (
    <BrowserRouter>
      <div className="grid grid-rows-[auto_1fr] h-full">
        <Top />
        <div className="overflow-auto px-5 pt-4 pb-10">
          <Routes>
            <Route path="/" element={<HomeView />} />
            <Route path="/map" element={<MapView />} />
            <Route path="/board" element={<BoardView />} />
            <Route path="/n/:ref" element={<NodeView />} />
          </Routes>
        </div>
      </div>
    </BrowserRouter>
  );
}

type Section = "grimoire" | "map" | "board";

const SECTION_PATH: Record<Section, string> = { grimoire: "/", map: "/map", board: "/board" };

function sectionOfPath(pathname: string): Section | null {
  if (pathname === "/") return "grimoire";
  if (pathname.startsWith("/map")) return "map";
  if (pathname.startsWith("/board")) return "board";
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

function BookIcon() {
  return (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d="M12 6.5c-1.6-1.4-3.9-2-7-2v13c3.1 0 5.4.6 7 2 1.6-1.4 3.9-2 7-2v-13c-3.1 0-5.4.6-7 2z" />
      <path d="M12 6.5v13" />
    </svg>
  );
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
  const findings = lint.data?.findings.length ?? 0;
  const repo = status.data?.repository.split("/").pop();
  return (
    <header className="flex items-center gap-4 px-4 py-2 bg-bg-1 border-b border-line">
      <nav className="flex gap-1 items-center">
        <Tab section="grimoire" lit={lit}>
          <BookIcon />
          <span className="font-bold tracking-wide">Grimoire</span>
        </Tab>
        <Tab section="map" lit={lit}>
          Map
        </Tab>
        <Tab section="board" lit={lit}>
          Board
        </Tab>
      </nav>
      <Search />
      <span className="flex-1" />
      <span className="text-fg-faint text-xs">
        {status.error && <span className="text-bad">daemon unreachable</span>}
        {status.data && (
          <>
            {repo} · {status.data.nodes.page} pages · {status.data.nodes.req} reqs · {status.data.nodes.module} modules · {status.data.nodes.ticket} tickets ·{" "}
            <Link to="/map" className={findings ? "text-bad" : ""}>
              {findings} finding{findings === 1 ? "" : "s"}
            </Link>
          </>
        )}
      </span>
    </header>
  );
}

function Search() {
  const [q, setQ] = useState("");
  const [hits, setHits] = useState<Summary[]>([]);
  const [open, setOpen] = useState(false);
  const nav = useNavigate();
  const box = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!q.trim()) {
      setHits([]);
      return;
    }
    const t = setTimeout(() => api.find(q, undefined, 12).then((r) => setHits(r.nodes)).catch(() => setHits([])), 120);
    return () => clearTimeout(t);
  }, [q]);
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
          if (e.key === "Enter" && hits[0]) {
            nav(nodePath(hits[0].ref));
            setOpen(false);
          }
          if (e.key === "Escape") setOpen(false);
        }}
      />
      {open && hits.length > 0 && (
        <div className="absolute top-full left-0 right-0 z-10 mt-1 bg-bg-2 border border-line rounded-md max-h-[60vh] overflow-auto shadow-xl shadow-bg">
          {hits.map((h) => (
            <Link key={h.ref} to={nodePath(h.ref)} onClick={() => setOpen(false)} className="block px-2.5 py-1.5 text-fg hover:bg-bg-3 hover:no-underline">
              <KindTag ref={h.ref} /> {h.title} <span className={`text-fg-faint ${mono}`}>{idOf(h.ref)}</span>
            </Link>
          ))}
        </div>
      )}
    </div>
  );
}
