//! Blinks the LED on a Pico board and log to USB console
//!
//! This will blink an LED attached to GP25, which is the pin the Pico uses for the on-board LED.
//! USB console can be used with write!() macro and is also used for panic! error message.
#![no_std]
#![no_main]

use embedded_hal::digital::v2::OutputPin;
use embedded_time::fixed_point::FixedPoint as _;
use log::info;
use rp2040_hal as hal;
use rp2040_hal::{clocks::Clock as _, gpio, pac, sio::Sio, watchdog::Watchdog};

mod blocking_spi;
mod pico_wireless;

#[link_section = ".boot2"]
#[used]
pub static BOOT2: [u8; 256] = rp2040_boot2::BOOT_LOADER_W25Q080;

// External high-speed crystal on the pico board is 12Mhz
pub const XOSC_CRYSTAL_FREQ: u32 = 12_000_000;

const ESP_LED_R: u8 = 25;
const ESP_LED_G: u8 = 26;
const ESP_LED_B: u8 = 27;

#[cortex_m_rt::entry]
fn main() -> ! {
    let mut pac = pac::Peripherals::take().unwrap();
    let core = pac::CorePeripherals::take().unwrap();
    let mut watchdog = Watchdog::new(pac.WATCHDOG);

    let clocks = hal::clocks::init_clocks_and_plls(
        XOSC_CRYSTAL_FREQ,
        pac.XOSC,
        pac.CLOCKS,
        pac.PLL_SYS,
        pac.PLL_USB,
        &mut pac.RESETS,
        &mut watchdog,
    )
    .ok()
    .unwrap();

    pico_usb_console::init_usb_manager(
        pac.USBCTRL_REGS,
        pac.USBCTRL_DPRAM,
        clocks.usb_clock,
        &mut pac.RESETS,
    );

    let console = pico_usb_console::get_console();

    unsafe {
        log::set_logger_racy(console)
            .map(|()| log::set_max_level(log::LevelFilter::Info))
            .unwrap();
    }

    let mut delay = cortex_m::delay::Delay::new(core.SYST, clocks.system_clock.freq().integer());

    {
        // Wait until USB console is ready
        let ms = pico_usb_console::wait_until_ready(&mut delay);
        info!("USB console initialized after {ms} ms.");
    }

    {
        let system_freq = clocks.system_clock.freq().integer() as f32 / 1E6;
        info!("System clock frequency: {system_freq} MHz");
    }
    info!("Initializing pins");

    let sio = Sio::new(pac.SIO);
    let pins = hal::gpio::Pins::new(
        pac.IO_BANK0,
        pac.PADS_BANK0,
        sio.gpio_bank0,
        &mut pac.RESETS,
    );
    let mut led_pin = pins.gpio25.into_push_pull_output();

    let button_a = pico_wireless::ButtonA::new(pins.gpio12);

    let cs = pins.gpio7.into_push_pull_output();
    let gpio2 = pins.gpio2.into_push_pull_output();
    let resetn = pins.gpio11.into_push_pull_output();
    let ack = pins.gpio10.into_pull_down_input();
    let _ = pins.gpio16.into_mode::<gpio::FunctionSpi>();
    let _ = pins.gpio18.into_mode::<gpio::FunctionSpi>();
    let _ = pins.gpio19.into_mode::<gpio::FunctionSpi>();

    info!("Creating ESP32 inteface");

    let mut esp32 = pico_wireless::Esp32::new(
        &mut pac.RESETS,
        pac.SPI0,
        cs,
        ack,
        gpio2,
        resetn,
        &mut delay,
        clocks.system_clock.freq().integer(),
    );

    esp32.analog_write(ESP_LED_G, 0).unwrap();

    show_networks(&mut esp32);

    loop {
        led_pin.set_high().unwrap();
        esp32.analog_write(ESP_LED_R, 255).unwrap();
        esp32.analog_write(ESP_LED_B, 0).unwrap();
        info!("On {}", button_a.pressed());
        delay.delay_ms(500);

        led_pin.set_low().unwrap();
        esp32.analog_write(ESP_LED_R, 0).unwrap();
        esp32.analog_write(ESP_LED_B, 255).unwrap();
        info!("Off {}", button_a.pressed());
        delay.delay_ms(500);
    }
}

fn show_networks(esp32: &mut pico_wireless::Esp32) {
    let mut data = [0; 256];
    let mut offsets = [0; 16];

    let n = esp32.scan_networks(&mut data, &mut offsets).unwrap();
    info!("SSIDs: {n}");
    info!("{:?}", &data[..16]);
    info!("{:?}", &offsets[..16]);

    for i in 0..n {
        let ssid = core::str::from_utf8(&data[offsets[i]..offsets[i+1]]).unwrap();
        info!("{ssid}");
    }
}