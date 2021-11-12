/// Shader and things to bend the world
///
/// ForeNote : My understanding ob bevy's internals is very incomplete, as my shaders understanding is.
///            Lots of code may be underoptimum.
///
/// So, the idea is to, render time, bend the meshes as if there were a sphere (like in eco or animal crossing).
/// Doing so require to tinker both the mesh and shadow rendering.
/// The Shadow part is required because the shadow "map" are calculated in another pipeline.
/// If the shadow map is not bent the same way as the world is, the shadows will be completely borked.
///
/// From the mesh side it applies a transformation on each vertex based on how far there are from the camera.
/// For the shadow, we firts have to send the camera position (as during shadow render phase, the
///  camera that does the rendering is the light itself, and we want to bind from the true camera
///  pos and not the shadow one.)
///
/// Current limitation :
/// * shitty algo : the bending is not a true sphere (I will soon correct that)
/// * Cannot handle multiple camera
/// * Do not yet bend light position (leading to "moving" shadows)
///
/// Usage :
/// Simply use BendPbrBundle on meshes you want to bend where you would have use a standard pbr thing
/// Add the BendOrigin on the camera (it is the point the world will be bent from)

use bevy::{
    prelude::AddAsset,

    app::{
        App,
        Plugin,
    },
    asset::Handle,
    core_pipeline::Transparent3d,
    ecs::{
        bundle::Bundle,
        entity::Entity,
        query::With,
        system::{
            Commands,
            lifetimeless::{
                Read,
                SQuery,
                SRes,
            },
            Query,
            Res,
            ResMut,
            SystemParamItem,
            SystemState,
        },
        world::{
            FromWorld,
            World,
        },
    },
    math::{
        Mat4,
        Vec3,
        Vec4,
    },
    pbr2::{
        DrawMesh,
        GpuStandardMaterial,
        LightMeta,
        MeshUniform,
        NotShadowCaster,
        PbrShaders,
        PbrViewBindGroup,
        SHADOW_FORMAT,
        SetMeshViewBindGroup,
        SetTransformBindGroup,
        Shadow,
        StandardMaterial,
        TransformBindGroup,
        ViewLights,
    },

    reflect::TypeUuid,
    render2::{
        mesh::Mesh,
        RenderApp,
        RenderStage,
        render_asset::{
            PrepareAssetError,
            RenderAsset,
            RenderAssetPlugin,
            RenderAssets,
        },
        render_component::{
            DynamicUniformIndex,
            ExtractComponentPlugin,
        },
        render_phase::{
            AddRenderCommand,
            Draw,
            DrawFunctions,
            RenderCommand,
            RenderPhase,
            TrackedRenderPass,
        },
        render_resource::*,
        renderer::{
            RenderDevice,
            RenderQueue,
        },
        shader::Shader,
        texture::{
            BevyDefault,
            GpuImage,
            Image,
            TextureFormatPixelInfo,
        },
        view::{
            ExtractedView,
            ViewUniformOffset,
            ViewUniforms,
        },
    },
    transform::components::{
        GlobalTransform,
        Transform
    },
};

use wgpu::{
    ImageCopyTexture,
    ImageDataLayout,
    Origin3d,
};

use crevice::std140::AsStd140;


//
// =============== Material part ===============
//


/// Add everything required for the bendmaterial to work correctly
pub struct BendMaterialPlugin;
impl Plugin for BendMaterialPlugin {
    fn build(&self, app: &mut App) {
        app.add_asset::<BendMaterial>()
            // whut the hell is extract shit ?
            .add_plugin(ExtractComponentPlugin::<Handle<BendMaterial>>::default())
            .add_plugin(RenderAssetPlugin::<BendMaterial>::default())
        ;
        app.sub_app(RenderApp)
            .add_render_command::<Transparent3d, DrawBend>()
            .init_resource::<BendPipeline>()
            .add_system_to_stage(RenderStage::Queue,   queue_bendmaterial)

            .add_system_to_stage(RenderStage::Extract, extract_camera_pos)
            .add_system_to_stage(RenderStage::Queue,   queue_shadows)
            .add_system_to_stage(RenderStage::Queue,   queue_bend_shadow_view_bind_group)
            .add_system_to_stage(RenderStage::Queue,   queue_camera_pos_bind_group)
            .add_system_to_stage(RenderStage::Prepare, prepare_camera_pos)
            .init_resource::<BendShadowShaders>()
            .init_resource::<CameraPos>()
            .init_resource::<CameraPosUniforms>()
        ;

        let render_app = app.sub_app(RenderApp);
        let draw_shadow_mesh = DrawCustomShadowMesh::new(&mut render_app.world);
        let render_world = render_app.world.cell();
        let draw_functions = render_world
            .get_resource::<DrawFunctions<Shadow>>()
            .unwrap();
        draw_functions.write().add(draw_shadow_mesh);
    }
}

