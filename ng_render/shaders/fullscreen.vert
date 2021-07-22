#version 450

layout(location = 0) out vec4 ndc;

vec2 positions[3] = vec2[](
    vec2(1.0, 1.0),
    vec2(1.0, -3.0),
    vec2(-3.0, 1.0)
);

void main() {
    ndc = vec4(positions[gl_VertexIndex], 1.0, 1.0);
    gl_Position = ndc;
}
