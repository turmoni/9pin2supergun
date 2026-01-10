# 9pin2supergun
A project to read a CD32, Mega Drive/Genesis, or generic 9-pin gamepad and output each control to a separate pin for use with Neo Geos and/or Superguns.

I am personally using this code with a [Monster Joysticks CD32 joystick](https://monsterjoysticks.com/deluxe-cd32-retro-joystick-kit-classic) and a [Minigun Supergun](https://www.arcade-projects.com/threads/minigun-supergun-an-open-source-supergun.9408/).

There is a chance that there will be USB host support in the future, if using [the PCB](pcb/).

## Hardware

There are two expected paths to getting the hardware needed to use this; either build the PCB, or hook up a Pi Pico. For the most compact setup, but needing some more moderate to advanced soldering skills, build the PCB. For a potentially easier build, hook up a Pi Pico.

The PCB's files and some basic instructions can be found in [the pcb directory](pcb/). This isn't for people who are new to soldering - it has surface mount components, a dense layout, and the RP2040 package is very fine pitched.

Schematics and a reference PCB for hooking up a Pi Pico, along with details about the parts used, can be found in [the pico_reference directory](pico_reference/).

## Installation

If you don't want to customise anything, grab the latest uf2 file from [the releases page](https://github.com/turmoni/9pin2supergun/releases/) that matches your setup and install it to your board by holding down BOOTSEL when plugging it in to a PC, and then copying the file on to the storage device that appears. The PCB's BOOTSEL button is at the top-right. The options are:

* 9pin2supergun-**pcb**-v\*.uf2 - if you're using [the PCB I designed](pcb/)
* 9pin2supergun-**pico**-v\*.uf2 - if you're wiring up a Pico as per the schematic

Don't worry if you're missing optional components, the code shouldn't care.

## Usage

Once you've got a board programmed, using it is pretty much a case of setting the switch to the right position, plugging it in, and going! If you've fitted the LED, it will tell you which mode you're in:
* Red: Amiga CD32 mode
* Blue: Mega Drive/generic mode
* Yellow: Mega Drive/generic mode with the buttons swapped

The button mapping is as follows:

| Output | CD32[^1]  | Mega Drive/Genesis (6- or 3-button) | Generic 9-pin |
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

[^1]: Single-letter CD32 buttons refer to the colour, LB and RB are the shoulder buttons
