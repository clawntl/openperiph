//! How OpenPeriph looks: colours, fonts, the background picture, frosted-glass panels, line icons
//! and drawings of the hardware.
//!
//! The background is drawn once when the app starts (plain, faint smoke made from noise, or a picture
//! of your own). A blurred copy of it is kept too: panels draw the part of the blurred copy that is
//! behind them under a dark tint, which is what makes them look like frosted glass.

use eframe::egui::{
    self, epaint::Vertex, Align2, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, Mesh, Painter, Pos2, Rect, RichText, Shape, Stroke,
    TextStyle, TextureHandle, TextureOptions, Ui, Vec2,
};
use std::sync::Arc;

// ---------------------------------------------------------------------------------------------
// Colours

pub const BG: Color32 = Color32::from_rgb(18, 21, 28);
pub const BG1: Color32 = Color32::from_rgb(24, 28, 37);
pub const BG2: Color32 = Color32::from_rgb(28, 33, 43);
pub const BG3: Color32 = Color32::from_rgb(36, 42, 54);
pub const LINE: Color32 = Color32::from_rgb(46, 53, 67);
pub const TEXT: Color32 = Color32::from_rgb(232, 236, 242);
pub const TEXT_DIM: Color32 = Color32::from_rgb(152, 161, 176);
pub const TEXT_MUT: Color32 = Color32::from_rgb(104, 113, 130);
pub const ACCENT: Color32 = Color32::from_rgb(142, 230, 168);
/// The second colour, used with the accent in gradients.
pub const SECOND: Color32 = Color32::from_rgb(120, 162, 236);
pub const GOOD: Color32 = Color32::from_rgb(120, 220, 150);
pub const WARN: Color32 = Color32::from_rgb(255, 184, 84);
pub const BAD: Color32 = Color32::from_rgb(255, 96, 104);
/// A button or key at rest.
pub const KEYCAP: Color32 = Color32::from_rgb(40, 46, 58);

/// The semi-bold font, for titles.
pub fn semibold() -> FontFamily {
    FontFamily::Name("semibold".into())
}

/// A card or section title.
pub fn title(text: impl Into<String>) -> RichText {
    RichText::new(text).size(14.0).family(semibold()).color(TEXT)
}

/// A page title.
pub fn page_title(text: impl Into<String>) -> RichText {
    RichText::new(text).size(20.0).family(semibold()).color(TEXT)
}

pub fn dim(text: impl Into<String>) -> RichText {
    RichText::new(text).color(TEXT_DIM)
}

pub fn small(text: impl Into<String>) -> RichText {
    RichText::new(text).size(11.5).color(TEXT_MUT)
}

// ---------------------------------------------------------------------------------------------
// Style and fonts

pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert("inter".into(), Arc::new(FontData::from_static(include_bytes!("../../assets/fonts/Inter-Regular.ttf"))));
    fonts.font_data.insert("inter-semibold".into(), Arc::new(FontData::from_static(include_bytes!("../../assets/fonts/Inter-SemiBold.ttf"))));
    // Inter first; egui's own fonts stay behind it for symbols and emoji.
    let fallback = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    fonts.families.entry(FontFamily::Proportional).or_default().insert(0, "inter".into());
    let mut semi = vec!["inter-semibold".to_string()];
    semi.extend(fallback);
    fonts.families.insert(semibold(), semi);
    ctx.set_fonts(fonts);

    let mut v = egui::Visuals::dark();
    v.panel_fill = Color32::TRANSPARENT;
    v.window_fill = BG2;
    v.window_stroke = Stroke::new(1.0, LINE);
    v.window_corner_radius = CornerRadius::same(12);
    v.window_shadow = egui::epaint::Shadow { offset: [0, 10], blur: 30, spread: 0, color: Color32::from_black_alpha(140) };
    v.popup_shadow = egui::epaint::Shadow { offset: [0, 6], blur: 16, spread: 0, color: Color32::from_black_alpha(120) };
    v.menu_corner_radius = CornerRadius::same(8);
    v.extreme_bg_color = Color32::from_rgb(12, 15, 20);
    v.faint_bg_color = Color32::from_rgba_unmultiplied(255, 255, 255, 6);
    v.code_bg_color = BG3;
    v.hyperlink_color = ACCENT;
    v.selection.bg_fill = ACCENT.gamma_multiply(0.55);
    v.selection.stroke = Stroke::new(1.0, ACCENT);
    v.override_text_color = None;
    v.slider_trailing_fill = true;
    v.handle_shape = egui::style::HandleShape::Circle;
    let w = &mut v.widgets;
    w.noninteractive.bg_stroke = Stroke::new(1.0, LINE);
    w.noninteractive.fg_stroke = Stroke::new(1.0, TEXT_DIM);
    w.noninteractive.corner_radius = CornerRadius::same(7);
    for (s, fill, text) in [
        (&mut w.inactive, KEYCAP, TEXT),
        (&mut w.hovered, Color32::from_rgb(52, 60, 75), Color32::WHITE),
        (&mut w.active, ACCENT.gamma_multiply(0.8), Color32::BLACK),
        (&mut w.open, BG3, TEXT),
    ] {
        s.bg_fill = fill;
        s.weak_bg_fill = fill;
        s.bg_stroke = Stroke::NONE;
        s.fg_stroke = Stroke::new(1.2, text);
        s.corner_radius = CornerRadius::same(7);
        s.expansion = 0.0;
    }
    w.inactive.bg_stroke = Stroke::new(1.0, Color32::from_white_alpha(16));
    w.inactive.bg_fill = Color32::from_rgb(44, 51, 64);
    w.inactive.weak_bg_fill = Color32::from_rgba_unmultiplied(255, 255, 255, 10);
    w.hovered.weak_bg_fill = Color32::from_rgba_unmultiplied(255, 255, 255, 18);
    w.hovered.bg_stroke = Stroke::new(1.0, Color32::from_rgba_unmultiplied(142, 230, 168, 90));
    ctx.set_visuals(v);
    ctx.all_styles_mut(|s| {
        s.text_styles = [
            (TextStyle::Small, FontId::proportional(11.0)),
            (TextStyle::Body, FontId::proportional(13.0)),
            (TextStyle::Button, FontId::proportional(13.0)),
            (TextStyle::Heading, FontId::new(18.0, semibold())),
            (TextStyle::Monospace, FontId::monospace(12.5)),
        ]
        .into();
        s.spacing.item_spacing = Vec2::new(8.0, 6.0);
        s.spacing.button_padding = Vec2::new(10.0, 4.0);
        s.spacing.interact_size = Vec2::new(36.0, 24.0);
        s.spacing.slider_width = 200.0;
        s.spacing.slider_rail_height = 4.0;
        s.spacing.combo_height = 300.0;
        s.spacing.menu_margin = Margin::same(6);
        s.spacing.window_margin = Margin::same(16);
    });
}

// ---------------------------------------------------------------------------------------------
// Background

/// The background picture and its blurred copy.
#[derive(Clone)]
pub struct Backdrop {
    sharp: TextureHandle,
    blurred: TextureHandle,
    aspect: f32,
    /// Where it was drawn this frame, and the part of the picture shown.
    screen: Rect,
    uv: Rect,
}

fn backdrop_id() -> egui::Id {
    egui::Id::unique("openperiph-backdrop")
}

/// Which background to use: faint smoke, none, or a picture.
pub enum BackgroundChoice {
    Smoke,
    Plain,
    Picture(std::path::PathBuf),
}

