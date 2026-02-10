//! Reads a CD32 or Mega Drive gamepad and breaks it out into 7 individual active-low GPIO pins
#![no_std]
#![no_main]

use defmt::*;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_rp::bind_interrupts;
use embassy_rp::clocks::clk_sys_freq;
use embassy_rp::gpio::{Input, Level, Output, OutputOpenDrain, Pull};
use embassy_rp::peripherals::PIO0;
use embassy_rp::pio::program::pio_asm;
use embassy_rp::pio::{Direction as PioDirection, InterruptHandler, Pio, StateMachine};
use embassy_rp::pio_programs::ws2812::{PioWs2812, PioWs2812Program};
use embassy_rp::{Peri, Peripherals};
use embassy_time::Instant;
use fixed::traits::ToFixed;
use panic_probe as _;

use smart_leds::RGB8;

// If using a generic two-button controller, map A+B to be start
const MAP_GENERIC_AB_TO_START: bool = true;

// What bitmask should we look for to let us send the coin button in CD32 mode?
// The bits are as follows:
// Pause, LB, RB, Green, Yellow, Red, Blue
// Set to 0 to disable the feature.
// Default: Pause + Green
const CD32_COIN_BITMASK: u32 = 0b1001000;

bind_interrupts!(struct Irqs {
    PIO0_IRQ_0 => InterruptHandler<PIO0>;
});

// Board-specific config
#[cfg(not(feature = "pi_pico"))]
mod pcb;
#[cfg(feature = "pi_pico")]
mod pico;

#[cfg(not(feature = "pi_pico"))]
use pcb as board;
#[cfg(feature = "pi_pico")]
use pico as board;

use board::*;

type BoardPins = (
    Selector,
    PinSixDirection,
    Soe,
    InLatchPower,
    InPowerSelect,
    InFire1ClockBA,
    InUpZ,
    InDownY,
    InLeftGnd,
    InRightModeGnd,
    InFire2DataCStart,
    Led,
    FifteenPinOutput,
);

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    info!("Program start");
    let p = embassy_rp::init(Default::default());
    let (pins, pio0, _pio1, dma0, ..) = split_peripherals(p);

    let platform_pins = get_pins(pins);

    let (
        selector,
        pin_six_direction,
        shifter_oe,
        in_latch_power,
        in_power_select,
        in_fire1_clock_b_a,
        in_up_z,
        in_down_y,
        in_left_x_gnd,
        in_right_mode_gnd,
        in_fire2_data_c_start,
        led,
        outputs,
    ) = platform_pins;

    let Pio {
        mut common,
        sm0,
        sm1,
        ..
    } = Pio::new(pio0, Irqs);

    // Set up LED
    let led_prog = PioWs2812Program::new(&mut common);
    let mut ws: PioWs2812<'_, PIO0, 1, 1> = PioWs2812::new(&mut common, sm1, dma0, led, &led_prog);

    // Connect GPIO2 to GND for Mega Drive/generic 9-pin. high or floating for CD32.
    let selector = Input::new(selector, Pull::Up);
    let use_cd32 = selector.is_low();

    let pin_six_direction_set = Output::new(pin_six_direction, Level::Low);
    let shifter_oe_set = Output::new(shifter_oe, Level::Low);

    if use_cd32 {
        info!("CD32 mode");

        let colour: RGB8 = (255, 0, 0).into();
        ws.write(&[colour]).await;

        let power = Output::new(in_power_select, Level::Low);
        let up = Input::new(in_up_z, Pull::None);
        let down = Input::new(in_down_y, Pull::None);
        let left = Input::new(in_left_x_gnd, Pull::None);
        let right = Input::new(in_right_mode_gnd, Pull::None);
        let latch = common.make_pio_pin(in_latch_power);
        let clock = common.make_pio_pin(in_fire1_clock_b_a);
        let data = common.make_pio_pin(in_fire2_data_c_start);

        let cd32_pins = CD32Pins {
            power,
            up,
            down,
            left,
            right,
            latch,
            clock,
            data,
        };

        run_cd32_code(
            pin_six_direction_set,
            shifter_oe_set,
            cd32_pins,
            outputs,
            common,
            sm0,
            _spawner,
        );
    } else {
        info!("Mega Drive mode");

        let colour: RGB8 = (0, 0, 255).into();
        ws.write(&[colour]).await;

        // First set up all the pins
        let power = Output::new(in_latch_power, Level::Low);
        let select = common.make_pio_pin(in_power_select);
        let b_a = common.make_pio_pin(in_fire1_clock_b_a);
        let up_z = common.make_pio_pin(in_up_z);
        let down_y = common.make_pio_pin(in_down_y);
        let left_x_gnd = common.make_pio_pin(in_left_x_gnd);
        let right_mode_gnd = common.make_pio_pin(in_right_mode_gnd);
        let c_start = common.make_pio_pin(in_fire2_data_c_start);

        let md_pins = MDPins {
            power,
            select,
            b_a,
            up_z,
            down_y,
            left_x_gnd,
            right_mode_gnd,
            c_start,
        };

        // Then run the actual code
        run_md_code(
            pin_six_direction_set,
            shifter_oe_set,
            md_pins,
            outputs,
            common,
            sm0,
            ws,
            _spawner,
        );
    }
}

