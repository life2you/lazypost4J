//! 全屏 TUI（ratatui + crossterm）。

mod app;
mod body_cursor;
mod dir_picker;

use std::io::stdout;
use std::path::PathBuf;

pub use dir_picker::pick_project_dir;
use std::sync::mpsc;
use std::time::Duration;

use crate::scanner::{self, JavaSourceCache, ScanMode, ScanReport};
use app::{App, DetailPane, HostsEditKind, MainPanel, MainUiLayout, RequestHeaderEditKind};
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
                    app.set_last_http_response(h);
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
                let mark = if i == app.base_url_index {
                    "▶ "
                } else {
                    "  "
                };
                let label = h.display_label();
                let st = if i == app.base_url_index {
                    Style::default().fg(Color::Green)
                } else {
                    Style::default()
                };
                let line1 = Line::from(vec![Span::styled(mark, st), Span::styled(label, st)]);
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
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" 已保存的域名 "),
            );
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
                let line1 = Line::from(vec![Span::styled(mark, st), Span::styled(label, st)]);
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
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" 已保存的请求头 "),
            );
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
    let title = if focused {
        " [3] 全局头 · e 编辑 "
    } else {
        " [3] 全局头 "
    };
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
                let l1 = Line::from(vec![Span::styled(mark, st), Span::styled(label, st)]);
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
                .title(title),
        )
        .scroll((app.header_sidebar_scroll, 0))
        .wrap(Wrap { trim: true });
    f.render_widget(p, area);
}

fn draw_host_sidebar(f: &mut Frame, app: &App, area: Rect, focused: bool) {
    let title = if focused {
        " [2] 域名 · e 编辑 "
    } else {
        " [2] 域名 "
    };
    let lines: Vec<Line> = app
        .hosts
        .iter()
        .enumerate()
        .flat_map(|(i, h)| {
            let mark = if i == app.base_url_index {
                "▶ "
            } else {
                "  "
            };
            let label = h.display_label();
            let st = if i == app.base_url_index {
                Style::default().fg(Color::Green)
            } else {
                Style::default()
            };
            let l1 = Line::from(vec![Span::styled(mark, st), Span::styled(label, st)]);
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
                .title(title),
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

    let list_title = format!(" [1] 接口列表{} ", app.list_title_suffix());
    let list_block = Block::default()
        .borders(Borders::ALL)
        .title(list_title.as_str());
    let api_list = left_col[0];
    let api_list_inner = list_block.inner(api_list);

    let host_sidebar = left_col[1];
    let host_block = Block::default().borders(Borders::ALL).title(" [2] 域名 ");
    let host_sidebar_inner = host_block.inner(host_sidebar);

    let header_sidebar = left_col[2];
    let hdr_block = Block::default().borders(Borders::ALL).title(" [3] 全局头 ");
    let header_sidebar_inner = hdr_block.inner(header_sidebar);

    let detail = top[1];
    let detail_block = Block::default().borders(Borders::ALL).title(" 详情 ");
    let detail_inner = detail_block.inner(detail);

    let detail_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(6), Constraint::Min(10)])
        .split(detail_inner);
    let detail_summary = detail_chunks[0];

    let detail_panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(34),
            Constraint::Percentage(28),
            Constraint::Percentage(38),
        ])
        .split(detail_chunks[1]);
    let detail_params = detail_panes[0];
    let detail_params_inner = Block::default()
        .borders(Borders::ALL)
        .title(" [4] Params ")
        .inner(detail_params);
    let detail_headers = detail_panes[1];
    let detail_headers_inner = Block::default()
        .borders(Borders::ALL)
        .title(" [5] Headers ")
        .inner(detail_headers);
    let detail_body = detail_panes[2];
    let detail_body_inner = Block::default()
        .borders(Borders::ALL)
        .title(" [6] Body ")
        .inner(detail_body);

    let response = main[1];
    let resp_block = Block::default().borders(Borders::ALL).title(" [7] 响应 ");
    let response_inner = resp_block.inner(response);

    let layout = MainUiLayout {
        api_list,
        api_list_inner,
        host_sidebar,
        host_sidebar_inner,
        header_sidebar,
        header_sidebar_inner,
        detail,
        detail_summary,
        detail_params,
        detail_params_inner,
        detail_headers,
        detail_headers_inner,
        detail_body,
        detail_body_inner,
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
            app.move_sel(-1);
        } else {
            app.move_sel(1);
        }
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
        if layout.detail_params.contains(p) {
            app.detail_pane = DetailPane::Params;
            let lines = app.detail_params_text().lines().count() as u16;
            let inner_h = layout.detail_params_inner.height.max(1);
            paragraph_wheel(&mut app.detail_params_scroll, scroll_up, lines, inner_h);
        } else if layout.detail_headers.contains(p) {
            app.detail_pane = DetailPane::Headers;
            let lines = app.detail_headers_text().lines().count() as u16;
            let inner_h = layout.detail_headers_inner.height.max(1);
            paragraph_wheel(&mut app.detail_headers_scroll, scroll_up, lines, inner_h);
        } else if layout.detail_body.contains(p) {
            app.detail_pane = DetailPane::Body;
            let lines = app.detail_body_text().lines().count() as u16;
            let inner_h = layout.detail_body_inner.height.max(1);
            paragraph_wheel(&mut app.detail_body_scroll, scroll_up, lines, inner_h);
        }
        return;
    }
    if layout.response.contains(p) {
        app.main_panel = MainPanel::Response;
        let resp_lines = response_body_line_estimate(app);
        let inner_h = layout.response_inner.height.max(1);
        paragraph_wheel(&mut app.response_scroll, scroll_up, resp_lines, inner_h);
    }
}

