use anyhow::Result;
use chrono::{DateTime, Utc};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Cell, Paragraph, Row, Table, TableState, Wrap,
    },
    Frame, Terminal,
};
use std::io;
use uuid::Uuid;

use crate::types::{CreateMappingRequest, Mapping, MappingStatus};

/// Main TUI application state
pub struct App {
    pub server_url: String,
    pub mappings: Vec<Mapping>,
    pub table_state: TableState,
    pub current_view: View,
    pub form_state: FormState,
    pub status_message: Option<String>,
    pub should_quit: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum View {
    Dashboard,
    AddMapping,
    EditMapping(Uuid),
    DeleteConfirm(Uuid),
    Help,
}

#[derive(Debug, Clone)]
pub struct FormState {
    pub s3_url: String,
    pub short_url: String,
    pub hosted_zone_id: String,
    pub presign_duration_hours: String,
    pub refresh_interval_hours: String,
    pub current_field: usize,
}

impl Default for FormState {
    fn default() -> Self {
        Self {
            s3_url: String::new(),
            short_url: String::new(),
            hosted_zone_id: String::new(),
            presign_duration_hours: "12".to_string(),
            refresh_interval_hours: "11".to_string(),
            current_field: 0,
        }
    }
}

impl FormState {
    fn clear(&mut self) {
        *self = Self::default();
    }

    fn to_request(&self) -> Result<CreateMappingRequest> {
        let presign_duration_secs = self
            .presign_duration_hours
            .parse::<u64>()
            .map_err(|_| anyhow::anyhow!("Invalid presign duration"))?
            * 3600;
        let refresh_interval_secs = self
            .refresh_interval_hours
            .parse::<u64>()
            .map_err(|_| anyhow::anyhow!("Invalid refresh interval"))?
            * 3600;

        Ok(CreateMappingRequest {
            s3_url: self.s3_url.clone(),
            short_url: self.short_url.clone(),
            hosted_zone_id: self.hosted_zone_id.clone(),
            presign_duration_secs,
            refresh_interval_secs,
        })
    }

    fn from_mapping(&mut self, mapping: &Mapping) {
        self.s3_url = mapping.s3_url.clone();
        self.short_url = mapping.short_url.clone();
        self.hosted_zone_id = mapping.hosted_zone_id.clone();
        self.presign_duration_hours = (mapping.presign_duration_secs / 3600).to_string();
        self.refresh_interval_hours = (mapping.refresh_interval_secs / 3600).to_string();
    }
}

impl App {
    pub fn new(server_url: String) -> Self {
        Self {
            server_url,
            mappings: Vec::new(),
            table_state: TableState::default(),
            current_view: View::Dashboard,
            form_state: FormState::default(),
            status_message: None,
            should_quit: false,
        }
    }

    pub fn next_row(&mut self) {
        if self.mappings.is_empty() {
            return;
        }
        let i = match self.table_state.selected() {
            Some(i) => {
                if i >= self.mappings.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.table_state.select(Some(i));
    }

    pub fn previous_row(&mut self) {
        if self.mappings.is_empty() {
            return;
        }
        let i = match self.table_state.selected() {
            Some(i) => {
                if i == 0 {
                    self.mappings.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.table_state.select(Some(i));
    }

    pub fn selected_mapping(&self) -> Option<&Mapping> {
        self.table_state
            .selected()
            .and_then(|i| self.mappings.get(i))
    }
}

/// Run the TUI
pub async fn run_tui(server_url: String) -> Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(server_url);

    // Initial data fetch
    if let Err(e) = fetch_mappings(&mut app).await {
        app.status_message = Some(format!("Error: {}", e));
    }

    let res = run_app(&mut terminal, &mut app).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("Error: {:?}", err);
    }

    Ok(())
}

async fn run_app<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> Result<()> {
    loop {
        terminal.draw(|f| ui(f, app))?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match app.current_view {
                    View::Dashboard => handle_dashboard_input(app, key.code, key.modifiers).await?,
                    View::AddMapping | View::EditMapping(_) => {
                        handle_form_input(app, key.code, key.modifiers).await?
                    }
                    View::DeleteConfirm(_) => {
                        handle_delete_confirm_input(app, key.code).await?
                    }
                    View::Help => handle_help_input(app, key.code)?,
                }
            }
        }

        if app.should_quit {
            return Ok(());
        }
    }
}

fn ui(f: &mut Frame, app: &mut App) {
    match &app.current_view {
        View::Dashboard => draw_dashboard(f, app),
        View::AddMapping => draw_form(f, app, "Add New Mapping"),
        View::EditMapping(_) => draw_form(f, app, "Edit Mapping"),
        View::DeleteConfirm(id) => draw_delete_confirm(f, app, *id),
        View::Help => draw_help(f),
    }
}

fn draw_dashboard(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(3),
        ])
        .split(f.area());

