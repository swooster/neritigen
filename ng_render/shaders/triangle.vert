#version 450

layout(push_constant) uniform ViewBuffer {
    mat4 view;
} view_buffer;

layout(location = 0) out vec3 vertColor;
layout(location = 1) out vec3 vertNormal;

vec3 positions[6] = vec3[](
    vec3(1.0, 0.0, 0.0),
    vec3(-1.0, -1.0, 0.0),
    vec3(-1.0, 1.0, 0.0),

    vec3(0.0, 0.0, 1.0),
    vec3(1.0, 0.0, -1.0),
    vec3(-1.0, 0.0, -1.0)
);

vec3 normals[6] = vec3[](
    vec3(0.0, 0.0, -1.0),
    vec3(0.0, 0.0, -1.0),
    vec3(0.0, 0.0, -1.0),

    vec3(0.0, 1.0, 0.0),
    vec3(0.0, 1.0, 0.0),
    vec3(0.0, 1.0, 0.0)
);

vec3 colors[3] = vec3[](
    vec3(1.0, 0.0, 0.0),
    vec3(0.0, 1.0, 0.0),
    vec3(0.0, 0.0, 1.0)
);

void main() {
    gl_Position = view_buffer.view * vec4(positions[gl_VertexIndex], 1.0);
    vertColor = colors[gl_VertexIndex % 3];

    // Map through view because the light shader has a screenspace-to-lightspace matrix
    // vec4 view_normal = view_buffer.view * vec4(normals[gl_VertexIndex], 0);
    // vertNormal = view_normal.xyz;
    vertNormal = normals[gl_VertexIndex];
}
