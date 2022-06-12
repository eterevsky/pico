use core::fmt;
use embedded_hal::digital::v2::{InputPin as _, OutputPin as _};
use log::info;
use rp2040_hal::{
    gpio::{
        pin,
        pin::bank0::{Gpio10, Gpio11, Gpio12, Gpio2, Gpio7},
        pin::PinId,
        Pin,
    },
    pac,
};

use crate::blocking_spi::Spi;
use crate::buffer::{Buffer, BufferError, GenBuffer};

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
    UnexpectedEncryptionType(u8),
    UnexpectedStatus(u8),
    ErrorCode(u8),
    ResponseBufferError(BufferError),
    WrongNumberOfResponseParams,
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
    StartClientTcp = 0x2d,
    StopClientTcp = 0x2e,
    GetIdxRssi = 0x32,
    GetIdxEnct = 0x33,
    SendDataUdp = 0x39,
    GetIdxBssid = 0x3c,
    GetIdxChannel = 0x3d,
    GetSocket = 0x3f,
    InsertDataBuf = 0x46,
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

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ConnectionStatus {
    Idle = 0,
    NoSsidAvail = 1,
    ScanCompleted = 2,
    Connected = 3,
    ConnectFailed = 4,
    ConnectionLost = 5,
    Disconnected = 6,
    ApListening = 7,
    ApConnected = 8,
    ApFailed = 9,
    NoShield = 255,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug)]
pub enum ProtocolMode {
    Tcp = 0,
    Udp = 1,
    Tls = 2,
    UdpMulticast = 3,
    TlsBearSsl = 4,
}

#[derive(Debug, Clone, Copy)]
pub struct IpV4([u8; 4]);

impl IpV4 {
    pub fn from_slice(data: &[u8]) -> Self {
        let mut addr = [0; 4];
        addr.clone_from_slice(data);
        IpV4(addr)
    }

    fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Display for IpV4 {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}.{}.{}.{}", self.0[0], self.0[1], self.0[2], self.0[3])
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Socket(u8);

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
        delay.delay_ms(750);

