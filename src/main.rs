//! Reads a CD32 gamepad and breaks it out into 7 individual active-low GPIO pins
#![no_std]
#![no_main]

use cortex_m::singleton;
use defmt::*;
use defmt_rtt as _;
use panic_probe as _;
use rp2040_hal::{self as hal, gpio::Pin};

use hal::{
    clocks::{init_clocks_and_plls, Clock},
    dma::{double_buffer, DMAExt},
    gpio,
    gpio::bank0,
    gpio::{
        AnyPin, DynPinId, DynPullType, FunctionPio0, FunctionSioInput, FunctionSioOutput, PinState,
        PullDown, ValidFunction,
    },
    pac,
    pio::PIOExt,
    pio::Rx,
    pio::StateMachine,
    pio::Stopped,
    sio::Sio,
    watchdog::Watchdog,
};

use embedded_hal::digital::InputPin;
use embedded_hal::digital::OutputPin;

use ws2812_pio::Ws2812;

use smart_leds::{SmartLedsWrite, RGB8};

type ArbitraryOutPin = Pin<DynPinId, FunctionSioOutput, DynPullType>;
type ArbitraryInPin = Pin<DynPinId, FunctionSioInput, DynPullType>;
type ArbitraryPioPin = Pin<DynPinId, FunctionPio0, PullDown>;
type RgbLed = Ws2812<
    pac::PIO1,
    hal::pio::SM0,
    hal::timer::CountDown,
    Pin<DynPinId, gpio::FunctionPio1, PullDown>,
>;
//type ArbitraryInOutPin = InOutPin<Pin<DynPinId, Function>>;

/// The linker will place this boot block at the start of our program image. We
/// need this to help the ROM bootloader get our code up and running.
/// Note: This boot block is not necessary when using a rp-hal based BSP
/// as the BSPs already perform this step.
#[link_section = ".boot2"]
#[used]
pub static BOOT2: [u8; 256] = rp2040_boot2::BOOT_LOADER_GENERIC_03H;

// If using a generic two-button controller, map A+B to be start
const MAP_GENERIC_AB_TO_START: bool = true;

// What bitmask should we look for to let us send the coin button in CD32 mode?
// The bits are as follows:
// Pause, LB, RB, Green, Yellow, Red, Blue
// Set to 0 to disable the feature.
// Default: Pause + Green
const CD32_COIN_BITMASK: u32 = 0b1001000;

