//! The VIA protocol: how an app reads and changes a QMK keyboard's keymap, macros and lighting
//! while it is plugged in. Every message is 32 bytes on QMK's raw HID interface (usage page FF60,
//! usage 61); the keyboard answers each one with 32 bytes that start with the same command byte,
//! or with 0xFF when it doesn't know the command.
//!
//! Based on QMK's `quantum/via.c` and `via.h` (protocol version 12).

use std::time::Duration;

use crate::hid;

pub const USAGE_PAGE: u16 = 0xFF60;
pub const USAGE: u16 = 0x61;
pub const MSG_LEN: usize = 32;

pub const CMD_PROTOCOL_VERSION: u8 = 0x01;
pub const CMD_GET_KEYBOARD_VALUE: u8 = 0x02;
pub const CMD_SET_KEYBOARD_VALUE: u8 = 0x03;
pub const CMD_KEYMAP_GET_KEYCODE: u8 = 0x04;
pub const CMD_KEYMAP_SET_KEYCODE: u8 = 0x05;
pub const CMD_KEYMAP_RESET: u8 = 0x06;
pub const CMD_CUSTOM_SET_VALUE: u8 = 0x07;
pub const CMD_CUSTOM_GET_VALUE: u8 = 0x08;
pub const CMD_CUSTOM_SAVE: u8 = 0x09;
pub const CMD_BOOTLOADER_JUMP: u8 = 0x0B;
pub const CMD_MACRO_GET_COUNT: u8 = 0x0C;
pub const CMD_MACRO_GET_BUFFER_SIZE: u8 = 0x0D;
pub const CMD_MACRO_GET_BUFFER: u8 = 0x0E;
pub const CMD_MACRO_SET_BUFFER: u8 = 0x0F;
pub const CMD_MACRO_RESET: u8 = 0x10;
pub const CMD_KEYMAP_GET_LAYER_COUNT: u8 = 0x11;
pub const CMD_KEYMAP_GET_BUFFER: u8 = 0x12;
pub const CMD_KEYMAP_SET_BUFFER: u8 = 0x13;
pub const UNHANDLED: u8 = 0xFF;

pub const VALUE_UPTIME: u8 = 0x01;
pub const VALUE_LAYOUT_OPTIONS: u8 = 0x02;
pub const VALUE_SWITCH_MATRIX: u8 = 0x03;
pub const VALUE_FIRMWARE_VERSION: u8 = 0x04;

/// Largest payload of one buffer read or write (32 bytes minus the 4-byte header).
const CHUNK: usize = 28;

/// Something that can exchange VIA messages: the real keyboard, or a simulated one in tests.
pub trait Transport: Send {
    fn exchange(&mut self, msg: &[u8; MSG_LEN]) -> Result<[u8; MSG_LEN], String>;
}

impl<T: Transport + ?Sized> Transport for Box<T> {
    fn exchange(&mut self, msg: &[u8; MSG_LEN]) -> Result<[u8; MSG_LEN], String> {
        (**self).exchange(msg)
    }
}

/// The keyboard over raw HID.
pub struct HidTransport {
    dev: hid::Handle,
    out_len: usize,
    in_len: usize,
}

impl HidTransport {
    pub fn open(c: &hid::Collection) -> Result<HidTransport, String> {
        let dev = hid::open_io(&c.path)?;
        // Report lengths include the report ID byte; QMK's are 32 + 1.
        let out_len = (c.output_len as usize).max(MSG_LEN + 1);
        let in_len = (c.input_len as usize).max(MSG_LEN + 1);
        let mut t = HidTransport { dev, out_len, in_len };
        t.drain();
        Ok(t)
    }

    /// Throws away answers left over from an earlier session.
    fn drain(&mut self) {
        let mut buf = vec![0u8; self.in_len];
        while self.dev.read(&mut buf, 5).is_some() {}
    }
}

impl Transport for HidTransport {
    fn exchange(&mut self, msg: &[u8; MSG_LEN]) -> Result<[u8; MSG_LEN], String> {
        let mut out = vec![0u8; self.out_len];
        out[1..1 + MSG_LEN].copy_from_slice(msg);
        if !self.dev.write(&out, 500) {
            return Err("The keyboard didn't accept a message (was it unplugged?).".into());
        }
        let mut buf = vec![0u8; self.in_len];
        // Skip anything that isn't the answer to this message.
        for _ in 0..8 {
            let Some(n) = self.dev.read(&mut buf, 500) else {
                return Err("The keyboard didn't answer.".into());
            };
            if n < MSG_LEN + 1 {
                continue;
            }
            let mut r = [0u8; MSG_LEN];
            r.copy_from_slice(&buf[1..1 + MSG_LEN]);
            if r[0] == msg[0] || r[0] == UNHANDLED {
                return Ok(r);
            }
        }
        Err("The keyboard sent unexpected answers.".into())
    }
}