/// queue the meshes to be rendered
#[allow(clippy::too_many_arguments)]
pub fn queue_bendmaterial(
    mut commands: Commands,
    transparent_3d_draw_functions: Res<DrawFunctions<Transparent3d>>,
    render_device: Res<RenderDevice>,
    pbr_shaders: Res<BendPipeline>,         // our pipeline
    shadow_shaders: Res<BendShadowShaders>, // our shadow pipeline
    light_meta: Res<LightMeta>,
    view_uniforms: Res<ViewUniforms>,
    render_materials: Res<RenderAssets<BendMaterial>>,
    standard_material_meshes: Query<
        (Entity, &Handle<BendMaterial>, &MeshUniform),
        With<Handle<Mesh>>,
    >,
    mut views: Query<(
        Entity,
        &ExtractedView,
        &ViewLights,
        &mut RenderPhase<Transparent3d>,
    )>,
) {
    if let (Some(view_binding), Some(light_binding)) = (
        view_uniforms.uniforms.binding(),
        light_meta.view_gpu_lights.binding(),
    ) {
        for (entity, view, view_lights, mut transparent_phase) in views.iter_mut() {
            let view_bind_group = render_device.create_bind_group(&BindGroupDescriptor {
                entries: &[
                    BindGroupEntry {
                        binding: 0,
                        resource: view_binding.clone(),
                    },
                    BindGroupEntry {
                        binding: 1,
                        resource: light_binding.clone(),
                    },
                    BindGroupEntry {
                        binding: 2,
                        resource: BindingResource::TextureView(
                            &view_lights.point_light_depth_texture_view,
                        ),
                    },
                    BindGroupEntry {
                        binding: 3,
                        resource: BindingResource::Sampler(&shadow_shaders.point_light_sampler),
                    },
                    BindGroupEntry {
                        binding: 4,
                        resource: BindingResource::TextureView(
                            &view_lights.directional_light_depth_texture_view,
                        ),
                    },
                    BindGroupEntry {
                        binding: 5,
                        resource: BindingResource::Sampler(
                            &shadow_shaders.directional_light_sampler,
                        ),
                    },
                ],
                label: Some("pbr_view_bind_group"),
                layout: &pbr_shaders.view_layout,
            });

            commands.entity(entity).insert(PbrViewBindGroup {
                value: view_bind_group,
            });

            let draw_pbr = transparent_3d_draw_functions
                .read()
                .get_id::<DrawBend>()
                .unwrap();

            let view_matrix = view.transform.compute_matrix();
            let view_row_2 = view_matrix.row(2);

            for (entity, material_handle, mesh_uniform) in standard_material_meshes.iter() {
                if !render_materials.contains_key(material_handle) {
                    continue;
                }
                // NOTE: row 2 of the view matrix dotted with column 3 of the model matrix
                //       gives the z component of translation of the mesh in view space
                let mesh_z = view_row_2.dot(mesh_uniform.transform.col(3));
                // TODO: currently there is only "transparent phase". this should pick transparent vs opaque according to the mesh material
                transparent_phase.add(Transparent3d {
                    entity,
                    draw_function: draw_pbr,
                    distance: mesh_z,
                });
            }
        }
    }
}

/// The material that will bend the world.
/// Actually just a redefinition of the pbr material
#[derive(Debug, Clone, TypeUuid)]
#[uuid = "4ee9c363-1124-4113-890e-199d81b00281"]
pub struct BendMaterial {
    pub standard_material : StandardMaterial,
}

impl Default for BendMaterial {
    fn default() -> Self {
        Self {
            standard_material : Default::default(),
        }
    }
}