#[rp2040_hal::entry]
fn main() -> ! {
    info!("Program start");
    let mut pac: pac::Peripherals = pac::Peripherals::take().unwrap();
    let mut watchdog = Watchdog::new(pac.WATCHDOG);
    let sio = Sio::new(pac.SIO);

    // External high-speed crystal on the pico board is 12Mhz
    let external_xtal_freq_hz = 12_000_000u32;
    let clocks = init_clocks_and_plls(
        external_xtal_freq_hz,
        pac.XOSC,
        pac.CLOCKS,
        pac.PLL_SYS,
        pac.PLL_USB,
        &mut pac.RESETS,
        &mut watchdog,
    )
    .ok()
    .unwrap();

    let cpu_freq = clocks.system_clock.freq().to_Hz();

    let pins = hal::gpio::Pins::new(
        pac.IO_BANK0,
        pac.PADS_BANK0,
        sio.gpio_bank0,
        &mut pac.RESETS,
    );

    let timer = hal::timer::Timer::new(pac.TIMER, &mut pac.RESETS, &clocks);

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

    let (mut led_pio, led_sm0, _, _, _) = pac.PIO1.split(&mut pac.RESETS);
    let mut ws = Ws2812::new(
        led.into_function().into_dyn_pin(),
        &mut led_pio,
        led_sm0,
        clocks.peripheral_clock.freq(),
        timer.count_down(),
    );

    // Connect GPIO2 to GND for Mega Drive/generic 9-pin. high or floating for CD32.
    let mut selector = selector.into_pull_up_input();
    let use_cd32 = selector.is_low().unwrap();

    let pin_six_direction_set = pin_six_direction.into_push_pull_output();
    let shifter_oe_set = shifter_oe.into_push_pull_output_in_state(PinState::Low);

    if use_cd32 {
        let colour: RGB8 = (255, 0, 0).into();
        ws.write([colour].iter().copied()).unwrap();
        debug!("Not implemented");
        let power: ArbitraryOutPin = in_power_select
            .into_function()
            .into_pull_type()
            .into_dyn_pin();

        let up: ArbitraryInPin = in_up_z.into_function().into_pull_type().into_dyn_pin();

        let down: ArbitraryInPin = in_down_y.into_function().into_pull_type().into_dyn_pin();

        let left: ArbitraryInPin = in_left_x_gnd
            .into_function()
            .into_pull_type()
            .into_dyn_pin();

        let right: ArbitraryInPin = in_right_mode_gnd
            .into_function()
            .into_pull_type()
            .into_dyn_pin();

        let latch: ArbitraryPioPin = in_latch_power
            .into_function()
            .into_pull_type()
            .into_dyn_pin();

        let clock: ArbitraryPioPin = in_fire1_clock_b_a
            .into_function()
            .into_pull_type()
            .into_dyn_pin();

        let data: ArbitraryPioPin = in_fire2_data_c_start
            .into_function()
            .into_pull_type()
            .into_dyn_pin();

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
            cpu_freq,
            pin_six_direction_set,
            shifter_oe_set,
            cd32_pins,
            outputs,
            pac.PIO0,
            pac.DMA,
            &mut pac.RESETS,
        );
    } else {
        let colour: RGB8 = (0, 0, 255).into();
        ws.write([colour].iter().copied()).unwrap();
        // First set up all the pins
        let power: ArbitraryOutPin = in_latch_power
            .into_function()
            .into_pull_type()
            .into_dyn_pin();

        let select: ArbitraryPioPin = in_power_select
            .into_function()
            .into_pull_type()
            .into_dyn_pin();

        let b_a: ArbitraryPioPin = in_fire1_clock_b_a
            .into_function()
            .into_pull_type()
            .into_dyn_pin();

        let up_z: ArbitraryPioPin = in_up_z.into_function().into_pull_type().into_dyn_pin();

        let down_y: ArbitraryPioPin = in_down_y.into_function().into_pull_type().into_dyn_pin();

        let left_x_gnd: ArbitraryPioPin = in_left_x_gnd
            .into_function()
            .into_pull_type()
            .into_dyn_pin();

        let right_mode_gnd: ArbitraryPioPin = in_right_mode_gnd
            .into_function()
            .into_pull_type()
            .into_dyn_pin();

        let c_start: ArbitraryPioPin = in_fire2_data_c_start
            .into_function()
            .into_pull_type()
            .into_dyn_pin();

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
            cpu_freq,
            pin_six_direction_set,
            shifter_oe_set,
            md_pins,
            outputs,
            timer,
            pac.PIO0,
            pac.DMA,
            &mut pac.RESETS,
            ws,
        );
    }
}

#[cfg(feature = "pi_pico")]
fn get_pins(
    pins: gpio::Pins,
) -> (
    Pin<bank0::Gpio2, gpio::FunctionNull, gpio::PullDown>,
    Pin<bank0::Gpio27, gpio::FunctionNull, gpio::PullDown>,
    Pin<bank0::Gpio26, gpio::FunctionNull, gpio::PullDown>,
    Pin<bank0::Gpio15, gpio::FunctionNull, gpio::PullDown>,
    Pin<bank0::Gpio16, gpio::FunctionNull, gpio::PullDown>,
    Pin<bank0::Gpio17, gpio::FunctionNull, gpio::PullDown>,
    Pin<bank0::Gpio18, gpio::FunctionNull, gpio::PullDown>,
    Pin<bank0::Gpio19, gpio::FunctionNull, gpio::PullDown>,
    Pin<bank0::Gpio20, gpio::FunctionNull, gpio::PullDown>,
    Pin<bank0::Gpio21, gpio::FunctionNull, gpio::PullDown>,
    Pin<bank0::Gpio22, gpio::FunctionNull, gpio::PullDown>,
    Pin<bank0::Gpio0, gpio::FunctionNull, gpio::PullDown>,
    FifteenPinOutput,
) {
    let output_start: ArbitraryOutPin = pins.gpio11.into_function().into_pull_type().into_dyn_pin(); // Start
    let output_coin: ArbitraryOutPin = pins.gpio12.into_function().into_pull_type().into_dyn_pin(); // Coin
    let output_b1: ArbitraryOutPin = pins.gpio7.into_function().into_pull_type().into_dyn_pin(); // 1
    let output_b2: ArbitraryOutPin = pins.gpio8.into_function().into_pull_type().into_dyn_pin(); // 2
    let output_b3: ArbitraryOutPin = pins.gpio9.into_function().into_pull_type().into_dyn_pin(); // 3
    let output_b4: ArbitraryOutPin = pins.gpio10.into_function().into_pull_type().into_dyn_pin(); // 4
    let output_b5: ArbitraryOutPin = pins.gpio13.into_function().into_pull_type().into_dyn_pin(); // 5
    let output_b6: ArbitraryOutPin = pins.gpio14.into_function().into_pull_type().into_dyn_pin(); // 6
    let output_up: ArbitraryOutPin = pins.gpio3.into_function().into_pull_type().into_dyn_pin(); // Up
    let output_down: ArbitraryOutPin = pins.gpio4.into_function().into_pull_type().into_dyn_pin(); // Down
    let output_left: ArbitraryOutPin = pins.gpio5.into_function().into_pull_type().into_dyn_pin(); // Left
    let output_right: ArbitraryOutPin = pins.gpio6.into_function().into_pull_type().into_dyn_pin(); // Right

    let fpo = FifteenPinOutput {
        output_start,
        output_coin,  // Coin
        output_b1,    // 1
        output_b2,    // 2
        output_b3,    // 3
        output_b4,    // 4
        output_b5,    // 5
        output_b6,    // 6
        output_up,    // Up
        output_down,  // Down
        output_left,  // Left
        output_right, // Right
    };
    let selector = pins.gpio2;
    let pin_six_direction = pins.gpio27;
    let soe = pins.gpio26;

    // 9-pin connector
    let in_latch_power = pins.gpio15; // Pin 5
    let in_power_select = pins.gpio16; // Pin 7
    let in_fire1_clock_b_a = pins.gpio17; // Pin 6
    let in_up_z = pins.gpio18; // Pin 1
    let in_down_y = pins.gpio19; // Pin 2
    let in_left_gnd = pins.gpio20; // Pin 3
    let in_right_mode_gnd = pins.gpio21; // Pin 4
    let in_fire2_data_c_start = pins.gpio22; // Pin 9
    return (
        selector,              // Selector
        pin_six_direction,     // I dunno
        soe,                   // Shifter's Output Enable
        in_latch_power,        // in_latch_power
        in_power_select,       // in_power_select
        in_fire1_clock_b_a,    // in_fire1_clock_b_a
        in_up_z,               // in_up_z
        in_down_y,             // in_down_y
        in_left_gnd,           // in_left_gnd
        in_right_mode_gnd,     // in_right_mode_gnd
        in_fire2_data_c_start, // in_fire2_data_c_start
        pins.gpio0,
        fpo,
    );
}

