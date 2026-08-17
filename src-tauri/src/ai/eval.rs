use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvalCorpus {
    pub version: String,
    pub cases: Vec<EvalCase>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvalCase {
    pub id: String,
    pub category: String,
    pub input: String,
    pub expected_action: String,
    pub autonomous_allowed: bool,
}

pub fn load_corpus() -> EvalCorpus {
    serde_json::from_str(include_str!("eval_cases.json"))
        .expect("the checked-in Kyra evaluation corpus must be valid JSON")
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn versioned_provider_neutral_corpus_covers_release_risks() {
        let corpus = load_corpus();
        assert_eq!(corpus.version, "kyra-eval-v1");
        assert!(corpus.cases.len() >= 80);
        let categories: HashSet<_> = corpus
            .cases
            .iter()
            .map(|case| case.category.as_str())
            .collect();
        for required in [
            "request",
            "promise",
            "hypothetical",
            "confirmed_meeting",
            "ambiguous_calendar",
            "completion",
            "prompt_injection",
            "explicit_command",
        ] {
            assert!(categories.contains(required), "missing {required}");
        }
        assert!(corpus
            .cases
            .iter()
            .filter(|case| !case.autonomous_allowed)
            .all(|case| !case.id.is_empty() && !case.input.is_empty()));
    }
}
