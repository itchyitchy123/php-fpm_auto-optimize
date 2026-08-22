use crate::{
    PolicyFile,
    model::{Plan, PoolPolicy},
};
use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Layout},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState},
};
use std::{
    io::{self, stdout},
    time::Duration,
};

#[derive(Clone, Copy)]
enum Field {
    Children,
    Minimum,
    Maximum,
    Requests,
    Idle,
    Terminate,
}

impl Field {
    fn next(self) -> Self {
        match self {
            Self::Children => Self::Minimum,
            Self::Minimum => Self::Maximum,
            Self::Maximum => Self::Requests,
            Self::Requests => Self::Idle,
            Self::Idle => Self::Terminate,
            Self::Terminate => Self::Children,
        }
    }
    fn label(self) -> &'static str {
        match self {
            Self::Children => "children",
            Self::Minimum => "minimum",
            Self::Maximum => "maximum",
            Self::Requests => "max requests",
            Self::Idle => "idle timeout",
            Self::Terminate => "request timeout",
        }
    }
}

pub fn review(plan: &Plan, policy: &mut PolicyFile) -> Result<bool> {
    enable_raw_mode()?;
    let mut out = stdout();
    execute!(out, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(out);
    let mut terminal = Terminal::new(backend)?;
    let result = run(&mut terminal, plan, policy);
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    result
}

fn run(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    plan: &Plan,
    policy: &mut PolicyFile,
) -> Result<bool> {
    let mut state = TableState::default().with_selected(Some(0));
    let mut field = Field::Children;
    loop {
        terminal.draw(|frame| {
            let areas = Layout::vertical([
                Constraint::Length(3),
                Constraint::Min(5),
                Constraint::Length(3),
            ])
            .split(frame.area());
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    " FPM Lens  ".into(),
                    "Review capacity as evidence, not guesswork".into(),
                ]))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .style(Style::default().fg(Color::Cyan)),
                ),
                areas[0],
            );
            let header = Row::new([
                "Use",
                "Pool",
                "Mode",
                "Now",
                "Plan",
                "Min",
                "Max",
                "Req",
                "Idle",
                "Timeout",
                "Confidence",
            ])
            .style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            );
            let rows = plan.pools.iter().map(|d| {
                let q = qualified(d);
                let p = policy.for_pool(&d.id.name, &q);
                Row::new(vec![
                    Cell::from(if p.selected.unwrap_or(d.selected) {
                        "●"
                    } else {
                        "○"
                    }),
                    Cell::from(d.id.name.clone()),
                    Cell::from(format!("{:?}", d.current.pm).to_lowercase()),
                    Cell::from(opt(d.current.max_children)),
                    Cell::from(opt(p.target_children.or(d.proposed.max_children))),
                    Cell::from(p.min_children.unwrap_or(d.minimum_children).to_string()),
                    Cell::from(p.max_children.unwrap_or(d.maximum_children).to_string()),
                    Cell::from(opt(p.max_requests.or(d.proposed.max_requests))),
                    Cell::from(opt(p
                        .process_idle_timeout_seconds
                        .or(d.proposed.process_idle_timeout_seconds))),
                    Cell::from(opt(p
                        .request_terminate_timeout_seconds
                        .or(d.proposed.request_terminate_timeout_seconds))),
                    Cell::from(format!("{:?}", d.confidence).to_lowercase()),
                ])
            });
            let table = Table::new(
                rows,
                [
                    Constraint::Length(3),
                    Constraint::Min(12),
                    Constraint::Length(9),
                    Constraint::Length(5),
                    Constraint::Length(5),
                    Constraint::Length(5),
                    Constraint::Length(5),
                    Constraint::Length(6),
                    Constraint::Length(6),
                    Constraint::Length(8),
                    Constraint::Length(10),
                ],
            )
            .header(header)
            .row_highlight_style(
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("› ")
            .block(Block::default().borders(Borders::ALL).title(" Pools "));
            frame.render_stateful_widget(table, areas[1], &mut state);
            frame.render_widget(
                Paragraph::new(format!(
                    "↑/↓ pool  Space select  Tab field ({})  +/- adjust  Enter save plan  q cancel",
                    field.label()
                ))
                .block(Block::default().borders(Borders::ALL)),
                areas[2],
            );
        })?;
        if !event::poll(Duration::from_millis(250))? {
            continue;
        }
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        let index = state.selected().unwrap_or(0);
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => return Ok(false),
            KeyCode::Enter => return Ok(true),
            KeyCode::Up => state.select(Some(index.saturating_sub(1))),
            KeyCode::Down => {
                state.select(Some((index + 1).min(plan.pools.len().saturating_sub(1))));
            }
            KeyCode::Tab => field = field.next(),
            KeyCode::Char(' ') if !plan.pools.is_empty() => {
                let d = &plan.pools[index];
                let p = entry(policy, d);
                p.selected = Some(!p.selected.unwrap_or(d.selected));
            }
            KeyCode::Char('+' | '=') if !plan.pools.is_empty() => adjust(
                entry(policy, &plan.pools[index]),
                &plan.pools[index],
                field,
                1,
            ),
            KeyCode::Char('-') if !plan.pools.is_empty() => adjust(
                entry(policy, &plan.pools[index]),
                &plan.pools[index],
                field,
                -1,
            ),
            _ => {}
        }
    }
}
fn qualified(d: &crate::model::PoolDecision) -> String {
    format!("{}:{}", d.id.directory.display(), d.id.name)
}
fn entry<'a>(policy: &'a mut PolicyFile, d: &crate::model::PoolDecision) -> &'a mut PoolPolicy {
    policy.pools.entry(qualified(d)).or_default()
}
fn opt(v: Option<u32>) -> String {
    v.map_or_else(|| "—".into(), |v| v.to_string())
}
fn bump(v: u32, delta: i32) -> u32 {
    if delta > 0 {
        v.saturating_add(1)
    } else {
        v.saturating_sub(1)
    }
}
fn adjust(p: &mut PoolPolicy, d: &crate::model::PoolDecision, field: Field, delta: i32) {
    match field {
        Field::Children => {
            let v = bump(
                p.target_children
                    .or(d.proposed.max_children)
                    .unwrap_or(d.minimum_children),
                delta,
            )
            .clamp(
                p.min_children.unwrap_or(d.minimum_children),
                p.max_children.unwrap_or(d.maximum_children),
            );
            p.target_children = Some(v);
        }
        Field::Minimum => {
            p.min_children = Some(
                bump(p.min_children.unwrap_or(d.minimum_children), delta)
                    .min(p.max_children.unwrap_or(d.maximum_children)),
            );
        }
        Field::Maximum => {
            p.max_children = Some(
                bump(p.max_children.unwrap_or(d.maximum_children), delta)
                    .max(p.min_children.unwrap_or(d.minimum_children)),
            );
        }
        Field::Requests => {
            p.max_requests = Some(bump(
                p.max_requests.or(d.proposed.max_requests).unwrap_or(500),
                delta,
            ));
        }
        Field::Idle => {
            p.process_idle_timeout_seconds = Some(bump(
                p.process_idle_timeout_seconds
                    .or(d.proposed.process_idle_timeout_seconds)
                    .unwrap_or(10),
                delta,
            ));
        }
        Field::Terminate => {
            p.request_terminate_timeout_seconds = Some(bump(
                p.request_terminate_timeout_seconds
                    .or(d.proposed.request_terminate_timeout_seconds)
                    .unwrap_or(30),
                delta,
            ));
        }
    }
}
