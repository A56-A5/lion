//! `tui/ui.rs`
//!
//! Professional multi-panel dashboard for the L.I.O.N sandbox monitor.
//! Redesigned for visual excellence and comprehensive real-time monitoring.

use ratatui::{
    prelude::*,
    widgets::{block::*, *},
};

use super::app::App;
use super::events::EventKind;

// ── Colour palette ───────────────────────────────────────────────────────────

const C_BRAND:      Color = Color::Rgb(0, 255, 230);   // vibrant cyan
const C_ACCENT:     Color = Color::Rgb(150, 100, 255); // rich purple
const C_GOOD:       Color = Color::Rgb(100, 255, 150); // bright mint green
const C_WARN:       Color = Color::Rgb(255, 180, 50);  // vivid amber
const C_BAD:        Color = Color::Rgb(255, 80, 80);   // bright coral red
const C_NET_OK:     Color = Color::Rgb(100, 200, 255); // sky blue
const C_NET_BL:     Color = Color::Rgb(255, 120, 150); // pink-ish red
const C_DIM:        Color = Color::Rgb(100, 100, 130); // muted grey-blue
const C_TEXT:       Color = Color::Rgb(220, 220, 240); // almost white
const C_HEADER_BG:  Color = Color::Rgb(25, 25, 45);    // deep navy
const C_PANEL_BG:   Color = Color::Rgb(15, 15, 25);    // shadow navy
const C_BORDER:     Color = Color::Rgb(60, 60, 90);    // subtle contrast

// ── Entry point ──────────────────────────────────────────────────────────────

pub fn render(app: &App, f: &mut Frame) {
    // Background fill
    f.render_widget(
        Block::default().style(Style::default().bg(C_PANEL_BG)),
        f.area(),
    );

    let full = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(0),    // Main Panels
            Constraint::Length(12),// Performance & Metrics
            Constraint::Length(2), // Footer
        ])
        .split(f.area());

    render_header(app, f, full[0]);
    render_main_panels(app, f, full[1]);
    render_performance_section(app, f, full[2]);
    render_footer(app, f, full[3]);
}

// ── Header ───────────────────────────────────────────────────────────────────

fn render_header(app: &App, f: &mut Frame, area: Rect) {
    let block = Block::default()
        .style(Style::default().bg(C_HEADER_BG))
        .borders(Borders::BOTTOM)
        .border_style(Style::default().fg(C_ACCENT).add_modifier(Modifier::BOLD));
    f.render_widget(block, area);

    let [left, right] = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
        .areas(area);

    // Left: Brand + Command
    let cmd = app.sandbox_info.command_str();
    let cmd_display = if cmd.is_empty() { "waiting..." } else { &cmd };
    
    let left_content = Line::from(vec![
        Span::styled(" ◈ L.I.O.N ", Style::default().bg(C_ACCENT).fg(Color::White).bold()),
        Span::styled(" SANDBOX ", Style::default().fg(C_BRAND).bold()),
        Span::styled("│ ", Style::default().fg(C_DIM)),
        Span::styled(cmd_display.to_string(), Style::default().fg(C_TEXT).italic()),
    ]);
    
    f.render_widget(
        Paragraph::new(left_content).alignment(Alignment::Left),
        inner_margin(left, 1, 1),
    );

    // Right: Global Status
    let kill_text = if app.force_kill_requested {
        Span::styled(" ● KILLING... ", Style::default().bg(C_BAD).fg(Color::White).bold())
    } else {
        Span::styled(" ● SANDBOXED ", Style::default().fg(C_GOOD).bold())
    };

    let right_content = Line::from(vec![
        Span::styled("⏱ ", Style::default().fg(C_DIM)),
        Span::styled(app.elapsed_str(), Style::default().fg(C_TEXT).bold()),
        Span::raw("  "),
        kill_text,
    ]);

    f.render_widget(
        Paragraph::new(right_content).alignment(Alignment::Right),
        inner_margin(right, 1, 1),
    );
}

// ── Main Panels (3 columns) ──────────────────────────────────────────────────

