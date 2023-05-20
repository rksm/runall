//! # runall
//!
//! Install: `cargo install runall`
//!
//! ## Usage
//!
//!```shell
//! $ runall --help
//! Run multiple commands in parallel.
//!
//! Usage: runall [OPTIONS] [COMMANDS]...
//!
//! Arguments:
//!   [COMMANDS]...
//!
//! Options:
//!   -n, --names <NAMES>
//!   -h, --help           Print help
//!```

use clap::Parser;
use std::{
    io::{BufRead, BufReader, Read},
    process,
};

#[derive(Parser)]
#[clap(about = "Run multiple commands in parallel.")]
pub struct Args {
    #[clap(short, long)]
    pub names: Option<Vec<String>>,

    #[clap()]
    pub commands: Vec<String>,
}

struct Process {
    pid: u32,
    proc: process::Child,
    prefix: String,
    stop_tx: flume::Sender<()>,
}

impl Process {
    pub fn spawn(name: impl ToString, prefix: impl ToString, cmd: &str) -> Self {
        let bin = "bash";
        let args = vec!["-c", cmd];
        let prefix = prefix.to_string();
        let name = name.to_string();

        eprintln!("starting {cmd} as {name}");

        let mut proc = process::Command::new(bin)
            .args(args)
            .stdout(process::Stdio::piped())
            .stderr(process::Stdio::piped())
            .spawn()
            .expect("start process");

        fn fwd_stream(prefix: impl ToString, stream: Option<impl Read + Send + 'static>) {
            let prefix = prefix.to_string();
            if let Some(stream) = stream {
                std::thread::spawn(move || {
                    let mut reader = BufReader::new(stream);
                    let mut line = String::new();
                    loop {
                        match reader.read_line(&mut line) {
                            Err(err) => {
                                eprintln!("error reading line: {err}");
                            }
                            Ok(0) => {
                                break;
                            }
                            Ok(_) => {
                                print!("{prefix} {line}");
                                line.clear();
                            }
                        }
                    }
                });
            }
        }

        fwd_stream(&prefix, proc.stdout.take());
        fwd_stream(&prefix, proc.stderr.take());

        let (stop_tx, stop_rx) = flume::bounded(1);
        let pid = proc.id();
        let prefix2 = prefix.clone();
        std::thread::spawn(move || {
            stop_rx.recv().expect("stop signal");
            eprintln!("{prefix2} sending sigterm to {pid}");
            sigterm(pid);
        });

        Self {
            proc,
            pid,
            prefix,
            stop_tx,
        }
    }

    #[allow(dead_code)]
    pub fn sigterm(&self) {
        eprintln!("{} sending sigterm to {}", self.prefix, self.pid);
        sigterm(self.pid);
    }

    pub fn wait(&mut self) {
        self.proc.wait().expect("wait for process");
    }
}

pub fn sigterm(pid: u32) {
    process::Command::new("kill")
        .arg("-SIGTERM")
        .arg(pid.to_string())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("send sigterm")
        .wait()
        .expect("wait for sigterm");
}

pub fn run(args: Args) {
    let names = args.names.clone().unwrap_or_else(|| {
        args.commands
            .iter()
            .enumerate()
            .map(|(i, _cmd)| format!("cmd-{}", i + 1))
            .collect::<Vec<_>>()
    });
    let name_padding = names.iter().map(|n| n.len()).max().unwrap_or(0);
    let prefixes = names
        .iter()
        .map(|name| format!("[{name}]{:width$}", "", width = name_padding - name.len()))
        .collect::<Vec<_>>();

    let procs = args
        .commands
        .iter()
        .zip(&names)
        .zip(&prefixes)
        .map(|((cmd, name), prefix)| Process::spawn(name, prefix, cmd))
        .collect::<Vec<_>>();

    let stop_senders = procs.iter().map(|p| p.stop_tx.clone()).collect::<Vec<_>>();

    ctrlc::set_handler(move || {
        eprintln!("got ctrl-c");

        for stop_tx in &stop_senders {
            if let Err(err) = stop_tx.try_send(()) {
                eprintln!("error sending stop signal: {err}");
            }
        }
    })
    .expect("set ctrl-c handler");

    for mut proc in procs {
        proc.wait();
    }
}

fn fixup_names(names: &mut Vec<String>, cmd_count: usize) {
    if names.len() == cmd_count {
        return;
    }

    if names.len() == 1 {
        *names = names[0]
            .split(',')
            .map(|s| s.to_string())
            .collect::<Vec<_>>();
    }
    if names.len() == cmd_count {
        return;
    }

    panic!("expected {} names, got {}", cmd_count, names.len());
}

fn main() {
    let mut args = Args::parse();
    if let Some(names) = &mut args.names {
        fixup_names(names, args.commands.len());
    }
    run(args);
}
