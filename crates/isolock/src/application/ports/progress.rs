//! Operator-facing progress is a port so use cases never print.

/// A sink for the human-readable lines a workflow reports while it runs.
pub trait Progress {
    /// Records one line, without its trailing newline.
    fn line(&mut self, line: &str);
}

/// A sink that keeps every line in order, for tests.
#[cfg(test)]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Recorded(pub Vec<String>);

#[cfg(test)]
impl Progress for Recorded {
    fn line(&mut self, line: &str) {
        self.0.push(line.to_owned());
    }
}

#[cfg(test)]
impl Recorded {
    /// The recorded lines joined with newlines, exactly as an operator would see them.
    #[must_use]
    pub fn joined(&self) -> String {
        self.0.join("\n")
    }
}
