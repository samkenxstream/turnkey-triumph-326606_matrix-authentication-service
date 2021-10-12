// Copyright 2021 The Matrix.org Foundation C.I.C.
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

use chrono::{DateTime, Duration, Utc};
use oauth2_types::{pkce::CodeChallengeMethod, scope::Scope};
use serde::Serialize;

pub trait StorageBackend {
    type UserData: Clone + std::fmt::Debug + PartialEq;
    type AuthenticationData: Clone + std::fmt::Debug + PartialEq;
    type BrowserSessionData: Clone + std::fmt::Debug + PartialEq;
    type ClientData: Clone + std::fmt::Debug + PartialEq;
    type SessionData: Clone + std::fmt::Debug + PartialEq;
    type AuthorizationCodeData: Clone + std::fmt::Debug + PartialEq;
    type AccessTokenData: Clone + std::fmt::Debug + PartialEq;
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct User<T: StorageBackend> {
    #[serde(skip_serializing)]
    pub data: T::UserData,
    pub username: String,
    pub sub: String,
}

impl<T: StorageBackend> User<T>
where
    T::UserData: Default,
{
    pub fn samples() -> Vec<Self> {
        vec![User {
            data: Default::default(),
            username: "john".to_string(),
            sub: "123-456".to_string(),
        }]
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Authentication<T: StorageBackend> {
    #[serde(skip_serializing)]
    pub data: T::AuthenticationData,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BrowserSession<T: StorageBackend> {
    #[serde(skip_serializing)]
    pub data: T::BrowserSessionData,
    pub user: User<T>,
    pub created_at: DateTime<Utc>,
    pub last_authentication: Option<Authentication<T>>,
}

impl<T: StorageBackend> BrowserSession<T>
where
    T::BrowserSessionData: Default,
    T::UserData: Default,
{
    pub fn samples() -> Vec<Self> {
        User::<T>::samples()
            .into_iter()
            .map(|user| BrowserSession {
                data: Default::default(),
                user,
                created_at: Utc::now(),
                last_authentication: None,
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Client<T: StorageBackend> {
    #[serde(skip_serializing)]
    pub data: T::ClientData,
    pub client_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Session<T: StorageBackend> {
    #[serde(skip_serializing)]
    pub data: T::SessionData,
    pub browser_session: Option<BrowserSession<T>>,
    pub client: Client<T>,
    pub scope: Scope,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Pkce {
    challenge_method: CodeChallengeMethod,
    challenge: String,
}

impl Pkce {
    pub fn new(challenge_method: CodeChallengeMethod, challenge: String) -> Self {
        Pkce {
            challenge_method,
            challenge,
        }
    }

    pub fn verify(&self, verifier: &str) -> bool {
        self.challenge_method.verify(&self.challenge, verifier)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AuthorizationCode<T: StorageBackend> {
    #[serde(skip_serializing)]
    pub data: T::AuthorizationCodeData,
    pub code: String,
    pub pkce: Pkce,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessToken<T: StorageBackend> {
    pub data: T::AccessTokenData,
    pub jti: String,
    pub token: String,
    pub expires_after: Duration,
    pub created_at: DateTime<Utc>,
}