/// A VIA client on top of any transport.
pub struct Via<T: Transport> {
    pub t: T,
}

fn msg(bytes: &[u8]) -> [u8; MSG_LEN] {
    let mut m = [0u8; MSG_LEN];
    m[..bytes.len()].copy_from_slice(bytes);
    m
}

impl<T: Transport> Via<T> {
    pub fn new(t: T) -> Via<T> {
        Via { t }
    }

    /// Sends a message; an unknown command is an error.
    fn ask(&mut self, bytes: &[u8]) -> Result<[u8; MSG_LEN], String> {
        let r = self.t.exchange(&msg(bytes))?;
        if r[0] == UNHANDLED && bytes[0] != UNHANDLED {
            return Err(format!("The keyboard's firmware doesn't support command 0x{:02X}.", bytes[0]));
        }
        Ok(r)
    }

    /// Like `ask`, but `None` when the firmware doesn't have the command.
    fn ask_opt(&mut self, bytes: &[u8]) -> Result<Option<[u8; MSG_LEN]>, String> {
        let r = self.t.exchange(&msg(bytes))?;
        Ok(if r[0] == UNHANDLED { None } else { Some(r) })
    }

    pub fn protocol_version(&mut self) -> Result<u16, String> {
        let r = self.ask(&[CMD_PROTOCOL_VERSION])?;
        Ok(u16::from_be_bytes([r[1], r[2]]))
    }

    fn keyboard_value_u32(&mut self, id: u8) -> Result<Option<u32>, String> {
        Ok(self.ask_opt(&[CMD_GET_KEYBOARD_VALUE, id])?.map(|r| u32::from_be_bytes([r[2], r[3], r[4], r[5]])))
    }

    pub fn layout_options(&mut self) -> Result<u32, String> {
        Ok(self.keyboard_value_u32(VALUE_LAYOUT_OPTIONS)?.unwrap_or(0))
    }

    pub fn set_layout_options(&mut self, v: u32) -> Result<(), String> {
        let b = v.to_be_bytes();
        self.ask(&[CMD_SET_KEYBOARD_VALUE, VALUE_LAYOUT_OPTIONS, b[0], b[1], b[2], b[3]]).map(|_| ())
    }

    pub fn firmware_version(&mut self) -> Result<Option<u32>, String> {
        self.keyboard_value_u32(VALUE_FIRMWARE_VERSION)
    }

    pub fn layer_count(&mut self) -> Result<u8, String> {
        Ok(self.ask(&[CMD_KEYMAP_GET_LAYER_COUNT])?[1])
    }

    /// The whole keymap: `layers` x `rows` x `cols` keycodes.
    pub fn read_keymap(&mut self, layers: usize, rows: usize, cols: usize) -> Result<Vec<Vec<Vec<u16>>>, String> {
        let total = layers * rows * cols * 2;
        let mut bytes = Vec::with_capacity(total);
        while bytes.len() < total {
            let off = bytes.len() as u16;
            let n = CHUNK.min(total - bytes.len());
            let o = off.to_be_bytes();
            let r = self.ask(&[CMD_KEYMAP_GET_BUFFER, o[0], o[1], n as u8])?;
            bytes.extend_from_slice(&r[4..4 + n]);
        }
        let mut out = vec![vec![vec![0u16; cols]; rows]; layers];
        for l in 0..layers {
            for r in 0..rows {
                for c in 0..cols {
                    let i = ((l * rows + r) * cols + c) * 2;
                    out[l][r][c] = u16::from_be_bytes([bytes[i], bytes[i + 1]]);
                }
            }
        }
        Ok(out)
    }

    /// Writes the whole keymap (layers x rows x cols) in 28-byte pieces.
    pub fn write_keymap(&mut self, keymap: &[Vec<Vec<u16>>]) -> Result<(), String> {
        let mut bytes = Vec::new();
        for layer in keymap {
            for row in layer {
                for k in row {
                    bytes.extend_from_slice(&k.to_be_bytes());
                }
            }
        }
        let mut off = 0usize;
        while off < bytes.len() {
            let n = CHUNK.min(bytes.len() - off);
            let o = (off as u16).to_be_bytes();
            let mut m = vec![CMD_KEYMAP_SET_BUFFER, o[0], o[1], n as u8];
            m.extend_from_slice(&bytes[off..off + n]);
            self.ask(&m)?;
            off += n;
        }
        Ok(())
    }

