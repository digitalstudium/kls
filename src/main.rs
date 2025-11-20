use anyhow::Context;
use anyhow::Result;
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyModifiers,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use directories::ProjectDirs;

use ratatui::{
    Terminal,
    backend::Backend,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
};
use std::collections::HashMap;
use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::process::Command as AsyncCommand;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

// --- CONFIGURATION ---
const FOOTER_HEIGHT: u16 = 3;

const TOP_API_RESOURCES: &[&str] = &[
    "pods",
    "services",
    "configmaps",
    "secrets",
    "persistentvolumeclaims",
    "ingresses",
    "nodes",
    "deployments",
    "statefulsets",
    "daemonsets",
    "storageclasses",
];

const BATCAT_STYLE: &str = " --paging always --style numbers";

// --- CACHE HELPERS ---

#[derive(serde::Serialize, serde::Deserialize)]
struct DiskResourceCache {
    // Ключ: "namespace|kind"
    data: HashMap<String, DiskCacheEntry>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct DiskCacheEntry {
    timestamp: u64, // Время сохранения (Unix timestamp)
    lines: Vec<String>,
}

const RESOURCES_CACHE_FILENAME: &str = "resources.json";
const RESOURCES_CACHE_TTL_SECONDS: u64 = 30;
const RESOURCES_CACHE_KEY_SEPARATOR: &str = "|";

fn get_cache_path(filename: &str) -> Option<PathBuf> {
    let proj_dirs = ProjectDirs::from("com", "kls", "kls")?;
    let cache_dir = proj_dirs.cache_dir();

    if !cache_dir.exists() {
        fs::create_dir_all(cache_dir).ok()?;
    }

    Some(cache_dir.join(filename))
}

fn save_to_json<T: serde::Serialize>(
    filename: &str,
    data: &T,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = get_cache_path(filename).ok_or("Unable to resolve cache path")?;
    let json = serde_json::to_string(data)?;
    fs::write(path, json)?;
    Ok(())
}

fn load_from_json<T: serde::de::DeserializeOwned>(
    filename: &str,
) -> Result<T, Box<dyn std::error::Error>> {
    let path = get_cache_path(filename).ok_or("Unable to resolve cache path")?;
    let content = fs::read_to_string(path)?;
    let data = serde_json::from_str(&content)?;
    Ok(data)
}

fn save_simple_cache(filename: &str, data: &[String]) {
    let _ = save_to_json(filename, &data);
}

fn load_simple_cache(filename: &str) -> Option<Vec<String>> {
    load_from_json(filename).ok()
}

fn save_resource_cache_to_disk(cache: &HashMap<(String, String), (Instant, Vec<String>)>) {
    let disk_cache = convert_memory_cache_to_disk(cache);
    // ИЗМЕНЕНИЕ: Используем универсальный метод напрямую
    let _ = save_to_json(RESOURCES_CACHE_FILENAME, &disk_cache);
}

fn load_resource_cache_from_disk() -> HashMap<(String, String), (Instant, Vec<String>)> {
    load_from_json::<DiskResourceCache>(RESOURCES_CACHE_FILENAME)
        .ok()
        .map(convert_disk_cache_to_memory)
        .unwrap_or_default()
}

fn get_current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn create_cache_key(namespace: &str, kind: &str) -> String {
    format!("{}{}{}", namespace, RESOURCES_CACHE_KEY_SEPARATOR, kind)
}

fn parse_cache_key(key: &str) -> Option<(String, String)> {
    key.split_once(RESOURCES_CACHE_KEY_SEPARATOR)
        .map(|(ns, kind)| (ns.to_string(), kind.to_string()))
}

fn instant_to_timestamp(instant: &Instant, current_timestamp: u64) -> u64 {
    let age_secs = instant.elapsed().as_secs();
    current_timestamp.saturating_sub(age_secs)
}

fn timestamp_to_instant(entry_timestamp: u64, current_timestamp: u64) -> Instant {
    let age = Duration::from_secs(current_timestamp.saturating_sub(entry_timestamp));
    Instant::now().checked_sub(age).unwrap_or_else(Instant::now)
}

fn is_cache_entry_valid(entry_timestamp: u64, current_timestamp: u64) -> bool {
    current_timestamp >= entry_timestamp
        && (current_timestamp - entry_timestamp) < RESOURCES_CACHE_TTL_SECONDS
}

fn convert_memory_entry_to_disk(
    ns: &str,
    kind: &str,
    instant: &Instant,
    lines: &[String],
    current_timestamp: u64,
) -> (String, DiskCacheEntry) {
    let key = create_cache_key(ns, kind);
    let timestamp = instant_to_timestamp(instant, current_timestamp);

    let entry = DiskCacheEntry {
        timestamp,
        lines: lines.to_vec(),
    };

    (key, entry)
}

fn convert_memory_cache_to_disk(
    cache: &HashMap<(String, String), (Instant, Vec<String>)>,
) -> DiskResourceCache {
    let current_timestamp = get_current_timestamp();

    let data = cache
        .iter()
        .map(|((ns, kind), (instant, lines))| {
            convert_memory_entry_to_disk(ns, kind, instant, lines, current_timestamp)
        })
        .collect();

    DiskResourceCache { data }
}

fn convert_disk_entry_to_memory(
    key: String,
    entry: DiskCacheEntry,
    current_timestamp: u64,
) -> Option<((String, String), (Instant, Vec<String>))> {
    if !is_cache_entry_valid(entry.timestamp, current_timestamp) {
        return None;
    }

    let (ns, kind) = parse_cache_key(&key)?;
    let instant = timestamp_to_instant(entry.timestamp, current_timestamp);

    Some(((ns, kind), (instant, entry.lines)))
}

fn convert_disk_cache_to_memory(
    disk_cache: DiskResourceCache,
) -> HashMap<(String, String), (Instant, Vec<String>)> {
    let current_timestamp = get_current_timestamp();

    disk_cache
        .data
        .into_iter()
        .filter_map(|(key, entry)| convert_disk_entry_to_memory(key, entry, current_timestamp))
        .collect()
}

// --- KUBECTL HELPER FUNCTIONS ---

fn run_kubectl_sync(args: &[&str]) -> Result<Vec<String>> {
    let output = Command::new("kubectl")
        .args(args)
        .output()
        .context("Failed to execute kubectl")?;

    if !output.status.success() {
        return Ok(vec![]);
    }

    let stdout = String::from_utf8(output.stdout)?;
    Ok(stdout
        .lines()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect())
}

// Асинхронная версия для получения данных
async fn run_kubectl_async(args: Vec<String>) -> Result<Vec<String>> {
    let output = AsyncCommand::new("kubectl").args(args).output().await?;

    if !output.status.success() {
        return Ok(vec![]);
    }

    let stdout = String::from_utf8(output.stdout)?;
    Ok(stdout
        .lines()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect())
}

// Переписали на async, чтобы не блокировать старт
async fn get_namespaces_async() -> Result<Vec<String>> {
    let all_ns_args = vec![
        "get".to_string(),
        "ns".to_string(),
        "--no-headers".to_string(),
        "-o".to_string(),
        "custom-columns=NAME:.metadata.name".to_string(),
    ];

    let current_ns_args = vec![
        "config".to_string(),
        "view".to_string(),
        "--minify".to_string(),
        "--output".to_string(),
        "jsonpath={..namespace}".to_string(),
    ];

    let all_ns = run_kubectl_async(all_ns_args).await.unwrap_or_default();
    let current_ns_vec = run_kubectl_async(current_ns_args).await.unwrap_or_default();
    let current_ns = current_ns_vec.first().cloned();

    if let Some(curr) = current_ns {
        let mut result = vec![curr.clone()];
        result.extend(all_ns.into_iter().filter(|ns| ns != &curr));
        Ok(result)
    } else {
        Ok(all_ns)
    }
}

async fn get_api_resources_async() -> Result<Vec<String>> {
    let args = vec![
        "api-resources".to_string(),
        "--no-headers".to_string(),
        "--verbs=get".to_string(),
    ];
    let output = run_kubectl_async(args).await.unwrap_or_default();

    let cluster_resources: Vec<String> = output
        .iter()
        .filter_map(|line| line.split_whitespace().next().map(|s| s.to_string()))
        .collect();

    let mut result = Vec::new();
    let mut seen = HashSet::new();

    for &res in TOP_API_RESOURCES {
        result.push(res.to_string());
        seen.insert(res.to_string());
    }

    for res in cluster_resources {
        if !seen.contains(&res) {
            result.push(res.clone());
            seen.insert(res);
        }
    }
    Ok(result)
}

async fn get_contexts_async() -> Result<Vec<String>> {
    let args = vec![
        "config".to_string(),
        "get-contexts".to_string(),
        "-o".to_string(),
        "name".to_string(),
    ];
    run_kubectl_async(args).await
}

// Функция для переключения (синхронная, так как блокирует работу пока не переключимся)
fn switch_context_sync(context_name: &str) -> Result<()> {
    run_kubectl_sync(&["config", "use-context", context_name]).map(|_| ())
}
// --- SYSTEM COMMAND EXECUTOR ---

fn execute_shell_command<B: Backend>(terminal: &mut Terminal<B>, command_str: &str) -> Result<()> {
    leave_tui_mode()?;
    run_shell_command(command_str);
    enter_tui_mode(terminal)?;
    Ok(())
}

fn leave_tui_mode() -> Result<()> {
    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture)?;
    Ok(())
}

