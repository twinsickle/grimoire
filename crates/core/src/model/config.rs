//! # Module contract
//!
//! ```json
//! {"purpose":"Define the store config: the repository it describes, its two roots, the languages to extract, the contract scope, the board's order, the site, the line limits and the project's recipe kinds.","criteria":[{"id":"config-fields","statement":"The store config is version, the repository's absolute path, its two roots, the languages to extract, an optional contract scope of include and exclude prefixes, and the page slugs its site shows."},{"id":"config-line-limits","statement":"The store config holds optional large-file and plain-file line limits, 1,000 and 5,000 when absent."},{"id":"config-recipe-kinds","statement":"The store config holds the project's recipe kinds, each with the kinds it may use."},{"id":"config-unknown-version","statement":"A store config version this build does not know is refused."},{"id":"roots-pair","statement":"A config's roots are the pair `{grimoire, rites}`, either of which may be missing."},{"id":"roots-distinct","statement":"A config's two roots are never the same page."},{"id":"roots-read-list","statement":"Roots held as a list of slugs are read with the first as the grimoire and the second as the rites, any further one dropped."},{"id":"roots-written-pair","statement":"Roots are written as the pair, whatever they were read as."},{"id":"roots-list","statement":"list gives the named roots, the grimoire first."},{"id":"roots-rename","statement":"rename renames each named root that matches."}]}
//! ```

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::Priority;

/// A store's two roots: the grimoire, the root page of what the project is and must do, whose
/// requirements the code carries, and the rites, the root page of how its people work on it. Either may be missing, which
/// the check reports; a config holding a list of slugs, as stores did before, reads its first as the
/// grimoire and its second as the rites.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Roots {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub grimoire: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub rites: Option<String>,
}

impl Roots {
    pub fn new(grimoire: &str, rites: &str) -> Roots {
        Roots { grimoire: Some(grimoire.to_string()), rites: Some(rites.to_string()) }
    }

    /// The roots that are named, the grimoire first.
    pub fn list(&self) -> Vec<&String> {
        self.grimoire.iter().chain(self.rites.iter()).collect()
    }

    pub fn contains(&self, slug: &str) -> bool {
        self.list().iter().any(|r| *r == slug)
    }

    /// Each named root that is `from`, renamed `to`; whether any was.
    pub fn rename(&mut self, from: &str, to: &str) -> bool {
        let mut hit = false;
        for r in [&mut self.grimoire, &mut self.rites].into_iter().flatten() {
            if r == from {
                *r = to.to_string();
                hit = true;
            }
        }
        hit
    }
}

/// The roots as an older build reads them, a list, when both are named, and as the pair only when
/// one is missing, which an older build could not have held: for what a server hands a machine.
pub fn roots_as_older_builds_read<S: Serializer>(roots: &Roots, s: S) -> Result<S::Ok, S::Error> {
    match (&roots.grimoire, &roots.rites) {
        (Some(g), Some(r)) => [g, r].serialize(s),
        _ => roots.serialize(s),
    }
}

/// A list of slugs, as stores held before the two roots: the first is the grimoire, the second the rites.
impl From<Vec<String>> for Roots {
    fn from(list: Vec<String>) -> Roots {
        let mut it = list.into_iter();
        Roots { grimoire: it.next(), rites: it.next() }
    }
}

impl<'de> Deserialize<'de> for Roots {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Roots, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Pair {
            #[serde(default)]
            grimoire: Option<String>,
            #[serde(default)]
            rites: Option<String>,
        }
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Shape {
            List(Vec<String>),
            Pair(Pair),
        }
        Ok(match Shape::deserialize(d).map_err(|_| serde::de::Error::custom("roots is `{grimoire, rites}`, each a page slug, or a list of slugs as older stores held"))? {
            Shape::List(list) => list.into(),
            Shape::Pair(p) => Roots { grimoire: p.grimoire, rites: p.rites },
        })
    }
}

/// `grimoire.json` at the root of a store.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoreConfig {
    pub version: u32,
    pub repository: String,
    pub roots: Roots,
    pub languages: Vec<String>,
    #[serde(default, skip_serializing_if = "Scope::is_empty")]
    pub scope: Scope,
    #[serde(default, skip_serializing_if = "Order::is_empty")]
    pub order: Order,
    // The pages the project's site shows, as page slugs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "ts", ts(as = "Option<Vec<String>>", optional))]
    pub site: Vec<String>,
    // A file of this many lines or more, its tests aside, is an omen; DEFAULT_LARGE_FILE when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub large_file: Option<u32>,
    // A file of more lines than this is shown plain, unhighlighted; DEFAULT_PLAIN_FILE when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub plain_file: Option<u32>,
    // The project's own kinds of recipe, each with the kinds its recipes may use (req:recipe-kinds).
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    #[cfg_attr(feature = "ts", ts(as = "Option<std::collections::BTreeMap<String, RecipeKind>>", optional))]
    pub recipe_kinds: std::collections::BTreeMap<String, RecipeKind>,
}