    pub fn set_keycode(&mut self, layer: u8, row: u8, col: u8, code: u16) -> Result<(), String> {
        let k = code.to_be_bytes();
        let r = self.ask(&[CMD_KEYMAP_SET_KEYCODE, layer, row, col, k[0], k[1]])?;
        // The keyboard echoes the message; check it kept the value.
        let back = self.ask(&[CMD_KEYMAP_GET_KEYCODE, layer, row, col])?;
        let _ = r;
        if u16::from_be_bytes([back[4], back[5]]) != code {
            return Err("The keyboard didn't keep the new key.".into());
        }
        Ok(())
    }

    pub fn reset_keymap(&mut self) -> Result<(), String> {
        self.ask(&[CMD_KEYMAP_RESET]).map(|_| ())
    }

    /// Reads a lighting or keyboard-specific value: `[channel, value id]` -> the value's bytes.
    pub fn get_value(&mut self, channel: u8, id: u8, len: usize) -> Result<Option<Vec<u8>>, String> {
        Ok(self.ask_opt(&[CMD_CUSTOM_GET_VALUE, channel, id])?.map(|r| r[3..3 + len.min(MSG_LEN - 3)].to_vec()))
    }

    pub fn set_value(&mut self, channel: u8, id: u8, data: &[u8]) -> Result<(), String> {
        let mut m = vec![CMD_CUSTOM_SET_VALUE, channel, id];
        m.extend_from_slice(&data[..data.len().min(MSG_LEN - 3)]);
        self.ask(&m).map(|_| ())
    }

    /// Makes the keyboard store a channel's values in its memory.
    pub fn save_values(&mut self, channel: u8) -> Result<(), String> {
        self.ask_opt(&[CMD_CUSTOM_SAVE, channel]).map(|_| ())
    }

    /// Whether the firmware has this lighting channel (1 backlight, 2 underglow, 3 RGB matrix,
    /// 4 audio, 5 LED matrix).
    pub fn has_channel(&mut self, channel: u8) -> Result<bool, String> {
        Ok(self.ask_opt(&[CMD_CUSTOM_GET_VALUE, channel, 1])?.is_some())
    }

    pub fn macro_count(&mut self) -> Result<u8, String> {
        Ok(self.ask_opt(&[CMD_MACRO_GET_COUNT])?.map(|r| r[1]).unwrap_or(0))
    }

    pub fn macro_buffer_size(&mut self) -> Result<u16, String> {
        Ok(self.ask_opt(&[CMD_MACRO_GET_BUFFER_SIZE])?.map(|r| u16::from_be_bytes([r[1], r[2]])).unwrap_or(0))
    }

    pub fn read_macro_buffer(&mut self, size: usize) -> Result<Vec<u8>, String> {
        let mut out = Vec::with_capacity(size);
        while out.len() < size {
            let off = (out.len() as u16).to_be_bytes();
            let n = CHUNK.min(size - out.len());
            let r = self.ask(&[CMD_MACRO_GET_BUFFER, off[0], off[1], n as u8])?;
            out.extend_from_slice(&r[4..4 + n]);
            // Everything after the last macro is unused; stop once only zeros remain.
            if out.len() >= size || out.iter().rev().take(CHUNK * 2).all(|b| *b == 0) && out.len() > CHUNK * 2 {
                break;
            }
        }
        Ok(out)
    }

    /// Writes the macro buffer. The last byte of the buffer is written first as non-zero and
    /// last as zero, the same way VIA does, so a half-written buffer is never played back.
    pub fn write_macro_buffer(&mut self, data: &[u8], size: usize) -> Result<(), String> {
        if data.len() > size {
            return Err(format!("The macros need {} bytes; the keyboard has room for {size}.", data.len()));
        }
        let last = (size as u16 - 1).to_be_bytes();
        self.ask(&[CMD_MACRO_SET_BUFFER, last[0], last[1], 1, 0xFF])?;
        let mut off = 0usize;
        while off < data.len() {
            let n = CHUNK.min(data.len() - off);
            let o = (off as u16).to_be_bytes();
            let mut m = vec![CMD_MACRO_SET_BUFFER, o[0], o[1], n as u8];
            m.extend_from_slice(&data[off..off + n]);
            self.ask(&m)?;
            off += n;
        }
        self.ask(&[CMD_MACRO_SET_BUFFER, last[0], last[1], 1, 0x00])?;
        Ok(())
    }