/// Not completely sure of what this does
/// I think it transform the standard material to a GPU friendly version
impl RenderAsset for BendMaterial {
    type ExtractedAsset = BendMaterial;
    type PreparedAsset = GpuStandardMaterial;
    type Param = (
        SRes<RenderDevice>,
        SRes<PbrShaders>,
        SRes<RenderAssets<Image>>,
    );

    fn extract_asset(&self) -> Self::ExtractedAsset {
        self.clone()
    }

    fn prepare_asset(
        material: Self::ExtractedAsset,
        //(render_device, pbr_shaders, gpu_images): &mut SystemParamItem<Self::Param>,
        olala: &mut SystemParamItem<Self::Param>,
    ) -> Result<Self::PreparedAsset, PrepareAssetError<Self::ExtractedAsset>> {
        let ret = match StandardMaterial::prepare_asset(material.standard_material, olala) {
            Ok(val)                                      => Ok(val),
            Err(PrepareAssetError::RetryNextUpdate(val)) =>
                Err(PrepareAssetError::RetryNextUpdate(BendMaterial{standard_material: val})),
            // HERE // _ => panic!("olalalalala"),
        };

        ret
    }
}

pub type DrawBend = (
    SetBendPipeline,               // our pipeline
    SetMeshViewBindGroup<0>,
    SetBendMaterialBindGroup<1>, // our bindgroup
    SetTransformBindGroup<2>,
    DrawMesh,
);

/// fill (I think) the bindgroup (gpu shader information) for the material
pub struct SetBendMaterialBindGroup<const I: usize>;
impl<const I: usize> RenderCommand<Transparent3d> for SetBendMaterialBindGroup<I> {
    type Param = (
        SRes<RenderAssets<BendMaterial>>,
        SQuery<Read<Handle<BendMaterial>>>,
    );
    #[inline]
    fn render<'w>(
        _view: Entity,
        item: &Transparent3d,
        (materials, handle_query): SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) {
        let handle = handle_query.get(item.entity).unwrap();
        let materials = materials.into_inner();
        let material = materials.get(handle).unwrap();

        pass.set_bind_group(I, &material.bind_group, &[]);
    }
}

/// Set the rendering pipeline for the render pass
pub struct SetBendPipeline;
impl RenderCommand<Transparent3d> for SetBendPipeline {
    type Param = SRes<BendPipeline>;
    #[inline]
    fn render<'w>(
        _view: Entity,
        _item: &Transparent3d,
        pbr_shaders: SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) {
        pass.set_render_pipeline(&pbr_shaders.into_inner().pipeline);
    }
}


/// The bend pipeline (with its required informations)
pub struct BendPipeline {
    pub pipeline: RenderPipeline,
    pub shader_module: ShaderModule,
    pub view_layout: BindGroupLayout,
    pub material_layout: BindGroupLayout,
    pub mesh_layout: BindGroupLayout,
    // This dummy white texture is to be used in place of optional StandardMaterial textures
    pub dummy_white_gpu_image: GpuImage,
}

