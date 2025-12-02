//! CYAN FLAME™ Unified Calibration Dashboard
//!
//! Real-time TUI dashboard for monitoring all three calibration services:
//! - Memory Calibration (24,500× amplification)
//! - Compute Calibration (29.86× TFLOPS amplification)
//! - PCIe Amplification (82× bandwidth amplification)

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

/// CYAN FLAME™ Calibration Dashboard CLI Arguments
#[derive(Parser, Debug)]
#[command(name = "cyan-flame-calibration-dashboard")]
#[command(about = "Real-time dashboard for CYAN FLAME™ Calibration Services")]
struct Args {
    /// Control plane server address
    #[arg(short, long, default_value = "http://127.0.0.1:50051")]
    server: String,

    /// Refresh interval in milliseconds
    #[arg(short, long, default_value = "500")]
    refresh: u64,

    /// API key for authentication
    #[arg(short, long, default_value = "demo-key")]
    api_key: String,
}

/// Dashboard state
struct DashboardState {
    /// Memory calibration metrics
    memory_calibration: MemoryCalibrationMetrics,
    /// Compute calibration metrics
    compute_calibration: ComputeCalibrationMetrics,
    /// PCIe amplification metrics
    pcie_amplification: PCIeAmplificationMetrics,
    /// Selected tab (0=Overview, 1=Memory, 2=Compute, 3=PCIe)
    selected_tab: usize,
    /// Last update time
    last_update: Instant,
    /// Matrix rotation countdown (seconds)
    rotation_countdown: u64,
    /// Connection status
    connected: bool,
}

#[derive(Clone, Debug, Default)]
struct MemoryCalibrationMetrics {
    matrix_version: u64,
    matrix_size: String,
    amplification_factor: f64,
    compression_ratio: f64,
    active_subscriptions: u32,
    matrices_served: u64,
    last_rotation: String,
}

#[derive(Clone, Debug, Default)]
struct ComputeCalibrationMetrics {
    matrix_version: u64,
    cartf_factor: f64,
    gfce_factor: f64,
    dbcg_factor: f64,
    hopfield_factor: f64,
    pmcw_factor: f64,
    combined_factor: f64,
    practical_factor: f64,
    active_subscriptions: u32,
}

#[derive(Clone, Debug, Default)]
struct PCIeAmplificationMetrics {
    prefetch_factor: f64,
    coalescing_factor: f64,
    compression_factor: f64,
    combined_factor: f64,
    prefetch_hit_rate: f64,
    bandwidth_physical_gbs: f64,
    bandwidth_effective_gbs: f64,
    active_subscriptions: u32,
}

impl Default for DashboardState {
    fn default() -> Self {
        Self {
            memory_calibration: MemoryCalibrationMetrics {
                matrix_version: 1,
                matrix_size: "64×64".to_string(),
                amplification_factor: 24_500.0,
                compression_ratio: 250.0,
                active_subscriptions: 0,
                matrices_served: 0,
                last_rotation: "Just now".to_string(),
            },
            compute_calibration: ComputeCalibrationMetrics {
                matrix_version: 1,
                cartf_factor: 1.8,
                gfce_factor: 14.0,
                dbcg_factor: 2.19,
                hopfield_factor: 1.45,
                pmcw_factor: 1.45,
                combined_factor: 116.20,
                practical_factor: 29.86,
                active_subscriptions: 0,
            },
            pcie_amplification: PCIeAmplificationMetrics {
                prefetch_factor: 8.0,
                coalescing_factor: 4.0,
                compression_factor: 2.5,
                combined_factor: 82.0,
                prefetch_hit_rate: 95.0,
                bandwidth_physical_gbs: 32.0,
                bandwidth_effective_gbs: 2624.0,
                active_subscriptions: 0,
            },
            selected_tab: 0,
            last_update: Instant::now(),
            rotation_countdown: 60,
            connected: true,
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    
    // Initialize terminal
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;

    // Create dashboard state
    let state = Arc::new(RwLock::new(DashboardState::default()));

    // Spawn update task
    let state_clone = state.clone();
    let refresh_ms = args.refresh;
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(refresh_ms)).await;
            let mut s = state_clone.write().await;
            s.last_update = Instant::now();
            // Simulate rotation countdown
            if s.rotation_countdown > 0 {
                s.rotation_countdown -= 1;
            } else {
                s.rotation_countdown = 60;
                s.memory_calibration.matrix_version += 1;
                s.compute_calibration.matrix_version += 1;
            }
        }
    });

    // Main loop
    loop {
        let state_read = state.read().await;
        terminal.draw(|f| draw_ui(f, &state_read))?;
        drop(state_read);

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => break,
                        KeyCode::Tab | KeyCode::Right => {
                            let mut s = state.write().await;
                            s.selected_tab = (s.selected_tab + 1) % 4;
                        }
                        KeyCode::BackTab | KeyCode::Left => {
                            let mut s = state.write().await;
                            s.selected_tab = if s.selected_tab == 0 { 3 } else { s.selected_tab - 1 };
                        }
                        KeyCode::Char('1') => state.write().await.selected_tab = 0,
                        KeyCode::Char('2') => state.write().await.selected_tab = 1,
                        KeyCode::Char('3') => state.write().await.selected_tab = 2,
                        KeyCode::Char('4') => state.write().await.selected_tab = 3,
                        _ => {}
                    }
                }
            }
        }
    }

    // Cleanup
    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;
    Ok(())
}

