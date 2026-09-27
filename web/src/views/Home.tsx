/**
 * # Module contract
 *
 * ```json
 * {"purpose":"The default view: the top of the manual as a page to read, in the root page's order, ending with every module contract grouped by area.","criteria":[{"id":"story-order","statement":"The story pages are rendered as Markdown in the root page's link order, each followed by its requirements as a compact list, and the pages under architecture as cards with their first paragraph and coverage."},{"id":"contracts-grouped","statement":"Every module with a purpose is listed once, one line each, grouped under the page that links it; files and declarations are not shown."}]}
 * ```
 */

import { Link } from "react-router-dom";
import { api, useQuery } from "../api";
import { ErrorBox, KindTag, Markdown, nodePath } from "../components";
import { kindOf, type PageNode, type Tree } from "../types";

/** Read the manual top-down: what this is, how it is built, what must hold, and what the code guarantees. */
export function HomeView() {
  const { data, error } = useQuery(() => api.outline(3), []);
  if (error) return <ErrorBox error={error} />;
  if (!data) return <p className="dim">Loading…</p>;
  const root = data.roots[0];
  if (!root) return <p className="empty">No root page. The store's `roots` names none that exist.</p>;
  const story = root.children.filter((c) => kindOf(c.ref) === "page");
  return (
    <div className="home">
      <RootHead tree={root} />
      {story.map((s) => (
        <StorySection key={s.ref} tree={s} />
      ))}
      <Contracts root={root} />
    </div>
  );
}

function RootHead({ tree }: { tree: Tree }) {
  const { data } = useQuery(() => api.get(tree.ref), [tree.ref]);
  const page = data?.node as PageNode | undefined;
  return (
    <header className="home-head">
      <h1>
        <Link to={nodePath(tree.ref)}>{tree.title}</Link>
      </h1>
      {page && <Markdown text={page.body} />}
      <nav className="home-toc">
        {tree.children
          .filter((c) => kindOf(c.ref) === "page")
          .map((c) => (
            <a key={c.ref} href={`#${anchor(c.ref)}`}>
              {c.title}
            </a>
          ))}
        <a href="#contracts">Contracts</a>
      </nav>
    </header>
  );
}

function anchor(ref: string): string {
  return ref.replace(/[^a-z0-9]+/gi, "-");
}

function StorySection({ tree }: { tree: Tree }) {
  const { data } = useQuery(() => api.get(tree.ref), [tree.ref]);
  const page = data?.node as PageNode | undefined;
  const reqs = tree.children.filter((c) => kindOf(c.ref) === "req");
  const pages = tree.children.filter((c) => kindOf(c.ref) === "page");
  return (
    <section className="home-section" id={anchor(tree.ref)}>
      <h2>
        <Link to={nodePath(tree.ref)}>{tree.title}</Link>
        {tree.findings > 0 && <span className="badge bad">{tree.findings} finding{tree.findings === 1 ? "" : "s"}</span>}
      </h2>
      {page ? <Markdown text={page.body} /> : <p className="dim">Loading…</p>}
      {reqs.length > 0 && (
        <ul className="req-list">
          {reqs.map((r) => (
            <li key={r.ref}>
              <KindTag ref={r.ref} /> <Link to={nodePath(r.ref)}>{r.title}</Link>
              {typeof r.coverage === "number" && r.coverage === 0 && <span className="badge warn">uncovered</span>}
            </li>
          ))}
        </ul>
      )}
      {pages.length > 0 && (
        <div className="cards">
          {pages.map((p) => (
            <AreaCard key={p.ref} tree={p} />
          ))}
        </div>
      )}
    </section>
  );
}

function firstParagraph(body: string): string {
  const para = body.split(/\n\s*\n/)[0] ?? "";
  return para.replace(/\s+/g, " ").trim();
}

function AreaCard({ tree }: { tree: Tree }) {
  const { data } = useQuery(() => api.get(tree.ref), [tree.ref]);
  const page = data?.node as PageNode | undefined;
  const pct = tree.coverage == null ? null : Math.round(tree.coverage * 100);
  const modules = tree.children.filter((c) => kindOf(c.ref) === "module").length;
  const reqs = tree.children.filter((c) => kindOf(c.ref) === "req").length;
  const subpages = tree.children.filter((c) => kindOf(c.ref) === "page").length;
  return (
    <Link to={nodePath(tree.ref)} className="card area-card">
      <div className="card-head">
        <strong>{tree.title}</strong>
        {pct != null && <span className={`badge ${pct === 100 ? "ok" : pct < 50 ? "warn" : ""}`}>{pct}%</span>}
        {tree.findings > 0 && <span className="badge bad">{tree.findings}</span>}
      </div>
      <p className="dim">{page ? firstParagraph(page.body) : "…"}</p>
      <p className="faint">
        {subpages > 0 && `${subpages} pages · `}
        {reqs} requirements · {modules} modules
      </p>
    </Link>
  );
}

/** Every module with a purpose, grouped by the page that links it. */
function Contracts({ root }: { root: Tree }) {
  const groups: { page: Tree; modules: Tree[] }[] = [];
  const seen = new Set<string>();
  const walk = (t: Tree) => {
    const mods = t.children.filter((c) => kindOf(c.ref) === "module" && !seen.has(c.ref));
    if (mods.length > 0) {
      mods.forEach((m) => seen.add(m.ref));
      groups.push({ page: t, modules: mods });
    }
    t.children.filter((c) => kindOf(c.ref) === "page").forEach(walk);
  };
  walk(root);
  if (groups.length === 0) return null;
  return (
    <section className="home-section" id="contracts">
      <h2>Contracts</h2>
      <p className="dim">What each module guarantees, in its own words. The code level of the manual, grouped by area.</p>
      {groups.map((g) => (
        <div key={g.page.ref} className="contract-group">
          <h3>
            <Link to={nodePath(g.page.ref)}>{g.page.title}</Link>
          </h3>
          <ul className="plain">
            {g.modules.map((m) => (
              <li key={m.ref}>
                <Link to={nodePath(m.ref)} className="mono contract-id">
                  {m.ref.slice("module:".length)}
                </Link>
                <span className="contract-purpose">{m.title}</span>
                {m.findings > 0 && <span className="badge bad">{m.findings}</span>}
              </li>
            ))}
          </ul>
        </div>
      ))}
    </section>
  );
}
