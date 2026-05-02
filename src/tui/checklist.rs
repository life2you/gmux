use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};

pub struct ChecklistState {
    pub title: String,
    pub subtitle: String,
    pub items: Vec<String>,
    pub details: Vec<Vec<String>>,
    pub list_state: ListState,
    pub selected: Vec<bool>,
    pub error: Option<String>,
    pub help_lines: Vec<String>,
    pub help_visible: bool,
}

pub enum ChecklistAction {
    Submit(Vec<usize>),
    Back,
    Quit,
}

impl ChecklistState {
    pub fn new(title: &str, subtitle: &str, items: Vec<String>) -> Self {
        let mut list_state = ListState::default();
        if !items.is_empty() {
            list_state.select(Some(0));
        }

        let selected = vec![false; items.len()];
        Self {
            title: title.to_string(),
            subtitle: subtitle.to_string(),
            items,
            details: Vec::new(),
            list_state,
            selected,
            error: None,
            help_lines: Vec::new(),
            help_visible: false,
        }
    }

    pub fn with_details(mut self, details: Vec<Vec<String>>) -> Self {
        self.details = details;
        self
    }

    pub fn with_help(mut self, lines: Vec<String>) -> Self {
        self.help_lines = lines;
        self
    }

    fn move_up(&mut self) {
        if self.items.is_empty() {
            return;
        }
        let next = match self.list_state.selected() {
            Some(0) => self.items.len() - 1,
            Some(index) => index.saturating_sub(1),
            None => 0,
        };
        self.list_state.select(Some(next));
    }

    fn move_down(&mut self) {
        if self.items.is_empty() {
            return;
        }
        let next = match self.list_state.selected() {
            Some(index) if index + 1 >= self.items.len() => 0,
            Some(index) => index + 1,
            None => 0,
        };
        self.list_state.select(Some(next));
    }

    fn toggle_selected(&mut self) {
        if let Some(index) = self.list_state.selected() {
            if let Some(selected) = self.selected.get_mut(index) {
                *selected = !*selected;
                self.error = None;
            }
        }
    }

