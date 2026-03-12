//! `tui/ui.rs`
//!
//! Rendering logic for the L.I.O.N TUI using `ratatui`.

use ratatui::{
    prelude::*,
    widgets::{block::*, *},
};

use super::app::{App, Tab};
use super::events::EventKind;

// ── Entry point ──────────────────────────────────────────────────────────────

pub fn render(app: &App, f: &mut Frame) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header/Tabs
            Constraint::Min(0),    // Main content
            Constraint::Length(1), // Footer/Status bar
        ])
        .split(f.area());

    render_header(app, f, chunks[0]);
    
    match app.current_tab {
        Tab::Events => render_events_tab(app, f, chunks[1]),
        Tab::Perf   => render_perf_tab(app, f, chunks[1]),
        Tab::Status => render_status_tab(app, f, chunks[1]),
    }

    render_footer(app, f, chunks[2]);
}

// ── Components ───────────────────────────────────────────────────────────────

fn render_header(app: &App, f: &mut Frame, area: Rect) {
    let block = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(Style::default().fg(Color::DarkGray));

    let titles = Tab::ALL.iter().map(|t| Line::from(t.title())).collect::<Vec<_>>();
    let tabs = Tabs::new(titles)
        .block(block)
        .select(app.current_tab as usize)
        .highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
                .add_modifier(Modifier::UNDERLINED),
        )
        .divider("│");

    f.render_widget(tabs, area);

    // Title in the top-right
    let title = Line::from(vec![
        Span::styled(" L.I.O.N ", Style::default().bg(Color::Cyan).fg(Color::Black).bold()),
        Span::raw(" Sandbox Monitoring "),
    ]);
    f.render_widget(Paragraph::new(title).alignment(Alignment::Right), area);
}

fn render_footer(app: &App, f: &mut Frame, area: Rect) {
    let [left, right] = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .areas(area);

    let help = Line::from(vec![
        Span::styled(" q", Style::default().bold().fg(Color::Yellow)),
        Span::raw(" quit │ "),
        Span::styled(" tab", Style::default().bold().fg(Color::Yellow)),
        Span::raw(" next │ "),
        Span::styled(" 1..3", Style::default().bold().fg(Color::Yellow)),
        Span::raw(" switch │ "),
        Span::styled(" f", Style::default().bold().fg(Color::Yellow)),
        Span::raw(if app.log_follow { " follow [on]" } else { " follow [off]" }),
    ]);
    f.render_widget(Paragraph::new(help), left);

    let stats = Line::from(vec![
        Span::raw("Elapsed: "),
        Span::styled(app.elapsed_str(), Style::default().fg(Color::Cyan)),
        Span::raw(" │ Usage: "),
        Span::styled(format!("{}%", app.cpu_pct_u64()), Style::default().fg(Color::Green)),
        Span::raw(" CPU, "),
        Span::styled(format!("{}MB", app.latest_perf.as_ref().map(|p| p.rss_kb / 1024).unwrap_or(0)), Style::default().fg(Color::Magenta)),
        Span::raw(" RAM "),
    ]);
    f.render_widget(Paragraph::new(stats).alignment(Alignment::Right), right);
}

fn render_events_tab(app: &App, f: &mut Frame, area: Rect) {
    let block = Block::default()
        .title(" Sandbox Events ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::DarkGray));

    let height = area.height.saturating_sub(2) as usize;
    let skip = if app.log_follow {
        app.log.len().saturating_sub(height)
    } else {
        app.log_scroll.saturating_sub(height.saturating_sub(1))
    };

    let items: Vec<ListItem> = app.log.iter()
        .skip(skip)
        .take(height)
        .map(|ev| {
            let color = match ev.kind {
                EventKind::Blocked    => Color::Red,
                EventKind::Missing    => Color::Yellow,
                EventKind::Write      | EventKind::Create => Color::Green,
                EventKind::ProxyBlock => Color::LightRed,
                EventKind::ProxyAllow => Color::Blue,
                _ => Color::Gray,
            };

            let style = if ev.kind == EventKind::Blocked {
                Style::default().fg(Color::Black).bg(Color::Red)
            } else {
                Style::default().fg(color)
            };

            let time = ev.timestamp.format("%H:%M:%S").to_string();
            let content = Line::from(vec![
                Span::styled(format!("{time} "), Style::default().fg(Color::DarkGray)),
                Span::styled(format!("[{}] ", ev.kind.label()), style.bold()),
                Span::raw(ev.path.as_deref().unwrap_or(&ev.raw)),
            ]);
            ListItem::new(content)
        }).collect();

    let list = List::new(items)
        .block(block)
        .highlight_style(Style::default().add_modifier(Modifier::ITALIC));

    f.render_widget(list, area);
}

fn render_perf_tab(app: &App, f: &mut Frame, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Gauges
            Constraint::Length(10), // Charts
            Constraint::Min(0),    // Details
        ])
        .split(area);

    // 1. Gauges
    let [cpu_area, ram_area] = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .areas(chunks[0]);

    let cpu_gauge = Gauge::default()
        .block(Block::default().title(" CPU Usage ").borders(Borders::ALL))
        .gauge_style(Style::default().fg(Color::Green))
        .percent(app.cpu_pct_u64() as u16);
    f.render_widget(cpu_gauge, cpu_area);

    let ram_gauge = Gauge::default()
        .block(Block::default().title(" RAM Usage ").borders(Borders::ALL))
        .gauge_style(Style::default().fg(Color::Magenta))
        .percent(app.ram_pct_u64() as u16);
    f.render_widget(ram_gauge, ram_area);

    // 2. Charts (Sparklines)
    let cpu_spark = Sparkline::default()
        .block(Block::default().title(" CPU Activity (60s) ").borders(Borders::LEFT | Borders::RIGHT))
        .data(&app.cpu_spark_data())
        .style(Style::default().fg(Color::Green));
    f.render_widget(cpu_spark, chunks[1]);

    // 3. Details Table
    let perf = app.latest_perf.as_ref();
    let rows = vec![
        Row::new(vec![Cell::from("State"), Cell::from(format!("{}", perf.map(|p| p.state).unwrap_or('?')))]),
        Row::new(vec![Cell::from("Threads"), Cell::from(format!("{}", perf.map(|p| p.threads).unwrap_or(0)))]),
        Row::new(vec![Cell::from("Resident Set (RSS)"), Cell::from(format!("{} KB", perf.map(|p| p.rss_kb).unwrap_or(0)))]),
        Row::new(vec![Cell::from("Virtual Size (VMSZ)"), Cell::from(format!("{} KB", perf.map(|p| p.vmsz_kb).unwrap_or(0)))]),
        Row::new(vec![Cell::from("Total Disk Read"), Cell::from(format!("{} KB", perf.map(|p| p.io_read_kb).unwrap_or(0)))]),
        Row::new(vec![Cell::from("Total Disk Write"), Cell::from(format!("{} KB", perf.map(|p| p.io_write_kb).unwrap_or(0)))]),
    ];

    let table = Table::new(rows, [Constraint::Length(25), Constraint::Min(0)])
        .block(Block::default().title(" Process Metrics ").borders(Borders::ALL).border_type(BorderType::Rounded))
        .header(Row::new(vec!["Metric", "Value"]).style(Style::default().bold().fg(Color::Cyan)));
    
    f.render_widget(table, chunks[2]);
}

