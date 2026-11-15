// SPDX-License-Identifier: CPAL-1.0
// Copyright (c) 2026 Dôm'Booth and the BoothSoft contributors. See LICENSE (Exhibits A and B) and NOTICE.
//! GPU live view: the camera frame goes straight from the decoder's RGB buffer
//! into an OpenGL texture, and everything else happens in a fragment shader.
//!
//! Compared with drawing the preview through an egui texture, per frame this
//! removes, on the CPU:
//! * the downscale to preview size (the GPU samples the full frame),
//! * the colour-filter loop over every pixel (a 3x3 matrix in the shader),
//! * the RGB -> RGBA `ColorImage` conversion and its allocation,
//! * the mirror and the "cover" crop (texture coordinates).
//!
//! The uploaded buffer is handed back to the camera thread, so the steady
//! state decodes into the same few buffers forever, with no allocation.
use boothsoft::effects::PhotoFilter;
use eframe::{
    egui::{self, Rect},
    egui_glow,
    glow::{self, HasContext},
};
use image::RgbaImage;
use std::sync::{mpsc::SyncSender, Arc, Mutex};

struct GlObjects {
    program: glow::Program,
    vertex_array: glow::VertexArray,
    vertex_buffer: glow::Buffer,
    texture: glow::Texture,
    texture_size: [u32; 2],
    /// Current GL_TEXTURE_MIN/MAG_FILTER, to avoid redundant state changes.
    filter: u32,
    u_uv: Option<glow::UniformLocation>,
    u_matrix: Option<glow::UniformLocation>,
    u_offset: Option<glow::UniformLocation>,
    u_texture: Option<glow::UniformLocation>,
    u_round: Option<glow::UniformLocation>,
    u_vignette: Option<glow::UniformLocation>,
    u_keep: Option<glow::UniformLocation>,
    u_dim: Option<glow::UniformLocation>,
    a_pos: u32,
}

#[derive(Default)]
struct Shared {
    /// OpenGL is rendered by the CPU (llvmpipe & co): trade bilinear filtering
    /// for nearest sampling when the image is shown close to 1:1.
    software: bool,
    gl: Option<GlObjects>,
    /// Newest frame not yet uploaded. Replaced, never queued.
    pending: Option<RgbaImage>,
    /// Size of the frame currently in the texture.
    shown: Option<[u32; 2]>,
    failed: bool,
}

/// Darkening applied inside the camera shader, so the overlays that used to be
/// separate blended full-width layers cost nothing extra without a GPU.
#[derive(Clone, Copy)]
pub struct Shade {
    /// Top vignette height (fraction of the view) and its max darkness (0..1).
    pub top: (f32, f32),
    /// Bottom vignette height and darkness.
    pub bottom: (f32, f32),
    /// Area kept bright (print area), in 0..1 view coordinates, and the darkness outside it.
    pub keep: Rect,
    pub outside: f32,
    /// Uniform dimming (countdown).
    pub dim: f32,
}

impl Default for Shade {
    fn default() -> Self {
        Self { top: (0.0, 0.0), bottom: (0.0, 0.0), keep: Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), outside: 0.0, dim: 0.0 }
    }
}

pub struct LiveView {
    shared: Arc<Mutex<Shared>>,
    recycle: SyncSender<RgbaImage>,
    /// Size of the most recent frame, known on the UI thread for layout.
    size: Option<[u32; 2]>,
}

impl LiveView {
    pub fn new(recycle: SyncSender<RgbaImage>, software: bool) -> Self {
        let shared = Shared { software, ..Default::default() };
        Self { shared: Arc::new(Mutex::new(shared)), recycle, size: None }
    }

    /// True once a frame has been received (it is uploaded on the next paint).
    pub fn has_frame(&self) -> bool { self.size.is_some() }

    pub fn frame_size(&self) -> Option<[u32; 2]> { self.size }

    pub fn failed(&self) -> bool { self.shared.lock().map_or(true, |s| s.failed) }

    pub fn submit(&mut self, frame: RgbaImage) {
        self.size = Some([frame.width(), frame.height()]);
        let Ok(mut shared) = self.shared.lock() else { return; };
        if let Some(dropped) = shared.pending.replace(frame) {
            let _ = self.recycle.try_send(dropped);
        }
    }

