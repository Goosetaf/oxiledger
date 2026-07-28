use crate::{
    models::bank_sync::{AspspInfo, ProviderBankAccountInfo},
    views::bank_account_new::ManualBankAccountFields,
};
use dioxus::prelude::*;
use uuid::Uuid;

#[cfg(feature = "server")]
use sqlx::Row;

#[get("/api/bank-sync/providers")]
pub async fn list_bank_sync_providers() -> Result<Vec<String>, ServerFnError> {
    #[cfg(feature = "server")]
    {
        let mut providers = Vec::new();
        if crate::bank_sync::EnableBankingProvider::is_configured() {
            providers.push("enable_banking".to_string());
        }
        if crate::bank_sync::GoCardlessProvider::is_configured() {
            providers.push("gocardless".to_string());
        }
        return Ok(providers);
    }

    #[allow(unreachable_code)]
    Ok(vec![])
}

/// Server function: list ASPSPs, optionally filtered by country.
#[post("/api/bank-sync/aspsps")]
pub async fn list_aspsps(
    provider_id: String,
    country: Option<String>,
) -> Result<Vec<AspspInfo>, ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::server::auth::{extract_context, require_auth};

        let (pool, cookies) = extract_context().await?;
        require_auth(&pool, &cookies).await?;

        return crate::server::bank_sync_cache::list_aspsps_cached(
            &provider_id,
            country.as_deref(),
        )
        .await;
    }

    #[allow(unreachable_code)]
    Ok(vec![])
}

/// Server function: begin authorization for a chosen ASPSP.
/// Returns the redirect URL the browser should navigate to.
#[post("/api/bank-sync/start-auth")]
pub async fn start_bank_auth(
    provider_id: String,
    institution_id: String,
    institution_name: String,
    institution_country: String,
    source_bank_account_id: Option<Uuid>,
) -> Result<String, ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::server::auth::{extract_context, require_auth};

        let (pool, cookies) = extract_context().await?;
        let user_id = require_auth(&pool, &cookies).await?;

        if let Some(source_bank_account_id) = source_bank_account_id {
            let is_manual = sqlx::query_scalar!(
                "SELECT is_manual FROM bank_accounts WHERE id = $1 AND user_id = $2",
                source_bank_account_id,
                user_id
            )
            .fetch_optional(&pool)
            .await
            .map_err(|e| ServerFnError::new(e.to_string()))?
            .ok_or_else(|| ServerFnError::new("Bank account not found"))?;

            if !is_manual {
                return Err(ServerFnError::new(
                    "Only manual bank accounts can be connected to a provider",
                ));
            }
        }

        let provider = crate::bank_sync::get_provider(&provider_id).await?;

        let base_url = std::env::var("BASE_URL").unwrap_or_else(|_| "http://localhost:8080".into());
        let redirect_url = format!("{base_url}/bank-sync/callback");
        let state = Uuid::new_v4().to_string();
        let expires_at = chrono::Utc::now() + chrono::Duration::minutes(15);

        let auth_url = provider
            .start_authorization(&redirect_url, &state, &institution_id, &institution_country)
            .await?;

        let (provider_reference, redirect_url) = if provider_id == "gocardless" {
            let mut parts = auth_url.splitn(2, '|');
            let provider_reference = parts
                .next()
                .map(str::to_string)
                .ok_or_else(|| ServerFnError::new("Missing GoCardless requisition ID"))?;
            let redirect_url = parts
                .next()
                .map(str::to_string)
                .ok_or_else(|| ServerFnError::new("Missing GoCardless redirect URL"))?;
            (Some(provider_reference), redirect_url)
        } else {
            (None, auth_url)
        };

        sqlx::query(
            r#"INSERT INTO bank_auth_states
                (user_id, provider_id, state, aspsp_name, aspsp_country, expires_at, provider_reference, source_bank_account_id)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"#,
        )
        .bind(user_id)
        .bind(provider_id)
        .bind(state)
        .bind(institution_name)
        .bind(institution_country)
        .bind(expires_at)
        .bind(provider_reference)
        .bind(source_bank_account_id)
        .execute(&pool)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

        return Ok(redirect_url);
    }

    #[allow(unreachable_code)]
    Err(ServerFnError::new("Server only"))
}

