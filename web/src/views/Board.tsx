/**
 * # Module contract
 *
 * ```json
 * {"purpose":"The board: tickets from the board tool with kind, status and priority toggles.","criteria":[{"id":"first-screen","statement":"The default filter is open and active change tickets, so gaps and questions are off the first screen."},{"id":"order-from-tool","statement":"Rows appear in the order the board tool returns them; the view does not re-sort."}]}
 * ```
 */

import { useState } from "react";
import { api, useQuery } from "../api";
import { ErrorBox, TicketTable } from "../components";

const KINDS = ["change", "gap", "question"] as const;
const STATUS = ["open", "active", "closed"] as const;
const PRIORITY = ["next", "high", "later"] as const;

function Toggle<T extends string>({ label, all, on, set }: { label: string; all: readonly T[]; on: T[]; set: (v: T[]) => void }) {
  return (
    <div className="group">
      <span>{label}</span>
      {all.map((v) => (
        <button key={v} className={on.includes(v) ? "on" : ""} onClick={() => set(on.includes(v) ? on.filter((x) => x !== v) : [...on, v])}>
          {v}
        </button>
      ))}
    </div>
  );
}

/** The first screen: next, then high, change tickets. Gaps and questions are a toggle away. */
export function BoardView() {
  const [kinds, setKinds] = useState<(typeof KINDS)[number][]>(["change"]);
  const [status, setStatus] = useState<(typeof STATUS)[number][]>(["open", "active"]);
  const [priority, setPriority] = useState<(typeof PRIORITY)[number][]>([...PRIORITY]);
  const { data, error } = useQuery(() => api.board({ kinds, status, priority }), [kinds.join(), status.join(), priority.join()]);
  return (
    <div className="board">
      <div className="filters">
        <Toggle label="Kind" all={KINDS} on={kinds} set={setKinds} />
        <Toggle label="Status" all={STATUS} on={status} set={setStatus} />
        <Toggle label="Priority" all={PRIORITY} on={priority} set={setPriority} />
      </div>
      {error && <ErrorBox error={error} />}
      {data && <TicketTable tickets={data.tickets} empty="No tickets match. File one through the tools: `file_ticket`." />}
    </div>
  );
}
