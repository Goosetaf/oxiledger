use dioxus::prelude::*;
use uuid::Uuid;

mod components;
mod models;
#[cfg(feature = "server")]
mod server;
mod views;

use components::nav::Navbar;
use views::{
    account_edit::EditAccount,
    account_new::NewAccount,
    accounts::Accounts,
    dashboard::Dashboard,
    error::AppErrorPage,
    login::Login,
    not_found::NotFound,
    oidc::{OidcCallback, OidcLogin},
    register::Register,
    transaction_edit::EditTransaction,
    transaction_new::NewTransaction,
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
            #[route("/transactions/:id/edit")]
            EditTransaction { id: Uuid },
        #[route("/:..segments")]
        NotFound { segments: Vec<String> },
}

#[component]
fn App() -> Element {
    rsx! {
        document::Link { rel: "icon", href: FAVICON }
        document::Link { rel: "stylesheet", href: MAIN_CSS }
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
    {
        tokio::runtime::Runtime::new()
            .expect("Failed to create Tokio runtime")
            .block_on(server_main());
    }
}

/// Custom server startup: initialise the DB pool, run migrations, then start axum.
#[cfg(feature = "server")]
async fn server_main() {
    use axum::Extension;
    use dioxus_server::{DioxusRouterExt, ServeConfig};
    use std::sync::Arc;
    use tower_cookies::CookieManagerLayer;

    dotenvy::dotenv().ok();

    let pool = server::db::connect().await;
    server::db::run_migrations(&pool).await;

    // Load OIDC provider config (performs discovery HTTP calls at startup).
    let oidc_config = Arc::new(server::oidc::load_config().await);

    let cfg = ServeConfig::new();

    let app = axum::Router::new()
        // Native axum routes for the OIDC redirect/callback — these must bypass
        // the Dioxus server function layer so they can issue HTTP redirects.
        .route(
            "/auth/oidc/{provider_id}/start",
            axum::routing::get(server::oidc::handle_start),
        )
        .route(
            "/auth/oidc/callback",
            axum::routing::get(server::oidc::handle_callback),
        )
        .serve_dioxus_application(cfg, App)
        .layer(CookieManagerLayer::new())
        .layer(Extension(oidc_config))
        .layer(Extension(pool));

    let addr: std::net::SocketAddr = std::env::var("OXILEDGER_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:8081".to_string())
        .parse()
        .expect("Invalid OXILEDGER_ADDR");
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("Failed to bind to address");

    axum::serve(listener, app.into_make_service())
        .await
        .expect("Server error");
}
