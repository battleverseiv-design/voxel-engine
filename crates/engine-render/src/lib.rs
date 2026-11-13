mod avatar;
pub mod textures;

pub use avatar::{build_avatar_mesh, build_first_person_arm_mesh, push_textured_box};

use std::collections::HashMap;
use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use engine_core::{EnvironmentState, PointLight};
use engine_world::{ChunkMesh, VoxelVertex};
use glam::{IVec3, Mat4, Vec3, Vec4};
use wgpu::util::DeviceExt;
use winit::window::Window;

const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct SceneUniform {
    view_proj: [[f32; 4]; 4],
    inv_view_proj: [[f32; 4]; 4],
    light_view_proj: [[f32; 4]; 4],
    fog_color: [f32; 4],
    camera_pos: [f32; 4],
    sun_dir: [f32; 4],
    sun_color: [f32; 4],
    ambient_sky: [f32; 4],
    ambient_ground: [f32; 4],
    sky_zenith: [f32; 4],
    point_lights_pos: [[f32; 4]; 8],
    point_lights_color: [[f32; 4]; 8],
    point_light_count: [u32; 4],
}

struct GpuChunkMesh {
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct Camera {
    pub position: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub roll: f32,
    pub fov_radians: f32,
    pub near: f32,
    pub far: f32,
}

impl Camera {
    pub fn new(position: Vec3) -> Self {
        Self {
            position,
            yaw: 0.0,
            pitch: -0.2,
            roll: 0.0,
            fov_radians: 75.0_f32.to_radians(),
            near: 0.1,
            far: 600.0,
        }
    }

    pub fn forward(&self) -> Vec3 {
        let (sin_y, cos_y) = self.yaw.sin_cos();
        let (sin_p, cos_p) = self.pitch.sin_cos();
        Vec3::new(cos_p * sin_y, sin_p, -cos_p * cos_y).normalize()
    }

    pub fn forward_xz(&self) -> Vec3 {
        let (sin_y, cos_y) = self.yaw.sin_cos();
        Vec3::new(sin_y, 0.0, -cos_y).normalize()
    }

