//! Board-specific configuration and types for the reference Pi Pico

use crate::FifteenPinOutput;
use crate::Pins;
use defmt::*;
use defmt_rtt as _;
use embassy_rp::Peri;

pub type OutputStart = Peri<'static, embassy_rp::peripherals::PIN_11>;
pub type OutputCoin = Peri<'static, embassy_rp::peripherals::PIN_12>;
pub type OutputB1 = Peri<'static, embassy_rp::peripherals::PIN_7>;
pub type OutputB2 = Peri<'static, embassy_rp::peripherals::PIN_8>;
pub type OutputB3 = Peri<'static, embassy_rp::peripherals::PIN_9>;
pub type OutputB4 = Peri<'static, embassy_rp::peripherals::PIN_10>;
pub type OutputB5 = Peri<'static, embassy_rp::peripherals::PIN_13>;
pub type OutputB6 = Peri<'static, embassy_rp::peripherals::PIN_14>;
pub type OutputUp = Peri<'static, embassy_rp::peripherals::PIN_3>;
pub type OutputDown = Peri<'static, embassy_rp::peripherals::PIN_4>;
pub type OutputLeft = Peri<'static, embassy_rp::peripherals::PIN_5>;
pub type OutputRight = Peri<'static, embassy_rp::peripherals::PIN_6>;

pub type Selector = Peri<'static, embassy_rp::peripherals::PIN_2>;
pub type PinSixDirection = Peri<'static, embassy_rp::peripherals::PIN_27>;
pub type Soe = Peri<'static, embassy_rp::peripherals::PIN_26>;
pub type Led = Peri<'static, embassy_rp::peripherals::PIN_0>;

pub type InLatchPower = Peri<'static, embassy_rp::peripherals::PIN_15>;
pub type InPowerSelect = Peri<'static, embassy_rp::peripherals::PIN_16>;
pub type InFire1ClockBA = Peri<'static, embassy_rp::peripherals::PIN_17>;
pub type InUpZ = Peri<'static, embassy_rp::peripherals::PIN_18>;
pub type InDownY = Peri<'static, embassy_rp::peripherals::PIN_19>;
pub type InLeftGnd = Peri<'static, embassy_rp::peripherals::PIN_20>;
pub type InRightModeGnd = Peri<'static, embassy_rp::peripherals::PIN_21>;
pub type InFire2DataCStart = Peri<'static, embassy_rp::peripherals::PIN_22>;

pub fn get_pins(pins: Pins) -> crate::BoardPins {
    let fpo = FifteenPinOutput {
        output_start: pins.pin11,
        output_coin: pins.pin12, // Coin
        output_b1: pins.pin7,    // 1
        output_b2: pins.pin8,    // 2
        output_b3: pins.pin9,    // 3
        output_b4: pins.pin10,   // 4
        output_b5: pins.pin13,   // 5
        output_b6: pins.pin14,   // 6
        output_up: pins.pin3,    // Up
        output_down: pins.pin4,  // Down
        output_left: pins.pin5,  // Left
        output_right: pins.pin6, // Right
    };

    (
        pins.pin2,  // Selector
        pins.pin27, // Whether pin 6 on a level shifter is in or out
        pins.pin26, // Shifter's Output Enable
        pins.pin15, // in_latch_power
        pins.pin16, // in_power_select
        pins.pin17, // in_fire1_clock_b_a
        pins.pin18, // in_up_z
        pins.pin19, // in_down_y
        pins.pin20, // in_left_gnd
        pins.pin21, // in_right_mode_gnd
        pins.pin22, // in_fire2_data_c_start
        pins.pin0,  // LED
        fpo,
    )
}
