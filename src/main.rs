mod engine;

use std::rc::Rc;
use std::sync::Arc;
use std::sync::Mutex;

use slint::{ComponentHandle, ModelRc, VecModel};
use tokio::runtime::Runtime;

use crate::engine::{html, layout, net, BoxContent, LaidOutBox};
use crate::engine::html::DomNode;

slint::include_modules!();

struct TabState {
    id: usize,
    title: String,
    url: String,
    loading: bool,
    text: String,
    dom: Option<DomNode>,
}

struct AppState {
    rt: Arc<Runtime>,
    client: reqwest::Client,
    next_tab_id: usize,
    tabs: Vec<TabState>,
    active: usize, // индекс в tabs
    history: Vec<String>,
    bookmarks: Vec<String>,
    viewport_width: f32,
}

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
    if !state.tabs.is_empty() {
        app.set_active_tab_id(state.tabs[state.active].id as i32);
    }
}

fn boxes_to_view(boxes: &[LaidOutBox]) -> (Vec<BoxView>, f32) {
    let mut views = Vec::new();
    let mut height = 0.0f32;
    for b in boxes {
        let (text, size, weight, is_rule, link) = match &b.content {
            BoxContent::Text(t, spec) => (
                t.clone(),
                spec.size,
                spec.weight as i32,
                false,
                b.link.clone().unwrap_or_default(),
            ),
            BoxContent::Rule => (String::new(), 0.0, 0, true, String::new()),
        };
        views.push(BoxView {
            x: b.x,
            y: b.y,
            width: b.width,
            height: b.height,
            size,
            weight,
            text: text.into(),
            is_rule,
            link: link.into(),
        });
        height = height.max(b.y + b.height);
    }
    (views, height)
}

fn sync_boxes(app: &BrowserWindow, boxes: &[LaidOutBox]) {
    let (views, height) = boxes_to_view(boxes);
    app.set_page_boxes(ModelRc::new(VecModel::from(views)));
    app.set_page_content_height(height);
}

fn restore_active(app: &BrowserWindow, st: &AppState) {
    if st.tabs.is_empty() {
        return;
    }
    let idx = st.active;
    let tab = &st.tabs[idx];
    app.set_page_title(tab.title.clone().into());
    app.set_page_text(tab.text.clone().into());
    app.set_current_url(tab.url.clone().into());
    app.set_load_progress(if tab.loading { 0.3 } else { 0.0 });
    let boxes = tab
        .dom
        .as_ref()
        .map(|d| layout::layout(d, st.viewport_width))
        .unwrap_or_default();
    sync_boxes(app, &boxes);
}

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

fn spawn_load(
    state_rc: Arc<Mutex<AppState>>,
    app_weak: slint::Weak<BrowserWindow>,
    tab_id: usize,
    url: String,
    viewport_width: f32,
) {
    let (client, rt) = {
        let st = state_rc.lock().unwrap();
        (st.client.clone(), st.rt.clone())
    };
    std::thread::spawn(move || {
        rt.block_on(async move {
            let result = net::fetch_text(&client, &url).await;
            let outcome = match &result {
                Ok(raw) => {
                    let dom = html::parse(raw);
                    let title = html::title_of(&dom).unwrap_or_else(|| url.clone());
                    let boxes = layout::layout(&dom, viewport_width);
                    let mut text = String::new();
                    for b in &boxes {
                        if let BoxContent::Text(line, _) = &b.content {
                            text.push_str(line);
                            text.push('\n');
                        }
                    }
                    (title, text, Some(dom), boxes, true)
                }
                Err(e) => (
                    "Ошибка".to_string(),
                    format!("Не удалось загрузить {url}: {e}"),
                    None,
                    Vec::new(),
                    false,
                ),
            };
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(app) = app_weak.upgrade() {
                    let (title, text, dom, boxes, ok) = outcome;
                    let mut st = state_rc.lock().unwrap();
                    if let Some(tab) = st.tabs.iter_mut().find(|t| t.id == tab_id) {
                        tab.title = title.clone();
                        tab.text = text.clone();
                        tab.url = url.clone();
                        tab.dom = dom;
                        tab.loading = false;
                    }
                    let is_active = st.tabs.get(st.active).map(|t| t.id) == Some(tab_id);
                    if ok {
                        if !st.history.last().map(|u| u == &url).unwrap_or(false) {
                            st.history.push(url.clone());
                        }
                    }
                    let history = st.history.clone();
                    let bookmarks = st.bookmarks.clone();
                    drop(st);

                    if is_active {
                        app.set_page_title(title.into());
                        app.set_page_text(text.into());
                        app.set_current_url(url.into());
                        app.set_load_progress(if ok { 1.0 } else { 0.0 });
                        sync_boxes(&app, &boxes);
                    }
                    let history: Vec<slint::SharedString> = history.into_iter().map(Into::into).collect();
                    let bookmarks: Vec<slint::SharedString> = bookmarks.into_iter().map(Into::into).collect();
                    app.set_history_entries(ModelRc::new(VecModel::from(history)));
                    app.set_bookmark_entries(ModelRc::new(VecModel::from(bookmarks)));
                }
            });
        });
    });
}

