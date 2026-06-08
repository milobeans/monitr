use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use crossterm::event::{
    self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEventKind,
};
use ratatui_core::{backend::Backend, terminal::Terminal};
use ratatui_widgets::table::TableState;
use sysinfo::Signal;

use crate::{
    MAX_INTERVAL_MS, MIN_INTERVAL_MS,
    error::Result,
    filter::Filter,
    history::History,
    inspect::{self, FileEntry, SocketEntry},
    sampler::{
        ProcessRow, ProcessSample, Sampler, Snapshot, apply_process_trends, collect_process_samples,
    },
    ui,
};

const NOTICE_TTL: Duration = Duration::from_secs(4);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Cpu,
    Memory,
    Energy,
    Disk,
    Network,
    Movers,
}

impl Tab {
    pub const ALL: [Self; 6] = [
        Self::Cpu,
        Self::Memory,
        Self::Energy,
        Self::Disk,
        Self::Network,
        Self::Movers,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Self::Cpu => "CPU",
            Self::Memory => "Memory",
            Self::Energy => "Energy",
            Self::Disk => "Disk",
            Self::Network => "Network",
            Self::Movers => "Movers",
        }
    }

    fn default_sort(self) -> SortKey {
        match self {
            Self::Cpu => SortKey::Cpu,
            Self::Memory => SortKey::Memory,
            Self::Energy => SortKey::Energy,
            Self::Disk => SortKey::DiskWrite,
            Self::Network => SortKey::Name,
            Self::Movers => SortKey::Trend,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    Cpu,
    Memory,
    Energy,
    DiskRead,
    DiskWrite,
    Trend,
    Name,
    Pid,
    User,
    Runtime,
}

impl SortKey {
    pub fn title(self) -> &'static str {
        match self {
            Self::Cpu => "% CPU",
            Self::Memory => "Memory",
            Self::Energy => "Impact",
            Self::DiskRead => "Read/s",
            Self::DiskWrite => "Write/s",
            Self::Trend => "Change",
            Self::Name => "Name",
            Self::Pid => "PID",
            Self::User => "User",
            Self::Runtime => "Runtime",
        }
    }

    fn default_desc(self) -> bool {
        !matches!(self, Self::Name | Self::Pid | Self::User)
    }
}

#[derive(Debug, Clone)]
pub struct Notice {
    text: String,
    created_at: Instant,
}

impl Notice {
    fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            created_at: Instant::now(),
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    fn expired(&self) -> bool {
        self.remaining().is_zero()
    }

    fn remaining(&self) -> Duration {
        NOTICE_TTL.saturating_sub(self.created_at.elapsed())
    }
}

/// A snapshot of one process's open files and sockets, captured on demand for
/// the handles overlay. Carries an optional error so permission failures show
/// in the panel rather than disappearing.
#[derive(Debug, Clone)]
pub struct HandlesView {
    pub pid: u32,
    pub name: String,
    pub files: Vec<FileEntry>,
    pub sockets: Vec<SocketEntry>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessIntent {
    Term,
    Kill,
    Stop,
    Continue,
    NiceLower,
    NiceHigher,
}

impl ProcessIntent {
    fn signal(self) -> Signal {
        match self {
            Self::Term => Signal::Term,
            Self::Kill => Signal::Kill,
            Self::Stop => Signal::Stop,
            Self::Continue => Signal::Continue,
            Self::NiceLower | Self::NiceHigher => unreachable!("renice actions are not signals"),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Term => "TERM",
            Self::Kill => "KILL",
            Self::Stop => "STOP",
            Self::Continue => "CONT",
            Self::NiceLower => "nice +5",
            Self::NiceHigher => "nice -5",
        }
    }

    fn cancel_label(self) -> &'static str {
        match self {
            Self::NiceLower | Self::NiceHigher => "priority change cancelled",
            _ => "process signal cancelled",
        }
    }

