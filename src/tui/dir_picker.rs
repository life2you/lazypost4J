//! 启动时选择项目根目录（终端内浏览 + 最近使用）。

use std::fs::read_dir;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Context;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::prelude::*;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};

struct TerminalRestore;

impl Drop for TerminalRestore {
    fn drop(&mut self) {
        ratatui::restore();
    }
}

fn refresh_rows(current: &PathBuf, rows: &mut Vec<PathBuf>, has_parent: &mut bool) {
    rows.clear();
    *has_parent = false;
    if let Some(p) = current.parent() {
        if p != current.as_path() {
            rows.push(p.to_path_buf());
            *has_parent = true;
        }
    }
    if let Ok(rd) = read_dir(current) {
        let mut dirs: Vec<PathBuf> = rd
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .map(|e| e.path())
            .collect();
        dirs.sort();
        rows.extend(dirs);
    }
}

fn row_label(rows: &[PathBuf], has_parent: bool, idx: usize) -> String {
    if idx >= rows.len() {
        return String::new();
    }
    if has_parent && idx == 0 {
        return "..".to_string();
    }
    rows[idx]
        .file_name()
        .and_then(|s| s.to_str())
        .map(|s| format!("{s}/"))
        .unwrap_or_else(|| "?".to_string())
}

/// 单行展示路径，过长则省略中间。
fn fmt_path_line(s: &str, max_chars: usize) -> String {
    let n = s.chars().count();
    if n <= max_chars {
        return s.to_string();
    }
    let keep = max_chars.saturating_sub(1);
    let head = keep / 2;
    let tail = keep - head;
    let mut ch = s.chars();
    let a: String = ch.by_ref().take(head).collect();
    let s_rev: String = s.chars().rev().take(tail).collect();
    let b: String = s_rev.chars().rev().collect();
    format!("{a}…{b}")
}

fn load_recent_paths() -> Vec<String> {
    let mut v = crate::user_config::load()
        .map(|c| c.recent_projects)
        .unwrap_or_default();
    v.retain(|p| {
        let path = Path::new(p);
        path.is_dir()
    });
    v
}