#[cfg(not(feature = "pi_pico"))]
fn get_pins(
    pins: gpio::Pins,
) -> (
    Pin<bank0::Gpio17, gpio::FunctionNull, gpio::PullDown>,
    Pin<bank0::Gpio22, gpio::FunctionNull, gpio::PullDown>,
    Pin<bank0::Gpio23, gpio::FunctionNull, gpio::PullDown>,
    Pin<bank0::Gpio20, gpio::FunctionNull, gpio::PullDown>,
    Pin<bank0::Gpio21, gpio::FunctionNull, gpio::PullDown>,
    Pin<bank0::Gpio24, gpio::FunctionNull, gpio::PullDown>,
    Pin<bank0::Gpio25, gpio::FunctionNull, gpio::PullDown>,
    Pin<bank0::Gpio26, gpio::FunctionNull, gpio::PullDown>,
    Pin<bank0::Gpio27, gpio::FunctionNull, gpio::PullDown>,
    Pin<bank0::Gpio28, gpio::FunctionNull, gpio::PullDown>,
    Pin<bank0::Gpio29, gpio::FunctionNull, gpio::PullDown>,
    Pin<bank0::Gpio16, gpio::FunctionNull, gpio::PullDown>,
    FifteenPinOutput,
) {
    let output_start: ArbitraryOutPin = pins.gpio9.into_function().into_pull_type().into_dyn_pin(); // Start
    let output_coin: ArbitraryOutPin = pins.gpio8.into_function().into_pull_type().into_dyn_pin(); // Coin
    let output_b1: ArbitraryOutPin = pins.gpio4.into_function().into_pull_type().into_dyn_pin(); // 1
    let output_b2: ArbitraryOutPin = pins.gpio5.into_function().into_pull_type().into_dyn_pin(); // 2
    let output_b3: ArbitraryOutPin = pins.gpio6.into_function().into_pull_type().into_dyn_pin(); // 3
    let output_b4: ArbitraryOutPin = pins.gpio7.into_function().into_pull_type().into_dyn_pin(); // 4
    let output_b5: ArbitraryOutPin = pins.gpio10.into_function().into_pull_type().into_dyn_pin(); // 5
    let output_b6: ArbitraryOutPin = pins.gpio11.into_function().into_pull_type().into_dyn_pin(); // 6
    let output_up: ArbitraryOutPin = pins.gpio0.into_function().into_pull_type().into_dyn_pin(); // Up
    let output_down: ArbitraryOutPin = pins.gpio1.into_function().into_pull_type().into_dyn_pin(); // Down
    let output_left: ArbitraryOutPin = pins.gpio2.into_function().into_pull_type().into_dyn_pin(); // Left
    let output_right: ArbitraryOutPin = pins.gpio3.into_function().into_pull_type().into_dyn_pin(); // Right

    let fpo = FifteenPinOutput {
        output_start,
        output_coin,  // Coin
        output_b1,    // 1
        output_b2,    // 2
        output_b3,    // 3
        output_b4,    // 4
        output_b5,    // 5
        output_b6,    // 6
        output_up,    // Up
        output_down,  // Down
        output_left,  // Left
        output_right, // Right
    };
    let selector = pins.gpio17;
    let pin_six_direction = pins.gpio22;
    let soe = pins.gpio23;
    let in_latch_power = pins.gpio20;
    let in_power_select = pins.gpio21;
    let in_fire1_clock_b_a = pins.gpio24;
    let in_up_z = pins.gpio25;
    let in_down_y = pins.gpio26;
    let in_left_gnd = pins.gpio27;
    let in_right_mode_gnd = pins.gpio28;
    let in_fire2_data_c_start = pins.gpio29;
    return (
        selector,              // Selector
        pin_six_direction,     // I dunno
        soe,                   // Shifter's Output Enable
        in_latch_power,        // in_latch_power
        in_power_select,       // in_power_select
        in_fire1_clock_b_a,    // in_fire1_clock_b_a
        in_up_z,               // in_up_z
        in_down_y,             // in_down_y
        in_left_gnd,           // in_left_gnd
        in_right_mode_gnd,     // in_right_mode_gnd
        in_fire2_data_c_start, // in_fire2_data_c_start
        pins.gpio16,
        fpo,
    );
}

