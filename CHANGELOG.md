# Changelog

All notable changes to oxGBC are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to a simple incrementing release number.

## [Unreleased]

### Added
- **A PortMaster package for the Linux handhelds** — `oxgbc-portmaster.zip`,
  carrying both ARM binaries, a launcher that shelves the card's Game Boy folder
  and keeps saves in the port's own directory, and the port's metadata.
- `OXGBC_ROMS_DIR` names the folder a fresh install shelves and browses first.
- **Select + Y opens the menu** as well as Select + Start, which a handheld never
  sees — there it closes whatever is running.
- A direction held on the pad repeats in the menus, as one held on the keyboard
  always has: a shelf of a hundred carts no longer takes a hundred presses.

### Fixed
- A shader the device cannot compile falls back to Passthrough instead of taking
  the app down with it — GLSL ES 1.00 has no bitwise operators, which several of
  them use.
- The shelf lists zipped ROMs, and matches an extension whatever its case: a
  folder of zips — how a collection usually arrives — came out empty, though both
  file browsers opened them. One list of extensions now serves all three.
- The retro frontend's file browser offers zips too.

### Changed
- **New picture defaults**: integer scaling, the Mono LCD shader on the GL backend,
  and the grid filter on the SDL2 one — whole pixels, and pixel edges either way
  the frame is drawn.
- Rewind moved to L1 and slow motion to Y, so the shoulders run time the way every
  other pad does: L1 back, R1 forward.
- The emulated model defaults to auto — a cart runs as the machine its header names.
  Forcing CGB, which colorizes DMG games, is now a setting rather than the default.
- The ARM builds link against the device's SDL2 instead of bundling one, and are
  cross-built against glibc 2.28: the previous ones needed 2.35 (aarch64) or 2.39
  (armhf), which no handheld userland has. `oxgbc-linux-armv7.zip` is now
  `oxgbc-linux-armhf.zip`.

## [0.22] - 2026-08-14

### Added
- **A new graphical frontend**, built on egui: a cartridge library with cover
  art in shelf, list and carousel layouts, save-state slots with previews and
  play time, settings pages with input rebinding and themes, an in-app file
  browser, a pause overlay and an About page.

### Changed
- The graphical frontend is now the default; the text menu stays available
  behind the `frontend-retro` feature.

### Fixed
- Cartridges are deduplicated by name instead of path.
- Window tiling on the GL backend, and the shader program is now bound before
  its uniforms are sent.

### Performance
- The menu is paced to 60 fps and repainted only on change: idle CPU 9.7% →
  0.97% of a core.
- Only the shelf rows on screen are built, with cards and textures cached:
  frame −20–25%, RSS 191 → 159.5 MB.

## [0.21] - 2026-08-02

### Fixed
- Sprites vanished in DMG games that leave junk in the CGB object attribute bits:
  the tile was fetched from VRAM bank 1, which such a game never writes.
  Regression since 0.18.
- LCDC.4 mid-fetch glitch read no longer applies while BG/window is disabled.

### Changed
- **Event-scheduled core: 29–40% less wall time.** Devices batch-advance to their
  next event instead of ticking per M-cycle, idle DMAs leave the loop, and HALT
  jumps to the next interrupt. A/B medians against the per-tick chain: cpu_instrs
  −39.7%, SameSuite APU −28.7%, DMG game −29.0%, CGB game −33.4%.

### Added
- CLI: a `bench` command and `--state-trace` for the differential harness.

## [0.20] - 2026-07-22

### Added
- Colorize original Game Boy (DMG) games **by default**, using the Game Boy
  Color boot-ROM palettes, with independent BG / OBJ0 / OBJ1 base palettes.
- New low-latency audio output: an SDL pull callback backed by a lock-free
  SPSC ring buffer.
- Per-channel mute toggles in the audio menu.
- Link cable: serial transfer with serial interrupt.
- Mealybug Tearoom PPU rendering test suite.
- Developer CLI: a headless test-ROM runner (`run` / `check`) with `--dump`,
  `--screenshot`, `--regs`, `--trace`, screenshot comparison, report
  generation, and a `--no-detect` flag; built on a shared headless harness.

### Changed
- Default hardware model is now **CGB**, which enables DMG colorization.
- Audio mixing is anti-aliased with a box filter instead of point sampling.
- Audio is resampled to exact 44.1 kHz via a fractional accumulator.
- Model-specific high-pass-filter constant and soft clipping on the volume path.

### Fixed
- **APU accuracy overhaul** (SameSuite: 7 → 69/78):
  - Frame sequencer clocked from the DIV-APU bit's falling edge.
  - Square channels: trigger delays, sample suppression, per-duty-step output
    latching, and freezing of inactive channels.
  - Noise channel: free counter, bit-edge LFSR, and background counting.
  - Wave channel: byte latch, trigger delay, live `NR32`, and wave-RAM
    redirection.
  - Envelope pipeline, zombie mode, and DAC-on for `NRx2 = 0x08`.
  - Sweep pipeline and frequency-change glitches.
  - Extra length-counter clock quirks and power-on DIV skip.
  - Power-on 1 MHz phase reseed and double-speed phase sign flip.
  - Uniform clocking and trigger delay in double-speed mode.
- **PPU accuracy**:
  - Passes the full mooneye PPU suite: edge-triggered STAT line and
    cycle-exact mode timings.
  - Pixel-exact window start position.
  - Emulates the `LCDC.4` mid-fetch glitch.
  - Correct pixel discard when scrolling.
  - CGB palette index accessible and incrementing during mode 3.
- Post-boot hardware state and CGB compatibility-mode registers (passes the
  mooneye boot tests).
- CPU: `EI` + `HALT` with a pending interrupt keeps `PC` on the `HALT`.
- DMA: store `HDMA5` length on every write; an LCD-off transfer starts one
  HBlank block.
- MBC3 RTC set / halt / carry and battery persistence, made WASM-safe.
- App: start the passed ROM regardless of `auto_continue`.

### Performance
- Dropped the ROM from save states and batched pauses in the sleep spin.
- APU: recompute the mix only when the output changes; tick only in trace mode.
- Serial: tick only during an active transfer.
- Timer: branchless TAC bit in the falling-edge detector.
- PPU: dropped per-pixel bookkeeping on FIFO push.

[0.22]: https://github.com/mxmgorin/oxgbc/releases/tag/0.22
[0.21]: https://github.com/mxmgorin/oxgbc/releases/tag/0.21
[0.20]: https://github.com/mxmgorin/oxgbc/releases/tag/0.20
