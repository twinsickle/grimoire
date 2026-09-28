/**
 * # Module contract
 *
 * ```json
 * {"purpose":"The default view: the top of the grimoire as a page to read, in the root page's order, ending with every module contract grouped by area.","criteria":[{"id":"story-order","statement":"The story pages are rendered as Markdown in the root page's link order, each followed by its requirements as a compact list, one marked uncovered only when it names no criterion and no test holds it, and the pages under architecture as cards with their first paragraph and coverage."},{"id":"contracts-grouped","statement":"Every module with a purpose is listed once, one line each, grouped under the page that links it; files and declarations are not shown."}]}
 * ```
 */

import { Link } from "react-router-dom";
import { api, useQuery } from "../api";
import { Badge, ErrorBox, KindTag, Markdown, card, empty, mono } from "../components";
import { kindOf, nodePath, type PageNode, type Tree } from "../types";

/** Read the grimoire top-down: what this is, how it is built, what must hold, and what the code guarantees. */
export function HomeView() {
  const { data, error } = useQuery(() => api.outline(3), []);
  if (error) return <ErrorBox error={error} />;
  if (!data) return <p className="text-fg-dim">Loading…</p>;
  const root = data.roots[0];
  if (!root) return <p className={empty}>No root page. The store's `roots` names none that exist.</p>;
  const story = root.children.filter((c) => kindOf(c.ref) === "page");
  return (
    <div className="max-w-[96ch] w-full mx-auto flex flex-col gap-7">
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
  const toc = "px-2.5 py-[3px] border border-line rounded-xl text-fg-dim text-xs hover:text-fg hover:border-accent hover:no-underline";
  return (
    <header>
      <h1 className="text-[26px] mb-3">
        <Link to={nodePath(tree.ref)} className="text-fg">
          {tree.title}
        </Link>
      </h1>
      {page && <Markdown text={page.body} />}
      <nav className="flex gap-1.5 flex-wrap mt-2">
        {tree.children
          .filter((c) => kindOf(c.ref) === "page")
          .map((c) => (
            <a key={c.ref} href={`#${anchor(c.ref)}`} className={toc}>
              {c.title}
            </a>
          ))}
        <a href="#contracts" className={toc}>
          Contracts
        </a>
      </nav>
    </header>
  );
}

function anchor(ref: string): string {
  return ref.replace(/[^a-z0-9]+/gi, "-");
}

const sectionHead = "text-[19px] flex items-center gap-2.5 mb-3 pt-2 border-t border-line";

function StorySection({ tree }: { tree: Tree }) {
  const { data } = useQuery(() => api.get(tree.ref), [tree.ref]);
  const page = data?.node as PageNode | undefined;
  const reqs = tree.children.filter((c) => kindOf(c.ref) === "req");
  const pages = tree.children.filter((c) => kindOf(c.ref) === "page");
  return (
    <section id={anchor(tree.ref)}>
      <h2 className={sectionHead}>
        <Link to={nodePath(tree.ref)} className="text-fg">
          {tree.title}
        </Link>
        {tree.findings > 0 && (
          <Badge tone="bad">
            {tree.findings} finding{tree.findings === 1 ? "" : "s"}
          </Badge>
        )}
      </h2>
      {page ? <Markdown text={page.body} /> : <p className="text-fg-dim">Loading…</p>}
      {reqs.length > 0 && (
        <ul className="list-none m-0 mt-2 p-0 grid grid-cols-[repeat(auto-fill,minmax(320px,1fr))] gap-x-5 gap-y-1">
          {reqs.map((r) => (
            <li key={r.ref} className="flex gap-2 items-baseline text-[13px]">
              <KindTag ref={r.ref} /> <Link to={nodePath(r.ref)}>{r.title}</Link>
              {typeof r.coverage === "number" && r.coverage === 0 && r.tests === 0 && <Badge tone="warn">uncovered</Badge>}
            </li>
          ))}
        </ul>
      )}
      {pages.length > 0 && (
        <div className="grid grid-cols-[repeat(auto-fill,minmax(260px,1fr))] gap-2.5 mt-3">
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
    <Link to={nodePath(tree.ref)} className={`${card} block text-fg hover:border-accent hover:no-underline m-0`}>
      <div className="flex items-center gap-2 mb-1">
        <strong>{tree.title}</strong>
        {pct != null && <Badge tone={pct === 100 ? "ok" : pct < 50 ? "warn" : "plain"}>{pct}%</Badge>}
        {tree.findings > 0 && <Badge tone="bad">{tree.findings}</Badge>}
      </div>
      <p className="text-fg-dim m-0 mb-1 text-[13px]">{page ? firstParagraph(page.body) : "…"}</p>
      <p className="text-fg-faint m-0 mb-1 text-[13px]">
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
    <section id="contracts">
      <h2 className={sectionHead}>Contracts</h2>
      <p className="text-fg-dim">What each module guarantees, in its own words. The code level of the grimoire, grouped by area.</p>
      {groups.map((g) => (
        <div key={g.page.ref} className="mt-3">
          <h3 className="mb-1">
            <Link to={nodePath(g.page.ref)} className="text-fg-dim">
              {g.page.title}
            </Link>
          </h3>
          <ul className="list-none m-0 p-0 flex flex-col gap-1">
            {g.modules.map((m) => (
              <li key={m.ref} className="grid grid-cols-[minmax(180px,28ch)_1fr_auto] gap-3 items-baseline text-[13px]">
                <Link to={nodePath(m.ref)} className={`${mono} text-module overflow-hidden text-ellipsis whitespace-nowrap`}>
                  {m.ref.slice("module:".length)}
                </Link>
                <span className="text-fg">{m.title}</span>
                {m.findings > 0 && <Badge tone="bad">{m.findings}</Badge>}
              </li>
            ))}
          </ul>
        </div>
      ))}
    </section>
  );
}
