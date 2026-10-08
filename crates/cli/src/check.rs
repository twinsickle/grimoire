//! # Module contract
//!
//! ```json
//! {"purpose":"Judge a checkout's code against its grimoire for a hook or a pipeline, and bottle the ticket a change carried: check and bottle, which share how a store is judged and its baseline pruned.","criteria":[{"id":"check-without-daemon","statement":"The check command runs the lints without a daemon, for hooks and CI, over the store as the checkout it runs in sees it, exits non-zero only when there are findings outside the baseline, and prunes the baseline itself when no daemon serves the store, but never on a project that names a server, nor from a checkout of a store of its own other than its main one; a command run in another checkout of a store of its own adds it to the store's checkouts when it is not one yet."},{"id":"check-own-work","statement":"check, with neither --ticket nor --main-line, gives the state the code of the checkout's base, the merge base of HEAD with the main line, when there is one, so it fails only on findings outside the baseline that its own work causes; beside each finding it prints every change still waiting to be bottled from this checkout that the lint answer's mended says would mend it, the finding still failing; it prints after the findings each advisory, what other work causes, under the work it comes from, the main line since the base or a ticket by id and title, saying they do not fail the check, and under --json answers them as `advisories`; with --ticket or --main-line it takes no base."},{"id":"check-names-a-ticket","statement":"check with --ticket judges the code against the truth plus that ticket's proposal in place of the current ticket's, a slug or a ticket: reference, and fails with the reason when the ticket does not exist or is neither brewing nor simmering, or, on a project that names a server, when the server's intent cannot be read as it is now; without it, against the current ticket's proposal, or the truth when there is no current ticket."},{"id":"check-main-line","statement":"check with --main-line judges the code against the truth whatever the current ticket, and is refused beside --ticket; on a project that names a server it prunes the server's baseline with prune_from_main_line, which reads the server's intent as it is now, never the checkout's copy, then sends the server the recipes that code reads with send_recipes, and prints the findings of the intent and code that prune judged, as check does, says how many entries went and how many recipes the server holds, on standard error under --json, or as a warning on standard error why the recipes were not taken; it exits non-zero when there are new findings or the prune failed, never because the recipes were not taken; on a store of the project's own it does what check does."},{"id":"unsupported-first","statement":"check prints a finding that the build cannot extract a configured language before every other line, baselined or not, so the reason an old binary fails is the first thing read."},{"id":"check-omens","statement":"check prints each omen after the findings and before the advisories, then a line counting them and saying they do not fail the check, exits non-zero for none of them, and under --json answers them as omens."},{"id":"bottle","statement":"bottle, and land, its name before it was bottle, which no help lists, runs bottle_ticket for the ticket it is given, or with none for the current ticket when that one is simmering; it prints what was bottled or that no ticket was simmering, or on a project that names a server and with no ticket given that a ticket is bottled on the main line, names the tickets still simmering; on a project that names a server, once a ticket it named has been bottled, it prunes the server's baseline from the checkout's code against the truth as it is now, with prune_from_main_line, and says how many entries went, then sends the server the recipes that code reads with send_recipes, whatever the prune came to, saying how many the server holds or, as a warning on standard error, why they were not taken; it exits non-zero when a bottling was refused, when a bottling taken on a team could not then prune, saying the ticket was bottled, printing the records that moved, or when the store cannot be reached, never because no ticket was simmering."}]}
//! ```

use anyhow::{bail, Context};
use grimoire_core::lint::{self, Rule};
use grimoire_server::store_io;
use serde_json::{json, Value};

use crate::{checkout_view, run_tool, team, Target};

