//! 全屏 TUI（ratatui + crossterm）。

mod app;
mod body_cursor;
mod dir_picker;

use std::io::stdout;
use std::path::PathBuf;

pub use dir_picker::pick_project_dir;
use std::sync::mpsc;
use std::time::Duration;

use app::{
    App, HostsEditKind, ListGroupMode, MainPanel, MainUiLayout, RequestHeaderEditKind,
};
use crate::scanner::{self, JavaSourceCache, ScanMode, ScanReport};
use crossterm::event::{
    self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
    Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use crossterm::execute;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::prelude::*;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap};

pub fn run(project: PathBuf) -> anyhow::Result<()> {
    let mut terminal = ratatui::init();
    execute!(stdout(), EnableBracketedPaste)?;
    execute!(stdout(), EnableMouseCapture)?;

    let mut app = App::new(project);
    app.scan_in_flight = true;
    app.status_msg = "正在扫描项目（后台）…".into();

    let (scan_tx, scan_rx) = mpsc::channel::<ScanReport>();
    let (cache_tx, cache_rx) = mpsc::channel::<JavaSourceCache>();
    let project0 = app.project.clone();
    let scan_tx_boot = scan_tx.clone();
    std::thread::spawn(move || {
        let report = scanner::scan_project_with_mode(&project0, ScanMode::LazyEndpoints);
        let _ = scan_tx_boot.send(report);
    });
    let project_cache = app.project.clone();
    std::thread::spawn(move || {
        let cache = JavaSourceCache::build(&project_cache);
        let _ = cache_tx.send(cache);
    });

    let (tx, rx) = mpsc::channel::<Result<crate::http_exec::HttpResult, String>>();

    loop {
        while let Ok(report) = scan_rx.try_recv() {
            app.scan_in_flight = false;
            app.apply_scan(report);
        }
        while let Ok(cache) = cache_rx.try_recv() {
            app.apply_source_cache(cache);
        }

        terminal.draw(|f| draw_ui(f, &mut app))?;

        while let Ok(r) = rx.try_recv() {
            app.pending_request = false;
            match r {
                Ok(h) => {
                    app.status_msg = format!("HTTP {} — {} ms", h.status, h.elapsed_ms);
                    app.last_http = Some(h);
                    app.response_scroll = 0;
                }
                Err(e) => app.status_msg = e,
            }
        }

        if !event::poll(Duration::from_millis(150))? {
            continue;
        }
        match event::read()? {
            Event::Key(key) => {
                if key.kind == KeyEventKind::Release {
                    continue;
                }
                if handle_key(&mut app, key, &tx, &scan_tx)? {
                    break;
                }
            }
            Event::Paste(pasted) => handle_paste(&mut app, &pasted),
            Event::Mouse(m) => handle_mouse(&mut app, m),
            _ => {}
        }
    }

    let _ = execute!(stdout(), DisableMouseCapture);
    let _ = execute!(stdout(), DisableBracketedPaste);
    ratatui::restore();
    Ok(())
}

fn draw_hosts_panel(f: &mut Frame, app: &mut App) {
    let area = centered_rect(78, 72, f.area());
    f.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" 域名 / Base URL ")
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(10),
            Constraint::Length(2),
        ])
        .split(inner);

    let cap = format!(
        "发请求: {}  （▶ 当前）",
        app.hosts
            .get(app.base_url_index)
            .map(|h| h.display_label())
            .unwrap_or_else(|| app.active_base_url().to_string())
    );
    f.render_widget(
        Paragraph::new(cap).style(Style::default().fg(Color::Yellow)),
        chunks[0],
    );

    if app.hosts_edit_line {
        let title = match app.hosts_edit_kind {
            Some(HostsEditKind::Url) => " 编辑 URL — Enter 保存 Esc 取消 ",
            Some(HostsEditKind::Description) => " 编辑描述 — Enter 保存 Esc 取消 ",
            None => " 编辑 — Enter 保存 Esc 取消 ",
        };
        let eb = Block::default().borders(Borders::ALL).title(title);
        let ein = eb.inner(chunks[1]);
        f.render_widget(eb, chunks[1]);
        let t = text_with_cursor(&app.host_buf, app.host_cursor_char);
        f.render_widget(Paragraph::new(t), ein);
    } else {
        let items: Vec<ListItem> = app
            .hosts
            .iter()
            .enumerate()
            .map(|(i, h)| {
                let mark = if i == app.base_url_index { "▶ " } else { "  " };
                let label = h.display_label();
                let st = if i == app.base_url_index {
                    Style::default().fg(Color::Green)
                } else {
                    Style::default()
                };
                let line1 = Line::from(vec![
                    Span::styled(mark, st),
                    Span::styled(label, st),
                ]);
                if let Some(sub) = h.display_subtitle() {
                    ListItem::new(Text::from(vec![
                        line1,
                        Line::from(Span::styled(
                            format!("   {sub}"),
                            Style::default().fg(Color::DarkGray),
                        )),
                    ]))
                } else {
                    ListItem::new(line1)
                }
            })
            .collect();
        let list = List::new(items)
            .highlight_style(Style::default().bg(Color::DarkGray))
            .block(Block::default().borders(Borders::ALL).title(" 已保存的域名 "));
        f.render_stateful_widget(list, chunks[1], &mut app.hosts_list_state);
    }

    let hint = if app.hosts_edit_line {
        "←/→ 移动光标 · Backspace/Delete"
    } else {
        "↑/↓ j/k · Enter 选用 · n 新增 · d 删除 · e 改URL · v 改描述 · Esc 关闭"
    };
    f.render_widget(
        Paragraph::new(hint).style(Style::default().fg(Color::DarkGray)),
        chunks[2],
    );
}

