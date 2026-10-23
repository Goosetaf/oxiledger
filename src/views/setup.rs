use std::{cell::RefCell, collections::BTreeSet, rc::Rc};

use crate::{
    milestones::{record_milestone, Milestone},
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
use dioxus_free_icons::{
    icons::ld_icons::{LdCheck, LdChevronDown},
    Icon,
};

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

/// A screen of quick or advanced setup.
/// The bank screens only show when the server has bank sync.
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

    /// The name and stages of this stage's path, without the bank step unless `with_bank`.
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
    /// The plan the user picked. `bank_plan` gives the one in effect.
    chosen_bank_plan: Signal<BankPlan>,
    bank_name: Signal<String>,
    bank_currency: Signal<String>,
    /// Asset account to link the first bank account to, or empty for none.
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

    /// Asset accounts the first bank account can be linked to.
    /// Includes both existing accounts and those about to be created.
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

/// Marks the action row while it is pinned over more content, so it can show a hint.
/// The row sits one pixel past the window edge while pinned, which is what this detects.
const PINNED_JS: &str = r#"
(() => {
    const actions = document.getElementById("setup-actions");
    if (!actions || !("IntersectionObserver" in window)) return;
    new IntersectionObserver(
        ([entry]) => {
            actions.dataset.pinned = String(entry.intersectionRatio < 1);
        },
        { threshold: 1 },
    ).observe(actions);
})();
"#;

/// Setup sits outside the app shell, so it brings its own phone gutters.
const SHELL: &str = "mx-auto flex min-h-screen w-[min(calc(100%-32px),860px)] flex-col gap-6 pt-8 pb-16 phone:mr-(--gutter-right) phone:ml-(--gutter-left) phone:w-auto phone:pt-5";

/// Unclipped, so the action row can stick to the bottom of the window.
const CARD: &str = "glass-card overflow-visible! flex flex-col gap-6 p-(--setup-pad) [--setup-pad:32px] max-sm:pt-5 max-sm:[--setup-pad:16px]";

/// Sticks to the bottom of the window, out to the card's edges and in its colour made solid.
/// While pinned it sits a pixel past the edge, which is how [`PINNED_JS`] tells, and fades the content above.
const ACTION_ROW: &str = "group sticky -bottom-px z-1 -mx-[calc(var(--setup-pad)-1px)] -mb-[calc(var(--setup-pad)-1px)] flex flex-wrap items-center justify-end gap-3 rounded-b-(--radius-card) border-t border-line-soft bg-page bg-[linear-gradient(var(--glass-bg),var(--glass-bg))] px-[calc(var(--setup-pad)-1px)] pt-4 pb-[17px] phone:pb-[calc(17px+var(--safe-bottom))] before:pointer-events-none before:absolute before:inset-x-0 before:bottom-[calc(100%+1px)] before:h-14 before:[background:inherit] before:opacity-0 before:[mask-image:linear-gradient(to_bottom,transparent,#000)] before:transition-opacity before:duration-160 data-[pinned=true]:before:opacity-100 motion-reduce:before:transition-none";

const FIELDSET: &str = "m-0 flex min-w-0 flex-col gap-2.5 border-0 p-0";

/// A pill with a checkbox or radio, for picking accounts and templates.
const OPTION: &str = "inline-flex items-center gap-2 rounded-full border border-line-soft px-3 py-1.75 text-[0.88rem] transition-colors duration-160 motion-reduce:transition-none";
const OPTION_ENABLED: &str =
    "cursor-pointer bg-white/2 has-checked:border-brand/55 has-checked:bg-brand/8";

const CHECKBOX: &str = "size-4 shrink-0 accent-primary-container focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-primary";

/// Optional setup for new users, also linked from the empty dashboard and accounts pages.
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
                div { class: SHELL,
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
    // Preselect connecting a bank only for new users on a server with bank sync.
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
    let mut skipping = use_signal(|| false);

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

        // An incomplete manual bank account gets fixed on the bank screen,
        // so Review sends the user back there.
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

    let handle_exit = move |_| async move {
        // Skipping counts as done, so the dashboard stops sending them back here
        if first_run {
            wizard.error.set(None);
            skipping.set(true);
            let skipped = record_milestone(Milestone::SetupSkipped).await;
            skipping.set(false);
            if let Err(err) = skipped {
                wizard.error.set(Some(err.to_string()));
                return;
            }
        }
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
    let busy = submitting() || skipping();

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
        div { class: SHELL,
            header { class: "flex items-center justify-between gap-4",
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
                        disabled: busy,
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
                main { class: CARD, SetupDone {} }
            } else {
                if path_stages.len() > 1 {
                    nav { aria_label: "Setup progress",
                        ol { class: "m-0 flex list-none gap-2 p-0",
                            for (index , (_ , label)) in path_stages.iter().enumerate() {
                                li {
                                    class: "flex flex-1 items-center gap-2.5 border-t-2 pt-3 text-[0.82rem] font-semibold max-sm:justify-center",
                                    class: if index == current { "border-brand text-ink" } else if index < current { "border-primary-container/60 text-muted" } else { "border-line-soft text-muted" },
                                    aria_current: if index == current { "step" },
                                    span {
                                        class: "grid size-6 shrink-0 place-items-center rounded-full text-[0.75rem]",
                                        class: if index <= current { "bg-primary-container text-on-primary-container" } else { "bg-white/5" },
                                        aria_hidden: "true",
                                        if index < current {
                                            Icon {
                                                class: "size-3.5",
                                                icon: LdCheck,
                                            }
                                        } else {
                                            "{index + 1}"
                                        }
                                    }
                                    span { class: "max-sm:hidden", "{label}" }
                                }
                            }
                        }
                    }
                }

                main { class: CARD,
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

                    div { id: "setup-actions", class: ACTION_ROW,
                        span {
                            class: "mr-auto hidden items-center gap-2.5 text-[0.82rem] text-soft group-data-[pinned=true]:inline-flex",
                            aria_hidden: "true",
                            Icon {
                                class: "size-5 animate-nudge motion-reduce:animate-none",
                                icon: LdChevronDown,
                            }
                            // Only the chevron on narrow phones, to leave room for the buttons
                            span { class: "max-sm:hidden", "More below" }
                        }
                        if stage == Stage::Start {
                            button {
                                class: "btn btn-secondary",
                                r#type: "button",
                                disabled: busy,
                                onclick: handle_advanced,
                                "Advanced setup"
                            }
                        } else {
                            button {
                                class: "btn btn-secondary",
                                r#type: "button",
                                disabled: busy,
                                onclick: handle_back,
                                "Back"
                            }
                        }
                        button {
                            class: "btn btn-primary",
                            r#type: "button",
                            disabled: busy,
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
        p { class: "m-0 text-[0.9rem] text-soft", role: "status",
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
/// Accounts whose keys are in `added` are briefly highlighted, and `wide` groups list them in two columns.
#[component]
fn LedgerGroup(
    heading: String,
    accounts: Vec<StarterAccount>,
    added: Vec<&'static str>,
    show_codes: bool,
    wide: bool,
) -> Element {
    rsx! {
        div {
            h3 { class: "m-0 pt-3 pb-1.5 text-[0.625rem] font-bold tracking-[0.14em] text-muted uppercase",
                "{heading}"
            }
            ul {
                class: "m-0 list-none p-0",
                class: if wide { "columns-2 gap-x-6 max-sm:columns-1" },
                for account in accounts {
                    // Too narrow for columns of codes on phones, so each group becomes a line of names
                    li {
                        key: "{account.key}",
                        class: "-mx-1.5 grid break-inside-avoid items-baseline gap-x-3 rounded-sm px-1.5 py-0.75 text-[0.86rem] leading-[1.4] max-sm:mx-0 max-sm:inline max-sm:p-0 max-sm:not-last:after:content-[',_']",
                        class: if show_codes { "grid-cols-[4ch_minmax(0,1fr)]" } else { "grid-cols-1" },
                        class: if added.contains(&account.key) { "animate-ledger-row-in motion-reduce:animate-none max-sm:animate-none" },
                        if show_codes {
                            span { class: "mono text-[0.76rem] text-muted max-sm:hidden",
                                "{account.code}"
                            }
                        }
                        span { "{account.name}" }
                    }
                }
            }
        }
    }
}

/// The accounts setup will create, laid out like a ledger page.
/// Accounts added while it is on screen are briefly highlighted.
#[component]
fn LedgerSheet() -> Element {
    let wizard = use_context::<Wizard>();
    let new_accounts = wizard.new_accounts();
    let count = new_accounts.len();

    // Keys shown on the last render. Nothing counts as added on the first one.
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

    // Where money sits on the left, the categories it moves through on the right
    let side = "min-w-0 flex-1 px-4 pt-1 pb-3.5 not-first:border-l not-first:border-line max-sm:not-first:border-l-0 max-sm:not-first:pt-0 max-sm:not-last:pb-0";

    rsx! {
        section {
            class: "rounded-md border border-line-soft bg-[rgba(30,33,39,0.11)]",
            aria_labelledby: "ledger-sheet-title",
            header { class: "flex items-baseline justify-between gap-4 rounded-t-md border-b border-line-soft bg-white/2 px-4 py-3",
                h2 {
                    id: "ledger-sheet-title",
                    class: "m-0 text-[0.9rem] font-semibold",
                    "Your ledger"
                }
                p {
                    class: "m-0 text-[0.82rem] whitespace-nowrap text-soft",
                    role: "status",
                    "{count_label}"
                }
            }
            if count == 0 {
                p { class: "m-0 px-4 py-3 text-[0.82rem] text-muted",
                    if all_exist {
                        "You already have all of these."
                    } else {
                        "You can add accounts later."
                    }
                }
            } else {
                div { class: "flex max-sm:flex-col",
                    if !holdings.is_empty() {
                        div { class: side,
                            for (heading , accounts) in holdings {
                                LedgerGroup {
                                    key: "{heading}",
                                    heading,
                                    accounts,
                                    added: added.clone(),
                                    show_codes,
                                    wide: false,
                                }
                            }
                        }
                    }
                    if !categories.is_empty() {
                        div { class: "{side} grow-2",
                            for (heading , accounts) in categories {
                                LedgerGroup {
                                    key: "{heading}",
                                    heading,
                                    accounts,
                                    added: added.clone(),
                                    show_codes,
                                    wide: true,
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
        dl { class: "m-0 flex flex-col rounded-md border border-line-soft",
            if has_new_accounts {
                ReviewRow { term: "Account codes",
                    if (wizard.use_codes)() {
                        "Numbered"
                    } else {
                        "None"
                    }
                }
            }
            if bank_plan == BankPlan::Connect {
                ReviewRow { term: "Bank sync", "Connect after setup" }
            }
            if let Some(bank_account) = bank_account {
                ReviewRow { term: "First bank account",
                    "{bank_account.name.trim()} ({bank_account.currency.trim().to_uppercase()})"
                    if let Some(linked) = bank_account.linked_account_name {
                        ", linked to {linked}"
                    }
                }
            }
        }
    }
}

#[component]
fn ReviewRow(term: &'static str, children: Element) -> Element {
    rsx! {
        div { class: "grid grid-cols-[180px_minmax(0,1fr)] gap-4 px-4 py-3.5 not-first:border-t not-first:border-line max-sm:grid-cols-1 max-sm:gap-1",
            dt { class: "text-[0.75rem] font-bold tracking-[0.04em] text-muted uppercase",
                "{term}"
            }
            dd { class: "m-0", {children} }
        }
    }
}

/// The heading of each setup screen, which takes focus when the screen changes.
#[component]
fn StageTitle(children: Element) -> Element {
    rsx! {
        h1 {
            id: "setup-step-title",
            class: "m-0 text-[1.6rem] leading-[1.15] font-bold tracking-[-0.02em] outline-none",
            tabindex: "-1",
            {children}
        }
    }
}

/// A radio option with a title and description, which can hold extra fields of its own.
#[component]
fn Choice(
    name: &'static str,
    value: &'static str,
    checked: bool,
    #[props(into)] title: String,
    #[props(into)] detail: String,
    #[props(default)] recommended: bool,
    onselect: EventHandler,
    children: Element,
) -> Element {
    rsx! {
        div { class: "rounded-md border border-line-soft bg-white/2 transition-colors duration-160 hover:border-white/16 has-checked:border-brand has-checked:bg-brand/7 has-[>label>input:focus-visible]:outline-2 has-[>label>input:focus-visible]:outline-offset-2 has-[>label>input:focus-visible]:outline-primary motion-reduce:transition-none",
            label { class: "grid cursor-pointer grid-cols-[auto_minmax(0,1fr)_auto] items-start gap-x-3 px-4 py-3.5",
                input {
                    class: "m-0 mt-0.5 grid size-4.5 cursor-pointer appearance-none place-content-center rounded-full border-[1.5px] border-muted bg-transparent outline-none transition-colors duration-120 before:size-2 before:scale-0 before:rounded-full before:bg-brand before:transition-transform before:duration-120 checked:border-brand checked:before:scale-100 motion-reduce:transition-none motion-reduce:before:transition-none",
                    r#type: "radio",
                    name,
                    value,
                    checked,
                    onchange: move |_| onselect.call(()),
                }
                span { class: "flex min-w-0 flex-col gap-1",
                    span { class: "leading-[1.35] font-semibold", "{title}" }
                    span { class: "tiny-text mt-0!", "{detail}" }
                }
                if recommended {
                    span { class: "chip chip-positive", "Recommended" }
                }
            }
            {children}
        }
    }
}

/// The first screen of quick setup, where the user picks a starting set of accounts.
#[component]
fn StageStart(first_run: bool) -> Element {
    let mut wizard = use_context::<Wizard>();
    let current = wizard.template().map(|template| template.id);

    rsx! {
        div { class: "flex flex-col gap-2.5",
            StageTitle {
                if first_run {
                    "Set up your ledger"
                } else {
                    "Add accounts"
                }
            }
            p { class: "supporting-text max-w-[76ch] leading-[1.55]",
                if first_run {
                    "Pick a starting point. You can change everything later."
                } else {
                    "Pick a set to add the accounts you are missing."
                }
            }
        }

        fieldset { class: FIELDSET,
            legend { class: "sr-only", "Starting point" }
            div { class: "grid grid-cols-2 gap-2.5 max-sm:grid-cols-1",
                for template in ACCOUNT_TEMPLATES {
                    Choice {
                        name: "setup-template",
                        value: template.id,
                        checked: current == Some(template.id),
                        title: template.name,
                        detail: template.description,
                        recommended: template.recommended,
                        onselect: move |_| wizard.choose_template(template),
                    }
                }
            }
        }

        LedgerSheet {}
    }
}

/// The bank screen of quick and advanced setup.
/// Advanced setup also lets the user pick the ledger account a manual bank account links to.
#[component]
fn StageBank(show_link: bool) -> Element {
    rsx! {
        StageTitle { "Add your bank" }

        BankPlanFields { show_link }
    }
}

/// The bank options, shown only when this server has bank sync.
/// `show_link` lets the user pick the ledger account a manual bank account links to.
#[component]
fn BankPlanFields(show_link: bool) -> Element {
    let mut wizard = use_context::<Wizard>();
    let provider_list = wizard.sync_providers().join(" or ");
    let plan = wizard.bank_plan();
    let linkable = wizard.linkable_accounts();
    let linked = wizard.linked_account().unwrap_or_default();

    rsx! {
        fieldset { class: FIELDSET,
            legend { class: "sr-only", "Your bank" }
            div { class: "grid grid-cols-1 gap-2.5",
                Choice {
                    name: "setup-bank-plan",
                    value: "connect",
                    checked: plan == BankPlan::Connect,
                    title: "Connect my bank",
                    detail: "Import transactions through {provider_list}",
                    recommended: true,
                    onselect: move |_| wizard.chosen_bank_plan.set(BankPlan::Connect),
                }
                Choice {
                    name: "setup-bank-plan",
                    value: "manual",
                    checked: plan == BankPlan::Manual,
                    title: "Add an account by hand",
                    detail: "Track it yourself",
                    onselect: move |_| wizard.chosen_bank_plan.set(BankPlan::Manual),
                    if plan == BankPlan::Manual {
                        // Lines up with the option's text, past the radio
                        div { class: "flex flex-col gap-3 pt-0.5 pr-4 pb-4 pl-11.5 max-sm:pl-4",
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
                                p { class: "tiny-text mt-0!", "Linked to {linked}" }
                            }
                        }
                    }
                }
                Choice {
                    name: "setup-bank-plan",
                    value: "later",
                    checked: plan == BankPlan::Later,
                    title: "Not now",
                    detail: "Add one later from Bank Accounts",
                    onselect: move |_| wizard.chosen_bank_plan.set(BankPlan::Later),
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
        StageTitle { "Choose accounts" }

        fieldset { class: FIELDSET,
            legend { class: "field-label mb-2.5 p-0", "Start from" }
            div { class: "flex flex-wrap gap-2",
                for template in ACCOUNT_TEMPLATES {
                    label { class: "{OPTION} {OPTION_ENABLED}",
                        input {
                            class: "{CHECKBOX} m-0",
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
            fieldset { class: FIELDSET,
                legend { class: "label-strong mb-2.5 p-0", "{heading}" }
                div { class: "flex flex-wrap gap-2",
                    for account in STARTER_ACCOUNTS.iter().filter(move |account| account.account_type == account_type) {
                        AccountOption { account: *account }
                    }
                }
            }
        }

        label { class: "flex cursor-pointer items-start gap-3",
            input {
                class: "{CHECKBOX} mx-0 mt-0.5 mb-0",
                r#type: "checkbox",
                checked: (wizard.use_codes)(),
                onchange: move |event| wizard.use_codes.set(event.checked()),
            }
            span { class: "flex min-w-0 flex-col gap-1",
                span { class: "label-strong", "Number the accounts" }
                span { class: "tiny-text mt-0!", "Adds codes like 1000 and 5020" }
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
            class: "{OPTION}",
            class: if exists { "cursor-default border-dashed bg-transparent text-muted" } else { "{OPTION_ENABLED}" },
            title: account.description,
            input {
                class: "{CHECKBOX} m-0",
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
                span { class: "text-[0.72rem] text-muted", "Already added" }
            }
        }
    }
}

#[component]
fn StageReview() -> Element {
    rsx! {
        div { class: "flex flex-col gap-2.5",
            StageTitle { "Review" }
            p { class: "supporting-text max-w-[76ch] leading-[1.55]",
                "Nothing is saved until you finish."
            }
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
        div { class: "flex flex-col gap-2.5",
            span {
                class: "grid size-10 place-items-center rounded-full bg-positive-soft text-positive",
                aria_hidden: "true",
                Icon { class: "size-5.5", icon: LdCheck }
            }
            StageTitle { "You're all set" }
            p {
                class: "supporting-text max-w-[76ch] leading-[1.55]",
                role: "status",
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
                if result.bank_account_existed {
                    " Your bank account already existed."
                } else if result.bank_account_id.is_some() {
                    " Bank account added."
                }
            }
        }

        // Up to three ways on from here, so on phones they stack instead of wrapping unevenly
        div { class: "{ACTION_ROW} max-sm:flex-col max-sm:items-stretch",
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