// TODO: this pattern for initializing the shaders / pipeline isn't ideal. this should be handled by the asset system
/// Create a new BendPipeline.
/// Also where the shader "uniform" layout is defined
/// Also where the shader file is defined
///
/// To the future me :
/// The layout is made of (3) group, dont mistake it for the full pipeline layout
impl FromWorld for BendPipeline {
    fn from_world(world: &mut World) -> Self {
        let render_device = world.get_resource::<RenderDevice>().unwrap();
        let shader = Shader::from_wgsl(include_str!("pbr.wgsl"));
        let shader_module = render_device.create_shader_module(&shader);


        let view_layout = render_device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            entries: &[
                // View
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::VERTEX | ShaderStages::FRAGMENT,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        // TODO: change this to ViewUniform::std140_size_static once crevice fixes this!
                        // Context: https://github.com/LPGhatguy/crevice/issues/29
                        min_binding_size: BufferSize::new(144),
                    },
                    count: None,
                },
                // Lights
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        // TODO: change this to GpuLights::std140_size_static once crevice fixes this!
                        // Context: https://github.com/LPGhatguy/crevice/issues/29
                        min_binding_size: BufferSize::new(1424),
                    },
                    count: None,
                },
                // Point Shadow Texture Cube Array
                BindGroupLayoutEntry {
                    binding: 2,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        multisampled: false,
                        sample_type: TextureSampleType::Depth,
                        view_dimension: TextureViewDimension::CubeArray,
                    },
                    count: None,
                },
                // Point Shadow Texture Array Sampler
                BindGroupLayoutEntry {
                    binding: 3,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler {
                        comparison: true,
                        filtering: true,
                    },
                    count: None,
                },
                // Directional Shadow Texture Array
                BindGroupLayoutEntry {
                    binding: 4,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        multisampled: false,
                        sample_type: TextureSampleType::Depth,
                        view_dimension: TextureViewDimension::D2Array,
                    },
                    count: None,
                },
                // Directional Shadow Texture Array Sampler
                BindGroupLayoutEntry {
                    binding: 5,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler {
                        comparison: true,
                        filtering: true,
                    },
                    count: None,
                },
            ],
            label: Some("pbr_view_layout"),
        });

        let material_layout = render_device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: BufferSize::new(
                            ShitHackStandardMaterialUniformData::std140_size_static() as u64,
                        ),
                    },
                    count: None,
                },
                // Base Color Texture
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        multisampled: false,
                        sample_type: TextureSampleType::Float { filterable: true },
                        view_dimension: TextureViewDimension::D2,
                    },
                    count: None,
                },
                // Base Color Texture Sampler
                BindGroupLayoutEntry {
                    binding: 2,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler {
                        comparison: false,
                        filtering: true,
                    },
                    count: None,
                },
                // Emissive Texture
                BindGroupLayoutEntry {
                    binding: 3,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        multisampled: false,
                        sample_type: TextureSampleType::Float { filterable: true },
                        view_dimension: TextureViewDimension::D2,
                    },
                    count: None,
                },
                // Emissive Texture Sampler
                BindGroupLayoutEntry {
                    binding: 4,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler {
                        comparison: false,
                        filtering: true,
                    },
                    count: None,
                },
                // Metallic Roughness Texture
                BindGroupLayoutEntry {
                    binding: 5,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        multisampled: false,
                        sample_type: TextureSampleType::Float { filterable: true },
                        view_dimension: TextureViewDimension::D2,
                    },
                    count: None,
                },
                // Metallic Roughness Texture Sampler
                BindGroupLayoutEntry {
                    binding: 6,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler {
                        comparison: false,
                        filtering: true,
                    },
                    count: None,
                },
                // Occlusion Texture
                BindGroupLayoutEntry {
                    binding: 7,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        multisampled: false,
                        sample_type: TextureSampleType::Float { filterable: true },
                        view_dimension: TextureViewDimension::D2,
                    },
                    count: None,
                },
                // Occlusion Texture Sampler
                BindGroupLayoutEntry {
                    binding: 8,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler {
                        comparison: false,
                        filtering: true,
                    },
                    count: None,
                },
            ],
            label: Some("pbr_material_layout"),
        });

        let mesh_layout = render_device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            entries: &[BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::VERTEX | ShaderStages::FRAGMENT,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    // TODO: change this to MeshUniform::std140_size_static once crevice fixes this!
                    // Context: https://github.com/LPGhatguy/crevice/issues/29
                    min_binding_size: BufferSize::new(144),
                },
                count: None,
            }],
            label: Some("pbr_mesh_layout"),
        });

        let pipeline_layout = render_device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("pbr_pipeline_layout"),
            push_constant_ranges: &[],
            bind_group_layouts: &[&view_layout, &material_layout, &mesh_layout],
        });

        let pipeline = render_device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("pbr_pipeline"),
            vertex: VertexState {
                buffers: &[VertexBufferLayout {
                    array_stride: 32,
                    step_mode: VertexStepMode::Vertex,
                    attributes: &[
                        // Position (GOTCHA! Vertex_Position isn't first in the buffer due to how Mesh sorts attributes (alphabetically))
                        VertexAttribute {
                            format: VertexFormat::Float32x3,
                            offset: 12,
                            shader_location: 0,
                        },
                        // Normal
                        VertexAttribute {
                            format: VertexFormat::Float32x3,
                            offset: 0,
                            shader_location: 1,
                        },
                        // Uv
                        VertexAttribute {
                            format: VertexFormat::Float32x2,
                            offset: 24,
                            shader_location: 2,
                        },
                    ],
                }],
                module: &shader_module,
                entry_point: "vertex",
            },
            fragment: Some(FragmentState {
                module: &shader_module,
                entry_point: "fragment",
                targets: &[ColorTargetState {
                    format: TextureFormat::bevy_default(),
                    blend: Some(BlendState {
                        color: BlendComponent {
                            src_factor: BlendFactor::SrcAlpha,
                            dst_factor: BlendFactor::OneMinusSrcAlpha,
                            operation: BlendOperation::Add,
                        },
                        alpha: BlendComponent {
                            src_factor: BlendFactor::One,
                            dst_factor: BlendFactor::One,
                            operation: BlendOperation::Add,
                        },
                    }),
                    write_mask: ColorWrites::ALL,
                }],
            }),
            depth_stencil: Some(DepthStencilState {
                format: TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: CompareFunction::Greater,
                stencil: StencilState {
                    front: StencilFaceState::IGNORE,
                    back: StencilFaceState::IGNORE,
                    read_mask: 0,
                    write_mask: 0,
                },
                bias: DepthBiasState {
                    constant: 0,
                    slope_scale: 0.0,
                    clamp: 0.0,
                },
            }),
            layout: Some(&pipeline_layout),
            multisample: MultisampleState::default(),
            primitive: PrimitiveState {
                topology: PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: FrontFace::Ccw,
                cull_mode: Some(Face::Back),
                polygon_mode: PolygonMode::Fill,
                clamp_depth: false,
                conservative: false,
            },
        });

        // A 1x1x1 'all 1.0' texture to use as a dummy texture to use in place of optional StandardMaterial textures
        let dummy_white_gpu_image = {
            let image = Image::new_fill(
                Extent3d::default(),
                TextureDimension::D2,
                &[255u8; 4],
                TextureFormat::bevy_default(),
            );
            let texture = render_device.create_texture(&image.texture_descriptor);
            let sampler = render_device.create_sampler(&image.sampler_descriptor);

            let format_size = image.texture_descriptor.format.pixel_size();
            let render_queue = world.get_resource_mut::<RenderQueue>().unwrap();
            render_queue.write_texture(
                ImageCopyTexture {
                    texture: &texture,
                    mip_level: 0,
                    origin: Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &image.data,
                ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(
                        std::num::NonZeroU32::new(
                            image.texture_descriptor.size.width * format_size as u32,
                        )
                        .unwrap(),
                    ),
                    rows_per_image: None,
                },
                image.texture_descriptor.size,
            );

            let texture_view = texture.create_view(&TextureViewDescriptor::default());
            GpuImage {
                texture,
                texture_view,
                sampler,
            }
        };
        BendPipeline {
            pipeline,
            shader_module,
            view_layout,
            material_layout,
            mesh_layout,
            dummy_white_gpu_image,
        }
    }
}