fn draw_headers_panel(f: &mut Frame, app: &mut App) {
    let area = centered_rect(78, 72, f.area());
    f.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" 全局请求头 ")
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(10),
            Constraint::Length(2),
        ])
        .split(inner);

    let cap = format!(
        "全局（所有请求附带）: {}  （▶ 当前）",
        app.request_headers
            .get(app.selected_request_header)
            .map(|h| h.display_label())
            .unwrap_or_else(|| "(无)".into())
    );
    f.render_widget(
        Paragraph::new(cap).style(Style::default().fg(Color::Yellow)),
        chunks[0],
    );

    if app.headers_edit_line {
        let title = match app.headers_edit_kind {
            Some(RequestHeaderEditKind::Name) => " 编辑名称 — Enter 保存 Esc 取消 ",
            Some(RequestHeaderEditKind::Value) => " 编辑值 — Enter 保存 Esc 取消 ",
            Some(RequestHeaderEditKind::Description) => " 编辑描述 — Enter 保存 Esc 取消 ",
            None => " 编辑 — Enter 保存 Esc 取消 ",
        };
        let eb = Block::default().borders(Borders::ALL).title(title);
        let ein = eb.inner(chunks[1]);
        f.render_widget(eb, chunks[1]);
        let t = text_with_cursor(&app.header_buf, app.header_cursor_char);
        f.render_widget(Paragraph::new(t), ein);
    } else {
        let items: Vec<ListItem> = app
            .request_headers
            .iter()
            .enumerate()
            .map(|(i, h)| {
                let mark = if i == app.selected_request_header {
                    "▶ "
                } else {
                    "  "
                };
                let label = h.display_label();
                let st = if i == app.selected_request_header {
                    Style::default().fg(Color::Green)
                } else {
                    Style::default()
                };
                let line1 = Line::from(vec![
                    Span::styled(mark, st),
                    Span::styled(label, st),
                ]);
                if let Some(sub) = h.display_subtitle() {
                    ListItem::new(Text::from(vec![
                        line1,
                        Line::from(Span::styled(
                            format!("   {sub}"),
                            Style::default().fg(Color::DarkGray),
                        )),
                    ]))
                } else {
                    ListItem::new(line1)
                }
            })
            .collect();
        let list = List::new(items)
            .highlight_style(Style::default().bg(Color::DarkGray))
            .block(Block::default().borders(Borders::ALL).title(" 已保存的请求头 "));
        f.render_stateful_widget(list, chunks[1], &mut app.headers_list_state);
    }

    let hint = if app.headers_edit_line {
        "←/→ 移动光标 · Backspace/Delete"
    } else {
        "↑/↓ j/k · Enter 选用 · n 新增 · d 删除 · e 改名称 · v 改值 · b 改描述 · Esc 关闭"
    };
    f.render_widget(
        Paragraph::new(hint).style(Style::default().fg(Color::DarkGray)),
        chunks[2],
    );
}

fn panel_border(focused: bool) -> Style {
    if focused {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    }
}