fn enter_tui_mode<B: Backend>(terminal: &mut Terminal<B>) -> Result<()> {
    execute!(io::stdout(), EnterAlternateScreen, EnableMouseCapture)?;
    enable_raw_mode()?;
    render_clean_screen(terminal)?;
    Ok(())
}

fn run_shell_command(command_str: &str) {
    let status = Command::new("sh").arg("-c").arg(command_str).status();

    if let Err(e) = status {
        println!("Error executing command: {}", e);
        println!("Press Enter to continue...");
        let _ = io::stdin().read_line(&mut String::new());
    }
}

fn render_clean_screen<B: Backend>(terminal: &mut Terminal<B>) -> Result<()> {
    terminal.clear()?;
    terminal.draw(|f| {
        let size = f.area();
        let block = Block::default().style(Style::default().bg(Color::Reset));
        f.render_widget(block, size);
    })?;
    Ok(())
}

// --- DATA STRUCTURES ---

struct Menu {
    title: String,
    items: Vec<String>,
    state: ListState,
    filter: String,
    filter_mode: bool,
    is_loading: bool, // Флаг загрузки
}

impl Menu {
    fn new(title: &str, items: Vec<String>) -> Self {
        let mut state = ListState::default();
        if !items.is_empty() {
            state.select(Some(0));
        }
        Menu {
            title: title.to_string(),
            items,
            state,
            filter: String::new(),
            filter_mode: false,
            is_loading: false,
        }
    }