    fn apply(self, sampler: &Sampler, pid: u32) -> Result<String> {
        match self {
            Self::Term | Self::Kill | Self::Stop | Self::Continue => {
                sampler.send_signal(pid, self.signal())?;
                Ok(format!("sent {} to pid {}", self.label(), pid))
            }
            Self::NiceLower => {
                let priority = sampler.adjust_priority(pid, 5)?;
                Ok(format!("set pid {pid} priority to {priority}"))
            }
            Self::NiceHigher => {
                let priority = sampler.adjust_priority(pid, -5)?;
                Ok(format!("set pid {pid} priority to {priority}"))
            }
        }
    }
}

pub struct App {
    sampler: Sampler,
    snapshot: Snapshot,
    previous_samples: HashMap<u32, ProcessSample>,
    history: History,
    pub table_state: TableState,
    pub visible: Vec<usize>,
    pub tab: Tab,
    pub sort_key: SortKey,
    pub sort_desc: bool,
    pub filter: String,
    pub filter_mode: bool,
    pub show_details: bool,
    pub show_help: bool,
    pub notice: Option<Notice>,
    pub confirm: Option<ProcessIntent>,
    pub handles: Option<HandlesView>,
    interval: Duration,
    last_refresh: Instant,
    should_quit: bool,
}

impl App {
    pub fn new(interval: Duration, initial_filter: Option<String>) -> Result<Self> {
        let mut sampler = Sampler::new()?;
        let snapshot = sampler.sample(None);
        let previous_samples = collect_process_samples(&snapshot.processes);
        let mut history = History::default();
        history.record(&snapshot);
        let mut app = Self {
            sampler,
            snapshot,
            previous_samples,
            history,
            table_state: TableState::default(),
            visible: Vec::new(),
            tab: Tab::Cpu,
            sort_key: SortKey::Cpu,
            sort_desc: true,
            filter: initial_filter.unwrap_or_default(),
            filter_mode: false,
            show_details: true,
            show_help: false,
            notice: None,
            confirm: None,
            handles: None,
            interval,
            last_refresh: Instant::now(),
            should_quit: false,
        };
        app.rebuild_view(None);
        Ok(app)
    }

    pub fn run<B>(&mut self, terminal: &mut Terminal<B>) -> Result<()>
    where
        B: Backend,
        B::Error: std::error::Error + Send + Sync + 'static,
    {
        let mut needs_draw = true;
        while !self.should_quit {
            if self.clear_expired_notice() {
                needs_draw = true;
            }

            if needs_draw {
                terminal.draw(|frame| ui::draw(frame, self))?;
                needs_draw = false;
            }

            let timeout = self.next_poll_timeout();
            if event::poll(timeout)? {
                needs_draw |= self.handle_event(event::read()?)?;
            }

            if self.last_refresh.elapsed() >= self.interval {
                self.refresh();
                needs_draw = true;
            }
        }
        Ok(())
    }

    pub fn snapshot(&self) -> &Snapshot {
        &self.snapshot
    }

    pub fn history(&self) -> &History {
        &self.history
    }

    pub fn handles_view(&self) -> Option<&HandlesView> {
        self.handles.as_ref()
    }

    pub fn selected_process(&self) -> Option<&ProcessRow> {
        self.table_state
            .selected()
            .and_then(|selected| self.visible.get(selected))
            .and_then(|index| self.snapshot.processes.get(*index))
    }

    pub fn selected_pid(&self) -> Option<u32> {
        self.selected_process().map(|process| process.pid)
    }

    pub fn interval(&self) -> Duration {
        self.interval
    }

    pub fn visible_count(&self) -> usize {
        self.visible.len()
    }

    pub fn process_count(&self) -> usize {
        self.snapshot.process_count
    }

    pub fn selected_position(&self) -> Option<usize> {
        self.table_state
            .selected()
            .filter(|selected| *selected < self.visible.len())
            .map(|selected| selected + 1)
    }