fn response_body_line_estimate(app: &App) -> u16 {
    if app.pending_request {
        return 1;
    }
    let Some(h) = app.last_http.as_ref() else {
        return 1;
    };
    if app.response_json.is_some() {
        let lines = 2usize + h.headers_text.lines().count() + 1 + app.response_json_lines().len();
        return lines.max(1) as u16;
    }
    let body = truncate(&h.body, 20000);
    let txt = format!(
        "HTTP {}  |  {} ms\n{}\n\n{}",
        h.status, h.elapsed_ms, h.headers_text, body
    );
    txt.lines().count().max(1) as u16
}

fn build_response_text(app: &App) -> Text<'static> {
    if app.pending_request {
        return Text::from("请求中…");
    }
    let Some(h) = &app.last_http else {
        return Text::from("尚无响应。选中接口后按 s 发送（需本地服务已启动）。");
    };
    if app.response_json.is_none() {
        let body = truncate(&h.body, 20000);
        return Text::from(format!(
            "HTTP {}  |  {} ms\n{}\n\n{}",
            h.status, h.elapsed_ms, h.headers_text, body
        ));
    }

    let mut lines = Vec::new();
    lines.push(Line::from(format!("HTTP {}  |  {} ms", h.status, h.elapsed_ms)));
    for hdr in h.headers_text.lines() {
        lines.push(Line::from(hdr.to_string()));
    }
    lines.push(Line::from(""));
    for (i, row) in app.response_json_lines().iter().enumerate() {
        let indent = "  ".repeat(row.depth);
        let marker = if row.expandable {
            if row.expanded { "▼ " } else { "▶ " }
        } else {
            "  "
        };
        let style = if app.main_panel == MainPanel::Response && i == app.response_focus {
            Style::default().fg(Color::Yellow).bg(Color::Rgb(45, 45, 50))
        } else {
            Style::default()
        };
        lines.push(Line::from(Span::styled(
            format!("{indent}{marker}{}", row.label),
            style,
        )));
    }
    Text::from(lines)
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
        if layout.detail_params.contains(p) {
            app.detail_pane = DetailPane::Params;
        } else if layout.detail_headers.contains(p) {
            app.detail_pane = DetailPane::Headers;
        } else if layout.detail_body.contains(p) {
            app.detail_pane = DetailPane::Body;
        }
        return;
    }
    if layout.response.contains(p) {
        app.main_panel = MainPanel::Response;
    }
}