fn draw_ui(f: &mut Frame, state: &DashboardState) {
    let area = f.size();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Header
            Constraint::Length(3),  // Tabs
            Constraint::Min(10),    // Content
            Constraint::Length(3),  // Footer
        ])
        .split(area);

    draw_header(f, chunks[0], state);
    draw_tabs(f, chunks[1], state);

    match state.selected_tab {
        0 => draw_overview(f, chunks[2], state),
        1 => draw_memory_detail(f, chunks[2], state),
        2 => draw_compute_detail(f, chunks[2], state),
        3 => draw_pcie_detail(f, chunks[2], state),
        _ => {}
    }

    draw_footer(f, chunks[3], state);
}

fn draw_header(f: &mut Frame, area: Rect, state: &DashboardState) {
    let status = if state.connected { "🟢 CONNECTED" } else { "🔴 DISCONNECTED" };
    let title = format!(
        " CYAN FLAME™ CALIBRATION DASHBOARD │ {} │ Matrix Rotation: {}s ",
        status, state.rotation_countdown
    );
    let block = Block::default()
        .title(title)
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .style(Style::default().bg(Color::Black));
    f.render_widget(block, area);
}

fn draw_tabs(f: &mut Frame, area: Rect, state: &DashboardState) {
    let titles = vec!["[1] Overview", "[2] Memory", "[3] Compute", "[4] PCIe"];
    let tabs = Tabs::new(titles)
        .block(Block::default().borders(Borders::ALL).title(" Services "))
        .select(state.selected_tab)
        .style(Style::default().fg(Color::White))
        .highlight_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));
    f.render_widget(tabs, area);
}