    fn handle_event(&mut self, event: Event) -> Result<bool> {
        match event {
            Event::Key(key) => {
                if key.kind != KeyEventKind::Press {
                    return Ok(false);
                }
                if is_ctrl_c(key) {
                    self.should_quit = true;
                    self.confirm = None;
                    self.filter_mode = false;
                    self.show_help = false;
                    self.handles = None;
                    return Ok(true);
                }

                if self.show_help {
                    return Ok(self.handle_help_key(key));
                }
                if self.handles.is_some() {
                    return Ok(self.handle_handles_key(key));
                }
                if let Some(intent) = self.confirm {
                    return self.handle_confirm_key(key, intent);
                }
                if self.filter_mode {
                    return Ok(self.handle_filter_key(key));
                }

                let changed = match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => {
                        self.should_quit = true;
                        true
                    }
                    KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        self.clear_filter()
                    }
                    KeyCode::Char('?') => {
                        self.show_help = true;
                        true
                    }
                    KeyCode::Char('/') => {
                        self.filter_mode = true;
                        true
                    }
                    KeyCode::Char('i') | KeyCode::Enter => {
                        self.show_details = !self.show_details;
                        true
                    }
                    KeyCode::Char('o') => self.toggle_handles(),
                    KeyCode::Char('r') => {
                        self.refresh();
                        true
                    }
                    KeyCode::Char('s') => {
                        self.cycle_sort();
                        true
                    }
                    KeyCode::Char('S') => {
                        self.sort_desc = !self.sort_desc;
                        self.rebuild_view(self.selected_pid());
                        true
                    }
                    KeyCode::Char('c') => {
                        self.set_sort(SortKey::Cpu, true);
                        true
                    }
                    KeyCode::Char('m') => {
                        self.set_sort(SortKey::Memory, true);
                        true
                    }
                    KeyCode::Char('e') => {
                        self.set_sort(SortKey::Energy, true);
                        true
                    }
                    KeyCode::Char('d') => {
                        self.set_sort(SortKey::DiskWrite, true);
                        true
                    }
                    KeyCode::Char('D') => {
                        self.set_sort(SortKey::DiskRead, true);
                        true
                    }
                    KeyCode::Char('n') => {
                        self.set_sort(SortKey::Name, false);
                        true
                    }
                    KeyCode::Char('p') => {
                        self.set_sort(SortKey::Pid, false);
                        true
                    }
                    KeyCode::Char('T') => {
                        self.set_sort(SortKey::Runtime, true);
                        true
                    }
                    KeyCode::Char('u') => {
                        self.set_sort(SortKey::User, false);
                        true
                    }
                    KeyCode::Char('t') => self.begin_action(ProcessIntent::Term),
                    KeyCode::Char('f') => self.begin_action(ProcessIntent::Kill),
                    KeyCode::Char('z') => self.begin_action(ProcessIntent::Stop),
                    KeyCode::Char('g') => self.begin_action(ProcessIntent::Continue),
                    KeyCode::Char('[') => self.begin_action(ProcessIntent::NiceLower),
                    KeyCode::Char(']') => self.begin_action(ProcessIntent::NiceHigher),
                    KeyCode::Char('+') | KeyCode::Char('=') => {
                        self.adjust_interval(false);
                        true
                    }
                    KeyCode::Char('-') => {
                        self.adjust_interval(true);
                        true
                    }
                    KeyCode::Char('1') => self.set_tab(Tab::Cpu),
                    KeyCode::Char('2') => self.set_tab(Tab::Memory),
                    KeyCode::Char('3') => self.set_tab(Tab::Energy),
                    KeyCode::Char('4') => self.set_tab(Tab::Disk),
                    KeyCode::Char('5') => self.set_tab(Tab::Network),
                    KeyCode::Char('6') => self.set_tab(Tab::Movers),
                    KeyCode::Tab => self.next_tab(),
                    KeyCode::BackTab => self.previous_tab(),
                    KeyCode::Down | KeyCode::Char('j') => self.select_next(1),
                    KeyCode::Up | KeyCode::Char('k') => self.select_previous(1),
                    KeyCode::PageDown => self.select_next(10),
                    KeyCode::PageUp => self.select_previous(10),
                    KeyCode::Home => self.select_first(),
                    KeyCode::End => self.select_last(),
                    _ => false,
                };
                Ok(changed)
            }
            Event::Mouse(mouse) => {
                if self.show_help
                    || self.handles.is_some()
                    || self.confirm.is_some()
                    || self.filter_mode
                {
                    return Ok(false);
                }
                let changed = match mouse.kind {
                    MouseEventKind::ScrollUp => self.select_previous(3),
                    MouseEventKind::ScrollDown => self.select_next(3),
                    _ => false,
                };
                Ok(changed)
            }
            _ => Ok(false),
        }
    }

    fn handle_help_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Esc | KeyCode::Enter | KeyCode::Char('?') | KeyCode::Char('q') => {
                self.show_help = false;
                true
            }
            _ => false,
        }
    }

    fn handle_handles_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Esc | KeyCode::Enter | KeyCode::Char('o') | KeyCode::Char('q') => {
                self.handles = None;
                true
            }
            _ => false,
        }
    }

    /// Toggle the open files/sockets overlay for the selected process. Opening
    /// runs `lsof` once for the current selection; the result (or a permission
    /// error) is captured so it stays stable while the panel is open.
    fn toggle_handles(&mut self) -> bool {
        if self.handles.is_some() {
            self.handles = None;
            return true;
        }
        let Some(process) = self.selected_process() else {
            return false;
        };
        let pid = process.pid;
        let name = process.name.clone();
        let view = match inspect::collect_handles(pid) {
            Ok(handles) => HandlesView {
                pid,
                name,
                files: handles.files,
                sockets: handles.sockets,
                error: None,
            },
            Err(error) => HandlesView {
                pid,
                name,
                files: Vec::new(),
                sockets: Vec::new(),
                error: Some(error.to_string()),
            },
        };
        self.handles = Some(view);
        true
    }

    fn handle_confirm_key(&mut self, key: KeyEvent, intent: ProcessIntent) -> Result<bool> {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                if let Some(pid) = self.selected_pid() {
                    match intent.apply(&self.sampler, pid) {
                        Ok(message) => {
                            self.notice = Some(Notice::new(message));
                            self.refresh();
                        }
                        Err(error) => self.notice = Some(Notice::new(error.to_string())),
                    }
                }
                self.confirm = None;
                Ok(true)
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                self.confirm = None;
                self.notice = Some(Notice::new(intent.cancel_label()));
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    fn handle_filter_key(&mut self, key: KeyEvent) -> bool {
        let previous_pid = self.selected_pid();
        match key.code {
            KeyCode::Esc | KeyCode::Enter => {
                self.filter_mode = false;
                true
            }
            KeyCode::Backspace => {
                self.filter.pop();
                self.refilter_view(previous_pid);
                true
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.filter.clear();
                self.refilter_view(previous_pid);
                true
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.filter.push(c);
                self.refilter_view(previous_pid);
                true
            }
            _ => false,
        }
    }

    fn refresh(&mut self) {
        let selected_pid = self.selected_pid();
        self.snapshot = self.sampler.sample(selected_pid);
        apply_process_trends(&mut self.snapshot.processes, &self.previous_samples);
        self.previous_samples = collect_process_samples(&self.snapshot.processes);
        self.history.record(&self.snapshot);
        self.last_refresh = Instant::now();
        self.rebuild_view(selected_pid);
    }

    fn clear_filter(&mut self) -> bool {
        if self.filter.is_empty() {
            return false;
        }
        self.filter.clear();
        self.refilter_view(self.selected_pid());
        self.notice = Some(Notice::new("filter cleared"));
        true
    }

    fn rebuild_view(&mut self, selected_pid: Option<u32>) {
        self.sort_processes();
        self.refilter_view(selected_pid);
    }

    fn refilter_view(&mut self, selected_pid: Option<u32>) {
        let filter = Filter::parse(self.filter.trim());
        self.visible = self
            .snapshot
            .processes
            .iter()
            .enumerate()
            .filter_map(|(index, process)| filter.matches(process).then_some(index))
            .collect();

        if self.visible.is_empty() {
            self.table_state.select(None);
            return;
        }

        let selected = selected_pid
            .and_then(|pid| {
                self.visible
                    .iter()
                    .position(|index| self.snapshot.processes[*index].pid == pid)
            })
            .or_else(|| self.table_state.selected())
            .unwrap_or(0)
            .min(self.visible.len() - 1);
        self.table_state.select(Some(selected));
        self.hydrate_selected_details();
    }

    fn sort_processes(&mut self) {
        let sort_key = self.sort_key;
        let sort_desc = self.sort_desc;
        self.snapshot.processes.sort_by(|left, right| {
            let ordering = match sort_key {
                SortKey::Cpu => left.cpu_usage.total_cmp(&right.cpu_usage),
                SortKey::Memory => left.memory.cmp(&right.memory),
                SortKey::Energy => left.energy_impact.total_cmp(&right.energy_impact),
                SortKey::DiskRead => left.disk_read_rate.total_cmp(&right.disk_read_rate),
                SortKey::DiskWrite => left.disk_write_rate.total_cmp(&right.disk_write_rate),
                SortKey::Trend => left.trend.score().total_cmp(&right.trend.score()),
                SortKey::Name => left.sort_name.cmp(&right.sort_name),
                SortKey::Pid => left.pid.cmp(&right.pid),
                SortKey::User => left.user.cmp(&right.user),
                SortKey::Runtime => left.run_time.cmp(&right.run_time),
            };
            if sort_desc {
                ordering.reverse()
            } else {
                ordering
            }
        });
    }

    fn set_tab(&mut self, tab: Tab) -> bool {
        if self.tab == tab {
            return false;
        }
        self.tab = tab;
        self.sort_key = tab.default_sort();
        self.sort_desc = self.sort_key.default_desc();
        self.rebuild_view(self.selected_pid());
        true
    }

    fn next_tab(&mut self) -> bool {
        let index = Tab::ALL
            .iter()
            .position(|tab| *tab == self.tab)
            .unwrap_or(0);
        self.set_tab(Tab::ALL[(index + 1) % Tab::ALL.len()])
    }

    fn previous_tab(&mut self) -> bool {
        let index = Tab::ALL
            .iter()
            .position(|tab| *tab == self.tab)
            .unwrap_or(0);
        self.set_tab(Tab::ALL[(index + Tab::ALL.len() - 1) % Tab::ALL.len()])
    }

    fn set_sort(&mut self, sort_key: SortKey, descending: bool) {
        if self.sort_key == sort_key {
            self.sort_desc = !self.sort_desc;
        } else {
            self.sort_key = sort_key;
            self.sort_desc = descending;
        }
        self.rebuild_view(self.selected_pid());
    }

    fn cycle_sort(&mut self) {
        const ORDER: [SortKey; 10] = [
            SortKey::Cpu,
            SortKey::Memory,
            SortKey::Energy,
            SortKey::DiskWrite,
            SortKey::DiskRead,
            SortKey::Trend,
            SortKey::Name,
            SortKey::Pid,
            SortKey::User,
            SortKey::Runtime,
        ];
        let index = ORDER
            .iter()
            .position(|key| *key == self.sort_key)
            .unwrap_or(0);
        self.sort_key = ORDER[(index + 1) % ORDER.len()];
        self.sort_desc = self.sort_key.default_desc();
        self.rebuild_view(self.selected_pid());
    }

    fn begin_action(&mut self, intent: ProcessIntent) -> bool {
        if self.selected_pid().is_some() {
            self.confirm = Some(intent);
            true
        } else {
            false
        }
    }

    fn adjust_interval(&mut self, faster: bool) {
        let millis = self.interval.as_millis() as u64;
        let next = if faster {
            millis.saturating_sub(250).max(MIN_INTERVAL_MS)
        } else {
            (millis + 250).min(MAX_INTERVAL_MS)
        };
        self.interval = Duration::from_millis(next);
        self.notice = Some(Notice::new(format!("refresh interval: {next} ms")));
    }

    fn select_next(&mut self, amount: usize) -> bool {
        if self.visible.is_empty() {
            return false;
        }
        let previous = self.table_state.selected();
        let selected = previous.unwrap_or(0);
        let next = (selected + amount).min(self.visible.len() - 1);
        self.table_state.select(Some(next));
        self.hydrate_selected_details();
        previous != Some(next)
    }

    fn select_previous(&mut self, amount: usize) -> bool {
        if self.visible.is_empty() {
            return false;
        }
        let selected = self.table_state.selected().unwrap_or(0);
        let next = selected.saturating_sub(amount);
        self.table_state.select(Some(next));
        self.hydrate_selected_details();
        next != selected
    }

    fn select_first(&mut self) -> bool {
        if !self.visible.is_empty() {
            let changed = self.table_state.selected() != Some(0);
            self.table_state.select(Some(0));
            self.hydrate_selected_details();
            changed
        } else {
            false
        }
    }

    fn select_last(&mut self) -> bool {
        if !self.visible.is_empty() {
            let last = self.visible.len() - 1;
            let changed = self.table_state.selected() != Some(last);
            self.table_state.select(Some(last));
            self.hydrate_selected_details();
            changed
        } else {
            false
        }
    }

    fn hydrate_selected_details(&mut self) {
        let Some(selected) = self.table_state.selected() else {
            return;
        };
        let Some(index) = self.visible.get(selected).copied() else {
            return;
        };
        if self.snapshot.processes[index].selected_details.is_some() {
            return;
        }
        let pid = self.snapshot.processes[index].pid;
        self.snapshot.processes[index].selected_details =
            self.sampler.selected_process_details(pid);
    }

    fn next_poll_timeout(&self) -> Duration {
        let refresh = self.interval.saturating_sub(self.last_refresh.elapsed());
        self.notice
            .as_ref()
            .map(Notice::remaining)
            .map_or(refresh, |notice| refresh.min(notice))
    }

    fn clear_expired_notice(&mut self) -> bool {
        if self.notice.as_ref().is_some_and(Notice::expired) {
            self.notice = None;
            true
        } else {
            false
        }
    }
}

