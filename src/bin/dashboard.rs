//! CYAN FLAME™ Real-Time TUI Dashboard
//!
//! A terminal-based dashboard for monitoring:
//! - Connected GPU agents and their specifications
//! - Certificate status and revocations
//! - Network traffic and amplification metrics
//! - System health and performance

use std::io::{self, stdout};
use std::time::{Duration, Instant};
use std::sync::Arc;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use ratatui::{
    prelude::*,
    widgets::*,
};
use tokio::sync::RwLock;
use clap::Parser;

/// CYAN FLAME™ Dashboard CLI Arguments
#[derive(Parser, Debug)]
#[command(name = "cyan-flame-dashboard")]
#[command(about = "Real-time monitoring dashboard for CYAN FLAME™ Control Plane")]
struct Args {
    /// Control plane server address
    #[arg(short, long, default_value = "http://127.0.0.1:8080")]
    server: String,

    /// Refresh interval in milliseconds
    #[arg(short, long, default_value = "1000")]
    refresh: u64,

    /// API key for authentication
    #[arg(short, long)]
    api_key: Option<String>,
}

/// Dashboard state
struct DashboardState {
    /// Connected agents
    agents: Vec<AgentInfo>,
    /// Certificate statistics
    cert_stats: CertificateStats,
    /// Network metrics
    network_metrics: NetworkMetrics,
    /// System metrics
    system_metrics: SystemMetrics,
    /// Selected tab
    selected_tab: usize,
    /// Scroll offset for agent list
    agent_scroll: usize,
    /// Last update time
    last_update: Instant,
}

#[derive(Clone, Debug)]
struct AgentInfo {
    agent_id: String,
    gpu_type: String,
    gpu_name: String,
    vram_gb: f32,
    tflops: f32,
    amplification_tier: String,
    connected_at: String,
    status: String,
    cert_fingerprint: String,
}

#[derive(Clone, Debug, Default)]
struct CertificateStats {
    total_issued: u64,
    active: u64,
    revoked: u64,
    expired: u64,
    pending_renewal: u64,
}

#[derive(Clone, Debug, Default)]
struct NetworkMetrics {
    total_connections: u64,
    active_connections: u64,
    bytes_in: u64,
    bytes_out: u64,
    requests_per_sec: f64,
    avg_latency_ms: f64,
    calibration_requests: u64,
    gpu_registrations: u64,
}

#[derive(Clone, Debug, Default)]
struct SystemMetrics {
    cpu_usage: f32,
    memory_usage: f32,
    memory_total_gb: f32,
    uptime_secs: u64,
    grpc_port: u16,
    http_port: u16,
    tls_enabled: bool,
    mtls_enabled: bool,
}

impl Default for DashboardState {
    fn default() -> Self {
        Self {
            agents: Vec::new(),
            cert_stats: CertificateStats::default(),
            network_metrics: NetworkMetrics::default(),
            system_metrics: SystemMetrics::default(),
            selected_tab: 0,
            agent_scroll: 0,
            last_update: Instant::now(),
        }
    }
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let args = Args::parse();
    
    // Initialize terminal
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;

    // Create shared state
    let state = Arc::new(RwLock::new(DashboardState::default()));
    
    // Spawn background data fetcher
    let state_clone = state.clone();
    let server = args.server.clone();
    let api_key = args.api_key.clone();
    tokio::spawn(async move {
        fetch_data_loop(state_clone, server, api_key).await;
    });

    // Run main loop
    let result = run_dashboard(&mut terminal, state, args.refresh).await;

    // Restore terminal
    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;

    result
}

async fn run_dashboard(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    state: Arc<RwLock<DashboardState>>,
    refresh_ms: u64,
) -> io::Result<()> {
    let tick_rate = Duration::from_millis(refresh_ms);
    let mut last_tick = Instant::now();

    loop {
        // Draw UI
        let state_read = state.read().await;
        terminal.draw(|frame| draw_ui(frame, &state_read))?;
        drop(state_read);

        // Handle input
        let timeout = tick_rate.saturating_sub(last_tick.elapsed());
        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    let mut state_write = state.write().await;
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                        KeyCode::Tab => state_write.selected_tab = (state_write.selected_tab + 1) % 4,
                        KeyCode::Up => state_write.agent_scroll = state_write.agent_scroll.saturating_sub(1),
                        KeyCode::Down => state_write.agent_scroll = state_write.agent_scroll.saturating_add(1),
                        _ => {}
                    }
                }
            }
        }

        if last_tick.elapsed() >= tick_rate {
            last_tick = Instant::now();
        }
    }
}

