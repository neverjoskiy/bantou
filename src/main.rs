mod engine;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use slint::{ComponentHandle, ModelRc, VecModel};
use tokio::runtime::Runtime;

use crate::engine::{html, layout, net, BoxContent};

slint::include_modules!();

struct TabState {
    id: usize,
    title: String,
    url: String,
    loading: bool,
    text: String,
}

struct AppState {
    rt: Arc<Runtime>,
    client: reqwest::Client,
    next_tab_id: usize,
    tabs: Vec<TabState>,
    active: usize, // индекс в tabs
}

impl AppState {
    /// Запускает загрузку страницы вне потока UI; результат — через event loop.
    fn spawn_load(&self, app: slint::Weak<BrowserWindow>, url: String) {
        let client = self.client.clone();
        let rt = self.rt.clone();
        std::thread::spawn(move || {
            rt.block_on(async move {
                let result = net::fetch_text(&client, &url).await;
                let outcome = match &result {
                    Ok(raw) => {
                        let dom = html::parse(raw);
                        let title = html::title_of(&dom).unwrap_or_else(|| url.clone());
                        // Ширина контента после сайдбара (80px) и полей.
                        let boxes = layout::layout(&dom, 1080.0);
                        let mut text = String::new();
                        for b in &boxes {
                            if let BoxContent::Text(line, _) = &b.content {
                                text.push_str(line);
                                text.push('\n');
                            }
                        }
                        (title, text)
                    }
                    Err(e) => {
                        ("Ошибка".to_string(), format!("Не удалось загрузить {url}: {e}"))
                    }
                };
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(app) = app.upgrade() {
                        let (title, text) = outcome;
                        app.set_page_title(title.into());
                        app.set_page_text(text.into());
                        app.set_current_url(url.into());
                        app.set_load_progress(1.0);
                    }
                });
            });
        });
    }
}

/// Синхронизирует вкладки из Rust-состояния в модель Slint.
fn sync_tabs(app: &slint::Weak<BrowserWindow>, state: &AppState) {
    let Some(app) = app.upgrade() else { return };
    let rows: Vec<TabInfo> = state
        .tabs
        .iter()
        .map(|t| TabInfo {
            id: t.id as i32,
            title: t.title.clone().into(),
            loading: t.loading,
        })
        .collect();
    app.set_tabs(ModelRc::new(VecModel::from(rows)));
    app.set_active_tab_id(state.tabs[state.active].id as i32);
}

/// Поисковая строка: URL или Google-поиск.
fn normalize_url(input: &str) -> String {
    let t = input.trim();
    if t.starts_with("http://") || t.starts_with("https://") {
        return t.to_string();
    }
    if t.contains('.') && !t.contains(' ') {
        return format!("https://{t}");
    }
    format!("https://www.google.com/search?q={}", urlencoding::encode(t))
}

fn main() {
    tracing_subscriber::fmt::init();
    let rt = Arc::new(Runtime::new().expect("tokio runtime"));
    let client = net::build_client().expect("reqwest client");

    let app = BrowserWindow::new().expect("slint window");
    let state = Rc::new(RefCell::new(AppState {
        rt,
        client,
        next_tab_id: 2,
        tabs: vec![TabState {
            id: 1,
            title: "Новая вкладка".into(),
            url: String::new(),
            loading: false,
            text: String::new(),
        }],
        active: 0,
    }));

    sync_tabs(&app, &state.borrow());

    // navigate: помечаем вкладку загружающейся и уходим в пул tokio.
    {
        let app_weak = app.as_weak();
        let state = state.clone();
        app.on_navigate(move |input| {
            let url = normalize_url(&input);
            let app_weak = app_weak.clone();
            let state = state.clone();

            {
                let mut st = state.borrow_mut();
                if let Some(tab) = st.tabs.get_mut(st.active) {
                    tab.loading = true;
                    tab.title = "Загрузка…".into();
                    tab.url = url.clone();
                }
                if let Some(app) = app_weak.upgrade() {
                    app.set_load_progress(0.15);
                    app.set_current_url(url.clone().into());
                }
            }
            sync_tabs(&app_weak, &state.borrow());
            state.borrow().spawn_load(app_weak.clone(), url);
        });
    }

    {
        let app_weak = app.as_weak();
        let state = state.clone();
        app.on_new_tab(move || {
            let mut st = state.borrow_mut();
            let id = st.next_tab_id;
            st.next_tab_id += 1;
            st.tabs.push(TabState {
                id,
                title: "Новая вкладка".into(),
                url: String::new(),
                loading: false,
                text: String::new(),
            });
            st.active = st.tabs.len() - 1;
            drop(st);
            sync_tabs(&app_weak, &state.borrow());
        });
    }

    {
        let app_weak = app.as_weak();
        let state = state.clone();
        app.on_tab_selected(move |id| {
            let st = state.borrow();
            if let Some(idx) = st.tabs.iter().position(|t| t.id as i32 == id) {
                st.active = idx;
                let tab = &st.tabs[idx];
                if let Some(app) = app_weak.upgrade() {
                    app.set_page_title(tab.title.clone().into());
                    app.set_page_text(tab.text.clone().into());
                    app.set_current_url(tab.url.clone().into());
                    app.set_load_progress(if tab.loading { 0.3 } else { 0.0 });
                }
            }
            drop(st);
            sync_tabs(&app_weak, &state.borrow());
        });
    }

    {
        let state = state.clone();
        app.on_tab_closed(move |id| {
            let mut st = state.borrow_mut();
            if let Some(idx) = st.tabs.iter().position(|t| t.id as i32 == id) {
                st.tabs.remove(idx);
                if st.active >= st.tabs.len() {
                    st.active = st.tabs.len().saturating_sub(1);
                }
            }
        });
    }

    app.on_bookmark_toggled(move || {});

    app.run().unwrap();
}