/// Makes (or removes) the background. Returns an error when a picture couldn't be read; the
/// smoke is used instead then.
pub fn set_background(ctx: &egui::Context, choice: &BackgroundChoice) -> Result<(), String> {
    let (w, h, mut pixels, err) = match choice {
        BackgroundChoice::Plain => {
            ctx.data_mut(|d| d.remove::<Backdrop>(backdrop_id()));
            return Ok(());
        }
        BackgroundChoice::Smoke => {
            let (w, h, p) = smoke(720, 432);
            (w, h, p, None)
        }
        BackgroundChoice::Picture(path) => match load_picture(path, 1280) {
            Ok((w, h, mut p)) => {
                // darken it so text on the glass stays readable
                for c in p.iter_mut() {
                    *c = [c[0] * 0.55, c[1] * 0.55, c[2] * 0.6];
                }
                (w, h, p, None)
            }
            Err(e) => {
                let (w, h, p) = smoke(720, 432);
                (w, h, p, Some(e))
            }
        },
    };
    vignette(w, h, &mut pixels);
    let sharp = ctx.load_texture("backdrop", to_image(w, h, &pixels), TextureOptions::LINEAR);
    let radius = (w as f32 / 70.0).round() as usize;
    let blurred_px = blur(w, h, &pixels, radius.max(2));
    let blurred = ctx.load_texture("backdrop-blurred", to_image(w, h, &blurred_px), TextureOptions::LINEAR);
    let b = Backdrop { sharp, blurred, aspect: w as f32 / h as f32, screen: Rect::NOTHING, uv: Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)) };
    ctx.data_mut(|d| d.insert_temp(backdrop_id(), b));
    match err {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

/// Paints the background over the whole window (call first each frame).
pub fn paint_background(ctx: &egui::Context, painter: &Painter, screen: Rect) {
    painter.rect_filled(screen, CornerRadius::ZERO, BG);
    let Some(mut b) = ctx.data(|d| d.get_temp::<Backdrop>(backdrop_id())) else {
        return;
    };
    // cover the window, keeping the picture's shape
    let sa = screen.width() / screen.height().max(1.0);
    let uv = if sa > b.aspect {
        let hh = b.aspect / sa;
        Rect::from_min_max(Pos2::new(0.0, 0.5 - hh / 2.0), Pos2::new(1.0, 0.5 + hh / 2.0))
    } else {
        let ww = sa / b.aspect;
        Rect::from_min_max(Pos2::new(0.5 - ww / 2.0, 0.0), Pos2::new(0.5 + ww / 2.0, 1.0))
    };
    painter.image(b.sharp.id(), screen, uv, Color32::WHITE);
    b.screen = screen;
    b.uv = uv;
    ctx.data_mut(|d| d.insert_temp(backdrop_id(), b));
}

/// How see-through a glass surface is.
#[derive(Clone, Copy)]
pub enum Glass {
    /// Big panels (top bar, navigation).
    Panel,
    /// Cards on a page.
    Card,
    /// Something inside a card.
    Inset,
}

/// A frosted-glass rounded rectangle.
pub fn glass(ctx: &egui::Context, rect: Rect, radius: f32, kind: Glass) -> Shape {
    let b = ctx.data(|d| d.get_temp::<Backdrop>(backdrop_id()));
    let (top, bottom, alpha) = match kind {
        Glass::Panel => (BG1, BG1, 190u8),
        Glass::Card => (BG2, BG1, 168),
        Glass::Inset => (BG3, BG2, 150),
    };
    let pts = rounded_points(rect, radius);
    let mut shapes = Vec::new();
    match &b {
        Some(b) if b.screen.is_positive() && !matches!(kind, Glass::Inset) => {
            let mut mesh = Mesh::with_texture(b.blurred.id());
            let uv_of = |p: Pos2| {
                let t = (p - b.screen.min) / b.screen.size();
                Pos2::new(b.uv.min.x + t.x * b.uv.width(), b.uv.min.y + t.y * b.uv.height())
            };
            fan(&mut mesh, rect.center(), &pts, |p| (uv_of(p), Color32::WHITE));
            shapes.push(Shape::mesh(mesh));
        }
        _ => {}
    }
    let alpha = if b.is_none() && !matches!(kind, Glass::Inset) { 255 } else { alpha };
    let mut tint = Mesh::default();
    fan(&mut tint, rect.center(), &pts, |p| {
        let t = ((p.y - rect.min.y) / rect.height().max(1.0)).clamp(0.0, 1.0);
        (Pos2::ZERO, with_alpha(lerp_colour(top, bottom, t), alpha))
    });
    shapes.push(Shape::mesh(tint));
    shapes.push(Shape::closed_line(pts, Stroke::new(1.0, Color32::from_rgba_unmultiplied(LINE.r(), LINE.g(), LINE.b(), 170))));
    Shape::Vec(shapes)
}

/// A card: glass behind the contents.
pub fn card(ui: &mut Ui, add: impl FnOnce(&mut Ui)) -> egui::Response {
    card_with(ui, Margin::same(14), add)
}

pub fn card_with(ui: &mut Ui, margin: Margin, add: impl FnOnce(&mut Ui)) -> egui::Response {
    let slot = ui.painter().add(Shape::Noop);
    let r = egui::Frame::new().inner_margin(margin).show(ui, |ui| {
        ui.set_width(ui.available_width());
        add(ui);
    });
    let shape = glass(ui.ctx(), r.response.rect, 12.0, Glass::Card);
    ui.painter().set(slot, shape);
    r.response
}

fn with_alpha(c: Color32, a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a)
}

pub fn lerp_colour(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_unmultiplied(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()), l(a.a(), b.a()))
}

/// The outline of a rounded rectangle, clockwise.
pub fn rounded_points(rect: Rect, radius: f32) -> Vec<Pos2> {
    let r = radius.min(rect.width() / 2.0).min(rect.height() / 2.0).max(0.0);
    let mut pts = Vec::with_capacity(40);
    let corners = [
        (Pos2::new(rect.max.x - r, rect.min.y + r), -90.0f32),
        (Pos2::new(rect.max.x - r, rect.max.y - r), 0.0),
        (Pos2::new(rect.min.x + r, rect.max.y - r), 90.0),
        (Pos2::new(rect.min.x + r, rect.min.y + r), 180.0),
    ];
    for (c, start) in corners {
        for s in 0..=8 {
            let a = (start + s as f32 * 90.0 / 8.0).to_radians();
            pts.push(c + Vec2::new(a.cos(), a.sin()) * r);
        }
    }
    pts
}

