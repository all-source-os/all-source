//! [`UserRecord`] — adapter-internal `User` wrapper that round-trips
//! `metadata` through the event payload.
//!
//! `better_auth_core::types::User` carries `#[serde(skip)]` on its
//! `metadata` field (`better-auth-core/src/types.rs:38`), so a naive
//! `serde_json::to_value(&user)` drops the field before it ever reaches
//! the event store. This made password hashes and any other consumer
//! data silently disappear at write time, and reappear as `Value::Null`
//! at read time — see issue #187 for the production fallout.
//!
//! `UserRecord` is a thin newtype with hand-written `Serialize` and
//! `Deserialize` impls that re-attach `metadata` on both sides of the
//! boundary. All adapter `UserOps` write paths serialize through it,
//! and all read paths deserialize through it, so the field round-trips
//! losslessly through `auth.user.created` / `auth.user.updated` events
//! without any change to the upstream `better-auth-core` crate.

use better_auth_core::types::User;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Newtype carrier that makes `User.metadata` survive serde round-trips.
#[derive(Debug, Clone)]
pub(crate) struct UserRecord(pub User);

impl UserRecord {
    pub fn into_user(self) -> User {
        self.0
    }
}

impl Serialize for UserRecord {
    fn serialize<S: Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        // Lean on `User`'s derived `Serialize` to render every field except
        // `metadata`, then splice `metadata` back in. Stays robust if the
        // upstream struct gains new fields.
        let mut value = serde_json::to_value(&self.0).map_err(serde::ser::Error::custom)?;
        if !self.0.metadata.is_null() {
            if let Some(obj) = value.as_object_mut() {
                obj.insert("metadata".to_string(), self.0.metadata.clone());
            }
        }
        value.serialize(ser)
    }
}

impl<'de> Deserialize<'de> for UserRecord {
    fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        let raw = serde_json::Value::deserialize(de)?;
        let mut user: User =
            serde_json::from_value(raw.clone()).map_err(serde::de::Error::custom)?;
        if let Some(meta) = raw.get("metadata") {
            user.metadata = meta.clone();
        }
        Ok(UserRecord(user))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn sample_user_with_metadata(metadata: serde_json::Value) -> User {
        let now = Utc::now();
        User {
            id: "uid-1".into(),
            email: Some("alias+test@example.com".into()),
            name: Some("Alias Test".into()),
            image: None,
            email_verified: false,
            created_at: now,
            updated_at: now,
            username: None,
            display_username: None,
            two_factor_enabled: false,
            role: None,
            banned: false,
            ban_reason: None,
            ban_expires: None,
            metadata,
        }
    }

    /// Regression for the second half of #187: `User.metadata` must
    /// survive serialize → JSON → deserialize through `UserRecord`.
    /// Bare `User` drops it because of upstream `#[serde(skip)]`.
    #[test]
    fn user_record_roundtrips_metadata() {
        let user = sample_user_with_metadata(serde_json::json!({
            "app_field": "kept",
            "nested": { "x": 1 }
        }));
        let json = serde_json::to_value(UserRecord(user.clone())).unwrap();
        assert_eq!(
            json.get("metadata"),
            Some(&serde_json::json!({"app_field": "kept", "nested": {"x": 1}})),
            "metadata must appear in the serialized payload"
        );

        let back: UserRecord = serde_json::from_value(json).unwrap();
        assert_eq!(back.0.metadata, user.metadata);
        assert_eq!(back.0.email, user.email);
        assert_eq!(back.0.id, user.id);
    }

    /// Demonstrates the upstream bare-`User` behavior we're working
    /// around: serializing/deserializing `User` directly loses metadata.
    #[test]
    fn bare_user_loses_metadata_demonstration() {
        let user = sample_user_with_metadata(serde_json::json!({"x": 1}));
        let json = serde_json::to_value(&user).unwrap();
        assert!(
            json.get("metadata").is_none(),
            "upstream User serializer should drop metadata (this asserts the bug we work around — if it ever passes, simplify UserRecord)"
        );
    }

    #[test]
    fn user_record_with_null_metadata_omits_field() {
        let user = sample_user_with_metadata(serde_json::Value::Null);
        let json = serde_json::to_value(UserRecord(user)).unwrap();
        assert!(
            json.get("metadata").is_none(),
            "null metadata should not be written into the payload — keeps events compact"
        );
    }

    #[test]
    fn user_record_deserialize_without_metadata_defaults_to_null() {
        let json = serde_json::json!({
            "id": "uid-2",
            "name": null,
            "email": "x@y.io",
            "emailVerified": false,
            "image": null,
            "createdAt": Utc::now().to_rfc3339(),
            "updatedAt": Utc::now().to_rfc3339(),
            "username": null,
            "displayUsername": null,
            "twoFactorEnabled": false,
            "role": null,
            "banned": false,
            "banReason": null,
            "banExpires": null
        });
        let back: UserRecord = serde_json::from_value(json).unwrap();
        assert_eq!(back.0.metadata, serde_json::Value::Null);
    }
}
