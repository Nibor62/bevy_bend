// NOTE: Keep in sync with pbr.wgsl
[[block]]
struct View {
    view_proj: mat4x4<f32>;
    projection: mat4x4<f32>;
    world_position: vec3<f32>;
};
[[group(0), binding(0)]]
var<uniform> view: View;
[[group(2), binding(0)]]
var<uniform> non: View;


[[block]]
struct Mesh {
    model: mat4x4<f32>;
    inverse_transpose_model: mat4x4<f32>;
    // 'flags' is a bit field indicating various options. u32 is 32 bits so we have up to 32 options.
    flags: u32;
};

[[group(1), binding(0)]]
var<uniform> mesh: Mesh;

struct Vertex {
    [[location(0)]] position: vec3<f32>;
};

struct VertexOutput {
    [[builtin(position)]] clip_position: vec4<f32>;
};

fn olala(input_pos: vec3<f32>) -> vec3<f32> {
    //let base_pos = vec2<f32>(view.world_position.x, view.world_position.z);
    let base_pos = vec2<f32>(non.world_position.x, non.world_position.z);
    let input_pos_2d = vec2<f32>(input_pos.xz);
    let dist = length(base_pos - input_pos_2d);
    let output_pos = vec3<f32>(input_pos.x, input_pos.y-(0.01*dist*dist), input_pos.z);
    //let output_pos = vec3<f32>(input_pos.x, input_pos.y-(0.01*abs(dist)), input_pos.z);

    return output_pos;
}

[[stage(vertex)]]
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;


    let world_position = mesh.model * vec4<f32>(vertex.position, 1.0);

    let world_position = vec4<f32>(olala(world_position.xyz), world_position[3]);
    //let olala_normal = vec3<f32>(vertex.normal.x+0.3, 0.0, vertex.normal.z);

    out.clip_position = view.view_proj * world_position ;
    //out.clip_position = view.view_proj * mesh.model * vec4<f32>(vertex.position, 1.0);
    return out;
}