    // Создаем меню сразу с состоянием загрузки
    fn new_loading(title: &str) -> Self {
        let mut m = Menu::new(title, vec!["loading...".to_string()]);
        m.is_loading = true;
        m
    }

    fn filtered_items(&self) -> Vec<String> {
        if self.filter.is_empty() {
            self.items.clone()
        } else {
            let lower_filter = self.filter.to_lowercase();
            self.items
                .iter()
                .filter(|item| item.to_lowercase().contains(&lower_filter))
                .cloned()
                .collect()
        }
    }

    fn selected_item(&self) -> Option<String> {
        if self.is_loading {
            return None;
        } // Нельзя выбрать "loading..."
        let items = self.filtered_items();
        self.state.selected().and_then(|i| items.get(i).cloned())
    }

    fn set_items(&mut self, new_items: Vec<String>) {
        // Запоминаем текущий индекс
        let previous_selection = self.state.selected();

        self.items = new_items;
        self.is_loading = false;

        if !self.items.is_empty() {
            // Пытаемся восстановить курсор
            if let Some(idx) = previous_selection {
                if idx < self.items.len() {
                    self.state.select(Some(idx));
                } else {
                    // Если элементов стало меньше, ставим на последний
                    self.state.select(Some(self.items.len() - 1));
                }
            } else {
                self.state.select(Some(0));
            }
        } else {
            self.state.select(None);
        }
    }

    fn set_loading(&mut self) {
        self.is_loading = true;
        self.items = vec!["loading...".to_string()];
        self.state.select(Some(0));
    }