fn run_cd32_code<A: AnyPin, B: AnyPin>(
    cpu_freq: u32,
    pin_six_direction: A,
    shifter_oe: B,
    cd32_pins: CD32Pins,
    outputs: FifteenPinOutput,
    pio: pac::PIO0,
    dma: pac::DMA,
    pac_resets: &mut pac::RESETS,
) -> !
where
    A::Id: ValidFunction<FunctionSioOutput>,
    B::Id: ValidFunction<FunctionSioOutput>,
{
    let (sm, rx, cd32_pins) = setup_state_machine_cd32(cpu_freq, pio, pac_resets, cd32_pins);
    read_cd32_loop(
        pin_six_direction,
        shifter_oe,
        sm,
        rx,
        cd32_pins,
        outputs,
        dma,
        pac_resets,
    );
}

fn setup_state_machine_cd32(
    cpu_freq: u32,
    pio: pac::PIO0,
    pac_resets: &mut pac::RESETS,
    cd32_pins: CD32Pins,
) -> (
    StateMachine<(hal::pac::PIO0, hal::pio::SM0), Stopped>,
    Rx<(hal::pac::PIO0, hal::pio::SM0)>,
    CD32Pins,
) {
    let pio_multiplier: u16 = (cpu_freq / 140000).try_into().unwrap();

    info!("PIO multiplier is {}", pio_multiplier);

    let (power, up, down, left, right, latch, clock, data) = cd32_pins.destructure();
    let set_base_id = latch.id().num;
    let side_set_base_id = clock.id().num;
    let in_base_id = data.id().num;

    let read_cd32 = pio_proc::pio_asm!(
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

    let (mut pio, sm0, _, _, _) = pio.split(pac_resets);
    let installed = pio.install(&read_cd32.program).unwrap();
    let (mut sm, rx, _) = rp2040_hal::pio::PIOBuilder::from_installed_program(installed)
        .side_set_pin_base(side_set_base_id)
        .set_pins(set_base_id, 1)
        .in_pin_base(in_base_id)
        .clock_divisor_fixed_point(pio_multiplier, 0)
        .build(sm0);

    sm.set_pindirs([
        (set_base_id, hal::pio::PinDir::Output),
        (side_set_base_id, hal::pio::PinDir::Output),
        (in_base_id, hal::pio::PinDir::Input),
    ]);

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

    (sm, rx, cd32_pins)
}

fn read_cd32_loop<A: AnyPin, B: AnyPin>(
    pin_six_direction: A,
    shifter_oe: B,
    sm: StateMachine<(hal::pac::PIO0, hal::pio::SM0), Stopped>,
    rx: Rx<(hal::pac::PIO0, hal::pio::SM0)>,
    cd32_pins: CD32Pins,
    outputs: FifteenPinOutput,
    dma: pac::DMA,
    pac_resets: &mut pac::RESETS,
) -> !
where
    A::Id: ValidFunction<FunctionSioOutput>,
    B::Id: ValidFunction<FunctionSioOutput>,
{
    let (mut power, mut up, mut down, mut left, mut right, _, _, _) = cd32_pins.destructure();
    power.set_high().unwrap();

    pin_six_direction
        .into()
        .into_push_pull_output()
        .set_high()
        .unwrap();

    // Enable the output on the level shifter
    shifter_oe.into().into_push_pull_output().set_low().unwrap();

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
    let mut pin_start: HiZPin = HiZPin::new(pin_start);
    let mut pin_coin: HiZPin = HiZPin::new(pin_coin);
    let mut pin_b1: HiZPin = HiZPin::new(pin_b1);
    let mut pin_b2: HiZPin = HiZPin::new(pin_b2);
    let mut pin_b3: HiZPin = HiZPin::new(pin_b3);
    let mut pin_b4: HiZPin = HiZPin::new(pin_b4);
    let mut pin_b5: HiZPin = HiZPin::new(pin_b5);
    let mut pin_b6: HiZPin = HiZPin::new(pin_b6);
    let mut pin_up: HiZPin = HiZPin::new(pin_up);
    let mut pin_down: HiZPin = HiZPin::new(pin_down);
    let mut pin_left: HiZPin = HiZPin::new(pin_left);
    let mut pin_right: HiZPin = HiZPin::new(pin_right);

    sm.start();

    let dma = dma.split(pac_resets);
    let rx_buf = singleton!(: u32 = 0).unwrap();
    let rx_buf2 = singleton!(: u32 = 0).unwrap();

    let rx_transfer = double_buffer::Config::new((dma.ch0, dma.ch1), rx, rx_buf).start();
    let mut rx_transfer = rx_transfer.write_next(rx_buf2);

    loop {
        if rx_transfer.is_done() {
            let (rx_buf, next_rx_transfer) = rx_transfer.wait();
            // We only care about 7 bits of the 32 bits, make it a bit easier to deal with
            let mut our_data = *rx_buf >> 25;

            // Set CD32_COIN_BITMASK to customise what this matches
            pin_coin
                .set_state(get_pin_state(our_data & CD32_COIN_BITMASK))
                .unwrap();

            // If we've got the chord for the coin button, get rid of the component buttons so they don't fire as well
            if (our_data & CD32_COIN_BITMASK) == 0 {
                our_data = our_data | CD32_COIN_BITMASK;
            }

            pin_start
                .set_state(get_pin_state(our_data & 0b1000000))
                .unwrap();
            pin_b1
                .set_state(get_pin_state(our_data & 0b0001000))
                .unwrap();
            pin_b2
                .set_state(get_pin_state(our_data & 0b0000100))
                .unwrap();
            pin_b3
                .set_state(get_pin_state(our_data & 0b0100000))
                .unwrap();
            pin_b4
                .set_state(get_pin_state(our_data & 0b0000010))
                .unwrap();
            pin_b5
                .set_state(get_pin_state(our_data & 0b0000001))
                .unwrap();
            pin_b6
                .set_state(get_pin_state(our_data & 0b0010000))
                .unwrap();

            if our_data != 127 {
                debug!("Got bits: {:#09b}", our_data);
                debug!("            S361245");
                debug!(
                    "Buttons: 1 {} 2 {} 3 {} 4 {} 5 {} 6 {} start {} coin {}",
                    pin_b1.is_high(),
                    pin_b2.is_high(),
                    pin_b3.is_high(),
                    pin_b4.is_high(),
                    pin_b5.is_high(),
                    pin_b6.is_high(),
                    pin_start.is_high(),
                    pin_coin.is_high(),
                );
            }

            rx_transfer = next_rx_transfer.write_next(rx_buf);

            pin_up
                .set_state(PinState::from(up.is_high().unwrap()))
                .unwrap();
            pin_down
                .set_state(PinState::from(down.is_high().unwrap()))
                .unwrap();
            pin_left
                .set_state(PinState::from(left.is_high().unwrap()))
                .unwrap();
            pin_right
                .set_state(PinState::from(right.is_high().unwrap()))
                .unwrap();

            debug!(
                "Up: {}, Down: {}, Left: {}, Right: {}",
                pin_up.is_high(),
                pin_down.is_high(),
                pin_left.is_high(),
                pin_right.is_high(),
            );
        }
    }
}

fn run_md_code<A: AnyPin, B: AnyPin>(
    cpu_freq: u32,
    pin_six_direction: A,
    shifter_oe: B,
    md_pins: MDPins,
    outputs: FifteenPinOutput,
    timer: hal::timer::Timer,
    pio: pac::PIO0,
    dma: pac::DMA,
    pac_resets: &mut pac::RESETS,
    ws: RgbLed,
) -> !
where
    A::Id: ValidFunction<FunctionSioOutput>,
    B::Id: ValidFunction<FunctionSioOutput>,
{
    let (sm, rx, md_pins) = setup_state_machine_md(cpu_freq, pio, pac_resets, md_pins);

    read_md_loop(
        pin_six_direction,
        shifter_oe,
        sm,
        rx,
        md_pins,
        outputs,
        timer,
        dma,
        pac_resets,
        ws,
    );
}

fn setup_state_machine_md(
    cpu_freq: u32,
    pio: pac::PIO0,
    pac_resets: &mut pac::RESETS,
    md_pins: MDPins,
) -> (
    StateMachine<(hal::pac::PIO0, hal::pio::SM0), Stopped>,
    Rx<(hal::pac::PIO0, hal::pio::SM0)>,
    MDPins,
) {
    let pio_multiplier: u16 = (cpu_freq / 140000).try_into().unwrap();

    info!("PIO multiplier is {}", pio_multiplier);

    let (power, side_set_base, in_base, up_z, down_y, left_x_gnd, right_mode_gnd, c_start) =
        md_pins.destructure();

    let side_set_base_id = side_set_base.id().num;
    let in_base_id = in_base.id().num;

    let read_md = pio_proc::pio_asm!(
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

    let (mut pio, sm0, _, _, _) = pio.split(pac_resets);
    let installed = pio.install(&read_md.program).unwrap();
    let (mut sm, rx, _) = rp2040_hal::pio::PIOBuilder::from_installed_program(installed)
        .side_set_pin_base(side_set_base_id)
        .in_pin_base(in_base_id)
        .clock_divisor_fixed_point(pio_multiplier, 0)
        .build(sm0);

    sm.set_pindirs([
        (side_set_base_id, hal::pio::PinDir::Output),
        (in_base_id, hal::pio::PinDir::Input),
        (in_base_id + 1, hal::pio::PinDir::Input),
        (in_base_id + 2, hal::pio::PinDir::Input),
        (in_base_id + 3, hal::pio::PinDir::Input),
        (in_base_id + 4, hal::pio::PinDir::Input),
        (in_base_id + 5, hal::pio::PinDir::Input),
    ]);

    let md_pins = MDPins {
        power,
        select: side_set_base,
        b_a: in_base,
        up_z,
        down_y,
        left_x_gnd,
        right_mode_gnd,
        c_start,
    };

    (sm, rx, md_pins)
}

fn read_md_loop<A: AnyPin, B: AnyPin>(
    pin_six_direction: A,
    shifter_oe: B,
    sm: StateMachine<(hal::pac::PIO0, hal::pio::SM0), Stopped>,
    rx: Rx<(hal::pac::PIO0, hal::pio::SM0)>,
    md_pins: MDPins,
    out_pins: FifteenPinOutput,
    timer: hal::timer::Timer,
    dma: pac::DMA,
    pac_resets: &mut pac::RESETS,
    mut ws: RgbLed,
) -> !
where
    A::Id: ValidFunction<FunctionSioOutput>,
    B::Id: ValidFunction<FunctionSioOutput>,
{
    let (mut plus_five_volts, _, _, _, _, _, _, _) = md_pins.destructure();
    plus_five_volts.set_high().unwrap();

    pin_six_direction
        .into()
        .into_push_pull_output()
        .set_low()
        .unwrap();

    // Enable the output on the level shifter
    shifter_oe.into().into_push_pull_output().set_low().unwrap();

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
    ) = out_pins.destructure();

    // Initialise all the pins
    let mut pin_start: HiZPin = HiZPin::new(pin_start);
    let mut pin_coin: HiZPin = HiZPin::new(pin_coin);
    let mut pin_b1: HiZPin = HiZPin::new(pin_b1);
    let mut pin_b2: HiZPin = HiZPin::new(pin_b2);
    let mut pin_b3: HiZPin = HiZPin::new(pin_b3);
    let mut pin_b4: HiZPin = HiZPin::new(pin_b4);
    let mut pin_b5: HiZPin = HiZPin::new(pin_b5);
    let mut pin_b6: HiZPin = HiZPin::new(pin_b6);
    let mut pin_up: HiZPin = HiZPin::new(pin_up);
    let mut pin_down: HiZPin = HiZPin::new(pin_down);
    let mut pin_left: HiZPin = HiZPin::new(pin_left);
    let mut pin_right: HiZPin = HiZPin::new(pin_right);

    sm.start();

    let dma = dma.split(pac_resets);
    let rx_buf = singleton!(: u32 = 0).unwrap();
    let rx_buf2 = singleton!(: u32 = 0).unwrap();

    let rx_transfer = double_buffer::Config::new((dma.ch0, dma.ch1), rx, rx_buf).start();
    let mut rx_transfer = rx_transfer.write_next(rx_buf2);

    let mut swap_rows = false;
    // Whilst theoretically using 0 for a null value might lead to clashes, it's irrelevant because the next polling loop will fix it
    let mut mode_pressed_time = 0;

    loop {
        if rx_transfer.is_done() {
            let (rx_buf, next_rx_transfer) = rx_transfer.wait();
            // We only care about 24 bits of the 32 bits, make it a bit easier to deal with
            let our_data = *rx_buf >> 8;

            if our_data & 0b000000011000011000000000 == 0 {
                debug!("Mega Drive");
                // Mega Drive controller
                // Three button Mega Drive controller buttons:
                // Data: 0bxxxxxxxxxxxxSGGDUACRLDUA
                pin_start
                    .set_state(get_pin_state(our_data & 0b100000000000))
                    .unwrap();
                pin_down
                    .set_state(get_pin_state(our_data & 0b000100000000))
                    .unwrap();
                pin_up
                    .set_state(get_pin_state(our_data & 0b000010000000))
                    .unwrap();

                if !swap_rows {
                    pin_b1
                        .set_state(get_pin_state(our_data & 0b000001000000))
                        .unwrap();
                    pin_b2
                        .set_state(get_pin_state(our_data & 0b000000000001))
                        .unwrap();
                    pin_b3
                        .set_state(get_pin_state(our_data & 0b000000100000))
                        .unwrap();
                }
                pin_right
                    .set_state(get_pin_state(our_data & 0b000000010000))
                    .unwrap();
                pin_left
                    .set_state(get_pin_state(our_data & 0b000000001000))
                    .unwrap();

                if our_data & 0b000000011110000000000000 == 0 {
                    // 6 button controller
                    // Extra buttons: 0bxMXYZx
                    let six_button_data = our_data >> 18;
                    debug!("Got a 6 button");
                    if swap_rows {
                        pin_b1
                            .set_state(get_pin_state(six_button_data & 0b001000))
                            .unwrap();
                        pin_b2
                            .set_state(get_pin_state(six_button_data & 0b000100))
                            .unwrap();
                        pin_b3
                            .set_state(get_pin_state(six_button_data & 0b000010))
                            .unwrap();
                        pin_b4
                            .set_state(get_pin_state(our_data & 0b000001000000))
                            .unwrap();
                        pin_b5
                            .set_state(get_pin_state(our_data & 0b000000000001))
                            .unwrap();
                        pin_b6
                            .set_state(get_pin_state(our_data & 0b000000100000))
                            .unwrap();
                    } else {
                        pin_b4
                            .set_state(get_pin_state(six_button_data & 0b001000))
                            .unwrap();
                        pin_b5
                            .set_state(get_pin_state(six_button_data & 0b000100))
                            .unwrap();
                        pin_b6
                            .set_state(get_pin_state(six_button_data & 0b000010))
                            .unwrap();
                    }

                    pin_coin
                        .set_state(get_pin_state(six_button_data & 0b010000))
                        .unwrap();

                    // Handle swapping around A/B/C and X/Y/Z
                    if six_button_data & 0b010000 == 0 {
                        if mode_pressed_time == 0 {
                            mode_pressed_time = timer.get_counter_low();
                        } else {
                            // Hold for three seconds to invert
                            if timer.get_counter_low() - mode_pressed_time > 1_000_000 * 3 {
                                swap_rows = !swap_rows;
                                mode_pressed_time = 0;
                                debug!("Swapping rows, inverted now: {}", swap_rows);
                            }
                            let mut colour: RGB8 = (0, 0, 255).into();

                            if swap_rows {
                                colour = (255, 255, 0).into();
                            }

                            ws.write([colour].iter().copied()).unwrap();
                        }
                    } else {
                        mode_pressed_time = 0;
                    }
                } else {
                    // Tidy up buttons we don't have
                    pin_b4.set_state(PinState::High).unwrap();
                    pin_b5.set_state(PinState::High).unwrap();
                    pin_b6.set_state(PinState::High).unwrap();

                    // There are no rows to swap
                    swap_rows = false;
                    mode_pressed_time = 0;
                }
            } else {
                // Generic controller
                // 0b2RLDU1
                if MAP_GENERIC_AB_TO_START {
                    pin_start
                        .set_state(get_pin_state(our_data & 0b100001))
                        .unwrap();
                } else {
                    pin_start.set_state(PinState::High).unwrap();
                }

                pin_b1
                    .set_state(get_pin_state(our_data & 0b000001))
                    .unwrap();

                pin_down
                    .set_state(get_pin_state(our_data & 0b000100))
                    .unwrap();
                pin_up
                    .set_state(get_pin_state(our_data & 0b000010))
                    .unwrap();
                pin_right
                    .set_state(get_pin_state(our_data & 0b010000))
                    .unwrap();
                pin_left
                    .set_state(get_pin_state(our_data & 0b001000))
                    .unwrap();

                // Tidy up missing buttons
                pin_b3.set_state(PinState::High).unwrap();
                pin_b4.set_state(PinState::High).unwrap();
                pin_b5.set_state(PinState::High).unwrap();
                pin_b6.set_state(PinState::High).unwrap();
                pin_coin.set_state(PinState::High).unwrap();
            }

            debug!("Got bits: {:#026b}", our_data);
            rx_transfer = next_rx_transfer.write_next(rx_buf);
        }
    }
}
// End of file

// Let's avoid having to have all this everywhere
struct FifteenPinOutput {
    output_start: ArbitraryOutPin,
    output_coin: ArbitraryOutPin,
    output_b1: ArbitraryOutPin,
    output_b2: ArbitraryOutPin,
    output_b3: ArbitraryOutPin,
    output_b4: ArbitraryOutPin,
    output_b5: ArbitraryOutPin,
    output_b6: ArbitraryOutPin,
    output_up: ArbitraryOutPin,
    output_down: ArbitraryOutPin,
    output_left: ArbitraryOutPin,
    output_right: ArbitraryOutPin,
}

impl FifteenPinOutput {
    fn destructure(
        self,
    ) -> (
        ArbitraryOutPin,
        ArbitraryOutPin,
        ArbitraryOutPin,
        ArbitraryOutPin,
        ArbitraryOutPin,
        ArbitraryOutPin,
        ArbitraryOutPin,
        ArbitraryOutPin,
        ArbitraryOutPin,
        ArbitraryOutPin,
        ArbitraryOutPin,
        ArbitraryOutPin,
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

// A wrapper that drives the pin low for low, but disables output for high
struct HiZPin {
    pin: ArbitraryOutPin,
}

impl HiZPin {
    fn new(mut pin: ArbitraryOutPin) -> HiZPin {
        // Initialise the pin by *first* ensuring output is disabled, *then* setting low.
        // This allows us to just toggle output being enabled or not
        pin.set_output_enable_override(gpio::OutputEnableOverride::Disable);
        pin.set_low().unwrap();

        Self { pin }
    }

    fn is_high(&mut self) -> bool {
        return self.pin.get_output_enable_override() == gpio::OutputEnableOverride::Disable;
    }

    fn set_state(&mut self, desired_state: PinState) -> Result<(), gpio::Error> {
        if desired_state == PinState::Low {
            self.pin
                .set_output_enable_override(gpio::OutputEnableOverride::Enable);
            Ok(())
        } else {
            self.pin
                .set_output_enable_override(gpio::OutputEnableOverride::Disable);
            Ok(())
        }
    }
}

// Turn a positive bitmask result into PinState::High and anything else into PinState::Low
fn get_pin_state(result: u32) -> PinState {
    PinState::from(result > 0)
}

struct CD32Pins {
    power: ArbitraryOutPin,
    up: ArbitraryInPin,
    down: ArbitraryInPin,
    left: ArbitraryInPin,
    right: ArbitraryInPin,
    latch: ArbitraryPioPin,
    clock: ArbitraryPioPin,
    data: ArbitraryPioPin,
}

impl CD32Pins {
    fn destructure(
        self,
    ) -> (
        ArbitraryOutPin,
        ArbitraryInPin,
        ArbitraryInPin,
        ArbitraryInPin,
        ArbitraryInPin,
        ArbitraryPioPin,
        ArbitraryPioPin,
        ArbitraryPioPin,
    ) {
        (
            self.power, self.up, self.down, self.left, self.right, self.latch, self.clock,
            self.data,
        )
    }
}

struct MDPins {
    power: ArbitraryOutPin,
    select: ArbitraryPioPin,
    b_a: ArbitraryPioPin,
    up_z: ArbitraryPioPin,
    down_y: ArbitraryPioPin,
    left_x_gnd: ArbitraryPioPin,
    right_mode_gnd: ArbitraryPioPin,
    c_start: ArbitraryPioPin,
}

impl MDPins {
    fn destructure(
        self,
    ) -> (
        ArbitraryOutPin,
        ArbitraryPioPin,
        ArbitraryPioPin,
        ArbitraryPioPin,
        ArbitraryPioPin,
        ArbitraryPioPin,
        ArbitraryPioPin,
        ArbitraryPioPin,
    ) {
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
