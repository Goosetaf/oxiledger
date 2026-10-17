use std::{cell::RefCell, collections::BTreeSet, rc::Rc};

use crate::{
    models::{
        account::AccountType,
        setup::{
            account_key, AccountTemplate, SetupBankAccountInput, SetupContext, SetupRequest,
            SetupResult, StarterAccount, StarterAccountInput, ACCOUNT_GROUPS, ACCOUNT_TEMPLATES,
            DEFAULT_TEMPLATE_ID, STARTER_ACCOUNTS,
        },
    },
    views::error::LoginRedirectNotice,
};
use dioxus::prelude::*;

/// Returns what the wizard needs to start, or `None` when nobody is signed in.
#[get("/api/setup/context")]
async fn get_setup_context() -> Result<Option<SetupContext>, ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::models::setup::ExistingAccount;
        use crate::server::auth::{extract_context, require_auth};

        let (pool, cookies) = extract_context().await?;
        let user_id = match require_auth(&pool, &cookies).await {
            Ok(user_id) => user_id,
            Err(ServerFnError::ServerError { code, .. })
                if code == StatusCode::UNAUTHORIZED.as_u16() =>
            {
                return Ok(None);
            }
            Err(err) => return Err(err),
        };

        let existing_accounts = sqlx::query_as!(
            ExistingAccount,
            r#"SELECT name, account_type AS "account_type: AccountType"
               FROM accounts
               WHERE user_id = $1
               ORDER BY name"#,
            user_id
        )
        .fetch_all(&pool)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

        let bank_sync_providers = crate::bank_sync::configured_provider_ids();

        return Ok(Some(SetupContext {
            existing_accounts,
            bank_sync_providers,
        }));
    }

    #[allow(unreachable_code)]
    Ok(None)
}

#[post("/api/setup/complete")]
async fn complete_setup(req: SetupRequest) -> Result<SetupResult, ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::server::auth::{extract_context, require_auth};

        let (pool, cookies) = extract_context().await?;
        let user_id = require_auth(&pool, &cookies).await?;

        return crate::server::setup::apply_setup(&pool, user_id, req).await;
    }

    #[allow(unreachable_code)]
    Err(ServerFnError::new("Server only"))
}

/// Where the user is in setup. Quick setup has everything preselected and
/// advanced setup has every option. Both have a bank step, but only on a server
/// with bank sync.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    Start,
    Bank,
    Accounts,
    AdvancedBank,
    Review,
}

/// The stages of each path in order, with their labels in the progress bar.
const QUICK_STAGES: &[(Stage, &str)] = &[(Stage::Start, "Accounts"), (Stage::Bank, "Bank")];
const ADVANCED_STAGES: &[(Stage, &str)] = &[
    (Stage::Accounts, "Accounts"),
    (Stage::AdvancedBank, "Bank"),
    (Stage::Review, "Review"),
];

impl Stage {
    fn is_bank(self) -> bool {
        matches!(self, Stage::Bank | Stage::AdvancedBank)
    }

    /// The path this stage is part of: its name and its stages in order,
    /// leaving out the bank step when `with_bank` is false.
    fn path(self, with_bank: bool) -> (&'static str, Vec<(Stage, &'static str)>) {
        let (name, stages) = match self {
            Stage::Start | Stage::Bank => ("Quick setup", QUICK_STAGES),
            Stage::Accounts | Stage::AdvancedBank | Stage::Review => {
                ("Advanced setup", ADVANCED_STAGES)
            }
        };
        let stages = stages
            .iter()
            .copied()
            .filter(|(stage, _)| with_bank || !stage.is_bank())
            .collect();
        (name, stages)
    }

    /// The next stage on this path, or `None` when this is the last one.
    fn next(self, with_bank: bool) -> Option<Stage> {
        let (_, stages) = self.path(with_bank);
        let index = stages.iter().position(|(stage, _)| *stage == self)?;
        stages.get(index + 1).map(|(stage, _)| *stage)
    }

    /// The previous stage on this path. Advanced setup starts from quick setup.
    fn previous(self, with_bank: bool) -> Stage {
        let (_, stages) = self.path(with_bank);
        match stages.iter().position(|(stage, _)| *stage == self) {
            Some(index) if index > 0 => stages[index - 1].0,
            _ => Stage::Start,
        }
    }
}

