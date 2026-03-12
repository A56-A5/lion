//! `tui/ui.rs`
//!
//! Professional multi-panel dashboard for the L.I.O.N sandbox monitor.
//! Layout (all panels always visible):
//!
//!  ┌─ Header ────────────────────────────────────────────────────────┐
//!  │  [L.I.O.N]  <command>                      Elapsed  CPU%  RAM  │
//!  ├──────────────────────────────┬──────────────────────────────────┤
//!  │                              │  ┌─ Modules ──────────────────┐  │
//!  │   Access Log                 │  │  ✓ X11   ✓ Wayland …      │  │
//!  │   (live events)              │  ├─ Exposed Paths ────────────┤  │
//!  │                              │  │  /home/...  /tmp/...       │  │
//!  │                              │  ├─ CPU ──────────────────────┤  │
//!  │                              │  │  [sparkline history]       │  │
//!  │                              │  ├─ RAM ──────────────────────┤  │
//!  │                              │  │  [sparkline history]       │  │
//!  │                              │  └────────────────────────────┘  │
//!  ├──────────────────────────────┴──────────────────────────────────┤
//!  │  Footer: counters & help                                        │
//!  └─────────────────────────────────────────────────────────────────┘

use ratatui::{
    prelude::*,
    widgets::{block::*, *},
};

use super::app::App;
use super::events::EventKind;

// ── Colour palette ───────────────────────────────────────────────────────────

const C_BRAND:      Color = Color::Rgb(0, 200, 200);   // cyan-ish
const C_ACCENT:     Color = Color::Rgb(130, 80, 255);  // purple
const C_GOOD:       Color = Color::Rgb(80, 220, 120);  // green
const C_WARN:       Color = Color::Rgb(255, 190, 60);  // amber
const C_BAD:        Color = Color::Rgb(240, 70, 70);   // red
const C_NET_OK:     Color = Color::Rgb(80, 140, 255);  // blue
const C_NET_BL:     Color = Color::Rgb(200, 80, 80);   // dark red
const C_DIM:        Color = Color::Rgb(90, 90, 110);   // dark grey
const C_TEXT:       Color = Color::Rgb(200, 200, 215); // light grey text
const C_HEADER_BG:  Color = Color::Rgb(18, 18, 30);    // very dark navy
const C_PANEL_BG:   Color = Color::Rgb(14, 14, 22);    // near-black
const C_BORDER:     Color = Color::Rgb(50, 50, 75);    // subtle border

// ── Entry point ──────────────────────────────────────────────────────────────

pub fn render(app: &App, f: &mut Frame) {
    // Fill the entire screen with the panel background
    f.render_widget(
        Block::default().style(Style::default().bg(C_PANEL_BG)),
        f.area(),
    );

    let full = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header bar
            Constraint::Min(0),    // Main body
            Constraint::Length(2), // Footer bar
        ])
        .split(f.area());

    render_header(app, f, full[0]);
    render_body(app, f, full[1]);
    render_footer(app, f, full[2]);
}

// ── Header ───────────────────────────────────────────────────────────────────

fn render_header(app: &App, f: &mut Frame, area: Rect) {
    let block = Block::default()
        .style(Style::default().bg(C_HEADER_BG))
        .borders(Borders::BOTTOM)
        .border_style(Style::default().fg(C_ACCENT));
    f.render_widget(block, area);

    let [left, right] = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .margin(0)
        .areas(area);

    // Left: brand + command
    let cmd = app.sandbox_info.command_str();
    let cmd_display = if cmd.is_empty() { "—".to_string() } else { cmd };
    let left_line = Line::from(vec![
        Span::styled(" ◈ L.I.O.N ", Style::default().bg(C_ACCENT).fg(Color::White).bold()),
        Span::styled(" SANDBOX MONITOR ", Style::default().fg(C_BRAND).bold()),
        Span::styled("│ ", Style::default().fg(C_DIM)),
        Span::styled(cmd_display, Style::default().fg(C_TEXT)),
    ]);
    f.render_widget(
        Paragraph::new(left_line)
            .style(Style::default().bg(C_HEADER_BG))
            .alignment(Alignment::Left),
        inner_margin(left, 0, 0),
    );

    // Right: live stats
    let perf = app.latest_perf.as_ref();
    let cpu_pct = perf.map(|p| p.cpu_pct as u64).unwrap_or(0);
    let ram_mb  = perf.map(|p| p.rss_kb / 1024).unwrap_or(0);
    let net_str = app.sandbox_info.network_mode.to_uppercase();

    let cpu_color = if cpu_pct > 80 { C_BAD } else if cpu_pct > 40 { C_WARN } else { C_GOOD };
    let ram_color = if ram_mb > 500 { C_BAD } else if ram_mb > 200 { C_WARN } else { C_GOOD };

    let right_line = Line::from(vec![
        Span::styled(" ⏱ ", Style::default().fg(C_DIM)),
        Span::styled(app.elapsed_str(), Style::default().fg(C_BRAND).bold()),
        Span::styled("  ⚡ CPU ", Style::default().fg(C_DIM)),
        Span::styled(format!("{cpu_pct:3}%"), Style::default().fg(cpu_color).bold()),
        Span::styled("  ◉ RAM ", Style::default().fg(C_DIM)),
        Span::styled(format!("{ram_mb} MB"), Style::default().fg(ram_color).bold()),
        Span::styled("  ⬡ NET ", Style::default().fg(C_DIM)),
        Span::styled(format!("{net_str} "), Style::default().fg(C_NET_OK).bold()),
    ]);
    f.render_widget(
        Paragraph::new(right_line)
            .style(Style::default().bg(C_HEADER_BG))
            .alignment(Alignment::Right),
        inner_margin(right, 0, 0),
    );
}