fn draw_header_sidebar(f: &mut Frame, app: &App, area: Rect, focused: bool) {
    let lines: Vec<Line> = if app.request_headers.is_empty() {
        vec![Line::from(Span::styled(
            "  (无)",
            Style::default().fg(Color::DarkGray),
        ))]
    } else {
        app.request_headers
            .iter()
            .enumerate()
            .flat_map(|(i, h)| {
                let mark = if i == app.selected_request_header {
                    "▶ "
                } else {
                    "  "
                };
                let label = h.display_label();
                let st = if i == app.selected_request_header {
                    Style::default().fg(Color::Green)
                } else {
                    Style::default()
                };
                let l1 = Line::from(vec![
                    Span::styled(mark, st),
                    Span::styled(label, st),
                ]);
                if let Some(sub) = h.display_subtitle() {
                    vec![
                        l1,
                        Line::from(Span::styled(
                            format!("   {sub}"),
                            Style::default().fg(Color::DarkGray),
                        )),
                    ]
                } else {
                    vec![l1]
                }
            })
            .collect()
    };
    let p = Paragraph::new(Text::from(lines))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(panel_border(focused))
                .title(" [3] 全局头（a） "),
        )
        .scroll((app.header_sidebar_scroll, 0))
        .wrap(Wrap { trim: true });
    f.render_widget(p, area);
}

fn draw_host_sidebar(f: &mut Frame, app: &App, area: Rect, focused: bool) {
    let lines: Vec<Line> = app
        .hosts
        .iter()
        .enumerate()
        .flat_map(|(i, h)| {
            let mark = if i == app.base_url_index { "▶ " } else { "  " };
            let label = h.display_label();
            let st = if i == app.base_url_index {
                Style::default().fg(Color::Green)
            } else {
                Style::default()
            };
            let l1 = Line::from(vec![
                Span::styled(mark, st),
                Span::styled(label, st),
            ]);
            if let Some(sub) = h.display_subtitle() {
                vec![
                    l1,
                    Line::from(Span::styled(
                        format!("   {sub}"),
                        Style::default().fg(Color::DarkGray),
                    )),
                ]
            } else {
                vec![l1]
            }
        })
        .collect();
    let p = Paragraph::new(Text::from(lines))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(panel_border(focused))
                .title(" [2] 域名（h） "),
        )
        .scroll((app.host_sidebar_scroll, 0))
        .wrap(Wrap { trim: true });
    f.render_widget(p, area);
}

fn prepare_main_ui(root: Rect, app: &App) -> (MainUiLayout, Rect, String) {
    let main = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(52),
            Constraint::Percentage(38),
            Constraint::Min(4),
        ])
        .split(root);

    let top = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(34), Constraint::Percentage(66)])
        .split(main[0]);

    let left_col = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(50),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
        ])
        .split(top[0]);

    let list_title = format!(" [1] 接口列表 (j/k){} ", app.list_title_suffix());
    let list_block = Block::default()
        .borders(Borders::ALL)
        .title(list_title.as_str());
    let api_list = left_col[0];
    let api_list_inner = list_block.inner(api_list);

    let host_sidebar = left_col[1];
    let host_block = Block::default()
        .borders(Borders::ALL)
        .title(" [2] 域名（h） ");
    let host_sidebar_inner = host_block.inner(host_sidebar);

    let header_sidebar = left_col[2];
    let hdr_block = Block::default()
        .borders(Borders::ALL)
        .title(" [3] 全局头（a） ");
    let header_sidebar_inner = hdr_block.inner(header_sidebar);

    let detail = top[1];
    let detail_block = Block::default()
        .borders(Borders::ALL)
        .title(" [4] 详情 (s/e/预设头/a) ");
    let detail_inner = detail_block.inner(detail);

    let response = main[1];
    let resp_block = Block::default().borders(Borders::ALL).title(" [5] 响应 ");
    let response_inner = resp_block.inner(response);

    let layout = MainUiLayout {
        api_list,
        api_list_inner,
        host_sidebar,
        host_sidebar_inner,
        header_sidebar,
        header_sidebar_inner,
        detail,
        detail_inner,
        response,
        response_inner,
    };
    (layout, main[2], list_title)
}

fn list_row_from_click(x: u16, y: u16, inner: Rect, nrows: usize) -> Option<usize> {
    if nrows == 0 || !inner.contains(ratatui::layout::Position { x, y }) {
        return None;
    }
    let r = (y.saturating_sub(inner.y)) as usize;
    (r < nrows).then_some(r)
}

const WHEEL_LINES: u16 = 3;

fn host_sidebar_line_count(app: &App) -> u16 {
    if app.hosts.is_empty() {
        return 1;
    }
    app.hosts
        .iter()
        .map(|h| {
            if h.display_subtitle().is_some() {
                2u16
            } else {
                1u16
            }
        })
        .sum::<u16>()
        .max(1)
}

