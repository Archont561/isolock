//! Maps application and domain failures onto clifacet's failure categories.
//!
//! The mapping is deliberately narrow: every use-case failure is an execution failure, which
//! `ExitPolicy::standard` renders as status 1, exactly as the string-typed implementation did.

use crate::domain::DomainError;
use clifacet::Failure;

/// Converts one application failure into the CLI's failure category.
#[must_use]
pub fn execution(error: DomainError) -> Failure {
    Failure::execution(error.message())
}

/// Converts a rendered message into a usage failure (exit status 2).
#[cfg(test)]
#[must_use]
pub fn usage(text: impl Into<String>) -> Failure {
    Failure::validation(text.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clifacet::FailureKind;

    #[test]
    fn domain_failures_are_execution_failures_with_the_original_message() {
        let failure = execution(DomainError::new("explicit --profile is required"));
        assert_eq!(failure.kind(), FailureKind::Execution);
        assert_eq!(failure.message(), "explicit --profile is required");
    }

    #[test]
    fn usage_failures_are_validation_failures() {
        assert_eq!(
            usage("expected a child command").kind(),
            FailureKind::Validation
        );
    }
}