fn handle_mouse(app: &mut App, mouse: MouseEvent) {
    if app.response_view_open {
        match mouse.kind {
            MouseEventKind::ScrollUp => app.scroll_response_view_by(-(WHEEL_LINES as i32)),
            MouseEventKind::ScrollDown => app.scroll_response_view_by(WHEEL_LINES as i32),
            _ => {}
        }
        return;
    }
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

    draw_host_sidebar(
        f,
        app,
        layout.host_sidebar,
        app.main_panel == MainPanel::HostSidebar,
    );
    draw_header_sidebar(
        f,
        app,
        layout.header_sidebar,
        app.main_panel == MainPanel::HeaderSidebar,
    );

    let detail_outer = Block::default()
        .borders(Borders::ALL)
        .border_style(panel_border(app.main_panel == MainPanel::Detail))
        .title(" 详情 ");
    f.render_widget(detail_outer, layout.detail);

    let summary = Paragraph::new(app.detail_summary_text())
        .block(Block::default().borders(Borders::ALL).title(" Summary "))
        .wrap(Wrap { trim: true });
    f.render_widget(summary, layout.detail_summary);

    let params_title =
        if app.main_panel == MainPanel::Detail && app.detail_pane == DetailPane::Params {
            " [4] Params · e 编辑 "
        } else {
            " [4] Params "
        };
    let params = Paragraph::new(app.detail_params_text())
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(panel_border(
                    app.main_panel == MainPanel::Detail && app.detail_pane == DetailPane::Params,
                ))
                .title(params_title),
        )
        .scroll((app.detail_params_scroll, 0))
        .wrap(Wrap { trim: true });
    f.render_widget(params, layout.detail_params);

    let headers_title =
        if app.main_panel == MainPanel::Detail && app.detail_pane == DetailPane::Headers {
            " [5] Headers · e 编辑 "
        } else {
            " [5] Headers "
        };
    let headers = Paragraph::new(app.detail_headers_text())
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(panel_border(
                    app.main_panel == MainPanel::Detail && app.detail_pane == DetailPane::Headers,
                ))
                .title(headers_title),
        )
        .scroll((app.detail_headers_scroll, 0))
        .wrap(Wrap { trim: true });
    f.render_widget(headers, layout.detail_headers);

    let body_title = if app.main_panel == MainPanel::Detail && app.detail_pane == DetailPane::Body {
        " [6] Body · e 编辑 "
    } else {
        " [6] Body "
    };
    let body = Paragraph::new(app.detail_body_text())
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(panel_border(
                    app.main_panel == MainPanel::Detail && app.detail_pane == DetailPane::Body,
                ))
                .title(body_title),
        )
        .scroll((app.detail_body_scroll, 0))
        .wrap(Wrap { trim: false });
    f.render_widget(body, layout.detail_body);

    let resp_title = if app.main_panel == MainPanel::Response && app.response_json.is_some() {
        " [7] 响应 · v 原始弹窗 · ↑/↓ 选择 · Enter/Space 折叠 "
    } else if app.main_panel == MainPanel::Response {
        " [7] 响应 · v 原始弹窗 "
    } else {
        " [7] 响应 "
    };
    let resp = Paragraph::new(build_response_text(app))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(panel_border(app.main_panel == MainPanel::Response))
                .title(resp_title),
        )
        .scroll((app.response_scroll, 0))
        .wrap(Wrap { trim: false });
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
        "Base: {} {}| {}{}{} | ?:help  f:模糊搜索接口  s:发送  q:退出",
        app.active_base_url(),
        hdr_hint,
        app.status_msg,
        idx_hint,
        scan_hint,
    );
    let fline = format!("{}{} filter=\"{}\"", footer, mode, app.filter,);
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
    if app.response_view_open {
        draw_response_viewer(f, app);
    }
}