/// A kind of recipe the project declares: the kinds its recipes may use.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecipeKind {
    #[serde(default)]
    pub uses: Vec<String>,
}

/// The order a person set within each priority band, as ticket slugs. A
/// ticket that has closed or changed band is ignored where it is read.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Order {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub next: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub high: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub later: Vec<String>,
}

impl Order {
    pub fn is_empty(&self) -> bool {
        self.next.is_empty() && self.high.is_empty() && self.later.is_empty()
    }

    pub fn band(&self, p: Priority) -> &[String] {
        match p {
            Priority::Next => &self.next,
            Priority::High => &self.high,
            Priority::Later => &self.later,
        }
    }

    pub fn band_mut(&mut self, p: Priority) -> &mut Vec<String> {
        match p {
            Priority::Next => &mut self.next,
            Priority::High => &mut self.high,
            Priority::Later => &mut self.later,
        }
    }
}

/// Which paths are expected to carry contracts: repository-relative,
/// `/`-separated prefixes matched on whole segments. An empty `include`
/// means everything.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scope {
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
}

impl Scope {
    pub fn is_empty(&self) -> bool {
        self.include.is_empty() && self.exclude.is_empty()
    }

    /// Whether a module with these files is in scope: some file under an
    /// include (or no include at all) and none under an exclude.
    pub fn covers(&self, files: &[String]) -> bool {
        let under = |f: &str, p: &str| f == p || f.strip_prefix(p).is_some_and(|rest| rest.starts_with('/'));
        let included = self.include.is_empty() || files.iter().any(|f| self.include.iter().any(|p| under(f, p)));
        let excluded = files.iter().any(|f| self.exclude.iter().any(|p| under(f, p)));
        included && !excluded
    }
}

impl StoreConfig {
    /// Version 2 holds only trimmed strings. A version 1 store, from before
    /// the write tools trimmed, is read with its strings trimmed and is
    /// rewritten as version 2 by the first writer to take its lock.
    pub const CURRENT_VERSION: u32 = 2;
    pub const TRIMMED_ON_READ: u32 = 1;
    /// A file of this many lines or more, its tests aside, is an omen when the config names no limit.
    pub const DEFAULT_LARGE_FILE: u32 = 1_000;
    /// A file of more lines than this is shown plain when the config names no limit.
    pub const DEFAULT_PLAIN_FILE: u32 = 5_000;

    /// The large-file limit in force: the config's, or the default.
    pub fn large_file(&self) -> u32 {
        self.large_file.unwrap_or(Self::DEFAULT_LARGE_FILE)
    }

