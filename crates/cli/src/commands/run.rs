//! `oxgbc-cli run <ROM>` — boot one ROM, report its outcome, optionally dump
//! the serial log and a screenshot.

use crate::args::{next_val, parse_args, parse_dump, parse_vram, print_common_usage, CommonOpts};
use crate::inspect::{dump_memory, dump_ppu, dump_regs, dump_vram, trace};
use crate::report::{print_result_line, RomResult};
use crate::rom::{compare_to_reference, save_screenshot};
use core::apu::apu::SAMPLING_FREQUENCY;
use core::cpu::Cpu;
use core::harness;
use core::ppu::ppu::TARGET_FPS_F;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// How the ROM is run and how its pass/fail is decided. The three non-default
/// modes are mutually exclusive.
enum Mode {
    /// Run under the pass/fail protocol detector (the default).
    Detect,
    /// `--no-detect`: run the full timeout with no detection.
    NoDetect,
    /// `--trace N`: record the last N instructions; a debugging run, never a pass.
    Trace(usize),
    /// `--compare`: run the full timeout, then diff the framebuffer against a
    /// reference PNG.
    Compare { reference: PathBuf, tolerance: u8 },
    /// `--state-trace`: emit an observable-state record per executed
    /// instruction, bounded by an emulated M-cycle budget (not wall clock, so
    /// builds of different speed stay comparable). The diff harness compares
    /// the streams; the first divergence names the instruction and register.
    StateTrace { interval: u64, m_cycles: usize },
}

/// ~19 emulated seconds at single speed; the diff runner overrides per suite.
const DEFAULT_STATE_TRACE_M_CYCLES: usize = 20_000_000;

/// `--state-trace` drains audio at frame flips, so the buffer holds a frame of
/// interleaved stereo samples; the slack factor covers stretches that flip no
/// frame at all (LCD off), where a full buffer would wrap and drop.
const STATE_TRACE_AUDIO_BUFFER: usize =
    (SAMPLING_FREQUENCY as f64 / TARGET_FPS_F) as usize * 2 * STATE_TRACE_AUDIO_SLACK;
const STATE_TRACE_AUDIO_SLACK: usize = 32;

/// Everything `run` accepts, parsed and validated.
struct RunOpts {
    rom: PathBuf,
    common: CommonOpts,
    mode: Mode,
    screenshot: Option<PathBuf>,
    serial: bool,
    regs: bool,
    ppu: bool,
    dumps: Vec<(u16, u16)>,
    vram_dumps: Vec<(u8, u16, u16)>,
}

pub fn cmd_run(args: &[String]) -> Result<ExitCode, String> {
    let Some(opts) = parse(args)? else {
        print_usage();
        return Ok(ExitCode::SUCCESS);
    };

    let mut cpu = harness::build_cpu_from_path(&opts.rom, opts.common.model)?;

    let passed = match &opts.mode {
        Mode::Detect => run_detect(&mut cpu, &opts),
        Mode::NoDetect => run_no_detect(&mut cpu, &opts),
        Mode::Trace(len) => {
            trace(&mut cpu, opts.common.timeout, *len);
            false
        }
        Mode::Compare {
            reference,
            tolerance,
        } => run_compare(&mut cpu, &opts, reference, *tolerance),
        Mode::StateTrace { interval, m_cycles } => {
            state_trace(&mut cpu, *interval, *m_cycles);
            true
        }
    };

    inspect_after(&mut cpu, &opts)?;

    Ok(crate::exit_code(passed))
}