/// What the user wants to do about their bank when setup finishes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BankPlan {
    /// Go straight on to connecting a bank for automatic imports.
    Connect,
    /// Create a first bank account to keep track of by hand.
    Manual,
    Later,
}

fn template_accounts(template_id: &str) -> BTreeSet<&'static str> {
    ACCOUNT_TEMPLATES
        .iter()
        .find(|template| template.id == template_id)
        .map(|template| template.accounts.iter().copied().collect())
        .unwrap_or_default()
}

/// Everything the user has chosen so far, shared by quick and advanced setup.
/// Nothing is saved until setup is finished.
#[derive(Clone, Copy)]
struct Wizard {
    context: Signal<SetupContext>,
    stage: Signal<Stage>,
    /// Keys of the selected [`STARTER_ACCOUNTS`].
    selected: Signal<BTreeSet<&'static str>>,
    use_codes: Signal<bool>,
    /// The plan as chosen; see [`Wizard::bank_plan`] for the one in effect.
    chosen_bank_plan: Signal<BankPlan>,
    bank_name: Signal<String>,
    bank_currency: Signal<String>,
    /// Name of the asset account the first bank account is linked to; empty for none.
    bank_link: Signal<String>,
    error: Signal<Option<String>>,
    result: Signal<Option<SetupResult>>,
}

impl Wizard {
    /// Whether the user already has an account like this starter account.
    fn already_exists(&self, account: &StarterAccount) -> bool {
        let key = account_key(account.name, account.account_type);
        self.context
            .read()
            .existing_accounts
            .iter()
            .any(|existing| account_key(&existing.name, existing.account_type) == key)
    }

    /// The selected starter accounts that still need to be created.
    fn new_accounts(&self) -> Vec<&'static StarterAccount> {
        let selected = self.selected.read();
        STARTER_ACCOUNTS
            .iter()
            .filter(|account| selected.contains(account.key) && !self.already_exists(account))
            .collect()
    }

    /// The template the current selection matches exactly, if any.
    fn template(&self) -> Option<&'static AccountTemplate> {
        let selected = self.selected.read();
        ACCOUNT_TEMPLATES.iter().find(|template| {
            template.accounts.len() == selected.len()
                && template.accounts.iter().all(|key| selected.contains(key))
        })
    }

    /// Replace the selection with a template's accounts.
    fn choose_template(&mut self, template: &AccountTemplate) {
        self.selected
            .set(template.accounts.iter().copied().collect());
    }

    /// Display names of the bank sync providers configured on this server.
    fn sync_providers(&self) -> Vec<&'static str> {
        self.context
            .read()
            .bank_sync_providers
            .iter()
            .map(|provider| match provider.as_str() {
                "enable_banking" => "Enable Banking",
                "gocardless" => "GoCardless",
                _ => "your bank sync provider",
            })
            .collect()
    }

    /// Whether setup has a bank step, which needs bank sync on this server.
    fn has_bank_step(&self) -> bool {
        !self.context.read().bank_sync_providers.is_empty()
    }

    /// The bank plan in effect. A bank can only be connected if this server has bank sync.
    fn bank_plan(&self) -> BankPlan {
        let plan = (self.chosen_bank_plan)();
        if plan == BankPlan::Connect && self.context.read().bank_sync_providers.is_empty() {
            BankPlan::Later
        } else {
            plan
        }
    }

    /// The provider to connect through once setup is saved, if that is the plan.
    fn connect_provider(&self) -> Option<String> {
        if self.bank_plan() != BankPlan::Connect {
            return None;
        }
        self.context.read().bank_sync_providers.first().cloned()
    }

    /// Asset accounts the first bank account can be linked to: the ones the user
    /// already has plus the ones about to be created.
    fn linkable_accounts(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .context
            .read()
            .existing_accounts
            .iter()
            .filter(|account| account.account_type == AccountType::Asset)
            .map(|account| account.name.clone())
            .collect();

        for account in self.new_accounts() {
            if account.account_type == AccountType::Asset {
                names.push(account.name.to_string());
            }
        }

        names
    }

    /// The chosen link target, if it is still among the linkable accounts.
    fn linked_account(&self) -> Option<String> {
        let link = self.bank_link.read().clone();
        self.linkable_accounts()
            .into_iter()
            .find(|name| *name == link)
    }

    fn request(&self) -> SetupRequest {
        let use_codes = (self.use_codes)();
        let add_bank_account = self.bank_plan() == BankPlan::Manual;

        SetupRequest {
            accounts: self
                .new_accounts()
                .into_iter()
                .map(|account| StarterAccountInput {
                    name: account.name.to_string(),
                    code: use_codes.then(|| account.code.to_string()),
                    account_type: account.account_type,
                    description: Some(account.description.to_string()),
                })
                .collect(),
            bank_account: add_bank_account.then(|| SetupBankAccountInput {
                name: (self.bank_name)(),
                currency: (self.bank_currency)(),
                linked_account_name: self.linked_account(),
            }),
        }
    }

    /// Returns a message if the first bank account still needs filling in.
    fn validate(&self) -> Option<String> {
        if self.bank_plan() == BankPlan::Manual {
            if self.bank_name.read().trim().is_empty() {
                return Some("Name the bank account.".to_string());
            }
            if self.bank_currency.read().trim().is_empty() {
                return Some("Enter a currency.".to_string());
            }
        }
        None
    }
}

