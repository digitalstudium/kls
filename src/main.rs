use anyhow::Result;
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};
use std::collections::HashSet;
use std::io;
use std::process::Command;
use std::time::{Duration, Instant};
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

// --- KUBECTL HELPER FUNCTIONS ---

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

// --- SYSTEM COMMAND EXECUTOR ---

fn execute_shell_command<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    command_str: &str,
) -> Result<()> {
    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture)?;

    let status = Command::new("sh").arg("-c").arg(command_str).status();

    if let Err(e) = status {
        println!("Error executing command: {}", e);
        println!("Press Enter to continue...");
        let _ = io::stdin().read_line(&mut String::new());
    }

    execute!(io::stdout(), EnterAlternateScreen, EnableMouseCapture)?;
    enable_raw_mode()?;
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
}

struct App {
    menus: Vec<Menu>,
    selected_menu_index: usize,
    should_quit: bool,
    event_tx: mpsc::UnboundedSender<AppEvent>,
    event_rx: mpsc::UnboundedReceiver<AppEvent>,
    current_fetch_task: Option<JoinHandle<()>>,
    fetch_id: usize, // <--- Счетчик версий запросов
}

impl App {
    fn new() -> Result<App> {
        // Создаем меню сразу в состоянии загрузки
        let menus = vec![
            Menu::new_loading("Namespaces"),
            Menu::new_loading("API Resources"),
            Menu::new("Resources", vec![]),
        ];

        let (tx, rx) = mpsc::unbounded_channel();

        let app = App {
            menus,
            selected_menu_index: 0,
            should_quit: false,
            event_tx: tx,
            event_rx: rx,
            current_fetch_task: None,
            fetch_id: 0, // <--- Инициализируем нулем
        };

        app.fetch_initial_data();
        Ok(app)
    }

    fn fetch_initial_data(&self) {
        let tx_ns = self.event_tx.clone();
        tokio::spawn(async move {
            if let Ok(data) = get_namespaces_async().await {
                let _ = tx_ns.send(AppEvent::Namespaces(data));
            }
        });

        let tx_api = self.event_tx.clone();
        tokio::spawn(async move {
            if let Ok(data) = get_api_resources_async().await {
                let _ = tx_api.send(AppEvent::ApiResources(data));
            }
        });
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

    // Добавляем аргумент is_auto_refresh
    fn trigger_resource_fetch(&mut self, is_auto_refresh: bool) {
        let ns_opt = self.menus[0].selected_item();
        let kind_opt = self.menus[1].selected_item();

        if ns_opt.is_none() || kind_opt.is_none() {
            self.menus[2].set_items(vec![]);
            return;
        }
        let ns = ns_opt.unwrap();
        let kind = kind_opt.unwrap();

        // Увеличиваем ID. Теперь все предыдущие летящие запросы считаются устаревшими.
        self.fetch_id += 1;
        let my_id = self.fetch_id;

        if !is_auto_refresh {
            self.menus[2].set_loading();
        }

        if let Some(task) = &self.current_fetch_task {
            task.abort();
        }

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

            // Передаем my_id обратно вместе с результатом
            if let Ok(lines) = run_kubectl_async(args).await {
                let _ = tx.send(AppEvent::Resources(lines, my_id));
            } else {
                let _ = tx.send(AppEvent::Resources(vec![], my_id));
            }
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
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // App::new теперь возвращает управление мгновенно
    let app_result = App::new();

    let res = match app_result {
        Ok(mut app) => run_loop(&mut terminal, &mut app).await,
        Err(e) => Err(e),
    };

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        eprintln!("Error: {:?}", err);
    }

    Ok(())
}

async fn run_loop<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
) -> Result<()> {
    // Таймер для автообновления
    let mut last_tick = Instant::now();
    let tick_rate = Duration::from_secs(2); // Обновляем каждые 2 секунды

    loop {
        terminal.draw(|f| ui(f, app))?;

        while let Ok(event) = app.event_rx.try_recv() {
            match event {
                AppEvent::Namespaces(items) => {
                    app.menus[0].set_items(items);
                    if !app.menus[1].is_loading {
                        app.trigger_resource_fetch(false);
                    }
                }
                AppEvent::ApiResources(items) => {
                    app.menus[1].set_items(items);
                    if !app.menus[0].is_loading {
                        app.trigger_resource_fetch(false);
                    }
                }
                // Принимаем ID и проверяем его
                AppEvent::Resources(items, id) => {
                    if id == app.fetch_id {
                        app.menus[2].set_items(items);
                    }
                    // Если id != app.fetch_id, просто ничего не делаем (игнорируем старые данные)
                }
            }
        }

        // --- ЛОГИКА АВТООБНОВЛЕНИЯ ---
        if last_tick.elapsed() >= tick_rate {
            // Если мы не в режиме ввода фильтра и не в процессе загрузки начальных данных
            if !app.menus[2].is_loading {
                app.trigger_resource_fetch(true); // true = тихое обновление (без loading...)
            }
            last_tick = Instant::now();
        }
        // -----------------------------

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));

        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    handle_input(app, key, terminal)?;
                }
            }
        }

        if app.should_quit {
            return Ok(());
        }
    }
}