fn draw_ui(frame: &mut Frame, state: &DashboardState) {
    let area = frame.size();

    // Create main layout
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Header
            Constraint::Length(3),  // Tabs
            Constraint::Min(10),    // Content
            Constraint::Length(3),  // Footer
        ])
        .split(area);

    // Draw header with ASCII art banner
    draw_header(frame, chunks[0]);

    // Draw tabs
    draw_tabs(frame, chunks[1], state.selected_tab);

    // Draw content based on selected tab
    match state.selected_tab {
        0 => draw_agents_tab(frame, chunks[2], state),
        1 => draw_certificates_tab(frame, chunks[2], state),
        2 => draw_network_tab(frame, chunks[2], state),
        3 => draw_system_tab(frame, chunks[2], state),
        _ => {}
    }

    // Draw footer
    draw_footer(frame, chunks[3], state);
}

fn draw_header(frame: &mut Frame, area: Rect) {
    let header = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("╔═══════════════════════════════════════════════════════════════════════════════╗", Style::default().fg(Color::Cyan)),
        ]),
        Line::from(vec![
            Span::styled("║  ", Style::default().fg(Color::Cyan)),
            Span::styled("🔥 CYAN FLAME™ ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled("Control Plane Dashboard", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled(" │ ", Style::default().fg(Color::DarkGray)),
            Span::styled("SYMMETRIX CORE™", Style::default().fg(Color::Magenta)),
            Span::styled("                        ║", Style::default().fg(Color::Cyan)),
        ]),
        Line::from(vec![
            Span::styled("╚═══════════════════════════════════════════════════════════════════════════════╝", Style::default().fg(Color::Cyan)),
        ]),
    ]);
    frame.render_widget(header, area);
}

fn draw_tabs(frame: &mut Frame, area: Rect, selected: usize) {
    let titles = vec!["🖥️  Agents", "📜 Certificates", "🌐 Network", "⚙️  System"];
    let tabs = Tabs::new(titles)
        .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::DarkGray)))
        .select(selected)
        .style(Style::default().fg(Color::White))
        .highlight_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .divider(Span::raw(" │ "));
    frame.render_widget(tabs, area);
}