    /// The plain-file limit in force: the config's, or the default.
    pub fn plain_file(&self) -> u32 {
        self.plain_file.unwrap_or(Self::DEFAULT_PLAIN_FILE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-config-names-two-roots", "checks": ["crates/core/src/model/config#roots-pair", "crates/core/src/model/config#roots-read-list", "crates/core/src/model/config#roots-written-pair", "crates/core/src/model/config#roots-list", "crates/core/src/model/config#roots-rename"]}
    /// ```
    #[test]
    fn a_config_names_two_roots() {
        let read = |json: &str| serde_json::from_str::<Roots>(json).unwrap();
        // The pair as written now, either half missing.
        assert_eq!(read(r#"{"grimoire":"overview","rites":"rites"}"#), Roots::new("overview", "rites"));
        assert_eq!(read(r#"{"grimoire":"overview"}"#), Roots { grimoire: Some("overview".into()), rites: None });
        // A list, as older stores held: the first the grimoire, the second the rites, any further one dropped.
        assert_eq!(read(r#"["overview","handbook","extra"]"#), Roots::new("overview", "handbook"));
        assert_eq!(read(r#"["overview"]"#), Roots { grimoire: Some("overview".into()), rites: None });
        assert!(serde_json::from_str::<Roots>(r#"{"grimoire":"a","other":"b"}"#).is_err(), "nothing else is a root");
        // Written as the pair, whatever it was read as.
        assert_eq!(serde_json::to_string(&read(r#"["overview","handbook"]"#)).unwrap(), r#"{"grimoire":"overview","rites":"handbook"}"#);
        // The named roots, the grimoire first; a rename moves whichever matches.
        let mut r = Roots::new("overview", "rites");
        assert_eq!(r.list(), [&"overview".to_string(), &"rites".to_string()]);
        assert!(r.rename("rites", "craft") && !r.rename("nowhere", "x"));
        assert_eq!(r, Roots::new("overview", "craft"));
        assert!(r.contains("craft") && !r.contains("rites"));
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-config-never-names-one-page-as-both-roots", "checks": ["crates/core/src/model/config#roots-distinct"]}
    /// ```
    #[test]
    fn a_config_never_names_one_page_as_both_roots() {
        let config = |roots: Roots| StoreConfig {
            site: vec![],
            version: StoreConfig::CURRENT_VERSION,
            repository: "C:/repo".into(),
            roots,
            languages: vec![],
            scope: Scope::default(),
            order: Default::default(),
            large_file: None,
            plain_file: None,
            recipe_kinds: Default::default(),
        };
        let refused = crate::store::validate_config(&config(Roots::new("overview", "overview")));
        assert!(refused.iter().any(|e| e.field == "roots" && e.reason.contains("two pages")), "{refused:?}");
        // Read as the older list too: the same slug twice is the same page.
        let twice: Roots = vec!["overview".to_string(), "overview".to_string()].into();
        assert!(!crate::store::validate_config(&config(twice)).is_empty());
        assert!(crate::store::validate_config(&config(Roots::new("overview", "rites"))).is_empty(), "two pages are accepted");
    }

    #[test]
    fn scope_matches_whole_segments() {
        let s = |inc: &[&str], exc: &[&str]| Scope { include: inc.iter().map(|x| x.to_string()).collect(), exclude: exc.iter().map(|x| x.to_string()).collect() };
        let files = |fs: &[&str]| fs.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert!(Scope::default().covers(&files(&["anything/at/all.rs"])), "no scope is everything");
        let core = s(&["crates/core"], &[]);
        assert!(core.covers(&files(&["crates/core/src/lib.rs"])));
        assert!(!core.covers(&files(&["crates/core-extra/src/lib.rs"])), "whole segments only");
        assert!(!core.covers(&files(&["crates/cli/src/main.rs"])));
        let carved = s(&["crates/core"], &["crates/core/src/generated"]);
        assert!(!carved.covers(&files(&["crates/core/src/generated/parser.rs"])));
        assert!(carved.covers(&files(&["crates/core/src/lint.rs"])));
        assert!(!s(&[], &["vendor"]).covers(&files(&["vendor/x.rs"])), "an exclude alone narrows everything");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-config-without-a-scope-loads", "checks": ["crates/core/src/model/config#config-fields", "crates/core/src/model/config#config-recipe-kinds"]}
    /// ```
    #[test]
    fn a_config_without_a_scope_loads() {
        let c: StoreConfig = serde_json::from_str(r#"{"version":1,"repository":"C:/repo","roots":["overview"],"languages":["rust"]}"#).unwrap();
        assert!(c.scope.is_empty());
        assert!(!serde_json::to_string(&c).unwrap().contains("scope"), "an empty scope is not written");
        assert!(c.recipe_kinds.is_empty() && !serde_json::to_string(&c).unwrap().contains("recipe_kinds"), "no recipe kinds, none written");
        let kinds: StoreConfig =
            serde_json::from_str(r#"{"version":2,"repository":"C:/repo","roots":["overview"],"languages":[],"recipe_kinds":{"component":{"uses":["component","token"]},"token":{}}}"#).unwrap();
        assert_eq!(kinds.recipe_kinds["component"].uses, ["component", "token"]);
        assert!(kinds.recipe_kinds["token"].uses.is_empty());
        assert!(serde_json::from_str::<RecipeKind>(r#"{"uses":[],"colour":"red"}"#).is_err(), "a kind holds its uses and nothing else");
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "line-limits-are-optional-with-defaults", "checks": ["crates/core/src/model/config#config-line-limits"]}
    /// ```
    #[test]
    fn line_limits_are_optional_with_defaults() {
        let c: StoreConfig = serde_json::from_str(r#"{"version":2,"repository":"C:/repo","roots":["overview"],"languages":["rust"]}"#).unwrap();
        assert_eq!((c.large_file(), c.plain_file()), (1_000, 5_000));
        assert!(!serde_json::to_string(&c).unwrap().contains("_file"), "an absent limit is not written");
        let c: StoreConfig = serde_json::from_str(r#"{"version":2,"repository":"C:/repo","roots":["overview"],"languages":["rust"],"large_file":600}"#).unwrap();
        assert_eq!((c.large_file(), c.plain_file()), (600, 5_000));
    }
}