/// Fills a convex outline with triangles around `centre`.
fn fan(mesh: &mut Mesh, centre: Pos2, pts: &[Pos2], attr: impl Fn(Pos2) -> (Pos2, Color32)) {
    let base = mesh.vertices.len() as u32;
    let (uv, color) = attr(centre);
    mesh.vertices.push(Vertex { pos: centre, uv, color });
    for &p in pts {
        let (uv, color) = attr(p);
        mesh.vertices.push(Vertex { pos: p, uv, color });
    }
    let n = pts.len() as u32;
    for i in 0..n {
        mesh.add_triangle(base, base + 1 + i, base + 1 + (i + 1) % n);
    }
}

// ----- making the pictures

fn hash(x: i32, y: i32, seed: u32) -> f32 {
    let mut h = (x as u32).wrapping_mul(374_761_393) ^ (y as u32).wrapping_mul(668_265_263) ^ seed.wrapping_mul(2_246_822_519);
    h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    ((h ^ (h >> 16)) & 0xffff) as f32 / 65535.0
}

fn noise(x: f32, y: f32, seed: u32) -> f32 {
    let (xi, yi) = (x.floor() as i32, y.floor() as i32);
    let (xf, yf) = (x - x.floor(), y - y.floor());
    let s = |t: f32| t * t * (3.0 - 2.0 * t);
    let (u, v) = (s(xf), s(yf));
    let a = hash(xi, yi, seed);
    let b = hash(xi + 1, yi, seed);
    let c = hash(xi, yi + 1, seed);
    let d = hash(xi + 1, yi + 1, seed);
    a + (b - a) * u + (c - a) * v + (a - b - c + d) * u * v
}

fn fbm(mut x: f32, mut y: f32, seed: u32) -> f32 {
    let (mut sum, mut amp) = (0.0, 0.5);
    for o in 0..5 {
        sum += amp * noise(x, y, seed + o);
        // rotate a little each octave so the grid doesn't show
        let (nx, ny) = (x * 1.6 - y * 1.2, x * 1.2 + y * 1.6);
        x = nx + 3.1;
        y = ny + 1.7;
        amp *= 0.5;
    }
    sum
}

/// Faint grey smoke on navy-grey, with a few stars. Linear RGB, 0–1.
fn smoke(w: usize, h: usize) -> (usize, usize, Vec<[f32; 3]>) {
    let base = [0.006f32, 0.008, 0.013];
    let teal = [0.075f32, 0.085, 0.11];
    let green = [0.09f32, 0.095, 0.12];
    let blue = [0.004f32, 0.006, 0.014];
    let mut px = vec![[0.0f32; 3]; w * h];
    for y in 0..h {
        for x in 0..w {
            let (u, v) = (x as f32 / h as f32 * 2.2, y as f32 / h as f32 * 2.2);
            let qx = fbm(u, v, 11);
            let qy = fbm(u + 5.2, v + 1.3, 23);
            let rx = fbm(u + 3.6 * qx + 1.7, v + 3.6 * qy + 9.2, 37);
            let ry = fbm(u + 3.6 * qx + 8.3, v + 3.6 * qy + 2.8, 41);
            let d = fbm(u + 3.0 * rx, v + 3.0 * ry, 53);
            let smoke = ((d - 0.40) / 0.45).clamp(0.0, 1.0).powf(2.2);
            let mix = fbm(u * 0.6 + 7.0, v * 0.6, 67).clamp(0.0, 1.0);
            let deep = (qx * 0.9).clamp(0.0, 1.0);
            let mut c = [0.0; 3];
            for k in 0..3 {
                let tint = teal[k] + (green[k] - teal[k]) * ((mix - 0.3) * 2.2).clamp(0.0, 1.0);
                c[k] = base[k] + blue[k] * deep + tint * smoke * 0.6;
            }
            // stars
            let s = hash(x as i32, y as i32, 99);
            if s > 0.9994 {
                let b = (s - 0.9994) / 0.0006 * 0.5 + 0.12;
                for k in c.iter_mut() {
                    *k += b;
                }
            }
            px[y * w + x] = c;
        }
    }
    (w, h, px)
}

fn vignette(w: usize, h: usize, px: &mut [[f32; 3]]) {
    for y in 0..h {
        for x in 0..w {
            let dx = x as f32 / w as f32 - 0.5;
            let dy = y as f32 / h as f32 - 0.5;
            let f = 1.0 - (dx * dx * 1.1 + dy * dy * 1.6).min(1.0) * 0.75;
            for k in px[y * w + x].iter_mut() {
                *k *= f;
            }
        }
    }
}

