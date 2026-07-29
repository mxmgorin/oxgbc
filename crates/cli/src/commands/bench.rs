//! `oxgbc-cli bench` — fixed-work macro benchmarks.
//!
//! `bench <ROM> [--frames N] [--repeat K]` runs N frames headless from reset
//! and reports wall vs emulated time; `--repeat` runs K times after a warm-up
//! and reports median/min/max. `m_cycles` is the determinism check: for a given
//! ROM + model + frame count it must not vary between runs or binaries.
//!
//! `bench matrix [--repeat K]` runs a fixed workload set (in-repo ROMs + games
//! from OXGBC_BENCH_GB_ROM / OXGBC_BENCH_GBC_ROM) and reports per-workload
//! medians — the single-binary counterpart to scripts/bench-ab.sh.
//!
//! The frame loop mirrors the app's (step + APU drain) minus video/input.

use crate::args::{next_val, parse_args, print_common_usage, CommonOpts};
use core::auxiliary::clock::T_CYCLES_PER_M_CYCLE;
use core::cpu::{Cpu, CPU_CLOCK_SPEED};
use core::harness;
use core::ppu::{LINES_PER_FRAME, TICKS_PER_LINE};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

/// Dots per frame (154 lines × 456 dots), fixed by the PPU.
const DOTS_PER_FRAME: usize = LINES_PER_FRAME * TICKS_PER_LINE;
/// Emulated seconds per frame ≈ 16.74 ms, fixed regardless of CPU speed.
const FRAME_SECONDS: f64 = DOTS_PER_FRAME as f64 / CPU_CLOCK_SPEED as f64;
/// M-cycles per frame at single speed.
const M_CYCLES_PER_FRAME: usize = DOTS_PER_FRAME / T_CYCLES_PER_M_CYCLE;
const DEFAULT_FRAMES: usize = 600;
const DEFAULT_MATRIX_REPEAT: usize = 5;

struct BenchOpts {
    rom: PathBuf,
    common: CommonOpts,
    frames: usize,
    repeat: usize,
    json: bool,
}

pub fn cmd_bench(args: &[String]) -> Result<ExitCode, String> {
    if args.first().map(String::as_str) == Some("matrix") {
        return cmd_matrix(&args[1..]);
    }

    let Some(opts) = parse(args)? else {
        print_usage();
        return Ok(ExitCode::SUCCESS);
    };

    let pristine = harness::build_cpu_from_path(&opts.rom, opts.common.model)?;

    // A warm-up run (discarded) absorbs cold-cache/CPU-clock ramp before timing.
    if opts.repeat > 1 {
        run_frames(&mut pristine.clone(), opts.frames)?;
    }

    let mut secs = Vec::with_capacity(opts.repeat);
    let mut m_cycles = 0;
    for i in 0..opts.repeat {
        let (s, m) = run_frames(&mut pristine.clone(), opts.frames)?;
        // Each run is from reset, so m_cycles must be identical; a mismatch means
        // the emulation is non-deterministic and the numbers are meaningless.
        if i > 0 && m != m_cycles {
            return Err(format!("non-deterministic m_cycles: run {i} gave {m}, expected {m_cycles}"));
        }
        m_cycles = m;
        secs.push(s);
    }

    let med = median(&secs);
    let lo = secs.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = secs.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let emulated_s = opts.frames as f64 * FRAME_SECONDS;
    let speed = emulated_s / med;
    let rom_name = opts.rom.display();

    if opts.json {
        println!(
            "{{\"rom\":\"{}\",\"frames\":{},\"repeat\":{},\"wall_ms\":{:.2},\"wall_min_ms\":{:.2},\"wall_max_ms\":{:.2},\"emulated_s\":{:.3},\"speed_x\":{:.2},\"m_cycles\":{}}}",
            rom_name.to_string().replace('"', "\\\""),
            opts.frames,
            opts.repeat,
            med * 1000.0,
            lo * 1000.0,
            hi * 1000.0,
            emulated_s,
            speed,
            m_cycles
        );
    } else if opts.repeat > 1 {
        println!(
            "BENCH   {rom_name}  frames={}  wall={med:.3}s (min {lo:.3} max {hi:.3}, n={})  emulated={emulated_s:.2}s  speed={speed:.1}x  m_cycles={m_cycles}",
            opts.frames, opts.repeat,
        );
    } else {
        println!(
            "BENCH   {rom_name}  frames={}  wall={med:.3}s  emulated={emulated_s:.2}s  speed={speed:.1}x  m_cycles={m_cycles}",
            opts.frames,
        );
    }

    Ok(ExitCode::SUCCESS)
}

