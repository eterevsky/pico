use embedded_hal::{
    digital::v2::{InputPin as _, OutputPin as _},
    spi::FullDuplex
};
use embedded_time::{fixed_point::FixedPoint as _, rate::Extensions as _};
use log::info;
use rp2040_hal::{
    gpio::{
        pin,
        pin::bank0::{Gpio2, Gpio7, Gpio10, Gpio11, Gpio12},
        pin::PinId,
        Pin,
    },
    pac, spi,
};
use crate::blocking_spi::Spi;

const START_CMD: u8 = 0xE0;
const END_CMD: u8 = 0xEE;
const ERR_CMD: u8 = 0xEF;

const REPLY_FLAG: u8 = 1 << 7;

const SET_ANALOG_WRITE: u8 = 0x52;
const BYTE_TIMEOUT: u32 = 1000;

pub struct ButtonA {
    pin: Pin<pin::bank0::Gpio12, pin::PullUpInput>,
}

impl ButtonA {
    pub fn new(pin: Pin<Gpio12, <Gpio12 as PinId>::Reset>) -> Self {
        ButtonA {
            pin: pin.into_pull_up_input(),
        }
    }

    pub fn pressed(&self) -> bool {
        self.pin.is_low().unwrap()
    }
}

#[derive(Debug, Clone)]
pub enum Esp32Error {
    Unknown,
    NoStartCmd,
    WaitForByteTimeout,
    ErrCmd,
    UnexpectedByte,
    ErrorCode(u8),
}

impl core::fmt::Display for Esp32Error {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "{:?}", self)
    }
}

pub struct Esp32 {
    spi: Spi<pac::SPI0>,
    cs: Pin<Gpio7, pin::PushPullOutput>,
    gpio2: Pin<Gpio2, pin::PushPullOutput>,
    // ack: Pin<Gpio10, pin::BusKeepInput>,
    ack: Pin<Gpio10, pin::PullDownInput>,
}

impl Esp32 {
    pub fn new(
        resets: &mut pac::RESETS,
        spi_device: pac::SPI0,
        mut cs: Pin<Gpio7, pin::PushPullOutput>,
        ack: Pin<Gpio10, pin::PullDownInput>,
        mut gpio2: Pin<Gpio2, pin::PushPullOutput>,
        mut resetn: Pin<Gpio11, pin::PushPullOutput>,
        delay: &mut cortex_m::delay::Delay,
    ) -> Self {
        let mut spi = Spi::new(spi_device);
        spi.init(resets, 8_000_000);
        spi.set_dummy_data(0xFF);

        cs.set_high().unwrap();

        // Reset
        info!("Resetting ESP32");
        gpio2.set_high().unwrap();
        cs.set_high().unwrap();
        resetn.set_low().unwrap();
        delay.delay_ms(10);
        resetn.set_high().unwrap();
        delay.delay_ms(1750);

        Esp32 { spi, cs, ack, gpio2 }
    }

    fn esp_select(&mut self) {
        self.cs.set_low().unwrap();
    }

    fn esp_deselect(&mut self) {
        self.cs.set_high().unwrap();
    }

    fn wait_for_esp_ready(&self) {
        while self.ack.is_high().unwrap() {
        }
    }

    fn wait_for_esp_ack(&self) {
        while self.ack.is_low().unwrap() {
        }
    }

    fn wait_for_esp_select(&mut self) {
        self.wait_for_esp_ready();
        self.esp_select();
        self.wait_for_esp_ack();
    }

    fn read_and_check_byte(&mut self, expected: u8) -> Result<(), Esp32Error> {
        let b = self.spi.read_byte();
        if b == expected { Ok(()) } else { Err(Esp32Error::UnexpectedByte)}
    }

    fn wait_for_byte(&mut self, expected: u8) -> Result<(), Esp32Error> {
        for _ in 0..BYTE_TIMEOUT {
            let b = self.spi.read_byte();
            if b == expected {
                return Ok(())
            } else if b == ERR_CMD {
                return Err(Esp32Error::ErrCmd)
            }
        }
        Err(Esp32Error::WaitForByteTimeout)
    }

    fn send_cmd(&mut self, cmd: u8, num_param: u8) {
        if num_param == 0 {
            self.spi.write(
                &[START_CMD, cmd & !REPLY_FLAG, 0, END_CMD]);
            } else {
            self.spi.write(
                &[START_CMD, cmd & !REPLY_FLAG, num_param]);
        }
    }

    fn send_param(&mut self, param: &[u8], last_param: bool) {
        assert!(param.len() < 256);
        self.spi.write_byte(param.len() as u8);
        self.spi.write(param);
        self.spi.write_byte(END_CMD);
    }

    fn wait_response_cmd1(&mut self, cmd: u8, ) -> Result<u8, Esp32Error> {
        info!("wait_for_byte {:02X}", START_CMD);
        self.wait_for_byte(START_CMD)?;
        info!("read_and_check_byte {:02X}", cmd | REPLY_FLAG);
        self.read_and_check_byte(cmd | REPLY_FLAG)?;
        info!("read_and_check_byte {}", 1);
        self.read_and_check_byte(1)?;  // num_param
        info!("read_and_check_byte {}", 1);
        self.read_and_check_byte(1)?;  // param_len_out
        info!("read");
        Ok(self.spi.read_byte())
    }

    pub fn analog_write(&mut self, pin: u8, value: u8) -> Result<(), Esp32Error> {
        info!("wait_for_esp_select");
        self.wait_for_esp_select();

        info!("send_cmd {}", SET_ANALOG_WRITE);
        self.send_cmd(SET_ANALOG_WRITE, 2);
        info!("send_param {pin}");
        self.send_param(&[pin], false);
        info!("send_param {value}");
        self.send_param(&[value], true);

        info!("read");
        self.spi.read_byte();

        info!("esp_deselect");
        self.esp_deselect();
        info!("wait_for_sp_select");
        self.wait_for_esp_select();

        info!("wait_responses_cmd1 {}", SET_ANALOG_WRITE);
        let error = self.wait_response_cmd1(SET_ANALOG_WRITE)?;
        if error == 1 {
            Ok(())
        } else {
            Err(Esp32Error::ErrorCode(error))
        }
    }
}
