use anyhow::{Context, Result}; // Context помогает понять, где упала ошибка
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
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

// --- CONFIGURATION ---
const HEADER_HEIGHT: u16 = 3;
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
    "serviceentries",
    "destinationrules",
    "authorizationpolicies",
    "virtualservices",
    "gateways",
    "telemetry",
    "envoyfilters",
];

// --- KUBECTL HELPER FUNCTIONS ---

fn run_kubectl(args: &[&str]) -> Result<Vec<String>> {
    let output = Command::new("kubectl")
        .args(args)
        .output()
        .context("Failed to execute kubectl. Is it installed and in PATH?")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow::anyhow!("Kubectl error: {}", stderr));
    }

    let stdout = String::from_utf8(output.stdout)?;
    Ok(stdout
        .lines()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect())
}

fn get_namespaces() -> Result<Vec<String>> {
    // 1. Получаем все неймспейсы
    let all_ns = run_kubectl(&[
        "get",
        "ns",
        "--no-headers",
        "-o",
        "custom-columns=NAME:.metadata.name",
    ])?;

    // 2. Получаем текущий неймспейс
    let current_ns_vec = run_kubectl(&[
        "config",
        "view",
        "--minify",
        "--output",
        "jsonpath={..namespace}",
    ]);

    // Обрабатываем случай, если current_ns пустой (контекст не задан)
    let current_ns = match current_ns_vec {
        Ok(vec) if !vec.is_empty() => Some(vec[0].clone()),
        _ => None,
    };

    // 3. Мержим: ставим текущий наверх
    if let Some(curr) = current_ns {
        let mut result = vec![curr.clone()];
        result.extend(all_ns.into_iter().filter(|ns| ns != &curr));
        Ok(result)
    } else {
        Ok(all_ns)
    }
}

fn get_api_resources() -> Result<Vec<String>> {
    // 1. Получаем список из кластера
    let output = run_kubectl(&["api-resources", "--no-headers", "--verbs=get"])?;

    // Парсим только первое слово (имя ресурса)
    let cluster_resources: Vec<String> = output
        .iter()
        .filter_map(|line| line.split_whitespace().next().map(|s| s.to_string()))
        .collect();

    // 2. Объединяем с TOP_API_RESOURCES, сохраняя порядок TOP, затем остальные
    let mut result = Vec::new();
    let mut seen = HashSet::new();

    // Сначала добавляем приоритетные
    for &res in TOP_API_RESOURCES {
        result.push(res.to_string());
        seen.insert(res.to_string());
    }

    // Затем добавляем те, что пришли из кластера, но отсутствуют в TOP
    for res in cluster_resources {
        if !seen.contains(&res) {
            result.push(res.clone());
            seen.insert(res);
        }
    }

    Ok(result)
}

// --- DATA STRUCTURES ---

struct Menu {
    title: String,
    items: Vec<String>,
    state: ListState,
    filter: String,
    filter_mode: bool,
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
        }
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

    fn next(&mut self) {
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

struct App {
    menus: Vec<Menu>,
    selected_menu_index: usize,
    should_quit: bool,
}

impl App {
    // Теперь new возвращает Result, так как мы делаем I/O операции
    fn new() -> Result<App> {
        // Загружаем реальные данные
        let namespaces = get_namespaces().context("Error fetching namespaces")?;
        let api_resources = get_api_resources().context("Error fetching api-resources")?;

        // Третье меню пока пустое (загрузку сделаем на следующем шаге)
        let resources: Vec<String> = vec![];

        let menus = vec![
            Menu::new("Namespaces", namespaces),
            Menu::new("API Resources", api_resources),
            Menu::new("Resources", resources),
        ];

        Ok(App {
            menus,
            selected_menu_index: 0,
            should_quit: false,
        })
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
}

// --- MAIN ---

fn main() -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Инициализация приложения может выдать ошибку (если нет kubectl)
    // Если ошибка - корректно восстанавливаем терминал перед выходом
    let app_result = App::new();

    let res = match app_result {
        Ok(mut app) => run_loop(&mut terminal, &mut app),
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

fn run_loop<B: ratatui::backend::Backend>(terminal: &mut Terminal<B>, app: &mut App) -> Result<()> {
    loop {
        terminal.draw(|f| ui(f, app))?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    handle_input(app, key);
                }
            }
        }

        if app.should_quit {
            return Ok(());
        }
    }
}

// --- UPDATE / LOGIC ---

fn handle_input(app: &mut App, key: event::KeyEvent) {
    let menu = app.active_menu_mut();

    if menu.filter_mode {
        match key.code {
            KeyCode::Esc => menu.exit_filter_mode(),
            KeyCode::Enter => menu.filter_mode = false,
            KeyCode::Backspace => {
                menu.filter.pop();
                menu.update_selection_after_filter();
            }
            KeyCode::Char(c) => {
                menu.filter.push(c);
                menu.update_selection_after_filter();
            }
            _ => {}
        }
    } else {
        match key.code {
            KeyCode::Char('q') => app.should_quit = true,
            KeyCode::Char('/') => menu.enter_filter_mode(),
            KeyCode::Esc => {
                if !menu.filter.is_empty() {
                    menu.exit_filter_mode();
                } else {
                    app.should_quit = true;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => menu.next(),
            KeyCode::Up | KeyCode::Char('k') => menu.previous(),
            KeyCode::Home => menu.state.select(Some(0)),
            KeyCode::Right | KeyCode::Tab => app.next_menu(),
            KeyCode::Left | KeyCode::BackTab => app.previous_menu(),
            _ => {}
        }
    }
}

// --- UI ---

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
            .map(|s| ListItem::new(Line::from(s.as_str())))
            .collect();

        let is_active_menu = i == app.selected_menu_index;

        let border_style = if is_active_menu {
            Style::default().fg(Color::Green)
        } else {
            Style::default().fg(Color::DarkGray)
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

        let list = List::new(items)
            .block(block)
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD))
            .highlight_symbol("> ");

        f.render_stateful_widget(list, menu_chunks[i], &mut menu.state);
    }

    let footer_text = "Tab/Arrows: Navigate | /: Filter | Esc: Clear/Exit | q: Quit | ^Y: Yaml | ^D: Describe | ^L: Logs | ^X: Exec";
    let footer = Paragraph::new(footer_text)
        .style(Style::default().fg(Color::Yellow))
        .block(Block::default().borders(Borders::TOP));

    f.render_widget(footer, footer_area);
}