// ── Main body: two columns ────────────────────────────────────────────────────

fn render_body(app: &App, f: &mut Frame, area: Rect) {
    let [log_area, right_area] = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(58), Constraint::Percentage(42)])
        .areas(area);

    render_log_panel(app, f, log_area);
    render_right_column(app, f, right_area);
}

// ── Left: access log ─────────────────────────────────────────────────────────

fn render_log_panel(app: &App, f: &mut Frame, area: Rect) {
    let follow_indicator = if app.log_follow { " ●LIVE" } else { " ◌PAUSED" };
    let follow_color     = if app.log_follow { C_GOOD } else { C_WARN };

    let title = Line::from(vec![
        Span::styled("  ACCESS LOG  ", Style::default().fg(C_BRAND).bold()),
        Span::styled(follow_indicator, Style::default().fg(follow_color).bold()),
        Span::styled(format!("  ({} events) ", app.log.len()), Style::default().fg(C_DIM)),
    ]);

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(C_BORDER))
        .style(Style::default().bg(C_PANEL_BG));

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
            let (tag_color, bg) = match ev.kind {
                EventKind::Blocked    => (C_BAD,    Some(Color::Rgb(40, 10, 10))),
                EventKind::Missing    => (C_WARN,   None),
                EventKind::Create     => (C_GOOD,   None),
                EventKind::Write      => (Color::Rgb(100, 200, 150), None),
                EventKind::Delete     => (Color::Rgb(200, 120, 60), None),
                EventKind::ProxyAllow => (C_NET_OK, None),
                EventKind::ProxyBlock => (C_NET_BL, Some(Color::Rgb(40, 10, 10))),
                EventKind::Read       => (C_DIM,    None),
                EventKind::Info       => (C_ACCENT, None),
            };

            let mut row_style = Style::default().fg(C_TEXT);
            if let Some(bg_color) = bg {
                row_style = row_style.bg(bg_color);
            }

            let time = ev.timestamp.format("%H:%M:%S%.3f").to_string();
            let tag_span = Span::styled(
                format!("[{}]", ev.kind.label()),
                Style::default().fg(tag_color).bold(),
            );
            let path_text = ev.path.as_deref().unwrap_or(&ev.raw);
            // Truncate very long paths
            let path_display = if path_text.len() > 60 {
                format!("…{}", &path_text[path_text.len().saturating_sub(58)..])
            } else {
                path_text.to_string()
            };

            let content = Line::from(vec![
                Span::styled(format!(" {time} "), Style::default().fg(C_DIM)),
                tag_span,
                Span::raw(" "),
                Span::styled(path_display, row_style),
            ]);
            ListItem::new(content)
        })
        .collect();

    let list = List::new(items)
        .style(Style::default().bg(C_PANEL_BG));
    f.render_widget(list, inner);
}

// ── Right column ─────────────────────────────────────────────────────────────

fn render_right_column(app: &App, f: &mut Frame, area: Rect) {
    let module_rows = (app.sandbox_info.active_modules.len().max(1) as u16 + 2).min(6);
    let path_rows   = (app.sandbox_info.exposed_paths.len().min(4).max(1) as u16 + 2).min(7);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(module_rows),
            Constraint::Length(path_rows),
            Constraint::Length(5),
            Constraint::Length(5),
            Constraint::Min(0),
        ])
        .split(area);

    render_modules_panel(app, f, chunks[0]);
    render_paths_panel(app, f, chunks[1]);
    render_cpu_panel(app, f, chunks[2]);
    render_ram_panel(app, f, chunks[3]);
    render_metrics_table(app, f, chunks[4]);
}