    fn next(&mut self) {
        if self.is_loading {
            return;
        }
        let items = self.filtered_items();
        if items.is_empty() {
            return;
        }
        let i = match self.state.selected() {
            Some(i) => {
                if i >= items.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.state.select(Some(i));
    }

    fn previous(&mut self) {
        if self.is_loading {
            return;
        }
        let items = self.filtered_items();
        if items.is_empty() {
            return;
        }
        let i = match self.state.selected() {
            Some(i) => {
                if i == 0 {
                    items.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.state.select(Some(i));
    }

    fn update_selection_after_filter(&mut self) {
        let len = self.filtered_items().len();
        if len == 0 {
            self.state.select(None);
        } else {
            match self.state.selected() {
                Some(i) if i >= len => self.state.select(Some(0)),
                None => self.state.select(Some(0)),
                _ => {}
            }
        }
    }

    fn enter_filter_mode(&mut self) {
        self.filter_mode = true;
        self.filter.clear();
        self.update_selection_after_filter();
    }

    fn exit_filter_mode(&mut self) {
        self.filter_mode = false;
        self.filter.clear();
        self.update_selection_after_filter();
    }
}

// Enum для событий обновления данных
enum AppEvent {
    Namespaces(Vec<String>),
    ApiResources(Vec<String>),
    Resources(Vec<String>, usize),
    Contexts(Vec<String>),
}

struct App {
    menus: Vec<Menu>,
    selected_menu_index: usize,
    should_quit: bool,
    event_tx: mpsc::UnboundedSender<AppEvent>,
    event_rx: mpsc::UnboundedReceiver<AppEvent>,
    current_fetch_task: Option<JoinHandle<()>>,
    fetch_id: usize, // <--- Счетчик версий запросов
    resource_cache: HashMap<(String, String), (Instant, Vec<String>)>,
    show_context_popup: bool,
    context_items: Vec<String>,
    context_state: ListState,
}

impl App {
    fn new() -> Result<App> {
        let cached_ns = Self::load_cached_namespaces();
        let cached_api = Self::load_cached_api_resources();

        let api_cache_exists = cached_api.is_some();

        let menu_ns = Self::create_namespace_menu(cached_ns);
        let menu_api = Self::create_api_menu(cached_api);
        let menus = vec![menu_ns, menu_api, Menu::new("Resources", vec![])];

        let (tx, rx) = mpsc::unbounded_channel();
        let resource_cache = load_resource_cache_from_disk();

        let app = App {
            menus,
            selected_menu_index: 0,
            should_quit: false,
            event_tx: tx,
            event_rx: rx,
            current_fetch_task: None,
            fetch_id: 0,
            resource_cache,
            show_context_popup: false,
            context_items: vec![],
            context_state: ListState::default(),
        };

        // фоновое обновление (даже если кэш есть)
        app.fetch_initial_data(api_cache_exists);

        Ok(app)
    }

    fn load_cached_namespaces() -> Option<Vec<String>> {
        load_simple_cache("namespaces.json")
    }

    fn load_cached_api_resources() -> Option<Vec<String>> {
        load_simple_cache("apis.json")
    }

    fn create_namespace_menu(cached_ns: Option<Vec<String>>) -> Menu {
        match cached_ns {
            Some(items) => Menu::new("Namespaces", items),
            None => Menu::new_loading("Namespaces"),
        }
    }

    fn create_api_menu(cached_api: Option<Vec<String>>) -> Menu {
        match cached_api {
            Some(items) => Menu::new("API Resources", items),
            None => Menu::new_loading("API Resources"),
        }
    }

    fn open_context_popup(&mut self) {
        self.show_context_popup = true;
        self.context_items = vec!["loading...".to_string()];
        self.context_state.select(Some(0));

        let tx = self.event_tx.clone();
        tokio::spawn(async move {
            let data = get_contexts_async().await.unwrap_or_default();
            let _ = tx.send(AppEvent::Contexts(data));
        });
    }

    fn fetch_initial_data(&self, skip_api_fetch: bool) {
        // --- NAMESPACES ---
        let tx_ns = self.event_tx.clone();
        tokio::spawn(async move {
            let data = get_namespaces_async().await.unwrap_or_default();

            // Если данные успешно получены и не пусты, сохраняем в кэш
            if !data.is_empty() {
                save_simple_cache("namespaces.json", &data);
            }

            let _ = tx_ns.send(AppEvent::Namespaces(data));
        });

        // --- API RESOURCES ---
        if !skip_api_fetch {
            let tx_api = self.event_tx.clone();
            tokio::spawn(async move {
                let data = get_api_resources_async().await.unwrap_or_default();
                if !data.is_empty() {
                    save_simple_cache("apis.json", &data);
                }
                let _ = tx_api.send(AppEvent::ApiResources(data));
            });
        }
    }

    fn refresh_metadata(&mut self) {
        self.fetch_id += 1;

        // Очищаем память
        self.resource_cache.clear();

        // Очищаем диск (перезаписываем пустым кэшем или удаляем файл)
        save_resource_cache_to_disk(&self.resource_cache);
        // Или: if let Some(p) = get_cache_path("resources.json") { let _ = fs::remove_file(p); }

        self.menus[0].set_loading();
        self.menus[1].set_loading();
        self.menus[2].set_items(vec![]);

        self.fetch_initial_data(false);
    }

    fn active_menu_mut(&mut self) -> &mut Menu {
        &mut self.menus[self.selected_menu_index]
    }

    fn next_menu(&mut self) {
        self.selected_menu_index = (self.selected_menu_index + 1) % self.menus.len();
    }

    fn previous_menu(&mut self) {
        if self.selected_menu_index == 0 {
            self.selected_menu_index = self.menus.len() - 1;
        } else {
            self.selected_menu_index -= 1;
        }
    }

    fn trigger_resource_fetch(&mut self, is_auto_refresh: bool) {
        let (ns, kind) = match self.selected_ns_and_kind() {
            Some(v) => v,
            None => {
                self.clear_resources_menu();
                return;
            }
        };

        self.advance_fetch_id();

        if !is_auto_refresh {
            let key = (ns.clone(), kind.clone());
            if let Some(items) = self.resources_cached_recently(&key) {
                self.menus[2].set_items(items);
                return;
            }
        }

        if !is_auto_refresh {
            self.set_resources_loading();
        }

        self.abort_current_fetch();
        self.launch_resource_fetch(ns, kind);
    }

    fn selected_ns_and_kind(&self) -> Option<(String, String)> {
        let ns = self.menus[0].selected_item()?;
        let kind = self.menus[1].selected_item()?;
        Some((ns, kind))
    }

    fn clear_resources_menu(&mut self) {
        self.menus[2].set_items(vec![]);
    }

    fn advance_fetch_id(&mut self) {
        self.fetch_id += 1;
    }

    fn resources_cached_recently(&self, key: &(String, String)) -> Option<Vec<String>> {
        self.resource_cache.get(key).and_then(|(timestamp, items)| {
            if timestamp.elapsed().as_secs() < 60 {
                Some(items.clone())
            } else {
                None
            }
        })
    }

    fn set_resources_loading(&mut self) {
        self.menus[2].set_loading();
    }

    fn abort_current_fetch(&mut self) {
        if let Some(task) = &self.current_fetch_task {
            task.abort();
        }
    }

    fn launch_resource_fetch(&mut self, ns: String, kind: String) {
        let fetch_id = self.fetch_id;
        let tx = self.event_tx.clone();

        let handle = tokio::spawn(async move {
            let args = vec![
                "-n".to_string(),
                ns,
                "get".to_string(),
                kind,
                "--no-headers".to_string(),
                "--ignore-not-found".to_string(),
            ];

            let lines = match run_kubectl_async(args).await {
                Ok(lines) => lines,
                Err(_) => vec![],
            };

            let _ = tx.send(AppEvent::Resources(lines, fetch_id));
        });

        self.current_fetch_task = Some(handle);
    }

    fn get_current_selection(&self) -> Option<(String, String, String)> {
        let ns = self.menus[0].selected_item()?;
        let kind = self.menus[1].selected_item()?;
        let row = self.menus[2].selected_item()?;

        // Игнорируем если выбрано "loading..." (хотя selected_item должен это обработать)
        if row == "loading..." {
            return None;
        }

        let name = row.split_whitespace().next()?.to_string();
        Some((ns, kind, name))
    }
}

// --- MAIN ---

#[tokio::main]
async fn main() -> Result<()> {
    let mut terminal = setup_terminal()?;

    let res = run_app(&mut terminal).await;

    restore_terminal(&mut terminal)?;

    if let Err(err) = res {
        eprintln!("Error: {:?}", err);
    }

    Ok(())
}

fn setup_terminal() -> Result<Terminal<CrosstermBackend<io::Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;
    Ok(terminal)
}

async fn run_app<B: Backend>(terminal: &mut Terminal<B>) -> Result<()> {
    let app_result = App::new();

    match app_result {
        Ok(mut app) => {
            maybe_trigger_initial_fetch(&mut app);
            run_loop(terminal, &mut app).await
        }
        Err(e) => Err(e),
    }
}

fn maybe_trigger_initial_fetch(app: &mut App) {
    if !app.menus[0].is_loading && !app.menus[1].is_loading {
        app.trigger_resource_fetch(false);
    }
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    Ok(())
}

async fn run_loop<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
) -> Result<()> {
    let ui_tick = Duration::from_millis(100); // частота проверки ввода/событий
    let refresh_interval = Duration::from_secs(2); // раз в 2 сек обновляем ресурсы
    let mut last_refresh = Instant::now();

    loop {
        // было: большой while + match
        process_pending_events(app);

        terminal.draw(|f| ui(f, app))?;

        if last_refresh.elapsed() >= refresh_interval && !app.menus[2].is_loading {
            app.trigger_resource_fetch(true); // тихое обновление
            last_refresh = Instant::now();
        }

        if event::poll(ui_tick)? {
            if let Event::Key(key) = event::read()? {
                handle_input(app, key, terminal)?;
            }
        }

        if app.should_quit {
            return Ok(());
        }
    }
}

fn process_pending_events(app: &mut App) {
    while let Ok(event) = app.event_rx.try_recv() {
        handle_app_event(app, event);
    }
}

fn handle_app_event(app: &mut App, event: AppEvent) {
    match event {
        AppEvent::Contexts(items) => handle_contexts_event(app, items),
        AppEvent::Namespaces(items) => handle_namespaces_event(app, items),
        AppEvent::ApiResources(items) => handle_api_resources_event(app, items),
        AppEvent::Resources(items, id) => handle_resources_event(app, items, id),
    }
}

fn handle_contexts_event(app: &mut App, items: Vec<String>) {
    app.context_items = items;
    if !app.context_items.is_empty() {
        app.context_state.select(Some(0));
    }
}

fn handle_namespaces_event(app: &mut App, items: Vec<String>) {
    app.menus[0].set_items(items);
    if !app.menus[1].is_loading {
        app.trigger_resource_fetch(false);
    }
}

fn handle_api_resources_event(app: &mut App, items: Vec<String>) {
    app.menus[1].set_items(items);
    if !app.menus[0].is_loading {
        app.trigger_resource_fetch(false);
    }
}

fn handle_resources_event(app: &mut App, items: Vec<String>, id: usize) {
    if id != app.fetch_id {
        return;
    }

    if let Some(ns) = app.menus[0].selected_item() {
        if let Some(kind) = app.menus[1].selected_item() {
            app.resource_cache
                .insert((ns.clone(), kind.clone()), (Instant::now(), items.clone()));

            let cache_clone = app.resource_cache.clone();
            tokio::task::spawn_blocking(move || {
                save_resource_cache_to_disk(&cache_clone);
            });
        }
    }

    app.menus[2].set_items(items);
}

#[derive(Default)]
struct InputOutcome {
    selection_changed: bool,
    force_refresh: bool,
    should_stop: bool,
}

fn handle_input<B: Backend>(
    app: &mut App,
    key: KeyEvent,
    terminal: &mut Terminal<B>,
) -> Result<()> {
    if app.show_context_popup {
        handle_context_popup_key(app, key);
        return Ok(());
    }

    let previous_menu_index = app.selected_menu_index;

    let outcome = process_main_input(app, key, terminal)?;

    if outcome.should_stop {
        return Ok(());
    }

    apply_input_results(app, &outcome, previous_menu_index);

    Ok(())
}

fn process_main_input<B: Backend>(
    app: &mut App,
    key: KeyEvent,
    terminal: &mut Terminal<B>,
) -> Result<InputOutcome> {
    let mut outcome = InputOutcome::default();

    if is_navigation_key(key.code) {
        handle_navigation_keys(app, key.code, &mut outcome.selection_changed);
        return Ok(outcome);
    }

    let filter_mode = {
        let menu = app.active_menu_mut();
        menu.filter_mode
    };

    if filter_mode {
        handle_filter_mode_keys(app, key.code, &mut outcome.selection_changed);
        Ok(outcome)
    } else {
        handle_normal_keys(app, key, terminal)
    }
}

fn apply_input_results(app: &mut App, outcome: &InputOutcome, previous_menu_index: usize) {
    if outcome.selection_changed && (previous_menu_index == 0 || previous_menu_index == 1) {
        app.trigger_resource_fetch(false);
    }

    if outcome.force_refresh {
        app.trigger_resource_fetch(false);
    }
}

fn handle_context_popup_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => close_context_popup(app),
        KeyCode::Down | KeyCode::Char('j') => move_context_selection_down(app),
        KeyCode::Up | KeyCode::Char('k') => move_context_selection_up(app),
        KeyCode::Enter => apply_selected_context(app),
        _ => {}
    }
}

fn close_context_popup(app: &mut App) {
    app.show_context_popup = false;
}

fn move_context_selection_down(app: &mut App) {
    let len = app.context_items.len();
    let i = match app.context_state.selected() {
        Some(i) => {
            if i >= len.saturating_sub(1) {
                0
            } else {
                i + 1
            }
        }
        None => 0,
    };
    app.context_state.select(Some(i));
}

fn move_context_selection_up(app: &mut App) {
    let len = app.context_items.len();
    let i = match app.context_state.selected() {
        Some(0) | None => len.saturating_sub(1),
        Some(i) => i - 1,
    };
    app.context_state.select(Some(i));
}

fn apply_selected_context(app: &mut App) {
    if let Some(idx) = app.context_state.selected() {
        if let Some(ctx_name) = app.context_items.get(idx) {
            if ctx_name != "loading..." {
                let _ = switch_context_sync(ctx_name);
                app.show_context_popup = false;
                app.refresh_metadata();
            }
        }
    }
}

fn is_navigation_key(code: KeyCode) -> bool {
    matches!(
        code,
        KeyCode::Down
            | KeyCode::Up
            | KeyCode::Home
            | KeyCode::End
            | KeyCode::Right
            | KeyCode::Left
            | KeyCode::Tab
            | KeyCode::BackTab
    )
}

fn handle_navigation_keys(app: &mut App, code: KeyCode, selection_changed: &mut bool) {
    match code {
        KeyCode::Down => {
            app.active_menu_mut().next();
            *selection_changed = true;
        }
        KeyCode::Up => {
            app.active_menu_mut().previous();
            *selection_changed = true;
        }
        KeyCode::Home => {
            let menu = app.active_menu_mut();
            if !menu.is_loading {
                menu.state.select(Some(0));
                *selection_changed = true;
            }
        }
        KeyCode::Right | KeyCode::Tab => app.next_menu(),
        KeyCode::Left | KeyCode::BackTab => app.previous_menu(),
        KeyCode::End => {
            // в исходном коде End помечен как "навигационный", но не используется
        }
        _ => {}
    }
}

fn handle_filter_mode_keys(app: &mut App, code: KeyCode, selection_changed: &mut bool) {
    let menu = app.active_menu_mut();

    match code {
        KeyCode::Esc => {
            menu.exit_filter_mode();
            *selection_changed = true;
        }
        KeyCode::Enter => {
            menu.filter_mode = false;
        }
        KeyCode::Backspace => {
            menu.filter.pop();
            menu.update_selection_after_filter();
            *selection_changed = true;
        }
        KeyCode::Char(c) => {
            menu.filter.push(c);
            menu.update_selection_after_filter();
            *selection_changed = true;
        }
        _ => {}
    }
}

fn handle_normal_keys<B: Backend>(
    app: &mut App,
    key: KeyEvent,
    terminal: &mut Terminal<B>,
) -> Result<InputOutcome> {
    let mut outcome = InputOutcome::default();

    match key.code {
        KeyCode::Char('q') => {
            app.should_quit = true;
        }
        KeyCode::Char('/') => {
            let menu = app.active_menu_mut();
            if !menu.is_loading {
                menu.enter_filter_mode();
            }
        }
        KeyCode::Char('j') => {
            app.active_menu_mut().next();
            outcome.selection_changed = true;
        }
        KeyCode::Char('k') => {
            app.active_menu_mut().previous();
            outcome.selection_changed = true;
        }
        KeyCode::Esc => {
            let menu = app.active_menu_mut();
            if !menu.filter_mode && !menu.filter.is_empty() {
                menu.exit_filter_mode();
                outcome.selection_changed = true;
            } else {
                app.should_quit = true;
            }
        }

        // --- Ctrl + Key ---
        KeyCode::Char(c) if key.modifiers.contains(KeyModifiers::CONTROL) => {
            if handle_ctrl_shortcuts(app, c, terminal)? {
                outcome.should_stop = true;
                return Ok(outcome);
            }
        }

        // Удаление ресурса
        KeyCode::Delete => {
            if let Some((ns, kind, name)) = app.get_current_selection() {
                let cmd = format!("kubectl -n {} delete {} {}", ns, kind, name);
                execute_shell_command(terminal, &cmd)?;
                outcome.force_refresh = true;
            }
        }

        _ => {}
    }

    Ok(outcome)
}

fn handle_ctrl_shortcuts<B: Backend>(
    app: &mut App,
    c: char,
    terminal: &mut Terminal<B>,
) -> Result<bool> {
    if handle_ctrl_quick_actions(app, c) {
        // true = нужно немедленно выйти из handle_input (Ctrl+S/Ctrl+R)
        return Ok(true);
    }

    if is_ctrl_resource_action(c) {
        handle_ctrl_resource_action(app, c, terminal)?;
    }

    Ok(false)
}

fn handle_ctrl_quick_actions(app: &mut App, c: char) -> bool {
    match c {
        's' => {
            app.open_context_popup();
            true
        }
        'r' => {
            app.refresh_metadata();
            true
        }
        _ => false,
    }
}

fn is_ctrl_resource_action(c: char) -> bool {
    matches!(c, 'y' | 'd' | 'e' | 'l' | 'x' | 'n' | 'a' | 'p')
}

fn handle_ctrl_resource_action<B: Backend>(
    app: &mut App,
    c: char,
    terminal: &mut Terminal<B>,
) -> Result<()> {
    if let Some((ns, kind, name)) = app.get_current_selection() {
        if let Some(template) = ctrl_command_template(c) {
            let mut cmd = build_shell_command(template, &ns, &kind, &name);

            if cmd.contains("batcat") {
                cmd.push_str(BATCAT_STYLE);
            }

            execute_shell_command(terminal, &cmd)?;
        }
    }

    Ok(())
}

fn ctrl_command_template(c: char) -> Option<&'static str> {
    match c {
        'y' => {
            Some("kubectl -n {namespace} get {api_resource} {resource} -o yaml | batcat -l yaml")
        }
        'd' => Some("kubectl -n {namespace} describe {api_resource} {resource} | batcat -l yaml"),
        'e' => Some("kubectl -n {namespace} edit {api_resource} {resource}"),
        'l' => Some("kubectl -n {namespace} logs {resource} | hl"),
        'x' => Some("kubectl -n {namespace} exec -it {resource} -- sh"),
        'n' => Some("kubectl -n {namespace} debug {resource} -it --image=nicolaka/netshoot"),
        'a' => Some(
            "kubectl -n {namespace} logs {resource} -c istio-proxy | jaq -Rc 'fromjson? | .' --sort-keys | hl",
        ),
        'p' => Some("kubectl -n {namespace} exec -it {resource} -c istio-proxy -- bash"),
        _ => None,
    }
}

