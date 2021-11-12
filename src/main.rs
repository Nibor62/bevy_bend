mod bend;
mod game;

use bevy::{
    diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin},
    prelude::*,
    pbr2::{PbrBundle, StandardMaterial, PointLightBundle, PointLight, NotShadowCaster},
    render2::{
        camera::PerspectiveCameraBundle,
        color::Color,
        render_resource,
        mesh,
        mesh::{shape, Mesh},
    },
    PipelinedDefaultPlugins,
};

use crate::bend::{
    BendMaterialPlugin,
    BendMaterial,
    BendPbrBundle,
    BendOrigin,
};

use crate::game::{Moving, moving};

// +X right
// +Y up
// +Z me

fn main() {
    App::new()
        .insert_resource(Msaa { samples: 4})
        .insert_resource(WindowDescriptor {
            vsync: false,
            ..Default::default()
        })

        //.add_plugins(DefaultPlugins)
        .add_plugins(PipelinedDefaultPlugins)

        .add_plugin(BendMaterialPlugin)

        .add_plugin(FrameTimeDiagnosticsPlugin::default())
        .add_plugin(LogDiagnosticsPlugin::default())

        .add_startup_system(setup.system())

        .add_system(cursor_thing.system())
        .add_system(moving.system())

        .add_system(make_chunk_mesh.system())

        .run();
}

const CHUNK_SIZE : usize = 16;
struct Chunk {
    cubes : [[[bool; CHUNK_SIZE]; CHUNK_SIZE]; CHUNK_SIZE],
}

impl Chunk {
    fn new() -> Self {
        let mut cubes = [[[false; CHUNK_SIZE]; CHUNK_SIZE]; CHUNK_SIZE];

        for x in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                let y_max = {
                    let x = x as i32;
                    let z = z as i32;
                    let half_chunk = (CHUNK_SIZE as i32) / 2;
                    let sqrt = (( (x-half_chunk).pow(2) + (z-half_chunk).pow(2) ) as f32).sqrt();
                    (sqrt * 0.5).ceil() as usize
                };

                //let y_max = 1;

                for y in 0..y_max {
                    cubes[x][y][z] = true;
                }
            }
        }

        Self{ cubes }
    }
}

fn make_chunk_mesh(
    mut query: Query<(&Chunk, &mut Handle<Mesh>), (Changed<Chunk>,)>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    for (chunk, mesh) in query.iter_mut() {
        let mut mesh_maker = MeshMaker::new();
        for (x, x_val) in chunk.cubes.iter().enumerate() {
            for (y, y_val) in x_val.iter().enumerate() {
                for (z, val) in y_val.iter().enumerate() {
                    if *val {
                        mesh_maker = mesh_maker.add_cube(x as f32, y as f32, z as f32, 1.);
                    }
                }
            }
        }

        let _ = meshes.set(&*mesh, mesh_maker.build());
    }
}

struct MeshMaker {
    positions : Vec<[f32; 3]>,
    normals   : Vec<[f32; 3]>,
    uvs       : Vec<[f32; 2]>,
    indices   : Vec<u32>,

    point_num : u32,
}

impl MeshMaker {
    fn new() -> Self {
        Self{
            positions : Vec::new(),
            normals   : Vec::new(),
            uvs       : Vec::new(),
            indices   : Vec::new(),

            point_num : 0,
        }
    }