#[get("/api/bank-sync/connection/:connection_id/source-account")]
async fn get_connection_source_bank_account_id(
    connection_id: Uuid,
) -> Result<Option<Uuid>, ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::server::auth::{extract_context, require_auth};

        let (pool, cookies) = extract_context().await?;
        let user_id = require_auth(&pool, &cookies).await?;

        let source_bank_account_id = sqlx::query_scalar::<_, Option<Uuid>>(
            "SELECT source_bank_account_id FROM bank_connections WHERE id = $1 AND user_id = $2",
        )
        .bind(connection_id)
        .bind(user_id)
        .fetch_optional(&pool)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?
        .flatten();

        return Ok(source_bank_account_id);
    }

    #[allow(unreachable_code)]
    Ok(None)
}

// ── Axum callback handler (server-only) ───────────────────────────────────────

#[cfg(feature = "server")]
pub mod callback {
    use axum::response::{IntoResponse, Redirect, Response};
    use sqlx::PgPool;
    use sqlx::Row;

    #[derive(serde::Deserialize)]
    pub struct BankCallbackParams {
        pub code: Option<String>,
        pub state: Option<String>,
        pub r#ref: Option<String>,
        pub error: Option<String>,
        pub error_description: Option<String>,
    }

    pub async fn handle_callback(
        axum::extract::Query(params): axum::extract::Query<BankCallbackParams>,
        axum::extract::Extension(pool): axum::extract::Extension<PgPool>,
        axum::extract::Extension(cookies): axum::extract::Extension<tower_cookies::Cookies>,
    ) -> Response {
        if let Some(err) = params.error {
            let desc = params
                .error_description
                .unwrap_or_else(|| "Unknown error".into());
            eprintln!("[bank-sync] Provider error in callback: {err}: {desc}");
            return Redirect::temporary(&format!(
                "/bank-accounts?error={}",
                urlencoding::encode(&desc)
            ))
            .into_response();
        }

        let callback_state = params.state.or(params.r#ref);
        let state = match callback_state {
            Some(s) => s,
            None => {
                return Redirect::temporary("/bank-accounts?error=Missing+state+parameter")
                    .into_response()
            }
        };

        let user_id = {
            let token = cookies
                .get(crate::server::auth::SESSION_COOKIE)
                .map(|c| c.value().to_string());

            let token = match token {
                Some(t) => t,
                None => return Redirect::temporary("/login").into_response(),
            };

            let row = sqlx::query!(
                "SELECT user_id FROM sessions WHERE token = $1 AND expires_at > NOW()",
                token
            )
            .fetch_optional(&pool)
            .await;

            match row {
                Ok(Some(r)) => r.user_id,
                _ => return Redirect::temporary("/login").into_response(),
            }
        };

        let state_row = sqlx::query(
            r#"DELETE FROM bank_auth_states
               WHERE state = $1 AND expires_at > NOW() AND user_id = $2
               RETURNING provider_id, aspsp_name, aspsp_country, provider_reference, source_bank_account_id"#,
        )
        .bind(state)
        .bind(user_id)
        .fetch_optional(&pool)
        .await;

        let state_row = match state_row {
            Ok(Some(r)) => r,
            Ok(None) => {
                return Redirect::temporary(
                    "/bank-accounts?error=Invalid+or+expired+authorization+state",
                )
                .into_response()
            }
            Err(e) => {
                eprintln!("[bank-sync] DB error fetching state: {e}");
                return (
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    "Database error",
                )
                    .into_response();
            }
        };

        let provider_id: String = state_row.get("provider_id");
        let aspsp_name: String = state_row.get("aspsp_name");
        let aspsp_country: String = state_row.get("aspsp_country");
        let provider_reference: Option<String> = state_row.get("provider_reference");
        let source_bank_account_id: Option<uuid::Uuid> = state_row.get("source_bank_account_id");

        let provider = match crate::bank_sync::get_provider(&provider_id).await {
            Ok(provider) => provider,
            Err(e) => {
                eprintln!("[bank-sync] Provider config error: {e}");
                return (
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    "Bank sync is not configured",
                )
                    .into_response();
            }
        };

        let completion_code = if provider_id == "gocardless" {
            match provider_reference {
                Some(reference) => reference,
                None => {
                    return Redirect::temporary(
                        "/bank-accounts?error=Missing+GoCardless+requisition+reference",
                    )
                    .into_response();
                }
            }
        } else {
            match params.code {
                Some(code) => code,
                None => {
                    return Redirect::temporary(
                        "/bank-accounts?error=No+authorization+code+received",
                    )
                    .into_response();
                }
            }
        };

        let (session_id, accounts) = match provider.complete_authorization(&completion_code).await {
            Ok(result) => result,
            Err(e) => {
                eprintln!("[bank-sync] Authorization completion failed: {e}");
                return Redirect::temporary(&format!(
                    "/bank-accounts?error={}",
                    urlencoding::encode(&e.to_string())
                ))
                .into_response();
            }
        };

        let access_valid_until = chrono::Utc::now() + chrono::Duration::days(90);
        let provider_accounts_json = match serde_json::to_value(&accounts) {
            Ok(value) => value,
            Err(e) => {
                eprintln!("[bank-sync] Failed to encode provider accounts: {e}");
                return (
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    "Failed to store bank accounts",
                )
                    .into_response();
            }
        };

        let connection_id: uuid::Uuid = match sqlx::query_scalar(
            r#"INSERT INTO bank_connections
                (user_id, provider_id, provider_session_id, aspsp_name, aspsp_country, access_valid_until, provider_accounts_json, source_bank_account_id)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            RETURNING id"#,
        )
        .bind(user_id)
        .bind(provider_id)
        .bind(session_id)
        .bind(aspsp_name)
        .bind(aspsp_country)
        .bind(access_valid_until)
        .bind(provider_accounts_json)
        .bind(source_bank_account_id)
        .fetch_one(&pool)
        .await
        {
            Ok(id) => id,
            Err(e) => {
                eprintln!("[bank-sync] Failed to insert bank_connection: {e}");
                return (
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    "Failed to save bank connection",
                )
                    .into_response();
            }
        };

        Redirect::temporary(&format!(
            "/bank-accounts/connect/map?connection_id={connection_id}"
        ))
        .into_response()
    }
}

#[get("/api/bank-sync/connection/:connection_id/accounts")]
pub async fn list_connection_accounts(
    connection_id: Uuid,
) -> Result<Vec<crate::models::bank_sync::BankAccountRecord>, ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::server::auth::{extract_context, require_auth};