fn handle_input<B: ratatui::backend::Backend>(
    app: &mut App,
    key: event::KeyEvent,
    terminal: &mut Terminal<B>,
) -> Result<()> {
    let menu_idx = app.selected_menu_index;
    let menu = app.active_menu_mut();

    let mut selection_changed = false;
    let mut force_refresh = false;

    let is_navigation = match key.code {
        KeyCode::Down
        | KeyCode::Up
        | KeyCode::Home
        | KeyCode::End
        | KeyCode::Right
        | KeyCode::Left
        | KeyCode::Tab
        | KeyCode::BackTab => true,
        _ => false,
    };

    if is_navigation {
        match key.code {
            KeyCode::Down => {
                menu.next();
                selection_changed = true;
            }
            KeyCode::Up => {
                menu.previous();
                selection_changed = true;
            }
            KeyCode::Home => {
                if !menu.is_loading {
                    menu.state.select(Some(0));
                    selection_changed = true;
                }
            }
            KeyCode::Right | KeyCode::Tab => app.next_menu(),
            KeyCode::Left | KeyCode::BackTab => app.previous_menu(),
            _ => {}
        }
    } else if menu.filter_mode {
        match key.code {
            KeyCode::Esc => {
                menu.exit_filter_mode();
                selection_changed = true;
            }
            KeyCode::Enter => {
                menu.filter_mode = false;
            }
            KeyCode::Backspace => {
                menu.filter.pop();
                menu.update_selection_after_filter();
                selection_changed = true;
            }
            KeyCode::Char(c) => {
                menu.filter.push(c);
                menu.update_selection_after_filter();
                selection_changed = true;
            }
            _ => {}
        }
    } else {
        match key.code {
            KeyCode::Char('q') => app.should_quit = true,
            KeyCode::Char('/') => {
                if !menu.is_loading {
                    menu.enter_filter_mode()
                }
            }
            KeyCode::Char('j') => {
                menu.next();
                selection_changed = true;
            }
            KeyCode::Char('k') => {
                menu.previous();
                selection_changed = true;
            }

            KeyCode::Esc => {
                if !menu.filter_mode && !menu.filter.is_empty() {
                    menu.exit_filter_mode();
                    selection_changed = true;
                } else {
                    app.should_quit = true;
                }
            }

            // --- KEY BINDINGS (Ctrl+Key) ---
            KeyCode::Char(c) if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Some((ns, kind, name)) = app.get_current_selection() {
                    let command_template = match c {
                        'y' => Some(
                            "kubectl -n {namespace} get {api_resource} {resource} -o yaml | batcat -l yaml",
                        ),
                        'd' => Some(
                            "kubectl -n {namespace} describe {api_resource} {resource} | batcat -l yaml",
                        ),
                        'e' => Some("kubectl -n {namespace} edit {api_resource} {resource}"),
                        'l' => Some("kubectl -n {namespace} logs {resource} | hl"),
                        'x' => Some("kubectl -n {namespace} exec -it {resource} -- sh"),
                        'n' => Some(
                            "kubectl -n {namespace} debug {resource} -it --image=nicolaka/netshoot",
                        ),
                        'a' => Some(
                            "kubectl -n {namespace} logs {resource} -c istio-proxy | jaq -Rc 'fromjson? | .' --sort-keys | hl",
                        ),
                        'p' => Some(
                            "kubectl -n {namespace} exec -it {resource} -c istio-proxy -- bash",
                        ),
                        'r' => Some(
                            "kubectl get secret {resource} -n {namespace} -o yaml | yq '.data |= with_entries(.value |= @base64d)' -y | batcat -l yaml",
                        ),
                        _ => None,
                    };

                    if let Some(tmpl) = command_template {
                        let mut cmd = tmpl
                            .replace("{namespace}", &ns)
                            .replace("{api_resource}", &kind)
                            .replace("{resource}", &name);

                        if cmd.contains("batcat") {
                            cmd.push_str(BATCAT_STYLE);
                        }

                        execute_shell_command(terminal, &cmd)?;
                    }
                }
            }
            KeyCode::Delete => {
                if let Some((ns, kind, name)) = app.get_current_selection() {
                    let cmd = format!("kubectl -n {} delete {} {}", ns, kind, name);
                    execute_shell_command(terminal, &cmd)?;
                    force_refresh = true;
                }
            }
            _ => {}
        }
    }

    // Обновление ресурсов только если мы в первых двух меню и выбор изменился
    if selection_changed && (menu_idx == 0 || menu_idx == 1) {
        app.trigger_resource_fetch(false);
    }

    if force_refresh {
        app.trigger_resource_fetch(false);
    }

    Ok(())
}

