use crate::state::State;

use glium;
use glium::glutin::{ContextBuilder, ContextTrait, EventsLoop, WindowBuilder};
use glium::{implement_vertex, program, uniform, Surface};

use harfbuzz_rs::Owned;
use harfbuzz_rs::{shape, Font as HBFont, UnicodeBuffer};
use rusttype::gpu_cache::Cache;
use rusttype::{point, vector, Font, GlyphId, PositionedGlyph, Rect, Scale};

use std::borrow::Cow;
use std::error::Error;
use std::sync::mpsc::channel;
use std::thread;

pub use glium::glutin::{Event, WindowEvent};

pub struct Display<'a> {
    display: glium::Display,
    program: glium::Program,
    font: Font<'a>,
    hb_font: Owned<HBFont<'a>>,
    cache: Cache<'a>,
    cache_tex: glium::texture::Texture2d,
}

pub trait Handler {
    fn on_window_event(&mut self, event: &WindowEvent);
}

impl<'a> Display<'a> {
    pub fn open<H: Handler + Send + 'static>(mut handler: H) -> Result<Self, Box<Error>> {
        let (tx, rx) = channel();
        thread::spawn(move || {
            let window = WindowBuilder::new()
                .with_dimensions((1024, 512).into())
                .with_title("GreenTTY");
            let context = ContextBuilder::new().with_vsync(true);
            let mut events_loop = EventsLoop::new();
            let context = context.build_windowed(window, &events_loop).unwrap();
            tx.send(context).unwrap();
            events_loop.run_forever(|e| {
                if let Event::WindowEvent { event, .. } = e {
                    handler.on_window_event(&event);
                }
                glium::glutin::ControlFlow::Continue
            });
        });

        let context = rx.recv().unwrap();
        unsafe { context.context().make_current().unwrap() }

        // let display = unsafe { glium::Display::unchecked(context).unwrap() };
        let display = glium::Display::from_gl_window(context).unwrap();

        use std::fs;
        use std::sync::Arc;
        let font_data = fs::read("fonts/RictyDiminishedDiscord-with-FiraCode-Regular.ttf")?;
        let bytes: Arc<[u8]> = font_data.clone().into();
        let hb_font = harfbuzz_rs::rusttype::create_harfbuzz_rusttype_font(bytes, 0)?;
        let font = Font::from_bytes(font_data)?;

        let dpi_factor = display.gl_window().get_hidpi_factor();
        let (cache_width, cache_height) =
            ((1024.0 * dpi_factor) as u32, (512.0 * dpi_factor) as u32);
        let cache = Cache::builder()
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

        Ok(Display {
            display,
            program,
            font,
            hb_font,
            cache,
            cache_tex,
        })
    }

    pub fn render(&mut self, state: &State) -> Result<(), Box<Error>> {
        let dpi_factor = self.display.gl_window().get_hidpi_factor();
        let (width, _): (u32, _) = self
            .display
            .gl_window()
            .get_inner_size()
            .ok_or("get_inner_size")?
            .to_physical(dpi_factor)
            .into();
        let dpi_factor = dpi_factor as f32;

        let lines = state
            .lines
            .iter()
            .map(|line| line.iter().collect::<String>())
            .collect::<Vec<String>>();

        let glyphs = layout_paragraph(
            &self.font,
            &self.hb_font,
            Scale::uniform(24.0 * dpi_factor),
            width,
            &lines,
        );
        for glyph in &glyphs {
            self.cache.queue_glyph(0, glyph.clone());
        }
        let cache_tex = &self.cache_tex;
        self.cache.cache_queued(|rect, data| {
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
            tex: self.cache_tex.sampled().magnify_filter(glium::uniforms::MagnifySamplerFilter::Nearest)
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
                let (w, h) = self.display.get_framebuffer_dimensions();
                (w as f32, h as f32)
            };
            let origin = point(0.0, 0.0);
            let vertices: Vec<Vertex> = glyphs
                .iter()
                .flat_map(|g| {
                    if let Ok(Some((uv_rect, screen_rect))) = self.cache.rect_for(0, g) {
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

            glium::VertexBuffer::new(&self.display, &vertices)?
        };

        let mut target = self.display.draw();
        target.clear_color(0.0, 0.0, 0.0, 0.0);
        target.draw(
            &vertex_buffer,
            glium::index::NoIndices(glium::index::PrimitiveType::TrianglesList),
            &self.program,
            &uniforms,
            &glium::DrawParameters {
                blend: glium::Blend::alpha_blending(),
                ..Default::default()
            },
        )?;
        target.finish()?;

        Ok(())
    }
}

fn layout_paragraph<'a, 'b>(
    font: &'b Font<'a>,
    hb_font: &'b HBFont,
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