    pub fn right_xz(&self) -> Vec3 {
        let fwd = self.forward_xz();
        fwd.cross(Vec3::Y).normalize()
    }

    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        let proj = Mat4::perspective_rh(self.fov_radians, aspect.max(0.01), self.near, self.far);
        let fwd = self.forward();
        let target = self.position + fwd;
        let up = if self.roll.abs() > 0.0001 {
            let right = fwd.cross(Vec3::Y).normalize_or_zero();
            let base_up = right.cross(fwd).normalize();
            let (sin_r, cos_r) = self.roll.sin_cos();
            (base_up * cos_r + right * sin_r).normalize()
        } else {
            Vec3::Y
        };
        let view = Mat4::look_at_rh(self.position, target, up);
        proj * view
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Plane {
    pub normal: Vec3,
    pub distance: f32,
}

impl Plane {
    #[inline]
    pub fn distance_to_point(&self, p: Vec3) -> f32 {
        self.normal.dot(p) + self.distance
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Frustum {
    pub planes: [Plane; 6],
}

impl Frustum {
    pub fn from_view_proj(vp: Mat4) -> Self {
        // glam Mat4 is column-major:
        // row i is: (vp.x_axis[i], vp.y_axis[i], vp.z_axis[i], vp.w_axis[i])
        let row0 = Vec4::new(vp.x_axis.x, vp.y_axis.x, vp.z_axis.x, vp.w_axis.x);
        let row1 = Vec4::new(vp.x_axis.y, vp.y_axis.y, vp.z_axis.y, vp.w_axis.y);
        let row2 = Vec4::new(vp.x_axis.z, vp.y_axis.z, vp.z_axis.z, vp.w_axis.z);
        let row3 = Vec4::new(vp.x_axis.w, vp.y_axis.w, vp.z_axis.w, vp.w_axis.w);

        let make_plane = |v: Vec4| -> Plane {
            let n = Vec3::new(v.x, v.y, v.z);
            let len = n.length();
            if len > 1e-6 {
                Plane {
                    normal: n / len,
                    distance: v.w / len,
                }
            } else {
                Plane {
                    normal: Vec3::Y,
                    distance: 0.0,
                }
            }
        };

        // Clip space in WebGPU/DirectX (0 <= z <= w):
        let left = make_plane(row3 + row0);
        let right = make_plane(row3 - row0);
        let bottom = make_plane(row3 + row1);
        let top = make_plane(row3 - row1);
        let near = make_plane(row2);
        let far = make_plane(row3 - row2);

        Self {
            planes: [left, right, bottom, top, near, far],
        }
    }

    /// Tests whether an AABB (min..max) intersects or is inside the frustum.
    /// Returns false if the AABB is completely outside any of the 6 planes.
    #[inline]
    pub fn intersects_aabb(&self, min: Vec3, max: Vec3) -> bool {
        for plane in &self.planes {
            let p = Vec3::new(
                if plane.normal.x >= 0.0 { max.x } else { min.x },
                if plane.normal.y >= 0.0 { max.y } else { min.y },
                if plane.normal.z >= 0.0 { max.z } else { min.z },
            );
            if plane.distance_to_point(p) < 0.0 {
                return false;
            }
        }
        true
    }
}

pub enum RenderError {
    ResizeNeeded,
    OutOfMemory,
    Other(String),
}

pub struct Renderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    size: winit::dpi::PhysicalSize<u32>,
    pipeline: wgpu::RenderPipeline,
    outline_pipeline: wgpu::RenderPipeline,
    sky_pipeline: wgpu::RenderPipeline,
    shadow_pipeline: wgpu::RenderPipeline,
    outline_vertex_buffer: wgpu::Buffer,
    avatar_vertex_buffer: wgpu::Buffer,
    scene_buffer: wgpu::Buffer,
    scene_bind_group: wgpu::BindGroup,
    shadow_bind_group: wgpu::BindGroup,
    depth_view: wgpu::TextureView,
    shadow_view: wgpu::TextureView,
    chunk_meshes: HashMap<IVec3, GpuChunkMesh>,
    egui_renderer: egui_wgpu::Renderer,
    vsync: bool,
    start_time: std::time::Instant,
}

impl Renderer {
    pub fn new(window: Arc<Window>, vsync: bool) -> Self {
        let size = window.inner_size();
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let surface = instance
            .create_surface(window)
            .expect("failed to create wgpu surface");

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .expect("no suitable GPU adapter found");

        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("voxel-engine-device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::Performance,
            },
            None,
        ))
        .expect("failed to create wgpu device");

        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(caps.formats[0]);

        let present_mode = if vsync {
            wgpu::PresentMode::AutoVsync
        } else {
            wgpu::PresentMode::AutoNoVsync
        };

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 1,
        };
        surface.configure(&device, &config);

        let depth_view = create_depth_view(&device, config.width, config.height);

        let default_env = EnvironmentState::default();
        let scene_uniform = SceneUniform {
            view_proj: Mat4::IDENTITY.to_cols_array_2d(),
            inv_view_proj: Mat4::IDENTITY.to_cols_array_2d(),
            light_view_proj: Mat4::IDENTITY.to_cols_array_2d(),
            fog_color: [
                default_env.fog_color[0],
                default_env.fog_color[1],
                default_env.fog_color[2],
                1.0,
            ],
            camera_pos: [0.0, 0.0, 0.0, 180.0],
            sun_dir: [
                default_env.sun_dir.x,
                default_env.sun_dir.y,
                default_env.sun_dir.z,
                default_env.sun_intensity,
            ],
            sun_color: [
                default_env.sun_color[0],
                default_env.sun_color[1],
                default_env.sun_color[2],
                1.0,
            ],
            ambient_sky: [
                default_env.ambient_sky[0],
                default_env.ambient_sky[1],
                default_env.ambient_sky[2],
                1.0,
            ],
            ambient_ground: [
                default_env.ambient_ground[0],
                default_env.ambient_ground[1],
                default_env.ambient_ground[2],
                1.0,
            ],
            sky_zenith: [
                default_env.sky_zenith[0],
                default_env.sky_zenith[1],
                default_env.sky_zenith[2],
                1.0,
            ],
            point_lights_pos: [[0.0; 4]; 8],
            point_lights_color: [[0.0; 4]; 8],
            point_light_count: [0; 4],
        };
        let scene_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("scene-uniform"),
            contents: bytemuck::bytes_of(&scene_uniform),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let (atlas_view, atlas_sampler) = textures::create_block_texture_array(&device, &queue);