fn build_shell_command(template: &str, ns: &str, kind: &str, name: &str) -> String {
    template
        .replace("{namespace}", ns)
        .replace("{api_resource}", kind)
        .replace("{resource}", name)
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

fn ui(f: &mut ratatui::Frame, app: &mut App) {
    let area = f.area();

    let (body_area, footer_area) = split_main_areas(area);
    let menu_chunks = split_menu_areas(body_area);

    render_menus(f, app, &menu_chunks);

    if app.show_context_popup {
        render_context_popup(f, app);
    }

    render_footer(f, footer_area);
}

fn split_main_areas(area: ratatui::layout::Rect) -> (ratatui::layout::Rect, ratatui::layout::Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(FOOTER_HEIGHT)])
        .split(area);

    (chunks[0], chunks[1])
}

fn split_menu_areas(body_area: ratatui::layout::Rect) -> [ratatui::layout::Rect; 3] {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(20),
            Constraint::Percentage(20),
            Constraint::Percentage(60),
        ])
        .split(body_area);

    [chunks[0], chunks[1], chunks[2]]
}

fn render_menus(f: &mut ratatui::Frame, app: &mut App, menu_chunks: &[ratatui::layout::Rect; 3]) {
    for (i, menu) in app.menus.iter_mut().enumerate() {
        let is_active_menu = i == app.selected_menu_index;
        let area = menu_chunks[i];
        render_single_menu(f, menu, area, is_active_menu);
    }
}