fn main() {
    tracing_subscriber::fmt::init();
    let rt = Arc::new(Runtime::new().expect("tokio runtime"));
    let client = net::build_client().expect("reqwest client");

    let app = BrowserWindow::new().expect("slint window");
    let state = Arc::new(Mutex::new(AppState {
        rt,
        client,
        next_tab_id: 2,
        tabs: vec![TabState {
            id: 1,
            title: "Новая вкладка".into(),
            url: String::new(),
            loading: false,
            text: String::new(),
            dom: None,
        }],
        active: 0,
        history: Vec::new(),
        bookmarks: Vec::new(),
        viewport_width: 1080.0,
    }));

    sync_tabs(&app.as_weak(), &state.lock().unwrap());
    let app_weak = app.as_weak();
    restore_active(&app, &state.lock().unwrap());

    // navigate
    {
        let app_weak = app.as_weak();
        let state = state.clone();
        app.on_navigate(move |input| {
            let url = normalize_url(&input);
            let app_weak = app_weak.clone();
            let state = state.clone();

            let tab_id = {
                let mut st = state.lock().unwrap();
                let active = st.active;
                if let Some(tab) = st.tabs.get_mut(active) {
                    tab.loading = true;
                    tab.title = "Загрузка…".into();
                    tab.url = url.clone();
                    tab.dom = None;
                }
                st.tabs.get(st.active).map(|t| t.id).unwrap_or(0)
            };
            if let Some(app) = app_weak.upgrade() {
                app.set_load_progress(0.15);
                app.set_current_url(url.clone().into());
            }
            sync_tabs(&app_weak, &state.lock().unwrap());
            let viewport_width = state.lock().unwrap().viewport_width;
            spawn_load(state.clone(), app_weak.clone(), tab_id, url, viewport_width);
        });
    }

    // new tab
    {
        let app_weak = app.as_weak();
        let state = state.clone();
        app.on_new_tab(move || {
            let mut st = state.lock().unwrap();
            let id = st.next_tab_id;
            st.next_tab_id += 1;
            st.tabs.push(TabState {
                id,
                title: "Новая вкладка".into(),
                url: String::new(),
                loading: false,
                text: String::new(),
                dom: None,
            });
            st.active = st.tabs.len() - 1;
            drop(st);
            sync_tabs(&app_weak, &state.lock().unwrap());
            if let Some(app) = app_weak.upgrade() {
                restore_active(&app, &state.lock().unwrap());
            }
        });
    }

    // tab selected
    {
        let app_weak = app.as_weak();
        let state = state.clone();
        app.on_tab_selected(move |id| {
            let mut st = state.lock().unwrap();
            if let Some(idx) = st.tabs.iter().position(|t| t.id as i32 == id) {
                st.active = idx;
            }
            drop(st);
            if let Some(app) = app_weak.upgrade() {
                restore_active(&app, &state.lock().unwrap());
            }
            sync_tabs(&app_weak, &state.lock().unwrap());
        });
    }

    // tab closed
    {
        let app_weak = app.as_weak();
        let state = state.clone();
        app.on_tab_closed(move |id| {
            let mut st = state.lock().unwrap();
            if let Some(idx) = st.tabs.iter().position(|t| t.id as i32 == id) {
                st.tabs.remove(idx);
                if st.tabs.is_empty() {
                    // keep at least one tab
                    let new_id = st.next_tab_id;
                    st.next_tab_id += 1;
                    st.tabs.push(TabState {
                        id: new_id,
                        title: "Новая вкладка".into(),
                        url: String::new(),
                        loading: false,
                        text: String::new(),
                        dom: None,
                    });
                }
                if st.active >= st.tabs.len() {
                    st.active = st.tabs.len().saturating_sub(1);
                }
            }
            drop(st);
            if let Some(app) = app_weak.upgrade() {
                restore_active(&app, &state.lock().unwrap());
            }
            sync_tabs(&app_weak, &state.lock().unwrap());
        });
    }

    // bookmark toggled
    {
        let app_weak = app.as_weak();
        let state = state.clone();
        app.on_bookmark_toggled(move || {
            let mut st = state.lock().unwrap();
            let url = st.tabs.get(st.active).map(|t| t.url.clone()).unwrap_or_default();
            if url.is_empty() {
                return;
            }
            if let Some(pos) = st.bookmarks.iter().position(|u| u == &url) {
                st.bookmarks.remove(pos);
            } else {
                st.bookmarks.push(url);
            }
            let bookmarks = st.bookmarks.clone();
            drop(st);
            let bookmarks: Vec<slint::SharedString> = bookmarks.into_iter().map(Into::into).collect();
            if let Some(app) = app_weak.upgrade() {
                app.set_bookmark_entries(ModelRc::new(VecModel::from(bookmarks)));
            }
        });
    }

    // viewport changed
    {
        let app_weak = app.as_weak();
        let state = state.clone();
        app.on_viewport_changed(move |w| {
            if w <= 0.0 {
                return;
            }
            let mut st = state.lock().unwrap();
            st.viewport_width = w;
            let dom = st.tabs.get(st.active).and_then(|t| t.dom.clone());
            drop(st);
            if let Some(dom) = dom {
                let boxes = layout::layout(&dom, w);
                if let Some(app) = app_weak.upgrade() {
                    sync_boxes(&app, &boxes);
                }
            }
        });
    }

    app.run().unwrap();
}