    pub fn reset_macros(&mut self) -> Result<(), String> {
        self.ask(&[CMD_MACRO_RESET]).map(|_| ())
    }

    /// Restarts into the bootloader, if the firmware allows it from VIA.
    pub fn bootloader(&mut self) -> Result<bool, String> {
        // The keyboard resets before it can answer, so no answer is the expected outcome.
        match self.t.exchange(&msg(&[CMD_BOOTLOADER_JUMP])) {
            Ok(r) => Ok(r[0] != UNHANDLED),
            Err(_) => Ok(true),
        }
    }
}

/// Waits briefly between retries when the keyboard is busy writing its memory.
#[allow(dead_code)]
pub fn settle() {
    std::thread::sleep(Duration::from_millis(5));
}

// ---------------------------------------------------------------------------------------------
// A simulated QMK keyboard, for tests and the --demo option

/// Behaves like QMK's via.c with dynamic keymap, macros and RGB Matrix.
pub struct FakeKeyboard {
    pub rows: usize,
    pub cols: usize,
    pub layers: usize,
    pub keymap: Vec<u16>,
    pub macros: Vec<u8>,
    pub layout_options: u32,
    /// RGB Matrix: brightness, effect, speed, hue, saturation.
    pub rgb: [u8; 5],
    pub saved_rgb: [u8; 5],
    pub default_keymap: Vec<u16>,
}

impl FakeKeyboard {
    pub fn new(rows: usize, cols: usize, layers: usize, base: &[u16]) -> FakeKeyboard {
        let mut keymap = vec![0u16; layers * rows * cols];
        for (i, k) in base.iter().enumerate().take(rows * cols) {
            keymap[i] = *k;
        }
        for l in 1..layers {
            for i in 0..rows * cols {
                keymap[l * rows * cols + i] = 0x0001;
            }
        }
        FakeKeyboard {
            rows,
            cols,
            layers,
            default_keymap: keymap.clone(),
            keymap,
            macros: vec![0u8; 512],
            layout_options: 0,
            rgb: [200, 5, 128, 170, 255],
            saved_rgb: [200, 5, 128, 170, 255],
        }
    }
}