fn render_modules_panel(app: &App, f: &mut Frame, area: Rect) {
    let block = Block::default()
        .title(Line::from(vec![
            Span::styled(" ⬡ OPTIONAL MODULES", Style::default().fg(C_ACCENT).bold()),
        ]))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(C_BORDER))
        .style(Style::default().bg(C_PANEL_BG));
    let inner = block.inner(area);
    f.render_widget(block, area);

    // Show enabled modules as colored badges, or "none" if empty
    let modules = &app.sandbox_info.active_modules;
    if modules.is_empty() {
        let line = Line::from(Span::styled(" none enabled", Style::default().fg(C_DIM).italic()));
        f.render_widget(Paragraph::new(line).style(Style::default().bg(C_PANEL_BG)), inner);
        return;
    }

    // Build one or two rows of module badges
    let mut lines: Vec<Line> = Vec::new();
    let mut spans_row: Vec<Span> = Vec::new();
    let mut col = 0;
    for m in modules {
        let badge = format!(" {} ", m);
        let w = badge.len() + 2;
        if col + w > inner.width as usize && col > 0 {
            lines.push(Line::from(spans_row.clone()));
            spans_row.clear();
            col = 0;
        }
        spans_row.push(Span::styled(
            format!("✓ {} ", m),
            Style::default().fg(C_GOOD).bold(),
        ));
        col += w;
    }
    if !spans_row.is_empty() {
        lines.push(Line::from(spans_row));
    }

    f.render_widget(
        Paragraph::new(lines).style(Style::default().bg(C_PANEL_BG)),
        inner,
    );
}

fn render_paths_panel(app: &App, f: &mut Frame, area: Rect) {
    let block = Block::default()
        .title(Line::from(Span::styled(
            " ⬡ EXPOSED PATHS",
            Style::default().fg(C_ACCENT).bold(),
        )))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(C_BORDER))
        .style(Style::default().bg(C_PANEL_BG));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let paths = &app.sandbox_info.exposed_paths;
    if paths.is_empty() {
        let line = Line::from(Span::styled(" —", Style::default().fg(C_DIM).italic()));
        f.render_widget(Paragraph::new(line).style(Style::default().bg(C_PANEL_BG)), inner);
        return;
    }

    let max_w = inner.width as usize;
    let lines: Vec<Line> = paths
        .iter()
        .take(4)
        .map(|p| {
            // Shorten the path to fit
            let display = if p.len() > max_w.saturating_sub(3) {
                format!("…{}", &p[p.len().saturating_sub(max_w.saturating_sub(4))..])
            } else {
                p.clone()
            };
            // Color based on access
            let color = if display.contains("(ro)") { C_DIM } else { C_WARN };
            Line::from(Span::styled(format!(" ↳ {display}"), Style::default().fg(color)))
        })
        .collect();

    f.render_widget(
        Paragraph::new(lines).style(Style::default().bg(C_PANEL_BG)),
        inner,
    );
}

fn render_cpu_panel(app: &App, f: &mut Frame, area: Rect) {
    let cpu_pct = app.latest_perf.as_ref().map(|p| p.cpu_pct as u64).unwrap_or(0);
    let cpu_color = if cpu_pct > 80 { C_BAD } else if cpu_pct > 40 { C_WARN } else { C_GOOD };

    let title = Line::from(vec![
        Span::styled(" ⚡ CPU ", Style::default().fg(cpu_color).bold()),
        Span::styled(
            format!("{cpu_pct:3}%  threads:{} ", 
                app.latest_perf.as_ref().map(|p| p.threads).unwrap_or(0)),
            Style::default().fg(C_TEXT),
        ),
    ]);

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(cpu_color))
        .style(Style::default().bg(C_PANEL_BG));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let data = app.cpu_spark_data();
    let spark = Sparkline::default()
        .data(&data)
        .max(100)
        .style(Style::default().fg(cpu_color).bg(C_PANEL_BG));
    f.render_widget(spark, inner);
}

fn render_ram_panel(app: &App, f: &mut Frame, area: Rect) {
    let rss_mb  = app.latest_perf.as_ref().map(|p| p.rss_kb / 1024).unwrap_or(0);
    let vmsz_mb = app.latest_perf.as_ref().map(|p| p.vmsz_kb / 1024).unwrap_or(0);
    let ram_color = if rss_mb > 500 { C_BAD } else if rss_mb > 200 { C_WARN } else { Color::Rgb(140, 100, 255) };

    let title = Line::from(vec![
        Span::styled(" ◉ RAM ", Style::default().fg(ram_color).bold()),
        Span::styled(
            format!("RSS {rss_mb} MB  VMSZ {vmsz_mb} MB "),
            Style::default().fg(C_TEXT),
        ),
    ]);

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ram_color))
        .style(Style::default().bg(C_PANEL_BG));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let data = app.ram_spark_data();
    let spark = Sparkline::default()
        .data(&data)
        .max(100)
        .style(Style::default().fg(ram_color).bg(C_PANEL_BG));
    f.render_widget(spark, inner);
}

