mod pty;

use pty::{Pty, PtyConfig};

use glium::glutin::{ContextBuilder, EventsLoop, WindowBuilder};
use glium::*;
use vte::{Parser, Perform};

use std::borrow::Cow;
use std::error::Error;
use std::thread;
use std::time::{Duration, Instant};

use harfbuzz_rs::{shape, Font as FBFont, UnicodeBuffer};
use rusttype::gpu_cache::Cache;
use rusttype::{point, vector, Font, GlyphId, PositionedGlyph, Rect, Scale};

fn layout_paragraph<'a, 'b>(
    font: &'a Font,
    hb_font: &'a FBFont,
    scale: Scale,
    width: u32,
    lines: &'b Vec<String>,
) -> Vec<PositionedGlyph<'a>> {
    // use unicode_normalization::UnicodeNormalization;
    let mut result = Vec::new();
    let v_metrics = font.v_metrics(scale);
    let advance_height = v_metrics.ascent - v_metrics.descent + v_metrics.line_gap;
    let mut caret = point(0.0, v_metrics.ascent);
    let mut last_glyph_id = None;

    for line in &lines[..] {
        let buffer = UnicodeBuffer::new().add_str(line);
        let output = shape(&hb_font, buffer, &[]);

        let positions = output.get_glyph_positions();
        let infos = output.get_glyph_infos();

        for (_position, info) in positions.iter().zip(infos) {
            let base_glyph = font.glyph(GlyphId(info.codepoint));

            if let Some(id) = last_glyph_id.take() {
                caret.x += font.pair_kerning(scale, id, base_glyph.id());
            }
            last_glyph_id = Some(base_glyph.id());
            let mut glyph = base_glyph.scaled(scale).positioned(caret);
            if let Some(bb) = glyph.pixel_bounding_box() {
                if bb.max.x > width as i32 {
                    caret = point(0.0, caret.y + advance_height);
                    glyph.set_position(caret);
                    last_glyph_id = None;
                }
            }
            caret.x += glyph.unpositioned().h_metrics().advance_width;
            result.push(glyph);
        }

        caret = point(0.0, caret.y + advance_height);
    }

    result
}

struct Position {
    x: usize,
    y: usize,
}

struct Grid {
    cursor: Position,
    lines: Vec<Vec<char>>,
}

impl Grid {
    fn new() -> Self {
        Grid {
            cursor: Position { x: 0, y: 0 },
            lines: vec![],
        }
    }

    fn lines(&self) -> Vec<String> {
        self.lines
            .iter()
            .map(|line| line.iter().collect::<String>())
            .collect::<Vec<String>>()
    }
}

impl Perform for Grid {
    fn print(&mut self, c: char) {
        while self.cursor.y >= self.lines.len() {
            self.lines.push(vec![]);
        }
        while self.cursor.x >= self.lines[self.cursor.y].len() {
            self.lines[self.cursor.y].push(' ');
        }
        self.lines[self.cursor.y][self.cursor.x] = c;
        self.cursor.x += 1;
    }

    fn csi_dispatch(&mut self, params: &[i64], _intermediates: &[u8], _ignore: bool, c: char) {
        match c {
            'H' => {
                let y = if params.len() >= 1 { params[0] - 1 } else { 0 } as usize;
                let x = if params.len() >= 2 { params[1] - 1 } else { 0 } as usize;
                self.cursor.y = y;
                self.cursor.x = x;
            }
            'X' => {
                // FIXME
                // let s = if params.len() >= 1 { params[0] } else { 1 } as usize;
                while self.cursor.y >= self.lines.len() {
                    self.lines.push(vec![]);
                }
                self.lines[self.cursor.y].resize(self.cursor.x, ' ');
            }
            'C' => {
                let s = if params.len() >= 1 { params[0] } else { 1 } as usize;
                self.cursor.x += s;
            }
            _ => {}
        }
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            8 /* BS */ => {
                self.cursor.x -= 1;
            }
            10 /* LF */ => {
                self.cursor.y += 1;
            }
            13 /* CR */ => {
                self.cursor.x = 0;
            }
            _ => {}
        }
    }

    fn hook(&mut self, _params: &[i64], _intermediates: &[u8], _ignore: bool) {}
    fn put(&mut self, _byte: u8) {}
    fn unhook(&mut self) {}
    fn osc_dispatch(&mut self, _params: &[&[u8]]) {}
    fn esc_dispatch(&mut self, _params: &[i64], _intermediates: &[u8], _ignore: bool, _byte: u8) {}
}

