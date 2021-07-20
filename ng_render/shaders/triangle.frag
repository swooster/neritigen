#version 450

layout(location = 0) in vec3 vertColor;
layout(location = 1) in vec3 vertNormal;

layout(location = 0) out vec3 diffuse;
layout(location = 1) out vec3 normal;

void main() {
    diffuse = vertColor;
    float facing_scale = gl_FrontFacing ? 1.0 : -1.0;
    normal = 0.5 * facing_scale * normalize(vertNormal) + vec3(0.5);
}
