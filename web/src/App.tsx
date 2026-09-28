/**
 * # Module contract
 *
 * ```json
 * {"purpose":"The shell: routes to the summary, the map, the board and node pages, the header with status and finding count, and search over find.","criteria":[{"id":"four-views","statement":"The routes are the summary at /, the map at /map, the board at /board and a node page at /n/<ref>; nothing else."},{"id":"search-is-find","statement":"The search box calls the find tool and offers its results as links; Enter opens the first."}]}
 * ```
 */

import { useEffect, useRef, useState } from "react";
import { BrowserRouter, Link, NavLink, Route, Routes, useNavigate } from "react-router-dom";
import { api, useQuery } from "./api";
import { KindTag, mono, nodePath } from "./components";
import { idOf, type Summary } from "./types";
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

const navLink = ({ isActive }: { isActive: boolean }) =>
  `px-2.5 py-1 rounded-md hover:bg-bg-2 hover:text-fg hover:no-underline ${isActive ? "bg-bg-2 text-fg" : "text-fg-dim"}`;

function Top() {
  const status = useQuery(() => api.status(), []);
  const lint = useQuery(() => api.lint(), []);
  const findings = lint.data?.findings.length ?? 0;
  const repo = status.data?.repository.split("/").pop();
  return (
    <header className="flex items-center gap-4 px-4 py-2 bg-bg-1 border-b border-line">
      <Link to="/" className="font-bold tracking-wide text-fg">
        grimoire
      </Link>
      <nav className="flex gap-1">
        <NavLink to="/" end className={navLink}>
          Manual
        </NavLink>
        <NavLink to="/map" className={navLink}>
          Map
        </NavLink>
        <NavLink to="/board" className={navLink}>
          Board
        </NavLink>
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
