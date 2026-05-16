use serde::Deserialize;

use crate::auth::Role;

#[derive(Deserialize)]
pub(super) struct MemberRequest {
    pub name: Option<String>,
    pub role: Option<Role>,
    pub photo_url: Option<String>,
    pub photo_path: Option<String>,
}