        let (pool, cookies) = extract_context().await?;
        let user_id = require_auth(&pool, &cookies).await?;

        let rows = sqlx::query_as::<_, crate::models::bank_sync::BankAccountRecord>(
            r#"SELECT id, user_id, bank_connection_id, internal_account_id,
                provider_account_uid, iban, name, currency, balance, last_synced_at, created_at
            FROM bank_accounts
            WHERE bank_connection_id = $1 AND user_id = $2"#,
        )
        .bind(connection_id)
        .bind(user_id)
        .fetch_all(&pool)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

        return Ok(rows);
    }

    #[allow(unreachable_code)]
    Ok(vec![])
}

#[post("/api/bank-sync/connection/:connection_id/map")]
pub async fn map_bank_account(
    connection_id: Uuid,
    provider_account_uid: String,
    provider_account_name: Option<String>,
    provider_account_iban: Option<String>,
    provider_account_currency: String,
    internal_account_id: Option<Uuid>,
) -> Result<Uuid, ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::server::auth::{extract_context, require_auth};

        let (pool, cookies) = extract_context().await?;
        let user_id = require_auth(&pool, &cookies).await?;

        let count = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM bank_connections WHERE id = $1 AND user_id = $2",
            connection_id,
            user_id
        )
        .fetch_one(&pool)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?
        .unwrap_or(0);

        if count == 0 {
            return Err(ServerFnError::new("Connection not found"));
        }

        let bank_account_id: Uuid = sqlx::query_scalar(
            r#"INSERT INTO bank_accounts
                (user_id, bank_connection_id, internal_account_id,
                 provider_account_uid, iban, name, currency, is_manual)
            VALUES ($1, $2, $3, $4, $5, $6, $7, FALSE)
            ON CONFLICT (bank_connection_id, provider_account_uid)
                WHERE bank_connection_id IS NOT NULL AND provider_account_uid IS NOT NULL
                DO UPDATE SET
                    internal_account_id = EXCLUDED.internal_account_id,
                    iban = EXCLUDED.iban,
                    name = EXCLUDED.name,
                    currency = EXCLUDED.currency,
                    is_manual = FALSE,
                    last_synced_at = NULL,
                    balance = NULL
            RETURNING id"#,
        )
        .bind(user_id)
        .bind(connection_id)
        .bind(internal_account_id)
        .bind(provider_account_uid)
        .bind(provider_account_iban)
        .bind(provider_account_name)
        .bind(provider_account_currency)
        .fetch_one(&pool)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

        return Ok(bank_account_id);
    }

    #[allow(unreachable_code)]
    Err(ServerFnError::new("Server only"))
}