fn run_cd32_code(
    pin_six_direction: Output<'static>,
    shifter_oe: Output<'static>,
    cd32_pins: CD32Pins,
    outputs: FifteenPinOutput,
    mut pio: embassy_rp::pio::Common<'static, PIO0>,
    mut sm0: StateMachine<'static, PIO0, 0>,
    spawner: Spawner,
) {
    let cd32_pins = setup_state_machine_cd32(&mut pio, &mut sm0, cd32_pins);

    let _ = spawner.spawn(read_cd32_loop(
        pin_six_direction,
        shifter_oe,
        sm0,
        cd32_pins,
        outputs,
    ));
}

fn setup_state_machine_cd32<'d>(
    pio: &mut embassy_rp::pio::Common<'d, PIO0>,
    sm0: &mut StateMachine<'d, PIO0, 0>,
    cd32_pins: CD32Pins,
) -> CD32Pins {
    let pio_multiplier = clk_sys_freq() / 140_000;
    info!("PIO multiplier is {}", pio_multiplier);

    let (power, up, down, left, right, set_pin, side_set_pin, in_pin) = cd32_pins.destructure();

    let read_cd32 = pio_asm!(
        ".side_set 1 opt",
        "begin:",
        "    set pins, 0    side 0 [2]",
        "    in  pins, 1    side 0 [1]", // Blue button (5)
        "    nop            side 1",
        "    in  pins, 1    side 0 [3]", // Red (4)
        "    nop            side 1",
        "    in  pins, 1    side 0 [3]", // Yellow (2)
        "    nop            side 1",
        "    in  pins, 1    side 0 [3]", // Green (1)
        "    nop            side 1",
        "    in  pins, 1    side 0 [3]", // Right front (6)
        "    nop            side 1",
        "    in  pins, 1    side 0 [3]", // Left front (3)
        "    nop            side 1",
        "    in  pins, 1    side 0 [3]", // Pause (Start)
        "    push noblock          [2]", // Move to the RX FIFO so the main code can deal with it
        "    set x, 31",
        "    nop                   [7]",
        "    nop                   [7]",
        "    nop                   [7]",
        "    nop                   [3]",
        "    set pins, 1    side 1 [3]",
        "wait_loop:",
        "    nop                   [7]",
        "    nop                   [7]",
        "    nop                   [7]",
        "    nop                   [3]",
        "    jmp x-- wait_loop",
        "    jmp begin",
    );

    let mut cfg = embassy_rp::pio::Config::default();
    cfg.use_program(&pio.load_program(&read_cd32.program), &[&side_set_pin]);
    cfg.set_set_pins(&[&set_pin]);
    cfg.set_in_pins(&[&in_pin]);
    cfg.clock_divider = pio_multiplier.to_fixed();
    //cfg.shift_in.auto_fill = true;
    //cfg.shift_in.direction = embassy_rp::pio::ShiftDirection::Left;

    sm0.set_pin_dirs(PioDirection::Out, &[&set_pin, &side_set_pin]);
    sm0.set_pin_dirs(PioDirection::In, &[&in_pin]);

    sm0.set_config(&cfg);
    sm0.set_enable(false);

    CD32Pins {
        power,
        up,
        down,
        left,
        right,
        latch: set_pin,
        clock: side_set_pin,
        data: in_pin,
    }
}

