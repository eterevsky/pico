//! Blinks the LED on a Pico board and log to USB console
//!
//! This will blink an LED attached to GP25, which is the pin the Pico uses for the on-board LED.
#![no_std]
#![no_main]

use core::panic::PanicInfo;
use core::time::Duration;
use cortex_m::peripheral::{syst::SystClkSource, SYST};

#[link_section = ".boot2"]
#[used]
pub static BOOT2: [u8; 256] = rp2040_boot2::BOOT_LOADER_W25Q080;

#[panic_handler]
fn panic(_panic_info: &PanicInfo) -> ! {
    loop {}
}

struct Timer {
    syst: SYST,
}

impl Timer {
    fn new(syst: SYST) -> Self {
        Self { syst }
    }

    fn sleep(&mut self, duration: Duration) {
        // Microsecond timer
        self.syst.set_clock_source(SystClkSource::External);

        let duration_ticks = duration.as_micros();

        let full_cycles = (duration_ticks >> 24) as u32;
        if full_cycles > 0 {
            self.syst.set_reload(0x00ffffff);
            self.syst.clear_current();
            self.syst.enable_counter();

            for _ in 0..full_cycles {
                while !self.syst.has_wrapped() {}
            }
        }

        let ticks_left = (duration_ticks & 0x00ffffff) as u32;
        if ticks_left > 1 {
            self.syst.set_clock_source(SystClkSource::External);
            self.syst.set_reload(ticks_left - 1);
            self.syst.clear_current();
            self.syst.enable_counter();

            while !self.syst.has_wrapped() {}
        }
    }
}

#[cortex_m_rt::entry]
fn main() -> ! {
    let pac = rp2040_pac::Peripherals::take().unwrap();
    let core = rp2040_pac::CorePeripherals::take().unwrap();
    let pin_id = 25;

    pac.RESETS.reset.modify(|_, w| w.io_bank0().set_bit());
    pac.RESETS.reset.modify(|_, w| w.io_bank0().clear_bit());
    while pac.RESETS.reset_done.read().io_bank0().bit_is_clear() {}

    pac.IO_BANK0.gpio[pin_id].gpio_ctrl.modify(|_, w| w.funcsel().sio());

    pac.SIO.gpio_oe.write(|w| unsafe { w.bits(1 << pin_id) } );

    let mut timer = Timer::new(core.SYST);
    let delay_500ms = Duration::from_millis(500);

    loop {
        pac.SIO.gpio_out.write(|w| unsafe { w.bits(0) } );
        timer.sleep(delay_500ms);
        pac.SIO.gpio_out.write(|w| unsafe { w.bits(1 << pin_id) } );
        timer.sleep(delay_500ms);
    }
}