    pub fn handle_key_event(&mut self) -> Option<ChecklistAction> {
        if let Ok(Event::Key(key)) = event::read() {
            if key.kind != KeyEventKind::Press {
                return None;
            }

            if self.help_visible {
                match key.code {
                    KeyCode::Char('?') | KeyCode::Esc | KeyCode::Char('b') => {
                        self.help_visible = false;
                    }
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        return Some(ChecklistAction::Quit);
                    }
                    _ => {}
                }
                return None;
            }

            match key.code {
                KeyCode::Up | KeyCode::Char('k') => self.move_up(),
                KeyCode::Down | KeyCode::Char('j') => self.move_down(),
                KeyCode::Char(' ') => self.toggle_selected(),
                KeyCode::Char('?') => self.help_visible = true,
                KeyCode::Enter => {
                    let indexes: Vec<usize> = self
                        .selected
                        .iter()
                        .enumerate()
                        .filter_map(|(index, selected)| selected.then_some(index))
                        .collect();
                    if indexes.is_empty() {
                        self.error = Some("至少选择一个目标分支".to_string());
                    } else {
                        return Some(ChecklistAction::Submit(indexes));
                    }
                }
                KeyCode::Esc | KeyCode::Char('b') => return Some(ChecklistAction::Back),
                KeyCode::Char('q') => return Some(ChecklistAction::Quit),
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    return Some(ChecklistAction::Quit);
                }
                _ => {}
            }
        }

        None
    }

    pub fn render(&mut self, frame: &mut Frame) {
        let area = frame.area();
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(6),
                Constraint::Length(2),
                Constraint::Length(8),
                Constraint::Length(1),
            ])
            .split(area);

        self.render_header(frame, chunks[0]);
        self.render_list(frame, chunks[1]);
        self.render_error(frame, chunks[2]);
        self.render_details(frame, chunks[3]);
        self.render_footer(frame, chunks[4]);
        if self.help_visible {
            self.render_help_overlay(frame);
        }
    }

    fn render_header(&self, frame: &mut Frame, area: Rect) {
        let selected_count = self.selected.iter().filter(|selected| **selected).count();
        let header = Paragraph::new(vec![
            Line::from(Span::styled(
                format!("  {}", self.title),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                format!(
                    "  {}  [已选 {selected_count} / {}]",
                    self.subtitle,
                    self.items.len()
                ),
                Style::default().fg(Color::DarkGray),
            )),
        ])
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(Color::Rgb(81, 81, 81))),
        );
        frame.render_widget(header, area);
    }

    fn render_list(&mut self, frame: &mut Frame, area: Rect) {
        let selected_index = self.list_state.selected();
        let items: Vec<ListItem> = self
            .items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                let marker = if self.selected[index] { "[x]" } else { "[ ]" };
                if selected_index == Some(index) {
                    ListItem::new(Line::from(vec![
                        Span::styled("  ▶ ", Style::default().fg(Color::Cyan)),
                        Span::styled(marker, Style::default().fg(Color::Yellow)),
                        Span::raw(" "),
                        Span::styled(
                            item.as_str(),
                            Style::default()
                                .fg(Color::White)
                                .add_modifier(Modifier::BOLD),
                        ),
                    ]))
                } else {
                    ListItem::new(Line::from(vec![
                        Span::raw("    "),
                        Span::styled(marker, Style::default().fg(Color::DarkGray)),
                        Span::raw(" "),
                        Span::styled(
                            item.as_str(),
                            Style::default().fg(Color::Rgb(153, 153, 200)),
                        ),
                    ]))
                }
            })
            .collect();

        let list = List::new(items).block(Block::default());
        frame.render_stateful_widget(list, area, &mut self.list_state);
    }

    fn render_error(&self, frame: &mut Frame, area: Rect) {
        if let Some(error) = &self.error {
            let widget = Paragraph::new(Line::from(Span::styled(
                format!("  {error}"),
                Style::default().fg(Color::Red),
            )));
            frame.render_widget(widget, area);
        }
    }

    fn render_details(&self, frame: &mut Frame, area: Rect) {
        let selected = self.list_state.selected().unwrap_or(0);
        let lines: Vec<Line> = if selected < self.details.len() {
            self.details[selected]
                .iter()
                .map(|line| {
                    Line::from(Span::styled(
                        format!("  {line}"),
                        Style::default().fg(Color::Rgb(153, 200, 200)),
                    ))
                })
                .collect()
        } else {
            Vec::new()
        };

        let details = Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::TOP)
                .border_style(Style::default().fg(Color::Rgb(81, 81, 81))),
        );
        frame.render_widget(details, area);
    }

    fn render_footer(&self, frame: &mut Frame, area: Rect) {
        let footer = Paragraph::new(Line::from(vec![
            Span::styled("  ↑/↓", Style::default().fg(Color::DarkGray)),
            Span::styled(" 移动  ", Style::default().fg(Color::DarkGray)),
            Span::styled("Space", Style::default().fg(Color::DarkGray)),
            Span::styled(" 勾选  ", Style::default().fg(Color::DarkGray)),
            Span::styled("?", Style::default().fg(Color::DarkGray)),
            Span::styled(" 说明  ", Style::default().fg(Color::DarkGray)),
            Span::styled("Enter", Style::default().fg(Color::DarkGray)),
            Span::styled(" 确认  ", Style::default().fg(Color::DarkGray)),
            Span::styled("b", Style::default().fg(Color::DarkGray)),
            Span::styled(" 返回  ", Style::default().fg(Color::DarkGray)),
            Span::styled("q", Style::default().fg(Color::DarkGray)),
            Span::styled(" 退出", Style::default().fg(Color::DarkGray)),
        ]));
        frame.render_widget(footer, area);
    }

    fn render_help_overlay(&self, frame: &mut Frame) {
        let area = centered_rect(80, 70, frame.area());
        let mut lines = vec![
            Line::from(Span::styled(
                "帮助说明",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::raw(""),
        ];

        if self.help_lines.is_empty() {
            lines.push(Line::from(Span::raw("• 当前页暂无额外说明")));
        } else {
            for line in &self.help_lines {
                lines.push(Line::from(Span::raw(format!("• {line}"))));
            }
        }

        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            "? / Esc / b 关闭说明",
            Style::default().fg(Color::DarkGray),
        )));

        frame.render_widget(Clear, area);
        frame.render_widget(
            Paragraph::new(lines)
                .block(
                    Block::default()
                        .title("  页面说明  ")
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(Color::Yellow)),
                )
                .wrap(Wrap { trim: false }),
            area,
        );
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
