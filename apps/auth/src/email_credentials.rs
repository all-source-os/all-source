//! The upstream 0.8 email plugin puts password_hash in User.metadata, which
//! our durable adapter intentionally rejects. Use the credential Account model
//! instead, while retaining better-auth request processing and session creation.
use async_trait::async_trait;
use better_auth_core::{
    AuthContext, AuthError, AuthPlugin, AuthRequest, AuthResponse, AuthResult, AuthRoute,
    CreateAccount, CreateUser, DatabaseAdapter, HttpMethod, SessionManager,
    entity::{AuthAccount, AuthSession, AuthUser},
    utils::{cookie_utils::create_session_cookie, password as password_utils},
};
use serde::Deserialize;
use validator::Validate;

pub struct EmailCredentialsPlugin;

#[derive(Deserialize, Validate)]
struct Credentials {
    #[validate(email)]
    email: String,
    #[validate(length(min = 8, max = 128))]
    password: String,
    #[serde(default)]
    name: String,
}

#[async_trait]
impl<DB: DatabaseAdapter> AuthPlugin<DB> for EmailCredentialsPlugin {
    fn name(&self) -> &'static str {
        "email-credentials"
    }

    fn routes(&self) -> Vec<AuthRoute> {
        vec![
            AuthRoute::post("/sign-up/email", "sign_up_email"),
            AuthRoute::post("/sign-in/email", "sign_in_email"),
        ]
    }

    async fn on_request(
        &self,
        req: &AuthRequest,
        ctx: &AuthContext<DB>,
    ) -> AuthResult<Option<AuthResponse>> {
        if *req.method() != HttpMethod::Post
            || !["/sign-up/email", "/sign-in/email"].contains(&req.path())
        {
            return Ok(None);
        }
        let mut input: Credentials = match better_auth_core::validate_request_body(req) {
            Ok(input) => input,
            Err(response) => return Ok(Some(response)),
        };
        input.email = input.email.trim().to_lowercase();
        let signup = req.path() == "/sign-up/email";
        let existing = ctx.database.get_user_by_email(&input.email).await?;
        let user = if signup {
            if input.name.trim().is_empty() || input.name.len() > 200 {
                return Err(AuthError::bad_request("Name must contain 1–200 characters"));
            }
            if existing.is_some() {
                return Err(AuthError::conflict(
                    "An account already exists. Please sign in.",
                ));
            }
            let hash = password_utils::hash_password(None, &input.password).await?;
            let user = ctx
                .database
                .create_user(
                    CreateUser::new()
                        .with_email(&input.email)
                        .with_name(input.name.trim()),
                )
                .await?;
            let account = CreateAccount {
                user_id: user.id().to_owned(),
                account_id: user.id().to_owned(),
                provider_id: "credential".into(),
                password: Some(hash),
                access_token: None,
                refresh_token: None,
                id_token: None,
                access_token_expires_at: None,
                refresh_token_expires_at: None,
                scope: None,
            };
            if let Err(error) = ctx.database.create_account(account).await {
                // Avoid leaving an identity that can never log in after a
                // partial storage failure. No pre-existing user is touched.
                let _ = ctx.database.delete_user(user.id()).await;
                return Err(error);
            }
            user
        } else {
            let user = existing.ok_or(AuthError::InvalidCredentials)?;
            let account = ctx
                .database
                .get_account("credential", user.id())
                .await?
                .ok_or(AuthError::InvalidCredentials)?;
            let hash = account.password().ok_or(AuthError::InvalidCredentials)?;
            password_utils::verify_password(None, &input.password, hash).await?;
            user
        };
        // Never bypass a configured second factor or an account ban.
        if user.banned() || user.two_factor_enabled() {
            return Err(AuthError::forbidden(
                "Account requires additional verification",
            ));
        }
        let session = SessionManager::new(ctx.config.clone(), ctx.database.clone())
            .create_session(&user, None, None)
            .await?;
        let response = AuthResponse::json(
            200,
            &serde_json::json!({"token":session.token(),"user":user}),
        )?
        .with_header("Set-Cookie", create_session_cookie(session.token(), ctx));
        Ok(Some(response))
    }
}