fn is_ctrl_c(key: KeyEvent) -> bool {
    matches!(key.code, KeyCode::Char('c') | KeyCode::Char('C'))
        && key.modifiers.contains(KeyModifiers::CONTROL)
}

#[cfg(test)]
mod tests {
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

    use crate::sampler::ProcessRow;

    use super::{App, HandlesView, ProcessIntent, SortKey, Tab};

    fn open_handles() -> HandlesView {
        HandlesView {
            pid: 1,
            name: "demo".into(),
            files: Vec::new(),
            sockets: Vec::new(),
            error: None,
        }
    }

    #[test]
    fn ctrl_c_quits_from_filter_mode() {
        let mut app = App::new(std::time::Duration::from_millis(1_000), None).unwrap();
        app.filter_mode = true;

        assert!(
            app.handle_event(Event::Key(KeyEvent::new(
                KeyCode::Char('c'),
                KeyModifiers::CONTROL,
            )))
            .unwrap()
        );
        assert!(app.should_quit);
        assert!(!app.filter_mode);
    }

    #[test]
    fn uppercase_sort_shortcuts_select_hidden_sort_keys() {
        let mut app = App::new(std::time::Duration::from_millis(1_000), None).unwrap();

        assert!(
            app.handle_event(Event::Key(KeyEvent::new(
                KeyCode::Char('D'),
                KeyModifiers::SHIFT,
            )))
            .unwrap()
        );
        assert_eq!(app.sort_key, SortKey::DiskRead);
        assert!(app.sort_desc);

        assert!(
            app.handle_event(Event::Key(KeyEvent::new(
                KeyCode::Char('T'),
                KeyModifiers::SHIFT,
            )))
            .unwrap()
        );
        assert_eq!(app.sort_key, SortKey::Runtime);
        assert!(app.sort_desc);
    }

