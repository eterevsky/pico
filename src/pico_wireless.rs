use embedded_hal::digital::v2::InputPin as _;
use rp2040_hal::{
    gpio::{
        pin, pin::PinId, pin::bank0::Gpio12, Pin
    }
};

// pub const ESP_LED_R: u8 = 25;
// pub const ESP_LED_G: u8 = 26;
// pub const ESP_LED_B: u8 = 27;

// pub const ESP_SD_DETECT: u8 = 15;

pub struct ButtonA {
    pin: Pin<pin::bank0::Gpio12, pin::PullUpInput>,
}

impl ButtonA {
    pub fn new(pin: Pin<Gpio12, <Gpio12 as PinId>::Reset>) -> Self {
        ButtonA {
            pin: pin.into_pull_up_input()
        }
    }

    pub fn pressed(&self) -> bool {
        self.pin.is_low().unwrap()
    }
}