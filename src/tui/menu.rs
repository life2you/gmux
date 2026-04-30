use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame,
};

pub struct MenuState {
    pub title: String,
    pub subtitle: String,
    pub items: Vec<String>,
    pub details: Vec<Vec<String>>,
    pub list_state: ListState,
}

pub enum MenuAction {
    Select(usize),
    Back,
    Quit,
}

impl MenuState {
    pub fn new(title: &str, subtitle: &str, items: Vec<String>) -> Self {
        let mut list_state = ListState::default();
        if !items.is_empty() {
            list_state.select(Some(0));
        }
        Self {
            title: title.to_string(),
            subtitle: subtitle.to_string(),
            items,
            details: Vec::new(),
            list_state,
        }
    }

    pub fn with_details(mut self, details: Vec<Vec<String>>) -> Self {
        self.details = details;
        self
    }

    pub fn selected(&self) -> Option<usize> {
        self.list_state.selected()
    }

    fn move_up(&mut self) {
        if self.items.is_empty() {
            return;
        }
        let i = match self.list_state.selected() {
            Some(0) => self.items.len() - 1,
            Some(i) => i - 1,
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    fn move_down(&mut self) {
        if self.items.is_empty() {
            return;
        }
        let i = match self.list_state.selected() {
            Some(i) if i >= self.items.len() - 1 => 0,
            Some(i) => i + 1,
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    pub fn handle_key_event(&mut self) -> Option<MenuAction> {
        if let Ok(Event::Key(key)) = event::read() {
            if key.kind != KeyEventKind::Press {
                return None;
            }
            match key.code {
                KeyCode::Up | KeyCode::Char('k') => self.move_up(),
                KeyCode::Down | KeyCode::Char('j') => self.move_down(),
                KeyCode::Enter => {
                    if let Some(i) = self.selected() {
                        return Some(MenuAction::Select(i));
                    }
                }
                KeyCode::Char('b') | KeyCode::Esc => return Some(MenuAction::Back),
                KeyCode::Char('q') => return Some(MenuAction::Quit),
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
                Constraint::Length(3), // header
                Constraint::Min(5),   // list
                Constraint::Length(8), // details
                Constraint::Length(1), // footer
            ])
            .split(area);

        self.render_header(frame, chunks[0]);
        self.render_list(frame, chunks[1]);
        self.render_details(frame, chunks[2]);
        self.render_footer(frame, chunks[3]);
    }

    fn render_header(&self, frame: &mut Frame, area: Rect) {
        let header = Paragraph::new(vec![
            Line::from(Span::styled(
                format!("  {}", self.title),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                format!("  {}", self.subtitle),
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
        let selected_idx = self.list_state.selected().unwrap_or(0);

        let items: Vec<ListItem> = self
            .items
            .iter()
            .enumerate()
            .map(|(i, item)| {
                if i == selected_idx {
                    ListItem::new(Line::from(vec![
                        Span::styled("  ▶ ", Style::default().fg(Color::Cyan)),
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
                        Span::styled(
                            format!("{}. ", i + 1),
                            Style::default().fg(Color::DarkGray),
                        ),
                        Span::styled(item.as_str(), Style::default().fg(Color::Rgb(153, 153, 200))),
                    ]))
                }
            })
            .collect();

        let list = List::new(items).block(Block::default());
        frame.render_stateful_widget(list, area, &mut self.list_state);
    }

    fn render_details(&self, frame: &mut Frame, area: Rect) {
        let selected = self.list_state.selected().unwrap_or(0);
        let detail_lines: Vec<Line> = if selected < self.details.len() {
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

        let details = Paragraph::new(detail_lines).block(
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
            Span::styled("Enter", Style::default().fg(Color::DarkGray)),
            Span::styled(" 确认  ", Style::default().fg(Color::DarkGray)),
            Span::styled("b", Style::default().fg(Color::DarkGray)),
            Span::styled(" 返回  ", Style::default().fg(Color::DarkGray)),
            Span::styled("q", Style::default().fg(Color::DarkGray)),
            Span::styled(" 退出", Style::default().fg(Color::DarkGray)),
        ]));
        frame.render_widget(footer, area);
    }
}
