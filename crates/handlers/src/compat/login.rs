// Copyright 2022 The Matrix.org Foundation C.I.C.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use axum::{response::IntoResponse, Extension, Json};
use hyper::StatusCode;
use mas_config::MatrixConfig;
use mas_data_model::{Device, TokenType};
use mas_storage::compat::compat_login;
use rand::thread_rng;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use thiserror::Error;

use super::MatrixError;

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
enum LoginType {
    #[serde(rename = "m.login.password")]
    Password,
}

#[derive(Debug, Serialize, Deserialize)]
struct LoginTypes {
    flows: Vec<LoginType>,
}

pub(crate) async fn get() -> impl IntoResponse {
    let res = LoginTypes {
        flows: vec![LoginType::Password],
    };

    Json(res)
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum RequestBody {
    #[serde(rename = "m.login.password")]
    Password {
        identifier: Identifier,
        password: String,
    },

    #[serde(other)]
    Unsupported,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Identifier {
    #[serde(rename = "m.id.user")]
    User { user: String },

    #[serde(other)]
    Unsupported,
}

#[derive(Debug, Serialize)]
pub struct ResponseBody {
    access_token: String,
    device_id: Device,
    user_id: String,
}

#[derive(Debug, Error)]
pub enum RouteError {
    #[error(transparent)]
    Internal(Box<dyn std::error::Error + Send + Sync + 'static>),

    #[error("unsupported login method")]
    Unsupported,

    #[error("login failed")]
    LoginFailed,
}

impl From<sqlx::Error> for RouteError {
    fn from(e: sqlx::Error) -> Self {
        Self::Internal(Box::new(e))
    }
}

impl IntoResponse for RouteError {
    fn into_response(self) -> axum::response::Response {
        match self {
            Self::Internal(_e) => MatrixError {
                errcode: "M_UNKNOWN",
                error: "Internal server error",
                status: StatusCode::INTERNAL_SERVER_ERROR,
            },
            Self::Unsupported => MatrixError {
                errcode: "M_UNRECOGNIZED",
                error: "Invalid login type",
                status: StatusCode::BAD_REQUEST,
            },
            Self::LoginFailed => MatrixError {
                errcode: "M_UNAUTHORIZED",
                error: "Invalid username/password",
                status: StatusCode::FORBIDDEN,
            },
        }
        .into_response()
    }
}

#[tracing::instrument(skip_all, err)]
pub(crate) async fn post(
    Extension(pool): Extension<PgPool>,
    Extension(config): Extension<MatrixConfig>,
    Json(input): Json<RequestBody>,
) -> Result<impl IntoResponse, RouteError> {
    let mut conn = pool.acquire().await?;
    let (username, password) = match input {
        RequestBody::Password {
            identifier: Identifier::User { user },
            password,
        } => (user, password),
        _ => {
            return Err(RouteError::Unsupported);
        }
    };

    let (token, device) = {
        let mut rng = thread_rng();
        let token = TokenType::CompatAccessToken.generate(&mut rng);
        let device = Device::generate(&mut rng);
        (token, device)
    };

    let (token, session) = compat_login(&mut conn, &username, &password, device, token)
        .await
        .map_err(|_| RouteError::LoginFailed)?;

    let user_id = format!("@{}:{}", session.user.username, config.homeserver);

    Ok(Json(ResponseBody {
        access_token: token.token,
        device_id: session.device,
        user_id,
    }))
}