    /// Forget the current image (between guests). The GL texture is kept: it is
    /// GPU memory, and re-creating it for every guest would cost more.
    pub fn clear(&mut self) {
        self.size = None;
        if let Ok(mut shared) = self.shared.lock() {
            if let Some(frame) = shared.pending.take() { let _ = self.recycle.try_send(frame); }
            shared.shown = None;
        }
    }

    /// Queue the GL draw for `rect`. Nothing is drawn until a frame exists.
    /// Queue the GL draw for `rect`. Several calls per frame (main view + filter
    /// thumbnails) share one texture upload: only the first draw uploads.
    pub fn paint(&self, painter: &egui::Painter, rect: Rect, mirror: bool, filter: PhotoFilter, radius: f32, shade: Shade) -> bool {
        let Some([w, h]) = self.size else { return false; };
        let mut uv = super::ui::cover_uv(egui::vec2(w as f32, h as f32), rect.size());
        if mirror { uv = Rect::from_min_max(egui::pos2(uv.max.x, uv.min.y), egui::pos2(uv.min.x, uv.max.y)); }
        let (matrix, offset) = filter_matrix(filter);
        let shared = Arc::clone(&self.shared);
        let recycle = self.recycle.clone();
        let callback = egui_glow::CallbackFn::new(move |info, painter| {
            let Ok(mut shared) = shared.lock() else { return; };
            let ppp = info.pixels_per_point;
            let round = [info.viewport.width() * ppp, info.viewport.height() * ppp, radius * ppp];
            // SAFETY: called by egui_glow on the thread that owns the GL context,
            // between its own draw calls; egui_glow restores its state afterwards.
            let params = DrawParams { uv, matrix, offset, round, shade };
            unsafe { shared.draw(painter.gl(), &params, &recycle) };
        });
        painter.add(egui::PaintCallback { rect, callback: Arc::new(callback) });
        true
    }

    /// Release GL objects (on exit). Safe to call without a context.
    pub fn destroy(&self, gl: &glow::Context) {
        if let Ok(mut shared) = self.shared.lock() {
            if let Some(objects) = shared.gl.take() {
                // SAFETY: objects were created on this context.
                unsafe {
                    gl.delete_program(objects.program);
                    gl.delete_vertex_array(objects.vertex_array);
                    gl.delete_buffer(objects.vertex_buffer);
                    gl.delete_texture(objects.texture);
                }
            }
        }
    }
}

/// Per-draw uniforms.
struct DrawParams {
    uv: Rect,
    matrix: [f32; 9],
    offset: [f32; 3],
    round: [f32; 3],
    shade: Shade,
}

impl Shared {
    unsafe fn draw(&mut self, gl: &glow::Context, params: &DrawParams, recycle: &SyncSender<RgbaImage>) {
        let DrawParams { uv, matrix, offset, round, shade } = params;
        let (uv, offset, round) = (*uv, *offset, *round);
        if self.failed { return; }
        if self.gl.is_none() {
            match unsafe { create_objects(gl) } {
                Ok(objects) => self.gl = Some(objects),
                Err(error) => {
                    eprintln!("Live view GL init failed, falling back to textures: {error}");
                    self.failed = true;
                    return;
                }
            }
        }
        let objects = self.gl.as_mut().expect("initialised above");
        unsafe {
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(objects.texture));
            if let Some(frame) = self.pending.take() {
                let size = [frame.width(), frame.height()];
                // RGBA8 in, RGBA8 stored: a plain copy for the driver, no CPU repacking.
                if objects.texture_size != size {
                    gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA8 as i32, size[0] as i32, size[1] as i32, 0,
                        glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelUnpackData::Slice(Some(frame.as_raw())));
                    objects.texture_size = size;
                } else {
                    gl.tex_sub_image_2d(glow::TEXTURE_2D, 0, 0, 0, size[0] as i32, size[1] as i32,
                        glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelUnpackData::Slice(Some(frame.as_raw())));
                }
                self.shown = Some(size);
                let _ = recycle.try_send(frame);
            }
            if self.shown.is_none() { return; }
            // Without a GPU, bilinear filtering is ~20 % of the camera quad's cost.
            // Near 1:1 (720p camera on an 800p screen = x1.11) nearest sampling is
            // visually equivalent; beyond x1.3 magnification keep bilinear.
            let texels = (uv.max.x - uv.min.x).abs() * objects.texture_size[0] as f32;
            let magnification = if texels > 0.0 { round[0] / texels } else { 1.0 };
            let wanted = if self.software && magnification <= 1.3 { glow::NEAREST } else { glow::LINEAR };
            if wanted != objects.filter {
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, wanted as i32);
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, wanted as i32);
                objects.filter = wanted;
            }
            gl.use_program(Some(objects.program));
            gl.uniform_4_f32(objects.u_uv.as_ref(), uv.min.x, uv.min.y, uv.max.x, uv.max.y);
            gl.uniform_matrix_3_f32_slice(objects.u_matrix.as_ref(), false, matrix.as_slice());
            gl.uniform_3_f32(objects.u_offset.as_ref(), offset[0], offset[1], offset[2]);
            gl.uniform_1_i32(objects.u_texture.as_ref(), 0);
            gl.uniform_3_f32(objects.u_round.as_ref(), round[0], round[1], round[2]);
            gl.uniform_4_f32(objects.u_vignette.as_ref(), shade.top.0, shade.top.1, shade.bottom.0, shade.bottom.1);
            gl.uniform_4_f32(objects.u_keep.as_ref(), shade.keep.min.x, shade.keep.min.y, shade.keep.max.x, shade.keep.max.y);
            gl.uniform_2_f32(objects.u_dim.as_ref(), shade.outside, shade.dim);
            gl.bind_vertex_array(Some(objects.vertex_array));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(objects.vertex_buffer));
            gl.enable_vertex_attrib_array(objects.a_pos);
            gl.vertex_attrib_pointer_f32(objects.a_pos, 2, glow::FLOAT, false, 8, 0);
            // Rounded thumbnails blend (egui's premultiplied mode, corners fade out);
            // the opaque full-screen view skips blending: no framebuffer read per pixel.
            let opaque = round[2] <= 0.0;
            if opaque { gl.disable(glow::BLEND); }
            gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
            if opaque { gl.enable(glow::BLEND); }
            gl.bind_vertex_array(None);
        }
    }
}

