//! Filter evaluation engine

use crate::error::Result;
use crate::filter::{Filter, FilterRule};
use crate::protocol::Identity;

/// A group of rules that are ANDed together
#[derive(Debug, Clone, Default)]
pub struct FilterGroup {
    /// Rules in this group (ANDed together)
    rules: Vec<FilterRule>,
}

impl FilterGroup {
    /// Parse filter strings into a group
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

    /// Get rules for inspection
    pub fn rules(&self) -> &[FilterRule] {
        &self.rules
    }
}

/// Evaluator for filter groups (ORed together)
#[derive(Debug, Clone, Default)]
pub struct FilterEvaluator {
    /// Groups to evaluate (ORed together, each group is ANDed internally)
    groups: Vec<FilterGroup>,
}

impl FilterEvaluator {
    /// Create a new filter evaluator from groups
    pub fn new(groups: Vec<FilterGroup>) -> Self {
        Self { groups }
    }

    /// Parse filter group strings into an evaluator
    /// Outer array is OR, inner array is AND
    pub fn parse(filter_groups: &[Vec<String>]) -> Result<Self> {
        let groups = filter_groups
            .iter()
            .map(|g| FilterGroup::parse(g))
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { groups })
    }

    /// Check if any group matches the given identity (OR logic between groups)
    pub fn matches(&self, identity: &Identity) -> bool {
        // Empty groups = match all
        if self.groups.is_empty() {
            return true;
        }
        self.groups.iter().any(|g| g.matches(identity))
    }

    /// Filter a list of identities
    pub fn filter_identities(&self, identities: Vec<Identity>) -> Vec<Identity> {
        identities.into_iter().filter(|i| self.matches(i)).collect()
    }

    /// Get the number of groups
    pub fn len(&self) -> usize {
        self.groups.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.groups.is_empty()
    }

    /// Get groups for inspection
    pub fn groups(&self) -> &[FilterGroup] {
        &self.groups
    }

    /// Ensure all async filters are loaded (GitHub keys, etc.)
    pub async fn ensure_loaded(&self) -> Result<()> {
        for group in &self.groups {
            for rule in group.rules() {
                match &rule.filter {
                    Filter::GitHub(m) => m.ensure_loaded().await?,
                    Filter::Keyfile(m) => m.reload()?,
                    _ => {}
                }
            }
        }
        Ok(())
    }

    /// Reload all reloadable filters
    pub async fn reload(&self) -> Result<()> {
        for group in &self.groups {
            for rule in group.rules() {
                match &rule.filter {
                    Filter::GitHub(m) => m.fetch_keys().await?,
                    Filter::Keyfile(m) => m.reload()?,
                    _ => {}
                }
            }
        }
        Ok(())
    }

    /// Get descriptions of all rules (grouped)
    pub fn descriptions(&self) -> Vec<Vec<String>> {
        self.groups
            .iter()
            .map(|g| g.rules().iter().map(|r| r.description()).collect())
            .collect()
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
        let evaluator = FilterEvaluator::parse(&[vec!["comment=test".to_string()]]).unwrap();
        assert!(evaluator.matches(&make_identity("test")));
        assert!(!evaluator.matches(&make_identity("other")));
    }

    #[test]
    fn test_multiple_rules_and() {
        // Single group with multiple rules (AND)
        let evaluator = FilterEvaluator::parse(&[vec![
            "comment=*@work*".to_string(),
            "not-comment=*@work.bad*".to_string(),
        ]])
        .unwrap();

        assert!(evaluator.matches(&make_identity("user@work.good")));
        assert!(!evaluator.matches(&make_identity("user@work.bad")));
        assert!(!evaluator.matches(&make_identity("user@home")));
    }

    #[test]
    fn test_filter_identities() {
        let evaluator = FilterEvaluator::parse(&[vec!["comment=*@work*".to_string()]]).unwrap();
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

    #[test]
    fn test_or_logic() {
        // Two groups (OR): either @work or admin
        let evaluator = FilterEvaluator::parse(&[
            vec!["comment=*@work*".to_string()],
            vec!["comment=admin*".to_string()],
        ])
        .unwrap();

        assert!(evaluator.matches(&make_identity("user@work")));
        assert!(evaluator.matches(&make_identity("admin@home")));
        assert!(!evaluator.matches(&make_identity("user@home")));
    }

    #[test]
    fn test_and_or_combined() {
        // (f1 AND f2) OR f3
        let evaluator = FilterEvaluator::parse(&[
            vec![
                "comment=*kawaz*".to_string(),
                "comment=*ed25519*".to_string(),
            ],
            vec!["comment=*syun*".to_string()],
        ])
        .unwrap();

        assert!(evaluator.matches(&make_identity("kawaz-ed25519"))); // f1 AND f2
        assert!(evaluator.matches(&make_identity("syun-key"))); // f3
        assert!(!evaluator.matches(&make_identity("kawaz-rsa"))); // only f1
        assert!(!evaluator.matches(&make_identity("other"))); // none
    }
}
