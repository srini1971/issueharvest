use std::fmt;

/// A normalized priority derived from the labels of a GitHub issue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum Priority {
    #[default]
    Unknown,
    Low,
    Medium,
    High,
    Critical,
}

impl Priority {
    pub(crate) fn rank(self) -> u8 {
        match self {
            Self::Unknown => 0,
            Self::Low => 1,
            Self::Medium => 2,
            Self::High => 3,
            Self::Critical => 4,
        }
    }
}

impl fmt::Display for Priority {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Unknown => "unknown",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Critical => "critical",
        })
    }
}

/// How strongly the source data supports the derived priority.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confidence {
    None,
    High,
}

/// An explainable priority classification rather than an opaque score.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PriorityAssessment {
    pub level: Priority,
    pub confidence: Confidence,
    pub reasons: Vec<String>,
}

pub(crate) fn assess(labels: &[String]) -> PriorityAssessment {
    let mut best = Priority::Unknown;
    let mut reasons = Vec::new();

    for label in labels {
        let normalized = label.to_ascii_lowercase();
        let candidate = match normalized.as_str() {
            "p0" | "priority: critical" | "priority:critical" | "critical" | "blocker"
            | "urgent" => Priority::Critical,
            "p1" | "priority: high" | "priority:high" | "high priority" | "severity: high"
            | "severity:high" => Priority::High,
            "p2" | "priority: medium" | "priority:medium" | "medium priority" => Priority::Medium,
            "p3" | "priority: low" | "priority:low" | "low priority" | "minor" => Priority::Low,
            _ => Priority::Unknown,
        };

        if candidate.rank() > best.rank() {
            best = candidate;
            reasons.clear();
            reasons.push(format!("matched issue label `{label}`"));
        } else if candidate == best && candidate != Priority::Unknown {
            reasons.push(format!("matched issue label `{label}`"));
        }
    }

    PriorityAssessment {
        level: best,
        confidence: if best == Priority::Unknown {
            Confidence::None
        } else {
            Confidence::High
        },
        reasons,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn most_urgent_known_label_wins() {
        let assessment = assess(&["bug".into(), "P1".into(), "blocker".into()]);
        assert_eq!(assessment.level, Priority::Critical);
        assert_eq!(assessment.confidence, Confidence::High);
    }
}
