//! Filter evaluation engine

use crate::error::Result;
use crate::filter::{Filter, FilterRule};
use crate::protocol::Identity;

/// Evaluator for a set of filter rules
#[derive(Debug, Clone, Default)]
pub struct FilterEvaluator {
    /// Rules to evaluate (ANDed together)
    rules: Vec<FilterRule>,
}

impl FilterEvaluator {
    /// Create a new filter evaluator from rules
    pub fn new(rules: Vec<FilterRule>) -> Self {
        Self { rules }
    }

    /// Parse filter strings into an evaluator
    pub fn parse(filter_strs: &[String]) -> Result<Self> {
        let rules = filter_strs
            .iter()
            .map(|s| FilterRule::parse(s))
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { rules })
    }

    /// Check if all rules match the given identity (AND logic)
    pub fn matches(&self, identity: &Identity) -> bool {
        // Empty rules = match all
        if self.rules.is_empty() {
            return true;
        }
        self.rules.iter().all(|r| r.matches(identity))
    }

    /// Filter a list of identities
    pub fn filter_identities(&self, identities: Vec<Identity>) -> Vec<Identity> {
        identities.into_iter().filter(|i| self.matches(i)).collect()
    }

    /// Get the number of rules
    pub fn len(&self) -> usize {
        self.rules.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// Get rules for inspection
    pub fn rules(&self) -> &[FilterRule] {
        &self.rules
    }

    /// Ensure all async filters are loaded (GitHub keys, etc.)
    pub async fn ensure_loaded(&self) -> Result<()> {
        for rule in &self.rules {
            match &rule.filter {
                Filter::GitHub(m) => m.ensure_loaded().await?,
                Filter::Keyfile(m) => m.reload()?,
                _ => {}
            }
        }
        Ok(())
    }

    /// Reload all reloadable filters
    pub async fn reload(&self) -> Result<()> {
        for rule in &self.rules {
            match &rule.filter {
                Filter::GitHub(m) => m.fetch_keys().await?,
                Filter::Keyfile(m) => m.reload()?,
                _ => {}
            }
        }
        Ok(())
    }

    /// Get descriptions of all rules
    pub fn descriptions(&self) -> Vec<String> {
        self.rules.iter().map(|r| r.description()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;

    fn make_identity(comment: &str) -> Identity {
        Identity::new(Bytes::new(), comment.to_string())
    }

    #[test]
    fn test_empty_evaluator() {
        let evaluator = FilterEvaluator::default();
        assert!(evaluator.is_empty());
        assert!(evaluator.matches(&make_identity("any")));
    }

    #[test]
    fn test_single_rule() {
        let evaluator = FilterEvaluator::parse(&["comment=test".to_string()]).unwrap();
        assert!(evaluator.matches(&make_identity("test")));
        assert!(!evaluator.matches(&make_identity("other")));
    }

    #[test]
    fn test_multiple_rules_and() {
        let evaluator = FilterEvaluator::parse(&[
            "comment=*@work*".to_string(),
            "not-comment=*@work.bad*".to_string(),
        ])
        .unwrap();

        assert!(evaluator.matches(&make_identity("user@work.good")));
        assert!(!evaluator.matches(&make_identity("user@work.bad")));
        assert!(!evaluator.matches(&make_identity("user@home")));
    }

    #[test]
    fn test_filter_identities() {
        let evaluator = FilterEvaluator::parse(&["comment=*@work*".to_string()]).unwrap();
        let identities = vec![
            make_identity("user@work"),
            make_identity("user@home"),
            make_identity("admin@work"),
        ];

        let filtered = evaluator.filter_identities(identities);
        assert_eq!(filtered.len(), 2);
        assert_eq!(filtered[0].comment, "user@work");
        assert_eq!(filtered[1].comment, "admin@work");
    }
}
