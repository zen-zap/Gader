use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table, Wrap},
};

use crate::app::{App, LEVELS, View};

fn level_style(level: &str) -> Style {
    match level {
        "ERROR" | "FATAL" => Style::default().fg(Color::Red),
        "WARN" => Style::default().fg(Color::Yellow),
        "INFO" => Style::default().fg(Color::Green),
        "DEBUG" => Style::default().fg(Color::Blue),
        _ => Style::default().fg(Color::White),
    }
}

fn truncate(s: &str, max_chars: usize) -> String {
    let s = s.trim();
    if s.chars().count() > max_chars {
        let t: String = s.chars().take(max_chars - 3).collect();
        format!("{}...", t)
    } else {
        s.to_owned()
    }
}

pub fn view(f: &mut Frame, app: &mut App) {
    match app.view {
        View::Table => render_table(f, app),
        View::Detail => render_detail(f, app),
    }
}

fn render_table(f: &mut Frame, app: &mut App) {
    let has_search = app.searching || !app.search_query.is_empty();

    let mut constraints = vec![Constraint::Length(3), Constraint::Min(1)];
    if has_search {
        constraints.push(Constraint::Length(1));
    }
    constraints.push(Constraint::Length(1));

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(f.area());

    let service_name = app.filter_name().to_owned();
    let level_name = LEVELS[app.active_level];
    let follow = app.follow;
    let total_count = app.logs.len();

    let (visible_count, rows): (usize, Vec<Row>) = {
        let filtered = app.filtered_logs();
        let count = filtered.len();
        let rows = filtered
            .into_iter()
            .map(|(arrival, log)| {
                Row::new(vec![
                    Cell::from(format!("{arrival}")),
                    Cell::from(log.timestamp.to_string()),
                    Cell::from(log.service.to_string()).style(Style::default().fg(Color::Magenta)),
                    Cell::from(log.level.to_string()).style(level_style(&log.level)),
                    Cell::from(truncate(&log.message, 80)),
                ])
            })
            .collect();
        (count, rows)
    };

    let follow_indicator = if follow { " [LIVE]" } else { "" };
    let header_text = format!(
        " Gader | {visible_count}/{total_count} | Svc: {service_name} | Lvl: {level_name}{follow_indicator} ",
    );
    let header = Paragraph::new(header_text)
        .block(Block::default().borders(Borders::ALL).title(" Status "))
        .style(Style::default().fg(Color::Cyan));
    f.render_widget(header, chunks[0]);

    let col_header = Row::new(vec![
        Cell::from("  #").style(
            Style::default()
                .fg(Color::Gray)
                .add_modifier(Modifier::BOLD),
        ),
        Cell::from("Timestamp").style(
            Style::default()
                .fg(Color::Gray)
                .add_modifier(Modifier::BOLD),
        ),
        Cell::from("Service").style(
            Style::default()
                .fg(Color::Gray)
                .add_modifier(Modifier::BOLD),
        ),
        Cell::from("Level").style(
            Style::default()
                .fg(Color::Gray)
                .add_modifier(Modifier::BOLD),
        ),
        Cell::from("Message").style(
            Style::default()
                .fg(Color::Gray)
                .add_modifier(Modifier::BOLD),
        ),
    ]);

    let table = Table::new(
        rows,
        [
            Constraint::Length(6),
            Constraint::Length(26),
            Constraint::Length(12),
            Constraint::Length(7),
            Constraint::Min(10),
        ],
    )
    .header(col_header)
    .block(Block::default().borders(Borders::ALL).title(" Live Logs "))
    .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED))
    .highlight_symbol(">> ");

    f.render_stateful_widget(table, chunks[1], &mut app.table_state);

    let (search_chunk, footer_chunk) = if has_search { (Some(2usize), 3usize) } else { (None, 2) };

    if let Some(idx) = search_chunk {
        let (text, style) = if app.searching {
            (
                format!(" / {}█", app.search_query),
                Style::default().fg(Color::Yellow),
            )
        } else {
            (
                format!(" / {}  (Esc: clear)", app.search_query),
                Style::default().fg(Color::DarkGray),
            )
        };
        f.render_widget(Paragraph::new(text).style(style), chunks[idx]);
    }

    let footer_text = if app.searching {
        "Esc/Enter: Close search | Backspace: Delete | ↑↓: Navigate results"
    } else {
        "q: Quit | ↑↓/Scroll: Navigate | Space: Latest | Tab: Service | l: Level | s: Search | e: Expand"
    };
    f.render_widget(
        Paragraph::new(footer_text).style(Style::default().fg(Color::DarkGray)),
        chunks[footer_chunk],
    );
}

fn render_detail(f: &mut Frame, app: &App) {
    let area = f.area();

    let log = match app.selected_log() {
        Some(l) => l,
        None => {
            let msg = Paragraph::new("No entry selected. Press Backspace to go back.")
                .block(Block::default().borders(Borders::ALL).title(" Log Detail "));
            f.render_widget(msg, area);
            return;
        }
    };

    let arrival = app.table_state.selected().map(|i| i + 1).unwrap_or(0);

    let mut content: Vec<Line> = vec![
        Line::from(vec![
            Span::styled("  Timestamp : ", Style::default().fg(Color::DarkGray)),
            Span::raw(log.timestamp.to_string()),
        ]),
        Line::from(vec![
            Span::styled("  Service   : ", Style::default().fg(Color::Magenta)),
            Span::raw(log.service.to_string()),
        ]),
        Line::from(vec![
            Span::styled("  Level     : ", Style::default().fg(Color::DarkGray)),
            Span::styled(log.level.to_string(), level_style(&log.level)),
        ]),
        Line::from(vec![
            Span::styled("  Context   : ", Style::default().fg(Color::DarkGray)),
            Span::raw(log.context.to_string()),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "  Message",
            Style::default()
                .fg(Color::Gray)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            "  ───────────────────────────────────────────── ",
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(""),
    ];

    for line in log.message.lines() {
        content.push(Line::from(format!("  {line}")));
    }

    let detail_area = Rect {
        height: area.height.saturating_sub(1),
        ..area
    };
    let footer_area = Rect {
        y: area.y + area.height.saturating_sub(1),
        height: 1,
        ..area
    };

    let detail = Paragraph::new(content)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" Log Detail  [#{}] ", arrival)),
        )
        .wrap(Wrap { trim: false });
    f.render_widget(detail, detail_area);

    let footer =
        Paragraph::new("Backspace / Esc: Back to logs").style(Style::default().fg(Color::DarkGray));
    f.render_widget(footer, footer_area);
}
