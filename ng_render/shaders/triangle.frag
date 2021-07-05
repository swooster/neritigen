#version 450

layout(location = 0) in vec3 vertColor;

layout(location = 0) out vec3 fragColor;

void main() {
    fragColor = vertColor;
}
