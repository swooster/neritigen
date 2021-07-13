#version 450

layout(push_constant) uniform ViewBuffer {
    mat4 view;
} view_buffer;

layout(location = 0) out vec3 vertColor;

vec2 positions[3] = vec2[](
    vec2(0.0, -1.0),
    vec2(1.0, 1.0),
    vec2(-1.0, 1.0)
);

vec3 colors[3] = vec3[](
    vec3(1.0, 0.0, 0.0),
    vec3(0.0, 1.0, 0.0),
    vec3(0.0, 0.0, 1.0)
);

void main() {
    gl_Position = view_buffer.view * vec4(positions[gl_VertexIndex], 0.0, 1.0);
    vertColor = colors[gl_VertexIndex];
}