// TODO : try to alias the main type
// Had to hack cause otherwise I cannot acces an std140 function
/// For some reason I can import the StandardMaterialUniformData but not the std140_size_static
/// function. Therefore I had to redefine everytinhg here.
/// Probably a bug, and i would guess it is linked to the fact that crevice is not imported in bevy
#[derive(Clone, AsStd140)]
pub struct ShitHackStandardMaterialUniformData {
    /// Doubles as diffuse albedo for non-metallic, specular for metallic and a mix for everything
    /// in between.
    pub base_color: Vec4,
    // Use a color for user friendliness even though we technically don't use the alpha channel
    // Might be used in the future for exposure correction in HDR
    pub emissive: Vec4,
    /// Linear perceptual roughness, clamped to [0.089, 1.0] in the shader
    /// Defaults to minimum of 0.089
    pub roughness: f32,
    /// From [0.0, 1.0], dielectric to pure metallic
    pub metallic: f32,
    /// Specular intensity for non-metals on a linear scale of [0.0, 1.0]
    /// defaults to 0.5 which is mapped to 4% reflectance in the shader
    pub reflectance: f32,
    pub flags: u32,
}


/// Imitate the actual PbrBundle but for the bending pipeline
#[derive(Bundle)]
pub struct BendPbrBundle {
    pub mesh: Handle<Mesh>,
    pub material: Handle<BendMaterial>,
    pub transform: Transform,
    pub global_transform: GlobalTransform,
    pub not_shadow_caster: NotShadowCaster, // had to prevent the pbr shadows as we use ours
}

