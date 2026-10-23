//! Funded response admission. Reservations do not confer source-access authority.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const MAX_RESPONSE_BYTES: u64 = 16 * 1024 * 1024;
pub const RESERVATION_SECONDS: i64 = 30;
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
            .sum();
        let held_prepaid: u64 = self.reservations.values().map(|r| r.prepaid).sum();
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
            in_flight: self.reservations.len(),
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
        self.receipts.retain(|_, r| r.expires_at > event.at);
        if event.period > self.period {
            self.period = event.period;
            self.used_bytes = 0;
        }
        let mut reserved_bytes = 0;
        let mut replayed = false;
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
        })
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
