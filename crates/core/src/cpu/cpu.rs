use crate::auxiliary::clock::Clock;
use crate::cpu::Registers;
use serde::{Deserialize, Serialize};

pub const CPU_CLOCK_SPEED: u32 = 4194304;

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct StepCtx {
    pub opcode: u8,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Cpu {
    pub registers: Registers,
    pub enabling_ime: bool,
    pub clock: Clock,
    pub step_ctx: StepCtx,
    pub stop_m_cycles: u32,
}

impl Cpu {
    pub fn new(clock: Clock) -> Self {
        Self {
            registers: Registers::new(clock.bus.io.ppu.lcd.model),
            enabling_ime: false,
            step_ctx: StepCtx::default(),
            clock,
            stop_m_cycles: 0,
        }
    }

    /// Costs 2 M-Cycles with push PC
    #[inline(always)]
    pub fn goto_addr_with_cond<const C: u8>(&mut self, addr: u16) {
        if self.check_cond::<C>() {
            self.goto_addr(addr);
        }
    }

    #[inline(always)]
    pub fn goto_addr_push_pc_with_cond<const C: u8>(&mut self, addr: u16) {
        if self.check_cond::<C>() {
            self.goto_addr_push_pc(addr);
        }
    }

    #[inline(always)]
    pub fn goto_addr(&mut self, addr: u16) {
        self.clock.tick_m_cycles(1); // internal: branch decision?
        self.registers.pc = addr;
    }

    #[inline(always)]
    pub fn goto_addr_push_pc(&mut self, addr: u16) {
        self.clock.tick_m_cycles(1); // internal: branch decision?
        self.push16(self.registers.pc);
        self.registers.pc = addr;
    }

    /// Reads 8bit immediate data by PC and increments PC + 1. Costs 1 M-Cycle.
    #[inline(always)]
    pub fn read_pc(&mut self) -> u8 {
        let value = self.clock.bus.read(self.registers.pc);
        self.registers.pc = self.registers.pc.wrapping_add(1);
        self.clock.tick_m_cycles(1);

        value
    }

    /// Reads 16bit immediate data by PC and increments PC + 2. Costs 1 M-Cycle.
    #[inline(always)]
    pub fn read_pc16(&mut self) -> u16 {
        u16::from_le_bytes([self.read_pc(), self.read_pc()])
    }

    /// Reads data from memory. Costs 1 M-Cycle.
    #[inline(always)]
    pub fn read_memory(&mut self, address: u16) -> u8 {
        let value = self.clock.bus.read(address);
        self.clock.tick_m_cycles(1);

        value
    }

    /// Writes to memory. Costs 1 M-Cycle.
    #[inline(always)]
    pub fn write_to_memory(&mut self, address: u16, value: u8) {
        self.clock.bus.write(address, value);
        self.clock.tick_m_cycles(1);
    }

    #[inline]
    pub fn step_debug(&mut self, debugger: &mut crate::debugger::Debugger) {
        debugger.print(self);
        debugger.poll_serial(&mut self.clock.bus);
        self.step();
    }

    #[inline]
    pub fn step(&mut self) {
        if self.stop_m_cycles > 0 {
            self.stop_m_cycles -= 1;
            return;
        }

        self.handle_interrupts();

        if self.clock.is_cpu_halted() {
            if !self.clock.bus.io.interrupts.ime && self.clock.bus.io.interrupts.has_pending() {
                // HALT bug: continue executing instructions
                // The CPU continues execution after the HALT, but the byte after it is read twice in a row (PC is not incremented).
                self.clock.cpu_halted = false;
            } else {
                // Wait for an interrupt by jumping to the next event whose
                // interrupt is enabled instead of idling one M-cycle at a
                // time; the horizon ends the window on that event's exact
                // M-cycle, so dispatch timing is unchanged.
                let jump = crate::auxiliary::scheduler::halt_horizon(&self.clock);
                self.clock.tick_m_cycles(jump);
                return;
            }
        }

        self.step_ctx.opcode = self.read_pc();
        let prev_enabling_ime = self.enabling_ime;
        self.execute_opcode();

        if self.enabling_ime && prev_enabling_ime {
            // execute after next instruction when flag is changed
            self.enabling_ime = false;
            self.clock.bus.io.interrupts.ime = true;
        }
    }
}