impl Default for BendPbrBundle {
    fn default() -> Self {
        Self {
            mesh: Default::default(),
            material: Default::default(),
            transform: Default::default(),
            global_transform: Default::default(),
            not_shadow_caster: NotShadowCaster{},
        }
    }
}


//
// =============== Shadow part ===============
//

pub struct BendOrigin();

/// shadow pipeline
pub struct BendShadowShaders {
    pub shader_module: ShaderModule,
    pub pipeline: RenderPipeline,
    pub view_layout: BindGroupLayout,
    pub olala_layout: BindGroupLayout,
    pub point_light_sampler: Sampler,
    pub directional_light_sampler: Sampler,
}

/// create the new shadow pipeline stuff
///
/// Added a custom uniform to track the camera position as we must bend the world from the 
/// camera perspective and not from the light perspective
/// (could do without it assuming a true round bending and some rotation here and there)
///
impl FromWorld for BendShadowShaders {
    fn from_world(world: &mut World) -> Self {
        let render_device = world.get_resource::<RenderDevice>().unwrap();
        //let pbr_shaders = world.get_resource::<PbrShaders>().unwrap();
        let pbr_shaders = world.get_resource::<BendPipeline>().unwrap();
        let shader = Shader::from_wgsl(include_str!("depth.wgsl"));
        let shader_module = render_device.create_shader_module(&shader);

        let view_layout = render_device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            entries: &[
                // View
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::VERTEX | ShaderStages::FRAGMENT,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        // TODO: change this to ViewUniform::std140_size_static once crevice fixes this!
                        // Context: https://github.com/LPGhatguy/crevice/issues/29
                        min_binding_size: BufferSize::new(144),
                    },
                    count: None,
                },
            ],
            label: Some("shadow_view_layout"),
        });

        // HERE
        let olala_layout = render_device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            entries: &[
                // View
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::VERTEX | ShaderStages::FRAGMENT,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        // TODO: change this to ViewUniform::std140_size_static once crevice fixes this!
                        // Context: https://github.com/LPGhatguy/crevice/issues/29
                        min_binding_size: BufferSize::new(144),
                    },
                    count: None,
                },
            ],
            label: Some("shadow_olala_layout"),
        });

        let pipeline_layout = render_device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("shadow_pipeline_layout"),
            push_constant_ranges: &[],
            bind_group_layouts: &[&view_layout, &pbr_shaders.mesh_layout, &olala_layout],
        });

        let pipeline = render_device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("shadow_pipeline"),
            vertex: VertexState {
                buffers: &[VertexBufferLayout {
                    array_stride: 32,
                    step_mode: VertexStepMode::Vertex,
                    attributes: &[
                        // Position (GOTCHA! Vertex_Position isn't first in the buffer due to how Mesh sorts attributes (alphabetically))
                        VertexAttribute {
                            format: VertexFormat::Float32x3,
                            offset: 12,
                            shader_location: 0,
                        },
                        // Normal
                        VertexAttribute {
                            format: VertexFormat::Float32x3,
                            offset: 0,
                            shader_location: 1,
                        },
                        // Uv
                        VertexAttribute {
                            format: VertexFormat::Float32x2,
                            offset: 24,
                            shader_location: 2,
                        },
                    ],
                }],
                module: &shader_module,
                entry_point: "vertex",
            },
            fragment: None,
            depth_stencil: Some(DepthStencilState {
                format: SHADOW_FORMAT,
                depth_write_enabled: true,
                depth_compare: CompareFunction::GreaterEqual,
                stencil: StencilState {
                    front: StencilFaceState::IGNORE,
                    back: StencilFaceState::IGNORE,
                    read_mask: 0,
                    write_mask: 0,
                },
                bias: DepthBiasState {
                    constant: 0,
                    slope_scale: 0.0,
                    clamp: 0.0,
                },
            }),
            layout: Some(&pipeline_layout),
            multisample: MultisampleState::default(),
            primitive: PrimitiveState {
                topology: PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: PolygonMode::Fill,
                clamp_depth: false,
                conservative: false,
            },
        });

        BendShadowShaders {
            shader_module,
            pipeline,
            view_layout,
            olala_layout,
            point_light_sampler: render_device.create_sampler(&SamplerDescriptor {
                address_mode_u: AddressMode::ClampToEdge,
                address_mode_v: AddressMode::ClampToEdge,
                address_mode_w: AddressMode::ClampToEdge,
                mag_filter: FilterMode::Linear,
                min_filter: FilterMode::Linear,
                mipmap_filter: FilterMode::Nearest,
                compare: Some(CompareFunction::GreaterEqual),
                ..Default::default()
            }),
            directional_light_sampler: render_device.create_sampler(&SamplerDescriptor {
                address_mode_u: AddressMode::ClampToEdge,
                address_mode_v: AddressMode::ClampToEdge,
                address_mode_w: AddressMode::ClampToEdge,
                mag_filter: FilterMode::Linear,
                min_filter: FilterMode::Linear,
                mipmap_filter: FilterMode::Nearest,
                compare: Some(CompareFunction::GreaterEqual),
                ..Default::default()
            }),
        }
    }
}

