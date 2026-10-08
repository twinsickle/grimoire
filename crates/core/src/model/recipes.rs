//! # Module contract
//!
//! ```json
//! {"purpose":"Hold what is recorded of recipes: each judgement of a recipe against what it represents, and the recipes the main line last sent a team's server.","criteria":[{"id":"judgement-record","statement":"A judgement is exactly a recipe's id, the fingerprint it was judged at, an ISO date, the ticket it was judged in and its outcome, held or updated; a recipe read from the repository carries its digest, the fingerprint a judgement is matched against."},{"id":"main-line-recipes-record","statement":"The main line's recipes are exactly a list of recipes, each an id, kind, title, text, its ingredients each an id and what it says, the ids of the recipes it uses and its digest, and nothing of its files, its file or its line; one is made from a recipe read from the repository, and read back as that recipe with no files, no file and no line."}]}
//! ```

use serde::{Deserialize, Serialize};

use super::{Ingredient, Recipe};

/// What judging a recipe found: it still says truly how to use what it
/// represents, or it was updated to say so.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JudgementOutcome {
    Held,
    Updated,
}

/// One judgement of a recipe against its files (req:store-judgement-record): a
/// person's or an agent's act, so dated, and bound to the state it judged by the
/// fingerprint of the recipe and its files then.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Judgement {
    pub recipe: String,
    pub fingerprint: String,
    pub date: String,
    pub ticket: String,
    pub outcome: JudgementOutcome,
}

/// `judgements/<recipe>.json`: one recipe's judgements, oldest first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Judgements {
    pub recipe: String,
    pub judgements: Vec<Judgement>,
}

/// `recipes.json`: the recipes the main line last sent a team's server, which holds no code to read
/// them, so its site can show them (req:recipe-main-line-snapshot). A published snapshot, replaced whole
/// by each send: never the source of truth, and nothing of the recipes' files.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MainLineRecipes {
    pub recipes: Vec<SentRecipe>,
}

/// One recipe as the main line sends it: what a reader of the site is shown, and the digest a judgement is matched against.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SentRecipe {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub text: String,
    pub ingredients: Vec<SentIngredient>,
    pub uses: Vec<String>,
    pub digest: String,
}

/// An ingredient of a sent recipe: its id and what it says.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SentIngredient {
    pub id: String,
    pub says: String,
}

impl From<&Recipe> for SentRecipe {
    fn from(r: &Recipe) -> Self {
        SentRecipe {
            id: r.id.clone(),
            kind: r.kind.clone(),
            title: r.title.clone(),
            text: r.text.clone(),
            ingredients: r.ingredients.iter().map(|i| SentIngredient { id: i.id.clone(), says: i.says.clone() }).collect(),
            uses: r.uses.clone(),
            digest: r.digest.clone(),
        }
    }
}

impl SentRecipe {
    /// The recipe as one read from the repository would be, but with nothing of its files, which were never sent.
    pub fn as_recipe(&self) -> Recipe {
        Recipe {
            id: self.id.clone(),
            kind: self.kind.clone(),
            title: self.title.clone(),
            text: self.text.clone(),
            ingredients: self.ingredients.iter().map(|i| Ingredient { recipe: self.id.clone(), id: i.id.clone(), says: i.says.clone() }).collect(),
            files: Vec::new(),
            uses: self.uses.clone(),
            file: String::new(),
            line: 0,
            missing: Vec::new(),
            digest: self.digest.clone(),
        }
    }
}
