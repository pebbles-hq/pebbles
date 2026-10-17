//! Per-case status (§6.11) and the process exit code (§6.4).

/// A case's verdict. Order of the hard statuses follows §6.11.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Status {
    Pass,
    Fail,
    Xfail,
    Xpass,
    Panic,
    Hang,
    GpuError,
    Unstable,
    Size,
    MissingRef,
    ProbeFail,
    /// The case rendered without a hard failure but its spec reference does not exist yet
    /// (the golden model lands with V44–V47; self-goldens with V12). Never fails the run.
    Unjudged,
}

impl Status {
    /// Every status, in report order.
    pub const ALL: [Status; 12] = [
        Self::Pass,
        Self::Fail,
        Self::Xfail,
        Self::Xpass,
        Self::Panic,
        Self::Hang,
        Self::GpuError,
        Self::Unstable,
        Self::Size,
        Self::MissingRef,
        Self::ProbeFail,
        Self::Unjudged,
    ];

    /// The report label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Pass => "PASS",
            Self::Fail => "FAIL",
            Self::Xfail => "XFAIL",
            Self::Xpass => "XPASS",
            Self::Panic => "PANIC",
            Self::Hang => "HANG",
            Self::GpuError => "GPU_ERROR",
            Self::Unstable => "UNSTABLE",
            Self::Size => "SIZE",
            Self::MissingRef => "MISSING_REF",
            Self::ProbeFail => "PROBE_FAIL",
            Self::Unjudged => "UNJUDGED",
        }
    }

    /// Whether this status fails the run.
    pub fn fails_run(self) -> bool {
        !matches!(self, Self::Pass | Self::Xfail | Self::Unjudged)
    }
}

/// Exit code for a finished run: 0 when every case is PASS / XFAIL / UNJUDGED, else 1.
pub fn exit_code(statuses: &[Status]) -> i32 {
    i32::from(statuses.iter().any(|s| s.fails_run()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_code_zero_for_pass_and_xfail() {
        assert_eq!(exit_code(&[]), 0);
        assert_eq!(exit_code(&[Status::Pass, Status::Xfail, Status::Pass]), 0);
    }

    #[test]
    fn exit_code_one_for_xpass() {
        assert_eq!(exit_code(&[Status::Pass, Status::Xpass]), 1);
    }

    #[test]
    fn exit_code_one_for_unstable() {
        assert_eq!(exit_code(&[Status::Unstable]), 1);
    }

    #[test]
    fn exit_code_one_for_every_hard_status() {
        for s in Status::ALL {
            let want = i32::from(!matches!(s, Status::Pass | Status::Xfail | Status::Unjudged));
            assert_eq!(exit_code(&[s]), want, "{s:?}");
        }
    }
}