    #[test]
    fn tab_switch_uses_default_sort_direction() {
        let mut app = App::new(std::time::Duration::from_millis(1_000), None).unwrap();

        assert!(app.set_tab(Tab::Memory));
        assert_eq!(app.sort_key, SortKey::Memory);
        assert!(app.sort_desc);

        assert!(app.set_tab(Tab::Network));
        assert_eq!(app.sort_key, SortKey::Name);
        assert!(!app.sort_desc);

        assert!(app.set_tab(Tab::Movers));
        assert_eq!(app.sort_key, SortKey::Trend);
        assert!(app.sort_desc);
    }

    #[test]
    fn power_action_shortcuts_prompt_for_confirmation() {
        let mut app = App::new(std::time::Duration::from_millis(1_000), None).unwrap();
        if app.selected_pid().is_none() {
            return;
        }

        assert!(
            app.handle_event(Event::Key(KeyEvent::new(
                KeyCode::Char('z'),
                KeyModifiers::NONE,
            )))
            .unwrap()
        );
        assert_eq!(app.confirm, Some(ProcessIntent::Stop));

        app.confirm = None;
        assert!(
            app.handle_event(Event::Key(KeyEvent::new(
                KeyCode::Char('g'),
                KeyModifiers::NONE,
            )))
            .unwrap()
        );
        assert_eq!(app.confirm, Some(ProcessIntent::Continue));

        app.confirm = None;
        assert!(
            app.handle_event(Event::Key(KeyEvent::new(
                KeyCode::Char('['),
                KeyModifiers::NONE,
            )))
            .unwrap()
        );
        assert_eq!(app.confirm, Some(ProcessIntent::NiceLower));

        app.confirm = None;
        assert!(
            app.handle_event(Event::Key(KeyEvent::new(
                KeyCode::Char(']'),
                KeyModifiers::NONE,
            )))
            .unwrap()
        );
        assert_eq!(app.confirm, Some(ProcessIntent::NiceHigher));
    }

