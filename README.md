<p align="center">
  <a href="https://github.com/mxmgorin/oxgbc">
    <img src="media/logo.svg" alt="oxGBC" width="200">
  </a>
</p>

<p align="center">
  <b>A Game Boy &amp; Game Boy Color emulator written in Rust.</b>
</p>

<p align="center">
  <a href="https://mxmgorin.github.io/oxgbc/"><b>🕹️&nbsp;&nbsp;Play online</b></a>
  &nbsp;&nbsp;&nbsp;
  <a href="https://github.com/mxmgorin/oxgbc/releases/latest"><b>📥&nbsp;&nbsp;Download</b></a>
</p>

---

<div align="center">

[![Tests](https://github.com/mxmgorin/oxgbc/actions/workflows/test.yml/badge.svg)](https://github.com/mxmgorin/oxgbc/actions/workflows/test.yml)
[![Release](https://img.shields.io/github/v/release/mxmgorin/oxgbc?color=blue)](https://github.com/mxmgorin/oxgbc/releases/latest)
[![License](https://img.shields.io/github/license/mxmgorin/oxgbc?color=blue)](./LICENSE)
[![Dependencies](https://deps.rs/repo/github/mxmgorin/oxgbc/status.svg)](https://deps.rs/repo/github/mxmgorin/oxgbc)

</div>

<!-- optional extras:
per-platform build badges (re-enable if you want them back):
[![Android](https://github.com/mxmgorin/oxgbc/actions/workflows/build-android.yml/badge.svg)](https://github.com/mxmgorin/oxgbc/actions/workflows/build-android.yml)
[![Windows](https://github.com/mxmgorin/oxgbc/actions/workflows/build-windows.yml/badge.svg)](https://github.com/mxmgorin/oxgbc/actions/workflows/build-windows.yml)
[![macOS](https://github.com/mxmgorin/oxgbc/actions/workflows/build-macos.yml/badge.svg)](https://github.com/mxmgorin/oxgbc/actions/workflows/build-macos.yml)
[![Linux](https://github.com/mxmgorin/oxgbc/actions/workflows/build-linux.yml/badge.svg)](https://github.com/mxmgorin/oxgbc/actions/workflows/build-linux.yml)
[![Linux ARM](https://github.com/mxmgorin/oxgbc/actions/workflows/build-linux-arm.yml/badge.svg)](https://github.com/mxmgorin/oxgbc/actions/workflows/build-linux-arm.yml)

re-enable Downloads once the count is higher:
[![Downloads](https://img.shields.io/github/downloads/mxmgorin/oxgbc/total.svg?color=blue)](https://github.com/mxmgorin/oxgbc/releases)
[![Lines of code](https://tokei.rs/b1/github/mxmgorin/oxgbc)](https://github.com/mxmgorin/oxgbc)
-->

`oxGBC` (**ox**ide + **G**ame **B**oy **C**olor) is built around a single portable emulation core powering Windows, macOS, Linux, Android, and WebAssembly. It aims for high accuracy through sub-instruction CPU timing and dot-level PPU emulation while providing modern features such as save states, rewind, shaders, and configurable controls.

The emulator passes a wide range of community test suites and is continuously validated against them in CI, including [Blargg](https://github.com/retrio/gb-test-roms), [Mooneye](https://github.com/Gekkio/mooneye-test-suite), [SameSuite](https://github.com/liji32/samesuite), [DMG-acid2](https://github.com/mattcurrie/dmg-acid2), [CGB-acid2](https://github.com/mattcurrie/cgb-acid2), [CGB-acid-hell](https://github.com/mattcurrie/cgb-acid-hell), and [Magen](https://github.com/alloncm/MagenTests).

## Screenshots

| Shelf | List | Carousel | Cartridge |
|:---:|:---:|:---:|:---:|
| ![The library as a shelf of cartridges, each one drawn from its header and carrying its cover art on the label](media/screenshots/library-shelf.png) | ![The same library as a list: one game per row, its cart beside the title and the play time behind it](media/screenshots/library-list.png) | ![The same library as a carousel: one cart held in front, its neighbours standing back on either side, its name underneath](media/screenshots/library-carousel.png) | ![What one cartridge offers besides playing it — rename and cover — over the dimmed game](media/screenshots/cart-actions.png) |

| Pause | Save states | Settings | Key bindings |
|:---:|:---:|:---:|:---:|
| ![The pause overlay, titled by the game it suspended: resume, save states, restart, library, settings and quit](media/screenshots/pause.png) | ![The save-state slots, each with the screen it was saved with and how long ago](media/screenshots/save-states.png) | ![The settings page: palette, video, audio, emulation and input rows with the value each is set to](media/screenshots/settings.png) | ![Every action and the keys it answers to — the d-pad, the buttons, the diagonals and the shortcuts — each one rebindable from here](media/screenshots/key-bindings.png) |

## Demos

<p align="center">
  <a href="https://raw.githubusercontent.com/mxmgorin/oxgbc/main/media/acid.gif" target="_blank">
    <img src="https://raw.githubusercontent.com/mxmgorin/oxgbc/main/media/acid.gif" alt="Passing the CGB-acid2 and CGB-acid-hell PPU tests" width="200"/>
  </a>&nbsp;&nbsp;
  <a href="https://raw.githubusercontent.com/mxmgorin/oxgbc/main/media/prehistorik.gif" target="_blank">
    <img src="https://raw.githubusercontent.com/mxmgorin/oxgbc/main/media/prehistorik.gif" alt="Prehistorik Man" width="200"/>
  </a>
</p>

## Features

**Gameplay**

- **Save States** — Multiple save slots with optional automatic save and restore
- **Rewind** — Configurable rewind for undoing gameplay actions
- **Speed Control** — Adjustable emulation speed with configurable Slow and Turbo modes
- **Custom Controls** — Rebindable controls for keyboard and gamepad with support for button combinations

**Video & Rendering**

- **Shader Support** — Optional OpenGL backend with custom GLSL shaders.
- **Visual Filters** — Grid, subpixel, scanline, dot-matrix, and vignette effects
- **Frame Blending** — Configurable LCD ghosting simulation with multiple blending modes
- **Palettes** — Multiple built-in palettes with support for user-defined palettes via `palettes.json`

**Interface & Tooling**

- **GUI & Configuration** — Full graphical configuration with optional manual editing of `config.json`
- **File Browser** — Browse and launch ROMs directly from the emulator
- **ROM Library** — Automatic ROM directory scanning with menu-based launching
- **WebAssembly Build** — Runs entirely in the browser with no installation required
- **Tile Viewer** — Real-time inspection of background and sprite tiles (SDL2 renderer)

**Emulation**

- **CPU** — Sharp LR35902 with sub-instruction timing
- **PPU** — Dot-level LCD controller emulation synchronized with the CPU
- **APU** — All four Game Boy audio channels
- **Cartridge Hardware** — MBC0, MBC1, MBC1M, MBC2, MBC3, and MBC5
- **Real-Time Clock** — Battery-backed MBC3 RTC
- **Battery-backed SRAM** — Persistent cartridge save data

## 🎮 Controls

Every binding is customizable from the in-app settings menu (or by editing
`config.json`), for both keyboard and gamepad.

In the menus the d-pad moves and A confirms, B backs out, **Start** opens whatever
else can be done with the focused item — a cartridge's sheet, a save slot's — and
**Select** goes straight to the settings.

<details>
<summary><b>Default control mappings</b> (click to expand)</summary>

| Action                           | ⌨️ Keyboard              | 🎮 Gamepad                                 |
| -------------------------------- | ------------------------ | ------------------------------------------ |
| D-pad Up                         | Arrow Up                 | D-pad Up                                   |
| D-pad Down                       | Arrow Down               | D-pad Down                                 |
| D-pad Left                       | Arrow Left               | D-pad Left                                 |
| D-pad Right                      | Arrow Right              | D-pad Right                                |
| B                                | Z                        | B                                          |
| A                                | X                        | A                                          |
| Start                            | Enter or S               | Start                                      |
| Select                           | Backspace or A           | Select                                     |
| Rewind (hold)                    | R                        | LB                                         |
| Turbo mode (hold)                | Tab                      | RB                                         |
| Slow mode (hold)                 | Space                    | Y                                          |
| Main menu                        | Esc or Q                 | Select + Start or Select + Y               |
| Screen scale Up and Down         | + (Equals) and - (Minus) |                                            |
| Fullscreen Toggle                | F11                      |                                            |
| Mute audio                       | M                        |                                            |
| Invert palette                   | I                        | Select + X                                 |
| Next palette                     | P                        | X                                          |
| Load save state (1–4)            | F1–F4                    | Select + LB                                |
| Create save state (1–9)          | 1–9                      | Select + RB                                |
| Volume Up and Down               | PageUp and PageDown      | Start + D-pad Up and Start + D-pad Down    |
| Prev and Next Save State Slot    |                          | Start + D-pad Right and Start + D-pad Left |
| Prev and Next Shader             | [ and ]                  | Select + B and Select + A                  |
| Pause/Stepping mode              | F5                       |                                            |
| Step frame                       | F6                       |                                            |
| Step scanline                    | F7                       |                                            |
| Clear screen                     | F10                      |                                            |
| Toggle debugger (In debug build) | ~                        |                                            |

</details>

## 📦 Installing

Grab the latest build for your platform — Windows, macOS, Linux (x86-64 and
ARM for retro handhelds), or Android — from the
[**Releases**](https://github.com/mxmgorin/oxgbc/releases/latest) page, or
[**play online**](https://mxmgorin.github.io/oxgbc/) with nothing to install.

### Retro handhelds

`oxgbc-portmaster.zip` is a [PortMaster](https://portmaster.games) port for the
Linux handhelds (aarch64 and armhf, glibc 2.28 and up): unzip it into `ports/`
on the card, and oxGBC appears in the Ports menu. It shelves your `gb`, `gbc` or
`gameboy` folder on the first launch and keeps its saves and settings in
`ports/oxgbc`. `make portmaster` builds the same zip locally.

### macOS first launch

Because the app is only ad-hoc signed (no paid Apple Developer ID), Gatekeeper
blocks the first launch with an *"unidentified developer"* warning. To open it:

- **macOS 15 (Sequoia) and later:** go to **System Settings → Privacy &
  Security**, scroll to the message about oxGBC, and click **Open Anyway**.
- **macOS 14 (Sonoma) and earlier:** right-click oxGBC → **Open** → **Open**.

Or clear the quarantine flag from a terminal (works on every version):

```bash
xattr -dr com.apple.quarantine /Applications/oxGBC.app
```

## 🛠️ Building

First, make sure you have Rust installed. If you don't, install it with:

```
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Then install the SDL2 development libraries for your platform:

```bash
# Arch Linux
sudo pacman -S sdl2

# Debian / Ubuntu
sudo apt install libsdl2-dev

# Fedora
sudo dnf install SDL2-devel

# macOS (Homebrew)
brew install sdl2
```

> No system SDL2 (e.g. Windows)? Compile it from source with the bundled feature:
> `cargo build --release -p desktop --features sdl2-bundled`

After that, build the release binary:

```bash
cargo build --release
```

## Running

Launch with a ROM:

```bash
cargo run --release -p desktop -- path/to/game.gb
```

Or run without arguments and use the built-in file browser / ROM scanner to pick a game from the GUI:

```bash
cargo run --release -p desktop
```

## Support the project ⭐

Bug reports, and feature requests are welcome. If a game misbehaves, please [open an issue](https://github.com/mxmgorin/oxgbc/issues)
with the ROM title and what went wrong — accuracy reports are especially
valuable.

If you find the project useful, please give it a star on GitHub. It helps others
discover it and keeps development going.

## License

This project is licensed under the terms of the **GNU General Public License v3.0 (GPLv3)**.
See the [LICENSE](LICENSE) file for the full text.

## References

Here are some useful resources for Game Boy development and emulation:

- [Game Boy Complete Technical Reference](https://gbdev.io/pandocs/)
- [Gekkio's Complete Technical Reference](https://gekkio.fi/files/gb-docs/gbctr.pdf)
- [Game Boy CPU Opcodes](https://www.pastraiser.com/cpu/gameboy/gameboy_opcodes.html)
- [Gbops, an accurate opcode table for the Game Boy](https://izik1.github.io/gbops/index.html)
- [RGBDS GBZ80 Assembly Documentation](https://rgbds.gbdev.io/docs/v0.9.0/gbz80.7)
- [A curated list of Game Boy development resources](https://github.com/gbdev/awesome-gbdev)

## Acknowledgments

This project was possible because of homebrew and emulation community. Huge thanks to everyone who wrote the documentation and test suites that make accurate emulation possible.

**Test suites** — bundled in [`roms/`](roms):

- [SM83 tests](https://github.com/SingleStepTests/sm83) — single-step CPU instruction tests
- [Blargg's test ROMs](https://github.com/retrio/gb-test-roms) — CPU, timing, and APU accuracy
- [Mooneye test suite](https://github.com/Gekkio/mooneye-test-suite) — timing and hardware-behavior tests
- [SameSuite](https://github.com/LIJI32/SameSuite) — APU and edge-case hardware tests
- [GBMicrotest](https://github.com/aappleby/GBMicrotest) — fine-grained timing microtests
- [dmg-acid2](https://github.com/mattcurrie/dmg-acid2) — PPU rendering (DMG)
- [cgb-acid2](https://github.com/mattcurrie/cgb-acid2) — PPU rendering (CGB)
- [cgb-acid-hell](https://github.com/mattcurrie/cgb-acid-hell) — advanced mid-frame PPU rendering (CGB)
- [Mealybug Tearoom tests](https://github.com/mattcurrie/mealybug-tearoom-tests) — mid-scanline PPU rendering
- [MagenTests](https://github.com/alloncm/MagenTests) — PPU rendering (DMG and CGB)
- [rtc3test](https://github.com/aaaaaa123456789/rtc3test) — MBC3 real-time-clock tests

Many of these are packaged through c-sp's [game-boy-test-roms](https://github.com/c-sp/game-boy-test-roms) collection.

**Code & assets:**

- [SameBoy](https://github.com/LIJI32/SameBoy) — used as a reference for cross-checking behavior (especially the APU), and the source of the shaders (modified for GLES compatibility)

The web demo also bundles open-source homebrew games and test ROMs — see [ROM credits & licenses](crates/web/assets/README.md).