fn main() -> Result<(), Box<Error>> {
    let config = PtyConfig {
        shell: "powershell",
        ..Default::default()
    };
    let pty = Pty::spawn(&config)?;
    let mut grid = Grid::new();
    let mut parser = Parser::new();

    use std::fs;
    use std::sync::Arc;
    let font_data = fs::read("fonts/RictyDiminishedDiscord-with-FiraCode-Regular.ttf")?;
    let bytes: Arc<[u8]> = font_data.clone().into();
    let hb_font = harfbuzz_rs::rusttype::create_harfbuzz_rusttype_font(bytes, 0)?;
    let font = Font::from_bytes(&font_data)?;

    let window = WindowBuilder::new()
        .with_dimensions((1024, 512).into())
        .with_title("GreenTTY");
    let context = ContextBuilder::new().with_vsync(true);
    let mut events_loop = EventsLoop::new();
    let display = Display::new(window, context, &events_loop)?;

    let dpi_factor = display.gl_window().get_hidpi_factor();

    let (cache_width, cache_height) = ((1024.0 * dpi_factor) as u32, (512.0 * dpi_factor) as u32);
    let mut cache = Cache::builder()
        .dimensions(cache_width, cache_height)
        .build();

    let program = program!(
    &display,
    140 => {
            vertex: "
                #version 140
                in vec2 position;
                in vec2 tex_coords;
                in vec4 colour;
                out vec2 v_tex_coords;
                out vec4 v_colour;
                void main() {
                    gl_Position = vec4(position, 0.0, 1.0);
                    v_tex_coords = tex_coords;
                    v_colour = colour;
                }
            ",

            fragment: "
                #version 140
                uniform sampler2D tex;
                in vec2 v_tex_coords;
                in vec4 v_colour;
                out vec4 f_colour;
                void main() {
                    f_colour = v_colour * vec4(1.0, 1.0, 1.0, texture(tex, v_tex_coords).r);
                }
            "
    })?;
    let cache_tex = glium::texture::Texture2d::with_format(
        &display,
        glium::texture::RawImage2d {
            data: Cow::Owned(vec![128u8; cache_width as usize * cache_height as usize]),
            width: cache_width,
            height: cache_height,
            format: glium::texture::ClientFormat::U8,
        },
        glium::texture::UncompressedFloatFormat::U8,
        glium::texture::MipmapsOption::NoMipmap,
    )?;

    let mut accumulator = Duration::new(0, 0);
    let mut previous_clock = Instant::now();
    loop {
        loop {
            if let Ok(b) = pty.try_receive() {
                parser.advance(&mut grid, b);
            } else {
                break;
            }
        }

        let dpi_factor = display.gl_window().get_hidpi_factor();
        let (width, _): (u32, _) = display
            .gl_window()
            .get_inner_size()
            .ok_or("get_inner_size")?
            .to_physical(dpi_factor)
            .into();
        let dpi_factor = dpi_factor as f32;

        let mut finished = false;
        events_loop.poll_events(|event| {
            use glium::glutin::*;

            if let Event::WindowEvent { event, .. } = event {
                match event {
                    WindowEvent::CloseRequested => finished = true,
                    WindowEvent::KeyboardInput {
                        input:
                            KeyboardInput {
                                state: ElementState::Pressed,
                                virtual_keycode: Some(keypress),
                                ..
                            },
                        ..
                    } => match keypress {
                        VirtualKeyCode::Escape => finished = true,
                        VirtualKeyCode::Back => {}
                        _ => (),
                    },
                    WindowEvent::ReceivedCharacter(c) => {
                        let mut bytes = [0; 4];
                        let s = c.encode_utf8(&mut bytes);
                        for b in s.bytes() {
                            pty.send(b).unwrap();
                        }
                    }
                    _ => {}
                }
            }
        });
        if finished {
            break;
        }

        let lines = grid.lines();
        let glyphs = layout_paragraph(
            &font,
            &hb_font,
            Scale::uniform(24.0 * dpi_factor),
            width,
            &lines,
        );
        for glyph in &glyphs {
            cache.queue_glyph(0, glyph.clone());
        }
        cache.cache_queued(|rect, data| {
            cache_tex.main_level().write(
                glium::Rect {
                    left: rect.min.x,
                    bottom: rect.min.y,
                    width: rect.width(),
                    height: rect.height(),
                },
                glium::texture::RawImage2d {
                    data: Cow::Borrowed(data),
                    width: rect.width(),
                    height: rect.height(),
                    format: glium::texture::ClientFormat::U8,
                },
            );
        })?;

        let uniforms = uniform! {
            tex: cache_tex.sampled().magnify_filter(glium::uniforms::MagnifySamplerFilter::Nearest)
        };

        let vertex_buffer = {
            #[derive(Copy, Clone)]
            struct Vertex {
                position: [f32; 2],
                tex_coords: [f32; 2],
                colour: [f32; 4],
            }

            implement_vertex!(Vertex, position, tex_coords, colour);
            let colour = [1.0, 1.0, 1.0, 1.0];
            let (screen_width, screen_height) = {
                let (w, h) = display.get_framebuffer_dimensions();
                (w as f32, h as f32)
            };
            let origin = point(0.0, 0.0);
            let vertices: Vec<Vertex> = glyphs
                .iter()
                .flat_map(|g| {
                    if let Ok(Some((uv_rect, screen_rect))) = cache.rect_for(0, g) {
                        let gl_rect = Rect {
                            min: origin
                                + (vector(
                                    screen_rect.min.x as f32 / screen_width - 0.5,
                                    1.0 - screen_rect.min.y as f32 / screen_height - 0.5,
                                )) * 2.0,
                            max: origin
                                + (vector(
                                    screen_rect.max.x as f32 / screen_width - 0.5,
                                    1.0 - screen_rect.max.y as f32 / screen_height - 0.5,
                                )) * 2.0,
                        };
                        arrayvec::ArrayVec::<[Vertex; 6]>::from([
                            Vertex {
                                position: [gl_rect.min.x, gl_rect.max.y],
                                tex_coords: [uv_rect.min.x, uv_rect.max.y],
                                colour,
                            },
                            Vertex {
                                position: [gl_rect.min.x, gl_rect.min.y],
                                tex_coords: [uv_rect.min.x, uv_rect.min.y],
                                colour,
                            },
                            Vertex {
                                position: [gl_rect.max.x, gl_rect.min.y],
                                tex_coords: [uv_rect.max.x, uv_rect.min.y],
                                colour,
                            },
                            Vertex {
                                position: [gl_rect.max.x, gl_rect.min.y],
                                tex_coords: [uv_rect.max.x, uv_rect.min.y],
                                colour,
                            },
                            Vertex {
                                position: [gl_rect.max.x, gl_rect.max.y],
                                tex_coords: [uv_rect.max.x, uv_rect.max.y],
                                colour,
                            },
                            Vertex {
                                position: [gl_rect.min.x, gl_rect.max.y],
                                tex_coords: [uv_rect.min.x, uv_rect.max.y],
                                colour,
                            },
                        ])
                    } else {
                        arrayvec::ArrayVec::new()
                    }
                })
                .collect();

            glium::VertexBuffer::new(&display, &vertices)?
        };

        let mut target = display.draw();
        target.clear_color(0.0, 0.0, 0.0, 0.0);
        target.draw(
            &vertex_buffer,
            glium::index::NoIndices(glium::index::PrimitiveType::TrianglesList),
            &program,
            &uniforms,
            &glium::DrawParameters {
                blend: glium::Blend::alpha_blending(),
                ..Default::default()
            },
        )?;

        target.finish()?;

        let now = Instant::now();
        accumulator += now - previous_clock;
        previous_clock = now;
        let fixed_time_stamp = Duration::new(0, 16666667);
        while accumulator >= fixed_time_stamp {
            accumulator -= fixed_time_stamp;
        }
        thread::sleep(fixed_time_stamp - accumulator);
    }

    Ok(())
}