/// Parse `run`'s arguments; `None` means help was requested.
fn parse(args: &[String]) -> Result<Option<RunOpts>, String> {
    let mut common = CommonOpts::default();
    let mut rom: Option<PathBuf> = None;
    let mut screenshot: Option<PathBuf> = None;
    let mut serial = false;
    let mut regs = false;
    let mut ppu = false;
    let mut dumps: Vec<(u16, u16)> = Vec::new();
    let mut vram_dumps: Vec<(u8, u16, u16)> = Vec::new();
    let mut compare: Option<PathBuf> = None;
    let mut tolerance: u8 = 0;
    let mut trace_len: Option<usize> = None;
    let mut no_detect = false;
    let mut state_trace = false;
    let mut interval: u64 = 1;
    let mut m_cycles: usize = DEFAULT_STATE_TRACE_M_CYCLES;

    let help = parse_args(args, &mut common, |arg, it| {
        match arg {
            "--screenshot" => screenshot = Some(PathBuf::from(next_val(it, "--screenshot")?)),
            "--serial" => serial = true,
            "--regs" => regs = true,
            "--ppu" => ppu = true,
            "--dump" => dumps.push(parse_dump(&next_val(it, "--dump")?)?),
            "--vram" => vram_dumps.push(parse_vram(&next_val(it, "--vram")?)?),
            "--compare" => compare = Some(PathBuf::from(next_val(it, "--compare")?)),
            "--tolerance" => {
                let v = next_val(it, "--tolerance")?;
                tolerance = v
                    .parse::<u8>()
                    .map_err(|_| format!("invalid tolerance '{v}'"))?;
            }
            "--trace" => {
                let v = next_val(it, "--trace")?;
                let n = v
                    .parse::<usize>()
                    .map_err(|_| format!("invalid trace length '{v}'"))?;
                if n == 0 {
                    return Err("trace length must be > 0".to_string());
                }
                trace_len = Some(n);
            }
            "--no-detect" => no_detect = true,
            "--state-trace" => state_trace = true,
            "--interval" => {
                let v = next_val(it, "--interval")?;
                interval = v
                    .parse::<u64>()
                    .map_err(|_| format!("invalid interval '{v}'"))?;
                if interval == 0 {
                    return Err("interval must be > 0".to_string());
                }
            }
            "--m-cycles" => {
                let v = next_val(it, "--m-cycles")?;
                m_cycles = v
                    .parse::<usize>()
                    .map_err(|_| format!("invalid m-cycle budget '{v}'"))?;
                if m_cycles == 0 {
                    return Err("m-cycle budget must be > 0".to_string());
                }
            }
            other if other.starts_with('-') => return Err(format!("unknown flag '{other}'")),
            other if rom.is_none() => rom = Some(PathBuf::from(other)),
            other => return Err(format!("unexpected argument '{other}'")),
        }
        Ok(())
    })?;
    if help {
        return Ok(None);
    }

    let mode = match (compare, trace_len, no_detect, state_trace) {
        (Some(reference), None, false, false) => Mode::Compare {
            reference,
            tolerance,
        },
        (None, Some(len), false, false) => Mode::Trace(len),
        (None, None, true, false) => Mode::NoDetect,
        (None, None, false, true) => Mode::StateTrace { interval, m_cycles },
        (None, None, false, false) => Mode::Detect,
        _ => {
            return Err(
                "--compare, --trace, --no-detect and --state-trace are mutually exclusive"
                    .to_string(),
            )
        }
    };

    Ok(Some(RunOpts {
        rom: rom.ok_or("missing <ROM> path")?,
        common,
        mode,
        screenshot,
        serial,
        regs,
        ppu,
        dumps,
        vram_dumps,
    }))
}

/// Default mode: run under the pass/fail detector, print the result line and
/// (with `--serial`) the captured serial log.
fn run_detect(cpu: &mut Cpu, opts: &RunOpts) -> bool {
    let run = harness::run(cpu, opts.common.protocol, opts.common.timeout);
    print_result_line(&RomResult::from_run(opts.rom.display().to_string(), &run));

    if opts.serial && !run.serial.is_empty() {
        println!("--- serial ---");
        println!("{}", run.serial.trim_end_matches(['\n', '\r']));
    }

    run.outcome.is_pass()
}

/// `--no-detect`: run the full timeout with no pass/fail detection — lets you
/// screenshot or dump a ROM whose result is screen-only, or whose memory
/// happens to trip a false detection (e.g. `auto` matching gbmicrotest's
/// $FF82), without the detector stopping the run after the first frame.
fn run_no_detect(cpu: &mut Cpu, opts: &RunOpts) -> bool {
    let start = std::time::Instant::now();
    harness::run_duration(cpu, opts.common.timeout);
    println!(
        "RAN     {}  ({:.2}s, no-detect)",
        opts.rom.display(),
        start.elapsed().as_secs_f64()
    );

    true
}

