# oxGBC

A Game Boy and Game Boy Color emulator written in Rust — sub-instruction CPU
timing, a dot-level PPU, and a gamepad-first interface. Cartridges are yours to
bring; none ship with the port.

## Setup

1. Put `.gb` or `.gbc` files (or a zip of one) in your Game Boy folder.
2. Launch oxGBC — a `gbc`, `gb` or `gameboy` folder beside your ROMs is shelved
   on the first run.
3. Any other folder can be picked from **Library → Browse**.

Battery saves, save states, covers and settings live in `ports/oxgbc`.

## Controls

| Button           | In a menu                | In a game                   |
|------------------|--------------------------|-----------------------------|
| D-pad            | Move                     | D-pad                       |
| A                | Confirm                  | A                           |
| B                | Back                     | B                           |
| Start            | Confirm                  | Start                       |
| Select           | Options for the item     | Select                      |
| Select + Y       | —                        | Menu                        |
| L1 / R1 (hold)   | —                        | Rewind / turbo              |
| Y (hold)         | —                        | Slow motion                 |
| Select + L1 / R1 | —                        | Load / create a save state  |
| X                | —                        | Next palette (Select + X inverts it) |
| Select + A / B   | —                        | Next / previous shader      |
| Start + ↑ / ↓    | —                        | Volume                      |
| Start + ← / →    | —                        | Previous / next state slot  |

The menu is on **Select + Y** here because Select + Start is the hotkey that
closes a port. Quitting from the menu and closing the port both write your
battery save. Every binding can be changed in **Settings → Input**.

## Credits

- Developed and ported by mxmgorin
- Source and issues: https://github.com/mxmgorin/oxgbc
- Licensed under the GPL-3.0; see `licenses/`