fn header_sidebar_line_count(app: &App) -> u16 {
    if app.request_headers.is_empty() {
        return 1;
    }
    app.request_headers
        .iter()
        .map(|h| {
            if h.display_subtitle().is_some() {
                2u16
            } else {
                1u16
            }
        })
        .sum::<u16>()
        .max(1)
}

fn paragraph_wheel(scroll: &mut u16, scroll_up: bool, total_lines: u16, visible_h: u16) {
    if visible_h == 0 {
        return;
    }
    let vh = visible_h.max(1);
    let total = total_lines.max(1);
    let max = total.saturating_sub(vh.saturating_sub(1));
    if scroll_up {
        *scroll = scroll.saturating_sub(WHEEL_LINES);
    } else {
        *scroll = (*scroll + WHEEL_LINES).min(max);
    }
}

fn handle_mouse_wheel(app: &mut App, mouse: MouseEvent) {
    let Some(layout) = app.last_main_layout else {
        return;
    };
    let scroll_up = matches!(mouse.kind, MouseEventKind::ScrollUp);
    let p = ratatui::layout::Position {
        x: mouse.column,
        y: mouse.row,
    };

    if layout.api_list.contains(p) {
        app.main_panel = MainPanel::ApiList;
        if scroll_up {
            app.list_state.scroll_up_by(WHEEL_LINES);
        } else {
            app.list_state.scroll_down_by(WHEEL_LINES);
        }
        app.sync_detail_from_selection();
        return;
    }
    if layout.host_sidebar.contains(p) {
        app.main_panel = MainPanel::HostSidebar;
        let lines = host_sidebar_line_count(app);
        let inner_h = layout.host_sidebar_inner.height.max(1);
        paragraph_wheel(&mut app.host_sidebar_scroll, scroll_up, lines, inner_h);
        return;
    }
    if layout.header_sidebar.contains(p) {
        app.main_panel = MainPanel::HeaderSidebar;
        let lines = header_sidebar_line_count(app);
        let inner_h = layout.header_sidebar_inner.height.max(1);
        paragraph_wheel(&mut app.header_sidebar_scroll, scroll_up, lines, inner_h);
        return;
    }
    if layout.detail.contains(p) {
        app.main_panel = MainPanel::Detail;
        let detail_txt = app.detail_lines();
        let lines = detail_txt.lines().count() as u16;
        let inner_h = layout.detail_inner.height.max(1);
        paragraph_wheel(&mut app.detail_scroll, scroll_up, lines, inner_h);
        return;
    }
    if layout.response.contains(p) {
        app.main_panel = MainPanel::Response;
        let resp_lines = response_body_line_estimate(app.pending_request, app.last_http.as_ref());
        let inner_h = layout.response_inner.height.max(1);
        paragraph_wheel(&mut app.response_scroll, scroll_up, resp_lines, inner_h);
    }
}

fn response_body_line_estimate(pending: bool, last: Option<&crate::http_exec::HttpResult>) -> u16 {
    if pending {
        return 1;
    }
    let Some(h) = last else {
        return 1;
    };
    let body = truncate(&h.body, 20000);
    let txt = format!(
        "HTTP {}  |  {} ms\n{}\n\n{}",
        h.status, h.elapsed_ms, h.headers_text, body
    );
    txt.lines().count().max(1) as u16
}

fn handle_mouse_click(app: &mut App, layout: MainUiLayout, mouse: MouseEvent) {
    let x = mouse.column;
    let y = mouse.row;
    let p = ratatui::layout::Position { x, y };
    if layout.api_list.contains(p) {
        app.main_panel = MainPanel::ApiList;
        if let Some(row) = list_row_from_click(x, y, layout.api_list_inner, app.list_rows.len()) {
            app.list_state.select(Some(row));
            app.sync_detail_from_selection();
        }
        return;
    }
    if layout.host_sidebar.contains(p) {
        app.main_panel = MainPanel::HostSidebar;
        return;
    }
    if layout.header_sidebar.contains(p) {
        app.main_panel = MainPanel::HeaderSidebar;
        return;
    }
    if layout.detail.contains(p) {
        app.main_panel = MainPanel::Detail;
        return;
    }
    if layout.response.contains(p) {
        app.main_panel = MainPanel::Response;
    }
}

fn handle_mouse(app: &mut App, mouse: MouseEvent) {
    if app.show_help
        || app.request_editor_open
        || app.hosts_panel_open
        || app.headers_panel_open
        || app.search_focus
    {
        return;
    }
    let Some(layout) = app.last_main_layout else {
        return;
    };
    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => handle_mouse_click(app, layout, mouse),
        MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => handle_mouse_wheel(app, mouse),
        _ => {}
    }
}