fn render_status_tab(app: &App, f: &mut Frame, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(14), // Info + active modules + exposures
            Constraint::Min(0),    // Counters
        ])
        .split(area);

    // 1. Sandbox Info
    let info = &app.sandbox_info;
    let modules_label = if info.active_modules.is_empty() {
        "none".to_string()
    } else {
        info.active_modules.join(", ")
    };
    let mut details = vec![
        Line::from(vec![Span::from("Command: ").bold(), Span::from(info.command_str())]),
        Line::from(vec![Span::from("Network: ").bold(), Span::from(&info.network_mode)]),
        Line::from(vec![Span::from("Project access: ").bold(), Span::from(&info.project_access)]),
        Line::from(vec![Span::from("Bwrap PID: ").bold(), Span::from(info.pid.to_string())]),
        Line::from(vec![Span::from("Started: ").bold(), Span::from(info.started_at.map(|t| t.to_rfc2822()).unwrap_or_else(|| "N/A".into()))]),
        Line::from(vec![Span::from("Active modules: ").bold(), Span::from(modules_label)]),
    ];

    if !info.exposed_paths.is_empty() {
        details.push(Line::from(Span::styled("Exposed paths:", Style::default().fg(Color::Cyan).bold())));
        for path in info.exposed_paths.iter().take(4) {
            details.push(Line::from(format!("  • {path}")));
        }
        if info.exposed_paths.len() > 4 {
            details.push(Line::from(format!("  … {} more", info.exposed_paths.len() - 4)));
        }
    }
    let info_block = Paragraph::new(details)
        .block(Block::default().title(" Sandbox Environment ").borders(Borders::ALL).border_type(BorderType::Double));
    f.render_widget(info_block, chunks[0]);

    // 2. Statistics Grid
    let grid_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[1]);

    let fs_stats = vec![
        Line::from(vec![Span::raw("File Reads:    "), Span::styled(app.count_read.to_string(), Style::default().fg(Color::Cyan))]),
        Line::from(vec![Span::raw("File Writes:   "), Span::styled(app.count_write.to_string(), Style::default().fg(Color::Green))]),
        Line::from(vec![Span::raw("File Creates:  "), Span::styled(app.count_create.to_string(), Style::default().fg(Color::LightGreen))]),
        Line::from(vec![Span::raw("File Deletes:  "), Span::styled(app.count_delete.to_string(), Style::default().fg(Color::Red))]),
        Line::from(vec![Span::raw("Violations:    "), Span::styled(app.count_blocked.to_string(), Style::default().fg(Color::LightRed).bold())]),
    ];
    f.render_widget(Paragraph::new(fs_stats).block(Block::default().title(" Filesystem Activity ").borders(Borders::ALL)), grid_layout[0]);

    let net_stats = vec![
        Line::from(vec![Span::raw("Domains Allowed: "), Span::styled(app.count_net_allow.to_string(), Style::default().fg(Color::Blue))]),
        Line::from(vec![Span::raw("Domains Blocked: "), Span::styled(app.count_net_block.to_string(), Style::default().fg(Color::Red))]),
    ];
    f.render_widget(Paragraph::new(net_stats).block(Block::default().title(" Network Activity ").borders(Borders::ALL)), grid_layout[1]);
}