    // Header
    let title = Paragraph::new("S3 Buddy - URL Mapping Manager")
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(title, chunks[0]);

    // Table
    let header_cells = ["ID", "S3 URL", "Short URL", "Status", "Last Refresh"]
        .iter()
        .map(|h| Cell::from(*h).style(Style::default().fg(Color::Yellow)));
    let header = Row::new(header_cells).height(1).bottom_margin(1);

    let rows = app.mappings.iter().map(|m| {
        let status_color = match m.status {
            MappingStatus::Active => Color::Green,
            MappingStatus::Paused => Color::Yellow,
            MappingStatus::Error => Color::Red,
            MappingStatus::Pending => Color::Blue,
        };

        let last_refresh = m
            .last_refresh
            .map(|dt| format_datetime(dt))
            .unwrap_or_else(|| "Never".to_string());

        Row::new(vec![
            Cell::from(m.id.to_string().chars().take(8).collect::<String>()),
            Cell::from(m.s3_url.clone()),
            Cell::from(m.short_url.clone()),
            Cell::from(m.status.to_string()).style(Style::default().fg(status_color)),
            Cell::from(last_refresh),
        ])
    });

    let table = Table::new(
        rows,
        [
            Constraint::Length(10),
            Constraint::Percentage(30),
            Constraint::Percentage(25),
            Constraint::Length(10),
            Constraint::Percentage(20),
        ],
    )
    .header(header)
    .block(Block::default().borders(Borders::ALL).title("Mappings"))
    .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
    .highlight_symbol(">> ");

    f.render_stateful_widget(table, chunks[1], &mut app.table_state);

    // Footer with keybindings and status
    let keybindings = vec![
        Span::raw("a: Add | "),
        Span::raw("e: Edit | "),
        Span::raw("d: Delete | "),
        Span::raw("p: Pause/Resume | "),
        Span::raw("r: Refresh | "),
        Span::raw("?: Help | "),
        Span::raw("q: Quit"),
    ];

    let footer_text = if let Some(msg) = &app.status_message {
        vec![Line::from(msg.clone())]
    } else {
        vec![Line::from(keybindings)]
    };

    let footer = Paragraph::new(footer_text)
        .block(Block::default().borders(Borders::ALL))
        .alignment(Alignment::Left);
    f.render_widget(footer, chunks[2]);
}

fn draw_form(f: &mut Frame, app: &mut App, title: &str) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(3),
        ])
        .split(f.area());

    // Title
    let title_widget = Paragraph::new(title)
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(title_widget, chunks[0]);

    // Form fields
    let form_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(0),
        ])
        .split(chunks[1]);

    let fields = [
        ("S3 URL", &app.form_state.s3_url),
        ("Short URL", &app.form_state.short_url),
        ("Hosted Zone ID", &app.form_state.hosted_zone_id),
        ("Presign Duration (hours)", &app.form_state.presign_duration_hours),
        ("Refresh Interval (hours)", &app.form_state.refresh_interval_hours),
    ];

    for (i, (label, value)) in fields.iter().enumerate() {
        let style = if i == app.form_state.current_field {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        };

        let input = Paragraph::new(value.as_str())
            .style(style)
            .block(Block::default().borders(Borders::ALL).title(*label));
        f.render_widget(input, form_chunks[i]);
    }

    // Footer
    let footer = Paragraph::new("Tab: Next field | Shift+Tab: Previous | Enter: Submit | Esc: Cancel")
        .block(Block::default().borders(Borders::ALL))
        .alignment(Alignment::Center);
    f.render_widget(footer, chunks[2]);
}

fn draw_delete_confirm(f: &mut Frame, app: &mut App, id: Uuid) {
    let mapping = app.mappings.iter().find(|m| m.id == id);

    let area = centered_rect(60, 30, f.area());

    let text = if let Some(m) = mapping {
        format!(
            "Are you sure you want to delete this mapping?\n\nS3 URL: {}\nShort URL: {}\n\nPress 'y' to confirm or 'n' to cancel",
            m.s3_url, m.short_url
        )
    } else {
        "Mapping not found".to_string()
    };

    let paragraph = Paragraph::new(text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Confirm Delete")
                .style(Style::default().fg(Color::Red)),
        )
        .wrap(Wrap { trim: true })
        .alignment(Alignment::Center);

    f.render_widget(paragraph, area);
}

