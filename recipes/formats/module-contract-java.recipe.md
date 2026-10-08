# Write a module contract in Java

How do I say what a Java class or package promises? Put a `# Module contract` heading and one line of JSON in a top-level Javadoc: a purpose and the criteria a test or a reviewer can check.

```java
package com.example.core;

import com.example.core.util.Strings;

/**
 * # Module contract
 *
 * ```json
 * {"purpose":"Greet people by name.","criteria":[{"id":"greets","statement":"A greeting names the person greeted."}]}
 * ```
 */
public class Greeter { /* ... */ }
```

**Where it goes.** A top-level `/** */` Javadoc, outside every type's body. The first Javadoc holding the heading is the one read, so the class's own Javadoc after the package and imports carries it, and a `/* */` licence header above it is never taken for it. A package's contract goes in the Javadoc of its `package-info.java`.

**Name the language.** Add `java` to the store config's languages with `configure`.

**Which files need one.** Every `.java` file under a `src/main/java` folder, at any depth. Not `package-info.java` or `module-info.java`, though either may carry one, not anything under `src/test`, and not files elsewhere.

**What the module is called.** Its path from the repository root without `.java`: `core/src/main/java/com/example/core/Greeter.java` is `core/src/main/java/com/example/core/Greeter`. A `package-info.java` stands for its folder, the package. Imports are resolved through the packages your own files declare, and a class of its own package that a class names needs no import to count, so the graph shows which classes a class rests on.

**The JSON.** Strict: an unknown field, a missing purpose, an empty `criteria` or an id used twice makes the contract malformed, and the check reports the module. Keep it on one line.

**One behaviour each.** A criterion states one thing a test could check. One that says what each of several cases does is split into a criterion for each, so a test names exactly the criteria it checks and a behaviour no test checks is reported as untested.

**When not to.** A rule about how the parts are arranged is a constraint, a requirement held by a test, not one class's criterion.

To say which criteria a JUnit test checks, see the test contract recipe.

## Recipe

```json
{"id":"module-contract-java","kind":"format","ingredients":[{"id":"heading","says":"`# Module contract` in the first top-level Javadoc holding it, followed by a fenced json block."},{"id":"purpose","says":"One sentence: what the class or package is for, not what it contains."},{"id":"criteria","says":"At least one; each an id, a slug unique in the module, and a statement, one checkable sentence in the present tense stating one behaviour."},{"id":"module-id","says":"The path from the repository root without .java; package-info.java stands for its package's folder."},{"id":"coverage","says":"Every .java under a src/main/java folder, but package-info.java and module-info.java, which may carry one."}],"files":["crates/extract/src/java.rs","crates/extract/src/contract.rs",".claude/skills/grimoire/references/contracts.md"],"uses":["test-contract"]}
```