fn render_main_panels(app: &App, f: &mut Frame, area: Rect) {
    let [log_area, tree_area, status_area] = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(50), // Access Log
            Constraint::Percentage(25), // Process Tree
            Constraint::Percentage(25), // Modules & Paths
        ])
        .areas(area);

    render_log_panel(app, f, log_area);
    render_process_tree(app, f, tree_area);
    render_status_column(app, f, status_area);
}

fn render_log_panel(app: &App, f: &mut Frame, area: Rect) {
    let follow_status = if app.log_follow { "● LIVE" } else { "◌ PAUSED" };
    let follow_color = if app.log_follow { C_GOOD } else { C_WARN };

    let block = Block::default()
        .title(Line::from(vec![
            Span::styled(" ⚡ ACCESS LOG ", Style::default().fg(C_BRAND).bold()),
            Span::styled(format!(" {} ", follow_status), Style::default().fg(follow_color).bold()),
            Span::styled(format!("({}) ", app.log.len()), Style::default().fg(C_DIM)),
        ]))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(C_BORDER));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let height = inner.height as usize;
    let skip = if app.log_follow {
        app.log.len().saturating_sub(height)
    } else {
        app.log_scroll
    };

    let items: Vec<ListItem> = app.log.iter()
        .skip(skip)
        .take(height)
        .map(|ev| {
            let color = match ev.kind {
                EventKind::Blocked => C_BAD,
                EventKind::ProxyBlock => C_NET_BL,
                EventKind::Create => C_GOOD,
                EventKind::Write | EventKind::Delete => C_WARN,
                EventKind::ProxyAllow => C_NET_OK,
                _ => C_DIM,
            };

            let time = ev.timestamp.format("%H:%M:%S").to_string();
            let label = ev.kind.label();
            let path = ev.path.as_deref().unwrap_or(&ev.raw);
            
            ListItem::new(Line::from(vec![
                Span::styled(format!("{} ", time), Style::default().fg(C_DIM)),
                Span::styled(format!("{:<8} ", label), Style::default().fg(color).bold()),
                Span::styled(path.to_string(), Style::default().fg(C_TEXT)),
            ]))
        })
        .collect();

    f.render_widget(List::new(items), inner);
}

fn render_process_tree(app: &App, f: &mut Frame, area: Rect) {
    let processes = app.latest_perf.as_ref().map(|p| &p.processes);
    
    let block = Block::default()
        .title(Line::from(vec![
            Span::styled(" ◈ PROCESS TREE ", Style::default().fg(C_ACCENT).bold()),
        ]))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(C_BORDER));

    let inner = block.inner(area);
    f.render_widget(block, area);

    if let Some(procs) = processes {
        let items: Vec<ListItem> = procs.iter()
            .map(|p| {
                let mem_mb = p.mem / 1024;
                ListItem::new(Line::from(vec![
                    Span::styled(format!("{:<6} ", p.pid), Style::default().fg(C_DIM)),
                    Span::styled(p.comm.clone(), Style::default().fg(C_TEXT).bold()),
                    Span::styled(format!(" {:>4}MB", mem_mb), Style::default().fg(C_BRAND)),
                ]))
            })
            .collect();
        f.render_widget(List::new(items), inner);
    } else {
        f.render_widget(
            Paragraph::new("no data").style(Style::default().fg(C_DIM)).alignment(Alignment::Center),
            inner
        );
    }
}