/// `--state-trace`: step until the M-cycle budget is spent, emitting records:
///
/// - `S <m_cycles> <op> <pc> <af> <bc> <de> <hl> <sp> <ime> <if> <ie> <div>
///   <tima> <tma> <tac> <lcdc> <stat> <ly> <nr52> <pcm12> <pcm34>` — one per
///   `interval` *executed* instructions, all hex except m_cycles;
/// - `F <frame> <hash>` — framebuffer hash at each frame boundary;
/// - `A <hash>` — audio hash of the samples since the previous `F`.
///
/// IO is read via `Bus::read` — exactly what the CPU observes here. Reads are
/// side-effect-free and must stay so under the scheduler; the diff verifies it.
///
/// Records are anchored to emulated-time events (executed instruction, frame
/// flip), never to a bare `step()` call, so traces stay comparable between
/// builds whose HALT step granularity differs.
fn state_trace(cpu: &mut Cpu, interval: u64, m_cycles: usize) {
    use std::io::Write;
    let stdout = std::io::stdout();
    let mut out = std::io::BufWriter::with_capacity(1 << 20, stdout.lock());
    let budget_end = cpu.clock.get_m_cycles() + m_cycles;
    let mut last_frame = cpu.clock.bus.io.ppu.current_frame;
    let mut instr: u64 = 0;

    // The app's default buffer is sized for its tighter drain cadence.
    cpu.clock.bus.io.apu.config.buffer_size = STATE_TRACE_AUDIO_BUFFER;
    cpu.clock.bus.io.apu.update_buffer_size();

    while cpu.clock.get_m_cycles() < budget_end {
        let halted_before = cpu.clock.is_cpu_halted();
        cpu.step();

        // A step can cross the budget — by a few M-cycles for an instruction,
        // by a whole jump for a halt wait. Drop its records so every build ends
        // at the same emulated instant whatever its step granularity.
        if cpu.clock.get_m_cycles() > budget_end {
            return;
        }

        let frame = cpu.clock.bus.io.ppu.current_frame;
        if frame != last_frame {
            last_frame = frame;
            let fb: &[u8] = &cpu.clock.bus.io.ppu.lcd.buffer;
            // a closed stdout (e.g. `| head`) just ends the trace
            if writeln!(out, "F {frame} {:016x}", fnv1a(fb)).is_err() {
                return;
            }

            let samples = cpu.clock.bus.io.apu.get_buffer();
            if !samples.is_empty() {
                let mut hash = FNV_OFFSET;
                for sample in samples {
                    hash = fnv1a_step(hash, &sample.to_bits().to_le_bytes());
                }
                cpu.clock.bus.io.apu.clear_buffer();
                if writeln!(out, "A {hash:016x}").is_err() {
                    return;
                }
            }
        }

        // A step that stays halted executed no instruction.
        if halted_before && cpu.clock.is_cpu_halted() {
            continue;
        }
        instr += 1;

        if instr % interval == 0 {
            let ime = cpu.clock.bus.io.interrupts.ime as u8;
            let op = cpu.step_ctx.opcode;
            let r = &mut cpu.registers;
            let (pc, af) = (r.pc, u16::from_be_bytes([r.a, r.flags.get_byte()]));
            let bc = u16::from_be_bytes([r.b, r.c]);
            let de = u16::from_be_bytes([r.d, r.e]);
            let hl = u16::from_be_bytes([r.h, r.l]);
            let sp = r.sp;
            let bus = &cpu.clock.bus;
            let io = |a: u16| bus.read(a);
            let ok = writeln!(
                out,
                "S {} {op:02x} {pc:04x} {af:04x} {bc:04x} {de:04x} {hl:04x} {sp:04x} {ime:x} \
                 {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x}",
                cpu.clock.get_m_cycles(),
                io(0xFF0F), // IF
                io(0xFFFF), // IE
                io(0xFF04), // DIV
                io(0xFF05), // TIMA
                io(0xFF06), // TMA
                io(0xFF07), // TAC
                io(0xFF40), // LCDC
                io(0xFF41), // STAT
                io(0xFF44), // LY
                io(0xFF26), // NR52
                io(0xFF76), // PCM12
                io(0xFF77), // PCM34
            );
            if ok.is_err() {
                return;
            }
        }
    }
}