#[embassy_executor::task]
async fn read_cd32_loop(
    mut pin_six_direction: Output<'static>,
    mut shifter_oe: Output<'static>,
    mut sm0: StateMachine<'static, PIO0, 0>,
    cd32_pins: CD32Pins,
    outputs: FifteenPinOutput,
) {
    let (mut power, up, down, left, right, _, _, _) = cd32_pins.destructure();

    // Pin six is going out
    pin_six_direction.set_high();

    // Enable the output on the level shifter
    shifter_oe.set_low();

    // Enable the power to the controller
    power.set_high();

    // There absolutely must be a better way of doing this, but this works.
    let (
        pin_start,
        pin_coin,
        pin_b1,
        pin_b2,
        pin_b3,
        pin_b4,
        pin_b5,
        pin_b6,
        pin_up,
        pin_down,
        pin_left,
        pin_right,
    ) = outputs.destructure();
    // Initialise all the pins
    let mut pin_start = OutputOpenDrain::new(pin_start, Level::High);
    let mut pin_coin = OutputOpenDrain::new(pin_coin, Level::High);
    let mut pin_b1 = OutputOpenDrain::new(pin_b1, Level::High);
    let mut pin_b2 = OutputOpenDrain::new(pin_b2, Level::High);
    let mut pin_b3 = OutputOpenDrain::new(pin_b3, Level::High);
    let mut pin_b4 = OutputOpenDrain::new(pin_b4, Level::High);
    let mut pin_b5 = OutputOpenDrain::new(pin_b5, Level::High);
    let mut pin_b6 = OutputOpenDrain::new(pin_b6, Level::High);
    let mut pin_up = OutputOpenDrain::new(pin_up, Level::High);
    let mut pin_down = OutputOpenDrain::new(pin_down, Level::High);
    let mut pin_left = OutputOpenDrain::new(pin_left, Level::High);
    let mut pin_right = OutputOpenDrain::new(pin_right, Level::High);

    sm0.set_enable(true);

    loop {
        let rx = sm0.rx().wait_pull().await;
        // We only care about 7 bits of the 32 bits, make it a bit easier to deal with
        let mut our_data = rx >> 25;

        // Set CD32_COIN_BITMASK to customise what this matches
        pin_coin.set_level(get_pin_state(our_data & CD32_COIN_BITMASK));

        // If we've got the chord for the coin button, get rid of the component buttons so they don't fire as well
        if (our_data & CD32_COIN_BITMASK) == 0 {
            our_data |= CD32_COIN_BITMASK;
        }

        pin_start.set_level(get_pin_state(our_data & 0b1000000));
        pin_b1.set_level(get_pin_state(our_data & 0b0001000));
        pin_b2.set_level(get_pin_state(our_data & 0b0000100));
        pin_b3.set_level(get_pin_state(our_data & 0b0100000));
        pin_b4.set_level(get_pin_state(our_data & 0b0000010));
        pin_b5.set_level(get_pin_state(our_data & 0b0000001));
        pin_b6.set_level(get_pin_state(our_data & 0b0010000));

        if our_data != 127 {
            debug!("Got bits: {:#09b}", our_data);
            debug!("            S361245");
            debug!(
                "Buttons: 1 {} 2 {} 3 {} 4 {} 5 {} 6 {} start {} coin {}",
                pin_b1.is_set_high(),
                pin_b2.is_set_high(),
                pin_b3.is_set_high(),
                pin_b4.is_set_high(),
                pin_b5.is_set_high(),
                pin_b6.is_set_high(),
                pin_start.is_set_high(),
                pin_coin.is_set_high(),
            );
        }

        pin_up.set_level(Level::from(up.is_high()));
        pin_down.set_level(Level::from(down.is_high()));
        pin_left.set_level(Level::from(left.is_high()));
        pin_right.set_level(Level::from(right.is_high()));

        debug!(
            "Up: {}, Down: {}, Left: {}, Right: {}",
            pin_up.is_set_high(),
            pin_down.is_set_high(),
            pin_left.is_set_high(),
            pin_right.is_set_high(),
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn run_md_code(
    pin_six_direction: Output<'static>,
    shifter_oe: Output<'static>,
    md_pins: MDPins,
    outputs: FifteenPinOutput,
    mut pio: embassy_rp::pio::Common<'static, PIO0>,
    mut sm0: StateMachine<'static, PIO0, 0>,
    ws: PioWs2812<'static, PIO0, 1, 1>,
    spawner: Spawner,
) {
    let md_pins = setup_state_machine_md(&mut pio, &mut sm0, md_pins);

    let _ = spawner.spawn(read_md_loop(
        pin_six_direction,
        shifter_oe,
        sm0,
        md_pins,
        outputs,
        ws,
    ));
}

fn setup_state_machine_md<'d>(
    pio: &mut embassy_rp::pio::Common<'d, PIO0>,
    sm0: &mut StateMachine<'d, PIO0, 0>,
    md_pins: MDPins,
) -> MDPins {
    let pio_multiplier = clk_sys_freq() / 140_000;

    info!("PIO multiplier is {}", pio_multiplier);

    let (power, side_set_pin, b_a, up_z, down_y, left_x_gnd, right_mode_gnd, c_start) =
        md_pins.destructure();

    let read_md = pio_asm!(
    ".side_set 1",
    ".wrap_target",
    "    in  pins, 6        side 0", // Start/A/GND/GND
    "    in  pins, 6        side 1", // Normal
    "    nop                side 0",
    "    nop                side 1", // Same as first IN
    "    nop                side 0", // Same as second IN
    "    in  pins, 6        side 1", // All directions GND if 6-button
    "    in  pins, 6        side 0", // Mode/X/Y/Z
    "    push noblock       side 1", // Push off to FIFO
    "    set x, 31          side 1", // These delays get it down to around 140Hz polling
    "    nop           [6]  side 1", // This has been verified via oscilloscope rather than maths
    "    nop           [6]  side 1",
    "    nop           [6]  side 1",
    "    nop           [6]  side 1",
    "    nop           [6]  side 1",
    "    nop           [2]  side 1",
    "wait_loop:"                     // 30 cycles each iteration of this loop.
    "    nop           [6]  side 1",
    "    nop           [6]  side 1",
    "    nop           [6]  side 1",
    "    nop           [6]  side 1",
    "    nop                side 1",
    "    jmp x-- wait_loop  side 1",
    ".wrap",
    );

    let mut cfg = embassy_rp::pio::Config::default();
    cfg.use_program(&pio.load_program(&read_md.program), &[&side_set_pin]);
    cfg.set_in_pins(&[&b_a, &up_z, &down_y, &left_x_gnd, &right_mode_gnd, &c_start]);
    cfg.clock_divider = pio_multiplier.to_fixed();

    sm0.set_pin_dirs(PioDirection::Out, &[&side_set_pin]);
    sm0.set_pin_dirs(
        PioDirection::In,
        &[&b_a, &up_z, &down_y, &left_x_gnd, &right_mode_gnd, &c_start],
    );

    sm0.set_config(&cfg);
    sm0.set_enable(false);

    MDPins {
        power,
        select: side_set_pin,
        b_a,
        up_z,
        down_y,
        left_x_gnd,
        right_mode_gnd,
        c_start,
    }
}

#[embassy_executor::task]
async fn read_md_loop(
    mut pin_six_direction: Output<'static>,
    mut shifter_oe: Output<'static>,
    mut sm0: StateMachine<'static, PIO0, 0>,
    md_pins: MDPins,
    outputs: FifteenPinOutput,
    mut ws: PioWs2812<'static, PIO0, 1, 1>,
) -> ! {
    let (mut plus_five_volts, _, _, _, _, _, _, _) = md_pins.destructure();
    plus_five_volts.set_high();

    pin_six_direction.set_low();
    // Enable the output on the level shifter
    shifter_oe.set_low();

    // There absolutely must be a better way of doing this, but this works.
    let (
        pin_start,
        pin_coin,
        pin_b1,
        pin_b2,
        pin_b3,
        pin_b4,
        pin_b5,
        pin_b6,
        pin_up,
        pin_down,
        pin_left,
        pin_right,
    ) = outputs.destructure();

    // Initialise all the pins
    let mut pin_start = OutputOpenDrain::new(pin_start, Level::High);
    let mut pin_coin = OutputOpenDrain::new(pin_coin, Level::High);
    let mut pin_b1 = OutputOpenDrain::new(pin_b1, Level::High);
    let mut pin_b2 = OutputOpenDrain::new(pin_b2, Level::High);
    let mut pin_b3 = OutputOpenDrain::new(pin_b3, Level::High);
    let mut pin_b4 = OutputOpenDrain::new(pin_b4, Level::High);
    let mut pin_b5 = OutputOpenDrain::new(pin_b5, Level::High);
    let mut pin_b6 = OutputOpenDrain::new(pin_b6, Level::High);
    let mut pin_up = OutputOpenDrain::new(pin_up, Level::High);
    let mut pin_down = OutputOpenDrain::new(pin_down, Level::High);
    let mut pin_left = OutputOpenDrain::new(pin_left, Level::High);
    let mut pin_right = OutputOpenDrain::new(pin_right, Level::High);

    sm0.set_enable(true);

    let mut swap_rows = false;
    // Whilst theoretically using 0 for a null value might lead to clashes, it's irrelevant because the next polling loop will fix it
    let mut mode_pressed_time = 0;

    loop {
        let rx = sm0.rx().wait_pull().await;
        // We only care about 24 bits of the 32 bits, make it a bit easier to deal with
        let our_data = rx >> 8;

        if our_data & 0b000000011000011000000000 == 0 {
            debug!("Mega Drive");
            // Mega Drive controller
            // Three button Mega Drive controller buttons:
            // Data: 0bxxxxxxxxxxxxSGGDUACRLDUA
            pin_start.set_level(get_pin_state(our_data & 0b100000000000));
            pin_down.set_level(get_pin_state(our_data & 0b000100000000));
            pin_up.set_level(get_pin_state(our_data & 0b000010000000));

            if !swap_rows {
                pin_b1.set_level(get_pin_state(our_data & 0b000001000000));
                pin_b2.set_level(get_pin_state(our_data & 0b000000000001));
                pin_b3.set_level(get_pin_state(our_data & 0b000000100000));
            }
            pin_right.set_level(get_pin_state(our_data & 0b000000010000));
            pin_left.set_level(get_pin_state(our_data & 0b000000001000));

            if our_data & 0b000000011110000000000000 == 0 {
                // 6 button controller
                // Extra buttons: 0bxMXYZx
                let six_button_data = our_data >> 18;
                debug!("Got a 6 button");
                if swap_rows {
                    pin_b1.set_level(get_pin_state(six_button_data & 0b001000));
                    pin_b2.set_level(get_pin_state(six_button_data & 0b000100));
                    pin_b3.set_level(get_pin_state(six_button_data & 0b000010));
                    pin_b4.set_level(get_pin_state(our_data & 0b000001000000));
                    pin_b5.set_level(get_pin_state(our_data & 0b000000000001));
                    pin_b6.set_level(get_pin_state(our_data & 0b000000100000));
                } else {
                    pin_b4.set_level(get_pin_state(six_button_data & 0b001000));
                    pin_b5.set_level(get_pin_state(six_button_data & 0b000100));
                    pin_b6.set_level(get_pin_state(six_button_data & 0b000010));
                }

                pin_coin.set_level(get_pin_state(six_button_data & 0b010000));

                // Handle swapping around A/B/C and X/Y/Z
                if six_button_data & 0b010000 == 0 {
                    if mode_pressed_time == 0 {
                        mode_pressed_time = Instant::now().as_millis();
                    } else {
                        // Hold for three seconds to invert
                        if Instant::now().as_millis() - mode_pressed_time > 1_000 * 3 {
                            swap_rows = !swap_rows;
                            mode_pressed_time = 0;
                            debug!("Swapping rows, inverted now: {}", swap_rows);
                        }
                        let mut colour: RGB8 = (0, 0, 255).into();

                        if swap_rows {
                            colour = (255, 255, 0).into();
                        }

                        ws.write(&[colour]).await;
                    }
                } else {
                    mode_pressed_time = 0;
                }
            } else {
                // Tidy up buttons we don't have
                pin_b4.set_level(Level::High);
                pin_b5.set_level(Level::High);
                pin_b6.set_level(Level::High);

                // There are no rows to swap
                swap_rows = false;
                mode_pressed_time = 0;
            }
        } else {
            // Generic controller
            // 0b2RLDU1
            if MAP_GENERIC_AB_TO_START {
                pin_start.set_level(get_pin_state(our_data & 0b100001));
            } else {
                pin_start.set_level(Level::High);
            }

            pin_b1.set_level(get_pin_state(our_data & 0b000001));

            pin_down.set_level(get_pin_state(our_data & 0b000100));
            pin_up.set_level(get_pin_state(our_data & 0b000010));
            pin_right.set_level(get_pin_state(our_data & 0b010000));
            pin_left.set_level(get_pin_state(our_data & 0b001000));

            // Tidy up missing buttons
            pin_b3.set_level(Level::High);
            pin_b4.set_level(Level::High);
            pin_b5.set_level(Level::High);
            pin_b6.set_level(Level::High);
            pin_coin.set_level(Level::High)
        }

        debug!("Got bits: {:#026b}", our_data);
    }
}

// Let's avoid having to have all this everywhere
struct FifteenPinOutput {
    output_start: OutputStart,
    output_coin: OutputCoin,
    output_b1: OutputB1,
    output_b2: OutputB2,
    output_b3: OutputB3,
    output_b4: OutputB4,
    output_b5: OutputB5,
    output_b6: OutputB6,
    output_up: OutputUp,
    output_down: OutputDown,
    output_left: OutputLeft,
    output_right: OutputRight,
}

impl FifteenPinOutput {
    fn destructure(
        self,
    ) -> (
        OutputStart,
        OutputCoin,
        OutputB1,
        OutputB2,
        OutputB3,
        OutputB4,
        OutputB5,
        OutputB6,
        OutputUp,
        OutputDown,
        OutputLeft,
        OutputRight,
    ) {
        (
            self.output_start,
            self.output_coin,
            self.output_b1,
            self.output_b2,
            self.output_b3,
            self.output_b4,
            self.output_b5,
            self.output_b6,
            self.output_up,
            self.output_down,
            self.output_left,
            self.output_right,
        )
    }
}

// Turn a positive bitmask result into Level::High and anything else into Level::Low
fn get_pin_state(result: u32) -> Level {
    Level::from(result > 0)
}

fn split_peripherals(
    p: Peripherals,
) -> (
    Pins,
    Peri<'static, embassy_rp::peripherals::PIO0>,
    Peri<'static, embassy_rp::peripherals::PIO1>,
    Peri<'static, embassy_rp::peripherals::DMA_CH0>,
) {
    (
        Pins {
            pin0: p.PIN_0,
            pin1: p.PIN_1,
            pin2: p.PIN_2,
            pin3: p.PIN_3,
            pin4: p.PIN_4,
            pin5: p.PIN_5,
            pin6: p.PIN_6,
            pin7: p.PIN_7,
            pin8: p.PIN_8,
            pin9: p.PIN_9,
            pin10: p.PIN_10,
            pin11: p.PIN_11,
            pin12: p.PIN_12,
            pin13: p.PIN_13,
            pin14: p.PIN_14,
            pin15: p.PIN_15,
            pin16: p.PIN_16,
            pin17: p.PIN_17,
            pin18: p.PIN_18,
            pin19: p.PIN_19,
            pin20: p.PIN_20,
            pin21: p.PIN_21,
            pin22: p.PIN_22,
            pin23: p.PIN_23,
            pin24: p.PIN_24,
            pin25: p.PIN_25,
            pin26: p.PIN_26,
            pin27: p.PIN_27,
            pin28: p.PIN_28,
            pin29: p.PIN_29,
        },
        p.PIO0,
        p.PIO1,
        p.DMA_CH0,
    )
}

struct CD32Pins {
    power: Output<'static>,
    up: Input<'static>,
    down: Input<'static>,
    left: Input<'static>,
    right: Input<'static>,
    latch: embassy_rp::pio::Pin<'static, PIO0>,
    clock: embassy_rp::pio::Pin<'static, PIO0>,
    data: embassy_rp::pio::Pin<'static, PIO0>,
}

type CD32PinsTuple = (
    Output<'static>,
    Input<'static>,
    Input<'static>,
    Input<'static>,
    Input<'static>,
    embassy_rp::pio::Pin<'static, PIO0>,
    embassy_rp::pio::Pin<'static, PIO0>,
    embassy_rp::pio::Pin<'static, PIO0>,
);

impl CD32Pins {
    fn destructure(self) -> CD32PinsTuple {
        (
            self.power, self.up, self.down, self.left, self.right, self.latch, self.clock,
            self.data,
        )
    }
}
struct MDPins {
    power: Output<'static>,
    select: embassy_rp::pio::Pin<'static, PIO0>,
    b_a: embassy_rp::pio::Pin<'static, PIO0>,
    up_z: embassy_rp::pio::Pin<'static, PIO0>,
    down_y: embassy_rp::pio::Pin<'static, PIO0>,
    left_x_gnd: embassy_rp::pio::Pin<'static, PIO0>,
    right_mode_gnd: embassy_rp::pio::Pin<'static, PIO0>,
    c_start: embassy_rp::pio::Pin<'static, PIO0>,
}

type MDPinsTuple = (
    Output<'static>,
    embassy_rp::pio::Pin<'static, PIO0>,
    embassy_rp::pio::Pin<'static, PIO0>,
    embassy_rp::pio::Pin<'static, PIO0>,
    embassy_rp::pio::Pin<'static, PIO0>,
    embassy_rp::pio::Pin<'static, PIO0>,
    embassy_rp::pio::Pin<'static, PIO0>,
    embassy_rp::pio::Pin<'static, PIO0>,
);

impl MDPins {
    fn destructure(self) -> MDPinsTuple {
        (
            self.power,
            self.select,
            self.b_a,
            self.up_z,
            self.down_y,
            self.left_x_gnd,
            self.right_mode_gnd,
            self.c_start,
        )
    }
}

#[allow(dead_code)]
struct Pins {
    pin0: Peri<'static, embassy_rp::peripherals::PIN_0>,
    pin1: Peri<'static, embassy_rp::peripherals::PIN_1>,
    pin2: Peri<'static, embassy_rp::peripherals::PIN_2>,
    pin3: Peri<'static, embassy_rp::peripherals::PIN_3>,
    pin4: Peri<'static, embassy_rp::peripherals::PIN_4>,
    pin5: Peri<'static, embassy_rp::peripherals::PIN_5>,
    pin6: Peri<'static, embassy_rp::peripherals::PIN_6>,
    pin7: Peri<'static, embassy_rp::peripherals::PIN_7>,
    pin8: Peri<'static, embassy_rp::peripherals::PIN_8>,
    pin9: Peri<'static, embassy_rp::peripherals::PIN_9>,
    pin10: Peri<'static, embassy_rp::peripherals::PIN_10>,
    pin11: Peri<'static, embassy_rp::peripherals::PIN_11>,
    pin12: Peri<'static, embassy_rp::peripherals::PIN_12>,
    pin13: Peri<'static, embassy_rp::peripherals::PIN_13>,
    pin14: Peri<'static, embassy_rp::peripherals::PIN_14>,
    pin15: Peri<'static, embassy_rp::peripherals::PIN_15>,
    pin16: Peri<'static, embassy_rp::peripherals::PIN_16>,
    pin17: Peri<'static, embassy_rp::peripherals::PIN_17>,
    pin18: Peri<'static, embassy_rp::peripherals::PIN_18>,
    pin19: Peri<'static, embassy_rp::peripherals::PIN_19>,
    pin20: Peri<'static, embassy_rp::peripherals::PIN_20>,
    pin21: Peri<'static, embassy_rp::peripherals::PIN_21>,
    pin22: Peri<'static, embassy_rp::peripherals::PIN_22>,
    pin23: Peri<'static, embassy_rp::peripherals::PIN_23>,
    pin24: Peri<'static, embassy_rp::peripherals::PIN_24>,
    pin25: Peri<'static, embassy_rp::peripherals::PIN_25>,
    pin26: Peri<'static, embassy_rp::peripherals::PIN_26>,
    pin27: Peri<'static, embassy_rp::peripherals::PIN_27>,
    pin28: Peri<'static, embassy_rp::peripherals::PIN_28>,
    pin29: Peri<'static, embassy_rp::peripherals::PIN_29>,
}