/// Flags the action row while it is pinned to the bottom of the window with
/// more of the page underneath, so that it can say so. The row is styled to
/// sit one pixel past the edge while pinned, which is what this looks for.
const PINNED_JS: &str = r#"
(() => {
    const actions = document.querySelector(".setup-actions");
    if (!actions || !("IntersectionObserver" in window)) return;
    new IntersectionObserver(
        ([entry]) => {
            actions.dataset.pinned = String(entry.intersectionRatio < 1);
        },
        { threshold: 1 },
    ).observe(actions);
})();
"#;

/// The optional setup shown to new users, and again from Settings.
#[component]
pub fn Setup() -> Element {
    let context_future = use_server_future(get_setup_context)?;

    let context = match context_future() {
        Some(Ok(Some(context))) => context,
        Some(Ok(None)) => {
            return rsx! {
                LoginRedirectNotice {
                    title: "Please sign in".to_string(),
                    message: "You need an active session to view this page.".to_string(),
                }
            };
        }
        Some(Err(err)) => return Err(err.into()),
        None => {
            return rsx! {
                div { class: "setup-shell",
                    div { class: "message message-info", "Loading setup..." }
                }
            };
        }
    };

    rsx! {
        SetupWizard { context }
    }
}

#[component]
fn SetupWizard(context: SetupContext) -> Element {
    let nav = use_navigator();
    // Someone without any accounts is setting up for the first time.
    let first_run = context.existing_accounts.is_empty();

    let default_link = STARTER_ACCOUNTS
        .iter()
        .find(|account| account.key == "checking")
        .map(|account| account.name.to_string())
        .unwrap_or_default();
    // New users are offered bank sync up front where the server has it; anyone
    // running setup again decides for themselves.
    let default_bank_plan = if first_run && !context.bank_sync_providers.is_empty() {
        BankPlan::Connect
    } else {
        BankPlan::Later
    };

    let context = use_signal(|| context);
    let stage = use_signal(|| Stage::Start);
    let selected = use_signal(|| template_accounts(DEFAULT_TEMPLATE_ID));
    let use_codes = use_signal(|| true);
    let chosen_bank_plan = use_signal(|| default_bank_plan);
    let bank_name = use_signal(String::new);
    let bank_currency = use_signal(|| "EUR".to_string());
    let bank_link = use_signal(|| default_link);
    let error = use_signal(|| None::<String>);
    let result = use_signal(|| None::<SetupResult>);

    let mut wizard = use_context_provider(|| Wizard {
        context,
        stage,
        selected,
        use_codes,
        chosen_bank_plan,
        bank_name,
        bank_currency,
        bank_link,
        error,
        result,
    });
    let mut submitting = use_signal(|| false);

    // Start each screen at the top with focus on its heading, so keyboard and
    // screen reader users are not left at the bottom of the previous one.
    let mut mounted = use_signal(|| false);
    use_effect(move || {
        let _ = (wizard.stage)();
        let _ = wizard.result.read().is_some();
        if !*mounted.peek() {
            mounted.set(true);
            return;
        }
        document::eval(
            r#"window.scrollTo(0, 0);
            document.getElementById("setup-step-title")?.focus({ preventScroll: true });"#,
        );
    });

    use_effect(move || {
        document::eval(PINNED_JS);
    });

    let handle_advanced = move |_| {
        wizard.error.set(None);
        wizard.stage.set(Stage::Accounts);
    };

    let handle_back = move |_| {
        wizard.error.set(None);
        let previous = (wizard.stage)().previous(wizard.has_bank_step());
        wizard.stage.set(previous);
    };

    let handle_next = move |_| async move {
        let stage = (wizard.stage)();
        wizard.error.set(None);

        // A hand-tracked bank account is filled in on the bank screen, so that is
        // where an incomplete one gets fixed.
        if stage.is_bank() || stage == Stage::Review {
            if let Some(message) = wizard.validate() {
                if stage == Stage::Review {
                    wizard.stage.set(Stage::AdvancedBank);
                }
                wizard.error.set(Some(message));
                return;
            }
        }

        if let Some(next) = stage.next(wizard.has_bank_step()) {
            return wizard.stage.set(next);
        }

        let connect_provider = wizard.connect_provider();
        submitting.set(true);
        match complete_setup(wizard.request()).await {
            Ok(result) => {
                if let Some(provider) = connect_provider {
                    // Straight on to choosing a bank, which leaves this page.
                    nav.push(crate::Route::BankConnect {
                        source_id: None,
                        provider: Some(provider),
                    });
                    return;
                }
                wizard.result.set(Some(result));
            }
            Err(err) => wizard.error.set(Some(err.to_string())),
        }
        submitting.set(false);
    };

    let handle_exit = move |_| {
        nav.push(crate::Route::Dashboard {});
    };

    let stage = (wizard.stage)();
    let with_bank = wizard.has_bank_step();
    let (path_name, path_stages) = stage.path(with_bank);
    let current = path_stages
        .iter()
        .position(|(path_stage, _)| *path_stage == stage)
        .unwrap_or_default();
    let finished = wizard.result.read().is_some();

    let primary_label = if stage.next(with_bank).is_some() {
        "Continue"
    } else if submitting() {
        "Setting up..."
    } else if wizard.bank_plan() == BankPlan::Connect {
        "Finish and connect bank"
    } else {
        "Finish setup"
    };

    rsx! {
        div { class: "setup-shell",
            header { class: "setup-header",
                div { class: "auth-brand",
                    div { class: "auth-brand-mark",
                        img { src: asset!("/assets/header.svg"), alt: "" }
                    }
                    div { class: "brand-copy",
                        span { class: "brand-title", "OxiLedger" }
                        span { class: "brand-subtitle", "{path_name}" }
                    }
                }
                if !finished {
                    button {
                        class: "btn btn-secondary btn-sm",
                        r#type: "button",
                        disabled: submitting(),
                        onclick: handle_exit,
                        if first_run {
                            "Skip setup"
                        } else {
                            "Close"
                        }
                    }
                }
            }

            if finished {
                main { class: "setup-card", SetupDone {} }
            } else {
                if path_stages.len() > 1 {
                    nav { aria_label: "Setup progress",
                        ol { class: "setup-steps",
                            for (index , (_ , label)) in path_stages.iter().enumerate() {
                                li {
                                    class: if index == current { "setup-step is-current" } else if index < current { "setup-step is-done" } else { "setup-step" },
                                    aria_current: if index == current { "step" },
                                    span {
                                        class: "setup-step-index",
                                        aria_hidden: "true",
                                        if index < current {
                                            "✓"
                                        } else {
                                            "{index + 1}"
                                        }
                                    }
                                    span { class: "setup-step-label", "{label}" }
                                }
                            }
                        }
                    }
                }

                main { class: "setup-card",
                    {
                        match stage {
                            Stage::Start => rsx! {
                                StageStart { first_run }
                            },
                            Stage::Bank => rsx! {
                                StageBank { show_link: false }
                            },
                            Stage::Accounts => rsx! {
                                StageAccounts {}
                            },
                            Stage::AdvancedBank => rsx! {
                                StageBank { show_link: true }
                            },
                            Stage::Review => rsx! {
                                StageReview {}
                            },
                        }
                    }

                    if let Some(err) = (wizard.error)() {
                        div { class: "message message-error", role: "alert", "{err}" }
                    }

                    div { class: "setup-actions",
                        span { class: "setup-more", aria_hidden: "true", "More below" }
                        if stage == Stage::Start {
                            button {
                                class: "btn btn-secondary",
                                r#type: "button",
                                disabled: submitting(),
                                onclick: handle_advanced,
                                "Advanced setup"
                            }
                        } else {
                            button {
                                class: "btn btn-secondary",
                                r#type: "button",
                                disabled: submitting(),
                                onclick: handle_back,
                                "Back"
                            }
                        }
                        button {
                            class: "btn btn-primary",
                            r#type: "button",
                            disabled: submitting(),
                            onclick: handle_next,
                            "{primary_label}"
                        }
                    }
                }
            }
        }
    }
}

