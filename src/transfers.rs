use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confidence {
    High,
    Medium,
    Low,
}

impl Confidence {
    pub fn as_str(&self) -> &'static str {
        match self {
            Confidence::High => "high",
            Confidence::Medium => "medium",
            Confidence::Low => "low",
        }
    }

    pub fn from_str(s: &str) -> Option<Confidence> {
        match s {
            "high" => Some(Confidence::High),
            "medium" => Some(Confidence::Medium),
            "low" => Some(Confidence::Low),
            _ => None,
        }
    }
}

impl fmt::Display for Confidence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Status {
    Pending,
    Confirmed,
    Rejected,
}

impl Status {
    pub fn as_str(&self) -> &'static str {
        match self {
            Status::Pending => "pending",
            Status::Confirmed => "confirmed",
            Status::Rejected => "rejected",
        }
    }

    pub fn from_str(s: &str) -> Option<Status> {
        match s {
            "pending" => Some(Status::Pending),
            "confirmed" => Some(Status::Confirmed),
            "rejected" => Some(Status::Rejected),
            _ => None,
        }
    }
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone)]
pub struct TransferPair {
    pub txn_id_a: i64,
    pub txn_id_b: i64,
    pub amount_cents: i64,
    pub confidence: Confidence,
    pub status: Status,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_confidence_roundtrip() {
        for c in [Confidence::High, Confidence::Medium, Confidence::Low] {
            assert_eq!(Confidence::from_str(c.as_str()), Some(c));
        }
        assert_eq!(Confidence::from_str("invalid"), None);
    }

    #[test]
    fn test_status_roundtrip() {
        for s in [Status::Pending, Status::Confirmed, Status::Rejected] {
            assert_eq!(Status::from_str(s.as_str()), Some(s));
        }
        assert_eq!(Status::from_str("invalid"), None);
    }

    #[test]
    fn test_transfer_pair_construction() {
        let pair = TransferPair {
            txn_id_a: 1,
            txn_id_b: 2,
            amount_cents: 5000,
            confidence: Confidence::High,
            status: Status::Pending,
        };
        assert_eq!(pair.txn_id_a, 1);
        assert_eq!(pair.txn_id_b, 2);
        assert_eq!(pair.amount_cents, 5000);
        assert_eq!(pair.confidence, Confidence::High);
        assert_eq!(pair.status, Status::Pending);
    }

    #[test]
    fn test_display() {
        assert_eq!(format!("{}", Confidence::High), "high");
        assert_eq!(format!("{}", Status::Confirmed), "confirmed");
    }
}
