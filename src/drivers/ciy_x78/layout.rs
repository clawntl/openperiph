//! The keyboard's physical keys: where each one is drawn, which Windows key it sends by default,
//! and which slot of the keyboard's key table ("matrix index") it lives in.
//!
//! Positions and slot numbers come from the original program's Cfg.ini `[KEY]` section
//! (x1, y1, x2, y2, type, Windows key, -, matrix index, light index). See MECHANICS.md.

/// One physical key.
#[derive(Clone, Copy, Debug)]
pub struct KeyDef {
    /// Number used by the original's `[KEY]`/`[FN]` sections (K1..K88).
    pub id: u8,
    /// Short label drawn on the key.
    pub label: &'static str,
    /// Rectangle in the layout's own coordinate space (about 720 x 270).
    pub rect: [f32; 4],
    /// Windows virtual-key code the key sends out of the box.
    pub vk: u8,
    /// Slot in the keyboard's key table (column * 6 + row).
    pub matrix: u8,
}

const fn k(id: u8, label: &'static str, x1: f32, y1: f32, x2: f32, y2: f32, vk: u8, matrix: u8) -> KeyDef {
    KeyDef { id, label, rect: [x1, y1, x2, y2], vk, matrix }
}

/// The 88 keys of this UK ISO tenkeyless board (labels are UK legends).
pub const KEYS: &[KeyDef] = &[
    k(1, "Esc", 26., 39., 54., 67., 0x1B, 0),
    k(2, "F1", 101., 38., 129., 66., 0x70, 12),
    k(3, "F2", 138., 38., 166., 66., 0x71, 18),
    k(4, "F3", 175., 38., 203., 66., 0x72, 24),
    k(5, "F4", 213., 38., 241., 66., 0x73, 30),
    k(6, "F5", 269., 38., 297., 66., 0x74, 42),
    k(7, "F6", 305., 39., 333., 67., 0x75, 48),
    k(8, "F7", 342., 39., 370., 67., 0x76, 54),
    k(9, "F8", 378., 39., 406., 67., 0x77, 60),
    k(10, "F9", 435., 39., 463., 67., 0x78, 66),
    k(11, "F10", 472., 39., 500., 67., 0x79, 72),
    k(12, "F11", 509., 39., 537., 67., 0x7A, 78),
    k(13, "F12", 547., 39., 575., 67., 0x7B, 84),
    k(14, "PrtSc", 593., 39., 621., 67., 0x2C, 90),
    k(15, "ScrLk", 631., 39., 659., 67., 0x91, 96),
    k(16, "Pause", 667., 39., 695., 67., 0x13, 102),
    k(17, "`¬", 29., 86., 57., 114., 0xC0, 1),
    k(18, "1", 64., 86., 92., 114., 0x31, 7),
    k(19, "2 \"", 100., 86., 128., 114., 0x32, 13),
    k(20, "3 £", 138., 85., 166., 113., 0x33, 19),
    k(21, "4", 175., 85., 203., 113., 0x34, 25),
    k(22, "5", 212., 85., 240., 113., 0x35, 31),
    k(23, "6", 249., 85., 277., 113., 0x36, 37),
    k(24, "7", 287., 85., 315., 113., 0x37, 43),
    k(25, "8", 323., 85., 351., 113., 0x38, 49),
    k(26, "9", 360., 85., 388., 113., 0x39, 55),
    k(27, "0", 398., 85., 426., 113., 0x30, 61),
    k(28, "-", 434., 85., 462., 113., 0xBD, 67),
    k(29, "=", 471., 85., 499., 113., 0xBB, 73),
    k(30, "Backspace", 510., 85., 574., 113., 0x08, 85),
    k(31, "Ins", 594., 86., 622., 114., 0x2D, 91),
    k(32, "Home", 630., 86., 658., 114., 0x24, 97),
    k(33, "PgUp", 667., 86., 695., 114., 0x21, 103),
    k(34, "Tab", 30., 123., 72., 151., 0x09, 2),
    k(35, "Q", 82., 123., 110., 151., 0x51, 8),
    k(36, "W", 120., 123., 148., 151., 0x57, 14),
    k(37, "E", 156., 123., 184., 151., 0x45, 20),
    k(38, "R", 194., 123., 222., 151., 0x52, 26),
    k(39, "T", 231., 123., 259., 151., 0x54, 32),
    k(40, "Y", 268., 123., 296., 151., 0x59, 38),
    k(41, "U", 306., 123., 334., 151., 0x55, 44),
    k(42, "I", 343., 123., 371., 151., 0x49, 50),
    k(43, "O", 380., 123., 408., 151., 0x4F, 56),
    k(44, "P", 418., 123., 446., 151., 0x50, 62),
    k(45, "[", 454., 123., 482., 151., 0xDB, 68),
    k(46, "]", 491., 123., 519., 151., 0xDD, 74),
    k(47, "#~", 500., 160., 528., 188., 0xDC, 80),
    k(48, "Del", 593., 123., 621., 151., 0x2E, 92),
    k(49, "End", 629., 123., 657., 151., 0x23, 98),
    k(50, "PgDn", 666., 123., 694., 151., 0x22, 104),
    k(51, "Caps", 30., 160., 80., 188., 0x14, 3),
    k(52, "A", 92., 160., 120., 188., 0x41, 9),
    k(53, "S", 130., 160., 158., 188., 0x53, 15),
    k(54, "D", 167., 159., 195., 187., 0x44, 21),
    k(55, "F", 204., 159., 232., 187., 0x46, 27),
    k(56, "G", 241., 160., 269., 188., 0x47, 33),
    k(57, "H", 279., 160., 307., 188., 0x48, 39),
    k(58, "J", 314., 160., 342., 188., 0x4A, 45),
    k(59, "K", 351., 160., 379., 188., 0x4B, 51),
    k(60, "L", 389., 160., 417., 188., 0x4C, 57),
    k(61, ";", 426., 160., 454., 188., 0xBA, 63),
    k(62, "'@", 463., 160., 491., 188., 0xDE, 69),
    k(63, "Enter", 528., 124., 574., 152., 0x0D, 81),
    k(64, "Shift", 29., 196., 63., 224., 0xA0, 4),
    k(65, "Z", 109., 197., 137., 225., 0x5A, 10),
    k(66, "X", 146., 197., 174., 225., 0x58, 16),
    k(67, "C", 183., 197., 211., 225., 0x43, 22),
    k(68, "V", 221., 197., 249., 225., 0x56, 28),
    k(69, "B", 258., 197., 286., 225., 0x42, 34),
    k(70, "N", 295., 197., 323., 225., 0x4E, 40),
    k(71, "M", 332., 198., 360., 226., 0x4D, 46),
    k(72, ",", 370., 197., 398., 225., 0xBC, 52),
    k(73, ".", 407., 197., 435., 225., 0xBE, 58),
    k(74, "/", 444., 197., 472., 225., 0xBF, 64),
    k(75, "Shift", 482., 197., 572., 225., 0xA1, 82),
    k(76, "Up", 631., 197., 659., 225., 0x26, 100),
    k(77, "Ctrl", 30., 233., 64., 261., 0xA2, 5),
    k(78, "Win", 74., 233., 108., 261., 0x5B, 11),
    k(79, "Alt", 120., 234., 154., 262., 0xA4, 17),
    k(80, "Space", 165., 234., 385., 262., 0x20, 35),
    k(81, "Alt Gr", 401., 234., 435., 262., 0xA5, 53),
    k(82, "Fn", 448., 234., 482., 262., 0xFA, 59),
    k(83, "Menu", 493., 234., 527., 262., 0x5D, 65),
    k(84, "Ctrl", 538., 234., 572., 262., 0xA3, 77),
    k(85, "Left", 594., 234., 622., 262., 0x25, 95),
    k(86, "Down", 631., 233., 659., 261., 0x28, 101),
    k(87, "Right", 668., 233., 696., 261., 0x27, 107),
    k(88, "\\|", 73., 197., 101., 225., 0xE2, 88),
];

