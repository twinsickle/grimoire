/**
 * # Module contract
 *
 * ```json
 * {"purpose":"The shell: routes to the map, the board and node pages, the header with status and finding count, and search over find.","criteria":[{"id":"three-views","statement":"The routes are the map at /, the board at /board and a node page at /n/<ref>; nothing else."},{"id":"search-is-find","statement":"The search box calls the find tool and offers its results as links; Enter opens the first."}]}
 * ```
 */

import { useEffect, useRef, useState } from "react";
import { BrowserRouter, Link, NavLink, Route, Routes, useNavigate } from "react-router-dom";
import { api, useQuery } from "./api";
import { KindTag, nodePath } from "./components";
import { idOf, type Summary } from "./types";
import { BoardView } from "./views/Board";
import { MapView } from "./views/Map";
import { NodeView } from "./views/Node";

export function App() {
  return (
    <BrowserRouter>
      <div className="shell">
        <Top />
        <div className="main">
          <Routes>
            <Route path="/" element={<MapView />} />
            <Route path="/board" element={<BoardView />} />
            <Route path="/n/:ref" element={<NodeView />} />
          </Routes>
        </div>
      </div>
    </BrowserRouter>
  );
}

function Top() {
  const status = useQuery(() => api.status(), []);
  const lint = useQuery(() => api.lint(), []);
  const findings = lint.data?.findings.length ?? 0;
  const repo = status.data?.repository.split("/").pop();
  return (
    <header className="top">
      <Link to="/" className="brand">
        grimoire
      </Link>
      <nav>
        <NavLink to="/" end>
          Map
        </NavLink>
        <NavLink to="/board">Board</NavLink>
      </nav>
      <Search />
      <span className="spacer" />
      <span className="status">
        {status.error && <span className="bad">daemon unreachable</span>}
        {status.data && (
          <>
            {repo} · {status.data.nodes.page} pages · {status.data.nodes.req} reqs · {status.data.nodes.module} modules · {status.data.nodes.ticket} tickets ·{" "}
            <Link to="/" className={findings ? "bad" : ""}>
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
    <div className="search" ref={box}>
      <input
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
        <div className="results">
          {hits.map((h) => (
            <Link key={h.ref} to={nodePath(h.ref)} onClick={() => setOpen(false)}>
              <KindTag ref={h.ref} /> {h.title} <span className="faint mono">{idOf(h.ref)}</span>
            </Link>
          ))}
        </div>
      )}
    </div>
  );
}