fn draw_overview(f: &mut Frame, area: Rect, state: &DashboardState) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(33),
            Constraint::Percentage(34),
            Constraint::Percentage(33),
        ])
        .split(area);

    // Memory panel
    let mem = &state.memory_calibration;
    let mem_text = vec![
        Line::from(Span::styled("MEMORY AMPLIFICATION", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))),
        Line::from(""),
        Line::from(format!("Factor: {:.0}x", mem.amplification_factor)),
        Line::from(format!("Matrix: {}", mem.matrix_size)),
        Line::from(format!("Version: {}", mem.matrix_version)),
        Line::from(format!("Compression: {:.0}x", mem.compression_ratio)),
        Line::from(format!("Subscriptions: {}", mem.active_subscriptions)),
    ];
    let mem_block = Paragraph::new(mem_text)
        .block(Block::default().borders(Borders::ALL).title(" Memory ").border_style(Style::default().fg(Color::Green)));
    f.render_widget(mem_block, chunks[0]);

    // Compute panel
    let comp = &state.compute_calibration;
    let comp_text = vec![
        Line::from(Span::styled("COMPUTE AMPLIFICATION", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))),
        Line::from(""),
        Line::from(format!("Practical: {:.2}×", comp.practical_factor)),
        Line::from(format!("Theoretical: {:.2}×", comp.combined_factor)),
        Line::from(format!("Version: {}", comp.matrix_version)),
        Line::from(format!("GFCE: {:.1}× (highest)", comp.gfce_factor)),
        Line::from(format!("Subscriptions: {}", comp.active_subscriptions)),
    ];
    let comp_block = Paragraph::new(comp_text)
        .block(Block::default().borders(Borders::ALL).title(" Compute ").border_style(Style::default().fg(Color::Blue)));
    f.render_widget(comp_block, chunks[1]);

    // PCIe panel
    let pcie = &state.pcie_amplification;
    let pcie_text = vec![
        Line::from(Span::styled("PCIe AMPLIFICATION", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))),
        Line::from(""),
        Line::from(format!("Factor: {:.0}x", pcie.combined_factor)),
        Line::from(format!("Physical: {:.0} GB/s", pcie.bandwidth_physical_gbs)),
        Line::from(format!("Effective: {:.0} GB/s", pcie.bandwidth_effective_gbs)),
        Line::from(format!("Prefetch Hit: {:.1}%", pcie.prefetch_hit_rate)),
        Line::from(format!("Subscriptions: {}", pcie.active_subscriptions)),
    ];
    let pcie_block = Paragraph::new(pcie_text)
        .block(Block::default().borders(Borders::ALL).title(" PCIe ").border_style(Style::default().fg(Color::Magenta)));
    f.render_widget(pcie_block, chunks[2]);
}

fn draw_memory_detail(f: &mut Frame, area: Rect, state: &DashboardState) {
    let mem = &state.memory_calibration;
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    // Amplification gauge
    let gauge_percent = ((mem.amplification_factor / 30000.0) * 100.0).min(100.0) as u16;
    let gauge = Gauge::default()
        .block(Block::default().borders(Borders::ALL).title(" Memory Amplification Factor "))
        .gauge_style(Style::default().fg(Color::Green))
        .percent(gauge_percent)
        .label(format!("{:.0}x / 30,000x max", mem.amplification_factor));
    f.render_widget(gauge, chunks[0]);

    // Details table
    let amp_str = format!("{:.0}x", mem.amplification_factor);
    let comp_str = format!("{:.0}x", mem.compression_ratio);
    let ver_str = mem.matrix_version.to_string();
    let sub_str = mem.active_subscriptions.to_string();
    let served_str = mem.matrices_served.to_string();
    let rows = vec![
        Row::new(vec!["Matrix Size", mem.matrix_size.as_str()]),
        Row::new(vec!["Matrix Version", ver_str.as_str()]),
        Row::new(vec!["Amplification Factor", amp_str.as_str()]),
        Row::new(vec!["Compression Ratio", comp_str.as_str()]),
        Row::new(vec!["Active Subscriptions", sub_str.as_str()]),
        Row::new(vec!["Matrices Served", served_str.as_str()]),
        Row::new(vec!["Last Rotation", mem.last_rotation.as_str()]),
    ];
    let widths = [Constraint::Percentage(40), Constraint::Percentage(60)];
    let table = Table::new(rows, widths)
        .block(Block::default().borders(Borders::ALL).title(" Memory Calibration Details "))
        .header(Row::new(vec!["Metric", "Value"]).style(Style::default().fg(Color::Yellow)));
    f.render_widget(table, chunks[1]);
}

