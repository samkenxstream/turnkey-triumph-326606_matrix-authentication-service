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

use lettre::{message::Mailbox, Address};
use mas_config::{CookiesConfig, CsrfConfig};
use mas_data_model::BrowserSession;
use mas_email::Mailer;
use mas_storage::{
    user::{
        add_user_email, get_user_email, get_user_emails, remove_user_email,
        set_user_email_as_primary,
    },
    PostgresqlBackend,
};
use mas_templates::{AccountEmailsContext, EmailVerificationContext, TemplateContext, Templates};
use mas_warp_utils::{
    errors::WrapError,
    filters::{
        cookies::{encrypted_cookie_saver, EncryptedCookieSaver},
        csrf::{protected_form, updated_csrf_token},
        database::{connection, transaction},
        session::session,
        with_templates, CsrfToken,
    },
};
use serde::Deserialize;
use sqlx::{pool::PoolConnection, PgExecutor, PgPool, Postgres, Transaction};
use tracing::info;
use url::Url;
use warp::{filters::BoxedFilter, reply::html, Filter, Rejection, Reply};

pub(super) fn filter(
    pool: &PgPool,
    templates: &Templates,
    mailer: &Mailer,
    csrf_config: &CsrfConfig,
    cookies_config: &CookiesConfig,
) -> BoxedFilter<(Box<dyn Reply>,)> {
    let mailer = mailer.clone();

    let get = with_templates(templates)
        .and(encrypted_cookie_saver(cookies_config))
        .and(updated_csrf_token(cookies_config, csrf_config))
        .and(session(pool, cookies_config))
        .and(connection(pool))
        .and_then(get);

    let post = with_templates(templates)
        .and(warp::any().map(move || mailer.clone()))
        .and(encrypted_cookie_saver(cookies_config))
        .and(updated_csrf_token(cookies_config, csrf_config))
        .and(session(pool, cookies_config))
        .and(transaction(pool))
        .and(protected_form(cookies_config))
        .and_then(post);

    let get = warp::get().and(get);
    let post = warp::post().and(post);
    let filter = get.or(post).unify();

    warp::path!("emails").and(filter).boxed()
}

#[derive(Deserialize, Debug)]
#[serde(tag = "action", rename_all = "snake_case")]
enum Form {
    Add { email: String },
    ResendConfirmation { data: String },
    SetPrimary { data: String },
    Remove { data: String },
}

async fn get(
    templates: Templates,
    cookie_saver: EncryptedCookieSaver,
    csrf_token: CsrfToken,
    session: BrowserSession<PostgresqlBackend>,
    mut conn: PoolConnection<Postgres>,
) -> Result<Box<dyn Reply>, Rejection> {
    render(templates, cookie_saver, csrf_token, session, &mut conn).await
}

async fn render(
    templates: Templates,
    cookie_saver: EncryptedCookieSaver,
    csrf_token: CsrfToken,
    session: BrowserSession<PostgresqlBackend>,
    executor: impl PgExecutor<'_>,
) -> Result<Box<dyn Reply>, Rejection> {
    let emails = get_user_emails(executor, &session.user)
        .await
        .wrap_error()?;

    let ctx = AccountEmailsContext::new(emails)
        .with_session(session)
        .with_csrf(csrf_token.form_value());

    let content = templates.render_account_emails(&ctx).await?;
    let reply = html(content);
    let reply = cookie_saver.save_encrypted(&csrf_token, reply)?;

    Ok(Box::new(reply))
}

async fn post(
    templates: Templates,
    mailer: Mailer,
    cookie_saver: EncryptedCookieSaver,
    csrf_token: CsrfToken,
    mut session: BrowserSession<PostgresqlBackend>,
    mut txn: Transaction<'_, Postgres>,
    form: Form,
) -> Result<Box<dyn Reply>, Rejection> {
    match form {
        Form::Add { email } => {
            // TODO: verify email format
            // TODO: send verification email
            add_user_email(&mut txn, &session.user, email)
                .await
                .wrap_error()?;
        }
        Form::Remove { data } => {
            let id = data.parse().wrap_error()?;
            let email = get_user_email(&mut txn, &session.user, id)
                .await
                .wrap_error()?;
            remove_user_email(&mut txn, email).await.wrap_error()?;
        }
        Form::ResendConfirmation { data } => {
            let id: i64 = data.parse().wrap_error()?;

            let email: Address = get_user_email(&mut txn, &session.user, id)
                .await
                .wrap_error()?
                .email
                .parse()
                .wrap_error()?;

            let mailbox = Mailbox::new(Some(session.user.username.clone()), email);

            // TODO: actually generate a verification link
            let context = EmailVerificationContext::new(
                session.user.clone().into(),
                Url::parse("https://example.com/verify").unwrap(),
            );

            mailer
                .send_verification_email(mailbox, &context)
                .await
                .wrap_error()?;

            info!(email.id = id, "Verification email sent");
        }
        Form::SetPrimary { data } => {
            let id = data.parse().wrap_error()?;
            let email = get_user_email(&mut txn, &session.user, id)
                .await
                .wrap_error()?;
            set_user_email_as_primary(&mut txn, &email)
                .await
                .wrap_error()?;
            session.user.primary_email = Some(email);
        }
    };

    let reply = render(templates, cookie_saver, csrf_token, session, &mut txn).await?;

    txn.commit().await.wrap_error()?;

    Ok(reply)
}