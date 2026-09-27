//! Canonical query-meter admission. Receipts confer no data or execution authority.
use crate::error::{AllSourceError, Result};
use serde::{Deserialize, Serialize};

pub const QUERY_RETRY_SECONDS: i64 = 3_600;
pub const MAX_QUERY_RECEIPTS: usize = 4_096;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct QueryUsageRequest {
    pub operation_id: String,
    pub fingerprint: String,
    pub count: u64,
    pub expected_period: u64,
}

impl QueryUsageRequest {
    pub fn expires_at(&self) -> Result<i64> {
        let invalid = || AllSourceError::ValidationError("Invalid query admission request".into());
        let (time, nonce) = self.operation_id.split_once(':').ok_or_else(invalid)?;
        let issued: i64 = time.parse().map_err(|_| invalid())?;
        let uuid = uuid::Uuid::parse_str(nonce).map_err(|_| invalid())?;
        if issued < 0
            || time != issued.to_string()
            || nonce != uuid.hyphenated().to_string()
            || !(1..=4).contains(&self.count)
            || self.fingerprint.len() != 64
            || !self
                .fingerprint
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        {
            return Err(invalid());
        }
        issued.checked_add(QUERY_RETRY_SECONDS).ok_or_else(invalid)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct QueryUsageReceipt {
    pub operation_id: String,
    pub fingerprint: String,
    pub count: u64,
    pub period: u64,
    pub used: u64,
    pub expires_at: i64,
}

impl QueryUsageReceipt {
    pub fn matches(&self, request: &QueryUsageRequest) -> bool {
        self.fingerprint == request.fingerprint
            && self.count == request.count
            && self.period == request.expected_period
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum QueryUsageDenial {
    ExpiredOperation,
    OperationConflict,
    PeriodChanged,
    QuotaExceeded,
    ReceiptCapacity,
    InactiveTenant,
    InvalidQuotaMetadata,
}

#[derive(Debug, PartialEq)]
pub enum QueryUsageDecision {
    Admitted {
        receipt: QueryUsageReceipt,
        replayed: bool,
    },
    Denied(QueryUsageDenial),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueryUsageReset {
    pub expected_period: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct QueryUsageResetReceipt {
    pub previous_period: u64,
    pub period: u64,
}

#[derive(Debug, Serialize)]
pub struct QueryUsageSnapshot {
    pub period: u64,
    pub used: u64,
    pub quota: i64,
    pub managed: bool,
}

#[derive(Debug, PartialEq)]
pub enum QueryUsageResetDecision {
    Reset {
        receipt: QueryUsageResetReceipt,
        replayed: bool,
    },
    Denied(QueryUsageDenial),
}