fn draw_ui(f: &mut Frame, app: &mut App) {
    app.sync_detail_from_selection();

    let root = f.area();
    let (layout, footer_area, list_title) = prepare_main_ui(root, app);
    app.last_main_layout = Some(layout);

    let items: Vec<ListItem> = app
        .clone_list_items()
        .into_iter()
        .map(|(t, st)| ListItem::new(Line::from(vec![Span::styled(t, st)])))
        .collect();
    let list_w = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(panel_border(app.main_panel == MainPanel::ApiList))
                .title(list_title),
        )
        .highlight_style(Style::default().bg(Color::DarkGray));
    f.render_stateful_widget(list_w, layout.api_list, &mut app.list_state);

    draw_host_sidebar(f, app, layout.host_sidebar, app.main_panel == MainPanel::HostSidebar);
    draw_header_sidebar(
        f,
        app,
        layout.header_sidebar,
        app.main_panel == MainPanel::HeaderSidebar,
    );

    let detail_txt = app.detail_lines();
    let detail = Paragraph::new(detail_txt)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(panel_border(app.main_panel == MainPanel::Detail))
                .title(" [4] 详情 (s/e/预设头/a) "),
        )
        .scroll((app.detail_scroll, 0))
        .wrap(Wrap { trim: true });
    f.render_widget(detail, layout.detail);

    let resp_text = if app.pending_request {
        "请求中…".to_string()
    } else if let Some(h) = &app.last_http {
        let body = truncate(&h.body, 20000);
        format!(
            "HTTP {}  |  {} ms\n{}\n\n{}",
            h.status,
            h.elapsed_ms,
            h.headers_text,
            body
        )
    } else {
        "尚无响应。选中接口后按 s 发送（需本地服务已启动）。".into()
    };
    let resp = Paragraph::new(resp_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(panel_border(app.main_panel == MainPanel::Response))
                .title(" [5] 响应 "),
        )
        .scroll((app.response_scroll, 0))
        .wrap(Wrap { trim: true });
    f.render_widget(resp, layout.response);

    let scan_hint = if app.scan_errors.is_empty() {
        String::new()
    } else {
        format!(" | 扫描警告: {} ", app.scan_errors.len())
    };
    let idx_hint = if app.source_cache_loading {
        " | 索引DTO源码…"
    } else {
        ""
    };
    let mode = if app.search_focus {
        " [筛选]"
    } else if app.hosts_panel_open {
        " [域名]"
    } else if app.headers_panel_open {
        " [全局头]"
    } else if app.request_editor_open {
        " [请求参数]"
    } else {
        ""
    };
    let hdr_hint = match app.effective_request_header_count() {
        0 => String::new(),
        n => format!(" ({n} 头) "),
    };
    let footer = format!(
        "Base: {} {}| {}{}{} | 1–5/点击/滚轮 · ? · q · / · e · g · s · r · h · a",
        app.active_base_url(),
        hdr_hint,
        app.status_msg,
        idx_hint,
        scan_hint,
    );
    let fline = format!(
        "{}{} filter=\"{}\"",
        footer,
        mode,
        app.filter,
    );
    let bar = Paragraph::new(fline).block(Block::default().borders(Borders::ALL));
    f.render_widget(bar, footer_area);

    if app.show_help {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" 帮助 （任意键关闭） ");
        let area = centered_rect(70, 55, f.area());
        f.render_widget(Clear, area);
        let inner = block.inner(area);
        f.render_widget(block, area);
        let text = HELP_TEXT;
        let p = Paragraph::new(text).wrap(Wrap { trim: true });
        f.render_widget(p, inner);
    }

    if app.hosts_panel_open {
        draw_hosts_panel(f, app);
    }
    if app.headers_panel_open {
        draw_headers_panel(f, app);
    }
    if app.request_editor_open {
        draw_request_editor(f, app);
    }
}

