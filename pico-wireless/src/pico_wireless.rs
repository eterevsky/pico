use embedded_hal::{
    digital::v2::{InputPin as _, OutputPin as _},
    // spi::FullDuplex
};
// use embedded_time::{fixed_point::FixedPoint as _, rate::Extensions as _};
use log::info;
use rp2040_hal::{
    gpio::{
        pin,
        pin::bank0::{Gpio2, Gpio7, Gpio10, Gpio11, Gpio12},
        pin::PinId,
        Pin,
    },
    pac,
};
use crate::blocking_spi::Spi;

const START_CMD: u8 = 0xE0;
const END_CMD: u8 = 0xEE;
const ERR_CMD: u8 = 0xEF;
const DUMMY_DATA: u8 = 0xFF;

const REPLY_FLAG: u8 = 1 << 7;

const BYTE_TIMEOUT: u32 = 5000;

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
    IncorrectEncryptionType,
    ErrorCode(u8),
}

impl core::fmt::Display for Esp32Error {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "{:?}", self)
    }
}

enum CmdResponseType {
    Normal,
    Cmd,
    Data8,
    Data16,
}

#[repr(u8)]
enum Esp32Command {
    SetPassphrase = 0x11,
    GetConnStatus = 0x20,
    GetIpAddr = 0x21,
    ScanNetworks = 0x27,
    GetIdxRssi = 0x32,
    GetIdxEnct = 0x33,
    GetIdxBssid = 0x3c,
    GetIdxChannel = 0x3d,
    SetAnalogWrite = 0x52,
}

#[repr(u8)]
#[derive(Debug, Clone, Copy)]
pub enum EncryptionType {
    Tkip = 2,
    Ccmp = 4,
    Wep = 5,
    None = 7,
    Auto = 8,
    Unknown = 255,
}