fn draw_agents_tab(frame: &mut Frame, area: Rect, state: &DashboardState) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
        .split(area);

    // Agent list
    let header = Row::new(vec!["Agent ID", "GPU Type", "VRAM", "TFLOPS", "Tier", "Status"])
        .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
        .bottom_margin(1);

    let rows: Vec<Row> = state.agents.iter().map(|agent| {
        let status_style = match agent.status.as_str() {
            "active" => Style::default().fg(Color::Green),
            "idle" => Style::default().fg(Color::Yellow),
            _ => Style::default().fg(Color::Red),
        };
        Row::new(vec![
            Cell::from(agent.agent_id.chars().take(12).collect::<String>()),
            Cell::from(agent.gpu_type.clone()),
            Cell::from(format!("{:.0}GB", agent.vram_gb)),
            Cell::from(format!("{:.1}", agent.tflops)),
            Cell::from(agent.amplification_tier.clone()),
            Cell::from(agent.status.clone()).style(status_style),
        ])
    }).collect();

    let table = Table::new(rows, [
        Constraint::Length(14),
        Constraint::Length(12),
        Constraint::Length(8),
        Constraint::Length(8),
        Constraint::Length(12),
        Constraint::Length(10),
    ])
    .header(header)
    .block(Block::default()
        .title(" Connected GPU Agents ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan)));

    frame.render_widget(table, chunks[0]);

    // Agent summary
    let summary = vec![
        Line::from(vec![
            Span::styled("Total Agents: ", Style::default().fg(Color::Gray)),
            Span::styled(format!("{}", state.agents.len()), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("By GPU Type:", Style::default().fg(Color::Yellow)),
        ]),
        Line::from(vec![
            Span::styled("  H100: ", Style::default().fg(Color::Gray)),
            Span::styled(format!("{}", state.agents.iter().filter(|a| a.gpu_type == "H100").count()), Style::default().fg(Color::Green)),
        ]),
        Line::from(vec![
            Span::styled("  H200: ", Style::default().fg(Color::Gray)),
            Span::styled(format!("{}", state.agents.iter().filter(|a| a.gpu_type == "H200").count()), Style::default().fg(Color::Green)),
        ]),
        Line::from(vec![
            Span::styled("  RTX 4090: ", Style::default().fg(Color::Gray)),
            Span::styled(format!("{}", state.agents.iter().filter(|a| a.gpu_type == "RTX4090").count()), Style::default().fg(Color::Green)),
        ]),
        Line::from(vec![
            Span::styled("  RTX 5090: ", Style::default().fg(Color::Gray)),
            Span::styled(format!("{}", state.agents.iter().filter(|a| a.gpu_type == "RTX5090").count()), Style::default().fg(Color::Green)),
        ]),
        Line::from(vec![
            Span::styled("  L40S: ", Style::default().fg(Color::Gray)),
            Span::styled(format!("{}", state.agents.iter().filter(|a| a.gpu_type == "L40S").count()), Style::default().fg(Color::Green)),
        ]),
    ];

    let summary_widget = Paragraph::new(summary)
        .block(Block::default()
            .title(" Summary ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan)));

    frame.render_widget(summary_widget, chunks[1]);
}

fn draw_certificates_tab(frame: &mut Frame, area: Rect, state: &DashboardState) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    // Certificate stats
    let stats = &state.cert_stats;
    let cert_data = vec![
        ("Total Issued", stats.total_issued, Color::White),
        ("Active", stats.active, Color::Green),
        ("Revoked", stats.revoked, Color::Red),
        ("Expired", stats.expired, Color::Yellow),
        ("Pending Renewal", stats.pending_renewal, Color::Cyan),
    ];

    let cert_lines: Vec<Line> = cert_data.iter().map(|(label, value, color)| {
        Line::from(vec![
            Span::styled(format!("{:.<20}", label), Style::default().fg(Color::Gray)),
            Span::styled(format!("{:>8}", value), Style::default().fg(*color).add_modifier(Modifier::BOLD)),
        ])
    }).collect();

    let cert_widget = Paragraph::new(cert_lines)
        .block(Block::default()
            .title(" 📜 Certificate Statistics ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan)));

    frame.render_widget(cert_widget, chunks[0]);

    // Certificate health gauge
    let active_pct = if stats.total_issued > 0 {
        (stats.active as f64 / stats.total_issued as f64 * 100.0) as u16
    } else {
        0
    };

    let gauge = Gauge::default()
        .block(Block::default()
            .title(" Certificate Health ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan)))
        .gauge_style(Style::default().fg(Color::Green).bg(Color::DarkGray))
        .percent(active_pct)
        .label(format!("{}% Active", active_pct));

    frame.render_widget(gauge, chunks[1]);
}

fn draw_network_tab(frame: &mut Frame, area: Rect, state: &DashboardState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    let metrics = &state.network_metrics;

    // Connection stats
    let conn_lines = vec![
        Line::from(vec![
            Span::styled("Total Connections: ", Style::default().fg(Color::Gray)),
            Span::styled(format!("{}", metrics.total_connections), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled("Active Connections: ", Style::default().fg(Color::Gray)),
            Span::styled(format!("{}", metrics.active_connections), Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled("Requests/sec: ", Style::default().fg(Color::Gray)),
            Span::styled(format!("{:.2}", metrics.requests_per_sec), Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled("Avg Latency: ", Style::default().fg(Color::Gray)),
            Span::styled(format!("{:.2}ms", metrics.avg_latency_ms), Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        ]),
    ];

    let conn_widget = Paragraph::new(conn_lines)
        .block(Block::default()
            .title(" 🌐 Connection Metrics ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan)));

    frame.render_widget(conn_widget, chunks[0]);

    // Traffic stats
    let traffic_lines = vec![
        Line::from(vec![
            Span::styled("Bytes In: ", Style::default().fg(Color::Gray)),
            Span::styled(format_bytes(metrics.bytes_in), Style::default().fg(Color::Green)),
        ]),
        Line::from(vec![
            Span::styled("Bytes Out: ", Style::default().fg(Color::Gray)),
            Span::styled(format_bytes(metrics.bytes_out), Style::default().fg(Color::Blue)),
        ]),
        Line::from(vec![
            Span::styled("Calibration Requests: ", Style::default().fg(Color::Gray)),
            Span::styled(format!("{}", metrics.calibration_requests), Style::default().fg(Color::Magenta)),
        ]),
        Line::from(vec![
            Span::styled("GPU Registrations: ", Style::default().fg(Color::Gray)),
            Span::styled(format!("{}", metrics.gpu_registrations), Style::default().fg(Color::Yellow)),
        ]),
    ];

    let traffic_widget = Paragraph::new(traffic_lines)
        .block(Block::default()
            .title(" 📊 Traffic Statistics ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan)));

    frame.render_widget(traffic_widget, chunks[1]);
}

fn draw_system_tab(frame: &mut Frame, area: Rect, state: &DashboardState) {
    let metrics = &state.system_metrics;

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    // System info
    let sys_lines = vec![
        Line::from(vec![
            Span::styled("gRPC Port: ", Style::default().fg(Color::Gray)),
            Span::styled(format!("{}", metrics.grpc_port), Style::default().fg(Color::Cyan)),
        ]),
        Line::from(vec![
            Span::styled("HTTP Port: ", Style::default().fg(Color::Gray)),
            Span::styled(format!("{}", metrics.http_port), Style::default().fg(Color::Cyan)),
        ]),
        Line::from(vec![
            Span::styled("TLS Enabled: ", Style::default().fg(Color::Gray)),
            Span::styled(if metrics.tls_enabled { "✓ Yes" } else { "✗ No" },
                Style::default().fg(if metrics.tls_enabled { Color::Green } else { Color::Red })),
        ]),
        Line::from(vec![
            Span::styled("mTLS Enabled: ", Style::default().fg(Color::Gray)),
            Span::styled(if metrics.mtls_enabled { "✓ Yes" } else { "✗ No" },
                Style::default().fg(if metrics.mtls_enabled { Color::Green } else { Color::Red })),
        ]),
        Line::from(vec![
            Span::styled("Uptime: ", Style::default().fg(Color::Gray)),
            Span::styled(format_duration(metrics.uptime_secs), Style::default().fg(Color::White)),
        ]),
    ];

    let sys_widget = Paragraph::new(sys_lines)
        .block(Block::default()
            .title(" ⚙️  System Configuration ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan)));

    frame.render_widget(sys_widget, chunks[0]);

    // Resource gauges
    let gauge_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[1]);

    let cpu_gauge = Gauge::default()
        .block(Block::default().title(" CPU Usage ").borders(Borders::ALL).border_style(Style::default().fg(Color::Cyan)))
        .gauge_style(Style::default().fg(Color::Yellow).bg(Color::DarkGray))
        .percent(metrics.cpu_usage as u16)
        .label(format!("{:.1}%", metrics.cpu_usage));

    let mem_gauge = Gauge::default()
        .block(Block::default().title(" Memory Usage ").borders(Borders::ALL).border_style(Style::default().fg(Color::Cyan)))
        .gauge_style(Style::default().fg(Color::Magenta).bg(Color::DarkGray))
        .percent(metrics.memory_usage as u16)
        .label(format!("{:.1}% of {:.1}GB", metrics.memory_usage, metrics.memory_total_gb));

    frame.render_widget(cpu_gauge, gauge_chunks[0]);
    frame.render_widget(mem_gauge, gauge_chunks[1]);
}

fn draw_footer(frame: &mut Frame, area: Rect, state: &DashboardState) {
    let elapsed = state.last_update.elapsed();
    let footer = Paragraph::new(vec![
        Line::from(vec![
            Span::styled(" [Tab] ", Style::default().fg(Color::Yellow)),
            Span::styled("Switch tabs", Style::default().fg(Color::Gray)),
            Span::styled(" │ ", Style::default().fg(Color::DarkGray)),
            Span::styled("[↑↓] ", Style::default().fg(Color::Yellow)),
            Span::styled("Scroll", Style::default().fg(Color::Gray)),
            Span::styled(" │ ", Style::default().fg(Color::DarkGray)),
            Span::styled("[q/Esc] ", Style::default().fg(Color::Yellow)),
            Span::styled("Quit", Style::default().fg(Color::Gray)),
            Span::styled(" │ ", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("Last update: {:.1}s ago", elapsed.as_secs_f64()), Style::default().fg(Color::DarkGray)),
        ]),
    ])
    .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::DarkGray)));

    frame.render_widget(footer, area);
}

fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

fn format_duration(secs: u64) -> String {
    let days = secs / 86400;
    let hours = (secs % 86400) / 3600;
    let mins = (secs % 3600) / 60;
    let secs = secs % 60;

    if days > 0 {
        format!("{}d {}h {}m", days, hours, mins)
    } else if hours > 0 {
        format!("{}h {}m {}s", hours, mins, secs)
    } else if mins > 0 {
        format!("{}m {}s", mins, secs)
    } else {
        format!("{}s", secs)
    }
}

async fn fetch_data_loop(
    state: Arc<RwLock<DashboardState>>,
    server: String,
    _api_key: Option<String>,
) {
    let client = reqwest::Client::new();
    let mut interval = tokio::time::interval(Duration::from_secs(2));

    loop {
        interval.tick().await;

        // Fetch system metrics
        if let Ok(response) = client.get(format!("{}/api/v1/status", server)).send().await {
            if let Ok(json) = response.json::<serde_json::Value>().await {
                let mut state_write = state.write().await;

                // Parse system metrics
                if let Some(system) = json.get("system") {
                    state_write.system_metrics.cpu_usage = system.get("cpu_usage")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0) as f32;
                    state_write.system_metrics.memory_usage = system.get("memory_usage")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0) as f32;
                }

                state_write.last_update = Instant::now();
            }
        }

        // Generate demo data for visualization
        let mut state_write = state.write().await;

        // Demo agents
        if state_write.agents.is_empty() {
            state_write.agents = vec![
                AgentInfo {
                    agent_id: "agent-h100-001".to_string(),
                    gpu_type: "H100".to_string(),
                    gpu_name: "NVIDIA H100 80GB HBM3".to_string(),
                    vram_gb: 80.0,
                    tflops: 1979.0,
                    amplification_tier: "Enterprise".to_string(),
                    connected_at: "2024-01-15 10:30:00".to_string(),
                    status: "active".to_string(),
                    cert_fingerprint: "a1b2c3d4...".to_string(),
                },
                AgentInfo {
                    agent_id: "agent-4090-002".to_string(),
                    gpu_type: "RTX4090".to_string(),
                    gpu_name: "NVIDIA GeForce RTX 4090".to_string(),
                    vram_gb: 24.0,
                    tflops: 660.0,
                    amplification_tier: "Pro".to_string(),
                    connected_at: "2024-01-15 11:45:00".to_string(),
                    status: "active".to_string(),
                    cert_fingerprint: "e5f6g7h8...".to_string(),
                },
            ];
        }

        // Demo certificate stats
        state_write.cert_stats = CertificateStats {
            total_issued: 150,
            active: 142,
            revoked: 3,
            expired: 5,
            pending_renewal: 12,
        };

        // Demo network metrics
        state_write.network_metrics.total_connections += 1;
        state_write.network_metrics.active_connections = state_write.agents.len() as u64;
        state_write.network_metrics.bytes_in += 1024 * 50;
        state_write.network_metrics.bytes_out += 1024 * 200;
        state_write.network_metrics.requests_per_sec = 45.7;
        state_write.network_metrics.avg_latency_ms = 2.3;
        state_write.network_metrics.calibration_requests += 1;

        // Demo system metrics
        state_write.system_metrics.grpc_port = 50051;
        state_write.system_metrics.http_port = 8080;
        state_write.system_metrics.tls_enabled = true;
        state_write.system_metrics.mtls_enabled = true;
        state_write.system_metrics.uptime_secs += 2;
        state_write.system_metrics.memory_total_gb = 32.0;
    }
}

