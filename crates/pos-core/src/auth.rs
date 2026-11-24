use argon2::{Algorithm, Argon2, Params, Version};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{DateTime, Duration, Utc};
use hmac::{Hmac, Mac};
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use thiserror::Error;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("Account is temporarily locked until {0} due to repeated failed PIN attempts")]
    AccountLocked(String),
    #[error("Invalid operator PIN")]
    InvalidCredentials,
    #[error("Malformed or untrusted session token")]
    InvalidToken,
    #[error("Session or override token has expired")]
    TokenExpired,
    #[error("Insufficient role permission: {0:?} required")]
    Forbidden(Permission),
    #[error("Manager step-up approval required for action: {0}")]
    ManagerOverrideRequired(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Owner,
    Manager,
    Bartender,
    Server,
    Customer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    ManageTheme,
    ManageCatalog,
    ManageMods,
    ViewAuditLog,
    ManageAllOrders,
    QuickBarService,
    FloorTableService,
    VoidUnfiredItem,
    VoidFiredOrCompItem,
    IssueRefund,
    ManageDrawerShift,
    ApproveManagerOverride,
    CustomerSelfOrder,
    CustomerPayCheck,
}

impl Role {
    pub fn has_permission(&self, perm: Permission) -> bool {
        match self {
            Role::Owner => true,
            Role::Manager => !matches!(perm, Permission::ManageMods),
            Role::Bartender => matches!(
                perm,
                Permission::ManageAllOrders
                    | Permission::QuickBarService
                    | Permission::FloorTableService
                    | Permission::VoidUnfiredItem
                    | Permission::ManageDrawerShift
                    | Permission::CustomerSelfOrder
                    | Permission::CustomerPayCheck
            ),
            Role::Server => matches!(
                perm,
                Permission::ManageAllOrders
                    | Permission::FloorTableService
                    | Permission::VoidUnfiredItem
                    | Permission::CustomerSelfOrder
                    | Permission::CustomerPayCheck
            ),
            Role::Customer => matches!(
                perm,
                Permission::CustomerSelfOrder | Permission::CustomerPayCheck
            ),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperatorAccount {
    pub id: String,
    pub name: String,
    pub role: Role,
    pub pin_salt_hex: String,
    pub pin_argon2_hex: String,
    pub active: bool,
    #[serde(default)]
    pub failed_attempts: u32,
    #[serde(default)]
    pub locked_until: Option<DateTime<Utc>>,
}

impl OperatorAccount {
    pub fn new(id: impl Into<String>, name: impl Into<String>, role: Role, raw_pin: &str) -> Self {
        let id_str = id.into();
        let mut salt = [0u8; 16];
        OsRng.fill_bytes(&mut salt);
        let salt_hex = hex::encode(salt);
        let pin_argon2_hex = Self::derive_argon2id(&id_str, raw_pin, &salt);
        Self {
            id: id_str,
            name: name.into(),
            role,
            pin_salt_hex: salt_hex,
            pin_argon2_hex,
            active: true,
            failed_attempts: 0,
            locked_until: None,
        }
    }

    fn argon2_instance() -> Argon2<'static> {
        // Calibrated interactive POS Argon2id parameters (16 MiB, t=2, p=1)
        let params = Params::new(16 * 1024, 2, 1, Some(32)).expect("valid Argon2id params");
        Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
    }

    pub fn derive_argon2id(account_id: &str, raw_pin: &str, salt: &[u8]) -> String {
        let mut output = [0u8; 32];
        let material = format!("pos-argon2id-v2:{account_id}:{raw_pin}");
        Self::argon2_instance()
            .hash_password_into(material.as_bytes(), salt, &mut output)
            .expect("Argon2id derivation succeeded");
        hex::encode(output)
    }

    /// Verifies the PIN with constant-time equality and updates brute-force lockout state.
    pub fn authenticate_pin(&mut self, raw_pin: &str) -> Result<(), AuthError> {
        if !self.active {
            return Err(AuthError::InvalidCredentials);
        }
        let now = Utc::now();
        if let Some(until) = self.locked_until {
            if now < until {
                return Err(AuthError::AccountLocked(until.to_rfc3339()));
            } else {
                self.locked_until = None;
                self.failed_attempts = 0;
            }
        }

        let salt = hex::decode(&self.pin_salt_hex).unwrap_or_default();
        let candidate_hex = Self::derive_argon2id(&self.id, raw_pin, &salt);

        if constant_time_eq(candidate_hex.as_bytes(), self.pin_argon2_hex.as_bytes()) {
            self.failed_attempts = 0;
            self.locked_until = None;
            Ok(())
        } else {
            self.failed_attempts += 1;
            if self.failed_attempts >= 5 {
                self.locked_until = Some(now + Duration::minutes(5));
            }
            Err(AuthError::InvalidCredentials)
        }
    }

    pub fn default_accounts() -> Vec<Self> {
        vec![
            Self::new("op-owner", "Elena Vance (Store Owner)", Role::Owner, "1111"),
            Self::new("op-manager", "David Park (Floor Manager)", Role::Manager, "9999"),
            Self::new("op-bartender", "Marcus Cole (Bartender / Cashier)", Role::Bartender, "2222"),
            Self::new("op-server", "Sofia Chen (Server / Stylist)", Role::Server, "3333"),
        ]
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionClaims {
    pub jti: String,
    pub actor_id: String,
    pub actor_name: String,
    pub role: Role,
    pub store_id: String,
    /// Required when `role == Role::Customer` (deny-by-default isolation!)
    pub scoped_order_id: Option<String>,
    pub exp_unix: i64,
}

impl SessionClaims {
    /// Deny-by-default order access check:
    /// Customers can ONLY access the exact `order_id` cryptographically bound in their QR token.
    pub fn can_access_order(&self, order_id: &str) -> bool {
        match self.role {
            Role::Owner | Role::Manager | Role::Bartender | Role::Server => true,
            Role::Customer => self
                .scoped_order_id
                .as_deref()
                .map(|scoped| scoped == order_id)
                .unwrap_or(false),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManagerOverrideClaims {
    pub jti: String,
    pub manager_id: String,
    pub manager_name: String,
    pub role: Role,
    pub approved_action: String,
    pub exp_unix: i64,
}

pub struct TokenSigner {
    secret: Vec<u8>,
}

impl TokenSigner {
    pub fn new(secret: impl Into<Vec<u8>>) -> Self {
        Self {
            secret: secret.into(),
        }
    }

    pub fn sign_session(&self, claims: &SessionClaims) -> String {
        self.sign_json("pos_sess", claims)
    }

    pub fn verify_session(&self, token: &str) -> Result<SessionClaims, AuthError> {
        let claims: SessionClaims = self.verify_json("pos_sess", token)?;
        if Utc::now().timestamp() > claims.exp_unix {
            return Err(AuthError::TokenExpired);
        }
        Ok(claims)
    }

    pub fn sign_manager_override(&self, claims: &ManagerOverrideClaims) -> String {
        self.sign_json("pos_mgr", claims)
    }

    pub fn verify_manager_override(&self, token: &str) -> Result<ManagerOverrideClaims, AuthError> {
        let claims: ManagerOverrideClaims = self.verify_json("pos_mgr", token)?;
        if Utc::now().timestamp() > claims.exp_unix {
            return Err(AuthError::TokenExpired);
        }
        if !claims.role.has_permission(Permission::ApproveManagerOverride) {
            return Err(AuthError::Forbidden(Permission::ApproveManagerOverride));
        }
        Ok(claims)
    }

    fn sign_json<T: Serialize>(&self, prefix: &str, payload: &T) -> String {
        let json_bytes = serde_json::to_vec(payload).expect("serializable claims");
        let b64_payload = URL_SAFE_NO_PAD.encode(&json_bytes);
        let signing_input = format!("{prefix}.{b64_payload}");
        let mut mac = HmacSha256::new_from_slice(&self.secret).expect("HMAC key");
        mac.update(signing_input.as_bytes());
        let sig = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());
        format!("{signing_input}.{sig}")
    }

    fn verify_json<T: for<'de> Deserialize<'de>>(
        &self,
        expected_prefix: &str,
        token: &str,
    ) -> Result<T, AuthError> {
        let mut parts = token.splitn(3, '.');
        let prefix = parts.next().ok_or(AuthError::InvalidToken)?;
        let b64_payload = parts.next().ok_or(AuthError::InvalidToken)?;
        let b64_sig = parts.next().ok_or(AuthError::InvalidToken)?;

        if prefix != expected_prefix {
            return Err(AuthError::InvalidToken);
        }

        let signing_input = format!("{prefix}.{b64_payload}");
        let sig_bytes = URL_SAFE_NO_PAD
            .decode(b64_sig)
            .map_err(|_| AuthError::InvalidToken)?;

        let mut mac = HmacSha256::new_from_slice(&self.secret).expect("HMAC key");
        mac.update(signing_input.as_bytes());
        mac.verify_slice(&sig_bytes)
            .map_err(|_| AuthError::InvalidToken)?;

        let json_bytes = URL_SAFE_NO_PAD
            .decode(b64_payload)
            .map_err(|_| AuthError::InvalidToken)?;
        serde_json::from_slice(&json_bytes).map_err(|_| AuthError::InvalidToken)
    }
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (&x, &y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}
