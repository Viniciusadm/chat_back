use serde::Deserialize;

#[derive(Deserialize)]
pub(super) struct PasswordSettingsRequest {
    pub password_salt: Option<String>,
    pub password_verifier_ciphertext: Option<String>,
    pub password_verifier_iv: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct KeyBackupRequest {
    pub ciphertext: String,
    pub iv: String,
    pub enc_version: i32,
}

#[derive(Deserialize)]
pub(super) struct KeyShareRequest {
    pub ephemeral_public_key: String,
    pub iv: String,
    pub ciphertext: String,
}