fn render_status_column(app: &App, f: &mut Frame, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3), // Modules
            Constraint::Min(3), // Paths
        ])
        .split(area);

    // Modules
    let m_block = Block::default()
        .title(" ⬡ MODULES ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(C_BORDER));
    let m_inner = m_block.inner(chunks[0]);
    f.render_widget(m_block, chunks[0]);

    let modules = &app.sandbox_info.active_modules;
    let m_lines: Vec<Line> = modules.iter()
        .map(|m| Line::from(vec![
            Span::styled("✓ ", Style::default().fg(C_GOOD).bold()),
            Span::styled(m.clone(), Style::default().fg(C_TEXT)),
        ]))
        .collect();
    f.render_widget(Paragraph::new(m_lines), m_inner);

    // Paths
    let p_block = Block::default()
        .title(" ⬡ PATHS ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(C_BORDER));
    let p_inner = p_block.inner(chunks[1]);
    f.render_widget(p_block, chunks[1]);

    let paths = &app.sandbox_info.exposed_paths;
    let p_lines: Vec<Line> = paths.iter()
        .take(6)
        .map(|p| {
            let color = if p.contains("(ro)") { C_DIM } else { C_WARN };
            Line::from(Span::styled(format!("↳ {}", p), Style::default().fg(color)))
        })
        .collect();
    f.render_widget(Paragraph::new(p_lines), p_inner);
}

// ── Performance Section (Graphs) ─────────────────────────────────────────────

fn render_performance_section(app: &App, f: &mut Frame, area: Rect) {
    let block = Block::default()
        .title(Line::from(vec![
            Span::styled(" ◈ PERFORMANCE MONITOR ", Style::default().fg(C_BRAND).bold()),
        ]))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(C_BORDER));
    
    let inner = block.inner(area);
    f.render_widget(block, area);

    let [top, bottom] = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .areas(inner);

    // CPU
    let cpu_val = app.latest_perf.as_ref().map(|p| p.cpu_pct as u64).unwrap_or(0);
    let cpu_color = if cpu_val > 80 { C_BAD } else if cpu_val > 40 { C_WARN } else { C_GOOD };
    let cpu_spark = Sparkline::default()
        .data(&app.cpu_spark_data())
        .max(100)
        .style(Style::default().fg(cpu_color));
    
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("⚡ CPU USAGE ", Style::default().fg(C_DIM)),
            Span::styled(format!("{}%", cpu_val), Style::default().fg(cpu_color).bold()),
        ])),
        Rect { x: top.x, y: top.y, width: 20, height: 1 }
    );
    f.render_widget(cpu_spark, Rect { x: top.x + 20, y: top.y, width: top.width - 20, height: 5 });

    // RAM
    let ram_mb = app.latest_perf.as_ref().map(|p| p.rss_kb / 1024).unwrap_or(0);
    let ram_spark = Sparkline::default()
        .data(&app.ram_spark_data())
        .max(100)
        .style(Style::default().fg(C_ACCENT));
    
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("◉ RAM USAGE ", Style::default().fg(C_DIM)),
            Span::styled(format!("{}MB", ram_mb), Style::default().fg(C_ACCENT).bold()),
        ])),
        Rect { x: bottom.x, y: bottom.y, width: 20, height: 1 }
    );
    f.render_widget(ram_spark, Rect { x: bottom.x + 20, y: bottom.y, width: bottom.width - 20, height: 5 });
}

// ── Footer ───────────────────────────────────────────────────────────────────

fn render_footer(app: &App, f: &mut Frame, area: Rect) {
    let block = Block::default()
        .style(Style::default().bg(C_HEADER_BG))
        .borders(Borders::TOP)
        .border_style(Style::default().fg(C_BORDER));
    f.render_widget(block.clone(), area);

    let [left, right] = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .areas(block.inner(area));

    let help = Line::from(vec![
        Span::styled(" Q", Style::default().fg(C_ACCENT).bold()), Span::styled(" exit  ", Style::default().fg(C_DIM)),
        Span::styled(" K", Style::default().fg(C_BAD).bold()), Span::styled(" force-kill  ", Style::default().fg(C_DIM)),
        Span::styled(" F", Style::default().fg(C_GOOD).bold()), Span::styled(" follow  ", Style::default().fg(C_DIM)),
        Span::styled(" ↑↓", Style::default().fg(C_ACCENT).bold()), Span::styled(" scroll", Style::default().fg(C_DIM)),
    ]);
    f.render_widget(Paragraph::new(help), left);

    let stats = Line::from(vec![
        Span::styled(" EVENTS ", Style::default().fg(C_DIM)),
        Span::styled(format!("{} ", app.log.len()), Style::default().fg(C_BRAND).bold()),
        Span::styled(" BLOCKS ", Style::default().fg(C_DIM)),
        Span::styled(format!("{} ", app.count_blocked), Style::default().fg(C_BAD).bold()),
    ]);
    f.render_widget(Paragraph::new(stats).alignment(Alignment::Right), right);
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn inner_margin(area: Rect, x: u16, y: u16) -> Rect {
    Rect {
        x: area.x + x,
        y: area.y + y,
        width: area.width.saturating_sub(x * 2),
        height: area.height.saturating_sub(y * 2),
    }
}
