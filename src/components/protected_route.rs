use dioxus::prelude::*;

#[get("/api/auth/me")]
async fn get_current_user() -> Result<Option<String>, ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::server::auth::{extract_context, require_auth};

        let (pool, cookies) = extract_context().await?;
        match require_auth(&pool, &cookies).await {
            Ok(user_id) => {
                let row = sqlx::query!("SELECT username FROM users WHERE id = $1", user_id)
                    .fetch_optional(&pool)
                    .await
                    .map_err(|e| ServerFnError::new(e.to_string()))?;
                return Ok(row.map(|r| r.username));
            }
            Err(_) => return Ok(None),
        }
    }

    #[cfg(not(feature = "server"))]
    Ok(None)
}

/// Layout component that guards all authenticated routes.
#[component]
pub fn ProtectedRoute() -> Element {
    let user_future = use_server_future(get_current_user)?;
    let nav = use_navigator();

    let username: Option<String> = user_future().and_then(|r| r.ok()).flatten();

    // Once the future has resolved (is Some), check authentication.
    if user_future().is_some() && username.is_none() {
        nav.push(crate::Route::Login {});
        return rsx! {};
    }

    use_context_provider(|| Signal::new(username));

    rsx! {
        Outlet::<crate::Route> {}
    }
}
