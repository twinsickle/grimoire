# What change requires what manual update

The rule is that the manual moves with the code in the same piece of work.
This is the mapping. "Page" means the area page that covers the module;
find it with `rests_on_me` on the module, or `find`.

| Change in the code | Update in the manual |
|---|---|
| New module | Contract in the file. A requirement per criterion (`put_requirement`). Link each requirement and the module from its area page (`patch_page` with `add_links`). |
| New behaviour in an existing module | New criterion in the contract. A requirement covering it. Link the requirement from the page. |
| Changed behaviour | Reword the criterion and the requirement's statement. `record_decision` on the requirement: what changed and why. |
| Removed behaviour | Remove the criterion. The requirement naming it goes dangling: reword its `covered_by`, or delete the requirement (`delete_node`) and remove its link from the page. |
| Renamed criterion id | Same as removed plus new: update `covered_by` on every requirement that named it. `find` the old id first. |
| New area (a group of modules with its own purpose) | A page under architecture (or the right parent), with intent in the body, linking its requirements and modules. `patch_page` the parent with `add_links`. |
| A choice made while working, however small it feels | `record_decision` on the requirement it governs: what was chosen and why, one paragraph, written for a reader a year from now. |
| Behaviour added to a module the ticket does not touch | `update_ticket` to add the module to `touches`, then the row above that fits. A ticket's `touches` is a claim about what changed; keep it true. |
| A question or gap discovered while working | `file_ticket` with kind `question` or `gap`, `relies_on` this ticket. Do not resolve it alone. |
| Code that turns out to contradict a requirement | Stop. Either the code is wrong (fix it) or the requirement is (reword it and `record_decision` why). Never leave both standing. |

## What does not need a manual update

- Refactors that change no guarantee: renamed locals, extracted helpers,
  moved private functions. If a criterion still reads true, nothing moves.
- Test changes. Tests are not manual material.
- Dependency bumps, formatting, comments that are not the contract.

## Checking your own work before `finish`

1. `lint`. Every finding this work created is yours to fix.
2. `get` each node in the ticket's `touches`. Does its record reflect the
   change, or only the code?
3. For each new or changed criterion: is there a requirement naming it, and
   a page linking that requirement? `coverage` on the module answers this
   in one call.
4. Did you make a choice? Is it recorded on a requirement, or only in your
   head and the ticket body, which is discarded on close?
