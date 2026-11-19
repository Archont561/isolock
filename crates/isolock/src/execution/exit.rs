//! Child exit mapping is separate from session lifetime and command construction.

use std::process::{ExitCode, ExitStatus};

pub fn map_status(status: ExitStatus) -> ExitCode {
    code(status).map_or(ExitCode::FAILURE, ExitCode::from)
}

fn code(status: ExitStatus) -> Option<u8> {
    if let Some(code) = status.code() {
        return u8::try_from(code).ok();
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        status
            .signal()
            .and_then(|signal| u8::try_from(128 + signal).ok())
    }
    #[cfg(not(unix))]
    None
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::process::ExitStatusExt;

    #[test]
    fn maps_success_failure_and_signal_without_truncation() {
        assert_eq!(code(ExitStatus::from_raw(0)), Some(0));
        assert_eq!(code(ExitStatus::from_raw(7 << 8)), Some(7));
        assert_eq!(code(ExitStatus::from_raw(15)), Some(143));
    }
}