fn draw_request_editor(f: &mut Frame, app: &mut App) {
    let area = centered_rect(90, 88, f.area());
    f.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" 请求参数 Path/Query/方法预设头/Body · Esc 保存退出 · Enter 换行或下一项 · Tab 切换字段 ");
    let inner = block.inner(area);
    f.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Length(2),
            Constraint::Min(14),
        ])
        .split(inner);

    let header = Paragraph::new(app.request_sheet_header_text()).wrap(Wrap { trim: true });
    f.render_widget(header, chunks[0]);

    let focus = Paragraph::new(app.request_editor_focus_label_line())
        .style(Style::default().fg(Color::Yellow));
    f.render_widget(focus, chunks[1]);

    let edit_title = if app.request_editor_is_body_row() {
        " 编辑区 · Body JSON — Esc 保存退出 · Enter 换行 · Tab 切换字段 "
    } else {
        " 编辑区 · Path/Query/头 — Esc 保存退出 · ←/→ 移动光标 · Enter 下一字段 · Tab 切换字段 "
    };
    let eb = Block::default().borders(Borders::ALL).title(edit_title);
    let einner = eb.inner(chunks[2]);
    f.render_widget(eb, chunks[2]);
    if app.request_editor_is_body_row() {
        let t = text_with_cursor(&app.body_buf, app.body_cursor_char);
        f.render_widget(Paragraph::new(t).wrap(Wrap { trim: false }), einner);
    } else {
        let t = text_with_cursor(&app.param_edit_buf, app.param_cursor_char);
        f.render_widget(Paragraph::new(t).wrap(Wrap { trim: false }), einner);
    }
}

/// 在 `cursor` 字符位置插入可见光标（▏），便于终端里辨认插入点（Body 多行或 Path/Query 单行）。
fn text_with_cursor(body: &str, cursor: usize) -> Text<'static> {
    let st = Style::default().fg(Color::Yellow).bg(Color::Rgb(45, 45, 50));
    let mut lines: Vec<Line> = Vec::new();
    let mut line_spans: Vec<Span> = Vec::new();
    let mut char_idx = 0usize;
    for c in body.chars() {
        if char_idx == cursor {
            line_spans.push(Span::styled("▏", st));
        }
        if c == '\n' {
            if char_idx == cursor {
                line_spans.push(Span::styled("▏", st));
            }
            lines.push(Line::from(line_spans));
            line_spans = Vec::new();
        } else {
            line_spans.push(Span::raw(c.to_string()));
        }
        char_idx += 1;
    }
    if char_idx == cursor {
        line_spans.push(Span::styled("▏", st));
    }
    lines.push(Line::from(line_spans));
    Text::from(lines)
}

const HELP_TEXT: &str = r"lazypost — Spring API 扫描与调试

1–5         主界面切换焦点区块（标题 [1] 列表 [2] 域名 [3] 全局头 [4] 详情 [5] 响应；黄框为当前块）
鼠标左键     点击某区块切换焦点；点在接口列表内会选中对应行（需终端支持鼠标）
滚轮         指针悬停在各区块上时上下滚动（列表会顺带移动选中行）
q           退出
?           本帮助
/           聚焦底栏筛选（Enter 或 Esc 均可退出筛选）
g           列表分组循环：按项目目录（默认）→ 平铺 → 按类；进入时项目与类默认全折叠
r           重新扫描项目
h           域名面板：多 URL/描述、e 改 URL、v 改描述、Enter 选用（左侧栏同步展示）
a           全局附加请求头（所有接口共性，如 Token）；发送顺序：a → 详情按 GET/POST 等方法预设头 → 扫描到的 @RequestHeader（同名后者覆盖）
[ / ]       详情区在 Path / Query / 方法预设请求头 行间切换焦点（▶）
j / ↓       在列表行上移动（含类标题与接口）
k / ↑       同上
Space / Enter  分组模式下选中 📁 项目行或类标题时：折叠/展开
s           发送当前请求（path/query/方法预设头/body；预设头含 Accept、有 Body 时常含 Content-Type: application/json）
e           打开/关闭请求参数（Path / Query / 方法预设头 / Body 同一窗口；Esc 保存退出；非 Body 行可按 o 关闭）
Tab         请求参数窗口内：切换字段（Shift+Tab 上一字段）；非 Body 与 ↑/↓/j/k 换字段作用相同（二选一即可）
Ctrl+Q      仅在「请求参数」窗口内退出程序；主界面用 q
";

fn centered_rect(pct_x: u16, pct_y: u16, r: Rect) -> Rect {
    let pop_w = (r.width * pct_x / 100).max(20);
    let pop_h = (r.height * pct_y / 100).max(5);
    let x = r.x + (r.width.saturating_sub(pop_w)) / 2;
    let y = r.y + (r.height.saturating_sub(pop_h)) / 2;
    Rect::new(x, y, pop_w, pop_h)
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…\n\n[已截断，共 {} 字节]", &s[..max], s.len())
    }
}

