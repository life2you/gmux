#![allow(dead_code)]

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

pub struct InputState {
    pub title: String,
    pub subtitle: String,
    pub label: String,
    pub placeholder: String,
    pub value: String,
    pub error: Option<String>,
    pub cursor_pos: usize,
}

pub enum InputAction {
    Submit(String),
    Back,
    Quit,
}

impl InputState {
    pub fn new(title: &str, subtitle: &str, label: &str, placeholder: &str) -> Self {
        Self {
            title: title.to_string(),
            subtitle: subtitle.to_string(),
            label: label.to_string(),
            placeholder: placeholder.to_string(),
            value: String::new(),
            error: None,
            cursor_pos: 0,
        }
    }

    pub fn handle_key_event(&mut self) -> Option<InputAction> {
        if let Ok(Event::Key(key)) = event::read() {
            if key.kind != KeyEventKind::Press {
                return None;
            }
            match key.code {
                KeyCode::Enter => {
                    if self.value.is_empty() {
                        self.error = Some("输入不能为空".to_string());
                    } else {
                        return Some(InputAction::Submit(self.value.clone()));
                    }
                }
                KeyCode::Esc => return Some(InputAction::Back),
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    return Some(InputAction::Quit);
                }
                KeyCode::Char(c) => {
                    self.value.insert(self.cursor_pos, c);
                    self.cursor_pos += 1;
                    self.error = None;
                }
                KeyCode::Backspace => {
                    if self.cursor_pos > 0 {
                        self.cursor_pos -= 1;
                        self.value.remove(self.cursor_pos);
                        self.error = None;
                    }
                }
                KeyCode::Left => {
                    if self.cursor_pos > 0 {
                        self.cursor_pos -= 1;
                    }
                }
                KeyCode::Right => {
                    if self.cursor_pos < self.value.len() {
                        self.cursor_pos += 1;
                    }
                }
                KeyCode::Home => self.cursor_pos = 0,
                KeyCode::End => self.cursor_pos = self.value.len(),
                _ => {}
            }
        }
        None
    }

    pub fn render(&self, frame: &mut Frame) {
        let area = frame.area();

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),  // header
                Constraint::Length(2),  // label + placeholder
                Constraint::Length(2),  // error
                Constraint::Length(3),  // input
                Constraint::Min(1),    // spacer
                Constraint::Length(1), // footer
            ])
            .split(area);

        self.render_header(frame, chunks[0]);
        self.render_label(frame, chunks[1]);
        self.render_error(frame, chunks[2]);
        self.render_input(frame, chunks[3]);
        self.render_footer(frame, chunks[5]);
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

    fn render_label(&self, frame: &mut Frame, area: Rect) {
        let mut lines = vec![Line::from(Span::styled(
            format!("  {}", self.label),
            Style::default().fg(Color::Rgb(153, 153, 200)),
        ))];
        if !self.placeholder.is_empty() {
            lines.push(Line::from(Span::styled(
                format!("  例如：{}", self.placeholder),
                Style::default().fg(Color::DarkGray),
            )));
        }
        let label = Paragraph::new(lines);
        frame.render_widget(label, area);
    }

    fn render_error(&self, frame: &mut Frame, area: Rect) {
        if let Some(ref err) = self.error {
            let error = Paragraph::new(Line::from(Span::styled(
                format!("  {err}"),
                Style::default().fg(Color::Red),
            )));
            frame.render_widget(error, area);
        }
    }

    fn render_input(&self, frame: &mut Frame, area: Rect) {
        let display = if self.value.is_empty() {
            Span::styled("", Style::default().fg(Color::DarkGray))
        } else {
            Span::styled(&self.value, Style::default().fg(Color::White))
        };

        let input = Paragraph::new(Line::from(vec![
            Span::styled("> ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            display,
        ]))
        .block(Block::default());

        frame.render_widget(input, area);

        // Set cursor position
        frame.set_cursor_position((
            area.x + 2 + self.cursor_pos as u16,
            area.y,
        ));
    }

    fn render_footer(&self, frame: &mut Frame, area: Rect) {
        let footer = Paragraph::new(Line::from(vec![
            Span::styled("  Enter", Style::default().fg(Color::DarkGray)),
            Span::styled(" 确认  ", Style::default().fg(Color::DarkGray)),
            Span::styled("Esc", Style::default().fg(Color::DarkGray)),
            Span::styled(" 返回", Style::default().fg(Color::DarkGray)),
        ]));
        frame.render_widget(footer, area);
    }
}
