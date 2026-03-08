use std::sync::Arc;

use crossterm::event::KeyCode;
use gader_common::{LogEntry, NetworkPacket};
use ratatui::widgets::TableState;

pub const LEVELS: &[&str] = &["All", "DEBUG", "INFO", "WARN", "ERROR", "LOG"];

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
    pub active_level: usize,
    pub search_query: String,
    pub searching: bool,
    pub follow: bool,
    pub view: View,
    pub outbox: Vec<NetworkPacket>,
}

pub enum Action {
    Network(NetworkPacket),
    Input(KeyCode),
}

impl App {
    pub fn new() -> Self {
        Self {
            logs: Vec::with_capacity(1000),
            table_state: TableState::default(),
            should_quit: false,
            known_services: Vec::new(),
            active_filter: 0,
            active_level: 0,
            search_query: String::new(),
            searching: false,
            follow: true,
            view: View::Table,
            outbox: Vec::new(),
        }
    }

    pub fn filter_name(&self) -> &str {
        if self.active_filter == 0 {
            "All"
        } else {
            &self.known_services[self.active_filter - 1]
        }
    }

    fn matches_filters(&self, log: &LogEntry, search_lower: &str) -> bool {
        if self.active_filter != 0 {
            let svc = &self.known_services[self.active_filter - 1];
            if &log.service != svc {
                return false;
            }
        }
        if self.active_level != 0 && !log.level.eq_ignore_ascii_case(LEVELS[self.active_level]) {
            return false;
        }
        if !search_lower.is_empty() {
            let in_msg = log.message.to_ascii_lowercase().contains(search_lower);
            let in_ctx = log.context.to_ascii_lowercase().contains(search_lower);
            if !in_msg && !in_ctx {
                return false;
            }
        }
        true
    }

    pub fn filtered_logs(&self) -> Vec<(usize, &LogEntry)> {
        let q = self.search_query.to_ascii_lowercase();
        self.logs
            .iter()
            .enumerate()
            .filter(|(_, l)| self.matches_filters(l, &q))
            .map(|(i, log)| (i + 1, log))
            .collect()
    }

    pub fn filtered_len(&self) -> usize {
        let q = self.search_query.to_ascii_lowercase();
        self.logs
            .iter()
            .filter(|l| self.matches_filters(l, &q))
            .count()
    }

    pub fn selected_log(&self) -> Option<LogEntry> {
        let idx = self.table_state.selected()?;
        self.filtered_logs()
            .into_iter()
            .nth(idx)
            .map(|(_, log)| log.clone())
    }

    fn clamp_selection(&mut self) {
        let len = self.filtered_len();
        if len == 0 {
            self.table_state.select(None);
        } else if let Some(i) = self.table_state.selected() {
            self.table_state.select(Some(i.min(len - 1)));
        }
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
        let service = if self.active_filter == 0 {
            None
        } else {
            Some(self.known_services[self.active_filter - 1].to_string())
        };
        self.outbox.push(NetworkPacket::UpdateFilter { service });
        self.jump_to_latest();
    }

    fn cycle_level(&mut self) {
        self.active_level = (self.active_level + 1) % LEVELS.len();
        self.clamp_selection();
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

            Action::Input(key) => {
                if self.searching {
                    match key {
                        // All printable chars (including space) feed the query.
                        KeyCode::Char(c) => {
                            self.search_query.push(c);
                            self.clamp_selection();
                        }
                        KeyCode::Backspace => {
                            self.search_query.pop();
                            self.clamp_selection();
                        }
                        KeyCode::Esc | KeyCode::Enter => {
                            self.searching = false;
                        }
                        // Navigation still works with the bar open.
                        KeyCode::Down => self.next(),
                        KeyCode::Up => self.previous(),
                        _ => {}
                    }
                } else {
                    match key {
                        KeyCode::Char('q') => self.should_quit = true,
                        KeyCode::Esc => match self.view {
                            View::Detail => self.view = View::Table,
                            View::Table if !self.search_query.is_empty() => {
                                self.search_query.clear();
                                self.clamp_selection();
                            }
                            _ => {}
                        },
                        KeyCode::Backspace => {
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
                        KeyCode::Char('l') => {
                            if self.view == View::Table {
                                self.cycle_level();
                            }
                        }
                        KeyCode::Char('s') => {
                            if self.view == View::Table {
                                self.searching = true;
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
