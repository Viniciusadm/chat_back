use serde::Deserialize;
use uuid::Uuid;

#[derive(Deserialize)]
pub(super) struct DeviceRequest {
    pub device_id: Uuid,
    pub push_token: Option<String>,
    pub public_key: Option<String>,
}