#[post("/api/bank-sync/connection/:connection_id/link-existing")]
pub async fn link_existing_bank_account(
    connection_id: Uuid,
    bank_account_id: Uuid,
    provider_account_uid: String,
    provider_account_name: Option<String>,
    provider_account_iban: Option<String>,
    provider_account_currency: String,
) -> Result<(), ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::server::auth::{extract_context, require_auth};

        let (pool, cookies) = extract_context().await?;
        let user_id = require_auth(&pool, &cookies).await?;

        let existing = sqlx::query(
            r#"SELECT ba.is_manual, ba.bank_connection_id, bc.source_bank_account_id
            FROM bank_accounts ba
            LEFT JOIN bank_connections bc ON bc.id = $2
            WHERE ba.id = $1 AND ba.user_id = $3"#,
        )
        .bind(bank_account_id)
        .bind(connection_id)
        .bind(user_id)
        .fetch_optional(&pool)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?
        .ok_or_else(|| ServerFnError::new("Bank account not found"))?;

        let is_manual: bool = existing.get("is_manual");
        let source_bank_account_id: Option<Uuid> = existing.get("source_bank_account_id");

        if !is_manual {
            return Err(ServerFnError::new(
                "Only manual bank accounts can be connected",
            ));
        }

        if source_bank_account_id != Some(bank_account_id) {
            return Err(ServerFnError::new(
                "This connection was not started for that bank account",
            ));
        }

        sqlx::query(
            r#"UPDATE bank_accounts
            SET bank_connection_id = $2,
                provider_account_uid = $3,
                iban = COALESCE($4, iban),
                name = COALESCE($5, name),
                currency = $6,
                is_manual = FALSE,
                last_synced_at = NULL,
                balance = NULL
            WHERE id = $1 AND user_id = $7"#,
        )
        .bind(bank_account_id)
        .bind(connection_id)
        .bind(provider_account_uid)
        .bind(provider_account_iban)
        .bind(provider_account_name)
        .bind(provider_account_currency)
        .bind(user_id)
        .execute(&pool)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

        return Ok(());
    }

    #[allow(unreachable_code)]
    Err(ServerFnError::new("Server only"))
}