/// The Fn key itself (it can't be remapped: it switches layers).
pub const FN_KEY_ID: u8 = 82;

/// Default Fn-layer functions, in the order Cfg.ini `[FN]` lists them. The position in this list
/// is the Fn slot number the app gives the key. Values are the program's internal 32-bit codes;
/// the bytes sent are these codes high byte first.
pub const DEFAULT_FN: &[(u8, u32, &str)] = &[
    (1, 0x0e00_0004, "Keyboard function (Esc)"),
    (2, 0x0400_0194, "My Computer"),
    (3, 0x0400_0223, "Browser home"),
    (4, 0x0400_018a, "Email"),
    (5, 0x0400_0192, "Calculator"),
    (6, 0x0400_00b7, "Stop"),
    (7, 0x0400_00b6, "Previous track"),
    (8, 0x0400_00cd, "Play / Pause"),
    (9, 0x0400_00b5, "Next track"),
    (10, 0x0b00_0300, "Keyboard function (F9)"),
    (11, 0x0400_00ea, "Volume down"),
    (12, 0x0400_00e9, "Volume up"),
    (13, 0x0400_00e2, "Mute"),
    (78, 0x0e00_0001, "Windows key lock"),
    (83, 0x0e00_0008, "Keyboard function (Menu)"),
    (14, 0x0e00_000d, "Keyboard function (PrtSc)"),
    (15, 0x0e00_000e, "Keyboard function (ScrLk)"),
    (31, 0x0e00_000c, "Keyboard function (Ins)"),
    (32, 0x0e00_000f, "Keyboard function (Home)"),
    (33, 0x0c00_0100, "Keyboard function (PgUp)"),
    (48, 0x0e00_0005, "Keyboard function (Del)"),
    (49, 0x0e00_0002, "Keyboard function (End)"),
    (50, 0x0c00_0200, "Keyboard function (PgDn)"),
    (85, 0x0e00_000a, "Previous light effect"),
    (87, 0x0e00_000b, "Next light effect"),
    (76, 0x1200_0100, "Brightness up"),
    (86, 0x1200_0200, "Brightness down"),
    (28, 0x0d00_0200, "Light speed down"),
    (29, 0x0d00_0100, "Light speed up"),
    (18, 0x0b00_0020, "Keyboard function (1)"),
    (19, 0x0b00_0021, "Keyboard function (2)"),
    (20, 0x0b00_0022, "Keyboard function (3)"),
    (21, 0x0b00_0023, "Keyboard function (4)"),
    (22, 0x0b00_0024, "Keyboard function (5)"),
];

/// Bounding box of the whole layout, in layout units.
pub const LAYOUT_W: f32 = 682.0;
pub const LAYOUT_H: f32 = 236.0;

pub fn key_by_id(id: u8) -> Option<&'static KeyDef> {
    KEYS.iter().find(|k| k.id == id)
}