fn handle_paste(app: &mut App, pasted: &str) {
    if app.request_editor_open {
        if app.request_editor_is_body_row() {
            app.body_insert_str(pasted);
        } else {
            app.param_insert_str(pasted);
        }
    } else if app.hosts_panel_open && app.hosts_edit_line {
        app.host_insert_str(pasted);
    } else if app.headers_panel_open && app.headers_edit_line {
        app.header_insert_str(pasted);
    } else if app.search_focus {
        app.filter.push_str(pasted);
        app.refresh_filter();
    }
}

fn handle_key(
    app: &mut App,
    key: event::KeyEvent,
    tx: &mpsc::Sender<Result<crate::http_exec::HttpResult, String>>,
    scan_tx: &mpsc::Sender<ScanReport>,
) -> anyhow::Result<bool> {
    use KeyCode::*;

    if app.show_help {
        app.show_help = false;
        return Ok(false);
    }

    if app.request_editor_open {
        // 避免 q/j/k/o 与「退出 / vim 式换行 / 关表单」冲突：Body 编辑区里这些键必须能当普通字符输入
        if matches!(key.code, Char('q') | Char('Q')) && key.modifiers.contains(KeyModifiers::CONTROL) {
            return Ok(true);
        }
        match key.code {
            Esc => {
                app.save_request_editor_row();
                app.request_editor_open = false;
            }
            Down => {
                if app.request_editor_is_body_row() {
                    app.body_cursor_down();
                } else {
                    app.move_request_editor_focus(1);
                }
            }
            Up => {
                if app.request_editor_is_body_row() {
                    app.body_cursor_up();
                } else {
                    app.move_request_editor_focus(-1);
                }
            }
            Left => {
                if app.request_editor_is_body_row() {
                    app.body_cursor_left();
                } else {
                    app.param_cursor_left();
                }
            }
            Right => {
                if app.request_editor_is_body_row() {
                    app.body_cursor_right();
                } else {
                    app.param_cursor_right();
                }
            }
            Tab => app.move_request_editor_focus(1),
            BackTab => app.move_request_editor_focus(-1),
            Enter => {
                if app.request_editor_is_body_row() {
                    app.body_insert_char('\n');
                } else {
                    app.move_request_editor_focus(1);
                }
            }
            Char(c) => {
                if app.request_editor_is_body_row() {
                    app.body_insert_char(c);
                } else {
                    match c {
                        'j' => app.move_request_editor_focus(1),
                        'k' => app.move_request_editor_focus(-1),
                        'o' | 'O' => {
                            app.save_request_editor_row();
                            app.request_editor_open = false;
                        }
                        _ => app.param_insert_char(c),
                    }
                }
            }
            Backspace => {
                if app.request_editor_is_body_row() {
                    app.body_backspace();
                } else {
                    app.param_backspace();
                }
            }
            Delete => {
                if app.request_editor_is_body_row() {
                    app.body_delete_forward();
                } else {
                    app.param_delete_forward();
                }
            }
            _ => {}
        }
        return Ok(false);
    }

    if app.hosts_panel_open {
        if app.hosts_edit_line {
            match key.code {
                Esc => app.cancel_host_line_edit(),
                Enter => app.commit_host_line_edit(),
                Left => app.host_cursor_left(),
                Right => app.host_cursor_right(),
                Char(c) => app.host_insert_char(c),
                Backspace => app.host_backspace(),
                Delete => app.host_delete_forward(),
                _ => {}
            }
            return Ok(false);
        }
        match key.code {
            Esc => app.hosts_panel_open = false,
            Enter => app.set_active_host_from_cursor(),
            Char('n') | Char('N') => app.add_host_row(),
            Char('d') | Char('D') => app.delete_host_at_cursor(),
            Char('e') | Char('E') => app.begin_edit_host_url(),
            Char('v') | Char('V') => app.begin_edit_host_description(),
            Down | Char('j') => {
                let i = app.hosts_list_state.selected().unwrap_or(0);
                let max = app.hosts.len().saturating_sub(1);
                app.hosts_list_state
                    .select(Some((i + 1).min(max)));
            }
            Up | Char('k') => {
                let i = app.hosts_list_state.selected().unwrap_or(0);
                app.hosts_list_state.select(Some(i.saturating_sub(1)));
            }
            _ => {}
        }
        return Ok(false);
    }

    if app.headers_panel_open {
        if app.headers_edit_line {
            match key.code {
                Esc => app.cancel_header_line_edit(),
                Enter => app.commit_header_line_edit(),
                Left => app.header_cursor_left(),
                Right => app.header_cursor_right(),
                Char(c) => app.header_insert_char(c),
                Backspace => app.header_backspace(),
                Delete => app.header_delete_forward(),
                _ => {}
            }
            return Ok(false);
        }
        match key.code {
            Esc => app.headers_panel_open = false,
            Enter => app.set_active_header_from_cursor(),
            Char('n') | Char('N') => app.add_header_row(),
            Char('d') | Char('D') => app.delete_header_at_cursor(),
            Char('e') | Char('E') => app.begin_edit_header_name(),
            Char('v') | Char('V') => app.begin_edit_header_value(),
            Char('b') | Char('B') => app.begin_edit_header_description(),
            Down | Char('j') => {
                let i = app.headers_list_state.selected().unwrap_or(0);
                let max = app.request_headers.len().saturating_sub(1);
                app.headers_list_state
                    .select(Some((i + 1).min(max)));
            }
            Up | Char('k') => {
                let i = app.headers_list_state.selected().unwrap_or(0);
                app.headers_list_state.select(Some(i.saturating_sub(1)));
            }
            _ => {}
        }
        return Ok(false);
    }

    if app.search_focus {
        match key.code {
            Esc => app.search_focus = false,
            Enter => app.search_focus = false,
            Char(c) => app.filter.push(c),
            Backspace => {
                app.filter.pop();
            }
            _ => {}
        }
        app.refresh_filter();
        return Ok(false);
    }

    match key.code {
        Char(c @ '1'..='5') => {
            if let Some(p) = MainPanel::from_digit(c) {
                app.main_panel = p;
            }
        }
        Char('q') | Char('Q') => return Ok(true),
        Char('?') => app.show_help = true,
        Char('/') => {
            app.search_focus = true;
        }
        Char('g') | Char('G') => {
            app.cycle_list_group_mode();
            app.status_msg = match app.list_group_mode {
                ListGroupMode::ByController => "列表：按 Controller 类分组".into(),
                ListGroupMode::ByProjectFolder => {
                    "列表：按首级项目目录分组（适合工作区下多子项目）".into()
                }
                ListGroupMode::Flat => "列表：平铺".into(),
            };
        }
        Char(' ') | Enter => {
            if !app.toggle_collapse_selected_header() && app.list_group_mode != ListGroupMode::Flat {
                app.status_msg =
                    "折叠：选中 📁 项目行或黄色类行后按 Space 或 Enter".into();
            }
        }
        Char('r') | Char('R') => {
            if app.scan_in_flight {
                app.status_msg = "仍在扫描，请稍候再试".into();
            } else {
                app.scan_in_flight = true;
                app.status_msg = "正在重新扫描…".into();
                let p = app.project.clone();
                let sx = scan_tx.clone();
                std::thread::spawn(move || {
                    let _ = sx.send(scanner::scan_project_with_mode(&p, ScanMode::LazyEndpoints));
                });
            }
        }
        Char('h') | Char('H') => {
            app.open_hosts_panel();
        }
        Char('a') | Char('A') => {
            app.open_headers_panel();
        }
        Char('e') | Char('E') => {
            match app.toggle_request_editor() {
                Ok(()) => {}
                Err(m) => app.status_msg = m.into(),
            }
        }
        Char('[') => app.move_param_focus(-1),
        Char(']') => app.move_param_focus(1),
        Char('s') | Char('S') => {
            if app.pending_request {
                app.status_msg = "尚有请求在执行".into();
                return Ok(false);
            }
            if let Some(api) = app.current_api.clone() {
                let url_m = crate::http_exec::compose_url(
                    app.active_base_url(),
                    &api.path,
                    &app.path_vals,
                    &app.query_vals,
                );
                let url = match url_m {
                    Ok(u) => u,
                    Err(e) => {
                        app.status_msg = e;
                        return Ok(false);
                    }
                };
                let method = api.http_method.clone();
                let headers = app.merged_http_headers();
                let body_owned = api
                    .body_binding
                    .as_ref()
                    .map(|_| app.body_draft.clone());
                app.pending_request = true;
                app.status_msg = format!("→ {method} {url}");
                let tx2 = tx.clone();
                std::thread::spawn(move || {
                    let body_ref = body_owned.as_deref();
                    let res = crate::http_exec::send_http(
                        &method,
                        &url,
                        &headers,
                        body_ref,
                        30,
                    );
                    let _ = tx2.send(res);
                });
            } else {
                app.status_msg = "请先选择接口".into();
            }
        }
        Char('j') | Down => app.move_sel(1),
        Char('k') | Up => app.move_sel(-1),
        _ => {}
    }

    Ok(false)
}