        Esp32 {
            spi,
            cs,
            ack,
            gpio2,
            command_length: 0,
        }
    }

    fn esp_select(&mut self) {
        self.cs.set_low().unwrap();
    }

    fn esp_deselect(&mut self) {
        self.cs.set_high().unwrap();
    }

    fn wait_for_esp_ready(&self) {
        while self.ack.is_high().unwrap() {}
    }

    fn wait_for_esp_ack(&self) {
        while self.ack.is_low().unwrap() {}
    }

    fn wait_for_esp_select(&mut self) {
        self.wait_for_esp_ready();
        self.esp_select();
        self.wait_for_esp_ack();
    }

    fn read_and_check_byte(&mut self, expected: u8) -> Result<(), Esp32Error> {
        // info!("read_and_check_byte({expected})");
        let b = self.spi.read_byte();
        if b == expected {
            Ok(())
        } else {
            Err(Esp32Error::UnexpectedByte)
        }
    }

    fn wait_for_byte(&mut self, expected: u8) -> Result<(), Esp32Error> {
        for _ in 0..BYTE_TIMEOUT {
            let b = self.spi.read_byte();
            if b == expected {
                return Ok(());
            } else if b == ERR_CMD {
                return Err(Esp32Error::ErrCmd);
            }
        }
        Err(Esp32Error::WaitForByteTimeout)
    }

    fn start_cmd(&mut self, cmd: Esp32Command, num_param: u8) {
        self.wait_for_esp_select();

        self.spi
            .write(&[START_CMD, (cmd as u8) & !REPLY_FLAG, num_param]);
        self.command_length += 3;
    }

    fn send_param(&mut self, param: &[u8]) {
        assert!(param.len() < 256);
        self.spi.write_byte(param.len() as u8);
        self.spi.write(param);
        self.command_length += param.len() as u32 + 1;
    }

    fn send_buffer(&mut self, param: &[u8]) {
        self.spi.write_byte((param.len() / 256) as u8);
        self.spi.write_byte((param.len() % 256) as u8);
        self.spi.write(param);
        self.command_length += param.len() as u32 + 1;
    }

    fn end_cmd(&mut self) {
        self.spi.write_byte(END_CMD);
        self.command_length += 1;

        while self.command_length % 4 != 0 {
            self.spi.read_byte();
            self.command_length += 1;
        }

        self.command_length = 0;
        self.esp_deselect();
    }

    fn get_response_impl(
        &mut self,
        cmd: Esp32Command,
        buffer: &mut dyn GenBuffer,
        expected_num_params: Option<usize>,
    ) -> Result<(), Esp32Error> {
        self.wait_for_byte(START_CMD)?;
        self.read_and_check_byte(cmd as u8 | REPLY_FLAG)?;

        let num_params = self.spi.read_byte();

        if expected_num_params.is_some() && num_params as usize != expected_num_params.unwrap() {
            return Err(Esp32Error::WrongNumberOfResponseParams);
        }

        for _ in 0..num_params {
            let field_size = self.spi.read_byte();
            let field = buffer
                .add_field(field_size as usize)
                .map_err(|e| Esp32Error::ResponseBufferError(e))?;
            self.spi.read_bytes(field);
        }

        self.read_and_check_byte(END_CMD)
    }

    fn get_response(
        &mut self,
        cmd: Esp32Command,
        buffer: &mut dyn GenBuffer,
        expected_num_params: Option<usize>,
    ) -> Result<(), Esp32Error> {
        self.wait_for_esp_select();
        let response = self.get_response_impl(cmd, buffer, expected_num_params);
        self.esp_deselect();

        response
    }

    fn get_response_u8(&mut self, cmd: Esp32Command) -> Result<u8, Esp32Error> {
        let mut buffer: Buffer<1, 2> = Buffer::new();
        self.get_response(cmd, &mut buffer, Some(1))?;
        buffer
            .field_as_u8(0)
            .map_err(|e| Esp32Error::ResponseBufferError(e))
    }

    fn get_response_i32(&mut self, cmd: Esp32Command) -> Result<i32, Esp32Error> {
        let mut buffer: Buffer<4, 2> = Buffer::new();
        self.get_response(cmd, &mut buffer, Some(1))?;
        buffer
            .field_as_i32(0)
            .map_err(|e| Esp32Error::ResponseBufferError(e))
    }

    fn check_response_status(&mut self, command: Esp32Command) -> Result<(), Esp32Error> {
        let status = self.get_response_u8(command)?;

        if status == 1 {
            Ok(())
        } else {
            Err(Esp32Error::ErrorCode(status))
        }

    }

    pub fn analog_write(&mut self, pin: u8, value: u8) -> Result<(), Esp32Error> {
        self.start_cmd(Esp32Command::SetAnalogWrite, 2);
        self.send_param(&[pin]);
        self.send_param(&[value]);
        self.end_cmd();

        self.check_response_status(Esp32Command::SetAnalogWrite)
    }

    pub fn scan_networks(&mut self, ssids: &mut dyn GenBuffer) -> Result<(), Esp32Error> {
        self.start_cmd(Esp32Command::ScanNetworks, 0);
        self.end_cmd();

        self.get_response(Esp32Command::ScanNetworks, ssids, None)
    }

    pub fn get_channel(&mut self, idx: u8) -> Result<u8, Esp32Error> {
        self.start_cmd(Esp32Command::GetIdxChannel, 1);
        self.send_param(&[idx]);
        self.end_cmd();

        self.get_response_u8(Esp32Command::GetIdxChannel)
    }

    pub fn get_rssi(&mut self, idx: u8) -> Result<i32, Esp32Error> {
        self.start_cmd(Esp32Command::GetIdxRssi, 1);
        self.send_param(&[idx]);
        self.end_cmd();

        self.get_response_i32(Esp32Command::GetIdxRssi)
    }

    pub fn get_encryption_type(&mut self, idx: u8) -> Result<EncryptionType, Esp32Error> {
        self.start_cmd(Esp32Command::GetIdxEnct, 1);
        self.send_param(&[idx]);
        self.end_cmd();

        let response = self.get_response_u8(Esp32Command::GetIdxEnct)?;

        // It sucks, but looks like there is no way to directly convert a number to an enum with
        // the same value numbers
        match response {
            2 => Ok(EncryptionType::Tkip),
            4 => Ok(EncryptionType::Ccmp),
            5 => Ok(EncryptionType::Wep),
            7 => Ok(EncryptionType::None),
            8 => Ok(EncryptionType::Auto),
            255 => Ok(EncryptionType::Unknown),
            _ => Err(Esp32Error::UnexpectedEncryptionType(response)),
        }
    }

    pub fn wifi_set_passphrase(&mut self, ssid: &str, passphrase: &str) -> Result<(), Esp32Error> {
        self.start_cmd(Esp32Command::SetPassphrase, 2);
        self.send_param(ssid.as_bytes());
        self.send_param(passphrase.as_bytes());
        self.end_cmd();

        self.check_response_status(Esp32Command::SetPassphrase)
    }

    pub fn get_conn_status(&mut self) -> Result<ConnectionStatus, Esp32Error> {
        self.start_cmd(Esp32Command::GetConnStatus, 0);
        self.end_cmd();

        let status = self.get_response_u8(Esp32Command::GetConnStatus)?;

        match status {
            0 => Ok(ConnectionStatus::Idle),
            1 => Ok(ConnectionStatus::NoSsidAvail),
            2 => Ok(ConnectionStatus::ScanCompleted),
            3 => Ok(ConnectionStatus::Connected),
            4 => Ok(ConnectionStatus::ConnectFailed),
            5 => Ok(ConnectionStatus::ConnectionLost),
            6 => Ok(ConnectionStatus::Disconnected),
            7 => Ok(ConnectionStatus::ApListening),
            8 => Ok(ConnectionStatus::ApConnected),
            9 => Ok(ConnectionStatus::ApFailed),
            255 => Ok(ConnectionStatus::NoShield),
            _ => Err(Esp32Error::UnexpectedStatus(status)),
        }
    }

    pub fn get_network_data(&mut self) -> Result<(IpV4, IpV4, IpV4), Esp32Error> {
        self.start_cmd(Esp32Command::GetIpAddr, 0);
        self.end_cmd();

        let mut buffer = Buffer::<12, 4>::new();
        self.get_response(Esp32Command::GetIpAddr, &mut buffer, Some(3))?;

        let addr_slice = buffer
            .field_as_slice_fixed(0, 4)
            .map_err(|e| Esp32Error::ResponseBufferError(e))?;
        let mask_slice = buffer
            .field_as_slice_fixed(1, 4)
            .map_err(|e| Esp32Error::ResponseBufferError(e))?;
        let gateway_slice = buffer
            .field_as_slice_fixed(2, 4)
            .map_err(|e| Esp32Error::ResponseBufferError(e))?;

        Ok((
            IpV4::from_slice(addr_slice),
            IpV4::from_slice(mask_slice),
            IpV4::from_slice(gateway_slice),
        ))
    }

    pub fn get_socket(&mut self) -> Result<Socket, Esp32Error> {
        self.start_cmd(Esp32Command::GetSocket, 0);
        self.end_cmd();

        let socket_id = self.get_response_u8(Esp32Command::GetSocket)?;

        Ok(Socket(socket_id))
    }

    pub fn start_client(
        &mut self,
        ip: IpV4,
        port: u16,
        sock: Socket,
        mode: ProtocolMode,
    ) -> Result<(), Esp32Error> {
        self.start_cmd(Esp32Command::StartClientTcp, 4);
        self.send_param(ip.as_bytes());
        self.send_param(&port.to_ne_bytes());
        self.send_param(&[sock.0]);
        self.send_param(&[mode as u8]);
        self.end_cmd();

        self.check_response_status(Esp32Command::StartClientTcp)
    }

    pub fn insert_data_buf(&mut self, sock: Socket, buf: &[u8]) -> Result<(), Esp32Error> {
        self.start_cmd(Esp32Command::InsertDataBuf, 2);
        self.send_param(&[sock.0]);
        self.send_buffer(buf);
        self.end_cmd();

        self.check_response_status(Esp32Command::InsertDataBuf)
    }

    pub fn send_data_udp(&mut self, sock: Socket) -> Result<(), Esp32Error> {
        self.start_cmd(Esp32Command::SendDataUdp, 1);
        self.send_param(&[sock.0]);
        self.end_cmd();

        self.check_response_status(Esp32Command::SendDataUdp)
    }
}
