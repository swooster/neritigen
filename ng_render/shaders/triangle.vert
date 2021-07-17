#version 450

layout(push_constant) uniform ViewBuffer {
    mat4 view;
} view_buffer;

layout(location = 0) out vec3 vertColor;

vec3 positions[6] = vec3[](
    vec3(1.0, 0.0, 0.0),
    vec3(-1.0, -1.0, 0.0),
    vec3(-1.0, 1.0, 0.0),

    vec3(0.0, 0.0, 1.0),
    vec3(1.0, 0.0, -1.0),
    vec3(-1.0, 0.0, -1.0)
);

vec3 colors[3] = vec3[](
    vec3(1.0, 0.0, 0.0),
    vec3(0.0, 1.0, 0.0),
    vec3(0.0, 0.0, 1.0)
);

void main() {
    gl_Position = view_buffer.view * vec4(positions[gl_VertexIndex], 1.0);
    vertColor = colors[gl_VertexIndex % 3];
}