#[component]
pub fn BankConnect(source_id: Option<Uuid>) -> Element {
    let nav = use_navigator();
    let available_providers_resource = use_server_future(list_bank_sync_providers)?;
    let internal_accounts_resource =
        use_resource(crate::views::transactions::list_accounts_for_txn);

    let mut selected_method = use_signal(|| {
        if source_id.is_some() {
            String::new()
        } else {
            "manual".to_string()
        }
    });
    let mut form_error = use_signal(|| None::<String>);
    let mut manual_submitting = use_signal(|| false);

    let provider_options = match available_providers_resource() {
        Some(Ok(list)) => list,
        Some(Err(err)) => {
            return rsx! {
                div { class: "app-container",
                    div { class: "message message-error", "{err}" }
                }
            };
        }
        None => vec![],
    };

    let internal_accounts = match internal_accounts_resource() {
        Some(Ok(list)) => list,
        Some(Err(_)) | None => vec![],
    };

    let handle_manual_submit = move |fields: ManualBankAccountFields| async move {
        form_error.set(None);
        manual_submitting.set(true);
        match crate::views::bank_account_new::create_manual_bank_account(
            fields.name,
            fields.iban,
            fields.currency,
            fields.internal_account_id,
        )
        .await
        {
            Ok(_) => {
                let _ = nav.push(crate::Route::BankAccounts {});
            }
            Err(err) => {
                form_error.set(Some(err.to_string()));
                manual_submitting.set(false);
            }
        }
    };

    rsx! {
        div { class: "app-container",
            div { class: "page-stack",
                div { class: "section-header",
                    div {
                        h1 { class: "section-title",
                            if source_id.is_some() {
                                "Connect bank account"
                            } else {
                                "Add a bank account"
                            }
                        }
                        p { class: "section-subtitle",
                            if source_id.is_some() {
                                "Connect this manual bank account to a provider."
                            } else {
                                "Choose how you want to add a bank account."
                            }
                        }
                    }
                    Link {
                        class: "btn btn-secondary",
                        to: crate::Route::BankAccounts {},
                        "Cancel"
                    }
                }

                if let Some(err) = form_error() {
                    div { class: "message message-error", "{err}" }
                }

                section { class: "glass-card editor-shell",
                    div { class: "stack-lg",
                        div { class: "field-block",
                            label { class: "field-label", r#for: "connect-method", "Provider" }
                            select {
                                id: "connect-method",
                                class: "select",
                                value: selected_method,
                                onchange: move |e| selected_method.set(e.value()),
                                option {
                                    value: "",
                                    disabled: true,
                                    selected: selected_method().is_empty(),
                                    "Choose a provider"
                                }
                                if source_id.is_none() {
                                    option { value: "manual", "Manual" }
                                }
                                for provider in provider_options.iter() {
                                    option { value: "{provider}",
                                        {if provider == "gocardless" { "GoCardless" } else { "Enable Banking" }}
                                    }
                                }
                            }
                        }
                    }

                    if selected_method() == "manual" && source_id.is_none() {
                        crate::views::bank_account_new::ManualBankAccountForm {
                            internal_accounts,
                            submitting: manual_submitting(),
                            onsubmit: handle_manual_submit,
                        }
                    } else if !selected_method().is_empty() {
                        BankConnectAspsp { provider_id: selected_method(), source_id }
                    }
                }
            }
        }
    }
}

#[component]
fn BankConnectAspsp(provider_id: String, source_id: Option<Uuid>) -> Element {
    let provider_id_for_fetch = provider_id.clone();
    let mut country_filter = use_signal(String::new);
    let mut search_text = use_signal(String::new);
    let selecting = use_signal(|| false);
    let select_error = use_signal(|| None::<String>);

    let aspsps_resource = use_resource(move || {
        let provider_id = provider_id_for_fetch.clone();
        let country = country_filter();
        let country_opt = if country.trim().is_empty() {
            None
        } else {
            Some(country.trim().to_uppercase())
        };

        async move {
            if provider_id == "gocardless" && country_opt.is_none() {
                Ok::<Vec<AspspInfo>, ServerFnError>(vec![])
            } else {
                list_aspsps(provider_id, country_opt).await
            }
        }
    });

    let aspsps = match aspsps_resource() {
        Some(Ok(list)) => list,
        Some(Err(err)) => {
            return rsx! {
                div { class: "message message-error", "Failed to load banks: {err}" }
            };
        }
        None => vec![],
    };

    let search = search_text().to_lowercase();
    let filtered: Vec<_> = aspsps
        .iter()
        .filter(|a| {
            search.is_empty()
                || a.name.to_lowercase().contains(&search)
                || a.country.to_lowercase().contains(&search)
        })
        .collect();

    rsx! {
        if let Some(err) = select_error() {
            div { class: "message message-error", "{err}" }
        }
        if selecting() {
            div { class: "message message-info", "Redirecting to bank authorization..." }
        }

        div { class: "form-grid three-up",
            div { class: "field-block",
                label { class: "field-label", r#for: "country-filter", "Country (ISO)" }
                input {
                    id: "country-filter",
                    class: "input",
                    r#type: "text",
                    placeholder: if provider_id == "gocardless" { "Required, e.g. FI, SE, DE" } else { "e.g. FI, SE, DE" },
                    maxlength: 2,
                    value: country_filter,
                    oninput: move |e| country_filter.set(e.value()),
                }
            }
            div { class: "field-block",
                label { class: "field-label", r#for: "bank-search", "Search" }
                input {
                    id: "bank-search",
                    class: "input",
                    r#type: "text",
                    placeholder: "Search by bank name...",
                    value: search_text,
                    oninput: move |e| search_text.set(e.value()),
                }
            }
        }

        if provider_id == "gocardless" && country_filter().trim().is_empty() {
            div { class: "message message-info", "Choose a country to load GoCardless institutions." }
        } else if aspsps_resource().is_none() {
            div { class: "message message-info", "Loading banks..." }
        } else if filtered.is_empty() {
            div { class: "empty-state",
                p { class: "supporting-text", "No banks found matching your filter." }
            }
        } else {
            table { class: "data-table",
                thead {
                    tr {
                        th { "Bank" }
                        th { "Country" }
                        th { class: "col-right", "Action" }
                    }
                }
                tbody {
                    for aspsp in filtered {
                        tr {
                            td {
                                div { class: "stack-sm",
                                    span { class: "label-strong", "{aspsp.name}" }
                                }
                            }
                            td { class: "mono muted", "{aspsp.country}" }
                            td { class: "col-right",
                                button {
                                    class: "btn btn-primary btn-sm",
                                    r#type: "button",
                                    disabled: selecting(),
                                    onclick: {
                                        let institution_id = aspsp
                                            .institution_id
                                            .clone()
                                            .unwrap_or_else(|| aspsp.name.clone());
                                        let institution_name = aspsp.name.clone();
                                        let institution_country = aspsp.country.clone();
                                        let provider_id = provider_id.clone();
                                        let mut select_error = select_error;
                                        let mut selecting = selecting;
                                        let source_id = source_id;
                                        move |_| {
                                            let provider_id = provider_id.clone();
                                            let institution_id = institution_id.clone();
                                            let institution_name = institution_name.clone();
                                            let institution_country = institution_country.clone();
                                            async move {
                                                select_error.set(None);
                                                selecting.set(true);

                                                match start_bank_auth(
                                                        provider_id.clone(),
                                                        institution_id,
                                                        institution_name,
                                                        institution_country,
                                                        source_id,
                                                    )
                                                    .await
                                                {
                                                    Ok(redirect_url) => {
                                                        #[cfg(feature = "web")]
                                                        {
                                                            use web_sys::window;
                                                            if let Some(win) = window() {
                                                                let _ = win.location().set_href(&redirect_url);
                                                            }
                                                        }
                                                        let _ = redirect_url;
                                                    }
                                                    Err(e) => {
                                                        select_error.set(Some(e.to_string()));
                                                        selecting.set(false);
                                                    }
                                                }
                                            }
                                        }
                                    },
                                    "Connect"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub fn BankAccountMap(connection_id: Uuid) -> Element {
    let nav = use_navigator();
    let internal_accounts_resource = use_loader(crate::views::transactions::list_accounts_for_txn)?;
    let provider_accounts_resource =
        use_server_future(move || get_provider_accounts_for_connection(connection_id))?;
    let source_account_resource =
        use_server_future(move || get_connection_source_bank_account_id(connection_id))?;
    let mut mapping_error = use_signal(|| None::<String>);
    let mut mapping_done = use_signal(|| false);

    let provider_accounts = match provider_accounts_resource() {
        Some(Ok(list)) => list,
        Some(Err(err)) => {
            return rsx! {
                div { class: "app-container",
                    div { class: "message message-error", "Failed to load provider accounts: {err}" }
                }
            };
        }
        None => vec![],
    };

    let source_bank_account_id = match source_account_resource() {
        Some(Ok(value)) => value,
        Some(Err(err)) => {
            return rsx! {
                div { class: "app-container",
                    div { class: "message message-error", "Failed to load connection context: {err}" }
                }
            };
        }
        None => None,
    };

    let internal_accounts = internal_accounts_resource.read().clone();
    let mut selections: Signal<Vec<(String, Option<String>, Option<String>, String, String)>> =
        use_signal(|| {
            provider_accounts
                .iter()
                .map(|a| {
                    (
                        a.uid.clone(),
                        a.name.clone(),
                        a.iban.clone(),
                        a.currency.clone(),
                        String::new(),
                    )
                })
                .collect()
        });

    let handle_save = move |_| async move {
        mapping_error.set(None);
        let sels = selections();

        if let Some(source_bank_account_id) = source_bank_account_id {
            if sels.len() != 1 {
                mapping_error.set(Some("Choose a bank that returns exactly one account when connecting an existing manual account.".to_string()));
                return;
            }

            let (uid, name, iban, currency, _) = &sels[0];
            match link_existing_bank_account(
                connection_id,
                source_bank_account_id,
                uid.clone(),
                name.clone(),
                iban.clone(),
                currency.clone(),
            )
            .await
            {
                Ok(_) => {
                    mapping_done.set(true);
                    let _ = nav.push(crate::Route::BankAccounts {});
                }
                Err(e) => mapping_error.set(Some(e.to_string())),
            }
            return;
        }

        for (uid, name, iban, currency, internal_id_str) in &sels {
            let internal_id = if internal_id_str.is_empty() {
                None
            } else {
                match Uuid::parse_str(internal_id_str) {
                    Ok(id) => Some(id),
                    Err(_) => {
                        mapping_error.set(Some(format!(
                            "Invalid ledger account for: {}",
                            name.as_deref().or(iban.as_deref()).unwrap_or(uid)
                        )));
                        return;
                    }
                }
            };

            match map_bank_account(
                connection_id,
                uid.clone(),
                name.clone(),
                iban.clone(),
                currency.clone(),
                internal_id,
            )
            .await
            {
                Ok(_) => {}
                Err(e) => {
                    mapping_error.set(Some(e.to_string()));
                    return;
                }
            }
        }

        mapping_done.set(true);
        let _ = nav.push(crate::Route::BankAccounts {});
    };

    rsx! {
        div { class: "app-container",
            div { class: "page-stack",
                div { class: "section-header",
                    div {
                        h1 { class: "section-title", "Map bank accounts" }
                        p { class: "section-subtitle",
                            if source_bank_account_id.is_some() {
                                "Choose which provider account should be linked to your manual bank account."
                            } else {
                                "Assign each bank account to a ledger account."
                            }
                        }
                    }
                }

                if let Some(err) = mapping_error() {
                    div { class: "message message-error", "{err}" }
                }

                if provider_accounts.is_empty() {
                    div { class: "message message-info",
                        "No accounts were returned by the bank. Please try reconnecting."
                    }
                } else {
                    section { class: "editor-shell",
                        div { class: "stack-lg",
                            for (i , account) in provider_accounts.iter().enumerate() {
                                div { class: "glass-card",
                                    div { class: "form-grid two-up",
                                        div { class: "field-block",
                                            label { class: "field-label", "Bank account" }
                                            div { class: "stack-sm",
                                                span { class: "label-strong",
                                                    {account.name.as_deref().unwrap_or("Unnamed")}
                                                }
                                                if let Some(iban) = account.iban.as_deref() {
                                                    span { class: "tiny-text mono muted",
                                                        "{iban}"
                                                    }
                                                }
                                                span { class: "tiny-text muted", "{account.currency}" }
                                            }
                                        }
                                        if source_bank_account_id.is_none() {
                                            div { class: "field-block",
                                                label { class: "field-label", "Ledger account" }
                                                select {
                                                    class: "select",
                                                    onchange: move |e| {
                                                        selections.write()[i].4 = e.value();
                                                    },
                                                    option {
                                                        value: "",
                                                        disabled: true,
                                                        selected: selections()[i].4.is_empty(),
                                                        "Select a ledger account"
                                                    }
                                                    for acc in internal_accounts.iter() {
                                                        option {
                                                            value: "{acc.id}",
                                                            selected: selections()[i].4 == acc.id.to_string(),
                                                            {
                                                                match acc.code.as_deref() {
                                                                    Some(code) => format!("{} - {}", code, acc.name),
                                                                    None => acc.name.clone(),
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            div { class: "actions-row justify-end",
                                Link {
                                    class: "btn btn-secondary",
                                    to: crate::Route::BankAccounts {},
                                    "Cancel"
                                }
                                button {
                                    class: "btn btn-primary",
                                    r#type: "button",
                                    disabled: mapping_done(),
                                    onclick: handle_save,
                                    if source_bank_account_id.is_some() {
                                        "Link account"
                                    } else {
                                        "Save mapping"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[get("/api/bank-sync/connection/:connection_id/provider-accounts")]
async fn get_provider_accounts_for_connection(
    connection_id: Uuid,
) -> Result<Vec<ProviderBankAccountInfo>, ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::{
            bank_sync::provider::ProviderBankAccount,
            server::auth::{extract_context, require_auth},
        };

        let (pool, cookies) = extract_context().await?;
        let user_id = require_auth(&pool, &cookies).await?;

        let row = sqlx::query(
            "SELECT provider_accounts_json FROM bank_connections WHERE id = $1 AND user_id = $2",
        )
        .bind(connection_id)
        .bind(user_id)
        .fetch_optional(&pool)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?
        .ok_or_else(|| ServerFnError::new("Connection not found"))?;

        let accounts: Vec<ProviderBankAccount> = row
            .get::<Option<serde_json::Value>, _>("provider_accounts_json")
            .map(serde_json::from_value)
            .transpose()
            .map_err(|e| ServerFnError::new(e.to_string()))?
            .unwrap_or_default();

        return Ok(accounts
            .into_iter()
            .map(|account| ProviderBankAccountInfo {
                uid: account.uid,
                name: account.name,
                iban: account.iban,
                currency: account.currency,
            })
            .collect());
    }

    #[allow(unreachable_code)]
    Ok(vec![])
}