/// create the bind group for shadows shader things
pub fn queue_bend_shadow_view_bind_group(
    render_device: Res<RenderDevice>,
    shadow_shaders: Res<BendShadowShaders>,
    mut light_meta: ResMut<LightMeta>,
    view_uniforms: Res<ViewUniforms>,
) {
    if let Some(view_binding) = view_uniforms.uniforms.binding() {
        light_meta.shadow_view_bind_group =
            Some(render_device.create_bind_group(&BindGroupDescriptor {
                entries: &[
                    BindGroupEntry {
                        binding: 0,
                        resource: view_binding,
                    },
                ],
                label: Some("shadow_view_bind_group"),
                layout: &shadow_shaders.view_layout,
            }));
    }
}

/// GPU structure to share the camera position during shadow calculation
/// I was bored to search for a good solution I did copy/hacked a bit of working code
/// TODO : use a struct with only position and no vector
#[derive(Clone, AsStd140)]
pub struct CameraPosUniform {
    view_proj: Mat4,
    projection: Mat4,
    world_position: Vec3,
}

/// Vector for the camera pos (yes it is useless, see CameraPosUniform for details)
#[derive(Default)]
pub struct CameraPosUniforms {
    uniforms: DynamicUniformVec<CameraPosUniform>,
}

/// Find the camera (cause it have `BendOrigin`) and insert it in the rendering realm with required infos.
/// So, If I understand things correctly, the render app is completely separated from the main one
///  and this extract function is the bridge between the two.
fn extract_camera_pos(
    mut commands: Commands,
    query: Query<(Entity, &BendOrigin, &GlobalTransform)>,
) {
    for (entity, _, transform) in query.iter() {
        commands.get_or_spawn(entity)
            .insert_bundle((
                BendOrigin(),
                ExtractedView {
                    projection: Default::default(),
                    transform: *transform,
                    width: 0,
                    height: 0,
                },
            ))
        ;
    }
}

/// If I am right, it is the one that, withint the render realm, fills the actual GPU structures
fn prepare_camera_pos(
    mut _commands: Commands,
    render_device: Res<RenderDevice>,
    render_queue: Res<RenderQueue>,
    mut camera_pos_uniform: ResMut<CameraPosUniforms>,
    extracted_views: Query<(Entity, &BendOrigin, &ExtractedView)>,
) {
    camera_pos_uniform
        .uniforms
        .reserve_and_clear(1, &render_device);

    for (_entity, _, extracted_view) in extracted_views.iter() {
        camera_pos_uniform.uniforms.push(CameraPosUniform {
            view_proj: Default::default(),
            projection: Default::default(),
            world_position: extracted_view.transform.translation,
        });
    }

    camera_pos_uniform.uniforms.write_buffer(&render_queue);
}