        let shadow_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("shadow-depth-texture"),
            size: wgpu::Extent3d {
                width: 2048,
                height: 2048,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });

        let shadow_view = shadow_texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("shadow-view"),
            aspect: wgpu::TextureAspect::DepthOnly,
            ..Default::default()
        });

        let shadow_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("shadow-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });

        let scene_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("scene-bind-group-layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2Array,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

        let scene_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("scene-bind-group"),
            layout: &scene_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: scene_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&atlas_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&atlas_sampler),
                },
            ],
        });

        let shadow_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("shadow-bind-group-layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Depth,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                        count: None,
                    },
                ],
            });

        let shadow_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shadow-bind-group"),
            layout: &shadow_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&shadow_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&shadow_sampler),
                },
            ],
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("voxel-shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("voxel-pipeline-layout"),
            bind_group_layouts: &[&scene_bind_group_layout, &shadow_bind_group_layout],
            push_constant_ranges: &[],
        });

        let shadow_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("shadow-pipeline-layout"),
            bind_group_layouts: &[&scene_bind_group_layout],
            push_constant_ranges: &[],
        });

        let vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<VoxelVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: 12,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: 24,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: 36,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Float32,
                },
                wgpu::VertexAttribute {
                    offset: 40,
                    shader_location: 4,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: 48,
                    shader_location: 5,
                    format: wgpu::VertexFormat::Uint32,
                },
                wgpu::VertexAttribute {
                    offset: 52,
                    shader_location: 6,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        };

        let shadow_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("shadow-pipeline"),
            layout: Some(&shadow_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_shadow"),
                buffers: &[vertex_layout.clone()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_shadow"),
                targets: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState {
                    constant: 3,
                    slope_scale: 2.5,
                    clamp: 0.0,
                },
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("voxel-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[vertex_layout],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let outline_vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[wgpu::VertexAttribute {
                offset: 0,
                shader_location: 0,
                format: wgpu::VertexFormat::Float32x3,
            }],
        };

        let outline_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("voxel-outline-pipeline"),
            layout: Some(&shadow_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_outline"),
                buffers: &[outline_vertex_layout],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_outline"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::LineList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let outline_vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("outline-vb"),
            size: (24 * std::mem::size_of::<[f32; 3]>()) as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let avatar_vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("avatar-vb"),
            size: (65536 * std::mem::size_of::<VoxelVertex>()) as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let sky_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("voxel-sky-pipeline"),
            layout: Some(&shadow_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_sky"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_sky"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Always,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let egui_renderer = egui_wgpu::Renderer::new(&device, format, None, 1, true);

        Self {
            surface,
            device,
            queue,
            config,
            size,
            pipeline,
            outline_pipeline,
            sky_pipeline,
            shadow_pipeline,
            outline_vertex_buffer,
            avatar_vertex_buffer,
            scene_buffer,
            scene_bind_group,
            shadow_bind_group,
            depth_view,
            shadow_view,
            chunk_meshes: HashMap::new(),
            egui_renderer,
            vsync,
            start_time: std::time::Instant::now(),
        }
    }

    pub fn set_vsync(&mut self, vsync: bool) {
        if self.vsync == vsync {
            return;
        }
        self.vsync = vsync;
        self.config.present_mode = if vsync {
            wgpu::PresentMode::AutoVsync
        } else {
            wgpu::PresentMode::AutoNoVsync
        };
        self.surface.configure(&self.device, &self.config);
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width == 0 || new_size.height == 0 {
            return;
        }
        self.size = new_size;
        self.config.width = new_size.width;
        self.config.height = new_size.height;
        self.surface.configure(&self.device, &self.config);
        self.depth_view = create_depth_view(&self.device, new_size.width, new_size.height);
    }

    pub fn upload_chunk_meshes(&mut self, meshes: Vec<(IVec3, ChunkMesh)>) {
        for (pos, mesh) in meshes {
            if mesh.indices.is_empty() {
                self.chunk_meshes.remove(&pos);
                continue;
            }
            let vertex_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("chunk-vb"),
                contents: bytemuck::cast_slice(&mesh.vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
            let index_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("chunk-ib"),
                contents: bytemuck::cast_slice(&mesh.indices),
                usage: wgpu::BufferUsages::INDEX,
            });
            self.chunk_meshes.insert(
                pos,
                GpuChunkMesh {
                    vertex_buffer,
                    index_buffer,
                    index_count: mesh.indices.len() as u32,
                },
            );
        }
    }

    pub fn unload_chunks(&mut self, coords: &[IVec3]) {
        for pos in coords {
            self.chunk_meshes.remove(pos);
        }
    }

    pub fn clear_chunk_meshes(&mut self) {
        self.chunk_meshes.clear();
    }

    pub fn render(
        &mut self,
        camera: &Camera,
        fog_distance: f32,
        brightness: f32,
        environment: &EnvironmentState,
        point_lights: &[PointLight],
        avatar_vertices: Option<&[VoxelVertex]>,
        selected_block: Option<IVec3>,
        pixels_per_point: f32,
        paint_jobs: &[egui::ClippedPrimitive],
        textures_delta: &egui::TexturesDelta,
    ) -> Result<(), RenderError> {
        let aspect = self.size.width as f32 / self.size.height.max(1) as f32;
        let view_proj = camera.view_proj(aspect);
        let inv_view_proj = view_proj.inverse();

        // Directional Light Projection for dynamic cascaded shadows (Sun by day, Moon by night)
        let shadow_range = 95.0_f32;
        let sun_dir_norm = environment.sun_dir.normalize();
        let light_dir = if sun_dir_norm.y > -0.05 {
            Vec3::new(sun_dir_norm.x, sun_dir_norm.y.max(0.08), sun_dir_norm.z).normalize()
        } else {
            Vec3::new(-sun_dir_norm.x, (-sun_dir_norm.y).max(0.12), -sun_dir_norm.z).normalize()
        };

        let shadow_texel = (shadow_range * 2.0) / 2048.0;
        let snapped_x = (camera.position.x / shadow_texel).floor() * shadow_texel;
        let snapped_y = (camera.position.y / shadow_texel).floor() * shadow_texel;
        let snapped_z = (camera.position.z / shadow_texel).floor() * shadow_texel;
        let shadow_center = Vec3::new(snapped_x, snapped_y, snapped_z);

        let light_pos = shadow_center + light_dir * (shadow_range * 1.4);
        let up = if light_dir.y.abs() > 0.95 {
            Vec3::Z
        } else {
            Vec3::Y
        };
        let light_view = Mat4::look_to_rh(light_pos, -light_dir, up);
        let light_proj = Mat4::orthographic_rh(
            -shadow_range,
            shadow_range,
            -shadow_range,
            shadow_range,
            1.0,
            shadow_range * 2.8,
        );
        let light_view_proj = light_proj * light_view;

        let uniform = SceneUniform {
            view_proj: view_proj.to_cols_array_2d(),
            inv_view_proj: inv_view_proj.to_cols_array_2d(),
            light_view_proj: light_view_proj.to_cols_array_2d(),
            fog_color: [
                environment.fog_color[0],
                environment.fog_color[1],
                environment.fog_color[2],
                brightness,
            ],
            camera_pos: [camera.position.x, camera.position.y, camera.position.z, fog_distance],
            sun_dir: [
                environment.sun_dir.x,
                environment.sun_dir.y,
                environment.sun_dir.z,
                environment.sun_intensity,
            ],
            sun_color: [
                environment.sun_color[0],
                environment.sun_color[1],
                environment.sun_color[2],
                1.0,
            ],
            ambient_sky: [
                environment.ambient_sky[0],
                environment.ambient_sky[1],
                environment.ambient_sky[2],
                1.0,
            ],
            ambient_ground: [
                environment.ambient_ground[0],
                environment.ambient_ground[1],
                environment.ambient_ground[2],
                1.0,
            ],
            sky_zenith: [
                environment.sky_zenith[0],
                environment.sky_zenith[1],
                environment.sky_zenith[2],
                self.start_time.elapsed().as_secs_f32(),
            ],
            point_lights_pos: {
                let mut pos = [[0.0f32; 4]; 8];
                let count = point_lights.len().min(8);
                for i in 0..count {
                    let pl = &point_lights[i];
                    pos[i] = [pl.position.x, pl.position.y, pl.position.z, pl.radius];
                }
                pos
            },
            point_lights_color: {
                let mut col = [[0.0f32; 4]; 8];
                let count = point_lights.len().min(8);
                for i in 0..count {
                    let pl = &point_lights[i];
                    col[i] = [pl.color[0], pl.color[1], pl.color[2], pl.intensity];
                }
                col
            },
            point_light_count: [point_lights.len().min(8) as u32, 0, 0, 0],
        };
        self.queue
            .write_buffer(&self.scene_buffer, 0, bytemuck::bytes_of(&uniform));

        if let Some(pos) = selected_block {
            let vertices = build_outline_vertices(pos);
            self.queue
                .write_buffer(&self.outline_vertex_buffer, 0, bytemuck::cast_slice(&vertices));
        }

        let avatar_vertex_count = if let Some(verts) = avatar_vertices {
            if !verts.is_empty() {
                let count = verts.len().min(65536);
                self.queue.write_buffer(
                    &self.avatar_vertex_buffer,
                    0,
                    bytemuck::cast_slice(&verts[..count]),
                );
                count as u32
            } else {
                0
            }
        } else {
            0
        };

        let output = match self.surface.get_current_texture() {
            Ok(frame) => frame,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                return Err(RenderError::ResizeNeeded);
            }
            Err(wgpu::SurfaceError::OutOfMemory) => return Err(RenderError::OutOfMemory),
            Err(err) => return Err(RenderError::Other(format!("{err:?}"))),
        };
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("render-encoder"),
            });

        // 1. Dynamic Directional Cascaded Shadow Pass
        {
            let mut shadow_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("shadow-pass"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.shadow_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            shadow_pass.set_pipeline(&self.shadow_pipeline);
            shadow_pass.set_bind_group(0, &self.scene_bind_group, &[]);

            let shadow_sq = (shadow_range + 20.0) * (shadow_range + 20.0);
            for (chunk_pos, mesh) in &self.chunk_meshes {
                let center = (chunk_pos.as_vec3() + Vec3::splat(0.5)) * 16.0;
                if center.distance_squared(camera.position) > shadow_sq {
                    continue;
                }
                shadow_pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                shadow_pass.set_index_buffer(mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                shadow_pass.draw_indexed(0..mesh.index_count, 0, 0..1);
            }

            if avatar_vertex_count > 0 {
                shadow_pass.set_vertex_buffer(0, self.avatar_vertex_buffer.slice(..));
                shadow_pass.draw(0..avatar_vertex_count, 0..1);
            }
        }

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("voxel-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: environment.fog_color[0] as f64,
                            g: environment.fog_color[1] as f64,
                            b: environment.fog_color[2] as f64,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // 1. Procedural Sky & Sun Disc
            pass.set_pipeline(&self.sky_pipeline);
            pass.set_bind_group(0, &self.scene_bind_group, &[]);
            pass.draw(0..3, 0..1);

            // 2. Voxel chunks
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.scene_bind_group, &[]);
            pass.set_bind_group(1, &self.shadow_bind_group, &[]);
            let camera_frustum = Frustum::from_view_proj(view_proj);
            for (chunk_pos, chunk) in &self.chunk_meshes {
                let center = (chunk_pos.as_vec3() + Vec3::splat(0.5)) * 16.0;
                // Always render chunks immediately surrounding the player (36m radius)
                if center.distance_squared(camera.position) > 36.0 * 36.0 {
                    // Generous 4-block margin prevents any edge-of-screen pop-in or water wave displacement issues
                    let margin = 4.0;
                    let min = (chunk_pos * 16).as_vec3() - Vec3::splat(margin);
                    let max = min + Vec3::splat(16.0 + margin * 2.0);
                    if !camera_frustum.intersects_aabb(min, max) {
                        continue;
                    }
                }
                pass.set_vertex_buffer(0, chunk.vertex_buffer.slice(..));
                pass.set_index_buffer(chunk.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..chunk.index_count, 0, 0..1);
            }

            // 3. Avatar character (Unturned style humanoid)
            if avatar_vertex_count > 0 {
                pass.set_vertex_buffer(0, self.avatar_vertex_buffer.slice(..));
                pass.draw(0..avatar_vertex_count, 0..1);
            }

            // 4. Selection outline
            if selected_block.is_some() {
                pass.set_pipeline(&self.outline_pipeline);
                pass.set_bind_group(0, &self.scene_bind_group, &[]);
                pass.set_vertex_buffer(0, self.outline_vertex_buffer.slice(..));
                pass.draw(0..24, 0..1);
            }
        }

        for (id, delta) in &textures_delta.set {
            self.egui_renderer
                .update_texture(&self.device, &self.queue, *id, delta);
        }

        let screen_descriptor = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [self.config.width, self.config.height],
            pixels_per_point,
        };
        let extra_cmds = self.egui_renderer.update_buffers(
            &self.device,
            &self.queue,
            &mut encoder,
            paint_jobs,
            &screen_descriptor,
        );

        {
            let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("egui-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            let mut pass = pass.forget_lifetime();
            self.egui_renderer
                .render(&mut pass, paint_jobs, &screen_descriptor);
        }

        for id in &textures_delta.free {
            self.egui_renderer.free_texture(id);
        }

        self.queue.submit(
            extra_cmds
                .into_iter()
                .chain(std::iter::once(encoder.finish())),
        );
        output.present();
        Ok(())
    }
}