const FNV_OFFSET: u64 = 0xcbf29ce484222325;

/// FNV-1a — tiny, dependency-free; only stream equality matters, not quality.
fn fnv1a_step(mut hash: u64, bytes: &[u8]) -> u64 {
    for &b in bytes {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn fnv1a(bytes: &[u8]) -> u64 {
    fnv1a_step(FNV_OFFSET, bytes)
}

/// `--compare`: run for the timeout, then diff the framebuffer against a
/// reference PNG (screenshot-based tests have no register/serial signal).
fn run_compare(cpu: &mut Cpu, opts: &RunOpts, reference: &Path, tolerance: u8) -> bool {
    harness::run_duration(cpu, opts.common.timeout);

    match compare_to_reference(cpu, reference, tolerance) {
        Ok(()) => {
            println!("PASS    {}  (visual)", opts.rom.display());
            true
        }
        Err(detail) => {
            println!("FAIL    {}  (visual)  {detail}", opts.rom.display());
            false
        }
    }
}

/// Post-run inspection: screenshot, memory/VRAM hex dumps, PPU and CPU state.
fn inspect_after(cpu: &mut Cpu, opts: &RunOpts) -> Result<(), String> {
    if let Some(path) = &opts.screenshot {
        save_screenshot(cpu, path)?;
        println!("screenshot -> {}", path.display());
    }

    for &(addr, len) in &opts.dumps {
        dump_memory(cpu, addr, len);
    }

    for &(bank, addr, len) in &opts.vram_dumps {
        dump_vram(cpu, bank, addr, len);
    }

    if opts.ppu {
        dump_ppu(cpu);
    }

    if opts.regs {
        dump_regs(cpu);
    }

    Ok(())
}

/// `run`'s full help: synopsis, common options, own flags.
pub fn print_usage() {
    eprintln!("USAGE:  oxgbc-cli run <ROM> [options]\n");
    print_common_usage();
    print_options();
}

/// Only `run`'s option block (also part of the global usage).
pub fn print_options() {
    eprintln!("run OPTIONS:");
    eprintln!("  --screenshot <PATH>      save the final framebuffer as PNG");
    eprintln!("  --no-detect              run the full timeout with no pass/fail detection");
    eprintln!("                           (for screen-only ROMs / to avoid false detections)");
    eprintln!("  --serial                 print captured serial output");
    eprintln!("  --regs                   print CPU registers + opcode bytes at PC after the run");
    eprintln!("  --dump <ADDR[:LEN]>      hex-dump memory after the run (ADDR hex, LEN dec;");
    eprintln!("                           repeatable, e.g. --dump C000:8)");
    eprintln!("  --vram <B:ADDR[:LEN]>    hex-dump VRAM bank B directly (no mode-3 blocking;");
    eprintln!("                           repeatable, e.g. --vram 1:9C00:32)");
    eprintln!("  --ppu                    print PPU registers, window state, and OAM after the run");
    eprintln!("  --trace <N>              record the last N instructions (freezes on a hang)");
    eprintln!("  --state-trace            emit observable-state records to stdout for the");
    eprintln!("                           differential harness (scripts/state-diff.sh)");
    eprintln!("  --interval <N>           state-trace: record every N instructions (default: 1)");
    eprintln!("  --m-cycles <N>           state-trace: emulated M-cycle budget (default: 20M)");
    eprintln!("  --compare <PNG>          diff the final framebuffer against a reference PNG");
    eprintln!("  --tolerance <N>          per-channel diff allowed by --compare (default 0)\n");
}