/// Create a bind group with the GPU data structure things
pub fn queue_camera_pos_bind_group(
    render_device: Res<RenderDevice>,
    shadow_shaders: Res<BendShadowShaders>,
    mut camera_pos: ResMut<CameraPos>,
    camera_pos_uniform: Res<CameraPosUniforms>,
) {
    if let Some(view_binding) = camera_pos_uniform.uniforms.binding() {
        camera_pos.bind_group = Some(
            render_device.create_bind_group(&BindGroupDescriptor {
                entries: &[
                    BindGroupEntry {
                        binding: 0,
                        resource: view_binding,
                    },
                ],
                label: Some("shadow_olala_bind_group"),
                layout: &shadow_shaders.olala_layout,
            })
        )
    }
}

#[derive(Default)]
pub struct CameraPos {
    bind_group: Option<BindGroup>,
}

pub struct DrawCustomShadowMesh {
    params: SystemState<(
        SRes<BendShadowShaders>,
        SRes<LightMeta>,
        SRes<CameraPos>,
        SRes<TransformBindGroup>,
        SRes<RenderAssets<Mesh>>,
        SQuery<(Read<DynamicUniformIndex<MeshUniform>>, Read<Handle<Mesh>>)>,
        SQuery<Read<ViewUniformOffset>>,
    )>,
}

impl DrawCustomShadowMesh {
    pub fn new(world: &mut World) -> Self {
        Self {
            params: SystemState::new(world),
        }
    }
}

/// assemble everything to make the final raw call ?
impl Draw<Shadow> for DrawCustomShadowMesh {
    fn draw<'w>(
        &mut self,
        world: &'w World,
        pass: &mut TrackedRenderPass<'w>,
        view: Entity,
        item: &Shadow,
    ) {
        let (shadow_shaders, light_meta, camera_pos, transform_bind_group, meshes, items, views) =
            self.params.get(world);
        let (transform_index, mesh_handle) = items.get(item.entity).unwrap();
        let view_uniform_offset = views.get(view).unwrap();
        pass.set_render_pipeline(&shadow_shaders.into_inner().pipeline);
        let non = light_meta.into_inner();
        pass.set_bind_group(
            0,
            //light_meta
            //    .into_inner()
            non
                .shadow_view_bind_group
                .as_ref()
                .unwrap(),
            &[view_uniform_offset.offset],
        );

        pass.set_bind_group(
            1,
            &transform_bind_group.into_inner().value,
            &[transform_index.index()],
        );

        pass.set_bind_group(
            2,
            camera_pos
                .into_inner()
                .bind_group
                .as_ref()
                .unwrap(),
            &[0], // hack there. Cause of lazy i am using a vecotr where I need a single element. So always indice 0
        );

        let gpu_mesh = meshes.into_inner().get(mesh_handle).unwrap();
        pass.set_vertex_buffer(0, gpu_mesh.vertex_buffer.slice(..));
        if let Some(index_info) = &gpu_mesh.index_info {
            pass.set_index_buffer(index_info.buffer.slice(..), 0, IndexFormat::Uint32);
            pass.draw_indexed(0..index_info.count, 0, 0..1);
        } else {
            panic!("non-indexed drawing not supported yet")
        }
    }
}

pub fn queue_shadows(
    shadow_draw_functions: Res<DrawFunctions<Shadow>>,
    casting_meshes: Query<Entity, (With<Handle<Mesh>>, With<NotShadowCaster>)>, // our shadows must be NotShadowCaster or will have both pbr and our
    mut view_lights: Query<&ViewLights>,
    mut view_light_shadow_phases: Query<&mut RenderPhase<Shadow>>,
) {
    for view_lights in view_lights.iter_mut() {
        // ultimately lights should check meshes for relevancy (ex: light views can "see" different meshes than the main view can)
        let draw_shadow_mesh = shadow_draw_functions
            .read()
            .get_id::<DrawCustomShadowMesh>()
            .unwrap();
        for view_light_entity in view_lights.lights.iter().copied() {
            let mut shadow_phase = view_light_shadow_phases.get_mut(view_light_entity).unwrap();
            // TODO: this should only queue up meshes that are actually visible by each "light view"
            for entity in casting_meshes.iter() {
                shadow_phase.add(Shadow {
                    draw_function: draw_shadow_mesh,
                    entity,
                    distance: 0.0, // TODO: sort back-to-front
                })
            }
        }
    }
}