impl Transport for FakeKeyboard {
    fn exchange(&mut self, m: &[u8; MSG_LEN]) -> Result<[u8; MSG_LEN], String> {
        let mut r = *m;
        let be16 = |a: u8, b: u8| u16::from_be_bytes([a, b]) as usize;
        match m[0] {
            CMD_PROTOCOL_VERSION => {
                r[1] = 0;
                r[2] = 12;
            }
            CMD_GET_KEYBOARD_VALUE => match m[1] {
                VALUE_UPTIME => r[2..6].copy_from_slice(&123456u32.to_be_bytes()),
                VALUE_LAYOUT_OPTIONS => r[2..6].copy_from_slice(&self.layout_options.to_be_bytes()),
                VALUE_FIRMWARE_VERSION => r[2..6].copy_from_slice(&0u32.to_be_bytes()),
                VALUE_SWITCH_MATRIX => {
                    for b in &mut r[3..] {
                        *b = 0;
                    }
                }
                _ => r[0] = UNHANDLED,
            },
            CMD_SET_KEYBOARD_VALUE => match m[1] {
                VALUE_LAYOUT_OPTIONS => self.layout_options = u32::from_be_bytes([m[2], m[3], m[4], m[5]]),
                _ => r[0] = UNHANDLED,
            },
            CMD_KEYMAP_GET_KEYCODE | CMD_KEYMAP_SET_KEYCODE => {
                let (l, row, c) = (m[1] as usize, m[2] as usize, m[3] as usize);
                if l < self.layers && row < self.rows && c < self.cols {
                    let i = (l * self.rows + row) * self.cols + c;
                    if m[0] == CMD_KEYMAP_SET_KEYCODE {
                        self.keymap[i] = u16::from_be_bytes([m[4], m[5]]);
                    }
                    r[4..6].copy_from_slice(&self.keymap[i].to_be_bytes());
                }
            }
            CMD_KEYMAP_RESET => self.keymap = self.default_keymap.clone(),
            CMD_KEYMAP_GET_LAYER_COUNT => r[1] = self.layers as u8,
            CMD_KEYMAP_SET_BUFFER => {
                let (off, n) = (be16(m[1], m[2]), (m[3] as usize).min(28));
                for k in 0..n {
                    let b = off + k;
                    if let Some(w) = self.keymap.get_mut(b / 2) {
                        let mut bytes = w.to_be_bytes();
                        bytes[b % 2] = m[4 + k];
                        *w = u16::from_be_bytes(bytes);
                    }
                }
            }
            CMD_KEYMAP_GET_BUFFER => {
                let (off, n) = (be16(m[1], m[2]), m[3] as usize);
                for k in 0..n.min(28) {
                    let b = off + k;
                    let w = self.keymap.get(b / 2).copied().unwrap_or(0).to_be_bytes();
                    r[4 + k] = w[b % 2];
                }
            }
            CMD_CUSTOM_GET_VALUE | CMD_CUSTOM_SET_VALUE | CMD_CUSTOM_SAVE => {
                if m[1] != 3 {
                    r[0] = UNHANDLED;
                } else if m[0] == CMD_CUSTOM_SAVE {
                    self.saved_rgb = self.rgb;
                } else {
                    let set = m[0] == CMD_CUSTOM_SET_VALUE;
                    match m[2] {
                        1 | 2 | 3 => {
                            let i = (m[2] - 1) as usize;
                            if set {
                                self.rgb[i] = m[3];
                            }
                            r[3] = self.rgb[i];
                        }
                        4 => {
                            if set {
                                self.rgb[3] = m[3];
                                self.rgb[4] = m[4];
                            }
                            r[3] = self.rgb[3];
                            r[4] = self.rgb[4];
                        }
                        _ => {}
                    }
                }
            }
            CMD_MACRO_GET_COUNT => r[1] = 16,
            CMD_MACRO_GET_BUFFER_SIZE => r[1..3].copy_from_slice(&(self.macros.len() as u16).to_be_bytes()),
            CMD_MACRO_GET_BUFFER | CMD_MACRO_SET_BUFFER => {
                let (off, n) = (be16(m[1], m[2]), (m[3] as usize).min(28));
                for k in 0..n {
                    if off + k < self.macros.len() {
                        if m[0] == CMD_MACRO_SET_BUFFER {
                            self.macros[off + k] = m[4 + k];
                        }
                        r[4 + k] = self.macros[off + k];
                    }
                }
            }
            CMD_MACRO_RESET => self.macros.fill(0),
            _ => r[0] = UNHANDLED,
        }
        Ok(r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kb() -> Via<FakeKeyboard> {
        let base: Vec<u16> = (0..9 * 8).map(|i| 4 + i as u16 % 26).collect();
        Via::new(FakeKeyboard::new(9, 8, 4, &base))
    }

    #[test]
    fn reads_and_writes_the_keymap() {
        let mut v = kb();
        assert_eq!(v.protocol_version().unwrap(), 12);
        assert_eq!(v.layer_count().unwrap(), 4);
        let km = v.read_keymap(4, 9, 8).unwrap();
        assert_eq!(km[0][0][0], 4);
        assert_eq!(km[0][8][7], 4 + 71 % 26);
        assert_eq!(km[3][5][5], 1);
        v.set_keycode(1, 2, 3, 0x5221).unwrap();
        assert_eq!(v.read_keymap(4, 9, 8).unwrap()[1][2][3], 0x5221);
        let mut km = v.read_keymap(4, 9, 8).unwrap();
        km[2][0][1] = 0x7C00;
        v.write_keymap(&km).unwrap();
        assert_eq!(v.read_keymap(4, 9, 8).unwrap(), km);
        v.reset_keymap().unwrap();
        assert_eq!(v.read_keymap(4, 9, 8).unwrap()[1][2][3], 1);
    }

    #[test]
    fn macros_and_values() {
        let mut v = kb();
        let size = v.macro_buffer_size().unwrap() as usize;
        let data = b"hello\0\x01\x01\x04world\0".to_vec();
        v.write_macro_buffer(&data, size).unwrap();
        let back = v.read_macro_buffer(size).unwrap();
        assert_eq!(&back[..data.len()], &data[..]);
        assert_eq!(*back.last().unwrap_or(&0), 0);
        assert!(v.has_channel(3).unwrap());
        assert!(!v.has_channel(2).unwrap());
        v.set_value(3, 2, &[7]).unwrap();
        assert_eq!(v.get_value(3, 2, 1).unwrap(), Some(vec![7]));
        v.set_value(3, 4, &[10, 20]).unwrap();
        assert_eq!(v.get_value(3, 4, 2).unwrap(), Some(vec![10, 20]));
        v.save_values(3).unwrap();
        assert_eq!(v.t.saved_rgb[1], 7);
    }
}