unsafe fn create_objects(gl: &glow::Context) -> Result<GlObjects, String> {
    let version = egui_glow::ShaderVersion::get(gl);
    let modern = version.is_new_shader_interface();
    let header = format!("{}{}", version.version_declaration(),
        if matches!(version, egui_glow::ShaderVersion::Es100 | egui_glow::ShaderVersion::Es300) { "precision mediump float;\n" } else { "" });
    let (attr, vary_out, vary_in) = if modern { ("in", "out", "in") } else { ("attribute", "varying", "varying") };
    let vertex = format!("{header}
{attr} vec2 a_pos;
uniform vec4 u_uv;
{vary_out} vec2 v_uv;
{vary_out} vec2 v_local;
void main() {{
    v_uv = mix(u_uv.xy, u_uv.zw, a_pos);
    v_local = a_pos;
    gl_Position = vec4(a_pos.x * 2.0 - 1.0, 1.0 - a_pos.y * 2.0, 0.0, 1.0);
}}");
    let (frag_decl, frag_out, sample) = if modern { ("out vec4 frag_color;", "frag_color", "texture") } else { ("", "gl_FragColor", "texture2D") };
    let fragment = format!("{header}
uniform sampler2D u_texture;
uniform mat3 u_matrix;
uniform vec3 u_offset;
uniform vec3 u_round; // width px, height px, corner radius px
uniform vec4 u_vignette; // top height, top darkness, bottom height, bottom darkness
uniform vec4 u_keep; // print area (min.xy, max.xy) in view coordinates
uniform vec2 u_dim; // darkness outside the print area, global darkness
{vary_in} vec2 v_uv;
{vary_in} vec2 v_local;
{frag_decl}
void main() {{
    vec3 rgb = clamp(u_matrix * {sample}(u_texture, v_uv).rgb + u_offset, 0.0, 1.0);
    // Full-screen view: opaque, no per-pixel corner maths (matters without a GPU).
    // Vignettes, print-area shading and dimming are folded in here: one pass
    // instead of several blended full-width layers.
    if (u_round.z <= 0.0) {{
        float y = v_local.y;
        float shade = (1.0 - u_vignette.y * clamp(1.0 - y / max(u_vignette.x, 0.0001), 0.0, 1.0))
                    * (1.0 - u_vignette.w * clamp((y - 1.0 + u_vignette.z) / max(u_vignette.z, 0.0001), 0.0, 1.0))
                    * (1.0 - u_dim.y);
        if (v_local.x < u_keep.x || v_local.x > u_keep.z || y < u_keep.y || y > u_keep.w) {{ shade *= 1.0 - u_dim.x; }}
        {frag_out} = vec4(rgb * shade, 1.0);
        return;
    }}
    // Rounded-rectangle coverage (signed distance), premultiplied for egui's blending.
    vec2 half_size = 0.5 * u_round.xy;
    vec2 q = abs(v_local * u_round.xy - half_size) - (half_size - vec2(u_round.z));
    float d = length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - u_round.z;
    float a = clamp(0.5 - d, 0.0, 1.0);
    {frag_out} = vec4(rgb * a, a);
}}");
    unsafe {
        let program = gl.create_program()?;
        let mut shaders = Vec::new();
        for (kind, source) in [(glow::VERTEX_SHADER, vertex), (glow::FRAGMENT_SHADER, fragment)] {
            let shader = gl.create_shader(kind)?;
            gl.shader_source(shader, &source);
            gl.compile_shader(shader);
            if !gl.get_shader_compile_status(shader) {
                return Err(gl.get_shader_info_log(shader));
            }
            gl.attach_shader(program, shader);
            shaders.push(shader);
        }
        gl.link_program(program);
        if !gl.get_program_link_status(program) {
            return Err(gl.get_program_info_log(program));
        }
        for shader in shaders {
            gl.detach_shader(program, shader);
            gl.delete_shader(shader);
        }
        let a_pos = gl.get_attrib_location(program, "a_pos").ok_or("a_pos missing")?;
        let vertex_array = gl.create_vertex_array()?;
        let vertex_buffer = gl.create_buffer()?;
        gl.bind_vertex_array(Some(vertex_array));
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(vertex_buffer));
        let quad: [f32; 8] = [0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 1.0];
        let bytes: Vec<u8> = quad.iter().flat_map(|v| v.to_ne_bytes()).collect();
        gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, &bytes, glow::STATIC_DRAW);
        gl.bind_vertex_array(None);
        let texture = gl.create_texture()?;
        gl.bind_texture(glow::TEXTURE_2D, Some(texture));
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
        Ok(GlObjects {
            u_uv: gl.get_uniform_location(program, "u_uv"),
            u_matrix: gl.get_uniform_location(program, "u_matrix"),
            u_offset: gl.get_uniform_location(program, "u_offset"),
            u_texture: gl.get_uniform_location(program, "u_texture"),
            u_round: gl.get_uniform_location(program, "u_round"),
            u_vignette: gl.get_uniform_location(program, "u_vignette"),
            u_keep: gl.get_uniform_location(program, "u_keep"),
            u_dim: gl.get_uniform_location(program, "u_dim"),
            program, vertex_array, vertex_buffer, texture, texture_size: [0, 0], filter: glow::LINEAR, a_pos,
        })
    }
}

