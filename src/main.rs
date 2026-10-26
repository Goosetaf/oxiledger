use dioxus::prelude::*;
use uuid::Uuid;

#[cfg(feature = "server")]
mod bank_sync;
mod components;
mod models;
mod navigation;
mod preferences;
#[cfg(feature = "server")]
mod server;
mod views;

use components::nav::Navbar;
use views::{
    account_edit::EditAccount,
    account_new::NewAccount,
    accounts::Accounts,
    bank_account_detail::BankAccountDetail,
    bank_account_edit::BankAccountEdit,
    bank_account_new::NewBankAccount,
    bank_accounts::BankAccounts,
    bank_connect::{BankAccountMap, BankConnect},
    customize::Customize,
    dashboard::Dashboard,
    error::AppErrorPage,
    login::Login,
    not_found::NotFound,
    oidc::{OidcCallback, OidcLogin},
    register::Register,
    setup::Setup,
    transaction_edit::EditTransaction,
    transaction_new::{NewTransaction, NewTransactionFromBank},
    transactions::Transactions,
};

const FAVICON: Asset = asset!("/assets/favicon.ico");
const MAIN_CSS: Asset = asset!("/assets/styling/main.css");

/// All application routes. Auth routes are full-page; authenticated routes use the Navbar layout.
#[derive(Debug, Clone, Routable, PartialEq)]
#[rustfmt::skip]
enum Route {
    #[layout(AppFrame)]
        // Auth routes — no layout
        #[route("/login")]
        Login {},
        #[route("/register")]
        Register {},
        #[route("/setup")]
        Setup {},
        // OIDC routes — OidcLogin lists configured providers.
        // /auth/oidc/:id/start and /auth/oidc/callback are native axum handlers
        // registered in server_main; they redirect the browser and never reach here.
        #[route("/auth/oidc/login")]
        OidcLogin {},
        #[route("/auth/oidc/callback")]
        OidcCallback {},
        // Authenticated routes — wrapped in Navbar layout
        #[layout(Navbar)]
            #[route("/")]
            Dashboard {},
            #[route("/accounts")]
            Accounts {},
            #[route("/accounts/new")]
            NewAccount {},
            #[route("/accounts/:id/edit")]
            EditAccount { id: Uuid },
            #[route("/transactions")]
            Transactions {},
            #[route("/transactions/new")]
            NewTransaction {},
            #[route("/transactions/new/from-bank/:bank_txn_id")]
            NewTransactionFromBank { bank_txn_id: Uuid },
            #[route("/transactions/:id/edit")]
            EditTransaction { id: Uuid },
            // Bank sync routes
            #[route("/bank-accounts")]
            BankAccounts {},
            #[route("/bank-accounts/new")]
            NewBankAccount {},
            #[route("/bank-accounts/:id")]
            BankAccountDetail { id: Uuid },
            #[route("/bank-accounts/:id/edit")]
            BankAccountEdit { id: Uuid },
            #[route("/bank-accounts/connect?:source_id")]
            BankConnect { source_id: Option<Uuid> },
            #[route("/bank-accounts/connect/map?:connection_id")]
            BankAccountMap { connection_id: Uuid },
            #[route("/customize")]
            Customize {},
        #[route("/:..segments")]
        NotFound { segments: Vec<String> },
}

impl Route {
    pub fn title(&self) -> &'static str {
        match self {
            Route::Login {} => "Sign In",
            Route::Register {} => "Create Account",
            Route::Setup {} => "Set Up",
            Route::OidcLogin {} => "Sign In",
            Route::OidcCallback {} => "Signing In",
            Route::Dashboard {} => "Dashboard Overview",
            Route::Accounts {} => "Accounts",
            Route::NewAccount {} => "Accounts",
            Route::EditAccount { .. } => "Accounts",
            Route::Transactions {} => "Transactions",
            Route::NewTransaction {} => "Transactions",
            Route::NewTransactionFromBank { .. } => "Transactions",
            Route::EditTransaction { .. } => "Transactions",
            Route::BankAccounts {} => "Bank Accounts",
            Route::NewBankAccount {} => "Connect a Bank",
            Route::BankAccountDetail { .. } => "Bank Account",
            Route::BankAccountEdit { .. } => "Bank Account",
            Route::BankConnect { .. } => "Connect a Bank",
            Route::BankAccountMap { .. } => "Map Bank Accounts",
            Route::Customize {} => "Customize",
            Route::NotFound { .. } => "Page Not Found",
        }
    }
}

#[component]
fn App() -> Element {
    rsx! {
        document::Link { rel: "icon", href: FAVICON }
        document::Link { rel: "stylesheet", href: MAIN_CSS }
        // Lets the bottom bar keep clear of the iPhone home indicator
        document::Meta {
            name: "viewport",
            content: "width=device-width, initial-scale=1, viewport-fit=cover",
        }
        // Colours the phone browser's toolbar to match the app
        document::Meta { name: "theme-color", content: "#090b0e" }
        Router::<Route> {}
    }
}

#[component]
fn AppFrame() -> Element {
    rsx! {
        ErrorBoundary {
            handle_error: |errors: ErrorContext| rsx! {
                AppErrorPage { errors }
            },
            SuspenseBoundary {
                fallback: |_| rsx! {
                    div { class: "app-container",
                        div { class: "message message-info", "Loading page..." }
                    }
                },
                Outlet::<Route> {}
            }
        }
    }
}

fn main() {
    #[cfg(not(feature = "server"))]
    dioxus::launch(App);

    #[cfg(feature = "server")]
    server_main();
}

#[cfg(feature = "server")]
fn server_main() {
    use axum::Extension;
    use std::sync::Arc;
    use tower_cookies::CookieManagerLayer;

    dotenvy::dotenv().ok();

    dioxus::serve(|| async move {
        let pool = server::db::connect().await;
        server::db::run_migrations(&pool).await;
        server::bank_sync_cache::start_background_scheduler(pool.clone()).await;

        let oidc_config = Arc::new(server::oidc::load_config().await);

        Ok(dioxus::server::router(App)
            .route(
                "/auth/oidc/{provider_id}/start",
                axum::routing::get(server::oidc::handle_start),
            )
            .route(
                "/auth/oidc/callback",
                axum::routing::get(server::oidc::handle_callback),
            )
            .route(
                "/bank-sync/callback",
                axum::routing::get(views::bank_connect::callback::handle_callback),
            )
            .layer(CookieManagerLayer::new())
            .layer(Extension(oidc_config))
            .layer(Extension(pool)))
    });
}