    /// cube coordinate are for first point (aka not the cube center)
    fn add_cube(mut self, x: f32, y: f32, z: f32, size: f32) -> Self {
        self.positions.push([x,      y,      z]);
        self.positions.push([x+size, y,      z]);
        self.positions.push([x,      y+size, z]);
        self.positions.push([x+size, y+size, z]);
        self.normals.push([0., 0., -1.]);
        self.normals.push([0., 0., -1.]);
        self.normals.push([0., 0., -1.]);
        self.normals.push([0., 0., -1.]);
        self.uvs.push([0., 0.]);
        self.uvs.push([1., 0.]);
        self.uvs.push([0., 1.]);
        self.uvs.push([1., 1.]);
        self.indices.append(&mut vec![self.point_num+0, self.point_num+2, self.point_num+1]);
        self.indices.append(&mut vec![self.point_num+1, self.point_num+2, self.point_num+3]);
        self.point_num += 4;

        self.positions.push([x,      y,      z+size]);
        self.positions.push([x+size, y,      z+size]);
        self.positions.push([x,      y+size, z+size]);
        self.positions.push([x+size, y+size, z+size]);
        self.normals.push([0., 0., 1.]);
        self.normals.push([0., 0., 1.]);
        self.normals.push([0., 0., 1.]);
        self.normals.push([0., 0., 1.]);
        self.uvs.push([0., 0.]);
        self.uvs.push([1., 0.]);
        self.uvs.push([0., 1.]);
        self.uvs.push([1., 1.]);
        self.indices.append(&mut vec![self.point_num+0, self.point_num+1, self.point_num+2]);
        self.indices.append(&mut vec![self.point_num+2, self.point_num+1, self.point_num+3]);
        self.point_num += 4;

        // --

        self.positions.push([x,      y,      z     ]);
        self.positions.push([x+size, y,      z     ]);
        self.positions.push([x,      y,      z+size]);
        self.positions.push([x+size, y,      z+size]);
        self.normals.push([0., -1., 0.]);
        self.normals.push([0., -1., 0.]);
        self.normals.push([0., -1., 0.]);
        self.normals.push([0., -1., 0.]);
        self.uvs.push([0., 0.]);
        self.uvs.push([1., 0.]);
        self.uvs.push([0., 1.]);
        self.uvs.push([1., 1.]);
        self.indices.append(&mut vec![self.point_num+0, self.point_num+1, self.point_num+2]);
        self.indices.append(&mut vec![self.point_num+2, self.point_num+1, self.point_num+3]);
        self.point_num += 4;

        self.positions.push([x,      y+size, z     ]);
        self.positions.push([x+size, y+size, z     ]);
        self.positions.push([x,      y+size, z+size]);
        self.positions.push([x+size, y+size, z+size]);
        self.normals.push([0., 1., 0.]);
        self.normals.push([0., 1., 0.]);
        self.normals.push([0., 1., 0.]);
        self.normals.push([0., 1., 0.]);
        self.uvs.push([0., 0.]);
        self.uvs.push([1., 0.]);
        self.uvs.push([0., 1.]);
        self.uvs.push([1., 1.]);
        self.indices.append(&mut vec![self.point_num+0, self.point_num+2, self.point_num+1]);
        self.indices.append(&mut vec![self.point_num+1, self.point_num+2, self.point_num+3]);
        self.point_num += 4;

        // --

        self.positions.push([x,      y,      z     ]);
        self.positions.push([x,      y+size, z     ]);
        self.positions.push([x,      y,      z+size]);
        self.positions.push([x,      y+size, z+size]);
        self.normals.push([-1., 0., 0.]);
        self.normals.push([-1., 0., 0.]);
        self.normals.push([-1., 0., 0.]);
        self.normals.push([-1., 0., 0.]);
        self.uvs.push([0., 0.]);
        self.uvs.push([1., 0.]);
        self.uvs.push([0., 1.]);
        self.uvs.push([1., 1.]);
        self.indices.append(&mut vec![self.point_num+0, self.point_num+2, self.point_num+1]);
        self.indices.append(&mut vec![self.point_num+1, self.point_num+2, self.point_num+3]);
        self.point_num += 4;

        self.positions.push([x+size, y,      z     ]);
        self.positions.push([x+size, y+size, z     ]);
        self.positions.push([x+size, y,      z+size]);
        self.positions.push([x+size, y+size, z+size]);
        self.normals.push([1., 0., 0.]);
        self.normals.push([1., 0., 0.]);
        self.normals.push([1., 0., 0.]);
        self.normals.push([1., 0., 0.]);
        self.uvs.push([0., 0.]);
        self.uvs.push([1., 0.]);
        self.uvs.push([0., 1.]);
        self.uvs.push([1., 1.]);
        self.indices.append(&mut vec![self.point_num+0, self.point_num+1, self.point_num+2]);
        self.indices.append(&mut vec![self.point_num+2, self.point_num+1, self.point_num+3]);
        self.point_num += 4;

        self
    }

    fn build(self) -> Mesh {
        let indices = mesh::Indices::U32(self.indices);

        let mut mesh = Mesh::new(render_resource::PrimitiveTopology::TriangleList);
        mesh.set_attribute(Mesh::ATTRIBUTE_POSITION, self.positions);
        mesh.set_attribute(Mesh::ATTRIBUTE_NORMAL,   self.normals);
        mesh.set_attribute(Mesh::ATTRIBUTE_UV_0,     self.uvs);
        mesh.set_indices(Some(indices));

        mesh
    }
}

//const SHADER_VERTEX: &str = r#"
//#version 450
//
//layout(location = 0) in vec3 Vertex_Position;
//
//layout(set = 0, binding = 0) uniform CameraViewProj {
//    mat4 view_proj;
//};
//layout(set = 1, binding = 0) uniform Tranform {
//    mat4 model;
//};
//
//void main() {
//    gl_Position = view_proj * model * vec4(Vertex_Position, 1.0);
//}
//
//"#;

