// XInput (xinput1_1 to xinput1_4, xinput9_1_0): the game controller.
//
// Controller 0 is the page's: the on-screen gamepad, or a real controller the browser
// sees (Gamepad API). The page sends its state with win_pad; games read it here the way
// they read an Xbox controller. Controllers 1 to 3 are never connected.

use super::*;

const ERROR_SUCCESS: u64 = 0;
const ERROR_EMPTY: u64 = 4306;
const ERROR_DEVICE_NOT_CONNECTED: u64 = 1167;

pub(super) fn call(e: &mut Engine, name: &str) -> Option<u64> {
    let user = e.arg(0) as u32;
    Some(match name {
        "XInputGetState" | "XInputGetStateEx" => {
            if !connected(e, user) {
                return Some(ERROR_DEVICE_NOT_CONNECTED);
            }
            let out = e.arg(1);
            get_state(e, out);
            ERROR_SUCCESS
        }
        "XInputGetCapabilities" | "XInputGetCapabilitiesEx" => {
            if !connected(e, user) {
                return Some(ERROR_DEVICE_NOT_CONNECTED);
            }
            let out = e.arg(2);
            get_capabilities(e, out);
            ERROR_SUCCESS
        }
        // vibration: the page has no motor to drive
        "XInputSetState" => {
            if connected(e, user) {
                ERROR_SUCCESS
            } else {
                ERROR_DEVICE_NOT_CONNECTED
            }
        }
        "XInputEnable" => 0,
        "XInputGetBatteryInformation" => {
            if !connected(e, user) {
                return Some(ERROR_DEVICE_NOT_CONNECTED);
            }
            let out = e.arg(2);
            e.w8(out, 1); // BATTERY_TYPE_WIRED
            e.w8(out + 1, 3); // BATTERY_LEVEL_FULL
            ERROR_SUCCESS
        }
        "XInputGetKeystroke" => {
            if connected(e, user) {
                ERROR_EMPTY
            } else {
                ERROR_DEVICE_NOT_CONNECTED
            }
        }
        "XInputGetDSoundAudioDeviceGuids" | "XInputGetAudioDeviceIds" => ERROR_DEVICE_NOT_CONNECTED,
        _ => return e.not_mine(),
    })
}

fn connected(e: &Engine, user: u32) -> bool {
    user == 0 && e.s.pad_on
}

/// XINPUT_STATE: packet number, then XINPUT_GAMEPAD (buttons, triggers, four thumb axes).
fn get_state(e: &mut Engine, out: u64) {
    let packet = e.s.pad_packet;
    let pad = e.s.pad;
    e.w32(out, packet);
    let _ = e.mmu.write_bytes(out + 4, &pad);
}

/// XINPUT_CAPABILITIES: a gamepad with every button, both triggers and both sticks.
fn get_capabilities(e: &mut Engine, out: u64) {
    e.w8(out, 1); // XINPUT_DEVTYPE_GAMEPAD
    e.w8(out + 1, 1); // XINPUT_DEVSUBTYPE_GAMEPAD
    e.w16(out + 2, 0);
    e.w16(out + 4, 0xF3FF); // every button
    e.w8(out + 6, 255);
    e.w8(out + 7, 255);
    for k in 0..4 {
        e.w16(out + 8 + 2 * k, 0xFFC0);
    }
    e.w16(out + 16, 255); // vibration motors
    e.w16(out + 18, 255);
}

/// The page's controller: on (connected), buttons | left trigger << 16 | right trigger << 24,
/// left stick x | y << 16, right stick x | y << 16 (XInput axis values, up is positive).
#[no_mangle]
pub extern "C" fn win_pad(on: u32, buttons: u32, left: u32, right: u32) {
    let e = crate::eng();
    let _g = crate::lock::lock();
    let mut pad = [0u8; 12];
    pad[..4].copy_from_slice(&buttons.to_le_bytes());
    pad[4..8].copy_from_slice(&left.to_le_bytes());
    pad[8..].copy_from_slice(&right.to_le_bytes());
    if pad != e.s.pad || (on != 0) != e.s.pad_on {
        e.s.pad_packet = e.s.pad_packet.wrapping_add(1);
    }
    e.s.pad = pad;
    e.s.pad_on = on != 0;
}
