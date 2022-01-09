use embedded_hal::{
    digital::v2::{InputPin as _, OutputPin as _},
    spi::FullDuplex
};
use embedded_time::{fixed_point::FixedPoint as _, rate::Extensions as _};
use log::info;
use rp2040_hal::{
    gpio::{
        pin,
        pin::bank0::{Gpio7, Gpio10, Gpio12},
        pin::PinId,
        Pin,
    },
    pac, spi,
    spi::Spi,
};

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
    spi: Spi<spi::Enabled, pac::SPI0, 8>,
    cs: Pin<Gpio7, pin::PushPullOutput>,
    ack: Pin<Gpio10, pin::PullDownInput>,
}

impl Esp32 {
    pub fn new(
        mut resets: pac::RESETS,
        spi_device: pac::SPI0,
        mut cs: Pin<Gpio7, pin::PushPullOutput>,
        ack: Pin<Gpio10, pin::PullDownInput>,
    ) -> Self {
        let spi = Spi::<_, _, 8>::new(spi_device).init(
            &mut resets,
            125_000_000u32.Hz(),
            8_000_000u32.Hz(),
            &embedded_hal::spi::MODE_0,
        );

        cs.set_high().unwrap();

        Esp32 { spi, cs, ack}
    }

    fn esp_select(&mut self) {
        self.cs.set_high().unwrap();
    }

    fn esp_deselect(&mut self) {
        self.cs.set_low().unwrap();
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
        info!("wait_for_esp_ready");
        self.wait_for_esp_ready();
        // info!("esp_select");
        self.esp_select();
        // info!("wait_for_esp_ack");
        self.wait_for_esp_ack();
        info!("finished wait_for_esp_select");
    }

    fn read_and_check_byte(&mut self, expected: u8) -> Result<(), Esp32Error> {
        let b = self.spi.read().unwrap();
        if b == expected { Ok(()) } else { Err(Esp32Error::UnexpectedByte)}
    }

    fn wait_for_byte(&mut self, expected: u8) -> Result<(), Esp32Error> {
        for _ in 0..BYTE_TIMEOUT {
            let b = self.spi.read().unwrap();
            if b == expected {
                return Ok(())
            } else if b == ERR_CMD {
                return Err(Esp32Error::ErrCmd)
            }
        }
        Err(Esp32Error::WaitForByteTimeout)
    }

    fn send_cmd(&mut self, cmd: u8, num_param: u8) {
        self.spi.send(START_CMD).ok();
        self.spi.send(cmd & !REPLY_FLAG).ok();
        self.spi.send(num_param).ok();

        if num_param == 0 {
            self.spi.send(END_CMD).ok();
        }
    }

    fn send_param(&mut self, param: &[u8], last_param: bool) {
        assert!(param.len() < 256);
        self.spi.send(param.len() as u8).ok();

        for b in param {
            self.spi.send(*b).ok();
        }

        if last_param {
            self.spi.send(END_CMD).ok();
        }
    }

    fn wait_response_cmd1(&mut self, cmd: u8, ) -> Result<u8, Esp32Error> {
        self.wait_for_byte(START_CMD)?;
        self.read_and_check_byte(cmd | REPLY_FLAG)?;
        self.read_and_check_byte(1)?;  // num_param
        self.read_and_check_byte(1)?;  // param_len_out
        Ok(self.spi.read().unwrap())
    }

    pub fn analog_write(&mut self, pin: u8, value: u8) -> Result<(), Esp32Error> {
        info!("wait_for_esp_select");
        self.wait_for_esp_select();

        info!("send_cmd");
        self.send_cmd(SET_ANALOG_WRITE, 2);
        self.send_param(&[pin], false);
        self.send_param(&[value], true);

        self.spi.read().ok();

        self.esp_deselect();
        self.wait_for_esp_select();

        let error = self.wait_response_cmd1(SET_ANALOG_WRITE)?;
        if error == 1 {
            Ok(())
        } else {
            Err((Esp32Error::ErrorCode(error)))
        }
    }
}