/// 未选目录（Esc / q）时返回 `None`。
pub fn pick_project_dir() -> anyhow::Result<Option<PathBuf>> {
    let _restore = TerminalRestore;
    let mut terminal = ratatui::init();

    let mut recent = load_recent_paths();
    let mut recent_state = ListState::default();
    if !recent.is_empty() {
        recent_state.select(Some(0));
    }
    let mut focus_recent = !recent.is_empty();

    let mut cwd = std::env::current_dir()
        .ok()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("/"));
    cwd = std::fs::canonicalize(&cwd).unwrap_or(cwd);

    let mut rows: Vec<PathBuf> = Vec::new();
    let mut has_parent = false;
    let mut dir_state = ListState::default();

    refresh_rows(&cwd, &mut rows, &mut has_parent);
    if rows.is_empty() {
        dir_state.select(None);
    } else {
        dir_state.select(Some(0));
    }

    loop {
        terminal.draw(|f| {
            let area = f.area();
            f.render_widget(Clear, area);

            let block = Block::default()
                .borders(Borders::ALL)
                .title(" 选择项目根目录 ")
                .border_style(Style::default().fg(Color::Cyan));
            let inner = block.inner(area);
            f.render_widget(block, area);

            let constraints: Vec<Constraint> = if recent.is_empty() {
                vec![
                    Constraint::Length(2),
                    Constraint::Min(6),
                    Constraint::Length(3),
                ]
            } else {
                let h = (recent.len().min(6) as u16).saturating_add(3);
                vec![
                    Constraint::Length(h),
                    Constraint::Length(2),
                    Constraint::Min(5),
                    Constraint::Length(3),
                ]
            };
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints(constraints)
                .split(inner);

            let (path_chunk, list_chunk, hint_chunk) = if recent.is_empty() {
                (chunks[0], chunks[1], chunks[2])
            } else {
                let recent_items: Vec<ListItem> = recent
                    .iter()
                    .map(|p| {
                        ListItem::new(fmt_path_line(p, (area.width as usize).saturating_sub(8)))
                    })
                    .collect();
                let recent_block = Block::default().borders(Borders::ALL).title({
                    let t = if focus_recent {
                        " 最近项目 （Tab 切到浏览） "
                    } else {
                        " 最近项目 "
                    };
                    t.to_string()
                });
                let rin = recent_block.inner(chunks[0]);
                f.render_widget(recent_block, chunks[0]);
                let rlist = List::new(recent_items).highlight_style(
                    Style::default()
                        .bg(Color::DarkGray)
                        .add_modifier(Modifier::BOLD),
                );
                f.render_stateful_widget(rlist, rin, &mut recent_state);

                (chunks[1], chunks[2], chunks[3])
            };

            let path_para = Paragraph::new(cwd.display().to_string())
                .style(Style::default().fg(Color::Yellow));
            f.render_widget(path_para, path_chunk);

            let dir_block = Block::default().borders(Borders::ALL).title({
                let t = if !recent.is_empty() && !focus_recent {
                    " 浏览 （Tab 切到最近） "
                } else {
                    " 目录 "
                };
                t.to_string()
            });
            let din = dir_block.inner(list_chunk);
            f.render_widget(dir_block, list_chunk);

            let items: Vec<ListItem> = rows
                .iter()
                .enumerate()
                .map(|(i, _)| {
                    let label = row_label(&rows, has_parent, i);
                    let style = if has_parent && i == 0 {
                        Style::default().fg(Color::DarkGray)
                    } else {
                        Style::default()
                    };
                    ListItem::new(Line::from(label)).style(style)
                })
                .collect();
            let list = List::new(items).highlight_style(
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            );
            f.render_stateful_widget(list, din, &mut dir_state);

            let hint = if recent.is_empty() {
                "↑/↓ j/k 移动 · ←/h/Backspace 后退 · →/l 前进 · 空格或Enter 确认 · Esc/q 取消"
            } else if focus_recent {
                "↑/↓ 移动 · 空格或Enter 打开 · x/Delete 从历史移除 · Tab 浏览磁盘 · Esc 取消"
            } else {
                "↑/↓ j/k · ←/h 后退 · →/l 前进 · 空格或Enter 确认当前目录 · Tab 最近项目 · Esc 取消"
            };
            f.render_widget(
                Paragraph::new(hint).style(Style::default().fg(Color::DarkGray)),
                hint_chunk,
            );
        })?;

        if !event::poll(Duration::from_millis(50))? {
            continue;
        }
        if let Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Release {
                continue;
            }
            match key.code {
                KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('Q') => {
                    return Ok(None);
                }
                KeyCode::Tab if !recent.is_empty() => {
                    focus_recent = !focus_recent;
                }
                KeyCode::Char(' ') | KeyCode::Enter => {
                    if focus_recent && !recent.is_empty() {
                        if let Some(i) = recent_state.selected() {
                            if i < recent.len() {
                                let p = PathBuf::from(&recent[i]);
                                if p.is_dir() {
                                    let chosen = std::fs::canonicalize(&p).with_context(|| {
                                        format!("无法解析路径: {}", p.display())
                                    })?;
                                    return Ok(Some(chosen));
                                }
                            }
                        }
                    } else {
                        let chosen = std::fs::canonicalize(&cwd)
                            .with_context(|| format!("无法解析路径: {}", cwd.display()))?;
                        return Ok(Some(chosen));
                    }
                }
                KeyCode::Char('x') | KeyCode::Delete if focus_recent && !recent.is_empty() => {
                    if let Some(i) = recent_state.selected() {
                        if i < recent.len() {
                            recent.remove(i);
                            let _ = crate::user_config::set_recent_projects(recent.clone());
                            if recent.is_empty() {
                                focus_recent = false;
                                recent_state.select(None);
                            } else {
                                let max = recent.len().saturating_sub(1);
                                let ni = i.min(max);
                                recent_state.select(Some(ni));
                            }
                        }
                    }
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if focus_recent && !recent.is_empty() {
                        let i = recent_state.selected().unwrap_or(0);
                        let max = recent.len().saturating_sub(1);
                        recent_state.select(Some((i + 1).min(max)));
                    } else {
                        let i = dir_state.selected().unwrap_or(0);
                        let max = rows.len().saturating_sub(1);
                        dir_state.select(Some((i + 1).min(max)));
                    }
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    if focus_recent && !recent.is_empty() {
                        let i = recent_state.selected().unwrap_or(0);
                        recent_state.select(Some(i.saturating_sub(1)));
                    } else {
                        let i = dir_state.selected().unwrap_or(0);
                        dir_state.select(Some(i.saturating_sub(1)));
                    }
                }
                KeyCode::Char('l') | KeyCode::Right if !focus_recent || recent.is_empty() => {
                    if let Some(i) = dir_state.selected() {
                        if i < rows.len() {
                            cwd = rows[i].clone();
                            refresh_rows(&cwd, &mut rows, &mut has_parent);
                            dir_state.select(if rows.is_empty() {
                                None
                            } else {
                                Some(0)
                            });
                        }
                    }
                }
                KeyCode::Char('h') | KeyCode::Left | KeyCode::Backspace
                    if !focus_recent || recent.is_empty() =>
                {
                    if let Some(p) = cwd.parent() {
                        if p != cwd.as_path() {
                            cwd = p.to_path_buf();
                            refresh_rows(&cwd, &mut rows, &mut has_parent);
                            dir_state.select(if rows.is_empty() {
                                None
                            } else {
                                Some(0)
                            });
                        }
                    }
                }
                _ => {}
            }
        }
    }
}