/// Three box blurs (close to a Gaussian).
fn blur(w: usize, h: usize, px: &[[f32; 3]], r: usize) -> Vec<[f32; 3]> {
    let mut a = px.to_vec();
    let mut b = vec![[0.0f32; 3]; w * h];
    for _ in 0..3 {
        box_pass(&a, &mut b, w, h, r, true);
        box_pass(&b, &mut a, w, h, r, false);
    }
    a
}

fn box_pass(src: &[[f32; 3]], dst: &mut [[f32; 3]], w: usize, h: usize, r: usize, horizontal: bool) {
    let (n, lines) = if horizontal { (w, h) } else { (h, w) };
    let at = |line: usize, i: usize| {
        if horizontal {
            line * w + i
        } else {
            i * w + line
        }
    };
    let norm = 1.0 / (2 * r + 1) as f32;
    for line in 0..lines {
        // the sum over [-r, r] around index 0, edges repeated
        let mut acc = [0.0f32; 3];
        for i in 0..=(2 * r) {
            let j = (i as isize - r as isize).clamp(0, n as isize - 1) as usize;
            for k in 0..3 {
                acc[k] += src[at(line, j)][k];
            }
        }
        for i in 0..n {
            for k in 0..3 {
                dst[at(line, i)][k] = acc[k] * norm;
            }
            let out = (i as isize - r as isize).clamp(0, n as isize - 1) as usize;
            let inn = (i as isize + r as isize + 1).clamp(0, n as isize - 1) as usize;
            for k in 0..3 {
                acc[k] += src[at(line, inn)][k] - src[at(line, out)][k];
            }
        }
    }
}

fn to_image(w: usize, h: usize, px: &[[f32; 3]]) -> egui::ColorImage {
    let mut bytes = Vec::with_capacity(w * h * 3);
    for c in px {
        for k in c {
            // linear → sRGB
            let v = k.clamp(0.0, 1.0);
            let s = if v <= 0.003_130_8 { v * 12.92 } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 };
            bytes.push((s * 255.0).round() as u8);
        }
    }
    egui::ColorImage::from_rgb([w, h], &bytes)
}

/// Reads a picture (PNG, JPEG or WebP), at most `max` pixels wide. Linear RGB.
fn load_picture(path: &std::path::Path, max: u32) -> Result<(usize, usize, Vec<[f32; 3]>), String> {
    let img = image::open(path).map_err(|e| format!("Couldn't read {}: {e}", path.display()))?;
    let img = if img.width() > max { img.resize(max, max * img.height() / img.width().max(1), image::imageops::FilterType::Triangle) } else { img };
    let rgb = img.to_rgb8();
    let (w, h) = (rgb.width() as usize, rgb.height() as usize);
    let lin = |b: u8| {
        let s = b as f32 / 255.0;
        if s <= 0.04045 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    };
    Ok((w, h, rgb.pixels().map(|p| [lin(p[0]), lin(p[1]), lin(p[2])]).collect()))
}

/// A picture to show as a texture (for photos of devices).
pub fn load_texture(ctx: &egui::Context, path: &std::path::Path) -> Result<TextureHandle, String> {
    let img = image::open(path).map_err(|e| format!("Couldn't read {}: {e}", path.display()))?;
    let img = if img.width() > 1200 { img.resize(1200, 1200 * img.height() / img.width().max(1), image::imageops::FilterType::Triangle) } else { img };
    let rgba = img.to_rgba8();
    let image = egui::ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], rgba.as_raw());
    Ok(ctx.load_texture(path.display().to_string(), image, TextureOptions::LINEAR))
}

// ---------------------------------------------------------------------------------------------
// Icons (drawn with lines, so they're sharp at any size)

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Icon {
    Home,
    Keyboard,
    Mouse,
    Keymap,
    Lighting,
    Macros,
    Device,
    Gauge,
    Info,
}