fn render_single_menu(
    f: &mut ratatui::Frame,
    menu: &mut Menu,
    area: ratatui::layout::Rect,
    is_active_menu: bool,
) {
    let filtered_items = menu.filtered_items();
    let items = build_menu_items(menu, &filtered_items);

    let border_style = menu_border_style(is_active_menu);
    let title = build_menu_title(menu);
    let title_style = menu_title_style(is_active_menu);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(title, title_style))
        .border_style(border_style);

    let list = if menu.is_loading {
        // Если загрузка — без подсветки курсора
        List::new(items).block(block)
    } else {
        List::new(items)
            .block(block)
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD))
            .highlight_symbol("> ")
    };

    f.render_stateful_widget(list, area, &mut menu.state);
}

fn build_menu_items<'a>(menu: &Menu, filtered_items: &'a [String]) -> Vec<ListItem<'a>> {
    filtered_items
        .iter()
        .map(|s| {
            if menu.is_loading {
                // подсветка loading...
                ListItem::new(Line::from(Span::styled(
                    s.as_str(),
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::ITALIC),
                )))
            } else {
                ListItem::new(Line::from(s.as_str()))
            }
        })
        .collect()
}

fn menu_border_style(is_active_menu: bool) -> Style {
    if is_active_menu {
        Style::default().fg(Color::Green)
    } else {
        Style::default().fg(Color::Gray)
    }
}

