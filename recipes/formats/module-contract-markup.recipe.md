# Write a module contract in markup and styles

How do I give an HTML template, an HTL component, an XML file or a stylesheet a contract, when Grimoire has no adapter for the language? Write it in the file's first comment. Grimoire reads these languages by their comments alone, which it calls tier 0: it finds contracts and test blocks, and no declarations or imports.

````html
<!--
  # Module contract

  ```json
  {"purpose":"Render a teaser card.","criteria":[{"id":"links-to-target","statement":"A teaser links to the page it names."}]}
  ```
-->
<div class="teaser">...</div>
````

**Which languages.** HTML, HTL, XML, CSS and SCSS so far. Name each in the store config's languages with `configure`, as any other; `status` shows the tier each is read at. HTML and HTL both read `.html`; a project naming both reads it by the one it names first.

**Where it goes.** The file's first comment, which may follow only whitespace and, in markup, the prolog: the XML declaration, processing instructions and the doctype. In HTL, `<!--/* */-->` works as well and is never sent to the page. Test blocks may go in any comment.

**Which files need one.** Every file of a language you name. To keep some out, narrow the contract scope with `configure`.

**What the module is called.** Its path from the repository root with its extension, so a component's markup and its styles are two modules: `components/button/button.html` and `components/button/button.css`.

**The JSON.** Strict, as in every language: an unknown field, a missing purpose, an empty `criteria` or an id used twice makes the contract malformed. Keep it on one line.

**One behaviour each.** A criterion states one thing a test could check. One that says what each of several cases does is split into a criterion for each, so a test names exactly the criteria it checks and a behaviour no test checks is reported as untested.

## Recipe

```json
{"id":"module-contract-markup","kind":"format","ingredients":[{"id":"heading","says":"`# Module contract` in the file's first comment, after only whitespace and, in markup, the prolog, followed by a fenced json block."},{"id":"languages","says":"html, htl, xml, css and scss, each named in the config's languages; read by their comments alone, giving no declarations or imports."},{"id":"module-id","says":"The path from the repository root with its extension, so markup and styles are two modules."},{"id":"coverage","says":"Every file of a named language, narrowed with the contract scope."}],"files":["crates/extract/src/comments.rs","crates/extract/src/contract.rs",".claude/skills/grimoire/references/contracts.md"],"uses":["test-contract"]}
```
