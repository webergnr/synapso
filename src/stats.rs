//! Frame timing to a CSV, when `SYNAPSO_STATS` names a file. One row per
//! presented frame: time since start, interval since the last frame, and
//! where the CPU spent it (tree update, layout, render), all in ms.
use std::{fs::File, io::Write, time::Instant};

use freya_winit::plugins::{FreyaPlugin, PluginEvent, PluginHandle};

pub struct Stats {
    out: File,
    start: Instant,
    last_present: Option<Instant>,
    frame_start: Option<Instant>,
    update: (Option<Instant>, f64),
    layout: (Option<Instant>, f64),
    render: (Option<Instant>, f64),
    work: f64,
}

impl Stats {
    pub fn from_env() -> Option<Self> {
        let path = std::env::var_os("SYNAPSO_STATS")?;
        let mut out = File::create(path).ok()?;
        writeln!(
            out,
            "t_ms,interval_ms,update_ms,layout_ms,render_ms,work_ms"
        )
        .ok()?;
        Some(Self {
            out,
            start: Instant::now(),
            last_present: None,
            frame_start: None,
            update: (None, 0.),
            layout: (None, 0.),
            render: (None, 0.),
            work: 0.,
        })
    }
}

fn ms(a: Instant, b: Instant) -> f64 {
    (b - a).as_secs_f64() * 1000.
}

impl FreyaPlugin for Stats {
    fn plugin_id(&self) -> &'static str {
        "synapso-stats"
    }

    fn on_event(&mut self, event: &mut PluginEvent, _: PluginHandle) {
        let now = Instant::now();
        let open = |slot: &mut (Option<Instant>, f64), frame: &mut Option<Instant>| {
            slot.0 = Some(now);
            frame.get_or_insert(now);
        };
        match event {
            PluginEvent::StartedUpdatingTree { .. } => {
                open(&mut self.update, &mut self.frame_start)
            }
            PluginEvent::FinishedUpdatingTree { .. } => {
                if let Some(s) = self.update.0.take() {
                    self.update.1 += ms(s, now);
                }
            }
            PluginEvent::StartedMeasuringLayout { .. } => {
                open(&mut self.layout, &mut self.frame_start)
            }
            PluginEvent::FinishedMeasuringLayout { .. } => {
                if let Some(s) = self.layout.0.take() {
                    self.layout.1 += ms(s, now);
                }
            }
            PluginEvent::BeforeRender { .. } => open(&mut self.render, &mut self.frame_start),
            PluginEvent::AfterRender { .. } => {
                if let Some(s) = self.render.0.take() {
                    self.render.1 += ms(s, now);
                }
            }
            PluginEvent::BeforePresenting { .. } => {
                self.work = self.frame_start.map(|s| ms(s, now)).unwrap_or(0.);
            }
            PluginEvent::AfterPresenting { .. } => {
                let interval = self.last_present.map(|l| ms(l, now)).unwrap_or(0.);
                let _ = writeln!(
                    self.out,
                    "{:.2},{:.3},{:.3},{:.3},{:.3},{:.3}",
                    ms(self.start, now),
                    interval,
                    self.update.1,
                    self.layout.1,
                    self.render.1,
                    self.work
                );
                self.last_present = Some(now);
                self.frame_start = None;
                self.update.1 = 0.;
                self.layout.1 = 0.;
                self.render.1 = 0.;
            }
            _ => {}
        }
    }
}