/// Run `frames` frames from the cpu's current state and return
/// `(wall_seconds, cumulative_m_cycles)`.
fn run_frames(cpu: &mut Cpu, frames: usize) -> Result<(f64, usize), String> {
    let target_frame = cpu.clock.bus.io.ppu.current_frame + frames;
    // A ROM that leaves the LCD off stops producing frames; bound the run by
    // emulated work so it can't hang. 16× covers double speed + LCD-off gaps.
    let cap_m_cycles = cpu.clock.get_m_cycles() + (frames + 2) * M_CYCLES_PER_FRAME * 16;
    let start = Instant::now();

    while cpu.clock.bus.io.ppu.current_frame < target_frame {
        cpu.step();

        // Mirror EmuRuntime::step's APU drain; skipping it lets the buffer
        // wrap and drifts measured work from what the app pays.
        let apu = &mut cpu.clock.bus.io.apu;
        if apu.buffer_ready() {
            apu.clear_buffer();
        }

        if cpu.clock.get_m_cycles() > cap_m_cycles {
            return Err(format!(
                "no frame progress after {} M-cycles ({}/{} frames) — LCD off?",
                cpu.clock.get_m_cycles(),
                cpu.clock.bus.io.ppu.current_frame,
                target_frame
            ));
        }
    }

    Ok((start.elapsed().as_secs_f64(), cpu.clock.get_m_cycles()))
}

/// Median of `xs` (non-empty); the caller guarantees at least one sample.
fn median(xs: &[f64]) -> f64 {
    let mut v = xs.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = v.len();
    if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    }
}

/// One `bench matrix` entry: a name plus the fixed work to time.
struct Workload {
    name: &'static str,
    kind: Kind,
}

enum Kind {
    /// N frames from reset (build excluded from timing).
    Frames { rom: PathBuf, frames: usize },
    /// Every ROM under a directory run to pass/fail/timeout (build included).
    Suite { dir: PathBuf, timeout: Duration },
}

impl Workload {
    /// The input path this workload needs, if it is absent (so the matrix can
    /// skip it instead of erroring).
    fn missing(&self) -> Option<&Path> {
        let p = match &self.kind {
            Kind::Frames { rom, .. } => rom.as_path(),
            Kind::Suite { dir, .. } => dir.as_path(),
        };
        (!p.exists()).then_some(p)
    }

    /// Time one run of this workload in seconds.
    fn measure(&self, common: &CommonOpts) -> Result<f64, String> {
        match &self.kind {
            Kind::Frames { rom, frames } => {
                let mut cpu = harness::build_cpu_from_path(rom, common.model)?;
                Ok(run_frames(&mut cpu, *frames)?.0)
            }
            Kind::Suite { dir, timeout } => {
                let mut roms = Vec::new();
                crate::rom::collect_roms(dir, true, &mut roms).map_err(|e| e.to_string())?;
                roms.sort();
                if roms.is_empty() {
                    return Err(format!("no ROMs in {}", dir.display()));
                }
                let start = Instant::now();
                for rom in &roms {
                    let mut cpu = harness::build_cpu_from_path(rom, common.model)?;
                    let _ = harness::run(&mut cpu, common.protocol, *timeout);
                }
                Ok(start.elapsed().as_secs_f64())
            }
        }
    }
}

/// The pinned workload matrix: in-repo ROMs always, real games when the env
/// vars point at them. Mirrors scripts/bench-ab.sh's set.
fn matrix_workloads() -> Vec<Workload> {
    let mut w = vec![
        Workload {
            name: "cpu_instrs-600f",
            kind: Kind::Frames {
                rom: PathBuf::from("roms/cpu_instrs.gb"),
                frames: 600,
            },
        },
        Workload {
            name: "same-suite-apu",
            kind: Kind::Suite {
                dir: PathBuf::from("roms/same-suite/apu"),
                timeout: Duration::from_secs(5),
            },
        },
    ];
    for (var, name) in [
        ("OXGBC_BENCH_GB_ROM", "gb-game-1200f"),
        ("OXGBC_BENCH_GBC_ROM", "gbc-game-1200f"),
    ] {
        if let Ok(p) = std::env::var(var) {
            if !p.is_empty() {
                w.push(Workload {
                    name,
                    kind: Kind::Frames {
                        rom: PathBuf::from(p),
                        frames: 1200,
                    },
                });
            }
        }
    }
    w
}

