//! Board-specific configuration and types for the PCB

use crate::FifteenPinOutput;
use crate::Pins;
use embassy_rp::Peri;

pub type OutputStart = Peri<'static, embassy_rp::peripherals::PIN_9>;
pub type OutputCoin = Peri<'static, embassy_rp::peripherals::PIN_8>;
pub type OutputB1 = Peri<'static, embassy_rp::peripherals::PIN_4>;
pub type OutputB2 = Peri<'static, embassy_rp::peripherals::PIN_5>;
pub type OutputB3 = Peri<'static, embassy_rp::peripherals::PIN_6>;
pub type OutputB4 = Peri<'static, embassy_rp::peripherals::PIN_7>;
pub type OutputB5 = Peri<'static, embassy_rp::peripherals::PIN_10>;
pub type OutputB6 = Peri<'static, embassy_rp::peripherals::PIN_11>;
pub type OutputUp = Peri<'static, embassy_rp::peripherals::PIN_0>;
pub type OutputDown = Peri<'static, embassy_rp::peripherals::PIN_1>;
pub type OutputLeft = Peri<'static, embassy_rp::peripherals::PIN_2>;
pub type OutputRight = Peri<'static, embassy_rp::peripherals::PIN_3>;

pub type Selector = Peri<'static, embassy_rp::peripherals::PIN_17>;
pub type PinSixDirection = Peri<'static, embassy_rp::peripherals::PIN_22>;
pub type Soe = Peri<'static, embassy_rp::peripherals::PIN_23>;
pub type Led = Peri<'static, embassy_rp::peripherals::PIN_16>;

pub type InLatchPower = Peri<'static, embassy_rp::peripherals::PIN_20>;
pub type InPowerSelect = Peri<'static, embassy_rp::peripherals::PIN_21>;
pub type InFire1ClockBA = Peri<'static, embassy_rp::peripherals::PIN_24>;
pub type InUpZ = Peri<'static, embassy_rp::peripherals::PIN_25>;
pub type InDownY = Peri<'static, embassy_rp::peripherals::PIN_26>;
pub type InLeftGnd = Peri<'static, embassy_rp::peripherals::PIN_27>;
pub type InRightModeGnd = Peri<'static, embassy_rp::peripherals::PIN_28>;
pub type InFire2DataCStart = Peri<'static, embassy_rp::peripherals::PIN_29>;

pub fn get_pins(pins: Pins) -> crate::BoardPins {
    let fpo = FifteenPinOutput {
        output_start: pins.pin9,
        output_coin: pins.pin8,  // Coin
        output_b1: pins.pin4,    // 1
        output_b2: pins.pin5,    // 2
        output_b3: pins.pin6,    // 3
        output_b4: pins.pin7,    // 4
        output_b5: pins.pin10,   // 5
        output_b6: pins.pin11,   // 6
        output_up: pins.pin0,    // Up
        output_down: pins.pin1,  // Down
        output_left: pins.pin2,  // Left
        output_right: pins.pin3, // Right
    };

    (
        pins.pin17, // Selector
        pins.pin22, // Whether pin 6 on a level shifter is in or out
        pins.pin23, // Shifter's Output Enable
        pins.pin20, // in_latch_power
        pins.pin21, // in_power_select
        pins.pin24, // in_fire1_clock_b_a
        pins.pin25, // in_up_z
        pins.pin26, // in_down_y
        pins.pin27, // in_left_gnd
        pins.pin28, // in_right_mode_gnd
        pins.pin29, // in_fire2_data_c_start
        pins.pin16, // LED
        fpo,
    )
}
