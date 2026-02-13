#![warn(clippy::pedantic)]

use chrono::{Local, Timelike};
use sysctl::{Ctl, Sysctl};

use std::{
    fmt,
    io::{self, Write},
    sync::{Arc, Mutex, mpsc},
    thread,
    time::Duration,
};

use swayipc::{Connection, Event, EventType, WindowChange};

fn read_ctl(name: &str) -> Option<i32> {
    let ctl = Ctl::new(name).ok()?;
    let val = ctl.value().ok()?;
    val.as_int().copied()
}

enum Power {
    Charging(u32),
    Discharging(u32),
}

struct State {
    title: String,
    power: Power,
    datetime: String,
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let datetime = &self.datetime;
        let status_icon = "⚗".to_string();
        let title = &self.title;
        //"🝭⚗  ⚡ 🝗"
        write!(
            f,
            r#"[{{"full_text": "{title}", "name": "title", "separator": false, "align": "left", "min_width": 1700}},  {{"full_text": "{status_icon}", "separator": false, "name": "test"}}, {{"full_text": "{datetime}", "name": "datetime", "min_width": 100, "separator": false}}]"#
        )
    }
}

fn get_time() -> String {
    Local::now().format("%a %d %b %H:%M").to_string()
}

fn main() {
    //println!("{:?}", read_ctl("hw.acpi.battery.life"));

    //println!("{:?}", read_ctl("hw.acpi.battery.acline"));
    //println!("{:?}", read_ctl("hw.acpi.battery.time"));
    //println!("{:?}", read_ctl("hw.acpi.battery.state"));

    //println!("{} 🜁", get_time());

    let state = Arc::new(Mutex::new(State {
        title: String::new(),
        power: Power::Discharging(0),
        datetime: get_time(),
    }));

    let (tx, rx) = mpsc::channel::<()>();

    {
        let state = Arc::clone(&state);
        let tx = tx.clone();
        thread::spawn(move || {
            if let Ok(mut conn) = Connection::new()
                && let Ok(tree) = conn.get_tree()
                && let Some(node) = tree.find_focused_as_ref(|_| true)
            {
                let title = node.name.clone().unwrap_or_default();
                state.lock().unwrap().title = title;
                let _ = tx.send(());
            }

            loop {
                let Ok(stream) = Connection::new().and_then(|c| c.subscribe([EventType::Window]))
                else {
                    thread::sleep(Duration::from_secs(2));
                    continue;
                };

                for item in stream {
                    let Ok(ev) = item else { break };

                    if let Event::Window(we) = ev
                        && matches!(we.change, WindowChange::Focus | WindowChange::Title)
                    {
                        let title = we.container.name.clone().unwrap_or_default();
                        state.lock().unwrap().title = title;
                        let _ = tx.send(());
                    }
                }

                thread::sleep(Duration::from_secs(1));
            }
        });
    }

    {
        let state = Arc::clone(&state);
        let tx = tx.clone();

        thread::spawn(move || {
            loop {
                {
                    let mut s = state.lock().unwrap();
                    s.datetime = get_time();
                }

                let _ = tx.send(());

                let now = Local::now();
                thread::sleep(Duration::from_secs(60u64 - now.second() as u64));
            }
        });
    }

    let mut out = io::stdout().lock();

    let _ = writeln!(out, "{{\"version\": 1}}\n[\n[]");

    loop {
        if rx.recv().is_err() {
            // channel disconnected
            break;
        }

        while rx.try_recv().is_ok() {}

        let s = state.lock().unwrap();
        let _ = writeln!(out, ",{s}");
        let _ = out.flush();
    }
}