fn menu_title_style(is_active_menu: bool) -> Style {
    if is_active_menu {
        Style::default().add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    }
}

fn build_menu_title(menu: &Menu) -> String {
    let mut title = menu.title.clone();

    if menu.filter_mode {
        title = format!("{} [/{}]", title, menu.filter);
    } else if !menu.filter.is_empty() {
        title = format!("{} (/{})", title, menu.filter);
    }

    title
}

fn render_footer(f: &mut ratatui::Frame, footer_area: ratatui::layout::Rect) {
    let footer_text = "Tab/Arrows: Navigate | /: Filter | Esc: Clear/Exit | q: Quit | ^Y: Yaml | ^D: Describe | ^L: Logs | ^X: Exec";

    let footer = Paragraph::new(footer_text)
        .style(Style::default().fg(Color::Yellow))
        .block(Block::default().borders(Borders::TOP));

    f.render_widget(footer, footer_area);
}

fn render_context_popup(f: &mut ratatui::Frame, app: &mut App) {
    let block = Block::default()
        .title("Select Context")
        .borders(Borders::ALL)
        .style(Style::default().bg(Color::DarkGray));

    let area = centered_rect(60, 50, f.area());

    // очищаем область под попапом
    f.render_widget(Clear, area);

    let items: Vec<ListItem> = app
        .context_items
        .iter()
        .map(|i| ListItem::new(Line::from(i.as_str())))
        .collect();

    let list = List::new(items)
        .block(block)
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(">> ");

    f.render_stateful_widget(list, area, &mut app.context_state);
}
