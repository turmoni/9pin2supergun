# 9pin2supergun
Code for a Pi Pico to read a CD32, Mega Drive/Genesis, or generic 9-pin gamepad and output each control to a separate pin for use with Neo Geos and/or Superguns.

I am personally using this code with a [Monster Joysticks CD32 joystick](https://monsterjoysticks.com/deluxe-cd32-retro-joystick-kit-classic) and a [Minigun Supergun](https://www.arcade-projects.com/threads/minigun-supergun-an-open-source-supergun.9408/).

There is a chance that there will be USB host support in the future, if using [the PCB](/pcb/).

## Requirements

For the hardware, you have two options; either build the PCB, details of which are in the [pcb directory](/pcb/), or wire up a Pi Pico. The PCB is the most compact and solid option, but isn't a very beginner-friendly assembly, given its surface mount components, dense layout, and the fine pitch of the RP2040 package. Wiring up a Pico is more accessible, but bulkier.

If you're using a Pi Pico, you will need the following:

* A Pi Pico or compatible device that exposes the GPIO pins
* Level shifters and pull-up resistors if you want to not risk the potential of having +5V in to the RP2040 (the schematic specifies ones that work, albeit in a surface mount package)
* Some kind of configurable level shifter or power switch if you want to be able to use this with both CD32 and Mega Drive pads[^2]. Again, the schematic specifies ones that work, so go with that if you're comfortable with surface mount soldering or find them in a through-hole package
* A DE9 and DA15[^1] connector (ideally plug for DE9, socket for DA15)
* Optionally a mode selector switch if you want to be able to use both CD32 and Mega Drive pads
* Optionally an RGB LED compatible with the WS2812 for a status LED, mostly useful if you're building it to support both CD32 and Mega Drive modes

I have made a schematic that shows the components that I used:

![Schematic showing how to wire up the setup](/9pin2supergun.svg)

I went through quite a few headaches to get a level shifting setup that works in as generic a way as possible, so following what I've done will be the path of least resistance, just with some fairly fine pitched soldering. However, if you're not so worried about doing it Properly, there's probably little harm in playing around with those generic level shifter modules you get, and which I used during initial development of the CD32 side.

## Component explanations

The switch SW1 is used to select between CD32 and Mega Drive/Genesis mode; when tied to ground, it's Mega Drive mode, when floating, it's CD32 mode. When running with an attached RGB LED, it will indicate the mode it's running in (see Usage section for more details). If you're building this yourself and only care about one of the modes, you can wire it up as you want.

The button S1 is currently unused, and there for futureproofing.

The level shifter U6 handles those inputs that are only ever inputs (pulled up to 5V). U7 handles both of the two outputs that can also be the +5V supply, and also the one remaining pin that's either an input or an output depending on whether it's in CD32 or Mega Drive mode. U6 isn't anything special, it's just a generic directional level shifter. U7 is effectively two directional level shifters in one package, with two I/Os per logical shifter.

The connections to the DE9 connector are as follows:

| Pin | CD32 Function | CD32 Direction | Mega Drive Function | MD Direction |
| --- | ------------- | -------------- | ------------------- | ------------ |
| 1   | Up            | In             | Up/Z                | In           |
| 2   | Down          | In             | Down/Y              | In           |
| 3   | Left          | In             | Left/X/GND          | In           |
| 4   | Right         | In             | Right/Mode/GND      | In           |
| 5   | Latch         | Out            | +5V                 | Out          |
| 6   | Fire 1/Clock  | Out            | B/A                 | In           |
| 7   | +5V           | Out            | Select              | Out          |
| 8   | GND           | N/A            | GND                 | N/A          |
| 9   | Fire 2/Data   | In             | C/Start             | In           |

As the table shows, the DE9 connector is a lot simpler if you're only targetting a single platform; for a CD32, you can permanently wire pin 7 to +5V, and pin 6 as an output, whereas with a Mega Drive controller you can wire pin 5 to +5V and pin 6 as an input. You may also be able to get away with not needing level shifters in this case, if you're willing to risk +5V into your Pico's GPIO pins, and your controller recognises 3.3V logic levels.

## Installation

If you don't want to customise anything, grab the latest uf2 file from [the releases page](https://github.com/turmoni/9pin2supergun/releases/) that matches your setup and install it to your board by holding down BOOTSEL when plugging it in to a PC, and then copying the file on to the storage device that appears. The PCB's BOOTSEL button is at the top-right. The options are:

* 9pin2supergun-**pcb**-v\*.uf2 - if you're using [the PCB I designed](/pcb/)
* 9pin2supergun-**pico**-v\*.uf2 - if you're wiring up a Pico as per the schematic

Don't worry if you're missing optional components, the code shouldn't care.

## Usage

Once you've got a board programmed, using it is pretty much a case of setting the switch to the right position, plugging it in, and going! If you've fitted the LED, it will tell you which mode you're in:
* Red: Amiga CD32 mode
* Blue: Mega Drive/generic mode
* Yellow: Mega Drive/generic mode with the buttons swapped

The button mapping is as follows:

| Output | CD32[^3]  | Mega Drive/Genesis (6- or 3-button) | Generic 9-pin |
| ------ | --------  | ----------------------------------- | ------------- |
| Up     | Up        | Up                                  | Up            |
| Down   | Down      | Down                                | Down          |
| Left   | Left      | Left                                | Left          |
| Right  | Right     | Right                               | Right         |
| 1      | G         | A                                   | 1             |
| 2      | Y         | B                                   | 2             |
| 3      | LS        | C                                   | N/A           |
| 4      | R         | X                                   | N/A           |
| 5      | B         | Y                                   | N/A           |
| 6      | RS        | Z                                   | N/A           |
| Start  | Pause     | Start                               | 1 + 2         |
| Coin   | Pause + G | Mode                                | N/A           |

Of course, X, Y, Z, and Mode aren't present on a 3-button Mega Drive controller, so these buttons aren't available there.

Since the Mega Drive layout may be the opposite of what's wanted, you can press and hold mode for three seconds to swap ABC and XYZ.

If you don't want the 1+2 mapping for generic 9-pin controllers, set `MAP_GENERIC_AB_TO_START` to false and rebuild the code. If you don't want the Pause + Green mapping for CD32 controllers, or want it to be a different button combination, change `CD32_COIN_BITMASK` (there are comments in the code to explain its format). Since these are pretty niche, I'm not sure figuring out a runtime configuration option is worth it.

## Building the code

This project is based on the [rp2040 project template](https://github.com/rp-rs/rp2040-project-template), have a look there for details. You need a `rust` build environment and the project will build with Cargo.

If you're using a Pi Pico, rather than the PCB, pass the `--features pi_pico` flag to `cargo` when building to get the right pin assignments.

## Anything else

This polls at 125Hz for the CD32 - I don't know whether a real CD32 pad will be happy with this, but I don't see why it wouldn't. The timing for each polling burst is based on what I observed with my PAL Amiga 500+, so that should be fine, but it was only polling at 50Hz. Given how little time is spent polling compared to waiting even at 125Hz, though, I can't imagine what issues might arise.

The Mega Drive controller support polls at around 140Hz.

Current draw seems fairly low, hovering at around 20-25mA on the controllers I've tested it with. If you're worried, power it up via USB (not whilst connected to a 15-pin port!) and use a USB power meter to see how much it's drawing.

## Credits

Thanks to [Mathew Carr's PSCD32 Development Diary](https://www.mrdictionary.net/PSCD32/diary/2019_08_09.htm) for documenting how the CD32 protocol actually works, and to [the RetroSix Wiki's Mega Drive controller page](https://www.retrosix.wiki/controller-interface-sega-mega-drive) for documenting the Mega Drive bits.

[^1]: Literally everyone seems to call this a DB15 connector, but it is actually DA15.
[^2]: Annoyingly, they use different pinouts, with +5V being on a different pin
[^3]: Single-letter CD32 buttons refer to the colour, LB and RB are the shoulder buttons
