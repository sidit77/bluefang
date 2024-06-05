# Bluefang

A experimental, very non-conformant, pure Rust Bluetooth stack.
This project focuses on implementing a implementing a audio speaker with remote control functionallity. As BLE Audio is not supported by most of the devices I own, this Project uses Bluetooth Classic.

## Supported Hardware
I'm using UGREEN Bluetooth Dongle *TODO insert bt version + chipset*. As this library needs to take exclusive access of the Bluetooth dongle the default driver must be replaced with WinUSB on Windows using a tool like Zadig. 

## Commandline Flags
* `BTSNOOP_LOG`: When set to a valid path the system will create a log file containing all sent and received packets, which can be read using software like [Wireshark](https://www.wireshark.org/).
* *TODO Realtek firmware path*

## Specifications
The source code contains references to the following specifications:
* [Core 5.4](https://www.bluetooth.com/specifications/specs/core-specification-5-4/)
* [Assigned Numbers](https://www.bluetooth.com/specifications/assigned-numbers/)
* [AVDTP 1.3](https://www.bluetooth.com/specifications/specs/a-v-distribution-transport-protocol-1-3/)
* [A2DP 1.4](https://www.bluetooth.com/specifications/specs/advanced-audio-distribution-profile-1-4/)
* [AVCTP 1.4](https://www.bluetooth.com/specifications/specs/a-v-control-transport-protocol-1-4/)
* [AVRCP 1.6.2](https://www.bluetooth.com/specifications/specs/a-v-remote-control-profile-1-6-2/)
* [AVC 4.1](https://www.bluetooth.com/specifications/AVC-Digital-Interface-Command-Set-4.1)
* [AVC Panel 1.1](https://www.bluetooth.com/specifications/AVC-Panel-Subunit-1.1)

# Related Projects:
* [Bumble](https://github.com/google/bumble) - A dual-mode Bluetooth stack in Python
* [Burble](https://github.com/mxk/burble) - A BLE stack in Rust
