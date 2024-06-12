# Bluefang

A experimental, very non-conformant, pure Rust Bluetooth stack.
This project focuses on implementing an audio speaker with remote control functionality. As BLE Audio is not supported by most of the devices I own, this Project uses Bluetooth Classic.

## Supported Hardware
I'm using UGREEN Bluetooth 5.0 Adapter.

Firmware upload is currently only supported for Realtek chips.

As this library needs to take exclusive access of the Bluetooth dongle some platforms require additional setup:
* **Windows**: The default driver must be replaced with WinUSB using a tool like [Zadig](https://zadig.akeo.ie/).
* **Linux**: The user running this executable must have access to the Bluetooth device. This can be achieved by adding the correct udev rules or by running the executable as root.

## Running

### Optional: Download Realtek firmware files
```bash
mkdir firmware && cd firmware
wget -qO- https://kernel.googlesource.com/pub/scm/linux/kernel/git/firmware/linux-firmware/+archive/refs/heads/main/rtl_bt.tar.gz | tar xvz
```

### Run the AudioSink example
Go to `examples/audio_sink.rs` and change the vendor id filter of the usb device enumeration to the vendor of your bluetooth dongle.
```bash
cargo run --example audio_sink --release
```


## Commandline Flags
* `BTSNOOP_LOG`: When set to a valid path the system will create a log file containing all sent and received packets, which can be read using software like [Wireshark](https://www.wireshark.org/).
* `RUST_LOG`: Change the log level of the examples. For example, `RUST_LOG=debug` will show debug logs.

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

## Related Projects:
* [Bumble](https://github.com/google/bumble) - A dual-mode Bluetooth stack in Python
* [Burble](https://github.com/mxk/burble) - A BLE stack in Rust
