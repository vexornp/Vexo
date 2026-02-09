use crate::QuadInstance::QuadInstance;

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub pos: [f32; 3],
}

impl Vertex {
    const ATTRIBS: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![0 => Float32x3];

    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        use std::mem;
        wgpu::VertexBufferLayout {
            array_stride: mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}

pub struct TextRequest {
    pub content: String,
    pub position: (f32, f32),
    pub size: f32,
    pub color: [f32; 4],
}

pub struct Bounds {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

pub struct EditorRequest {
    pub id: String,
    pub bounds: Bounds, // x, y, width, height
    pub color: [f32; 4],
}

pub struct UiBatcher {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u16>,
    pub text_requests: Vec<TextRequest>, // For normal Text widget
    pub editor_requests: Vec<EditorRequest>, // For TextEdit widget
    pub quad_instances: Vec<QuadInstance>,

    screen_width: f32,  // Logical width: pixel_width * scale_factor
    screen_height: f32, // Logical height: pixel_height * scale_factor
}

impl UiBatcher {
    pub fn new() -> Self {
        Self {
            vertices: Vec::new(),
            indices: Vec::new(),
            text_requests: Vec::new(),
            editor_requests: Vec::new(),
            quad_instances: Vec::new(),
            screen_width: 1.0,
            screen_height: 1.0,
        }
    }

    pub fn clear(&mut self) {
        self.vertices.clear();
        self.indices.clear();
        self.text_requests.clear();
        self.editor_requests.clear();
        self.quad_instances.clear();
    }

    // Set logical size
    pub fn set_screen_size(&mut self, width: f32, height: f32) {
        self.screen_width = width;
        self.screen_height = height;
    }

    pub fn add_rect(
        &mut self,
        pos: [f32; 2],
        size: [f32; 2],
        color: [f32; 4],
        border_color: [f32; 4],
        border_width: f32,
    ) {
        self.quad_instances.push(QuadInstance {
            position: pos,
            size,
            color,
            border_color,
            border_width,
            _padding: [0.0; 3],
        });
    }

    pub fn add_text(&mut self, content: String, x: f32, y: f32, size: f32, color: [f32; 3]) {
        let color_rgba = [color[0], color[1], color[2], 1.0];

        self.text_requests.push(TextRequest {
            content,
            position: (x, y),
            size,
            color: color_rgba,
        });
    }

    pub fn add_editor_request(&mut self, id: impl Into<String>, bounds: Bounds) {
        self.editor_requests.push(EditorRequest {
            id: id.into(),
            bounds,
            color: [1.0, 1.0, 1.0, 1.0],
        });
    }
}