/// How many accounts are selected, under the advanced account picker.
#[component]
fn CreationSummary() -> Element {
    let wizard = use_context::<Wizard>();
    let count = wizard.new_accounts().len();
    let all_exist = count == 0 && !wizard.selected.read().is_empty();

    rsx! {
        p { class: "setup-summary", role: "status",
            if all_exist {
                "You already have all of these."
            } else if count == 0 {
                "No accounts selected."
            } else if count == 1 {
                "1 account selected."
            } else {
                "{count} accounts selected."
            }
        }
    }
}

/// One type of account on the ledger sheet, such as "Assets".
/// Accounts whose keys are in `added` are briefly highlighted.
#[component]
fn LedgerGroup(
    heading: String,
    accounts: Vec<StarterAccount>,
    added: Vec<&'static str>,
    show_codes: bool,
) -> Element {
    rsx! {
        div { class: "ledger-group",
            h3 { class: "ledger-group-title", "{heading}" }
            ul { class: "ledger-rows",
                for account in accounts {
                    li {
                        key: "{account.key}",
                        class: if added.contains(&account.key) { "ledger-row is-added" } else { "ledger-row" },
                        if show_codes {
                            span { class: "mono", "{account.code}" }
                        }
                        span { "{account.name}" }
                    }
                }
            }
        }
    }
}

