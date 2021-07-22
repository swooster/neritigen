#version 450

layout(set = 0, binding = 3) uniform sampler2D shadow;

layout(push_constant) uniform LightBuffer {
    mat4 light_to_screen;
    vec4 sunlight_direction;
    int shadow_size;
} light_buffer;

ivec2 square_vertices[6] = ivec2[](
    ivec2(-1, -1),
    ivec2(-1, 0),
    ivec2(0, 0),

    ivec2(-1, -1),
    ivec2(0, 0),
    ivec2(0, -1)
);

int quad_offsets[6] = int[](           1,     1,     0,    1,     1,    2 );
bool quad_bottom_track[6] = bool[]( true, false, false, false, true, true );

void main() {
    int shadow_size = light_buffer.shadow_size;

    int topVertexIndex = gl_VertexIndex - (shadow_size + 1)*(shadow_size + 1) * 6;
    if (topVertexIndex < 0) {

        int square = gl_VertexIndex / 6;
        int vertex = gl_VertexIndex % 6;

        ivec2 ij = ivec2(square % (shadow_size + 1), square / (shadow_size + 1))
                 + square_vertices[vertex];
        ivec2 clamped_ij = clamp(ij, ivec2(0), ivec2(shadow_size-1));

        vec2 shadow_coords = vec2(clamped_ij) / (shadow_size - 1);
        float shadow_depth = ij == clamped_ij ? texture(shadow, shadow_coords).r : 1;
        vec4 light_coords = vec4(shadow_coords * 2 - vec2(1.0, 1.0), shadow_depth, 1.0);
        gl_Position = light_buffer.light_to_screen * light_coords;

    } else {

        int quad = topVertexIndex / 6;
        int vertex = topVertexIndex % 6;

        int track_i = quad + quad_offsets[vertex];

        int b  = int(  quad_bottom_track[vertex] );
        int nb = int( !quad_bottom_track[vertex] );

        ivec2 ij = clamp(track_i,                     0, shadow_size - 1) * ivec2(b, nb)
                 + clamp(track_i - (shadow_size - 1), 0, shadow_size - 1) * ivec2(nb, b);

        vec2 shadow_coords = vec2(ij) / (shadow_size - 1);
        vec4 light_coords = vec4(shadow_coords * 2 - vec2(1.0, 1.0), 1.0, 1.0);
        gl_Position = light_buffer.light_to_screen * light_coords;
    }
}