fn draw_request_editor(f: &mut Frame, app: &mut App) {
    let area = match app.detail_pane {
        DetailPane::Body => centered_rect(90, 88, f.area()),
        DetailPane::Params => centered_rect(86, 76, f.area()),
        DetailPane::Headers => centered_rect(82, 72, f.area()),
    };
    f.render_widget(Clear, area);
    let title = match app.detail_pane {
        DetailPane::Params => " 编辑 Params · Esc 关闭 ",
        DetailPane::Headers => " 编辑 Headers · Esc 关闭 ",
        DetailPane::Body => " 编辑 Body · Esc 关闭 ",
    };
    let block = Block::default().borders(Borders::ALL).title(title);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(2),
            Constraint::Length(2),
            Constraint::Min(14),
        ])
        .split(inner);

    let header = Paragraph::new(app.request_sheet_header_text()).wrap(Wrap { trim: true });
    f.render_widget(header, chunks[0]);

    let err = app.request_editor_error.clone().unwrap_or_default();
    let err_style = if app.request_editor_error.is_some() {
        Style::default().fg(Color::Red)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    f.render_widget(Paragraph::new(err).style(err_style), chunks[1]);

    let focus = Paragraph::new(app.request_editor_focus_label_line())
        .style(Style::default().fg(Color::Yellow));
    f.render_widget(focus, chunks[2]);

    let edit_title = match app.detail_pane {
        DetailPane::Params => " 编辑区 ",
        DetailPane::Headers => " 编辑区 ",
        DetailPane::Body => " 编辑区 · Body JSON — Enter 换行 · - 清空 · Esc 校验并退出 ",
    };
    let eb = Block::default().borders(Borders::ALL).title(edit_title);
    let einner = eb.inner(chunks[3]);
    f.render_widget(eb, chunks[3]);
    if app.request_editor_is_body_row() {
        let t = text_with_cursor(&app.body_buf, app.body_cursor_char);
        f.render_widget(Paragraph::new(t).wrap(Wrap { trim: false }), einner);
    } else {
        let t = text_with_cursor(&app.param_edit_buf, app.param_cursor_char);
        f.render_widget(Paragraph::new(t).wrap(Wrap { trim: false }), einner);
    }
}

fn draw_response_viewer(f: &mut Frame, app: &mut App) {
    let area = centered_rect(92, 88, f.area());
    f.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" 原始响应 · Esc 关闭 · ↑/↓/j/k/PgUp/PgDn 滚动 ");
    let inner = block.inner(area);
    f.render_widget(block, area);
    let wrapped = soft_wrap_lines(&app.response_popup_text(), inner.width);
    let max_scroll = wrapped
        .len()
        .saturating_sub(inner.height.max(1) as usize) as u16;
    app.set_response_view_max_scroll(max_scroll);
    let visible = wrapped
        .into_iter()
        .skip(app.response_view_scroll as usize)
        .take(inner.height.max(1) as usize)
        .collect::<Vec<_>>()
        .join("\n");
    let para = Paragraph::new(visible).wrap(Wrap { trim: false });
    f.render_widget(para, inner);
}

/// 在 `cursor` 字符位置插入可见光标（▏），便于终端里辨认插入点（Body 多行或 Path/Query 单行）。
fn text_with_cursor(body: &str, cursor: usize) -> Text<'static> {
    let st = Style::default()
        .fg(Color::Yellow)
        .bg(Color::Rgb(45, 45, 50));
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

1–3         主界面切换模块：1 列表，2 域名，3 全局头
4–7         详情/响应模块：4 Params，5 Headers，6 Body，7 响应
列表区       ↑/↓ 切换接口；Space / Enter 折叠或展开项目/类分组
鼠标左键     点击某区块切换焦点；点在接口列表内会选中对应行（需终端支持鼠标）
            详情区内点击 Params / Headers / Body 或响应区都可切换焦点
滚轮         指针悬停在各区块上时上下滚动（列表区会切换接口）
e           编辑当前模块：2 域名，3 全局头，4 Params，5 Headers，6 Body
s           发送当前接口请求
v           在 [7] 响应模块打开原始返回结果弹窗
q           退出
?           打开或关闭帮助
f           打开或关闭接口模糊搜索
            搜索状态下可直接输入、粘贴和 Backspace 修改关键字
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

