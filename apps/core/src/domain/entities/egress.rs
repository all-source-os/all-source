//! Funded response admission. Reservations do not confer source-access authority.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const MAX_RESPONSE_BYTES: u64 = 16 * 1024 * 1024;
pub const RESERVATION_SECONDS: i64 = 30;
/// Larger than the control plane's 16 response slots × 16 MiB, so a machine
/// whose slots are all busy still has headroom and renews on its timer rather
/// than once per response. Also the most a crashed control plane leaves unbilled.
pub const MAX_LEASE_BYTES: u64 = 1024 * 1024 * 1024;
pub const LEASE_SECONDS: i64 = 120;
pub const MAX_LEASES: usize = 32;
const SETTLE_RECEIPT_SECONDS: i64 = 3600;
const MAX_RECEIPTS: usize = 4096;
const MAX_PAYMENTS: usize = 10_000;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum EgressCommand {
    Configure {
        included_bytes: u64,
    },
    ConfigurePlan {
        included_bytes: u64,
    },
    Reserve {
        id: String,
    },
    Complete {
        id: String,
        bytes: u64,
    },
    Lease {
        id: String,
        bytes: u64,
    },
    Settle {
        id: String,
        bytes: u64,
        next: Option<LeaseRequest>,
    },
    Credit {
        payment_id: String,
        bytes: u64,
    },
    Refund {
        payment_id: String,
    },
    Payment {
        payment_id: String,
        bytes: u64,
        refunded: bool,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LeaseRequest {
    pub id: String,
    pub bytes: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct EgressEvent {
    pub command: EgressCommand,
    pub at: i64,
    pub period: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct Reservation {
    period: u32,
    included: u64,
    prepaid: u64,
    expires_at: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct Lease {
    period: u32,
    included: u64,
    prepaid: u64,
    expires_at: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct Receipt {
    bytes: u64,
    expires_at: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct Payment {
    bytes: u64,
    refunded: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct EgressAccount {
    invalid: bool,
    configured: bool,
    plan_managed: bool,
    period: u32,
    included_bytes: u64,
    used_bytes: u64,
    prepaid_bytes: u64,
    prepaid_used: u64,
    refunded_bytes: u64,
    reservations: BTreeMap<String, Reservation>,
    leases: BTreeMap<String, Lease>,
    receipts: BTreeMap<String, Receipt>,
    payments: BTreeMap<String, Payment>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct EgressSnapshot {
    pub period: u32,
    pub included_bytes: u64,
    pub used_bytes: u64,
    pub included_available: u64,
    pub prepaid_available: u64,
    pub in_flight: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct EgressAdmission {
    pub snapshot: EgressSnapshot,
    pub reserved_bytes: u64,
    pub replayed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lease_expires_at: Option<i64>,
    /// The part of a lease grant drawn from the included allowance; the rest
    /// of `reserved_bytes` is prepaid.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lease_included: Option<u64>,
    /// Why a settle's `next` lease was not granted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_refused: Option<EgressDenial>,
}

struct LeaseGrant {
    included: u64,
    prepaid: u64,
    expires_at: i64,
    replayed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EgressDenial {
    Unconfigured,
    Exhausted,
    Busy,
    Invalid,
    Conflict,
    Expired,
    Capacity,
}

impl EgressAccount {
    pub fn invalidate(&mut self) {
        self.invalid = true;
    }

    pub fn configured(&self) -> bool {
        self.configured
    }

    pub fn plan_managed(&self) -> bool {
        self.plan_managed
    }

    pub fn snapshot(&self) -> EgressSnapshot {
        let held_included: u64 = self
            .reservations
            .values()
            .filter(|r| r.period == self.period)
            .map(|r| r.included)
            .chain(
                self.leases
                    .values()
                    .filter(|l| l.period == self.period)
                    .map(|l| l.included),
            )
            .sum();
        let held_prepaid: u64 = self
            .reservations
            .values()
            .map(|r| r.prepaid)
            .chain(self.leases.values().map(|l| l.prepaid))
            .sum();
        EgressSnapshot {
            period: self.period,
            included_bytes: self.included_bytes,
            used_bytes: self.used_bytes,
            included_available: self
                .included_bytes
                .saturating_sub(self.used_bytes)
                .saturating_sub(held_included),
            prepaid_available: self
                .prepaid_bytes
                .saturating_sub(self.refunded_bytes)
                .saturating_sub(self.prepaid_used)
                .saturating_sub(held_prepaid),
            in_flight: self.reservations.len() + self.leases.len(),
        }
    }

    pub fn apply(&mut self, event: &EgressEvent) -> Result<EgressAdmission, EgressDenial> {
        let mut next = self.clone();
        let result = next.transition(event)?;
        *self = next;
        Ok(result)
    }

    fn transition(&mut self, event: &EgressEvent) -> Result<EgressAdmission, EgressDenial> {
        use EgressDenial::{Capacity, Conflict, Exhausted, Expired, Invalid};
        if self.invalid || event.at < 0 || event.period < self.period {
            return Err(Invalid);
        }
        self.reservations.retain(|_, r| r.expires_at > event.at);
        self.leases.retain(|_, l| l.expires_at > event.at);
        self.receipts.retain(|_, r| r.expires_at > event.at);
        if event.period > self.period {
            self.period = event.period;
            self.used_bytes = 0;
        }
        let mut reserved_bytes = 0;
        let mut replayed = false;
        let mut lease_expires_at = None;
        let mut lease_included = None;
        let mut next_refused = None;
        match &event.command {
            EgressCommand::Payment {
                payment_id,
                bytes,
                refunded,
            } => {
                let credit = self.transition(&EgressEvent {
                    command: EgressCommand::Credit {
                        payment_id: payment_id.clone(),
                        bytes: *bytes,
                    },
                    at: event.at,
                    period: event.period,
                })?;
                replayed = credit.replayed;
                if *refunded {
                    let reversed = self.transition(&EgressEvent {
                        command: EgressCommand::Refund {
                            payment_id: payment_id.clone(),
                        },
                        at: event.at,
                        period: event.period,
                    })?;
                    replayed &= reversed.replayed;
                }
            }
            EgressCommand::Configure { included_bytes }
            | EgressCommand::ConfigurePlan { included_bytes } => {
                if *included_bytes > 100_000_000_000_000 {
                    return Err(Invalid);
                }
                self.included_bytes = *included_bytes;
                self.configured = true;
                self.plan_managed = matches!(event.command, EgressCommand::ConfigurePlan { .. });
            }
            EgressCommand::Reserve { id } => {
                valid_operation(id)?;
                if !self.configured {
                    return Err(EgressDenial::Unconfigured);
                }
                if self.receipts.contains_key(id) {
                    return Err(Conflict);
                }
                if let Some(r) = self.reservations.get(id) {
                    reserved_bytes = r.included + r.prepaid;
                    replayed = true;
                } else {
                    if self.reservations.len() >= 4 {
                        return Err(EgressDenial::Busy);
                    }
                    if self.receipts.len() + self.reservations.len() >= MAX_RECEIPTS {
                        return Err(Capacity);
                    }
                    let snapshot = self.snapshot();
                    let included = snapshot.included_available.min(MAX_RESPONSE_BYTES);
                    let prepaid = snapshot
                        .prepaid_available
                        .min(MAX_RESPONSE_BYTES - included);
                    reserved_bytes = included + prepaid;
                    if reserved_bytes == 0 {
                        return Err(if self.reservations.is_empty() {
                            Exhausted
                        } else {
                            EgressDenial::Busy
                        });
                    }
                    self.reservations.insert(
                        id.clone(),
                        Reservation {
                            period: self.period,
                            included,
                            prepaid,
                            expires_at: event.at.checked_add(RESERVATION_SECONDS).ok_or(Invalid)?,
                        },
                    );
                }
            }
            EgressCommand::Complete { id, bytes } => {
                valid_operation(id)?;
                if let Some(r) = self.receipts.get(id) {
                    if r.bytes != *bytes {
                        return Err(Conflict);
                    }
                    replayed = true;
                } else {
                    let r = self.reservations.remove(id).ok_or(Expired)?;
                    if *bytes > r.included + r.prepaid {
                        return Err(Invalid);
                    }
                    let included = (*bytes).min(r.included);
                    let available = self.snapshot();
                    if (r.period == self.period && included > available.included_available)
                        || bytes - included > available.prepaid_available
                    {
                        return Err(Exhausted);
                    }
                    if r.period == self.period {
                        self.used_bytes = self.used_bytes.checked_add(included).ok_or(Invalid)?;
                    }
                    self.prepaid_used = self
                        .prepaid_used
                        .checked_add(bytes - included)
                        .ok_or(Invalid)?;
                    self.receipts.insert(
                        id.clone(),
                        Receipt {
                            bytes: *bytes,
                            expires_at: event.at.checked_add(300).ok_or(Invalid)?,
                        },
                    );
                }
            }
            EgressCommand::Lease { id, bytes } => {
                let grant = self.grant_lease(id, *bytes, event.at)?;
                reserved_bytes = grant.included + grant.prepaid;
                lease_expires_at = Some(grant.expires_at);
                lease_included = Some(grant.included);
                replayed = grant.replayed;
            }
            EgressCommand::Settle { id, bytes, next } => {
                valid_operation(id)?;
                let mut settle_replayed = true;
                if let Some(r) = self.receipts.get(id) {
                    if r.bytes != *bytes {
                        return Err(Conflict);
                    }
                } else {
                    settle_replayed = false;
                    self.charge_lease(id, *bytes, event.at)?;
                }
                let mut next_replayed = true;
                if let Some(next) = next {
                    match self.grant_lease(&next.id, next.bytes, event.at) {
                        Ok(grant) => {
                            reserved_bytes = grant.included + grant.prepaid;
                            lease_expires_at = Some(grant.expires_at);
                            lease_included = Some(grant.included);
                            next_replayed = grant.replayed;
                        }
                        Err(denial @ (EgressDenial::Exhausted | EgressDenial::Busy)) => {
                            next_refused = Some(denial);
                        }
                        Err(denial) => return Err(denial),
                    }
                }
                replayed = settle_replayed && next_replayed;
            }
            EgressCommand::Credit { payment_id, bytes } => {
                valid_payment(payment_id)?;
                if *bytes == 0 || *bytes > 100_000_000_000_000 {
                    return Err(Invalid);
                }
                if let Some(payment) = self.payments.get(payment_id) {
                    if payment.bytes != *bytes {
                        return Err(Conflict);
                    }
                    replayed = true;
                } else {
                    if self.payments.len() >= MAX_PAYMENTS {
                        return Err(Capacity);
                    }
                    self.prepaid_bytes = self.prepaid_bytes.checked_add(*bytes).ok_or(Invalid)?;
                    self.payments.insert(
                        payment_id.clone(),
                        Payment {
                            bytes: *bytes,
                            refunded: false,
                        },
                    );
                }
            }
            EgressCommand::Refund { payment_id } => {
                valid_payment(payment_id)?;
                let payment = self.payments.get_mut(payment_id).ok_or(Conflict)?;
                if payment.refunded {
                    replayed = true;
                } else {
                    self.refunded_bytes = self
                        .refunded_bytes
                        .checked_add(payment.bytes)
                        .ok_or(Invalid)?;
                    payment.refunded = true;
                }
            }
        }
        Ok(EgressAdmission {
            snapshot: self.snapshot(),
            reserved_bytes,
            replayed,
            lease_expires_at,
            lease_included,
            next_refused,
        })
    }

    /// Under 2 GiB a grant takes half the balance, or one full response when
    /// half is smaller, so a second holder (a second machine, or a restart while
    /// the dead process's lease runs out) can still be served.
    fn grant_lease(&mut self, id: &str, bytes: u64, at: i64) -> Result<LeaseGrant, EgressDenial> {
        valid_operation(id)?;
        if !self.configured {
            return Err(EgressDenial::Unconfigured);
        }
        if self.receipts.contains_key(id) {
            return Err(EgressDenial::Conflict);
        }
        if let Some(lease) = self.leases.get(id) {
            return Ok(LeaseGrant {
                included: lease.included,
                prepaid: lease.prepaid,
                expires_at: lease.expires_at,
                replayed: true,
            });
        }
        if self.leases.len() >= MAX_LEASES
            || self.receipts.len() + self.reservations.len() + self.leases.len() >= MAX_RECEIPTS
        {
            return Err(EgressDenial::Capacity);
        }
        let snapshot = self.snapshot();
        let available = snapshot
            .included_available
            .saturating_add(snapshot.prepaid_available);
        let share = available.div_ceil(2).max(available.min(MAX_RESPONSE_BYTES));
        let granted = bytes.min(MAX_LEASE_BYTES).min(share);
        let included = granted.min(snapshot.included_available);
        let prepaid = (granted - included).min(snapshot.prepaid_available);
        if included + prepaid == 0 {
            return Err(if self.leases.is_empty() && self.reservations.is_empty() {
                EgressDenial::Exhausted
            } else {
                EgressDenial::Busy
            });
        }
        let expires_at = at.checked_add(LEASE_SECONDS).ok_or(EgressDenial::Invalid)?;
        self.leases.insert(
            id.to_owned(),
            Lease {
                period: self.period,
                included,
                prepaid,
                expires_at,
            },
        );
        Ok(LeaseGrant {
            included,
            prepaid,
            expires_at,
            replayed: false,
        })
    }

    /// Charges a settle against its lease, or as a late settle when the lease has
    /// expired and no receipt exists. Charges are capped at what is available, so
    /// a settle never fails for lack of balance.
    fn charge_lease(&mut self, id: &str, bytes: u64, at: i64) -> Result<(), EgressDenial> {
        let (cap_included, cap_prepaid, counts_included, total) = match self.leases.get(id) {
            Some(lease) => {
                if bytes > lease.included + lease.prepaid {
                    return Err(EgressDenial::Invalid);
                }
                (
                    lease.included,
                    lease.prepaid,
                    lease.period == self.period,
                    bytes,
                )
            }
            None => (u64::MAX, u64::MAX, true, bytes.min(MAX_LEASE_BYTES)),
        };
        self.leases.remove(id);
        let available = self.snapshot();
        let included = if counts_included {
            total.min(cap_included).min(available.included_available)
        } else {
            total.min(cap_included)
        };
        let prepaid = (total - included)
            .min(cap_prepaid)
            .min(available.prepaid_available);
        if counts_included {
            self.used_bytes = self
                .used_bytes
                .checked_add(included)
                .ok_or(EgressDenial::Invalid)?;
        }
        self.prepaid_used = self
            .prepaid_used
            .checked_add(prepaid)
            .ok_or(EgressDenial::Invalid)?;
        self.receipts.insert(
            id.to_owned(),
            Receipt {
                bytes,
                expires_at: at
                    .checked_add(SETTLE_RECEIPT_SECONDS)
                    .ok_or(EgressDenial::Invalid)?,
            },
        );
        Ok(())
    }
}

fn valid_operation(id: &str) -> Result<(), EgressDenial> {
    let value = uuid::Uuid::parse_str(id).map_err(|_| EgressDenial::Invalid)?;
    if id != value.hyphenated().to_string() {
        return Err(EgressDenial::Invalid);
    }
    Ok(())
}

fn valid_payment(id: &str) -> Result<(), EgressDenial> {
    if id.is_empty()
        || id.len() > 160
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_:".contains(&b))
    {
        return Err(EgressDenial::Invalid);
    }
    Ok(())
}

pub fn included_egress_bytes(tier: &str) -> Option<u64> {
    let gb = match tier {
        "free" | "trial" => 1,
        "indie" | "pro" | "professional" | "starter" | "developer" => 25,
        "studio" | "growth" | "team" => 150,
        "scale" => 750,
        _ => return None,
    };
    Some(gb * 1_000_000_000)
}