/// The accounts setup will create, laid out like a page of a ledger: where
/// money sits on one side, the categories it moves through on the other, each
/// account's code beside its name. Accounts that a change of selection adds
/// while the sheet is on screen are briefly highlighted.
#[component]
fn LedgerSheet() -> Element {
    let wizard = use_context::<Wizard>();
    let new_accounts = wizard.new_accounts();
    let count = new_accounts.len();

    // What the sheet showed last time it rendered; nothing counts as added the
    // first time.
    let shown = use_hook(|| Rc::new(RefCell::new(None::<BTreeSet<&'static str>>)));
    let keys: BTreeSet<&'static str> = new_accounts.iter().map(|account| account.key).collect();
    let added: Vec<&'static str> = match shown.replace(Some(keys.clone())) {
        Some(previous) => keys.difference(&previous).copied().collect(),
        None => Vec::new(),
    };
    let show_codes = (wizard.use_codes)();
    let has_existing = !wizard.context.read().existing_accounts.is_empty();
    let all_exist = count == 0 && !wizard.selected.read().is_empty();

    let group = |account_type: AccountType| {
        let accounts: Vec<StarterAccount> = new_accounts
            .iter()
            .filter(|account| account.account_type == account_type)
            .map(|account| **account)
            .collect();
        let heading = ACCOUNT_GROUPS
            .iter()
            .find(|(group_type, _)| *group_type == account_type)
            .map(|(_, heading)| heading.to_string())
            .unwrap_or_default();
        (!accounts.is_empty()).then_some((heading, accounts))
    };
    let holdings: Vec<_> = [
        AccountType::Asset,
        AccountType::Liability,
        AccountType::Equity,
    ]
    .into_iter()
    .filter_map(group)
    .collect();
    let categories: Vec<_> = [AccountType::Revenue, AccountType::Expense]
        .into_iter()
        .filter_map(group)
        .collect();

    let count_label = match (count, has_existing) {
        (0, _) if all_exist => "Nothing to add".to_string(),
        (0, _) => "No accounts".to_string(),
        (1, false) => "1 account".to_string(),
        (1, true) => "1 new account".to_string(),
        (count, false) => format!("{count} accounts"),
        (count, true) => format!("{count} new accounts"),
    };

    rsx! {
        section { class: "ledger-sheet", aria_labelledby: "ledger-sheet-title",
            header { class: "ledger-sheet-head",
                h2 { id: "ledger-sheet-title", class: "ledger-sheet-title", "Your ledger" }
                p { class: "ledger-sheet-count", role: "status", "{count_label}" }
            }
            if count == 0 {
                p { class: "ledger-sheet-note",
                    if all_exist {
                        "You already have all of these."
                    } else {
                        "You can add accounts later."
                    }
                }
            } else {
                div { class: "ledger-sides",
                    if !holdings.is_empty() {
                        div { class: "ledger-side",
                            for (heading , accounts) in holdings {
                                LedgerGroup {
                                    key: "{heading}",
                                    heading,
                                    accounts,
                                    added: added.clone(),
                                    show_codes,
                                }
                            }
                        }
                    }
                    if !categories.is_empty() {
                        div { class: "ledger-side is-wide",
                            for (heading , accounts) in categories {
                                LedgerGroup {
                                    key: "{heading}",
                                    heading,
                                    accounts,
                                    added: added.clone(),
                                    show_codes,
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// What else finishing setup does, for the review screen.
#[component]
fn SetupSummary() -> Element {
    let wizard = use_context::<Wizard>();
    let has_new_accounts = !wizard.new_accounts().is_empty();
    let bank_plan = wizard.bank_plan();
    let bank_account = wizard.request().bank_account;

    if !has_new_accounts && bank_plan != BankPlan::Connect && bank_account.is_none() {
        return rsx! {};
    }

    rsx! {
        dl { class: "review-list",
            if has_new_accounts {
                div { class: "review-row",
                    dt { "Account codes" }
                    dd {
                        if (wizard.use_codes)() {
                            "Numbered"
                        } else {
                            "None"
                        }
                    }
                }
            }
            if bank_plan == BankPlan::Connect {
                div { class: "review-row",
                    dt { "Bank sync" }
                    dd { "Connect after setup" }
                }
            }
            if let Some(bank_account) = bank_account {
                div { class: "review-row",
                    dt { "First bank account" }
                    dd {
                        "{bank_account.name.trim()} ({bank_account.currency.trim().to_uppercase()})"
                        if let Some(linked) = bank_account.linked_account_name {
                            ", linked to {linked}"
                        }
                    }
                }
            }
        }
    }
}

/// Quick setup, first screen: a starting set of accounts, shown as it is chosen.
#[component]
fn StageStart(first_run: bool) -> Element {
    let mut wizard = use_context::<Wizard>();
    let current = wizard.template().map(|template| template.id);

    rsx! {
        div { class: "setup-intro",
            h1 {
                id: "setup-step-title",
                class: "setup-title",
                tabindex: "-1",
                if first_run {
                    "Set up your ledger"
                } else {
                    "Add accounts"
                }
            }
            p { class: "supporting-text",
                if first_run {
                    "Pick a starting point. You can change everything later."
                } else {
                    "Pick a set to add the accounts you are missing."
                }
            }
        }

        fieldset { class: "setup-fieldset",
            legend { class: "visually-hidden", "Starting point" }
            div { class: "choice-grid",
                for template in ACCOUNT_TEMPLATES {
                    div { class: "choice",
                        label { class: "choice-head",
                            input {
                                r#type: "radio",
                                name: "setup-template",
                                value: template.id,
                                checked: current == Some(template.id),
                                onchange: move |_| wizard.choose_template(template),
                            }
                            span { class: "choice-copy",
                                span { class: "choice-title", "{template.name}" }
                                span { class: "tiny-text", "{template.description}" }
                            }
                            if template.recommended {
                                span { class: "chip chip-positive", "Recommended" }
                            }
                        }
                    }
                }
            }
        }

        LedgerSheet {}
    }
}

/// The bank screen of both quick and advanced setup. Advanced setup also lets
/// the user pick which ledger account a hand-tracked bank account is linked to.
#[component]
fn StageBank(show_link: bool) -> Element {
    rsx! {
        div { class: "setup-intro",
            h1 {
                id: "setup-step-title",
                class: "setup-title",
                tabindex: "-1",
                "Add your bank"
            }
        }

        BankPlanFields { show_link }
    }
}

/// The choice of what to do about the user's bank, on the bank screen of both
/// quick and advanced setup. Only shown when this server has bank sync. `show_link` lets the user pick which ledger
/// account a hand-tracked bank account is linked to; otherwise the default is used.
#[component]
fn BankPlanFields(show_link: bool) -> Element {
    let mut wizard = use_context::<Wizard>();
    let provider_list = wizard.sync_providers().join(" or ");
    let plan = wizard.bank_plan();
    let linkable = wizard.linkable_accounts();
    let linked = wizard.linked_account().unwrap_or_default();

    rsx! {
        fieldset { class: "setup-fieldset",
            legend { class: "visually-hidden", "Your bank" }
            div { class: "choice-grid is-stacked",
                div { class: "choice",
                    label { class: "choice-head",
                        input {
                            r#type: "radio",
                            name: "setup-bank-plan",
                            value: "connect",
                            checked: plan == BankPlan::Connect,
                            onchange: move |_| wizard.chosen_bank_plan.set(BankPlan::Connect),
                        }
                        span { class: "choice-copy",
                            span { class: "choice-title", "Connect my bank" }
                            span { class: "tiny-text", "Import transactions through {provider_list}" }
                        }
                        span { class: "chip chip-positive", "Recommended" }
                    }
                }
                div { class: "choice",
                    label { class: "choice-head",
                        input {
                            r#type: "radio",
                            name: "setup-bank-plan",
                            value: "manual",
                            checked: plan == BankPlan::Manual,
                            onchange: move |_| wizard.chosen_bank_plan.set(BankPlan::Manual),
                        }
                        span { class: "choice-copy",
                            span { class: "choice-title", "Add an account by hand" }
                            span { class: "tiny-text", "Track it yourself" }
                        }
                    }
                    if plan == BankPlan::Manual {
                        div { class: "choice-extra",
                            div { class: if show_link { "form-grid three-up" } else { "form-grid two-up" },
                                div { class: "field-block",
                                    label {
                                        class: "field-label",
                                        r#for: "setup-bank-name",
                                        "Account name"
                                    }
                                    input {
                                        id: "setup-bank-name",
                                        class: "input",
                                        r#type: "text",
                                        placeholder: "Main Checking",
                                        maxlength: 255,
                                        value: wizard.bank_name,
                                        oninput: move |event| wizard.bank_name.set(event.value()),
                                    }
                                }
                                div { class: "field-block",
                                    label {
                                        class: "field-label",
                                        r#for: "setup-bank-currency",
                                        "Currency"
                                    }
                                    input {
                                        id: "setup-bank-currency",
                                        class: "input",
                                        r#type: "text",
                                        placeholder: "EUR",
                                        maxlength: 10,
                                        value: wizard.bank_currency,
                                        oninput: move |event| wizard.bank_currency.set(event.value()),
                                    }
                                }
                                if show_link {
                                    div { class: "field-block",
                                        label {
                                            class: "field-label",
                                            r#for: "setup-bank-link",
                                            "Ledger account"
                                        }
                                        select {
                                            id: "setup-bank-link",
                                            class: "select",
                                            onchange: move |event| wizard.bank_link.set(event.value()),
                                            option {
                                                value: "",
                                                selected: linked.is_empty(),
                                                "Not linked"
                                            }
                                            for name in linkable {
                                                option {
                                                    value: "{name}",
                                                    selected: linked == name,
                                                    "{name}"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            if !show_link && !linked.is_empty() {
                                p { class: "tiny-text", "Linked to {linked}" }
                            }
                        }
                    }
                }
                div { class: "choice",
                    label { class: "choice-head",
                        input {
                            r#type: "radio",
                            name: "setup-bank-plan",
                            value: "later",
                            checked: plan == BankPlan::Later,
                            onchange: move |_| wizard.chosen_bank_plan.set(BankPlan::Later),
                        }
                        span { class: "choice-copy",
                            span { class: "choice-title", "Not now" }
                            span { class: "tiny-text", "Add one later from Bank Accounts" }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn StageAccounts() -> Element {
    let mut wizard = use_context::<Wizard>();
    let current = wizard.template().map(|template| template.id);

    rsx! {
        div { class: "setup-intro",
            h1 {
                id: "setup-step-title",
                class: "setup-title",
                tabindex: "-1",
                "Choose accounts"
            }
        }

        fieldset { class: "setup-fieldset",
            legend { class: "field-label", "Start from" }
            div { class: "account-options",
                for template in ACCOUNT_TEMPLATES {
                    label { class: "account-option",
                        input {
                            r#type: "radio",
                            name: "setup-preset",
                            value: template.id,
                            checked: current == Some(template.id),
                            onchange: move |_| wizard.choose_template(template),
                        }
                        span { "{template.name}" }
                    }
                }
            }
        }

        for (account_type , heading) in ACCOUNT_GROUPS.iter().copied() {
            fieldset { class: "setup-fieldset",
                legend { class: "label-strong", "{heading}" }
                div { class: "account-options",
                    for account in STARTER_ACCOUNTS.iter().filter(move |account| account.account_type == account_type) {
                        AccountOption { account: *account }
                    }
                }
            }
        }

        label { class: "check-row",
            input {
                r#type: "checkbox",
                checked: (wizard.use_codes)(),
                onchange: move |event| wizard.use_codes.set(event.checked()),
            }
            span { class: "check-row-copy",
                span { class: "label-strong", "Number the accounts" }
                span { class: "tiny-text", "Adds codes like 1000 and 5020" }
            }
        }

        CreationSummary {}
    }
}

#[component]
fn AccountOption(account: StarterAccount) -> Element {
    let mut wizard = use_context::<Wizard>();
    let exists = wizard.already_exists(&account);
    let checked = exists || wizard.selected.read().contains(account.key);

    rsx! {
        label {
            class: if exists { "account-option is-disabled" } else { "account-option" },
            title: account.description,
            input {
                r#type: "checkbox",
                checked,
                disabled: exists,
                onchange: move |event| {
                    if event.checked() {
                        wizard.selected.write().insert(account.key);
                    } else {
                        wizard.selected.write().remove(account.key);
                    }
                },
            }
            span { "{account.name}" }
            if exists {
                span { class: "tiny-text", "Already added" }
            }
        }
    }
}

#[component]
fn StageReview() -> Element {
    rsx! {
        div { class: "setup-intro",
            h1 {
                id: "setup-step-title",
                class: "setup-title",
                tabindex: "-1",
                "Review"
            }
            p { class: "supporting-text", "Nothing is saved until you finish." }
        }

        LedgerSheet {}

        SetupSummary {}
    }
}

#[component]
fn SetupDone() -> Element {
    let wizard = use_context::<Wizard>();
    let Some(result) = (wizard.result)() else {
        return rsx! {};
    };

    let sync_provider = wizard.context.read().bank_sync_providers.first().cloned();
    let created = result.created_accounts;

    rsx! {
        div { class: "setup-intro",
            span { class: "setup-done-mark", aria_hidden: "true", "✓" }
            h1 {
                id: "setup-step-title",
                class: "setup-title",
                tabindex: "-1",
                "You're all set"
            }
            p { class: "supporting-text", role: "status",
                if created == 1 {
                    "1 account created."
                } else if created > 1 {
                    "{created} accounts created."
                } else {
                    "No accounts created."
                }
                if result.skipped_accounts > 0 {
                    " {result.skipped_accounts} already existed."
                }
                if result.bank_account_id.is_some() {
                    " Bank account added."
                }
            }
        }

        div { class: "setup-actions",
            if let Some(bank_account_id) = result.bank_account_id {
                if sync_provider.is_some() {
                    Link {
                        class: "btn btn-secondary",
                        to: crate::Route::BankConnect {
                            source_id: Some(bank_account_id),
                            provider: sync_provider.clone(),
                        },
                        "Connect your bank"
                    }
                }
            }
            if created > 0 || !wizard.context.read().existing_accounts.is_empty() {
                Link {
                    class: "btn btn-secondary",
                    to: crate::Route::NewTransaction {},
                    "Record a transaction"
                }
            }
            Link { class: "btn btn-primary", to: crate::Route::Dashboard {}, "Go to dashboard" }
        }
    }
}