pub fn icon(p: &Painter, rect: Rect, which: Icon, colour: Color32) {
    let s = Stroke::new(1.5, colour);
    let c = rect.center();
    let u = rect.width().min(rect.height()) / 16.0;
    let at = |x: f32, y: f32| c + Vec2::new(x, y) * u;
    match which {
        Icon::Home => {
            p.add(Shape::line(vec![at(-6.5, -0.5), at(0.0, -6.5), at(6.5, -0.5)], s));
            p.add(Shape::line(vec![at(-4.5, -2.0), at(-4.5, 6.0), at(4.5, 6.0), at(4.5, -2.0)], s));
            p.add(Shape::line(vec![at(-1.5, 6.0), at(-1.5, 2.0), at(1.5, 2.0), at(1.5, 6.0)], s));
        }
        Icon::Keyboard => {
            p.rect_stroke(Rect::from_min_max(at(-7.5, -4.5), at(7.5, 4.5)), CornerRadius::same(2), s, egui::StrokeKind::Middle);
            for (row, y) in [-1.8f32, 0.6].iter().enumerate() {
                for i in 0..5 {
                    let x = -5.0 + i as f32 * 2.5 + if row == 1 { 0.6 } else { 0.0 };
                    p.circle_filled(at(x, *y), 0.75 * u, colour);
                }
            }
            p.line_segment([at(-3.0, 2.9), at(3.0, 2.9)], s);
        }
        Icon::Mouse => {
            p.rect_stroke(Rect::from_min_max(at(-4.5, -7.0), at(4.5, 7.0)), CornerRadius::same((4.5 * u) as u8), s, egui::StrokeKind::Middle);
            p.line_segment([at(0.0, -7.0), at(0.0, -2.0)], s);
            p.line_segment([at(-4.5, -2.0), at(4.5, -2.0)], Stroke::new(1.0, colour.gamma_multiply(0.6)));
        }
        Icon::Keymap => {
            p.rect_stroke(Rect::from_min_max(at(-6.5, -6.5), at(6.5, 6.5)), CornerRadius::same(3), s, egui::StrokeKind::Middle);
            p.rect_stroke(
                Rect::from_min_max(at(-3.5, -4.0), at(3.5, 2.5)),
                CornerRadius::same(2),
                Stroke::new(1.2, colour.gamma_multiply(0.75)),
                egui::StrokeKind::Middle,
            );
            p.line_segment([at(-2.0, -1.0), at(2.0, -1.0)], Stroke::new(1.2, colour));
        }
        Icon::Lighting => {
            p.circle_stroke(at(0.0, -2.0), 4.8 * u, s);
            p.line_segment([at(-2.2, 4.0), at(2.2, 4.0)], s);
            p.line_segment([at(-1.6, 6.5), at(1.6, 6.5)], s);
            p.line_segment([at(-1.2, 2.5), at(-1.2, 4.0)], s);
            p.line_segment([at(1.2, 2.5), at(1.2, 4.0)], s);
        }
        Icon::Macros => {
            for (i, y) in [-4.5f32, 0.0, 4.5].iter().enumerate() {
                p.circle_filled(at(-5.5, *y), 1.1 * u, colour);
                p.line_segment([at(-2.5, *y), at(if i == 2 { 3.5 } else { 6.5 }, *y)], s);
            }
        }
        Icon::Device => {
            p.circle_stroke(c, 2.6 * u, s);
            for k in 0..8 {
                let a = (k as f32 * 45.0).to_radians();
                let d = Vec2::new(a.cos(), a.sin());
                p.line_segment([c + d * 4.6 * u, c + d * 6.8 * u], Stroke::new(2.2, colour));
            }
            p.circle_stroke(c, 5.0 * u, Stroke::new(1.3, colour));
        }
        Icon::Gauge => {
            let pts: Vec<Pos2> = (0..=16)
                .map(|k| {
                    let a = (180.0 + k as f32 * 180.0 / 16.0).to_radians();
                    at(0.0, 3.0) + Vec2::new(a.cos(), a.sin()) * 7.0 * u
                })
                .collect();
            p.add(Shape::line(pts, s));
            p.line_segment([at(0.0, 3.0), at(3.5, -2.0)], Stroke::new(1.8, colour));
            p.circle_filled(at(0.0, 3.0), 1.4 * u, colour);
        }
        Icon::Info => {
            p.circle_stroke(c, 7.0 * u, s);
            p.line_segment([at(0.0, -0.5), at(0.0, 4.0)], Stroke::new(1.7, colour));
            p.circle_filled(at(0.0, -3.3), 1.0 * u, colour);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Drawings of the hardware

/// One key of a keyboard drawing, in key units (1 = one key wide).
#[derive(Clone, Debug)]
pub struct ArtKey {
    pub rect: Rect,
    /// A second rectangle (ISO Enter's lower part).
    pub second: Option<Rect>,
    pub label: String,
}

/// A keyboard seen from above: case, plate, keycaps and (optionally) the backlight colour.
pub fn keyboard_art(p: &Painter, area: Rect, keys: &[ArtKey], glow: Option<Color32>, legends: bool) {
    if keys.is_empty() {
        return;
    }
    let mut bounds = keys[0].rect;
    for k in keys {
        bounds = bounds.union(k.rect);
        if let Some(s) = k.second {
            bounds = bounds.union(s);
        }
    }
    let pad = 0.45;
    let w = bounds.width() + pad * 2.0;
    let h = bounds.height() + pad * 2.0;
    let scale = (area.width() / w).min(area.height() / h);
    let size = Vec2::new(w, h) * scale;
    let case = Rect::from_center_size(area.center(), size);
    let to = |r: Rect| Rect::from_min_max(case.min + ((r.min - bounds.min) + Vec2::splat(pad)) * scale, case.min + ((r.max - bounds.min) + Vec2::splat(pad)) * scale);
    let radius = (scale * 0.35).min(22.0);
    // shadow under the board
    p.add(
        egui::epaint::Shadow { offset: [0, (scale * 0.4) as i8], blur: (scale * 0.9).min(60.0) as u8, spread: 0, color: Color32::from_black_alpha(150) }
            .as_shape(case, CornerRadius::same(radius as u8)),
    );
    // case: a little lighter at the top
    let mut m = Mesh::default();
    let pts = rounded_points(case, radius);
    fan(&mut m, case.center(), &pts, |q| {
        let t = ((q.y - case.min.y) / case.height()).clamp(0.0, 1.0);
        (Pos2::ZERO, lerp_colour(Color32::from_rgb(58, 62, 70), Color32::from_rgb(30, 32, 37), t))
    });
    p.add(Shape::mesh(m));
    p.add(Shape::closed_line(pts, Stroke::new(1.0, Color32::from_rgb(78, 83, 92))));
    // plate
    let plate = case.shrink(pad * scale * 0.55);
    p.rect_filled(plate, CornerRadius::same((radius * 0.5) as u8), Color32::from_rgb(20, 21, 25));
    let gap = (scale * 0.05).max(1.0);
    if let Some(g) = glow {
        for k in keys {
            for r in std::iter::once(k.rect).chain(k.second) {
                let r = to(r).shrink(gap * 0.5);
                p.rect_filled(r.expand(scale * 0.04), CornerRadius::same((scale * 0.14) as u8), g.gamma_multiply(0.3));
            }
        }
    }
    let cr = CornerRadius::same((scale * 0.11).clamp(1.0, 10.0) as u8);
    for k in keys {
        let rects: Vec<Rect> = std::iter::once(k.rect).chain(k.second).map(|r| to(r).shrink(gap)).collect();
        // skirt
        for r in &rects {
            p.rect_filled(*r, cr, Color32::from_rgb(26, 28, 33));
        }
        // top face: inset more at the bottom, so the key looks tilted towards you
        for r in &rects {
            let top = Rect::from_min_max(r.min + Vec2::new(scale * 0.1, scale * 0.05), r.max - Vec2::new(scale * 0.1, scale * 0.16));
            let mut m = Mesh::default();
            let pts = rounded_points(top, cr.nw as f32 * 0.8);
            fan(&mut m, top.center(), &pts, |q| {
                let t = ((q.y - top.min.y) / top.height().max(1.0)).clamp(0.0, 1.0);
                (Pos2::ZERO, lerp_colour(Color32::from_rgb(64, 68, 77), Color32::from_rgb(46, 49, 56), t))
            });
            p.add(Shape::mesh(m));
        }
        if legends && scale >= 26.0 && !k.label.is_empty() {
            let r = rects[0];
            let fs = (scale * 0.2).clamp(7.0, 12.0);
            let colour = glow.map_or(Color32::from_rgb(200, 204, 212), |g| lerp_colour(Color32::WHITE, g, 0.6));
            p.text(r.left_top() + Vec2::new(scale * 0.2, scale * 0.13), Align2::LEFT_TOP, short_legend(&k.label), FontId::proportional(fs), colour);
        }
    }
}

fn short_legend(s: &str) -> String {
    let s = s.trim();
    if s.chars().count() > 6 {
        s.chars().take(5).collect()
    } else {
        s.to_string()
    }
}

/// A symmetrical mouse seen from above, front at the top.
pub fn mouse_art(p: &Painter, area: Rect, body: Color32) {
    let hgt = area.height().min(area.width() * 1.75);
    let wid = hgt * 0.56;
    let c = area.center();
    let (a, b) = (wid / 2.0, hgt / 2.0);
    // outline: a squarish ellipse, narrower at the front
    let n = 72;
    let mut pts = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f32 / n as f32 * std::f32::consts::TAU;
        let (ct, st) = (t.cos(), t.sin());
        let e = 2.0 / 2.6;
        let x = ct.signum() * ct.abs().powf(e) * a;
        let y = st.signum() * st.abs().powf(e) * b;
        let taper = 0.82 + 0.18 * ((y + b) / (2.0 * b));
        pts.push(c + Vec2::new(x * taper, y));
    }
    p.add(
        egui::epaint::Shadow { offset: [0, (hgt * 0.04) as i8], blur: (hgt * 0.12).min(90.0) as u8, spread: 0, color: Color32::from_black_alpha(170) }
            .as_shape(Rect::from_center_size(c + Vec2::new(0.0, hgt * 0.03), Vec2::new(wid * 0.85, hgt * 0.92)), CornerRadius::same((wid * 0.4).min(120.0) as u8)),
    );
    // body: lit from the top left
    let light = c + Vec2::new(-wid * 0.18, -hgt * 0.22);
    let lit = lerp_colour(body, Color32::WHITE, 0.16);
    let dark = lerp_colour(body, Color32::BLACK, 0.55);
    let mut m = Mesh::default();
    fan(&mut m, light, &pts, |q| {
        let d = ((q - light).length() / (hgt * 0.7)).clamp(0.0, 1.0);
        (Pos2::ZERO, if q == light { lit } else { lerp_colour(body, dark, d) })
    });
    p.add(Shape::mesh(m));
    p.add(Shape::closed_line(pts.clone(), Stroke::new(1.2, lerp_colour(body, Color32::WHITE, 0.08))));
    // buttons and the line between them and the palm rest
    let split_y = c.y - hgt * 0.08;
    let groove = Stroke::new((hgt * 0.006).max(1.0), Color32::from_black_alpha(200));
    p.line_segment([Pos2::new(c.x, c.y - b + hgt * 0.015), Pos2::new(c.x, split_y)], groove);
    let arc: Vec<Pos2> = (0..=20)
        .map(|k| {
            let f = k as f32 / 20.0 * 2.0 - 1.0;
            Pos2::new(c.x + f * a * 0.9, split_y + (1.0 - f * f) * hgt * 0.035)
        })
        .collect();
    p.add(Shape::line(arc, groove));
    // scroll wheel
    let wheel = Rect::from_center_size(Pos2::new(c.x, c.y - hgt * 0.29), Vec2::new(wid * 0.1, hgt * 0.13));
    p.rect_filled(wheel.expand(hgt * 0.008), CornerRadius::same((wid * 0.06) as u8), Color32::from_black_alpha(220));
    p.rect_filled(wheel, CornerRadius::same((wid * 0.05) as u8), Color32::from_rgb(52, 55, 62));
    for k in 1..8 {
        let y = wheel.min.y + wheel.height() * k as f32 / 8.0;
        p.line_segment([Pos2::new(wheel.min.x + 2.0, y), Pos2::new(wheel.max.x - 2.0, y)], Stroke::new(1.0, Color32::from_rgb(34, 36, 41)));
    }
    // DPI button behind the wheel
    let dpi = Rect::from_center_size(Pos2::new(c.x, c.y - hgt * 0.17), Vec2::new(wid * 0.07, hgt * 0.04));
    p.rect_filled(dpi, CornerRadius::same(3), Color32::from_black_alpha(160));
    // side buttons on the left
    for (k, off) in [0.0f32, 0.12].iter().enumerate() {
        let y = c.y - hgt * 0.02 + off * hgt;
        let x = c.x - a * (0.82 + 0.18 * ((y - c.y + b) / (2.0 * b))) * 0.985;
        let r = Rect::from_min_size(Pos2::new(x - wid * 0.025, y), Vec2::new(wid * 0.05, hgt * (0.1 - k as f32 * 0.01)));
        p.rect_filled(r, CornerRadius::same(4), lerp_colour(body, Color32::WHITE, 0.1));
    }
    // a small logo
    p.circle_filled(Pos2::new(c.x, c.y + hgt * 0.22), hgt * 0.018, Color32::from_white_alpha(40));
}

/// A photo fitted inside `area`, keeping its shape.
pub fn photo(p: &Painter, area: Rect, tex: &TextureHandle) {
    let [w, h] = tex.size();
    let s = (area.width() / w as f32).min(area.height() / h as f32);
    let r = Rect::from_center_size(area.center(), Vec2::new(w as f32, h as f32) * s);
    p.image(tex.id(), r, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blur_keeps_flat_colour() {
        let px = vec![[0.5f32, 0.25, 1.0]; 20 * 10];
        let out = blur(20, 10, &px, 3);
        for c in out {
            assert!((c[0] - 0.5).abs() < 1e-4 && (c[1] - 0.25).abs() < 1e-4 && (c[2] - 1.0).abs() < 1e-4);
        }
    }

    #[test]
    #[ignore]
    fn smoke_picture() {
        let (w, h, mut px) = smoke(720, 432);
        vignette(w, h, &mut px);
        let img = to_image(w, h, &px);
        let bytes: Vec<u8> = img.pixels.iter().flat_map(|c| [c.r(), c.g(), c.b()]).collect();
        image::save_buffer("/tmp/smoke.png", &bytes, w as u32, h as u32, image::ColorType::Rgb8).unwrap();
    }

    #[test]
    fn smoke_is_dark_enough_for_text() {
        let (w, h, px) = smoke(160, 96);
        let mean: f32 = px.iter().map(|c| (c[0] + c[1] + c[2]) / 3.0).sum::<f32>() / (w * h) as f32;
        assert!(mean < 0.12, "mean {mean}");
    }
}