fn cmd_matrix(args: &[String]) -> Result<ExitCode, String> {
    let mut common = CommonOpts::default();
    let mut repeat = DEFAULT_MATRIX_REPEAT;

    let help = parse_args(args, &mut common, |arg, it| {
        match arg {
            "--repeat" => repeat = parse_repeat(&next_val(it, "--repeat")?)?,
            other if other.starts_with('-') => return Err(format!("unknown flag '{other}'")),
            other => return Err(format!("unexpected argument '{other}'")),
        }
        Ok(())
    })?;
    if help {
        print_matrix_usage();
        return Ok(ExitCode::SUCCESS);
    }

    println!("workload            median    min       max");
    println!("------------------  --------  --------  --------");
    for w in matrix_workloads() {
        if let Some(missing) = w.missing() {
            eprintln!("skip {}: {} not found", w.name, missing.display());
            continue;
        }
        w.measure(&common)?; // warm-up, discarded
        let mut secs = Vec::with_capacity(repeat);
        for _ in 0..repeat {
            secs.push(w.measure(&common)?);
        }
        let lo = secs.iter().copied().fold(f64::INFINITY, f64::min);
        let hi = secs.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        println!(
            "{:<18}  {:>7.3}s  {:>7.3}s  {:>7.3}s",
            w.name,
            median(&secs),
            lo,
            hi
        );
    }

    Ok(ExitCode::SUCCESS)
}

/// Parse `bench <ROM>`'s arguments; `None` means help was requested.
fn parse(args: &[String]) -> Result<Option<BenchOpts>, String> {
    let mut common = CommonOpts::default();
    let mut rom: Option<PathBuf> = None;
    let mut frames = DEFAULT_FRAMES;
    let mut repeat = 1;
    let mut json = false;

    let help = parse_args(args, &mut common, |arg, it| {
        match arg {
            "--frames" => {
                let v = next_val(it, "--frames")?;
                frames = v
                    .parse::<usize>()
                    .map_err(|_| format!("invalid frame count '{v}'"))?;
                if frames == 0 {
                    return Err("frame count must be > 0".to_string());
                }
            }
            "--repeat" => repeat = parse_repeat(&next_val(it, "--repeat")?)?,
            "--json" => json = true,
            other if other.starts_with('-') => return Err(format!("unknown flag '{other}'")),
            other if rom.is_none() => rom = Some(PathBuf::from(other)),
            other => return Err(format!("unexpected argument '{other}'")),
        }
        Ok(())
    })?;
    if help {
        return Ok(None);
    }

    Ok(Some(BenchOpts {
        rom: rom.ok_or("missing <ROM> path")?,
        common,
        frames,
        repeat,
        json,
    }))
}

fn parse_repeat(v: &str) -> Result<usize, String> {
    let n = v
        .parse::<usize>()
        .map_err(|_| format!("invalid repeat count '{v}'"))?;
    if n == 0 {
        return Err("repeat count must be > 0".to_string());
    }
    Ok(n)
}

pub fn print_usage() {
    eprintln!("Run N frames headless from reset and report wall vs emulated time.\n");
    eprintln!("USAGE:");
    eprintln!("  oxgbc-cli bench <ROM> [--frames N] [--repeat K] [--json] [--model ..]");
    eprintln!("  oxgbc-cli bench matrix [--repeat K] [--model ..]\n");
    print_options();
    print_common_usage();
}

fn print_matrix_usage() {
    eprintln!("Run the pinned workload matrix and report per-workload medians.\n");
    eprintln!("USAGE:");
    eprintln!("  oxgbc-cli bench matrix [--repeat K] [--model ..]\n");
    eprintln!("Games are added when OXGBC_BENCH_GB_ROM / OXGBC_BENCH_GBC_ROM point at them.\n");
    print_common_usage();
}

pub fn print_options() {
    eprintln!("bench OPTIONS:");
    eprintln!("  --frames <n>             frames to emulate (default: {DEFAULT_FRAMES})");
    eprintln!("  --repeat <k>             timed runs; reports median/min/max (default: 1)");
    eprintln!("  --json                   machine-readable one-line JSON report");
    eprintln!("  matrix                   subcommand: fixed workload matrix, per-workload medians\n");
}