/// Column-major 3x3 colour matrix + offset, from the shared definition in `effects`.
pub fn filter_matrix(filter: PhotoFilter) -> ([f32; 9], [f32; 3]) {
    let (rows, offset) = boothsoft::effects::color_matrix(filter);
    let mut columns = [0.0; 9];
    for (row, values) in rows.iter().enumerate() {
        for (column, value) in values.iter().enumerate() {
            columns[column * 3 + row] = *value;
        }
    }
    (columns, offset)
}

#[cfg(test)]
mod tests {
    use super::*;
    use boothsoft::effects;

    fn apply(filter: PhotoFilter, rgb: [u8; 3]) -> [u8; 3] {
        let (m, o) = filter_matrix(filter);
        let c = rgb.map(|v| f32::from(v) / 255.0);
        let mut out = [0u8; 3];
        for row in 0..3 {
            let v = m[row] * c[0] + m[3 + row] * c[1] + m[6 + row] * c[2] + o[row];
            out[row] = (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        }
        out
    }

    #[test]
    fn shader_matrices_match_the_cpu_filters_used_for_the_saved_photo() {
        for filter in PhotoFilter::ALL {
            for rgb in [[0, 0, 0], [255, 255, 255], [200, 120, 40], [10, 90, 230], [128, 128, 128]] {
                let mut image = image::RgbImage::from_pixel(1, 1, image::Rgb(rgb));
                effects::apply_in_place(&mut image, filter);
                let cpu = image.get_pixel(0, 0).0;
                let gpu = apply(filter, rgb);
                for channel in 0..3 {
                    assert!((i16::from(cpu[channel]) - i16::from(gpu[channel])).abs() <= 3,
                        "{filter:?} {rgb:?}: cpu {cpu:?} gpu {gpu:?}");
                }
            }
        }
    }
}