/*
const SHADER_VERTEX: &str = r#"
#version 450

layout(location = 0) in vec3 Vertex_Position;
layout(location = 1) in vec3 Vertex_Normal;
layout(location = 2) in vec2 Vertex_Uv;

layout(set = 0, binding = 0) uniform Camera {
    mat4 ViewProj;
};
layout(set = 1, binding = 0) uniform Transform {
    mat4 Model;
};

void main() {
    gl_Position = ViewProj * Model * vec4(Vertex_Position, 1.0);
}
"#;
*/

//const SHADER_FRAGMENT: &str = r#"
//#version 450
//layout(location = 0) out vec4 o_Target;
//
////layout(set = 3, binding = 0) uniform MyMaterial_color {
////    vec4 color;
////};
//
//void main() {
//    o_Target = vec4(1,1,1,1);
//}
//"#;

/*
const SHADER_FRAGMENT: &str = r#"
#version 450

layout(location = 0) in vec4 v_Position;

layout(location = 0) out vec4 o_Target;

void main() {
    o_Target = vec4(1.0, 1.0, 0.0, 1.0);
}
"#;
*/

fn setup(
    mut commands: Commands,
    //mut pipelines: ResMut<Assets<PipelineDescriptor>>,
    //mut shaders: ResMut<Assets<Shader>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut cust_materials: ResMut<Assets<BendMaterial>>,
) {
    //commands.spawn_bundle(PbrBundle {
    //    mesh: meshes.add(MeshMaker::new()
    //        .add_cube(0.,0.,0., 1.)
    //        .add_cube(2.,0.,0., 1.)
    //        .add_cube(4.,0.,0., 1.)
    //        .add_cube(0.,1.,0., 1.)
    //        .add_cube(2.,1.,0., 1.)
    //        .add_cube(4.,1.,0., 1.)
    //        .build()
    //    ),
    //    material: materials.add(Color::rgb(1., 0., 0.).into()),
    //    //transform: trans,
    //    ..Default::default()
    //});
    /*
    let pipeline = pipelines.add(
        PipelineDescriptor::default_config(ShaderStages {
            vertex: shaders.add(Shader::from_glsl(ShaderStage::Vertex, SHADER_VERTEX)),
            fragment: Some(shaders.add(Shader::from_glsl(ShaderStage::Fragment, SHADER_FRAGMENT))),
            //fragment: None,
        })
    );
    */

    for x in -5..5 {
        for z in -5..5 {
            //commands.spawn_bundle(PbrBundle {
            commands.spawn_bundle(BendPbrBundle {
                mesh: meshes.add(MeshMaker::new().build()),
                material: cust_materials.add(BendMaterial {
                    standard_material: StandardMaterial {
                        base_color : Color::hex("ff0000").unwrap(),
                        ..Default::default()
                    }
                }),
                transform: Transform::from_xyz(
                    CHUNK_SIZE as f32 * x as f32,
                    0.,
                    CHUNK_SIZE as f32 * z as f32
                ),
                //render_pipelines: RenderPipelines::from_pipelines(
                //    vec![RenderPipeline::new(pipeline.clone()),],
                //),
                ..Default::default()
            }).insert(Chunk::new())
            .insert(NotShadowCaster);
        }
    }

    commands.spawn_bundle(PbrBundle {
        mesh: meshes.add(Mesh::from(shape::Cube { size: 1.0 })),
        material: materials.add( StandardMaterial {
                base_color : Color::hex("ff0000").unwrap(),
                ..Default::default()
        }),
        transform: Transform::from_xyz(
               0.,
               0.,
               0.,
        ),
        //render_pipelines: RenderPipelines::from_pipelines(
        //    vec![RenderPipeline::new(pipeline.clone()),],
        //),
        ..Default::default()
    });

    commands.spawn_bundle(PointLightBundle {
        transform: Transform::from_xyz(4., 6.1, 3.),
        point_light: PointLight {
            intensity: 4000.,
            range: 100.,
            ..Default::default()
        },
        ..Default::default()
    });

    commands.spawn_bundle(PerspectiveCameraBundle {
        transform: Transform::from_xyz(5., 16., 5.).looking_at(Vec3::ZERO, Vec3::Y),
        ..Default::default()
    })
        .insert(Moving())
        .insert(BendOrigin())
    ;
}

fn cursor_thing(
    mut windows: ResMut<Windows>,
    btn: Res<Input<MouseButton>>,
    key: Res<Input<KeyCode>>,
) {
    let window = windows.get_primary_mut().unwrap();

    if btn.just_pressed(MouseButton::Left) {
        window.set_cursor_lock_mode(true);
        window.set_cursor_visibility(false);
    }

    if key.just_pressed(KeyCode::Escape) {
        window.set_cursor_lock_mode(false);
        window.set_cursor_visibility(true);
    }
}

