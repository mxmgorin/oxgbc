//! `Serial::advance` must be bit-identical to the per-tick loop with its
//! per-tick `is_active` gate — swept across clock phases and window sizes.
use core::auxiliary::io::Serial;
use core::cpu::interrupts::Interrupts;

#[test]
fn test_serial_advance_equals_ticks() {
    for div0 in 0u16..0x400 {
        for wsize in [1usize, 4, 8, 12, 20, 24] {
            let mut s_ref = Serial::default();
            let mut s_new = Serial::default();
            let bit0 = div0 & 0x100 != 0;
            s_ref.write_sc(0x81, bit0);
            s_new.write_sc(0x81, bit0);
            let mut i_ref = Interrupts::default();
            let mut i_new = Interrupts::default();

            let mut div_w = div0;
            let total = 6000usize;
            let mut k = 0usize;
            while k < total {
                let w = wsize.min(total - k);
                // reference: per-tick with per-tick gate
                for j in 1..=w {
                    if s_ref.is_active() {
                        let bit = div_w.wrapping_add(j as u16) & 0x100 != 0;
                        s_ref.tick(bit, &mut i_ref);
                    }
                }
                // new: windowed
                if s_new.is_active() {
                    s_new.advance(div_w, w, &mut i_new);
                }
                div_w = div_w.wrapping_add(w as u16);
                k += w;
                assert_eq!(s_ref.is_active(), s_new.is_active(), "active: div0={div0:#x} w={wsize} k={k}");
                assert_eq!(i_ref.int_flags, i_new.int_flags, "IF: div0={div0:#x} w={wsize} k={k}");
            }
        }
    }
}

/// Multi-byte printing like the mooneye framework does: transfer completes,
/// the ROM immediately starts the next byte (write_sc reseeds the edge
/// detector between windows). This is where the system-level divergence
/// lived — a single-transfer sweep never re-seeds.
#[test]
fn test_serial_advance_multi_byte_with_reseeds() {
    for div0 in (0u16..0x400).step_by(7) {
        for wsize in [4usize, 8, 20] {
            let mut s_ref = Serial::default();
            let mut s_new = Serial::default();
            let mut i_ref = Interrupts::default();
            let mut i_new = Interrupts::default();

            let bit0 = div0 & 0x100 != 0;
            s_ref.write_sc(0x81, bit0);
            s_new.write_sc(0x81, bit0);

            let mut div_w = div0;
            let mut bytes_done_ref = 0;
            let mut bytes_done_new = 0;
            let total = 60_000usize;
            let mut k = 0usize;
            while k < total {
                let w = wsize.min(total - k);
                for j in 1..=w {
                    if s_ref.is_active() {
                        let bit = div_w.wrapping_add(j as u16) & 0x100 != 0;
                        s_ref.tick(bit, &mut i_ref);
                    }
                }
                if s_new.is_active() {
                    s_new.advance(div_w, w, &mut i_new);
                }
                div_w = div_w.wrapping_add(w as u16);
                k += w;

                // the "ROM": on completion, start the next byte at the next
                // window boundary, like the print loop does
                let bit_now = div_w & 0x100 != 0;
                if !s_ref.is_active() && bytes_done_ref < 5 {
                    bytes_done_ref += 1;
                    s_ref.write_sc(0x81, bit_now);
                }
                if !s_new.is_active() && bytes_done_new < 5 {
                    bytes_done_new += 1;
                    s_new.write_sc(0x81, bit_now);
                }

                assert_eq!(
                    s_ref.is_active(),
                    s_new.is_active(),
                    "active: div0={div0:#x} w={wsize} k={k}"
                );
                assert_eq!(
                    i_ref.int_flags, i_new.int_flags,
                    "IF: div0={div0:#x} w={wsize} k={k}"
                );
                assert_eq!(
                    bytes_done_ref, bytes_done_new,
                    "bytes: div0={div0:#x} w={wsize} k={k}"
                );
            }
        }
    }
}

/// A DIV write mid-transfer jumps the counter phase: the serial clock bit
/// can fall with it, which must shift a bit exactly like the per-tick edge
/// detector does (the serial flavor of the TIMA DIV-write glitch).
#[test]
fn test_serial_advance_div_reset_mid_transfer() {
    for div0 in (0u16..0x400).step_by(3) {
        let mut s_ref = Serial::default();
        let mut s_new = Serial::default();
        let mut i_ref = Interrupts::default();
        let mut i_new = Interrupts::default();

        let bit0 = div0 & 0x100 != 0;
        s_ref.write_sc(0x81, bit0);
        s_new.write_sc(0x81, bit0);

        let mut div_w = div0;
        for step in 0..2000usize {
            // "the ROM writes DIV" at assorted window boundaries
            if step % 37 == 5 {
                div_w = 0;
            }

            let w = 4usize;
            for j in 1..=w {
                if s_ref.is_active() {
                    let bit = div_w.wrapping_add(j as u16) & 0x100 != 0;
                    s_ref.tick(bit, &mut i_ref);
                }
            }
            if s_new.is_active() {
                s_new.advance(div_w, w, &mut i_new);
            }
            div_w = div_w.wrapping_add(w as u16);

            assert_eq!(
                s_ref.is_active(),
                s_new.is_active(),
                "active: div0={div0:#x} step={step}"
            );
            assert_eq!(i_ref.int_flags, i_new.int_flags, "IF: div0={div0:#x} step={step}");
        }
    }
}