fn render_metrics_table(app: &App, f: &mut Frame, area: Rect) {
    let block = Block::default()
        .title(Line::from(Span::styled(" ◈ ACTIVITY", Style::default().fg(C_BRAND).bold())))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(C_BORDER))
        .style(Style::default().bg(C_PANEL_BG));
    let inner = block.inner(area);
    f.render_widget(block, area);

    // Two-column mini grid
    let [left, right] = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .areas(inner);

    let fs_lines = vec![
        stat_line("● Reads",    app.count_read,      C_DIM),
        stat_line("● Writes",   app.count_write,     C_GOOD),
        stat_line("● Creates",  app.count_create,    C_NET_OK),
        stat_line("● Deletes",  app.count_delete,    C_WARN),
        stat_line("⚠ Blocked",  app.count_blocked,   C_BAD),
    ];
    let net_lines = vec![
        stat_line("◉ PID",          app.sandbox_info.pid as usize,  C_BRAND),
        stat_line("◉ IO Read KB",   app.latest_perf.as_ref().map(|p| p.io_read_kb as usize).unwrap_or(0), C_NET_OK),
        stat_line("◉ IO Write KB",  app.latest_perf.as_ref().map(|p| p.io_write_kb as usize).unwrap_or(0), C_WARN),
        stat_line("◉ Net Allows",   app.count_net_allow, C_NET_OK),
        stat_line("◉ Net Blocks",   app.count_net_block, C_BAD),
    ];

    f.render_widget(
        Paragraph::new(fs_lines).style(Style::default().bg(C_PANEL_BG)),
        left,
    );
    f.render_widget(
        Paragraph::new(net_lines).style(Style::default().bg(C_PANEL_BG)),
        right,
    );
}

fn stat_line(label: &str, value: usize, color: Color) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!(" {:<13}", label),
            Style::default().fg(C_DIM),
        ),
        Span::styled(
            format!("{value}"),
            Style::default().fg(color).bold(),
        ),
    ])
}

// ── Footer ───────────────────────────────────────────────────────────────────

fn render_footer(app: &App, f: &mut Frame, area: Rect) {
    let block = Block::default()
        .style(Style::default().bg(C_HEADER_BG))
        .borders(Borders::TOP)
        .border_style(Style::default().fg(C_BORDER));
    f.render_widget(block.clone(), area);

    let inner = block.inner(area);
    let [left, right] = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(65), Constraint::Percentage(35)])
        .areas(inner);

    let help = Line::from(vec![
        Span::styled(" q", Style::default().fg(C_ACCENT).bold()),
        Span::styled(" quit ", Style::default().fg(C_DIM)),
        Span::styled("  f", Style::default().fg(C_ACCENT).bold()),
        Span::styled(
            if app.log_follow { " follow:ON " } else { " follow:OFF" },
            Style::default().fg(if app.log_follow { C_GOOD } else { C_WARN }),
        ),
        Span::styled("  ↑↓", Style::default().fg(C_ACCENT).bold()),
        Span::styled(" scroll ", Style::default().fg(C_DIM)),
        Span::styled("  G ", Style::default().fg(C_ACCENT).bold()),
        Span::styled("bottom ", Style::default().fg(C_DIM)),
        Span::styled("  g ", Style::default().fg(C_ACCENT).bold()),
        Span::styled("top", Style::default().fg(C_DIM)),
    ]);
    f.render_widget(
        Paragraph::new(help).style(Style::default().bg(C_HEADER_BG)),
        left,
    );

    // Right: access summary
    let summary = Line::from(vec![
        Span::styled(format!(" ⚠ {}", app.count_blocked), Style::default().fg(C_BAD).bold()),
        Span::styled("  ", Style::default()),
        Span::styled(format!("reads:{}", app.count_read), Style::default().fg(C_DIM)),
        Span::styled("  ", Style::default()),
        Span::styled(format!("writes:{}", app.count_write), Style::default().fg(C_GOOD)),
        Span::styled("  ", Style::default()),
        Span::styled(format!("net✓:{} ✗:{} ", app.count_net_allow, app.count_net_block), Style::default().fg(C_NET_OK)),
    ]);
    f.render_widget(
        Paragraph::new(summary)
            .style(Style::default().bg(C_HEADER_BG))
            .alignment(Alignment::Right),
        right,
    );
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