    #[test]
    fn handles_overlay_captures_keys_and_toggles_closed() {
        let mut app = App::new(std::time::Duration::from_millis(1_000), None).unwrap();
        app.handles = Some(open_handles());
        let starting_tab = app.tab;

        // While the overlay is open, unrelated keys are swallowed, not acted on.
        assert!(
            !app.handle_event(Event::Key(KeyEvent::new(
                KeyCode::Char('1'),
                KeyModifiers::NONE,
            )))
            .unwrap()
        );
        assert_eq!(app.tab, starting_tab);
        assert!(app.handles.is_some());

        // `o` again closes it.
        assert!(
            app.handle_event(Event::Key(KeyEvent::new(
                KeyCode::Char('o'),
                KeyModifiers::NONE,
            )))
            .unwrap()
        );
        assert!(app.handles.is_none());
    }

    #[test]
    fn selection_hydrates_details_without_waiting_for_refresh() {
        let mut app = App::new(std::time::Duration::from_millis(1_000), None).unwrap();
        if app.visible.len() < 2 {
            return;
        }

        let second_index = app.visible[1];
        app.snapshot.processes[second_index].selected_details = None;

        assert!(app.select_next(1));
        assert!(
            app.selected_process()
                .and_then(|process| process.selected_details.as_ref())
                .is_some()
        );
    }