fn ui(f: &mut ratatui::Frame, app: &mut App) {
    let area = f.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(FOOTER_HEIGHT)])
        .split(area);

    let body_area = chunks[0];
    let footer_area = chunks[1];

    let menu_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(20),
            Constraint::Percentage(20),
            Constraint::Percentage(60),
        ])
        .split(body_area);

    for (i, menu) in app.menus.iter_mut().enumerate() {
        let filtered_items = menu.filtered_items();

        let items: Vec<ListItem> = filtered_items
            .iter()
            .map(|s| {
                // Подсветка для loading...
                if menu.is_loading {
                    ListItem::new(Line::from(Span::styled(
                        s,
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::ITALIC),
                    )))
                } else {
                    ListItem::new(Line::from(s.as_str()))
                }
            })
            .collect();

        let is_active_menu = i == app.selected_menu_index;

        let border_style = if is_active_menu {
            Style::default().fg(Color::Green)
        } else {
            Style::default().fg(Color::Gray)
        };

        let mut title = menu.title.clone();
        if menu.filter_mode {
            title = format!("{} [/{}]", title, menu.filter);
        } else if !menu.filter.is_empty() {
            title = format!("{} (/{})", title, menu.filter);
        }

        let block = Block::default()
            .borders(Borders::ALL)
            .title(Span::styled(
                title,
                if is_active_menu {
                    Style::default().add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                },
            ))
            .border_style(border_style);

        // Если загрузка, не показываем хайлайт курсора
        let list = if menu.is_loading {
            List::new(items).block(block)
        } else {
            List::new(items)
                .block(block)
                .highlight_style(Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD))
                .highlight_symbol("> ")
        };

        f.render_stateful_widget(list, menu_chunks[i], &mut menu.state);
    }

    let footer_text = "Tab/Arrows: Navigate | /: Filter | Esc: Clear/Exit | q: Quit | ^Y: Yaml | ^D: Describe | ^L: Logs | ^X: Exec";
    let footer = Paragraph::new(footer_text)
        .style(Style::default().fg(Color::Yellow))
        .block(Block::default().borders(Borders::TOP));

    f.render_widget(footer, footer_area);
}
