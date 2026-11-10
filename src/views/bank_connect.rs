use crate::{
    models::bank_sync::{AspspInfo, ProviderBankAccountInfo},
    views::bank_account_new::ManualBankAccountFields,
};
use dioxus::prelude::*;
use uuid::Uuid;

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

        let (storage, cookies) = extract_context().await?;
        require_auth(&storage, &cookies).await?;

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
        use crate::{
            server::auth::{extract_context, require_auth},
            storage::bank_auth_states::BankAuthState,
        };

        let (storage, cookies) = extract_context().await?;
        let user_id = require_auth(&storage, &cookies).await?;

        if let Some(source_bank_account_id) = source_bank_account_id {
            let source = storage
                .bank_accounts
                .find(user_id, source_bank_account_id)
                .await?
                .ok_or_else(|| ServerFnError::new("Bank account not found"))?;

            if !source.is_manual {
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

        storage
            .bank_auth_states
            .create(BankAuthState {
                user_id,
                provider_id,
                state,
                aspsp_name: Some(institution_name),
                aspsp_country: Some(institution_country),
                provider_reference,
                source_bank_account_id,
                expires_at,
            })
            .await?;

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

        let (storage, cookies) = extract_context().await?;
        let user_id = require_auth(&storage, &cookies).await?;

        return Ok(storage
            .bank_connections
            .find(user_id, connection_id)
            .await?
            .and_then(|connection| connection.source_bank_account_id));
    }

    #[allow(unreachable_code)]
    Ok(None)
}

// ── Axum callback handler (server-only) ───────────────────────────────────────

#[cfg(feature = "server")]
pub mod callback {
    use axum::response::{IntoResponse, Redirect, Response};

    use crate::storage::{bank_connections::NewBankConnection, Storage};

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
        axum::extract::Extension(storage): axum::extract::Extension<Storage>,
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

