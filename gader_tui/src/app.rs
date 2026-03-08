use std::sync::Arc;

use crossterm::event::KeyCode;
use gader_common::{LogEntry, NetworkPacket};
use ratatui::widgets::TableState;

#[derive(Copy, Clone, PartialEq)]
pub enum View {
    Table,
    Detail,
}

pub struct App {
    pub logs: Vec<LogEntry>,
    pub table_state: TableState,
    pub should_quit: bool,
    pub known_services: Vec<Arc<str>>,
    pub active_filter: usize,
    pub follow: bool,
    pub view: View,
}

pub enum Action {
    Network(NetworkPacket),
    Input(KeyCode),
    Tick,
    Quit,
}

impl App {
    pub fn new() -> Self {
        Self {
            logs: Vec::with_capacity(1000),
            table_state: TableState::default(),
            should_quit: false,
            known_services: Vec::new(),
            active_filter: 0,
            follow: true,
            view: View::Table,
        }
    }

    pub fn filter_name(&self) -> &str {
        if self.active_filter == 0 {
            "All"
        } else {
            &self.known_services[self.active_filter - 1]
        }
    }

    pub fn filtered_logs(&self) -> Vec<(usize, &LogEntry)> {
        if self.active_filter == 0 {
            self.logs
                .iter()
                .enumerate()
                .map(|(i, log)| (i + 1, log))
                .collect()
        } else {
            let service = &self.known_services[self.active_filter - 1];
            self.logs
                .iter()
                .enumerate()
                .filter(|(_, l)| &l.service == service)
                .map(|(i, log)| (i + 1, log))
                .collect()
        }
    }

    pub fn filtered_len(&self) -> usize {
        if self.active_filter == 0 {
            self.logs.len()
        } else {
            let service = &self.known_services[self.active_filter - 1];
            self.logs.iter().filter(|l| &l.service == service).count()
        }
    }

    pub fn selected_log(&self) -> Option<LogEntry> {
        let idx = self.table_state.selected()?;
        self.filtered_logs()
            .into_iter()
            .nth(idx)
            .map(|(_, log)| log.clone())
    }

    fn next(&mut self) {
        let len = self.filtered_len();
        if len == 0 {
            return;
        }
        let i = match self.table_state.selected() {
            Some(i) => (i + 1).min(len - 1),
            None => 0,
        };
        self.table_state.select(Some(i));
        self.follow = i >= len - 1;
    }

    fn previous(&mut self) {
        let len = self.filtered_len();
        if len == 0 {
            return;
        }
        let i = match self.table_state.selected() {
            Some(i) => i.saturating_sub(1),
            None => 0,
        };
        self.table_state.select(Some(i));
        self.follow = false;
    }

    fn jump_to_latest(&mut self) {
        let len = self.filtered_len();
        if len > 0 {
            self.table_state.select(Some(len - 1));
        }
        self.follow = true;
    }

    fn cycle_filter(&mut self) {
        let total = self.known_services.len() + 1;
        self.active_filter = (self.active_filter + 1) % total;
        self.jump_to_latest();
    }

    pub fn update(&mut self, action: Action) {
        match action {
            Action::Network(packet) => {
                if let NetworkPacket::Batch(new_logs) = packet {
                    for log in &new_logs {
                        if !self.known_services.contains(&log.service) {
                            self.known_services.push(Arc::clone(&log.service));
                        }
                    }
                    self.logs.extend(new_logs);
                    if self.follow {
                        self.jump_to_latest();
                    }
                }
            }

            Action::Input(key) => match key {
                KeyCode::Char('q') => self.should_quit = true,
                KeyCode::Esc | KeyCode::Backspace => {
                    if self.view == View::Detail {
                        self.view = View::Table;
                    }
                }
                KeyCode::Char('e') => {
                    if self.view == View::Table && self.table_state.selected().is_some() {
                        self.view = View::Detail;
                    }
                }
                KeyCode::Down => {
                    if self.view == View::Table {
                        self.next();
                    }
                }
                KeyCode::Up => {
                    if self.view == View::Table {
                        self.previous();
                    }
                }
                KeyCode::Char(' ') => {
                    if self.view == View::Table {
                        self.jump_to_latest();
                    }
                }
                KeyCode::Tab => {
                    if self.view == View::Table {
                        self.cycle_filter();
                    }
                }
                _ => {}
            },

            Action::Quit => self.should_quit = true,

            _ => {}
        }
    }
}