pub struct Esp32 {
    spi: Spi<pac::SPI0>,
    cs: Pin<Gpio7, pin::PushPullOutput>,
    gpio2: Pin<Gpio2, pin::PushPullOutput>,
    ack: Pin<Gpio10, pin::PullDownInput>,
    command_length: u32,
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
        system_clock_freq: u32,
    ) -> Self {
        let mut spi = Spi::new(spi_device);
        spi.init(resets, 8_000_000, system_clock_freq);
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

        Esp32 { spi, cs, ack, gpio2, command_length: 0 }
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
        while self.ack.is_low().unwrap() { }
    }

    fn wait_for_esp_select(&mut self) {
        self.wait_for_esp_ready();
        self.esp_select();
        self.wait_for_esp_ack();
    }

    fn read_and_check_byte(&mut self, expected: u8) -> Result<(), Esp32Error> {
        // info!("read_and_check_byte({expected})");
        let b = self.spi.read_byte();
        if b == expected { Ok(()) } else { Err(Esp32Error::UnexpectedByte)}
    }

    fn wait_for_byte(&mut self, expected: u8) -> Result<(), Esp32Error> {
        // info!("wait_for_byte({expected})");
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

    fn start_cmd(&mut self, cmd: Esp32Command, num_param: u8) {
        self.wait_for_esp_select();

        // info!("send_cmd({cmd}, {num_param})");
        self.spi.write(&[START_CMD, (cmd as u8) & !REPLY_FLAG, num_param]);
        self.command_length += 3;
    }

    fn send_param(&mut self, param: &[u8]) {
        assert!(param.len() < 256);
        // info!("send_param({param:?})");
        self.spi.write_byte(param.len() as u8);
        self.spi.write(param);
        self.command_length += param.len() as u32 + 1;
    }

    fn end_cmd(&mut self) {
        // info!("end_cmd {command_length}");
        self.spi.write_byte(END_CMD);
        self.command_length += 1;

        while self.command_length % 4 != 0 {
            self.spi.read_byte();
            self.command_length += 1;
        }

        self.command_length = 0;
        self.esp_deselect();
    }

    fn wait_response_cmd_u8(&mut self, cmd: Esp32Command) -> Result<u8, Esp32Error> {
        self.wait_for_esp_select();

        self.wait_for_byte(START_CMD)?;
        self.read_and_check_byte(cmd as u8 | REPLY_FLAG)?;
        self.read_and_check_byte(1)?;  // num_param
        self.read_and_check_byte(1)?;  // param_len_out
        let response = self.spi.read_byte();  // param_out
        self.read_and_check_byte(END_CMD)?;

        self.esp_deselect();

        Ok(response)
    }

    fn wait_response_cmd_i32(&mut self, cmd: Esp32Command) -> Result<i32, Esp32Error> {
        self.wait_for_esp_select();

        self.wait_for_byte(START_CMD)?;
        self.read_and_check_byte(cmd as u8 | REPLY_FLAG)?;
        self.read_and_check_byte(1)?;  // num_param
        self.read_and_check_byte(4)?;  // param_len_out
        let mut response_bytes = [0; 4];
        self.spi.read_bytes(&mut response_bytes);  // param_out
        self.read_and_check_byte(END_CMD)?;

        self.esp_deselect();

        Ok(i32::from_ne_bytes(response_bytes))
    }

    pub fn analog_write(&mut self, pin: u8, value: u8) -> Result<(), Esp32Error> {
        self.start_cmd(Esp32Command::SetAnalogWrite, 2);
        self.send_param(&[pin]);
        self.send_param(&[value]);

        self.end_cmd();

        let error = self.wait_response_cmd_u8(Esp32Command::SetAnalogWrite)?;

        if error == 1 {
            Ok(())
        } else {
            Err(Esp32Error::ErrorCode(error))
        }
    }

    pub fn scan_networks(&mut self, ssids: &mut [u8], offsets: &mut [usize]) -> Result<usize, Esp32Error> {
        self.start_cmd(Esp32Command::ScanNetworks, 0);
        self.end_cmd();

        self.wait_for_esp_select();
        let mut offset = 0;

        self.wait_for_byte(START_CMD)?;
        self.read_and_check_byte(Esp32Command::ScanNetworks as u8 | REPLY_FLAG)?;
        let num_params = self.spi.read_byte() as usize;
        let mut saved_params = num_params;
        let mut skipping_the_rest = false;

        for index in 0..num_params as usize {
            let param_len = self.spi.read_byte() as usize;

            let end_offset = offset + param_len;
            if !skipping_the_rest && index < offsets.len() - 1 && end_offset <= ssids.len() {
                self.spi.read_bytes(&mut ssids[offset..end_offset]);
                offsets[index] = offset;
                offsets[index + 1] = end_offset;
                offset = end_offset;
                saved_params = index + 1;
            } else {
                skipping_the_rest = true;
                self.spi.skip_bytes(param_len);
            }
        }

        self.esp_deselect();
        Ok(saved_params)

    }

    pub fn get_channel(&mut self, idx: u8) -> Result<u8, Esp32Error> {
        self.start_cmd(Esp32Command::GetIdxChannel, 1);
        self.send_param(&[idx]);
        self.end_cmd();

        self.wait_response_cmd_u8(Esp32Command::GetIdxChannel)
    }

    pub fn get_rssi(&mut self, idx: u8) -> Result<i32, Esp32Error> {
        self.start_cmd(Esp32Command::GetIdxRssi, 1);
        self.send_param(&[idx]);
        self.end_cmd();

        self.wait_response_cmd_i32(Esp32Command::GetIdxRssi)
    }

    pub fn get_encryption_type(&mut self, idx: u8) -> Result<EncryptionType, Esp32Error> {
        self.start_cmd(Esp32Command::GetIdxEnct, 1);
        self.send_param(&[idx]);
        self.end_cmd();

        let response = self.wait_response_cmd_u8(Esp32Command::GetIdxEnct)?;

        // It sucks, but looks like there is no way to directly convert a number to an enum with
        // the same value numbers
        match response {
            2 => Ok(EncryptionType::Tkip),
            4 => Ok(EncryptionType::Ccmp),
            5 => Ok(EncryptionType::Wep),
            7 => Ok(EncryptionType::None),
            8 => Ok(EncryptionType::Auto),
            255 => Ok(EncryptionType::Unknown),
            _ => Err(Esp32Error::IncorrectEncryptionType)
        }
    }

    pub fn wifi_set_passphrase(&mut self, ssid: &str, passphrase: &str) -> Result<(), Esp32Error> {
        self.start_cmd(Esp32Command::SetPassphrase, 2);
        self.send_param(ssid.as_bytes());
        self.send_param(passphrase.as_bytes());
        self.end_cmd();

        let response = self.wait_response_cmd_u8(Esp32Command::SetPassphrase)?;

        info!("set_passphrase {response}");

        Ok(())
    }

    pub fn get_conn_status(&mut self) -> Result<u8, Esp32Error> {
        self.start_cmd(Esp32Command::GetConnStatus, 0);
        self.end_cmd();

        self.wait_response_cmd_u8(Esp32Command::GetConnStatus)
    }
}