fn soft_wrap_lines(text: &str, width: u16) -> Vec<String> {
    let w = width.max(1) as usize;
    let mut out = Vec::new();
    for line in text.lines() {
        if line.is_empty() {
            out.push(String::new());
            continue;
        }
        let chars: Vec<char> = line.chars().collect();
        for chunk in chars.chunks(w) {
            out.push(chunk.iter().collect());
        }
    }
    if out.is_empty() {
        out.push(String::new());
    }
    out
}

fn handle_paste(app: &mut App, pasted: &str) {
    if app.request_editor_open {
        if app.request_editor_is_body_row() {
            app.request_editor_error = None;
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
        match key.code {
            Esc | Char('?') => app.show_help = false,
            _ => {}
        }
        return Ok(false);
    }

    if app.response_view_open {
        match key.code {
            Esc => app.response_view_open = false,
            Up | Char('k') | Char('K') => app.scroll_response_view_by(-1),
            Down | Char('j') | Char('J') => app.scroll_response_view_by(1),
            PageUp => app.scroll_response_view_by(-10),
            PageDown => app.scroll_response_view_by(10),
            _ => {}
        }
        return Ok(false);
    }

    if app.request_editor_open {
        if matches!(key.code, Char('q') | Char('Q'))
            && key.modifiers.contains(KeyModifiers::CONTROL)
        {
            app.save_request_editor_row();
            app.persist_current_request_draft();
            return Ok(true);
        }
        match key.code {
            Esc => {
                if app.request_editor_is_body_row() {
                    if let Err(err) = app.validate_body_editor_json() {
                        app.request_editor_error = Some(err.clone());
                        app.status_msg = err;
                        return Ok(false);
                    }
                }
                app.request_editor_error = None;
                app.save_request_editor_row();
                app.persist_current_request_draft();
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
            Tab => {
                if app.detail_pane == DetailPane::Headers
                    && app.request_editor_on_extra_header_row()
                {
                    app.save_request_editor_row();
                    app.request_editor_extra_kv = !app.request_editor_extra_kv;
                    app.load_request_editor_row();
                } else {
                    app.move_request_editor_focus(1);
                }
            }
            BackTab => {
                if app.detail_pane == DetailPane::Headers
                    && app.request_editor_on_extra_header_row()
                {
                    app.save_request_editor_row();
                    app.request_editor_extra_kv = !app.request_editor_extra_kv;
                    app.load_request_editor_row();
                } else {
                    app.move_request_editor_focus(-1);
                }
            }
            Enter => {
                if app.request_editor_is_body_row() {
                    app.request_editor_error = None;
                    app.body_insert_char('\n');
                } else {
                    app.param_insert_char('\n');
                }
            }
            Char(c) => {
                if app.request_editor_is_body_row() {
                    if c == '-' {
                        app.request_editor_error = None;
                        app.clear_body_editor();
                        app.status_msg = "已清空 Body".into();
                    } else {
                        app.request_editor_error = None;
                        app.body_insert_char(c);
                    }
                } else if app.detail_pane == DetailPane::Params && c == '+' {
                    app.add_extra_query_param();
                    app.status_msg = "已新增参数".into();
                } else if app.detail_pane == DetailPane::Headers && c == '+' {
                    app.add_extra_request_header();
                    app.status_msg = "已新增 Header".into();
                } else if app.detail_pane == DetailPane::Params
                    && c == '-'
                    && app.request_editor_on_extra_query_row()
                {
                    app.delete_extra_query_row_at_focus();
                    app.status_msg = "已删除参数".into();
                } else if app.detail_pane == DetailPane::Headers
                    && c == '-'
                    && app.request_editor_on_extra_header_row()
                {
                    app.delete_extra_header_row_at_focus();
                    app.status_msg = "已删除 Header".into();
                } else {
                    app.param_insert_char(c);
                }
            }
            Backspace => {
                if app.request_editor_is_body_row() {
                    app.request_editor_error = None;
                    app.body_backspace();
                } else {
                    app.param_backspace();
                }
            }
            Delete => {
                if app.request_editor_is_body_row() {
                    app.request_editor_error = None;
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
            Down | Char('j') | Char('J') => {
                let i = app.hosts_list_state.selected().unwrap_or(0);
                let max = app.hosts.len().saturating_sub(1);
                app.hosts_list_state.select(Some((i + 1).min(max)));
            }
            Up | Char('k') | Char('K') => {
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
            Down | Char('j') | Char('J') => {
                let i = app.headers_list_state.selected().unwrap_or(0);
                let max = app.request_headers.len().saturating_sub(1);
                app.headers_list_state.select(Some((i + 1).min(max)));
            }
            Up | Char('k') | Char('K') => {
                let i = app.headers_list_state.selected().unwrap_or(0);
                app.headers_list_state.select(Some(i.saturating_sub(1)));
            }
            _ => {}
        }
        return Ok(false);
    }

    if app.search_focus {
        match key.code {
            Esc | Enter => app.search_focus = false,
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
        Char('q') | Char('Q') => {
            app.persist_current_request_draft();
            return Ok(true);
        }
        Char('?') => {
            app.show_help = true;
            return Ok(false);
        }
        Char('f') | Char('F') => {
            app.search_focus = true;
            return Ok(false);
        }
        Char('s') | Char('S') => {
            if app.pending_request {
                app.status_msg = "已有请求进行中".into();
                return Ok(false);
            }
            app.persist_current_request_draft();
            match app.prepare_request_for_send() {
                Ok((method, full_url, headers, body)) => {
                    app.pending_request = true;
                    app.status_msg = format!("发送中: {} {}", method, full_url);
                    app.response_scroll = 0;
                    let tx = tx.clone();
                    std::thread::spawn(move || {
                        let result = crate::http_exec::send_http(
                            &method,
                            &full_url,
                            &headers,
                            body.as_deref(),
                            30,
                        );
                        let _ = tx.send(result);
                    });
                }
                Err(err) => app.status_msg = err,
            }
            return Ok(false);
        }
        Char('v') | Char('V') => {
            if app.main_panel == MainPanel::Response {
                if let Err(err) = app.open_response_view() {
                    app.status_msg = err.into();
                }
                return Ok(false);
            }
        }
        _ => {}
    }

    if let Char(c @ '1'..='7') = key.code {
        app.select_main_module_digit(c);
        return Ok(false);
    }

    if matches!(key.code, Char('e') | Char('E')) {
        if let Err(m) = app.edit_current_module() {
            app.status_msg = m.into();
        }
    } else if app.main_panel == MainPanel::Response {
        let visible_h = app
            .last_main_layout
            .map(|layout| layout.response_inner.height.max(1))
            .unwrap_or(1);
        match key.code {
            Up => {
                app.move_response_focus(-1);
                app.ensure_response_focus_visible(visible_h);
            }
            Down => {
                app.move_response_focus(1);
                app.ensure_response_focus_visible(visible_h);
            }
            Char(' ') | Enter => {
                if !app.toggle_response_node_at_focus() {
                    app.status_msg = "当前响应节点不可折叠".into();
                } else {
                    app.ensure_response_focus_visible(visible_h);
                }
            }
            _ => {}
        }
    } else if app.main_panel == MainPanel::ApiList {
        match key.code {
            Up => app.move_sel(-1),
            Down => app.move_sel(1),
            Char(' ') | Enter => {
                if !app.toggle_collapse_selected_header() {
                    app.status_msg = "折叠/展开仅对列表里的项目分组和类分组生效".into();
                }
            }
            _ => {}
        }
    }

    let _ = tx;
    let _ = scan_tx;

    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ApiParam, LocalApi, ParamLocation};
    use std::path::PathBuf;

    fn req_param(name: &str, location: ParamLocation, required: bool) -> ApiParam {
        ApiParam {
            name: name.into(),
            java_type: "String".into(),
            location,
            required,
            default_value: None,
        }
    }

    fn make_app_with_api(api: LocalApi) -> App {
        let mut app = App::new(PathBuf::from("."));
        app.apis = vec![api];
        app.list_rows = vec![app::ListRow::Endpoint { api_index: 0 }];
        app.list_state.select(Some(0));
        app.sync_detail_from_selection();
        app
    }

    #[test]
    fn digits_type_into_host_editor_instead_of_switching_modules() {
        let mut api = LocalApi::new_stub(
            "1".into(),
            "Demo.post".into(),
            "DemoController".into(),
            "Demo.java".into(),
            1,
            "POST".into(),
            "/demo".into(),
            ".".into(),
        );
        api.body_binding = Some(req_param("body", ParamLocation::Body, true));

        let mut app = make_app_with_api(api);
        app.hosts_panel_open = true;
        app.hosts_edit_line = true;
        app.host_buf.clear();
        app.host_cursor_char = 0;

        let (tx, _rx) = mpsc::channel::<Result<crate::http_exec::HttpResult, String>>();
        let (scan_tx, _scan_rx) = mpsc::channel::<ScanReport>();

        let _ = handle_key(
            &mut app,
            event::KeyEvent::new(KeyCode::Char('6'), KeyModifiers::NONE),
            &tx,
            &scan_tx,
        )
        .unwrap();
        assert!(app.hosts_panel_open);
        assert!(app.hosts_edit_line);
        assert_eq!(app.main_panel, MainPanel::ApiList);
        assert_eq!(app.host_buf, "6");
    }

    #[test]
    fn digits_type_into_request_editor_instead_of_switching_modules() {
        let mut api = LocalApi::new_stub(
            "1".into(),
            "Demo.post".into(),
            "DemoController".into(),
            "Demo.java".into(),
            1,
            "POST".into(),
            "/demo".into(),
            ".".into(),
        );
        api.body_binding = Some(req_param("body", ParamLocation::Body, true));

        let mut app = make_app_with_api(api);
        app.detail_pane = DetailPane::Body;
        app.open_request_editor_for_detail_pane().unwrap();

        let (tx, _rx) = mpsc::channel::<Result<crate::http_exec::HttpResult, String>>();
        let (scan_tx, _scan_rx) = mpsc::channel::<ScanReport>();

        let _ = handle_key(
            &mut app,
            event::KeyEvent::new(KeyCode::Char('6'), KeyModifiers::NONE),
            &tx,
            &scan_tx,
        )
        .unwrap();

        assert!(app.request_editor_open);
        assert_eq!(app.main_panel, MainPanel::ApiList);
        assert_eq!(app.detail_pane, DetailPane::Body);
        assert!(app.body_buf.ends_with('6'));
    }

    #[test]
    fn response_panel_v_opens_raw_response_popup() {
        let api = LocalApi::new_stub(
            "1".into(),
            "Demo.get".into(),
            "DemoController".into(),
            "Demo.java".into(),
            1,
            "GET".into(),
            "/demo".into(),
            ".".into(),
        );

        let mut app = make_app_with_api(api);
        app.main_panel = MainPanel::Response;
        app.set_last_http_response(crate::http_exec::HttpResult {
            status: 200,
            elapsed_ms: 8,
            headers_text: "content-type: application/json".into(),
            body: "{\"ok\":true}".into(),
        });

        let (tx, _rx) = mpsc::channel::<Result<crate::http_exec::HttpResult, String>>();
        let (scan_tx, _scan_rx) = mpsc::channel::<ScanReport>();

        let _ = handle_key(
            &mut app,
            event::KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE),
            &tx,
            &scan_tx,
        )
        .unwrap();

        assert!(app.response_view_open);
        assert!(app.response_popup_text().contains("\"ok\": true"));
    }

    #[test]
    fn soft_wrap_lines_accounts_for_soft_wrap() {
        assert_eq!(soft_wrap_lines("abcdef", 4), vec!["abcd", "ef"]);
        assert_eq!(soft_wrap_lines("a\nbcdef", 4), vec!["a", "bcde", "f"]);
    }
}