            match storage.sessions.find_active_user(&token).await {
                Ok(Some(user_id)) => user_id,
                _ => return Redirect::temporary("/login").into_response(),
            }
        };

        let auth_state = match storage.bank_auth_states.consume(user_id, &state).await {
            Ok(Some(auth_state)) => auth_state,
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

        let provider = match crate::bank_sync::get_provider(&auth_state.provider_id).await {
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

        let completion_code = if auth_state.provider_id == "gocardless" {
            match auth_state.provider_reference {
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

        let connection = NewBankConnection {
            user_id,
            provider_id: auth_state.provider_id,
            provider_session_id: session_id,
            aspsp_name: auth_state.aspsp_name,
            aspsp_country: auth_state.aspsp_country,
            access_valid_until: chrono::Utc::now() + chrono::Duration::days(90),
            provider_accounts: accounts,
            source_bank_account_id: auth_state.source_bank_account_id,
        };

        let connection_id = match storage.bank_connections.create(connection).await {
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

        let (storage, cookies) = extract_context().await?;
        let user_id = require_auth(&storage, &cookies).await?;

        return Ok(storage
            .bank_accounts
            .list_for_connection(user_id, connection_id)
            .await?);
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
        use crate::{
            server::auth::{extract_context, require_auth},
            storage::bank_accounts::ProviderAccount,
        };

        let (storage, cookies) = extract_context().await?;
        let user_id = require_auth(&storage, &cookies).await?;

        storage
            .bank_connections
            .find(user_id, connection_id)
            .await?
            .ok_or_else(|| ServerFnError::new("Connection not found"))?;

        return Ok(storage
            .bank_accounts
            .map_provider_account(
                user_id,
                connection_id,
                ProviderAccount {
                    uid: provider_account_uid,
                    name: provider_account_name,
                    iban: provider_account_iban,
                    currency: provider_account_currency,
                },
                internal_account_id,
            )
            .await?);
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
        use crate::{
            server::auth::{extract_context, require_auth},
            storage::bank_accounts::ProviderAccount,
        };

        let (storage, cookies) = extract_context().await?;
        let user_id = require_auth(&storage, &cookies).await?;

        let existing = storage
            .bank_accounts
            .find(user_id, bank_account_id)
            .await?
            .ok_or_else(|| ServerFnError::new("Bank account not found"))?;

        if !existing.is_manual {
            return Err(ServerFnError::new(
                "Only manual bank accounts can be connected",
            ));
        }

        let source_bank_account_id = storage
            .bank_connections
            .find(user_id, connection_id)
            .await?
            .and_then(|connection| connection.source_bank_account_id);

        if source_bank_account_id != Some(bank_account_id) {
            return Err(ServerFnError::new(
                "This connection was not started for that bank account",
            ));
        }

        storage
            .bank_accounts
            .connect_to_provider(
                user_id,
                bank_account_id,
                connection_id,
                ProviderAccount {
                    uid: provider_account_uid,
                    name: provider_account_name,
                    iban: provider_account_iban,
                    currency: provider_account_currency,
                },
            )
            .await?;

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
                label { class: "field-label", r#for: "country-filter", "Country" }
                select {
                    id: "country-filter",
                    class: "select",
                    value: country_filter,
                    onchange: move |e| country_filter.set(e.value()),
                    option { value: "", "All countries" }
                    option { value: "AT", "Austria (AT)" }
                    option { value: "BE", "Belgium (BE)" }
                    option { value: "BG", "Bulgaria (BG)" }
                    option { value: "HR", "Croatia (HR)" }
                    option { value: "CZ", "Czech Republic (CZ)" }
                    option { value: "DK", "Denmark (DK)" }
                    option { value: "EE", "Estonia (EE)" }
                    option { value: "FI", "Finland (FI)" }
                    option { value: "FR", "France (FR)" }
                    option { value: "DE", "Germany (DE)" }
                    option { value: "GR", "Greece (GR)" }
                    option { value: "HU", "Hungary (HU)" }
                    option { value: "IS", "Iceland (IS)" }
                    option { value: "IE", "Ireland (IE)" }
                    option { value: "IT", "Italy (IT)" }
                    option { value: "LV", "Latvia (LV)" }
                    option { value: "LI", "Liechtenstein (LI)" }
                    option { value: "LT", "Lithuania (LT)" }
                    option { value: "LU", "Luxembourg (LU)" }
                    option { value: "MT", "Malta (MT)" }
                    option { value: "NL", "Netherlands (NL)" }
                    option { value: "NO", "Norway (NO)" }
                    option { value: "PL", "Poland (PL)" }
                    option { value: "PT", "Portugal (PT)" }
                    option { value: "RO", "Romania (RO)" }
                    option { value: "SK", "Slovakia (SK)" }
                    option { value: "SI", "Slovenia (SI)" }
                    option { value: "ES", "Spain (ES)" }
                    option { value: "SE", "Sweden (SE)" }
                    option { value: "GB", "United Kingdom (GB)" }
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
            div { class: "message message-info", "Select a country to load GoCardless institutions." }
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
    let mut selections: Signal<
        Vec<(String, Option<String>, Option<String>, String, String, bool)>,
    > = use_signal(|| {
        provider_accounts
            .iter()
            .map(|a| {
                (
                    a.uid.clone(),
                    a.name.clone(),
                    a.iban.clone(),
                    a.currency.clone(),
                    String::new(),
                    a.already_linked,
                )
            })
            .collect()
    });
    let mut selected_uid: Signal<Option<String>> = use_signal(|| {
        if source_bank_account_id.is_some() {
            provider_accounts.first().map(|a| a.uid.clone())
        } else {
            None
        }
    });

    let has_linked = if source_bank_account_id.is_some() {
        false
    } else {
        provider_accounts.iter().any(|a| a.already_linked)
    };

    let handle_save = move |_| async move {
        mapping_error.set(None);
        let sels = selections();

        if let Some(source_bank_account_id) = source_bank_account_id {
            let uid = match selected_uid() {
                Some(uid) => uid,
                None => {
                    mapping_error.set(Some("Please select a bank account to link.".to_string()));
                    return;
                }
            };

            let entry = sels.into_iter().find(|s| s.0 == uid);
            let (_, name, iban, currency, _, _) = match entry {
                Some(e) => e,
                None => {
                    mapping_error.set(Some("Selected bank account not found.".to_string()));
                    return;
                }
            };

            match link_existing_bank_account(
                connection_id,
                source_bank_account_id,
                uid,
                name,
                iban,
                currency,
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

        for (uid, name, iban, currency, internal_id_str, already_linked) in &sels {
            if *already_linked {
                continue;
            }

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

                if has_linked {
                    div { class: "message message-warning",
                        "Some bank accounts are already linked to another account and will be skipped when saving."
                    }
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
                                    div {
                                        class: if source_bank_account_id.is_some() { "form-grid" } else { "form-grid two-up" },
                                        style: "padding: 1rem",
                                        div { class: "field-block",
                                            div { class: "flex-row gap-sm align-center",
                                                div { class: "stack-sm",
                                                    span { class: "label-strong",
                                                        if source_bank_account_id.is_some() {
                                                            input {
                                                                r#type: "radio",
                                                                name: "provider-account",
                                                                value: "{account.uid}",
                                                                checked: selected_uid() == Some(account.uid.clone()),
                                                                oninput: move |e| selected_uid.set(Some(e.value())),
                                                            }
                                                        }
                                                        {account.name.as_deref().unwrap_or("Unnamed")}
                                                        if account.already_linked {
                                                            span { class: "chip chip-warning",
                                                                "Already linked"
                                                            }
                                                        }
                                                    }
                                                    if let Some(iban) = account.iban.as_deref() {
                                                        span { class: "tiny-text mono muted",
                                                            "{iban}"
                                                        }
                                                    }
                                                    span { class: "tiny-text muted",
                                                        "{account.currency}"
                                                    }
                                                }
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
        use crate::server::auth::{extract_context, require_auth};

        let (storage, cookies) = extract_context().await?;
        let user_id = require_auth(&storage, &cookies).await?;

        let accounts = storage
            .bank_connections
            .find(user_id, connection_id)
            .await?
            .ok_or_else(|| ServerFnError::new("Connection not found"))?
            .provider_accounts;

        // Determine which provider UIDs are already linked on other connections.
        let uids: Vec<String> = accounts.iter().map(|a| a.uid.clone()).collect();
        let linked_uids = if uids.is_empty() {
            Vec::new()
        } else {
            storage
                .bank_accounts
                .linked_provider_uids(user_id, uids, connection_id)
                .await?
        };

        return Ok(accounts
            .into_iter()
            .map(|account| ProviderBankAccountInfo {
                already_linked: linked_uids.contains(&account.uid),
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