    #[test]
    fn ctrl_u_clears_active_filter_outside_filter_mode() {
        let mut app = App::new(
            std::time::Duration::from_millis(1_000),
            Some("codex".into()),
        )
        .unwrap();
        app.filter_mode = false;

        assert!(
            app.handle_event(Event::Key(KeyEvent::new(
                KeyCode::Char('u'),
                KeyModifiers::CONTROL,
            )))
            .unwrap()
        );
        assert!(app.filter.is_empty());
        assert_eq!(
            app.notice.as_ref().map(|notice| notice.text()),
            Some("filter cleared")
        );
    }

    #[test]
    fn refilter_preserves_existing_process_order() {
        let mut app = App::new(std::time::Duration::from_millis(1_000), None).unwrap();
        let Some(template) = app.snapshot.processes.first().cloned() else {
            return;
        };

        app.sort_key = SortKey::Name;
        app.sort_desc = false;
        app.snapshot.processes = vec![
            fake_process(&template, 1, "zeta"),
            fake_process(&template, 2, "alpha"),
            fake_process(&template, 3, "beta"),
        ];
        app.filter = "a".into();

        app.refilter_view(None);

        let ordered_pids = app
            .visible
            .iter()
            .map(|index| app.snapshot.processes[*index].pid)
            .collect::<Vec<_>>();
        assert_eq!(ordered_pids, vec![1, 2, 3]);
    }

    fn fake_process(template: &ProcessRow, pid: u32, name: &str) -> ProcessRow {
        let mut process = template.clone();
        process.pid = pid;
        process.name = name.to_string();
        process.sort_name = name.to_lowercase();
        process.search_text = format!("{pid} {name} user command running");
        process
    }
}