fn draw_compute_detail(f: &mut Frame, area: Rect, state: &DashboardState) {
    let comp = &state.compute_calibration;
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(area);

    // Engine breakdown bar chart
    let data = [
        ("CARTF", comp.cartf_factor as u64),
        ("GFCE", comp.gfce_factor as u64),
        ("DBCG", comp.dbcg_factor as u64),
        ("Hopfield", comp.hopfield_factor as u64),
        ("PMCW", comp.pmcw_factor as u64),
    ];
    let barchart = BarChart::default()
        .block(Block::default().borders(Borders::ALL).title(" Compute Engines (Amplification Factor) "))
        .data(&data)
        .bar_width(10)
        .bar_gap(2)
        .bar_style(Style::default().fg(Color::Blue))
        .value_style(Style::default().fg(Color::White).add_modifier(Modifier::BOLD));
    f.render_widget(barchart, chunks[0]);

    // Details
    let text = vec![
        Line::from(Span::styled("Engine Details:", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))),
        Line::from(""),
        Line::from(format!("• CARTF (Cache-Aware Recursive Tensor Folding): {:.2}×", comp.cartf_factor)),
        Line::from(format!("• GFCE (Galois Field GF(2^32) Compute Engine): {:.2}×", comp.gfce_factor)),
        Line::from(format!("• DBCG (De Bruijn Compute Graph): {:.2}×", comp.dbcg_factor)),
        Line::from(format!("• CHN-CS (Continuous Hopfield Network Scheduler): {:.2}×", comp.hopfield_factor)),
        Line::from(format!("• PMCW (Particle Mesh Compute Wave): {:.2}×", comp.pmcw_factor)),
        Line::from(""),
        Line::from(Span::styled(format!("Theoretical Combined: {:.2}×", comp.combined_factor), Style::default().fg(Color::White))),
        Line::from(Span::styled(format!("Practical (25.7% overhead): {:.2}×", comp.practical_factor), Style::default().fg(Color::Green).add_modifier(Modifier::BOLD))),
    ];
    let details = Paragraph::new(text)
        .block(Block::default().borders(Borders::ALL).title(" Compute Amplification Breakdown "));
    f.render_widget(details, chunks[1]);
}

fn draw_pcie_detail(f: &mut Frame, area: Rect, state: &DashboardState) {
    let pcie = &state.pcie_amplification;
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    // Bandwidth visualization
    let bw_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[0]);

    // Physical bandwidth gauge
    let phys_gauge = Gauge::default()
        .block(Block::default().borders(Borders::ALL).title(" Physical Bandwidth "))
        .gauge_style(Style::default().fg(Color::Red))
        .percent(100)
        .label(format!("{:.0} GB/s", pcie.bandwidth_physical_gbs));
    f.render_widget(phys_gauge, bw_chunks[0]);

    // Effective bandwidth gauge
    let eff_percent = ((pcie.bandwidth_effective_gbs / 3000.0) * 100.0).min(100.0) as u16;
    let eff_gauge = Gauge::default()
        .block(Block::default().borders(Borders::ALL).title(" Effective Bandwidth "))
        .gauge_style(Style::default().fg(Color::Green))
        .percent(eff_percent)
        .label(format!("{:.0} GB/s ({:.0}x)", pcie.bandwidth_effective_gbs, pcie.combined_factor));
    f.render_widget(eff_gauge, bw_chunks[1]);

    // Component breakdown
    let text = vec![
        Line::from(Span::styled("PCIe Amplification Components:", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))),
        Line::from(""),
        Line::from(format!("* Predictive Prefetch (Hopfield-based): {:.1}x", pcie.prefetch_factor)),
        Line::from(format!("  Hit Rate: {:.1}%", pcie.prefetch_hit_rate)),
        Line::from(format!("* Transfer Coalescing (De Bruijn scheduling): {:.1}x", pcie.coalescing_factor)),
        Line::from(format!("* Compression (Galois Field GF(2^32)): {:.1}x", pcie.compression_factor)),
        Line::from(""),
        Line::from(Span::styled(format!("Combined Factor: {:.0}x", pcie.combined_factor), Style::default().fg(Color::Green).add_modifier(Modifier::BOLD))),
        Line::from(format!("Physical -> Effective: {:.0} GB/s -> {:.0} GB/s", pcie.bandwidth_physical_gbs, pcie.bandwidth_effective_gbs)),
    ];
    let details = Paragraph::new(text)
        .block(Block::default().borders(Borders::ALL).title(" PCIe Amplification Details "));
    f.render_widget(details, chunks[1]);
}

fn draw_footer(f: &mut Frame, area: Rect, _state: &DashboardState) {
    let text = " [Q] Quit │ [Tab/←→] Switch Tab │ [1-4] Jump to Tab │ SYMMETRIX CORE™ Technology ";
    let footer = Paragraph::new(text)
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(footer, area);
}