fn draw_help(f: &mut Frame) {
    let help_text = vec![
        Line::from(""),
        Line::from(vec![Span::styled(
            "S3 Buddy - Keyboard Shortcuts",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(""),
        Line::from("Dashboard View:"),
        Line::from("  ↑/↓ or j/k    - Navigate mappings"),
        Line::from("  a             - Add new mapping"),
        Line::from("  e             - Edit selected mapping"),
        Line::from("  d             - Delete selected mapping"),
        Line::from("  p             - Pause/Resume selected mapping"),
        Line::from("  r             - Refresh mappings list"),
        Line::from("  ?             - Show this help"),
        Line::from("  q             - Quit application"),
        Line::from(""),
        Line::from("Form View:"),
        Line::from("  Tab           - Next field"),
        Line::from("  Shift+Tab     - Previous field"),
        Line::from("  Enter         - Submit form"),
        Line::from("  Esc           - Cancel"),
        Line::from(""),
        Line::from("Press any key to return to dashboard"),
    ];

    let area = centered_rect(70, 80, f.area());

    let paragraph = Paragraph::new(help_text)
        .block(Block::default().borders(Borders::ALL).title("Help"))
        .alignment(Alignment::Left);

    f.render_widget(paragraph, area);
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

async fn handle_dashboard_input(
    app: &mut App,
    key: KeyCode,
    modifiers: KeyModifiers,
) -> Result<()> {
    match key {
        KeyCode::Char('q') => app.should_quit = true,
        KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => app.should_quit = true,
        KeyCode::Down | KeyCode::Char('j') => app.next_row(),
        KeyCode::Up | KeyCode::Char('k') => app.previous_row(),
        KeyCode::Char('a') => {
            app.form_state.clear();
            app.current_view = View::AddMapping;
        }
        KeyCode::Char('e') => {
            if let Some(mapping) = app.selected_mapping().cloned() {
                let id = mapping.id;
                app.form_state.from_mapping(&mapping);
                app.current_view = View::EditMapping(id);
            }
        }
        KeyCode::Char('d') => {
            if let Some(mapping) = app.selected_mapping() {
                app.current_view = View::DeleteConfirm(mapping.id);
            }
        }
        KeyCode::Char('p') => {
            if let Some(mapping) = app.selected_mapping() {
                let id = mapping.id;
                if mapping.status == MappingStatus::Paused {
                    resume_mapping(app, id).await?;
                } else {
                    pause_mapping(app, id).await?;
                }
            }
        }
        KeyCode::Char('r') => {
            fetch_mappings(app).await?;
        }
        KeyCode::Char('?') => {
            app.current_view = View::Help;
        }
        _ => {}
    }
    Ok(())
}

async fn handle_form_input(app: &mut App, key: KeyCode, modifiers: KeyModifiers) -> Result<()> {
    match key {
        KeyCode::Esc => {
            app.current_view = View::Dashboard;
            app.form_state.clear();
        }
        KeyCode::Tab => {
            if modifiers.contains(KeyModifiers::SHIFT) {
                if app.form_state.current_field > 0 {
                    app.form_state.current_field -= 1;
                } else {
                    app.form_state.current_field = 4;
                }
            } else {
                app.form_state.current_field = (app.form_state.current_field + 1) % 5;
            }
        }
        KeyCode::Enter => {
            // Submit form
            match &app.current_view {
                View::AddMapping => {
                    if let Err(e) = create_mapping(app).await {
                        app.status_message = Some(format!("Error: {}", e));
                    } else {
                        app.current_view = View::Dashboard;
                        app.form_state.clear();
                        app.status_message = Some("Mapping created successfully".to_string());
                    }
                }
                View::EditMapping(id) => {
                    let id = *id;
                    if let Err(e) = update_mapping(app, id).await {
                        app.status_message = Some(format!("Error: {}", e));
                    } else {
                        app.current_view = View::Dashboard;
                        app.form_state.clear();
                        app.status_message = Some("Mapping updated successfully".to_string());
                    }
                }
                _ => {}
            }
        }
        KeyCode::Char(c) => {
            let field = match app.form_state.current_field {
                0 => &mut app.form_state.s3_url,
                1 => &mut app.form_state.short_url,
                2 => &mut app.form_state.hosted_zone_id,
                3 => &mut app.form_state.presign_duration_hours,
                4 => &mut app.form_state.refresh_interval_hours,
                _ => return Ok(()),
            };
            field.push(c);
        }
        KeyCode::Backspace => {
            let field = match app.form_state.current_field {
                0 => &mut app.form_state.s3_url,
                1 => &mut app.form_state.short_url,
                2 => &mut app.form_state.hosted_zone_id,
                3 => &mut app.form_state.presign_duration_hours,
                4 => &mut app.form_state.refresh_interval_hours,
                _ => return Ok(()),
            };
            field.pop();
        }
        _ => {}
    }
    Ok(())
}

async fn handle_delete_confirm_input(app: &mut App, key: KeyCode) -> Result<()> {
    match key {
        KeyCode::Char('y') => {
            if let View::DeleteConfirm(id) = app.current_view {
                if let Err(e) = delete_mapping(app, id).await {
                    app.status_message = Some(format!("Error: {}", e));
                } else {
                    app.status_message = Some("Mapping deleted successfully".to_string());
                }
                app.current_view = View::Dashboard;
            }
        }
        KeyCode::Char('n') | KeyCode::Esc => {
            app.current_view = View::Dashboard;
        }
        _ => {}
    }
    Ok(())
}

fn handle_help_input(app: &mut App, _key: KeyCode) -> Result<()> {
    app.current_view = View::Dashboard;
    Ok(())
}

// API client functions
async fn fetch_mappings(app: &mut App) -> Result<()> {
    let url = format!("{}/mappings", app.server_url);
    let client = reqwest::Client::new();
    let response = client.get(&url).send().await?;

    if !response.status().is_success() {
        anyhow::bail!("Failed to fetch mappings: {}", response.status());
    }

    let data: crate::types::ListMappingsResponse = response.json().await?;
    app.mappings = data.mappings;

    // Ensure table state is valid
    if !app.mappings.is_empty() && app.table_state.selected().is_none() {
        app.table_state.select(Some(0));
    }

    Ok(())
}

async fn create_mapping(app: &mut App) -> Result<()> {
    let request = app.form_state.to_request()?;
    let url = format!("{}/mappings", app.server_url);
    let client = reqwest::Client::new();
    let response = client.post(&url).json(&request).send().await?;

    if !response.status().is_success() {
        let error_text = response.text().await?;
        anyhow::bail!("Failed to create mapping: {}", error_text);
    }

    fetch_mappings(app).await?;
    Ok(())
}

async fn update_mapping(app: &mut App, id: Uuid) -> Result<()> {
    let request = app.form_state.to_request()?;
    let update_request = crate::types::UpdateMappingRequest {
        s3_url: Some(request.s3_url),
        short_url: Some(request.short_url),
        hosted_zone_id: Some(request.hosted_zone_id),
        presign_duration_secs: Some(request.presign_duration_secs),
        refresh_interval_secs: Some(request.refresh_interval_secs),
    };

    let url = format!("{}/mappings/{}", app.server_url, id);
    let client = reqwest::Client::new();
    let response = client.put(&url).json(&update_request).send().await?;

    if !response.status().is_success() {
        let error_text = response.text().await?;
        anyhow::bail!("Failed to update mapping: {}", error_text);
    }

    fetch_mappings(app).await?;
    Ok(())
}

async fn delete_mapping(app: &mut App, id: Uuid) -> Result<()> {
    let url = format!("{}/mappings/{}", app.server_url, id);
    let client = reqwest::Client::new();
    let response = client.delete(&url).send().await?;

    if !response.status().is_success() {
        let error_text = response.text().await?;
        anyhow::bail!("Failed to delete mapping: {}", error_text);
    }

    fetch_mappings(app).await?;
    Ok(())
}

async fn pause_mapping(app: &mut App, id: Uuid) -> Result<()> {
    let url = format!("{}/mappings/{}/pause", app.server_url, id);
    let client = reqwest::Client::new();
    let response = client.post(&url).send().await?;

    if !response.status().is_success() {
        let error_text = response.text().await?;
        anyhow::bail!("Failed to pause mapping: {}", error_text);
    }

    fetch_mappings(app).await?;
    app.status_message = Some("Mapping paused".to_string());
    Ok(())
}

async fn resume_mapping(app: &mut App, id: Uuid) -> Result<()> {
    let url = format!("{}/mappings/{}/resume", app.server_url, id);
    let client = reqwest::Client::new();
    let response = client.post(&url).send().await?;

    if !response.status().is_success() {
        let error_text = response.text().await?;
        anyhow::bail!("Failed to resume mapping: {}", error_text);
    }

    fetch_mappings(app).await?;
    app.status_message = Some("Mapping resumed".to_string());
    Ok(())
}

fn format_datetime(dt: DateTime<Utc>) -> String {
    dt.format("%Y-%m-%d %H:%M:%S").to_string()
}