fn build_outline_vertices(pos: IVec3) -> [[f32; 3]; 24] {
    let eps = 0.002_f32;
    let x0 = pos.x as f32 - eps;
    let y0 = pos.y as f32 - eps;
    let z0 = pos.z as f32 - eps;
    let x1 = pos.x as f32 + 1.0 + eps;
    let y1 = pos.y as f32 + 1.0 + eps;
    let z1 = pos.z as f32 + 1.0 + eps;

    [
        // Bottom 4 edges
        [x0, y0, z0], [x1, y0, z0],
        [x1, y0, z0], [x1, y0, z1],
        [x1, y0, z1], [x0, y0, z1],
        [x0, y0, z1], [x0, y0, z0],
        // Top 4 edges
        [x0, y1, z0], [x1, y1, z0],
        [x1, y1, z0], [x1, y1, z1],
        [x1, y1, z1], [x0, y1, z1],
        [x0, y1, z1], [x0, y1, z0],
        // Vertical 4 edges
        [x0, y0, z0], [x0, y1, z0],
        [x1, y0, z0], [x1, y1, z0],
        [x1, y0, z1], [x1, y1, z1],
        [x0, y0, z1], [x0, y1, z1],
    ]
}

fn create_depth_view(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("depth-texture"),
        size: wgpu::Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frustum_culling() {
        let camera = Camera::new(Vec3::new(8.0, 17.5, 8.0));
        let aspect = 16.0 / 9.0;
        let vp = camera.view_proj(aspect);
        let f = Frustum::from_view_proj(vp);
        assert_eq!(f.planes.len(), 6);

        // Chunk in front of camera should be visible
        let min = Vec3::new(0.0, 0.0, -32.0);
        let max = Vec3::new(16.0, 16.0, -16.0);
        assert!(f.intersects_aabb(min, max));
    }
}