/// Run the lints for a hook or a pipeline and print what they found; exits 1 when the check fails.
pub(crate) fn check(target: Target, rules: Vec<String>, json: bool, ticket: Option<String>, main_line: bool) -> anyhow::Result<()> {
    let store_dir = target.store_dir.clone();
    // On a team, what the main line's prune came to, and its sending of the recipes.
    let mut pruned = None;
    let mut sent = None;
    let mut loaded = match &target.team {
        // The main line reads the server's intent into memory, never the copy a daemon beside it may not have
        // synced, prunes from it, and reports on that same intent, the one the prune was judged against.
        Some(team) if main_line => {
            let server = team.server()?;
            let mut line = team::prune_from_main_line(team, &server)?;
            for e in &line.loaded.extract_errors {
                eprintln!("warning: {e}");
            }
            // Whatever the prune came to: the recipes are read whatever the languages, and the digest the prune names leaves them out.
            sent = Some(team::send_recipes(team, &server, &mut line.loaded));
            pruned = Some(line.pruned);
            line.loaded
        }
        // The server's intent, synced into this checkout's copy when the server answers, with this checkout's code.
        // Nothing is pruned: a finding fixed on a branch is not fixed on the main line.
        Some(team) => {
            // A named ticket is a pipeline's gate: only the server's intent as it is now will do.
            let opened = team::open(team, &team.server()?, ticket.is_some())?;
            for e in &opened.loaded.extract_errors {
                eprintln!("warning: {e}");
            }
            opened.loaded
        }
        None => {
            // The baseline is pruned here only by the store's writer: with the lock
            // taken before loading, so what is pruned is what was read under it. A
            // daemon serving holds the lock and prunes on its own rebuilds; another
            // writer may be mid-change; either way the check is a read, and reads.
            let lock = store_io::lock(&store_dir).ok();
            // Read as the checkout it runs in; only the store's main checkout prunes its baseline.
            let view = checkout_view(&target, lock.as_ref())?;
            let mut loaded = grimoire_server::load_for(&store_dir, view.as_deref())?;
            for e in &loaded.extract_errors {
                eprintln!("warning: {e}");
            }
            if lock.is_some() && view.is_none() {
                if let Some(changed) = loaded.state.prune_baseline() {
                    store_io::persist(&store_dir, loaded.state.store(), &changed)?;
                }
            }
            loaded
        }
    };
    // A branch is judged by its own work: what reached the main line since it was cut is shown,
    // not failed (req:check-own-work). A pipeline's gates on the main line take no base.
    if ticket.is_none() && !main_line {
        if let Err(e) = grimoire_server::base::give(&mut loaded.state, &loaded.repo) {
            eprintln!("warning: {e}");
        }
    }
    let adopted = loaded.state.store().baseline.is_some();
    let rules: Vec<Rule> = if rules.is_empty() {
        Rule::ALL.to_vec()
    } else {
        rules.iter().map(|r| serde_json::from_value(serde_json::Value::String(r.clone())).with_context(|| format!("unknown rule `{r}`"))).collect::<anyhow::Result<_>>()?
    };
    // On a branch the current ticket is the branch; its proposal is the intent the code is judged against.
    // A pipeline has no current ticket and names the one it checks, as it names the one it bottles.
    // The main line is judged against the truth, whatever ticket this checkout is working.
    let judged = match &ticket {
        Some(t) => Some(t.trim().strip_prefix("ticket:").unwrap_or(t.trim()).to_string()),
        None if main_line => None,
        None => loaded.state.current(),
    };
    let answer = match loaded.state.lint(judged.as_deref(), &rules) {
        Ok(a) => a,
        Err(e) => bail!("cannot check against ticket:{}: {}", judged.as_deref().unwrap_or(""), e),
    };
    let (findings, new) = (&answer.findings, answer.new);
    if json {
        println!("{}", serde_json::to_string_pretty(&answer)?);
    } else {
        // A build that cannot read part of the project explains the rest,
        // so it comes first. Then the new findings; inherited ones are
        // counted, not listed: the list is the work this commit is
        // answerable for.
        let unsupported = |f: &&lint::Finding| f.check == "unsupported-language";
        for f in findings.iter().filter(unsupported) {
            println!("{:<11} {}  {}", f.rule.name(), f.r#ref, f.reason);
        }
        for f in findings.iter().filter(|f| !f.baselined && !unsupported(f)) {
            println!("{:<11} {}  {}", f.rule.name(), f.r#ref, f.reason);
            // A pipeline's gates judge one named intent: what this checkout still carries is a branch's concern.
            if ticket.is_none() && !main_line {
                print_mended(&answer.mended, f);
            }
        }
        let counts = grimoire_core::lint::counts(findings);
        let mut line = format!("{} finding(s): {}", findings.len(), counts.iter().map(|(k, v)| format!("{v} {k}")).collect::<Vec<_>>().join(", "));
        if adopted {
            line.push_str(&format!("; {} baselined, {new} new", findings.len() - new));
        }
        println!("{line}");
        print_omens(&answer.omens);
        print_advisories(&answer.advisories);
    }
    // What the main line told a team's server; a store of the project's own was pruned above.
    let mut failed = new > 0;
    if let Some(pruned) = pruned {
        match pruned {
            // Kept off standard output under --json, which is the findings alone.
            Ok(went) if json => eprintln!("pruned {went} baselined finding(s) the main line no longer produces"),
            Ok(went) => println!("pruned {went} baselined finding(s) the main line no longer produces"),
            Err(e) => {
                eprintln!("{e}");
                failed = true;
            }
        }
    }
    if let Some(sent) = sent {
        say_sent(sent, json);
    }
    if failed {
        std::process::exit(1)
    }
    Ok(())
}

/// What sending the main line's recipes came to: said, never failing, since the site's catalog is a
/// published snapshot the next bottling or main-line check sends again (req:recipe-main-line-snapshot).
fn say_sent(sent: anyhow::Result<u64>, json: bool) {
    match sent {
        Ok(held) if json => eprintln!("sent the server the main line's recipes: it holds {held}"),
        Ok(held) => println!("sent the server the main line's recipes: it holds {held}"),
        Err(e) => eprintln!("warning: {e}; the team's site shows the recipes it last took"),
    }
}

/// Signs that something may be wrong, after the findings: never failing (req:lint-omens).
fn print_omens(omens: &[grimoire_core::lint::Omen]) {
    if omens.is_empty() {
        return;
    }
    for o in omens {
        println!("{:<11} {}  {}", "omen", o.r#ref, o.reason);
    }
    println!("{} omen(s); they do not fail the check", omens.len());
}

/// Beside a finding, each change this checkout still carries that would mend it: it still fails,
/// since a branch holds one ticket until it is bottled (req:check-own-work).
fn print_mended(mended: &[grimoire_core::advisories::Mended], f: &lint::Finding) {
    for m in mended.iter().filter(|m| m.finding == f.key()) {
        println!("            ticket:{} ({}) would mend this: it is still simmering from this checkout; bottle it first", m.ticket, m.title);
    }
}

/// What other work causes, under the work it comes from: shown after the findings, never failing.
fn print_advisories(advisories: &[grimoire_core::advisories::Advisory]) {
    use grimoire_core::advisories::OtherWork;
    if advisories.is_empty() {
        return;
    }
    let mut from: Vec<&OtherWork> = Vec::new();
    for a in advisories {
        if !from.contains(&&a.from) {
            from.push(&a.from);
        }
    }
    for work in from {
        match work {
            OtherWork::SinceBase => println!("may conflict, reached the main line since this branch's base (merge the main line):"),
            OtherWork::Ticket { ticket, title } => println!("may conflict, ticket:{ticket} ({title}) would bring when it is bottled:"),
        }
        for f in advisories.iter().filter(|a| &a.from == work).map(|a| &a.finding) {
            println!("  {:<11} {}  {}", f.rule.name(), f.r#ref, f.reason);
        }
    }
    println!("{} advisory(ies) from other work; they do not fail the check", advisories.len());
}

/// Bottle a ticket and say what happened, for the post-commit hook and for a
/// pipeline. A commit with no ticket simmering on it is ordinary and exits 0;
/// a bottling that was refused exits 1, and so does a store that cannot be
/// reached, which a pipeline must hear of and a post-commit hook cannot act on.
pub(crate) fn bottle(target: &Target, ticket: Option<&str>) -> anyhow::Result<()> {
    let input = match ticket {
        Some(t) => json!({ "ref": format!("ticket:{}", t.strip_prefix("ticket:").unwrap_or(t)) }),
        None => json!({}),
    };
    match run_tool(target, "bottle_ticket", input, None)? {
        Ok(out) => {
            if let Some(note) = out["note"].as_str() {
                // On a team a commit on a machine is not the main line: nothing is bottled here unnamed.
                println!("nothing bottled: {note}");
            } else if out["bottled"].as_bool() == Some(true) {
                println!("bottled {}: {}", out["node"]["ref"].as_str().unwrap_or("the ticket"), out["node"]["title"].as_str().unwrap_or(""));
                let new = out["findings"].as_array().map_or(0, Vec::len);
                if new > 0 {
                    println!("{new} new finding(s) on what it touched; run `grimoire check`");
                }
                // What a close answers with is seen here now: the nodes the ticket named and never changed.
                let untouched: Vec<&str> = out["untouched"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
                if !untouched.is_empty() {
                    println!("touched but never changed: {}", untouched.join(", "));
                }
                // On a team a ticket is bottled by name on the main line, which then tells the server what is fixed,
                // against the truth with the ticket applied. A store of the project's own was pruned by the write.
                if let Some(team) = &target.team {
                    let (server, mut line) = match team.server().and_then(|server| team::prune_from_main_line(team, &server).map(|line| (server, line))) {
                        Ok(read) => read,
                        Err(e) => {
                            eprintln!("the ticket was bottled, but {e}");
                            std::process::exit(1)
                        }
                    };
                    // After the prune, whatever it came to: the recipes are read whatever the languages.
                    let sent = team::send_recipes(team, &server, &mut line.loaded);
                    let failed = match line.pruned {
                        Ok(went) => {
                            println!("pruned {went} baselined finding(s) the main line no longer produces");
                            false
                        }
                        Err(e) => {
                            eprintln!("the ticket was bottled, but {e}");
                            true
                        }
                    };
                    say_sent(sent, false);
                    if failed {
                        std::process::exit(1)
                    }
                }
            } else {
                println!("no ticket was simmering");
            }
            let waiting: Vec<String> = out["waiting"].as_array().into_iter().flatten().map(|w| format!("{} ({})", w["ref"].as_str().unwrap_or("?"), w["title"].as_str().unwrap_or(""))).collect();
            if !waiting.is_empty() {
                println!("still simmering: {}; bottle one with `grimoire bottle <ticket>`", waiting.join(", "));
            }
            Ok(())
        }
        Err(e) => {
            eprintln!("bottling refused: {}", e["message"].as_str().unwrap_or("unknown error"));
            for d in e["details"].as_array().into_iter().flatten() {
                eprintln!("  {}: {}", d["field"].as_str().unwrap_or("?"), d["reason"].as_str().unwrap_or(""));
            }
            std::process::exit(1)
        }
    }
}
